#!/bin/sh
# 假的 `claude -p …` 开窗那一句（适配层 `agents/claudecode/mod.rs::WARM` 发的那一串；`protocol_doc_guard` 从这里读它认的旗标）。
# 只认那几个旗标，各带不带值照真 claude；认不得的照实退出。说完回一段 `--output-format json` 那一形。
while true; do
  case "${1:-}" in
    -p) shift;;
    --model) shift 2;;
    --tools) shift 2;;
    --no-session-persistence) shift;;
    --setting-sources) shift 2;;
    --strict-mcp-config) shift;;
    --output-format) shift 2;;
    --system-prompt) shift 2;;
    --*) echo "unexpected flag: $1" >&2; exit 2;;
    *) break;;
  esac
done
[ $# -eq 1 ] || { echo "unexpected args: $*" >&2; exit 2; }
printf '{"type":"result","is_error":false,"result":"a","usage":{"input_tokens":870,"output_tokens":4}}\n'
