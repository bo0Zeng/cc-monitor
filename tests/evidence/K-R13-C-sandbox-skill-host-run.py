#!/usr/bin/env python3
"""K-R13 实现拍（C）的第二件量具 —— **在沙箱里单跑 `skill_host` 那几条判据**，
并且能把那条 shim **在容器里盖掉**（宿主上那条一个字节都不动）。

住址：<工作树>/evidence/K-R13-C-sandbox-skill-host-run.py
被测对象：**默认是 `.claude/worktrees/k-r13`**（可用 `--wt <绝对路径>` 改）。
          ⚠ 复跑前先读这一行：本量具**不是**指向主树的。

为什么要它（两条，都是门禁给不了的）：
  ① `scripts/gate.sh` 的 `run_gate_sum` 只把 `cargo` 的**退出码**写进 `fails`，
     **panic 正文一个字都不印** ⇒ 「哪一格红、红成什么样」在门禁日志里读不出来。
     而本件第一条验收要的正是「改之前先跑一遍确认它是红的」。
  ② 本件验收 4 要「把 shim 盖掉、那两条判据仍然绿」。
     🔴 宿主上那条 shim **有 4 个依赖者、今天拆不了**（`K-R11` 此刻实现不了）⇒
     只能在**容器自己的挂载命名空间**里盖：往 `<proj>/.claude/worktrees/.claude`
     挂一层空 tmpfs。容器里那条 shim 就此不存在，**宿主那条毫发无伤**（挂载不写盘）。
     这比「拷一份副本」更干净：拷 git 工作树还得先删副本里的 `.git`
     （brief 12c，否则会写进原树的暂存区）。

跑法（宿主上跑本文件；它自己 docker run 进沙箱）：
    python3 evidence/K-R13-C-sandbox-skill-host-run.py                 # 正常跑
    python3 evidence/K-R13-C-sandbox-skill-host-run.py --mask-shim     # 容器里盖掉 shim 再跑
    python3 evidence/K-R13-C-sandbox-skill-host-run.py --filter <name> # 只跑某一条
    python3 evidence/K-R13-C-sandbox-skill-host-run.py --show-mask     # 只打印它会怎么挂，不跑

⚠ 它与门禁**共用** `CARGO_TARGET_DIR`（`.claude/pm-targets/k-r13-c1`）⇒
   **不许与门禁同时跑**（cargo 会在 target 锁上排队，读数会互相拖）。
"""

import subprocess
import sys
from pathlib import Path

PROJ = Path("/home/zbl/文档/claudecode-frontend")
IMAGE = "ccmon-devbox:latest"
SKILL = Path("/home/zbl/.claude-accts/z/skills/planned-build")
CARGO_CACHE = "ccmon-cargo-registry"
# 那条 shim 的**父目录**（`.claude/worktrees/.claude`）—— 盖它比盖 symlink 本身干净：
# 挂在 symlink 上 docker 会去解析它，等于把真计划目录挂了一遍。
SHIM_PARENT = PROJ / ".claude" / "worktrees" / ".claude"


def main():
    argv = sys.argv[1:]
    wt = PROJ / ".claude" / "worktrees" / "k-r13"
    if "--wt" in argv:
        wt = Path(argv[argv.index("--wt") + 1])
    tag = "k-r13-c1"
    if "--tag" in argv:
        tag = argv[argv.index("--tag") + 1]
    filt = "skill_host"
    if "--filter" in argv:
        filt = argv[argv.index("--filter") + 1]
    mask = "--mask-shim" in argv

    targets = PROJ / ".claude" / "pm-targets" / tag
    cmd = [
        "docker", "run", "--rm", "--network", "none",
        "-v", f"{PROJ}:{PROJ}",
        "-v", f"{SKILL}:{SKILL}:ro",
        "-v", f"{CARGO_CACHE}:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={targets}",
        "-e", "HOME=/home/zbl",
    ]
    if mask:
        # 空 tmpfs 盖住 shim 的父目录 —— **只在这个容器的挂载命名空间里**。
        cmd += ["--mount", f"type=tmpfs,destination={SHIM_PARENT}"]
    cmd += [
        "-w", str(wt), IMAGE,
        "bash", "-o", "pipefail", "-c",
        # 先把「shim 到底在不在」现打一次印出来 —— 别让读者靠猜。
        f'echo "--- 容器里看到的 shim ---"; ls -la "{SHIM_PARENT}" 2>&1 | head -5; '
        f'echo "--- 容器里 {SHIM_PARENT.name}/planned-build 存在吗 ---"; '
        f'test -e "{SHIM_PARENT}/planned-build" && echo "在" || echo "不在（已盖掉）"; '
        f'echo "--- cargo test --lib {filt} ---"; '
        f"cd src-tauri && cargo test --lib {filt} 2>&1",
    ]

    if "--show-mask" in argv:
        print(" \\\n  ".join(cmd))
        return 0

    print(f"[被测工作树] {wt}")
    print(f"[target]     {targets}")
    print(f"[盖 shim]    {mask}")
    p = subprocess.run(cmd)
    print(f"[退出码] {p.returncode}")
    return p.returncode


if __name__ == "__main__":
    sys.exit(main())
