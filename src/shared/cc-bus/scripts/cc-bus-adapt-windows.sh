#!/usr/bin/env bash
# cc-bus-adapt-windows.sh —— ② OS 适配 ＋ ③ 存储适配的 **Windows 那一侧**(设计)。
#
# ## 这一侧今天做到哪(如实写,别读宽)
#
# ✅ **存储适配做了**:cc-peek / cc-commit 在 Windows 的 bash(MSYS2 / Cygwin / Git-Bash)上
#    跑得起来 —— 只要那台机器上有 `flock(1)`。判据靠 `CCBUS_ADAPT_OS=windows` 在 Linux 上
#    把这一侧整段跑一遍(否则"Windows 那一侧"永远没人判,只能靠读代码相信)。
# ⚠ **共享读锁买不到**:`flock -s` 这一侧没有对应物 ⇒ `store_lock_shared` **退化成排他**,
#    并在 `bus.log` 里说清降级了。预登记的正是这一格:
#    「若那一侧没有,`bus-peek` 在 Windows 上要退化成排他锁,**语义自陈那一层就丢了**」。
#    ⇒ 丢的只有"自陈"(读锁 ＝ 这一跳不写 这条**表达**);互斥与正确性一条没丢 ——
#      排他比共享**更严**,peek 仍然不写任何东西(零写面由判据现打,不靠锁的种类保证)。
# ❌ **投递通道没做**:Windows 上没有 tmux,ConPTY 那条路本轮**一行都没写**。
#    ⇒ `os_send_keys` / `os_pane_fingerprint` **显式返回 13(办不到)**,不静默成功、不假装投递。
# 这里原先引的同族是 `cc_bus_deploy.rs` 那句「本机没有可查的 ccm —— 这一格没做预检」;
#    那一格今天**真探了**(问 cc-monitor 装的那份 ccm 的 build 与能力,`windows_ccm_precheck`),
#    而**本文件这一格仍然没做** —— 两件事从此不再同族,别照旧读。纪律照旧:
#    没做就写"没做",别让调用方以为敲过门了。
# ⚠ **没有真 Windows 机器现打**:本文件的全部证据来自「在 Linux 上按这一侧的实现跑一遍」。
#    `flock` 在 MSYS2 里有(util-linux 包)、Git-Bash 默认**没有** —— 那一格**判不了**,
#    所以缺 `flock` 时也走 13,而不是回落到一个"看起来在锁、其实没锁"的假锁。
#
# ## 为什么不用 mkdir 自旋锁兜底
#
# 无 `flock` 时常见的兜底是 `mkdir` 原子建目录 ＋ 失败重试。本文件**刻意不做**,两条理由:
#   ① 重试要 `sleep`,而 `sleep` 在本仓是**登记在册的周期唤醒**(轮询登记表逐文件记数);
#      为一个没有真机验证过的兜底新增一处轮询,是拿一条真判据换一段想象。
#   ② 不带等待的 `mkdir` 锁会在有人持锁时**当场失败**,那与"锁不住"只差一句话 ——
#      而 13 这条路至少让调用方知道"这一侧办不到",不会写出半行。
#
# 只提供函数、不自执行;由 `cc-bus-adapt.sh` 的 `ccbus_adapt_load` source 进来。

_win_have_flock() { command -v flock >/dev/null 2>&1; }

# ── ③ 存储 ─────────────────────────────────────────────────────────────────
# `shared_lock=no` 是这一侧与 POSIX 的**唯一一处能力差**,判据按这一位对拍两侧。
store_caps() { printf 'os=windows lock=flock-or-none shared_lock=no atomic_append=append-under-lock timestamp=date-iso path=pwd-fallback\n'; }

store_lock_exclusive() {
  if _win_have_flock; then flock "$1"; return $?; fi
  printf 'cc-bus-adapt(windows): 这台机器上没有 flock —— 排他锁办不到,不假装成功\n' >&2
  ccbus_adapt_log "NOLOCK windows 缺 flock ⇒ 拒绝(rc=13)"
  return 13
}

# 🔴 共享退化成排他:**语义自陈丢了,互斥没丢**。每次都记一行,别让降级变成静默。
store_lock_shared() {
  ccbus_adapt_log "DEGRADE lock:shared→exclusive(windows 侧没有 flock -s 的对应物)"
  store_lock_exclusive "$1"
}

store_append_line() { printf '%s\n' "$2" >> "$1"; }

store_now() { date -Iseconds; }

# `readlink -f` 在部分 Windows bash 里没有 ⇒ 退回"进目录再 pwd"。
# ⚠ **买不到**:盘符大小写、UNC 路径、`/c/` 与 `C:\` 两种写法的互认 —— 无真机,判不了。
store_path_real() {
  if readlink -f "$1" 2>/dev/null; then return 0; fi
  local d b
  d=$(dirname "$1"); b=$(basename "$1")
  printf '%s/%s\n' "$(cd "$d" 2>/dev/null && pwd)" "$b"
}

# ── ② OS ───────────────────────────────────────────────────────────────────
# `deliver=none`:这一侧**没有投递通道**。能力自陈成 none,上层按能力降级(§3.2b),
# 而不是等到投递那一刻才发现 tmux 不存在。
os_caps() { printf 'os=windows deliver=none fingerprint=none\n'; }

os_send_keys() {
  printf 'cc-bus-adapt(windows): 投递通道这一格没做(ConPTY 那条路本轮一行都没写)\n' >&2
  ccbus_adapt_log "NOIMPL windows os_send_keys target=$1"
  return 13
}

os_pane_fingerprint() {
  ccbus_adapt_log "NOIMPL windows os_pane_fingerprint target=$1"
  return 13
}
