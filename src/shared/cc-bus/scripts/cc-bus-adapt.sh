#!/usr/bin/env bash
# cc-bus-adapt.sh —— 三个适配面的【契约 ＋ 装载口】(设计 95 §3.2 / §3.2b)。
#
# ## 为什么有这一层
#
# 95 §3.1 现打:cc-bus 的脚本里 tmux 47 处、flock 20 处 —— **三个适配面全糊在通用层里**,
# 换一个 OS 或换一个 agent 就要改遍所有脚本。本文件把那道缝写成**函数名的契约**:
#   ① agent 适配 —— 结束钩子形状(能不能拦停)、文本怎么算"提交"、能力三位
#   ② OS   适配 —— 投递通道(POSIX: tmux send-keys ↔ Windows: 别的通道)、进程指纹
#   ③ 存储 适配 —— 排他/共享锁、原子追加、时间戳、路径原语
#
# 🔴 **缝切在 shell 里,不搬进后端**(条 55,用户 2026-09-18 逐字「不要. cc-bus 就是额外的
#    东西解耦清楚」)⇒ 通用层(cc-peek / cc-commit / cc-bus-lib.sh 的阀门段)不认识
#    tmux / flock,只认识下面这三张表里的函数名。
#
# ## ①②③ 刻意用两种抄法,不是一种(95 §3.2b)
#
# · ② OS 与 ③ 存储 照 `platform/` 抄:**判定相同、只是读法不同** ⇒ 翻译官形。
# · ① agent 照 `agents/` 抄:**词典式,不立大 trait** —— 今天只有 claude 那一份是已知的
#   (`D4`:接口由现有能力反推)。而且它比翻译官**多一维**:别的 agent 可能**根本没有**
#   结束钩子 ⇒ 那不是"说法不同",是**能力缺失**,翻译官在这一格失效。
#   ⇒ 每个 agent 词典声明自己支持哪几种投递语义(can_block / can_nudge / can_readback),
#     配的行为在不支持时**按下面那条降级链降级,不硬失败**。
#
# 本文件只提供函数与常量、**不自执行**;被 cc-peek / cc-commit / cc-bus-lib.sh source。
# 返回码:13 = 适配层能力缺失(没装齐 / 这一侧没做)—— 与 11/12 同族:不是崩,是"办不到"。

# ── 契约:每个 trait 的必备函数 ───────────────────────────────────────────────
# ⚠ 这三张表是**判据的分母**:装载后逐项现打 `declare -F`,少一个就 13(fail-closed)。
#   表空了判据当场红(反空真),不会因为"扫不到"而放行。
CCBUS_TRAIT_STORE_FNS="store_caps store_lock_shared store_lock_exclusive store_append_line store_now store_path_real"
CCBUS_TRAIT_OS_FNS="os_caps os_send_keys os_pane_fingerprint"
CCBUS_TRAIT_AGENT_FNS="agent_caps agent_block_reason agent_submit_enter"

# ── 能力降级链:§3.2b 那张表的**唯一的家** ───────────────────────────────────
# kinds.tsv 里配的每一项行为,在 agent 不支持时都必须有一个**明确的去处** ——
# 没有去处的组合 = 配置错误,部署时就该报,不该等到投递时静默丢。
#   拦停 block ⇒ 降成敲门 nudge(至少让它知道有消息)
#   敲门 nudge ⇒ 降成只入收件箱 inbox(等它自己来读)
#   都不支持   ⇒ **投递前就拒** reject(别写进收件箱让消息烂在那儿)
CCBUS_DEGRADE_CHAIN="block:nudge nudge:inbox inbox:reject"
# 行为 → 它要求的那一位能力(判据对拍:两侧的键集合必须相等)
CCBUS_BEHAVIOR_CAP="block:can_block nudge:can_nudge inbox:can_readback"
# 链的终点:它不是一种行为,是"拒"。判据要求这条链**无环且终于它**。
CCBUS_DEGRADE_SINK="reject"

ccbus_adapt_dir() { (cd "$(dirname "${BASH_SOURCE[0]}")" && pwd); }

# 适配日志:降级、能力缺失都要留痕(§3.2b 逐字「并在 bus.log 里说清降级了」)。
# ⚠ 刻意不调 route_log:本文件会被**不 source cc-bus-lib.sh** 的 cc-peek / cc-commit 用。
ccbus_adapt_log() {
  local bus="${BUS:-${CC_BUS_HOME:-$HOME/.cc-bus}}"
  mkdir -p "$bus/log" 2>/dev/null || true
  printf '[%s] ADAPT %s\n' "$(date -Iseconds 2>/dev/null)" "$*" >> "$bus/log/bus.log" 2>/dev/null || true
}

# 认 OS 族。⚠ 只分两族:windows 与 posix。
# **不认识的 uname 归 posix 并留一行日志**,不 fail-closed —— 这是刻意的:
# 今天真做出来的只有 posix 那一族,把 SunOS 之类判成"办不到"等于凭空关掉一台能跑的机器;
# 而真正该 fail-closed 的那一格在下面 `ccbus_adapt_require`(函数少了就 13)。
ccbus_adapt_detect_os() {
  case "$(uname -s 2>/dev/null || printf 'unknown')" in
    MINGW*|MSYS*|CYGWIN*|Windows_NT) printf 'windows' ;;
    Linux|Darwin|*BSD|DragonFly)     printf 'posix' ;;
    *) ccbus_adapt_log "uname 不认识 —— 按 posix 装载"; printf 'posix' ;;
  esac
}

# 装齐没有?少一个函数就 13。**fail-closed 的那一格在这儿**,不在 detect。
ccbus_adapt_require() {
  local trait="$1" fns="$2" f miss=""
  for f in $fns; do
    declare -F "$f" >/dev/null 2>&1 || miss="$miss $f"
  done
  if [ -n "$miss" ]; then
    printf 'cc-bus-adapt: %s 适配没装齐,缺:%s\n' "$trait" "$miss" >&2
    ccbus_adapt_log "MISSING $trait:$miss"
    return 13
  fi
  return 0
}

# 装载三个适配面。
# CCBUS_ADAPT_OS / CCBUS_ADAPT_AGENT 是**判据用的强制口**(照 `K_R80_ROOT` 那条取法),
# 日常跑一律不带 —— 带上就能在 Linux 上跑 Windows 那一侧的实现,否则那一侧永远没人判。
ccbus_adapt_load() {
  local dir os agent osf agentf
  dir=$(ccbus_adapt_dir)
  os="${CCBUS_ADAPT_OS:-$(ccbus_adapt_detect_os)}"
  agent="${CCBUS_ADAPT_AGENT:-claude}"
  osf="$dir/cc-bus-adapt-$os.sh"
  agentf="$dir/cc-bus-agent-$agent.sh"
  if [ ! -f "$osf" ]; then
    printf 'cc-bus-adapt: 没有 %s 这一侧的实现(%s)\n' "$os" "$osf" >&2
    ccbus_adapt_log "NOIMPL os=$os"; return 13
  fi
  if [ ! -f "$agentf" ]; then
    printf 'cc-bus-adapt: 没有 %s 这个 agent 的词典(%s)\n' "$agent" "$agentf" >&2
    ccbus_adapt_log "NOIMPL agent=$agent"; return 13
  fi
  # shellcheck disable=SC1090
  . "$osf" || return 13
  # shellcheck disable=SC1090
  . "$agentf" || return 13
  CCBUS_ADAPT_OS_LOADED="$os"; CCBUS_ADAPT_AGENT_LOADED="$agent"
  ccbus_adapt_require 存储 "$CCBUS_TRAIT_STORE_FNS"  || return 13
  ccbus_adapt_require OS    "$CCBUS_TRAIT_OS_FNS"    || return 13
  ccbus_adapt_require agent "$CCBUS_TRAIT_AGENT_FNS" || return 13
  return 0
}

# 取某个 caps 串里的一位:`ccbus_cap agent_caps can_block` → yes / no / 空
ccbus_cap() {
  local fn="$1" key="$2" s
  s=$("$fn" 2>/dev/null || true)
  printf '%s' " $s " | sed -n "s/.* $key=\([^ ]*\).*/\1/p"
}

# 按能力降级:给一个想要的行为,返回**这个 agent 真做得到**的那一档(或 reject)。
# 每降一级记一行日志 —— 静默降级与静默丢在输出上一模一样,那正是本仓在治的病。
ccbus_degrade() {
  local want="$1" cur="$want" cap nxt guard=0
  while [ "$cur" != "$CCBUS_DEGRADE_SINK" ]; do
    guard=$((guard+1)); [ "$guard" -le 8 ] || { printf '%s' "$CCBUS_DEGRADE_SINK"; return 0; }
    cap=$(printf '%s' " $CCBUS_BEHAVIOR_CAP " | sed -n "s/.* $cur:\([^ ]*\).*/\1/p")
    if [ -z "$cap" ]; then printf '%s' "$CCBUS_DEGRADE_SINK"; return 0; fi
    if [ "$(ccbus_cap agent_caps "$cap")" = "yes" ]; then printf '%s' "$cur"; return 0; fi
    nxt=$(printf '%s' " $CCBUS_DEGRADE_CHAIN " | sed -n "s/.* $cur:\([^ ]*\).*/\1/p")
    [ -n "$nxt" ] || { printf '%s' "$CCBUS_DEGRADE_SINK"; return 0; }
    ccbus_adapt_log "DEGRADE $cur→$nxt(agent=${CCBUS_ADAPT_AGENT_LOADED:-?} 不支持 $cap)"
    cur="$nxt"
  done
  printf '%s' "$CCBUS_DEGRADE_SINK"
}
