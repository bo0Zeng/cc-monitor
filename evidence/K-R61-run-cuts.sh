#!/usr/bin/env bash
# K-R61 变异台驱动：施一刀 → 在沙箱里跑**整包**（拿最小面）→ 撤刀 → 记一行。
# 住址：scratchpad/kr61-run-cuts.sh（`kr61-` 前缀，独属本轮）
# 被测对象：/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r61
set -o pipefail
S=/tmp/claude-1000/-home-zbl----claudecode-frontend/f80a7aea-0ccf-4854-9b7a-12961a9c55aa/scratchpad
WT=/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r61
OUT="$S/mut-table.log"

run_one() {
  local cut="$1" tree="$2" file="$3"
  echo "════════ $cut ════════" | tee -a "$OUT"
  python3 "$S/kr61-mutate.py" "$cut" 2>&1 | tee -a "$OUT" || return 1
  local cmd
  if [ "$tree" = monitor ]; then
    cmd='cd src-tauri && cargo test --lib 2>&1'
  else
    cmd='cd remote-daemon-proto && cargo test --bin cc-monitor-remote 2>&1'
  fi
  "$S/kr61-box.sh" "$cmd" > "$S/raw-$cut.log" 2>&1
  {
    echo "--- 判定行 ---"
    grep -E '^test result:' "$S/raw-$cut.log" || echo "🔴 判定行不见了 ⇒ 按 CRASH 记"
    echo "--- 红了哪几条（最小面）---"
    sed -n '/^failures:$/,/^test result:/p' "$S/raw-$cut.log" | grep -E '^    [a-z]' | sort -u || true
  } | tee -a "$OUT"
  git -C "$WT" checkout -- "$file"
  local st
  st=$(git -C "$WT" status --porcelain)
  [ -z "$st" ] || { echo "🔴 撤刀没干净：$st" | tee -a "$OUT"; return 1; }
  echo "撤刀后 git status 干净 ✓" | tee -a "$OUT"
}

for c in "$@"; do
  case "$c" in
    M10*) run_one "$c" daemon remote-daemon-proto/src/control/ccm/mod.rs ;;
    M11*) run_one "$c" daemon remote-daemon-proto/src/control/ccm/plan.rs ;;
    M12*) run_one "$c" monitor-daemon "" ;;
    M1*|M2*|M3*|M4*|M5*|M6*|M7*|M8*) run_one "$c" monitor src-tauri/src/history.rs ;;
  esac
done
