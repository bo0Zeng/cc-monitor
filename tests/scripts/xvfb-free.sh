#!/usr/bin/env bash
# 起一台私有 Xvfb：挑一个空号、占住，挑到的显示号（不带冒号）由 Xvfb 自己写在标准输出第一行（-displayfd，
# 它绑好套接字之后才写），本脚本 exec 成那台 Xvfb 本身（调用方拿到的 pid 就是它）。
# 文件窗口台架（tests/frontend/filewin/xvfb_rig.rs）与截图工具（tests/shots/filewin.mjs）都经这一份起，
# 两边不各写一段号。
#
# 为什么不光靠 -displayfd 让服务器自己挑：门禁把单测跑在新的网络命名空间里（bwrap --unshare-net），
# 而服务器认「这个号有人了」看的是抽象套接字 —— 它按网络命名空间分开 ⇒ 沙箱里外各起一台会挑到同一个号。
# 文件系统那一份是共用的 ⇒ 占号用 X 的老规矩：/tmp/.X<n>-lock（O_EXCL 建、写自己的 pid），
# 挑号整段在 /tmp/.X11-claim.lock 的 flock 里（回收死锁与建锁不交错）。记的进程已不在的锁当场回收。
#
#   xvfb-free.sh                    起一台（号段 :100–:899，避开真会话常用的低位号）
#   xvfb-free.sh release <n> <pid>  收场：锁记的是 <pid> 才删那个锁与那份套接字（别人的不碰）
set -euo pipefail

if [ "${1:-}" = release ]; then
  n=$2
  pid=$3
  lock=/tmp/.X$n-lock
  if [ "$(tr -d ' \n' < "$lock" 2> /dev/null || true)" = "$pid" ]; then
    rm -f "$lock" "/tmp/.X11-unix/X$n"
  fi
  exit 0
fi
if [ "$#" -gt 0 ]; then
  echo "xvfb-free.sh: 只认不带参数（起一台）或 release <n> <pid>" >&2
  exit 2
fi

exec 9> /tmp/.X11-claim.lock
flock 9
for n in $(seq 100 899); do
  lock=/tmp/.X$n-lock
  if [ -e "$lock" ]; then
    held=$(tr -d ' \n' < "$lock" 2> /dev/null || true)
    if [ -n "$held" ] && [ -d "/proc/$held" ]; then
      continue
    fi
    rm -f "$lock" "/tmp/.X11-unix/X$n"
  fi
  [ -e "/tmp/.X11-unix/X$n" ] && continue
  if (set -o noclobber; printf '%10d\n' "$$" > "$lock") 2> /dev/null; then
    exec 9>&-
    exec Xvfb ":$n" -displayfd 1 -screen 0 1600x1200x24 -nolisten tcp -noreset
  fi
done
echo "xvfb-free.sh: :100–:899 没有空号" >&2
exit 1
