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
# ★★ **fail-closed：本套件对 `jq` 也是硬依赖**〔K-C1 D 阶段审计 `I4`，08-24 补〕。
#   `K-C1` 起 `mk_mirror_daemon` 用 `jq` 把夹具 manifest 翻成 `--list-accounts` 帧形状；
#   缺 `jq` 它**不报错、只少吐账号行** ⇒ ccm 拿到空表，症状是 `可用: (无账号库)`，
#   **诊断指向账号库、不指向缺 jq**（场景 1 会红成那个样子）。照上面 tmux 那条同一个纪律。
command -v jq >/dev/null 2>&1 || {
  echo "需要 jq —— 本套件的假 daemon 靠它把夹具 manifest 翻成 --list-accounts 帧形状；"
  echo "     缺它会静默少吐账号行（症状看着像「账号库坏了」）。这是环境缺工具，不是套件退化。"
  exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"; "$TMUX_BIN" -L "$SOCK" kill-server 2>/dev/null' EXIT

# tmux shim：把 CLI 内部裸调的 `tmux` 全部导到隔离 socket。
BIN="$TMP/bin"; mkdir -p "$BIN"
printf '#!/bin/sh\nexec %s -L %s "$@"\n' "$TMUX_BIN" "$SOCK" > "$BIN/tmux"; chmod +x "$BIN/tmux"
# 假 launcher：不起真 agent（真 claude 会清屏 → 探针假 PASS，F01 踩过），
# 只把「我看到的环境」落盘，供断言。**场景 3b / 5ter 需要它自己的 PID**：
# ⚠ `U-NP④`（08-14）之后用途换了 —— 原先是「ccm 的 poller 按 `$$` 找自己的会话文件」，
# 那条 poller 已删；今天这个 PID 是用来**读 `/proc/<pid>/environ` 的 `TMUX_PANE`** 的，
# 也就是 daemon 定位「该把 `@ccm_sid` 打到哪个 tmux 会话」用的那把钥匙。
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
# ★★ `K-C1`（08-24）：**账号解析改走 daemon 之后，本套件必须自己带一份假 daemon。**
#
# 两个理由，都实测过：
#  ① 不带的话，查找次序会摸到 `$HOME/.cc-monitor/bin/cc-monitor-remote` ——
#     开发机上那是**用户的真二进制**（本套件其余每一处都在极力避免碰用户的东西），
#     CI 上它不存在 ⇒ 同一条判据在两处走**两条不同的路**。
#  ② 顺带把本套件对「这台机器装没装 daemon」的隐性依赖去掉：身份前置检查
#     （`U-NP④`：在 tmux 里起 claude 而找不到 daemon ⇒ exit 2）此前是靠**开发机恰好装了**
#     才过的。钉一份假的之后，本套件在裸 CI runner 上也走同一条路。
# ⚠ 这份假 daemon **只答 `--list-accounts`**，一个字节都不写盘 ⇒ 场景 3b 那条负向断言
#   （「ccm 自己不打 @ccm_sid」）射程不变。它也**不进流模式**（那才会往 tmux server 装 hook）。
# ⚠ **单一事实源仍是那份 manifest**：它把 `<accts-dir>/accounts.json` 原样翻成帧形状，
#   不在这里手抄一张账号表（抄了就有两份要同步）。
# ★★ 🔴 〔`K-P2` `F` 拍 09-04；用@「**ccm不要管找不到, 统一走后端**」〕**它现在还要真的建会话。**
#
# 上面那段头注逐字写着「这份假 daemon **只答 `--list-accounts`**，一个字节都不写盘」——
# 那句话今天**必须作废**：`--tmux` 建会话的本地退路删掉了，会话改由后端建。
# 只答账号的话，`--launch` 拿到的是**空回帧** ⇒ ccm 读成「后端答不出」⇒ `exit 4`
# ⇒ 本套件 29 条里 24 条连锁失败（现打过），而它们红的原因与它们要测的东西**毫无关系**。
#
# ⇒ 把 `--launch` 那一格委托给**仓里那份可复用的假后端** `e2e/fake-daemon.sh`：
#   它按 `doc/IPC-PROTOCOL.md` 的 `create-or-attach` 形状真去 `tmux new-session` / 打标 / `send-keys`，
#   顺序与 `remote-daemon-proto/src/control/launch.rs` 的 `CreateOrAttach` 臂逐条同序。
#   ⚠ **不在这里再写一份**：写了就是同一件事两份实现（本套件治的正是那一族）。
#   ⚠ 它调的 `tmux` 走 **PATH** ⇒ 落在本套件那个 `-L $SOCK` 的 shim 上，隔离面一格没变。
# ⚠ **`@ccm_sid` 那条负向断言的射程仍然没变**：这份假后端写的是**意图**标记
#   `@ccm_sid_expect`，与真 daemon 同形；**事实**标记 `@ccm_sid` 它一个字都不写
#   （场景 3b 断言的就是「ccm 自己不打 `@ccm_sid`」）。
mk_mirror_daemon() { # mk_mirror_daemon <落点>
  cat > "$1" <<MIRROR
#!/bin/sh
[ "\$1" = --launch ] && exec "$REPO/e2e/fake-daemon.sh" "\$@"
[ "\$1" = --list-accounts ] || { cat >/dev/null; exit 0; }
d=""; while [ \$# -gt 0 ]; do [ "\$1" = --accts-dir ] && d="\$2"; shift; done
printf '{"accountZeroAware":true,"acctsDir":"%s","count":0,"enabled":true,"error":null,"kind":"accounts-meta","manifestPath":"%s/accounts.json","sharedStore":null,"updatedAt":null}\n' "\$d" "\$d"
jq -c '.accounts[] | {configDir:(.configDir // null),email:"",exists:true,isDefault:(.isDefault // false),loggedIn:false,mode:"isolated",name:.name}' "\$d/accounts.json" 2>/dev/null
exit 0
MIRROR
  chmod +x "$1"
}
# `FAKE_DAEMON_TMUX_SOCK` 与本套件的 `-L $SOCK` **给同一个名字**（理由见 `e2e/fake-daemon.sh` 头注）。
export FAKE_DAEMON_TMUX_SOCK="$SOCK"
mk_mirror_daemon "$BIN/kc1-mirror-daemon"
export CCM_DAEMON_BIN="$BIN/kc1-mirror-daemon"

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
ck "会话 proj-cc 被建出" "yes" "$(T has-session -t '=proj-cc:' 2>/dev/null && echo yes || echo no)"
ck "@ccm_agent 打上" "claude" "$(opt proj-cc @ccm_agent)"
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
wait_opt proj-cc @ccm_sid_expect || echo "      (注：等 @ccm_sid_expect 超时)"
sleep 2   # 留给 poller 的真实窗口——下一条是负向断言，必须让它"有机会却不该"提升
ck "通道A：已知 sid 建时立刻打 @ccm_sid_expect" "deadbeef-1234" "$(opt proj-cc @ccm_sid_expect)"
ck "F04：@ccm_sid（事实）此时仍未设——CCMPROBE 从未写 sessions/*.json，通道B无可确认之物" \
   "" "$(opt proj-cc @ccm_sid)"

echo
echo "===== 场景 3b：身份——通道B**已搬去 daemon**（U-NP④，2026-08-14）====="
#
# ★★ 本场景整段改写过，**不是判据放宽**：
#   它原来验的是「ccm 里那条每秒 poller 读到 `sessions/<PID>.json` 之后把 `@ccm_sid` 打上」。
#   用户 08-14 裁定「**可以动ccm. 不要轮询**」＋「**ccm做到必须走daemon**」⇒ 那条 poller
#   被**整条删掉**（连同解析器 `_ccm_sid_from_file`），`@ccm_sid` 改由 daemon 的
#   `remote-daemon-proto/src/control/identity_tag.rs` 在 pidfile inotify 事件上打。
#
# ⚠ **本套件不起 daemon**（硬约束：daemon 一进流模式就往**用户真实 tmux server** 装三条
#   全局 hook，槽位 [50]、装即覆盖）。所以这里能验、且必须验的是两件**独立于 daemon 的**事实：
#     ① **ccm 自己确实不再打 `@ccm_sid` 了**（负向：会话文件就摆在那儿，它也不该动）——
#        这是「poller 真没了」在真 tmux 上的读数，比读源码强；
#     ② **daemon 打标所需的那把钥匙在**：pane 里那个进程的 `/proc/<pid>/environ` 带
#        `TMUX_PANE`，且它解析回来的正是这个会话。daemon 的 join 就是这一步
#        （`identity_tag::pane_of` → `gate::probe`）。钥匙在 = 那半接得上。
#   ③ 顺带**照着 daemon 的做法**手工打一次标，验「打完之后 `@ccm_sid` 与窗口标题都对」。
#      这一格是机制的端到端，只是把 Rust 那 20 行换成等价的三条 tmux 命令。
reset
( cd "$TMP/proj" && bash "$CCM" --tmux --account z --ccm-sid deadbeef-3b --launcher CCMPROBE >/dev/null 2>&1 & )
wait_grep . "$TMP/ccmprobe.pid" || { echo "      (注：等 CCMPROBE 落 PID 超时)"; dump_panes; }
CCMPROBE_PID="$(head -1 "$TMP/ccmprobe.pid" 2>/dev/null)"
mkdir -p "$ACCTS/z/sessions"
[ -n "$CCMPROBE_PID" ] && printf '{"sessionId":"deadbeef-3b"}' > "$ACCTS/z/sessions/$CCMPROBE_PID.json"
sleep 2   # 给足「旧 poller 若还在，它早该打上了」的窗口 —— 下一条是负向断言
ck "★ 通道B 已不在 ccm 里：会话文件就摆在那儿，ccm 也**不**打 @ccm_sid（poller 真的没了）" \
   "" "$(opt proj-cc @ccm_sid)"
ck "@ccm_sid_expect（意图）照旧由 ccm 打 —— 两个 key 的分离一个字没改" "deadbeef-3b" \
   "$(opt proj-cc @ccm_sid_expect)"
# ② daemon 的 join 钥匙：`/proc/<pid>/environ` 的 `TMUX_PANE`
_pane=""
[ -n "$CCMPROBE_PID" ] && [ -r "/proc/$CCMPROBE_PID/environ" ] && \
  _pane="$(tr '\0' '\n' < "/proc/$CCMPROBE_PID/environ" 2>/dev/null | sed -n 's/^TMUX_PANE=//p' | head -1)"
case "$_pane" in %[0-9]*) _shape=ok ;; *) _shape="bad:[$_pane]" ;; esac
ck "★ daemon 的 join 钥匙在：agent 进程的 environ 带 TMUX_PANE（%N 形态）" "ok" "$_shape"
ck "★ 那把钥匙解析回来正是这个会话（daemon 就是这么定位「该打哪儿」的）" "proj-cc" \
   "$([ -n "$_pane" ] && T display-message -p -t "$_pane" '#{session_name}' 2>/dev/null)"
# ③ 照 daemon 的做法手工打一次（三条命令 = identity_tag.rs 的等价物）
if [ -n "$_pane" ]; then
  _sess="$(T display-message -p -t "$_pane" '#{session_id}' 2>/dev/null)"
  [ -n "$_sess" ] && T set-option -t "$_sess" @ccm_sid deadbeef-3b >/dev/null 2>&1
fi
ck "★ 按 daemon 的算法打完，@ccm_sid（事实）就位" "deadbeef-3b" "$(opt proj-cc @ccm_sid)"
ck "★ 打完之后窗口标题 marker 立刻成立（tmux 自己按 set-titles-string 合成，不靠谁重打）" \
   "ccm-rbind-deadbeef-3b" \
   "$(T display-message -p -t '=proj-cc:' '#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}' 2>/dev/null)"

echo
echo "===== 场景 4：agent 轴 ====="
# 外层先带毒（模拟 issue #24 的 tmux server env 污染），验两个 agent 的清理差异。
reset
( cd "$TMP/proj" && CLAUDECODE=1 bash "$CCM" --tmux --agent codex --launcher CCMPROBE >/dev/null 2>&1 & )
wait_probe || echo "      (注：等 probe 记录超时，下面断言会如实报空)"
ck "@ccm_agent = codex" "codex" "$(opt proj-cc @ccm_agent)"
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
wait_session proj-cc-2 || echo "      (注：等 proj-cc-2 出现超时)"
ck "同目录连开两次 → 两个会话（proj-cc + proj-cc-2）" "proj-cc proj-cc-2" \
   "$(T ls -F '#{session_name}' 2>/dev/null | sort | tr '\n' ' ' | sed 's/ $//')"
# **第三次**：避让必须会继续往上数。只写死 `-2` 的话第三次会撞回 proj-cc-2 并 attach 进去
# ——正是本次要修的那个病，只是换了个门牌号。
( cd "$TMP/proj" && bash "$CCM" --tmux --launcher CCMPROBE >/dev/null 2>&1 & )
wait_session proj-cc-3 || echo "      (注：等 proj-cc-3 出现超时)"
ck "再开第三次 → proj-cc-3（避让会继续数，不是只到 -2）" "proj-cc proj-cc-2 proj-cc-3" \
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
echo "===== 场景 5ter：多开出来的那个会话**也认得出**（issue #76 的真「孤儿」判据）====="
#
# 「孤儿」在本仓被用在两个意思上，别混：
#   ① 字面「多出一个你没要的会话」—— 场景 5 反转的就是它，那个会话 monitor 看得见、有 tab
#      （识别 Claude Code 会话靠 pidfile，与 tmux 名无关）。
#   ② **issue #76 的真孤儿**：tmux 会话**没有 `@ccm_sid`** ⇒ cc-monitor attach / 管理不了。
#
# 场景 5 只证明了「会多出一个会话」，**没证明它不是第 ② 种**。这条不该靠推理。
#
# ★★ `U-NP④`（08-14）之后本场景的**判法**变了，要防的东西一个字没变：
#   身份不再由 ccm 自己打（那条每秒 poller 已删），而由 daemon 按
#   `/proc/<pid>/environ` 的 `TMUX_PANE` 定位后打。⇒ 「第二个会话会不会变成 #76 孤儿」
#   这个问题，在本套件（**刻意不起 daemon**，见场景 3b 头注）上的等价形式是：
#   **两个会话各自的 agent 进程都带着能解析回自己会话的 `TMUX_PANE`**，
#   且**两把钥匙指向的是两个不同的会话**（串了的话 attach 会连到错的那个 —— 与旧判据同一个后果）。
#
# **仍然必须带 `--account z`**：不带的话下面合成的会话文件会落到**真实 `$HOME/.claude/sessions`**。
reset
( cd "$TMP/proj" && bash "$CCM" --tmux --account z --launcher CCMPROBE >/dev/null 2>&1 & )
wait_session proj-cc || echo "      (注：等 proj-cc 出现超时)"
( cd "$TMP/proj" && bash "$CCM" --tmux --account z --launcher CCMPROBE >/dev/null 2>&1 & )
wait_session proj-cc-2 || echo "      (注：等 proj-cc-2 出现超时)"
wait_grep '^[0-9]*$' "$TMP/ccmprobe.pid" || true
# 等两行 PID 都落盘（两个 CCMPROBE 各追加一行）
for _i in $(seq 1 80); do [ "$(wc -l < "$TMP/ccmprobe.pid" 2>/dev/null || echo 0)" -ge 2 ] && break; sleep 0.25; done
mkdir -p "$ACCTS/z/sessions"
_p1="$(sed -n '1p' "$TMP/ccmprobe.pid" 2>/dev/null)"
_p2="$(sed -n '2p' "$TMP/ccmprobe.pid" 2>/dev/null)"
[ -n "$_p1" ] && printf '{"sessionId":"aaaaaaaa-1111"}' > "$ACCTS/z/sessions/$_p1.json"
[ -n "$_p2" ] && printf '{"sessionId":"bbbbbbbb-2222"}' > "$ACCTS/z/sessions/$_p2.json"
_pane_of() { # $1=pid → stdout: TMUX_PANE（拿不到就空）
  [ -n "${1:-}" ] && [ -r "/proc/$1/environ" ] && \
    tr '\0' '\n' < "/proc/$1/environ" 2>/dev/null | sed -n 's/^TMUX_PANE=//p' | head -1
}
_n1="$(T display-message -p -t "$(_pane_of "$_p1")" '#{session_name}' 2>/dev/null)"
_n2="$(T display-message -p -t "$(_pane_of "$_p2")" '#{session_name}' 2>/dev/null)"
ck "第一个会话的 agent 进程解析回自己的会话" "proj-cc" "$_n1"
ck "★ 多开出来的第二个会话**也解析得回自己**（不是 #76 那种管不了的孤儿）" "proj-cc-2" "$_n2"
ck "★ 两把钥匙**不许串**（串了 attach 会连到错的那个 —— 与旧判据同一个后果）" "differ" \
   "$([ -n "$_n1" ] && [ "$_n1" != "$_n2" ] && echo differ || echo same)"
# 照 daemon 的算法各打各的，验「两个会话拿到的是各自的 sid」——这正是旧判据的那一格。
for _pair in "$_p1:aaaaaaaa-1111" "$_p2:bbbbbbbb-2222"; do
  _pp="${_pair%%:*}"; _ss="${_pair##*:}"
  _pn="$(_pane_of "$_pp")"
  [ -n "$_pn" ] && T set-option -t "$(T display-message -p -t "$_pn" '#{session_id}' 2>/dev/null)" \
      @ccm_sid "$_ss" >/dev/null 2>&1
done
ck "按 daemon 的算法打完：第一个会话拿到自己的 sid" "aaaaaaaa-1111" "$(opt proj-cc @ccm_sid)"
ck "★ 按 daemon 的算法打完：第二个会话拿到的是**它自己的** sid（不是第一个的）" "bbbbbbbb-2222" \
   "$(opt proj-cc-2 @ccm_sid)"

echo
echo "===== 场景 6：会话名过 cc-monitor 的 is_ccm_tmux_name（F04 之前也能被控制面接受）====="
name="$(T ls -F '#{session_name}' 2>/dev/null | head -1)"
# S4b-3b：命名反转成 `<X>-cc`（含撞名的 `<X>-cc-<N>`）。Rust 侧 `is_ccm_tmux_name`
# 同时认新后缀与老前缀（老会话还在跑，不能不认），这里钉的是**新产的名字**。
case "$name" in *-cc|*-cc-[0-9]*) r=yes ;; *) r=no ;; esac
ck "会话名以 -cc 结尾（新命名）" "yes" "$r"

echo
echo "===== 场景 7：账号解析走 daemon（K-C1）—— **真起会话**那条路，不是 --print ====="
#
# ★ 与 `e2e/ccm-cli.test.sh` 那一节的分工：那节全走 `--print`（离线预言机）。
#   本场景验的是**真的建了 tmux 会话、载荷真的被 send-keys 打进去、agent 进程真的拿到了
#   那个 `CLAUDE_CONFIG_DIR`** —— 而账号解析在这条路上发生**两次**
#   （外层建容器前先校验一次、内层注入时再一次），`--print` 一次都答不了「内层那次拿到了什么」。
#   件计划 `KCY1` 点名的第二个失效面就是这个（「`--print` 那条路与真起会话那条路可能不同源」）。
#
# ★ 夹具的要害同 `ccm-cli`：**daemon 与 manifest 必须答不同的目录**。
#   manifest 里 z 指向 `$ACCTS/z`，这个假 daemon 让 z 指向 `$ACCTS/z-daemon`
#   ⇒ probe.log 里出现哪一个，就说明**内层那次**读的是哪一边。
#   （上面场景 1–6 用的是**镜像**假 daemon：两边同值 ⇒ 那些断言钉的是「值穿过了 tmux 边界」，
#     **不是** provenance。别把它们当 provenance 判据。）
mkdir -p "$ACCTS/z-daemon"
cat > "$BIN/kc1-diff-daemon" <<STUB
#!/bin/sh
# 〔\`K-P2\` \`F\` 拍 09-04〕\`--launch\` 委托给仓里那份可复用的假后端（理由同 \`mk_mirror_daemon\`：
# 会话改由后端建了，只答账号的话这两条场景连会话都起不来）。
[ "\$1" = --launch ] && exec "$REPO/e2e/fake-daemon.sh" "\$@"
if [ "\$1" = --list-accounts ]; then
  printf '%s\n' '{"accountZeroAware":true,"acctsDir":"x","count":1,"enabled":true,"error":null,"kind":"accounts-meta","manifestPath":"x","sharedStore":null,"updatedAt":null}'
  printf '%s\n' '{"configDir":"$ACCTS/z-daemon","email":"","exists":true,"isDefault":true,"loggedIn":false,"mode":"isolated","name":"z"}'
  exit 0
fi
cat >/dev/null
exit 0
STUB
chmod +x "$BIN/kc1-diff-daemon"
# ★ 自检必须问「daemon **实际答了什么**」，不是比两个路径字面量（那两个字符串恒不相等 ⇒
#   「把假 daemon 改成答与 manifest 相同的目录」那一刀在它眼里毫无变化、恒绿）。
#   `e2e/ccm-cli.test.sh` 那节的同款自检 08-24 就是这么栽的，三处一起改。
ck "场景7 · 夹具自检：daemon **实际答的** z 的 configDir 与 manifest 里那个刻意不同" "differ" \
   "$(_mf="$(jq -r '.accounts[]|select(.name=="z")|.configDir' "$ACCTS/accounts.json" 2>/dev/null)"
      _dm="$("$BIN/kc1-diff-daemon" --list-accounts --accts-dir "$ACCTS" 2>/dev/null \
             | jq -r 'select(.name=="z")|.configDir' 2>/dev/null)"
      if [ -z "$_mf" ] || [ -z "$_dm" ]; then echo "抽取器坏了:[mf=$_mf][dm=$_dm]"
      elif [ "$_mf" != "$_dm" ]; then echo differ; else echo "same:[$_dm]"; fi)"
reset
CCM_DAEMON_BIN="$BIN/kc1-diff-daemon" \
  bash -c "cd '$TMP/proj' && CCM_DAEMON_BIN='$BIN/kc1-diff-daemon' bash '$CCM' --tmux --account z --launcher CCMPROBE >/dev/null 2>&1 &"
wait_probe || true
ck "★ 场景7 · 真起会话：内层拿到的 CLAUDE_CONFIG_DIR 来自 **daemon**（不是 manifest）" \
   "CFG=$ACCTS/z-daemon" "$(grep '^CFG=' "$TMP/probe.log" | head -1)"
# ★★ 🔴 反向那一格〔`K-P2` `F` 拍 09-04 换了对照物〕：**只有正向那一格时，「值恒是那个」也能绿。**
#
# 从前这里用 `CCM_NO_DAEMON=1`（「明示关掉 ⇒ 落回 manifest 那份」）。
# 用@09-04「统一走后端」之后**没有 manifest 那份可落**了：关掉之后 `exit 4`，会话根本起不来
# ⇒ 照旧写的话，这一格读到的是空串，而它标签说的是 provenance。
# ⇒ 换成**换一份后端**：镜像那份（`kc1-mirror-daemon`，答的是 manifest 里那个 `$ACCTS/z`）
#   vs 差异那份（`kc1-diff-daemon`，答 `$ACCTS/z-daemon`）—— **同一条代码路径，只有输入不同**，
#   而 provenance 正是后者。⚠ 这比从前**更贴**：从前换的是「有没有后端」（两条不同的路径）。
reset
bash -c "cd '$TMP/proj' && CCM_DAEMON_BIN='$BIN/kc1-mirror-daemon' bash '$CCM' --tmux --account z --launcher CCMPROBE >/dev/null 2>&1 &"
wait_probe || true
ck "★ 场景7 · 反向：**换一份后端**（答 manifest 那个目录）⇒ 真起会话拿到的值跟着换" \
   "CFG=$ACCTS/z" "$(grep '^CFG=' "$TMP/probe.log" | head -1)"
# 🔴 补一格：**`CCM_NO_DAEMON=1` 不再是逃生口** —— 真机上它也只是把失败原因换了一格。
#   这一格把「关掉后端会怎样」的今天的答案钉在**真 tmux** 上，免得下一个人照旧以为「落回文件」。
reset
CCM_NO_DAEMON=1 CCM_DAEMON_BIN="$BIN/kc1-diff-daemon" \
  bash "$CCM" --tmux --account z --launcher CCMPROBE >/dev/null 2>"$TMP/nod.err"; _nod_rc=$?
ck "★ 场景7 · CCM_NO_DAEMON=1 ⇒ 真机上也是**响亮失败**（rc=4，不再落回 manifest）" "4" "$_nod_rc"
ck "★ 场景7 · 而且**一个会话都没建出来**（退路真的没了，不是「建了但没账号」）" "" \
   "$(T ls -F '#{session_name}' 2>/dev/null)"

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
