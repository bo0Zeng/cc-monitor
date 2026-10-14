#!/usr/bin/env bash
# 重活一律经它跑：cargo 编 / 测、quick-check、门禁、gradle、截图台架。
# 用法：bash tests/scripts/heavy.sh <命令…>
#
# 为什么（10-10 14:53）：几路同时编、门禁同时跑，主会话那一组 tmux 进程占到 36 GB，
#   systemd-oomd 按「整组」杀，主会话和各路一起没了（不是谁发了 kill）。
# 做法两件：
#   ① 每份重活单独一个 systemd scope、封顶内存 ⇒ 内存吃紧时 oomd / 内核挑的是这一份，主会话那一组不受牵连；
#   ② 槽位：同时最多 HEAVY_SLOTS 份（默认 3），多出来的排队等，不靠「大家自觉少开」。
#   ③ 让着人（10-13 busy1）：scope 进专用切片 ccheavy.slice，切片 CPUWeight=20（默认 100；systemctl --user set-property --runtime，
#      写在运行时目录、重启即无）；命令前加 nice -n 10 · ionice -c 3（有就加）。机器忙时交互的那几个进程先拿到 CPU。
#      权重设在切片上才管用：scope 自己的权重只跟同一个父组里的兄弟比（heavy 的 scope 本来都在 app.slice 里，彼此一样低等于没降）；
#      切片名不带「-」：systemd 把 a-b.slice 拆成 a.slice/a-b.slice，权重落在内层又白设。
#      IO 这一维照实：ionice 只有调度器认优先级时才管用（bfq · mq-deadline；调度器是 none 的 NVMe 上不起作用）；
#      IOWeight 要用户管理器委派了 io 控制器，没委派的机器上落不下，所以不写。
# 没有 `systemd-run --user` 的机器（CI · Windows · 没有 user session）⇒ 不封顶、直接跑，stderr 说一声；nice / ionice 照加；槽位照排（没有 flock 就不排）。
# gradle 要带 --no-daemon（守护进程会留在 scope 里、一直占着槽位）。
# 环境变量：HEAVY_SLOTS（默认 3）· HEAVY_MEM_HIGH（默认 12G）· HEAVY_MEM_MAX（默认 14G）·
#   GATE_STATE_DIR 槽位所在（默认 ${XDG_RUNTIME_DIR:-/tmp}/cc-monitor-gate，槽位在它下面的 heavy-slots/，几棵树共用）。
set -u
case "${1:-}" in
  -h|--help) sed -n '2,19p' "$0"; exit 0 ;;
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
  slice=ccheavy.slice
  systemctl --user set-property --runtime "$slice" CPUWeight=20 >/dev/null 2>&1 \
    || echo "heavy: 设不了 $slice 的 CPU 权重 ⇒ 照跑，CPU 不让" >&2
  scope=(systemd-run --user --scope --quiet --slice="$slice" -p MemoryHigh="$high" -p MemoryMax="$max" --)
else
  echo "heavy: 这台没有 systemd-run --user（CI / Windows / 没有 user session）⇒ 不封顶内存，直接跑" >&2
  scope=()
fi
# 让着人：nice / ionice 有就加，没有说一声（Windows 的 Git Bash 两样都没有）。
if command -v nice >/dev/null 2>&1; then scope+=(nice -n 10); else echo "heavy: 没有 nice ⇒ CPU 优先级不降" >&2; fi
if command -v ionice >/dev/null 2>&1; then scope+=(ionice -c 3); else echo "heavy: 没有 ionice ⇒ IO 优先级不降" >&2; fi
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
