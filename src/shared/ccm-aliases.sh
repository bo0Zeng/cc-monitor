# cc-monitor 装进来的终端接入（卸载时整段删掉）：让终端认得 ccm，并接上 cc-monitor「别名」里的那份清单。
case ":$PATH:" in *":$HOME/{{BIN_DIR}}:"*) ;; *) export PATH="$HOME/{{BIN_DIR}}:$PATH";; esac
if [ -r "$HOME/{{ALIASES}}" ]; then . "$HOME/{{ALIASES}}"; fi
# 本机桌面上开的终端（不是经 ssh 登进来的）：留一份记录，cc-monitor 据它认出这个终端窗口（↗ 切得回来）；这里不等、不起后台。
if [ -z "${SSH_CONNECTION:-}" ] && [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ] && __ccm_tty=$(tty 2>/dev/null) && [ -r "/proc/$$/stat" ]; then
  __ccm_start=$({{SELF_START}})
  export LC_CCM_WINDOW="$$-$__ccm_start"
  __ccm_dir="${{{DATA_DIR_ENV}}:-$HOME/{{DATA_HOME}}}/{{AWAIT_SUBDIR}}"
  if mkdir -p "$__ccm_dir" 2>/dev/null; then
    printf '{"shell_pid":%s,"proc_start":"%s","tty":"%s"}\n' "$$" "$__ccm_start" "$__ccm_tty" > "$__ccm_dir/$$.tty.part" && mv -f "$__ccm_dir/$$.tty.part" "$__ccm_dir/$$.tty"
  fi
  unset __ccm_tty __ccm_start __ccm_dir
fi
