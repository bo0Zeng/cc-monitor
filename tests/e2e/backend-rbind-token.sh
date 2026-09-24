#!/usr/bin/env bash
# `设计/80 §8.7` 步 2：**启动期令牌真的坐上了那一帧** —— 真起后端，看帧里有没有 token。
#
# ## 为什么这条必须是真跑（**这一段是死值验量出来的，不是推的**）
#
# 单测那两条（`identity_tag::tests::the_token_is_read_back_out_of_a_real_child_process_environ`
# 与 `observe::watcher::tests::the_launch_token_rides_the_session_added_frame_only_when_the_client_asked`）
# 断的是**函数与函数之间**接上了。它们**看不见**这一段：
#
#   argv 上那条 `--with-rbind-token` → `split_stream_flags` → `is_query_mode`
#     → `watcher::spawn` → `watch_loop` → `ReaderState.with_rbind_token` → 帧上的字段
#     → `serde` 吐出的**那一行字节**
#
# 🔴 **它买到的那一格，现打出来了**〔09-23 死值验刀 D〕：把 `main.rs` 里那条
# `observe::watcher::spawn(agent_home, with_bg, tail_only, with_rbind_token)`
# 的最后一个实参改成死 `false`（= 接线断掉，CLI 上那条 flag 从此不起作用），
# **后端单测 797 条全绿**，而本套件 `[1]` 当场红。
# 那一格是单测**结构上**够不到的：夹具直接给 `ReaderState` 置字段，压根不经 `main.rs`。
#
# ⚠ **它买不到的那一格，也现打出来了**〔同日死值验刀 A，**没红**，如实登记〕：
# 把 `--with-rbind-token` 从 `STREAM_FLAGS` 里摘掉（= 不再剥离它）之后，
# **本套件照样全绿**。原因是那一位由 `args.iter().any(…)` 独立算出，**不经** `STREAM_FLAGS`；
# 而今天的 `is_query_mode` 对未知 `--flag` 是「忽略 + 一行 warn + 照常进流模式」
# ⇒ 在**今天这个**后端上，不剥离一条孤立的 flag 不产生任何可观测差异。
# 「不剥离」真正会咬的是**已部署的老后端**（那条 §26 死循环），
# 那一格由单测 `main_stream_flag_tests::every_capability_token_is_strippable` 接着
# （刀 A 在它那里**当场红**）。⇒ **别把本套件读成「§26 也验过了」。**
#
# ## 四组对照（缺任何一组读数都不可信）
#
# | 组 | argv 有 flag 吗 | 进程环境里有令牌吗 | 期望 |
# |---|---|---|---|
# | asked   | ✅ | ✅ 32 hex      | 帧上**有**那个令牌（正题） |
# | unasked | ❌ | ✅ 32 hex      | 帧上**没有**（令牌默认不上 wire，`§8.6 ③`） |
# | bare    | ✅ | ❌ 压根没设     | 帧上**没有** = 「这条会话真的没有令牌」（`§8.5 ②` 那个布尔） |
# | malformed | ✅ | ✅ 但形状不对 | 帧上**没有**（fail closed，不是把它原样报出去） |
#
# ★ **每一组都先过量具自检**：那一趟必须真的收到了 `session_added`。
#   不自检的话，后三组的「没有令牌」会与「压根没收到帧」不可分 ——
#   本仓 08-13 在 `backend-sessions-rewatch` 上栽过一次这个形状
#  （pidfile 文件名不对，三组全空，差点把「修好了」读成「没修」）。
#
# ## 本机安全
#
# 全程**不碰 tmux**（本套件不建任何会话、不打任何 `@ccm_sid`），`claude_dir` 一律在
# `/tmp` 下的临时目录，假会话的 pid 用一个自己起的 `sleep`（**不是真 claude**，`C7`）。
#
# ## ⚠ 它**买不到**什么（如实登记）
#
# **端到端那一维买不到。** `§8.7` 明写「1 与 2 之间有顺序依赖（没有 token 进环境，
# 后端读不到）」—— 把令牌注进启动命令（步 1）与本地半用它绑 HWND（步 3）都还没做。
# 本套件自己用 `env CCM_RBIND_TOKEN=…` 造出那个进程，**不依赖另一路**；
# 它证的是「**后端这一侧报得出**」，不是「↗ 已经不依赖 tmux 了」。
set -o pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
D="${CCM_E2E_BACKEND_BIN:-$REPO/.build/backend/debug/cc-monitor-backend}"
[ -x "$D" ] || { echo "需要后端二进制：$D（先 cargo build -p cc-monitor-backend）"; exit 1; }

W="$(mktemp -d /tmp/e2e-rbind-token.XXXXXX)"
cleanup() { rm -rf -- "$W"; }
trap cleanup EXIT

# 形状合法的令牌：恰好 32 个小写十六进制字符。值本身无意义。
TOK="0123456789abcdef0123456789abcdef"

fail=0
pass=0
chk() { if [ "$2" = "$3" ]; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1: 期望[$3] 实得[$2]"; fail=$((fail+1)); fi; }

# $1=标签  $2=asked|unasked  $3=令牌的值（空串 = 压根不设那个变量）
#
# 起一个 `sleep` 当假会话进程（可选地给它注 `CCM_RBIND_TOKEN`）、配一份合成 pidfile、
# 起后端、把帧收到文件里。把那一趟的帧文件路径留在 $OUT 供调用方 grep。
probe() {
  local label="$1" mode="$2" tokval="$3"
  local F="$W/$label"
  OUT="$W/$label.frames"
  rm -rf -- "$F"; mkdir -p "$F/projects/p" "$F/sessions"

  # ★ 假会话进程。`env -u` 先摘掉：跑这套的 shell 自己碰巧带着这个变量时
  #   （步 1 落地之后开发机上完全可能），`bare` 那一组会继承到它、当场变成假绿。
  if [ -n "$tokval" ]; then
    env -u CCM_RBIND_TOKEN CCM_RBIND_TOKEN="$tokval" sleep 60 & local vpid=$!
  else
    env -u CCM_RBIND_TOKEN sleep 60 & local vpid=$!
  fi
  local ticks; ticks=$(awk '{print $22}' "/proc/$vpid/stat")

  local flags=()
  [ "$mode" = asked ] && flags=(--with-rbind-token)

  CLAUDE_CONFIG_DIR="$F" timeout 20 "$D" "${flags[@]}" < /dev/null > "$OUT" 2> "$W/$label.err" &
  local dpid=$!
  sleep 2.5
  # procStart 逐位相等 ⇒ F20 的主证据放行（与「是不是真 claude」无关，cmdline 不参与）。
  printf '{"pid":%d,"sessionId":"sid-%s","cwd":"/tmp","kind":"interactive","procStart":"%s"}\n' \
    "$vpid" "$label" "$ticks" > "$F/sessions/$vpid.json"
  printf '{"type":"user","sessionId":"sid-%s"}\n' "$label" > "$F/projects/p/sid-$label.jsonl"
  sleep 4
  kill "$dpid" "$vpid" 2>/dev/null
  wait "$dpid" 2>/dev/null
}

# 那一趟到底收到了几帧 `session_added`（量具自检用）。
added_count() { grep -c '"kind":"session_added"' "$1" 2>/dev/null || true; }
# 那一趟的 `session_added` 里带着的令牌（没带就是空串）。
token_in_frame() {
  grep '"kind":"session_added"' "$1" 2>/dev/null \
    | sed -n 's/.*"rbind_token":"\([^"]*\)".*/\1/p' | head -1
}

echo "[0] 握手帧要声明 rbind-token 能力（老后端不声明 ⇒ monitor 诚实降级回标题路）"
probe caps asked "$TOK"
chk "hello 的 capabilities 里有 rbind-token" \
  "$(grep -c '"kind":"hello".*"rbind-token"' "$OUT" 2>/dev/null || true)" "1"

echo "[1] 正题：索要了 + 环境里有 ⇒ 帧上带着那个令牌"
probe asked asked "$TOK"
chk "asked 量具自检：真的收到了 session_added" "$(added_count "$OUT")" "1"
chk "asked 帧上的令牌 == 注进去的那个"          "$(token_in_frame "$OUT")" "$TOK"

echo "[2] 阴性一（**默认路**）：没索要 ⇒ 令牌不上 wire（§8.6 ③ 敏感数据）"
# ★ 这一组是「默认关」那条承诺的唯一判据：把闸门删成无条件读，只有它红。
probe unasked unasked "$TOK"
chk "unasked 量具自检：真的收到了 session_added" "$(added_count "$OUT")" "1"
chk "unasked 帧上没有 rbind_token 字段" \
  "$(grep -c '"rbind_token"' "$OUT" 2>/dev/null || true)" "0"

echo "[3] 阴性二（**归因那一格**）：索要了但这条会话压根没令牌 ⇒ 缺席（§8.5 ② 那个布尔）"
probe bare asked ""
chk "bare 量具自检：真的收到了 session_added" "$(added_count "$OUT")" "1"
chk "bare 帧上没有 rbind_token 字段" \
  "$(grep -c '"rbind_token"' "$OUT" 2>/dev/null || true)" "0"

echo "[4] 阴性三：形状不对 ⇒ fail closed（不是把它原样报出去）"
probe malformed asked "NOT-A-TOKEN"
chk "malformed 量具自检：真的收到了 session_added" "$(added_count "$OUT")" "1"
chk "malformed 帧上没有 rbind_token 字段" \
  "$(grep -c '"rbind_token"' "$OUT" 2>/dev/null || true)" "0"
# ★ 顺带断一件：那个值**没有**漏进 stderr 的日志里（`§8.6 ③`）。
#   后端在这一格会打一行 warn 说「形状过不了」，它只许印长度。
chk "malformed 的值没有漏进后端日志" \
  "$(grep -c 'NOT-A-TOKEN' "$W/malformed.err" 2>/dev/null || true)" "0"
chk "asked 的令牌也没有漏进后端日志" \
  "$(grep -c "$TOK" "$W/asked.err" 2>/dev/null || true)" "0"

echo
echo "backend-rbind-token: $pass passed, $fail failed"
[ "$fail" -eq 0 ] || exit 1
