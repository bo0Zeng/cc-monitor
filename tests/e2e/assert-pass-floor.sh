#!/usr/bin/env bash
# 跑一套真机 e2e（`npm run test:<套件>`），判它**跑完了、一条没红、确实断言过东西**。
#
# 判法（三条，全是 fail-closed）：
#   1. 套件非零退出 ⇒ 红（原样透传输出）；
#   2. 收尾那行 `===== 合计 PASS=<n> FAIL=<m> … =====` 抓不到 ⇒ 红 —— 没打印就是没跑到收尾；
#   3. `FAIL ≠ 0` 或 `PASS = 0` ⇒ 红 —— 退出码 0 而一条没过，与「一条都没跑」在终端上一样。
#
# 每套断言了几条**只住在套件自己的输出里**，本脚本与门禁都不抄这个数：
# 几路同时给同一套加断言时，不再在几处手抄的数上撞车。
# 它买不到「断言被删了几条」—— 那由改套件的那次提交自己说清，评审看 diff。
#
# 用法：bash tests/e2e/assert-pass-floor.sh <npm-script-后缀>
#   例：bash tests/e2e/assert-pass-floor.sh tmux-target   → 跑 `npm run test:tmux-target`
set -uo pipefail

SUITE="${1:?用法: assert-pass-floor.sh <npm-script-后缀>}"
[ "$#" -eq 1 ] || { echo "只收一个参数（套件名）；断言条数不由调用方给。实得：$*" >&2; exit 2; }
case "$SUITE" in ''|*[!a-z0-9-]*) echo "套件名只许小写字母、数字、连字符。实得：$SUITE" >&2; exit 2 ;; esac

OUT_FILE="$(mktemp)"
trap 'rm -f -- "$OUT_FILE"' EXIT

# 落文件再回显，别写成 `npm run … | tee`：管线会把 npm 的退出码藏起来。
set +e
npm run --silent "test:$SUITE" >"$OUT_FILE" 2>&1
rc=$?
set -e
cat "$OUT_FILE"

if [ "$rc" -ne 0 ]; then
  echo "::error::e2e 套件 $SUITE 失败（退出码 $rc）"
  exit "$rc"
fi

line="$(grep -E '合计 PASS=[0-9]+' "$OUT_FILE" | tail -1 || true)"
if [ -z "$line" ]; then
  echo "::error::$SUITE 的输出里找不到「合计 PASS=<n>」—— 套件没跑到收尾，或被改得不打印了。判不了 ⇒ 红。"
  exit 1
fi
n="$(printf '%s' "$line" | grep -oE 'PASS=[0-9]+' | grep -oE '[0-9]+')"
f="$(printf '%s' "$line" | grep -oE 'FAIL=[0-9]+' | grep -oE '[0-9]+' || true)"
k="$(printf '%s' "$line" | grep -oE 'SKIP=[0-9]+' | grep -oE '[0-9]+' || true)"
if [ -z "$f" ]; then
  echo "::error::$SUITE 的收尾行没有 FAIL=<m>：$line —— 判不了 ⇒ 红。"
  exit 1
fi
if [ "$f" -ne 0 ]; then
  echo "::error::$SUITE 收尾记着 FAIL=$f，退出码却是 0 —— 套件的退出码没跟上它自己的失败数。"
  exit 1
fi
if [ "$n" -eq 0 ]; then
  echo "::error::$SUITE 一条都没过（PASS=0）—— 退出码 0 而什么都没断言，与没跑分不开。"
  exit 1
fi

echo "[assert-pass-floor] $SUITE: 合计 PASS=$n FAIL=0${k:+ SKIP=$k}"
