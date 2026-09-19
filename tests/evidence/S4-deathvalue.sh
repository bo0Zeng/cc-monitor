#!/usr/bin/env bash
# 秤 4（`调研/设计/17-算法与复杂度.md` §6 表第 4 行）的**死值验**。
#
# 问的是一件事：**把快路的判定条件改反，判据会不会当场红。**
# 不会红的判据是安慰剂 —— 命中率那个数只要一条路都没走到也照样算得出来。
#
# 四刀，每刀只动 `src/branch-fold.ts` 里 `noteFastPathShadow` 的**一个字符串**，
# 跑完立刻从备份还原（`trap ... EXIT` 兜底，脚本被 Ctrl-C 也会还原）。
#
#   刀① fork 判反      —— `ledgerParentSeen.has(p)` 前面加一个 `!`
#   刀② fork 当命中    —— 分叉点不再算未命中（「快路恒真」的一半）
#   刀③ 快路恒假      —— 所有命中改判未命中（**空真那一形**：命中率变 0，判据必须红）
#   刀④ verify 判反    —— 把「影子算得对不对」那一格的比较取反
#
# ⚠ 这个脚本会**临时改生产文件**。同一棵树上有别的 agent 在干活时，窗口越短越好；
#   它是串行的，每刀之间都还原，总窗口 = 4 次 vitest 的时长（本机约 8 秒）。
#
# 用法：bash tests/evidence/S4-deathvalue.sh
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SRC="$ROOT/src/branch-fold.ts"
SPEC="tests/scale4-frame-ledger.vitest.ts"
BAK="$(mktemp)"
cp "$SRC" "$BAK"
restore() { cp "$BAK" "$SRC"; rm -f "$BAK"; }
trap restore EXIT

patch_once() {
  python3 - "$SRC" "$1" "$2" <<'PY'
import sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
src = open(path, encoding="utf-8").read()
hits = src.count(old)
if hits != 1:
    sys.exit(f"锚点在文件里命中 {hits} 次（期望正好 1 次），刀没下成：{old!r}")
open(path, "w", encoding="utf-8").write(src.replace(old, new, 1))
PY
}

run_spec() { (cd "$ROOT" && npx vitest run "$SPEC" 2>&1); }

knife() {
  local name="$1" old="$2" new="$3"
  cp "$BAK" "$SRC"
  echo
  echo "=================================================================="
  echo "### $name"
  echo "改：$old"
  echo "为：$new"
  echo "=================================================================="
  if ! patch_once "$old" "$new"; then
    echo "!!! 锚点对不上 —— 生产文件被改过，这一刀作废（不是「判据没牙」）"
    return 1
  fi
  local out rc
  out="$(run_spec)"; rc=$?
  cp "$BAK" "$SRC"
  if [ "$rc" -eq 0 ]; then
    echo "$out" | tail -8
    echo "!!! 死值验失败：刀下去了，判据**还是绿的** —— 这杆秤没有牙"
    return 1
  fi
  echo "$out" | grep -E "AssertionError|→ |Tests  |expected|Test Files" | head -20
  echo "--- 退出码 $rc（非 0 = 当场红，这一刀过）"
  return 0
}

echo "### 刀⓪ 未变异：判据必须绿（否则下面每一刀都说明不了任何事）"
base_out="$(run_spec)"; base_rc=$?
echo "$base_out" | grep -E "Tests  |Test Files" | head -4
if [ "$base_rc" -ne 0 ]; then
  echo "!!! 未变异态就是红的，死值验无意义"
  exit 1
fi

fails=0
knife '刀① fork 判反（原本无 child 这一问反过来）' \
  'else if (this.ledgerParentSeen.has(p)) miss = "parentHasChild";' \
  'else if (!this.ledgerParentSeen.has(p)) miss = "parentHasChild";' || fails=$((fails + 1))

knife "刀② fork 当命中（分叉点不再判未命中）" \
  'else if (this.ledgerParentSeen.has(p)) miss = "parentHasChild";' \
  'else if (this.ledgerParentSeen.has(p)) miss = null;' || fails=$((fails + 1))

knife "刀③ 快路恒假（命中率变 0 —— 空真那一形）" \
  '    else miss = null;
' \
  '    else miss = "parentOffMain";
' || fails=$((fails + 1))

knife '刀④ verify 判反（影子算得对不对 那一格）' \
  'if (setsEqual(predicted, next)) led.fastPathVerified++;' \
  'if (!setsEqual(predicted, next)) led.fastPathVerified++;' || fails=$((fails + 1))

echo
if [ "$fails" -eq 0 ]; then
  echo "=== 四刀全部当场红，秤 4 的判据有牙 ==="
else
  echo "=== 有 $fails 刀没红 —— 见上面的 !!! 行 ==="
fi
exit "$fails"
