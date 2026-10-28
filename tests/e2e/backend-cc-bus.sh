#!/usr/bin/env bash
# cc-bus 真跑：收件箱住后端（数据目录 `cc-bus/`），`--bus-send` · `--bus-peek` · `--bus-commit` · `--bus-unread` · `--bus-list` · `--bus-inbox`
# 是读写它的唯一入口；`cc-bus <动词>` 每一条只是把一句交给后端（A13-8）。
#
# ## 为什么必须有真跑这一层
#
# 单测把规矩（`cc_bus::rules`）· 收件箱（`cc_bus::store`）· 信封校验那些纯函数钉住了，但它们**证明不了命令调得动**：
# 08-13 实测撞到过一次 —— 命令进了 `inbound::REGISTRY`、分派臂也认，而 `main::is_query_mode` 这道**闸门**
# 读的是手写的 `SUBCOMMANDS`。漏加的后果是**静默的**：backend 打一行 warn 之后照常进流模式。⇒ 这套件跑的是**真二进制的真 argv**，
# 再经仓内这一版 cc-bus 脚本走一遍。
#
# ## 本机安全
#
# · 每一次起后端都显式给 `HOME`（沙箱里的家）与 `CLAUDE_CONFIG_DIR` —— 收件箱落在那个家的 `.cc-monitor/cc-bus/`，
#   绝不碰真实的 `~/.cc-monitor`；
# · 脚本用的是**仓内**那一份，`ccm` 换成台架的桩（`CCM_BIN`）：看屏 / 说一句只记下来，`--bus-*` 原样转给被测的后端；
# · ★★ **全程把 shim 放在 PATH 最前面**，任何裸 `tmux` 都被强制 `-L <隔离socket>`（C7i，事故换来的：本套件曾有一格
#   裸 `tmux new-session` 建到了用户的默认 socket 上）⇒ shim 全程在 PATH 上 + 起飞前双向自检。
# · 起的进程只有后端自己（一次性 exec，`</dev/null` + `timeout`）与 [13] 那一个诱饵窗格（私有 socket 上的 sleep）。
. "$(cd "$(dirname "$0")" && pwd)/sandbox-env.sh"  # 无条件清掉继承来的 CCM_* / CLAUDE* / ANTHROPIC_* / TMUX* / CC_BUS_*
set -o pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
# 后端说的那几句按文案键认，本文件不钉原文：zh = 插好值的整句（与界面同一个取文口）。
zh() { "$REPO/node_modules/.bin/tsx" "$REPO/tests/e2e/copy-text.mts" "$@"; }
D="${CARGO_TARGET_DIR:-$REPO/.build/backend}/debug/cc-monitor-backend"
[ -x "$D" ] || { echo "需要先 build backend：cd src/backend && cargo build"; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "需要 jq"; exit 1; }
TIMEOUT="$(command -v timeout)" || { echo "需要 timeout"; exit 1; }
REALTMUX="$(command -v tmux)" || { echo "需要 tmux（[13] 的窗格诱饵 · [SH1-c] 杀会话要它）"; exit 1; }

SANDBOX="$(mktemp -d)"
trap 'rm -rf "$SANDBOX"; tmux_shim_cleanup' EXIT
CLA="$SANDBOX/claude"; EMPTY="$SANDBOX/empty"; NOHOME="$SANDBOX/nohome"
mkdir -p "$CLA/projects" "$EMPTY" "$NOHOME"

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
[ -x "$SCRIPTS/cc-bus" ] || { echo "仓内没有 cc-bus：$SCRIPTS"; exit 1; }

pass=0; fail=0
chk() { if [ "$2" = "$3" ]; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1: 期望[$3] 实得[$2]"; fail=$((fail+1)); fi; }

# 一次调用 = 一个干净的 env（**逐次显式给全**，免得上一格的变量漏进来）。家缺省 $NOHOME，`DHOME` 换一个。
d() {
  env CLAUDE_CONFIG_DIR="$CLA" HOME="${DHOME:-$NOHOME}" \
      "$TIMEOUT" "$WALL_CEILING_S" "$D" -- "$@" 2>"$SANDBOX/err.txt"
}
# 那个家里的收件箱目录 · 一条会话的收件箱有几封。
busdir() { printf '%s/.cc-monitor/cc-bus' "${1:-$NOHOME}"; }
lines_of() { local f; f="$(busdir "${2:-}")/$1.jsonl"; if [ -f "$f" ]; then wc -l < "$f" | tr -d ' '; else echo 0; fi; }

echo "===== cc-bus：收件箱住后端，入口只是把一句交给它 ====="

echo "[1] --bus-list：空总线"
out="$(d --bus-list </dev/null)"
chk "空总线回空数组（不是报错）" "$(printf '%s' "$out" | jq -c '.agents')" "[]"
chk "  只读：没投过信就不建收件箱目录" "$([ -e "$(busdir)" ] && echo 建了 || echo 没建)" "没建"

# ===== 发信：cc-bus send 只把那一封交给后端 `bus-send`（核信封 · 范围 · 去重 · 限流都在那一处），后端收进收件箱 =====
# 台架的 ccm：一份记账的桩 —— `--bus-*` 原样转给被测的后端二进制（真判、真落盘，敲门也是它投进沙箱家里那条会话的输入队）。
#   `--session-self`（「我是哪条会话」）答 `$SELF_SID`；它空着 ⇒ 照后端的形状拒成 `not_in_session`。别的一律不答（退 9）：脚本不该问别的。
#   不起真 claude、不碰真会话：输入队只落在沙箱家里，没有投递那一侧把它送进哪个窗格。
SA="aaaaaaaa-1111-2222-3333-444444444444"   # 发件人（会话 id 的形状，占位）
SB="bbbbbbbb-1111-2222-3333-444444444444"   # 收件人
FK="$SANDBOX/fakeccm"; mkdir -p "$FK"; CALLS="$SANDBOX/ccm-calls.txt"
cat > "$FK/ccm" <<'EOF'
#!/bin/bash
printf '%s\n' "$*" >> "$CALLS"
case " $* " in
  *" --bus-"*) exec "$D" "$@" ;;
  *" --session-self "*)
    if [ -n "${SELF_SID:-}" ]; then printf '{"sid":"%s","groups":[],"looked":"this_machine"}\n' "$SELF_SID"
    else printf '{"code":"not_in_session","message":"no session above","detail":""}\n' >&2; exit 2; fi ;;
  *) echo "fake ccm: 没想到会被问 $*" >&2; exit 9 ;;
esac
EOF
chmod +x "$FK/ccm"
# 那个家里一条会话的输入队（后端敲门投进去的那几条）：`items` 照到达先后；队文件不在 ⇒ 空。
queue_of() { local f; f="${2:-$NOHOME}/.cc-monitor/session-queue/$1.json"; if [ -f "$f" ]; then jq -c '.items' "$f"; else echo '[]'; fi; }
# 一次 = 一个干净的 env（逐次给全）。FROM 缺 ⇒ SA；家缺省 $NOHOME。
bus() {  # <动词> [参数…]
  env CCM_BIN="$FK/ccm" D="$D" CALLS="$CALLS" CLAUDE_CONFIG_DIR="$CLA" HOME="${SNDHOME:-$NOHOME}" \
      SELF_SID="${FROM-$SA}" "$TIMEOUT" "$WALL_CEILING_S" "$SCRIPTS/cc-bus" "$@" 2>"$SANDBOX/snd.err"
}
snd() { bus send "$@"; }

echo "[2] --bus-list：名单的行是收件箱（会话 id · 待读），标题与在不在由后端判"
# 两条会话：SR 这台有记录（沙箱里的一份两行记录，正文是占位）、没有进程持着它；SU 这台没有记录。
SR="a1000000-0000-4000-8000-000000000001"; SU="a2000000-0000-4000-8000-000000000002"
mkdir -p "$CLA/projects/-lw"
printf '%s\n' "{\"type\":\"user\",\"uuid\":\"u1\",\"parentUuid\":null,\"sessionId\":\"$SR\",\"timestamp\":\"2026-10-26T00:00:00Z\",\"cwd\":\"/lw\",\"message\":{\"role\":\"user\",\"content\":\"占位的第一句\"}}" > "$CLA/projects/-lw/$SR.jsonl"
LH2="$SANDBOX/listhome"; mkdir -p "$LH2"
SNDHOME="$LH2" snd "$SR" --act inform --title "一" >/dev/null
SNDHOME="$LH2" snd "$SR" --act inform --title "二" >/dev/null
SNDHOME="$LH2" snd "$SU" --act inform --title "三" >/dev/null
out="$(DHOME="$LH2" d --bus-list </dev/null)"
chk "★ 名单就是那两个收件箱" "$(printf '%s' "$out" | jq -c '[.agents[].id] | sort')" "$(jq -cn --arg a "$SR" --arg b "$SU" '[$a,$b] | sort')"
chk "  待读数（真数出来的，不是猜的）" "$(printf '%s' "$out" | jq -r --arg s "$SR" '.agents[] | select(.id==$s) | .unread')" "2"
chk "★ 有记录的那条：标题是会话自己的（不是会话 id）· 没人持着它 ⇒ live=false" \
  "$(printf '%s' "$out" | jq -r --arg s "$SR" '.agents[] | select(.id==$s) | [(.title != $s and .title != ""), .live] | map(tostring) | join("|")')" "true|false"
chk "★ 这台没记录的那条：判不了 ⇒ live=null（不是 false）· 标题照实就是会话 id" \
  "$(printf '%s' "$out" | jq -r --arg s "$SU" '.agents[] | select(.id==$s) | [.title, .live] | map(tostring) | join("|")')" "$SU|null"

echo "[3] cc-bus send：信封过后端、落进收件人的收件箱；缺省二档、不敲门"
: > "$CALLS"
snd "$SB" --act inform --title "占位标题" "占位正文" >/dev/null; _rc=$?
chk "退出 0" "$_rc" "0"
chk "★ 收件箱（后端数据目录里，按会话 id）多了一行" "$(lines_of "$SB")" "1"
_e="$(tail -1 "$(busdir)/$SB.jsonl")"
chk "  信封各格：from · to · tier 缺省 2 · act · title · 正文" \
  "$(printf '%s' "$_e" | jq -c '[.from, .to, .tier, .act, .title, .text]')" "[\"$SA\",\"$SB\",2,\"inform\",\"占位标题\",\"占位正文\"]"
chk "  id 与时间是后端盖的（m 打头 · UTC）" \
  "$(printf '%s' "$_e" | jq -r '(.id | startswith("m")) and (.ts | endswith("Z"))')" "true"
chk "★ 二档不敲门：收件人的输入队里一条都没有" "$(queue_of "$SB")" "[]"
chk "  发信只问了后端一次 bus-send（别的一句都没问）" "$(grep -c -- '--bus-send' "$CALLS")|$(grep -vc -- '--bus-send\|--session-self' "$CALLS")" "1|0"

echo "[3b] ★ 去重：同一封 1 秒内发两次 ⇒ 后端只收一次，第二次退 3、说重复"
_before="$(lines_of "$SB")"
snd "$SB" --act ask --title "重复的那一封" "同一句" >/dev/null; _r1=$?
snd "$SB" --act ask --title "重复的那一封" "同一句" >/dev/null; _r2=$?
chk "★ 头一封收、第二封退 3" "$_r1|$_r2" "0|3"
chk "★ 收件箱只多一行" "$(lines_of "$SB")" "$((_before + 1))"
chk "  stderr 是后端那一句（重复）" "$(grep -cF "$(zh beCcBus.envelope.duplicate)" "$SANDBOX/snd.err")" "1"
snd "$SB" --act ask --title "重复的那一封" "换了一句" >/dev/null; _rc=$?
chk "  正文不同就是另一封" "$_rc|$(lines_of "$SB")" "0|$((_before + 2))"

echo "[4] ★ 信封不合格当场拒（rc=2）、一个字节都不写；那一句是后端文案表里的那一句"
_before="$(lines_of "$SB")"
rej() {  # <期望的那一句的文案键> <cc-bus send 参数…>
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

echo "[5] ★ 一档：后端收信那一下自己敲门 —— 投进收件人那条会话的输入队（闲着时由投递送）；同一条会话 2 秒内只敲一次"
SK="cccc0000-1111-2222-3333-444444444444"
: > "$CALLS"
snd "$SK" --tier 1 --act request --title "合一下" --ref commit:393349b8b >/dev/null; _rc=$?
chk "退出 0" "$_rc" "0"
chk "★ 敲了门：收件人的输入队多一条，发自 cc-bus，id 由那一封来" \
  "$(queue_of "$SK" | jq -c '[length, .[0].end, (.[0].id | startswith("cc-bus-m"))]')" '[1,"cc-bus",true]'
chk "★ 脚本只问了 bus-send：不看屏、不自己投" "$(grep -vc -- '--bus-send\|--session-self' "$CALLS")" "0"
chk "  敲门那一句不带标题与正文（只说谁来信、去收信）" "$(queue_of "$SK" | jq -r '.[0].text | contains("合一下")')" "false"
chk "  敲门那一句说发信人、叫对方跑 cc-bus recv（一个入口）" \
  "$(queue_of "$SK" | jq -r --arg a "$SA" '.[0].text | (contains($a) and contains("cc-bus recv"))')" "true"
snd "$SK" --tier 1 --act request --title "紧跟着又一封" >/dev/null; _rc=$?
chk "★ 去抖：紧跟着的第二封照收、不再敲" "$_rc|$(lines_of "$SK")|$(queue_of "$SK" | jq length)" "0|2|1"

echo "[5b] ★ 组范围（后端那一处判）：发件人在组里 ⇒ 只送得到同组；名单读不到 ⇒ 拒；退 3 与信封写错的 2 分得开"
# 沙箱家里的一份注解（只采结构）：SA · SD 在「开发组」，SG 不在任何组。
GH="$SANDBOX/grouphome"; mkdir -p "$GH/.cc-monitor"
SD="d0000000-1111-2222-3333-444444444444"; SG="99999999-1111-2222-3333-444444444444"
printf '{"version":1,"entries":{"%s":{"starred":false,"customTitle":null,"groups":["g1"],"updatedAt":1},"%s":{"starred":false,"customTitle":null,"groups":["g1"],"updatedAt":1}},"groups":{"g1":{"name":"开发组","made":1,"named":1}}}\n' "$SA" "$SD" > "$GH/.cc-monitor/history-metadata.json"
SNDHOME="$GH" snd "$SG" --act inform --title "组外" "x" >/dev/null; _rc=$?
chk "★ 收件人不在发件人那组：退 3、一个字节不写" "$_rc|$(lines_of "$SG" "$GH")" "3|0"
chk "  stderr 那一句带着发件人在哪组（开发组）" "$(grep -c '开发组' "$SANDBOX/snd.err")" "1"
SNDHOME="$GH" snd "$SD" --act inform --title "组内" "x" >/dev/null; _rc=$?
chk "★ 同组照常投到" "$_rc|$(lines_of "$SD" "$GH")" "0|1"
FROM="$SG" SNDHOME="$GH" snd "$SD" --act inform --title "组外的人发进来" "x" >/dev/null; _rc=$?
chk "  发件人不在任何组 ⇒ 照今天，发给谁都行" "$_rc|$(lines_of "$SD" "$GH")" "0|2"
printf 'not json' > "$GH/.cc-monitor/history-metadata.json"
SNDHOME="$GH" snd "$SD" --act inform --title "读不到名单" "x" >/dev/null; _rc=$?
chk "★ 名单读不到 ⇒ 退 3（不退回成全放行）· 一个字节不写" "$_rc|$(lines_of "$SD" "$GH")|$(grep -c 'group roster not read' "$SANDBOX/snd.err")" "3|2|1"

echo "[7] ★ 两个入口都有：CLI 面上调得动"
# ★ 这一格钉的是那条**静默失效**：闸门（`main::SUBCOMMANDS`）漏加时，这条 flag 不进一次性模式、掉进流模式吐一堆 jsonl 行。
for _c in --bus-list --bus-send --bus-peek --bus-commit --bus-unread --bus-inbox; do
  _o="$(printf '{}' | d "$_c")"
  _all="$( { printf '%s\n' "$_o"; cat "$SANDBOX/err.txt"; } | grep . )"
  chk "★ $_c 进的是一次性模式（正好一行回包或失败信封，不是流）" \
    "$(printf '%s\n' "$_all" | grep -c .)|$(printf '%s\n' "$_all" | jq -e 'type == "object"' >/dev/null 2>&1 && echo json || echo 不是)" "1|json"
done

echo "[9] ★ 声明「不收输入」的命令，stdin 不关时必须秒回"
# ★ 真事故：CLI 入口原来从 `fields` 派生「要不要读 stdin」，`bus-list` 无输入却有输出字段 ⇒ 挂住等一个永远不来的输入。
for _c in --ping --bus-list; do
  # stdin 开着两倍兜底上限、外面按兜底上限掐：挂着等输入就是被掐（124）。
  _o="$(env CLAUDE_CONFIG_DIR="$CLA" HOME="$NOHOME" \
        "$TIMEOUT" "$WALL_CEILING_S" "$D" -- "$_c" < <(exec sleep "$((2 * WALL_CEILING_S))") 2>/dev/null
        _r=$?; kill "$!" 2>/dev/null; exit "$_r")"; _rc9=$?
  chk "★ $_c：stdin 不关也返回了（不是挂到被掐）" \
    "$([ -n "$_o" ] && [ "$_rc9" -ne 124 ] && echo yes || echo "no（退出码 $_rc9，输出 ${_o:-<空>}）")" "yes"
done

echo "[10] ★ 人读的 cc-bus list：照后端答的原样印；没装 cc-monitor 就退 1、说没装"
_l="$(SNDHOME="$LH2" bus list)"
chk "★ 每个收件箱一行，在不在照后端答的（有记录的 false · 没记录的 null）" \
  "$(printf '%s\n' "$_l" | awk -F'\t' -v a="$SR" -v b="$SU" '$1==a||$1==b{print $1"|"$2}' | sort | tr '\n' ' ')" "$(printf '%s|false\n%s|null\n' "$SR" "$SU" | sort | tr '\n' ' ')"
_NL="$SANDBOX/noccm-list"; mkdir -p "$_NL"
for _t in bash sh env awk cat basename dirname readlink grep sed tr uname jq mktemp rm; do _p="$(command -v "$_t")" && ln -sf "$_p" "$_NL/$_t"; done
env PATH="$_NL" HOME="$NOHOME" "$SCRIPTS/cc-bus" list >/dev/null 2>"$SANDBOX/cl.err"; _rc=$?
chk "★ 没装 ccm：退 1、说没装 cc-monitor（不自己去读收件箱）" "$_rc|$(grep -c 'no cc-monitor backend' "$SANDBOX/cl.err")" "1|1"

echo "[11] ★ 收信：后端写好的那一段只印线上码（act · tier），读完推进已读位置"
_r="$(FROM="$SB" bus recv)"
chk "★ 每一封印 act= · tier= · title:" \
  "$(printf '%s\n' "$_r" | grep -c '^\[cc-bus\] id=m[0-9a-f]* from='"$SA"' ts=[^ ]* tier=[12] act=[a-z]*')" "$(lines_of "$SB")"
chk "  标题一行" "$(printf '%s\n' "$_r" | grep -c '^title: 占位标题$')" "1"
chk "  不印显示用的词（那是界面照码取文案表的事）" "$(printf '%s\n' "$_r" | grep -cF "$(zh ccBus.act.inform)")" "0"
chk "★ 读完推进已读位置（后端答未读 0）" "$(FROM="$SB" bus unread)" "0"
chk "  再收一次：没有新的、什么都不印" "$(FROM="$SB" bus recv | wc -c | tr -d ' ')" "0"
_r="$(FROM="$SK" bus recv)"
chk "  引用一行（只在有引用时）" "$(printf '%s\n' "$_r" | grep -c '^refs: commit:393349b8b$')" "1"

echo "[12] ★ 入口只把一句交给后端：没装 cc-monitor ⇒ cc-bus send 退 1、说没装，一个文件都不落"
EH="$SANDBOX/emptyhome"; mkdir -p "$EH"
_NC="$SANDBOX/noccm"; mkdir -p "$_NC"
for _t in bash sh env awk cat wc date tr sed grep head tail mkdir touch mv rm sort cut timeout jq cksum mktemp readlink dirname basename uname; do
  _p="$(command -v "$_t")" && ln -sf "$_p" "$_NC/$_t"
done
chk "  台架自检：这个 PATH 里没有 ccm" "$(PATH="$_NC" command -v ccm cc-monitor-backend >/dev/null 2>&1 && echo 有 || echo 没有)" "没有"
env PATH="$_NC" HOME="$EH" "$SCRIPTS/cc-bus" send "$SB" --act inform --title "t" "x" >/dev/null 2>"$SANDBOX/nc.err"; _rc=$?
chk "★ 退 1 · 说没装 cc-monitor" "$_rc|$(grep -c 'no cc-monitor backend' "$SANDBOX/nc.err")" "1|1"
chk "★ 那个家里一个文件都没落" "$(find "$EH" -mindepth 1 | wc -l | tr -d ' ')" "0"
for _v in "peek $SB" "unread $SB" "commit $SB 0.1.0000000000000000" "log $SB"; do
  # shellcheck disable=SC2086
  env PATH="$_NC" HOME="$EH" "$SCRIPTS/cc-bus" $_v >/dev/null 2>&1; _rc=$?
  chk "  cc-bus ${_v%% *}：没装 ⇒ 退 1（不自己读写收件箱）" "$_rc|$(find "$EH" -mindepth 1 | wc -l | tr -d ' ')" "1|0"
done
chk "★ 脚本源里零出现收件箱文件 · 锁 · 规矩（route_rate · flock · .jsonl · .pos）" \
  "$(cat "$SCRIPTS"/cc-bus "$SCRIPTS"/*.sh | grep -v '^\s*#' | grep -cE 'route_|flock|\.jsonl|\.pos\b|CC_BUS_HOME')" "0"

echo "[13] ★ 「我是哪条会话」只问 ccm 的 --session-self（装与不装 cc-monitor 各一条 · Codex 会话照样收得到）"
SC="eeeeeeee-1111-2222-3333-444444444444"   # 桩答的那条会话
DECOY="ffffffff-1111-2222-3333-444444444444" # 摆在环境与窗格上的诱饵：一个都不该认
# 一个私有 socket 上的窗格，挂着诱饵 `@ccm_sid`（cc-bus 不读 ccm 的私有标记）。
tmux new-session -d -s whoami_cc -c /tmp 'sleep 120' 2>/dev/null
_wp="$(tmux display-message -p -t "=whoami_cc:" '#{pane_id}' 2>/dev/null)"
tmux set-option -p -t "$_wp" @ccm_sid "$DECOY" >/dev/null 2>&1
chk "  台架自检：窗格上确实挂着诱饵标记" "$(tmux display-message -p -t "$_wp" '#{@ccm_sid}' 2>/dev/null)" "$DECOY"
# 一次 = 一个干净的 env；诱饵三样（CC_BUS_ID · CLAUDE_CODE_SESSION_ID · 窗格标记）全摆上。
idrun() {  # <动词> [参数…]；SELF_SID / AGENT / CCM 由调用方给
  local s="$1"; shift
  env ${CCM:+CCM_BIN="$CCM"} D="$D" CALLS="$CALLS" SELF_SID="${SELF_SID-}" HOME="$NOHOME" PATH="${IDPATH:-$PATH}" \
      CCBUS_ADAPT_AGENT="${AGENT:-claude}" CLAUDE_CONFIG_DIR="$CLA" \
      CC_BUS_ID="$DECOY" CLAUDE_CODE_SESSION_ID="$DECOY" TMUX_PANE="$_wp" \
      "$TIMEOUT" "$WALL_CEILING_S" "$SCRIPTS/cc-bus" "$s" "$@" 2>"$SANDBOX/id.err"
}
for AGENT in claude codex; do
  CCM="$FK/ccm" SELF_SID="$SC" idrun whoami > "$SANDBOX/who.txt"; _rc=$?
  chk "★ 装了 ccm（$AGENT 词典）：cc-bus whoami 给的是 --session-self 答的那条（诱饵一个不认）" "$_rc|$(cat "$SANDBOX/who.txt")" "0|$SC"
  FROM="$SA" snd "$SC" --act inform --title "给真身份的一封（$AGENT）" "x" >/dev/null
  FROM="$SA" snd "$DECOY" --act inform --title "给诱饵的一封（$AGENT）" "x" >/dev/null
  _h="$(printf '{}' | CCM="$FK/ccm" SELF_SID="$SC" idrun stop-hook)"; _rc=$?
  chk "★ 装了 ccm（$AGENT 词典）：Stop 钩子把那条会话的信喂回（拦停 · 是给真身份的那一封）" \
    "$_rc|$(printf '%s' "$_h" | jq -r '.decision' 2>/dev/null)|$(printf '%s' "$_h" | jq -r '.reason' 2>/dev/null | grep -c "给真身份的一封（$AGENT）")|$(printf '%s' "$_h" | jq -r '.reason' 2>/dev/null | grep -c '给诱饵的一封')" "0|block|1|0"
  chk "  喂回之后才推已读位置（未读 0）" "$(FROM="$SC" bus unread)" "0"
done
AGENT=claude
CCM="$FK/ccm" SELF_SID="" idrun whoami >/dev/null; _rc=$?
chk "★ ccm 说不在会话里：cc-bus whoami 退 1（不退回去认诱饵）" "$_rc" "1"
IDPATH="$_NC" SELF_SID="$SC" idrun whoami >/dev/null; _rc=$?
chk "★ 没装 ccm：cc-bus whoami 退 1、说没装 cc-monitor（诱饵一个不认）" "$_rc|$(grep -c 'no cc-monitor backend' "$SANDBOX/id.err")" "1|1"
FROM="$SA" snd "$SC" --act inform --title "没装时留着的一封" "x" >/dev/null
_h="$(printf '{}' | IDPATH="$_NC" SELF_SID="$SC" idrun stop-hook)"; _rc=$?
chk "★ 没装 ccm：Stop 钩子静默放行（不喂、不推已读位置）" "$_rc|${_h:-<空>}|$(FROM="$SC" bus unread)" "0|<空>|1"
tmux kill-session -t "=whoami_cc" 2>/dev/null || true

echo "[14] ★ unread · peek · commit：两阶段读，未读数与名单的待读同一个口径；读面一个字节不写"
SU2="f1000000-1111-2222-3333-444444444444"
for _i in 1 2 3; do FROM="$SA" snd "$SU2" --act inform --title "第 $_i 封" "x" >/dev/null; done
_usnap() { (cd "$(busdir)" && find . -printf '%p %s %T@\n' | sort | md5sum); }
_ub="$(_usnap)"
chk "★ 三封没读 ⇒ 3" "$(FROM="$SU2" bus unread)" "3"
TOK="$SANDBOX/tok"
_p="$(FROM="$SU2" bus peek --max-lines 1 --token-file "$TOK")"; _rc=$?
chk "★ 分段读：一轮只出一封、令牌在文件里" "$_rc|$(printf '%s\n' "$_p" | grep -c '^\[cc-bus\] id=')|$([ -s "$TOK" ] && echo 有 || echo 没有)" "0|1|有"
chk "★ peek · unread 跑过之后收件箱目录一个字节没变" "$(_usnap)" "$_ub"
FROM="$SU2" bus commit "$SU2" "$(cat "$TOK")" >/dev/null; _rc=$?
chk "★ commit：推了一封 ⇒ 未读 2" "$_rc|$(FROM="$SU2" bus unread)" "0|2"
FROM="$SU2" bus commit "$SU2" "$(cat "$TOK")" >/dev/null; _rc=$?
chk "★ 同一张令牌再交 ⇒ 11（别人已推过：照常处理手上那段）" "$_rc" "11"
FROM="$SU2" bus commit "$SU2" "not-a-token" >/dev/null; _rc=$?
chk "  令牌坏 ⇒ 2" "$_rc" "2"
FROM="$SU2" bus unread "a/b" >/dev/null; _rc=$?
chk "  id 不是那个形状 ⇒ 退 2" "$_rc" "2"
chk "  没有收件箱 ⇒ 0" "$(FROM="$SU2" bus unread "f2000000-1111-2222-3333-444444444444")" "0"

echo "[15] ★ 装：只往 PATH 上链 cc-bus 一个命令；链接目录里别的东西一概不碰；不建运行时目录"
IH="$SANDBOX/insthome"; mkdir -p "$IH/.local/bin" "$IH/other"
ln -s "$SCRIPTS/cc-whoami" "$IH/.local/bin/cc-whoami"                              # 指向某个 cc-bus/scripts/ 的别的名字（不是装口的事）
printf '#!/bin/sh\n' > "$IH/other/cc-recv"; chmod +x "$IH/other/cc-recv"; ln -s "$IH/other/cc-recv" "$IH/.local/bin/cc-recv"   # 别人的
env HOME="$IH" PATH="$IH/.local/bin:/usr/bin:/bin" bash "$SCRIPTS/cc-bus-install.sh" >"$SANDBOX/inst.out" 2>&1; _rc=$?
chk "★ 装完链接目录里多了 cc-bus 一个，原有的两个都在" "$_rc|$(ls "$IH/.local/bin" | tr '\n' ' ')" "0|cc-bus cc-recv cc-whoami "
chk "  cc-bus 指向仓里这一份入口" "$(readlink -f "$IH/.local/bin/cc-bus")" "$SCRIPTS/cc-bus"
chk "  原有的链接原样留着" "$(readlink "$IH/.local/bin/cc-recv")|$(readlink "$IH/.local/bin/cc-whoami")" "$IH/other/cc-recv|$SCRIPTS/cc-whoami"
chk "★ 不建 ~/.cc-bus（收件箱住后端）" "$([ -e "$IH/.cc-bus" ] && echo 建了 || echo 没建)" "没建"
_hv="$(env HOME="$IH" PATH="$IH/.local/bin:/usr/bin:/bin" cc-bus help 2>&1)"; _rc=$?
chk "  装完在 PATH 上叫得动：cc-bus help 退 0、列出各动词" "$_rc|$(printf '%s' "$_hv" | grep -cE '^ +(send|recv|peek|commit|list|whoami|unread|log|stop-hook) ')" "0|9"
env HOME="$IH" PATH="$IH/.local/bin:/usr/bin:/bin" cc-bus nosuchverb >/dev/null 2>&1; _rc=$?
chk "  不认识的动词退 2" "$_rc" "2"
env HOME="$IH" PATH="$IH/.local/bin:/usr/bin:/bin" bash "$SCRIPTS/cc-bus-install.sh" >/dev/null 2>&1
chk "  再装一次：照旧是那三个（幂等）" "$(ls "$IH/.local/bin" | tr '\n' ' ')" "cc-bus cc-recv cc-whoami "

echo "[SH1-b] ★ 驾驶舱读面 —— 名单只认会话 id 形状的收件箱 · 收件箱只看尾巴、不推位置"
# 台架造一个**脏**的收件箱目录（只采结构：名字不是会话 id 的 · 坏行）。
DH="$SANDBOX/dirtyhome"; mkdir -p "$DH"; DB="$(busdir "$DH")"
SDB="b2000000-1111-2222-3333-444444444444"
DHOME="$DH" d --bus-list </dev/null >/dev/null
FROM="$SA" SNDHOME="$DH" snd "$SDB" --act inform --title "占位" "占位一" >/dev/null
printf 'not json\n' >> "$DB/$SDB.jsonl"
FROM="$SA" SNDHOME="$DH" snd "$SDB" --act inform --title "占位" "占位二" >/dev/null
: > "$DB/--help.jsonl"; : > "$DB/a b.jsonl"; printf 'x\n' > "$DB/legacy_name.jsonl"
out="$(DHOME="$DH" d --bus-list </dev/null)"
chk "名单只有那一个会话 id（--help · 带空格的 · 老的按名字的都不算）" "$(printf '%s' "$out" | jq -c '[.agents[].id]')" "[\"$SDB\"]"
chk "  待读数照收件箱行数（坏行也是一行）" "$(printf '%s' "$out" | jq -r '.agents[0].unread')" "3"
_snap="$(cd "$DB" && find . -printf '%p %s %T@\n' | sort | md5sum)"
out="$(jq -cn --arg i "$SDB" '{id:$i, lines:2}' | DHOME="$DH" d --bus-inbox)"
chk "★ 收件箱只看尾巴（末 2 行：一条坏、一条好）" "$(printf '%s' "$out" | jq -c '[.messages[].text, .skipped]')" '["占位二",1]'
chk "★ 读收件箱一个字节不写（不推已读位置）" "$(cd "$DB" && find . -printf '%p %s %T@\n' | sort | md5sum)" "$_snap"
printf '{"id":"--help"}' | DHOME="$DH" d --bus-inbox >/dev/null
chk "★ --help 当收件箱 id ⇒ bad_id" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "bad_id"

echo "[SH1-c] ★ 杀会话不碰 cc-bus 的收件箱"
_dg="$SANDBOX/dg"; mkdir -p "$_dg"
tmux new-session -d -s dg-cc -c /tmp env OUT="$_dg" sh -c 'touch "$OUT/a"; exec sleep 60' 2>/dev/null
wall_until '[ -f "$_dg/a" ]' || true
_before="$(lines_of "$SB")"
_dgh="$(printf '{}' | d --terminals-list | jq -r '.terminals[] | select(.name == "dg-cc") | .terminal')"
out="$(jq -cn --arg t "$_dgh" '{terminal:$t}' | d --terminal-end)"
chk "结束本身照旧成功（按名单句柄交 terminal-end）" "$(printf '%s' "$out" | jq -r .terminal)" "$_dgh"
chk "★ 收件箱没被动" "$(lines_of "$SB")" "$_before"

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
one() { DHOME="$LH" d "$@"; }
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
printf '{"from":"a/b","to":"b","act":"ask","title":"t"}' | one --bus-send >/dev/null; _rc=$?
chk "★ bus-send 拒：非 0 · stderr 正好一行信封" \
  "$([ "$_rc" -ne 0 ] && echo 非0)|$(wc -l < "$SANDBOX/err.txt")|$(jq -r 'select(type=="object") | (.code)' < "$SANDBOX/err.txt" 2>/dev/null)" "非0|1|bad_from"
# 诊断文件那一层目录不在 ⇒ 不替它建（只建文件不建目录），stderr 照样干净。
NL="$SANDBOX/nolog"; mkdir -p "$NL"
DHOME="$NL" d --history-read </dev/null >/dev/null
chk "  没有日志目录：stderr 照样一行 · 不建目录" "$(wc -l < "$SANDBOX/err.txt")|$([ -e "$NL/.cc-monitor" ] && echo 建了 || echo 没建)" "1|没建"

"$REALTMUX" -L "$_SOCK" kill-server 2>/dev/null || true

echo
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ] || exit 1
echo "===== P4f 验收全部通过 ====="
