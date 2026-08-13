#!/usr/bin/env bash
# `P0b-Y2` 第十六拍〔08-13〕：**daemon 起得比 tmux server 早，也要看得见后来的会话。**
#
# ## 这是 `#60` 的根因
#
# `P5` 删掉 8s ticker 之后 daemon **零定时器**，之后每一拍都靠事件。而两条唤醒路
# **都以「已经见过 server」为前提**：tmux hook 是**观测到 server 那一刻**才装的；
# socket 目录的 inotify 路径是从 probe 结果里拿的 —— 没 server 就没有路径。
# ⇒ 零 server 起步 = **永远不再探** ⇒ 后来建的会话它一无所知 ⇒ `@ccm_sid` 到不了 monitor
# ⇒ 死亡一律判归档，**灰灯永不出现**（`#60` 现象 1，08-13 全链复现 2/2）。
#
# ## 为什么必须真跑
#
# 病在 **inotify 有没有那只耳朵**这个运行期事实上。单测钉得了「代码里有 watch 那一行」，
# 钉不了「server 后来出现时它真的醒了」——本轮反复撞的同一句话。
#
# ## 本机安全
#
# 全程 `-L <私有名>`（`C7i`），绝不碰用户的 default socket；假会话是 `sleep`，不起真 claude（`C7`）。
set -o pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
D="${CCM_E2E_DAEMON_BIN:-$REPO/remote-daemon-proto/target/debug/cc-monitor-remote}"
[ -x "$D" ] || { echo "需要 daemon 二进制：$D（先 cargo build -p cc-monitor-remote）"; exit 1; }
command -v tmux >/dev/null 2>&1 || { echo "需要 tmux"; exit 1; }
REALTMUX="$(command -v tmux)"

W="$(mktemp -d /tmp/e2e-lateserver.XXXXXX)"
SOCK="lateSrv$$"
cleanup() {
  set +e
  "$REALTMUX" -L "$SOCK" kill-server 2>/dev/null
  rm -rf -- "$W"
}
trap cleanup EXIT

fail=0
pass=0
chk() { if [ "$2" = "$3" ]; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1: 期望[$3] 实得[$2]"; fail=$((fail+1)); fi; }

# $1=label $2=late|early
#
# ⚠ **socket 目录的推算规则（`TMUX_TMPDIR` 优先、不硬编码 /tmp）不在这里钉** ——
#   那是个**纯函数**，单测才是对的 acceptor（见 daemon 侧 `tmux_socket_dir_follows_tmux_tmpdir`）。
#   而且在这里钉会逼本套件自己设 `TMUX_TMPDIR`，那正是 `C7i` 零例外禁止的形状
#   （`e2e_gate_registry` 那条守卫当场拦下了第一版 —— **它报得对**）。
run_case() {
  local label="$1" mode="$2"
  local F="$W/$label" out="$W/$label.frames" bin="$W/$label.bin"
  local sock="$SOCK$label"
  rm -rf -- "$F" "$bin"; mkdir -p "$F/sessions" "$F/projects/p" "$bin"
  # shim：daemon 内部裸调 `tmux`，塞不进 -L ⇒ 只能用 PATH 拦（同 tmux-shim.sh 的理由）。
  printf '#!/bin/sh\nexec %s -L %s "$@"\n' "$REALTMUX" "$sock" > "$bin/tmux"
  chmod +x "$bin/tmux"

  if [ "$mode" = early ]; then
    "$REALTMUX" -L "$sock" new-session -d -s "$label" -c /tmp 'sleep 300'
    "$REALTMUX" -L "$sock" set-option -t "=$label:" @ccm_sid "sid-$label"
    sleep 1
  fi

  # daemon 起（late 模式下此刻该 socket 上零 server）
  PATH="$bin:$PATH" CLAUDE_CONFIG_DIR="$F" timeout 30 "$D" --with-bg --tail-only \
    < /dev/null > "$out" 2> "$W/$label.err" &
  local dp=$!
  sleep 3

  if [ "$mode" = late ]; then
    "$REALTMUX" -L "$sock" new-session -d -s "$label" -c /tmp 'sleep 300'
    "$REALTMUX" -L "$sock" set-option -t "=$label:" @ccm_sid "sid-$label"
  fi
  sleep 8

  kill "$dp" 2>/dev/null; wait "$dp" 2>/dev/null
  "$REALTMUX" -L "$sock" kill-server 2>/dev/null
  # 判据：**帧里带着会话名与它的 @ccm_sid** —— 那正是 monitor 判「灰还是归档」要的东西。
  grep -c "sid-$label" "$out" 2>/dev/null || true
}

echo "[1] 量具自检：server **先于** daemon 存在时，帧里带 @ccm_sid"
# 它绿不了，下面两格的读数一个字都不能信（本仓一路在收的空真）。
chk "early：帧里有 sid-early" "$(run_case early early)" "1"

echo "[2] ★ 正题：daemon **先起**、tmux server 后起（#60 的根因形状）"
chk "late：帧里有 sid-late" "$(run_case late late)" "1"


# ⚠⚠ **「socket 目录被删掉再重建」那一格不在这里** —— 它需要一个**私有 socket 目录**，
#   而私有 socket 目录只能靠 `TMUX_TMPDIR`，那正是 `C7i` **零例外**禁止的东西
#   （`e2e_gate_registry::no_e2e_suite_isolates_with_tmux_tmpdir` 当场拦下了第一版）。
#   ⇒ 与 08-13 早些时候那次同样处置：**换判据落在哪一层**，不给红线开例外。
#   那条性质改由 daemon 侧的结构判据钉（`the_socket_dir_watch_survives_an_inode_swap`），
#   并**如实记下损失**：运行期行为只在 08-13 手工验过一次（读数记在 `P0b §1w`），
#   **没有进 CI**。红线与覆盖面冲突时，本仓选红线。
echo
echo "===== 合计 PASS=$pass FAIL=$fail ====="
if [ "$fail" -eq 0 ]; then echo "===== 迟到 server 验收全部通过 ====="; fi
exit "$fail"
