#!/bin/bash
# `K-R104`（2026-09-13）：**用量探针那两条帧面原语的真机验收** —— 整条重写。
#
# ## 它验的是什么（与上一版的分界线）
#
# 上一版（F10 / E42）验的是「monitor 渲染的那**一条 shell 串**在真 tmux 上干了什么」。
# 🔴 **那条串今天不存在了**：`K-R104` 把探针编排整条搬上 daemon 的帧面
#（`inbound::REGISTRY` 新增 `capture-pane` 与 `oneshot-session`），monitor 一个 shell 字符都不渲染。
# ⇒ 本套件跟着换被测面：把**真 daemon 二进制**起起来，往它 stdin 写**真帧行**，
#   读它 stdout 的应答帧，在**真 tmux**（隔离 socket）上看结果。
#
# 输入 = 真编码器产出的帧行（`cargo test --lib -- --ignored --nocapture
# emit_usage_probe_frames_for_e2e`，见 `src-tauri/src/account_usage.rs` 对应测试头注）——
# 不手搓等价 JSON。会话名由 daemon 铸，脚本按 `E2E_SESSION_PLACEHOLDER` 替换。
#
# ## 为什么必须有真跑这一层（同 `tests/e2e/daemon-cc-bus.sh` 头注那条）
#
# 单测把 `capture_argv` / `mint_name` / 分派档位那些钉住了，但它们证明不了
# **命令在真连接上调得动**：两条命令进了 `REGISTRY`、`hello.commands` 也报了它们、
# 分派臂也认，而真跑时可能在任何一环上静默失效。
#
# 红线：daemon 零改动（只跑它）· 不碰真 `~/.claude` · **不碰用户真实的 tmux server**。
#
# 跑法：bash tests/e2e/usage-probe-acceptance.sh   （npm run test:usage-probe）
set -o pipefail

E2E_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$E2E_DIR/../.." && pwd)"
DAEMON="${CCM_E2E_DAEMON:-$REPO/remote-daemon-proto/target/debug/cc-monitor-remote}"
[ -x "$DAEMON" ] || { echo "需要先 build daemon：cd remote-daemon-proto && cargo build"; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "需要 jq"; exit 1; }
REALTMUX="$(command -v tmux)" || { echo "需要 tmux"; exit 1; }

# ── 隔离：`C7i` 的共享原语。shim 全程在 PATH 最前，任何裸 `tmux` 都被强插 `-L <隔离socket>`。
#    ⚠ 这一条是事故换来的（08-11 一条同形态的探针把用户 9 个真实会话打没了）。
TMUX_SHIM_SOCK=e2eUsageProbe
# shellcheck source=tests/e2e/tmux-shim.sh
. "$E2E_DIR/tmux-shim.sh"

SP="$(mktemp -d /tmp/e2e-usage-probe.XXXXXX)"
CLAUDE_DIR="$SP/claude"; mkdir -p "$CLAUDE_DIR/projects"
IN="$SP/in.fifo"; OUT="$SP/out.jsonl"; ERR="$SP/daemon.stderr"
mkfifo "$IN"

cleanup() {
  set +e
  exec 3>&- 2>/dev/null
  [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
  rm -rf -- "$SP"
}
trap 'cleanup; tmux_shim_cleanup' EXIT

T() { "$REALTMUX" -L "$TMUX_SHIM_SOCK" "$@"; }
sessions() { T ls -F '#{session_name}' 2>/dev/null | sort | tr '\n' ' '; }

PASS=0; FAIL=0
ck() { if [ "$2" = "$3" ]; then printf 'PASS | %-58s | %s\n' "$1" "$3"; PASS=$((PASS+1));
       else printf 'FAIL | %-58s | 期望=%s 实得=%s\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi; }

# ── 假 claude stand-in：先打一行"欢迎"（让第一轮稳定轮询有内容可稳定），然后循环读行，
#    收到 "/usage" 就打印固定的合成用量文本 ＋ **当前列数**（几何那一格靠它验）。
BIN="$SP/bin"; mkdir -p "$BIN"
cat > "$BIN/FAKECLAUDE" <<'EOF'
#!/bin/sh
echo "Welcome to Claude Code (fake stand-in, not real claude)"
while IFS= read -r line; do
  if [ "$line" = "/usage" ]; then
    printf 'Current session\n  38%%\nCOLS=%s\n' "$(tput cols 2>/dev/null || echo '?')"
  fi
done
EOF
chmod +x "$BIN/FAKECLAUDE"
export PATH="$BIN:$PATH"

# ── 输入源：真编码器产的帧行 ────────────────────────────────────────────────
LINES="$SP/frames.tsv"
(cd "$REPO/src-tauri" && cargo test --lib -- --ignored --nocapture emit_usage_probe_frames_for_e2e 2>/dev/null) \
  | grep -P '^[a-z-]+\t\{' > "$LINES"
[ -s "$LINES" ] || { echo "cargo test 未产出任何帧行——检查上游 emit_usage_probe_frames_for_e2e 是否编译/运行成功"; exit 1; }
LINE() { grep -P "^$1\t" "$LINES" | cut -f2-; }
# 占位符**从 Rust 常量现读**，不在这里再写一份字面量（两侧各写一份就会漂）。
PLACEHOLDER="$(grep -oP 'E2E_SESSION_PLACEHOLDER: &str = "\K[^"]+' "$REPO/src-tauri/src/account_usage.rs")"
[ -n "$PLACEHOLDER" ] || { echo "读不到 E2E_SESSION_PLACEHOLDER —— 那个常量改名了？"; exit 1; }
# `K-R122`（09-14）：**这个函数原来叫 `FOR`，改名是因为 `shellcheck` 判它 error。**
# `SC1081` 逐字「Scripts are case sensitive. Use 'for', not 'FOR'」—— 它按「大小写写错的
# 关键字」判，定义处 ＋ 5 个调用点共 **6 处**全中（`--severity=error` ⇒ 整条 `e2e-smoke` job 红）。
# ⚠ 这是**脚本自己的病**，不是 shellcheck 误报：一个全大写的 `FOR` 在 bash 里合法，
#   但人和工具都会先把它读成关键字。⇒ 改名，不加 `# shellcheck disable=`。
# ⚠ **调用点一起改了**。现打（改名前，量于本工作树）：全仓匹配「那个旧名后面紧跟一个左括号」
#   只有下面这一处定义，别的文件零命中；本文件里的调用 5 处，全在下面改了。
#   新名 `frame_for` 改名前全仓零命中（所以不会撞上别人）。
frame_for() { LINE "$1" | sed "s/$PLACEHOLDER/$2/g"; }

# ── 起 daemon（一条长连接，fifo 当 stdin）────────────────────────────────────
CLAUDE_CONFIG_DIR="$CLAUDE_DIR" "$DAEMON" --tail-only <"$IN" >"$OUT" 2>"$ERR" &
DAEMON_PID=$!
exec 3>"$IN"   # 持住写端，否则第一个写者退出即 EOF

wait_for() {
  local pattern="$1" i
  for i in $(seq 1 200); do
    grep -q -- "$pattern" "$OUT" 2>/dev/null && return 0
    sleep 0.05
  done
  return 1
}
send() { printf '%s\n' "$1" >&3; }
# 等某个 id 的应答出现并把它整行吐出来。
# ⚠ 同一个 id 会被复用（抓屏那一轮一轮）—— 每次取**最后一行**，那就是这一趟的应答。
#    daemon 的「拒重复 id」只挡**同时在跑**的，逐条等应答之后再发下一条不会撞。
reply_of() {
  local id="$1" i line
  for i in $(seq 1 200); do
    line="$(grep -F "\"id\":\"$id\"" "$OUT" 2>/dev/null | tail -1)"
    [ -n "$line" ] && { printf '%s' "$line"; return 0; }
    sleep 0.05
  done
  return 1
}

wait_for '"kind":"hello"' || { echo "daemon 没吐 hello"; tail -5 "$ERR"; exit 1; }

echo "===== 场景 0：闸门 —— hello 声明的能力集里有那两条新原语 ====="
HELLO="$(grep '"kind":"hello"' "$OUT" | tail -1)"
ck "hello.commands 里有 capture-pane" "true" \
  "$(printf '%s' "$HELLO" | jq -r '.commands | index("capture-pane") != null')"
ck "hello.commands 里有 oneshot-session" "true" \
  "$(printf '%s' "$HELLO" | jq -r '.commands | index("oneshot-session") != null')"

echo
echo "===== 场景 1：正常路径 —— 起会话 → 送载荷 → 抓屏轮询 → 送 /usage → 抓屏 → 收尾 ====="
send "$(LINE oneshot)"
R="$(reply_of e2e-up-1)" || { echo "起会话没应答"; tail -5 "$ERR"; exit 1; }
ck "起会话成功" "true" "$(printf '%s' "$R" | jq -r '.ok')"
SESS="$(printf '%s' "$R" | jq -r '.data.session')"
ck "名字由 daemon 铸、落在专属前缀底下" "true" \
  "$(case "$SESS" in ccm-oneshot-usage-e2e-cc) echo true;; *) echo false;; esac)"
ck "回了句柄（后面破坏性动作对它下手，不对名字）" "true" \
  "$(printf '%s' "$R" | jq -r '.data.handle | test("^\\$[0-9]+$")')"
ck "那个会话真的在 tmux 上" "true" \
  "$(T has-session -t "=$SESS:" 2>/dev/null && echo true || echo false)"

send "$(frame_for send-payload "$SESS")"
ck "送启动载荷成功" "true" "$(reply_of e2e-up-3 | jq -r '.ok')"

# 调用方轮询：抓屏直到画面里出现 FAKECLAUDE 的欢迎行（**轮询在这一侧，daemon 里零定时器**）。
got_welcome=false
for i in $(seq 1 40); do
  sleep 0.25
  send "$(frame_for capture "$SESS")"
  SCREEN="$(reply_of e2e-up-5 | jq -r '.data.screen // ""')"
  case "$SCREEN" in *"fake stand-in"*) got_welcome=true; break;; esac
done
ck "抓屏拿回了真内容（stand-in 的欢迎行）" "true" "$got_welcome"

send "$(frame_for send-usage "$SESS")"
ck "送 /usage 成功" "true" "$(reply_of e2e-up-4 | jq -r '.ok')"
got_panel=false; cols=""
for i in $(seq 1 40); do
  sleep 0.25
  send "$(frame_for capture "$SESS")"
  SCREEN="$(reply_of e2e-up-5 | jq -r '.data.screen // ""')"
  case "$SCREEN" in *38%*) got_panel=true; cols="$(printf '%s' "$SCREEN" | grep -o 'COLS=[0-9]*' | tail -1)"; break;; esac
done
ck "抓到了 /usage 面板（38%）" "true" "$got_panel"
# 🔴 几何那一格：不给 -x/-y 时 detached 会话是 80 列，`/usage` 那张表会被折断。
ck "会话宽度是探针要的那个（不是 tmux 默认 80）" "COLS=200" "$cols"

send "$(frame_for kill "$SESS")"
ck "收尾杀会话成功" "true" "$(reply_of e2e-up-6 | jq -r '.ok')"
sleep 0.3
ck "探针会话用完即清（不残留）" "" "$(sessions)"

echo
echo "===== 场景 2：撞名 —— daemon 铸的那个名字已被占用 ⇒ 拒绝，且不碰别人那个会话 ====="
T new-session -d -s ccm-oneshot-usage-e2e-cc 'sh -c "while true; do sleep 1; done"'
T new-session -d -s decoy-keep-server-alive
sleep 0.3
ck "前置：撞名会话已存在" "true" \
  "$(T has-session -t '=ccm-oneshot-usage-e2e-cc:' 2>/dev/null && echo true || echo false)"
send "$(LINE oneshot | sed 's/e2e-up-1/e2e-up-7/')"
R2="$(reply_of e2e-up-7)"
ck "撞名被拒（不是静默接回）" "false" "$(printf '%s' "$R2" | jq -r '.ok')"
ck "拒绝码是 name_taken（不许被压进别的档）" "name_taken" "$(printf '%s' "$R2" | jq -r '.code')"
ck "别人那个会话没被动" "true" \
  "$(T has-session -t '=ccm-oneshot-usage-e2e-cc:' 2>/dev/null && echo true || echo false)"
T kill-session -t '=ccm-oneshot-usage-e2e-cc:' 2>/dev/null

echo
echo "===== 场景 3：自毁看门狗 —— 不发 kill，会话也必须到点自己没 ====="
send "$(LINE oneshot-shortdog)"
R3="$(reply_of e2e-up-2)"
ck "短看门狗那条起得起来" "true" "$(printf '%s' "$R3" | jq -r '.ok')"
DOG="$(printf '%s' "$R3" | jq -r '.data.session')"
ck "前置：它刚建出来时在" "true" \
  "$(T has-session -t "=$DOG:" 2>/dev/null && echo true || echo false)"
gone=false
for i in $(seq 1 40); do
  sleep 0.25
  T has-session -t "=$DOG:" 2>/dev/null || { gone=true; break; }
done
ck "到点之后它自己没了（没有人发过 kill）" "true" "$gone"
# 🔴 阴性对照：同一台 server 上没挂看门狗的那个还在 —— 否则「没了」也可能是 server 塌了。
ck "阴性对照：陪衬会话仍在（消失的原因是看门狗，不是 server 塌了）" "true" \
  "$(T has-session -t '=decoy-keep-server-alive:' 2>/dev/null && echo true || echo false)"
T kill-session -t '=decoy-keep-server-alive:' 2>/dev/null

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
