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
        printf '\033]0;ccm-rbind-%s\007' "$sid"
        # 同 sid 写进 tmux user option @ccm_sid：pane title 会被 Claude 自己的活动标题抢写
        # （不可靠），@ccm_sid 是 Claude 碰不到的通道 = 「这个 tmux 此刻在跑哪个 sid」的权威
        # 信号，随 /branch 漂移实时更新。cc-monitor 靠它精确认会话（attach/resume 不撞同目录别的）。
        # 契约见 cc-monitor doc/INVARIANTS.md §30 + 权威规格 agents/claude-code.md §4。
        # session 级 option（一 claude 一 session 的既有假设；同 session 多 pane 各跑不同 claude 时
        # 由最后写的 rbind 定，属边角）。
        [ -n "$TMUX" ] && tmux set-option @ccm_sid "$sid" >/dev/null 2>&1
        prev="$sid"
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
