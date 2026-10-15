#!/usr/bin/env bash
# 跑全量门禁的唯一入口（打发版标签前那一趟）：经 heavy.sh（它起命令前摘掉会话继承来的变量，表在 session-vars）排门禁锁跑 gate。
# 用法：在要判的那棵树里 `bash tests/scripts/gate-clean.sh [给 gate.sh 的参数…]`
#   机器忙（别的路在编）时并行门禁会把按墙钟判的测试挤到超时 ⇒ 拿到锁的那一刻看负载，高于核数的 0.6 就串跑（GATE_SERIAL=1）。
#   整趟门禁经 heavy.sh 放进自己的 systemd scope（封顶内存 ＋ 槽位），内存吃紧时 oomd 挑的是这一份，不是主会话那一组（10-10 14:53）。
#   门禁锁在 GATE_STATE_DIR（默认 ${XDG_RUNTIME_DIR:-/tmp}/cc-monitor-gate）下的 gate.lock：几棵树共用同一把。
#   cargo 并行数写死 8（发版前那一趟）。
set -euo pipefail
case "${1:-}" in -h|--help) sed -n '2,7p' "$0"; exit 0 ;; esac
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(git rev-parse --show-toplevel)
cd "$root"
state=${GATE_STATE_DIR:-${XDG_RUNTIME_DIR:-/tmp}/cc-monitor-gate}
mkdir -p "$state"
exec env CARGO_BUILD_JOBS=8 HEAVY_MEM_HIGH=20G HEAVY_MEM_MAX=24G GATE_STATE_DIR="$state" \
  bash "$here/heavy.sh" flock "$state/gate.lock" bash -c '
  cores=$(nproc); load=$(cut -d" " -f1 /proc/loadavg)
  if [ -z "${GATE_SERIAL:-}" ] && awk -v l="$load" -v c="$cores" "BEGIN{exit !(l > c*0.6)}"; then
    echo "gate-clean: 负载 $load（${cores} 核）⇒ 这一趟串跑"; export GATE_SERIAL=1
  fi
  exec npm run gate "$@"' _ "$@"
