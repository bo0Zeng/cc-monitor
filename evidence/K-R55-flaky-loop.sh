#!/usr/bin/env bash
# K-R55 实现方自用量具③：在**加载**下把那条 flaky 判据连跑 N 趟，数红几趟。
# 用法（容器内）：kr55-flaky-loop.sh <N>
N="${1:-40}"
BIN=$(ls -t "$CARGO_TARGET_DIR"/debug/deps/cc_monitor_remote-* 2>/dev/null | grep -v '\.d$' | head -1)
[ -x "$BIN" ] || { echo "找不到测试二进制"; exit 3; }
pids=()
for i in $(seq 1 $(( $(nproc) * 4 ))); do sh -c 'while :; do :; done' & pids+=($!); done
red=0
for i in $(seq 1 "$N"); do
  "$BIN" --exact observe::accounts_query::tests::an_unreadable_environ_is_never_reported_as_the_zero_account >/dev/null 2>&1 || red=$((red+1))
done
for p in "${pids[@]}"; do kill "$p" 2>/dev/null; done
wait 2>/dev/null
echo "加载下连跑 $N 趟，红 $red 趟"
