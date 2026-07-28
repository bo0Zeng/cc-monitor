#!/usr/bin/env bash
# B02 验收：`cc-spawn` 收编进 `ccm` 之后，行为仍与收编前等价。
#
# **本机安全（这条最重要，血的教训）**：开发机上住着**正在运行**的 tmux 会话与真实
# `~/.cc-bus/`（本会话自己就跑在其中一个 tmux 会话里）。本套件因此：
#   ① 一律经 PATH 上的 `tmux` shim 强制 `-L $SOCK`——`cc-spawn`/`ccm`/`cc-register`
#      内部都是裸调 `tmux`，塞不进 `-L`，只能用 shim 拦；
#   ② **起飞前自检**：断言 shim 看不到默认 socket 上的任何会话，不过就 exit 9 直接中止。
#      光"设了隔离"不算数——必须证明它生效了才允许往下跑；
#   ③ `CC_BUS_HOME` 指向临时目录，绝不碰真实 `~/.cc-bus/`；
#   ④ 清理只用 `tmux -L $SOCK kill-server`（**永远带 socket 名**）。
#      裸 `tmux kill-server` 在本套件里是禁用词——它会连开发机上正在跑的会话一起杀掉。
#   ⑤ 启动器一律假的（纯 sleep 脚本），绝不起真的已认证 claude/codex。
set -o pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
CCSPAWN="$REPO/shared/cc-bus/scripts/cc-spawn"
SOCK="ccmB02e2e$$"
REALTMUX="$(command -v tmux)" || { echo "SKIP: 未装 tmux"; exit 0; }

BIN="$(mktemp -d)"; HOMEBUS="$(mktemp -d)"; WORK="$(mktemp -d)"
cat > "$BIN/tmux" << EOF
#!/bin/bash
exec "$REALTMUX" -L $SOCK "\$@"
EOF
chmod +x "$BIN/tmux"
export PATH="$BIN:$PATH"
export CC_BUS_HOME="$HOMEBUS"

cleanup() { "$REALTMUX" -L "$SOCK" kill-server 2>/dev/null; rm -rf "$BIN" "$HOMEBUS" "$WORK"; }
trap cleanup EXIT

# ===== 起飞前自检（红线守卫）=====
for n in $("$REALTMUX" ls -F '#{session_name}' 2>/dev/null); do
  if tmux ls -F '#{session_name}' 2>/dev/null | grep -qx "$n"; then
    echo "FATAL 隔离失效：shim 能看到默认 socket 上的会话 '$n' —— 中止，绝不在真 socket 上跑测试"
    exit 9
  fi
done
case "$CC_BUS_HOME" in "$HOME"/.cc-bus*) echo "FATAL CC_BUS_HOME 指向真实总线"; exit 9 ;; esac
echo "[自检] tmux 隔离生效（-L $SOCK）；CC_BUS_HOME=$CC_BUS_HOME"

fail=0
chk() { if [ "$2" = "$3" ]; then echo "  PASS $1"; else echo "  FAIL $1: 期望[$3] 实得[$2]"; fail=$((fail+1)); fi; }

# 假启动器：记录自己收到的位置参数，然后常驻（模拟 agent 起来了）
mkdir -p "$WORK/proj"
cat > "$BIN/FAKEAGENT" << EOF
#!/bin/bash
printf '%s' "\$1" > "$WORK/task-seen.txt"
printf '%s' "\${CC_BUS_ID:-<unset>}" > "$WORK/busid.txt"
sleep 300
EOF
chmod +x "$BIN/FAKEAGENT"
export CCSPAWN_LAUNCH="$BIN/FAKEAGENT"

echo "[1] cc-spawn 经 ccm 建会话并返回（不挂在 attach 上）"
s=$(date +%s)
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" "$WORK/proj" "分析这个项目的架构" > "$WORK/out1.txt" 2>&1
rc=$?; e=$(date +%s)
chk "退出码 0" "$rc" "0"
if [ $((e-s)) -lt 15 ]; then
  echo "  PASS 未挂起（$((e-s))s）"
else
  echo "  FAIL 耗时 $((e-s))s，疑似挂在 attach"; fail=$((fail+1))
fi
chk "会话名 proj_cc 存在" "$(tmux has-session -t '=proj_cc' 2>/dev/null && echo YES || echo NO)" "YES"

echo "[2] --tmux-size 生效（原 cc-spawn 的 -x 220 -y 50 没被丢掉）"
chk "窗口 220x50" "$(tmux list-windows -t '=proj_cc' -F '#{window_width}x#{window_height}' 2>/dev/null)" "220x50"

echo "[3] 初始任务作为位置参数送达启动器"
for _ in $(seq 40); do [ -s "$WORK/task-seen.txt" ] && break; sleep 0.5; done
chk "启动器收到任务原文" "$(cat "$WORK/task-seen.txt" 2>/dev/null)" "分析这个项目的架构"

echo "[4] 台账与总线登记仍是 cc-spawn 自己的活（未随收编丢掉）"
chk "spawned.tsv 有记录" "$(grep -c '^proj_cc	' "$CC_BUS_HOME/spawned.tsv" 2>/dev/null || echo 0)" "1"
chk "agents.tsv 已登记" "$(cut -f1 "$CC_BUS_HOME/agents.tsv" 2>/dev/null | grep -cx 'proj_cc' || echo 0)" "1"

echo "[5] 默认复用：同目录再 spawn 不新建会话"
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" "$WORK/proj" "第二个任务" > "$WORK/out2.txt" 2>&1
chk "输出说复用" "$(grep -c '复用已有会话' "$WORK/out2.txt")" "1"
chk "会话数仍为 1" "$(tmux ls -F '#{session_name}' 2>/dev/null | grep -cx 'proj_cc')" "1"
chk "未新建 proj_cc-2" "$(tmux has-session -t '=proj_cc-2' 2>/dev/null && echo YES || echo NO)" "NO"

echo "[6] --new 强制新建，命名避让到 -2"
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" --new "$WORK/proj" > "$WORK/out3.txt" 2>&1
chk "proj_cc-2 建起来了" "$(tmux has-session -t '=proj_cc-2' 2>/dev/null && echo YES || echo NO)" "YES"

echo "[7] codex 下 CC_BUS_ID 由 ccm 自动派生 = 会话名（不需要 --bus-id）"
mkdir -p "$WORK/cx"
CCM_NO_PRETRUST=1 timeout 30 "$CCSPAWN" --tool codex "$WORK/cx" > "$WORK/out4.txt" 2>&1
for _ in $(seq 40); do [ -s "$WORK/busid.txt" ] && break; sleep 0.5; done
chk "会话内 CC_BUS_ID" "$(cat "$WORK/busid.txt" 2>/dev/null)" "cx_cc"

echo
if [ "$fail" -eq 0 ]; then echo "===== cc-spawn 收编验收全部通过 ====="; else echo "===== FAIL=$fail ====="; fi
exit "$fail"
