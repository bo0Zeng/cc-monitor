#!/bin/sh
# 假的 `claude auth login --claudeai`（判据 `tests/backend/accounts/login_tests.rs` 用；照 loginapp 实测的那几行说话，链接 · 码 · 凭据都是假串）。
# 后端交来的三个参数是 `auth login --claudeai`：前两个跳过，第三个不对就照实退出（`protocol_doc_guard` 从这里读它认的旗标）。
shift 2
[ "${1:-}" = --claudeai ] || { echo "unexpected args: $*" >&2; exit 2; }
echo "Opening browser to sign in…"
echo "If the browser didn't open, visit: https://claude.com/cai/oauth/authorize?code=true&client_id=fake&state=st8"
printf "Paste code here if prompted > "
while read -r line; do
  case "$line" in
    "good#st8") printf '{"claudeAiOauth":{"accessToken":"fake"}}' > "$CLAUDE_CONFIG_DIR/.credentials.json"
                printf '{"oauthAccount":{"emailAddress":"a@example.test"}}' > "$CLAUDE_CONFIG_DIR/.claude.json"
                echo "Login successful."; exit 0;;
    "bad#st8") echo "Login failed: fake refusal" >&2; exit 1;;
    *) echo "Invalid code" >&2;;
  esac
done
