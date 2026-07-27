#!/bin/bash
# F03「--print 平价预言机」：验证 renderCli 产出的 `ccm …` 调用行，被真 shared/ccm 解析后，
# 展开结果里确实含有 renderCli 想表达的每个意图（sid / tmux 名 / cwd / launcher / ccm-sid）。
#
# 这是唯一能在没有真远端机器的场景下验证「CLI 渲染器真的会让 ccm 干对事」的手段——
# e2e/resume-suite.sh / restart-suite.sh 的 shim 对未知 invoke 一律走 default 分支（等价于
# 探测失败），天然只覆盖兜底渲染器路径，测不到 CLI 渲染器这条新路径是否真的对得上 ccm 的行为。
#
# 跑法：bash e2e/ccm-print-parity.sh   （npm run test:ccm-print-parity）
set -o pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"

PASS=0; FAIL=0
ck() { if [ "$2" = "$3" ]; then printf 'PASS | %s\n' "$1"; PASS=$((PASS+1))
       else printf 'FAIL | %s\n      期望含: %s\n      实得: %s\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi; }
contains() { case "$2" in *"$1"*) echo yes ;; *) echo no ;; esac; }

echo "===== 生产命令行（来自真 renderCli，不手搓）====="
TSV="$(cd "$REPO" && npx tsx e2e/ccm-print-parity-emit.mts)"
echo "$TSV" | sed 's/^/  /'

get_line() { echo "$TSV" | awk -F'\t' -v k="$1" '$1==k{print $2}'; }

# 隔离环境：不受本机 CLAUDE_CONFIG_DIR/manifest/工作区污染（R11 教训）。
run_print() {
  env -u TMUX -u CLAUDE_CONFIG_DIR CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent \
    CCM_ACCTS_MANIFEST=/nonexistent bash -c "$1 --print"
}

echo
echo "===== 场景 resumeTmuxWithIdentity ====="
LINE="$(get_line resumeTmuxWithIdentity)"
OUT="$(run_print "$LINE")"
NEEDLE_TMUX_NAME="-s 'cc-p1'"
NEEDLE_CWD="-c '/tmp'"
NEEDLE_SID_TAG="@ccm_sid_expect 'p1'"
NEEDLE_BASE="'--base'"
ck "resume 动作被内层 ccm 收到（positional，非 flag——shared/ccm 内部约定）" yes "$(contains "resume" "$OUT")"
ck "sid p1 出现在内层调用里" yes "$(contains "p1" "$OUT")"
ck "tmux 名 cc-p1 出现在 new-session" yes "$(contains "$NEEDLE_TMUX_NAME" "$OUT")"
ck "cwd /tmp 出现在 -c" yes "$(contains "$NEEDLE_CWD" "$OUT")"
ck "@ccm_sid_expect 打标用了 p1（F04：通道A写意图，非事实 @ccm_sid）" yes "$(contains "$NEEDLE_SID_TAG" "$OUT")"
# F05：账号维度恒显式表态——base 态真的把 --base 传进内层调用（R11 同型 bug 修复的端到端验证：
# 以前账号维度触发即强制降级、CLI 渲染器测不到这条路径；现在 base 态本身就走 CLI，必须验证
# 真 ccm 收到了 --base，不是被悄悄吞掉/漏传）。
ck "--base 真的传进了内层调用（F05：账号维度恒显式表态）" yes "$(contains "$NEEDLE_BASE" "$OUT")"

echo
echo "===== 场景 newTmuxCustomLauncher ====="
LINE="$(get_line newTmuxCustomLauncher)"
OUT="$(run_print "$LINE")"
NEEDLE_CWD2="-c '/home/pi/my proj'"
NEEDLE_NAME2="-s 'cc-proj'"
ck "自定义 launcher CCMPROBE 传给了内层调用" yes "$(contains "CCMPROBE" "$OUT")"
ck "含空格 cwd 正确带引号" yes "$(contains "$NEEDLE_CWD2" "$OUT")"
ck "会话名 cc-proj 正确出现" yes "$(contains "$NEEDLE_NAME2" "$OUT")"

echo
echo "===== 场景 attach ====="
LINE="$(get_line attach)"
OUT="$(run_print "$LINE")"
NEEDLE_ATTACH="attach -t '=cc-p1:'"
ck "attach 到 cc-p1" yes "$(contains "$NEEDLE_ATTACH" "$OUT")"

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
