# shellcheck shell=bash
# 被 e2e 套件在最前面 source：**无条件**清掉从开发机会话继承来的环境变量 ——
# `CCM_*`（本机后端的监听口 · 中转口 · stderr 日志 · 凭据落点，都指真 `~/.cc-monitor`）·
# `CLAUDE*` · `ANTHROPIC_*` · `TMUX*` · `CC_BUS_*`。
# 套件起的后端、claude 桩、tmux 只认套件自己给的环境：单跑（`npm run test:<套件>`）与经门禁跑同一个起点。
# 漏一个会怎样：后端带着 `CCM_LISTEN_PORT` 去绑真端口、把真 `stderr.log` 轮转掉（10-02 一趟门禁里真发生过）。
for _ccm_sandbox_k in $(compgen -e); do
  case "$_ccm_sandbox_k" in
    CCM_*|CLAUDE*|ANTHROPIC_*|TMUX*|CC_BUS_*) unset "$_ccm_sandbox_k" ;;
  esac
done
unset _ccm_sandbox_k
