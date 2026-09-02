#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-H2b 第十轮（C 第十轮）量具：在**沙箱里**跑快道 `cargo test -p monitor --lib`。

住址：`evidence/K-H2b-C10-sbx-fastlane.py`（本件本轮独占的名字）。
被测对象**由 argv 给出**，脚本里没有硬编码任何一棵树：

    python3 evidence/K-H2b-C10-sbx-fastlane.py <工作树绝对路径> <target 目录名> [cargo 过滤参数…]

🔴 它**只跑测试，一个字节的源码都不改** —— 本轮所有源码改动一律走 `Edit`/`Write`。
🔴 宿主上零测试（用户 08-29 的硬条款）⇒ 一切在 `ccmon-devbox:latest` 里跑。
   挂载与 `.claude/devbox/gate` 逐项同形（项目目录 · 那条硬写的 skill 路径 · crates 缓存卷），
   **不挂** `/tmp/tmux-1000` · `~/.cc-monitor` · `$HOME` 其余部分。

# 为什么不写成 `.sh`

第一版写成了 `evidence/K-H2b-C10-sbx-fastlane.sh`，当场被本仓一条真判据逮住：
`shell_lint_registry::tests::every_shell_script_is_either_linted_or_registered_as_exempt`
—— 仓里新增的 shell 脚本必须进 lint 名单或豁免登记，而那份登记**不在本轮写区**。
⇒ 换成 `.py`（`evidence/` 下已有 7 份 `.py` 先例，那条判据不管它们）。
**这一格如实记在交回里，不是「顺手换个后缀」。**

# 网络口径（**这一格必须对账，别默认**）

`.claude/devbox/gate` 的 `DEVBOX_NET` 默认 `none`。本轮入场门禁是用 `DEVBOX_NET=host`
跑的，快道要与它同口径 ⇒ 本脚本默认 `--network host`，可用 `CCM_NET` 覆盖。
`--network none` 下 `ssh_source::tier1_tests::race_watchdog_times_out_on_blackhole`
会**假红**（现打：`Network is unreachable (os error 101)`，它等的是超时不是 unreachable）
⇒ 换网络口径就等于换分母，**两个分母的读数不许混着报**。
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
    net = os.environ.get("CCM_NET", "host")
    inner = (
        'mkdir -p "$HOME/.claude/projects" && cd src-tauri && '
        f"cargo test -p monitor --lib {extra} 2>&1 | tail -30"
    )
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
