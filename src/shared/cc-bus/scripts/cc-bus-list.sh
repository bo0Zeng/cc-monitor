#!/usr/bin/env bash
# cc-bus list  列出各收件箱(会话 id)、叫它什么、在不在、待读几条。名字与在不在只问 cc-monitor(`ccm -- --bus-list`);
#   没装 cc-monitor ⇒ 只列会话 id 与待读,并说名字与在不在判不了。
# cc-bus list --tsv  机器可读形(后端 bus-list 读它,不读收件箱本身):
#   首行 `#cc-list-tsv<TAB>2` · 每个收件箱一行 `会话 id<TAB>待读` · 末行 `#skipped<TAB><坏行数>`。
#   坏行 = 收件箱文件名不过字符集的。
set -euo pipefail
BUS="${CC_BUS_HOME:-$HOME/.cc-bus}"
SELFDIR=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)

# shellcheck source=cc-bus-lib.sh
. "$SELFDIR/cc-bus-lib.sh" || exit 13

# 一个收件箱一行:`会话 id<TAB>待读`(待读与 `cc-bus unread` 同一处算:`ccbus_unread_of`)。名字不过字符集的计坏行。
inboxes() {
  local f id bad=0
  for f in "$BUS"/inbox/*.jsonl; do
    [ -e "$f" ] || continue
    id=$(basename "$f" .jsonl)
    case "$id" in *[!A-Za-z0-9_-]*|'') bad=$((bad+1)); continue ;; esac
    printf '%s\t%s\n' "$id" "$(ccbus_unread_of "$id")"
  done
  printf '#skipped\t%d\n' "$bad"
}

if [ "${1:-}" = --tsv ]; then
  printf '#cc-list-tsv\t2\n'
  inboxes
  exit 0
fi

if ccm=$(ccbus_ccm_bin); then
  out=$("$ccm" -- --bus-list --within-ms 10000 </dev/null) || { echo "cc-bus list: cc-monitor did not answer bus-list" >&2; exit 1; }
  # 一行一个:会话 id · live(true / false / null = 判不了)· 待读 · 标题。照 cc-monitor 答的原样印,这里不判。
  printf 'id\tlive\tunread\ttitle\n'
  printf '%s' "$out" | jq -r '(.data // .).agents[] | "\(.id)\t\(.live)\t\(.unread)\t\(.title)"'
  exit 0
fi
echo "cc-bus list: no cc-monitor backend — titles and liveness cannot be told; inboxes only:" >&2
inboxes | grep -v '^#'
