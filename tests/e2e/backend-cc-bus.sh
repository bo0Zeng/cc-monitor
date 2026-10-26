#!/usr/bin/env bash
# `P4f`：backend 的 cc-bus 基础命令 —— `--bus-list` / `--bus-send` **真跑**。
#
# ## 为什么必须有真跑这一层
#
# 单测把 `parse_list` / `classify_send` 那些纯函数钉住了，但它们**证明不了命令调得动**：
# 08-13 实测撞到过一次 —— 两条命令进了 `inbound::REGISTRY`、`hello.commands` 也报了它们、
# 分派臂也认（`stream::cli::handles` 是派生的），而 `main::is_query_mode` 这道**闸门**
# 读的是手写的 `SUBCOMMANDS`。漏加两行的后果是**静默的**：backend 打一行
# 「未知 flag，已忽略」的 warn 之后**照常进流模式**，调用方拿到一堆 jsonl 行。
# 单测全绿。⇒ 这套件跑的是**真二进制的真 argv**。
#
# ## 本机安全
#
# · `CLAUDE_CONFIG_DIR` 指向 mktemp 沙箱 —— **不指的话后端会去流式读你真实的
#   `~/.claude`**（只读，但会把你的转录刷一屏，08-13 我就这么干了一次）；
# · `CC_BUS_HOME` 同样在沙箱里，绝不碰真实 `~/.cc-bus/`；
# · `CC_BUS_BIN_DIR` 指向**仓内**的 cc-bus 脚本（不是已装的那份）——
#   验的是仓里这一版，与 `exec-bit-guard` 的口径一致；
# · ★★ **全程把 shim 放在 PATH 最前面**，任何裸 `tmux` 都被强制 `-L <隔离socket>`
#   （`C7i`：backend 与 cc-bus 内部都是裸调 `tmux`，塞不进 `-L`，只能这样拦）。
#   ⚠⚠ **这条是事故换来的**：本套件原来只在用到 tmux 的那一格前面挂 shim，
#   头注还写着「本套件不用 tmux」。后来我往一格里加了几行裸 `tmux new-session` ——
#   **那句过时的注释正是我省掉 shim 的理由** —— 于是 `kreal_cc` / `kocc_cc`
#   **建到了用户的默认 socket 上**（事后按名字精确收掉了，两个里面都只有本套件的 sleep）。
#   ⇒ 修法不是「下次记得加前缀」，是让它**写不出来**：shim 全程在 PATH 上 + 起飞前双向自检。
# · 起的进程只有后端自己（一次性 exec，`</dev/null` + `timeout`）。
. "$(cd "$(dirname "$0")" && pwd)/sandbox-env.sh"  # 无条件清掉继承来的 CCM_* / CLAUDE* / ANTHROPIC_* / TMUX* / CC_BUS_*
set -o pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
# 后端说的那几句按文案键认，本文件不钉原文：zh = 插好值的整句（与界面同一个取文口）；zh_frag = 那一条最长的一段固定字。
zh() { "$REPO/node_modules/.bin/tsx" "$REPO/tests/e2e/copy-text.mts" "$@"; }
zh_frag() { python3 -c 'import json,re,sys; z=json.load(open(sys.argv[1]))["entries"][sys.argv[2]]["zh"]; print(max(re.split(r"\{[A-Za-z0-9]+\}", z), key=len).strip())' "$REPO/src/shared/copy/table.json" "$1"; }
D="${CARGO_TARGET_DIR:-$REPO/.build/backend}/debug/cc-monitor-backend"
[ -x "$D" ] || { echo "需要先 build backend：cd src/backend && cargo build"; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "需要 jq"; exit 1; }
TIMEOUT="$(command -v timeout)" || { echo "需要 timeout"; exit 1; }
REALTMUX="$(command -v tmux)" || { echo "需要 tmux（[10] 的身份空间对账要它）"; exit 1; }

SANDBOX="$(mktemp -d)"
trap 'rm -rf "$SANDBOX"; tmux_shim_cleanup' EXIT
BUS="$SANDBOX/bus"; CLA="$SANDBOX/claude"; EMPTY="$SANDBOX/empty"; NOHOME="$SANDBOX/nohome"
mkdir -p "$BUS"/{inbox,state,log} "$CLA/projects" "$EMPTY" "$NOHOME"

# ===== 起飞前：把 tmux 钉死在隔离 socket 上（C7i）=====
# shellcheck source=tests/e2e/tmux-shim.sh
. "$REPO/tests/e2e/tmux-shim.sh" --names-only
_SOCK="$(e2e_run_name ccbusid)" || exit 2
# 收尾只收自己这一趟那台（中途退出也收）。
trap 'e2e_tmux_reap "$REALTMUX" "$_SOCK"; rm -rf "$SANDBOX"; tmux_shim_cleanup' EXIT
_SHIM="$SANDBOX/shim"; mkdir -p "$_SHIM"
printf '#!/bin/bash\nexec %s -L %s "$@"\n' "$REALTMUX" "$_SOCK" > "$_SHIM/tmux"
chmod +x "$_SHIM/tmux"
export PATH="$_SHIM:$PATH"
# 双向 canary：**两个方向都必须观测到确定的东西**（否定式守卫会空转）。
_canary="ccbuscanary$$"
tmux new-session -d -s "$_canary" -c /tmp 'sleep 60' 2>/dev/null
if ! tmux has-session -t "=$_canary" 2>/dev/null; then
  echo "起飞前自检失败：shim 上建不出 canary"; exit 9
fi
# 另一向问的是本趟的私有 socket（显式 `-L`），不去问缺省那台：问缺省那台本身就是在碰用户的 tmux。
if ! "$REALTMUX" -L "$_SOCK" has-session -t "=$_canary" 2>/dev/null; then
  echo "起飞前自检失败：canary 不在本趟的私有 socket 上 —— 裸调的 tmux 没落到 shim 指的那台"
  exit 9
fi
tmux kill-session -t "=$_canary" 2>/dev/null || true

SCRIPTS="$REPO/src/shared/cc-bus/scripts"
[ -x "$SCRIPTS/cc-list" ] || { echo "仓内没有 cc-list：$SCRIPTS"; exit 1; }

pass=0; fail=0
chk() { if [ "$2" = "$3" ]; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1: 期望[$3] 实得[$2]"; fail=$((fail+1)); fi; }

# 一次调用 = 一个干净的 env（**逐次显式给全**，免得上一格的变量漏进来）
d() {
  env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" \
      CCBUS_POLICY_MODE="${POLICY:-off}" CC_BUS_ID=probe_cc \
      "$TIMEOUT" "$WALL_CEILING_S" "$D" -- "$@" 2>"$SANDBOX/err.txt"
}

echo "===== P4f：backend 的 cc-bus 基础命令 ====="

echo "[1] --bus-list：空总线"
out="$(d --bus-list </dev/null)"
chk "空总线回空数组（不是报错）" "$(printf '%s' "$out" | jq -c '.agents')" "[]"

echo "[2] --bus-list：读得出 id / 地址 / 待读数"
# 台架**直接写** agents.tsv：它扮演的是"cc-bus 的状态"。
# ⚠ 被测的 backend **不许**读这个格式（`no_cc_bus_data_layout_leaks_into_the_backend` 钉着），
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
out="$(printf '{"to":"alpha_cc","text":"来自后端的一条"}' | d --bus-send)"
chk "回 sent=true" "$(printf '%s' "$out" | jq -r '.sent')" "true"
chk "回显收件人" "$(printf '%s' "$out" | jq -r '.to')" "alpha_cc"
chk "★ 收件箱真的多了一行" "$(( $(wc -l < "$BUS/inbox/alpha_cc.jsonl") - before ))" "1"
chk "  正文一字不差" \
  "$(tail -1 "$BUS/inbox/alpha_cc.jsonl" | jq -r .text)" "来自后端的一条"

echo "[4] ★ 收件人非法：交给 cc-send 之前后端先判形状（INVARIANTS §47 ①，拒码 bad_id）"
printf '{"to":"a/b","text":"x"}' | d --bus-send >"$SANDBOX/o4.txt"; rc4=$?
chk "退出码非 0" "$([ "$rc4" -ne 0 ] && echo yes || echo no)" "yes"
chk "码是 bad_id" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "bad_id"
# 失败信封 {code, message, detail}：message 是按码定的那一句，处理器原话（点名那个值 · 查过哪几处 · 实测字节数）在复制详情 detail 里。
chk "  消息里点名那个值（后端拒的，不是 cc-send）" \
  "$(jq -r '.message + "\n" + .detail' < "$SANDBOX/err.txt" | grep -c '"a/b"')" "1"

echo "[5] ★ 被路由层拦下：与「名字写错了」必须分得开"
printf 'probe_cc\tnobody\n' > "$BUS/policy.tsv"
POLICY=on
printf '{"to":"alpha_cc","text":"x"}' | d --bus-send >/dev/null; rc5=$?
POLICY=off
chk "退出码非 0" "$([ "$rc5" -ne 0 ] && echo yes || echo no)" "yes"
chk "★ 码是 rejected（不是 bad_args）" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "rejected"
rm -f "$BUS/policy.tsv"

echo "[6] ★ 没装 cc-bus：说得出查过哪儿"
env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$EMPTY" HOME="$NOHOME" PATH="$EMPTY" \
    "$TIMEOUT" "$WALL_CEILING_S" "$D" -- --bus-list </dev/null >/dev/null 2>"$SANDBOX/err6.txt"
chk "码是 not_installed（不是笼统的 failed）" "$(jq -r .code < "$SANDBOX/err6.txt" 2>/dev/null)" "not_installed"
msg="$(jq -r '.message + "\n" + .detail' < "$SANDBOX/err6.txt" 2>/dev/null)"
chk "  列出了 ~/.local/bin 那一处" "$(printf '%s' "$msg" | grep -c '.local/bin/cc-list')" "1"
chk "  列出了 skills 那一处" "$(printf '%s' "$msg" | grep -c 'skills/cc-bus/scripts/cc-list')" "1"
chk "  告诉人怎么指过去" "$(printf '%s' "$msg" | grep -c 'CC_BUS_BIN_DIR')" "1"

echo "[7] ★ 两个入口都有：CLI 面上调得动"
# ★ 这一格钉的是那条**静默失效**：闸门（`main::SUBCOMMANDS`）漏加时，这条 flag 不进一次性模式、
#   掉进流模式吐一堆 jsonl 行 ⇒ 下面这格会红。
out="$(d --bus-list </dev/null)"
chk "★ --bus-list 调得动（漏了闸门这格会红）" \
  "$(printf '%s' "$out" | jq -e 'has("agents")' >/dev/null 2>&1 && echo yes || echo no)" "yes"

echo "[8] ★ cc-bus 命令卡住时，backend 不许陪着一起卡"
# ★ 真事故：`Command::output()` **无限等**。把 cc-send 换成 sleep 300 的桩，
#   --bus-send 25 秒没回来（25 是从外面掐的，backend 自己没有期限）。
#   而这两条是阻塞档，一条卡住占死一个 tokio worker，且 cancel 对 spawn_blocking 是空操作。
# ⚠ 修法**不是**在后端里加计时器（零定时器铁律 + 协议逐字「超时一律推给客户端」）——
#   而是让**子进程自己**有期限（timeout 前缀，同 ccm 问后端那条）。
mkdir -p "$SANDBOX/hangbin"
printf '#!/bin/bash\nsleep %s\n' "$((2 * WALL_CEILING_S))" > "$SANDBOX/hangbin/cc-send"
chmod +x "$SANDBOX/hangbin/cc-send"
cp "$SCRIPTS/cc-list" "$SANDBOX/hangbin/cc-list"
# 卡住的那一个睡两倍兜底上限、外面按兜底上限掐 ⇒ 期限不生效就是被掐（退出码 124）。不按墙钟数「几秒内回来」。
printf '{"to":"x_cc","text":"hi"}' | env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" \
    CC_BUS_BIN_DIR="$SANDBOX/hangbin" CC_BUS_TIMEOUT_SECS=2 \
    "$TIMEOUT" "$WALL_CEILING_S" "$D" -- --bus-send >/dev/null 2>"$SANDBOX/err8.txt"
_rc8=$?
chk "★ 2 秒的期限：自己回来了（不是等到我们从外面掐）" \
  "$([ "$_rc8" -ne 124 ] && echo yes || echo "no（被外面掐了）")" "yes"
chk "  码是 child_timed_out（不是笼统的 failed）" "$(jq -r .code < "$SANDBOX/err8.txt" 2>/dev/null)" "child_timed_out"
chk "  消息说得出多半卡在哪（cc-bus 的锁；flock 是禁档词，句子改说「锁」）" \
  "$(jq -r '.message + "\n" + .detail' < "$SANDBOX/err8.txt" 2>/dev/null | grep -cF "$(zh_frag beCcBus.timedOut.say)")" "1"

echo "[9] ★ 声明「不收输入」的命令，stdin 不关时必须秒回"
# ★ 真事故：CLI 入口原来从 `fields` **派生**「要不要读 stdin」，而 `fields` 是
#   「args 和 data 的字段名」。`bus-list` 无输入却有输出字段 ⇒ 被判成要读 stdin
#   ⇒ **挂住等一个永远不来的输入**（实测 --ping 120ms 回、--bus-list 6 秒被掐死才停）。
# ⚠ 守它的那条单测**是恒真的**（两个分支各是同一个表达式的复述），一声没吭。
#   ⇒ 这一格钉**行为**：真起进程 + 一条不关的管道。声明真不真，由它说了算。
for _c in --ping --bus-list; do
  # stdin 开着两倍兜底上限、外面按兜底上限掐：挂着等输入就是被掐（124）。
  _o="$(env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" \
        "$TIMEOUT" "$WALL_CEILING_S" "$D" -- "$_c" < <(exec sleep "$((2 * WALL_CEILING_S))") 2>/dev/null
        _r=$?; kill "$!" 2>/dev/null; exit "$_r")"; _rc9=$?
  chk "★ $_c：stdin 不关也返回了（不是挂到被掐）" \
    "$([ -n "$_o" ] && [ "$_rc9" -ne 124 ] && echo yes || echo "no（退出码 $_rc9，输出 ${_o:-<空>}）")" "yes"
done

echo "[10] ★ 总线成员是**身份空间的子集**，不是第二套名单"
# 〔用@08-13〕逐字：「那他不应该是身份空间的子集吗? 他应该去调用身份空间啊」。
# cc-bus 的 agents.tsv 记的地址**会过期**（会话名被重用是常态）——08-13 实测后果是
# 敲门文字打进**陌生占用者**的屏幕。⇒ live/ccm_sid 由后端去问 tmux，agents.tsv
# 只回答「谁登记过 + 还剩几条没读」。
# ⚠ live 是**三态**：true / false / null。判据三格全钉——只钉前两格的话，
#   「问不到就当成不在」这种最坏的读法会溜过去（把一屋子活人判成死人）。
tmux new-session -d -s alive_cc -c /tmp 'cat'
tmux set-option -t alive_cc @ccm_sid 'sid-1234' >/dev/null 2>&1
new_bus_state() {
  printf 'alive_cc\talive_cc:0.0\tts\t1\ngone_cc\tgone_cc:0.0\tts\t2\n' > "$BUS/agents.tsv"
  : > "$BUS/inbox/alive_cc.jsonl"; : > "$BUS/inbox/gone_cc.jsonl"
}
new_bus_state
_j="$(env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" \
      "$TIMEOUT" "$WALL_CEILING_S" "$D" -- --bus-list </dev/null 2>/dev/null)"
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
       "$TIMEOUT" "$WALL_CEILING_S" "$D" -- --bus-list </dev/null 2>/dev/null)"
chk "★ 问不到身份空间 ⇒ live 是 **null**（不是 false）" \
  "$(printf '%s' "$_j2" | jq -r '.agents[0].live')" "null"
chk "  但成员本身照样列得出来（邮箱状态不依赖身份空间）" \
  "$(printf '%s' "$_j2" | jq -r '.agents | length')" "2"

echo "[11] ★ bus-send 也要说清「有没有人会读」"
# 两种「没人会读」今天都长得像成功：① 收件人压根没登记（打错一个字母就造出幽灵收件箱）；
# ② 登记过、**会话早没了**（cc-bus 那份名单会过期）。
# ⚠ 不改变投递（先发后到是正当用法），只是把话说清楚。
printf 'alive_cc\talive_cc:0.0\tts\t1\ngone_cc\tgone_cc:0.0\tts\t2\n' > "$BUS/agents.tsv"
_snd() {
  printf '{"to":"%s","text":"x"}' "$1" | env CLAUDE_CONFIG_DIR="$CLA" \
    CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" "$TIMEOUT" "$WALL_CEILING_S" "$D" -- --bus-send 2>/dev/null
}
_a="$(_snd alive_cc)"; _g="$(_snd gone_cc)"; _n="$(_snd nobody_cc)"
chk "★ 活着的收件人：registered=true live=true" \
  "$(printf '%s' "$_a" | jq -c '[.registered,.live]')" "[true,true]"
chk "★ 登记过但会话没了：registered=true **live=false**" \
  "$(printf '%s' "$_g" | jq -c '[.registered,.live]')" "[true,false]"
chk "★ 压根没登记：registered=false live=null" \
  "$(printf '%s' "$_n" | jq -c '[.registered,.live]')" "[false,null]"
chk "  三种都**照发不误**（先发后到是正当用法）" \
  "$(printf '%s\n%s\n%s' "$_a" "$_g" "$_n" | jq -r .sent | grep -c true)" "3"
chk "  且消息真的落进了各自的收件箱" \
  "$(wc -l < "$BUS/inbox/nobody_cc.jsonl" 2>/dev/null || echo 0)" "1"

echo "[12] ★ 正文太长要**归对因**：不是 cc-bus 坏了，是塞不进 argv"
# 实测：200KB 正文 → 原来回 {"code":"failed","message":"起不来 cc-send：Argument list too long"}。
# 两处不对：① failed 是兜底桶，调用方分不出「我的消息太长」（自己能修）与「cc-bus 坏了」；
# ② 那句话归错了因 —— cc-send 好好的。⇒ 单独的 too_long + 实测字节数。
# ⚠ 上限是**内核**的（MAX_ARG_STRLEN 128 KiB，P4b §7g-8b 量过：131000 OK / 131072 E2BIG），
#   不是我们能改的东西；能保证的是**说得准**。
python3 -c 'import json,sys; sys.stdout.write(json.dumps({"to":"alive_cc","text":"x"*200000}))' \
  > "$SANDBOX/big.json"
env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" \
    "$TIMEOUT" "$WALL_CEILING_S" "$D" -- --bus-send < "$SANDBOX/big.json" >/dev/null 2>"$SANDBOX/err12.txt"
chk "★ 码是 too_long（不是兜底的 failed）" "$(jq -r .code < "$SANDBOX/err12.txt" 2>/dev/null)" "too_long"
_m12="$(jq -r '.message + "\n" + .detail' < "$SANDBOX/err12.txt" 2>/dev/null)"
chk "  说了实测字节数（别让人自己去量）" "$(printf '%s' "$_m12" | grep -cF "$(zh beCcBus.deliver.tooLong reason= n=200000)")" "1"
chk "  明说不是 cc-bus 坏了（归因不许甩锅）" "$(printf '%s' "$_m12" | grep -cF "$(zh beCcBus.notRun.tooLong)")" "1"
# 对照：120KB 必须仍然发得出去（免得判据把上限收窄成"长的都不让发"）
python3 -c 'import json,sys; sys.stdout.write(json.dumps({"to":"alive_cc","text":"y"*120000}))' \
  > "$SANDBOX/mid.json"
_o12="$(env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" \
        "$TIMEOUT" "$WALL_CEILING_S" "$D" -- --bus-send < "$SANDBOX/mid.json" 2>/dev/null)"
chk "  对照：120KB 照样发得出去" "$(printf '%s' "$_o12" | jq -r .sent)" "true"

echo "[13] ★ 以谁的身份发：不给 from 的话，回复会掉进没人读的收件箱"
# 实测：backend 跑 cc-send 时不在任何 tmux pane 里 ⇒ cc-whoami 解不出身份 ⇒
# 收信人看到「来自 unknown」，而它给的回复方式是 `cc-send unknown "…"` ——
# **那是我们自己制造的幽灵收件箱**（同 [4] 那条打错名字的病，只是这次是工具造的）。
# ⇒ bus-send 收可选的 from，作为 CC_BUS_ID 传给子进程（cc-whoami 优先级第一条，
#   是 cc-bus **现成的契约**，不改它本体）。
_fb="$SANDBOX/frombus"; mkdir -p "$_fb"/{inbox,state,log}
printf 'x_cc	x_cc:0.0	ts	1
' > "$_fb/agents.tsv"
_send_from() {
  printf '%s' "$2" | env -u TMUX -u TMUX_PANE -u CC_BUS_ID CLAUDE_CONFIG_DIR="$CLA" \
    CC_BUS_HOME="$_fb" CC_BUS_BIN_DIR="$SCRIPTS" "$TIMEOUT" "$WALL_CEILING_S" "$D" -- --bus-send 2>/dev/null
}
: > "$_fb/inbox/x_cc.jsonl"
_r1="$(_send_from x '{"to":"x_cc","text":"没给 from"}')"
chk "不给 from ⇒ 回值里 from 是 null（如实回显，别让调用方以为有身份）" \
  "$(printf '%s' "$_r1" | jq -r '.from')" "null"
chk "  ⚠ 而收信人看到的确实是 unknown（这就是那条幽灵）" \
  "$(jq -r .from < "$_fb/inbox/x_cc.jsonl" | tail -1)" "unknown"
_r2="$(_send_from x '{"to":"x_cc","text":"给了 from","from":"cc-monitor"}')"
chk "★ 给了 from ⇒ 回值回显它" "$(printf '%s' "$_r2" | jq -r '.from')" "cc-monitor"
chk "★ 且收信人看到的就是它（不再是 unknown）" \
  "$(jq -r .from < "$_fb/inbox/x_cc.jsonl" | tail -1)" "cc-monitor"

echo "[SH1-a] ★ INVARIANTS §49：读会话名 / 地址的 tmux 客户端是 UTF-8 客户端（非 UTF-8 locale ＋ 中文会话名）"
# `cc-register` 记登记地址 · `cc-whoami` 三条认身份的路，读的都是**会话名**（可以是中文）。
# 台架：pane 里整条命令跑在 `LC_ALL=C` 下 —— tmux 只看 `LC_ALL`→`LC_CTYPE`→`LANG` 第一个非空值有没有 `UTF-8`，
# 这一形就是「非 UTF-8 客户端」。不带旗的话，中文被改写成 `_`、退出码仍是 0。
# 期望值由**同一个 locale 下的同一条消毒**现算（`cc-whoami::resolve` 的那条 sed），不写死。
# ⚠ 〔SH1 09-26 现打，tmux 3.6〕`TMUX` 已设的客户端 tmux **一律按 UTF-8 打**、不看 locale —— 这五处在生产上都跑在
#   tmux 里（`TMUX` 恒设），所以 3.6 上它们今天其实没被改写（潜伏的违反，靠的是一条手册里没写的启发式）。
#   旗保证的是不靠它 ⇒ 台架给 pane 里的 `tmux` 另挂一层 shim：摘掉客户端那一侧的 `TMUX` 再转给隔离 socket，
#   才造得出真正的非 UTF-8 客户端（反向正控就是验这一格）。脚本自己读到的 `$TMUX` 不动（`cc-whoami` 兜底 1 要它）。
_u8d="$SANDBOX/u8"; _u8bus="$SANDBOX/u8bus"; _u8shim="$SANDBOX/u8shim"; mkdir -p "$_u8d" "$_u8bus" "$_u8shim"
printf '#!/bin/bash\nunset TMUX\nexec %s -L %s "$@"\n' "$REALTMUX" "$_SOCK" > "$_u8shim/tmux"
chmod +x "$_u8shim/tmux"
_u8name="u8甲乙"
tmux new-session -d -s "$_u8name" -c /tmp \
  env LC_ALL=C LANG=C LC_CTYPE=C PATH="$_u8shim:$PATH" CC_BUS_HOME="$_u8bus" SCRIPTS="$SCRIPTS" OUT="$_u8d" sh -c '
    "$SCRIPTS/cc-register" > "$OUT/reg" 2>&1
    "$SCRIPTS/cc-whoami" > "$OUT/who1" 2>&1
    env -u TMUX_PANE "$SCRIPTS/cc-whoami" > "$OUT/who2" 2>&1
    env -u TMUX_PANE TMUX="${TMUX%,*}," "$SCRIPTS/cc-whoami" > "$OUT/who3" 2>&1
    tmux display-message -p "#S" > "$OUT/raw" 2>&1
    touch "$OUT/done"; exec sleep 30' 2>/dev/null
wall_until '[ -f "$_u8d/done" ]' || true
_u8want="$(printf '%s' "$_u8name" | LC_ALL=C sed 's/[^A-Za-z0-9_-]/-/g; s/^-*//; s/-*$//')"
chk "  反向正控：同台架上不带旗的 display-message 确实被改写（台架真是非 UTF-8 客户端）" \
  "$([ "$(cat "$_u8d/raw" 2>/dev/null)" != "$_u8name" ] && [ -f "$_u8d/done" ] && echo yes || echo no)" "yes"
chk "★ cc-register 记下的登记地址是那个中文会话名（不是 _ 改写过的）" \
  "$(awk -F'\t' -v id="$_u8want" '$1==id{print $2}' "$_u8bus/agents.tsv" 2>/dev/null)" "$_u8name:0.0"
chk "★ cc-whoami（TMUX_PANE 那一条）认出的身份" "$(cat "$_u8d/who1" 2>/dev/null)" "$_u8want"
chk "★ cc-whoami（按 \$TMUX 反查会话 id 那一条）认出的身份" "$(cat "$_u8d/who2" 2>/dev/null)" "$_u8want"
chk "★ cc-whoami（沿进程树找 pane 那一条）认出的身份" "$(cat "$_u8d/who3" 2>/dev/null)" "$_u8want"
tmux kill-session -t "=$_u8name" 2>/dev/null || true

echo "[SH1-b] ★ 驾驶舱读面 —— 后端转调 cc-bus 的机器可读读命令（名单只认好行 · 收件箱只看尾巴）"
# 台架写一份**脏**的名册（只采结构：`--help` 行 · 缺字段行 · 真空行 · 只有 TAB 的行）。
printf 'alpha_cc\talpha_cc:0.0\t2026-01-01T00:00:00+00:00\t111\n--help\tx:0.0\tts\t1\nshort\tonly\n\n\t\t\t\nbeta_cc\tbeta_cc:0.0\tts2\t\n' > "$BUS/agents.tsv"
printf '{"from":"peer_cc","ts":"t1","text":"占位一"}\nnot json\n{"from":"peer_cc","ts":"t2","text":"占位二"}\n' > "$BUS/inbox/alpha_cc.jsonl"
echo 1 > "$BUS/state/alpha_cc.pos"
out="$(d --bus-list </dev/null)"
chk "名单两条好行（--help 被本侧 id 判定拒掉）" "$(printf '%s' "$out" | jq -c '[.agents[].id]')" '["alpha_cc","beta_cc"]'
chk "  待读数照旧（3 行 − 已读 1）" "$(printf '%s' "$out" | jq -r '.agents[0].unread')" "2"
_pos_before="$(cat "$BUS/state/alpha_cc.pos")"; _state_before="$(ls "$BUS/state" | tr '\n' ' ')"
out="$(printf '{"id":"alpha_cc","lines":2}' | d --bus-inbox)"
chk "★ 收件箱只看尾巴（末 2 行：一条坏、一条好）" "$(printf '%s' "$out" | jq -c '[.messages[].text, .skipped]')" '["占位二",1]'
chk "★ 读收件箱不推已读位置" "$(cat "$BUS/state/alpha_cc.pos")" "$_pos_before"
chk "  也不在 state/ 里写任何东西" "$(ls "$BUS/state" | tr '\n' ' ')" "$_state_before"
printf '{"id":"--help"}' | d --bus-inbox >/dev/null
chk "★ --help 当收件箱 id ⇒ bad_id（交给 cc-log 之前拒）" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "bad_id"
# 老 cc-bus：`cc-list` 不认 --tsv（照打人读表）⇒ 明说要重新部署，不解成一份空名单。
_old="$SANDBOX/oldbus"; mkdir -p "$_old"
printf '#!/bin/sh\necho "ID           TMUX               待读"\n' > "$_old/cc-list"; chmod +x "$_old/cc-list"
env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$_old" CC_BUS_ID=probe_cc "$TIMEOUT" "$WALL_CEILING_S" "$D" -- --bus-list </dev/null >/dev/null 2>"$SANDBOX/err.txt"
# 那一句按文案键认（beCcBus.read.tooOld 最长的那一段固定字），不钉原文。
_TOOOLD="$(zh_frag beCcBus.read.tooOld)"
chk "★ 老 cc-bus ⇒ failed 且说要重新部署（beCcBus.read.tooOld）" "$(jq -r --arg w "$_TOOOLD" '.code + " " + ((.message + "\n" + .detail) | contains($w) | tostring)' < "$SANDBOX/err.txt" 2>/dev/null)" "failed true"
rm -f "$BUS/agents.tsv"

echo "[SH1-c] ★ 杀会话不碰 cc-bus 名册（顺手注销那一段删了；名册改按会话 id 推之前，登记照旧留着）"
: > "$BUS/agents.tsv"
_dg="$SANDBOX/dg"; mkdir -p "$_dg"
tmux new-session -d -s dg-cc -c /tmp env CC_BUS_HOME="$BUS" SCRIPTS="$SCRIPTS" OUT="$_dg" sh -c '
  "$SCRIPTS/cc-register" dg_cc >/dev/null 2>&1; touch "$OUT/a"; exec sleep 60' 2>/dev/null
wall_until '[ -f "$_dg/a" ]' || true
printf '{"from":"x","text":"占位"}\n' >> "$BUS/inbox/dg_cc.jsonl"
chk "  台架：会话登记上了" "$(awk -F'\t' '$1=="dg_cc"' "$BUS/agents.tsv" | wc -l | tr -d ' ')" "1"
out="$(printf '{"name":"dg-cc"}' | d --kill)"
chk "杀会话本身照旧成功" "$(printf '%s' "$out" | jq -c '[.session, .killed]')" '["dg-cc",true]'
chk "★ 名册里那一行没被动" "$(awk -F'\t' '$1=="dg_cc"' "$BUS/agents.tsv" | wc -l | tr -d ' ')" "1"
chk "★ 收件箱也没被动" "$([ -e "$BUS/inbox/dg_cc.jsonl" ] && echo 在 || echo 没了)" "在"

echo "[CLI2] ★ CLI 面的入参不再隐含「调用方写得了 stdin」：argv 载荷口 · 开着不写不挂 · --stdin-line 任意位置 · 超大回码"
# 一份沙箱里的会话记录（夹具文字，不是真正文）。
mkdir -p "$CLA/projects/-cli2"
J="$CLA/projects/-cli2/c2000000-0000-4000-8000-000000000001.jsonl"
printf '%s\n' \
  '{"type":"user","uuid":"u1","parentUuid":null,"sessionId":"c2000000-0000-4000-8000-000000000001","timestamp":"2026-10-09T00:00:00Z","cwd":"/w","message":{"role":"user","content":"夹具一句"}}' \
  '{"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"c2000000-0000-4000-8000-000000000001","timestamp":"2026-10-09T00:00:01Z","message":{"role":"assistant","model":"m","content":[{"type":"text","text":"夹具回话"}]}}' > "$J"
for _cmd in history-read history-facts; do
  _j="$(jq -cn --arg p "$J" '{path:$p}')"
  printf '%s' "$_j" | d "--$_cmd" >"$SANDBOX/c2-in.txt"; _rc_in=$?; cp "$SANDBOX/err.txt" "$SANDBOX/c2-in.err"
  _b="$(printf '%s' "$_j" | base64 -w0)"
  d "--$_cmd" --args-b64 "$_b" < <(sleep 30) >"$SANDBOX/c2-av.txt"; _rc_av=$?
  chk "★ $_cmd：stdin 那一形答出来了（退出 0）" "$_rc_in" "0"
  # 续传令牌的 `rev`（会话事实那一份）每个进程按钟各取各的种 ⇒ 两个一次性进程各铸一个，比之前摘掉；别的逐字比。
  _norev='walk(if type == "object" then del(.rev) else . end)'
  chk "★ $_cmd：argv 载荷口与 stdin 逐字一样（stdout，摘掉 rev）" "$(cmp -s <(jq -cS "$_norev" "$SANDBOX/c2-in.txt") <(jq -cS "$_norev" "$SANDBOX/c2-av.txt") && echo 同 || echo 不同)" "同"
  chk "  $_cmd：退出码 · stderr 也一样" "$_rc_av|$(cat "$SANDBOX/err.txt")" "$_rc_in|$(cat "$SANDBOX/c2-in.err")"
  cp "$SANDBOX/c2-in.txt" "$SANDBOX/c2-$_cmd.txt"
done
# 成品行是通用记录（`record`：`t` ∈ said · reply …，`id` 是那一行自己的身份）；空包 / 换了格名读不到 ⇒ 这里拿到的是 `null:null`。
chk "  history-read 出的是成品行（不是空包）" "$(jq -r '[.rows[] | "\(.record.t):\(.record.id)"] | join(",")' < "$SANDBOX/c2-history-read.txt")" "said:u1,reply:a1"
# 出口的声明（--view，与帧面信封的 view 同一处解、同一处裁）：只要行摘要的 end ⇒ 每行只剩 end；JSON 原样与 base64 两种写法逐字一样；
#   认不出的格 ⇒ bad_args（不静默放过）。
_v='{"cells":{"read_row":["end"]}}'
jq -cn --arg p "$J" '{path:$p}' | d --history-read --view "$_v" >"$SANDBOX/c2-view.txt"
chk "★ --view 挑格：每行只剩 end" "$(jq -r '[.rows[] | keys | join("+")] | unique | join(",")' < "$SANDBOX/c2-view.txt")" "end"
jq -cn --arg p "$J" '{path:$p}' | d --history-read --view "$(printf %s "$_v" | base64 | tr -d '\n')" >"$SANDBOX/c2-view64.txt"
chk "  --view 的 base64 写法与 JSON 原样逐字一样" "$(cmp -s "$SANDBOX/c2-view.txt" "$SANDBOX/c2-view64.txt" && echo 同 || echo 不同)" "同"
jq -cn --arg p "$J" '{path:$p}' | d --history-read --view '{"omit":{"record":["blcoks"]}}' >/dev/null
chk "★ --view 点了目录里没有的格：bad_args" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "bad_args"
# stdin 开着、一直不写：两种读法都立即回 no_input，不挂到被掐。
# stdin 开着两倍兜底上限、`d` 按兜底上限掐 ⇒ 挂着等就是 124，不是 2。
for _extra in "" --stdin-line; do
  d --history-read $_extra < <(exec sleep "$((2 * WALL_CEILING_S))") >/dev/null; _rc=$?; kill "$!" 2>/dev/null
  chk "★ 开着不写（${_extra:-默认}）：立即回码（不是挂到被掐）" "$([ "$_rc" -eq 2 ] && echo 是 || echo "否 rc=$_rc")" "是"
  chk "  码是 no_input" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "no_input"
done
# stdin 是 EOF：照旧当 {}，由命令自己说缺什么。
d --history-read </dev/null >/dev/null
chk "★ stdin 是 EOF：命令自己回缺入参（bad_args）" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "bad_args"
# --stdin-line 不在第二格也认：读到换行就动手，stdin 后面不关也不挂。
# stdin 由本 shell 拿着一个管子的写端：写完一行不关，答出来之后才关 ⇒ 「不等 EOF」不靠墙钟分。
_sl="$SANDBOX/c2-sl.fifo"; rm -f "$_sl"; mkfifo "$_sl"
d --history-read --no-such-option --stdin-line <"$_sl" >"$SANDBOX/c2-sl.txt" &
_pid=$!; exec 7>"$_sl"; printf '%s\n' "$(jq -cn --arg p "$J" '{path:$p}')" >&7
_ok=否; wall_until '[ -s "$SANDBOX/c2-sl.txt" ]' && _ok=是
exec 7>&-; kill "$_pid" 2>/dev/null; wait "$_pid" 2>/dev/null
chk "★ --stdin-line 在后面也认（stdin 还开着就答出、不等 EOF）" "$_ok" "是"
# 两个口都给 ⇒ bad_args。
d --history-read --args-b64 e30= --stdin-line < <(sleep 30) >/dev/null
chk "★ 两个口都给：bad_args" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "bad_args"
# 超大：stdin 一形（1 MiB ＋ 1）· argv 一形（系统放得进来、本后端嫌大的那一段）。
head -c 1048577 /dev/zero | tr '\0' ' ' | d --history-read >/dev/null
chk "★ stdin 超 1 MiB：args_too_large（不截断、不挂）" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "args_too_large"
d --history-read --args-b64 "$(head -c 131070 /dev/zero | tr '\0' A)" < <(sleep 30) >/dev/null
chk "★ argv 值超本后端的上限：args_too_large" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "args_too_large"
# 超过系统单个参数的上限（Linux 131072 含 NUL）：起不来，错在 exec 那一层 —— 也是立即失败、不挂。
d --history-read --args-b64 "$(head -c 140000 /dev/zero | tr '\0' A)" < <(exec sleep "$((2 * WALL_CEILING_S))") >/dev/null 2>&1; _rc=$?; kill "$!" 2>/dev/null
chk "★ argv 超系统上限：exec 那一层立即失败（非 0、不挂）" "$([ "$_rc" -ne 0 ] && [ "$_rc" -ne 124 ] && echo 是 || echo "否 rc=$_rc")" "是"

# `--resolve`（帧命令 `resolve` 派生的那一面，手写臂删了）同一套口：argv 与 stdin 逐字一样 · 开着不写回 no_input。
_rq='{"sessionId":"s1"}'
printf '%s' "$_rq" | d --resolve >"$SANDBOX/c2-rs-in.txt"; _rc_in=$?
d --resolve --args-b64 "$(printf '%s' "$_rq" | base64 -w0)" < <(sleep 30) >"$SANDBOX/c2-rs-av.txt"; _rc_av=$?
chk "★ --resolve：argv 载荷口与 stdin 逐字一样（退出 0）" "$_rc_in|$_rc_av|$(cmp -s "$SANDBOX/c2-rs-in.txt" "$SANDBOX/c2-rs-av.txt" && echo 同 || echo 不同)" "0|0|同"
chk "  --resolve 成品是那条恢复命令" "$(jq -r .command < "$SANDBOX/c2-rs-av.txt" 2>/dev/null)" "claude --resume s1"
d --resolve < <(exec sleep "$((2 * WALL_CEILING_S))") >/dev/null; _rc=$?; kill "$!" 2>/dev/null
chk "★ --resolve 开着不写：立即回 no_input" "$([ "$_rc" -eq 2 ] && echo 是 || echo "否 rc=$_rc")|$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "是|no_input"

echo "[CLI3] ★ 一次性模式的 stderr 是协议通道：成功 ⇒ 一个字节都没有；失败 ⇒ 正好一行信封 {code, message, detail}；诊断进后端日志文件"
# 日志落点 = 那台后端自己的 stderr 诊断文件（`~/.cc-monitor/logs/backend/stderr.log`，宿主交了 CCM_BACKEND_STDERR_LOG 就用交的那份）。
LH="$SANDBOX/lh"; mkdir -p "$LH/.cc-monitor/logs/backend"; LOGF="$LH/.cc-monitor/logs/backend/stderr.log"
one() { HOME="$LH" d "$@"; }
# 成功两族：历史查询那条臂（history-read）· 控制面那条臂（bus-list）。
printf '%s' "$(jq -cn --arg p "$J" '{path:$p}')" | one --history-read >/dev/null; _rc=$?
chk "★ history-read 成功：退出 0 · stderr 0 字节" "$_rc|$(wc -c < "$SANDBOX/err.txt")" "0|0"
one --bus-list </dev/null >/dev/null; _rc=$?
chk "★ bus-list 成功：退出 0 · stderr 0 字节" "$_rc|$(wc -c < "$SANDBOX/err.txt")" "0|0"
# 失败两族：缺入参（契约错，会记一行诊断）· 控制面拒（bad_id）。
one --history-read </dev/null >/dev/null; _rc=$?
chk "★ history-read 缺 path：非 0 · stderr 正好一行 · 那一行是带 code 的信封" \
  "$([ "$_rc" -ne 0 ] && echo 非0)|$(wc -l < "$SANDBOX/err.txt")|$(jq -r 'select(type=="object") | .code' < "$SANDBOX/err.txt" 2>/dev/null)" "非0|1|bad_args"
# history-read 是 typed 命令：缺入参那一句是 serde 的原话（`missing field `path``，第 8 批第 3 刀起）。
chk "  契约错的诊断进了日志文件（不在 stderr）" "$(grep -c 'contract: malformed request: missing field `path`' "$LOGF" 2>/dev/null)|$(grep -c 'malformed request' "$SANDBOX/err.txt")" "1|0"
printf '{"to":"a/b","text":"x"}' | one --bus-send >/dev/null; _rc=$?
chk "★ bus-send 拒：非 0 · stderr 正好一行信封" \
  "$([ "$_rc" -ne 0 ] && echo 非0)|$(wc -l < "$SANDBOX/err.txt")|$(jq -r 'select(type=="object") | (.code)' < "$SANDBOX/err.txt" 2>/dev/null)" "非0|1|bad_id"
# 诊断文件那一层目录不在 ⇒ 不替它建（只建文件不建目录），stderr 照样干净。
NL="$SANDBOX/nolog"; mkdir -p "$NL"
HOME="$NL" d --history-read </dev/null >/dev/null
chk "  没有日志目录：stderr 照样一行 · 不建目录" "$(wc -l < "$SANDBOX/err.txt")|$([ -e "$NL/.cc-monitor" ] && echo 建了 || echo 没建)" "1|没建"

"$REALTMUX" -L "$_SOCK" kill-server 2>/dev/null || true

echo
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ] || exit 1
echo "===== P4f 验收全部通过 ====="
