# __ccm_rbind：注册原语（只注册，不启动）。与 exec claude 同一 (子)shell 内调用：
#   ( __ccm_rbind; exec claude "$@" )
__ccm_rbind() {
  if [ -n "$TMUX" ]; then
    # tmux 默认不把 pane title 传给外层终端窗口标题（marker 会被截住）；
    # 只对当前 session 开直通，不动全局配置
    tmux set set-titles on >/dev/null 2>&1
    tmux set set-titles-string "#T" >/dev/null 2>&1
  fi
  local cpid=$BASHPID
  ( prev="" n=0
    while kill -0 "$cpid" 2>/dev/null; do
      sid=$(grep -o '"sessionId":"[^"]*"' ~/.claude/sessions/$cpid.json 2>/dev/null | head -1 | cut -d'"' -f4)
      n=$((n+1))
      # sid 变了立即刷；每 20 秒周期重打一次自愈（外层标题可能被 PS1 转义/切窗覆写）
      if [ -n "$sid" ] && { [ "$sid" != "$prev" ] || [ $((n % 20)) -eq 0 ]; }; then
        printf '\033]0;ccm-rbind-%s\007' "$sid"; prev="$sid"
      fi
      sleep 1
    done
  ) &
}
# ccm：便捷启动器（可选）。已有同名函数/命令时不覆盖——自有启动器请在
# 自己的函数里调 __ccm_rbind（见上方契约）。
if ! declare -f ccm >/dev/null 2>&1; then
ccm() { ( __ccm_rbind; exec claude "$@" ); }
fi
