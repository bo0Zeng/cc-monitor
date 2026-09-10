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
# `C7i` 隔离：走**共享原语**（`P0e` 08-12）。shim 强插 `-L e2eGate2`，漏什么环境变量都打不偏。
# ⚠ 此前靠 `TMUX_TMPDIR`，那是 `C7i` 逐字禁止的形态（08-11 同形态探针打没了用户 9 个真实会话）。
TMUX_SHIM_SOCK=e2eGate2
# shellcheck source=e2e/tmux-shim.sh
. "$(cd "$(dirname "$0")" && pwd)/tmux-shim.sh"
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
  # C7i：socket 显式给死（见 local-backend-supervise.sh 的同款注释）
  "$TMUX_BIN" -L "$TMUX_SHIM_SOCK" kill-server 2>/dev/null
  rm -rf -- "$WORK"
}
trap cleanup EXIT

pass=0; fail=0; skip=0; waived=0

# ★★ **造不出的名字：登记豁免**〔`P0e` 08-13，执行的是本套件自己开的方子〕。
#
# 收尾原本逐字写着「本套件不接受 skip（**造不出的名字应当在纯函数轨覆盖并从表里说明**）」——
# 而那句话一直没人执行：本机（tmux 3.6）实测把 `cc-a:b` 建成 `cc-a_b`
# （当场用私有 socket 验过：`tmux -L p0eProbe new-session -s 'cc-a:b'` → 会话名是 `cc-a_b`）
# ⇒ e2e 这一轨**物理上造不出这个名字**，于是每跑必 skip、必 RC=1。
#
# ⚠ 豁免**不是**「这条不验了」：判定表被**三方**独立读（monitor Rust / daemon Rust / 本脚本），
#   前两轨照常验它 —— 那两轨不依赖 tmux 怎么给会话命名。本轨欠的只是「真会话」这一层。
# ⚠ 只豁免**登记在册**的：没登记的 skip 仍然让整套 RC=1（原纪律一个字没松）。
#
# 🔴🔴 **在这里加/删一条 = 同一拍要改 CI 的地板**〔`CI-J3` 09-09〕。
#   豁免走的是下面 `skipped()` 里 `waived=$((waived+1))` 那一支 —— **既不进 `pass` 也不进 `skip`**
#   ⇒ 每登记一条，本套件的**可达 PASS 上限就少一格**。08-13 加第一条（`meta_colon`）时
#   没人动 CI 那个数，于是那条地板从此**够不到**：它不会以「地板红」的形式被看见，
#   只会以「排在它后面的每一步整片 skipped」的形式被看见 —— 一直挡到 09-09 才有人发现。
#   ⇒ 现在这条耦合有判据看着了：`remote-daemon-proto/src/control/gate.rs` 的
#   `the_gate2_floor_still_makes_a_skip_hurt` **现数**本函数里的登记条数，
#   要求 CI 地板 == 判定表行数 + 表外固定场景数 − 登记条数，**低了高了各红一件不同的事**。
#   ⇒ 你在这里加一条，那条判据会当场告诉你地板该改成几；**别绕过它去改数**。
#
# 🔴🔴 **第二条登记（`meta_dollar`，`CI-J3` 09-09）与它带来的一格诚实边界：豁免是「环境相关」的。**
#
#   `meta_colon` 那条在**今天所有** tmux 上都成立（`:`→`_` 至今未变，本机 3.6 实测）。
#   `meta_dollar` 那条**不是** —— 它只在 **tmux ≤3.4** 上成立（机制与出处写在它自己的理由里）。
#   ⇒ 那台 runner 哪天升到 ≥3.5，这一格会**真的建出会话、真的跑、真的过**：
#     `PASS` 从 34 变 35、`WAIVED` 从 2 变 1，而**没有任何东西会红**
#     （地板判法是 `at-least`，只挡缩水；`gate.rs` 那条算的 `reachable` 仍按「登记了 2 条」= 34）。
#     ⇒ **一条已经不需要的豁免会静悄悄留着。**
#
#   ⚠ **为什么这里不装「豁免没被用到就红」**（想过，是错的）：开发机今天就是 **3.6**，
#     那条判据会让本套件**在更新的 tmux 上恒红** —— 用「你的 tmux 太新了」去挡人，
#     比这条豁免多留一天坏得多。而「**所有**环境都不再需要它了吗」这一问，
#     **单次运行在原理上答不出来**（它只看得见自己这一台）。⇒ 如实登记，不假装治了。
#
#   ⇒ 今天的处置是三条，都不靠人的记忆：
#     ① 本轮跑在哪个 tmux 上、用掉了几条豁免，**收尾那行打出来**（见文件末尾）；
#     ② 真要删这条豁免时**不用记得改地板**：删掉这条臂 ⇒ 登记数 2→1 ⇒ `reachable` 34→35
#        ⇒ `gate.rs` 的 `floor >= reachable` **当场红**，诊断直接说该棘到几；
#     ③ 「PASS 涨了而地板没跟」这个一般形态**不是本件新开的洞**，它有主：`K-G8`/`K-G3` 的
#        `exact` 判法（今天只在 `scripts/gate.sh` 那 4 条上生效，CI 这 23 条仍是 `at-least`，
#        理由逐字在 `.github/workflows/ci.yml` 那段 `K-G8` 里：谁在跑那把尺子，谁才配换判法）。
#     **解锁条件一句话**：CI 的 runner 上 `tmux -V` ≥ 3.5 之后，删掉 `meta_dollar` 这条臂，
#     让 ② 那条判据把地板逼到 35。
waiver_reason() {
  case "$1" in
    meta_colon) echo "tmux 会把名字里的 ':' 换成 '_'（本机 3.6 实测 cc-a:b → cc-a_b）⇒ 这一轨造不出真会话；判定由 monitor/daemon 两条纯函数轨覆盖（同一张 TSV）" ;;
    meta_dollar) echo "tmux <=3.4 会在会话名的 \$ 前插一个反斜杠（session_check_name 过 utf8_stravis；3.4 的 utf8.c 那条 \$ 规则**不看任何 flag**：\$ 后面跟字母 / _ / { 就插）⇒ cc-a\$x 存进 server 的真名是「cc-a 反斜杠 \$x」，'=cc-a\$x:' 当然找不到 ⇒ 这一轨在 <=3.4 上造不出真会话。上游 692ce59bcef5（2024-05-24）给它加了 VIS_DQ 门、首次随 3.5 出货；CI 的 ubuntu-24.04 装的是 3.4（runner 日志逐字 tmux 3.4-1ubuntu0.1），开发机 3.6 ⇒ 本地物理上看不见这个病。判定由 monitor/daemon 两条纯函数轨覆盖（同一张 TSV）" ;;
    *) echo "" ;;
  esac
}

ok()   { printf '  PASS %s\n' "$1"; pass=$((pass+1)); }
bad()  { printf '  FAIL %s\n' "$1"; fail=$((fail+1)); }
skipped() {
  local id="${1%%：*}"
  local why; why="$(waiver_reason "$id")"
  if [ -n "$why" ]; then
    printf '  waive %s\n        理由：%s\n' "$1" "$why"
    waived=$((waived+1))
  else
    printf '  skip %s\n' "$1"
    skip=$((skip+1))
  fi
}

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

  "$TMUX_BIN" -L "$TMUX_SHIM_SOCK" kill-server 2>/dev/null || true
  sleep 0.2
  # `--` 让 `-cc` 这种以短横开头的名字不被 tmux 的 getopt 当成选项。
  "$TMUX_BIN" new-session -d -s "$name" 2>/dev/null \
    || "$TMUX_BIN" new-session -d -s -- "$name" 2>/dev/null \
    || { skipped "$id：tmux 建不出这个名字的会话（$name）"; continue; }
  # ★ audit-0805 F20：**建完要验它真的以那个名字存在**。
  #
  # 起因：CI 上 `meta_dollar`（`cc-a$x`）报的是 daemon 的 `no_such_session`，而不是本例期望的
  # `wrong_owner` —— 也就是说**会话没按那个名字建起来**，而上面那条 `||` 链**返回了 0**
  # （某一步「成功」了，只是建出来的东西不叫这个名字）。于是用例继续往下跑，
  # 最后给出一个**指向错误方向**的失败：看起来像「Gate 2 判错了」，实际是「夹具没准备好」。
  #
  # ⇒ 这里用 `=name:` 精确匹配复核一次（同 daemon 侧 `launch::exact_target` 的形状）。
  # 不存在就**诚实 skip 并把 tmux 实况打出来**，而不是让下游去猜。
  # ⚠ 本机 tmux 3.6 上这条恒真（所以本地看不到差别）；它是给**别的 tmux 版本**准备的。
  if ! "$TMUX_BIN" has-session -t "=$name:" 2>/dev/null; then
    skipped "$id：建完之后 tmux 里找不到 \"$name\"（这台 tmux 对这个名字的处理与本机不同）。\
实际会话：[$("$TMUX_BIN" ls -F '#{session_name}' 2>/dev/null | tr '\n' ' ')]"
    continue
  fi
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
"$TMUX_BIN" -L "$TMUX_SHIM_SOCK" kill-server 2>/dev/null || true; sleep 0.2
send '{"id":"e2e-gate2-nos","cmd":"launch","args":{"mode":"send-into","name":"cc-nope","payload":"true"}}'
if wait_for '"id":"e2e-gate2-nos"'; then
  R="$(reply_of e2e-gate2-nos)"
  if printf '%s' "$R" | grep -qF 'no_such_session'; then
    ok "目标不存在 → 仍报 no_such_session（Gate 2 没吞掉这一档）"
  else
    bad "目标不存在的应答不对（期望 no_such_session）：$R"
  fi
else bad "目标不存在场景 5s 内无应答"; fi

# ── 场景 N+1b（F04c）：**新 mode `send-keys-raw` 也过同一道 Gate 2** ────────────
# 一个新 mode 绕过身份门，是「加功能顺手开个后门」最典型的形状：功能测试全绿，
# 而「往别人的 tmux 会话里打字」这道门只对旧 mode 生效。
# 拒绝这一档同样要**两件事都满足**：应答说 false，**且 pane 真的没被污染**。
"$TMUX_BIN" -L "$TMUX_SHIM_SOCK" kill-server 2>/dev/null || true; sleep 0.2
"$TMUX_BIN" new-session -d -s notours   # 不是本工具的命名形状，且不设 @ccm_sid
send '{"id":"e2e-gate2-raw","cmd":"launch","args":{"mode":"send-keys-raw","name":"notours","payload":"printf %s CCMGATE_RAW"}}'
if wait_for '"id":"e2e-gate2-raw"'; then
  R="$(reply_of e2e-gate2-raw)"
  sleep 0.4
  P="$($TMUX_BIN capture-pane -p -t '=notours:' 2>/dev/null || true)"
  if printf '%s' "$R" | grep -qF '"ok":false' && printf '%s' "$R" | grep -qF 'wrong_owner'; then
    if printf '%s' "$P" | grep -q CCMGATE_RAW; then
      bad "send-keys-raw 回了 wrong_owner，**但键已经打进去了** —— 门在动作之后"
    else
      ok "send-keys-raw 也过 Gate 2（非本工具会话 → wrong_owner，pane 未被污染）"
    fi
  else
    bad "send-keys-raw 没被 Gate 2 拒（期望 wrong_owner）：$R"
  fi
else bad "send-keys-raw 的 Gate 2 场景 5s 内无应答"; fi

# ── 场景 N+2：`@ccm_sid_expect` 已设但 `@ccm_sid` 未设 ⇒ **照样拒绝** ───────────
# 这条不在判定表里（表是纯判定，不认识 tmux option 名），但它是本门最容易被放宽的一处：
# 「通道 A 声明了意图」不等于「通道 B 确认了事实」，而破坏性动作只认事实。
"$TMUX_BIN" -L "$TMUX_SHIM_SOCK" kill-server 2>/dev/null || true; sleep 0.2
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
  "$TMUX_BIN" -L "$TMUX_SHIM_SOCK" kill-server 2>/dev/null || true; sleep 0.2
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
"$TMUX_BIN" -L "$TMUX_SHIM_SOCK" kill-server 2>/dev/null || true; sleep 0.2
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
echo "===== 合计 PASS=$pass FAIL=$fail SKIP=$skip WAIVED=$waived ====="
# ⚠ **豁免是环境相关的，把环境打出来**〔`CI-J3` 09-09，见 `waiver_reason` 头上那段〕。
#   `meta_dollar` 那条只在 tmux ≤3.4 上会走到；换一台 ≥3.5 的机器它会真的过 ⇒
#   `WAIVED` 少一条、`PASS` 多一格，而**地板判法是 at-least，不会因此红**。
#   ⇒ 这一行是那件事**唯一**会出声的地方：用掉的条数少于登记条数时，说明这台机器
#   已经不需要那几条了 —— 删掉 `waiver_reason` 里对应的臂，`gate.rs` 那条判据会把地板逼上去。
#   ⚠ 刻意**不在这里**判「用掉 < 登记 ⇒ 红」：开发机今天是 3.6，那会让本套件在更新的
#     tmux 上恒红；而「所有环境都不再需要它了吗」单次运行原理上答不出来。
echo "（本轮 tmux：$("$TMUX_BIN" -V 2>/dev/null || echo 未知)；用掉登记豁免 $waived 条）"
# ⚠ **这里刻意不写数字地板。** 定框 §4：「e2e 各套通过数（CI 两处 + 本地脚本），
#   **同一个数不许两侧各写一份**」—— 本套件初版在这里硬写了 `-ge 28`，而 CI 的
#   `assert-pass-floor.sh daemon-gate2 28` 已经有同一个数。那正是账本记着的那个病
#   （实测两侧都写 6/5 而真值 9/7，两侧都没棘过）。F+ 回看抓到，这里改成**导出式自检**：
#   判定表有几行、就必须尝试过几行。加一行用例不用改这里，而它照样挡得住「静默跳过」。
[ "$skip" -eq 0 ] || { echo "有 $skip 条被跳过 —— 本套件不接受**未登记**的 skip（造不出的名字要进 waiver_reason 并写明纯函数轨怎么覆盖它）"; exit 1; }
[ "$n" -eq "$ROWS" ] || { echo "判定表 $ROWS 行，只尝试了 $n 行 —— 循环被提前中断了"; exit 1; }
[ "$fail" -eq 0 ]
