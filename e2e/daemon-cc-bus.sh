#!/usr/bin/env bash
# `P4f`：daemon 的 cc-bus 基础命令 —— `--bus-list` / `--bus-send` **真跑**。
#
# ## 为什么必须有真跑这一层
#
# 单测把 `parse_list` / `classify_send` 那些纯函数钉住了，但它们**证明不了命令调得动**：
# 08-13 实测撞到过一次 —— 两条命令进了 `inbound::REGISTRY`、`hello.commands` 也报了它们、
# 分派臂也认（`cli_control::handles` 是派生的），而 `main::is_query_mode` 这道**闸门**
# 读的是手写的 `SUBCOMMANDS`。漏加两行的后果是**静默的**：daemon 打一行
# 「未知 flag，已忽略」的 warn 之后**照常进流模式**，调用方拿到一堆 jsonl 行。
# 单测全绿。⇒ 这套件跑的是**真二进制的真 argv**。
#
# ## 本机安全
#
# · `CLAUDE_CONFIG_DIR` 指向 mktemp 沙箱 —— **不指的话 daemon 会去流式读你真实的
#   `~/.claude`**（只读，但会把你的转录刷一屏，08-13 我就这么干了一次）；
# · `CC_BUS_HOME` 同样在沙箱里，绝不碰真实 `~/.cc-bus/`；
# · `CC_BUS_BIN_DIR` 指向**仓内**的 cc-bus 脚本（不是已装的那份）——
#   验的是仓里这一版，与 `exec-bit-guard` 的口径一致；
# · 用到 tmux 的只有 `[10]`（身份空间对账），且**一律经 PATH 上的 shim 强制 `-L <隔离socket>`**
#   （`C7i`：daemon 内部是裸调 `tmux`，塞不进 `-L`，只能这样拦）；其余各格不碰 tmux；
# · 起的进程只有 daemon 自己（一次性 exec，`</dev/null` + `timeout`）。
set -o pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
D="$REPO/remote-daemon-proto/target/debug/cc-monitor-remote"
[ -x "$D" ] || { echo "需要先 build daemon：cd remote-daemon-proto && cargo build"; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "需要 jq"; exit 1; }
TIMEOUT="$(command -v timeout)" || { echo "需要 timeout"; exit 1; }
REALTMUX="$(command -v tmux)" || { echo "需要 tmux（[10] 的身份空间对账要它）"; exit 1; }

SANDBOX="$(mktemp -d)"
trap 'rm -rf "$SANDBOX"' EXIT
BUS="$SANDBOX/bus"; CLA="$SANDBOX/claude"; EMPTY="$SANDBOX/empty"; NOHOME="$SANDBOX/nohome"
mkdir -p "$BUS"/{inbox,state,log,queue} "$CLA/projects" "$EMPTY" "$NOHOME"

SCRIPTS="$REPO/shared/cc-bus/scripts"
[ -x "$SCRIPTS/cc-list" ] || { echo "仓内没有 cc-list：$SCRIPTS"; exit 1; }

pass=0; fail=0
chk() { if [ "$2" = "$3" ]; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1: 期望[$3] 实得[$2]"; fail=$((fail+1)); fi; }

# 一次调用 = 一个干净的 env（**逐次显式给全**，免得上一格的变量漏进来）
d() {
  env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" \
      CCBUS_POLICY_MODE="${POLICY:-off}" CC_BUS_ID=probe_cc \
      "$TIMEOUT" 20 "$D" "$@" 2>"$SANDBOX/err.txt"
}

echo "===== P4f：daemon 的 cc-bus 基础命令 ====="

echo "[1] --bus-list：空总线"
out="$(d --bus-list </dev/null)"
chk "空总线回空数组（不是报错）" "$(printf '%s' "$out" | jq -c '.agents')" "[]"

echo "[2] --bus-list：读得出 id / 地址 / 待读数"
# 台架**直接写** agents.tsv：它扮演的是"cc-bus 的状态"。
# ⚠ 被测的 daemon **不许**读这个格式（`no_cc_bus_data_layout_leaks_into_the_daemon` 钉着），
#   台架知道它是可以的 —— 格式变了这套件会红，那正是我们要的信号。
printf 'alpha_cc\talpha_cc:0.0\t2026-08-13T00:00:00-07:00\t111\n' > "$BUS/agents.tsv"
printf 'a\nb\n' > "$BUS/inbox/alpha_cc.jsonl"
echo 0 > "$BUS/state/alpha_cc.pos"
out="$(d --bus-list </dev/null)"
chk "id" "$(printf '%s' "$out" | jq -r '.agents[0].id')" "alpha_cc"
chk "tmux 地址" "$(printf '%s' "$out" | jq -r '.agents[0].target')" "alpha_cc:0.0"
chk "待读数（真数出来的，不是猜的）" "$(printf '%s' "$out" | jq -r '.agents[0].unread')" "2"

echo "[3] --bus-send：真的送到了收件人的收件箱"
before="$(wc -l < "$BUS/inbox/alpha_cc.jsonl")"
out="$(printf '{"to":"alpha_cc","text":"来自 daemon 的一条"}' | d --bus-send)"
chk "回 sent=true" "$(printf '%s' "$out" | jq -r '.sent')" "true"
chk "回显收件人" "$(printf '%s' "$out" | jq -r '.to')" "alpha_cc"
chk "★ 收件箱真的多了一行" "$(( $(wc -l < "$BUS/inbox/alpha_cc.jsonl") - before ))" "1"
chk "  正文一字不差" \
  "$(tail -1 "$BUS/inbox/alpha_cc.jsonl" | jq -r .text)" "来自 daemon 的一条"

echo "[4] ★ 收件人非法：daemon **不预判**，由 cc-bus 自己拒（白名单只有一份）"
printf '{"to":"a/b","text":"x"}' | d --bus-send >"$SANDBOX/o4.txt"; rc4=$?
chk "退出码非 0" "$([ "$rc4" -ne 0 ] && echo yes || echo no)" "yes"
chk "码是 invalid_args" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "invalid_args"
chk "  消息里带着 cc-send 自己那句话（证明是它拒的，不是我们）" \
  "$(jq -r .message < "$SANDBOX/err.txt" | grep -c '非法收件人')" "1"

echo "[5] ★ 被路由层拦下：与「名字写错了」必须分得开"
printf 'probe_cc\tnobody\n' > "$BUS/policy.tsv"
POLICY=on
printf '{"to":"alpha_cc","text":"x"}' | d --bus-send >/dev/null; rc5=$?
POLICY=off
chk "退出码非 0" "$([ "$rc5" -ne 0 ] && echo yes || echo no)" "yes"
chk "★ 码是 rejected（不是 invalid_args）" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "rejected"
rm -f "$BUS/policy.tsv"

echo "[6] ★ 没装 cc-bus：说得出查过哪儿"
env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$EMPTY" HOME="$NOHOME" PATH="$EMPTY" \
    "$TIMEOUT" 20 "$D" --bus-list </dev/null >/dev/null 2>"$SANDBOX/err6.txt"
chk "码是 not_installed（不是笼统的 failed）" "$(jq -r .code < "$SANDBOX/err6.txt" 2>/dev/null)" "not_installed"
msg="$(jq -r .message < "$SANDBOX/err6.txt" 2>/dev/null)"
chk "  列出了 ~/.local/bin 那一处" "$(printf '%s' "$msg" | grep -c '.local/bin/cc-list')" "1"
chk "  列出了 skills 那一处" "$(printf '%s' "$msg" | grep -c 'skills/cc-bus/scripts/cc-list')" "1"
chk "  告诉人怎么指过去" "$(printf '%s' "$msg" | grep -c 'CC_BUS_BIN_DIR')" "1"

echo "[7] ★ 两个入口都有：能力探测口报得出这两条"
probe="$(d --daemon-probe </dev/null)"
# ⚠ 探测口报的是 **CLI flag 形式**（`--bus-list`），不是裸命令名 —— 我第一版按裸名断言，
#   当场红。写判据前先看一眼真实输出，别照着脑子里的形状写。
chk "探测口的 commands 里有 --bus-list" \
  "$(printf '%s' "$probe" | jq -r '.commands | index("--bus-list") != null')" "true"
chk "探测口的 commands 里有 --bus-send" \
  "$(printf '%s' "$probe" | jq -r '.commands | index("--bus-send") != null')" "true"
# ★ 这一格钉的是那条**静默失效**：命令"被报出来"不等于"调得动"。
#   闸门（`main::SUBCOMMANDS`）漏加时，上面两格照样绿，而下面这格会红。
out="$(d --bus-list </dev/null)"
chk "★ 报得出**且**调得动（漏了闸门这格会红）" \
  "$(printf '%s' "$out" | jq -e 'has("agents")' >/dev/null 2>&1 && echo yes || echo no)" "yes"

echo "[8] ★ cc-bus 命令卡住时，daemon 不许陪着一起卡"
# ★ 真事故：`Command::output()` **无限等**。把 cc-send 换成 sleep 300 的桩，
#   --bus-send 25 秒没回来（25 是从外面掐的，daemon 自己没有期限）。
#   而这两条是阻塞档，一条卡住占死一个 tokio worker，且 cancel 对 spawn_blocking 是空操作。
# ⚠ 修法**不是**在 daemon 里加计时器（零定时器铁律 + 协议逐字「超时一律推给客户端」）——
#   而是让**子进程自己**有期限（timeout 前缀，同 ccm 问 daemon 那条）。
mkdir -p "$SANDBOX/hangbin"
printf '#!/bin/bash\nsleep 300\n' > "$SANDBOX/hangbin/cc-send"
chmod +x "$SANDBOX/hangbin/cc-send"
cp "$SCRIPTS/cc-list" "$SANDBOX/hangbin/cc-list"
_t0=$(date +%s)
printf '{"to":"x_cc","text":"hi"}' | env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" \
    CC_BUS_BIN_DIR="$SANDBOX/hangbin" CC_BUS_TIMEOUT_SECS=2 \
    "$TIMEOUT" 30 "$D" --bus-send >/dev/null 2>"$SANDBOX/err8.txt"
_el=$(( $(date +%s) - _t0 ))
chk "★ 2 秒的期限：真的在 5 秒内回来了（不是等到我们从外面掐）" \
  "$([ "$_el" -le 5 ] && echo yes || echo "no（用了 ${_el}s）")" "yes"
chk "  码是 timed_out（不是笼统的 failed）" "$(jq -r .code < "$SANDBOX/err8.txt" 2>/dev/null)" "timed_out"
chk "  消息说得出去哪儿看（flock / *.lock）" \
  "$(jq -r .message < "$SANDBOX/err8.txt" 2>/dev/null | grep -c 'flock')" "1"

echo "[9] ★ 声明「不收输入」的命令，stdin 不关时必须秒回"
# ★ 真事故：CLI 入口原来从 `fields` **派生**「要不要读 stdin」，而 `fields` 是
#   「args 和 data 的字段名」。`bus-list` 无输入却有输出字段 ⇒ 被判成要读 stdin
#   ⇒ **挂住等一个永远不来的输入**（实测 --ping 120ms 回、--bus-list 6 秒被掐死才停）。
# ⚠ 守它的那条单测**是恒真的**（两个分支各是同一个表达式的复述），一声没吭。
#   ⇒ 这一格钉**行为**：真起进程 + 一条不关的管道。声明真不真，由它说了算。
for _c in --ping --bus-list; do
  _t0=$(date +%s%N)
  _o="$(env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" \
        "$TIMEOUT" 6 "$D" "$_c" < <(sleep 30) 2>/dev/null)"
  _ms=$(( ($(date +%s%N) - _t0) / 1000000 ))
  chk "★ $_c：stdin 不关也返回了（不是挂到被掐）" \
    "$([ -n "$_o" ] && [ "$_ms" -lt 5000 ] && echo yes || echo "no（${_ms}ms，输出 ${_o:-<空>}）")" "yes"
done

echo "[10] ★ 总线成员是**身份空间的子集**，不是第二套名单"
# 〔用@08-13〕逐字：「那他不应该是身份空间的子集吗? 他应该去调用身份空间啊」。
# cc-bus 的 agents.tsv 记的地址**会过期**（会话名被重用是常态）——08-13 实测后果是
# 敲门文字打进**陌生占用者**的屏幕。⇒ live/ccm_sid 由 daemon 去问 tmux，agents.tsv
# 只回答「谁登记过 + 还剩几条没读」。
# ⚠ live 是**三态**：true / false / null。判据三格全钉——只钉前两格的话，
#   「问不到就当成不在」这种最坏的读法会溜过去（把一屋子活人判成死人）。
_SOCK="ccbusid$$"
_SHIM="$SANDBOX/shim"; mkdir -p "$_SHIM"
printf '#!/bin/bash\nexec %s -L %s "$@"\n' "$REALTMUX" "$_SOCK" > "$_SHIM/tmux"
chmod +x "$_SHIM/tmux"
PATH="$_SHIM:$PATH" tmux new-session -d -s alive_cc -c /tmp 'cat'
sleep 0.4
PATH="$_SHIM:$PATH" tmux set-option -t alive_cc @ccm_sid 'sid-1234' >/dev/null 2>&1
new_bus_state() {
  printf 'alive_cc\talive_cc:0.0\tts\t1\ngone_cc\tgone_cc:0.0\tts\t2\n' > "$BUS/agents.tsv"
  : > "$BUS/inbox/alive_cc.jsonl"; : > "$BUS/inbox/gone_cc.jsonl"
}
new_bus_state
_j="$(PATH="$_SHIM:$PATH" env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" \
      "$TIMEOUT" 20 "$D" --bus-list </dev/null 2>/dev/null)"
chk "★ 活着的成员 live=true" "$(printf '%s' "$_j" | jq -r '.agents[] | select(.id=="alive_cc") | .live')" "true"
chk "  且挂到了真身份上（@ccm_sid）" "$(printf '%s' "$_j" | jq -r '.agents[] | select(.id=="alive_cc") | .ccm_sid')" "sid-1234"
chk "★ 名单里有、会话已经没了的 live=false" "$(printf '%s' "$_j" | jq -r '.agents[] | select(.id=="gone_cc") | .live')" "false"

# 第三态：**问不到身份空间**。造一个有 coreutils、只缺 tmux 的 PATH
#（`PATH` 清空是不行的 —— 那样 cc-list 自己的 awk 也没了，测的就不是这件事了）。
_NT="$SANDBOX/notmux"; mkdir -p "$_NT"
for _t in bash sh env awk cat wc date tr sed grep head tail mkdir touch flock mv rm sort cut timeout; do
  [ -x "/usr/bin/$_t" ] && ln -sf "/usr/bin/$_t" "$_NT/$_t"
done
chk "  台架自检：这个 PATH 里确实没有 tmux" \
  "$(PATH="$_NT" command -v tmux >/dev/null 2>&1 && echo 有 || echo 没有)" "没有"
_j2="$(env PATH="$_NT" CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" \
       "$TIMEOUT" 20 "$D" --bus-list </dev/null 2>/dev/null)"
chk "★ 问不到身份空间 ⇒ live 是 **null**（不是 false）" \
  "$(printf '%s' "$_j2" | jq -r '.agents[0].live')" "null"
chk "  但成员本身照样列得出来（邮箱状态不依赖身份空间）" \
  "$(printf '%s' "$_j2" | jq -r '.agents | length')" "2"
"$REALTMUX" -L "$_SOCK" kill-server 2>/dev/null || true

echo
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ] || exit 1
echo "===== P4f 验收全部通过 ====="
