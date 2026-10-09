#!/bin/bash
# F03「--print 平价预言机」：验证**生产渲染器**产出的 `ccm …` 调用行，被真 ccm 解析后，
# 展开结果里确实含有渲染器想表达的每个意图（sid / tmux 名 / cwd / launcher / ccm-sid）。
# 生产渲染器是 Rust（`ccm_invocation::render_ccm_invocation`）；那几行从入库夹具
# `cli-golden.json` 取（cargo 逐字节保证它们 == 生产产出，来历链见 `ccm-print-parity-emit.mts` 头注）。
# 原先现场跑 TS 渲染器 `renderCli`（已删，零生产调用）。**12 条断言一个字没改。**
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

PASS=0; FAIL=0
ck() { if [ "$2" = "$3" ]; then printf 'PASS | %s\n' "$1"; PASS=$((PASS+1))
       else printf 'FAIL | %s\n      期望含: %s\n      实得: %s\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi; }
contains() { case "$2" in *"$1"*) echo yes ;; *) echo no ;; esac; }

# renderCli 产出的命令行以裸 `ccm` 开头（那正是它该产出的东西），所以下面 `bash -c` 执行它时
# `ccm` 要经 PATH 解析。**必须把 PATH 指向仓内 shared/ccm**，否则解析到的是开发者本机
# `~/.local/bin/ccm` 那份装机版——于是这套「平价预言机」验的就不是本仓代码，而是碰巧装在
# 机器上的某个版本（可能比仓内旧/新）。CI 上根本没装 ccm，12 条断言全红，正是靠这个才暴露出来。
# 教训清单第 5 条的同型问题：不显式隔离，开发者本机状态就会污染测试断言。
#
# ★ `K-R48` 第二拍（09-11）：`ccm` 从**仓内 bash 脚本**换成**后端二进制本体**。
#   〔用@09-11 `K33`〕逐字「后端**只有一个**，**不要有什么 bash 脚本**，**不要有什么单独的 ccm**」
#   ⇒ 终端里敲的 `ccm` 就是 `cc-monitor-backend`（分流不看 argv[0]：没有打头的 `--` 就是一次性模式）。
#   **本套件 12 条断言一个字都没改** —— 它测的一直是「renderCli 渲出来的那行，被真 `ccm`
#   解析后展开成什么」，那是后端今天仍要保证的命令契约，与用什么语言实现无关。
#   依据是 `K-R48` 第一拍的逐字节对拍（SAME=27/DIFF=2）。
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

echo "===== 生产命令行（入库夹具里 cargo 对过生产渲染器的那四行，不手搓）====="
TSV="$(cd "$REPO" && npx tsx tests/e2e/ccm-print-parity-emit.mts)"
echo "$TSV" | sed 's/^/  /'

get_line() { echo "$TSV" | awk -F'\t' -v k="$1" '$1==k{print $2}'; }

# 隔离环境：不受本机 CLAUDE_CONFIG_DIR/manifest/工作区污染（R11 教训）。
#
# ★★ 〔`K-P2` `F` 拍 09-04；用@09-04「**ccm不要管找不到, 统一走后端**」〕**两处跟着契约改**：
#
#  ① `CCM_BACKEND_BIN` 指到 `tests/e2e/fake-backend.sh`。
#     账号解析从此**没有本地退路** ⇒ 不给后端的话这 12 条会**全部**死在 `exit 4` 上
#     （现打过：那不是「判据红了」，是**整套跑不起来**）。
#     ⚠ 这**不是**放宽断言：本套件测的一直是「`renderCli` 渲出来的那行，被真 `ccm` 解析后
#     展开成什么」——「这台机器装没装后端」从来不是它要测的变量。
#     照它自己头注那条纪律（「不显式隔离，开发者本机状态就会污染测试断言」）：
#     从前它靠「没有后端 ⇒ 走本地那条」把这个变量拿掉，今天靠**自带一份后端**拿掉。
#
#  ② `HOME` 换成一个空的临时目录：账号库跟着家目录走（`<家>/.cc-monitor/accounts/`），
#     空家目录 = 「这台机器没有账号库」，开发者本机的账号库不会被注入断言。
FAKE_BACKEND="$REPO/tests/e2e/fake-backend.sh"
run_print() {
  env -u TMUX -u CLAUDE_CONFIG_DIR \
    CCM_BACKEND_BIN="$FAKE_BACKEND" \
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
echo "===== 场景 resumeTmuxWithModel（F08：--model 闭合 R14①）====="
LINE="$(get_line resumeTmuxWithModel)"
OUT="$(run_print "$LINE")"
# --tmux 场景的 --print 只展示外层 tmux 编排命令（真正的 export ANTHROPIC_MODEL 只会在内层
# ccm 调用真的执行时才展开，不在这次静态 --print 里）——同既有 NEEDLE_BASE 的验证口径：
# 验证 flag 真的被转传进了 send-keys 送进去的内层调用负载里。两个 token 分开断言——负载整体
# 又被外层单引号包一层，token 之间的空格实际是 `'\'' '\''` 这种嵌套转义，不是裸空格。
ck "--model flag 真的传进了内层调用" yes "$(contains "'--model'" "$OUT")"
ck "opus 值真的传进了内层调用" yes "$(contains "'opus'" "$OUT")"

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
