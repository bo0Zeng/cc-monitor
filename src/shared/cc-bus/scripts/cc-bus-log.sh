#!/usr/bin/env bash
# cc-bus log <id> [-n N]  只读看一个 agent 收件箱的尾巴(后端 bus-inbox 读它,不读收件箱文件本身)。
#   首行 `#cc-log<TAB>1`,随后收件箱末 N 行原样(缺省 200)。不推已读位置、不写任何状态(与 cc-bus peek 同一条零写面);
#   持共享锁,锁文件仍是 <inbox>.lock(锁族不增)。退出码:0 · 2 = 参数非法 · 13 = 适配层办不到。
set -euo pipefail
BUS="${CC_BUS_HOME:-$HOME/.cc-bus}"
SELFDIR=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
# shellcheck source=cc-bus-adapt.sh
. "$SELFDIR/cc-bus-adapt.sh"
ccbus_adapt_load || exit 13
id=""; n=200
while [ $# -gt 0 ]; do
  case "$1" in
    -n) [ $# -ge 2 ] || { echo "cc-bus log: -n 后面要跟一个数" >&2; exit 2; }; n="$2"; shift 2 ;;
    -*) echo "cc-bus log: 不认识的旗标 '$1'" >&2; exit 2 ;;
    *)  [ -z "$id" ] || { echo "cc-bus log: 多给了一个 id '$1'" >&2; exit 2; }; id="$1"; shift ;;
  esac
done
case "$n" in ''|*[!0-9]*|0) echo "cc-bus log: -n 要正整数,给的是 '$n'" >&2; exit 2;; esac
case "$id" in *[!A-Za-z0-9_-]*|'') echo "cc-bus log: 非法/空 id '$id'" >&2; exit 2;; esac
inbox="$BUS/inbox/$id.jsonl"
printf '#cc-log\t1\n'
[ -f "$inbox" ] || exit 0
( store_lock_shared 9 || exit 13; tail -n "$n" "$inbox" ) 9>>"$inbox.lock"
