#!/usr/bin/env bash
# auto-e2e F-E2(daemon-frame 级,后端半场):resume idle 就地复用的**复活清灰**边界——单测碰不到的
# 跨进程/tmux 判活边沿。不需 GUI/SSH:daemon 二进制指向隔离 fixture 跑,读它 stdout 线协议帧。
# **为何 daemon-frame 级是复活断言的诚实天花板**:前端 `[e2e] tab-state ... archived→live` 需整个 app
# 在跑,而 Linux 上 GUI resume 触发经 `launch.rs::launch_powershell_window` 仅 Windows → 必回退剪贴板、
# 绝不执行(结构性,见 e2e/README + resume-suite.sh 头注)。故复活的**执行**由本脚本用真源 builder 造的
# 命令驱动(命令级),复活的**检测**(灰→live)由 daemon 判活边沿断言(后端半场)。
# 序列:
#   gen-idle-tmux(fake-claude 活 + @ccm_sid)        → SessionAdded(sid)      = live
#   (kill fake-claude,tmux 会话留活)                → SessionRemoved(sid)    = 灰(claude 死、tmux 在)
#   tmux 帧仍含 @ccm_sid                              = 灰后端条件(Idle 非 Archive)
#   (跑**真源** buildResumeIntoExistingTmuxCmd 就地 resume,复用原名)
#     → fake-claude 复活(新 pidfile,同 sessionId)   → SessionAdded(sid) 再现 = **复活清灰**(后端边沿)
#   全程 tmux 只有一个 cc-<sid8>(复用,无 -N 孤儿,治 #76)
# 红线:daemon 零改动(只跑它)/ CLAUDE_CONFIG_DIR 隔离绝不碰真 ~/.claude / 不改 TMUX_LS_FMT。
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

E2E_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$E2E_DIR/.." && pwd)"
DAEMON="${CCM_E2E_DAEMON:-$REPO/remote-daemon-proto/target/debug/cc-monitor-remote}"
CLAUDE_DIR="${CCM_E2E_CLAUDE_DIR:-/tmp/e2e-resume-frames}"
FAKE="$E2E_DIR/fake-claude"
DRIVER="$E2E_DIR/resume-cmd-driver.ts"
WORK="$(mktemp -d /tmp/e2e-resume-frames.XXXXXX)"
FRAMES="$WORK/frames.jsonl"
DAEMON_ERR="$WORK/daemon.stderr"

[ -x "$DAEMON" ] || { echo "daemon 二进制不存在/不可执行:$DAEMON"; exit 1; }
command -v tmux >/dev/null || { echo "无 tmux"; exit 1; }

SID="$(cat /proc/sys/kernel/random/uuid)"
SID8="${SID:0:8}"
SESSION="cc-$SID8"
KEEP="cc-e2ekeep-$$"   # 无关 cc-* 会话:§24bis 空 backend 守卫,防最后会话卡灰

pass=0; fail=0
ok()  { echo "  PASS $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }

cleanup() {
  set +e
  [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
  [ -n "${FAKE_PID:-}" ] && kill "$FAKE_PID" 2>/dev/null
  # 复活的 fake-claude(新 pid)
  for pf in "$CLAUDE_DIR"/sessions/*.json; do
    [ -f "$pf" ] || continue
    p="$(awk -F'[:,]' '{for(i=1;i<=NF;i++) if($i ~ /"pid"/){print $(i+1); exit}}' "$pf" 2>/dev/null)"
    [ -n "$p" ] && kill "$p" 2>/dev/null
  done
  tmux kill-session -t "=$SESSION:" 2>/dev/null
  tmux kill-session -t "=$KEEP:" 2>/dev/null
  rm -rf "$CLAUDE_DIR" "$WORK"
}
trap 'cleanup; _gc_sock_cleanup' EXIT

# 轮询帧日志直到出现 pattern(在给定起始行之后),或超时。回显命中行。
wait_line() {  # <startline> <grep-ere> <timeout-s>
  local start="$1" pat="$2" to="$3" i hit
  for ((i=0; i<to*2; i++)); do
    hit="$(tail -n "+$((start+1))" "$FRAMES" | grep -E "$pat" | head -1 || true)"
    [ -n "$hit" ] && { echo "$hit"; return 0; }
    sleep 0.5
  done
  return 1
}
orphan_count() {  # <base>
  local n
  n="$(tmux list-sessions -F '#{session_name}' 2>/dev/null | { grep -cE "^$1-[0-9]+$" || true; })"
  echo "${n:-0}"
}

echo "== F-E2 daemon-frame resume 复活清灰套件 =="
echo "sid=$SID  session=$SESSION  claude_dir=$CLAUDE_DIR"
echo "daemon=$DAEMON"

rm -rf "$CLAUDE_DIR"
mkdir -p "$CLAUDE_DIR/sessions" "$CLAUDE_DIR/projects"
tmux new-session -d -s "$KEEP" "exec sh"

# 造 idle-tmux fixture(初始 live)。CLAUDE_CONFIG_DIR 内联进 tmux 命令串(见 gen-idle-tmux)。
CCM_E2E_FAKE_CLAUDE="$FAKE" CLAUDE_CONFIG_DIR="$CLAUDE_DIR" \
  bash "$E2E_DIR/gen-idle-tmux.sh" "$SID" >/dev/null

# 启动 daemon(隔离 CLAUDE_CONFIG_DIR),stdout=帧。
CLAUDE_CONFIG_DIR="$CLAUDE_DIR" "$DAEMON" >"$FRAMES" 2>"$DAEMON_ERR" &
DAEMON_PID=$!

# ── 1. LIVE:SessionAdded(sid)────────────────────────────────────────────────
SA="$(wait_line 0 "\"kind\":\"session_added\".*$SID" 15)" \
  && ok "SessionAdded(live):$SA" \
  || bad "15s 内未见 SessionAdded($SID)"
TS_LIVE="$(wait_line 0 "\"kind\":\"tmux_sessions\".*$SID" 12)" \
  && ok "TmuxSessions 带 @ccm_sid(live):$(printf '%.140s' "$TS_LIVE")" \
  || bad "12s 内未见含 @ccm_sid 的 tmux_sessions 帧"

# ── 2. GRAY:kill fake-claude(留 tmux)→ SessionRemoved + tmux 帧仍含 @ccm_sid ──
FAKE_PID="$(awk -F'[:,]' '{for(i=1;i<=NF;i++) if($i ~ /"pid"/){print $(i+1); exit}}' "$CLAUDE_DIR"/sessions/*.json)"
echo "-- kill fake-claude pid=$FAKE_PID(claude 退,tmux 会话保留 = 灰)--"
kill "$FAKE_PID" 2>/dev/null || true
FAKE_PID=""
MARK_KILL="$(wc -l <"$FRAMES")"
SR="$(wait_line 0 "\"kind\":\"session_removed\".*$SID" 12)" \
  && ok "SessionRemoved(claude 死 → 灰):$SR" \
  || bad "12s 内未见 SessionRemoved($SID)"
# **E67③（2026-07-31）：这一条原来等的是「kill 之后**新**发一帧 tmux_sessions」，
# P5 之后那一帧根本不会来 —— 而且是**永远**不会来，不是慢。**且它是裸赋值**
# （`TS_GRAY="$(wait_line …)"`，不在 `&&/||` 列表里），`set -e` 会让整个脚本在超时那一刻
# 直接退出，连一行 FAIL 都不打 —— CI 上看到的就是「跑到第三条断言就没了、退出码 1」。
#
# **为什么那一帧不会来**（读代码得到，非猜测，位置逐个给出）：
#   · `watcher.rs:305 initial_tmux_probe` 头注：「P5：一次性初探（取代 P2 那个 8s ticker
#     线程）……之后的每一拍都由事件驱动」；`67653e2` 删掉了 `TMUX_EMIT_INTERVAL`。
#   · 事件源只有三个：`tmux_hook.rs:49 HOOK_EVENTS = [session-created, session-closed,
#     session-renamed]`（→ SIGUSR1 → `watcher.rs:798 Poke`）、pidfd、socket inotify。
#   · **claude 在 pane 里死掉不属于其中任何一个** —— tmux 会话没生、没死、没改名。
#
# 所以本条改成断言它真正想断言的那件事：**claude 死了，但那个 tmux 会话还在，
# 而 monitor 手上那份快照仍把它连同 `@ccm_sid` 列着** ⇒ 灰（Idle）而非 Archive。
# 「手上那份」= 最近一帧，不要求是 kill 之后新发的 —— 生产里 monitor 用的也正是缓存的那份。
#
# **顺带查出一个真问题（已登记 BACKLOG E76，本轮不修）**：P5 之后 `@ccm_sid` 被写入
# （`shared/ccm` 通道 B 回填、`/branch` 漂移）**不触发任何重探**，因为没有「设置用户选项」
# 这种 tmux hook。于是 daemon 的快照可能长期带着**空的** `@ccm_sid`。本套件之所以没暴露它，
# 是因为夹具 `gen-idle-tmux.sh` 在 daemon 启动**之前**就把 `@ccm_sid` 设好了。
GRAY_ALIVE="$(tmux has-session -t "=$SESSION:" 2>/dev/null && echo 1 || echo 0)"
TS_GRAY="$(grep -E '"kind":"tmux_sessions"' "$FRAMES" | tail -1 || true)"
if [ "$GRAY_ALIVE" = 1 ] && printf '%s' "$TS_GRAY" | grep -q "$SID"; then
  ok "claude 死后 tmux 会话仍在、最近一帧仍含 @ccm_sid ⇒ 灰(Idle 非 Archive)"
else bad "claude 死后会话没了($GRAY_ALIVE) 或最近一帧丢了 @ccm_sid(不该)"; fi

# ── 3. REVIVE:跑真源就地 resume 命令(复用原名)→ fake-claude 复活 → SessionAdded 再现 = 清灰 ──
echo "-- 就地 resume(真源 buildResumeIntoExistingTmuxCmd,复用 $SESSION,注入 daemon 所看目录)--"
# configDir = daemon 监视目录 → 复活的 fake-claude pidfile 落这里,daemon 判活得到 = 后端复活。
CMD="$(npx tsx "$DRIVER" into-existing "$SID" "$SESSION" "$FAKE" "$CLAUDE_DIR")"
echo "   cmd: $CMD"
echo "$CMD" | grep -q "send-keys -t =$SESSION: " && ! echo "$CMD" | grep -q "new-session" \
  && ok "resume 命令就地复用 $SESSION、无 new-session(#76)" \
  || bad "resume 命令未就地复用"
MARK_REVIVE="$(wc -l <"$FRAMES")"
timeout 8 bash -c "$CMD" >/dev/null 2>&1 || true
SA2="$(wait_line "$MARK_REVIVE" "\"kind\":\"session_added\".*$SID" 15)" \
  && ok "复活清灰:kill 后再 resume → SessionAdded($SID) 再现(后端灰→live 边沿):$SA2" \
  || bad "15s 内未见复活 SessionAdded($SID)(清灰失败)"

# ── 4. 无孤儿:全程 tmux 只有一个 cc-<sid8>(复用,无 -N)────────────────────────
ORPH="$(orphan_count "$SESSION")"
CNT="$(tmux list-sessions -F '#{session_name}' 2>/dev/null | { grep -cE "^$SESSION$" || true; })"
echo "-- tmux ls: $(tmux list-sessions -F '#{session_name}' 2>/dev/null | grep "^$SESSION" | paste -sd, -)  (孤儿=$ORPH)"
[ "$ORPH" = 0 ] && [ "$CNT" = 1 ] && ok "复活后仍单会话 $SESSION、孤儿数=0(治 #76)" || bad "孤儿数=$ORPH / 会话数=$CNT(不该)"

echo "== 结果:$pass 过 / $fail 败 =="
# G-C：与另外 8 套逐字一致的收尾格式，好让 `e2e/assert-pass-floor.sh` 用同一条正则抓。
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ]
