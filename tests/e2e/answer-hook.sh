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
  # 宽限不写：`--grace` 须大于后端退出排空上限（30 秒），写小了整条被拒成 bad_args、常驻一个都没停（10-11 留下三个就是这么来的）。
  "$BACKEND" -- --resident-stop >/dev/null 2>&1
  rm -rf -- "$WORK"
  tmux_shim_cleanup
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
for it in v.get("data", {}).get("items", []):
    r = it.get("session") or {}
    a = (r.get("needs") or {}).get("ask")
    if r.get("sid") == sys.argv[1] and a: print(a["id"]); break' "$SID"
}


echo "== 常驻不在 ⇒ hook 客户端空手退"
out="$(hook_input | "$BACKEND" -- --answer-hook claude)"; rc=$?
[ "$rc" -eq 0 ] && [ -z "$out" ] && ok "退出 0、stdout 一个字节都没有" || bad "rc=$rc out=$out"

echo "== 起沙箱常驻后端"
RPID="$(CCM_SANDBOX=1 "$BACKEND" -- --resident-ensure 2>/dev/null | python3 -I -c 'import json,sys; print(json.load(sys.stdin).get("pid") or "")' 2>/dev/null)"
wall_until '[ -S "$SOCK" ]' && ok "套接字在" || { bad "常驻没起来"; echo "合计 PASS=$pass FAIL=$fail"; exit 1; }
# 常驻后端裸调 `tmux`（会话快照 · 终端那几处）⇒ 它 PATH 上第一个 tmux 必须是这一趟的 shim（钉着本趟私有名），不然连到的是缺省 server。
rtmux="$(PATH="$(grep -z '^PATH=' "/proc/$RPID/environ" 2>/dev/null | tr -d '\0' | sed 's/^PATH=//')" command -v tmux)"
[ -n "${TMUX_SHIM_SOCK:-}" ] && grep -qF -- "-L $TMUX_SHIM_SOCK " "$rtmux" 2>/dev/null \
  && ok "常驻后端找到的 tmux 是本趟私有的那一台（-L $TMUX_SHIM_SOCK）" || bad "常驻后端（pid ${RPID:-?}）找到的 tmux 是 ${rtmux:-<没有>}，不是本趟私有的"

# 一条活会话：一个睡着的进程 ＋ 它的 pidfile 说在等批准（sessions-needs 只列活着的会话）。
sleep 60 & SLEEPER=$!
ticks="$(proc_start "$SLEEPER")"
printf '{"pid":%s,"sessionId":"%s","kind":"interactive","procStart":"%s","status":"waiting","waitingFor":"permission prompt","statusUpdatedAt":%s000}\n' \
  "$SLEEPER" "$SID" "$ticks" "$(date +%s)" > "$CLAUDE_CONFIG_DIR/sessions/$SLEEPER.json"

echo "== 经环境来的 hook（换了启动器那一支，子进程也继承得到）：发问的是会话里 Bash 起的子 claude ⇒ 空手退、不挂"
# 发问的进程 ＝ hook 的父进程：一个起来时环境里带着嵌套标记的 sh（`; :` 让它 fork 出 hook、自己留着当父进程）。
PLUG="$HOME/.cc-monitor/answer-plugin"
out="$(hook_input | env CLAUDE_CODE_PLUGIN_DIRS="/u/mine:$PLUG" CLAUDECODE=1 timeout "$WALL_CEILING_S" sh -c '"$0" -- --answer-hook claude; :' "$BACKEND")"; rc=$?
[ "$rc" -eq 0 ] && [ -z "$out" ] && ok "子进程那一发：退出 0、stdout 一个字节都没有、没挂着等" || bad "rc=$rc out=$out"
[ -z "$(ask_id)" ] && ok "needs 里没有它" || bad "子进程那一发挂进了 needs"
# 对照：发问的进程起来时没有嵌套标记（ccm 起会话前清掉了它）⇒ 认是 ccm 起的那一个，照常挂上。
#   sh 把 hook 放后台（stdin 先存到 3 号：后台命令的 stdin 会被换成 /dev/null）、记下它的 pid、等它（hook 的父进程仍是这个 sh）；收的时候按那个 pid 停。
hook_input | env CLAUDE_CODE_PLUGIN_DIRS="$PLUG" sh -c 'exec 3<&0; "$0" -- --answer-hook claude <&3 & echo $! > "$1"; wait' "$BACKEND" "$WORK/hook.pid" > /dev/null 2>&1 & SHPID=$!
wall_until '[ -n "$(ask_id)" ]' && ok "对照：没有嵌套标记的那一发照常挂上" || bad "对照：没挂上"
kill -TERM "$(cat "$WORK/hook.pid")" 2>/dev/null; wait "$SHPID" 2>/dev/null
wall_until '[ -z "$(ask_id)" ]' || bad "对照那一发没撤"

echo "== 挂上 ⇒ needs 出 ask ⇒ 答 ⇒ hook 打出决定"
hook_input | "$BACKEND" -- --answer-hook claude > "$WORK/out1" & HOOKPID=$!
wall_until '[ -n "$(ask_id)" ]' && ok "needs 里出了 ask" || bad "needs 里没有 ask"
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
wall_until '[ -n "$(ask_id)" ]' || bad "第二条没挂上"
id="$(ask_id)"
kill -TERM "$HOOKPID"; wait "$HOOKPID" 2>/dev/null; HOOKPID=""
wall_until '[ -z "$(ask_id)" ]' && ok "连接断了 ⇒ needs 里那条 ask 撤了" || bad "没撤"
r="$(ask needs-answer "{\"sid\":\"$SID\",\"ask\":\"$id\",\"choice\":\"allow\"}")"
case "$r" in *closed_elsewhere*) ok "再答 ⇒ closed_elsewhere" ;; *) bad "撤了之后再答：$r" ;; esac
[ ! -s "$WORK/out2" ] && ok "被杀的那个 hook 什么都没打" || bad "被杀的 hook 打了：$(cat "$WORK/out2")"

echo "== 收尾 ⇒ 常驻停了、这一趟起的进程一个不剩"
kill "$SLEEPER" 2>/dev/null; wait "$SLEEPER" 2>/dev/null; SLEEPER=""
r="$("$BACKEND" -- --resident-stop 2>&1)"
case "$r" in *'"stopped":"graceful"'*|*'"stopped":"killed"'*) ok "resident-stop 停下了常驻（$r）" ;; *) bad "resident-stop：$r" ;; esac
left="$(e2e_leftovers "$HOME")"
[ -z "$left" ] && ok "家目录是 $HOME 的进程一个不剩" || bad "这一趟留下的进程：$left"

echo "合计 PASS=$pass FAIL=$fail"
[ "$fail" -eq 0 ]
