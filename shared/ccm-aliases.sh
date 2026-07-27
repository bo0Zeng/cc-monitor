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
if ! declare -f cc >/dev/null 2>&1; then
cc()  { ccm "$@"; }                        # 智能选目录起会话
fi
if ! declare -f cch >/dev/null 2>&1; then
cch() { ccm --cwd . "$@"; }                # 当前目录直起
fi
if ! declare -f cct >/dev/null 2>&1; then
cct() { ccm --tmux "$@"; }                 # 在 tmux 里起（断线可 attach 回来）
fi

# 每账号别名按需自己加，例如（账号名来自 ~/.claude-accts/accounts.json）：
#   zcc()  { ccm --account z "$@"; }
#   zcct() { ccm --tmux --account z "$@"; }
