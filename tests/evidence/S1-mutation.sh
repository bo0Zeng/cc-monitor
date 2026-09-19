#!/usr/bin/env bash
# 秤 1 的**死值验** ——「没红 ≠ 守住了」的解药：故意把某个子段的计时摘掉、
# 或者把桶边界改坏，判据必须**当场红**。
#
# 🔴 **不在真仓里改。** 整棵 `src/` + `tests/` `cp -a` 到 `/tmp/scale1-mutation/`
# （`node_modules` 软链过去），在**副本**里变异、跑、还原。
# 理由不是洁癖：本轮是多路并发，真仓的 `src/` 有别的路在动，
# 哪怕两秒钟的窗口也不该开。跑完真仓那两个文件的 `git status --porcelain` 必须是空的。
# （这条做法逐字照抄 `tests/evidence/U-scale2-mutation.sh`，别重新发明。）
#
# 用法：bash tests/evidence/S1-mutation.sh          # 从仓根跑
# 读数：tests/evidence/S1-mutation-log.txt（本脚本覆盖写）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SANDBOX=/tmp/scale1-mutation
LOG="$ROOT/tests/evidence/S1-mutation-log.txt"
GATE="tests/scale1-render-cost.vitest.ts"
SRC="src/render-stream-record.ts"

# 真仓那两个文件跑之前的指纹 —— 收尾时对拍。
# ⚠ **刻意不用 `git status`**：本轮这两个文件本来就是未提交的工作区改动，
# `git status` 永远非空，那条自检会变成一句看不出真假的废话。指纹才答得了
# 「本脚本有没有动过真仓」这个问题。
MD5_BEFORE="$(md5sum "$ROOT/$SRC" "$ROOT/$GATE")"

rm -rf "$SANDBOX"; mkdir -p "$SANDBOX"
cp -a "$ROOT/src" "$ROOT/tests" "$ROOT/package.json" "$ROOT/tsconfig.json" \
      "$ROOT/vitest.config.ts" "$ROOT/vite.config.ts" "$ROOT/index.html" "$SANDBOX/"
ln -s "$ROOT/node_modules" "$SANDBOX/node_modules"
cd "$SANDBOX"

: > "$LOG"
say () { printf '%s\n' "$*" | tee -a "$LOG"; }

# ⚠ `|| true`：变异跑的时候 vitest **本来就该**非零退出，而本文件开着 `set -e -o pipefail`
# ——不兜住的话脚本会在第一个变异之后就死掉。
run_gate () {
  { npx vitest run "$GATE" 2>&1 || true; } \
    | grep -E '^\s+×|^\s+Tests |AssertionError' | head -12 || true
}

say "# 秤 1 死值验 —— 原文（bash tests/evidence/S1-mutation.sh 覆盖重写）"
say "# 时间：$(date -Is)"
say "# 机器：$(hostname) · $(uname -srm) · node $(node -v)"
say ""
say "######## M0 · 基线（未变异，必须全绿）"
run_gate | tee -a "$LOG"

# $1 = 标题  $2 = 要改的文件  $3.. = sed 表达式（可多条）
mutate () {
  local title="$1"; shift
  local file="$1"; shift
  cp "$file" /tmp/s1-mut.bak
  local e
  for e in "$@"; do sed -i "$e" "$file"; done
  say ""
  say "######## $title"
  run_gate | tee -a "$LOG"
  cp /tmp/s1-mut.bak "$file"
}

# ── 一、摘掉某个子段的计时（设计逐字「拆 4 个子段」，摘一个就该红）────────────
mutate "M1 · 摘掉 \`estimate\` 子段（两处写成 0）—— 预期红" "$SRC" \
  's|estimate: tEstimate - tRender,|estimate: 0,|' \
  's|estimate: tEstimate - tMerge,|estimate: 0,|'

mutate "M2 · 摘掉 \`mount\` 子段（三处写成 0）—— 预期红" "$SRC" \
  's|mount: tMount - tEstimate,|mount: 0,|g' \
  's|mount: tMount - tMerge,|mount: 0,|'

mutate "M3 · 摘掉 \`merge\` 子段（两处写成 0）—— 预期红" "$SRC" \
  's|merge: tMerge - tRender,|merge: 0,|g'

# ⚠ 这一条是**补洞补出来的**：`total` 如果被写成「四段之和」，
#   「入口出口真夹」那句话当场变成假话，而只看上侧的残余判据**恒绿**
#   （残余变成 0，而 0 < 10%）。⇒ 判据里补了下侧那一格，这一条就是验它的。
mutate "M4 · \`total\` 不再入口出口夹，改成 Σ四段（伪装成真夹）—— 预期红" "$SRC" \
  's|^function pushCostSample(ring: RenderCostSample\[\], s: RenderCostSample): void {|&\n  s.total = s.render + s.merge + s.estimate + s.mount;|'

# ── 二、把分桶那根轴打坏 ─────────────────────────────────────────────────
mutate "M5 · 探针自报字节恒为 0（分桶轴塌成一个桶）—— 预期红" "$SRC" \
  's|return new TextEncoder().encode(JSON.stringify(message)).length;|return 0;|'

mutate "M6 · 桶边界 2-8K 的上界 8192 → 16384（桶边界改坏）—— 预期红" "$GATE" \
  's|\["2-8K", 2048, 8192\],|["2-8K", 2048, 16384],|'

# ── 三、把"反空真"那一格自己打坏 ────────────────────────────────────────
mutate "M7 · 探针根本没开（缓冲恒空 ⇒ 分桶表是空的）—— 预期红" "$GATE" \
  's|^  enableRenderCostProbe();$|  // enableRenderCostProbe();|'

# 🔴 **这一条第一轮是绿的，那是判据自己的洞，不是变异不够狠。**
#   第一版的判据写成 `got === EXPECTED_PER_PASS[b] * PASSES`，两边都含 `PASSES`
#   ⇒ 恒等式，少跑一半样本照样全绿。修法：另立一张**绝对条数**登记表
#   （`EXPECTED_SAMPLES_PER_BUCKET` / `EXPECTED_SAMPLES`），判据对拍绝对数。
#   ⇒ 本条现在预期红。**这就是死值验的价值：它逮住的是判据，不是被测代码。**
mutate "M8 · 少跑一半遍数（PASSES 8 → 4）—— 预期红〔第一轮曾绿，见上方注释〕" "$GATE" \
  's|^const PASSES = 8;|const PASSES = 4;|'

mutate "M9 · 登记表里 8-32K 桶改成 16（差一条）—— 预期红" "$GATE" \
  's|^  "8-32K": 17,|  "8-32K": 16,|'

mutate "M10 · 环形缓冲 cap 5000 → 50000（设计逐字那个数被改）—— 预期红" "$SRC" \
  's|^export const RENDER_COST_RING_CAP = 5000;|export const RENDER_COST_RING_CAP = 50000;|'

# ── 四、对照组：**预期不红**，用来说明判据的射程到哪为止 ──────────────────
mutate "M11 · 去掉预热那一遍（读数会抖，但判据只钉占比与倍率）—— **预期不红**" "$GATE" \
  's|^  drivePass(fixtureLines);$|  // drivePass(fixtureLines);|'

say ""
say "######## 全部还原后复跑（必须回到 M0 的全绿）"
run_gate | tee -a "$LOG"

say ""
say "######## 真仓那两个文件有没有被动过（指纹对拍）"
MD5_AFTER="$(md5sum "$ROOT/$SRC" "$ROOT/$GATE")"
if [ "$MD5_BEFORE" = "$MD5_AFTER" ]; then
  say "真仓未被触碰 ✓"
  say "$MD5_AFTER"
else
  say "🔴 真仓被改了！变异应当只发生在 $SANDBOX 里"
  say "before: $MD5_BEFORE"
  say "after : $MD5_AFTER"
fi
say "[END]"
echo
echo "读数写到了 $LOG"
