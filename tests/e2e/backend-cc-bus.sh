#!/usr/bin/env bash
# `P4f`：backend 的 cc-bus 命令 —— `--bus-list` / `--bus-inbox` **真跑**；发信 `cc-send` 问 `--bus-envelope` 核信封、再自己投（A13-2）。
#
# ## 为什么必须有真跑这一层
#
# 单测把 `parse_roster_tsv` / `classify_exit` / 信封校验那些纯函数钉住了，但它们**证明不了命令调得动**：
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
REALTMUX="$(command -v tmux)" || { echo "需要 tmux（[13] 的窗格诱饵 · [SH1-c] 杀会话要它）"; exit 1; }

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
      CCBUS_POLICY_MODE="${POLICY:-off}" \
      "$TIMEOUT" "$WALL_CEILING_S" "$D" -- "$@" 2>"$SANDBOX/err.txt"
}

echo "===== P4f：backend 的 cc-bus 基础命令 ====="

echo "[1] --bus-list：空总线"
out="$(d --bus-list </dev/null)"
chk "空总线回空数组（不是报错）" "$(printf '%s' "$out" | jq -c '.agents')" "[]"

echo "[2] --bus-list：名单的行是收件箱（会话 id · 待读），标题与在不在由后端判"
# 台架**直接写**收件箱：它扮演的是"cc-bus 的状态"。
# ⚠ 被测的 backend **不许**读这个格式（`no_cc_bus_data_layout_leaks_into_the_backend` 钉着），
#   台架知道它是可以的 —— 格式变了这套件会红，那正是我们要的信号。
# 两条会话：SR 这台有记录（沙箱里的一份两行记录，正文是占位）、没有进程持着它；SU 这台没有记录。
LB="$SANDBOX/lbus"; mkdir -p "$LB"/{inbox,state,log}
SR="a1000000-0000-4000-8000-000000000001"; SU="a2000000-0000-4000-8000-000000000002"
mkdir -p "$CLA/projects/-lw"
printf '%s\n' "{\"type\":\"user\",\"uuid\":\"u1\",\"parentUuid\":null,\"sessionId\":\"$SR\",\"timestamp\":\"2026-10-26T00:00:00Z\",\"cwd\":\"/lw\",\"message\":{\"role\":\"user\",\"content\":\"占位的第一句\"}}" > "$CLA/projects/-lw/$SR.jsonl"
printf 'a\nb\n' > "$LB/inbox/$SR.jsonl"; echo 0 > "$LB/state/$SR.pos"
printf 'a\n' > "$LB/inbox/$SU.jsonl"
out="$(env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$LB" CC_BUS_BIN_DIR="$SCRIPTS" "$TIMEOUT" "$WALL_CEILING_S" "$D" -- --bus-list </dev/null 2>/dev/null)"
chk "★ 名单就是那两个收件箱（不读登记表）" "$(printf '%s' "$out" | jq -c '[.agents[].id] | sort')" "$(jq -cn --arg a "$SR" --arg b "$SU" '[$a,$b] | sort')"
chk "  待读数（真数出来的，不是猜的）" "$(printf '%s' "$out" | jq -r --arg s "$SR" '.agents[] | select(.id==$s) | .unread')" "2"
chk "★ 有记录的那条：标题是会话自己的（不是会话 id）· 没人持着它 ⇒ live=false" \
  "$(printf '%s' "$out" | jq -r --arg s "$SR" '.agents[] | select(.id==$s) | [(.title != $s and .title != ""), .live] | map(tostring) | join("|")')" "true|false"
chk "★ 这台没记录的那条：判不了 ⇒ live=null（不是 false）· 标题照实就是会话 id" \
  "$(printf '%s' "$out" | jq -r --arg s "$SU" '.agents[] | select(.id==$s) | [.title, .live] | map(tostring) | join("|")')" "$SU|null"

# ===== 发信：cc-send 问后端 `bus-envelope` 核信封（唯一一处），再自己走管线投递 =====
# 台架的 ccm：一份记账的桩 —— `--bus-envelope` 原样转给被测的后端二进制（真核），看屏 / 说一句只记下来、答「送到了」。
#   `--session-self`（「我是哪条会话」）答 `$SELF_SID`；它空着 ⇒ 照后端的形状拒成 `not_in_session`。
#   不起真 claude、不碰真会话：敲门那一下到桩为止，桩里看得见 cc-bus 要后端做什么（`idle_only` 带没带）。
SA="aaaaaaaa-1111-2222-3333-444444444444"   # 发件人（会话 id 的形状，占位）
SB="bbbbbbbb-1111-2222-3333-444444444444"   # 收件人
FK="$SANDBOX/fakeccm"; mkdir -p "$FK"; CALLS="$SANDBOX/ccm-calls.txt"; SAYS="$SANDBOX/ccm-says.txt"
cat > "$FK/ccm" <<'EOF'
#!/bin/bash
printf '%s\n' "$*" >> "$CALLS"
case " $* " in
  *" --bus-envelope "*) exec "$D" "$@" ;;
  *" --session-self "*)
    if [ -n "${SELF_SID:-}" ]; then printf '{"sid":"%s","groups":[],"looked":"this_machine"}\n' "$SELF_SID"
    else printf '{"code":"not_in_session","message":"no session above","detail":""}\n' >&2; exit 2; fi ;;
  *" --bus-list "*) exec "$D" "$@" ;;
  *" --terminal-preview "*) cat >/dev/null; printf '{"screen":"fp-0"}\n' ;;
  *" --session-say "*) cat >> "$SAYS"; printf '\n'; printf '{"result":"delivered"}\n' ;;
  *) echo "fake ccm: 没想到会被问 $*" >&2; exit 9 ;;
esac
EOF
chmod +x "$FK/ccm"
# 一次发信 = 一个干净的 env（逐次给全）。FROM 缺 ⇒ SA。
snd() {
  env CCM_BIN="$FK/ccm" D="$D" CALLS="$CALLS" SAYS="$SAYS" CLAUDE_CONFIG_DIR="$CLA" HOME="$NOHOME" \
      CC_BUS_HOME="$BUS" SELF_SID="${FROM-$SA}" CCBUS_POLICY_MODE="${POLICY:-off}" \
      "$TIMEOUT" "$WALL_CEILING_S" "$SCRIPTS/cc-send" "$@" 2>"$SANDBOX/snd.err"
}
lines_of() { if [ -f "$BUS/inbox/$1.jsonl" ]; then wc -l < "$BUS/inbox/$1.jsonl" | tr -d ' '; else echo 0; fi; }

echo "[3] cc-send：信封过后端、落进收件人的收件箱；缺省二档、不敲门"
: > "$CALLS"; : > "$SAYS"
snd "$SB" --act inform --title "占位标题" "占位正文" >/dev/null; _rc=$?
chk "退出 0" "$_rc" "0"
chk "★ 收件箱（按会话 id）多了一行" "$(lines_of "$SB")" "1"
_e="$(tail -1 "$BUS/inbox/$SB.jsonl")"
chk "  信封各格：from · to · tier 缺省 2 · act · title · 正文" \
  "$(printf '%s' "$_e" | jq -c '[.from, .to, .tier, .act, .title, .text]')" "[\"$SA\",\"$SB\",2,\"inform\",\"占位标题\",\"占位正文\"]"
chk "  id 与时间是后端盖的（m 打头 · UTC）" \
  "$(printf '%s' "$_e" | jq -r '(.id | startswith("m")) and (.ts | endswith("Z"))')" "true"
chk "★ 二档不敲门：一次看屏 / 说一句都没问" "$(grep -cE -- '--terminal-preview|--session-say' "$CALLS")" "0"
chk "  信封确实是问后端核的（问过一次 bus-envelope）" "$(grep -c -- '--bus-envelope' "$CALLS")" "1"

echo "[4] ★ 信封不合格当场拒（rc=2）、一个字节都不写；那一句是后端文案表里的那一句"
_before="$(lines_of "$SB")"
rej() {  # <期望的那一句的文案键> <cc-send 参数…>
  local key="$1"; shift
  snd "$@" >/dev/null; local rc=$?
  chk "  [$key] 退出码 2" "$rc" "2"
  chk "  [$key] stderr 里是那一句" "$(grep -cF "$(zh "$key")" "$SANDBOX/snd.err")" "1"
}
rej beCcBus.envelope.badTitle "$SB" --act ask "没给标题"
rej beCcBus.envelope.badTitle "$SB" --act ask --title "$(printf '一行\n两行')" "x"
rej beCcBus.envelope.badTitle "$SB" --act ask --title "$(printf '字%.0s' $(seq 81))" "x"
rej beCcBus.envelope.needRe "$SB" --act answer --title "答了却没说答哪一条"
rej beCcBus.envelope.badRe "$SB" --act answer --re "not-an-id" --title "t"
rej beCcBus.envelope.tooLong "$SB" --act inform --title "长正文" "$(head -c 1201 /dev/zero | tr '\0' x)"
rej beCcBus.envelope.badRef "$SB" --act inform --title "t" --ref "ftp://x" "x"
rej beCcBus.envelope.badTier "$SB" --tier 3 --act inform --title "t"
rej beCcBus.envelope.badTo "a/b" --act inform --title "t"
FROM="a_cc" rej beCcBus.envelope.badFrom "$SB" --act inform --title "t"
FROM="" rej beCcBus.envelope.badFrom "$SB" --act inform --title "t"
snd "$SB" --act notify --title "t" >/dev/null; _rc=$?
chk "  写错的 act 码：退出码 2（解入参那一下就拒）" "$_rc" "2"
chk "★ 拒掉的一封都没进收件箱" "$(lines_of "$SB")" "$_before"
head -c 1200 /dev/zero | tr '\0' y | snd "$SB" --act inform --title "正好 1200 字节" - >/dev/null; _rc=$?
chk "  对照：正好 1200 字节（从 stdin 读）照样发得出去" "$_rc|$(lines_of "$SB")" "0|$((_before + 1))"

echo "[5] ★ 一档：投完敲门，且要后端「只在闲着时送」；被路由层拦下 ⇒ rc=3"
: > "$CALLS"; : > "$SAYS"
snd "$SB" --tier 1 --act request --title "合一下" --ref commit:393349b8b >/dev/null; _rc=$?
chk "退出 0" "$_rc" "0"
chk "★ 敲了门：先看屏、再说一句" "$(grep -c -- '--terminal-preview' "$CALLS")|$(grep -c -- '--session-say' "$CALLS")" "1|1"
chk "★ 说的那一句要后端只在闲着时送（idle_only）、敲的是收件人那条会话、带着看到的指纹" \
  "$(jq -sc '.[0] | [.idle_only, .sid, .seen_screen]' "$SAYS")" "[true,\"$SB\",\"fp-0\"]"
chk "  敲门那一句不带正文（只说去收信）" "$(jq -s -r '.[0].text | contains("合一下")' "$SAYS")" "false"
printf 'probe*\tnobody\n%s\tnobody\n' "$SA" > "$BUS/policy.tsv"
POLICY=on snd "$SB" --act inform --title "t" >/dev/null; _rc=$?
chk "★ 被路由层拦下：rc=3（与信封被拒的 2 分得开）" "$_rc" "3"
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
# ★ 真事故：`Command::output()` **无限等**。把被调的 cc-bus 命令换成 sleep 300 的桩，
#   那条帧命令 25 秒没回来（25 是从外面掐的，backend 自己没有期限）。
#   而这几条是阻塞档，一条卡住占死一个 tokio worker，且 cancel 对 spawn_blocking 是空操作。
# ⚠ 修法**不是**在后端里加计时器（零定时器铁律 + 协议逐字「超时一律推给客户端」）——
#   而是让**子进程自己**有期限（timeout 前缀，同 ccm 问后端那条）。
mkdir -p "$SANDBOX/hangbin"
printf '#!/bin/bash\nsleep %s\n' "$((2 * WALL_CEILING_S))" > "$SANDBOX/hangbin/cc-log"
chmod +x "$SANDBOX/hangbin/cc-log"
# 卡住的那一个睡两倍兜底上限、外面按兜底上限掐 ⇒ 期限不生效就是被掐（退出码 124）。不按墙钟数「几秒内回来」。
printf '{"id":"x_cc"}' | env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" \
    CC_BUS_BIN_DIR="$SANDBOX/hangbin" CC_BUS_TIMEOUT_SECS=2 \
    "$TIMEOUT" "$WALL_CEILING_S" "$D" -- --bus-inbox >/dev/null 2>"$SANDBOX/err8.txt"
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

echo "[10] ★ 人读的 cc-list：名字与在不在只问 cc-monitor；没装就只列收件箱、说判不了"
# 装了：桩把 `--bus-list` 转给被测的后端（真判），cc-list 照它答的原样印。
_l="$(env CCM_BIN="$FK/ccm" D="$D" CALLS="$CALLS" CLAUDE_CONFIG_DIR="$CLA" HOME="$NOHOME" CC_BUS_HOME="$LB" CC_BUS_BIN_DIR="$SCRIPTS" \
      "$TIMEOUT" "$WALL_CEILING_S" "$SCRIPTS/cc-list" 2>/dev/null)"
chk "★ 装了 ccm：每个收件箱一行，在不在照后端答的（有记录的 false · 没记录的 null）" \
  "$(printf '%s\n' "$_l" | awk -F'\t' -v a="$SR" -v b="$SU" '$1==a||$1==b{print $1"|"$2}' | sort | tr '\n' ' ')" "$(printf '%s|false\n%s|null\n' "$SR" "$SU" | sort | tr '\n' ' ')"
_NL="$SANDBOX/noccm-list"; mkdir -p "$_NL"
for _t in bash sh env awk cat basename dirname readlink grep sed tr; do _p="$(command -v "$_t")" && ln -sf "$_p" "$_NL/$_t"; done
_l="$(env PATH="$_NL" HOME="$NOHOME" CC_BUS_HOME="$LB" "$SCRIPTS/cc-list" 2>"$SANDBOX/cl.err")"
chk "★ 没装 ccm：只列会话 id 与待读，并说名字与在不在判不了" \
  "$(printf '%s\n' "$_l" | sort | tr '\n' ' ')|$(grep -c 'cannot be told' "$SANDBOX/cl.err")" "$(printf '%s\t2\n%s\t1\n' "$SR" "$SU" | sort | tr '\n' ' ')|1"

echo "[11] ★ 收信的那一段只印线上码（act · tier），不放显示用的词"
_r="$(env CCM_BIN="$FK/ccm" CALLS="$CALLS" SELF_SID="$SB" CC_BUS_HOME="$BUS" HOME="$NOHOME" "$TIMEOUT" "$WALL_CEILING_S" "$SCRIPTS/cc-recv" 2>/dev/null)"
chk "★ 每一封印 act= · tier= · title:" \
  "$(printf '%s\n' "$_r" | grep -c '^\[cc-bus\] id=m[0-9a-f]* from='"$SA"' ts=[^ ]* tier=[12] act=[a-z]*')" "3"
chk "  标题一行" "$(printf '%s\n' "$_r" | grep -c '^title: 占位标题$')" "1"
chk "  引用一行（只在有引用时）" "$(printf '%s\n' "$_r" | grep -c '^refs: commit:393349b8b$')" "1"
chk "  不印显示用的词（那是界面照码取文案表的事）" "$(printf '%s\n' "$_r" | grep -cF "$(zh ccBus.act.inform)")" "0"
chk "  读完推进已读位置" "$(cat "$BUS/state/$SB.pos")" "$(lines_of "$SB")"

echo "[12] ★ 旧收件箱迁一次：没读的补成新信封（过后端那一处）、别的进存档，盘上只留一种读法"
MB="$SANDBOX/migbus"; mkdir -p "$MB"/{inbox,state,log}
SX="cccccccc-1111-2222-3333-444444444444"; SY="dddddddd-1111-2222-3333-444444444444"
# 台架写旧版的样子（只采结构）：名字当地址 · 第 5 列是会话 id · 已读位置 · 自动接回复那套状态。
printf 'old_x\told_x:0.0\tts\t1\t%s\nold_y\told_y:0.0\tts\t2\t%s\nold_z\told_z:0.0\tts\t3\t\n' "$SX" "$SY" > "$MB/agents.tsv"
printf '%s\n' '{"from":"old_y","ts":"t1","text":"读过的一条"}' '{"from":"old_y","ts":"t2","text":"没读的一条\n第二行"}' '{"from":"old_z","ts":"t3","text":"发件人换不出会话 id"}' > "$MB/inbox/old_x.jsonl"
echo 1 > "$MB/state/old_x.pos"
printf 'm-old\t0\told_y\n' > "$MB/state/lastread-old_x__old_y"
printf '%s\n' '{"from":"old_x","ts":"t4","text":"收件人换不出会话 id"}' > "$MB/inbox/old_z.jsonl"
env CCM_BIN="$FK/ccm" D="$D" CALLS="$CALLS" SAYS="$SAYS" CLAUDE_CONFIG_DIR="$CLA" HOME="$NOHOME" \
    CC_BUS_HOME="$MB" SELF_SID="$SX" "$TIMEOUT" "$WALL_CEILING_S" "$SCRIPTS/cc-send" "$SY" --act ask --title "迁完之后的第一封" >/dev/null 2>&1
chk "★ 迁过一次：标记在" "$([ -e "$MB/state/envelope-2" ] && echo 在 || echo 没有)" "在"
chk "★ 旧的按名字的收件箱不在原处了" "$(ls "$MB/inbox" | grep -c '^old_')" "0"
chk "  旧状态（已读位置 · lastread）也挪走了" "$(ls "$MB/state" | grep -cE '^old_|^lastread-')" "0"
chk "  存档里原样留着" "$(cat "$MB"/legacy/*/inbox/old_x.jsonl | wc -l | tr -d ' ')|$(ls "$MB"/legacy/*/state | grep -c lastread)" "3|1"
_m="$(head -1 "$MB/inbox/$SX.jsonl" 2>/dev/null)"
chk "★ 没读的那一条补成了新信封：发件人换成会话 id · 二档 inform · 标题取正文第一行" \
  "$(printf '%s' "$_m" | jq -c '[.from, .to, .tier, .act, .title, .text]' 2>/dev/null)" "[\"$SY\",\"$SX\",2,\"inform\",\"没读的一条\",\"没读的一条\\n第二行\"]"
chk "  读过的那一条不搬、换不出会话 id 的不搬（只在存档里）" "$(wc -l < "$MB/inbox/$SX.jsonl" | tr -d ' ')" "1"
chk "  bus.log 记下了留在存档里的那两条" "$(grep -c 'MIGRATE archived-only' "$MB/log/bus.log")" "2"
chk "  迁完之后那一封照常投到" "$(wc -l < "$MB/inbox/$SY.jsonl" | tr -d ' ')" "1"
env CCM_BIN="$FK/ccm" D="$D" CALLS="$CALLS" SAYS="$SAYS" CLAUDE_CONFIG_DIR="$CLA" HOME="$NOHOME" \
    CC_BUS_HOME="$MB" SELF_SID="$SX" "$TIMEOUT" "$WALL_CEILING_S" "$SCRIPTS/cc-send" "$SY" --act ask --title "再一封" >/dev/null 2>&1
chk "★ 只迁一次：再发一封不再迁（存档只有一份）" "$(ls -d "$MB"/legacy/*/ | wc -l | tr -d ' ')|$(wc -l < "$MB/inbox/$SX.jsonl" | tr -d ' ')" "1|1"

echo "[13] ★ 「我是哪条会话」只问 ccm 的 --session-self（装与不装 cc-monitor 各一条 · Codex 会话照样收得到）"
SC="eeeeeeee-1111-2222-3333-444444444444"   # 桩答的那条会话
DECOY="ffffffff-1111-2222-3333-444444444444" # 摆在环境与窗格上的诱饵：一个都不该认
IB="$SANDBOX/idbus"; mkdir -p "$IB"/{inbox,state,log}; touch "$IB/state/envelope-2"   # 新家：不走旧收件箱那一迁
# 一个私有 socket 上的窗格，挂着诱饵 `@ccm_sid`（cc-bus 不读 ccm 的私有标记）。
tmux new-session -d -s whoami_cc -c /tmp 'sleep 120' 2>/dev/null
_wp="$(tmux display-message -p -t "=whoami_cc:" '#{pane_id}' 2>/dev/null)"
tmux set-option -p -t "$_wp" @ccm_sid "$DECOY" >/dev/null 2>&1
chk "  台架自检：窗格上确实挂着诱饵标记" "$(tmux display-message -p -t "$_wp" '#{@ccm_sid}' 2>/dev/null)" "$DECOY"
# 一次 = 一个干净的 env；诱饵三样（CC_BUS_ID · CLAUDE_CODE_SESSION_ID · 窗格标记）全摆上。
idrun() {  # <要跑的脚本> [参数…]；SELF_SID / AGENT / CCM 由调用方给
  local s="$1"; shift
  env ${CCM:+CCM_BIN="$CCM"} D="$D" CALLS="$CALLS" SAYS="$SAYS" SELF_SID="${SELF_SID-}" HOME="$NOHOME" PATH="${IDPATH:-$PATH}" \
      CC_BUS_HOME="$IB" CCBUS_ADAPT_AGENT="${AGENT:-claude}" CLAUDE_CONFIG_DIR="$CLA" \
      CC_BUS_ID="$DECOY" CLAUDE_CODE_SESSION_ID="$DECOY" TMUX_PANE="$_wp" \
      "$TIMEOUT" "$WALL_CEILING_S" "$SCRIPTS/$s" "$@" 2>"$SANDBOX/id.err"
}
# 收件箱里给两条会话各放一封（经 cc-send 正常投：信封过后端）。
FROM="$SA" snd "$SC" --act inform --title "给真身份的一封" "x" >/dev/null
FROM="$SA" snd "$DECOY" --act inform --title "给诱饵的一封" "x" >/dev/null
cp "$BUS/inbox/$SC.jsonl" "$IB/inbox/"; cp "$BUS/inbox/$DECOY.jsonl" "$IB/inbox/"
for AGENT in claude codex; do
  CCM="$FK/ccm" SELF_SID="$SC" idrun cc-whoami > "$SANDBOX/who.txt"; _rc=$?
  chk "★ 装了 ccm（$AGENT 词典）：cc-whoami 给的是 --session-self 答的那条（诱饵一个不认）" "$_rc|$(cat "$SANDBOX/who.txt")" "0|$SC"
  rm -f "$IB/state/"*.pos
  _h="$(printf '{}' | CCM="$FK/ccm" SELF_SID="$SC" idrun cc-bus-stop-hook)"; _rc=$?
  chk "★ 装了 ccm（$AGENT 词典）：Stop 钩子把那条会话的信喂回（拦停 · 是给真身份的那一封）" \
    "$_rc|$(printf '%s' "$_h" | jq -r '.decision' 2>/dev/null)|$(printf '%s' "$_h" | jq -r '.reason' 2>/dev/null | grep -c '给真身份的一封')|$(printf '%s' "$_h" | jq -r '.reason' 2>/dev/null | grep -c '给诱饵的一封')" "0|block|1|0"
done
AGENT=claude
CCM="$FK/ccm" SELF_SID="" idrun cc-whoami >/dev/null; _rc=$?
chk "★ ccm 说不在会话里：cc-whoami 退 1（不退回去认诱饵）" "$_rc" "1"
# 没装：PATH 里只有基本工具（没有 ccm · cc-monitor-backend），家目录里也没有 ~/.cc-monitor/bin/ccm。
_NC="$SANDBOX/noccm"; mkdir -p "$_NC"
for _t in bash sh env awk cat wc date tr sed grep head tail mkdir touch flock mv rm sort cut timeout jq cksum mktemp readlink dirname basename; do
  _p="$(command -v "$_t")" && ln -sf "$_p" "$_NC/$_t"
done
chk "  台架自检：这个 PATH 里没有 ccm" "$(PATH="$_NC" command -v ccm cc-monitor-backend >/dev/null 2>&1 && echo 有 || echo 没有)" "没有"
rm -f "$IB/state/"*.pos
IDPATH="$_NC" SELF_SID="$SC" idrun cc-whoami >/dev/null; _rc=$?
chk "★ 没装 ccm：cc-whoami 退 1、说没装 cc-monitor（诱饵一个不认）" "$_rc|$(grep -c 'no cc-monitor backend' "$SANDBOX/id.err")" "1|1"
_h="$(printf '{}' | IDPATH="$_NC" SELF_SID="$SC" idrun cc-bus-stop-hook)"; _rc=$?
chk "★ 没装 ccm：Stop 钩子静默放行（不喂、不推已读位置）" "$_rc|${_h:-<空>}|$(ls "$IB/state" | grep -c '\.pos$')" "0|<空>|0"
_before="$(wc -l < "$IB/inbox/$SC.jsonl" | tr -d ' ')"
IDPATH="$_NC" idrun cc-send "$SC" --act inform --title "t" >/dev/null; _rc=$?
chk "★ 没装 ccm：cc-send 退 1、一个字节不写" "$_rc|$(wc -l < "$IB/inbox/$SC.jsonl" | tr -d ' ')" "1|$_before"
env PATH="$_NC" HOME="$NOHOME" CC_BUS_HOME="$IB" bash -c '. "$1/cc-bus-lib.sh" && route_load_config && route_nudge "$2" "$3"' _ "$SCRIPTS" "$SC" "$SA" 2>/dev/null
chk "★ 没装 ccm：敲门那一步照实记 NUDGE noccm（只入收件箱，等 Stop 钩子）" "$(grep -c "NUDGE noccm sid=$SC" "$IB/log/bus.log")" "1"
tmux kill-session -t "=whoami_cc" 2>/dev/null || true

echo "[14] ★ unread：只读的未读数（与名单的待读同一个口径），一个字节不写"
UB="$SANDBOX/unbus"; mkdir -p "$UB"/{inbox,state,log}; touch "$UB/state/envelope-2"
printf '%s\n' '{"id":"m1"}' '{"id":"m2"}' '{"id":"m3"}' > "$UB/inbox/$SC.jsonl"; : > "$UB/inbox/$SC.jsonl.lock"; echo 1 > "$UB/state/$SC.pos"
unr() { env HOME="$NOHOME" CC_BUS_HOME="$UB" "$TIMEOUT" "$WALL_CEILING_S" "$SCRIPTS/cc-bus-unread.sh" "$@" 2>"$SANDBOX/un.err"; }
_usnap() { (cd "$UB" && find . -printf '%p %s %T@\n' | sort | md5sum); }
_ub="$(_usnap)"
_u="$(unr "$SC")"; _rc=$?
chk "★ 三封读掉一封 ⇒ 2（退出 0）" "$_rc|$_u" "0|2"
_u="$(unr "$SA")"; _rc=$?
chk "  没有收件箱 ⇒ 0" "$_rc|$_u" "0|0"
unr "a/b" >/dev/null; _rc=$?
chk "  id 不是那个形状 ⇒ 退 2" "$_rc" "2"
chk "★ 跑过三趟，总线目录一个字节没变（不建目录 · 不迁 · 不写锁文件以外的东西）" "$(_usnap)" "$_ub"

echo "[SH1-b] ★ 驾驶舱读面 —— 后端转调 cc-bus 的机器可读读命令（名单只认好名字 · 收件箱只看尾巴）"
# 台架造一个**脏**的总线家（只采结构：文件名不过字符集的收件箱 · 坏行 · 已读位置）。
DB="$SANDBOX/dirtybus"; mkdir -p "$DB"/{inbox,state,log}
_env1='{"id":"m19a1b2c3d4e0001","ts":"t1","from":"peer-a","to":"alpha-b","tier":2,"act":"inform","title":"占位","re":null,"refs":[],"text":"占位一"}'
_env2='{"id":"m19a1b2c3d4e0002","ts":"t2","from":"peer-a","to":"alpha-b","tier":2,"act":"inform","title":"占位","re":null,"refs":[],"text":"占位二"}'
printf '%s\nnot json\n%s\n' "$_env1" "$_env2" > "$DB/inbox/alpha_cc.jsonl"
echo 1 > "$DB/state/alpha_cc.pos"
: > "$DB/inbox/--help.jsonl"; : > "$DB/inbox/a b.jsonl"
dd() { env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$DB" CC_BUS_BIN_DIR="$SCRIPTS" "$TIMEOUT" "$WALL_CEILING_S" "$D" -- "$@" 2>"$SANDBOX/err.txt"; }
out="$(dd --bus-list </dev/null)"
chk "名单只有那一个好名字（--help 被本侧 id 判定拒、带空格的被 cc-bus 记成坏行）" "$(printf '%s' "$out" | jq -c '[.agents[].id]')" '["alpha_cc"]'
chk "  待读数照旧（3 行 − 已读 1）" "$(printf '%s' "$out" | jq -r '.agents[0].unread')" "2"
_pos_before="$(cat "$DB/state/alpha_cc.pos")"; _state_before="$(ls "$DB/state" | tr '\n' ' ')"
out="$(printf '{"id":"alpha_cc","lines":2}' | dd --bus-inbox)"
chk "★ 收件箱只看尾巴（末 2 行：一条坏、一条好）" "$(printf '%s' "$out" | jq -c '[.messages[].text, .skipped]')" '["占位二",1]'
chk "★ 读收件箱不推已读位置" "$(cat "$DB/state/alpha_cc.pos")" "$_pos_before"
chk "  也不在 state/ 里写任何东西" "$(ls "$DB/state" | tr '\n' ' ')" "$_state_before"
printf '{"id":"--help"}' | dd --bus-inbox >/dev/null
chk "★ --help 当收件箱 id ⇒ bad_id（交给 cc-log 之前拒）" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "bad_id"
# 老 cc-bus：`cc-list` 不认 --tsv（照打人读表）或还是上一版的名册 ⇒ 明说要重新部署，不解成一份空名单。
_TOOOLD="$(zh_frag beCcBus.read.tooOld)"
_old="$SANDBOX/oldbus"; mkdir -p "$_old"
for _shape in 'echo "ID           TMUX               待读"' 'printf "#cc-list-tsv\\t1\\nalpha_cc\\talpha_cc:0.0\\tts\\t1\\t0\\n#skipped\\t0\\n"'; do
  printf '#!/bin/sh\n%s\n' "$_shape" > "$_old/cc-list"; chmod +x "$_old/cc-list"
  env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$DB" CC_BUS_BIN_DIR="$_old" "$TIMEOUT" "$WALL_CEILING_S" "$D" -- --bus-list </dev/null >/dev/null 2>"$SANDBOX/err.txt"
  # 那一句按文案键认（beCcBus.read.tooOld 最长的那一段固定字），不钉原文。
  chk "★ 老 cc-bus（${_shape:0:12}…）⇒ failed 且说要重新部署（beCcBus.read.tooOld）" "$(jq -r --arg w "$_TOOOLD" '.code + " " + ((.message + "\n" + .detail) | contains($w) | tostring)' < "$SANDBOX/err.txt" 2>/dev/null)" "failed true"
done

echo "[SH1-c] ★ 杀会话不碰 cc-bus 的收件箱"
_dg="$SANDBOX/dg"; mkdir -p "$_dg"
tmux new-session -d -s dg-cc -c /tmp env OUT="$_dg" sh -c 'touch "$OUT/a"; exec sleep 60' 2>/dev/null
wall_until '[ -f "$_dg/a" ]' || true
printf '{"from":"x","text":"占位"}\n' >> "$BUS/inbox/dg_cc.jsonl"
out="$(printf '{"name":"dg-cc"}' | d --kill)"
chk "杀会话本身照旧成功" "$(printf '%s' "$out" | jq -c '[.session, .killed]')" '["dg-cc",true]'
chk "★ 收件箱没被动" "$([ -e "$BUS/inbox/dg_cc.jsonl" ] && echo 在 || echo 没了)" "在"

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

# 帧命令派生的另一条（`--profiles-from-line`：把一条命令行拆成配置那几格，纯函数、不碰盘）同一套口：
#   argv 与 stdin 逐字一样 · 开着不写回 no_input。从前这一格用的是 `--resolve`，10-26 随那条帧命令撤了。
_rq='{"name":"p","line":"claude --model opus"}'
printf '%s' "$_rq" | d --profiles-from-line >"$SANDBOX/c2-pf-in.txt"; _rc_in=$?
d --profiles-from-line --args-b64 "$(printf '%s' "$_rq" | base64 -w0)" < <(sleep 30) >"$SANDBOX/c2-pf-av.txt"; _rc_av=$?
chk "★ --profiles-from-line：argv 载荷口与 stdin 逐字一样（退出 0）" "$_rc_in|$_rc_av|$(cmp -s "$SANDBOX/c2-pf-in.txt" "$SANDBOX/c2-pf-av.txt" && echo 同 || echo 不同)" "0|0|同"
chk "  --profiles-from-line 成品是拆出来的那几格" "$(jq -r '.name + "|" + .args' < "$SANDBOX/c2-pf-av.txt" 2>/dev/null)" "p|--model opus"
d --profiles-from-line < <(exec sleep "$((2 * WALL_CEILING_S))") >/dev/null; _rc=$?; kill "$!" 2>/dev/null
chk "★ --profiles-from-line 开着不写：立即回 no_input" "$([ "$_rc" -eq 2 ] && echo 是 || echo "否 rc=$_rc")|$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "是|no_input"

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
printf '{"from":"a/b","to":"b","act":"ask","title":"t"}' | one --bus-envelope >/dev/null; _rc=$?
chk "★ bus-envelope 拒：非 0 · stderr 正好一行信封" \
  "$([ "$_rc" -ne 0 ] && echo 非0)|$(wc -l < "$SANDBOX/err.txt")|$(jq -r 'select(type=="object") | (.code)' < "$SANDBOX/err.txt" 2>/dev/null)" "非0|1|bad_from"
# 诊断文件那一层目录不在 ⇒ 不替它建（只建文件不建目录），stderr 照样干净。
NL="$SANDBOX/nolog"; mkdir -p "$NL"
HOME="$NL" d --history-read </dev/null >/dev/null
chk "  没有日志目录：stderr 照样一行 · 不建目录" "$(wc -l < "$SANDBOX/err.txt")|$([ -e "$NL/.cc-monitor" ] && echo 建了 || echo 没建)" "1|没建"

"$REALTMUX" -L "$_SOCK" kill-server 2>/dev/null || true

echo
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ] || exit 1
echo "===== P4f 验收全部通过 ====="
