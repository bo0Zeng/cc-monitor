#!/usr/bin/env bash
# auto-e2e F-E1(全链级):驱 gray-light 生命周期,断言 monitor 日志里的 `[e2e] tab-state` 序列。
# **前置**(同 e2e/f40-suite.sh 契约,见 e2e/README):
#   - Xvfb 上跑着 dev 实例(`npx tauri dev`,DEV 探针内建);
#   - config.json 配了一个 loopback 远端,daemonPath 指向 e2e/daemon-wrapper.sh
#     (把 daemon 的 CLAUDE_CONFIG_DIR 钉到隔离 fixture 目录,防与本地会话双 tab);
#   - 本机可读 monitor 日志(fe_perf/[e2e] 行是断言数据源)。
# 序列(跨进程整链,单测碰不到):
#   建 fixture(fake-claude 活 + @ccm_sid) → app 经 daemon SessionAdded 建 live 远端 tab
#   → kill fake-claude(留 tmux shell) → daemon SessionRemoved + TmuxSessions 仍带 @ccm_sid
#     → emitter 判 Idle → SESSION_IDLE → tabs.markTmuxIdle → `[e2e] tab-state … status=live tmuxIdle=1`(灰)
#   → tmux kill-session(另留一个无关 cc-* 防空 backend 卡灰,§24bis) → @ccm_sid 消失
#     → 收割/对账 retire → SESSION_ENDED → tabs.archiveTab → `[e2e] tab-state … status=archived`
# **status=live tmuxIdle=1 这一行同时证明**:该 tab 变灰前是 live(status 字段)+ 此刻进灰(tmuxIdle=1)。
set -euo pipefail

# ── G-C（解 BACKLOG E41）：把整套件钉在**自己的 tmux server** 上 ──────────────────
# 此前这套件裸调 tmux ⇒ 在开发者机器上会**直接操作默认 socket 上的真实会话**，
# 所以它既进不了 CI 也不敢在有活会话的机器上跑（E41）。
#
# **两件事都必须做，缺一就不隔离**（2026-07-30 本机实测）：
#   ① `unset TMUX` —— 从 tmux 会话里跑这套件时，`$TMUX` 会让客户端连**外层那台 server**
#      并**完全忽略 `TMUX_TMPDIR`**（实测：设了 TMUX_TMPDIR 仍在默认 socket 上建出了会话）。
#      **这才是 E41 的实质**：不只是「缺 `-L`」，是「继承了 `$TMUX`」。
#   ② `TMUX_TMPDIR` 必须是**短路径** —— unix socket 路径上限 108 字节，指向长目录时
#      tmux 报 `File name too long`（实测在 scratchpad 那种长路径上必踩）。
#
# 这样做的好处是**零调用点改动**：套件里 84 处裸 `tmux` 一个都不用改，
# 也自动覆盖它 shell out 出去的东西（`ccm` / `cc-spawn` 内部也是裸调 tmux）。
unset TMUX TMUX_PANE
TMUX_TMPDIR="$(mktemp -d /tmp/e2e-sock.XXXXXX)"; export TMUX_TMPDIR
# 收尾：只用 **`-S <私有 socket>`** 收自己那台（**绝不裸 `kill-server`** —— 万一上面的
# 隔离没生效，裸的那个会打到用户的 server 上）。server 无会话时本就会自己退，这条是兜底。
_gc_sock_cleanup() {
  set +e
  [ -n "${TMUX_TMPDIR:-}" ] && /usr/bin/tmux -S "$TMUX_TMPDIR/tmux-$(id -u)/default" kill-server 2>/dev/null
  [ -n "${TMUX_TMPDIR:-}" ] && rm -rf -- "$TMUX_TMPDIR"
}
# ─────────────────────────────────────────────────────────────────────────────

DISPLAY="${E2E_DISPLAY:-:80}"; export DISPLAY
REPO="$(cd "$(dirname "$0")/.." && pwd)"
LOG="${E2E_LOG:-$(ls -t "$HOME"/.claude/claudecode-frontend/logs/monitor.*.log 2>/dev/null | head -1)}"
CLAUDE_DIR="${CCM_E2E_CLAUDE_DIR:-/tmp/e2e-remote-claude}"
GRAY_WAIT="${E2E_GRAY_WAIT:-30}"    # 灰:daemon 判活轮询(2s)+ TmuxSessions 帧(≤8s)+ emitter
ARCH_WAIT="${E2E_ARCH_WAIT:-40}"    # 归档:kill-session 后 TmuxSessions 帧 + 对账去抖

[ -f "$LOG" ] || { echo "monitor 日志不存在:$LOG(dev 实例在跑吗?)"; exit 1; }

SID="$(cat /proc/sys/kernel/random/uuid)"; SID8="${SID:0:8}"
SESSION="cc-$SID8"; KEEP="cc-e2ekeep-$$"

pass=0; fail=0
ok()  { echo "  PASS $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }

cleanup() {
  set +e
  if [ -n "${FAKE_PID:-}" ]; then kill "$FAKE_PID" 2>/dev/null; fi
  tmux kill-session -t "=$SESSION:" 2>/dev/null
  tmux kill-session -t "=$KEEP:" 2>/dev/null
}
trap 'cleanup; _gc_sock_cleanup' EXIT

# 等 monitor 日志(从 start 行之后)出现匹配 pattern 的行,回显之;超时非零。
wait_log() {  # <startline> <grep-ere> <timeout-s>
  local start="$1" pat="$2" to="$3" i hit
  for ((i=0; i<to*2; i++)); do
    hit="$(tail -n "+$((start+1))" "$LOG" | grep -E "$pat" | tail -1 || true)"
    if [ -n "$hit" ]; then echo "$hit"; return 0; fi
    sleep 0.5
  done
  return 1
}

echo "== F-E1 full-chain gray-light 套件(display $DISPLAY)=="
echo "sid=$SID session=$SESSION claude_dir=$CLAUDE_DIR"
echo "log=$LOG"

# 无关 keepalive tmux 会话(kill fixture 后 backend 仍非空)
tmux new-session -d -s "$KEEP" "exec sh"

MARK="$(wc -l <"$LOG")"

# ── 建 live fixture(隔离目录,与 daemon-wrapper 一致)──────────────────────────
CLAUDE_CONFIG_DIR="$CLAUDE_DIR" CCM_E2E_FAKE_CLAUDE="$REPO/e2e/fake-claude" \
  bash "$REPO/e2e/gen-idle-tmux.sh" "$SID" >/dev/null
echo "-- fixture 已建:$SESSION(等 fake-claude 落 pidfile → app 经 daemon 建 live tab)--"

# fake-claude 在 tmux 内**异步**起,pidfile 晚于 gen-idle-tmux 返回 → 必须**轮询等它出现**
# 再读 pid(否则 glob 竞态读空 → 杀不到 → 不变灰,首跑实测踩中)。pidfile 落地 = live 前置成立。
FAKE_PID=""
for _ in $(seq 1 20); do
  PF="$(ls "$CLAUDE_DIR"/sessions/*.json 2>/dev/null | head -1 || true)"
  if [ -n "$PF" ]; then
    FAKE_PID="$(awk -F'[:,]' '{for(i=1;i<=NF;i++) if($i ~ /"pid"/){print $(i+1); exit}}' "$PF")"
    break
  fi
  sleep 0.5
done
[ -n "$FAKE_PID" ] \
  && ok "live 前置:fake-claude pidfile 落地 pid=$FAKE_PID(app 经 daemon SessionAdded 建 live tab)" \
  || bad "10s 内 fake-claude 未落 pidfile 到隔离目录(fixture 失败)"

# **必须等 app 收到一帧含 @ccm_sid=sid 的 TmuxSessions**(daemon 每 8s 才发一次)再杀 claude,
# 否则 SessionRemoved 到达时 app 的 tmux 账本还没这条 → emitter classify_removed 找不到 @ccm_sid
# → 判 Archive(直接归档)而非 Idle(灰),灰灯永不出现(首跑实测踩中)。留足 > 一个 8s 发帧周期。
TMUX_SETTLE="${E2E_TMUX_SETTLE:-14}"
echo "-- 等 ${TMUX_SETTLE}s 让 app 收到含 @ccm_sid 的 TmuxSessions 帧(daemon 8s 发一次)--"
sleep "$TMUX_SETTLE"

# ── GRAY:kill fake-claude(留 tmux)→ 灰灯 tab-state(status=live tmuxIdle=1)────
echo "-- kill fake-claude pid=${FAKE_PID:-?}(claude 退,tmux shell 留)--"
[ -n "${FAKE_PID:-}" ] && kill "$FAKE_PID" 2>/dev/null || true
FAKE_PID=""
GRAY="$(wait_log "$MARK" "\[e2e\] tab-state sid=$SID8 status=live tmuxIdle=1" "$GRAY_WAIT")" \
  && ok "灰灯(live→gray):$GRAY" \
  || bad "${GRAY_WAIT}s 内未见灰灯 tab-state(sid=$SID8 status=live tmuxIdle=1)"

# ── ARCHIVE:tmux kill-session → archived tab-state ───────────────────────────
echo "-- tmux kill-session $SESSION(@ccm_sid 消失 → 归档)--"
tmux kill-session -t "=$SESSION:" 2>/dev/null || true
ARCH="$(wait_log "$MARK" "\[e2e\] tab-state sid=$SID8 status=archived" "$ARCH_WAIT")" \
  && ok "归档(gray→archived):$ARCH" \
  || bad "${ARCH_WAIT}s 内未见归档 tab-state(sid=$SID8 status=archived)"

echo "== 结果:$pass 过 / $fail 败 =="
# G-C：与另外 8 套逐字一致的收尾格式，好让 `e2e/assert-pass-floor.sh` 用同一条正则抓。
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ]
