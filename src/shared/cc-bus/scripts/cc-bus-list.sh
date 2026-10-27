#!/usr/bin/env bash
# cc-bus list  列出这台的各收件箱:会话 id · 在不在(true / false / null = 判不了)· 待读 · 标题。
#   交后端 `bus-list`,照它答的原样印,这里不判。退出码:0 · 1 = 后端不在 / 读不成。
set -uo pipefail
SELFDIR=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
# shellcheck source=cc-bus-lib.sh
. "$SELFDIR/cc-bus-lib.sh"
[ $# -eq 0 ] || { echo "cc-bus list: 不收参数" >&2; exit 2; }
ccbus_ask list '{}' || exit 1
printf 'id\tlive\tunread\ttitle\n'
printf '%s' "$CCBUS_OUT" | jq -r '.agents[] | "\(.id)\t\(.live)\t\(.unread)\t\(.title)"'
