#!/usr/bin/env bash
# app 起会话时真正跑的那一行（后端 `session-open` 本体交出去的那一行）—— e2e 取串的唯一出口。
#
# 用法：open-line-emit.sh shape=<…> [sid=…] [cwd=…] [name=…] launcher=<…> [account=<号名>|base|-] [model=…] [here=local|remote]
#   （或整份意图放进 CCM_E2E_OPEN）。shape ∈ resume-tmux · into-existing · direct · attach · new-tmux；account 缺 / `-` ＝ 跟随；
#   here 缺 ＝ remote。跑 `#[ignore]` 数据出口
#   （`session_open_tests.rs::emit_open_line_for_e2e`），只印那一行（`OPEN_LINE<<<…>>>` 里的串）。
# 「没输出」不是绿：编译失败、过滤器打错都会得到 0 个测试、退出码 0 —— 取不到标记行 ⇒ 退 2；拒了 ⇒ 理由进 stderr、退 3。
set -uo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
if [ $# -gt 0 ]; then
  CCM_E2E_OPEN="$(python3 -I -c '
import json, sys
kv = dict(a.split("=", 1) for a in sys.argv[1:])
kv.setdefault("here", "remote")
acct = kv.pop("account", "-")
if acct == "base":
    kv["account"] = {"kind": "base"}
elif acct != "-":
    kv["account"] = {"kind": "named", "name": acct}
print(json.dumps(kv))' "$@")" || exit 2
  export CCM_E2E_OPEN
fi
[ -n "${CCM_E2E_OPEN:-}" ] || { echo "缺意图（参数或 CCM_E2E_OPEN）" >&2; exit 2; }
OUT="$(cd "$REPO/src/backend" && cargo test --lib -- --ignored --nocapture --exact control::session_open::tests::emit_open_line_for_e2e 2>&1)"
LINE="$(printf '%s\n' "$OUT" | grep -E 'OPEN_LINE(_ERR)?<<<' | head -1)"
case "$LINE" in
  OPEN_LINE\<\<\<*) LINE="${LINE#OPEN_LINE<<<}"; printf '%s\n' "${LINE%>>>}" ;;
  OPEN_LINE_ERR\<\<\<*) echo "session-open 拒了：${LINE#OPEN_LINE_ERR<<<}" >&2; exit 3 ;;
  *) printf '%s\n' "$OUT" | tail -40 >&2; echo "取不到 session-open 交出去的那一行 —— e2e 无对象可跑（别当绿过）" >&2; exit 2 ;;
esac
