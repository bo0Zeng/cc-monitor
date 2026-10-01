#!/usr/bin/env bash
# auto-e2e F-E2(backend-frame 级,后端半场):resume idle 就地复用的**复活清灰**边界——单测碰不到的
# 跨进程/tmux 判活边沿。不需 GUI/SSH:backend 二进制指向隔离 fixture 跑,读它 stdout 线协议帧。
# **为何 backend-frame 级是复活断言的诚实天花板**:前端 `[e2e] tab-state ... archived→live` 需整个 app
# 在跑,而 Linux 上 GUI resume 触发经 `platform/terminal.rs::launch_powershell_window` 仅 Windows → 必回退剪贴板、
# 绝不执行(结构性,见 tests/e2e/README + resume-suite.sh 头注)。故复活的**执行**由本脚本用生产渲染链造的
# 命令驱动(命令级),复活的**检测**(灰→live)由后端判活边沿断言(后端半场)。
# 序列:
#   gen-idle-tmux(fake-claude 活 + @ccm_sid)        → SessionAdded(sid)      = live
#   (kill fake-claude,tmux 会话留活)                → SessionRemoved(sid)    = 灰(claude 死、tmux 在)
#   SessionState(sid, reconnectable)                  = 灰后端条件(后端会话账本裁,原看 tmux 快照帧)
#   (跑**生产渲染链**〔生产 planResumeIntoExistingTmux → 生产 Rust render_launch_payload〕就地 resume,复用原名)
#     → fake-claude 复活(新 pidfile,同 sessionId)   → SessionAdded(sid) 再现 = **复活清灰**(后端边沿)
#   全程 tmux 只有一个 cc-<sid8>(复用,无 -N 孤儿,治 #76)
# 红线:backend 零改动(只跑它)/ CLAUDE_CONFIG_DIR 隔离绝不碰真 ~/.claude / 不改 TMUX_LS_FMT。
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
# `C7i` 隔离：走**共享原语**（`P0e` 08-12 抽出来的，原本这段在各套件里各抄一份）。
# 它把 `$BIN/tmux` shim 放进 PATH 最前、强插 `-L e2eResumeFrames` —— 漏什么环境变量都打不偏。
# ⚠ 本套件此前靠 `TMUX_TMPDIR` 隔离，那是 `C7i` 逐字禁止的形态
#   （08-11 一条同形态的探针把用户 **9 个真实会话**打没了）。
TMUX_SHIM_SOCK=e2eResumeFrames
# shellcheck source=tests/e2e/tmux-shim.sh
. "$(cd "$(dirname "$0")" && pwd)/tmux-shim.sh"
_gc_sock_cleanup() { tmux_shim_cleanup; }
# ─────────────────────────────────────────────────────────────────────────────

E2E_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$E2E_DIR/../.." && pwd)"
BACKEND="${CCM_E2E_BACKEND:-$REPO/.build/backend/debug/cc-monitor-backend}"
CLAUDE_DIR="${CCM_E2E_CLAUDE_DIR:-/tmp/e2e-resume-frames}"
FAKE="$E2E_DIR/fake-claude"
DRIVER="$E2E_DIR/resume-cmd-driver.ts"
WORK="$(mktemp -d /tmp/e2e-resume-frames.XXXXXX)"
# 〔纪律 25〕启动器路径要过 §47 的字符闸（只许 ASCII 那一族）；仓可能住在非 ASCII 目录（如 `~/文档/`）⇒ 同 `restart-backend-frames.sh`，
#   把 fake-claude 拷进 ASCII 的 $WORK 再当启动器（主树路径下跑，就地 resume 那一步原先 DRIVER_THROW REFUSE）。
cp "$FAKE" "$WORK/fake-claude" && chmod +x "$WORK/fake-claude" && FAKE="$WORK/fake-claude"
FRAMES="$WORK/frames.jsonl"
BACKEND_ERR="$WORK/backend.stderr"

[ -x "$BACKEND" ] || { echo "backend 二进制不存在/不可执行:$BACKEND"; exit 1; }
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
  [ -n "${BACKEND_PID:-}" ] && kill "$BACKEND_PID" 2>/dev/null
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

echo "== F-E2 backend-frame resume 复活清灰套件 =="
echo "sid=$SID  session=$SESSION  claude_dir=$CLAUDE_DIR"
echo "backend=$BACKEND"

rm -rf "$CLAUDE_DIR"
mkdir -p "$CLAUDE_DIR/sessions" "$CLAUDE_DIR/projects"
tmux new-session -d -s "$KEEP" "exec sh"

# 造 idle-tmux fixture(初始 live)。CLAUDE_CONFIG_DIR 内联进 tmux 命令串(见 gen-idle-tmux)。
CCM_E2E_FAKE_CLAUDE="$FAKE" CLAUDE_CONFIG_DIR="$CLAUDE_DIR" \
  bash "$E2E_DIR/gen-idle-tmux.sh" "$SID" >/dev/null

# 启动 backend(隔离 CLAUDE_CONFIG_DIR),stdout=帧。
CLAUDE_CONFIG_DIR="$CLAUDE_DIR" "$BACKEND" -- --stream >"$FRAMES" 2>"$BACKEND_ERR" &
BACKEND_PID=$!

# ── 1. LIVE:SessionAdded(sid)────────────────────────────────────────────────
SA="$(wait_line 0 "\"kind\":\"session_added\".*$SID" 15)" \
  && ok "SessionAdded(live):$SA" \
  || bad "15s 内未见 SessionAdded($SID)"
# 「标签挂着谁」直接问 tmux（经 `-L` shim）；原来看的 `tmux_sessions` 快照帧删了。
TAG="$(tmux show-options -v -t "=$SESSION:" @ccm_sid 2>/dev/null || true)"
[ "$TAG" = "$SID" ] \
  && ok "tmux 会话 $SESSION 挂着 @ccm_sid=$SID(live)" \
  || bad "$SESSION 的 @ccm_sid 不是 $SID（实得 '$TAG'）"

# ── 2. GRAY:kill fake-claude(留 tmux)→ SessionRemoved + tmux 帧仍含 @ccm_sid ──
FAKE_PID="$(awk -F'[:,]' '{for(i=1;i<=NF;i++) if($i ~ /"pid"/){print $(i+1); exit}}' "$CLAUDE_DIR"/sessions/*.json)"
echo "-- kill fake-claude pid=$FAKE_PID(claude 退,tmux 会话保留 = 灰)--"
# 记号必须在 kill **之前**取：后端靠 pidfd 判死、几乎零延迟，kill 之后再数行数，那一拍的 session_state
#   可能已经写进帧日志、落在记号之前 ⇒ 「等不到可重连」（主线 6 过 1 红的根因；与仓路径是不是 ASCII 无关）。
MARK_KILL="$(wc -l <"$FRAMES")"
kill "$FAKE_PID" 2>/dev/null || true
FAKE_PID=""
SR="$(wait_line 0 "\"kind\":\"session_removed\".*$SID" 12)" \
  && ok "SessionRemoved(claude 死 → 灰):$SR" \
  || bad "12s 内未见 SessionRemoved($SID)"
# claude 死、tmux 会话还挂着它 ⇒ 后端会话账本裁「可重连」（灰）。原来 monitor 拿缓存的最后一份 `tmux_sessions` 快照自己裁
# （E67③ 那段：kill 之后不会有新快照，只能读最近一帧）；裁决进了后端、快照帧删了 ⇒ 等成品帧 `session_state`（它随 session_removed 同拍发）。
GRAY_ALIVE="$(tmux has-session -t "=$SESSION:" 2>/dev/null && echo 1 || echo 0)"
SS_GRAY="$(wait_line "$MARK_KILL" "\"kind\":\"session_state\".*$SID" 12 || true)"  # 从 kill 那一刻之后找（它就是这一拍的裁决）
if [ "$GRAY_ALIVE" = 1 ] && printf '%s' "$SS_GRAY" | grep -q '"state":"reconnectable"'; then
  ok "claude 死后 tmux 会话仍在、后端裁可重连 ⇒ 灰(非已结束):$SS_GRAY"
else bad "claude 死后会话没了($GRAY_ALIVE) 或没裁成可重连(不该):$SS_GRAY"; fi

# ── 3. REVIVE:跑真源就地 resume 命令(复用原名)→ fake-claude 复活 → SessionAdded 再现 = 清灰 ──
echo "-- 就地 resume(生产渲染链 planResumeIntoExistingTmux → render_launch_payload,复用 $SESSION,注入后端所看目录)--"
# configDir = backend 监视目录 → 复活的 fake-claude pidfile 落这里,backend 判活得到 = 后端复活。
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
# G-C：与另外 8 套逐字一致的收尾格式，好让 `tests/e2e/assert-pass-floor.sh` 用同一条正则抓。
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ]
