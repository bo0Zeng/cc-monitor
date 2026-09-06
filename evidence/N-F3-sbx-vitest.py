#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""N-F3 量具：在**沙箱里**窄跑 vitest（`npm run test:dom` 的一小块）。

住址：`evidence/N-F3-sbx-vitest.py`（本件独占的名字 —— 风险 5k：同名量具被别人覆盖过，
照原住址复跑会得到另一棵树上的数，而输出长得一模一样）。
被测对象**由 argv 给出**，脚本里没有硬编码任何一棵树：

    python3 evidence/N-F3-sbx-vitest.py <工作树绝对路径> <target 目录名> [vitest 参数…]

例：
    python3 evidence/N-F3-sbx-vitest.py \\
        /home/zbl/文档/claudecode-frontend/.claude/worktrees/n-f3 n-f3 \\
        src/first-run-hint.vitest.ts

🔴 它**只跑测试，一个字节的源码都不改** —— 本轮所有源码改动一律走 `Edit`/`Write`。
🔴 宿主上零测试（用户 08-29 的硬条款）⇒ 一切在 `ccmon-devbox:latest` 里跑。
   挂载与 `.claude/devbox/gate` 逐项同形（项目目录 · 那条硬写的 skill 路径 · crates 缓存卷），
   **不挂** `/tmp/tmux-1000` · `~/.cc-monitor` · `$HOME` 其余部分。

# 为什么是 `.py` 不是 `.sh`

`K-H2b-C10-sbx-fastlane.py` 的头注逐字记着：仓里新增 shell 脚本会被
`shell_lint_registry::tests::every_shell_script_is_either_linted_or_registered_as_exempt`
逮住（要么进 lint 名单要么进豁免登记），而那份登记不在本轮写区。⇒ 照它写成 `.py`。

# 这把尺子买到什么、买不到什么（诚实边界）

- 买到：**单个 `*.vitest.ts` 的逐条 pass/fail**。死值验（`§3` 五刀）要的就是这个粒度 ——
  整跑一趟门禁看不出「红的是哪一条」。
- **买不到**：门禁的九格。九格只有 `.claude/devbox/gate` 那一条命令算数，
  这把尺子的读数**不许**拿去顶替门禁读数。
- 网络：默认 `--network none`（与 `.claude/devbox/gate` 的 `DEVBOX_NET` 缺省同口径）。
  vitest 不需要网；换口径就等于换分母，`CCM_NET` 可覆盖但两个分母的读数不许混报。
"""

import os
import subprocess
import sys

PROJ = "/home/zbl/文档/claudecode-frontend"
SKILL = "/home/zbl/.claude-accts/z/skills/planned-build"


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    wt, tag = sys.argv[1], sys.argv[2]
    extra = " ".join(sys.argv[3:])
    net = os.environ.get("CCM_NET", "none")
    # ⚠ `tail` 的行数要可调：死值验用 `--reporter=verbose` 时逐条测试名会把尾巴顶出去，
    #   而「红了哪几条」正是这把尺子唯一要买的东西 —— 截掉了就只剩「红了」。
    tail = os.environ.get("CCM_TAIL", "60")
    inner = f"npx vitest run {extra} 2>&1 | tail -{tail}"
    cmd = [
        "docker", "run", "--rm", "--network", net,
        "-v", f"{PROJ}:{PROJ}",
        "-v", f"{SKILL}:{SKILL}:ro",
        "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={PROJ}/.claude/pm-targets/{tag}",
        "-e", "HOME=/home/zbl",
        "-w", wt,
        "ccmon-devbox:latest",
        "bash", "-o", "pipefail", "-c", inner,
    ]
    print(f"· 被测对象 {wt}  · target {tag}  · --network {net}", flush=True)
    return subprocess.call(cmd)


if __name__ == "__main__":
    sys.exit(main())
