#!/usr/bin/env bash
# `P4f`：backend 的 cc-bus 基础命令 —— `--bus-list` / `--bus-send` **真跑**。
#
# ## 为什么必须有真跑这一层
#
# 单测把 `parse_list` / `classify_send` 那些纯函数钉住了，但它们**证明不了命令调得动**：
# 08-13 实测撞到过一次 —— 两条命令进了 `inbound::REGISTRY`、`hello.commands` 也报了它们、
# 分派臂也认（`cli_control::handles` 是派生的），而 `main::is_query_mode` 这道**闸门**
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
#   头注还写着「本套件不用 tmux」。后来我往 `[15]` 里加了几行裸 `tmux new-session` ——
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
trap 'rm -rf "$SANDBOX"' EXIT
BUS="$SANDBOX/bus"; CLA="$SANDBOX/claude"; EMPTY="$SANDBOX/empty"; NOHOME="$SANDBOX/nohome"
mkdir -p "$BUS"/{inbox,state,log,queue} "$CLA/projects" "$EMPTY" "$NOHOME"

# ===== 起飞前：把 tmux 钉死在隔离 socket 上（C7i）=====
# shellcheck source=tests/e2e/tmux-shim.sh
. "$REPO/tests/e2e/tmux-shim.sh" --names-only
_SOCK="$(e2e_run_name ccbusid)" || exit 2
# 收尾只收自己这一趟那台（中途退出也收）。
trap 'e2e_tmux_reap "$REALTMUX" "$_SOCK"; rm -rf "$SANDBOX"' EXIT
_SHIM="$SANDBOX/shim"; mkdir -p "$_SHIM"
printf '#!/bin/bash\nexec %s -L %s "$@"\n' "$REALTMUX" "$_SOCK" > "$_SHIM/tmux"
chmod +x "$_SHIM/tmux"
export PATH="$_SHIM:$PATH"
# 双向 canary：**两个方向都必须观测到确定的东西**（否定式守卫会空转，`cc-spawn-uplift` 记着为什么）。
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
      "$TIMEOUT" 20 "$D" -- "$@" 2>"$SANDBOX/err.txt"
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
    "$TIMEOUT" 20 "$D" -- --bus-list </dev/null >/dev/null 2>"$SANDBOX/err6.txt"
chk "码是 not_installed（不是笼统的 failed）" "$(jq -r .code < "$SANDBOX/err6.txt" 2>/dev/null)" "not_installed"
msg="$(jq -r '.message + "\n" + .detail' < "$SANDBOX/err6.txt" 2>/dev/null)"
chk "  列出了 ~/.local/bin 那一处" "$(printf '%s' "$msg" | grep -c '.local/bin/cc-list')" "1"
chk "  列出了 skills 那一处" "$(printf '%s' "$msg" | grep -c 'skills/cc-bus/scripts/cc-list')" "1"
chk "  告诉人怎么指过去" "$(printf '%s' "$msg" | grep -c 'CC_BUS_BIN_DIR')" "1"

echo "[7] ★ 两个入口都有：能力探测口报得出这两条"
probe="$(d --backend-probe </dev/null)"
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

echo "[8] ★ cc-bus 命令卡住时，backend 不许陪着一起卡"
# ★ 真事故：`Command::output()` **无限等**。把 cc-send 换成 sleep 300 的桩，
#   --bus-send 25 秒没回来（25 是从外面掐的，backend 自己没有期限）。
#   而这两条是阻塞档，一条卡住占死一个 tokio worker，且 cancel 对 spawn_blocking 是空操作。
# ⚠ 修法**不是**在后端里加计时器（零定时器铁律 + 协议逐字「超时一律推给客户端」）——
#   而是让**子进程自己**有期限（timeout 前缀，同 ccm 问后端那条）。
mkdir -p "$SANDBOX/hangbin"
printf '#!/bin/bash\nsleep 300\n' > "$SANDBOX/hangbin/cc-send"
chmod +x "$SANDBOX/hangbin/cc-send"
cp "$SCRIPTS/cc-list" "$SANDBOX/hangbin/cc-list"
_t0=$(date +%s)
printf '{"to":"x_cc","text":"hi"}' | env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" \
    CC_BUS_BIN_DIR="$SANDBOX/hangbin" CC_BUS_TIMEOUT_SECS=2 \
    "$TIMEOUT" 30 "$D" -- --bus-send >/dev/null 2>"$SANDBOX/err8.txt"
_el=$(( $(date +%s) - _t0 ))
chk "★ 2 秒的期限：真的在 5 秒内回来了（不是等到我们从外面掐）" \
  "$([ "$_el" -le 5 ] && echo yes || echo "no（用了 ${_el}s）")" "yes"
chk "  码是 timed_out（不是笼统的 failed）" "$(jq -r .code < "$SANDBOX/err8.txt" 2>/dev/null)" "timed_out"
chk "  消息说得出多半卡在哪（cc-bus 的锁；flock 是禁档词，句子改说「锁」）" \
  "$(jq -r '.message + "\n" + .detail' < "$SANDBOX/err8.txt" 2>/dev/null | grep -cF "$(zh_frag beCcBus.timedOut.say)")" "1"

echo "[9] ★ 声明「不收输入」的命令，stdin 不关时必须秒回"
# ★ 真事故：CLI 入口原来从 `fields` **派生**「要不要读 stdin」，而 `fields` 是
#   「args 和 data 的字段名」。`bus-list` 无输入却有输出字段 ⇒ 被判成要读 stdin
#   ⇒ **挂住等一个永远不来的输入**（实测 --ping 120ms 回、--bus-list 6 秒被掐死才停）。
# ⚠ 守它的那条单测**是恒真的**（两个分支各是同一个表达式的复述），一声没吭。
#   ⇒ 这一格钉**行为**：真起进程 + 一条不关的管道。声明真不真，由它说了算。
for _c in --ping --bus-list; do
  _t0=$(date +%s%N)
  _o="$(env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" \
        "$TIMEOUT" 6 "$D" -- "$_c" < <(sleep 30) 2>/dev/null)"
  _ms=$(( ($(date +%s%N) - _t0) / 1000000 ))
  chk "★ $_c：stdin 不关也返回了（不是挂到被掐）" \
    "$([ -n "$_o" ] && [ "$_ms" -lt 5000 ] && echo yes || echo "no（${_ms}ms，输出 ${_o:-<空>}）")" "yes"
done

echo "[10] ★ 总线成员是**身份空间的子集**，不是第二套名单"
# 〔用@08-13〕逐字：「那他不应该是身份空间的子集吗? 他应该去调用身份空间啊」。
# cc-bus 的 agents.tsv 记的地址**会过期**（会话名被重用是常态）——08-13 实测后果是
# 敲门文字打进**陌生占用者**的屏幕。⇒ live/ccm_sid 由后端去问 tmux，agents.tsv
# 只回答「谁登记过 + 还剩几条没读」。
# ⚠ live 是**三态**：true / false / null。判据三格全钉——只钉前两格的话，
#   「问不到就当成不在」这种最坏的读法会溜过去（把一屋子活人判成死人）。
tmux new-session -d -s alive_cc -c /tmp 'cat'
sleep 0.4
tmux set-option -t alive_cc @ccm_sid 'sid-1234' >/dev/null 2>&1
new_bus_state() {
  printf 'alive_cc\talive_cc:0.0\tts\t1\ngone_cc\tgone_cc:0.0\tts\t2\n' > "$BUS/agents.tsv"
  : > "$BUS/inbox/alive_cc.jsonl"; : > "$BUS/inbox/gone_cc.jsonl"
}
new_bus_state
_j="$(env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" \
      "$TIMEOUT" 20 "$D" -- --bus-list </dev/null 2>/dev/null)"
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
       "$TIMEOUT" 20 "$D" -- --bus-list </dev/null 2>/dev/null)"
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
    CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" "$TIMEOUT" 20 "$D" -- --bus-send 2>/dev/null
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
    "$TIMEOUT" 20 "$D" -- --bus-send < "$SANDBOX/big.json" >/dev/null 2>"$SANDBOX/err12.txt"
chk "★ 码是 too_long（不是兜底的 failed）" "$(jq -r .code < "$SANDBOX/err12.txt" 2>/dev/null)" "too_long"
_m12="$(jq -r '.message + "\n" + .detail' < "$SANDBOX/err12.txt" 2>/dev/null)"
chk "  说了实测字节数（别让人自己去量）" "$(printf '%s' "$_m12" | grep -cF "$(zh beCcBus.deliver.tooLong reason= n=200000)")" "1"
chk "  明说不是 cc-bus 坏了（归因不许甩锅）" "$(printf '%s' "$_m12" | grep -cF "$(zh beCcBus.notRun.tooLong)")" "1"
# 对照：120KB 必须仍然发得出去（免得判据把上限收窄成"长的都不让发"）
python3 -c 'import json,sys; sys.stdout.write(json.dumps({"to":"alive_cc","text":"y"*120000}))' \
  > "$SANDBOX/mid.json"
_o12="$(env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" \
        "$TIMEOUT" 20 "$D" -- --bus-send < "$SANDBOX/mid.json" 2>/dev/null)"
chk "  对照：120KB 照样发得出去" "$(printf '%s' "$_o12" | jq -r .sent)" "true"

echo "[13] ★ 以谁的身份发：不给 from 的话，回复会掉进没人读的收件箱"
# 实测：backend 跑 cc-send 时不在任何 tmux pane 里 ⇒ cc-whoami 解不出身份 ⇒
# 收信人看到「来自 unknown」，而它给的回复方式是 `cc-send unknown "…"` ——
# **那是我们自己制造的幽灵收件箱**（同 [4] 那条打错名字的病，只是这次是工具造的）。
# ⇒ bus-send 收可选的 from，作为 CC_BUS_ID 传给子进程（cc-whoami 优先级第一条，
#   是 cc-bus **现成的契约**，不改它本体）。
_fb="$SANDBOX/frombus"; mkdir -p "$_fb"/{inbox,state,log,queue}
printf 'x_cc	x_cc:0.0	ts	1
' > "$_fb/agents.tsv"
_send_from() {
  printf '%s' "$2" | env -u TMUX -u TMUX_PANE -u CC_BUS_ID CLAUDE_CONFIG_DIR="$CLA" \
    CC_BUS_HOME="$_fb" CC_BUS_BIN_DIR="$SCRIPTS" "$TIMEOUT" 20 "$D" -- --bus-send 2>/dev/null
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

echo "[15] ★ bus-kill：收掉一个成员，且不许收错人"
# ⚠ 真跑撞出来一条：我给 cc-kill 传了 `--`（那是 cc-send 的参数形状，cc-kill **不解析旗标**）
#   ⇒ 它去杀一个名叫 `--` 的 agent（`-` 在它的白名单里，连报错都没有），
#   三种情形全回 killed:false 而真 agent 好好活着。★ 抄参数形状前先看被调方怎么解析。
tmux new-session -d -s kreal_cc -c /tmp 'sleep 300'; sleep 0.3
TMUX_PANE="$(tmux list-panes -t '=kreal_cc' -F '#{pane_id}' | head -1)" \
  CC_BUS_HOME="$BUS" bash "$SCRIPTS/cc-register" kreal_cc >/dev/null 2>&1
tmux new-session -d -s kocc_cc -c /tmp 'sleep 300'; sleep 0.3
TMUX_PANE="$(tmux list-panes -t '=kocc_cc' -F '#{pane_id}' | head -1)" \
  CC_BUS_HOME="$BUS" bash "$SCRIPTS/cc-register" kocc_cc >/dev/null 2>&1
tmux kill-session -t '=kocc_cc'; sleep 0.2
tmux new-session -d -s kocc_cc -c /tmp 'sleep 999'; sleep 0.3     # 同名的无辜占用者
_occpid="$(tmux list-panes -t '=kocc_cc' -F '#{pane_pid}' | head -1)"
_dk() {
  printf '{"id":"%s"}' "$1" | env CLAUDE_CONFIG_DIR="$CLA" \
    CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" "$TIMEOUT" 20 "$D" -- --bus-kill 2>"$SANDBOX/kerr.txt"
}
_k1="$(_dk kreal_cc)"
chk "★ 真 agent：killed=true" "$(printf '%s' "$_k1" | jq -r .killed)" "true"
chk "  会话真的没了" "$(tmux has-session -t '=kreal_cc' 2>/dev/null && echo 还在 || echo 没了)" "没了"
_k2="$(_dk kocc_cc)"
chk "★★ 名字被别人占：**不是 killed**，而是 stale_only" \
  "$(printf '%s' "$_k2" | jq -c '[.killed,.stale_only]')" "[false,true]"
chk "★★ 无辜会话还在" "$(tmux has-session -t '=kocc_cc' 2>/dev/null && echo 在 || echo 没了)" "在"
chk "★★ 无辜进程还在" "$(ps -p "$_occpid" >/dev/null 2>&1 && echo 在 || echo 被杀了)" "在"
_dk 'bad/id' >/dev/null
chk "  非法 id ⇒ bad_id（交给 cc-kill 之前后端先判形状，§47 ①）" \
  "$(jq -r .code < "$SANDBOX/kerr.txt" 2>/dev/null)" "bad_id"

echo "[16] ★ bus-spawn：本机派生走后端原语（BS1b）—— 真跑 cc-spawn，启动器是假 agent"
# ⚠ 后端起插件前先 `env_clear()`，只放行白名单 ＋ `CC_BUS_*`/`CCBUS_*` 两个前缀
#   （`plugin::invoke::INHERITED_ENV_KEYS`）⇒ `CCSPAWN_LAUNCH` / `CCM_BIN` 这类测试钩子**到不了** cc-spawn。
#   ⇒ 本格改从 **PATH** 与 **HOME**（两者都在白名单里）喂，而且 PATH 是**收窄过的**
#   （`$_SB` ＋ tmux 垫片 ＋ 系统目录，**不含**用户的 `~/.local/bin` / `~/.cc-monitor/bin`）：
#   沙箱 HOME 里没有 `~/.cc-monitor/bin/`、PATH 上也没有 `cc-monitor-backend` ⇒ cc-spawn 按查找次序
#   落到 PATH 上的 `ccm`（= 本工作树刚 build 的那一份），它再起 PATH 上的
#   `claude`（= 下面那个只记参数然后 sleep 的假 agent）。
# 分流只看 argv、不看 argv[0] ⇒ 叫 `ccm` 还是叫 `cc-monitor-backend` 走同一条规则；
#   从前的入口②（`cc-monitor-backend ccm …`）〔散文墓碑〕。两个名字同形由下面 [17] 真跑判。
_SB="$SANDBOX/spawnbin"; _SH="$SANDBOX/spawnhome"; _SW="$SANDBOX/spawnwork"
mkdir -p "$_SB" "$_SH" "$_SW/proj"
ln -sf "$D" "$_SB/ccm"
_SPATH="$_SB:$_SHIM:/usr/local/bin:/usr/bin:/bin"
printf '#!/bin/bash\nprintf "%%s\\n" "$*" > "%s/agent-args.txt"\nsleep 300\n' "$_SW" > "$_SB/claude"
chmod +x "$_SB/claude"
# 🔴🔴 **启动器是在 tmux 的 pane 里按名字找的**：pane 里 `claude` 解析到谁，决定了会不会起一个
#   用户**真的** claude（带着任务文本、用他的账号烧额度）。现打：pane 的环境取自**起会话的那个
#   tmux 客户端**（只改服务端全局 PATH，pane 里看到的仍是客户端那份；本机 `~/.local/bin/claude`
#   就是真的那个）。⇒ 两道闸：
#   ① 隔离服务端的全局 PATH / HOME 也换成沙箱的（兜底：有哪一跳没带客户端环境时仍落在沙箱里）；
#   ② **起飞前自检**：用与派生那一趟**同样的客户端环境**在同一台服务端上开一个 pane 问
#      `command -v claude`，不是 `$_SB/claude` 就**整格不跑**（记一条 FAIL），
#      绝不在没证明是假 agent 的时候派生。
tmux set-environment -g PATH "$_SPATH"
tmux set-environment -g HOME "$_SH"
# ⚠ 自检的 tmux **客户端**要带与派生那一趟同样的 PATH / HOME：新会话的环境取自**客户端**
#   （现打：只改服务端全局 PATH，pane 里看到的仍是客户端那份），而派生那一趟的客户端是 ccm，
#   它的 PATH / HOME 就是下面 `_ds` 交给后端的那一份（后端按白名单原样传下去）。
env HOME="$_SH" PATH="$_SPATH" \
  tmux new-session -d -s spawncanary -c /tmp "command -v claude > '$_SW/which.txt'; sleep 30"
for _i in 1 2 3 4 5 6 7 8 9 10; do [ -s "$_SW/which.txt" ] && break; sleep 0.3; done
tmux kill-session -t '=spawncanary' 2>/dev/null || true
_which="$(cat "$_SW/which.txt" 2>/dev/null)"
chk "起飞前：隔离服务端的 pane 里 claude 解析到假 agent" "$_which" "$_SB/claude"
_ds() {
  printf '%s' "$1" | env HOME="$_SH" PATH="$_SPATH" CLAUDE_CONFIG_DIR="$CLA" \
    CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" CC_BUS_SCRIPTS="$SCRIPTS" CC_BUS_TIMEOUT_SECS=40 \
    "$TIMEOUT" 60 "$D" -- --bus-spawn 2>"$SANDBOX/serr.txt"
}
if [ "$_which" != "$_SB/claude" ]; then
  echo "  !! 自检没过 —— 本格不派生（宁可少判，也不在用户的 claude 上起会话）"
else
_s1="$(_ds "{\"tool\":\"claude\",\"dir\":\"$_SW/proj\",\"task\":\"跑一遍门禁\",\"base\":true}")"
chk "★ spawned=true" "$(printf '%s' "$_s1" | jq -r .spawned 2>/dev/null)" "true"
chk "★ 回值里认出了新会话的 id" "$(printf '%s' "$_s1" | jq -r .id 2>/dev/null)" "proj_cc"
chk "  会话真的起了（隔离 socket 上）" "$(tmux has-session -t '=proj_cc' 2>/dev/null && echo 在 || echo 没有)" "在"
for _i in 1 2 3 4 5 6 7 8 9 10; do [ -s "$_SW/agent-args.txt" ] && break; sleep 0.3; done
chk "  初始任务作为参数送到了启动器" "$(tr -d '\n' < "$_SW/agent-args.txt" 2>/dev/null)" "跑一遍门禁"
chk "  登记进了总线名册" "$(cut -f1 "$BUS/agents.tsv" 2>/dev/null | grep -cx proj_cc || true)" "1"
_ds '{"tool":"claude","dir":"/tmp"}' >/dev/null
chk "★ 账号不表态 ⇒ bad_args（不替用户选默认号）" "$(jq -r .code < "$SANDBOX/serr.txt" 2>/dev/null)" "bad_args"
_ds "{\"tool\":\"not-an-agent\",\"dir\":\"$_SW/proj\",\"base\":true}" >/dev/null
chk "★ 不认的 tool ⇒ 由 cc-spawn 自己拒成 bad_args（后端不写第二份白名单）" \
  "$(jq -r .code < "$SANDBOX/serr.txt" 2>/dev/null)" "bad_args"
chk "  …而且没起出第二个会话" "$(tmux has-session -t '=proj_cc-2' 2>/dev/null && echo 起了 || echo 没起)" "没起"
fi
echo "[17] ★ ccm 在 pane 里重起自己：名叫 ccm 与名叫 cc-monitor-backend 同形（CC1；分流不看 argv0）"
# 〔BS1b 现打〕从前走入口②时，pane 里那一跳丢了 `ccm` 这个词 ⇒ 空 bash 而 cc-spawn 照报 rc=0（假成功）。
# 入口②删了、分流不看名字 ⇒ 本格改钉「换个文件名结果逐字节一样」：两个名字各真跑一次
#   cc-spawn，把 pane 里真正执行的那条 argv 抓出来比。
# ⚠ **两个入口是同一份 wrapper 换个名字**：它把「自己被怎么叫」（`$0` ＋ argv）按 NUL 逐字落一个文件，
#   再用**同一个 argv0**（`exec -a`）交给真二进制 ⇒ ccm 看到的入口就是那个名字，而我们看到的是
#   **真实产物**（cc-spawn 怎么叫它的、pane 里怎么叫它的），不是任何一个拼串函数的自述。
# ⚠ 启动器用**绝对路径**的假 agent（`CCSPAWN_LAUNCH`）⇒ pane 里不按名字找，碰不到用户真的 claude。
_EW="$SANDBOX/entrywork"
mkdir -p "$_EW/e1" "$_EW/e2" "$_EW/e3" "$_EW/bin" "$_EW/proj" "$_EW/bus"/{inbox,state,log,queue}
for _e in e1/ccm e2/cc-monitor-backend e3/ccm; do
  printf '#!/bin/bash\nprintf "%%s\\0" "$0" "$@" > "%s/call.$$"\nexec -a "$0" "%s" "$@"\n' \
    "$_EW/$(dirname "$_e")" "$D" > "$_EW/$_e"
  chmod +x "$_EW/$_e"
done
printf '#!/bin/bash\nprintf "%%s\\n" "$*" > "$PWD/agent-args.$PPID"\nexec sleep 300\n' > "$_EW/bin/fake-agent"
chmod +x "$_EW/bin/fake-agent"
_sp() {  # $1.. = 额外的 VAR=值；目录与任务两个入口一样（那样内层参数才可能逐字相等）
  env -u TMUX -u TMUX_PANE -u CC_BUS_ID HOME="$_EW" CC_BUS_HOME="$_EW/bus" \
      CC_BUS_SCRIPTS="$SCRIPTS" CCSPAWN_LAUNCH="$_EW/bin/fake-agent" \
      "$@" \
      "$TIMEOUT" 60 bash "$SCRIPTS/cc-spawn" --base "$_EW/proj" "任务乙"
}
# 判据：每个入口的调用按内容分三类（探针 `--ccm-probe` / 建会话那趟 `--detach` / 其余 = pane 里那一跳；
#   带 `--ccm-print` 的是自检那一趟，单列）。「入口前缀」**从 cc-spawn 那一趟的真实 argv 里现取**：
#   两个入口的建会话 argv 取最长公共后缀 = cc-spawn 交给 ccm 的那串，余下的前段就是各自的入口前缀
#   ⇒ 判据里**不写死** `ccm` 这个词。要求：pane 那一跳 = 各自入口前缀 ＋ 同一串参数（逐字节）。
_entry_judge() {
  python3 - "$_EW/e1" "$_EW/e2" <<'PY'
import glob, sys
def calls(d):
    return [open(f, 'rb').read().split(b'\0')[:-1] for f in sorted(glob.glob(d + '/call.*'))]
def split(cs):
    outer = [c for c in cs if b'--detach' in c]
    rest = [c for c in cs if b'--ccm-probe' not in c and b'--detach' not in c]
    return outer, [c for c in rest if b'--ccm-print' not in c]  # 自检那一趟带的是 --ccm-print
(o1, p1), (o2, p2) = split(calls(sys.argv[1])), split(calls(sys.argv[2]))
if len(o1) != 1 or len(o2) != 1:
    print(f"建会话那趟不是恰好各一次：{len(o1)}/{len(o2)}"); sys.exit()
if len(p1) != 1 or len(p2) != 1:
    print(f"pane 里那一跳不是恰好各一次：{len(p1)}/{len(p2)}"); sys.exit()
o1, o2, p1, p2 = o1[0], o2[0], p1[0], p2[0]
k = 0
while k < min(len(o1), len(o2)) and o1[-1 - k] == o2[-1 - k]:
    k += 1
pre1, pre2 = o1[:len(o1) - k], o2[:len(o2) - k]
if p1[:len(pre1)] != pre1 or p2[:len(pre2)] != pre2:
    print(f"pane 那一跳没走 cc-spawn 叫它的那个入口：入口前缀 {pre1!r}/{pre2!r}，pane 实得 {p1!r}/{p2!r}"); sys.exit()
t1, t2 = p1[len(pre1):], p2[len(pre2):]
print("同形" if t1 == t2 else f"参数不同：{t1!r} ≠ {t2!r}")
PY
}
_wait_agent() {  # 等到第 $1 个假 agent 真的起来（坏了永远等不到，给 3 秒上限）
  for _i in $(seq 1 30); do
    [ "$(find "$_EW/proj" -maxdepth 1 -name 'agent-args.*' | wc -l)" -ge "$1" ] && break; sleep 0.1
  done
}
_o1="$(_sp CCM_BIN="$_EW/e1/ccm" 2>"$_EW/err1.txt")"; _r1=$?
_wait_agent 1
_o2="$(_sp CCM_BIN="$_EW/e2/cc-monitor-backend" 2>"$_EW/err2.txt")"; _r2=$?
_wait_agent 2
_n1="$(printf '%s\n' "$_o1" | sed -n 's/^已 spawn: \([^ ]*\).*/\1/p')"
_n2="$(printf '%s\n' "$_o2" | sed -n 's/^已 spawn: \([^ ]*\).*/\1/p')"
chk "  对照：名叫 ccm cc-spawn rc=0" "$_r1" "0"
chk "★ 名叫 cc-monitor-backend cc-spawn rc=0" "$_r2" "0"
chk "★★ pane 里那一跳：两个名字 = 各自入口前缀 ＋ **逐字节同一串**参数" "$(_entry_judge)" "同形"
chk "★ 两个名字的 pane 都真的起到了 agent（任务原样送达）" \
  "$(cat "$_EW/proj"/agent-args.* 2>/dev/null | grep -cx '任务乙')" "2"
chk "  两个名字都登记上了总线（各一条）" \
  "$(cut -f1 "$_EW/bus/agents.tsv" 2>/dev/null | grep -cxF -e "${_n1:-<无>}" -e "${_n2:-<无>}")" "2"

echo "[17b] ★ pane 里起的东西当场报参数错误 ⇒ cc-spawn **不许**报成功、**不许**登记（CC1）"
# 造一个「不认这套参数」的入口（旧副本的形状：认不得就 exit 2），让 ccm 在 pane 里叫的是它。
# 从前经环境变量 `CCM_SELF` 指过去；那个变量删了，内层载荷
#   只认「这个进程自己被怎么叫的」⇒ 改成**真的**那样叫它：一个入口脚本 `exec -a <旧副本路径>` 真身，
#   真身跑起来 `argv[0]` 就是旧副本的路径（basename 仍是 `ccm` ⇒ 入口①）——正是「PATH 上那个 `ccm`
#   是一份不认新参数的旧副本」的现场形状。这一格钉的「假成功看得见」一个字没变。
mkdir -p "$_EW/old"
printf '#!/bin/bash\necho "ccm(旧副本): 未知选项: $1" >&2\nexit 2\n' > "$_EW/old/ccm"
chmod +x "$_EW/old/ccm"
mkdir -p "$_EW/e4"
printf '#!/bin/bash\nexec -a "%s" "%s" "$@"\n' "$_EW/old/ccm" "$D" > "$_EW/e4/ccm"
chmod +x "$_EW/e4/ccm"
mkdir -p "$_EW/bad"
_ob="$(env -u TMUX -u TMUX_PANE -u CC_BUS_ID HOME="$_EW" CC_BUS_HOME="$_EW/bus" \
      CC_BUS_SCRIPTS="$SCRIPTS" CCSPAWN_LAUNCH="$_EW/bin/fake-agent" \
      CCM_BIN="$_EW/e4/ccm" \
      "$TIMEOUT" 60 bash "$SCRIPTS/cc-spawn" --base "$_EW/bad" "任务丙" 2>"$_EW/errb.txt")"; _rb=$?
for _i in $(seq 1 30); do tmux capture-pane -p -t '=bad_cc:' 2>/dev/null | grep -q '未知选项' && break; sleep 0.1; done
chk "  前提：pane 里那一跳真的当场报了参数错误（不是本格没打到）" \
  "$(tmux capture-pane -p -t '=bad_cc:' 2>/dev/null | grep -c '未知选项')" "1"
chk "★ cc-spawn 退出码非 0（不是假成功）" "$([ "$_rb" -ne 0 ] && echo 非0 || echo "0（假成功）")" "非0"
chk "★ 没有登记上总线" "$(cut -f1 "$_EW/bus/agents.tsv" 2>/dev/null | grep -cx 'bad_cc' || true)" "0"
chk "  也没有报「已 spawn」" "$(printf '%s\n' "$_ob" | grep -c '^已 spawn: ' || true)" "0"

echo "[18] ★ bus-broadcast：广播这个组合收进后端（C4e）—— 只发在线的、三个数分开、不发给自己"
# 此前广播是 monitor 里的组合（列名单 ＋ 逐个发）；界面改经通道直接说后端之后收进后端。
# 名册：一个活着（隔离 socket 上真有会话）· 一个会话没了 · 一个是 `from` 自己（也活着）。
tmux new-session -d -s bcast_cc -c /tmp 'cat' 2>/dev/null
tmux new-session -d -s bcme_cc -c /tmp 'cat' 2>/dev/null
sleep 0.3
printf 'bcast_cc\tbcast_cc:0.0\tts\t1\nbgone_cc\tbgone_cc:0.0\tts\t2\nbcme_cc\tbcme_cc:0.0\tts\t3\n' > "$BUS/agents.tsv"
: > "$BUS/inbox/bcast_cc.jsonl"; : > "$BUS/inbox/bgone_cc.jsonl"; : > "$BUS/inbox/bcme_cc.jsonl"
_b="$(printf '{"text":"来自后端的广播","from":"bcme_cc"}' | env CLAUDE_CONFIG_DIR="$CLA" \
      CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" "$TIMEOUT" 30 "$D" -- --bus-broadcast 2>"$SANDBOX/berr.txt")"
chk "★ 只投给在线的那一个（sent=1）" "$(printf '%s' "$_b" | jq -r .sent 2>/dev/null)" "1"
chk "★ 会话没了的那一个计进 skipped_offline（不是悄悄丢掉）" "$(printf '%s' "$_b" | jq -r .skipped_offline 2>/dev/null)" "1"
chk "  身份空间答得上 ⇒ liveness_unknown=false" "$(printf '%s' "$_b" | jq -r .liveness_unknown 2>/dev/null)" "false"
chk "  没有失败的" "$(printf '%s' "$_b" | jq -c .failed 2>/dev/null)" "[]"
chk "★ 在线那一个的收件箱真的收到了" "$(grep -c '来自后端的广播' "$BUS/inbox/bcast_cc.jsonl" 2>/dev/null || true)" "1"
chk "★ 会话没了的那一个没被投（不再造幽灵收件箱）" "$(grep -c '来自后端的广播' "$BUS/inbox/bgone_cc.jsonl" 2>/dev/null || true)" "0"
chk "★ 不发给自己（from）" "$(grep -c '来自后端的广播' "$BUS/inbox/bcme_cc.jsonl" 2>/dev/null || true)" "0"
printf '{"text":"   "}' | env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" \
  "$TIMEOUT" 30 "$D" -- --bus-broadcast >/dev/null 2>"$SANDBOX/berr.txt"
chk "★ 空正文 ⇒ bad_args（空广播不是缺省）" "$(jq -r .code < "$SANDBOX/berr.txt" 2>/dev/null)" "bad_args"
tmux kill-session -t '=bcast_cc' 2>/dev/null || true
tmux kill-session -t '=bcme_cc' 2>/dev/null || true

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
for _i in $(seq 1 100); do [ -f "$_u8d/done" ] && break; sleep 0.1; done
_u8want="$(printf '%s' "$_u8name" | LC_ALL=C sed 's/[^A-Za-z0-9_-]/-/g; s/^-*//; s/-*$//')"
chk "  反向正控：同台架上不带旗的 display-message 确实被改写（台架真是非 UTF-8 客户端）" \
  "$([ "$(cat "$_u8d/raw" 2>/dev/null)" != "$_u8name" ] && [ -f "$_u8d/done" ] && echo yes || echo no)" "yes"
chk "★ cc-register 记下的登记地址是那个中文会话名（不是 _ 改写过的）" \
  "$(awk -F'\t' -v id="$_u8want" '$1==id{print $2}' "$_u8bus/agents.tsv" 2>/dev/null)" "$_u8name:0.0"
chk "★ cc-whoami（TMUX_PANE 那一条）认出的身份" "$(cat "$_u8d/who1" 2>/dev/null)" "$_u8want"
chk "★ cc-whoami（按 \$TMUX 反查会话 id 那一条）认出的身份" "$(cat "$_u8d/who2" 2>/dev/null)" "$_u8want"
chk "★ cc-whoami（沿进程树找 pane 那一条）认出的身份" "$(cat "$_u8d/who3" 2>/dev/null)" "$_u8want"
tmux kill-session -t "=$_u8name" 2>/dev/null || true

echo "[SH1-b] ★ 驾驶舱读面 —— 后端转调 cc-bus 的机器可读读命令（登记时间 · 派生时间 · 坏行数 · 收件箱只看尾巴）"
# 台架写一份**脏**的名册与台账（只采结构：`--help` 行 · 缺字段行 · 真空行 · 只有 TAB 的行 · 任务里带 TAB），
# 坏行数期望由这份夹具手算，不从被测输出里取。
printf 'alpha_cc\talpha_cc:0.0\t2026-01-01T00:00:00+00:00\t111\n--help\tx:0.0\tts\t1\nshort\tonly\n\n\t\t\t\nbeta_cc\tbeta_cc:0.0\tts2\t\n' > "$BUS/agents.tsv"
printf 'gamma_cc\t/d\tts3\ttask\twith tab\nbroken\n' > "$BUS/spawned.tsv"
printf '{"from":"peer_cc","ts":"t1","text":"占位一"}\nnot json\n{"from":"peer_cc","ts":"t2","text":"占位二"}\n' > "$BUS/inbox/alpha_cc.jsonl"
echo 1 > "$BUS/state/alpha_cc.pos"
out="$(d --bus-state </dev/null)"
chk "名册两条好行（--help 被本侧 id 判定拒掉、计进坏行）" "$(printf '%s' "$out" | jq -c '[.agents[].id]')" '["alpha_cc","beta_cc"]'
chk "★ 登记时间回来了" "$(printf '%s' "$out" | jq -r '.agents[0].registered_at')" "2026-01-01T00:00:00+00:00"
chk "  待读数照旧（3 行 − 已读 1）" "$(printf '%s' "$out" | jq -r '.agents[0].unread')" "2"
chk "★ 派生时间回来了、任务里的 TAB 没被截" "$(printf '%s' "$out" | jq -c '.spawned[0] | [.spawned_at, .task]')" '["ts3","task\twith tab"]'
chk "★ 坏行数：名册 short · 只有 TAB · --help 三行 ＋ 台账 broken 一行 = 4（真空行不算）" "$(printf '%s' "$out" | jq -r '.skipped')" "4"
_pos_before="$(cat "$BUS/state/alpha_cc.pos")"; _state_before="$(ls "$BUS/state" | tr '\n' ' ')"
out="$(printf '{"id":"alpha_cc","lines":2}' | d --bus-inbox)"
chk "★ 收件箱只看尾巴（末 2 行：一条坏、一条好）" "$(printf '%s' "$out" | jq -c '[.messages[].text, .skipped]')" '["占位二",1]'
chk "★ 读收件箱不推已读位置" "$(cat "$BUS/state/alpha_cc.pos")" "$_pos_before"
chk "  也不在 state/ 里写任何东西" "$(ls "$BUS/state" | tr '\n' ' ')" "$_state_before"
printf '{"id":"--help"}' | d --bus-inbox >/dev/null
chk "★ --help 当收件箱 id ⇒ bad_id（交给 cc-log 之前拒）" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "bad_id"
# 老 cc-bus：`cc-list` / `cc-agents` 不认 --tsv（照打人读表）⇒ 明说要重新部署，不解成一份空名单。
_old="$SANDBOX/oldbus"; mkdir -p "$_old"
printf '#!/bin/sh\necho "ID           TMUX               待读"\n' > "$_old/cc-list"; cp "$_old/cc-list" "$_old/cc-agents"; chmod +x "$_old/cc-list" "$_old/cc-agents"
env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$_old" CC_BUS_ID=probe_cc "$TIMEOUT" 20 "$D" -- --bus-state </dev/null >/dev/null 2>"$SANDBOX/err.txt"
# 那一句按文案键认（beCcBus.read.tooOld 最长的那一段固定字），不钉原文。
_TOOOLD="$(zh_frag beCcBus.read.tooOld)"
chk "★ 老 cc-bus ⇒ failed 且说要重新部署（beCcBus.read.tooOld）" "$(jq -r --arg w "$_TOOOLD" '.code + " " + ((.message + "\n" + .detail) | contains($w) | tostring)' < "$SANDBOX/err.txt" 2>/dev/null)" "failed true"
rm -f "$BUS/agents.tsv" "$BUS/spawned.tsv"

echo "[SH1-c] ★ D-g：monitor 杀会话成功 ⇒ 对登记在那个会话 pane 上的 id 调 cc-kill（认 pane pid，不按会话名猜）"
: > "$BUS/agents.tsv"
_dg="$SANDBOX/dg"; mkdir -p "$_dg"
tmux new-session -d -s dg-cc -c /tmp env CC_BUS_HOME="$BUS" SCRIPTS="$SCRIPTS" OUT="$_dg" sh -c '
  "$SCRIPTS/cc-register" dg_cc >/dev/null 2>&1; touch "$OUT/a"; exec sleep 60' 2>/dev/null
tmux new-session -d -s dgother-cc -c /tmp env CC_BUS_HOME="$BUS" SCRIPTS="$SCRIPTS" OUT="$_dg" sh -c '
  "$SCRIPTS/cc-register" dgother_cc >/dev/null 2>&1; touch "$OUT/b"; exec sleep 60' 2>/dev/null
for _i in $(seq 1 100); do [ -f "$_dg/a" ] && [ -f "$_dg/b" ] && break; sleep 0.1; done
printf '{"from":"x","text":"占位"}\n' >> "$BUS/inbox/dg_cc.jsonl"
chk "  台架：两个会话都登记上了（带 pane pid）" "$(awk -F'\t' '$4!=""{n++} END{print n+0}' "$BUS/agents.tsv")" "2"
out="$(printf '{"name":"dg-cc"}' | d --kill)"
chk "杀会话本身照旧成功" "$(printf '%s' "$out" | jq -c '[.session, .killed]')" '["dg-cc",true]'
# 注销的结局进成品的 `bus` 那一格：注销了谁 · 谁没注销成 · 名册读得到。
chk "★ 成品 bus 那一格说出注销了 dg_cc（没有失败、名册读得到）" "$(printf '%s' "$out" | jq -c '[.bus.removed, .bus.failed, .bus.unread]')" '[["dg_cc"],[],null]'
chk "★ 登记在被杀会话上的 dg_cc 从名册里没了" "$(awk -F'\t' '$1=="dg_cc"' "$BUS/agents.tsv" | wc -l | tr -d ' ')" "0"
chk "★ 它的收件箱也清了（cc-bus「收掉成员」的全套）" "$([ -e "$BUS/inbox/dg_cc.jsonl" ] && echo 在 || echo 没了)" "没了"
chk "★ 别的会话上登记的 dgother_cc 原样在" "$(awk -F'\t' '$1=="dgother_cc"' "$BUS/agents.tsv" | wc -l | tr -d ' ')" "1"
tmux kill-session -t '=dgother-cc' 2>/dev/null || true

echo "[CLI2] ★ CLI 面的入参不再隐含「调用方写得了 stdin」：argv 载荷口 · 开着不写不挂 · --stdin-line 任意位置 · 超大回码"
# 一份沙箱里的会话记录（夹具文字，不是真正文）。
mkdir -p "$CLA/projects/-cli2"
J="$CLA/projects/-cli2/c2000000-0000-4000-8000-000000000001.jsonl"
printf '%s\n' \
  '{"type":"user","uuid":"u1","parentUuid":null,"sessionId":"c2000000-0000-4000-8000-000000000001","timestamp":"2026-10-09T00:00:00Z","cwd":"/w","message":{"role":"user","content":"夹具一句"}}' \
  '{"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"c2000000-0000-4000-8000-000000000001","timestamp":"2026-10-09T00:00:01Z","message":{"role":"assistant","model":"m","content":[{"type":"text","text":"夹具回话"}]}}' > "$J"
c2ms() { local t=${EPOCHREALTIME/./}; echo $(( t / 1000 )); }
for _cmd in history-read history-facts; do
  _j="$(jq -cn --arg p "$J" '{path:$p}')"
  printf '%s' "$_j" | d "--$_cmd" >"$SANDBOX/c2-in.txt"; _rc_in=$?; cp "$SANDBOX/err.txt" "$SANDBOX/c2-in.err"
  _b="$(printf '%s' "$_j" | base64 -w0)"
  d "--$_cmd" --args-b64 "$_b" < <(sleep 30) >"$SANDBOX/c2-av.txt"; _rc_av=$?
  chk "★ $_cmd：stdin 那一形答出来了（退出 0）" "$_rc_in" "0"
  chk "★ $_cmd：argv 载荷口与 stdin 逐字一样（stdout）" "$(cmp -s "$SANDBOX/c2-in.txt" "$SANDBOX/c2-av.txt" && echo 同 || echo 不同)" "同"
  chk "  $_cmd：退出码 · stderr 也一样" "$_rc_av|$(cat "$SANDBOX/err.txt")" "$_rc_in|$(cat "$SANDBOX/c2-in.err")"
  cp "$SANDBOX/c2-in.txt" "$SANDBOX/c2-$_cmd.txt"
done
# 成品行是通用记录（`record`：`t` ∈ said · reply …，`id` 是那一行自己的身份）；空包 / 换了格名读不到 ⇒ 这里拿到的是 `null:null`。
chk "  history-read 出的是成品行（不是空包）" "$(jq -r '[.rows[] | "\(.record.t):\(.record.id)"] | join(",")' < "$SANDBOX/c2-history-read.txt")" "said:u1,reply:a1"
# stdin 开着、一直不写：两种读法都立即回 no_input，不挂到被掐。
for _extra in "" --stdin-line; do
  _t0=$(c2ms); d --history-read $_extra < <(sleep 30) >/dev/null; _rc=$?; _dt=$(( $(c2ms) - _t0 ))
  chk "★ 开着不写（${_extra:-默认}）：立即回码（不是挂到被掐）" "$([ "$_rc" -eq 2 ] && [ "$_dt" -lt 5000 ] && echo 是 || echo "否 rc=$_rc ${_dt}ms")" "是"
  chk "  码是 no_input" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "no_input"
done
# stdin 是 EOF：照旧当 {}，由命令自己说缺什么。
d --history-read </dev/null >/dev/null
chk "★ stdin 是 EOF：命令自己回缺入参（bad_args）" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "bad_args"
# --stdin-line 不在第二格也认：读到换行就动手，stdin 后面不关也不挂。
_t0=$(c2ms); { printf '%s\n' "$(jq -cn --arg p "$J" '{path:$p}')"; sleep 30; } | d --history-read --summaryOnly-ignored --stdin-line >"$SANDBOX/c2-sl.txt" &
_pid=$!; _ok=否; for _ in $(seq 1 50); do [ -s "$SANDBOX/c2-sl.txt" ] && { _ok=是; break; }; sleep 0.1; done; kill "$_pid" 2>/dev/null; wait "$_pid" 2>/dev/null
chk "★ --stdin-line 在后面也认（5 s 内答出、不等 EOF）" "$_ok" "是"
# 两个口都给 ⇒ bad_args。
d --history-read --args-b64 e30= --stdin-line < <(sleep 30) >/dev/null
chk "★ 两个口都给：bad_args" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "bad_args"
# 超大：stdin 一形（1 MiB ＋ 1）· argv 一形（系统放得进来、本后端嫌大的那一段）。
head -c 1048577 /dev/zero | tr '\0' ' ' | d --history-read >/dev/null
chk "★ stdin 超 1 MiB：args_too_large（不截断、不挂）" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "args_too_large"
d --history-read --args-b64 "$(head -c 131070 /dev/zero | tr '\0' A)" < <(sleep 30) >/dev/null
chk "★ argv 值超本后端的上限：args_too_large" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "args_too_large"
# 超过系统单个参数的上限（Linux 131072 含 NUL）：起不来，错在 exec 那一层 —— 也是立即失败、不挂。
_t0=$(c2ms); d --history-read --args-b64 "$(head -c 140000 /dev/zero | tr '\0' A)" < <(sleep 30) >/dev/null 2>&1; _rc=$?; _dt=$(( $(c2ms) - _t0 ))
chk "★ argv 超系统上限：exec 那一层立即失败（非 0、不挂）" "$([ "$_rc" -ne 0 ] && [ "$_rc" -ne 124 ] && [ "$_dt" -lt 5000 ] && echo 是 || echo "否 rc=$_rc ${_dt}ms")" "是"

# `--resolve`（与仓外 aterm 冻结的那一条）同一套口：argv 与 stdin 逐字一样 · 开着不写回 no_input（码全集钉在金样里）。
_rq='{"sessionId":"s1"}'
printf '%s' "$_rq" | d --resolve >"$SANDBOX/c2-rs-in.txt"; _rc_in=$?
d --resolve --args-b64 "$(printf '%s' "$_rq" | base64 -w0)" < <(sleep 30) >"$SANDBOX/c2-rs-av.txt"; _rc_av=$?
chk "★ --resolve：argv 载荷口与 stdin 逐字一样（退出 0）" "$_rc_in|$_rc_av|$(cmp -s "$SANDBOX/c2-rs-in.txt" "$SANDBOX/c2-rs-av.txt" && echo 同 || echo 不同)" "0|0|同"
chk "  --resolve 成品是那条恢复命令" "$(jq -r .command < "$SANDBOX/c2-rs-av.txt" 2>/dev/null)" "claude --resume s1"
_t0=$(c2ms); d --resolve < <(sleep 30) >/dev/null; _rc=$?; _dt=$(( $(c2ms) - _t0 ))
chk "★ --resolve 开着不写：立即回 no_input" "$([ "$_rc" -eq 2 ] && [ "$_dt" -lt 5000 ] && echo 是 || echo "否 rc=$_rc ${_dt}ms")|$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "是|no_input"

"$REALTMUX" -L "$_SOCK" kill-server 2>/dev/null || true

echo
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ] || exit 1
echo "===== P4f 验收全部通过 ====="
