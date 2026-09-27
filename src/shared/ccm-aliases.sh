# cc-monitor 统一启动器的**别名层**。
#
# 实现是一个可执行文件（不是 shell 函数 —— 与 shell 无关，zsh/fish 同样可用）；落点见下面那行 PATH。
# 本块只放**组合层别名**：自定义在这里，不在实现里。随便改、随便加。
#
#   ccm 是 claude 的壳：ccm [壳层选项...] [交给 claude 的参数...]
#     壳层选项: --tmux[=<名>]  --account <名>|--base  --cwd <dir>  --agent claude|codex  --launcher <cmd>  --attach <名>
#     其余(--resume <sid> / --continue / --model / -p ...)原样交给 claude；claude 自己的同名选项写在 -- 后面
#   详见 `ccm --ccm-help`
#
# 加一个新维度 = ccm 多一个 flag + 这里多一行别名，不是再写一个实现。

# ccm 的落点：本机 ~/.cc-monitor/bin（monitor 放的那份）· 远端也是 ~/.cc-monitor/bin（部署后端时放的入口；更早的版本放在 ~/.local/bin，那份不删、照旧能用）
for __ccm_d in "$HOME/.local/bin" "$HOME/.cc-monitor/bin"; do case ":$PATH:" in *":$__ccm_d:"*) ;; *) export PATH="$__ccm_d:$PATH";; esac; done; unset __ccm_d  # 两边共用一份文本 ⇒ 两个都加；本机那个排在前面（赢过你那份旧的）

# 便捷别名 —— **不覆盖你已有的同名函数**（有自己启动器的用户在自己的函数里调 ccm 即可）
#
# 〔用@09-11〕`cch` 删了：它当初的意思是「别猜目录，就在当前目录起」，
# 而**不给 --cwd 现在本来就是当前目录**（ccm 不再替你跳工作区 / 跳 git 仓的父目录）
# ⇒ 它和 cc 一模一样。**`cch` 这个名字从此是你自己的**，想怎么定义都行。
if ! declare -f cc >/dev/null 2>&1; then
cc()  { ccm "$@"; }                        # 在当前目录起会话
fi
if ! declare -f cct >/dev/null 2>&1; then
cct() { ccm --tmux "$@"; }                 # 在 tmux 里起（断线可 attach 回来）
fi
if ! declare -f cca >/dev/null 2>&1; then
cca() { ccm --attach "$@"; }               # 接回一个 tmux 会话（cca <会话名>）
fi

# 每账号别名 —— K-R49 起**不用再自己加了**：在 cc-monitor 的「账号」里点一下「生成命令」，
# 它会按账号表整份重写下面这份文件（加了账号就多一条，删了账号那条就没了）。
#   alphacc()  { ccm --account z "$@"; }      ← 它生成的就是这一形
#   alphacct() { ccm --tmux --account z "$@"; }
#
# 这一行让那份文件自动接上：**它是 cc-monitor 自己的文件**（不在你的 rc 里、随时可删），
# 没生成过就什么都不做。所以你这份 shell 配置**只会被写这一次**。
# 写成 if/fi 而不是 `[ -r … ] && . …`：后者在文件不存在时整行返回 1，而这是本片段的最后一行。
if [ -r "$HOME/.cc-monitor/aliases.sh" ]; then . "$HOME/.cc-monitor/aliases.sh"; fi
