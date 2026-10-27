#!/usr/bin/env bash
# cc-bus unread [<会话 id>]  印那条会话的未读数(一个数;没有收件箱 ⇒ 0)。缺 id ⇒ 问 `cc-bus whoami`。
#   只读:交后端 `bus-unread`,与名单的待读同一处算。pb 那一半(「N 条未读」)只经这一条读。
#   退出码:0 · 2 = id 不是那个形状 · 1 = 后端不在 / 读不成。
set -uo pipefail
SELFDIR=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
# shellcheck source=cc-bus-lib.sh
. "$SELFDIR/cc-bus-lib.sh"
[ $# -le 1 ] || { echo "cc-bus unread: 多给了参数" >&2; exit 2; }
id="${1:-}"
[ -n "$id" ] || id=$("$SELFDIR/cc-bus-whoami.sh") || exit 1
ccbus_ask unread "$(jq -cn --arg id "$id" '{id:$id}')" || { case "$CCBUS_CODE" in bad_*) exit 2 ;; *) exit 1 ;; esac; }
printf '%s' "$CCBUS_OUT" | jq -r '.unread'
