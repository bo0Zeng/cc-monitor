#!/usr/bin/env bash
# cc-bus Stop 钩子:agent 想结束本轮时,先看收件箱;有新信就拦下并喂回。
#   两阶段:① 后端 `bus-peek` 纯读,拿正文 ＋ 令牌(不推已读位置)② 喂回成功之后才 `bus-commit`。
#   ⇒ 没确认就不推进:喂不回去,位置从头到尾没动,下一轮原样再来(at-least-once:可能重复,绝不吞掉)。
#   正文走文件交给词典(`agent_block_reason` 用 `--rawfile`,不走 argv:大批积压会撞单个参数的上限)。
#   `CCBUS_PEEK_MAX_LINES`:一轮最多喂几封(分段读);缺省不分段。「一次处理多少」归调用方(本钩子),不归后端。
set -uo pipefail
SELFDIR=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
cat >/dev/null                     # agent 从 stdin 传入 JSON(含 stop_hook_active 等),这里用不着
id=$("$SELFDIR/cc-bus-whoami.sh" 2>/dev/null || true)
[ -n "$id" ] || exit 0             # cc-monitor 说不出这是哪条会话(没装 · 不在会话里)= 不是总线成员,放行
# shellcheck source=cc-bus-lib.sh
. "$SELFDIR/cc-bus-lib.sh"
ccbus_agent_load || exit 0         # 词典装不上:放行(钩子的失败不该卡死这一轮)
tf=$(mktemp) || exit 0
rf=$(mktemp) || { rm -f "$tf"; exit 0; }
trap 'rm -f "$tf" "$rf"' EXIT
peekargs=()
[ -n "${CCBUS_PEEK_MAX_LINES:-}" ] && peekargs=(--max-lines "$CCBUS_PEEK_MAX_LINES")
msgs=$("$SELFDIR/cc-bus-peek.sh" "$id" --token-file "$tf" "${peekargs[@]+"${peekargs[@]}"}" 2>/dev/null || true)
[ -n "$msgs" ] && [ -s "$tf" ] || exit 0
{ printf '收到新的 cc-bus 消息,先处理完再结束本轮:\n'; printf '%s' "$msgs"; } > "$rf" || exit 0
if agent_block_reason "$rf"; then  # 拦下结束,把信喂回
  rc=0
  "$SELFDIR/cc-bus-commit.sh" "$id" "$(cat "$tf")" 2>/dev/null || rc=$?
  case "$rc" in
    0)  ;;
    11) echo "cc-bus stop-hook: 有人替我推过了(rc=11)——手上这段照常已喂回,不重试" >&2 ;;
    12) echo "cc-bus stop-hook: 收件箱在两跳之间变了(rc=12)——位置没推,下一轮会再喂一次" >&2 ;;
    *)  echo "cc-bus stop-hook: cc-bus commit rc=$rc —— 位置没推,信不会丢(下一轮还在)" >&2 ;;
  esac
else
  echo "cc-bus stop-hook: 喂回失败(rc=$?)——已读位置没动,信不会丢" >&2
fi
exit 0
