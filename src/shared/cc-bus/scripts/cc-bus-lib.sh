#!/usr/bin/env bash
# shellcheck disable=SC2034  # CCBUS_CODE · CCBUS_OUT 是给 source 本文件的那几个动词读的
# cc-bus-lib.sh —— 各动词共用的几行:找后端 · 把一句交给后端 · 装 agent 词典。被各动词 source,不自执行。
# 规矩(信封 · 范围 · 去重 · 限流 · 敲不敲)、收件箱、已读位置、敲门都住 cc-monitor 后端(`ccm -- --bus-<动词>`);
# 这里不读写任何文件、不加锁、不判、不往会话里说话。

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
