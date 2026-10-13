#!/bin/sh
# 假的 `claude mcp login <名> --no-browser`（判据 `tests/backend/accounts/sign_in_mcp_tests.rs` 用；照 2.1.296 内嵌 JS 那几行说话，链接 · 码 · 令牌都是假串）。
# 判据在前面接一行 `AUTH=<假 MCP 登录服务的地址>`。stdin · stdout 不是终端 ⇒ 照官方那句退出（核心得起在伪终端里）。
# 粘对了 ⇒ 拿码去假服务的令牌端点换，换来的写进 `$CLAUDE_CONFIG_DIR/.credentials.json` 的 `mcpOAuth`、删掉需登录缓存、退 0；
# 换不成 ⇒ 退 1；粘的不是这一趟的回跳地址 ⇒ 说一句、接着等。
# 后端交来的是 `mcp login --no-browser <名>`：前两个核过跳过，第三个不对就照实退出（`protocol_doc_guard` 从这里读它认的旗标）。
[ "$1 $2" = "mcp login" ] || { echo "unexpected args: $*" >&2; exit 2; }
shift 2
[ "${1:-}" = --no-browser ] || { echo "unexpected args: $*" >&2; exit 2; }
name=$2
printf 'Starting authentication for "%s"…\n' "$name"
back="http://localhost:43117/callback"
link="$AUTH/authorize?response_type=code&client_id=fake-client&redirect_uri=http%3A%2F%2Flocalhost%3A43117%2Fcallback&state=st9"
printf 'Visit this URL to authorize:\n  \033]8;;%s\007\033[94m%s\033[39m\033]8;;\007\n' "$link" "$link"
printf 'Waiting for authorization… (^C to cancel)\n'
if [ ! -t 0 ] || [ ! -t 1 ]; then
  echo "Couldn't complete authentication for \"$name\": stdin isn't a terminal, so authentication can't be completed here." >&2
  exit 1
fi
while :; do
  printf 'Or paste the redirect URL here: '
  read -r line || exit 1
  case "$line" in
    "$back?"*"state=st9"*) ;;
    *) echo "That doesn't look like a redirect URL for this sign-in — paste the full address from the browser page this sign-in opened."; continue;;
  esac
  code=$(printf '%s' "$line" | sed -n 's/.*[?&]code=\([^&]*\).*/\1/p')
  tok=$(curl -s -f -X POST --data "code=$code" "$AUTH/token") || {
    echo "Couldn't complete authentication for \"$name\": the token endpoint refused the code" >&2
    exit 1
  }
  at=$(printf '%s' "$tok" | sed -n 's/.*"access_token":"\([^"]*\)".*/\1/p')
  rt=$(printf '%s' "$tok" | sed -n 's/.*"refresh_token":"\([^"]*\)".*/\1/p')
  exp=$(( $(date +%s) * 1000 + 3600000 ))
  printf '{"claudeAiOauth":{"accessToken":"fake-at","refreshToken":"fake-rt","expiresAt":1900000000000},"mcpOAuth":{"%s|0123456789abcdef":{"serverName":"%s","serverUrl":"%s/mcp","accessToken":"%s","refreshToken":"%s","expiresAt":%s,"clientId":"fake-client"}}}' \
    "$name" "$name" "$AUTH" "$at" "$rt" "$exp" > "$CLAUDE_CONFIG_DIR/.credentials.json"
  rm -f "$CLAUDE_CONFIG_DIR/mcp-needs-auth-cache.json"
  printf 'Authenticated with "%s". Its tools are now available in Claude Code.\n' "$name"
  exit 0
done
