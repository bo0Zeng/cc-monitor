# cc-monitor 装进来的 ccm 别名（卸载时整段删掉）。
#
#   ccm 是 claude 的壳：ccm [交给 claude 的参数...] -- [ccm 自己的选项...]
#     没有 -- 时整行原样交给 claude(--resume <会话ID> / --continue / --model / --tmux / -p ...)
#     ccm 自己的选项写在最后一个 -- 右边: --ccm-tmux[=<名>]  --account <名>|--base  --cwd <dir>  --ccm-agent claude|codex  --launcher <cmd>  --attach <名>
#   详见 ccm -- --ccm-help
#
# 想要别的组合，照下面的样子自己加一个函数就行。

# ccm 装在 ~/.cc-monitor/bin/ccm（本机与远端一样）
case ":$PATH:" in *":$HOME/.cc-monitor/bin:"*) ;; *) export PATH="$HOME/.cc-monitor/bin:$PATH";; esac

# 自带的几个别名；你已经有同名函数时不覆盖它
if ! declare -f cc >/dev/null 2>&1; then
cc()  { ccm "$@"; }                        # 在当前目录起会话
fi
if ! declare -f cct >/dev/null 2>&1; then
cct() { ccm "$@" -- --ccm-tmux; }          # 在 tmux 里起（断线可 attach 回来）；你跟的参数交给 claude
fi
if ! declare -f cca >/dev/null 2>&1; then
cca() { ccm -- --attach "$@"; }            # 接回一个 tmux 会话（cca <会话名>）
fi

# 每个账号一条的别名（如 alphacc / alphacct）在 cc-monitor 的「别名」那一块生成，写在 ~/.cc-monitor/aliases.sh：
#   alphacc()  { ccm "$@" -- --account z; }
#   alphacct() { ccm "$@" -- --ccm-tmux --account z; }
# 下面这一行接上那份文件；没生成过就什么都不做。
if [ -r "$HOME/.cc-monitor/aliases.sh" ]; then . "$HOME/.cc-monitor/aliases.sh"; fi
