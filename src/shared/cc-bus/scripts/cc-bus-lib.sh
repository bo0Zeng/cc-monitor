#!/usr/bin/env bash
# shellcheck disable=SC2034  # CCBUS_CODE · CCBUS_OUT 是给 source 本文件的那几个动词读的
# cc-bus-lib.sh —— 各动词共用的几行:找后端 · 把一句交给后端 · 敲门。被各动词 source,不自执行。
# 规矩(信封 · 范围 · 去重 · 限流 · 敲不敲)、收件箱、已读位置都住 cc-monitor 后端(`ccm -- --bus-<动词>`);
# 这里不读写任何文件、不加锁、不判。

# 找这台的后端(就是 `ccm`)。唯一一处查找次序:
#   `$CCM_BIN`(显式) → 部署落点 `~/.cc-monitor/bin/ccm` → PATH 上的 `cc-monitor-backend` → PATH 上的 `ccm`。
# 刻意不认 `$CCM_BACKEND_BIN`:那是 monitor 给终端窗口导的「正在跑的那份常驻后端」,不是给 CLI 找入口用的。
# 必须 `-f` 而不能只 `-x`:`[ -x <目录> ]` 为真(装个名叫 `ccm` 的目录就会劫持解析)。
ccbus_ccm_bin() {
  local x
  for x in "${CCM_BIN:-}" "$HOME/.cc-monitor/bin/ccm" \
           "$(command -v cc-monitor-backend 2>/dev/null)" "$(command -v ccm 2>/dev/null)"; do
    if [ -n "$x" ] && [ -f "$x" ] && [ -x "$x" ]; then printf '%s' "$x"; return 0; fi
  done
  return 1
}

# ccbus_ask <动词> <JSON 入参> —— 交给后端 `--bus-<动词>`,成了 ⇒ 回包放进 `CCBUS_OUT`、返回 0。
# 不成 ⇒ 后端那一句 ＋ 原话印到 stderr,码放进 `CCBUS_CODE`(找不到后端 ⇒ `no_backend`),返回非 0。
# 在当前 shell 里调(别包进 `$( )`:那两格是给调用方读的变量)。
CCBUS_CODE=""
CCBUS_OUT=""
ccbus_ask() {
  local verb="$1" req="$2" ccm errf out rc
  CCBUS_CODE=""
  CCBUS_OUT=""
  ccm=$(ccbus_ccm_bin) || {
    CCBUS_CODE=no_backend
    echo "cc-bus $verb: no cc-monitor backend (~/.cc-monitor/bin/ccm) — nothing done" >&2
    return 1
  }
  errf=$(mktemp) || return 1
  out=$(printf '%s' "$req" | "$ccm" -- "--bus-$verb" --within-ms 10000 2>"$errf"); rc=$?
  if [ "$rc" -ne 0 ]; then
    CCBUS_CODE=$(jq -r '.code // ""' <"$errf" 2>/dev/null || true)
    jq -r '.message + (if (.detail // "") != "" then "\n" + .detail else "" end)' <"$errf" >&2 2>/dev/null || cat "$errf" >&2
    rm -f "$errf"
    return "$rc"
  fi
  rm -f "$errf"
  CCBUS_OUT=$(printf '%s' "$out" | jq -c '(.data // .)')
}

# 装那一家 agent 的词典(结束钩子的回包怎么写:`agent_block_reason`)。`CCBUS_ADAPT_AGENT` 选,缺省 claude。
ccbus_agent_load() {
  local f
  f="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/cc-bus-agent-${CCBUS_ADAPT_AGENT:-claude}.sh"
  [ -f "$f" ] || { echo "cc-bus: 没有 ${CCBUS_ADAPT_AGENT:-claude} 这个 agent 的词典($f)" >&2; return 1; }
  # shellcheck disable=SC1090
  . "$f"
}

# ── 敲门那一句 ───────────────────────────────────────────────────────────
# 🔴 敲门那一句是对方的输入,正文进去就成了指令 ⇒ 只填发信人(会话 id,后端核过)。正文只经 Stop 钩子喂回。
ccbus_knock_text() { printf '🔔 cc-bus: 你有来自 %s 的新消息,运行 cc-bus recv 读取并按内容处理' "$1"; }

# 经后端往那条会话说一句(敲门):先看一眼那一屏拿指纹,再带着它说 ——
#   后端那一道照它的规矩判:窗格真是那条会话 · 只在它闲着时送(`idle_only`)· 画面在这两下之间变了就不送。
#   没送 ⇒ rc=1 并说一句(信已在收件箱里,回合结束时 Stop 钩子照样喂回,不丢)。
ccbus_say() {   # <会话 id> <那一句>
  local sid="$1" text="$2" ccm screen out res
  ccm=$(ccbus_ccm_bin) || return 1
  screen=$(jq -cn --arg s "$sid" '{sid:$s, color:false}' \
    | "$ccm" -- --terminal-preview --within-ms 10000 2>/dev/null | jq -r '(.data // .).screen // empty' 2>/dev/null)
  if [ -z "$screen" ]; then
    echo "cc-bus: 看不到 $sid 那一屏,没敲门 —— 信在收件箱里,轮末照样喂回" >&2; return 1
  fi
  out=$(jq -cn --arg s "$sid" --arg t "$text" --arg v "$screen" '{sid:$s, text:$t, seen_screen:$v, idle_only:true}' \
    | "$ccm" -- --session-say --within-ms 10000 2>/dev/null)
  res=$(printf '%s' "$out" | jq -r '(.data // .).result // empty' 2>/dev/null)
  case "$res" in delivered|unsure) return 0 ;; esac
  echo "cc-bus: 没敲门($(printf '%s' "$out" | jq -r '(.data // .).why // "?"' 2>/dev/null))—— 信在收件箱里,轮末照样喂回" >&2
  return 1
}
