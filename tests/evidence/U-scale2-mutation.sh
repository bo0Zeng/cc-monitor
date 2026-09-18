#!/usr/bin/env bash
# 秤 2 的**变异自检** ——「没红 ≠ 守住了」的解药：故意把 `height-estimate.ts` 的常数改坏，
# 门禁必须当场红。
#
# 🔴 **不在真仓里改。** 整棵 `src/` + `tests/` `cp -a` 到 `/tmp/scale2-mutation/`
# （`node_modules` 软链过去），在**副本**里变异、跑、还原。
# 理由不是洁癖：本轮是六路并发，真仓的 `src/` 有别的路在动，
# 哪怕两秒钟的窗口也不该开。跑完 `git status --porcelain src/height-estimate.ts` 必须是空的。
#
# 用法：bash tests/evidence/U-scale2-mutation.sh          # 从仓根跑
# 读数：tests/evidence/U-scale2-mutation-log.txt（本脚本覆盖写）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SANDBOX=/tmp/scale2-mutation
LOG="$ROOT/tests/evidence/U-scale2-mutation-log.txt"
GATE="tests/scale2-height-truth.vitest.ts"

rm -rf "$SANDBOX"; mkdir -p "$SANDBOX"
cp -a "$ROOT/src" "$ROOT/tests" "$ROOT/package.json" "$ROOT/tsconfig.json" \
      "$ROOT/vitest.config.ts" "$ROOT/vite.config.ts" "$ROOT/index.html" "$SANDBOX/"
ln -s "$ROOT/node_modules" "$SANDBOX/node_modules"
cd "$SANDBOX"

: > "$LOG"
say () { printf '%s\n' "$*" | tee -a "$LOG"; }

# ⚠ `|| true`：变异跑的时候 vitest **本来就该**非零退出，而本文件开着 `set -e -o pipefail`
# ——不兜住的话脚本会在第一个变异之后就死掉（现打踩过：日志只剩 M0/M1）。
run_gate () {
  { npx vitest run "$GATE" 2>&1 || true; } \
    | grep -E '^\s+×|Tests  |card-[a-z-]+：p90|现算 [0-9.]+ ≠ 金标准' | head -8 || true
}

say "# 秤 2 变异自检 —— 原文（bash tests/evidence/U-scale2-mutation.sh 覆盖重写）"
say ""
say "######## M0 · 基线（未变异）"
run_gate | tee -a "$LOG"

mutate () {
  cp src/height-estimate.ts /tmp/he-scale2.bak
  sed -i "$2" src/height-estimate.ts
  say ""
  say "######## $1"
  run_gate | tee -a "$LOG"
  cp /tmp/he-scale2.bak src/height-estimate.ts
}

mutate "M1 · card-api-retry 常数 24 → 120（退回修之前的 CSS 兜底值）" \
  's|if (el.classList.contains("card-api-retry")) return 24;|if (el.classList.contains("card-api-retry")) return 120;|'
mutate "M2 · 删掉 card-api-retry 那一行（退回 return null ⇒ 落 CSS 120 兜底）" \
  '/if (el.classList.contains("card-api-retry")) return 24;/d'
mutate "M3 · SUMMARY_H 38 → 52（折叠卡估高改坏）" \
  's|^const SUMMARY_H = 38;|const SUMMARY_H = 52;|'
mutate "M4 · LH_PROSE ×1.65 → ×1.20（正文行高往**真值方向**改 ⇒ 预期**不红**，见读数里那段说明）" \
  's|^const LH_PROSE = 15 \* 1.65;|const LH_PROSE = 15 * 1.20;|'
mutate "M5 · COL_W 780 → 390（列宽假设改错一半）" \
  's|^const COL_W = 780;|const COL_W = 390;|'
mutate "M6 · LH_PROSE ×1.65 → ×2.40（正文行高往虚高方向再推）" \
  's|^const LH_PROSE = 15 \* 1.65;|const LH_PROSE = 15 * 2.40;|'
mutate "M7 · BASH_OUTPUT_MAX_LINES 20 → 60" \
  's|^const BASH_OUTPUT_MAX_LINES = 20;|const BASH_OUTPUT_MAX_LINES = 60;|'
mutate "M8 · card-slash 34 → 19（往真值方向改一个常数）" \
  's|if (el.classList.contains("card-slash")) return 34;|if (el.classList.contains("card-slash")) return 19;|'
mutate "M9 · BASH_OUTPUT_HEADER_H 19 → 90" \
  's|^const BASH_OUTPUT_HEADER_H = 19;|const BASH_OUTPUT_HEADER_H = 90;|'

say ""
say "######## 全部还原后复跑"
run_gate | tee -a "$LOG"

say ""
say "######## 真仓 src/height-estimate.ts 的状态（下一行为空 = 一个字没改）"
( cd "$ROOT" && git status --porcelain src/height-estimate.ts ) | tee -a "$LOG"
say "[END]"
echo
echo "读数写到了 $LOG"
