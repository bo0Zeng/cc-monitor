#!/usr/bin/env bash
# cc-bus peek [id] [--max-lines N] [--token-file PATH]
#   两阶段读的第一跳:纯读。交后端 `bus-peek`,正文印到 stdout(后端写好的那一段),令牌默认印到 stderr(裸的一行)、
#   给了 `--token-file` 就写进那份文件(调用方指定,不进正文:Stop 钩子把 stdout 整个当正文喂回)。不推已读位置。
#   第二跳是 cc-bus commit;`cc-bus recv` 就是入口里 peek 接 commit。
#   退出码:0 正常(有没有新信都是 0)· 2 参数非法 · 1 后端不在 / 读不成。
set -uo pipefail
SELFDIR=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
# shellcheck source=cc-bus-lib.sh
. "$SELFDIR/cc-bus-lib.sh"

id=""; maxlines=""; tokfile=""
while [ $# -gt 0 ]; do
  case "$1" in
    --max-lines)  [ $# -ge 2 ] || { echo "cc-bus peek: --max-lines 后面要跟一个数" >&2; exit 2; }
                  maxlines="$2"; shift 2 ;;
    --token-file) [ $# -ge 2 ] || { echo "cc-bus peek: --token-file 后面要跟一个路径" >&2; exit 2; }
                  tokfile="$2"; shift 2 ;;
    --) shift; break ;;
    -*) echo "cc-bus peek: 不认识的旗标 '$1'" >&2; exit 2 ;;
    *)  [ -z "$id" ] || { echo "cc-bus peek: 多给了一个 id '$1'" >&2; exit 2; }; id="$1"; shift ;;
  esac
done
case "$maxlines" in ''|*[!0-9]*|0) [ -z "$maxlines" ] || { echo "cc-bus peek: --max-lines 要正整数,给的是 '$maxlines'" >&2; exit 2; } ;; esac
[ -n "$id" ] || id=$("$SELFDIR/cc-bus-whoami.sh") || exit 1

req=$(jq -cn --arg id "$id" --arg m "$maxlines" '{id:$id} + (if $m == "" then {} else {max_lines:($m | tonumber)} end)')
ccbus_ask peek "$req" || { case "$CCBUS_CODE" in bad_*) exit 2 ;; *) exit 1 ;; esac; }
token=$(printf '%s' "$CCBUS_OUT" | jq -r '.token // empty')
[ -n "$token" ] || exit 0                      # 没有新信:不出正文、不出令牌
printf '%s' "$CCBUS_OUT" | jq -j '.text'
if [ -n "$tokfile" ]; then
  printf '%s' "$token" > "$tokfile"
else
  printf '%s\n' "$token" >&2
fi
