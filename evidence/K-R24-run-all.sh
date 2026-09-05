#!/usr/bin/env bash
# K-R24 下一拍：一趟把要的读数全量出来（**串行** —— 几格共用同一个 CARGO_TARGET_DIR，
# 并行会撞 cargo 的锁）。
#
#   ① 门禁 `none` 口径（沙箱默认）  —— 交回要的两个口径之一，**走真的 .claude/devbox/gate**
#   ② 门禁 `host` 口径              —— 另一个
#   ③ `HOME` 轴四格 A/B/C/D        —— 见 K-R24-home-axis-ab.sh 的头注
set -o pipefail
PROJ=/home/zbl/文档/claudecode-frontend
WT="$PROJ/.claude/worktrees/k-r24b"
OUT="${1:?用法: K-R24-run-all.sh <输出目录>}"
mkdir -p "$OUT"

cd "$PROJ" || exit 3

echo "=== ① 门禁 none 口径（真 gate）==="
PB_WS=backend-consolidation .claude/devbox/gate "$WT" k-r24b >"$OUT/gate-none.log" 2>&1
echo "rc=$?"

echo "=== ② 门禁 host 口径（真 gate）==="
PB_WS=backend-consolidation DEVBOX_NET=host .claude/devbox/gate "$WT" k-r24b >"$OUT/gate-host.log" 2>&1
echo "rc=$?"

for cell in A B C D; do
  echo "=== ③ HOME 轴 · 格 $cell ==="
  PB_WS=backend-consolidation bash "$WT/evidence/K-R24-home-axis-ab.sh" "$cell" "$OUT"
done
echo "=== 全部跑完 ==="
