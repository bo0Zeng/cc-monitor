#!/usr/bin/env bash
# 重活一律经它跑：cargo 编 / 测、quick-check、门禁、gradle、截图台架。
# 用法：bash tests/scripts/heavy.sh <命令…>
#
# 为什么（10-10 14:53）：几路同时编、门禁同时跑，主会话那一组 tmux 进程占到 36 GB，
#   systemd-oomd 按「整组」杀，主会话和各路一起没了（不是谁发了 kill）。
# 做法两件：
#   ① 每份重活单独一个 systemd scope、封顶内存 ⇒ 内存吃紧时 oomd / 内核挑的是这一份，主会话那一组不受牵连；
#   ② 槽位：同时最多 HEAVY_SLOTS 份（默认 3），多出来的排队等，不靠「大家自觉少开」。
# 没有 `systemd-run --user` 的机器（CI · Windows · 没有 user session）⇒ 不封顶、直接跑，stderr 说一声；槽位照排（没有 flock 就不排）。
# gradle 要带 --no-daemon（守护进程会留在 scope 里、一直占着槽位）。
# 环境变量：HEAVY_SLOTS（默认 3）· HEAVY_MEM_HIGH（默认 12G）· HEAVY_MEM_MAX（默认 14G）·
#   GATE_STATE_DIR 槽位所在（默认 ${XDG_RUNTIME_DIR:-/tmp}/cc-monitor-gate，槽位在它下面的 heavy-slots/，几棵树共用）。
set -u
case "${1:-}" in
  -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
  '') echo "用法：heavy.sh <命令…>" >&2; exit 2 ;;
esac
# 已经在一份重活里（外层 heavy 拿了槽位、起了 scope，比如 gate-affected 经 heavy 跑 quick-check、quick-check 里又经 heavy 跑 gradle）
#   ⇒ 直接跑：再排一个槽位会和外层互等（三份外层占满三个槽位、各自等第四个），内存也已经被外层那个 scope 封顶。
[ -n "${HEAVY_HELD:-}" ] && exec "$@"
slots=${HEAVY_SLOTS:-3}
max=${HEAVY_MEM_MAX:-14G}
high=${HEAVY_MEM_HIGH:-12G}
dir=${GATE_STATE_DIR:-${XDG_RUNTIME_DIR:-/tmp}/cc-monitor-gate}/heavy-slots
mkdir -p "$dir"
if command -v systemd-run >/dev/null 2>&1 && command -v systemctl >/dev/null 2>&1 \
   && systemctl --user show-environment >/dev/null 2>&1; then
  scope=(systemd-run --user --scope --quiet -p MemoryHigh="$high" -p MemoryMax="$max" --)
else
  echo "heavy: 这台没有 systemd-run --user（CI / Windows / 没有 user session）⇒ 不封顶内存，直接跑" >&2
  scope=()
fi
command -v flock >/dev/null 2>&1 || { echo "heavy: 没有 flock ⇒ 不排槽位" >&2; export HEAVY_HELD=0; exec "${scope[@]}" "$@"; }
waited=0
while :; do
  for i in $(seq 1 "$slots"); do
    exec 9>"$dir/$i"
    if flock -n 9; then
      [ "$waited" -gt 0 ] && echo "heavy: 等了 ${waited} 秒拿到槽位 $i" >&2
      export HEAVY_HELD=$i
      exec "${scope[@]}" "$@"
    fi
    exec 9>&-
  done
  [ "$waited" -eq 0 ] && echo "heavy: ${slots} 个槽位都占着，排队等…" >&2
  sleep 5; waited=$((waited + 5))
done
