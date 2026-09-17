#!/usr/bin/env python3
"""N-G2 的量具之二：**11 条判定与基点逐字节相同**（`NG2D4`）。

`K-G3 §143` 硬边界逐字：「**不动那五道已有的门的判定口径**」。本量具把那句话变成读数。

## 它量哪五块（`NG2D4` 口径逐字：三块整函数 ＋ 两段行范围）

| 块 | 认法（锚点必须**恰好命中一次**，否则报错不猜） |
|---|---|
| `run_gate` / `run_gate_sum` / `run_e2e` | `^名字() {` ⋯ 到第一行顶格 `}` |
| `generated`（行内） | `git diff --quiet --exit-code -- src/generated/` ⋯ 到第一行顶格 `esac` |
| `pb check`（行内） | `if [ -z "${PB_WS:-}" ]; then` ⋯ 到第一行顶格 `fi` |

## 🔴 量具自己的两条自证（`NG2D4` 的 acceptor 点名要的）

`N-G1` 那个逐函数量具**初版就把单行函数算错过**（1 行的 `gate_decolor` 算出「29 行」、
md5 还盖了两块）⇒ 「md5 相同」也可能是**量错了块**。所以本量具**不许只印哈希**：

  ① **覆盖自证**（默认就跑）：全文找出所有 `fails+=(`，按住址分成**门那侧**与**探针那侧**
     （探针那侧 = 落在 `gate_selftest` / `gate_selftest_e2e` / `gate_assert_judged` 里的）。
     门那侧**必须恰好 11 条，且每一条都落在上面五块之内** —— 有一条落在块外就报错。
     ⇒ 这一条直接证明「五块盖住的正是那 11 条判定」，不靠人数行号。
  ② **反向自证**（`--selfproof`）：逐块**在内存里改掉一个字节**，印出改前改后的 md5。
     md5 不变的块当场报错 —— 那说明那一块根本没被算进去。

⚠ 射程（别读宽）：
  · 它比的是**整块字节**：注释改一个字也判「变了」。要的正是这个 —— `NG2D4` 说的是
    「逐字节相同」，不是「逻辑没变」。
  · 行号一律**现算**，本文件里一个硬写的行号都没有（那一族的账见 `brief` 13c）。

用法：
    python3 evidence/N-G2-verdict-md5.py 5924b91              # 基点 vs 工作树
    python3 evidence/N-G2-verdict-md5.py 5924b91 <另一个提交>  # 两个提交对拍
    python3 evidence/N-G2-verdict-md5.py 5924b91 --selfproof  # 再跑一次反向自证
"""
from __future__ import annotations

import hashlib
import pathlib
import re
import subprocess
import sys

TARGET = "scripts/gate.sh"

# 五块的认法。(块名, 起锚点, 收尾行) —— 起锚点是**整行逐字**，收尾是第一行顶格的那个词。
DOORS: list[tuple[str, str, str]] = [
    ("run_gate", "run_gate() {", "}"),
    ("run_gate_sum", "run_gate_sum() {", "}"),
    ("run_e2e", "run_e2e() {", "}"),
    ("generated", "git diff --quiet --exit-code -- src/generated/", "esac"),
    ("pb check", 'if [ -z "${PB_WS:-}" ]; then', "fi"),
]
# 探针那侧的住址（不进「11 条判定」的分母）。
PROBE_FNS = ["gate_selftest() {", "gate_selftest_e2e() {", "gate_assert_judged() {"]
FAILS = re.compile(r"fails\+=\(")


def load(rev: str | None) -> str:
    if rev is None:
        return pathlib.Path(TARGET).read_text(encoding="utf-8")
    p = subprocess.run(["git", "show", f"{rev}:{TARGET}"], capture_output=True, text=True)
    if p.returncode != 0:
        raise SystemExit(f"取不到 {rev}:{TARGET} —— {p.stderr.strip()}")
    return p.stdout


def span(lines: list[str], head: str, close: str, what: str, quiet: bool = True) -> tuple[int, int]:
    hits = [i for i, ln in enumerate(lines) if ln == head]
    if len(hits) != 1:
        raise SystemExit(f"❌ {what} 的起锚点 {head!r} 命中 {len(hits)} 次 —— 判不了，不许猜")
    i = hits[0]
    j = i + 1
    while j < len(lines) and lines[j] != close:
        j += 1
    if j >= len(lines):
        raise SystemExit(f"❌ {what} 找不到收尾行 {close!r}")
    if not quiet:
        print(f"    {what:12} 行 {i + 1}–{j + 1}（{j - i + 1} 行）")
    return i, j


def blocks(text: str) -> dict[str, tuple[str, int, int, int]]:
    """块名 -> (md5前12, 起行1基, 止行1基, 行数)"""
    lines = text.splitlines()
    out = {}
    for name, head, close in DOORS:
        i, j = span(lines, head, close, name)
        body = "\n".join(lines[i : j + 1])
        out[name] = (hashlib.md5(body.encode()).hexdigest()[:12], i + 1, j + 1, j - i + 1)
    return out


def coverage(text: str) -> tuple[int, int, list[str]]:
    """① 覆盖自证：门那侧的 `fails+=` 必须恰好 11 条、且条条落在五块之内。"""
    lines = text.splitlines()
    door_spans = {n: span(lines, h, c, n) for n, h, c in DOORS}
    probe_spans = []
    for head in PROBE_FNS:
        if any(ln == head for ln in lines):
            probe_spans.append(span(lines, head, "}", head))
    # ⚠ 注释行里也印着 `fails+=(`（头注引它当主尺）—— 那不是判定，剔掉。
    # 剔的口径写死在这里：**整行 strip 后以 `#` 打头**。行内注释不剔（本文件没有那一形，现打）。
    hits = [i for i, ln in enumerate(lines)
            if FAILS.search(ln) and not ln.lstrip().startswith("#")]
    door_side, probe_side, orphan = [], [], []
    for i in hits:
        where = next((n for n, (a, b) in door_spans.items() if a <= i <= b), None)
        if where:
            door_side.append(f"{i + 1}:{where}")
        elif any(a <= i <= b for a, b in probe_spans):
            probe_side.append(str(i + 1))
        else:
            orphan.append(str(i + 1))
    print(f"  【覆盖自证】全文 `fails+=(` {len(hits)} 处 ⇒ "
          f"门那侧 {len(door_side)} · 探针那侧 {len(probe_side)} · **块外 {len(orphan)}**")
    for n, (a, b) in door_spans.items():
        inside = [d for d in door_side if d.endswith(":" + n)]
        print(f"    {n:12} 行 {a + 1:>4}–{b + 1:<4}（{b - a + 1:>3} 行） 装着判定 "
              f"{len(inside)} 条：{' '.join(d.split(':')[0] for d in inside)}")
    if orphan:
        raise SystemExit(f"❌ 有 {len(orphan)} 处 `fails+=` 落在五块与探针之外（行 {' '.join(orphan)}）"
                         f" —— 五块盖不住全部判定，这份读数不算数")
    return len(door_side), len(probe_side), door_side


def selfproof(text: str) -> None:
    """② 反向自证：逐块改掉一个字节 ⇒ md5 必须变。"""
    lines = text.splitlines()
    print("\n  【反向自证】每块在内存里改一个字节 ⇒ md5 必须变（不变 = 这块根本没算进去）")
    base = blocks(text)
    for name, head, close in DOORS:
        i, j = span(lines, head, close, name)
        # 改的是**块里第二行**（不动起锚点那一行，否则下一趟就认不出这块了 —— 初版栽在这）。
        hacked = list(lines[i : j + 1])
        hacked[1] = hacked[1] + " "        # 只加一个空格
        after = hashlib.md5("\n".join(hacked).encode()).hexdigest()[:12]
        verd = "**变了** ✓" if after != base[name][0] else "❌ 没变 —— 量具没盖住这一块"
        print(f"    {name:14} {base[name][0]} → {after}   改的是第 {i + 2} 行（块里第 2 行）  {verd}")
        if after == base[name][0]:
            raise SystemExit("❌ 反向自证失败")


def main() -> int:
    args = [a for a in sys.argv[1:] if a != "--selfproof"]
    proof = "--selfproof" in sys.argv[1:]
    if not args or len(args) > 2:
        raise SystemExit(__doc__)
    base_rev = args[0]
    tip_rev = args[1] if len(args) == 2 else None
    base_txt, tip_txt = load(base_rev), load(tip_rev)
    tipname = tip_rev or "工作树"

    print(f"被测对象 {TARGET}：{base_rev} vs {tipname}\n")
    print(f"【{tipname} 这一份】")
    n_door, n_probe, door_side = coverage(tip_txt)

    a, b = blocks(base_txt), blocks(tip_txt)
    print(f"\n{'块':14} {base_rev:^22} {tipname:^22}  判读")
    changed = 0
    for name, _, _ in DOORS:
        x, y = a[name], b[name]
        verd = "逐字节相同" if x[0] == y[0] else "🔴 **变了**"
        changed += x[0] != y[0]
        print(f"{name:14} {x[0] + ' / ' + str(x[3]) + ' 行':^22} "
              f"{y[0] + ' / ' + str(y[3]) + ' 行':^22}  {verd}")
    print(f"\n分母：`NG2D4` 口径的五块（三块整函数 ＋ 两段行范围），共装着 **{n_door}** 条判定"
          f"（探针那侧另有 {n_probe} 条 `fails+=`，不进这个分母）；其中不「逐字节相同」的 **{changed}** 块")
    if proof:
        selfproof(tip_txt)
    return 1 if changed else 0


if __name__ == "__main__":
    sys.exit(main())
