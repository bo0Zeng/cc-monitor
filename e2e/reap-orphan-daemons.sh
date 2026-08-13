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
# ⚠⚠ **那条射程 08-13 真的撞上了**：跑全链台架时，盘上有一个跑了 8.9h、
# 父进程是**活着的 sshd** 的 daemon。本刀按 `PPID == 1` 判它「活着，不动」——**判得对**
# （它可能正是用户 app 的那条链），但也因此**收不掉它**，而 `graylight-suite` 的丁那格
# 只数个数 ⇒ 台架起不来。⇒ 那一格该怎么收窄记成了待决 `U10g`。
# ★ 同一拍里本刀**也证明了自己有用**：另一个 `ppid=1` 的孤儿（我一次自伤 `pkill` 留下的）
#   被精准认出、只收了它，用户那两个一个没动。
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
FIXTURE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --yes)     YES=1 ;;
    --fixture) shift; FIXTURE="${1:-}" ;;
    *) echo "用法: $0 [--fixture <claude_dir>] [--yes]" >&2; exit 2 ;;
  esac
  shift
done

# ★★ `--fixture <dir>`〔`U10g` 落地之后 08-13 加〕：**按身份收**，不再只看 `PPID == 1`。
#
# 病：台架起的 daemon 是 app **经 SSH exec** 的，杀掉 app **不会带走它**（SSH 会话还开着，
# 父进程是活的 wrapper）⇒ 它既不是孤儿、又确实是上一跑的残留。
# 本会话被这个形态挡了**三次**：下一跑的 gate 丁 数到它就 ABORT，每次都要人工核 pid。
#
# 而 `U10g` 量出来的身份信号正好能用：daemon 的 `/proc/<pid>/environ` 里有 `CLAUDE_CONFIG_DIR`
#（台架的 wrapper 一律 `exec env CLAUDE_CONFIG_DIR=… daemon`）。
# ⇒ 指定 fixture 目录 = **点名收那些盯着它的**，与父进程活不活无关。
#
# ⚠⚠ **红线：绝不收盯着真实 claude 目录的**。那是用户自己的 cc-monitor 在用的 daemon。
# 下面有一道硬门：`--fixture` 的值必须落在 `/tmp/` 下，否则**拒绝执行**（不是警告，是退出）。
# 理由与 `cc_bus_deploy::fenced_dest` 同族 —— 一把会杀进程的刀，射程要**结构性**地关死。
if [ -n "$FIXTURE" ]; then
  case "$FIXTURE" in
    /tmp/*) : ;;
    *) echo "拒绝：--fixture 只许指 /tmp 下的一次性目录（实得：$FIXTURE）" >&2
       echo "      盯着真实 claude 目录的 daemon 是**用户自己的**，本刀绝不碰。" >&2
       exit 2 ;;
  esac
fi

# 两族都要数（这正是 gate 丁 原本漏掉的那一半）。
PATTERNS=(
  'remote-daemon-proto/target/[^ ]*/cc-monitor-remote'   # dev/target 那一族
  '\.cc-monitor/bin/cc-monitor-remote'                   # 部署落点那一族
)

found=0
orphans=()
fixture_hits=()
for pat in "${PATTERNS[@]}"; do
  while read -r pid; do
    [ -n "$pid" ] || continue
    found=$((found + 1))
    ppid="$(ps -o ppid= -p "$pid" 2>/dev/null | tr -d ' ')"
    args="$(ps -o args= -p "$pid" 2>/dev/null || true)"
    # ⚠ **按 exe 复核**：`pgrep -f` 会匹配到任何命令行里含这个模式的进程 ——
    #   08-13 实测它数进了**我自己那条正在跑的 shell**（与本轮五次 `pkill -f` 自伤同一族）。
    case "$(readlink -f "/proc/$pid/exe" 2>/dev/null)" in
      *cc-monitor-remote) : ;;
      *) found=$((found - 1)); continue ;;
    esac
    env_dir=""
    [ -r "/proc/$pid/environ" ] && \
      env_dir="$(tr '\0' '\n' < "/proc/$pid/environ" | sed -n 's/^CLAUDE_CONFIG_DIR=//p' | head -1)"
    if [ -n "$FIXTURE" ] && [ "$env_dir" = "$FIXTURE" ]; then
      # 身份对上了 —— **与父进程活不活无关**。台架的残留正是这一族。
      fixture_hits+=("$pid")
      echo "  本跑  pid=$pid ppid=$ppid 盯 $env_dir  ${args:0:60}"
    elif [ "${ppid:-0}" = "1" ]; then
      orphans+=("$pid")
      echo "  孤儿  pid=$pid ppid=1  ${args:0:80}"
    else
      pparg="$(ps -o args= -p "${ppid:-0}" 2>/dev/null | head -c 40 || true)"
      echo "  活着  pid=$pid ppid=$ppid 盯 ${env_dir:-<默认 ~/.claude>} ($pparg)  —— **不动它**"
    fi
  done < <(pgrep -f "$pat" 2>/dev/null || true)
done

echo "  共 $found 个 daemon 进程，其中孤儿 ${#orphans[@]} 个${FIXTURE:+，盯 $FIXTURE 的 ${#fixture_hits[@]} 个}"

targets=("${orphans[@]}" ${fixture_hits[@]+"${fixture_hits[@]}"})
if [ "${#targets[@]}" -eq 0 ]; then
  echo "  没有可收的，什么都不做。"
  exit 0
fi

if [ "$YES" != "1" ]; then
  echo "  （干跑：没杀任何东西。确认无误后加 --yes）"
  exit 0
fi

for pid in "${targets[@]}"; do
  # 再核一次：从列出到动手之间，那个 pid 可能已经没了、甚至被别人复用。
  # ⚠ 两族各有各的复核：孤儿看 `ppid==1`，本跑看 `CLAUDE_CONFIG_DIR` 还是不是那个 fixture。
  env_now=""
  [ -r "/proc/$pid/environ" ] && \
    env_now="$(tr '\0' '\n' < "/proc/$pid/environ" | sed -n 's/^CLAUDE_CONFIG_DIR=//p' | head -1)"
  ppid_now="$(ps -o ppid= -p "$pid" 2>/dev/null | tr -d ' ' || true)"
  if [ -n "$FIXTURE" ] && [ "$env_now" = "$FIXTURE" ]; then
    : # 身份仍然对得上，收
  elif [ "${ppid_now:-}" != "1" ]; then
    echo "  跳过 pid=$pid —— 现在 ppid=${ppid_now:-<没了>} 且身份对不上，不再符合判据（pid 复用是真事）"
    continue
  fi
  kill "$pid" 2>/dev/null && echo "  已收 pid=$pid" || echo "  收不动 pid=$pid（权限？已退出？）"
done
