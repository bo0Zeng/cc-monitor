#!/usr/bin/env bash
# cc-bus-lib.sh — 路由管线的【唯一实现】,被 cc-send source(投递只一条路:就地跑管线写进收件箱)。
# 约定:route_* 阀门返回 0=放行,返回 1=拦截(并已 route_log 记原因)。
# 本文件只提供函数、不自执行。BUS 由调用方设好(或此处按 CC_BUS_HOME 兜底)。
#
# 管线顺序(route_process):policy → rate → deliver → nudge → log。防风暴只靠限流。
#
# config 键(~/.cc-bus/config,sourceable KEY=VAL,全部有默认):
#   CCBUS_NUDGE_DEBOUNCE  同一收件人两次敲门最小间隔秒(默 2)
#   CCBUS_POLICY_MODE     off=全放行 / on=按 policy.tsv 强制 ACL(默 off)
#   CCBUS_RATE_PAIR / CCBUS_RATE_GLOBAL / CCBUS_RATE_WINDOW  限流(见 route_rate_check)
# policy.tsv 行:  <from-glob>\t<允许的 to-glob,逗号分隔>   (# 开头为注释)

: "${BUS:=${CC_BUS_HOME:-$HOME/.cc-bus}}"

# ── 两个适配面(设计):本文件是**通用层**,不认识 tmux / flock ─────────────
# 缝切在 shell 里、**不搬进后端**。装不齐就地 return 13(fail-closed):
# 少一个函数而继续跑,后果是投递路径上某一步静默变成 no-op —— 那正是本仓在治的病。
# shellcheck source=cc-bus-adapt.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/cc-bus-adapt.sh"
ccbus_adapt_load || return 13

# ── 敲门那一句 ───────────────────────────────────────────────────────────
# 🔴 敲门那一句是对方的输入,正文进去就成了指令 ⇒ 只填发信人(过了白名单)。正文只经 Stop 钩子喂回。
ccbus_knock_text() { printf '🔔 cc-bus: 你有来自 %s 的新消息,运行 cc-recv 读取并按内容处理' "$1"; }

# 两阶段读口的锚(cc-peek 发、cc-commit 校验,**两边必须是同一份实现**,否则永远比不中)。
# 取第 <行号> 行现算 cksum,形如 `<校验和>-<字节数>`,零新依赖。
twophase_anchor() { sed -n "${2}p" "$1" | cksum | awk '{print $1"-"$2}'; }

route_load_config() {
  # shellcheck disable=SC1091
  [ -f "$BUS/config" ] && . "$BUS/config" || true
  : "${CCBUS_NUDGE_DEBOUNCE:=2}"
  : "${CCBUS_POLICY_MODE:=off}"
  : "${CCBUS_RATE_PAIR:=0}"             # 0=关
  : "${CCBUS_RATE_GLOBAL:=0}"           # 0=关
  : "${CCBUS_RATE_WINDOW:=60}"
  # 数字键防呆:config typo(非数字)回落安全值,避免 [ -ge ] 报错刷屏 + 静默关阀
  [[ "$CCBUS_NUDGE_DEBOUNCE" =~ ^[0-9]+$ ]] || CCBUS_NUDGE_DEBOUNCE=2
  [[ "$CCBUS_RATE_WINDOW"    =~ ^[0-9]+$ ]] || CCBUS_RATE_WINDOW=60
  local _k
  for _k in CCBUS_RATE_PAIR CCBUS_RATE_GLOBAL; do
    [[ "${!_k}" =~ ^[0-9]+$ ]] || printf -v "$_k" '%s' 0
  done
}

route_log() { printf '[%s] %s\n' "$(store_now)" "$*" >> "$BUS/log/bus.log" 2>/dev/null || true; }

# 阀门1:ACL。off/无 policy.tsv → 放行;on → 首个匹配 from 的规则决定 to 是否允许。
route_policy_check() {
  local from="$1" to="$2" pf="$BUS/policy.tsv" mode="${CCBUS_POLICY_MODE:-off}"
  case "${mode,,}" in on|1|true|yes|enabled) ;; *) return 0;; esac   # 大小写不敏感;其余=全放行
  [ -f "$pf" ] || return 0
  local fglob allow g matched=0 ok=0
  while IFS=$'\t' read -r fglob allow; do
    [ -n "$fglob" ] || continue
    case "$fglob" in \#*) continue;; esac
    # shellcheck disable=SC2254
    if [[ "$from" == $fglob ]]; then
      matched=1
      # 用 read -ra 按逗号切,避免 globs=($allow) 对 "*" 做路径名展开(会把 * 展成文件名)
      local globs; IFS=',' read -ra globs <<< "$allow"
      for g in "${globs[@]}"; do
        g="${g// /}"; [ -n "$g" ] || continue
        # shellcheck disable=SC2254
        if [[ "$to" == $g ]]; then ok=1; break; fi
      done
      break
    fi
  done < "$pf"
  { [ "$matched" = 1 ] && [ "$ok" = 1 ]; } && return 0
  route_log "REJECT acl $from->$to"
  return 1
}

# 阀门2:限流(per-pair + global 固定窗口计数 + 熔断)。全 0=关(放行)。
# 拆成【只读 CHECK】(投递前)+ 【COMMIT】(deliver 成功后才做):
# 一次 deliver 失败不留计数增量:发信方重发时不会被误 THROTTLE。
# 代价:并发投递下限额变近似(可能超出约并发度),对限流可接受。
route_rate_check() {
  local from="$1" to="$2"
  local rp="${CCBUS_RATE_PAIR:-0}" rg="${CCBUS_RATE_GLOBAL:-0}" rw="${CCBUS_RATE_WINDOW:-60}"
  [ "$rp" = 0 ] && [ "$rg" = 0 ] && return 0
  if [ "$rp" != 0 ]; then
    _rate_peek "$BUS/state/rate-${from}__${to}" "$rp" "$rw" || { route_log "THROTTLE pair $from->$to"; return 1; }
  fi
  if [ "$rg" != 0 ]; then
    _rate_peek "$BUS/state/rate-global" "$rg" "$rw" || { route_log "THROTTLE global $from->$to"; return 1; }
  fi
  return 0
}
route_rate_commit() {
  local from="$1" to="$2"
  local rp="${CCBUS_RATE_PAIR:-0}" rg="${CCBUS_RATE_GLOBAL:-0}" rw="${CCBUS_RATE_WINDOW:-60}"
  [ "$rp" != 0 ] && _rate_bump "$BUS/state/rate-${from}__${to}" "$rw"
  [ "$rg" != 0 ] && _rate_bump "$BUS/state/rate-global" "$rw"
  return 0
}
# _rate_peek <file> <limit> <window> → 0=未超限(不写) / 1=超限。只读;窗口翻转按 0 计。
_rate_peek() {
  local f="$1" limit="$2" win="$3" now
  now=$(date +%s)
  mkdir -p "$BUS/state"
  (
    flock 7
    start=0; cnt=0
    read -r start cnt 2>/dev/null < "$f" || true   # 2>/dev/null 须在 < 前才压得住"文件不存在"
    [[ "$start" =~ ^[0-9]+$ ]] || start=0
    [[ "$cnt" =~ ^[0-9]+$ ]] || cnt=0
    if [ $(( now - start )) -ge "$win" ]; then cnt=0; fi   # 窗口翻转 → 检查按重置算
    [ "$cnt" -lt "$limit" ]
  ) 7>>"$f.lock"
}
# _rate_bump <file> <window> → 递增固定窗口计数。仅在 deliver 成功后调用。
_rate_bump() {
  local f="$1" win="$2" now
  now=$(date +%s)
  mkdir -p "$BUS/state"
  (
    flock 7
    start=0; cnt=0
    read -r start cnt 2>/dev/null < "$f" || true
    [[ "$start" =~ ^[0-9]+$ ]] || start=0
    [[ "$cnt" =~ ^[0-9]+$ ]] || cnt=0
    if [ $(( now - start )) -ge "$win" ]; then start=$now; cnt=0; fi
    printf '%s %s\n' "$start" "$((cnt+1))" > "$f"
  ) 7>>"$f.lock"
}

# 投递:写收件人 inbox(源头),加锁。返回非零=投递失败。
route_deliver() {
  local to="$1" line="$2"
  local inbox="$BUS/inbox/$to.jsonl"      # 独立行:同一 local 里引用刚赋的 $to 会取到旧值
  mkdir -p "$BUS/inbox" || return 1
  ( store_lock_exclusive 9; store_append_line "$inbox" "$line" ) 9>>"$inbox.lock" || return 1
  return 0
}

# 经后端往那条会话说一句(敲门的那一句):先看一眼那一屏拿指纹,再带着它说 ——
#   后端那一道照它的规矩判:窗格真是那条会话 · 只在它闲着时送(`idle_only`:一轮在跑 · 在等人 · 判不出都不送,不插进那一轮中间)·
#   画面在这两下之间变了(人正在打字)就不送。
#   看不到那一屏 / 没送 ⇒ rc=1(调用方照只入收件箱办;回合结束时 Stop 钩子照样喂回,不丢)。终端没了不恢复(带了指纹就不恢复)。
# 找不到后端 ⇒ rc=1 并说清。cc-bus 只认后端的公开命令,不认它的数据布局。
ccbus_say() {   # <会话编号> <那一句>
  local sid="$1" text="$2" ccm screen out res
  ccm=$(ccbus_ccm_bin) || { route_log "NUDGE noccm sid=$sid —— 找不到后端(~/.cc-monitor/bin/ccm),只入收件箱"; return 1; }
  screen=$(jq -cn --arg s "$sid" '{sid:$s, color:false}' \
    | "$ccm" -- --terminal-preview --within-ms 10000 2>/dev/null | jq -r '(.data // .).screen // empty' 2>/dev/null)
  if [ -z "$screen" ]; then
    route_log "NUDGE noscreen sid=$sid —— 看不到那一屏,只入收件箱"; return 1
  fi
  out=$(jq -cn --arg s "$sid" --arg t "$text" --arg v "$screen" '{sid:$s, text:$t, seen_screen:$v, idle_only:true}' \
    | "$ccm" -- --session-say --within-ms 10000 2>/dev/null)
  res=$(printf '%s' "$out" | jq -r '(.data // .).result // empty' 2>/dev/null)
  case "$res" in delivered|unsure) return 0 ;; esac
  route_log "NUDGE refused sid=$sid why=$(printf '%s' "$out" | jq -r '(.data // .).why // "?"' 2>/dev/null) —— 没送,只入收件箱"
  return 1
}

# 敲门(去抖):同一收件人 DEBOUNCE 秒内只敲一次;经后端往它的会话说一句(`ccbus_say`),不往窗格里打字。
# 收件人就是会话 id(信封由后端核过),敲的就是那一条会话。
route_nudge() {
  local to="$1" from="$2"
  local nf="$BUS/state/nudge-$to" now last   # 独立行:$to 已赋值后再拼路径
  now=$(date +%s)
  mkdir -p "$BUS/state"
  (
    flock 8
    last=$(cat "$nf" 2>/dev/null || echo 0); [[ "$last" =~ ^[0-9]+$ ]] || last=0
    if [ $(( now - last )) -ge "${CCBUS_NUDGE_DEBOUNCE:-2}" ]; then
      if ccbus_say "$to" "$(ccbus_knock_text "$from")"; then
        echo "$now" > "$nf"
      fi
    else
      route_log "NUDGE skip $to"
    fi
  ) 8>>"$nf.lock"
}

# 管线入口:收的是后端 `bus-envelope` 盖好的规范信封(信封的规矩只在后端那一处,这里一格都不再判)。
# 返回码:0=已投递;10=被阀门拦截(不投递);1=投递出错。
# 两档:tier 1 ⇒ 投完敲门(对方闲着才送);tier 2 ⇒ 只进收件箱,不敲 —— 都由 Stop 钩子在轮末喂回。
route_process() {
  local line="$1" from to tier
  from=$(printf '%s' "$line" | jq -r '.from' 2>/dev/null)
  to=$(printf '%s' "$line" | jq -r '.to' 2>/dev/null)
  tier=$(printf '%s' "$line" | jq -r '.tier' 2>/dev/null)
  # 这两道门是【只读】的;限流的计数仅在 deliver 成功后(见下)才提交。
  route_policy_check "$from" "$to" || return 10
  route_rate_check   "$from" "$to" || return 10
  # 能力降级:按收件方 agent 词典的三位能力,把「拦停」降到做得到的那一档。
  #   拦停 ⇒ 敲门 ⇒ 只入收件箱 ⇒ **投递前就拒**(别让消息烂在收件箱里)。每降一级 bus.log 里一行。
  # ⚠ 买不到:「收件方是哪种 agent」—— 按**本进程**装载的词典算(今天只有 claude 一本)。
  local got nudge_on=no
  got=$(ccbus_degrade block)
  if [ "$got" = "$CCBUS_DEGRADE_SINK" ]; then
    route_log "REJECT $from->$to —— 收件方一种投递语义都做不到(降级链走到底),投递前就拒"
    return 10
  fi
  case "$got" in
    nudge) nudge_on=yes ;;                                   # 拦停降成了敲门
    block) [ "$(ccbus_degrade nudge)" = nudge ] && nudge_on=yes ;;
  esac
  route_deliver "$to" "$line" || { route_log "ERROR deliver $from->$to"; return 1; }
  route_rate_commit "$from" "$to"
  if [ "$tier" != 1 ]; then
    route_log "NUDGE tier2 $to —— 二档只进收件箱"
  elif [ "$nudge_on" = yes ]; then
    route_nudge "$to" "$from" || true
  else
    route_log "NUDGE off $to"
  fi
  route_log "DELIVER $from->$to"
  return 0
}

# 旧收件箱迁一次(A13-2):旧版按名字存收件箱、信封没有 tier / act / title,自动接回复记在 state/lastread-*。
# 标记 state/envelope-2 在 ⇒ 什么都不做(盘上只有一种读法)。不在 ⇒ 锁里做一次:
#   ① 旧的整份(收件箱 · 已读位置 · lastread · 敲门去抖)挪进 legacy/<时刻>/ 存档,不再读;
#   ② 还没读的那几条照登记那一格(agents.tsv 第 5 列)把名字换成会话 id,补成二档 inform 的新信封,
#      **过后端那一处校验**再写进新收件箱;换不出会话 id · 后端拒(正文超长之类)· 找不到后端 ⇒ 只留在存档里,bus.log 记一行。
# 被 cc-send · cc-recv · Stop 钩子调(谁先跑谁迁)。迁不成也不挡调用方。
ccbus_migrate_once() {
  local mark="$BUS/state/envelope-2"
  [ -e "$mark" ] && return 0
  mkdir -p "$BUS/state" "$BUS/inbox" "$BUS/log" || return 0
  (
    store_lock_exclusive 7 || exit 0
    [ -e "$mark" ] && exit 0
    old="$BUS/legacy/$(date +%Y%m%d-%H%M%S)"
    mkdir -p "$old/inbox" "$old/state" || exit 0
    for f in "$BUS"/inbox/*.jsonl; do [ -e "$f" ] && mv "$f" "$old/inbox/"; done
    rm -f "$BUS"/inbox/*.lock
    for f in "$BUS"/state/*.pos "$BUS"/state/lastread-* "$BUS"/state/nudge-*; do [ -e "$f" ] && mv "$f" "$old/state/"; done
    ccm=$(ccbus_ccm_bin 2>/dev/null) || ccm=""
    for f in "$old"/inbox/*.jsonl; do
      [ -e "$f" ] || continue
      name=$(basename "$f" .jsonl)
      pos=$(cat "$old/state/$name.pos" 2>/dev/null || echo 0); [[ "$pos" =~ ^[0-9]+$ ]] || pos=0
      to_sid=$(awk -F'\t' -v id="$name" '$1==id{s=$5} END{print s}' "$BUS/agents.tsv" 2>/dev/null || true)
      tail -n "+$((pos+1))" "$f" | while IFS= read -r line; do
        [ -n "$line" ] || continue
        from_name=$(printf '%s' "$line" | jq -r '.from // empty' 2>/dev/null) || from_name=""
        from_sid=$(awk -F'\t' -v id="$from_name" '$1==id{s=$5} END{print s}' "$BUS/agents.tsv" 2>/dev/null || true)
        req=$(printf '%s' "$line" | jq -c --arg f "$from_sid" --arg t "$to_sid" \
          '{from:$f, to:$t, tier:2, act:"inform", text:(.text // ""),
            title:(((.text // "") | split("\n")[0] | .[0:70]) as $h | if $h == "" then "legacy" else $h end)}' 2>/dev/null) || req=""
        env=""
        if [ -n "$req" ] && [ -n "$ccm" ]; then
          env=$(printf '%s' "$req" | "$ccm" -- --bus-envelope --within-ms 10000 2>/dev/null | jq -c '(.data // .)' 2>/dev/null) || env=""
        fi
        if [ -n "$env" ] && route_deliver "$(printf '%s' "$env" | jq -r .to)" "$env"; then
          route_log "MIGRATE kept $name from=$from_name"
        else
          route_log "MIGRATE archived-only $name from=$from_name —— 换不出会话 id · 后端拒 · 或找不到后端,原样留在 $old"
        fi
      done
    done
    touch "$mark"
  ) 7>>"$BUS/state/migrate.lock"
}
