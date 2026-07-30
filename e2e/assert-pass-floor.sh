#!/usr/bin/env bash
# G-A（gate-integrity）：**跑一套真机 e2e，并断言它的运行期 PASS 数不低于地板。**
#
# ## 为什么需要它
#
# 那 8 套套件的退出码只看 `FAIL` 数 ⇒ **一条不跑也会 exit 0**。删掉断言、`return` 提前、
# 某个前置条件悄悄不满足导致整段被跳过 —— 全部表现为**绿**。这是门禁的天然失效模式：
# 不是变红，是**静默缩水**。地板挡的正是这个。
#
# ## fail-closed 的三条
#
# 1. 套件本身非零退出 ⇒ 直接失败（原样透传输出）
# 2. 抓不到 `合计 PASS=<n>` 那行 ⇒ **失败**，不当作 0 也不当作通过。
#    抓不到只有两种可能：套件被改得不打印了，或它压根没跑到收尾 —— 两种都该红。
# 3. `n < 地板` ⇒ 失败，诊断里同时给出实得与地板
#
# ## 地板值写在调用处（`ci.yml`），不写在这里
#
# 这个脚本对「哪套该有多少条」**一无所知**，它只是个可复用的度量器。地板与套件的对应
# 关系是 CI 的知识 ⇒ 写在 `ci.yml` 的调用行上，改地板时**一定**会在 diff 里看见。
# （对比 G-B：vendored `run-tests.sh` 的地板写在脚本自己里，那是因为 SS-10 不许改副本，
#  这里没有那个约束，所以按「改动可见性」选调用处。）
#
# 用法：bash e2e/assert-pass-floor.sh <npm-script-后缀> <地板>
#   例：bash e2e/assert-pass-floor.sh tmux-target 26   → 跑 `npm run test:tmux-target`
set -uo pipefail

SUITE="${1:?用法: assert-pass-floor.sh <npm-script-后缀> <地板>}"
FLOOR="${2:?缺地板值}"

case "$FLOOR" in ''|*[!0-9]*) echo "地板必须是非负整数，实得：$FLOOR" >&2; exit 2 ;; esac

OUT_FILE="$(mktemp)"
trap 'rm -f -- "$OUT_FILE"' EXIT

# **别写成 `npm run … | tee`**：管线会把 npm 的退出码藏起来（`pipefail` 也只在开了它时救得回来，
# 而这里要的是原样退出码 + 完整输出两者都要）。落文件再回显。
set +e
npm run --silent "test:$SUITE" >"$OUT_FILE" 2>&1
rc=$?
set -e
cat "$OUT_FILE"

if [ "$rc" -ne 0 ]; then
  echo "::error::e2e 套件 $SUITE 失败（退出码 $rc）"
  exit "$rc"
fi

# 只认收尾那行的格式：`===== 合计 PASS=<n> FAIL=<m> =====`（8 套逐字一致）。
n="$(grep -oE '合计 PASS=[0-9]+' "$OUT_FILE" | grep -oE '[0-9]+' | tail -1 || true)"
if [ -z "$n" ]; then
  echo "::error::$SUITE 的输出里找不到「合计 PASS=<n>」——套件被改得不打印了，或没跑到收尾。地板无从校验 ⇒ 判失败。"
  exit 1
fi

if [ "$n" -lt "$FLOOR" ]; then
  echo "::error::$SUITE 断言数缩水：实得 $n < 地板 $FLOOR。真删了断言就把地板一起降，并在 commit 里说明理由。"
  exit 1
fi

echo "[assert-pass-floor] $SUITE: PASS=$n（地板 $FLOOR）"
