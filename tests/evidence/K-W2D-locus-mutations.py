#!/usr/bin/env python3
"""K-W2D `KW2D3` 的**死值验台**：在沙箱容器里对**仓副本**跑那条判据，逐刀记读数。

为什么变异台跑在**副本**上而不是工作树上
----------------------------------------
本条判据同时读**两棵树**（daemon 那棵 + monitor 那棵，后者是运行时读）
⇒ 要让它红，得动 monitor 侧的源码，而 monitor 侧**不在本件写区**。
在副本上切刀 ⇒ 工作树一个字节没动（交回时用三处 `git status` 对账），
也不会与同波别的道撞面。

⚠ 判定行的口径：本台跑的是**单条测试过滤**（`cargo test <过滤>`），
   给出的 `passed/failed` 只是那几条的数，**不是门禁的九格读数** ——
   门禁那份另跑，别把两个数混着报（本仓最贵那族病：量具的作用域对不上事实）。

用法
----
  python3 evidence/K-W2D-locus-mutations.py <repo-mut 目录> <target 目录> [过滤词]
默认过滤词 `panorama_locus`（本条判据那 5 格全在这个前缀下）。
"""

import re
import subprocess
import sys
from pathlib import Path

IMG = "ccmon-devbox:latest"
CARGO_VOLUME = "ccmon-cargo-registry"

SCRIPT = r"""
set -uo pipefail
export CARGO_HOME=/m/cargo-home CARGO_TARGET_DIR=/t
cd /m/repo-mut/remote-daemon-proto
cargo test --offline %(filter)s 2>&1 | tail -40
"""


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    mut, tgt = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
    flt = sys.argv[3] if len(sys.argv) > 3 else "panorama_locus"
    tgt.mkdir(parents=True, exist_ok=True)
    out = subprocess.run(
        ["docker", "run", "--rm", "--network", "none", "--user", "root",
         "-v", f"{mut.parent}:/m", "-v", f"{tgt}:/t",
         "-v", f"{CARGO_VOLUME}:/m/cargo-home/registry",
         "-e", "HOME=/root", IMG,
         "bash", "-o", "pipefail", "-c", SCRIPT % {"filter": flt}],
        capture_output=True, text=True)
    text = out.stdout + out.stderr
    print(text)
    bars = re.findall(r"test result: (\w+)\. (\d+) passed; (\d+) failed", text)
    print(f"---- 判定行 {len(bars)} 行：{bars}  · 退出码 {out.returncode}")
    if not bars:
        print("⚠ 一行判定行都没有 ⇒ 按 CRASH 记，**不许报「新红 0」**")
    return 0


if __name__ == "__main__":
    sys.exit(main())
