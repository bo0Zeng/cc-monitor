#!/usr/bin/env bash
# 跑弱网台架，判它**跑完了、一条没红、确实断言过东西** —— 与 `tests/e2e/assert-pass-floor.sh` 同一套判法。
#
# 另起一份是因为那一份经 `npm run test:<套件>` 起，而台架没有 npm 脚本、按路径直跑。
# 判法（fail-closed）：台架非零退出 ⇒ 红；抓不到 `合计 PASS=<n> FAIL=<m>` ⇒ 红；
# `FAIL ≠ 0` 或 `PASS = 0` ⇒ 红。断言了几条只住在台架自己的输出里，调用方不抄这个数。
#
# 用法：bash tests/e2e/weak-net/assert-floor.sh
set -uo pipefail

[ "$#" -eq 0 ] || { echo "不收参数；断言条数不由调用方给。实得：$*" >&2; exit 2; }

HERE="$(cd "$(dirname "$0")" && pwd)"
OUT_FILE="$(mktemp)"
trap 'rm -f -- "$OUT_FILE"' EXIT

bash "$HERE/rig.sh" >"$OUT_FILE" 2>&1
rc=$?
cat "$OUT_FILE"

if [ "$rc" -ne 0 ]; then
  echo "::error::弱网台架失败（退出码 $rc）"
  exit "$rc"
fi

line="$(grep -E '合计 PASS=[0-9]+' "$OUT_FILE" | tail -1 || true)"
n="$(printf '%s' "$line" | grep -oE 'PASS=[0-9]+' | grep -oE '[0-9]+' || true)"
f="$(printf '%s' "$line" | grep -oE 'FAIL=[0-9]+' | grep -oE '[0-9]+' || true)"
if [ -z "$n" ] || [ -z "$f" ]; then
  echo "::error::台架输出里找不到「合计 PASS=<n> FAIL=<m>」—— 没跑到收尾，或被改得不打印了。判不了 ⇒ 红。"
  exit 1
fi
if [ "$f" -ne 0 ] || [ "$n" -eq 0 ]; then
  echo "::error::弱网台架收尾 PASS=$n FAIL=$f，退出码却是 0 —— 有红、或一条都没断言。"
  exit 1
fi

echo "[weak-net] 合计 PASS=$n FAIL=0"
