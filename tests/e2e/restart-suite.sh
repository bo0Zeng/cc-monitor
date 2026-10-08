#!/usr/bin/env bash
# 换号重启（帧命令 `session-restart`）真进程端到端：真后端二进制 ＋ 私有 tmux server ＋ 假 claude，经流的入方向发命令、读应答。
#
# 覆盖：先压缩并等到摘要 ⇒ 同一终端名里用新号起、新进程报出 · 压缩超时照常重启（本机那一形）· 停不了 ⇒ 不起新的 ·
#       号选不了 ⇒ 什么都不动 · 等压缩时撤单 ⇒ 不停不起。新进程用的是新号的目录、经 ccm 起（进程环境里有中转地址）。
# 账号目录的 `sessions/` `projects/` 链回共享的 agent 家（与真机布局同形），后端盯的就是那一份。
# 红线：只用私有 tmux server（`tmux-shim.sh` 强插 `-L`）· 家目录整个换成沙箱 · 只收本套件起的进程。
. "$(cd "$(dirname "$0")" && pwd)/sandbox-env.sh"  # 无条件清掉继承来的 CCM_* / CLAUDE* / ANTHROPIC_* / TMUX* / CC_BUS_*
set -euo pipefail

# shellcheck source=tests/e2e/tmux-shim.sh
. "$(cd "$(dirname "$0")" && pwd)/tmux-shim.sh" e2eRestart
# 后端二进制以 `ccm` 之名上 PATH、家目录换成沙箱（要先 build：二进制在 .build/backend/debug/cc-monitor-backend）。
# shellcheck source=tests/e2e/ccm-shim.sh
. "$(cd "$(dirname "$0")" && pwd)/ccm-shim.sh"

E2E="$(cd "$(dirname "$0")" && pwd)"
WORK="$(mktemp -d /tmp/e2e-restart.XXXXXX)"
# 假 claude 以 `claude` 之名放进 ASCII 的 $WORK（前台命令名要认得出是 agent；启动器路径要过字符闸）。
mkdir -p "$WORK/bin"
cp "$E2E/fake-claude" "$WORK/bin/claude" && chmod +x "$WORK/bin/claude"
FAKE="$WORK/bin/claude"
CWD_DIR="/tmp/e2e-remote"
mkdir -p "$CWD_DIR"

SHARED="$HOME/.claude"
mkdir -p "$SHARED/sessions" "$SHARED/projects"
OLD="$CCM_SHIM_ACCOUNTS/bold"
NEW="$CCM_SHIM_ACCOUNTS/znew"
for d in "$OLD" "$NEW"; do
  mkdir -p "$d"
  printf '{}\n' >"$d/.credentials.json"
  ln -s "$SHARED/sessions" "$d/sessions"
  ln -s "$SHARED/projects" "$d/projects"
done
printf '{"version":1,"accounts":[{"name":"bold","configDir":"%s","isDefault":false},{"name":"znew","configDir":"%s","isDefault":true}]}\n' \
  "$OLD" "$NEW" >"$CCM_SHIM_ACCOUNTS/accounts.json"

# 中转：一个在听的回环口 ＋ 一把假钥匙（ccm 在最终那一跳判注入）。
mkdir -p "$HOME/.cc-monitor"
python3 -c 'import secrets;print(secrets.token_hex(32))' >"$HOME/.cc-monitor/relay-key"
python3 -c '
import socket,sys
s=socket.socket();s.bind(("127.0.0.1",0));s.listen(64)
open(sys.argv[1],"w").write(str(s.getsockname()[1]))
while True:
    c,_=s.accept();c.close()
' "$WORK/relay-port" &
RELAY_PID=$!
for _ in $(seq 1 50); do [ -s "$WORK/relay-port" ] && break; sleep 0.1; done
CCM_RELAY_PORT="$(cat "$WORK/relay-port")"
export CCM_RELAY_PORT
# 新起的假 claude 收到 `/compact` 就写一条压缩摘要（私有 tmux server 带着这份环境）。
export CCM_FAKE_COMPACT=answer

pass=0; fail=0
ok()  { echo "  PASS $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }

IN="$WORK/in.fifo"; OUT="$WORK/out.jsonl"; ERR="$WORK/backend.stderr"
cleanup() {
  set +e
  exec 3>&- 2>/dev/null
  [ -n "${BACKEND_PID:-}" ] && kill "$BACKEND_PID" 2>/dev/null
  [ -n "${RELAY_PID:-}" ] && kill "$RELAY_PID" 2>/dev/null
  for pf in "$SHARED"/sessions/*.json; do
    [ -f "$pf" ] || continue
    p="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["pid"])' "$pf" 2>/dev/null)"
    [ -n "$p" ] && kill "$p" 2>/dev/null
  done
  rm -rf "$WORK"
}
trap 'cleanup; tmux_shim_cleanup; ccm_shim_cleanup' EXIT

command -v tmux >/dev/null || { echo "无 tmux"; exit 1; }

mkfifo "$IN"
"$CCM_E2E_BIN" -- --tail-only <"$IN" >"$OUT" 2>"$ERR" &
BACKEND_PID=$!
exec 3>"$IN"
send() { printf '%s\n' "$1" >&3; }
# 等某个 id 的那一帧（应答 / 撤单），回显它；`$2` 秒内没等到 ⇒ 非零。
frame_of() {
  local id="$1" to="$2" i line
  for ((i=0; i<to*10; i++)); do
    line="$(grep -F "\"id\":\"$id\"" "$OUT" | grep -E '"kind":"(reply|cancelled)"' | head -1 || true)"
    [ -n "$line" ] && { printf '%s\n' "$line"; return 0; }
    sleep 0.1
  done
  return 1
}
field() { python3 -c 'import json,sys;v=json.loads(sys.argv[1]);
for k in sys.argv[2].split("."): v=(v or {}).get(k)
print(v if v is not None else "")' "$1" "$2"; }

for _ in $(seq 1 100); do grep -qF '"kind":"hello"' "$OUT" && break; sleep 0.1; done
grep -qF '"kind":"hello"' "$OUT" && ok "后端发出 hello" || { bad "10s 内没等到 hello"; tail -20 "$ERR"; }
grep -m1 -F '"kind":"hello"' "$OUT" | grep -qF '"session-restart"' && ok "hello.commands 里有 session-restart" || bad "hello 里没有 session-restart"

# 起一条活会话：用旧号，前台是假 claude（`extra` 是给这一个的额外环境）。回显 `sid 会话名`。
# 前台命令名要认得出是 agent：tmux 读的是 argv[0] ⇒ 以 `claude` 之名起那份脚本（同真 claude 的 argv[0]）。
make_live() {  # <extra env>
  local sid name
  sid="$(cat /proc/sys/kernel/random/uuid)"; name="rs-${sid:0:8}-cc"
  tmux new-session -d -s "$name" -c "$CWD_DIR" \
    "env CLAUDE_CONFIG_DIR='$OLD' $1 bash -c 'exec -a claude /bin/sh \"\$0\" \"\$1\"' '$FAKE' '$sid'"
  tmux set-option -t "=$name:" @ccm_sid "$sid"
  # 等到假 claude 两样都落了：pidfile（后端判活）**和**会话记录（压缩那一步按 sid 找记录、装耳朵）。
  # 假 claude 先落 pidfile、后落记录；只等前者，机器忙时下一步的压缩正好落在两者之间 ⇒ 「找不到记录」（R2 间歇红的根因）。
  for _ in $(seq 1 50); do
    grep -lqF "\"$sid\"" "$SHARED"/sessions/*.json 2>/dev/null &&
      compgen -G "$SHARED/projects/*/$sid.jsonl" >/dev/null && break
    sleep 0.1
  done
  printf '%s %s\n' "$sid" "$name"
}
# 这个 sid 此刻活着的进程数。
live_pids() {
  local n=0 pf p
  for pf in "$SHARED"/sessions/*.json; do
    [ -f "$pf" ] || continue
    grep -qF "\"$1\"" "$pf" || continue
    p="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["pid"])' "$pf")"
    kill -0 "$p" 2>/dev/null && n=$((n+1))
  done
  echo "$n"
}
resumed_in() { grep -cE "sid=$2 .*argv=--resume $2" "$1/argv.log" 2>/dev/null || true; }
restart_req() {  # <id> <sid> <account> <compact_first> <compact_ms> <local>
  printf '{"id":"%s","cmd":"session-restart","args":{"sid":"%s","cwd":"%s","account":"%s","compact_first":%s,"compact_within_ms":%s,"arrive_within_ms":20000,"local":%s,"agent":"claude","launcher":"%s","defaultLauncher":"claude"}}' \
    "$1" "$2" "$CWD_DIR" "$3" "$4" "$5" "$6" "$FAKE"
}

echo "== 换号重启（session-restart）真进程端到端 =="

# ── R1 先压缩：等到摘要 ⇒ 停旧 ⇒ 同名用新号起 ⇒ 等到它报出 ──────────────────────────────
read -r SID1 NAME1 < <(make_live "")
send "$(restart_req r1 "$SID1" znew true 20000 false)"
if R1="$(frame_of r1 40)"; then
  echo "   reply: $R1"
  [ "$(field "$R1" ok)" = True ] && ok "R1 成功应答" || bad "R1 失败：$(field "$R1" code) $(field "$R1" message)"
  [ "$(field "$R1" data.compact)" = "done" ] && ok "R1 压缩：等到了摘要（done）" || bad "R1 compact=$(field "$R1" data.compact)"
  [ "$(field "$R1" data.started)" = arrived ] && ok "R1 新进程报出了（arrived）" || bad "R1 started=$(field "$R1" data.started)"
  [ "$(field "$R1" data.terminal)" = "$NAME1" ] && ok "R1 同一个终端名 $NAME1" || bad "R1 terminal=$(field "$R1" data.terminal)"
  [ "$(field "$R1" data.account.name)" = znew ] && ok "R1 用的号是 znew" || bad "R1 account=$(field "$R1" data.account.name)"
else bad "R1 40s 内没等到应答"; tail -20 "$ERR"; fi
[ "$(resumed_in "$NEW" "$SID1")" -ge 1 ] && ok "R1 新进程用的是新号的目录（CLAUDE_CONFIG_DIR=$NEW）" || bad "R1 新号目录里没有 resume"
grep -E "sid=$SID1 .*argv=--resume" "$NEW/argv.log" 2>/dev/null | grep -q ' relay=set ' && ok "R1 经 ccm 起（进程环境里有中转地址）" || bad "R1 起出来的进程没有中转地址"
[ "$(resumed_in "$OLD" "$SID1")" = 0 ] && ok "R1 旧号目录里没有 resume（没落回旧号）" || bad "R1 resume 落回了旧号"
[ "$(live_pids "$SID1")" = 1 ] && ok "R1 这条会话恰好一个进程在跑" || bad "R1 在跑的进程数=$(live_pids "$SID1")"
tagged=""
for _ in $(seq 1 50); do
  tagged="$(tmux list-panes -t "=$NAME1:" -F '#{@ccm_sid}' 2>/dev/null | grep -xF "$SID1" || true)"
  [ -n "$tagged" ] && break; sleep 0.1
done
[ -n "$tagged" ] && ok "R1 新会话挂着这条会话的 @ccm_sid（下一次还认得出它在哪）" || { bad "R1 新会话没挂 @ccm_sid"; tmux list-panes -a -F '#{session_name} #{pane_current_command} [#{@ccm_sid}]'; tmux show-options -t "=$NAME1:" 2>&1 | head; }

# ── R2 压缩超时（本机那一形）：到点照常重启 ─────────────────────────────────────────────
read -r SID2 _NAME2 < <(make_live "CCM_FAKE_COMPACT=ignore")
send "$(restart_req r2 "$SID2" znew true 1500 true)"
if R2="$(frame_of r2 40)"; then
  [ "$(field "$R2" data.compact)" = timed_out ] && ok "R2 压缩到点没见摘要（timed_out）" || bad "R2 compact=$(field "$R2" data.compact) $(field "$R2" message)"
  [ "$(field "$R2" data.started)" = arrived ] && ok "R2 照常重启、新进程报出" || bad "R2 started=$(field "$R2" data.started)"
else bad "R2 40s 内没等到应答"; fi
[ "$(resumed_in "$NEW" "$SID2")" -ge 1 ] && ok "R2 新号目录里有 resume" || bad "R2 没有 resume"

# ── R3 停不了（会话里多一个窗口 ⇒ 门不放）⇒ 不起新的 ─────────────────────────────────────
read -r SID3 NAME3 < <(make_live "")
tmux new-window -d -t "=$NAME3:" "sleep 2147483647"
send "$(restart_req r3 "$SID3" znew false 1000 false)"
if R3="$(frame_of r3 30)"; then
  [ "$(field "$R3" code)" = stop_failed ] && ok "R3 停失败 ⇒ stop_failed（why=$(field "$R3" data.why)）" || bad "R3 code=$(field "$R3" code)"
else bad "R3 30s 内没等到应答"; fi
sleep 1
[ "$(resumed_in "$NEW" "$SID3")" = 0 ] && ok "R3 没起新的" || bad "R3 停失败还起了新的"
[ "$(live_pids "$SID3")" = 1 ] && ok "R3 旧进程还活着" || bad "R3 旧进程数=$(live_pids "$SID3")"

# ── R4 号选不了 ⇒ account_unavailable，什么都不动 ───────────────────────────────────────
read -r SID4 _NAME4 < <(make_live "")
send "$(restart_req r4 "$SID4" nope true 1000 false)"
if R4="$(frame_of r4 30)"; then
  [ "$(field "$R4" code)" = account_unavailable ] && ok "R4 account_unavailable（requested=$(field "$R4" data.requested)）" || bad "R4 code=$(field "$R4" code)"
else bad "R4 30s 内没等到应答"; fi
[ "$(live_pids "$SID4")" = 1 ] && ok "R4 旧进程还活着" || bad "R4 旧进程数=$(live_pids "$SID4")"

# ── R5 等压缩时撤单 ⇒ 不停、不起 ────────────────────────────────────────────────────────
read -r SID5 _NAME5 < <(make_live "CCM_FAKE_COMPACT=ignore")
send "$(restart_req r5 "$SID5" znew true 30000 false)"
sleep 2
send '{"id":"c5","cmd":"cancel","args":{"target":"r5"}}'
if R5="$(frame_of r5 10)"; then
  printf '%s' "$R5" | grep -qF '"kind":"cancelled"' && ok "R5 撤单生效（cancelled）" || bad "R5 不是 cancelled：$R5"
else bad "R5 10s 内没等到撤单那一帧"; fi
sleep 1
[ "$(live_pids "$SID5")" = 1 ] && ok "R5 旧进程还活着（没停）" || bad "R5 旧进程数=$(live_pids "$SID5")"
[ "$(resumed_in "$NEW" "$SID5")" = 0 ] && ok "R5 没起新的" || bad "R5 撤单之后还起了新的"

echo "== 结果:$pass 过 / $fail 败 =="
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ]
