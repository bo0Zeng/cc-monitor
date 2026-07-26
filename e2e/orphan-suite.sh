#!/usr/bin/env bash
# auto-e2e F-E4(命令级整合):孤儿 tmux 清理(audit-fixes F05)——真源判据 `findOrphanTmux` + 真源清理
# 编排 `cleanupOrphanTmux`(含新加的可注入 confirm seam) + **真 tmux** fixture。经 orphan-cmd-driver.ts
# import 真 src/tabs.ts(不重写),driver 的 Tauri IPC 边界经 orphan-shims 落到真 tmux(list/kill)。
#
# ★安全红线:这台真机有用户**真实**的 cc-<8hex> / *_cc 会话!套件用 **CCM_E2E_ORPHAN_SCOPE 白名单**
# (仅本轮建的 fixture 会话名)把可见/可杀集合严格限死——findOrphanTmux 永远看不到用户真实会话,
# kill 再加一道 scope+cc- 前缀防御(core.mjs)。random uuid 命名再降撞名概率到零。
#
# 诚实层级:TabManager 方法(findOrphanTmux/cleanupOrphanTmux)的天花板 = 命令级(真判据+真编排+真
# tmux kill/has-session);行为等价 confirm seam 的 DOM 层在 tabs.vitest.ts;GUI 触发(账号菜单
# 「清理孤儿会话…」)Linux 结构性不可达。逐边界标层级见 e2e/README「F-E4」。
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
DRIVER="$REPO/e2e/orphan-cmd-driver.ts"
GEN_IDLE="$REPO/e2e/gen-idle-tmux.sh"
GEN_LIVE="$REPO/e2e/gen-live-claude-tmux.sh"
FAKE="$REPO/e2e/fake-claude"

TAG="$$_$(date +%s)"
REMOTE_DIR="${CCM_E2E_ORPHAN_REMOTE_DIR:-/tmp/e2e-orphan-remote-$TAG}"
export CCM_ORPHAN_KILL_LOG="/tmp/e2e-orphan-kill-$TAG.log"
export CCM_TOAST_LOG="/tmp/e2e-orphan-toast-$TAG.log"
: >"$CCM_ORPHAN_KILL_LOG"; : >"$CCM_TOAST_LOG"

pass=0; fail=0
ok()  { echo "  PASS $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }
SESSIONS=()  # 建过的 fixture 会话名(cleanup 兜底 + 组成 scope 白名单)

cleanup() {
  set +e
  for s in "${SESSIONS[@]:-}"; do [ -n "$s" ] && tmux kill-session -t "$s" 2>/dev/null; done
  # 兜底 kill 隔离目录里 fake-claude 的 pidfile 进程
  for pf in "$REMOTE_DIR"/sessions/*.json; do
    [ -f "$pf" ] || continue
    p="$(awk -F'[:,]' '{for(i=1;i<=NF;i++) if($i ~ /"pid"/){print $(i+1); exit}}' "$pf" 2>/dev/null)"
    [ -n "$p" ] && kill "$p" 2>/dev/null
  done
  rm -rf "$REMOTE_DIR" /tmp/e2e-orphan-claudelink-$TAG 2>/dev/null
}
trap cleanup EXIT

# scope 白名单从当前 SESSIONS 数组即时组装(逗号分隔),export 给 driver→core.mjs。
set_scope() { local IFS=,; export CCM_E2E_ORPHAN_SCOPE="${SESSIONS[*]}"; }
drv() { set_scope; npx tsx "$DRIVER" "$@"; }
has() { tmux has-session -t "$1" 2>/dev/null && echo 1 || echo 0; }
panecmd() { tmux display-message -p -t "$1" '#{pane_current_command}' 2>/dev/null || echo "?"; }

echo "== F-E4 孤儿清理命令级整合套件（真源 findOrphanTmux + cleanupOrphanTmux + 真 tmux）=="
echo "repo=$REPO  remote=$REMOTE_DIR  tag=$TAG"

# ── fixtures ───────────────────────────────────────────────────────────────────
# O1:无 tab 的 cc-<sid8>(带 @ccm_sid)= 真孤儿(gen-idle-tmux,fake-claude 常驻)。
SID_O="$(cat /proc/sys/kernel/random/uuid)"; S_ORPH="cc-${SID_O:0:8}"
rm -rf "$REMOTE_DIR"; mkdir -p "$REMOTE_DIR/sessions" "$REMOTE_DIR/projects"
CLAUDE_CONFIG_DIR="$REMOTE_DIR" CCM_E2E_FAKE_CLAUDE="$FAKE" bash "$GEN_IDLE" "$SID_O" >/dev/null
SESSIONS+=("$S_ORPH")

# LIVE:正跑 claude(command=claude)的 cc-<sid8>(带 @ccm_sid),其 sid 不在 tabs → UX 审计 #2 接点。
SID_L="$(cat /proc/sys/kernel/random/uuid)"; S_LIVE="cc-${SID_L:0:8}"
CCM_E2E_CLAUDE_LINK_DIR="/tmp/e2e-orphan-claudelink-$TAG" bash "$GEN_LIVE" "$SID_L" >/dev/null
SESSIONS+=("$S_LIVE")

# MYWORK:非 cc-* 用户会话(不该被列/杀)。
S_MYWORK="mywork-e2e-$TAG"
tmux new-session -d -s "$S_MYWORK" "sleep 2147483647"
SESSIONS+=("$S_MYWORK")

# PROJCC:<project>_cc（cc-bus 资产,不以 cc- 开头 → isCcmTmuxName 天然排除）。
S_PROJCC="proje2e${TAG}_cc"
tmux new-session -d -s "$S_PROJCC" "sleep 2147483647"
SESSIONS+=("$S_PROJCC")

sleep 0.6
echo "fixtures: orphan=$S_ORPH  live=$S_LIVE(cmd=$(panecmd "$S_LIVE"))  mywork=$S_MYWORK  projcc=$S_PROJCC"
echo "scope-guard: 仅这 ${#SESSIONS[@]} 个会话对 driver 可见/可杀(用户真实 cc-*/​*_cc 会话被白名单隔离)"

# 全部 fixture 应真的在 tmux 里
allup=1
for s in "$S_ORPH" "$S_LIVE" "$S_MYWORK" "$S_PROJCC"; do [ "$(has "$s")" = 1 ] || allup=0; done
[ "$allup" = 1 ] && ok "fixtures 就位（4/4 has-session）" || bad "fixture 未全部起来"

# ── B-scan-mixed:scan(tab={LIVE})→ 只 O1 是孤儿；mywork/projcc/live 都不列（B1列入 + B2 + B3 + B5混合）──
echo "-- B(scan 混合): findOrphanTmux 只数真孤儿，不误列 non-cc / *_cc / 有 tab 的 --"
OUT="$(drv scan aya "$SID_L" 2>&1)"
echo "$OUT" | sed 's/^/   /'
CNT="$(echo "$OUT" | sed -n 's/^COUNT //p')"
echo "$OUT" | grep -q "^ORPHAN $S_ORPH$"    && ok "B1 无 tab 的 $S_ORPH 被列孤儿" || bad "B1 $S_ORPH 未被列孤儿"
echo "$OUT" | grep -q "^ORPHAN $S_MYWORK$"  && bad "B2 误列 non-cc 会话 $S_MYWORK" || ok "B2 non-cc 用户会话 $S_MYWORK 不列孤儿"
echo "$OUT" | grep -q "^ORPHAN $S_PROJCC$"  && bad "B3 误列 *_cc 资产 $S_PROJCC" || ok "B3 <project>_cc（$S_PROJCC）不列孤儿（isCcmTmuxName 只认 cc- 前缀）"
echo "$OUT" | grep -q "^ORPHAN $S_LIVE$"     && bad "B5 有 tab 的 $S_LIVE 被误列" || ok "B5 有活 tab 的 $S_LIVE 不列孤儿"
[ "$CNT" = 1 ] && ok "B5 混合场景计数只数真孤儿（COUNT=1）" || bad "B5 计数=$CNT（期望 1）"

# ── B6 = UX 审计 #2 接点：scan(tab=空)→ 活 claude 的 LIVE 因 sid 不在 tabs 被**误列**孤儿 ──────────
echo "-- B6 (UX 审计 #2 固化现状): 正跑 claude 的会话，sid 不在 tabs → findOrphanTmux 当前是否列孤儿 --"
OUT2="$(drv scan aya - 2>&1)"    # tabSids 空集
echo "$OUT2" | sed 's/^/   /'
LIVECMD="$(panecmd "$S_LIVE")"
if echo "$OUT2" | grep -q "^ORPHAN $S_LIVE$"; then
  ok "B6/#2 现状固化：活 claude 会话 $S_LIVE（pane_current_command=$LIVECMD）被 findOrphanTmux 误列孤儿（判据不看 command，符合 #2 预期）"
else
  bad "B6/#2 现状变了：活 claude $S_LIVE 未被列孤儿（若判据已改，需同步更新 #2 断言）"
fi

# ── B4-reject：cleanup(confirm=()=>false) → no-op，O1 仍在 ─────────────────────────────────
echo "-- B4 confirm 拒绝(()=>false) → no-op（has-session 仍在）--"
BEFORE_O="$(has "$S_ORPH")"
drv cleanup aya "$SID_L" 0 >/dev/null 2>&1 || true
AFTER_O_REJ="$(has "$S_ORPH")"
echo "   $S_ORPH before=$BEFORE_O afterReject=$AFTER_O_REJ"
[ "$BEFORE_O" = 1 ] && [ "$AFTER_O_REJ" = 1 ] && ok "B4 拒绝：孤儿 $S_ORPH 未被杀（仍 has-session）" || bad "B4 拒绝后 $S_ORPH 状态异常"

# ── B4-accept + B1-真删：cleanup(confirm=()=>true, tab={LIVE}) → 只杀 O1，其余不误伤 ───────────
echo "-- B1/B4 confirm 接受(()=>true) → 真删孤儿 O1；LIVE(有 tab)/mywork/projcc 不误伤 --"
drv cleanup aya "$SID_L" 1 >/dev/null 2>&1 || true
sleep 0.3
AFTER_O="$(has "$S_ORPH")"; AFTER_L="$(has "$S_LIVE")"; AFTER_M="$(has "$S_MYWORK")"; AFTER_P="$(has "$S_PROJCC")"
echo "   after-accept: orphan=$AFTER_O live=$AFTER_L mywork=$AFTER_M projcc=$AFTER_P"
[ "$AFTER_O" = 0 ] && ok "B1 孤儿 $S_ORPH 被真删（has-session 消失）" || bad "B1 $S_ORPH 未被删"
[ "$AFTER_L" = 1 ] && ok "B4/#2-缓解 有 tab 的活 $S_LIVE 未被杀（sid 在 tabs → 非孤儿）" || bad "B4 误杀了有 tab 的 $S_LIVE"
[ "$AFTER_M" = 1 ] && ok "B2 non-cc $S_MYWORK 未被杀" || bad "B2 误杀 $S_MYWORK"
[ "$AFTER_P" = 1 ] && ok "B3 *_cc $S_PROJCC 未被杀" || bad "B3 误杀 $S_PROJCC"
# kill 日志应只对 O1 落 kill-ok（纵深防御证据）
KILLED="$(grep -c '^kill-ok ' "$CCM_ORPHAN_KILL_LOG" 2>/dev/null || true)"
grep -q "^kill-ok $S_ORPH$" "$CCM_ORPHAN_KILL_LOG" && [ "$KILLED" = 1 ] && ok "kill 日志：仅 $S_ORPH 一条 kill-ok（scope 防御生效）" || bad "kill 日志异常（kill-ok 数=$KILLED）"

# ── B5-zero：O1 已删，scan(tab={LIVE}) 剩 live/mywork/projcc → COUNT=0；cleanup 亦 no-op ──────────
echo "-- B5 零孤儿：no-op、计数=0 --"
OUT3="$(drv scan aya "$SID_L" 2>&1)"
echo "$OUT3" | sed 's/^/   /'
CNT3="$(echo "$OUT3" | sed -n 's/^COUNT //p')"
[ "$CNT3" = 0 ] && ok "B5 零孤儿 COUNT=0" || bad "B5 零孤儿计数=$CNT3（期望 0）"
drv cleanup aya "$SID_L" 1 >/dev/null 2>&1 || true
[ "$(has "$S_LIVE")" = 1 ] && [ "$(has "$S_MYWORK")" = 1 ] && [ "$(has "$S_PROJCC")" = 1 ] \
  && ok "B5 零孤儿 cleanup no-op（live/mywork/projcc 全存活）" || bad "B5 零孤儿 cleanup 误动了会话"

echo "== 结果:$pass 过 / $fail 败 =="
[ "$fail" -eq 0 ]
