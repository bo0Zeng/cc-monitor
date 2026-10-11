#!/usr/bin/env bash
# 答题 hook 端到端（真后端二进制、沙箱家目录与私有套接字，不起任何真 agent）：
#   常驻不在 ⇒ hook 客户端空手退；常驻在 ⇒ 喂一份 PermissionRequest（照 10-10 真机录下的那一份脱敏）⇒ 这条会话的 needs 出 ask ⇒
#   needs-answer 答 ⇒ hook 客户端 stdout 打出那一家要的决定、退出 0；再答一次 ⇒ already_answered；hook 那一头被杀 ⇒ 撤掉、再答 closed_elsewhere。
# 跑法：bash tests/e2e/answer-hook.sh   （npm run test:answer-hook）
. "$(cd "$(dirname "$0")" && pwd)/sandbox-env.sh"  # 无条件清掉继承来的 CCM_* / CLAUDE* / ANTHROPIC_* / TMUX* / CC_BUS_*
set -uo pipefail

E2E_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$E2E_DIR/../.." && pwd)"
BACKEND="${CCM_E2E_BACKEND:-$REPO/.build/backend/debug/cc-monitor-backend}"
[ -x "$BACKEND" ] || { echo "backend 二进制不存在/不可执行：$BACKEND（先 cd src/backend && cargo build）"; exit 1; }
WORK="$(mktemp -d /tmp/e2e-ah.XXXXXX)"
export HOME="$WORK/home"
export CLAUDE_CONFIG_DIR="$WORK/claude"
mkdir -p "$HOME" "$CLAUDE_CONFIG_DIR/sessions"
SOCK="$HOME/.cc-monitor/run/backend.sock"
SID="0000a5c0-0000-4000-8000-0000000000a1"
pass=0; fail=0
ok() { pass=$((pass + 1)); echo "  ok   $1"; }
bad() { fail=$((fail + 1)); echo "  FAIL $1"; }
SLEEPER=""; HOOKPID=""
cleanup() {
  set +e
  [ -n "$HOOKPID" ] && kill "$HOOKPID" 2>/dev/null
  [ -n "$SLEEPER" ] && kill "$SLEEPER" 2>/dev/null
  "$BACKEND" -- --resident-stop --grace 5 >/dev/null 2>&1
  rm -rf -- "$WORK"
}
trap cleanup EXIT

# 10-10 真机那一份 PermissionRequest 的形（键原样；路径 · 会话 id · 命令换成编的）。
hook_input() {
  cat <<JSON
{"session_id":"$SID","transcript_path":"$CLAUDE_CONFIG_DIR/projects/-w/$SID.jsonl","cwd":"/w","scratchpad_dir":"/tmp/x/scratchpad","prompt_id":"00000000-0000-4000-8000-0000000000b2","permission_mode":"default","effort":{"level":"medium"},"hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"touch probe1.txt","description":"Create probe1.txt"},"permission_suggestions":[{"type":"addDirectories","directories":["/w"],"destination":"session"},{"type":"setMode","mode":"acceptEdits","destination":"session"}]}
JSON
}

# 经常驻套接字问一句（hello → attach → 一条命令 → 等它的应答）：打印应答那一行。
ask() {
  python3 -I - "$SOCK" "$1" "$2" <<'PY'
import json, socket, sys
sock, cmd, args = sys.argv[1], sys.argv[2], json.loads(sys.argv[3])
s = socket.socket(socket.AF_UNIX); s.connect(sock); f = s.makefile("rwb")
assert json.loads(f.readline())["kind"] == "hello"
f.write(b'{"attach":true}\n'); f.flush()
assert json.loads(f.readline()).get("attach") == "ok"
f.write((json.dumps({"id": "q", "cmd": cmd, "args": args}) + "\n").encode()); f.flush()
for line in f:
    v = json.loads(line)
    if v.get("kind") == "reply" and v.get("id") == "q":
        print(json.dumps(v)); break
PY
}

# 这条会话此刻挂着的 ask id（没有 ⇒ 空）。
ask_id() {
  ask sessions-needs '{}' | python3 -I -c '
import json, sys
v = json.load(sys.stdin)
for r in v.get("data", {}).get("waiting", []):
    a = (r.get("needs") or {}).get("ask")
    if r.get("sid") == sys.argv[1] and a: print(a["id"]); break' "$SID"
}

wait_for() {  # $1 = 条件命令，至多 10 秒
  local i
  for i in $(seq 1 100); do eval "$1" && return 0; sleep 0.1; done
  return 1
}

echo "== 常驻不在 ⇒ hook 客户端空手退"
out="$(hook_input | "$BACKEND" -- --answer-hook claude)"; rc=$?
[ "$rc" -eq 0 ] && [ -z "$out" ] && ok "退出 0、stdout 一个字节都没有" || bad "rc=$rc out=$out"

echo "== 起沙箱常驻后端"
CCM_SANDBOX=1 "$BACKEND" -- --resident-ensure >/dev/null 2>&1
wait_for '[ -S "$SOCK" ]' && ok "套接字在" || { bad "常驻没起来"; echo "合计 PASS=$pass FAIL=$fail"; exit 1; }

# 一条活会话：一个睡着的进程 ＋ 它的 pidfile 说在等批准（sessions-needs 只列活着的会话）。
sleep 60 & SLEEPER=$!
ticks="$(awk '{print $22}' "/proc/$SLEEPER/stat")"
printf '{"pid":%s,"sessionId":"%s","kind":"interactive","procStart":"%s","status":"waiting","waitingFor":"permission prompt","statusUpdatedAt":%s000}\n' \
  "$SLEEPER" "$SID" "$ticks" "$(date +%s)" > "$CLAUDE_CONFIG_DIR/sessions/$SLEEPER.json"

echo "== 挂上 ⇒ needs 出 ask ⇒ 答 ⇒ hook 打出决定"
hook_input | "$BACKEND" -- --answer-hook claude > "$WORK/out1" & HOOKPID=$!
wait_for '[ -n "$(ask_id)" ]' && ok "needs 里出了 ask" || bad "needs 里没有 ask"
id="$(ask_id)"
r="$(ask needs-answer "{\"sid\":\"$SID\",\"ask\":\"$id\",\"choice\":\"always-0\"}")"
case "$r" in *'"ok": true'*'"answered"'*) ok "needs-answer 回 answered" ;; *) bad "needs-answer：$r" ;; esac
wait "$HOOKPID"; rc=$?; HOOKPID=""
[ "$rc" -eq 0 ] && ok "hook 客户端退出 0" || bad "hook 客户端退出 $rc"
want='{"hookSpecificOutput":{"decision":{"behavior":"allow","updatedPermissions":[{"destination":"session","directories":["/w"],"type":"addDirectories"}]},"hookEventName":"PermissionRequest"}}'
got="$(python3 -I -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1])), sort_keys=True, separators=(",", ":")))' "$WORK/out1" 2>/dev/null)"
[ "$got" = "$want" ] && ok "stdout 是那一家要的决定（带上那条以后不再问）" || bad "stdout：$(cat "$WORK/out1")"
r="$(ask needs-answer "{\"sid\":\"$SID\",\"ask\":\"$id\",\"choice\":\"deny\"}")"
case "$r" in *already_answered*) ok "再答一次 ⇒ already_answered" ;; *) bad "再答：$r" ;; esac

echo "== hook 那一头走了 ⇒ 撤掉"
hook_input | "$BACKEND" -- --answer-hook claude > "$WORK/out2" & HOOKPID=$!
wait_for '[ -n "$(ask_id)" ]' || bad "第二条没挂上"
id="$(ask_id)"
kill -TERM "$HOOKPID"; wait "$HOOKPID" 2>/dev/null; HOOKPID=""
wait_for '[ -z "$(ask_id)" ]' && ok "连接断了 ⇒ needs 里那条 ask 撤了" || bad "没撤"
r="$(ask needs-answer "{\"sid\":\"$SID\",\"ask\":\"$id\",\"choice\":\"allow\"}")"
case "$r" in *closed_elsewhere*) ok "再答 ⇒ closed_elsewhere" ;; *) bad "撤了之后再答：$r" ;; esac
[ ! -s "$WORK/out2" ] && ok "被杀的那个 hook 什么都没打" || bad "被杀的 hook 打了：$(cat "$WORK/out2")"

echo "合计 PASS=$pass FAIL=$fail"
[ "$fail" -eq 0 ]
