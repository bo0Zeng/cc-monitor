#!/usr/bin/env bash
# auto-e2e F-E0:造一个「可变灰」的 @ccm_sid tmux 会话——cc-<sid8> 里跑 fake-claude(该 sid)。
#   - fake-claude 前台 sleep,pane 持有它 → backend 判活 SessionAdded(live) + tmux ls 带 @ccm_sid;
#   - `; exec sh`:kill fake-claude(读 pidfile pid)后 pane 落回 shell,**tmux 会话仍在、@ccm_sid 仍在**
#     → 灰灯态(claude 退但 tmux 未亡);随后 `tmux kill-session` 才 → archived。
# 只在调用方装了 shim 时跑(见下面开头那一道)。打印会话名(cc-<sid8>)。TMUX_LS_FMT / @ccm_sid 是后端与 monitor 双写点,勿改格式(红线)。
set -euo pipefail

# 隔离只靠调用方 PATH 上那份强插 `-L` 的 shim（`tmux-shim.sh` 装的，或套件自建的同形那一份：`exec <tmux> -L <名> "$@"`）。
# PATH 上的 tmux 不是它 ⇒ 一条 tmux 都不起、退 2：下面的裸 `tmux` 会落到缺省 server（开发机上是用户正在用的那台）。
# 认的是 shim 本身，不认 `TMUX_SHIM_SOCK`：那个名字 `tmux-shim.sh` 不 export，被调起的本脚本看不见。
_tmux="$(command -v tmux || true)"
if [ -z "$_tmux" ] || ! grep -qE '^exec .* -L [^ ]+ "\$@"$' "$_tmux" 2>/dev/null; then
  echo "gen-idle-tmux.sh：PATH 上的 tmux（${_tmux:-没有}）不是强插私有选择器的 shim —— 不造，免得落到缺省 server（先 . tmux-shim.sh <前缀>）" >&2
  exit 2
fi

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
tmux set-option -t "=$SESSION:" @ccm_sid "$SID"
echo "$SESSION"
