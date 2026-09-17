#!/usr/bin/env python3
"""N-G2 的量具之一：`scripts/gate.sh` **自检段的现打耗时**（`NG2D5`）。

它买的是一件事：头注那句「N 条探针合计 ≈ X 毫秒」**不许是抄来的常量** ——
`NG2D5` 的 acceptor 逐字点名「可以只量『加了几条探针』而不量耗时 ⇒ 要的是现打的秒数」。

## 量法（**跑的是真字节，不是复算**）

把 `gate.sh` 里自检段真正会走到的那几块**原样切下来**拼成一个 harness，再计时跑它：
  · A 段：文件开头 ⋯ 到那一行**裸 `gate_selftest` 调用**为止
    （含 `set -uo pipefail` · `cd` · `fails=()` · `gate_decolor` · `gate_diag` ·
      `run_gate` · `run_gate_sum` · `gate_assert_judged` · `gate_selftest` 定义与调用）
  · B 段：`run_e2e()` 整个函数（探针⑩ 要它）
  · C 段：`gate_selftest_e2e()` 整个函数 ＋ 那一行裸调用
  · 尾巴：harness 自己加的**两行**，把 `fails` 的条数与内容打出来（唯一的非原样字节，已标注）

⇒ harness **不含**任何一道真门（cargo / generated / daemon / npm / 四套 e2e / pb check），
   它们的调用行全都夹在 A 与 B 之间、被这个切法排除掉。

⚠ **射程（别读宽）**：
  · 它量的是**自检段**，不是整趟门禁；两个数分母不同，不许相减。
  · harness 落在 `/tmp/<自己的目录>` 下并把 `e2e/` 符号链接回被测树 ——
    因为 `gate.sh` 第一行 `cd "$(dirname "$0")/.."`，探针⑩ 要 `e2e/assert-pass-floor.sh` 在位。
    **它不往被测工作树写一个字节**（`NG2D5` 硬边界）。
  · 计时含**一次 `bash` 冷启动 ＋ 解析 750 行脚本**；这一份开销在真门禁里本来也要付一次，
    但它**不属于探针**。⇒ 报「空跑基线」那一栏（同一份 A 段、把 `gate_selftest` 调用行删掉）
    做减法，两个数都印出来，别只印差。

用法（**在沙箱里跑**，探针⑩ 要起 `bash e2e/assert-pass-floor.sh`）：
    python3 evidence/N-G2-selftest-cost.py            # 工作树现状
    python3 evidence/N-G2-selftest-cost.py 5924b91    # 某个提交的那一份
    python3 evidence/N-G2-selftest-cost.py 5924b91 9  # 再指定跑几趟（默认 9）
"""
from __future__ import annotations

import os
import pathlib
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

TARGET = "scripts/gate.sh"
TAIL = (
    '\n# ↓↓ 以下两行是 N-G2-selftest-cost.py 的 harness 自己加的，不是 gate.sh 的字节 ↓↓\n'
    'printf "FAILS=%s\\n" "${#fails[@]}"\n'
    'for f in ${fails[@]+"${fails[@]}"}; do printf "  RED %s\\n" "$f"; done\n'
)


def load(rev: str | None) -> str:
    if rev is None:
        return pathlib.Path(TARGET).read_text(encoding="utf-8")
    p = subprocess.run(["git", "show", f"{rev}:{TARGET}"], capture_output=True, text=True)
    if p.returncode != 0:
        raise SystemExit(f"取不到 {rev}:{TARGET} —— {p.stderr.strip()}")
    return p.stdout


def uniq_index(lines: list[str], want: str, what: str) -> int:
    """锚点必须**恰好命中一次** —— 命中 0 次或多次一律报出来，不许猜。"""
    hits = [i for i, ln in enumerate(lines) if ln == want]
    print(f"  锚点 {what:24} {want!r:34} 命中 {len(hits)} 次"
          f"{'（行 ' + str(hits[0] + 1) + '）' if len(hits) == 1 else ''}")
    if len(hits) != 1:
        raise SystemExit(f"❌ 锚点 {what} 命中 {len(hits)} 次，判不了 —— 不许猜，先修锚点")
    return hits[0]


def block(lines: list[str], head: str, what: str) -> tuple[int, int]:
    """`名字() {` ⋯ 到第一行顶格 `}` 为止。返回闭区间下标。"""
    i = uniq_index(lines, head, what)
    j = i + 1
    while j < len(lines) and lines[j] != "}":
        j += 1
    if j >= len(lines):
        raise SystemExit(f"❌ {what} 找不到收尾的顶格 `}}`")
    return i, j


def build(text: str) -> tuple[str, str, dict[str, int]]:
    """返回（带自检的 harness, 不带自检的空跑基线, 各段行数）。"""
    lines = text.splitlines()
    call_a = uniq_index(lines, "gate_selftest", "A 段收尾（裸调用）")
    defs = lines[:call_a]                      # A 段的**定义**部分（不含那一行裸调用）
    calls = [lines[call_a]]                    # 自检段的裸调用行，空跑基线里全部拿掉
    stats = {"A 段定义": len(defs)}

    if any(ln == "run_e2e() {" for ln in lines):
        b0, b1 = block(lines, "run_e2e() {", "B 段 run_e2e")
        defs += lines[b0 : b1 + 1]
        stats["B 段"] = b1 - b0 + 1
    if any(ln == "gate_selftest_e2e() {" for ln in lines):
        c0, c1 = block(lines, "gate_selftest_e2e() {", "C 段 gate_selftest_e2e")
        defs += lines[c0 : c1 + 1]
        calls.append(lines[uniq_index(lines, "gate_selftest_e2e", "C 段裸调用")])
        stats["C 段"] = c1 - c0 + 1
    else:
        print("  （这一份没有 gate_selftest_e2e —— 探针⑩ 不在，属基点那一版）")

    full = "\n".join(defs + calls) + "\n" + TAIL
    idle = "\n".join(defs) + "\n" + TAIL       # 同一批定义，只是一条探针都不调
    return full, idle, stats


def run_once(script: str, root: pathlib.Path, repo: pathlib.Path) -> tuple[float, str]:
    d = root / "scripts"
    d.mkdir(parents=True, exist_ok=True)
    p = d / "harness.sh"
    p.write_text(script, encoding="utf-8")
    link = root / "e2e"
    if not link.exists():
        link.symlink_to(repo / "e2e")
    t0 = time.perf_counter()
    r = subprocess.run(["bash", str(p)], capture_output=True, text=True)
    return (time.perf_counter() - t0) * 1000.0, r.stdout + r.stderr


def main() -> int:
    rev = sys.argv[1] if len(sys.argv) > 1 else None
    reps = int(sys.argv[2]) if len(sys.argv) > 2 else 9
    repo = pathlib.Path.cwd()
    print(f"被测对象：{repo}/{TARGET} @ {rev or '工作树'}    跑 {reps} 趟")
    full, idle, stats = build(load(rev))
    print("  段行数：" + " · ".join(f"{k} {v} 行" for k, v in stats.items()))

    root = pathlib.Path(tempfile.mkdtemp(prefix="ng2-selftest-cost-"))
    try:
        out_full = out_idle = ""
        ms_full, ms_idle = [], []
        for _ in range(reps):
            t, out_full = run_once(full, root, repo)
            ms_full.append(t)
            t, out_idle = run_once(idle, root, repo)
            ms_idle.append(t)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    def show(tag: str, xs: list[float]) -> float:
        print(f"  {tag:26} 中位 {statistics.median(xs):7.1f} ms · "
              f"最小 {min(xs):7.1f} ms · 最大 {max(xs):7.1f} ms")
        return statistics.median(xs)

    print("\n现打（墙钟，含一次 bash 冷启动 + 解析）：")
    a = show("带自检段", ms_full)
    b = show("空跑基线（去掉自检调用）", ms_idle)
    print(f"\n  ⇒ **自检段本身 ≈ {a - b:.1f} ms**（两个中位数相减；"
          f"分母 = 同一台机器、同一份 harness、各 {reps} 趟交替跑）")
    print(f"\n带自检那一趟的裁决（0 = 十条探针全绿）：\n{out_full.rstrip()}")
    if out_idle.strip() != "FAILS=0":
        print(f"⚠ 空跑基线不是 FAILS=0，读数不干净：{out_idle.rstrip()}")
    return 0


if __name__ == "__main__":
    os.environ.pop("GATE_DIAG_KEY", None)
    sys.exit(main())
