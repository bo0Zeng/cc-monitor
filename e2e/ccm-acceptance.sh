#!/bin/bash
# ccm CLI 的**真机行为验收**（unify-launch F02）。
#
# 与 e2e/ccm-cli.test.sh 的分工：那个断言「命令串长什么样」（--print），
# 本脚本断言「这条命令在真 tmux 上干了什么」。二者缺一不可
# （F01 教训：三门禁全绿仍放行了一个让 send-keys 完全失效的改动）。
#
# 核心要证的一件事：**账号 env 穿得过 tmux 进程边界**。
# 旧 cct 的做法是外层 export + 自建 tmux + send-keys，而 tmux 的 update-environment
# 默认列表不含 CLAUDE_CONFIG_DIR → export 被整个吃掉。本脚本同时跑**对照组**证明这一点。
#
# 隔离 -L socket + tmux shim + 假 launcher（不起真 claude），不碰任何真实会话。
# 跑法：bash e2e/ccm-acceptance.sh   （npm run test:ccm-acceptance）
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
CCM="$REPO/shared/ccm"
SOCK=ccmF02
TMUX_BIN="$(command -v tmux)" || { echo "需要 tmux"; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"; "$TMUX_BIN" -L "$SOCK" kill-server 2>/dev/null' EXIT

# tmux shim：把 CLI 内部裸调的 `tmux` 全部导到隔离 socket。
BIN="$TMP/bin"; mkdir -p "$BIN"
printf '#!/bin/sh\nexec %s -L %s "$@"\n' "$TMUX_BIN" "$SOCK" > "$BIN/tmux"; chmod +x "$BIN/tmux"
# 假 launcher：不起真 agent（真 claude 会清屏 → 探针假 PASS，F01 踩过），
# 只把「我看到的环境」落盘，供断言。**F04 场景3b 需要它自己的 PID**（供测试往
# `sessions/$PID.json` 里合成一个假会话文件，验证通道B poller 确实会把它读出来）——
# `exec` 保 PID 不变，故 CCMPROBE 里的 `$$` 就是 ccm 脚本里 poller 记的 `$ccm_pid`。
printf '#!/bin/sh\necho $$ >> %s/ccmprobe.pid\nprintf "CFG=%%s\\nPWD=%%s\\nNESTED=%%s\\n" "${CLAUDE_CONFIG_DIR:-<unset>}" "$PWD" "${CLAUDECODE:-<unset>}" >> %s/probe.log\nsleep 5\n' \
  "$TMP" "$TMP" > "$BIN/CCMPROBE"; chmod +x "$BIN/CCMPROBE"
export PATH="$BIN:$PATH"
# **测试自身必须干净**：本脚本可能跑在一个已经带 CLAUDE_CONFIG_DIR 的进程里
# （例如 cc-monitor 的开发者本人正用某个隔离账号跑 Claude Code）。不清掉的话，
# tmux server 会从这里继承，对照组就测不出东西（实测踩过：拿到的是开发者自己的账号目录）。
unset CLAUDE_CONFIG_DIR
# **同时必须脱离 $TMUX**：CLI 在 tmux 内会退化成"就地起"（不建嵌套会话，见 shared/ccm），
# 那时本脚本要验的容器行为根本不发生。生产路径是 `ssh -t … bash -lic`，$TMUX 本就不存在。
unset TMUX TMUX_PANE

# 隔离的账号库 + 配置（不碰用户真实的 ~/.claude-accts）
ACCTS="$TMP/accts"; mkdir -p "$ACCTS/z" "$ACCTS/b"
cat > "$ACCTS/accounts.json" <<JSON
{ "version": 1, "accounts": [
  { "name": "z", "configDir": "$ACCTS/z", "isDefault": true, "mode": "isolated" },
  { "name": "b", "configDir": "$ACCTS/b", "isDefault": false, "mode": "isolated" } ] }
JSON
CFG="$TMP/ccm-config"
printf 'CCM_ACCTS_MANIFEST=%s\nCCM_WORKSPACE=%s\n' "$ACCTS/accounts.json" "$TMP/ws" > "$CFG"
mkdir -p "$TMP/ws" "$TMP/proj"
export CCM_CONFIG="$CFG" CCM_SELF="$CCM"
# F11 Phase D 审计（阻塞项修复）：`--tmux` 建新会话现在会预信任 $cwd（写 ~/.claude.json /
# ~/.codex/config.toml）——本脚本每个场景默认 --agent claude、cwd 是 $TMP 下的隔离临时目录，
# 不隔离的话会真的往开发者/CI 机器的真实全局配置文件里永久写入一堆 /tmp/tmp.XXXXXXXX 垃圾
# trust 条目（已实测复现：连跑两次会分别新增两条不同的垃圾 key）。同 e2e/ccm-pretrust-
# acceptance.sh 的既有隔离手法，指向本次隔离 $TMP 下的副本，不碰真实文件。
export CCM_CLAUDEJSON="$TMP/claude.json" CCM_CODEXTOML="$TMP/config.toml"

T() { "$TMUX_BIN" -L "$SOCK" "$@"; }
reset() { T kill-server 2>/dev/null; : > "$TMP/probe.log"; rm -f "$TMP/ccmprobe.pid"; sleep 0.3; }
opt() { T show-options -v -t "=$1:" "$2" 2>/dev/null; }

# 固定 `sleep N` 等异步副作用是**机器速度的赌注**：本机 3s 够，2 核 CI runner 上不够
# （建会话 → send-keys → tmux fork 新 shell → 跑 CCMPROBE → 写 probe.log 这条链更慢），
# 于是断言读到空串、报成"账号没穿过 tmux 边界"这种**假失败**——首次把本套接进 CI 时实测到
# （PASS=10 FAIL=5，5 条全是 `实得=` 空）。改成轮询等待：慢机器上等够，快机器上第一次就命中，
# 本机总耗时反而比固定 sleep 更短。超时才算真失败。
wait_grep() { # wait_grep <正则> <文件> [超时秒，默认20]
  local pat="$1" f="$2" t="${3:-20}" n=0
  while [ "$n" -lt $((t * 10)) ]; do
    grep -q "$pat" "$f" 2>/dev/null && return 0
    sleep 0.1; n=$((n + 1))
  done
  return 1
}
# 等某个 tmux option 变非空（用于身份打标这类异步写入）。
wait_opt() { # wait_opt <会话名> <option> [超时秒，默认20]
  local s="$1" o="$2" t="${3:-20}" n=0
  while [ "$n" -lt $((t * 10)) ]; do
    [ -n "$(opt "$s" "$o")" ] && return 0
    sleep 0.1; n=$((n + 1))
  done
  return 1
}
# 超时时把每个会话的 pane 文本打出来。**没有这个，超时是一条无信息的死路**：
# 载荷是「send-keys 一条内层 ccm 调用进去」，它失败的原因（命令没找到 / 内层 ccm 自己报错 /
# shell 不对）全都只写在那个 pane 里，测试进程看不见。首次接进 CI 时就吃了这个亏：
# 只知道 probe.log 是空的，不知道为什么空。
dump_panes() {
  echo "      ---- 诊断：pane 内容 ----"
  local s
  for s in $(T ls -F '#{session_name}' 2>/dev/null); do
    echo "      [会话 $s]"
    T capture-pane -t "=$s:" -p 2>/dev/null | sed 's/^/        /' | grep -v '^\s*$'
  done
  echo "      ---- PATH=$PATH"
  echo "      ---- SHELL=${SHELL:-<unset>} tmux default-shell=$(T show-options -gv default-shell 2>/dev/null)"
  echo "      ------------------------"
}
# CCMPROBE 的三个字段由**同一条 printf** 写出，故等最后一个字段 NESTED= 到位即代表整条记录已落盘。
wait_probe() { wait_grep '^NESTED=' "$TMP/probe.log" "${1:-20}" || { dump_panes; return 1; }; }

# 等某个会话名出现。
# **为什么不能用 `wait_probe` 等第二个会话**：它 grep 的是 probe.log 里有没有 `^NESTED=`，
# 而第一次调用写下的那行还在 ⇒ 第二次 `wait_probe` **立刻返回、根本没等**
#（实测：第二个会话还在建，断言就跑了，读到只有一个会话；那个会话随后落到下一个场景的
# reset 之后，把下一条断言也带红）。等会话名是直接的、不受上一次残留影响。
wait_session() {
  local name="$1" to="${2:-20}" i
  for ((i=0; i<to*4; i++)); do
    T has-session -t "=$name:" 2>/dev/null && return 0
    sleep 0.25
  done
  return 1
}

PASS=0; FAIL=0
ck() { if [ "$2" = "$3" ]; then printf 'PASS | %-52s | %s\n' "$1" "$3"; PASS=$((PASS+1))
       else printf 'FAIL | %-52s | 期望=%s 实得=%s\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi; }

echo "===== 场景 1：ccm --tmux --account z —— 账号必须穿过 tmux 边界 ====="
reset
( cd "$TMP/proj" && bash "$CCM" --tmux --account z --launcher CCMPROBE >/dev/null 2>&1 & )
wait_probe || echo "      (注：等 probe 记录超时，下面断言会如实报空)"
ck "会话 cc-proj 被建出" "yes" "$(T has-session -t '=cc-proj:' 2>/dev/null && echo yes || echo no)"
ck "@ccm_agent 打上" "claude" "$(opt cc-proj @ccm_agent)"
ck "**CLAUDE_CONFIG_DIR 穿过了 tmux 边界**" "$ACCTS/z" \
   "$(grep -m1 '^CFG=' "$TMP/probe.log" 2>/dev/null | cut -d= -f2-)"
ck "cwd 正确落到 --cwd 解析值" "$TMP/proj" \
   "$(grep -m1 '^PWD=' "$TMP/probe.log" 2>/dev/null | cut -d= -f2-)"

echo
echo "===== 场景 2（对照组）：旧 cct 的做法 —— 证明它会丢账号 ====="
# **关键前提**：tmux server 必须先由一个**不带该 env 的 shell** 拉起。
# 若 server 恰好由带 env 的 shell 首次拉起，它会继承环境 → 对照组假阴性。
# 这也解释了为什么旧 cct 的账号丢失是**非确定性**的（用户原话「经常出现无法换号」而非「总是」）：
# 取决于 tmux server 是谁起的。update-environment 默认列表不含 CLAUDE_CONFIG_DIR，
# 故 server 一旦在跑，后续 new-session 一律拿不到。
reset
env -u CLAUDE_CONFIG_DIR "$TMUX_BIN" -L "$SOCK" new-session -d -s bootstrap   # 由**确定无该 env** 的 shell 拉起 server
sleep 0.5
(
  cd "$TMP/proj"
  export CLAUDE_CONFIG_DIR="$ACCTS/z"      # 外层 export（旧 cct 就是这样）
  T new-session -d -s old_cc -c "$TMP/proj"
  T send-keys -t '=old_cc:' 'CCMPROBE' Enter
) >/dev/null 2>&1
wait_probe || echo "      (注：等 probe 记录超时，下面断言会如实报空)"
ck "对照组：server 已在跑时，外层 export 被 tmux 边界吃掉" "<unset>" \
   "$(grep -m1 '^CFG=' "$TMP/probe.log" 2>/dev/null | cut -d= -f2-)"
ck "update-environment 默认列表确实不含 CLAUDE_CONFIG_DIR" "" \
   "$(T show-options -g update-environment 2>/dev/null | grep -c CLAUDE_CONFIG_DIR | sed 's/^0$//')"

echo
echo "===== 场景 3a：身份——通道A（意图）打 @ccm_sid_expect，不是 @ccm_sid ====="
reset
( cd "$TMP/proj" && bash "$CCM" --tmux --ccm-sid deadbeef-1234 --launcher CCMPROBE >/dev/null 2>&1 & )
wait_opt cc-proj @ccm_sid_expect || echo "      (注：等 @ccm_sid_expect 超时)"
sleep 2   # 留给 poller 的真实窗口——下一条是负向断言，必须让它"有机会却不该"提升
ck "通道A：已知 sid 建时立刻打 @ccm_sid_expect" "deadbeef-1234" "$(opt cc-proj @ccm_sid_expect)"
ck "F04：@ccm_sid（事实）此时仍未设——CCMPROBE 从未写 sessions/*.json，通道B无可确认之物" \
   "" "$(opt cc-proj @ccm_sid)"

echo
echo "===== 场景 3b：身份——通道B（poller）独立确认后才把 @ccm_sid_expect 提升为 @ccm_sid ====="
reset
( cd "$TMP/proj" && bash "$CCM" --tmux --account z --ccm-sid deadbeef-3b --launcher CCMPROBE >/dev/null 2>&1 & )
wait_grep . "$TMP/ccmprobe.pid" || { echo "      (注：等 CCMPROBE 落 PID 超时)"; dump_panes; }
# CCMPROBE 落盘了自己的 PID（= ccm 脚本 poller 记的 $ccm_pid，exec 保 PID 不变）；
# 合成一份 Claude Code 会话文件，模拟"agent 真的确认在跑这个 sid"。
CCMPROBE_PID="$(head -1 "$TMP/ccmprobe.pid" 2>/dev/null)"
mkdir -p "$ACCTS/z/sessions"
[ -n "$CCMPROBE_PID" ] && printf '{"sessionId":"deadbeef-3b"}' > "$ACCTS/z/sessions/$CCMPROBE_PID.json"
wait_opt cc-proj @ccm_sid || echo "      (注：等通道B提升 @ccm_sid 超时)"
ck "通道B：poller 读到会话文件后，把 @ccm_sid（事实）提升为确认值" "deadbeef-3b" "$(opt cc-proj @ccm_sid)"
ck "@ccm_sid_expect（意图）仍保留，两个 key 独立共存" "deadbeef-3b" "$(opt cc-proj @ccm_sid_expect)"

echo
echo "===== 场景 4：agent 轴 ====="
# 外层先带毒（模拟 issue #24 的 tmux server env 污染），验两个 agent 的清理差异。
reset
( cd "$TMP/proj" && CLAUDECODE=1 bash "$CCM" --tmux --agent codex --launcher CCMPROBE >/dev/null 2>&1 & )
wait_probe || echo "      (注：等 probe 记录超时，下面断言会如实报空)"
ck "@ccm_agent = codex" "codex" "$(opt cc-proj @ccm_agent)"
ck "codex 无嵌套 env 概念 → CLAUDECODE 保留" "1" \
   "$(grep -m1 '^NESTED=' "$TMP/probe.log" 2>/dev/null | cut -d= -f2-)"
reset
( cd "$TMP/proj" && CLAUDECODE=1 bash "$CCM" --tmux --launcher CCMPROBE >/dev/null 2>&1 & )
wait_probe || echo "      (注：等 probe 记录超时，下面断言会如实报空)"
ck "claude 起前清嵌套 env（治 issue #24 带毒）" "<unset>" \
   "$(grep -m1 '^NESTED=' "$TMP/probe.log" 2>/dev/null | cut -d= -f2-)"

echo
echo "===== 场景 5：无显式名 = 无条件新建（2026-07-31 语义变更，用户实测报障后改）====="
#
# **这条断言的期望被刻意反转了。** 原先钉的是「同目录连开两次接回同一会话，不产孤儿」。
# 用户报障：新开一个终端进同一目录敲 `cct`，会被 attach 进那个正被别处用着的会话
#（两个终端镜像同一个窗口、按键互相打架）。「我既然新开一个终端在这个路径 cc 了，
# 我肯定是要新建窗口而非 attach」。
#
# 改的依据不是"用户说了算"，是**那条路径专属终端**：`src/launch-render-cli.ts:95` 显示
# cc-monitor 起会话时永远传 `--tmux=<名>`；无名的 `--tmux` 只出现在诊断文案与别名块
# `cct() { ccm --tmux "$@"; }` 里。⇒ 这条路径上没有任何调用方对名字有期望。
# 显式名那条**仍然幂等**，见紧随其后的对照场景。
reset
( cd "$TMP/proj" && bash "$CCM" --tmux --launcher CCMPROBE >/dev/null 2>&1 & )
wait_probe || echo "      (注：等首个会话 probe 记录超时)"
( cd "$TMP/proj" && bash "$CCM" --tmux --launcher CCMPROBE >/dev/null 2>&1 & )
wait_session cc-proj-2 || echo "      (注：等 cc-proj-2 出现超时)"
ck "同目录连开两次 → 两个会话（cc-proj + cc-proj-2）" "cc-proj cc-proj-2" \
   "$(T ls -F '#{session_name}' 2>/dev/null | sort | tr '\n' ' ' | sed 's/ $//')"
# **第三次**：避让必须会继续往上数。只写死 `-2` 的话第三次会撞回 cc-proj-2 并 attach 进去
# ——正是本次要修的那个病，只是换了个门牌号。
( cd "$TMP/proj" && bash "$CCM" --tmux --launcher CCMPROBE >/dev/null 2>&1 & )
wait_session cc-proj-3 || echo "      (注：等 cc-proj-3 出现超时)"
ck "再开第三次 → cc-proj-3（避让会继续数，不是只到 -2）" "cc-proj cc-proj-2 cc-proj-3" \
   "$(T ls -F '#{session_name}' 2>/dev/null | sort | tr '\n' ' ' | sed 's/ $//')"

echo
echo "===== 场景 5bis：**显式名仍然幂等** —— cc-monitor 那条路不能被上面那条改动带偏 ====="
# cc-monitor 自己算好名字用 `--tmux=<名>` 传进来，期望的就是幂等接回。
# 若上面的无条件避让漏进了显式名分支，这里会冒出一个 cc-fixed-2。
reset
( cd "$TMP/proj" && bash "$CCM" --tmux=cc-fixed --launcher CCMPROBE >/dev/null 2>&1 & )
wait_probe || echo "      (注：等首个会话 probe 记录超时)"
( cd "$TMP/proj" && bash "$CCM" --tmux=cc-fixed --launcher CCMPROBE >/dev/null 2>&1 & )
sleep 2.5   # 幂等接回不再跑 CCMPROBE（无新记录可等），保留固定 grace
ck "显式名连开两次 → 仍只有一个 cc-fixed" "cc-fixed" \
   "$(T ls -F '#{session_name}' 2>/dev/null | sort | tr '\n' ' ' | sed 's/ $//')"

echo
echo "===== 场景 5ter：多开出来的那个会话**也被打上 @ccm_sid**（issue #76 的真「孤儿」判据）====="
#
# 「孤儿」在本仓被用在两个意思上，别混：
#   ① 字面「多出一个你没要的会话」—— 场景 5 反转的就是它，那个会话 monitor 看得见、有 tab
#      （识别 Claude Code 会话靠 pidfile，与 tmux 名无关）。
#   ② **issue #76 的真孤儿**：tmux 会话**没有 `@ccm_sid`** ⇒ cc-monitor attach / 管理不了。
#
# 场景 5 只证明了「会多出一个会话」，**没证明它不是第 ② 种**。ccm 的通道B（poller 按 `$$`
# 找自己的会话文件）理论上对每个 ccm 进程各自成立，但既然 #76 就摆在那儿，这条不该靠推理。
#
# **必须带 `--account z`**：不带的话 poller 读的是**真实 `$HOME/.claude/sessions`**，
# 本测试要往那儿合成会话文件就等于写用户家目录。带上就落到隔离的 $ACCTS/z 里。
reset
( cd "$TMP/proj" && bash "$CCM" --tmux --account z --launcher CCMPROBE >/dev/null 2>&1 & )
wait_session cc-proj || echo "      (注：等 cc-proj 出现超时)"
( cd "$TMP/proj" && bash "$CCM" --tmux --account z --launcher CCMPROBE >/dev/null 2>&1 & )
wait_session cc-proj-2 || echo "      (注：等 cc-proj-2 出现超时)"
wait_grep '^[0-9]*$' "$TMP/ccmprobe.pid" || true
# 等两行 PID 都落盘（两个 CCMPROBE 各追加一行）
for _i in $(seq 1 80); do [ "$(wc -l < "$TMP/ccmprobe.pid" 2>/dev/null || echo 0)" -ge 2 ] && break; sleep 0.25; done
mkdir -p "$ACCTS/z/sessions"
_p1="$(sed -n '1p' "$TMP/ccmprobe.pid" 2>/dev/null)"
_p2="$(sed -n '2p' "$TMP/ccmprobe.pid" 2>/dev/null)"
[ -n "$_p1" ] && printf '{"sessionId":"aaaaaaaa-1111"}' > "$ACCTS/z/sessions/$_p1.json"
[ -n "$_p2" ] && printf '{"sessionId":"bbbbbbbb-2222"}' > "$ACCTS/z/sessions/$_p2.json"
wait_opt cc-proj @ccm_sid   || echo "      (注：等 cc-proj 的 @ccm_sid 超时)"
wait_opt cc-proj-2 @ccm_sid || echo "      (注：等 cc-proj-2 的 @ccm_sid 超时)"
# 两个都要有标记，且**各是各的 sid**（都有值但串了的话，attach 会连到错的那个）
ck "第一个会话有 @ccm_sid" "aaaaaaaa-1111" "$(opt cc-proj @ccm_sid)"
ck "★ 多开出来的第二个会话**也有** @ccm_sid（不是 #76 那种管不了的孤儿）" "bbbbbbbb-2222" \
   "$(opt cc-proj-2 @ccm_sid)"

echo
echo "===== 场景 6：会话名过 cc-monitor 的 is_ccm_tmux_name（F04 之前也能被控制面接受）====="
name="$(T ls -F '#{session_name}' 2>/dev/null | head -1)"
case "$name" in cc-*) r=yes ;; *) r=no ;; esac
ck "会话名以 cc- 开头" "yes" "$r"

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
