#!/bin/bash
# 「--print 平价预言机」：验证**生产渲染器**产出的 `ccm …` 调用行，被真 ccm 解析后，
# 展开结果里确实含有渲染器想表达的每个意图（sid / tmux 名 / cwd / launcher / ccm-sid）。
# 那几行从 `session-open` 本体取（`open-line-emit.sh`：起会话时交出去的那一行，渲染器是 `ccm_invocation`）。
#
# 这是唯一能在没有真远端机器的场景下验证「CLI 渲染器真的会让 ccm 干对事」的手段——
# tests/e2e/resume-suite.sh / restart-suite.sh 的 shim 对未知 invoke 一律走 default 分支（等价于
# 探测失败），天然只覆盖兜底渲染器路径，测不到 CLI 渲染器这条新路径是否真的对得上 ccm 的行为。
#
# 跑法：bash tests/e2e/ccm-print-parity.sh   （npm run test:ccm-print-parity）
. "$(cd "$(dirname "$0")" && pwd)/sandbox-env.sh"  # 无条件清掉继承来的 CCM_* / CLAUDE* / ANTHROPIC_* / TMUX* / CC_BUS_*
set -o pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
# 后端起会话名时会列一遍 tmux 会话 ⇒ 挂 shim，落在本趟的私有 server 上（没挂就列的是缺省那台、开发机上用户正在用的）。
# shellcheck source=tests/e2e/tmux-shim.sh
. "$HERE/tmux-shim.sh" e2ePrintParity

. "$HERE/assert.sh"
contains() { case "$2" in *"$1"*) echo yes ;; *) echo no ;; esac; }

# 那几行以裸 `ccm` 开头（上面换过），所以下面 `bash -c` 执行它时
# `ccm` 要经 PATH 解析。**必须把 PATH 指向仓内 shared/ccm**，否则解析到的是开发者本机
# `~/.local/bin/ccm` 那份装机版——于是这套「平价预言机」验的就不是本仓代码，而是碰巧装在
# 机器上的某个版本（可能比仓内旧/新）。CI 上根本没装 ccm，12 条断言全红，正是靠这个才暴露出来。
# 教训清单第 5 条的同型问题：不显式隔离，开发者本机状态就会污染测试断言。
#
# ★ `ccm` 是**后端二进制本体**：后端只有一个，没有单独的 bash 版 ccm。
#   终端里敲的 `ccm` 就是 `cc-monitor-backend`（分流不看 argv[0]：没有打头的 `--` 就是一次性模式）。
#   本套件测的是「renderCli 渲出来的那行，被真 `ccm` 解析后展开成什么」——
#   后端要保证的命令契约，与用什么语言实现无关。
# 🔴 **fail-closed**：二进制没 build 就**响亮退出**，不许静默回落到 PATH 上碰巧有的那一份
#   —— 那正是本段头注第一句要治的病。
CCM_NATIVE="${CARGO_TARGET_DIR:-$REPO/.build/backend}/debug/cc-monitor-backend"
[ -x "$CCM_NATIVE" ] || {
  echo "::error::找不到原生入口 $CCM_NATIVE —— 先 \`cd src/backend && cargo build --bin cc-monitor-backend\`" >&2
  exit 2
}
BIN="$(mktemp -d)"; NOHOME="$(mktemp -d)"; trap 'rm -rf "$BIN" "$NOHOME"; tmux_shim_cleanup' EXIT
ln -s "$CCM_NATIVE" "$BIN/ccm"
export PATH="$BIN:$PATH"

echo "===== 生产命令行（session-open 交出去的那四行，不手搓）====="
# 打头那个词（这台装好的入口）换成 `ccm`：下面经 PATH 叫本树这一份。
emit() { local l; l="$(bash "$HERE/open-line-emit.sh" "$@")" || { echo "取不到 $*" >&2; exit 2; }; printf 'ccm %s' "${l#* }"; }
TSV="$(
  printf 'resumeTmuxWithIdentity\t%s\n' "$(emit shape=resume-tmux sid=p1 cwd=/tmp name=cc-p1 launcher=claude account=base)"
  printf 'newTmuxCustomLauncher\t%s\n' "$(emit shape=new-tmux cwd='/home/pi/my proj' name=cc-proj launcher=CCMPROBE account=base)"
  printf 'attach\t%s\n' "$(emit shape=attach name=cc-p1 launcher=claude)"
  printf 'resumeTmuxWithModel\t%s\n' "$(emit shape=resume-tmux sid=p1 cwd=/tmp name=cc-p1 launcher=claude account=base model=opus)"
)" || exit 2
echo "$TSV" | sed 's/^/  /'

get_line() { echo "$TSV" | awk -F'\t' -v k="$1" '$1==k{print $2}'; }

# 隔离环境：不受本机 CLAUDE_CONFIG_DIR/manifest/工作区污染。
#
# `HOME` 换成一个空的临时目录：账号库跟着家目录走（`<家>/.cc-monitor/accounts/`），
# 空家目录 = 「这台机器没有账号库」，开发者本机的账号库不会被注入断言。
run_print() {
  env -u TMUX -u CLAUDE_CONFIG_DIR \
    HOME="$NOHOME" bash -c "$1 --ccm-print"
}

echo
echo "===== 场景 resumeTmuxWithIdentity ====="
LINE="$(get_line resumeTmuxWithIdentity)"
OUT="$(run_print "$LINE")"
NEEDLE_TMUX_NAME="-s 'cc-p1'"
NEEDLE_CWD="-c '/tmp'"
NEEDLE_SID_TAG="@ccm_sid_expect 'p1'"
NEEDLE_BASE="'--base'"
# `--resume` 是交给 claude 的词，内层放在 `--` 后面原样带进去。
ck "--resume 被内层 ccm 原样带进去（交给 claude 的词）" yes "$(contains "--resume" "$OUT")"
ck "sid p1 出现在内层调用里" yes "$(contains "p1" "$OUT")"
ck "tmux 名 cc-p1 出现在 new-session" yes "$(contains "$NEEDLE_TMUX_NAME" "$OUT")"
ck "cwd /tmp 出现在 -c" yes "$(contains "$NEEDLE_CWD" "$OUT")"
ck "@ccm_sid_expect 打标用了 p1（通道A写意图，非事实 @ccm_sid）" yes "$(contains "$NEEDLE_SID_TAG" "$OUT")"
# 账号维度恒显式表态——base 态真的把 --base 传进内层调用：base 态走 CLI，
# 真 ccm 得收到 --base，不许被悄悄吞掉/漏传。
ck "--base 真的传进了内层调用（账号维度恒显式表态）" yes "$(contains "$NEEDLE_BASE" "$OUT")"

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
echo "===== 场景 resumeTmuxWithModel（--model 闭合）====="
LINE="$(get_line resumeTmuxWithModel)"
OUT="$(run_print "$LINE")"
# --tmux 场景的 --print 只展示外层 tmux 编排命令（真正的 export ANTHROPIC_MODEL 只会在内层
# ccm 调用真的执行时才展开，不在这次静态 --print 里）——同既有 NEEDLE_BASE 的验证口径：
# 验证 flag 真的被转传进了 send-keys 送进去的内层调用负载里。两个 token 分开断言——负载整体
# 又被外层单引号包一层，token 之间的空格实际是 `'\'' '\''` 这种嵌套转义，不是裸空格。
ck "--model flag 真的传进了内层调用" yes "$(contains "'--model'" "$OUT")"
ck "opus 值真的传进了内层调用" yes "$(contains "'opus'" "$OUT")"

echo
finish
