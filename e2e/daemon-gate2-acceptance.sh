#!/usr/bin/env bash
# F03：**§34 Gate 2 在 daemon 侧的真机行为验收**（真 daemon 二进制 + 真 tmux server）。
#
# 与两条 Rust 轨道的分工：
#   - `gate-core` 的单测断言「判定函数怎么答」；
#   - `gate2_parity` / `control/gate.rs` 的测试断言「两侧对同一张表答得一样」；
#   - **本脚本断言「真 daemon 收到 `launch{send-into}` 之后，在真 tmux 上到底干了什么」。**
# 门禁只锁判定不锁行为是 R1 的教训（三门禁全绿仍放行过一个让 send-keys 完全失效的改动）。
#
# ★ **跨轨钉**：用例不是手搓的，**逐行来自那张唯一的判定表**
#   `src-tauri/src/backend/control/fixtures/gate2-golden.tsv` —— 与另两条轨道同一份。
#   表变了三条轨道一起变；某一轨偷偷放宽，与表的差异当场可见。
#
# 红线：**绝不碰用户真实的 tmux server**（unset TMUX + 私有 TMUX_TMPDIR）；不碰真 ~/.claude。
# 跑法：bash e2e/daemon-gate2-acceptance.sh   （需要 tmux + 已编译的 daemon；npm run test:daemon-gate2）
set -euo pipefail

# ── 隔离（同 inbound-daemon-frames.sh 的两件事，缺一不可）──────────────────────
#   ① unset TMUX —— 否则 $TMUX 会让客户端连外层那台 server 并**完全忽略** TMUX_TMPDIR；
#   ② TMUX_TMPDIR 必须是短路径 —— unix socket 路径上限 108 字节。
unset TMUX TMUX_PANE
TMUX_TMPDIR="$(mktemp -d /tmp/e2e-gsock.XXXXXX)"; export TMUX_TMPDIR
TMUX_BIN="$(command -v tmux)" || { echo "需要 tmux"; exit 1; }

E2E_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$E2E_DIR/.." && pwd)"
DAEMON="${CCM_E2E_DAEMON:-$REPO/remote-daemon-proto/target/debug/cc-monitor-remote}"
GOLDEN="$REPO/src-tauri/src/backend/control/fixtures/gate2-golden.tsv"
WORK="$(mktemp -d /tmp/e2e-gate2.XXXXXX)"
IN="$WORK/in.fifo"; OUT="$WORK/out.jsonl"; ERR="$WORK/daemon.stderr"

[ -x "$DAEMON" ] || { echo "daemon 二进制不存在/不可执行：$DAEMON（先 cd remote-daemon-proto && cargo build）"; exit 1; }
[ -s "$GOLDEN" ]  || { echo "判定表读不到：$GOLDEN"; exit 1; }

cleanup() {
  set +e
  exec 3>&- 2>/dev/null
  [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
  "$TMUX_BIN" kill-server 2>/dev/null
  rm -rf -- "$WORK" "$TMUX_TMPDIR"
}
trap cleanup EXIT

pass=0; fail=0; skip=0
ok()   { printf '  PASS %s\n' "$1"; pass=$((pass+1)); }
bad()  { printf '  FAIL %s\n' "$1"; fail=$((fail+1)); }
skipped() { printf '  skip %s\n' "$1"; skip=$((skip+1)); }

mkdir -p "$WORK/claude/projects"
mkfifo "$IN"
CLAUDE_CONFIG_DIR="$WORK/claude" "$DAEMON" --tail-only <"$IN" >"$OUT" 2>"$ERR" &
DAEMON_PID=$!
exec 3>"$IN"   # 持住写端，否则第一个写者退出即 EOF，入方向当场寿终

wait_for() { local p="$1" i; for i in $(seq 1 200); do grep -qF -- "$p" "$OUT" && return 0; sleep 0.05; done; return 1; }
send()     { printf '%s\n' "$1" >&3; }
reply_of() { grep -F "\"id\":\"$1\"" "$OUT" | head -1; }

wait_for '"kind":"hello"' || { echo "10s 内没等到 hello"; tail -20 "$ERR"; exit 1; }
echo "== F03 Gate 2 · daemon 真机验收 =="
echo "daemon : $DAEMON"
echo "判定表 : ${GOLDEN#"$REPO"/}"
echo

# ── 抽取器自检：判定表真的解析出用例了吗 ───────────────────────────────────────
ROWS="$(grep -vE '^\s*#' "$GOLDEN" | grep -c . || true)"
if [ "$ROWS" -ge 20 ]; then ok "抽取器：判定表解析出 $ROWS 行用例"
else echo "  BROKEN 判定表只解析出 $ROWS 行 —— 下面全部场景会零命中地绿"; exit 2; fi

n=0
while IFS=$'\t' read -r id name sid expect; do
  case "$id" in ''|\#*) continue ;; esac
  n=$((n+1))

  # `:` / `=` 是 tmux 目标语法：`parse_request` 在门**之前**就 invalid_args。
  # ⚠ 这一档**不 skip、照样验** —— 「更早的一道门也把它挡住了」是要真的量出来的，
  #    写进 skip 消息里就成了一句没人验证的断言（本仓记过这类账）。
  EARLY_SHAPE=no
  case "$name" in *:*|*=*) EARLY_SHAPE=yes ;; esac

  "$TMUX_BIN" kill-server 2>/dev/null || true
  sleep 0.2
  # `--` 让 `-cc` 这种以短横开头的名字不被 tmux 的 getopt 当成选项。
  "$TMUX_BIN" new-session -d -s "$name" 2>/dev/null \
    || "$TMUX_BIN" new-session -d -s -- "$name" 2>/dev/null \
    || { skipped "$id：tmux 建不出这个名字的会话（$name）"; continue; }
  case "$sid" in
    '<none>'|'<unset>') : ;;   # 两者在真机上都是「@ccm_sid 没设」
    *) "$TMUX_BIN" set-option -t "=$name:" @ccm_sid "$sid" >/dev/null 2>&1 || true ;;
  esac

  marker="CCMGATE_${id}"
  rid="e2e-gate2-$n"
  # payload 里不含引号/反斜杠 ⇒ 直接拼进 JSON 是安全的（case_id 的字符集由夹具自检管）。
  send "{\"id\":\"$rid\",\"cmd\":\"launch\",\"args\":{\"mode\":\"send-into\",\"name\":\"$name\",\"payload\":\"printf %s $marker\"}}"
  wait_for "\"id\":\"$rid\"" || { bad "$id：5s 内没等到应答"; continue; }
  R="$(reply_of "$rid")"
  sleep 0.4
  PANE="$($TMUX_BIN capture-pane -p -t "=$name:" 2>/dev/null || true)"
  LANDED=miss; printf '%s' "$PANE" | grep -q "$marker" && LANDED=hit

  if [ "$expect" = "rejected" ]; then
    # 拒绝这一档要同时满足两件事：应答说 false，**且 pane 真的没被污染**。
    # 只看应答会漏掉「回了错但载荷已经打出去了」那种最坏形态。
    WANT_CODE=wrong_owner
    [ "$EARLY_SHAPE" = yes ] && WANT_CODE=invalid_args   # 形状层那道更早的门（实测，不是断言）
    if printf '%s' "$R" | grep -qF '"ok":false' && printf '%s' "$R" | grep -qF "$WANT_CODE"; then
      if [ "$LANDED" = miss ]; then ok "$id（$name / sid=$sid）→ 拒绝（$WANT_CODE），pane 未被污染"
      else bad "$id：回了 $WANT_CODE，**但载荷已经打进去了** —— 门在动作之后，等于没有"; fi
    else
      bad "$id：期望 $WANT_CODE，实得 $R"
    fi
  else
    if printf '%s' "$R" | grep -qF '"ok":true'; then
      if [ "$LANDED" = hit ]; then ok "$id（$name / sid=$sid）→ 放行，载荷送达"
      else bad "$id：回了 ok 但 pane 里找不到载荷 —— typed 谎报了"; fi
    else
      bad "$id：期望放行（$expect），实得 $R"
    fi
  fi
done < "$GOLDEN"

# ── 场景 N+1：目标不存在 ⇒ 仍是 no_such_session，新门不许把这一档吞掉 ──────────
"$TMUX_BIN" kill-server 2>/dev/null || true; sleep 0.2
send '{"id":"e2e-gate2-nos","cmd":"launch","args":{"mode":"send-into","name":"cc-nope","payload":"true"}}'
if wait_for '"id":"e2e-gate2-nos"'; then
  R="$(reply_of e2e-gate2-nos)"
  if printf '%s' "$R" | grep -qF 'no_such_session'; then
    ok "目标不存在 → 仍报 no_such_session（Gate 2 没吞掉这一档）"
  else
    bad "目标不存在的应答不对（期望 no_such_session）：$R"
  fi
else bad "目标不存在场景 5s 内无应答"; fi

# ── 场景 N+2：`@ccm_sid_expect` 已设但 `@ccm_sid` 未设 ⇒ **照样拒绝** ───────────
# 这条不在判定表里（表是纯判定，不认识 tmux option 名），但它是本门最容易被放宽的一处：
# 「通道 A 声明了意图」不等于「通道 B 确认了事实」，而破坏性动作只认事实。
"$TMUX_BIN" kill-server 2>/dev/null || true; sleep 0.2
"$TMUX_BIN" new-session -d -s expectonly
"$TMUX_BIN" set-option -t '=expectonly:' @ccm_sid_expect deadbeef >/dev/null 2>&1 || true
send '{"id":"e2e-gate2-exp","cmd":"launch","args":{"mode":"send-into","name":"expectonly","payload":"printf %s CCMGATE_EXPECT"}}'
if wait_for '"id":"e2e-gate2-exp"'; then
  R="$(reply_of e2e-gate2-exp)"
  sleep 0.4
  P="$($TMUX_BIN capture-pane -p -t '=expectonly:' 2>/dev/null || true)"
  if printf '%s' "$R" | grep -qF 'wrong_owner' && ! printf '%s' "$P" | grep -q CCMGATE_EXPECT; then
    ok "只设了 @ccm_sid_expect → 仍拒绝（意图不是事实）"
  else
    bad "@ccm_sid_expect 被当成了 @ccm_sid —— 这道门被放宽了：$R"
  fi
else bad "_expect 场景 5s 内无应答"; fi

# ── F04a：Gate 3（`windows == 1`，**只给破坏性动作**）的真机验收 ───────────
# 与 Gate 2 的用例表分开：Gate 3 的轴是**窗口数**，不是身份，塞进那张表会让两个轴混在一起。
echo
echo "-- F04a Gate 3（kill）--"
g3() { # <场景名> <会话名> <设不设sid> <开几个窗口> <期望码|OK>
  local what="$1" name="$2" sid="$3" wins="$4" want="$5" rid="e2e-g3-$6"
  "$TMUX_BIN" kill-server 2>/dev/null || true; sleep 0.2
  "$TMUX_BIN" new-session -d -s "$name" 2>/dev/null || { bad "$what：建不出会话"; return; }
  [ "$sid" = yes ] && "$TMUX_BIN" set-option -t "=$name:" @ccm_sid abc123 >/dev/null 2>&1
  local i=1; while [ "$i" -lt "$wins" ]; do "$TMUX_BIN" new-window -t "=$name:" >/dev/null 2>&1; i=$((i+1)); done
  send "{\"id\":\"$rid\",\"cmd\":\"kill\",\"args\":{\"name\":\"$name\"}}"
  wait_for "\"id\":\"$rid\"" || { bad "$what：5s 内无应答"; return; }
  local R; R="$(reply_of "$rid")"
  sleep 0.3
  local alive=no; "$TMUX_BIN" has-session -t "=$name:" 2>/dev/null && alive=yes
  if [ "$want" = OK ]; then
    if printf '%s' "$R" | grep -qF '"killed":true' && [ "$alive" = no ]; then ok "$what → 真的杀掉了"
    else bad "$what：期望杀掉，实得 $R（alive=$alive）"; fi
  else
    # 拒绝这一档要同时满足：应答带那个码，**且会话还活着**（只看应答会漏掉「回了错但已经杀了」）
    if printf '%s' "$R" | grep -qF "$want" && [ "$alive" = yes ]; then ok "$what → 拒绝（$want），会话仍存活"
    else bad "$what：期望 $want 且会话存活，实得 $R（alive=$alive）"; fi
  fi
}
g3 "本工具会话 + 单窗口" "g3-owned-cc" no 1 OK 1
g3 "本工具会话 + 2 窗口（Gate 3 挡）" "g3-owned-cc" no 2 too_many_windows 2
g3 "非本工具会话 + 单窗口（Gate 2 就挡住）" "someones-box" no 1 wrong_owner 3
g3 "自定义名 + @ccm_sid + 单窗口" "g3-custom" yes 1 OK 4
g3 "自定义名 + @ccm_sid + 3 窗口（Gate 3 挡）" "g3-custom" yes 3 too_many_windows 5
"$TMUX_BIN" kill-server 2>/dev/null || true; sleep 0.2
send '{"id":"e2e-g3-nos","cmd":"kill","args":{"name":"g3-nope-cc"}}'
if wait_for '"id":"e2e-g3-nos"'; then
  R="$(reply_of e2e-g3-nos)"
  if printf '%s' "$R" | grep -qF 'no_such_session'; then ok "目标不存在 → no_such_session（Gate 3 没吞掉这一档）"
  else bad "目标不存在的应答不对：$R"; fi
else bad "kill 不存在目标：5s 内无应答"; fi
send '{"id":"e2e-g3-bad","cmd":"kill","args":{"name":"a:b"}}'
if wait_for '"id":"e2e-g3-bad"'; then
  R="$(reply_of e2e-g3-bad)"
  if printf '%s' "$R" | grep -qF 'invalid_args'; then ok "名字含 \`:\` → invalid_args（形状门在三道门之前）"
  else bad "形状门没挡住 \`a:b\`：$R"; fi
else bad "kill 形状门：5s 内无应答"; fi

echo
echo "===== 合计 PASS=$pass FAIL=$fail SKIP=$skip ====="
# ⚠ **这里刻意不写数字地板。** 定框 §4：「e2e 各套通过数（CI 两处 + 本地脚本），
#   **同一个数不许两侧各写一份**」—— 本套件初版在这里硬写了 `-ge 28`，而 CI 的
#   `assert-pass-floor.sh daemon-gate2 28` 已经有同一个数。那正是账本记着的那个病
#   （实测两侧都写 6/5 而真值 9/7，两侧都没棘过）。F+ 回看抓到，这里改成**导出式自检**：
#   判定表有几行、就必须尝试过几行。加一行用例不用改这里，而它照样挡得住「静默跳过」。
[ "$skip" -eq 0 ] || { echo "有 $skip 条被跳过 —— 本套件不接受 skip（造不出的名字应当在纯函数轨覆盖并从表里说明）"; exit 1; }
[ "$n" -eq "$ROWS" ] || { echo "判定表 $ROWS 行，只尝试了 $n 行 —— 循环被提前中断了"; exit 1; }
[ "$fail" -eq 0 ]
