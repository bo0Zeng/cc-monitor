# cc-monitor 装进来的终端接入（卸载时整段删掉）：让终端认得 ccm，并接上 cc-monitor「别名」里的那份清单。
case ":$PATH:" in *":$HOME/.cc-monitor/bin:"*) ;; *) export PATH="$HOME/.cc-monitor/bin:$PATH";; esac
if [ -r "$HOME/.cc-monitor/aliases.sh" ]; then . "$HOME/.cc-monitor/aliases.sh"; fi
