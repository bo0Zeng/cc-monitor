#!/usr/bin/env bash
# cc-bus unread [<会话 id>]  印那条会话的未读数(一个数;没有收件箱 ⇒ 0)。缺 id ⇒ 问 `cc-bus whoami`。
#   只读:不写盘、不建目录、不迁旧收件箱;口径与名单的待读同一处(`cc-bus-lib.sh` 的 `ccbus_unread_of`)。
#   pb 那一半(「N 条未读」)只经这一条读。
#   退出码:0 · 2 = id 不是会话 id 那一形 · 13 = 适配层办不到。
set -euo pipefail
BUS="${CC_BUS_HOME:-$HOME/.cc-bus}"
SELFDIR=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
# shellcheck source=cc-bus-lib.sh
. "$SELFDIR/cc-bus-lib.sh" || exit 13
id="${1:-}"
[ $# -le 1 ] || { echo "cc-bus unread: 多给了参数" >&2; exit 2; }
[ -n "$id" ] || id=$("$SELFDIR/cc-whoami")
case "$id" in *[!A-Za-z0-9_-]*|'') echo "cc-bus unread: 非法/空 id '$id'" >&2; exit 2;; esac
ccbus_unread_of "$id"
