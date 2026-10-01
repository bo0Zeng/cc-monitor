#!/usr/bin/env bash
# cc-bus-lib.sh — 路由管线的【唯一实现】,被 cc-send(兜底路径)与 cc-busd(守护进程)source。
# 约定:route_* 阀门返回 0=放行,返回 1=拦截(并已 route_log 记原因)。
# 本文件只提供函数、不自执行。BUS 由调用方设好(或此处按 CC_BUS_HOME 兜底)。
#
# 管线顺序(route_process):policy → rate → loop → dedup → deliver → nudge → log
# 阀门归属:F02=policy/nudge/deliver/log(+rate/loop/dedup 占位放行);F03 填 rate/loop/dedup。
#
# config 键(~/.cc-bus/config,sourceable KEY=VAL,全部有默认):
#   CCBUS_NUDGE_DEBOUNCE  同一收件人两次敲门最小间隔秒(默 2)         [F02]
#   CCBUS_POLICY_MODE     off=全放行 / on=按 policy.tsv 强制 ACL(默 off) [F02]
#   （CCBUS_TTL / MAX_REVISIT / RATE_* / DEDUP_WINDOW 由 F03 追加）
# policy.tsv 行:  <from-glob>\t<允许的 to-glob,逗号分隔>   (# 开头为注释)

: "${BUS:=${CC_BUS_HOME:-$HOME/.cc-bus}}"

# ── 三个适配面(设计):本文件是**通用层**,不认识 tmux / flock ─────────────
# 缝切在 shell 里、**不搬进后端**(条 55)。装不齐就地 return 13(fail-closed):
# 少一个函数而继续跑,后果是投递路径上某一步静默变成 no-op —— 那正是本仓在治的病。
# shellcheck source=cc-bus-adapt.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/cc-bus-adapt.sh"
ccbus_adapt_load || return 13

# ── kinds 表(设计):信封的 `kind` → 一组行为 ──────────────────────────────
#
# cc-bus 的功能只有「把一段文本注入 agent」这一件;`kind` 决定的是**怎么注入**:
#   拦停(Stop 钩子要不要把这条喂回去、拦下本轮结束)· 敲门(要不要往对方屏幕打字)·
#   过哪几道阀门 · 两个模板(T1 喂给 Claude 的 / T2 打进屏幕的)。
# 表住 `~/.cc-bus/kinds.tsv`(用户可改);没有 ⇒ 用 skill 自带的 `examples/kinds.tsv`
# (部署时随包落盘、带注释);两份都没有 ⇒ 只有内置的 `msg` 一行 = 今天的行为,零回归。
#
# 🔴 **敲门模板(T2)里不许放正文 `{body}`**:T2 走 `send-keys` ＋ Enter,
#    正文里的换行会被当成回车 ⇒ 把消息内容当成对方的输入执行。三道闸,**任何一道单独都够**:
#    ① 装表时逐行判(`kinds_check_row`),含 `{body}` 的行整行作废,cc-send 当场 rc=2;
#    ② 渲染 T2 的函数**根本不收正文**(`kinds_render_nudge` 的参数表里没有它);
#    ③ 渲染结果里只要有控制字符或残留的 `{body}` 字面量就拒,退回内置 T2。
#    ⚠ 买不到:用户**自己**在 T2 里写一句祈使句 —— 那是他的 prompt,cc-bus 不替他审措辞。
#
# 🔴 **阀门① ACL 与 ② 限流不许按 kind 关**:kind 是**发信方**自己选的,
#    能靠选 kind 绕开 ACL 就等于没有 ACL。⇒ 装表时缺这两项的行作废(①),
#    而且 `route_process` 里那两道门**结构上就不看 kind**(②,无条件跑)。
#    按 kind 可关的只有 ③ 去重与 ④ 灭环(保活文本按定义重复 ⇒ 必须能关去重,§3bis)。
CCBUS_KIND_DEFAULT="msg"
# 内置的 msg 两段文本 —— **与改造前硬编码在 stop-hook / 本文件里的两句逐字节相同**,
# 判据把它与 `examples/kinds.tsv` 那一行对拍(两份表达同一件事,漂了就红)。
CCBUS_KIND_BUILTIN_T1='你收到新的 cc-bus 消息,请先处理完再结束本轮:\n{body}'
CCBUS_KIND_BUILTIN_T2='🔔 cc-bus: 你有来自 {from} 的新消息,运行 cc-recv 读取并按内容处理'
CCBUS_KIND_VALVES_KNOWN="acl rate dedup loop"
CCBUS_KIND_VALVES_REQUIRED="acl rate"
CCBUS_KIND_T1_VARS="from to ts kind count body"
CCBUS_KIND_T2_VARS="from to ts kind"          # ⚠ 没有 body,也没有 count(敲门时不知道几条)

# 生效的那份表在哪。没有 ⇒ rc=1(调用方落内置 msg)。
kinds_file() {
  if [ -f "$BUS/kinds.tsv" ]; then printf '%s' "$BUS/kinds.tsv"; return 0; fi
  local shipped
  shipped="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." 2>/dev/null && pwd)/examples/kinds.tsv"
  if [ -f "$shipped" ]; then printf '%s' "$shipped"; return 0; fi
  return 1
}

# 模板里的 `{变量}` 是不是都在允许表里。不认识的那个打到 stdout,rc=1。
_kinds_vars_ok() {
  local rest="$1" allow=" $2 " v
  while [[ "$rest" =~ \{([A-Za-z_]+)\}(.*) ]]; do
    v="${BASH_REMATCH[1]}"; rest="${BASH_REMATCH[2]}"
    case "$allow" in *" $v "*) ;; *) printf '%s' "$v"; return 1 ;; esac
  done
  return 0
}

# 判一行。合法 ⇒ rc=0;不合法 ⇒ 理由打到 stdout,rc=1。**纯判定,零 I/O。**
kinds_check_row() {
  local kind="$1" block="$2" nudge="$3" valves="$4" t1="$5" t2="$6" v bad got=" "
  [[ "$kind" =~ ^[a-z0-9_-]{1,32}$ ]] || { printf 'kind 名 %q 非法(只许 [a-z0-9_-],1–32 个字符)' "$kind"; return 1; }
  case "$block" in yes|no) ;; *) printf '%s: 拦停列要 yes/no,给的是 %q' "$kind" "$block"; return 1 ;; esac
  case "$nudge" in yes|no) ;; *) printf '%s: 敲门列要 yes/no,给的是 %q' "$kind" "$nudge"; return 1 ;; esac
  local parts; IFS=',' read -ra parts <<< "$valves"
  for v in "${parts[@]}"; do
    case " $CCBUS_KIND_VALVES_KNOWN " in
      *" $v "*) got="$got$v " ;;
      *) printf '%s: 不认识的阀门 %q(只有 %s)' "$kind" "$v" "$CCBUS_KIND_VALVES_KNOWN"; return 1 ;;
    esac
  done
  for v in $CCBUS_KIND_VALVES_REQUIRED; do
    case "$got" in
      *" $v "*) ;;
      *) printf '%s: 阀门列缺 %s —— ACL 与限流不许按 kind 关(kind 是发信方自己选的)' "$kind" "$v"; return 1 ;;
    esac
  done
  if [ "$block" = yes ] && [[ "$t1" != *"{body}"* ]]; then
    printf '%s: 拦停=yes 的注入模板必须含 {body} —— 否则消息根本没喂回去' "$kind"; return 1
  fi
  bad=$(_kinds_vars_ok "$t1" "$CCBUS_KIND_T1_VARS") || { printf '%s: 注入模板里有不认识的变量 {%s}' "$kind" "$bad"; return 1; }
  # 🔴 安全项:敲门模板不许放正文
  if [[ "$t2" == *"{body}"* ]]; then
    printf '%s: 🔴 敲门模板里不许放 {body} —— 它走 send-keys,正文里的换行会被当成回车执行' "$kind"; return 1
  fi
  if [[ "$t2" == *[[:cntrl:]]* ]]; then
    printf '%s: 🔴 敲门模板里有控制字符 —— 打进屏幕就是按键' "$kind"; return 1
  fi
  bad=$(_kinds_vars_ok "$t2" "$CCBUS_KIND_T2_VARS") || { printf '%s: 敲门模板里有不认识(或不许用)的变量 {%s}' "$kind" "$bad"; return 1; }
  if [ "$nudge" = yes ] && { [ -z "$t2" ] || [ "$t2" = "-" ]; }; then
    printf '%s: 敲门=yes 却没给敲门模板(写 - 表示不敲)' "$kind"; return 1
  fi
  return 0
}

# 查一个 kind。rc=0 ⇒ 设好 KIND_NAME/BLOCK/NUDGE/VALVES/T1/T2/SRC;
# rc=1 ⇒ 表里没有它;rc=2 ⇒ 有但那一行不合法。两种失败的理由都在 KIND_WHY。
# ⚠ `msg` 永远查得到:表里没有 msg 那一行 ⇒ 内置(零回归的那条底)。
# ⚠ 表里同名多行 ⇒ **第一行赢**(与 policy.tsv「首个匹配生效」同一条)。
kinds_lookup() {
  local want="$1" f="" row n
  KIND_WHY=""
  if f=$(kinds_file); then
    # 剥行尾 \r:Windows 上编辑过的表是 CRLF,而**敲门模板尾巴上的 \r 打进屏幕就是一个回车**
    row=$(awk -F'\t' -v k="$want" '{sub(/\r$/,"")} /^[[:space:]]*#/ {next} $1==k {print; exit}' "$f" 2>/dev/null || true)
    if [ -n "$row" ]; then
      local F; IFS=$'\t' read -ra F <<< "$row"
      n=${#F[@]}
      if [ "$n" -ne 6 ]; then
        KIND_WHY="$want: 那一行有 $n 列,要 6 列(kind 拦停 敲门 阀门 注入模板 敲门模板;空的写 -;列间是真 TAB)[$f]"
        return 2
      fi
      if ! KIND_WHY=$(kinds_check_row "${F[@]}"); then KIND_WHY="$KIND_WHY [$f]"; return 2; fi
      KIND_NAME="${F[0]}"; KIND_BLOCK="${F[1]}"; KIND_NUDGE="${F[2]}"; KIND_VALVES="${F[3]}"
      KIND_T1="${F[4]}"; KIND_T2="${F[5]}"; KIND_SRC="$f"
      return 0
    fi
  fi
  if [ "$want" = "$CCBUS_KIND_DEFAULT" ]; then
    KIND_NAME=msg; KIND_BLOCK=yes; KIND_NUDGE=yes; KIND_VALVES="acl,rate,dedup,loop"
    KIND_T1="$CCBUS_KIND_BUILTIN_T1"; KIND_T2="$CCBUS_KIND_BUILTIN_T2"; KIND_SRC=builtin
    return 0
  fi
  KIND_WHY="不认识的 kind '$want'(${f:-没有 kinds.tsv,只有内置的 msg})"
  return 1
}

# 查不到 / 不合法 ⇒ 落回 msg,并在 bus.log 里说清楚(**不静默**)。用于**已经入队**的信封:
# 表是入队之后才改的 —— 那时发信方已经走了,拒掉就是静默丢;按 msg 投是最不意外的去处。
kinds_lookup_or_msg() {
  kinds_lookup "$1" && return 0
  route_log "KIND fallback '$1'→msg($KIND_WHY)"
  kinds_lookup "$CCBUS_KIND_DEFAULT"
}

kinds_has_valve() { case ",$KIND_VALVES," in *",$1,"*) return 0 ;; *) return 1 ;; esac; }

# T1(注入模板)渲染到 <out>。正文走 `--rawfile`,**不走 argv**(160KB 那件事故)。
# `\n` 只在**模板**里展开成换行;`{body}` 最后填 ⇒ 正文里碰巧有 `{from}` 也不会被替换。
kinds_render_inject() {   # <模板> <out> <正文文件> <from> <to> <kind> <count>
  jq -jn --arg t "$1" --rawfile body "$3" --arg from "$4" --arg to "$5" \
         --arg kind "$6" --arg count "$7" --arg ts "$(store_now)" \
    '$t | gsub("\\\\n"; "\n")
        | split("{from}") | join($from) | split("{to}") | join($to)
        | split("{ts}") | join($ts) | split("{kind}") | join($kind)
        | split("{count}") | join($count) | split("{body}") | join($body)' > "$2"
}

# T2(敲门模板)渲染到 stdout。🔴 **参数表里没有正文** —— 这是三道闸里的第二道:
# 就算有人绕过装表那道判,这里也没有东西可以填进 `{body}`。
# 渲染结果里有控制字符(`{ts}` 来自信封,信封谁都能写)或残留的 `{body}` ⇒ rc=1,调用方退回内置。
kinds_render_nudge() {    # <模板> <from> <to> <kind> <ts>
  local out
  out=$(jq -jn --arg t "$1" --arg from "$2" --arg to "$3" --arg kind "$4" --arg ts "$5" \
    '$t | split("{from}") | join($from) | split("{to}") | join($to)
        | split("{ts}") | join($ts) | split("{kind}") | join($kind)') || return 1
  case "$out" in ''|*[[:cntrl:]]*|*'{body}'*) return 1 ;; esac
  printf '%s' "$out"
}

# Stop 钩子那一批里**该用哪一行拦停**。入参:cc-peek `--kinds-file` 写出的那一行(`kind:from kind:from …`)。
# 出:`<kind>\t<from>\t<条数>`;这一批**没有**要拦停的 kind ⇒ rc=1(调用方放行、不推进)。
# 优先级 = 表里的行序(越靠前越优先),内置 msg 排最后;同级取这一批里先出现的那条。
# ⚠ 一条都没解析出来(全是坏行)⇒ 按 msg 拦:那一批里至少有「跳过 N 条读不懂的」那句话要喂回去,
#   与改造前逐字同一个行为(否则毒丸会永远卡在收件箱里、每轮都被重读)。
kinds_pick_block() {
  local meta ent k fr best="" bestfrom="" bestrank=100000 rank n=0 f order=""
  meta=$(cat "$1" 2>/dev/null || true)
  if f=$(kinds_file); then
    order=$(awk -F'\t' '{sub(/\r$/,"")} /^[[:space:]]*#/ {next} NF {printf "%s ", $1}' "$f" 2>/dev/null || true)
  fi
  for ent in $meta; do
    n=$((n+1))
    k="${ent%%:*}"; fr="${ent#*:}"
    kinds_lookup "$k" || { kinds_lookup "$CCBUS_KIND_DEFAULT"; k=msg; }
    [ "$KIND_BLOCK" = yes ] || continue
    rank=$(printf '%s' "$order" | awk -v k="$k" '{for(i=1;i<=NF;i++) if($i==k){print i; exit}}')
    [ -n "$rank" ] || rank=99999
    if [ "$rank" -lt "$bestrank" ]; then best="$k"; bestfrom="$fr"; bestrank="$rank"; fi
  done
  if [ "$n" -eq 0 ]; then printf '%s\t%s\t%s' msg "?" 0; return 0; fi
  [ -n "$best" ] || return 1
  printf '%s\t%s\t%s' "$best" "$bestfrom" "$n"
}

# 两阶段读口的锚(cc-peek 发、cc-commit 校验,**两边必须是同一份实现**,否则永远比不中)。
# 取第 <行号> 行现算 cksum,形如 `<校验和>-<字节数>` —— 与下面 `_dedup_hash` 同一招,零新依赖。
twophase_anchor() { sed -n "${2}p" "$1" | cksum | awk '{print $1"-"$2}'; }

route_load_config() {
  # shellcheck disable=SC1091
  [ -f "$BUS/config" ] && . "$BUS/config" || true
  : "${CCBUS_NUDGE_DEBOUNCE:=2}"        # F02
  : "${CCBUS_POLICY_MODE:=off}"         # F02
  : "${CCBUS_TTL:=0}"                   # F03,0=关
  : "${CCBUS_MAX_REVISIT:=0}"           # F03,0=关
  : "${CCBUS_RATE_PAIR:=0}"             # F03,0=关
  : "${CCBUS_RATE_GLOBAL:=0}"           # F03,0=关
  : "${CCBUS_RATE_WINDOW:=60}"          # F03
  : "${CCBUS_DEDUP_WINDOW:=0}"          # F03,0=关
  # 数字键防呆:config typo(非数字)回落安全值,避免 [ -ge ] 报错刷屏 + 静默关阀
  [[ "$CCBUS_NUDGE_DEBOUNCE" =~ ^[0-9]+$ ]] || CCBUS_NUDGE_DEBOUNCE=2
  [[ "$CCBUS_RATE_WINDOW"    =~ ^[0-9]+$ ]] || CCBUS_RATE_WINDOW=60
  local _k
  for _k in CCBUS_TTL CCBUS_MAX_REVISIT CCBUS_RATE_PAIR CCBUS_RATE_GLOBAL CCBUS_DEDUP_WINDOW; do
    [[ "${!_k}" =~ ^[0-9]+$ ]] || printf -v "$_k" '%s' 0
  done
}

route_log() { printf '[%s] %s\n' "$(store_now)" "$*" >> "$BUS/log/bus.log" 2>/dev/null || true; }

# 守护进程是否在跑:pidfile + kill-0 + cmdline 校验。
# 【不能用 flock 试锁判活】——那会让并发探测者自己短暂持锁、彼此 flock -n 失败而互相误判"在跑",
# 进而入队却无人处理→静默丢消息(多 cc-send 并发是常态)。改用只读探测:
#   pidfile 存在 且 PID 活(kill -0,不获取任何锁,无自竞态)且 /proc/<pid>/cmdline 确是 cc-busd(防 PID 复用)。
# 崩溃后 pidfile 残留:PID 已死→kill -0 失败→正确判"未运行";PID 被复用→cmdline 不含 cc-busd→仍判"未运行"。
backend_running() {
  local pf="$BUS/cc-busd.pid" pid
  [ -f "$pf" ] || return 1
  pid=$(cat "$pf" 2>/dev/null) || return 1
  [[ "$pid" =~ ^[0-9]+$ ]] || return 1
  kill -0 "$pid" 2>/dev/null || return 1
  # 按 NUL 切开 argv,要求某个 arg 就是 cc-busd(或以 /cc-busd 结尾),而非 cmdline 含该子串即可——
  # 这样被复用的 PID 若在跑如 `tail .../cc-busd.log` 不会被误判成守护进程。
  tr '\0' '\n' < "/proc/$pid/cmdline" 2>/dev/null | grep -qE '(^|/)cc-busd$'
}

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
# 一次瞬时 deliver 失败 + 重排队,绝不能留下计数增量、进而在重投时误 THROTTLE 把消息静默丢掉。
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

# 阀门3:近窗去重(同 from+text 在 DEDUP_WINDOW 秒内重复则拒)。0=关。
# 同限流拆法:CHECK 只读(投递前);COMMIT 在 deliver 成功后才记 hash——
# 这样 deliver 失败 + 重排队不会留下 hash、进而在重投时误 COALESCE(丢弃)。尽力而为:极端并发下可能漏一条重复,对合并可接受。
_dedup_hash() {
  local line="$1" from text
  from=$(printf '%s' "$line" | jq -r '.from // ""')
  text=$(printf '%s' "$line" | jq -r '.text // ""')
  printf '%s' "$from|$text" | cksum | awk '{print $1"-"$2}'
}
route_dedup_check() {
  local line="$1" dw="${CCBUS_DEDUP_WINDOW:-0}"
  [ "$dw" = 0 ] && return 0
  local from to h now df rc
  from=$(printf '%s' "$line" | jq -r '.from // ""')
  to=$(printf '%s' "$line" | jq -r '.to // ""')
  h=$(_dedup_hash "$line")
  now=$(date +%s)
  df="$BUS/state/dedup-$to"
  mkdir -p "$BUS/state"
  (
    flock 6
    dup=0
    if [ -f "$df" ]; then
      while read -r eh ets; do
        [[ "$ets" =~ ^[0-9]+$ ]] || continue
        [ $(( now - ets )) -lt "$dw" ] || continue      # 过期,忽略
        [ "$eh" = "$h" ] && { dup=1; break; }
      done < "$df"
    fi
    [ "$dup" = 0 ]                                       # 子壳退出码=非重复(只读)
  ) 6>>"$df.lock"
  rc=$?
  [ "$rc" = 0 ] && return 0
  route_log "COALESCE dup $from->$to"; return 1
}
# 记录 hash(+ 剪过期)。仅在 deliver 成功后调用。
route_dedup_commit() {
  local line="$1" dw="${CCBUS_DEDUP_WINDOW:-0}"
  [ "$dw" = 0 ] && return 0
  local to h now df
  to=$(printf '%s' "$line" | jq -r '.to // ""')
  h=$(_dedup_hash "$line")
  now=$(date +%s)
  df="$BUS/state/dedup-$to"
  mkdir -p "$BUS/state"
  (
    flock 6
    tmp="$df.tmp.$$"; have=0; : > "$tmp"
    if [ -f "$df" ]; then
      while read -r eh ets; do
        [[ "$ets" =~ ^[0-9]+$ ]] || continue
        [ $(( now - ets )) -lt "$dw" ] || continue      # 过期,剪掉
        printf '%s %s\n' "$eh" "$ets" >> "$tmp"
        [ "$eh" = "$h" ] && have=1
      done < "$df"
    fi
    [ "$have" = 0 ] && printf '%s %s\n' "$h" "$now" >> "$tmp"
    mv "$tmp" "$df" 2>/dev/null || true
  ) 6>>"$df.lock"
}

# 阀门4:因果链灭环(hops>TTL 或 to 在 trace 出现≥MAX_REVISIT)。TTL/MAX_REVISIT 均 0=关。
# 诚实说明:此阀门无法区分"正经长对话"与"失控回环",仅作可选粗兜底;反 storm 首选限流。
route_loop_check() {
  local line="$1" ttl="${CCBUS_TTL:-0}" mr="${CCBUS_MAX_REVISIT:-0}"
  { [ "$ttl" = 0 ] && [ "$mr" = 0 ]; } && return 0
  local hops to trace t cnt parts
  hops=$(printf '%s' "$line" | jq -r '.hops // 0'); [[ "$hops" =~ ^[0-9]+$ ]] || hops=0
  if [ "$ttl" != 0 ] && [ "$hops" -gt "$ttl" ]; then route_log "DROP ttl hops=$hops"; return 1; fi
  if [ "$mr" != 0 ]; then
    to=$(printf '%s' "$line" | jq -r '.to // ""')
    trace=$(printf '%s' "$line" | jq -r '.trace // ""')
    cnt=0; parts=()
    IFS=',' read -ra parts <<< "$trace"      # read -ra 不做路径名展开(trace 可能来自不可信信封)
    for t in "${parts[@]}"; do [ "$t" = "$to" ] && cnt=$((cnt+1)); done
    if [ "$cnt" -ge "$mr" ]; then route_log "DROP loop to=$to revisit=$cnt"; return 1; fi
  fi
  return 0
}

# 投递:写收件人 inbox(源头),加锁。返回非零=投递失败(调用方应退回重试)。
route_deliver() {
  local to="$1" line="$2"
  local inbox="$BUS/inbox/$to.jsonl"      # 独立行:同一 local 里引用刚赋的 $to 会取到旧值
  mkdir -p "$BUS/inbox" || return 1
  ( store_lock_exclusive 9; store_append_line "$inbox" "$line" ) 9>>"$inbox.lock" || return 1
  return 0
}

# 敲门(去抖):同一收件人 DEBOUNCE 秒内只敲一次;send-keys 到其 pane。
# 敲什么由 kind 的 T2 定(`kinds_render_nudge` —— 它**不收正文**,见 kinds 那一节的三道闸)。
route_nudge() {
  local to="$1" from="$2" t2="${3:-$CCBUS_KIND_BUILTIN_T2}" kind="${4:-msg}" ts="${5:-}"
  local nf="$BUS/state/nudge-$to" now last target   # 独立行:$to 已赋值后再拼路径
  now=$(date +%s)
  mkdir -p "$BUS/state"
  (
    flock 8
    last=$(cat "$nf" 2>/dev/null || echo 0); [[ "$last" =~ ^[0-9]+$ ]] || last=0
    if [ $(( now - last )) -ge "${CCBUS_NUDGE_DEBOUNCE:-2}" ]; then
      target=$(awk -F'\t' -v id="$to" '$1==id{t=$2} END{print t}' "$BUS/agents.tsv" 2>/dev/null || true)
      # ★★ **敲门前先核这个 pane 还是不是它**〔08-13 实测误投〕
      # 地址是名字型的(`proj_cc:0.0`)而名字会被重用 ⇒ agent 退出后同名会话被别人占着时,
      # 敲门文字(带 Enter)会**打进陌生占用者的屏幕**。⇒ 比对登记时记下的 pane 根进程 pid。
      # ⚠ 第 4 列为空(老表/不在 tmux 里登记的)⇒ **按老行为敲**,不制造假跳过。
      want_pid=$(awk -F'\t' -v id="$to" '$1==id{p=$4} END{print p}' "$BUS/agents.tsv" 2>/dev/null || true)
      if [ -n "$target" ] && [ -n "$want_pid" ]; then
        have_pid=$(os_pane_fingerprint "$target" || true)
        if [ "$have_pid" != "$want_pid" ]; then
          route_log "NUDGE stale $to target=$target 登记时 pid=$want_pid 现在 pid=${have_pid:-<没有这个 pane>} —— 不敲,免得打进别人的屏幕"
          target=""
        fi
      fi
      # T2 渲染不出来(控制字符 / 残留 {body})⇒ 退回内置那一句,并**说出来**
      knock=$(kinds_render_nudge "$t2" "$from" "$to" "$kind" "$ts") || {
        route_log "NUDGE tpl-refused kind=$kind —— 敲门模板渲染出了控制字符或 {body},改敲内置那一句"
        knock=$(kinds_render_nudge "$CCBUS_KIND_BUILTIN_T2" "$from" "$to" "$kind" "")
      }
      if [ -n "$target" ] && os_send_keys "$target" "$knock"; then
        # claude 的输入要**另打一个 Enter** 才算提交 —— 这一位由 agent 词典自陈(§3.2b),
        # 不是所有 agent 都这样;`sleep 0.3` 刻意留在本文件(它是本仓登记在册的周期唤醒)。
        if agent_submit_enter; then
          sleep 0.3
          os_send_keys "$target" Enter || true
        fi
        echo "$now" > "$nf"
      fi
    else
      route_log "NUDGE skip $to"
    fi
  ) 8>>"$nf.lock"
}

# 管线入口。返回码:0=已投递;10=被阀门拦截/坏信封(已消费,不投递不重试);1=投递出错(应退回重试)。
route_process() {
  local line="$1" from to
  from=$(printf '%s' "$line" | jq -r '.from // "?"' 2>/dev/null || echo "?")
  to=$(printf '%s' "$line" | jq -r '.to // "?"' 2>/dev/null || echo "?")
  # 路径穿越纵深防御:to/from 都会拼进文件路径(库是唯一实现,backend 会读任意实例写的信封)
  case "$to"   in *[!A-Za-z0-9_-]*|'') route_log "DROP badname to=$to"; return 10;; esac
  case "$from" in *[!A-Za-z0-9_-]*|'') route_log "DROP badname from=$from"; return 10;; esac
  # 这里的门全是【只读】(无副作用)。限流/去重的状态仅在 deliver 成功后(见下)才提交,
  # 于是一次瞬时 deliver 失败 + 重排队会干净地重跑各门,而不会被 throttle/coalesce 掉后静默丢失。
  # kind:缺省 msg(老信封零回归)。查不到 / 那一行不合法 ⇒ 按 msg 投并记一行(已入队的不许静默丢)。
  local kind ts; kind=$(printf '%s' "$line" | jq -r '.kind // "msg"' 2>/dev/null || echo msg)
  ts=$(printf '%s' "$line" | jq -r '.ts // ""' 2>/dev/null || true)
  kinds_lookup_or_msg "$kind"; kind="$KIND_NAME"
  # 🔴 阀门① ACL 与 ② 限流**不看 kind**:这两行无条件跑(kind 是发信方自己选的,不许靠它绕开)。
  route_policy_check "$from" "$to" || return 10
  route_rate_check   "$from" "$to" || return 10
  # ③ 去重 ④ 灭环按 kind 可关(保活文本按定义重复 ⇒ 去重必须能关)
  if kinds_has_valve loop;  then route_loop_check  "$line" || return 10; fi
  if kinds_has_valve dedup; then route_dedup_check "$line" || return 10; fi
  # 能力降级:按收件方 agent 词典的三位能力,把想要的行为降到做得到的那一档。
  #   拦停 ⇒ 敲门 ⇒ 只入收件箱 ⇒ **投递前就拒**(别让消息烂在收件箱里)。每降一级 bus.log 里一行。
  # ⚠ 买不到:「收件方是哪种 agent」—— 按**本进程**装载的词典算(今天只有 claude 一本)。
  local want=inbox got nudge_on=no
  [ "$KIND_NUDGE" = yes ] && want=nudge
  [ "$KIND_BLOCK" = yes ] && want=block
  got=$(ccbus_degrade "$want")
  if [ "$got" = "$CCBUS_DEGRADE_SINK" ]; then
    route_log "REJECT kind=$kind $from->$to —— 收件方一种投递语义都做不到(降级链走到底),投递前就拒"
    return 10
  fi
  case "$got" in
    nudge) nudge_on=yes ;;                                   # 本来就要敲,或拦停降成了敲门
    block) if [ "$KIND_NUDGE" = yes ] && [ "$(ccbus_degrade nudge)" = nudge ]; then nudge_on=yes; fi ;;
  esac
  local t2="$KIND_T2"
  { [ -z "$t2" ] || [ "$t2" = "-" ]; } && t2="$CCBUS_KIND_BUILTIN_T2"   # 拦停降成敲门、而这一行本来不敲
  # msg-id 幂等:同 id 已在收件人 inbox → 跳过(reaper 退回重投的双投防护;正常唯一 id 永不命中)。
  # 只扫 inbox 尾部(重投的必是近期消息),避免 inbox 只增导致每次投递 O(n) 全量扫。
  local mid; mid=$(printf '%s' "$line" | jq -r '.id // ""')
  if [ -n "$mid" ] && [ -f "$BUS/inbox/$to.jsonl" ] \
     && tail -n 500 "$BUS/inbox/$to.jsonl" 2>/dev/null | grep -qF "\"id\":\"$mid\""; then
    route_log "DROP dupid $mid"; return 10
  fi
  route_deliver "$to" "$line" || { route_log "ERROR deliver $from->$to"; return 1; }
  # 已投递 → 现在才提交有副作用的门(安全:上面 deliver 失败绝不会走到这)。
  route_rate_commit  "$from" "$to"
  if kinds_has_valve dedup; then route_dedup_commit "$line"; fi
  if [ "$nudge_on" = yes ]; then
    route_nudge "$to" "$from" "$t2" "$kind" "$ts" || true
  else
    route_log "NUDGE off $to kind=$kind"
  fi
  if [ "$kind" = msg ]; then route_log "DELIVER $from->$to"; else route_log "DELIVER $from->$to kind=$kind"; fi
  return 0
}
