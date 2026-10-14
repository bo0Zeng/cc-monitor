#!/usr/bin/env bash
# cc-bus-adapt-posix.sh —— ② OS 适配 ＋ ③ 存储适配的 **POSIX 实现**(设计)。
#
# 它是今天唯一**真跑在生产上**的那一侧:读窗格身份 = tmux 窗格上的标记,锁 = flock(1)。
# 投递(敲门)不在这一层:往会话说一句交给后端(`ccm -- --session-say`,见 cc-bus-lib.sh 的 `ccbus_say`)。
# 只提供函数、不自执行;由 `cc-bus-adapt.sh` 的 `ccbus_adapt_load` source 进来。
#
# ⚠ **本文件是允许出现 tmux / flock 的地方**(判据的正控就靠它:通用层零脚印这一条,
#   要能证明扫描器真的看得见这两个词,否则"扫不到"与"没有"在输出上一模一样)。

# ── ③ 存储 ─────────────────────────────────────────────────────────────────
# 能力自陈。`shared_lock=yes` 是 POSIX 这一侧**独有**的那一位 —— Windows 那一侧买不到它,
# 两边的差就写在这一行上(登记的那条"判不了"在这儿变成一个现打得出来的值)。
store_caps() { printf 'os=posix lock=flock shared_lock=yes atomic_append=append-under-flock timestamp=date-iso path=readlink-f\n'; }

# 共享读锁。⚠ 全仓此前 13 处真调用**全是排他**,这是第一处 `-s`:
# 它的理由**不是性能**(读者本来就少),是**自陈** —— 读锁 ＝ 这一跳不写,
# 与 `D10`「只生成与真写入是两跳」的第一跳在锁上同形。
store_lock_shared()    { flock -s "$1"; }
store_lock_exclusive() { flock "$1"; }

# 原子追加:**调用方必须已经持有该文件的排他锁**(锁在外面,这里只管写)。
store_append_line() { printf '%s\n' "$2" >> "$1"; }

store_now()       { date -Iseconds; }
store_path_real() { readlink -f "$1"; }

# ── ② OS ───────────────────────────────────────────────────────────────────
os_caps() { printf 'os=posix identity=pane-tag fingerprint=pane_pid\n'; }

# 那个窗格里跑的是哪条会话:读后端起会话时打在窗格上的身份标记(`@ccm_sid`,只读,不碰后端的盘)。
# `<登记时的 pid>` 给了 ⇒ 先核窗格根进程:对不上(名字被别人占了)rc=3,不认它的身份 —— 否则敲门会敲到占了这个名字的那一条。
# 出:标记的值(没打标记 ⇒ 空串)rc=0;没有这个窗格 rc=1。`=` 前缀 = tmux 的精确名匹配。
os_pane_identity() {
  local got pid
  got=$(tmux display-message -p -t "=$1" '#{pane_pid} #{@ccm_sid}' 2>/dev/null) || return 1
  [ -n "$got" ] || return 1
  pid=${got%% *}
  if [ -n "${2:-}" ] && [ "$pid" != "$2" ]; then return 3; fi
  case "$got" in *' '*) printf '%s' "${got#* }" ;; esac
}
