#!/usr/bin/env bash
# auto-e2e F-E1(fallback 级):**daemon 线上帧**断言 gray-light 的后端半场——单测碰不到的
# 跨进程/tmux 判活边界。不需 GUI/SSH:直接把 daemon 二进制指向隔离 fixture 跑,读它 stdout 的
# 线协议帧(JSON-per-line,`{"kind":...}`)。序列:
#   SessionAdded(sid)               —— fake-claude 活 + @ccm_sid 已置 → live
#   (kill fake-claude 进程,tmux 会话留活)
#   SessionRemoved(sid)             —— daemon 2s 判活轮询发现 claude 死
#   TmuxSessions.raw 仍含 @ccm_sid   —— **关键**:claude 死但 tmux 还在 → monitor emitter 据此走
#                                       Idle(灰)而非 Archive(前端半场:markTmuxIdle→tmuxIdle=1)
#   (tmux kill-session)
#   TmuxSessions.raw 不再含 sid      —— @ccm_sid 没了 → monitor 归档触发边沿(archived)
# 前端 emitter→灰灯半场由单测(tabs.vitest.ts)+ 全链 GUI 跑覆盖;本脚本钉住后端边沿。
# 红线:daemon 零改动(只跑它) / 不碰真 ~/.claude(CLAUDE_CONFIG_DIR 隔离) / 不改 TMUX_LS_FMT。
set -euo pipefail

E2E_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$E2E_DIR/.." && pwd)"
DAEMON="${CCM_E2E_DAEMON:-$REPO/remote-daemon-proto/target/debug/cc-monitor-remote}"
CLAUDE_DIR="${CCM_E2E_CLAUDE_DIR:-/tmp/e2e-remote-claude}"
WORK="$(mktemp -d /tmp/e2e-graylight.XXXXXX)"
FRAMES="$WORK/frames.jsonl"
DAEMON_ERR="$WORK/daemon.stderr"

[ -x "$DAEMON" ] || { echo "daemon 二进制不存在/不可执行:$DAEMON"; exit 1; }
command -v tmux >/dev/null || { echo "无 tmux"; exit 1; }

SID="$(cat /proc/sys/kernel/random/uuid)"
SID8="${SID:0:8}"
SESSION="cc-$SID8"
KEEP="cc-e2ekeep-$$"   # 无关 cc-* 会话:kill 掉 fixture 后 backend 仍非空(§24bis 空 backend 守卫)

pass=0; fail=0
ok()  { echo "  PASS $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }

cleanup() {
  set +e
  [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
  # fixture pane 的 fake-claude(若还活)+ 两个 tmux 会话
  if [ -n "${FAKE_PID:-}" ]; then kill "$FAKE_PID" 2>/dev/null; fi
  tmux kill-session -t "$SESSION" 2>/dev/null
  tmux kill-session -t "$KEEP" 2>/dev/null
  rm -rf "$CLAUDE_DIR" "$WORK"
}
trap cleanup EXIT

echo "== F-E1 daemon-frame gray-light 套件 =="
echo "sid=$SID  session=$SESSION  claude_dir=$CLAUDE_DIR"
echo "daemon=$DAEMON"

# 干净 fixture 目录
rm -rf "$CLAUDE_DIR"
mkdir -p "$CLAUDE_DIR/sessions" "$CLAUDE_DIR/projects"

# 无关 keepalive tmux 会话(空 shell)
tmux new-session -d -s "$KEEP" "exec sh"

# 造 fixture:cc-<sid8> 跑 fake-claude(该 sid)+ 置 @ccm_sid。**导出 CLAUDE_CONFIG_DIR**——
# gen-idle-tmux 会把它内联进 tmux 命令串,让 fake-claude 落 pidfile 到隔离 fixture 而非真 ~/.claude。
CCM_E2E_FAKE_CLAUDE="$E2E_DIR/fake-claude" CLAUDE_CONFIG_DIR="$CLAUDE_DIR" \
  bash "$E2E_DIR/gen-idle-tmux.sh" "$SID" >/dev/null

# 启动 daemon(隔离 CLAUDE_CONFIG_DIR),stdout=帧,stderr 分离
CLAUDE_CONFIG_DIR="$CLAUDE_DIR" "$DAEMON" >"$FRAMES" 2>"$DAEMON_ERR" &
DAEMON_PID=$!

# 轮询帧日志直到出现 pattern(在给定起始行之后),或超时。回显命中行。
# 用法:wait_line <startline> <grep-ere> <timeout-s> <desc>
wait_line() {
  local start="$1" pat="$2" to="$3" desc="$4" i hit
  for ((i=0; i<to*2; i++)); do
    hit="$(tail -n "+$((start+1))" "$FRAMES" | grep -E "$pat" | head -1 || true)"
    if [ -n "$hit" ]; then echo "$hit"; return 0; fi
    sleep 0.5
  done
  return 1
}

# ── 1. LIVE:SessionAdded(sid)────────────────────────────────────────────────
SA="$(wait_line 0 "\"kind\":\"session_added\".*$SID" 15 'session_added')" \
  && { ok "SessionAdded(live):$SA"; } \
  || { bad "15s 内未见 SessionAdded($SID)"; }

# tmux 帧带 @ccm_sid(claude 活时)
TS_LIVE="$(wait_line 0 "\"kind\":\"tmux_sessions\".*$SID" 12 'tmux_sessions live')" \
  && ok "TmuxSessions 带 @ccm_sid(live):$(printf '%.160s' "$TS_LIVE")" \
  || bad "12s 内未见含 @ccm_sid 的 tmux_sessions 帧"

# ── 2. GRAY:杀 fake-claude(留 tmux 会话)→ SessionRemoved + tmux 帧仍含 @ccm_sid ──
FAKE_PID="$(awk -F'[:,]' '{for(i=1;i<=NF;i++) if($i ~ /"pid"/){print $(i+1); exit}}' "$CLAUDE_DIR"/sessions/*.json)"
echo "-- kill fake-claude pid=$FAKE_PID(claude 退出,tmux 会话保留)--"
kill "$FAKE_PID" 2>/dev/null || true
FAKE_PID=""  # 已杀,cleanup 不再重复

MARK_KILL="$(wc -l <"$FRAMES")"
SR="$(wait_line 0 "\"kind\":\"session_removed\".*$SID" 12 'session_removed')" \
  && ok "SessionRemoved(claude 死):$SR" \
  || bad "12s 内未见 SessionRemoved($SID)"

# kill 之后的**新** tmux 帧:必须仍含 @ccm_sid(claude 死但 tmux 未亡 = 灰灯后端条件)
TS_GRAY="$(wait_line "$MARK_KILL" "\"kind\":\"tmux_sessions\"" 12 'tmux frame post-kill')"
if [ -n "$TS_GRAY" ]; then
  if printf '%s' "$TS_GRAY" | grep -q "$SID"; then
    ok "claude 死后 tmux 帧仍含 @ccm_sid ⇒ 灰(Idle 非 Archive):$(printf '%.160s' "$TS_GRAY")"
  else
    bad "claude 死后 tmux 帧丢了 @ccm_sid(不该):$TS_GRAY"
  fi
else
  bad "kill 后 12s 内无新 tmux_sessions 帧"
fi

# ── 3. ARCHIVE:tmux kill-session → 新 tmux 帧不再含 sid ───────────────────────
echo "-- tmux kill-session $SESSION(@ccm_sid 消失 → 归档触发)--"
tmux kill-session -t "$SESSION" 2>/dev/null || true
MARK_KS="$(wc -l <"$FRAMES")"
TS_ARCH="$(wait_line "$MARK_KS" "\"kind\":\"tmux_sessions\"" 14 'tmux frame post-kill-session')"
if [ -n "$TS_ARCH" ]; then
  if printf '%s' "$TS_ARCH" | grep -q "$SID"; then
    bad "kill-session 后 tmux 帧仍含 sid(不该):$TS_ARCH"
  else
    ok "kill-session 后 tmux 帧不再含 @ccm_sid ⇒ 归档触发边沿:$(printf '%.160s' "$TS_ARCH")"
  fi
else
  bad "kill-session 后 14s 内无新 tmux_sessions 帧"
fi

echo "== 结果:$pass 过 / $fail 败 =="
[ "$fail" -eq 0 ]
