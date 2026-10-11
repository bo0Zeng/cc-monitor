# shellcheck shell=bash
# 被 e2e 套件在最前面 source：**无条件**清掉从开发机会话继承来的环境变量 ——
# `CCM_*`（本机后端的监听口 · 中转口 · stderr 日志 · 凭据落点，都指真 `~/.cc-monitor`）·
# `CLAUDE*` · `ANTHROPIC_*` · `TMUX*` · `CC_BUS_*`。
# 套件起的后端、claude 桩、tmux 只认套件自己给的环境：单跑（`npm run test:<套件>`）与经门禁跑同一个起点。
# 漏一个会怎样：后端带着常驻开关去占真家里的门牌、把真 `stderr.log` 轮转掉（10-02 一趟门禁里真发生过）。
# 清完之后带上「这是一次沙箱跑」的标记（`relay_route_core::SANDBOX_ENV`）：套件起的常驻后端若要占本账号真家目录里的门牌就拒绝起。
for _ccm_sandbox_k in $(compgen -e); do
  case "$_ccm_sandbox_k" in
    CCM_*|CLAUDE*|ANTHROPIC_*|TMUX*|CC_BUS_*) unset "$_ccm_sandbox_k" ;;
  esac
done
unset _ccm_sandbox_k
export CCM_SANDBOX=1

# 测试里「等多久算挂了」的唯一上限（秒），与 Rust 的 `guard_core::wall::CEILING_SECS` 同一个数（判据 `wall_ceiling` 逐份核）。
# 墙钟只许兜底：等一件事成立用 `wall_until`，给一条命令封顶用 `timeout "$WALL_CEILING_S"`；
# 不拿「几秒之内」判快慢 —— 门禁并发、机器忙时那种判据会误红。要分「自己回来」与「挂住被掐」，看退出码（124 = 被掐）。
export WALL_CEILING_S=300
# `wall_until '<条件>'`：条件成立就回 0，每 0.05 秒再问一次；到上限还不成立回 1。条件串每次现 eval（单引号里的变量到时候才展开）。
wall_until() {
  local _wall_end=$((SECONDS + WALL_CEILING_S))
  until eval "$1"; do
    [ "$SECONDS" -lt "$_wall_end" ] || return 1
    sleep 0.05
  done
}
