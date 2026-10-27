#!/usr/bin/env bash
# cc-bus commit <id> <令牌>
#   两阶段读的第二跳:交后端 `bus-commit`,它在锁里比令牌(CAS)再推已读位置;比不上 ⇒ 一个字节都不写。
#   退出码:
#     0  推进成功
#     2  令牌 / 参数非法
#     11 STALE   别人已经推过了 —— 照常处理手上那段,不重试、不当错(语义是 at-least-once:可能重复,绝不吞掉)
#     12 CHANGED 收件箱在两跳之间变了 —— 位置没推,下一轮再读一次
#     1  后端不在 / 写不成
set -uo pipefail
SELFDIR=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
# shellcheck source=cc-bus-lib.sh
. "$SELFDIR/cc-bus-lib.sh"

[ $# -eq 2 ] && [ -n "$1" ] && [ -n "$2" ] || { echo "用法: cc-bus commit <id> <令牌>" >&2; exit 2; }
ccbus_ask commit "$(jq -cn --arg id "$1" --arg t "$2" '{id:$id, token:$t}')" && exit 0
case "$CCBUS_CODE" in
  stale) exit 11 ;;
  changed) exit 12 ;;
  bad_*) exit 2 ;;
  *) exit 1 ;;
esac
