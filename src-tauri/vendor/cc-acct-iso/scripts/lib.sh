#!/usr/bin/env bash
# cc-acct-iso 库:配置加载 / manifest 读写 / plan-apply 两段式 / 备份回退 / 路径校验。
# 被主脚本 source,不可独立执行。
# 安全约定:本文件只搬/链/stat 凭据文件,**从不读取其内容**。

# ---------- 输出 ----------
_c_reset=''; _c_red=''; _c_grn=''; _c_ylw=''; _c_dim=''; _c_bold=''
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  _c_reset=$'\033[0m'; _c_red=$'\033[31m'; _c_grn=$'\033[32m'
  _c_ylw=$'\033[33m'; _c_dim=$'\033[2m'; _c_bold=$'\033[1m'
fi
die()  { printf '%serror:%s %s\n' "$_c_red" "$_c_reset" "$*" >&2; exit 1; }
warn() { printf '%swarn:%s %s\n'  "$_c_ylw" "$_c_reset" "$*" >&2; }
info() { printf '%s\n' "$*"; }
ok()   { printf '  %s✔%s %s\n' "$_c_grn" "$_c_reset" "$*"; }
bad()  { printf '  %s✘%s %s\n' "$_c_red" "$_c_reset" "$*"; }
skip() { printf '  %s-%s %s\n'  "$_c_dim" "$_c_reset" "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }

# ---------- 路径 / 名字校验 ----------
# shell 危险字符:manifest 里的 configDir 会被消费方(cc-monitor)拼进
# `export CLAUDE_CONFIG_DIR='<dir>'`,单引号一旦被闭合就是命令注入。这里从源头掐掉。
# 仍允许空格与非 ASCII(常见且在单引号内无害)——消费端**依旧要按不可信字符串处理**。
path_shell_safe() {
  case "$1" in
    *"'"*|*'"'*|*\\*|*'`'*|*'$'*|*';'*|*'|'*|*'&'*|*'<'*|*'>'*|*'*'*|*'?'*|*'('*|*')'*|*'!'*)
      return 1 ;;
  esac
  # 视觉欺骗类 Unicode(双向覆盖/零宽/异常空白/行段分隔):单引号内无注入,但能在
  # cc-monitor 的 UI 里伪造同形/反向的账号名与路径。与 daemon 侧 is_deceptive_char
  # 对齐(否则写侧放行、读侧丢弃 → 账号在 monitor 里凭空消失)。用 UTF-8 字节模式匹配,
  # LC_ALL=C 保证按字节;grep -P 不可用时静默跳过(daemon 侧兜底拒绝,fail-safe)。
  if printf '%s' "$1" | LC_ALL=C grep -qP \
    '\xc2[\x85\xa0]|\xe2\x80[\x8b-\x8f\xa8\xa9\xaa-\xae]|\xe2\x81[\xa6-\xa9]|\xef\xbb\xbf' 2>/dev/null; then
    return 1
  fi
  return 0
}

path_sanitize() {  # 绝对路径 + 去重复/尾斜杠 + 拒 .. / 控制字符 / shell 危险字符
  local p="${1-}"
  [ -n "$p" ] || die "空路径"
  case "$p" in
    /*) ;;
    *)  die "需要绝对路径:$p" ;;
  esac
  case "$p" in
    *$'\n'*|*$'\t'*|*$'\r'*) die "路径含控制字符:$p" ;;
  esac
  case "$p" in
    */../*|*/..) die "路径含 '..':$p" ;;
  esac
  path_shell_safe "$p" || die "路径含 shell 危险字符(' \" \\ \` \$ ; | & < > * ? ( ) !):$p
  (manifest 里的 configDir 会被 cc-monitor 拼进 shell,故一律拒绝;请换一个干净的目录)"
  while [ "$p" != "${p//\/\//\/}" ]; do p="${p//\/\//\/}"; done
  [ "$p" = "/" ] || p="${p%/}"
  printf '%s' "$p"
}

name_check() {  # 账号名:[A-Za-z0-9_-],不以 - 或 _ 开头,≤32
  local n="${1-}"
  case "$n" in
    "")            die "账号名不能为空" ;;
    *[!A-Za-z0-9_-]*) die "账号名只允许 [A-Za-z0-9_-]:'$n'" ;;
    -*|_*)         die "账号名不能以 '-' 或 '_' 开头:'$n'" ;;
  esac
  [ "${#n}" -le 32 ] || die "账号名过长(>32):'$n'"
}

ts_check() {  # 备份时间戳:只允许 [0-9A-Za-z._-],不含斜杠(防目录穿越)
  local t="${1-}"
  case "$t" in
    ""|*[!0-9A-Za-z._-]*) die "非法的备份时间戳(只允许 [0-9A-Za-z._-]):'$t'" ;;
    *..*)                 die "非法的备份时间戳(含 '..'):'$t'" ;;
  esac
}

email_check() {
  local e="${1-}"
  [ "${#e}" -le 254 ] || die "邮箱过长(>254)"
  case "$e" in
    *[[:cntrl:]]*) die "邮箱含控制字符" ;;
    *'"'*|*\\*)   die "邮箱含引号/反斜杠" ;;
  esac
}

is_under() {  # is_under <child> <parent>:child 是否在 parent 之内(或相等)
  local c="$1" p="$2"
  [ "$c" = "$p" ] && return 0
  case "$c/" in "$p"/*) return 0 ;; esac
  return 1
}

real_of() {  # 尽量解析真实路径(不要求存在);没有 realpath 就原样返回
  if have realpath; then realpath -m -- "$1" 2>/dev/null || printf '%s' "$1"
  else printf '%s' "$1"; fi
}

# ---------- 配置 ----------
CFG_FILE=""
cfg_load() {
  local f="${CC_ACCT_ISO_CONFIG:-$HOME/.cc-acct-iso/config}"
  if [ -f "$f" ]; then
    if [ -O "$f" ]; then
      # shellcheck disable=SC1090
      . "$f" || die "读配置失败:$f"
      CFG_FILE="$f"
    else
      warn "配置文件不属于当前用户,已忽略:$f"
    fi
  fi
  SHARED_STORE="${SHARED_STORE:-$HOME/.claude}"
  ACCTS_DIR="${ACCTS_DIR:-$HOME/.claude-accts}"
  ISOLATE_SET="${ISOLATE_SET:-.credentials.json .claude.json backups policy-limits.json stats-cache.json}"
  SHARE_SET="${SHARE_SET:-@auto}"
  SHARE_EXCLUDE="${SHARE_EXCLUDE:-accounts *.bak *.bak-*}"
  LEGACY_HOME_ITEMS="${LEGACY_HOME_ITEMS:-.claude.json}"
  LEGACY_HOME_DIR="${LEGACY_HOME_DIR:-$HOME}"
  LAUNCHER="${LAUNCHER:-claude}"
}

cfg_finalize() {  # CLI flag 覆盖之后调用:规范化 + 一致性校验
  SHARED_STORE="$(path_sanitize "$SHARED_STORE")"
  ACCTS_DIR="$(path_sanitize "$ACCTS_DIR")"
  LEGACY_HOME_DIR="$(path_sanitize "$LEGACY_HOME_DIR")"
  [ -d "$SHARED_STORE" ] || die "共享库不存在:$SHARED_STORE(用 --shared-store 或配置文件指定)"
  local rs ra
  rs="$(real_of "$SHARED_STORE")"; ra="$(real_of "$ACCTS_DIR")"
  is_under "$ra" "$rs" && die "ACCTS_DIR 不能位于 SHARED_STORE 内部($ACCTS_DIR ⊂ $SHARED_STORE)"
  is_under "$rs" "$ra" && die "SHARED_STORE 不能位于 ACCTS_DIR 内部"
  MANIFEST="$ACCTS_DIR/accounts.json"
  # 列表在此一次性切成数组:切的时候必须关掉 globbing,否则 SHARE_EXCLUDE 里的
  # *.bak 会被当作路径通配去匹配 CWD —— 而 share_items 又开了 nullglob,不匹配就整个消失。
  local had_f=0; case $- in *f*) had_f=1 ;; esac
  set -f
  # shellcheck disable=SC2206
  ISOLATE_ARR=($ISOLATE_SET)
  # shellcheck disable=SC2206
  EXCLUDE_ARR=($SHARE_EXCLUDE)
  # shellcheck disable=SC2206
  LEGACY_ARR=($LEGACY_HOME_ITEMS)
  SHARE_ARR=()
  [ "$SHARE_SET" = "@auto" ] || {
    # shellcheck disable=SC2206
    SHARE_ARR=($SHARE_SET)
  }
  [ "$had_f" = 1 ] || set +f
}

cfg_dump() {
  info "${_c_bold}配置${_c_reset}${CFG_FILE:+ (来自 $CFG_FILE)}"
  info "  SHARED_STORE      $SHARED_STORE"
  info "  ACCTS_DIR         $ACCTS_DIR"
  info "  ISOLATE_SET       $ISOLATE_SET"
  info "  SHARE_SET         $SHARE_SET"
  info "  SHARE_EXCLUDE     $SHARE_EXCLUDE"
  info "  LEGACY_HOME_ITEMS $LEGACY_HOME_ITEMS (源目录 $LEGACY_HOME_DIR)"
  info "  LAUNCHER          $LAUNCHER"
  info "  manifest          $MANIFEST"
}

# ---------- 隔离集 / 共享集 ----------
is_isolate() {
  local it
  for it in ${ISOLATE_ARR[@]+"${ISOLATE_ARR[@]}"}; do [ "$it" = "$1" ] && return 0; done
  return 1
}

is_excluded() {
  local pat
  for pat in ${EXCLUDE_ARR[@]+"${EXCLUDE_ARR[@]}"}; do
    # shellcheck disable=SC2254
    case "$1" in $pat) return 0 ;; esac
  done
  return 1
}

# 每项以 NUL 结尾输出(文件名可能含换行,不能用按行协议)。
# 读法:while IFS= read -r -d '' item; do … done < <(share_items)
share_items() {
  local it base
  if [ "${#SHARE_ARR[@]}" -gt 0 ]; then          # 显式白名单
    for base in "${SHARE_ARR[@]}"; do
      is_isolate "$base" && continue
      { [ -e "$SHARED_STORE/$base" ] || [ -L "$SHARED_STORE/$base" ]; } || continue
      printf '%s\0' "$base"
    done
    return 0
  fi
  shopt -s nullglob dotglob
  for it in "$SHARED_STORE"/*; do   # dotglob 下 bash 的 * 不含 . 与 ..
    base="${it##*/}"
    is_isolate "$base" && continue
    is_excluded "$base" && continue
    case "$base" in
      *$'\n'*|*$'\t'*) warn "共享库里有文件名含换行/制表符,已跳过(请先改名):$SHARED_STORE/$base"; continue ;;
    esac
    printf '%s\0' "$base"
  done
  shopt -u nullglob dotglob
}

isolate_src() {  # 迁移时该隔离项的源路径(不存在则空)
  local item="$1" it
  if [ -e "$SHARED_STORE/$item" ] || [ -L "$SHARED_STORE/$item" ]; then
    printf '%s' "$SHARED_STORE/$item"; return 0
  fi
  for it in ${LEGACY_ARR[@]+"${LEGACY_ARR[@]}"}; do
    if [ "$it" = "$item" ] && { [ -e "$LEGACY_HOME_DIR/$item" ] || [ -L "$LEGACY_HOME_DIR/$item" ]; }; then
      printf '%s' "$LEGACY_HOME_DIR/$item"; return 0
    fi
  done
  return 0
}

acct_mode() {  # isolated | in-place
  if [ "$1" = "$SHARED_STORE" ]; then printf 'in-place'; else printf 'isolated'; fi
}

# ---------- JSON ----------
json_esc() {
  local s="$1" out='' i ch code
  s="${s//\\/\\\\}"; s="${s//\"/\\\"}"
  s="${s//$'\n'/\\n}"; s="${s//$'\t'/\\t}"; s="${s//$'\r'/\\r}"
  case "$s" in
    *[[:cntrl:]]*)   # 其余 C0 控制符必须转成 \u00XX,否则生成非法 JSON(自锁死)
      for (( i=0; i<${#s}; i++ )); do
        ch="${s:i:1}"
        case "$ch" in
          [[:cntrl:]]) printf -v code '%02x' "'$ch"; out+="\\u00$code" ;;
          *) out+="$ch" ;;
        esac
      done
      s="$out" ;;
  esac
  printf '%s' "$s"
}

claude_json_email() {  # 从 <configDir>/.claude.json 或指定文件读 oauthAccount.emailAddress(非 token)
  local f="$1"
  [ -n "$f" ] || return 0
  [ -d "$f" ] && f="$f/.claude.json"
  [ -f "$f" ] || return 0
  if have jq; then
    jq -r '.oauthAccount.emailAddress // empty' "$f" 2>/dev/null || true
  else
    { grep -o '"emailAddress"[[:space:]]*:[[:space:]]*"[^"]*"' "$f" 2>/dev/null \
      | head -1 | sed 's/.*"\([^"]*\)"$/\1/'; } || true
  fi
}

# ---------- manifest(M2 契约 schema v1)----------
# 内存表:MF[i] = name ␟ email ␟ configDir ␟ isDefault(true|false)
# 分隔符用 US(\x1f)而非 tab:tab 属 IFS 空白,连续分隔符会被 read 合并,空 email 会串位。
MF_SEP=$'\x1f'
MF=()

manifest_load() {
  MF=()
  [ -f "$MANIFEST" ] || return 0
  local ver
  if have jq; then
    ver="$(jq -r '.version // empty' "$MANIFEST" 2>/dev/null)" || die "manifest 不是合法 JSON:$MANIFEST"
    [ -n "$ver" ] || die "manifest 缺 version 字段:$MANIFEST"
    [ "$ver" = "1" ] || die "manifest schema 版本 $ver 不受支持(本工具只认 1)"
    local line
    while IFS= read -r line; do
      [ -n "$line" ] && MF+=("$line")
    done < <(jq -r '.accounts[]? | [.name, (.email // ""), .configDir, (if .isDefault then "true" else "false" end)] | join("\u001f")' "$MANIFEST")
  else
    ver="$(_jgrep_scalar version "$(cat "$MANIFEST")")"
    [ "$ver" = "1" ] || die "manifest schema 版本 '$ver' 不受支持(本工具只认 1;装 jq 可解析非本工具生成的格式)"
    local obj n e c d found=0
    while IFS= read -r obj; do
      n="$(_jgrep_str name "$obj")"; e="$(_jgrep_str email "$obj")"
      c="$(_jgrep_str configDir "$obj")"; d="$(_jgrep_scalar isDefault "$obj")"
      [ "$d" = "true" ] || d=false
      [ -n "$n" ] && { MF+=("$n$MF_SEP$e$MF_SEP$c$MF_SEP$d"); found=1; }
    done < <(grep -o '{[^{}]*"name"[^{}]*}' "$MANIFEST")
    if [ "$found" = 0 ] && grep -q '"accounts"[[:space:]]*:[[:space:]]*\[[[:space:]]*{' "$MANIFEST"; then
      die "无 jq 时只能解析本工具生成的紧凑格式,而 $MANIFEST 已被重新排版。请装 jq。"
    fi
  fi
  # 载入的 configDir 是「后续写入目标」,必须当不可信输入校验
  local i n e c d bad_i=()
  for i in "${!MF[@]}"; do
    IFS="$MF_SEP" read -r n e c d <<<"${MF[$i]}"
    if [ -z "$c" ] || ! path_shell_safe "$c"; then bad_i+=("$i"); warn "manifest 里账号 '$n' 的 configDir 非法,已忽略该账号:$c"; continue; fi
    case "$c" in /*) ;; *) bad_i+=("$i"); warn "manifest 里账号 '$n' 的 configDir 不是绝对路径,已忽略:$c"; continue ;; esac
    case "$c" in */../*|*/..) bad_i+=("$i"); warn "manifest 里账号 '$n' 的 configDir 含 '..',已忽略:$c"; continue ;; esac
  done
  local j out=()
  for i in "${!MF[@]}"; do
    local skip_it=0
    for j in ${bad_i[@]+"${bad_i[@]}"}; do [ "$i" = "$j" ] && skip_it=1; done
    [ "$skip_it" = 0 ] && out+=("${MF[$i]}")
  done
  MF=("${out[@]+"${out[@]}"}")
}

_jgrep_str()    { { printf '%s' "$2" | grep -o "\"$1\"[[:space:]]*:[[:space:]]*\"[^\"]*\"" | head -1 | sed 's/.*"\([^"]*\)"$/\1/'; } || true; }
_jgrep_scalar() { { printf '%s' "$2" | grep -o "\"$1\"[[:space:]]*:[[:space:]]*[a-z0-9]*" | head -1 | sed 's/.*[[:space:]:]\([a-z0-9]*\)$/\1/'; } || true; }

manifest_render() {
  local rec n e c d first=1
  printf '{\n'
  printf '  "version": 1,\n'
  printf '  "updatedAt": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf '  "sharedStore": "%s",\n' "$(json_esc "$SHARED_STORE")"
  printf '  "acctsDir": "%s",\n' "$(json_esc "$ACCTS_DIR")"
  if [ "${#MF[@]}" -eq 0 ]; then
    printf '  "accounts": []\n}\n'; return 0
  fi
  printf '  "accounts": [\n'
  for rec in "${MF[@]}"; do
    IFS="$MF_SEP" read -r n e c d <<<"$rec"
    [ "$first" = 1 ] || printf ',\n'
    first=0
    printf '    { "name": "%s", "email": "%s", "configDir": "%s", "isDefault": %s, "mode": "%s" }' \
      "$(json_esc "$n")" "$(json_esc "$e")" "$(json_esc "$c")" "$d" "$(acct_mode "$c")"
  done
  printf '\n  ]\n}\n'
}

mf_index() {  # 按名字找下标,找不到返回 1
  local i n
  for i in "${!MF[@]}"; do
    IFS="$MF_SEP" read -r n _ _ _ <<<"${MF[$i]}"
    [ "$n" = "$1" ] && { printf '%s' "$i"; return 0; }
  done
  return 1
}
mf_field() {  # mf_field <idx> <1..4>
  local rec="${MF[$1]}" n e c d
  IFS="$MF_SEP" read -r n e c d <<<"$rec"
  case "$2" in 1) printf '%s' "$n";; 2) printf '%s' "$e";; 3) printf '%s' "$c";; 4) printf '%s' "$d";; esac
}
mf_add()  { MF+=("$1$MF_SEP$2$MF_SEP$3$MF_SEP$4"); }
mf_set_default() {  # 只留一个 isDefault
  local i n e c d
  for i in "${!MF[@]}"; do
    IFS="$MF_SEP" read -r n e c d <<<"${MF[$i]}"
    if [ "$n" = "$1" ]; then d=true; else d=false; fi
    MF[i]="$n$MF_SEP$e$MF_SEP$c$MF_SEP$d"
  done
}
mf_del() {
  local i out=() n
  for i in "${!MF[@]}"; do
    IFS="$MF_SEP" read -r n _ _ _ <<<"${MF[$i]}"
    [ "$n" = "$1" ] || out+=("${MF[$i]}")
  done
  MF=("${out[@]+"${out[@]}"}")
}
mf_default_name() {
  local rec n d
  for rec in ${MF[@]+"${MF[@]}"}; do
    IFS="$MF_SEP" read -r n _ _ d <<<"$rec"
    [ "$d" = "true" ] && { printf '%s' "$n"; return 0; }
  done
  return 1
}

# ---------- plan / apply 两段式 ----------
PLAN=""; STAGE=""; BACKUP=""; UNDO=""; PROBE_FILE=""
_cleanup() {
  [ -n "$PROBE_FILE" ] && rm -f -- "$PROBE_FILE"
  [ -n "$PLAN" ] && rm -f -- "$PLAN"
  [ -n "$STAGE" ] && rm -rf -- "$STAGE"
  return 0
}
trap _cleanup EXIT     # 全局唯一 trap:各处只往 PROBE_FILE/PLAN/STAGE 里放东西,不再各自装 trap

plan_init() {
  PLAN="$(mktemp "${TMPDIR:-/tmp}/cc-acct-iso.plan.XXXXXX")" || die "mktemp 失败"
  STAGE="$(mktemp -d "${TMPDIR:-/tmp}/cc-acct-iso.stage.XXXXXX")" || die "mktemp -d 失败"
}

# 账号库级互斥锁。必须在 manifest_load **之前**拿:manifest 是读-改-写,
# 只锁住"写"的那一刻挡不住「两个进程各自基于陈旧快照渲染,后写者覆盖前写者」= 静默丢账号。
LOCK_HELD=0
lock_acquire() {
  [ "$LOCK_HELD" = 1 ] && return 0
  [ -d "$ACCTS_DIR" ] || return 0          # 还没 init:无处加锁,此时也没有并发对象
  exec 9>"$ACCTS_DIR/.lock" || die "无法建锁文件:$ACCTS_DIR/.lock"
  if have flock; then
    flock -w 30 9 || die "另一个 cc-acct-iso 正在操作 $ACCTS_DIR,等待 30s 超时"
  fi
  LOCK_HELD=1
  return 0
}
plan_add() {
  local op="$1" a
  shift
  printf '%s' "$op" >>"$PLAN"
  for a in "$@"; do
    case "$a" in
      *$'\t'*|*$'\n'*) die "路径含制表符/换行,本工具无法处理,请先改名:$a" ;;
    esac
    printf '\t%s' "$a" >>"$PLAN"
  done
  printf '\n' >>"$PLAN"
}
plan_empty() { [ ! -s "$PLAN" ]; }
plan_count() { [ -s "$PLAN" ] && wc -l <"$PLAN" || echo 0; }

plan_show() {
  local op a b c
  while IFS=$'\t' read -r op a b c; do
    case "$op" in
      MKDIR)    info "  建目录   $a" ;;
      LINK)     info "  建链接   $b → $a" ;;
      RELINK)   info "  修链接   $b → $a  ${_c_dim}(原指向有误)${_c_reset}" ;;
      MOVE)     info "  搬文件   $a → $b" ;;
      ISOLATE)  info "  私有化   $b  ${_c_dim}(内容取自 $a;共享库那份保留作模板)${_c_reset}" ;;
      COPY)     info "  复制     $a → $b" ;;
      SEED)     info "  种配置   $a → $b  ${_c_dim}(剥掉 oauthAccount)${_c_reset}" ;;
      RM)       info "  删除     $a" ;;
      CHMOD)    info "  改权限   $b → $a" ;;
      MANIFEST) info "  写 manifest  $MANIFEST" ;;
      *)        info "  ?? $op $a $b $c" ;;
    esac
  done <"$PLAN"
}

# 备份:只拷贝,**不登记 undo**。undo 一律在操作真正成功之后才登记,
# 保证「undo.tsv 里的每一条都对应一次真实发生过的改动」这个不变式。
bk_copy() {
  local p="$1" d parent
  { [ -e "$p" ] || [ -L "$p" ]; } || return 0
  parent="${p%/*}"; [ -n "$parent" ] || parent=/
  d="$BACKUP/root$parent"
  mkdir -p -- "$d" || die "建备份目录失败:$d"
  if [ -e "$d/${p##*/}" ] || [ -L "$d/${p##*/}" ]; then
    return 0   # 同一路径只备份第一次(保住最原始的内容)
  fi
  cp -a -- "$p" "$d/" || die "备份失败:$p"
}
undo_restore() { printf 'RESTORE\t%s\n' "$1" >>"$UNDO"; }
undo_delete()  { printf 'DELETE\t%s\n'  "$1" >>"$UNDO"; }

_exec_op() {
  local op="$1" a="$2" b="$3" tmp
  case "$op" in
    MKDIR)
      if [ ! -d "$a" ]; then
        mkdir -p -- "$a" || die "mkdir 失败:$a"
        undo_delete "$a"
      fi
      chmod 700 -- "$a" || die "chmod 700 失败(账号目录必须 700,里面是凭据):$a"
      ;;
    LINK)
      ln -s -- "$a" "$b" || die "建链接失败:$b"
      undo_delete "$b"
      ;;
    RELINK)
      bk_copy "$b"
      rm -f -- "$b" || die "删旧链接失败:$b"
      ln -s -- "$a" "$b" || die "建链接失败:$b"
      undo_restore "$b"
      ;;
    MOVE)
      bk_copy "$a"
      mv -- "$a" "$b" || die "搬移失败:$a → $b"
      undo_restore "$a"; undo_delete "$b"
      ;;
    ISOLATE)
      # 「隔离项现在是软链」⇒ 把它变成本账号私有实体。**copy-then-unlink，绝不 MOVE**:
      # MOVE 会把共享库那份搬走 ⇒ 只有第一个账号拿到文件、其余账号的软链全部悬空。
      # 共享库那份**保留**,作新账号的模板。
      #
      # 原子性:同目录 mktemp 写副本 → `mv -f` 盖过软链路径(rename(2),原子)。
      # 路径**没有一个瞬间是不存在的**,且内容与共享库逐字节相同 ⇒ 对正在读它的进程不可观测。
      #
      # CAS:`mv` 之前复核共享文件的 mtime+大小没变。变了说明有活进程在这中间写过它,
      # 我们手上这份副本已陈旧 ⇒ 不落盘、die,让人重跑(宁可不动也不落一次陈旧覆盖)。
      [ -e "$a" ] || die "私有化失败:共享库缺 $a"
      local sig_before sig_after
      sig_before="$(stat -c '%Y.%s' -- "$a" 2>/dev/null || echo unknown)"
      bk_copy "$b"
      tmp="$b.cc-acct-iso.tmp.$$"
      if [ -d "$a" ]; then
        rm -rf -- "$tmp"
        cp -a -- "$a" "$tmp" || { rm -rf -- "$tmp"; die "私有化复制失败(目录):$a → $b"; }
      else
        cp -- "$a" "$tmp" || { rm -f -- "$tmp"; die "私有化复制失败:$a → $b"; }
        chmod 600 -- "$tmp" || true
      fi
      sig_after="$(stat -c '%Y.%s' -- "$a" 2>/dev/null || echo unknown)"
      if [ "$sig_before" != "$sig_after" ]; then
        rm -rf -- "$tmp"
        die "私有化中止:$a 在复制期间被改动(有进程正在写它)。请重跑。"
      fi
      # 软链要先摘掉:`mv -f tmp link` 会跟随软链把内容写到共享库去(那是灾难性的反向覆盖)。
      [ -L "$b" ] && { rm -f -- "$b" || die "摘软链失败:$b"; }
      mv -f -- "$tmp" "$b" || { rm -rf -- "$tmp"; die "私有化落位失败:$b"; }
      # 自检:必须不再是软链,且内容与共享库一致。不符立刻回滚成软链。
      if [ -L "$b" ] || { [ ! -d "$a" ] && ! cmp -s -- "$a" "$b"; }; then
        rm -rf -- "$b"
        ln -s -- "$a" "$b" || warn "回滚建链也失败了:$b(备份在 $BACKUP)"
        die "私有化自检不通过,已回滚成软链:$b"
      fi
      undo_restore "$b"
      ;;
    COPY)
      tmp="$b.cc-acct-iso.tmp.$$"
      cp -- "$a" "$tmp" || { rm -f -- "$tmp"; die "复制失败:$a → $b"; }
      chmod 600 -- "$tmp" || true
      mv -f -- "$tmp" "$b" || { rm -f -- "$tmp"; die "落位失败:$b"; }
      undo_delete "$b"
      ;;
    SEED)
      have jq || die "--seed-claude-json 需要 jq"
      tmp="$b.cc-acct-iso.tmp.$$"
      jq 'del(.oauthAccount)' "$a" >"$tmp" || { rm -f -- "$tmp"; die "种配置失败:$a → $b"; }
      chmod 600 -- "$tmp" || true
      mv -f -- "$tmp" "$b" || { rm -f -- "$tmp"; die "落位失败:$b"; }
      undo_delete "$b"
      ;;
    RM)
      is_under "$a" "$ACCTS_DIR" || die "拒绝删除 ACCTS_DIR 之外的路径:$a"
      bk_copy "$a"
      rm -rf -- "$a" || die "删除失败:$a"
      undo_restore "$a"
      ;;
    CHMOD)
      if [ -e "$b" ]; then chmod "$a" -- "$b" || warn "chmod $a 失败:$b"; fi
      ;;
    MANIFEST)
      local existed=0
      [ -f "$MANIFEST" ] && { bk_copy "$MANIFEST"; existed=1; }
      tmp="$MANIFEST.cc-acct-iso.tmp.$$"
      cp -- "$STAGE/manifest.json" "$tmp" || { rm -f -- "$tmp"; die "写 manifest 失败"; }
      # 原子落位:cc-monitor 会并发读它,绝不能让它看见半截 JSON
      mv -f -- "$tmp" "$MANIFEST" || { rm -f -- "$tmp"; die "manifest 落位失败"; }
      if [ "$existed" = 1 ]; then undo_restore "$MANIFEST"; else undo_delete "$MANIFEST"; fi
      ;;
    *) die "内部错误:未知操作 $op" ;;
  esac
}

plan_commit() {
  if plan_empty; then
    info "无需改动(已是目标状态)。"
    return 0
  fi
  info ""
  info "${_c_bold}计划的改动:${_c_reset}"
  plan_show
  if [ "${APPLY:-0}" != 1 ]; then
    info ""
    info "${_c_dim}dry-run:以上均未执行。确认无误后重跑并加 --apply。${_c_reset}"
    return 0
  fi
  local ts op a b
  ts="$(date +%Y%m%d-%H%M%S)"
  mkdir -p -- "$ACCTS_DIR" || die "建 ACCTS_DIR 失败:$ACCTS_DIR"
  chmod 700 -- "$ACCTS_DIR" || die "chmod 700 失败(账号库必须 700,里面是凭据):$ACCTS_DIR"
  lock_acquire
  BACKUP="$ACCTS_DIR/.backup-$ts"
  [ -e "$BACKUP" ] && BACKUP="$ACCTS_DIR/.backup-$ts-$$"
  mkdir -p -- "$BACKUP/root" || die "建备份目录失败:$BACKUP"
  chmod 700 -- "$BACKUP" || die "chmod 700 失败(备份里有凭据副本):$BACKUP"
  UNDO="$BACKUP/undo.tsv"; : >"$UNDO"
  info ""
  info "${_c_dim}备份:$BACKUP(中途失败也可用它回退)${_c_reset}"
  while IFS=$'\t' read -r op a b _; do
    _exec_op "$op" "$a" "$b"
  done <"$PLAN"
  ok "已落盘。备份:$BACKUP"
  info "  回退:cc-acct-iso rollback ${BACKUP##*/.backup-} --apply"
  info "  ${_c_dim}注意:备份里含凭据明文副本,不再需要时请自行删除该目录。${_c_reset}"
}
