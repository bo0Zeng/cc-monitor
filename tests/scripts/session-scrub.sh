#!/usr/bin/env bash
# 起一条命令，先摘掉会话继承来的环境变量（表在同目录 `session-vars`，全仓只那一张）。
# 用法：bash tests/scripts/session-scrub.sh <命令…>
#   `heavy.sh` 起每份重活前、`quick-check.sh` 起每一步前都经它；要另给的环境（沙箱的 HOME · TMUX_TMPDIR）写在它后面的命令里。
#   `--list` 只印这一刻环境里会被摘掉的名字。
set -u
here=${BASH_SOURCE[0]%/*}; [ "$here" = "${BASH_SOURCE[0]}" ] && here=.
table="$here/session-vars"
take=(); keep=()
while IFS= read -r line || [ -n "$line" ]; do
  case "$line" in ''|'#'*) ;; '!'*) keep+=("${line#!}") ;; *) take+=("$line") ;; esac
done < "$table" || { echo "session-scrub: 读不了 $table" >&2; exit 2; }
[ ${#take[@]} -gt 0 ] || { echo "session-scrub: $table 里一条要摘的都没有" >&2; exit 2; }
hit() { local name=$1 p; shift
  for p in "$@"; do
    # shellcheck disable=SC2053  # 右边要的就是 glob：`X_*` 按前缀认
    [[ $name == $p ]] && return 0
  done
  return 1; }
gone=()
for k in $(compgen -e); do
  hit "$k" "${take[@]}" && ! hit "$k" ${keep[@]+"${keep[@]}"} && gone+=("$k")
done
if [ "${1:-}" = --list ]; then [ ${#gone[@]} -eq 0 ] || printf '%s\n' "${gone[@]}"; exit 0; fi
[ $# -gt 0 ] || { echo "用法：session-scrub.sh <命令…>" >&2; exit 2; }
for k in ${gone[@]+"${gone[@]}"}; do unset "$k"; done
exec "$@"
