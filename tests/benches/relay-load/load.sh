#!/usr/bin/env bash
# 中转压测台架：本机假上游 ＋ 沙箱家目录里的真后端 ＋ 一档档并发的流式请求，印一张表。读数不进判据。
#
#   bash load.sh <后端二进制> <工作目录> [rig.py run 的选项…]
#   例：bash tests/benches/relay-load/load.sh .build/backend/perf/cc-monitor-backend .build/relay-load --levels 100,200 --seconds 20
#
# 一个真请求都不发、不碰任何号：上游是 rig.py 自己在回环上起的；后端的家目录在 <工作目录>/home（现建，跑完删）。
# 先小档再大档、看着机器往上加（rig.py 在可用内存或负载过线时不开下一档）。起的进程由 rig.py 按 PID 收。
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
[ $# -ge 2 ] || { sed -n '2,8p' "$0" >&2; exit 2; }
bin=$(realpath "$1")
mkdir -p "$2"
work=$(realpath "$2")
shift 2
[ ! -e "$work/home" ] || { echo "load.sh：$work/home 已经在了（上一趟没收干净？）—— 换一个工作目录" >&2; exit 2; }
cleanup() { rm -rf "$work/home" "$work/tmux" "$work/run" "$work/upstream.port"; }
trap cleanup EXIT
python3 -I "$here/rig.py" run "$bin" "$work" "$@"
