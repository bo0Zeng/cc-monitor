#!/usr/bin/env bash
# cc-bus whoami  打印本 agent 在总线上的身份：**会话 id**（信封的 from / to 只许会话 id；形状由后端 `bus-send` 那一处核）。
# 「我是哪条会话」只问 cc-monitor 的公开命令 `ccm -- --session-self`（后端按调用进程往上找，Claude · Codex 同一条）。
# 找不到 ccm ⇒ 说没装、退 1；ccm 说不在会话里 ⇒ 照它那一句、退 1（cc-bus send 照交空串，后端拒成 bad_from）。
set -uo pipefail
SELFDIR=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
# shellcheck source=cc-bus-lib.sh
. "$SELFDIR/cc-bus-lib.sh"

ccm=$(ccbus_ccm_bin) || { echo "cc-bus whoami: no cc-monitor backend (~/.cc-monitor/bin/ccm) — cannot tell which session this is" >&2; exit 1; }
errf=$(mktemp) || exit 1
trap 'rm -f "$errf"' EXIT
out=$("$ccm" -- --session-self --within-ms 10000 </dev/null 2>"$errf")
rc=$?
id=$(printf '%s' "$out" | jq -r '(.data // .).sid // empty' 2>/dev/null)
if [ "$rc" -eq 0 ] && [ -n "$id" ]; then printf '%s\n' "$id"; exit 0; fi
said=$(jq -r '.said // empty' < "$errf" 2>/dev/null)
echo "cc-bus whoami: ${said:-cc-monitor did not say which session this is}" >&2
exit 1
