#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R5 · 实现拍（C）的量具 ①：**整道 cargo 门** 与 **只跑本条** 两个分母。

住址（唯一）：`<工作树>/evidence/K-R5-C-r7-door.py`
被测对象：**本量具所在的那棵工作树**（`--wt` 默认取本文件的 `../`），
          09-02 这一拍是 `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r5`，分支 `track/k-r5`。
          ⚠ `brief` 12 那一条：住址 + 被测对象指向哪棵树，两样都写在这里。

**为什么不直接跑 `.claude/devbox/gate`**：那是**七格**全量门禁（npm / e2e / pb check / …），
一刀变异要跑 41 次，全量跑不完；而本量具只要 `gate.sh` 里 **cargo 那一格**的四个数。
⇒ 本量具**逐字复刻** `scripts/gate.sh` 的三件东西（复刻，不是引用；`gate.sh` 是 `K-R11` 的面，不许改）：
  · `set -uo pipefail`（`gate.sh` 里那一行 —— 见 `--show-anchors`，认**函数名/命令**不认行号）
  · `run_gate_sum()` 的三支判定：`rc != 0` / `pkgs != want_pkgs` / `-z n || n -eq 0`
  · 它喂进去的那条命令：`cd src-tauri && cargo test --workspace --exclude code-picture-core --lib`

🔴 **一律在沙箱里跑**（`docker run ccmon-devbox:latest`，挂载与 `.claude/devbox/gate` 同形），
   **宿主上零测试**。`CARGO_TARGET_DIR` 只落 `.claude/pm-targets/<tag>`，绝不 `/tmp`。

用法：
  python3 evidence/K-R5-C-r7-door.py --label 形0-基线
  python3 evidence/K-R5-C-r7-door.py --label F2刀 --net host
  python3 evidence/K-R5-C-r7-door.py --show-anchors      # 只印它复刻的那几处在 gate.sh 今天的住址
"""
from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import subprocess
import sys
import time

PROJ = pathlib.Path("/home/zbl/文档/claudecode-frontend")
IMAGE = "ccmon-devbox:latest"
CARGO_CACHE = "ccmon-cargo-registry"
SKILL = pathlib.Path("/home/zbl/.claude-accts/z/skills/planned-build")

# `run_gate_sum cargo 8 …` 那个 8。加/删 crate 就来改这个数（与 gate.sh 同一条纪律）。
WANT_PKGS = 8
DOOR_CMD = (
    "cd src-tauri && cargo test --workspace --exclude code-picture-core --lib 2>&1"
)
ONE_CMD = (
    "cd src-tauri && cargo test --workspace --exclude code-picture-core --lib "
    "the_build_time_execution_surface_stays_registered 2>&1"
)
TESTNAME = "the_build_time_execution_surface_stays_registered"

RESULT_RE = re.compile(r"^test result: ok\. (\d+) passed", re.M)


def gate_anchors(wt: pathlib.Path) -> list[str]:
    """把本量具复刻的那几处，在**今天**的 `scripts/gate.sh` 里现打一次住址。

    🔴 **行号是快照**（`brief` 12 / `K-R9` `§3`）：`K-G3` 09-01 往 `gate.sh` 加了门六，
    把行号整体推下去，头注里四处引用当场全馊。⇒ 本函数**按内容找**，
    印出来的行号是「打这一次时的读数」，不是可以抄进任何文档的常量。
    """
    src = (wt / "scripts" / "gate.sh").read_text(encoding="utf-8").splitlines()
    wanted = [
        ("set -uo pipefail", lambda s: s.strip() == "set -uo pipefail"),
        ("run_gate_sum() 定义", lambda s: s.startswith("run_gate_sum()")),
        ("src-tauri 那条 cargo", lambda s: "run_gate_sum cargo" in s and "cd src-tauri" in s),
        ("remote-daemon-proto 那条 cargo", lambda s: "cd remote-daemon-proto && cargo test" in s),
    ]
    out = []
    for name, pred in wanted:
        hits = [i + 1 for i, s in enumerate(src) if pred(s)]
        out.append(f"  {name}: 命中 {len(hits)} 处，今天在 {hits}")
    return out


def run(wt: pathlib.Path, tag: str, net: str, cmd: str) -> tuple[int, str, float]:
    targets = PROJ / ".claude" / "pm-targets" / tag
    targets.mkdir(parents=True, exist_ok=True)
    argv = [
        "docker", "run", "--rm",
        "--network", net,
        "-v", f"{PROJ}:{PROJ}",
        "-v", f"{SKILL}:{SKILL}:ro",
        "-v", f"{CARGO_CACHE}:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={targets}",
        "-e", "HOME=/home/zbl",
        "-w", str(wt),
        IMAGE,
        # 逐字复刻 gate.sh 那一行 `set -uo pipefail`（`bash -o pipefail` + `set -u`）
        # 🔴 那个 `mkdir` 不是装饰：`.claude/devbox/gate` 最后一行逐字有它，
        #    少了它 `history::tests::the_delete_entry_point_actually_goes_through_the_fence`
        #    会因为「`~/.claude/projects` 不存在」被拒（**不是被围栏拒的**）⇒ 基线假红。
        #    09-02 现打过这个反例：不带 mkdir ⇒ `rc=101` · `pkgs=5` · `n=88` · 1 条 FAILED。
        "bash", "-o", "pipefail", "-c",
        f'set -u; mkdir -p "$HOME/.claude/projects"; {cmd}',
    ]
    t0 = time.time()
    p = subprocess.run(argv, capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr, time.time() - t0


def verdict(rc: int, out: str) -> dict:
    """逐字复刻 `run_gate_sum` 的三支判定（顺序也一样：rc → pkgs → n）。"""
    nums = RESULT_RE.findall(out)
    pkgs = len(nums)
    n = sum(int(x) for x in nums) if nums else 0
    if rc != 0:
        return dict(pkgs=pkgs, n=n, verdict="红", why=f"退出码 {rc}")
    if pkgs != WANT_PKGS:
        return dict(pkgs=pkgs, n=n, verdict="红", why=f"只跑到 {pkgs} 个包，应当 {WANT_PKGS}")
    if n == 0:
        return dict(pkgs=pkgs, n=n, verdict="红", why="读数是 0 —— 0 passed 不是绿")
    return dict(pkgs=pkgs, n=n, verdict="绿", why="")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--wt", default=str(pathlib.Path(__file__).resolve().parent.parent))
    ap.add_argument("--tag", default="k-r5-c1")
    ap.add_argument("--net", default="none", choices=["none", "host"])
    ap.add_argument("--label", default="(无标签)")
    ap.add_argument("--mode", default="door", choices=["door", "one"])
    ap.add_argument("--show-anchors", action="store_true")
    ap.add_argument("--brief", action="store_true", help="一行读数（变异表逐行用这个）")
    ap.add_argument("--save", default="")
    a = ap.parse_args()

    wt = pathlib.Path(a.wt).resolve()
    if a.show_anchors:
        print(f"# `scripts/gate.sh` 今天（现打）那几处的住址 —— 被测对象 {wt}")
        for line in gate_anchors(wt):
            print(line)
        return 0

    cmd = DOOR_CMD if a.mode == "door" else ONE_CMD
    rc, out, dt = run(wt, a.tag, a.net, cmd)
    v = verdict(rc, out)
    # 本条自己有没有开火 / 有没有被过滤掉（形 2s 那一族的自检）
    ours_failed = f"{TESTNAME} ... FAILED" in out or f"{TESTNAME} ... FAILED" in out.replace("::", "::")
    ours_hits = out.count(TESTNAME)
    filtered = re.findall(r"(\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out", out)
    rec = dict(
        label=a.label, mode=a.mode, wt=str(wt), tag=a.tag, net=a.net,
        rc=rc, seconds=round(dt, 1), **v,
        本条命中次数=ours_hits, 本条FAILED=ours_failed,
        FAILED行数=out.count(" ... FAILED"),
        filtered_out=filtered[:3],
    )
    if a.brief:
        print(
            f"{a.label}: rc={rc} · 包数={v['pkgs']} · 合计={v['n']} · 判定={v['verdict']}"
            f" · 本条FAILED={ours_failed} · FAILED行数={rec['FAILED行数']}"
            f" · {dt:.0f}s{('（' + v['why'] + '）') if v['why'] else ''}"
        )
    else:
        print(json.dumps(rec, ensure_ascii=False, indent=2))
    if a.save:
        pathlib.Path(a.save).write_text(out, encoding="utf-8")
        # 🔴 落到 stderr，不许污染 stdout —— 上一版把这一行印进 stdout，
        #    下游 `json.load` 当场 `Extra data`。（09-02 现打过一次。）
        print(f"# 原始输出落盘：{a.save}（{len(out)} 字节）", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
