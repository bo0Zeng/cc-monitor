#!/usr/bin/env bash
# 秤 1（`设计/17 §6` 表第 1 行）的**一键复算**。
#
# 它只干一件事：跑门禁并把**直方图原文**落到 `tests/evidence/S1-render-cost-log.txt`。
# 直方图是 `beforeAll` 里 `console.log` 出来的，而 vitest 的**默认 reporter 会把
# 通过用例的 stdout 吞掉** —— 现打踩过：同一条命令，红的时候看得见表、绿的时候什么都没有。
# ⇒ 这里固定 `--reporter=verbose`。
#
# ⚠ 秤 1 量的是 wall time，**读数天然随机器/负载浮动**。判据刻意只钉
# 「段之间的占比」与「桶之间的倍率」，不钉绝对毫秒（理由见被测文件头注射程边界第 5 条）。
# 所以同一台机器上两次跑出来的表**不会逐位相同，这是对的**，别把它当成漂移。
#
# 用法：bash tests/evidence/S1-run.sh          # 从仓根跑
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LOG="$ROOT/tests/evidence/S1-render-cost-log.txt"
GATE="tests/scale1-render-cost.vitest.ts"

cd "$ROOT"

{
  echo "# 秤 1 读数原文（bash tests/evidence/S1-run.sh 覆盖重写）"
  echo "# 时间：$(date -Is)"
  echo "# 机器：$(hostname) · $(uname -srm) · node $(node -v)"
  echo "# 语料：tests/__fixtures__/scale2-height-records.jsonl（与秤 2 同一份；结构真、正文全合成）"
  echo
} > "$LOG"

# `|| true`：本脚本开着 `set -e`，而我们要的是**把输出留下来**，不是在红的时候当场死掉。
{ npx vitest run "$GATE" --reporter=verbose 2>&1 || true; } | tee -a "$LOG"

echo
echo "读数写到了 $LOG"
