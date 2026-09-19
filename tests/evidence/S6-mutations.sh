#!/usr/bin/env bash
# 秤 6（`设计/17 §6` 表第 6 行）的**死值验**。
#
# 判据绿不等于判据在量东西。这个脚本逐个把秤上的一根针拔掉（每次只拔一根，
# 拔完立刻装回去），看 `tests/scale6-memory-ledger.vitest.ts` 会不会当场红。
# **哪一刀没让它红，哪一格就是装饰。** 原文记进 `tests/evidence/S6-memory-ledger.md`。
#
# 用法：bash tests/evidence/S6-mutations.sh
# 退出码：全部刀都让判据红 ⇒ 0；有任何一刀判据仍绿 ⇒ 1（并点名是哪一刀）。
#
# ⚠ 它会**临时改工作区里的 `src/cards/index.ts` 与 `src/tabs.ts`**，每刀结束立刻还原，
#   `trap` 兜底（Ctrl-C / 中途失败也还原）。跑之前工作区最好是干净的。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CARDS="$REPO/src/cards/index.ts"
TABS="$REPO/src/tabs.ts"
SPEC="tests/scale6-memory-ledger.vitest.ts"

BAK="$(mktemp -d)"
cp "$CARDS" "$BAK/cards.ts"
cp "$TABS" "$BAK/tabs.ts"
restore() {
  cp "$BAK/cards.ts" "$CARDS"
  cp "$BAK/tabs.ts" "$TABS"
}
trap 'restore; rm -rf "$BAK"' EXIT

fails=0

# apply <文件> <原文> <替换> —— 原文必须**恰好命中一次**，否则这把刀本身是坏的
apply() {
  local file="$1" from="$2" to="$3" n
  n="$(grep -cF -- "$from" "$file")"
  if [ "$n" != "1" ]; then
    echo "  ✗ 刀本身坏了：原文在 $(basename "$file") 里命中 $n 次（要恰好 1 次）"
    return 1
  fi
  python3 - "$file" "$from" "$to" <<'PY'
import sys
p, a, b = sys.argv[1], sys.argv[2], sys.argv[3]
s = open(p, encoding="utf-8").read()
open(p, "w", encoding="utf-8").write(s.replace(a, b, 1))
PY
}

cut() {
  local name="$1" file="$2" from="$3" to="$4"
  echo "── 刀：$name"
  if ! apply "$file" "$from" "$to"; then
    fails=$((fails + 1))
    restore
    return
  fi
  local out
  out="$(cd "$REPO" && npx vitest run "$SPEC" 2>&1)"
  if echo "$out" | grep -qE "Tests +[0-9]+ failed"; then
    echo "  ✓ 判据当场红：$(echo "$out" | grep -E "Tests +[0-9]+ failed" | head -1 | sed 's/^ *//')"
    echo "$out" | grep -E "^ *× |AssertionError" | head -4 | sed 's/^/     /'
  else
    echo "  ✗✗ 判据仍绿 —— 这一格是装饰"
    fails=$((fails + 1))
  fi
  restore
}

cut "M1 甲·出口侧不累加（producedUnits += 0）" "$CARDS" \
  "resultTextLedger.producedUnits += text.length;" \
  "resultTextLedger.producedUnits += 0;"

cut "M2 甲·闭包侧不累加（capturedUnits += 0）" "$CARDS" \
  "resultTextLedger.capturedUnits += text.length;" \
  "resultTextLedger.capturedUnits += 0;"

cut "M3 甲·出口侧不计条数（produced += 0）" "$CARDS" \
  "resultTextLedger.produced += 1;" \
  "resultTextLedger.produced += 0;"

cut "M4 乙·切断闭包→body 那条线（body 不再拿闭包里那份文本）" "$CARDS" \
  "if (!textBodyEl) textBodyEl = buildTextBody(text);" \
  "if (!textBodyEl) textBodyEl = buildTextBody(\"\");"

cut "M5 丙·branchRecords 写死 0" "$TABS" \
  "branchRecords: branchRecordCount(tab.branchFolder)," \
  "branchRecords: 0,"

cut "M6 丙·userInputs 写死 0" "$TABS" \
  "userInputs: tab.userInputs.length," \
  "userInputs: 0,"

cut "M7 丙·pending 写死 0" "$TABS" \
  "      pending: tab.window.pendingCount," \
  "      pending: 0,"

# ⚠ 这一刀**第一版是坏的**：原来改的是 `as unknown as { records?: unknown }` 里的**类型名**，
#   而类型在运行期被擦掉 ⇒ `inner.records` 照样读得到,判据仍绿。
#   （那一版当时"看起来红了"，红的其实是别路 agent 正在改坏的 `branch-fold.ts`——
#     **坏掉的邻居会把一把坏刀伪装成好刀**。原文记进 `S6-memory-ledger.md`。）
#   改成直接把返回值打成哨兵：这才是「字段改名读不到」在运行期的真形状。
cut "M8 丙·账本读不到 ⇒ branchRecordCount 返哨兵 -1" "$TABS" \
  "return Array.isArray(inner.records) ? inner.records.length : -1;" \
  "return -1;"

echo
if [ "$fails" = 0 ]; then
  echo "全部 8 刀都让判据当场红 ⇒ 秤 6 不是装饰。"
else
  echo "有 $fails 刀没让判据红 —— 上面点名的那几格是装饰，修完再报读数。"
fi
exit $([ "$fails" = 0 ] && echo 0 || echo 1)
