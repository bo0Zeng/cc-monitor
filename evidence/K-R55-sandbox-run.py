#!/usr/bin/env python3
"""K-R55 量具⑤：把一条命令串放进门禁那个沙箱里跑（挂载与 `.claude/devbox/gate` 同一套）。

住址：本文件（仓内 `evidence/`）。**被测对象指向 `.claude/worktrees/k-r55` 那棵树** ——
`brief` 第 12 条要的那一栏：同一住址下先后住过两份被测对象不同的量具，是一次静默的假读数。
用法：`python3 evidence/K-R55-sandbox-run.py '<容器里要跑的 bash 命令串>'`。
不写成 `.sh` 的理由同 `evidence/K-R55-flaky-loop.py`。
"""
import os, subprocess, sys

PROJ = "/home/zbl/文档/claudecode-frontend"
SKILL = "/home/zbl/.claude-accts/z/skills/planned-build"
WT = f"{PROJ}/.claude/worktrees/k-r55"
TARGET = f"{PROJ}/.claude/pm-targets/k-r55"


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 3
    cmd = sys.argv[1]
    argv = [
        "docker", "run", "--rm",
        "--network", os.environ.get("DEVBOX_NET", "none"),
        "-v", f"{PROJ}:{PROJ}",
        "-v", f"{SKILL}:{SKILL}:ro",
        "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={TARGET}",
        "-e", "HOME=/home/zbl",
        "-w", WT,
        "ccmon-devbox:latest",
        "bash", "-o", "pipefail", "-c",
        'mkdir -p "$HOME/.claude/projects"; ' + cmd,
    ]
    return subprocess.call(argv)


if __name__ == "__main__":
    raise SystemExit(main())
