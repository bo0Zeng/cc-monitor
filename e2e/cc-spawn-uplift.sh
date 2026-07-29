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

cleanup() { "$REALTMUX" -L "$SOCK" kill-server 2>/dev/null; "$REALTMUX" -L "${SOCK}b" kill-server 2>/dev/null; rm -rf "$BIN" "$SANDBOX" "$WORK"; }
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
chk() { if [ "$2" = "$3" ]; then echo "  PASS $1"; else echo "  FAIL $1: 期望[$3] 实得[$2]"; fail=$((fail+1)); fi; }
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
if [ $((e-s)) -lt 15 ]; then echo "  PASS 未挂起（$((e-s))s）"; else
  echo "  FAIL 耗时 $((e-s))s，疑似挂在 attach"; fail=$((fail+1)); fi
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

echo "[5] 默认复用：同目录再 spawn 不新建会话"
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" "$WORK/proj" "第二个任务" > "$WORK/out2.txt" 2>&1
chk "输出说复用" "$(grep -c '复用已有会话' "$WORK/out2.txt")" "1"
chk "未新建 proj_cc-2" "$(tmux has-session -t '=proj_cc-2' 2>/dev/null && echo YES || echo NO)" "NO"

echo "[6] --new 强制新建，命名避让到 -2"
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" --new "$WORK/proj" > "$WORK/out3.txt" 2>&1
chk "proj_cc-2 建起来了" "$(tmux has-session -t '=proj_cc-2' 2>/dev/null && echo YES || echo NO)" "YES"

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

echo
if [ "$fail" -eq 0 ]; then echo "===== cc-spawn 收编验收全部通过 ====="; else echo "===== FAIL=$fail ====="; fi
exit "$fail"
