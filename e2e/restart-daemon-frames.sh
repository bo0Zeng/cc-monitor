#!/usr/bin/env bash
# auto-e2e F-E3(daemon-frame 级,后端半场):换号重启的**跨账号迁移**边界——单测碰不到的跨进程/判活边沿。
# 不需 GUI/SSH:两个 daemon 二进制分别指向**旧账号**与**新账号**的隔离 CLAUDE_CONFIG_DIR 跑,读其 stdout
# 线协议帧。换号 = 会话从旧账号目录**消失**、在新账号目录**出现**,恰好由两个 daemon 的判活边沿钉住:
#   make_live(旧号目录,fake-claude 活)         → daemon_OLD: SessionAdded(sid)      = 旧号持有
#                                                daemon_NEW: 无 SessionAdded(sid)     = 新号尚无
#   驱动**真源** restartWithAccount 换号到 znew(经 restart-cmd-driver + shims,真 kill + 真 resume):
#     旧进程被 kill                             → daemon_OLD: SessionRemoved(sid)     = 旧号失去（exit→kill 后端半场）
#     新进程在**新号目录**resume 起来           → daemon_NEW: SessionAdded(sid)       = 新号获得（resume 落新账号,后端半场）
#   全程 tmux 只有一个 cc-<sid8>（复用,无 -N 孤儿）
#
# **为何 daemon-frame 级是换号迁移的诚实天花板**:前端 tab 徽章翻转需整个 app 在跑,而 Linux 上 GUI 触发经
# launch.rs::launch_powershell_window 仅 Windows → 必回退剪贴板、绝不执行(结构性)。故换号的**执行**由真源
# restartWithAccount 编排驱动(命令级),换号的**检测**(旧号失去/新号获得)由两个 daemon 判活边沿断言。
# 红线:daemon 零改动(只跑它)/ CLAUDE_CONFIG_DIR 隔离绝不碰真 ~/.claude / 只 kill 本套件建的 cc-<sid8>。
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

E2E="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$E2E/.." && pwd)"
FAKE="$E2E/fake-claude"
DRV="$E2E/restart-cmd-driver.ts"

# daemon 自愈:仓内 debug > release > app 已部署 bin > e2e（同 daemon-wrapper.sh 顺序）。
DAEMON="${CCM_E2E_DAEMON:-}"
if [ -z "$DAEMON" ]; then
  for c in \
    "$REPO/remote-daemon-proto/target/debug/cc-monitor-remote" \
    "$REPO/remote-daemon-proto/target/release/cc-monitor-remote" \
    "$HOME/.cc-monitor/bin/cc-monitor-remote" \
    "$HOME/.cc-monitor/e2e/cc-monitor-remote"; do
    [ -x "$c" ] && { DAEMON="$c"; break; }
  done
fi
# U0（2026-08-01）：这条 SKIP **不是**静默绿 —— 本套件不打印 `合计 PASS=`，
# `assert-pass-floor.sh:51-54` 抓不到那行就会判红。但诊断说的不是真因，同 npx / script(1) 那两处。
[ -n "$DAEMON" ] && [ -x "$DAEMON" ] || {
  echo "SKIP: 无可用 daemon 二进制（未构建且无已部署）——环境缺前置，不是套件退化。"
  echo "      本套件不打印「合计 PASS=」，故 assert-pass-floor 会以「抓不到断言数」判红；"
  echo "      真因是这里。先 cargo build 出 daemon 或部署一个，再跑。命令级由 restart-suite.sh 覆盖。"
  exit 0
}
command -v tmux >/dev/null || { echo "无 tmux"; exit 1; }

WORK="$(mktemp -d /tmp/e2e-restart-frames.XXXXXX)"
OLD="$WORK/acct-old"; NEW="$WORK/acct-new"
OLD_FR="$WORK/old.frames.jsonl"; NEW_FR="$WORK/new.frames.jsonl"
mkdir -p "$OLD/sessions" "$OLD/projects" "$NEW/sessions" "$NEW/projects" /tmp/e2e-remote
KEEP="cc-e2ekeep-$$"

pass=0; fail=0
ok()  { echo "  PASS $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }

# fresh sid,守卫不撞真 cc-* 会话
for _ in 1 2 3 4 5; do SID="$(cat /proc/sys/kernel/random/uuid)"; S="cc-${SID:0:8}"; tmux has-session -t "=$S:" 2>/dev/null || break; done

cleanup() {
  set +e
  [ -n "${DP_OLD:-}" ] && kill "$DP_OLD" 2>/dev/null
  [ -n "${DP_NEW:-}" ] && kill "$DP_NEW" 2>/dev/null
  for d in "$OLD" "$NEW"; do for pf in "$d"/sessions/*.json; do [ -f "$pf" ] || continue
    p="$(awk -F'[:,]' '{for(i=1;i<=NF;i++) if($i ~ /"pid"/){print $(i+1); exit}}' "$pf" 2>/dev/null)"; [ -n "$p" ] && kill "$p" 2>/dev/null; done; done
  tmux kill-session -t "=$S:" 2>/dev/null
  tmux kill-session -t "=$KEEP:" 2>/dev/null
  rm -rf "$WORK"
}
trap 'cleanup; _gc_sock_cleanup' EXIT

wait_line() {  # <file> <ere> <timeout-s>
  local f="$1" pat="$2" to="$3" i hit
  for ((i=0; i<to*2; i++)); do
    hit="$(grep -E "$pat" "$f" 2>/dev/null | head -1 || true)"
    [ -n "$hit" ] && { echo "$hit"; return 0; }
    sleep 0.5
  done
  return 1
}
orphan_count() { local n; n="$(tmux list-sessions -F '#{session_name}' 2>/dev/null | { grep -cE "^$1-[0-9]+$" || true; })"; echo "${n:-0}"; }

echo "== F-E3 daemon-frame 换号迁移套件（旧号失去 / 新号获得，真源 restartWithAccount 驱动）=="
echo "sid=$SID  session=$S  daemon=$DAEMON"
echo "old(acct bold)=$OLD  new(acct znew)=$NEW"

tmux new-session -d -s "$KEEP" "exec sh"

# 两个 daemon:分别监视旧号 / 新号目录。
CLAUDE_CONFIG_DIR="$OLD" "$DAEMON" >"$OLD_FR" 2>"$WORK/old.err" & DP_OLD=$!
CLAUDE_CONFIG_DIR="$NEW" "$DAEMON" >"$NEW_FR" 2>"$WORK/new.err" & DP_NEW=$!

# ── 1. 旧号持有:make_live 在旧号目录 → daemon_OLD SessionAdded；daemon_NEW 无 ──────────────────
CLAUDE_CONFIG_DIR="$OLD" CCM_E2E_FAKE_CLAUDE="$FAKE" bash "$E2E/gen-idle-tmux.sh" "$SID" >/dev/null
SA_OLD="$(wait_line "$OLD_FR" "\"kind\":\"session_added\".*$SID" 15)" \
  && ok "旧号 daemon 见 SessionAdded(live):$(printf '%.120s' "$SA_OLD")" \
  || bad "15s 内旧号 daemon 未见 SessionAdded($SID)"
if grep -qE "\"kind\":\"session_added\".*$SID" "$NEW_FR" 2>/dev/null; then bad "换号前新号 daemon 就见到该 sid（不该）"; else ok "换号前新号 daemon 无该 sid（新号尚未持有）"; fi

# ── 2. 驱动真源换号重启到 znew（新号目录）:真 kill 旧、真 resume 到新号 ────────────────────────
echo "-- 驱动真源 restartWithAccount 换号 → znew（kill 旧进程 + resume 到新号目录）--"
ACCTS='{"available":true,"error":null,"meta":null,"accounts":[{"name":"znew","email":"","configDir":"'"$NEW"'","isDefault":true,"mode":"isolated","exists":true,"loggedIn":true}]}'
OUT="$(CCM_ACCOUNTS_JSON="$ACCTS" CCM_SEQ_LOG="$WORK/seq.log" CCM_TOAST_LOG="$WORK/toast.log" \
  npx tsx "$DRV" restart aya "$SID" /tmp/e2e-remote "$S" znew "$FAKE" 0 1 1 1)"
echo "   $(echo "$OUT" | paste -sd' ' -)  | seq: $(paste -sd' ' -<"$WORK/seq.log")"

# ── 3. 旧号失去（SessionRemoved = exit→kill 后端半场）+ 新号获得（SessionAdded = resume 落新账号）──
SR_OLD="$(wait_line "$OLD_FR" "\"kind\":\"session_removed\".*$SID" 15)" \
  && ok "旧号 daemon 见 SessionRemoved（旧进程被 kill,旧账号失去该会话）:$(printf '%.120s' "$SR_OLD")" \
  || bad "15s 内旧号 daemon 未见 SessionRemoved($SID)"
SA_NEW="$(wait_line "$NEW_FR" "\"kind\":\"session_added\".*$SID" 15)" \
  && ok "新号 daemon 见 SessionAdded（resume 落**新账号目录**,新账号获得该会话 = 换号迁移完成）:$(printf '%.120s' "$SA_NEW")" \
  || bad "15s 内新号 daemon 未见 SessionAdded($SID)（换号迁移未到位）"

# ── 4. 无孤儿:全程单会话 cc-<sid8>（复用原名）────────────────────────────────────────────────
ORPH="$(orphan_count "$S")"
echo "-- tmux ls: $(tmux list-sessions -F '#{session_name}' 2>/dev/null | grep "^$S" | paste -sd, -)  (孤儿=$ORPH)"
[ "$ORPH" = 0 ] && ok "换号后无 cc-<sid8>-N 孤儿（复用原名）" || bad "孤儿数=$ORPH（不该）"

echo "== 结果:$pass 过 / $fail 败 =="
# G-C：与另外 8 套逐字一致的收尾格式，好让 `e2e/assert-pass-floor.sh` 用同一条正则抓。
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ]
