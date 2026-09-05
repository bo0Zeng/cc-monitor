#!/bin/sh
# 一份**可复用的假 `cc-monitor-remote`**〔`K-P2` `F` 拍 09-04，PM 裁定②〕。
#
# # 它为什么必须存在
#
# 用@09-04 逐字「**ccm不要管找不到, 统一走后端**」⇒ `shared/ccm` 的**建会话**与**账号解析**
# 两条路都没有本地退路了：问不到后端就 `exit 4`。
# 而 `e2e/` 里那几套**跑在一台没装后端的机器上**（沙箱容器 / CI runner），
# 于是它们从「测 ccm 的行为」退化成「测『这台机器没装后端』」——
# 现打过一次：`ccm-acceptance` 29/0 → 5/24、`cc-spawn-uplift` 67/5 → 31/41。
# ⇒ **套件要自带后端**，就像它们早就自带 `tmux` shim 与假 launcher 一样。
#
# # 它做什么、不做什么
#
# **做**：认 `shared/ccm` 真正会发的那三条一次性子命令，各按 `doc/IPC-PROTOCOL.md` 的形状答：
#   · `--list-accounts --accts-dir <d>`：把 `<d>/accounts.json` **原样翻成帧形状**；
#   · `--launch`（stdin 一条 JSON）：**真的**去 `tmux` 建会话 / 打标 / `send-keys`；
#   · `--resolve`（stdin 一条 JSON）：按 `FAKE_DAEMON_RESOLVE` 给的答案回，不给就回空。
#
# **不做**：不进流模式（那才会往 tmux server 装 hook）· 不读 `~/.claude` · 不碰用户任何东西 ·
# 不写 `@ccm_sid`（**事实**标记只由真 daemon 的 `identity_tag` 过检之后写；这里写的是
# **意图**标记 `@ccm_sid_expect`，与 `control/launch.rs` 的 `CreateOrAttach` 臂逐条同序）。
#
# ⚠ **单一事实源仍是那份 manifest**：账号那一段把 `<accts-dir>/accounts.json` 翻一遍，
#   **不在这里手抄一张账号表**（抄了就有两份要同步 —— 这份文件治的正是那一族）。
#
# ⚠⚠ **`tmux` 一律经 `_tmux()`，而它永远带 `-L`；不给 socket 就 fail-closed。**
#
#   第一版写的是「裸调 `tmux`，靠调用方 PATH 上的 shim 强插 `-L`」。
#   `e2e_gate_registry::no_e2e_suite_isolates_with_tmux_tmpdir` **当场把它逮住了**，
#   而且它是对的：那种写法**手跑一次就会把 fixture 会话建到用户的默认 socket 上**
#   （`C7i` 那条红线的来历正是 08-11 那次打没了用户 9 个真实会话）。
#   「靠调用方」这个理由对**别的**夹具成立（它们登记在那条判据的例外表里），
#   但那张表在本件写区外，而且**靠登记不如靠结构**：
#   ⇒ 本文件自带选择器，并且**没有 socket 就不跑** —— 手跑也伤不到任何人。
#   ⚠ 调用方的 shim 会再插一个 `-L`（`realtmux -L <套件的> -L <这里的> …`），
#     tmux 取**最后一个** ⇒ 两边给同一个名字即可，套件那一行是这么写的。
#
# # 环境钩子（都只影响这份假货，不进 `shared/ccm` 的任何契约）
#
#   FAKE_DAEMON_TMUX_SOCK=<名>  **`--launch` 必需**：tmux 的私有 socket 名（`-L <名>`）。
#                               不给就 fail-closed（见下方 `_tmux` 那段头注）。
#   FAKE_DAEMON_SPOOL=<目录>    落盘调用痕迹：`calls`（一行一次）· `argv` · `stdin.bin`
#   FAKE_DAEMON_RESOLVE=<串>    `--resolve` 回的 `command` 值；未设 ⇒ 回空 JSON（拿不到）
#   FAKE_DAEMON_DEAF=1          **装作答不上来**：任何子命令都只 `exit 0` 不吐东西
#                               （给「后端在但答非所问」那一格用 —— 它与「没装」是两回事）
#   FAKE_DAEMON_NO_LAUNCH=1     **只有 `--launch` 答不出**，账号那条照常答。
#                               这一格是**独立测「建会话那条腿没有退路」**用的：
#                               后端整个不在的话，账号那条腿会**先**报「后端不可达」——
#                               那时判据看到的红是账号那条腿的，不是建会话那条的。
#                               ⇒ 要证「建会话没有退路」，就得让账号那条腿先过去。
#
# ⚠ 硬依赖 `jq`（解析 stdin 那条 JSON）。**fail-closed**：缺它就非零退出并说一句，
#   不许静默少答一条 —— 静默少答会让调用方读成「后端答不出」，把环境缺工具误报成产品缺陷。
set -u

_spool="${FAKE_DAEMON_SPOOL:-}"
if [ -n "$_spool" ]; then
  mkdir -p "$_spool" 2>/dev/null || true
  echo call >> "$_spool/calls"
  printf '%s\n' "$*" >> "$_spool/argv"
fi

_sub="${1:-}"

# stdin 只在 `--launch` / `--resolve` 上有；其余子命令要把它吞掉，
# 不然调用方那边的 `printf | 本脚本` 会拿到 EPIPE。
_slurp() {
  if [ -n "$_spool" ]; then cat > "$_spool/stdin.bin"; cat "$_spool/stdin.bin"
  else cat; fi
}

if [ "${FAKE_DAEMON_DEAF:-}" = 1 ]; then
  case "$_sub" in --launch|--resolve) _slurp >/dev/null ;; *) cat >/dev/null 2>/dev/null ;; esac
  exit 0
fi

command -v jq >/dev/null 2>&1 || {
  echo "e2e/fake-daemon.sh: 需要 jq —— 它要解析 --launch/--list-accounts 那条 JSON。" >&2
  echo "     缺它会静默少答一条，而调用方会把那读成「后端答不出」（环境缺工具被误报成产品缺陷）。" >&2
  exit 3; }

# `tmux` 的唯一出口 —— **永远带选择器**（`-L`）。见头注那段「fail-closed」的理由。
_tmux() { tmux -L "$FAKE_DAEMON_TMUX_SOCK" "$@"; }

case "$_sub" in
  --list-accounts)
    d=""
    while [ $# -gt 0 ]; do [ "$1" = --accts-dir ] && d="${2:-}"; shift; done
    # 首行 meta —— `shared/ccm` 靠 `"kind":"accounts-meta"` 分辨「答上了」与「答不出」。
    printf '{"accountZeroAware":true,"acctsDir":"%s","count":0,"enabled":true,"error":null,"kind":"accounts-meta","manifestPath":"%s/accounts.json","sharedStore":null,"updatedAt":null}\n' "$d" "$d"
    # 每账号一行。⚠ manifest 不在**不是错**：那台机器就是没有账号库，
    #   帧照发（meta 在 ⇒ ccm 收到一张空表 ⇒ 退化为基座启动器，一个字不说）。
    [ -f "$d/accounts.json" ] && jq -c '.accounts[] | {configDir:(.configDir // null),email:"",exists:true,isDefault:(.isDefault // false),loggedIn:false,mode:"isolated",name:.name}' "$d/accounts.json" 2>/dev/null
    exit 0 ;;

  --resolve)
    _slurp >/dev/null
    [ -n "${FAKE_DAEMON_RESOLVE:-}" ] && printf '{"command":"%s"}\n' "$FAKE_DAEMON_RESOLVE"
    exit 0 ;;

  --launch)
    req="$(_slurp)"
    # 🔴 **fail-closed**：没给私有 socket 就不碰 tmux（手跑一次就会打到用户默认 socket ——
    #    `C7i` 那条红线的来历）。⚠ 这一条**排在最前**，别挪到后面去。
    [ -n "${FAKE_DAEMON_TMUX_SOCK:-}" ] || {
      echo "e2e/fake-daemon.sh: --launch 需要 FAKE_DAEMON_TMUX_SOCK=<私有 socket 名>。" >&2
      echo "     不给就不跑 —— 裸调 tmux 会把 fixture 会话建到**用户的默认 socket** 上（C7i）。" >&2
      exit 3; }
    # 只哑这一条腿（见头注 `FAKE_DAEMON_NO_LAUNCH`）。⚠ **stdin 先吞掉再退**：
    # 不吞的话调用方那条 `printf … | 本脚本` 会拿到 EPIPE，症状变成「管道坏了」而不是「答不出」。
    [ "${FAKE_DAEMON_NO_LAUNCH:-}" = 1 ] && exit 0
    mode="$(printf '%s' "$req" | jq -r '.mode // ""')"
    name="$(printf '%s' "$req" | jq -r '.name // ""')"
    payload="$(printf '%s' "$req" | jq -r '.payload // ""')"
    cwd="$(printf '%s' "$req" | jq -r '.cwd // ""')"
    sid="$(printf '%s' "$req" | jq -r '.ccm_sid // ""')"
    agent="$(printf '%s' "$req" | jq -r '.agent // ""')"
    w="$(printf '%s' "$req" | jq -r '.width // ""')"
    h="$(printf '%s' "$req" | jq -r '.height // ""')"
    [ "$mode" = create-or-attach ] || { printf '{"error":"unsupported mode"}\n'; exit 0; }
    t="=$name:"
    # 幂等闸，与 `control/launch.rs` 逐条同序：`new-session` 失败 ⇒ **短路，什么都不做**，
    # 回 `created:false`（撞名由**调用方**去走它自己的响亮失败，不在这里替它决定）。
    if [ -n "$cwd" ]; then set -- new-session -d -s "$name" -c "$cwd"; else set -- new-session -d -s "$name"; fi
    [ -n "$w" ] && [ -n "$h" ] && set -- "$@" -x "$w" -y "$h"
    if ! _tmux "$@" 2>/dev/null; then
      printf '{"session":"%s","created":false,"typed":false}\n' "$name"
      exit 0
    fi
    # 顺序照 `control/launch.rs`：`new-session` → `@ccm_agent` → `@ccm_sid_expect` → `send-keys`。
    [ -n "$agent" ] && _tmux set-option -t "$t" @ccm_agent "$agent" 2>/dev/null
    if [ -n "$sid" ]; then
      # ★ **意图**标记，不是事实标记。写裸 `@ccm_sid` 就是 F04 修掉的 `R10` 原路回来
      #   —— 那一格由 `ccm_cli_contract::the_intent_tag_and_the_fact_tag_are_not_merged_by_the_move`
      #   在**真** daemon 那侧钉着；这份假货照它的形状走，别在这里发明第二套。
      _tmux set-option -t "$t" @ccm_sid_expect "$sid" 2>/dev/null
      _tmux set-option -t "$t" set-titles on 2>/dev/null
      _tmux set-option -t "$t" set-titles-string '#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}' 2>/dev/null
    fi
    _tmux send-keys -t "$t" "$payload" Enter 2>/dev/null
    printf '{"session":"%s","created":true,"typed":true}\n' "$name"
    exit 0 ;;

  *)
    # 未知子命令：照真二进制的样子回一句可辨认的话（`main.rs:203-204` 逐字 `unknown argument`），
    # 别静默 `exit 0` —— 静默会让「这份假货不认它」与「后端答上了但内容是空」在调用方眼里同形。
    cat >/dev/null 2>/dev/null
    printf 'cc-monitor-remote query error: unknown argument: %s\n' "$_sub" >&2
    exit 2 ;;
esac
