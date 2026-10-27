#!/usr/bin/env bash
# cc-bus send <to-sid> --act <code> --title <one line> [--tier 1|2] [--re <msg-id>] [--ref <ref>]... [--] [body...]
#   组请求 → 交后端 `bus-send`(信封 · 范围 · 去重 · 限流都在那一处判,判完它收进收件人的收件箱)→ 后端说要敲门才敲。
#   act：ask · answer · request · inform · handback · correct（answer / handback / correct 必带 --re）。
#   tier：1 ⇒ 敲门（对方闲着才送）；2（缺省）⇒ 只进收件箱、轮末由 Stop 钩子喂回。
#   正文：其余参数用空格连起来；写 `-` ⇒ 从 stdin 读。正文不超过 1200 字节，长的写成文件、用 --ref 给路径。
#   退出码：0 已投递 · 2 信封被拒 / 参数用错 · 3 被范围（组）· 重复 · 限流拦下 · 1 后端不在 / 投递出错。
set -uo pipefail
SELFDIR=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
# shellcheck source=cc-bus-lib.sh
. "$SELFDIR/cc-bus-lib.sh"

usage() { sed -n '2,6p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }

to="${1:-}"; [ -n "$to" ] || usage; shift
act=""; title=""; tier=""; re=""; refs=(); body=()
while [ $# -gt 0 ]; do
  case "$1" in
    --act)   [ $# -ge 2 ] || usage; act="$2"; shift 2 ;;
    --title) [ $# -ge 2 ] || usage; title="$2"; shift 2 ;;
    --tier)  [ $# -ge 2 ] || usage; tier="$2"; shift 2 ;;
    --re)    [ $# -ge 2 ] || usage; re="$2"; shift 2 ;;
    --ref)   [ $# -ge 2 ] || usage; refs+=("$2"); shift 2 ;;
    --)      shift; body+=("$@"); break ;;
    *)       body+=("$1"); shift ;;
  esac
done
if [ "${#body[@]}" -eq 1 ] && [ "${body[0]}" = "-" ]; then text=$(cat); else text="${body[*]-}"; fi
from=$("$SELFDIR/cc-bus-whoami.sh" 2>/dev/null || true)   # 会话 id；认不出就交空串，后端拒成 bad_from

# 请求原样交给后端：每一格都不在这里判（tier 不是数字就原样当串交，后端按入参形状拒）。
req=$(jq -cn --arg from "$from" --arg to "$to" --arg act "$act" --arg title "$title" \
  --arg tier "$tier" --arg re "$re" --arg text "$text" --args \
  '{from:$from, to:$to, act:$act, title:$title, text:$text, refs:$ARGS.positional}
   + (if $tier == "" then {} else {tier: ($tier | tonumber? // $tier)} end)
   + (if $re == "" then {} else {re:$re} end)' "${refs[@]+"${refs[@]}"}")

if ! ccbus_ask send "$req"; then
  echo "cc-bus send: not sent (${CCBUS_CODE:-failed}) -> $to" >&2
  case "$CCBUS_CODE" in
    bad_*|need_re|too_long) exit 2 ;;
    not_same_group|no_roster|duplicate|throttled) exit 3 ;;
    *) exit 1 ;;
  esac
fi
if [ "$(printf '%s' "$CCBUS_OUT" | jq -r '.knock')" = true ]; then
  ccbus_say "$to" "$(ccbus_knock_text "$from")" || true
fi
printf 'sent %s -> %s\n' "$(printf '%s' "$CCBUS_OUT" | jq -r '.envelope.id')" "$to"
