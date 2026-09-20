#!/usr/bin/env bash
# cc-bus-adapt-posix.sh —— ② OS 适配 ＋ ③ 存储适配的 **POSIX 实现**(设计 95 §3.2)。
#
# 它是今天唯一**真跑在生产上**的那一侧:投递通道 = tmux send-keys,锁 = flock(1)。
# 只提供函数、不自执行;由 `cc-bus-adapt.sh` 的 `ccbus_adapt_load` source 进来。
#
# ⚠ **本文件是允许出现 tmux / flock 的地方**(判据的正控就靠它:通用层零脚印这一条,
#   要能证明扫描器真的看得见这两个词,否则"扫不到"与"没有"在输出上一模一样)。

# ── ③ 存储 ─────────────────────────────────────────────────────────────────
# 能力自陈。`shared_lock=yes` 是 POSIX 这一侧**独有**的那一位 —— Windows 那一侧买不到它,
# 两边的差就写在这一行上(95 §3ter.7 登记的那条"判不了"在这儿变成一个现打得出来的值)。
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
os_caps() { printf 'os=posix deliver=tmux-send-keys fingerprint=pane_pid\n'; }

# 往目标 pane 打字。`=` 前缀 = tmux 的精确名匹配(不做模糊),与老代码一字同义。
os_send_keys()        { local t="$1"; shift; tmux send-keys -t "=$t" "$@" 2>/dev/null; }
os_pane_fingerprint() { tmux display-message -p -t "=$1" '#{pane_pid}' 2>/dev/null; }
