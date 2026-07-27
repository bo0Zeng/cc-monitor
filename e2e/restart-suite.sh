#!/usr/bin/env bash
# auto-e2e F-E3(命令级整合):换号重启编排（#68/#69）——`compact → exit → kill → resume(新账号)` 序列 +
# resume 落**新账号的 CLAUDE_CONFIG_DIR** + §5.2 失败语义。驱动**真源** src/account-restart.ts
# `restartWithAccount`（经 restart-cmd-driver.ts + restart-shims/ 把 Tauri IPC 边界重定向到真 tmux +
# fake-claude,见那两个文件头注),逐边界断言编排真正发出的命令序列 / resume argv / 账号解析 / 失败语义。
#
# **诚实天花板 = 命令级**:GUI 全链在 Linux 结构性不可达(`launch.rs::launch_powershell_window` 仅
# Windows→回退剪贴板、绝不执行)。本套件测的是**真编排逻辑 + 真 tmux 效果 + 真账号解析**,唯一替换的
# 是那道无法在 Linux 触达、本该由后端 Rust 执行 tmux 的 IPC 边界(见 e2e/README + resume-suite.sh 头注)。
# 批量对齐 alignAllToCurrentAccount 的 idle/busy 分桶是 TabManager DOM 方法,其诚实天花板 = DOM(jsdom)级,
# 由 src/tabs.vitest.ts「account-ux U6」块覆盖(单独 vitest 跑);本套件在**命令级**钉 confirm 闸门
# (放行 / 拦下,B4/B1/B2),两者互补。
#
# 红线:daemon 零改(不跑它) / 隔离 CLAUDE_CONFIG_DIR 绝不碰真 ~/.claude / 只 kill 本套件建的 cc-<sid8>。
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
E2E="$REPO/e2e"
FAKE="$E2E/fake-claude"
GEN="$E2E/gen-idle-tmux.sh"
DRV="$E2E/restart-cmd-driver.ts"

WORK="$(mktemp -d /tmp/e2e-restart.XXXXXX)"
OLD="$WORK/acct-old"      # 旧账号 CLAUDE_CONFIG_DIR（account "bold"）
NEW="$WORK/acct-new"      # 新账号 CLAUDE_CONFIG_DIR（account "znew"，换号目标）
CWD_DIR="/tmp/e2e-remote"
mkdir -p "$OLD/sessions" "$OLD/projects" "$NEW/sessions" "$NEW/projects" "$CWD_DIR"

# 两账号 fixture（都可选:isolated + loggedIn + exists）。znew=换号目标、bold=旧号。
ACCTS='{"available":true,"error":null,"meta":null,"accounts":[{"name":"bold","email":"","configDir":"'"$OLD"'","isDefault":false,"mode":"isolated","exists":true,"loggedIn":true},{"name":"znew","email":"","configDir":"'"$NEW"'","isDefault":true,"mode":"isolated","exists":true,"loggedIn":true}]}'
export CCM_ACCOUNTS_JSON="$ACCTS"

pass=0; fail=0
ok()  { echo "  PASS $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }
SESSIONS=()

cleanup() {
  set +e
  for s in "${SESSIONS[@]:-}"; do [ -n "$s" ] && tmux kill-session -t "=$s:" 2>/dev/null; done
  for d in "$OLD" "$NEW"; do
    for pf in "$d"/sessions/*.json; do
      [ -f "$pf" ] || continue
      p="$(awk -F'[:,]' '{for(i=1;i<=NF;i++) if($i ~ /"pid"/){print $(i+1); exit}}' "$pf" 2>/dev/null)"
      [ -n "$p" ] && kill "$p" 2>/dev/null
    done
  done
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v tmux >/dev/null || { echo "无 tmux"; exit 1; }

# fresh sid,守卫:绝不撞已存在的**真** cc-* 会话（本机常有真 Claude 在跑）。
new_sid() {
  local sid s
  for _ in 1 2 3 4 5; do
    sid="$(cat /proc/sys/kernel/random/uuid)"; s="cc-${sid:0:8}"
    tmux has-session -t "=$s:" 2>/dev/null || { echo "$sid"; return 0; }
  done
  echo "new_sid: 连续撞名(不该)" >&2; return 1
}

# 造一个**活**远端会话（fake-claude 常驻,不 kill）在指定账号目录下。回显会话名。
make_live() {  # <sid> <dir>
  local sid="$1" dir="$2" sess="cc-${1:0:8}"
  CLAUDE_CONFIG_DIR="$dir" CCM_E2E_FAKE_CLAUDE="$FAKE" bash "$GEN" "$sid" >/dev/null
  SESSIONS+=("$sess")
  sleep 0.4
  echo "$sess"
}

# SEQ 里只保留主序列步（compact/exit/kill/resume），空格连成一行。pipefail 安全。
seq_core() { { grep -E '^(compact|exit|kill|resume)$' "$1" 2>/dev/null || true; } | paste -sd' ' -; }
# 等某账号目录 argv.log 出现该 sid 的 --resume 行,回显之。超时非零。
wait_argv() {  # <dir> <sid> <timeout-s>
  local dir="$1" sid="$2" to="$3" i hit
  for ((i=0; i<to*2; i++)); do
    hit="$(grep -E "sid=$sid .*argv=--resume $sid" "$dir/argv.log" 2>/dev/null | tail -1 || true)"
    [ -n "$hit" ] && { echo "$hit"; return 0; }
    sleep 0.5
  done
  return 1
}
session_alive() { tmux has-session -t "=$1:" 2>/dev/null && echo 1 || echo 0; }

# drive_restart <sid> <session> <account> <compactFirst> <confirm> <seqfile> <toastfile> [extra env...]
drive_restart() {
  local sid="$1" sess="$2" acct="$3" cf="$4" cfm="$5" seqf="$6" toastf="$7"; shift 7
  : >"$seqf"; : >"$toastf"
  env "$@" CCM_SEQ_LOG="$seqf" CCM_TOAST_LOG="$toastf" \
    npx tsx "$DRV" restart aya "$sid" "$CWD_DIR" "$sess" "$acct" "$FAKE" "$cf" "$cfm" 1 1
}

echo "== F-E3 换号重启编排 命令级整合套件（真源 restartWithAccount + 真 tmux + fake-claude）=="
echo "repo=$REPO  old(acct bold)=$OLD  new(acct znew)=$NEW  cwd=$CWD_DIR"

# ── B1:restart 账号 znew + compactFirst=true → compact→exit→kill→resume + argv 落新账号目录 ──────
echo "-- B1 compactFirst=true:序列 compact→exit→kill→resume + resume argv CLAUDE_CONFIG_DIR=新账号 --"
SID1="$(new_sid)"; S1="cc-${SID1:0:8}"
make_live "$SID1" "$OLD" >/dev/null
OUT1="$(drive_restart "$SID1" "$S1" znew 1 1 "$WORK/b1.seq" "$WORK/b1.toast")"
echo "   driver: $(echo "$OUT1" | paste -sd' ' -)"
CORE1="$(seq_core "$WORK/b1.seq")"
echo "   seq(core): [$CORE1]"
[ "$CORE1" = "compact exit kill resume" ] && ok "B1 序列 = compact→exit→kill→resume（真编排发出的有序命令）" || bad "B1 序列=[$CORE1]（期望 compact exit kill resume）"
echo "$OUT1" | grep -q "^RESULT true$" && ok "B1 restartWithAccount 返回 true（真拉起）" || bad "B1 RESULT≠true"
echo "$OUT1" | grep -q "^CONFIGDIR $NEW$" && ok "B1 真 accountConfigDir 解析 znew → $NEW（非旧号 $OLD）" || bad "B1 CONFIGDIR≠新账号目录"
if AL="$(wait_argv "$NEW" "$SID1" 12)"; then
  echo "   argv(new): $AL"
  echo "$AL" | grep -q "CLAUDE_CONFIG_DIR=$NEW " && ok "B1 resume argv 落**新账号** CLAUDE_CONFIG_DIR=$NEW" || bad "B1 resume argv 目录≠新账号"
else bad "B1 12s 内新账号目录 argv.log 无 resume 行"; fi
if grep -qE "sid=$SID1 .*argv=--resume" "$OLD/argv.log" 2>/dev/null; then bad "B1 串号:resume 泄漏到**旧账号**目录($OLD)"; else ok "B1 隔离:旧账号目录无该 sid 的 resume（换号未落回旧号）"; fi

# ── B2:restart 无 compact → 直接 exit→kill→resume（无 compact 等待）──────────────────────────
echo "-- B2 compactFirst=false:直接 exit→kill→resume（序列无 compact）--"
SID2="$(new_sid)"; S2="cc-${SID2:0:8}"
make_live "$SID2" "$OLD" >/dev/null
OUT2="$(drive_restart "$SID2" "$S2" znew 0 1 "$WORK/b2.seq" "$WORK/b2.toast")"
CORE2="$(seq_core "$WORK/b2.seq")"
echo "   seq(core): [$CORE2]"
[ "$CORE2" = "exit kill resume" ] && ok "B2 序列 = exit→kill→resume（无 compact 等待）" || bad "B2 序列=[$CORE2]（期望 exit kill resume）"
{ grep -qx "compact" "$WORK/b2.seq" 2>/dev/null && bad "B2 不该发 /compact 却发了"; } || ok "B2 全程未发 /compact（未勾选 compact）"
echo "$OUT2" | grep -q "^RESULT true$" && ok "B2 返回 true" || bad "B2 RESULT≠true"
wait_argv "$NEW" "$SID2" 12 >/dev/null && ok "B2 resume argv 落新账号目录" || bad "B2 无 resume argv"

# ── B3:mismatch 检测（活会话账号 ≠ origin 当前号）→ restart 后清零 ────────────────────────────
echo "-- B3 mismatch（真 detectAccountMismatch）restart 前后对比 --"
# 前:活会话在旧号 bold、当前工作账号 znew → 不一致 true。（B1 已换号,那个 sid 现活在 znew。）
BEFORE="$(CCM_ACCOUNTS_JSON="$ACCTS" npx tsx "$DRV" mismatch bold znew)"
AFTER="$(CCM_ACCOUNTS_JSON="$ACCTS" npx tsx "$DRV" mismatch znew znew)"
echo "   detectAccountMismatch(before live=bold, current=znew)=$BEFORE ; (after live=znew, current=znew)=$AFTER"
[ "$BEFORE" = "true" ] && [ "$AFTER" = "false" ] && ok "B3 mismatch 换号后清零（true→false）——B1 argv 已证 sid 现活在 znew" || bad "B3 mismatch 未按预期翻转（before=$BEFORE after=$AFTER）"

# ── B4:取消 confirm（()=>false）→ no-op:不 kill 不 resume,argv.log 无新行,会话仍活 ─────────────
echo "-- B4 取消 confirm ()=>false → no-op（不 kill/不 resume/argv 无新行）--"
SID4="$(new_sid)"; S4="cc-${SID4:0:8}"
make_live "$SID4" "$NEW" >/dev/null    # 活在新号,resume 若误发会往 NEW 追行——正好用来验"无新行"
BEFORE_LINES="$(wc -l <"$NEW/argv.log" 2>/dev/null || echo 0)"
OUT4="$(drive_restart "$SID4" "$S4" znew 0 0 "$WORK/b4.seq" "$WORK/b4.toast")"
CORE4="$(seq_core "$WORK/b4.seq")"
echo "   seq(core): [$CORE4]  result: $(echo "$OUT4" | grep '^RESULT')"
[ -z "$CORE4" ] && ok "B4 序列为空（confirm 拒 → 不 compact/不 exit/不 kill/不 resume）" || bad "B4 序列非空=[$CORE4]（取消后仍动手）"
echo "$OUT4" | grep -q "^RESULT false$" && ok "B4 返回 false（no-op）" || bad "B4 RESULT≠false"
[ "$(session_alive "$S4")" = 1 ] && ok "B4 会话未被杀（仍活）" || bad "B4 会话被误杀（取消后不该动）"
sleep 1
AFTER_LINES="$(wc -l <"$NEW/argv.log" 2>/dev/null || echo 0)"
[ "$BEFORE_LINES" = "$AFTER_LINES" ] && ok "B4 argv.log 无新行（before=$BEFORE_LINES after=$AFTER_LINES）" || bad "B4 argv.log 多了行（before=$BEFORE_LINES after=$AFTER_LINES）"

# ── B5:kill 失败 → 中止不续 resume（account-restart.ts:152-161）──────────────────────────────
echo "-- B5 kill 失败 → 中止（不 resume/不记账，account-restart.ts:152-161）--"
SID5="$(new_sid)"; S5="cc-${SID5:0:8}"
make_live "$SID5" "$OLD" >/dev/null
OUT5="$(drive_restart "$SID5" "$S5" znew 0 1 "$WORK/b5.seq" "$WORK/b5.toast" CCM_KILL_FAIL=1)"
echo "   seq: $(paste -sd' ' -<"$WORK/b5.seq")"
CORE5="$(seq_core "$WORK/b5.seq")"
grep -qx "kill-fail" "$WORK/b5.seq" && ok "B5 kill 失败已发生（kill-fail 帧）" || bad "B5 未见 kill-fail"
{ echo "$CORE5" | grep -qw "resume" && bad "B5 kill 失败后**仍 resume**（回归:防新旧双进程失守）"; } || ok "B5 kill 失败后**未 resume**（序列止于 kill 前:[$CORE5]）"
{ grep -q "^record " "$WORK/b5.seq" && bad "B5 kill 失败仍记账"; } || ok "B5 未记账（update_history_metadata 未发）"
echo "$OUT5" | grep -q "^RESULT false$" && ok "B5 返回 false" || bad "B5 RESULT≠false"
grep -q "重启已中止" "$WORK/b5.toast" && ok "B5 toast「重启已中止」" || bad "B5 无中止 toast"

# ── B6:resume 未起来 → 不记账不报成功（account-restart.ts:170-178）──────────────────────────
echo "-- B6 resume 未起来 → 不记账/返回 false（account-restart.ts:170-178）--"
SID6="$(new_sid)"; S6="cc-${SID6:0:8}"
make_live "$SID6" "$OLD" >/dev/null
OUT6="$(drive_restart "$SID6" "$S6" znew 0 1 "$WORK/b6.seq" "$WORK/b6.toast" CCM_RESUME_FAIL=1)"
echo "   seq: $(paste -sd' ' -<"$WORK/b6.seq")"
grep -qx "kill" "$WORK/b6.seq" && ok "B6 旧会话已真 kill（resume 前的破坏已发生）" || bad "B6 未见 kill"
grep -qx "resume-fail" "$WORK/b6.seq" && ok "B6 resume 拉起失败（resume-fail 帧）" || bad "B6 未见 resume-fail"
{ grep -q "^record " "$WORK/b6.seq" && bad "B6 resume 没起来却**记账**（回归:钉错账号归属）"; } || ok "B6 未记账（没起来就不钉账号归属）"
echo "$OUT6" | grep -q "^RESULT false$" && ok "B6 返回 false（不报成功）" || bad "B6 RESULT≠false"
grep -q "新会话未能自动拉起" "$WORK/b6.toast" && ok "B6 toast「旧会话已结束，但新会话未能自动拉起」" || bad "B6 无 resume 失败 toast"

echo "== 结果:$pass 过 / $fail 败 =="
[ "$fail" -eq 0 ]
