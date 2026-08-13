#!/usr/bin/env bash
# `P0b`：收割**孤儿** daemon —— 取代 `graylight-suite.sh` 原本教人的那句裸 `pkill -f`。
#
# ## 为什么不能裸 pkill
#
# 原建议逐字是「先 `pkill -f remote-daemon-proto/target`」。两条问题：
#
# ① **模式杀会打到不该打的东西**。本仓吃过一次：`P5L` 那拍我跑
#    `pkill -f xdg-terminal-exec`，**把我自己的 shell 打死了**（模式命中了我自己的命令行）。
#    模式杀没有"只杀我起的那些"这个概念。
# ② **它挑错了人群**。08-12 实测：盘上活着的 daemon 走的是**部署路径**
#    `~/.cc-monitor/bin/cc-monitor-remote`（`sftp::ensure_daemon_deployed` 的落点），
#    而那句 `pkill` 与 gate 丁 的计数器都只认 `remote-daemon-proto/target/...`。
#    ⇒ 真正在攒的那一族**可能根本不在人群里**，而计数器会安心地报 0。
#    （`needle_anchor_registry` 管的就是这个：**匹配单位不许比事实小**。）
#
# ## 「孤儿」的可判形状
#
# daemon 是 app **经 SSH exec** 起的，祖链是 `sshd(listener) → sshd-session[priv] →
# sshd-session zbl@notty → daemon`。08-12 实测一个**活着的**：父进程是 `sshd-session`。
# ⇒ 连接还在的那些**有一个活着的 sshd 父进程**；连接断了之后 daemon 会被 init 收养
# ⇒ **`PPID == 1` 就是孤儿**。本刀只收那些。
#
# ⚠ **射程如实登记**：`PPID == 1` 认不出「sshd 还在、但 app 早就没了」那种半残留 ——
# 那种今天靠 gate 丁 的**计数**拦（多于一个就 ABORT），不靠本刀。
# 别把本刀读成「跑完就一定干净」。
#
# ## 默认干跑
#
# 不带 `--yes` **只列不杀**。一把会杀进程的刀，默认就该是干跑的。
set -euo pipefail

YES=0
[ "${1:-}" = "--yes" ] && YES=1

# 两族都要数（这正是 gate 丁 原本漏掉的那一半）。
PATTERNS=(
  'remote-daemon-proto/target/[^ ]*/cc-monitor-remote'   # dev/target 那一族
  '\.cc-monitor/bin/cc-monitor-remote'                   # 部署落点那一族
)

found=0
orphans=()
for pat in "${PATTERNS[@]}"; do
  while read -r pid; do
    [ -n "$pid" ] || continue
    found=$((found + 1))
    ppid="$(ps -o ppid= -p "$pid" 2>/dev/null | tr -d ' ')"
    args="$(ps -o args= -p "$pid" 2>/dev/null || true)"
    if [ "${ppid:-0}" = "1" ]; then
      orphans+=("$pid")
      echo "  孤儿  pid=$pid ppid=1  ${args:0:80}"
    else
      pparg="$(ps -o args= -p "${ppid:-0}" 2>/dev/null | head -c 40 || true)"
      echo "  活着  pid=$pid ppid=$ppid ($pparg)  —— **不动它**"
    fi
  done < <(pgrep -f "$pat" 2>/dev/null || true)
done

echo "  共 $found 个 daemon 进程，其中孤儿 ${#orphans[@]} 个"

if [ "${#orphans[@]}" -eq 0 ]; then
  echo "  没有孤儿，什么都不做。"
  exit 0
fi

if [ "$YES" != "1" ]; then
  echo "  （干跑：没杀任何东西。确认无误后加 --yes）"
  exit 0
fi

for pid in "${orphans[@]}"; do
  # 再核一次：从列出到动手之间，那个 pid 可能已经没了、甚至被别人复用。
  ppid_now="$(ps -o ppid= -p "$pid" 2>/dev/null | tr -d ' ' || true)"
  if [ "${ppid_now:-}" != "1" ]; then
    echo "  跳过 pid=$pid —— 现在 ppid=${ppid_now:-<没了>}，不再符合孤儿判据（pid 复用是真事）"
    continue
  fi
  kill "$pid" 2>/dev/null && echo "  已收 pid=$pid" || echo "  收不动 pid=$pid（权限？已退出？）"
done
