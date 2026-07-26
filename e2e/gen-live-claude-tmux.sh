#!/usr/bin/env bash
# auto-e2e F-E4(UX 审计 #2 接点):造一个「**正在跑 claude 的活会话**」——cc-<sid8> tmux 里前台
# 命令真实报 `claude`(tmux `pane_current_command`),并带 @ccm_sid 身份标记,进程常驻(不退)。
#
# 为何这样造:fake-claude 尾部 `exec sleep` → pane_current_command=sleep,报不出 `claude`;真正让
# tmux 报 `claude` 需前台是一个**名为 claude 的独立可执行**(coreutils 多合一二进制被重命名会拒跑,
# 见调研)。故:把 `node` 软链成 `claude`,跑一个永不返回的 setInterval → tmux 前台命令 = claude、
# 进程活着 = 真「活 claude」。@ccm_sid 精确身份使它对 findOrphanTmux 判据(不看 command)完全等同
# 于一个真活会话。**这不改 daemon、不改 TMUX_LS_FMT**(只是本地起个 tmux fixture)。
#
# 打印会话名(cc-<sid8>)。红线:仅用于 e2e fixture,套件把它登记进 CCM_E2E_ORPHAN_SCOPE 白名单。
set -euo pipefail

SID="${1:?usage: gen-live-claude-tmux.sh <sid>}"
SID8="${SID:0:8}"
SESSION="cc-$SID8"

NODE_BIN="${CCM_E2E_NODE_BIN:-$(command -v node)}"
[ -n "$NODE_BIN" ] || { echo "gen-live-claude-tmux: node 不在 PATH" >&2; exit 1; }

# claude 软链目录(默认一次性 tmp;可 CCM_E2E_CLAUDE_LINK_DIR 覆盖)。软链名必须字面 claude。
LINK_DIR="${CCM_E2E_CLAUDE_LINK_DIR:-/tmp/e2e-orphan-claudelink}"
mkdir -p "$LINK_DIR"
CLAUDE_LINK="$LINK_DIR/claude"
ln -sf "$NODE_BIN" "$CLAUDE_LINK"

# 永不返回的前台进程(setInterval 空转);exec 保持是 pane 的前台命令 = claude。
tmux new-session -d -s "$SESSION" "exec '$CLAUDE_LINK' -e 'setInterval(function(){}, 1000000000)'"
tmux set-option -t "$SESSION" @ccm_sid "$SID"
echo "$SESSION"
