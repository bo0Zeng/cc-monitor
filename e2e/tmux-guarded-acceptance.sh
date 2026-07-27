#!/bin/bash
# F04 三道门（tmux.rs 的 Gate 1/2/3 原子 verify+act）的**真机行为验收**。
#
# 与 Rust 单测的分工：单测断言「命令串长什么样」，本脚本断言「这条命令在真 tmux 上干了什么」——
# 门禁只锁字符串形状不锁行为是 R1 的教训（三门禁全绿仍放行过一个让 send-keys 完全失效的改动），
# 本模块新增的原子命令构造有真实的 shell 语法复杂度（嵌套 if/then/else、`cut -f1/-f2`），必须过
# 真机而非只信 Rust 单测的字符串断言。
#
# 输入 = 真 builder 产出的生产命令串（`cargo test --lib -- --ignored --nocapture
# emit_guarded_commands_for_e2e`，见 src-tauri/src/tmux.rs 对应测试头注）——不手搓等价命令。
# 隔离 -L socket，不碰用户任何真实会话。
# 跑法：bash e2e/tmux-guarded-acceptance.sh   （需要 tmux + cargo；npm run test:tmux-guarded）
set -o pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
SOCK=ccmF04
TMUX_BIN="$(command -v tmux)" || { echo "需要 tmux"; exit 1; }
SP="$(mktemp -d)"
trap 'rm -rf "$SP"; "$TMUX_BIN" -L "$SOCK" kill-server 2>/dev/null' EXIT

(cd "$REPO/src-tauri" && cargo test --lib -- --ignored --nocapture emit_guarded_commands_for_e2e 2>/dev/null) \
  | grep -P '^\w+\t' > "$SP/f04-cmds.tsv"
# F04 Phase D 审计发现：若 cargo test 本身挂了（编译错误/panic），emit 是空的，后续场景会拿到空
# 命令串、报一堆看似随机的 FAIL，而不是一条清晰的诊断——提前失败并给出明确原因。
[ -s "$SP/f04-cmds.tsv" ] || { echo "cargo test 未产出任何命令串——检查上游 emit_guarded_commands_for_e2e 是否编译/运行成功"; exit 1; }

CMD() { grep -P "^$1\t" "$SP/f04-cmds.tsv" | cut -f2-; }
T() { "$TMUX_BIN" -L "$SOCK" "$@"; }
reset() { T kill-server 2>/dev/null; sleep 0.3; }
sessions() { T ls -F '#{session_name}' 2>/dev/null | sort | tr '\n' ' '; }
pane() { T capture-pane -p -t "=$1:" 2>/dev/null; }
# 生产命令串里裸调 `tmux`——shim 导到隔离 socket（同 tmux-target-acceptance.sh 的做法）。
BIN="$SP/bin"; mkdir -p "$BIN"
printf '#!/bin/sh\nexec %s -L %s "$@"\n' "$TMUX_BIN" "$SOCK" > "$BIN/tmux"; chmod +x "$BIN/tmux"
export PATH="$BIN:$PATH"

PASS=0; FAIL=0
ck() { if [ "$2" = "$3" ]; then printf 'PASS | %-56s | %s\n' "$1" "$3"; PASS=$((PASS+1));
       else printf 'FAIL | %-56s | 期望=%s 实得=%s\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi; }

echo "===== 场景 1：Gate 3（仅 kill）—— cc-* 前缀命中，2-window 会话 → 拒绝且存活 ====="
reset; T new-session -d -s cc-e2e-owned; T new-window -t cc-e2e-owned; sleep 0.3
OUT="$(bash -c "$(CMD kill_owned)")"
ck "拒绝，报 windows=2" "CCM_GUARD_REJECTED windows=2" "$OUT"
ck "会话仍存活（未被误杀）" "cc-e2e-owned " "$(sessions)"

echo
echo "===== 场景 2：Gate 3 —— cc-* 前缀命中，1-window 会话 → 真的 kill ====="
reset; T new-session -d -s cc-e2e-owned; sleep 0.3
OUT="$(bash -c "$(CMD kill_owned)")"
ck "无输出（kill-session 成功）" "" "$OUT"
ck "会话已不存在" "" "$(sessions)"

echo
echo "===== 场景 3：Gate 2 远端半支 —— 非前缀名、未设 @ccm_sid → 拒绝且存活 ====="
reset; T new-session -d -s e2e-custom; sleep 0.3
OUT="$(bash -c "$(CMD kill_custom)")"
ck "拒绝，sid 为空 + windows=1" "CCM_GUARD_REJECTED sid= windows=1" "$OUT"
ck "会话仍存活" "e2e-custom " "$(sessions)"

echo
echo "===== 场景 4：Gate 2 远端半支 —— 非前缀名、已设 @ccm_sid + 1-window → 真的 kill ====="
reset; T new-session -d -s e2e-custom; T set-option -t '=e2e-custom:' @ccm_sid abc123; sleep 0.3
OUT="$(bash -c "$(CMD kill_custom)")"
ck "无输出（kill-session 成功）" "" "$OUT"
ck "会话已不存在" "" "$(sessions)"

echo
echo "===== 场景 5：目标根本不存在 → CCM_NO_SESSION（kill 两种形态都测）====="
reset
ck "kill_owned 对不存在目标 → CCM_NO_SESSION" "CCM_NO_SESSION" "$(bash -c "$(CMD kill_owned)")"
ck "kill_custom 对不存在目标 → CCM_NO_SESSION" "CCM_NO_SESSION" "$(bash -c "$(CMD kill_custom)")"

echo
echo "===== 场景 6：send-keys —— cc-* 前缀命中，零 Gate 退化路径，正常送达 ====="
reset; T new-session -d -s cc-e2e-owned; sleep 0.3
bash -c "$(CMD send_keys_owned)" >/dev/null 2>&1
sleep 0.5
ck "载荷送达（零额外 round trip 的退化形态仍工作）" "HIT" "$(pane cc-e2e-owned | grep -q CCMPROBE && echo HIT || echo MISS)"

echo
echo "===== 场景 7：send-keys —— 非前缀名、未设 @ccm_sid → 拒绝，pane 不被污染 ====="
reset; T new-session -d -s e2e-custom; sleep 0.3
OUT="$(bash -c "$(CMD send_keys_custom)")"
ck "拒绝，sid 为空（不带无关的 windows 字段——send-keys 不受 Gate 3 约束，F04 Phase D 审计修）" \
   "CCM_GUARD_REJECTED sid=" "$OUT"
ck "pane 未被污染（send-keys 确实没发出去）" "clean" "$(pane e2e-custom | grep -q CCMPROBE && echo POLLUTED || echo clean)"

echo
echo "===== 场景 8：send-keys —— 非前缀名、已设 @ccm_sid → 正常送达 ====="
reset; T new-session -d -s e2e-custom; T set-option -t '=e2e-custom:' @ccm_sid abc123; sleep 0.3
bash -c "$(CMD send_keys_custom)" >/dev/null 2>&1
sleep 0.5
ck "载荷送达" "HIT" "$(pane e2e-custom | grep -q CCMPROBE && echo HIT || echo MISS)"

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
