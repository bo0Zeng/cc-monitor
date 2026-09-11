# cc-monitor 统一启动器的**别名层**。
#
# 唯一实现在 ~/.local/bin/ccm（可执行文件，不是 shell 函数 —— 与 shell 无关，zsh/fish 同样可用）。
# 本块只放**组合层别名**：自定义在这里，不在实现里。随便改、随便加。
#
#   ccm [动作] [修饰...] [-- 透传给 agent]
#     动作: (缺省)=new | resume <sid> | attach <名>
#     修饰: --tmux[=<名>]  --account <名>|--base  --cwd auto|<dir>  --agent claude|codex  --launcher <cmd>
#   配置(代理/工作区/账号库路径): ~/.config/ccm/config      详见 `ccm --help`
#
# 加一个新维度 = ccm 多一个 flag + 这里多一行别名，不是再写一个实现。

# ccm 装在 ~/.local/bin，确保它在 PATH 里
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) export PATH="$HOME/.local/bin:$PATH";; esac

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

# 每账号别名 —— K-R49 起**不用再自己加了**：在 cc-monitor 的「账号」里点一下「生成命令」，
# 它会按账号表整份重写下面这份文件（加了账号就多一条，删了账号那条就没了）。
#   zcc()  { ccm --account z "$@"; }      ← 它生成的就是这一形
#   zcct() { ccm --tmux --account z "$@"; }
#
# 这一行让那份文件自动接上：**它是 cc-monitor 自己的文件**（不在你的 rc 里、随时可删），
# 没生成过就什么都不做。所以你这份 shell 配置**只会被写这一次**。
# 写成 if/fi 而不是 `[ -r … ] && . …`：后者在文件不存在时整行返回 1，而这是本片段的最后一行。
if [ -r "$HOME/.cc-monitor/account-aliases.sh" ]; then . "$HOME/.cc-monitor/account-aliases.sh"; fi
