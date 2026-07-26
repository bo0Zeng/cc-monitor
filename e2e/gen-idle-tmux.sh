#!/usr/bin/env bash
# auto-e2e F-E0:造一个「可变灰」的 @ccm_sid tmux 会话——cc-<sid8> 里跑 fake-claude(该 sid)。
#   - fake-claude 前台 sleep,pane 持有它 → daemon 判活 SessionAdded(live) + tmux ls 带 @ccm_sid;
#   - `; exec sh`:kill fake-claude(读 pidfile pid)后 pane 落回 shell,**tmux 会话仍在、@ccm_sid 仍在**
#     → 灰灯态(claude 退但 tmux 未亡);随后 `tmux kill-session` 才 → archived。
# 打印会话名(cc-<sid8>)。TMUX_LS_FMT / @ccm_sid 是 daemon 与 monitor 双写点,勿改格式(红线)。
set -euo pipefail

SID="${1:?usage: gen-idle-tmux.sh <sid>}"
SID8="${SID:0:8}"
SESSION="cc-$SID8"
E2E_DIR="$(cd "$(dirname "$0")" && pwd)"
FAKE="${CCM_E2E_FAKE_CLAUDE:-$E2E_DIR/fake-claude}"

# new-session 经 `sh -c "<cmd>"` 跑:先 fork 子跑 fake-claude(execs sleep,pidfile 记该子 PID),
# 该子被 kill 后 sh -c 续跑 `exec sh` → pane 存活。**CLAUDE_CONFIG_DIR 必须内联进命令串**:
# tmux new-session 起的 shell 拿的是 tmux server 环境、**不继承**本 shell 的 CLAUDE_CONFIG_DIR
# (老坑,曾使 fake-claude 落到真 ~/.claude)。CCM_FAKE_SID 同理显式传,免 shim 再解析。
CCD="${CLAUDE_CONFIG_DIR:-}"
CMD="CCM_FAKE_SID='$SID' '$FAKE' '$SID'; exec sh"
[ -n "$CCD" ] && CMD="CLAUDE_CONFIG_DIR='$CCD' $CMD"
tmux new-session -d -s "$SESSION" "$CMD"
tmux set-option -t "$SESSION" @ccm_sid "$SID"
echo "$SESSION"
