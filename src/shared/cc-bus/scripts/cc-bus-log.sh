#!/usr/bin/env bash
# cc-bus log <id> [-n N]  只读看一条会话收件箱的尾巴(缺省 200 封):交后端 `bus-inbox`,一封一行印信封。
#   不推已读位置。退出码:0 · 2 = 参数非法 · 1 = 后端不在 / 读不成。
set -uo pipefail
SELFDIR=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
# shellcheck source=cc-bus-lib.sh
. "$SELFDIR/cc-bus-lib.sh"
id=""; n=200
while [ $# -gt 0 ]; do
  case "$1" in
    -n) [ $# -ge 2 ] || { echo "cc-bus log: -n 后面要跟一个数" >&2; exit 2; }; n="$2"; shift 2 ;;
    -*) echo "cc-bus log: 不认识的旗标 '$1'" >&2; exit 2 ;;
    *)  [ -z "$id" ] || { echo "cc-bus log: 多给了一个 id '$1'" >&2; exit 2; }; id="$1"; shift ;;
  esac
done
case "$n" in ''|*[!0-9]*|0) echo "cc-bus log: -n 要正整数,给的是 '$n'" >&2; exit 2;; esac
[ -n "$id" ] || { echo "用法: cc-bus log <id> [-n N]" >&2; exit 2; }
ccbus_ask inbox "$(jq -cn --arg id "$id" --argjson n "$n" '{id:$id, lines:$n}')" \
  || { case "$CCBUS_CODE" in bad_*) exit 2 ;; *) exit 1 ;; esac; }
printf '%s' "$CCBUS_OUT" | jq -c '.messages[] | del(.act_said)'
