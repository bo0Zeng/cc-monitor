#!/bin/sh
# auto-e2e F-E0:loopback-remote 的 daemon 包装器(测试 fixture,**非** daemon 改动)。
# 把 daemon 的 $CLAUDE_CONFIG_DIR 钉到一个一次性隔离目录(默认 /tmp/e2e-remote-claude),
# 让 app 经 loopback SSH 连上来时读的是 fixture 而**不是**真实 ~/.claude——否则本机会话会
# 同时以「本地 tab」和「远端 tab」双份出现(§ 双 tab)。config.json 的 daemonPath 指向本脚本即可。
# 默认 daemon 二进制 = 仓内 debug 构建;CCM_E2E_DAEMON / CCM_E2E_CLAUDE_DIR 可覆盖。
#
# ★重要(实测,F-E1 全链):app **会自动部署** daemon——若 daemonPath 同目录没有匹配当前
#   app 期望 build_id 的 `.build_id` 标记文件,app 会把内嵌 daemon 二进制**覆盖写到 daemonPath**
#   (把本脚本冲掉!)。故全链跑法:把本脚本(或其副本)放进一个目录,旁边放一个 `.build_id`
#   (内容 = app 期望的 daemon build_id,如 `p1p-tmux-frame`),再把 daemonPath 指向它 →
#   deploy_decision=Skip、脚本存活。(见 src-tauri/src/sftp.rs::deploy_decision +
#   ssh_source EXPECTED_DAEMON_BUILD_ID)
E2E_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO=$(CDPATH= cd -- "$E2E_DIR/.." && pwd)
: "${CCM_E2E_CLAUDE_DIR:=/tmp/e2e-remote-claude}"
: "${CCM_E2E_DAEMON:=$REPO/remote-daemon-proto/target/debug/cc-monitor-remote}"
# 仓内 debug 构建优先(CI 场景);缺失(如 worktree 未构建)时退到已部署/已构建的二进制。
# 经 SSH exec 本脚本时 env 不带 CCM_E2E_*,故默认必须能自愈到一个真存在的 daemon。
if [ ! -x "$CCM_E2E_DAEMON" ]; then
  # 顺序:仓内 release > app 已部署 bin/(随 app 更新,较新) > e2e/(可能陈旧,或缺 tmux_sessions 帧)。
  for c in \
    "$REPO/remote-daemon-proto/target/release/cc-monitor-remote" \
    "$HOME/.cc-monitor/bin/cc-monitor-remote" \
    "$HOME/.cc-monitor/e2e/cc-monitor-remote"; do
    if [ -x "$c" ]; then CCM_E2E_DAEMON="$c"; break; fi
  done
fi
exec env CLAUDE_CONFIG_DIR="$CCM_E2E_CLAUDE_DIR" "$CCM_E2E_DAEMON" "$@"
