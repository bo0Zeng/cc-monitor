#!/usr/bin/env bash
# B02 验收：`cc-spawn` 收编进 `ccm` 之后，行为仍与收编前等价。
#
# **本机安全（这条最重要，血的教训）**：开发机上住着**正在运行**的 tmux 会话与真实
# `~/.cc-bus/`（跑这套件的那个 CC 实例自己就在其中一个 tmux 会话里）。本套件因此：
#   ① 一律经 PATH 上的 `tmux` shim 强制 `-L $SOCK`——`cc-spawn`/`ccm`/`cc-register`
#      内部都是裸调 `tmux`，塞不进 `-L`，只能用 shim 拦；
#   ② **起飞前自检用 canary 双向断言**（见下方 preflight）——不是"没看到默认会话"这种
#      否定式（会因 0 个会话、会话名含空格、`tmux ls` 本身失败而空转恒绿），
#      而是"隔离 socket 上建一个 canary，断言默认 socket 看不到它 **且** shim 看得到它"，
#      两向都必须非空。任一向不成立就 exit 9；
#   ③ `CC_BUS_HOME`、`HOME`、`CCM_CLAUDEJSON`、`CCM_CODEXTOML` 全部指向临时目录，
#      绝不碰真实 `~/.cc-bus/` 与真实 `~/.claude.json`；
#   ④ 清理只用 `tmux -L $SOCK kill-server`（**永远带 socket 名**）。
#      裸 `tmux kill-server` 在本套件里是禁用词——它会连开发机上正在跑的会话一起杀掉。
#   ⑤ 启动器一律是假的（纯 sleep 脚本），绝不起真的已认证 claude/codex。
set -o pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
CCSPAWN="$REPO/shared/cc-bus/scripts/cc-spawn"
SOCK="ccmB02e2e$$"
# **`exit 1` 而不是 `exit 0`**（Phase G 审阅阻塞）：这里原先是 `echo "SKIP: 未装 tmux"; exit 0`,
# 于是在没有 tmux 的环境里 20 条断言一条不跑、套件报绿。同类的另外 7 套一律 `exit 1`。
# 一套能在零断言下报绿的套件，正好抵消掉 CI 里为它写的立项理由（"cargo/npm/tsc 全绿仍放行过
# 一个让 send-keys 完全失效的改动，因为那些门禁只断言我写出了打算写的字符串"）。
TMUX_BIN="$(command -v tmux)" || { echo "需要 tmux"; exit 1; }
REALTMUX="$TMUX_BIN"

BIN="$(mktemp -d)"; SANDBOX="$(mktemp -d)"; WORK="$(mktemp -d)"
cat > "$BIN/tmux" << EOF
#!/bin/bash
exec "$REALTMUX" -L $SOCK "\$@"
EOF
chmod +x "$BIN/tmux"
export PATH="$BIN:$PATH"
export CC_BUS_HOME="$SANDBOX/cc-bus"
# 预信任会写这两个文件——重定向到沙箱，绝不碰真实用户配置。
export CCM_CLAUDEJSON="$SANDBOX/claude.json"
export CCM_CODEXTOML="$SANDBOX/codex-config.toml"
# ★★ 🔴 〔`K-P2` `F` 拍 09-04；用@「**ccm不要管找不到, 统一走后端**」〕**本套件必须自带一份后端。**
#
# `cc-spawn` 起会话是**经 `ccm --tmux-base`** 的（这套件的正题就是那条上提）。
# 而 `ccm` 的两条腿 —— **账号解析** 与 **`--tmux` 建会话** —— 从此都没有本地退路：
# 问不到后端就 `exit 4`。不带后端的话，本套件 72 条里 41 条连锁失败（现打过），
# 而它们红的原因（这台机器没装后端）与它们要测的东西（cc-spawn 有没有把活交给 ccm）**无关**。
# ⇒ 与 `tmux` shim / 假 launcher 同一条既有纪律：**要测的变量之外的东西，套件自己钉住**。
# ⚠ `e2e/fake-daemon` 里的 `tmux` 走 **PATH** ⇒ 落在上面那个 `-L $SOCK` 的 shim 上，
#   隔离面一格没变（它碰不到用户的 tmux server）。
# ⚠ `CCM_ACCTS_MANIFEST` 指向一个**不存在的**隔离路径（形态仍是 `<目录>/accounts.json`）：
#   后端对它答「meta ＋ 零个账号」= 空表 ⇒ ccm 退化为基座启动器、不注入账号，
#   本套件要测的那一面因此干净；同时**绝不摸**开发者真实的 `~/.claude-accts`。
export CCM_DAEMON_BIN="$REPO/e2e/fake-daemon"
export CCM_ACCTS_MANIFEST="$SANDBOX/no-accts/accounts.json"

# ⚠⚠ **`set +e` 是这里的第一条**〔08-13 实测〕：本套件在 `:338` 之后 `set -e` 是**开着**的，
# 而清理里 `kill-server` 打在**可能不存在**的 socket 上（`${SOCK}b` 只在某一格才建 server）
# ⇒ 那条返回 1 ⇒ **trap 被 set -e 中途打断**：`rm -rf` 根本没跑到（临时目录泄漏），
# 且整套的退出码变成 1 —— 60 格全 PASS、打印「全部通过」，而 `assert-pass-floor`（fail-closed）
# 判它失败。**套件的裁决被清理绑架了**，这正是 gate-integrity 要防的那类事。
# ★ 惯例本来就在：`daemon-gate2` / `graylight-suite` / `graylight-daemon-frames` /
#   `restart-daemon-frames` 四套的 cleanup 第一行都是 `set +e`，只有本套漏了。
#   ⇒ 量完人群是 1，**不扩登记表**，照同一个形状补上即可。
cleanup() {
  set +e
  "$REALTMUX" -L "$SOCK" kill-server 2>/dev/null
  "$REALTMUX" -L "${SOCK}b" kill-server 2>/dev/null
  rm -rf "$BIN" "$SANDBOX" "$WORK"
}
trap cleanup EXIT

# ===== 起飞前自检（红线守卫）：canary 双向断言 =====
# 否定式守卫（"没看到默认会话"）有三个恒绿入口，B02 审计逐条实测过：默认 socket 上
# 0 个会话 → 循环空转；会话名含空格 → `for n in $(...)` 分词后对不上；`tmux ls` 本身
# 失败 → 没人看 rc。改成肯定式：两个方向都必须**观测到确定的东西**，无法空转。
preflight() {
  local canary="ccmB02canary$$"
  tmux new-session -d -s "$canary" -c /tmp 2>/dev/null \
    || { echo "FATAL 自检无法在隔离 socket 上建 canary 会话"; exit 9; }
  # 正向：shim 必须看得见 canary（证明 shim 确实连着我们以为的那个 server）
  tmux has-session -t "=$canary" 2>/dev/null \
    || { echo "FATAL 自检：shim 看不见自己刚建的 canary —— shim 没连上隔离 socket"; exit 9; }
  # 反向：默认 socket **绝不能**看得见它（证明隔离真的成立）
  if "$REALTMUX" has-session -t "=$canary" 2>/dev/null; then
    echo "FATAL 隔离失效：默认 socket 上出现了 canary '$canary' —— 立刻中止，"
    echo "      绝不在开发机的真 socket 上跑测试（那里住着正在运行的会话）"
    exit 9
  fi
  tmux kill-session -t "=$canary" 2>/dev/null
  echo "[自检] canary 双向断言通过：隔离生效（-L $SOCK）"
}
preflight
case "$CC_BUS_HOME" in "$HOME"/.cc-bus*) echo "FATAL CC_BUS_HOME 指向真实总线"; exit 9 ;; esac
echo "[自检] CC_BUS_HOME=$CC_BUS_HOME  CCM_CLAUDEJSON=$CCM_CLAUDEJSON"

fail=0
pass=0
# G-A：`pass` 计数是**门禁的一部分**，不是装饰——此前这套只在末尾说一句「全部通过」,
# 删掉几条断言它照样绿（门禁的天然失效模式就是静默缩水）。格式与另外 7 套逐字一致,
# 好让 `e2e/assert-pass-floor.sh` 用同一条正则抓。
chk() { if [ "$2" = "$3" ]; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1: 期望[$3] 实得[$2]"; fail=$((fail+1)); fi; }
# 等某个文件出现（每个场景用独立文件名，避免上一场景的残留让等待恒真）
waitfor() { local f="$1"; for _ in $(seq 40); do [ -s "$f" ] && return 0; sleep 0.5; done; return 1; }

# 假启动器：记录 会话名/收到的位置参数/CC_BUS_ID/cwd，然后常驻（模拟 agent 起来了）
mkdir -p "$WORK/proj"
cat > "$BIN/FAKEAGENT" << 'EOF'
#!/bin/bash
out="$WORK/rec-$(tmux display-message -p '#S' 2>/dev/null || echo nosess).txt"
{ printf 'sess=%s\n' "$(tmux display-message -p '#S' 2>/dev/null)"
  printf 'args=%s\n' "$*"
  printf 'busid=%s\n' "${CC_BUS_ID:-<unset>}"
  printf 'cwd=%s\n' "$PWD"
  printf 'wrapenv=%s\n' "${WRAPVAR:-<unset>}"; } > "$out"
sleep 300
EOF
chmod +x "$BIN/FAKEAGENT"
export WORK
export CCSPAWN_LAUNCH="$BIN/FAKEAGENT"

echo "[1] cc-spawn 经 ccm 建会话并返回（不挂在 attach 上）"
s=$(date +%s)
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" "$WORK/proj" "分析这个项目的架构" > "$WORK/out1.txt" 2>&1
rc=$?; e=$(date +%s)
chk "退出码 0" "$rc" "0"
# G-A：这条**手搓的**判定绕开了 `chk`，于是它既不计数也不在门禁的账里
#（实测：输出有 21 行 PASS，而计数只到 20 —— 差的就是这一条）。改成走 chk。
chk "未挂起（$((e-s))s < 15s）" "$([ $((e-s)) -lt 15 ] && echo YES || echo NO)" "YES"
chk "会话名 proj_cc 存在" "$(tmux has-session -t '=proj_cc' 2>/dev/null && echo YES || echo NO)" "YES"

echo "[2] --tmux-size 生效（原 cc-spawn 的 -x 220 -y 50 没被丢掉）"
chk "窗口 220x50" "$(tmux list-windows -t '=proj_cc' -F '#{window_width}x#{window_height}' 2>/dev/null)" "220x50"

echo "[3] 初始任务作为位置参数送达启动器 + 会话真的在指定目录"
waitfor "$WORK/rec-proj_cc.txt" || true
chk "启动器收到任务原文" "$(sed -n 's/^args=//p' "$WORK/rec-proj_cc.txt" 2>/dev/null)" "分析这个项目的架构"
# M12：cc-spawn 的核心承诺是「**在该目录**开会话」，之前没有任何断言守这条
chk "pane 工作目录" "$(tmux display-message -p -t '=proj_cc:' '#{pane_current_path}' 2>/dev/null)" "$WORK/proj"

echo "[4] 台账与总线登记仍是 cc-spawn 自己的活（未随收编丢掉）"
chk "spawned.tsv 有记录" "$(grep -c '^proj_cc	' "$CC_BUS_HOME/spawned.tsv" 2>/dev/null || true)" "1"
chk "agents.tsv 已登记" "$(cut -f1 "$CC_BUS_HOME/agents.tsv" 2>/dev/null | grep -cx 'proj_cc' || true)" "1"

# ★★ 〔`P4b` 08-12 改了语义，本段 08-13 跟改〕**默认不再复用**。
# 用户逐字（`C14`）：「所有起会话就是起会话……**spawn 就是起, 就是 creat**」。
# `P4b` 把 `cc-spawn` 里那段「到就用、没有才建」删掉了 —— 而本段一直在测**被删掉的那个行为**，
# 于是每跑必败 2 条。⚠ **没人发现，是因为这套 e2e 从来没被跑过**（`ROADMAP §5 3x` 那一族）。
# ⇒ 本段改成钉新语义：同目录再 spawn **必须新建并避让到 `-2`**。
echo "[5] 默认新建（C14）：同目录再 spawn 建出 proj_cc-2，命名避让生效"
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" "$WORK/proj" "第二个任务" > "$WORK/out2.txt" 2>&1
chk "输出不再说复用" "$(grep -c '复用已有会话' "$WORK/out2.txt")" "0"
chk "新建了 proj_cc-2" "$(tmux has-session -t '=proj_cc-2' 2>/dev/null && echo YES || echo NO)" "YES"

# `--new` 保留为**兼容用的 no-op**（`P4b-Y1`：外面可能有人在传它，让它报错等于弄坏别人的脚本）。
# ⇒ 它今天与不带旗标**行为一致**：再避让一格到 `-3`。
echo "[6] --new 仍被接受（no-op 兼容），继续避让到 -3"
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" --new "$WORK/proj" > "$WORK/out3.txt" 2>&1
chk "proj_cc-3 建起来了" "$(tmux has-session -t '=proj_cc-3' 2>/dev/null && echo YES || echo NO)" "YES"

echo "[7] codex 下 CC_BUS_ID 由 ccm 自动派生 = 会话名（不需要 --bus-id）"
mkdir -p "$WORK/cx"
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" --tool codex "$WORK/cx" > "$WORK/out4.txt" 2>&1
waitfor "$WORK/rec-cx_cc.txt" || true
chk "会话内 CC_BUS_ID" "$(sed -n 's/^busid=//p' "$WORK/rec-cx_cc.txt" 2>/dev/null)" "cx_cc"

echo "[8] 【审计重要-3】父环境里的 CC_BUS_ID 不得被继承（否则子 agent 冒用父身份读父 inbox）"
# **必须用一个全新的 socket**：污染路径的前提是 tmux **server 由带着 CC_BUS_ID 的那次调用
# 启动**（server 的全局环境继承自启动它的客户端）。上面的 $SOCK 早在 preflight 建 canary 时
# 就起了 server，此时再设 CC_BUS_ID 根本进不到 pane 环境里——我第一版就是这么写的，
# 结果把 `${CC_BUS_ID:-…}` 改回去测试照样绿，是个不折不扣的安慰剂（失效模式②：变异语义无效）。
SOCK8="${SOCK}b"
BIN8="$(mktemp -d)"
cat > "$BIN8/tmux" << EOF
#!/bin/bash
exec "$REALTMUX" -L $SOCK8 "\$@"
EOF
chmod +x "$BIN8/tmux"
mkdir -p "$WORK/inh"
(
  export PATH="$BIN8:$PATH"
  # 该 socket 上尚无 server → 这次调用会**现起**一个，从而把 CC_BUS_ID 带进 server 全局环境
  CCM_NO_PRETRUST=1 CC_BUS_ID=STALEPARENT timeout 30 "$CCSPAWN" --tool codex "$WORK/inh" > "$WORK/out5.txt" 2>&1
)
waitfor "$WORK/rec-inh_cc.txt" || true
# 先确认污染前提真的成立（否则这条测试又是安慰剂）
chk "前提：server 全局环境确被污染" \
  "$("$REALTMUX" -L "$SOCK8" show-environment -g CC_BUS_ID 2>/dev/null | grep -c '^CC_BUS_ID=STALEPARENT$' || true)" "1"
chk "CC_BUS_ID 应为会话名而非继承值" "$(sed -n 's/^busid=//p' "$WORK/rec-inh_cc.txt" 2>/dev/null)" "inh_cc"
"$REALTMUX" -L "$SOCK8" kill-server 2>/dev/null; rm -rf "$BIN8"

echo "[9] 【审计阻塞-1】预信任**未生效**时仍须成功建会话+上总线（此前恒 rc=1 留孤儿）"
# 不设 CCM_NO_PRETRUST：让预信任真的跑；把 CCM_CLAUDEJSON 指到不存在的路径使其失败，
# 于是信任框轮询子句被挂上——那正是 rc 泄漏的来源。
mkdir -p "$WORK/pt"
CCM_CLAUDEJSON="$SANDBOX/definitely-absent.json" timeout 40 "$CCSPAWN" "$WORK/pt" "任务P" > "$WORK/out6.txt" 2>&1
rc9=$?
chk "cc-spawn 退出码 0（不得谎报失败）" "$rc9" "0"
chk "会话存在" "$(tmux has-session -t '=pt_cc' 2>/dev/null && echo YES || echo NO)" "YES"
chk "已写台账（孤儿检测）" "$(grep -c '^pt_cc	' "$CC_BUS_HOME/spawned.tsv" 2>/dev/null || true)" "1"
chk "已上总线（孤儿检测）" "$(cut -f1 "$CC_BUS_HOME/agents.tsv" 2>/dev/null | grep -cx 'pt_cc' || true)" "1"

echo "[10] 【审计阻塞-2】多词 CCSPAWN_LAUNCH（cc-bus-install.sh:96 文档化的用法）"
mkdir -p "$WORK/wrap"
CCM_NO_PRETRUST=1 CCSPAWN_LAUNCH="env WRAPVAR=hello $BIN/FAKEAGENT" \
  timeout 30 "$CCSPAWN" "$WORK/wrap" "任务W" > "$WORK/out7.txt" 2>&1
waitfor "$WORK/rec-wrap_cc.txt" || true
chk "wrapper 的 env 前缀生效" "$(sed -n 's/^wrapenv=//p' "$WORK/rec-wrap_cc.txt" 2>/dev/null)" "hello"
chk "任务仍原样送达" "$(sed -n 's/^args=//p' "$WORK/rec-wrap_cc.txt" 2>/dev/null)" "任务W"

echo "[11] 【审计重要-7】ccm 版本太旧要报得准（不能说成"建会话失败"）"
cat > "$SANDBOX/oldccm" << 'EOF'
#!/bin/bash
[ "$1" = "--ccm-probe" ] && { printf 'capabilities=new,resume,attach,tmux,account,model,cwd,agent,launcher,ccm-sid,print\n'; exit 0; }
echo "ccm: 未知选项: --detach" >&2; exit 2
EOF
chmod +x "$SANDBOX/oldccm"
mkdir -p "$WORK/old"
CCM_BIN="$SANDBOX/oldccm" timeout 30 "$CCSPAWN" "$WORK/old" > "$WORK/out8.txt" 2>&1
chk "报的是版本太旧" "$(grep -c '版本太旧' "$WORK/out8.txt")" "1"

echo "[12] 【C15 08-13】命名避让**搬进 ccm 之后**仍然成立（同目录连开三个 → 名字退让）"
# ★ 为什么这条要真跑：`P4b①`② 把避让从 cc-spawn 搬进 `ccm --tmux-base`，
#   而**判据只能钉「实现在哪」，钉不了「它真的避让得对」** —— 名字退让是运行期事实
#  （`tmux has-session` 的返回值决定的），只有真起会话才验得出来。
#   ⚠ 同时也钉住那条前置：cc-spawn 现在**从 ccm 的 `ccm-session=` 读回名字**。
#   它要是读回了错的名字，下面「台账/总线里的名字」与「真实会话名」就对不上 ——
#   而那正是搬家最可能出的岔子（cc-spawn 报一个名、ccm 建了另一个）。
mkdir -p "$WORK/dup"
for _i in 1 2 3; do
  CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" "$WORK/dup" "任务$_i" > "$WORK/out-dup$_i.txt" 2>&1
done
chk "三个会话都在" \
  "$(tmux ls -F '#{session_name}' 2>/dev/null | grep -cx 'dup_cc\|dup_cc-2\|dup_cc-3')" "3"
# 台账/总线里的名字必须**与真实会话名逐字一致** —— 这是「读回来的名字对不对」的真判据。
chk "台账三行、名字对得上" \
  "$(cut -f1 "$CC_BUS_HOME/spawned.tsv" | grep -cx 'dup_cc\|dup_cc-2\|dup_cc-3')" "3"
chk "总线三条、名字对得上" \
  "$(cut -f1 "$CC_BUS_HOME/agents.tsv" | grep -cx 'dup_cc\|dup_cc-2\|dup_cc-3')" "3"
# 提示行里报的也得是**退让后**的名字（用户照着敲 `cc-send` 的就是它）。
chk "第三次的提示报的是 dup_cc-3" \
  "$(sed -n 's/^已 spawn: \([^ ]*\).*/\1/p' "$WORK/out-dup3.txt")" "dup_cc-3"

echo "[13] 【C15 08-13】总线登记 + 台账**由 ccm 做**（cc-spawn 一件专属逻辑不剩）"
# 上面 [1]-[12] 已经在验「登记与台账的**结果**对不对」——本格验的是**谁做的**。
# 为什么这条值得单列：把三件搬进 ccm 之后，最容易出的岔子是「两边各做一遍」
#（cc-spawn 没删干净 + ccm 又做了一次）⇒ 台账**两行**、地址簿被后写的那次覆盖。
# 结果上看不出来（名字一样），只有**数行数**才看得见。
mkdir -p "$WORK/once"
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" "$WORK/once" "任务O" > "$WORK/out-once.txt" 2>&1
waitfor "$WORK/rec-once_cc.txt" || true
chk "台账**恰好一行**（没有两边各写一遍）" \
  "$(grep -c '^once_cc	' "$CC_BUS_HOME/spawned.tsv" 2>/dev/null || true)" "1"
chk "地址簿恰好一条" "$(cut -f1 "$CC_BUS_HOME/agents.tsv" | grep -cx 'once_cc' || true)" "1"
chk "台账第 4 列是初始任务" \
  "$(awk -F'\t' '$1=="once_cc"{print $4}' "$CC_BUS_HOME/spawned.tsv")" "任务O"
# 没装 cc-bus 的人不该被总线脚本挡住起会话 —— ccm 静默 no-op，但**要吭一声**。
CC_BUS_SCRIPTS=/nonexistent CCM_NO_PRETRUST=1 timeout 30 "$REPO/shared/ccm" \
  --tmux-base=nobus --detach --bus-register --cwd "$WORK/once" \
  --launcher "$BIN/FAKEAGENT" > "$WORK/out-nobus.txt" 2>&1 || true
chk "找不到 cc-bus 时会话照样建出来" \
  "$(tmux has-session -t '=nobus' 2>/dev/null && echo YES || echo NO)" "YES"
chk "且没有一声不吭" "$(grep -c '没有登记' "$WORK/out-nobus.txt")" "1"
chk "没登记就真的没写进地址簿" \
  "$(cut -f1 "$CC_BUS_HOME/agents.tsv" | grep -cx 'nobus' || true)" "0"

echo "[14] 【08-13】**多行初始任务**：登记与台账都不许丢"
# ★ 这一格是真事故逼出来的：`cc-spawned-record` 原来**拒收**含换行的字段，
#   而 `ccm --bus-register` 整段是 best-effort（`|| true` 把退出码吞掉）
#   ⇒ 实测 `cc-spawn <目录> "第一行\n第二行"` = **登记上了总线、台账一行没有、一声不吭**。
#   本仓一路在治的那族：**悄悄丢数据**。
#   修法是让**格式的主人自己转义**（`\n`/`\t` 写进 TSV），而不是让调用方猜规则。
mkdir -p "$WORK/multi"
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" "$WORK/multi" "$(printf '第一行\n第二行')" \
  > "$WORK/out-multi.txt" 2>&1
waitfor "$WORK/rec-multi_cc.txt" || true
chk "多行任务：仍上总线" "$(cut -f1 "$CC_BUS_HOME/agents.tsv" | grep -cx 'multi_cc' || true)" "1"
chk "多行任务：台账恰好一行" "$(grep -c '^multi_cc	' "$CC_BUS_HOME/spawned.tsv" 2>/dev/null || true)" "1"
# ★ 撕没撕坏 TSV 的判据是**列数**，不是「有没有那行」——撕坏时会变成两行、每行列数不对。
chk "多行任务：那行仍是 4 列" \
  "$(awk -F'\t' '$1=="multi_cc"{print NF}' "$CC_BUS_HOME/spawned.tsv")" "4"
chk "多行任务：换行被转义、信息没丢" \
  "$(awk -F'\t' '$1=="multi_cc"{print $4}' "$CC_BUS_HOME/spawned.tsv")" '第一行\n第二行'

echo "[15] 【08-13】**并发 spawn 同一目录**：四个都得成，名字各不相同"
# ★ 这一格来自一次**被实测打脸的断言**：`C15` 的 commit 里我写过「让建的人自己避让
#   把窗口期关掉了」——**错的**。避让是 `has-session` 探完再 `new-session`，中间仍有窗口，
#   只是从「cc-spawn 探完到 ccm 建」搬到了「ccm 内部探完到建」。
#   实测 4 个并发：**只成 1 个**，另外 3 个报「经 ccm 建会话失败」。
#   ⇒ 处置是**调用方按 exit 3 重试**（`--tmux-base` 的语义就是「给我个新的」）。
# ⚠ 判据钉的是**三处账目一致**，不是「有 4 个会话」——名字对不上时会话数照样是 4。
mkdir -p "$WORK/race"
for _i in 1 2 3 4; do
  CCM_NO_PRETRUST=1 timeout 40 "$CCSPAWN" "$WORK/race" "并发$_i" > "$WORK/race-$_i.out" 2>&1 &
done
wait
chk "四个都报成功" \
  "$(cat "$WORK"/race-*.out | grep -c '^已 spawn: ')" "4"
chk "会话四个、名字互不相同" \
  "$(tmux ls -F '#{session_name}' | grep -c '^race_cc') " "$(tmux ls -F '#{session_name}' | grep '^race_cc' | sort -u | wc -l) "
chk "台账四行" "$(grep -c '^race_cc' "$CC_BUS_HOME/spawned.tsv" 2>/dev/null || true)" "4"
chk "总线四条、与会话名逐字对得上" \
  "$(cut -f1 "$CC_BUS_HOME/agents.tsv" | grep '^race_cc' | sort | md5sum | cut -c1-8)" \
  "$(tmux ls -F '#{session_name}' | grep '^race_cc' | sort | md5sum | cut -c1-8)"

echo "[16] 【08-13】目录名含**空格与中文**：quote 要穿过全链"
# ★ 为什么值得锁：`C15` 在这条路上加了好几层 quote —— ccm 的 `sq`（拼进 seq 的 tmux 命令串）·
#   `--bus-note` · `cc-register` 的 pane 目标 · 台账那行 TSV。任何一层漏 quote，
#   症状都是「会话建在了错的目录」或「台账被撕成两列」，而**两者都不会报错**。
# ⚠ 断言的是**会话真实 cwd**（`#{pane_current_path}`），不是「命令里带了那个路径」——
#   后者只证明字符串拼对了，证明不了 tmux 真的进了那个目录。
SPDIR="$WORK/带 空格 的目录"
mkdir -p "$SPDIR"
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" "$SPDIR" "任务 带空格" > "$WORK/out-sp.txt" 2>&1
SPNAME="$(sed -n 's/^已 spawn: \([^ ]*\).*/\1/p' "$WORK/out-sp.txt")"
chk "含空格目录：spawn 成功并报出名字" "$([ -n "$SPNAME" ] && echo yes || echo no)" "yes"
chk "含空格目录：会话真实 cwd 逐字相符" \
  "$(tmux display-message -p -t "=$SPNAME:" '#{pane_current_path}' 2>/dev/null)" "$SPDIR"
chk "含空格目录：台账仍是 4 列" \
  "$(awk -F'\t' -v n="$SPNAME" '$1==n{print NF}' "$CC_BUS_HOME/spawned.tsv")" "4"
chk "含空格目录：台账第 2 列是那个目录" \
  "$(awk -F'\t' -v n="$SPNAME" '$1==n{print $2}' "$CC_BUS_HOME/spawned.tsv")" "$SPDIR"

echo "[17] 【08-13】台账**写不进去**时要说话（磁盘满/只读的现实形态）"
# ★ 整段登记是 best-effort（`|| true`）——那是对的（台账写不了不该挡住起会话），
#   但**原来连 stderr 一起吞了**（`>/dev/null 2>&1`）⇒ 磁盘满 / 目录只读 / 文件只读
#   全都变成**悄悄没有台账**。本仓一路在治的那族。
#   ⇒ 改成**只吞 stdout**：这两个脚本成功时只往 stdout 说话（cc-register 打「已登记」、
#   cc-spawned-record 一个字不打）⇒ 不丢诊断、也不吵。
mkdir -p "$WORK/roproj"
# 先播一行、再把台账文件设成只读 —— 这是「磁盘满」在测试里的可控替身。
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" "$WORK/roproj" "第一个" > /dev/null 2>&1
chmod a-w "$CC_BUS_HOME/spawned.tsv"
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" "$WORK/roproj" "第二个" \
  > "$WORK/out-ro.txt" 2> "$WORK/err-ro.txt"
chmod u+w "$CC_BUS_HOME/spawned.tsv"
chk "台账写不进去时 spawn 仍成功" "$(grep -c '^已 spawn: ' "$WORK/out-ro.txt")" "1"
chk "且**说出了原因**（不是悄悄没有）" \
  "$(grep -c '权限不够\|Permission denied' "$WORK/err-ro.txt")" "1"
# 反向：正常路径不许多出噪声（放开 stderr 之后最容易出的回归）。
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" "$WORK/roproj" "第三个" \
  > /dev/null 2> "$WORK/err-ok.txt"
chk "正常路径 stderr 仍为空" "$(wc -l < "$WORK/err-ok.txt")" "0"

echo "[18] 【08-13】边界：cc-bus 脚本**不可执行** · 初始任务**超长**"
# ★ 这两格钉的都是**失败面**——`C15` 那批新代码的失败路径基本没被真跑过，
#   而失败面正是「假成功」最爱藏的地方（本轮已在这条路上逮到三次）。
TB="$(mktemp -d)"
cp "$REPO/shared/cc-bus/scripts/cc-register" "$REPO/shared/cc-bus/scripts/cc-spawned-record" "$TB/"
chmod -x "$TB/cc-spawned-record"
chk "台账脚本不可执行 ⇒ 明说「不进 spawn 台账」" \
  "$(CC_BUS_SCRIPTS="$TB" bash "$REPO/shared/ccm" new --tmux-base=q --detach --bus-register \
      --print --cwd /tmp 2>&1 >/dev/null | grep -c '不进 spawn 台账')" "1"
chmod -x "$TB/cc-register"
chk "连定位用的 cc-register 也不可执行 ⇒ 明说「没有登记」" \
  "$(CC_BUS_SCRIPTS="$TB" bash "$REPO/shared/ccm" new --tmux-base=q --detach --bus-register \
      --print --cwd /tmp 2>&1 >/dev/null | grep -c '没有登记')" "1"
rm -rf "$TB"
# 超长任务：**干净失败**（rc≠0、零会话、零台账），不是假成功。
# ⚠ 上界是内核的 `MAX_ARG_STRLEN` = 128 KiB（131072）——实测 131000 仍 OK、131072 报 E2BIG。
#   这不是我们能修的东西（单个 argv 的硬上限），能保证的是**失败得干净**。
BIGTASK="$(head -c 150000 /dev/zero | tr '\0' 'x')"
mkdir -p "$WORK/big"
set +e
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" "$WORK/big" "$BIGTASK" > "$WORK/out-big.txt" 2>&1
big_rc=$?
set -e
chk "超长任务：退出码非 0" "$([ "$big_rc" -ne 0 ] && echo yes || echo no)" "yes"
chk "超长任务：没有留下会话" "$(tmux ls -F '#{session_name}' 2>/dev/null | grep -c '^big_cc' || true)" "0"
chk "超长任务：没有写台账" "$(grep -c '^big_cc' "$CC_BUS_HOME/spawned.tsv" 2>/dev/null || true)" "0"

echo "[19] 【08-13】cc-register：**读不到旧地址簿就别覆盖**"
# ★ 真事故：`awk … || true` 失败之后**照样**追加并 mv ⇒ 地址簿被抹成只剩新登记的这一条。
#   实测三个不同 pane 登记好之后把 agents.tsv 设成 000、再登记第四个
#   ⇒ **3 行变 1 行，而 cc-register 照报「已登记」** —— 另外三个 agent 静默掉线。
#   与 ccm 里 codex 预信任那条**完全同族**：失败被吞掉，而后面有人依赖它成功。
# ⚠ 判据钉的是**旧条目还在**（数行 + 逐个名字），不是「命令失败了」——
#   失败与否是手段，**别人的地址没被抹掉**才是要保的东西。
mkdir -p "$WORK/reg"
for _n in ra rb rc; do
  tmux new-session -d -s "$_n" -c /tmp 'sleep 300'
  _p="$(tmux list-panes -t "=$_n" -F '#{pane_id}' | head -1)"
  TMUX_PANE="$_p" bash "$REPO/shared/cc-bus/scripts/cc-register" "${_n}_cc" >/dev/null 2>&1
done
chk "对照：三个不同 pane 各占一行" \
  "$(cut -f1 "$CC_BUS_HOME/agents.tsv" | grep -c '^r[abc]_cc$')" "3"
tmux new-session -d -s rd -c /tmp 'sleep 300'
_pd="$(tmux list-panes -t '=rd' -F '#{pane_id}' | head -1)"
chmod 000 "$CC_BUS_HOME/agents.tsv"
TMUX_PANE="$_pd" bash "$REPO/shared/cc-bus/scripts/cc-register" rd_cc > /dev/null 2>&1 || true
chmod 644 "$CC_BUS_HOME/agents.tsv"
chk "★ 旧表读不动时**拒绝登记**，别人的地址一条不少" \
  "$(cut -f1 "$CC_BUS_HOME/agents.tsv" | grep -c '^r[abc]_cc$')" "3"

echo "[20] 【08-13】敲门不许打进**别人的**屏幕"
# ★ 真事故：地址是**名字型**的（`proj_cc:0.0`），而名字会被重用 —— `cc-spawn` 就按目录
#   基名取会话名。agent 退出后同名会话被别的进程占着，再给它发消息 ⇒ 敲门文字（**带 Enter**）
#   被打进陌生占用者的屏幕（在 shell 里那行会被当命令执行），而真正的收件人一次也没被敲。
# ⇒ 登记时记下 pane 的根进程 pid（第 4 列），敲门前核一次。
# ⚠ 判据要**两个方向都钉**：活着的必须照敲、陌生人必须零打扰。只钉后者的话，
#   把敲门整个删掉也能"通过"。
tmux new-session -d -s live_cc -c /tmp 'cat'; sleep 0.4
_lp="$(tmux list-panes -t '=live_cc' -F '#{pane_id}' | head -1)"
TMUX_PANE="$_lp" bash "$REPO/shared/cc-bus/scripts/cc-register" live_cc >/dev/null 2>&1
chk "登记行带第 4 列（pane 根进程 pid，纯数字）" \
  "$(awk -F'\t' '$1=="live_cc"{print ($4 ~ /^[0-9]+$/) ? "yes" : "no"}' "$CC_BUS_HOME/agents.tsv")" "yes"
bash "$REPO/shared/cc-bus/scripts/cc-send" live_cc "给活着的它" >/dev/null 2>&1
sleep 1.2
chk "★ 方向一：活着的 agent **照样被敲**" \
  "$( [ "$(tmux capture-pane -t 'live_cc:0.0' -p | grep -c '🔔 cc-bus')" -gt 0 ] && echo yes || echo no)" "yes"

tmux new-session -d -s reuse_cc -c /tmp 'cat'; sleep 0.4
_rp="$(tmux list-panes -t '=reuse_cc' -F '#{pane_id}' | head -1)"
TMUX_PANE="$_rp" bash "$REPO/shared/cc-bus/scripts/cc-register" alpha_cc >/dev/null 2>&1
tmux kill-session -t '=reuse_cc'; sleep 0.3
tmux new-session -d -s reuse_cc -c /tmp 'cat'; sleep 0.5     # 陌生占用者，同名会话
bash "$REPO/shared/cc-bus/scripts/cc-send" alpha_cc "只该给 alpha_cc 看的" >/dev/null 2>&1
sleep 1.2
chk "★ 方向二：同名会话被陌生人占着 ⇒ **零打扰**" \
  "$(tmux capture-pane -t 'reuse_cc:0.0' -p | grep -c '🔔 cc-bus')" "0"
# ⚠ 两个坑一次踩齐（本套件当场演示了）：`grep -c` 没命中时**既打印 0 又返回 1**。
#   ① 写 `|| echo 0` ⇒ 再追加一个 0，读数变成 "0\n0"；
#   ② 什么都不写 ⇒ 这里 `set -e` 是**开着**的（见上方 `:338`），赋值失败当场杀掉整套，
#      表现为「最后一格和合计行凭空消失」——不是红，是**没跑完**。
#   ⇒ 用本文件既有的 `|| true`（它不打印任何东西），再用 `${_n:-0}` 兜住文件不存在。
_n="$(grep -c 'NUDGE stale alpha_cc' "$CC_BUS_HOME/log/bus.log" 2>/dev/null || true)"
chk "  且记了一笔（不是静默跳过）" "${_n:-0}" "1"
chk "  投递不受影响（收件箱照收）" \
  "$(wc -l < "$CC_BUS_HOME/inbox/alpha_cc.jsonl" 2>/dev/null || echo 0)" "1"

# 老表兼容：用户盘上那 86 行是 **3 列**（最早 07-18）。第 4 列为空 ⇒ 按老行为敲，
# **不许**把它们全判成 stale（那等于把所有存量 agent 的敲门一次性关掉）。
tmux new-session -d -s old_cc -c /tmp 'cat'; sleep 0.4
printf 'old_cc\told_cc:0.0\t2026-07-18T07:26:31-07:00\n' > "$CC_BUS_HOME/agents.tsv"
bash "$REPO/shared/cc-bus/scripts/cc-send" old_cc "老表照样要敲" >/dev/null 2>&1
sleep 1.2
chk "★ 老 3 列表（无第 4 列）**仍被敲到**" \
  "$( [ "$(tmux capture-pane -t 'old_cc:0.0' -p | grep -c '🔔 cc-bus')" -gt 0 ] && echo yes || echo no)" "yes"
_n="$(grep -c 'NUDGE stale old_cc' "$CC_BUS_HOME/log/bus.log" 2>/dev/null || true)"
chk "  且没有假 stale" "${_n:-0}" "0"

echo "[21] 【08-13】收掉 agent 时**不许杀掉占了同一个名字的无辜进程**"
# ★★ 真事故（比敲门那条重得多：那条是打扰，这条是**销毁**）：`cc-kill` 按**名字**杀
#   （`-t "=$id"` + kill -9 整棵进程树 + 删收件箱），而会话名会被重用。
#   实测：agent 退出后用户在同名会话里跑别的东西，UI 上点「收掉 agent」⇒ 那个无辜进程被杀。
# ⇒ 杀之前用 agents.tsv 第 4 列（登记时的 pane 根进程 pid）核一次。
# ⚠ 两个方向都要钉：真的那个必须照杀，无辜的必须一根汗毛不动。
tmux new-session -d -s realk_cc -c /tmp 'sleep 300'; sleep 0.3
TMUX_PANE="$(tmux list-panes -t '=realk_cc' -F '#{pane_id}' | head -1)" \
  bash "$REPO/shared/cc-bus/scripts/cc-register" realk_cc >/dev/null 2>&1
_rp="$(tmux list-panes -t '=realk_cc' -F '#{pane_pid}' | head -1)"
bash "$REPO/shared/cc-bus/scripts/cc-kill" realk_cc >/dev/null 2>&1
sleep 0.4
chk "★ 方向一：真的那个 agent 照样杀得掉（会话）" \
  "$(tmux has-session -t '=realk_cc' 2>/dev/null && echo 在 || echo 没了)" "没了"
chk "  连进程树一起（cc-kill 的正题）" \
  "$(ps -p "$_rp" >/dev/null 2>&1 && echo 在 || echo 没了)" "没了"

tmux new-session -d -s innoc_cc -c /tmp 'sleep 300'; sleep 0.3
TMUX_PANE="$(tmux list-panes -t '=innoc_cc' -F '#{pane_id}' | head -1)" \
  bash "$REPO/shared/cc-bus/scripts/cc-register" innoc_cc >/dev/null 2>&1
printf '一条没读的消息\n' > "$CC_BUS_HOME/inbox/innoc_cc.jsonl"
tmux kill-session -t '=innoc_cc'; sleep 0.3
tmux new-session -d -s innoc_cc -c /tmp 'sleep 999'; sleep 0.4    # 同名的无辜占用者
_ip="$(tmux list-panes -t '=innoc_cc' -F '#{pane_pid}' | head -1)"
bash "$REPO/shared/cc-bus/scripts/cc-kill" innoc_cc >/dev/null 2>&1
sleep 0.4
chk "★★ 方向二：名字被占时**无辜进程不许被杀**" \
  "$(ps -p "$_ip" >/dev/null 2>&1 && echo 在 || echo 被杀了)" "在"
chk "★★ 无辜会话也不许被杀" \
  "$(tmux has-session -t '=innoc_cc' 2>/dev/null && echo 在 || echo 没了)" "在"
chk "  但陈旧登记要摘掉（那条确实是过期的）" \
  "$(cut -f1 "$CC_BUS_HOME/agents.tsv" | grep -c '^innoc_cc$')" "0"
chk "  ★ 收件箱**不许删**（还不知道那个 agent 是不是真没了）" \
  "$([ -f "$CC_BUS_HOME/inbox/innoc_cc.jsonl" ] && echo 在 || echo 被删了)" "在"

# ⚠ 台架撞名过一次：本套件早有一个 `multi_cc`（多行任务那格），而 `set -e` 下
#   `duplicate session` 会**直接把套件打断**（没有 FAIL、没有合计行，看起来像"跑完了"）。
#   ⇒ 新场景取名前先在本文件里搜一遍。
# ⚠⚠ 上面那道身份核对**第一版有假阳性**（自己的探针撞出来的）：
#   它写 `list-panes -t "=$id" | head -1`，取的是**当前窗口**的 pane ——
#   用户在自己 agent 的会话里开一个新窗口，当前窗口就变了 ⇒ pid 对不上
#   ⇒ cc-kill 对**它自己的 agent** 说「名字被别人占了」。
#   ★ **假阳性比不查更坏**：它会让人以为这道保护坏了，进而把它删掉。
#   ⇒ 改成问 agents.tsv 第 2 列那个**完整地址**（`sess:win.pane`），与敲门那条一致。
tmux new-session -d -s mwin_cc -c /tmp 'sleep 300'; sleep 0.3
TMUX_PANE="$(tmux list-panes -t '=mwin_cc' -F '#{pane_id}' | head -1)" \
  bash "$REPO/shared/cc-bus/scripts/cc-register" mwin_cc >/dev/null 2>&1
tmux new-window -t '=mwin_cc' 'sleep 300'; sleep 0.3
chk "台架自检：这个 agent 现在有 2 个窗口" \
  "$(tmux display-message -p -t '=mwin_cc:' '#{session_windows}')" "2"
_kout="$(bash "$REPO/shared/cc-bus/scripts/cc-kill" mwin_cc 2>&1)"
sleep 0.3
chk "★ 自己的 agent 开了新窗口，仍认得出是它（照杀，不误判成「别人占了」）" \
  "$(tmux has-session -t '=mwin_cc' 2>/dev/null && echo 还在 || echo 杀了)" "杀了"
chk "  且把要一起收掉的窗口数说出来了" "$(printf '%s' "$_kout" | grep -c '有 2 个窗口')" "1"

echo "[22] 【08-13】cc-agents 的「活」也不许只看名字（同一族第五处）"
# 它原来只判 `tmux has-session -t "=$id"` ⇒ 名字被别人占了照样标「活」。
# ⚠ 它读的 spawned.tsv **没有身份列** ⇒ 去 agents.tsv 借第 4 列。借不到就说"核不了"。
# ★ 三态而不是两态：把「核不了」并进「活」正是今天这一族所有事故的共同起点。
tmux new-session -d -s areal_cc -c /tmp 'sleep 300'; sleep 0.3
TMUX_PANE="$(tmux list-panes -t '=areal_cc' -F '#{pane_id}' | head -1)" \
  bash "$REPO/shared/cc-bus/scripts/cc-register" areal_cc >/dev/null 2>&1
tmux new-session -d -s aghost_cc -c /tmp 'sleep 300'; sleep 0.3
TMUX_PANE="$(tmux list-panes -t '=aghost_cc' -F '#{pane_id}' | head -1)" \
  bash "$REPO/shared/cc-bus/scripts/cc-register" aghost_cc >/dev/null 2>&1
tmux kill-session -t '=aghost_cc'; sleep 0.2
tmux new-session -d -s aghost_cc -c /tmp 'sleep 999'; sleep 0.3   # 同名的无辜占用者
tmux new-session -d -s anoreg_cc -c /tmp 'sleep 300'; sleep 0.3   # 有会话但没登记过
printf 'areal_cc\t/tmp\tts\t任务A\naghost_cc\t/tmp\tts\t任务B\nanoreg_cc\t/tmp\tts\t任务C\n' \
  > "$CC_BUS_HOME/spawned.tsv"
_ag="$(bash "$REPO/shared/cc-bus/scripts/cc-agents")"
_st() { printf '%s' "$_ag" | awk -v id="$1" '$1==id{print $2}'; }
chk "★ 身份核过的 ⇒ 活" "$(_st areal_cc)" "活"
chk "★★ 名字被别人占了 ⇒ **已退**（不再假报活）" "$(_st aghost_cc)" "已退"
chk "★ 有会话但没登记、核不了 ⇒ **活?**（不是「活」）" "$(_st anoreg_cc)" "活?"

echo
echo "===== 合计 PASS=$pass FAIL=$fail ====="
if [ "$fail" -eq 0 ]; then echo "===== cc-spawn 收编验收全部通过 ====="; fi
exit "$fail"
