#!/usr/bin/env bash
# K-R55 量具④：照**门禁的真实条件**（全量并行 `cargo test` 的那个测试二进制）连跑 N 趟，
# 数那条判据红几趟，并把红的那一趟的 panic 原文留下来。
N="${1:-30}"
OUT="${2:-/home/zbl/文档/claudecode-frontend/.claude/pm-targets/k-r55/fullsuite-red.log}"
BIN=$(ls -t "$CARGO_TARGET_DIR"/debug/deps/cc_monitor_remote-* 2>/dev/null | grep -v '\.d$' | head -1)
[ -x "$BIN" ] || { echo "找不到测试二进制"; exit 3; }
: > "$OUT"
red=0; target_red=0
for i in $(seq 1 "$N"); do
  if ! out=$("$BIN" 2>&1); then
    red=$((red+1))
    echo "=== 第 $i 趟 ===" >> "$OUT"
    echo "$out" | grep -A 12 "^failures:" >> "$OUT"
    echo "$out" | grep -q "an_unreadable_environ_is_never_reported_as_the_zero_account" && target_red=$((target_red+1))
  fi
done
echo "全量并行连跑 $N 趟：整体红 $red 趟 · 其中命中那条判据 $target_red 趟（原文在 $OUT）"
