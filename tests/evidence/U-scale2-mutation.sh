#!/usr/bin/env bash
# 秤 2 的**变异自检** ——「没红 ≠ 守住了」的解药：故意把 `height-estimate.ts` 的常数改坏，
# 门禁必须当场红。
#
# 🔴 **不在真仓里改。** 整棵 `src/` + `tests/` `cp -a` 到 `/tmp/scale2-mutation/`
# （`node_modules` 软链过去），在**副本**里变异、跑、还原。
# 理由不是洁癖：本轮是六路并发，真仓的 `src/` 有别的路在动，
# 哪怕两秒钟的窗口也不该开。跑完对一次 `sha256sum src/height-estimate.ts`，前后必须逐位相同。
#
# 用法：bash tests/evidence/U-scale2-mutation.sh          # 从仓根跑
# 读数：tests/evidence/U-scale2-mutation-log.txt（本脚本覆盖写）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BEFORE_SHA="$( cd "$ROOT" && sha256sum src/height-estimate.ts )"
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

# ⚠ **变异必须真的落地**：sed 的模式一旦被源码改动甩开（本轮踩过：常数 24/32/34 改成
#   17/19/19 之后，旧模式一个都不匹配），`sed -i` **静默地什么都不做** ⇒ 门禁当然绿 ⇒
#   一份「9 个变异 9 个没抓到」的日志会被读成「秤不灵」，而真相是**变异根本没发生**。
#   ⇒ 每个变异后对一次文件，没变就当场喊，并记进日志。
mutate () {
  cp src/height-estimate.ts /tmp/he-scale2.bak
  sed -i "$2" src/height-estimate.ts
  say ""
  say "######## $1"
  if cmp -s /tmp/he-scale2.bak src/height-estimate.ts; then
    say "  🔴 变异没落地（sed 模式没匹配到任何一行）—— 下面这一格的绿**不算数**，去修模式"
  else
    run_gate | tee -a "$LOG"
  fi
  cp /tmp/he-scale2.bak src/height-estimate.ts
}

mutate "M1 · card-api-retry 常数 17 → 120（退回修之前的 CSS 兜底值）" \
  's|if (el.classList.contains("card-api-retry")) return 17;|if (el.classList.contains("card-api-retry")) return 120;|'
mutate "M2 · 删掉 card-api-retry 那一行（退回 return null ⇒ 落 CSS 120 兜底）" \
  '/if (el.classList.contains("card-api-retry")) return 17;/d'
mutate "M3 · SUMMARY_H 38 → 52（折叠卡估高改坏）" \
  's|^const SUMMARY_H = 38;|const SUMMARY_H = 52;|'
mutate "M4 · LH_PROSE ×1.65 → ×1.20（正文行高砍 27% ⇒ **仍然不红**，原因见日志末尾）" \
  's|^const LH_PROSE = 15 \* 1.65;|const LH_PROSE = 15 * 1.20;|'
mutate "M5 · COL_W 780 → 390（列宽假设改错一半）" \
  's|^const COL_W = 780;|const COL_W = 390;|'
mutate "M6 · LH_PROSE ×1.65 → ×2.40（正文行高往虚高方向再推）" \
  's|^const LH_PROSE = 15 \* 1.65;|const LH_PROSE = 15 * 2.40;|'
mutate "M7 · BASH_OUTPUT_MAX_LINES 20 → 60" \
  's|^const BASH_OUTPUT_MAX_LINES = 20;|const BASH_OUTPUT_MAX_LINES = 60;|'
mutate "M8 · card-slash 19 → 34（退回 2026-09-18 之前那个 border-box 手算值）" \
  's|if (el.classList.contains("card-slash")) return 19;|if (el.classList.contains("card-slash")) return 34;|'
mutate "M9 · BASH_OUTPUT_HEADER_H 19 → 90" \
  's|^const BASH_OUTPUT_HEADER_H = 19;|const BASH_OUTPUT_HEADER_H = 90;|'
# 🔴 M10 是本轮新增的那一条 —— 它钉的是「正文卡 ~2× 虚高」那个根：
#    把文本节点里的空白折叠去掉 ⇒ 退回「markdown 源码的排版换行 = 硬断行」。
#    `card-assistant` 的上限已经从 1.2 拧到 0.3，所以这条**必须**当场红；
#    它红不了就说明上限白拧了。
mutate "M10 · extractProseText 不再折叠源码换行（退回修之前那个「全留 \\n」）" \
  's|const s = raw.*|const s = raw;|'

say ""
say "######## 全部还原后复跑"
run_gate | tee -a "$LOG"

say ""
say "######## ⚠ M4 为什么还是绿的（这是秤的钝处，不是它守住了）"
say "  LH_PROSE ×1.65 → ×1.20 把正文行高砍掉 27%，card-assistant 的 p90 只从 21.8% 动到 25.7%"
say "  （p50 9.5% → 18.6%，max 21.8% → 30.6%，方向从「偏高」翻成「偏低」）—— 仍在 30% 门槛之内。"
say "  原因：门槛量的是 |相对误差| 的 p90，而这个改动把估值从「+10%」推到「−19%」，**穿过零点**，"
say "  两头都还在线内 ⇒ 行高常数的灵敏度粗度就是 ±30%。要更细必须再加一杆「有符号偏差」的秤（今天没有）。"
say "  ⚠ 2026-09-18 之前这一条更钝：那时 card-assistant 的上限挂着 1.2，而正文卡本身 2× 虚高，"
say "  砍行高等于往真值方向改 —— 门禁连「变差」都看不出来。这一轮把根修掉、上限拧到 0.3 之后，"
say "  同一个变异离红只剩 4.3 个点。**钝是真的钝，但比原来锐了一个数量级。**"
say ""
say "######## 真仓 src/height-estimate.ts 有没有被变异溅到（两行相同 = 一个字没改）"
# ⚠ 原先这一格看的是 `git status --porcelain`，那是**错的哨兵**：它只会告诉你"这个文件
#   相对 HEAD 改没改"，而本轮真仓里本来就躺着未提交的修（常数 + extractProseText）
#   ⇒ 它必然非空，于是这一格永远"红"，红得没有信息。真正要问的是
#   **"跑变异这段时间里它动没动"** ⇒ 比前后两个 sha256。
say "  跑之前 $BEFORE_SHA"
say "  跑之后 $( cd "$ROOT" && sha256sum src/height-estimate.ts )"
say "[END]"
echo
echo "读数写到了 $LOG"
