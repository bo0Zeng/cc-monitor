# shellcheck shell=bash
# 被 e2e 套件在最前面 source：**无条件**摘掉从开发机会话继承来的环境变量（表在 `tests/scripts/session-vars`，全仓只那一张；
# 族里照留的判据输入 `CCM_E2E_*` 等不摘）。套件起的后端、claude 桩、tmux 只认套件自己给的环境：单跑（`npm run test:<套件>`）与经门禁跑同一个起点。
# 漏一个会怎样：后端带着常驻开关去占真家里的门牌、把真 `stderr.log` 轮转掉。
# 摘完之后带上「这是一次沙箱跑」的标记（`relay_route_core::SANDBOX_ENV`）：套件起的常驻后端若要占本账号真家目录里的门牌就拒绝起。
_ccm_sandbox_names=$(bash "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../scripts/session-scrub.sh" --list) || exit 2
for _ccm_sandbox_k in $_ccm_sandbox_names; do unset "$_ccm_sandbox_k"; done
unset _ccm_sandbox_k _ccm_sandbox_names
export CCM_SANDBOX=1

# tmux：这一趟一台私有的（共享原语的 shim 放进 PATH 最前、强插 `-L <本趟私有名>`）。套件自己不碰 tmux 也装 ——
# 它起的后端会裸调 `tmux`（会话快照 · 终端那几处），没有 shim 就连到缺省 server（单跑时是用户正在用的那台）。
# 套件再 `. tmux-shim.sh <前缀>` 只是把同一份 shim 改钉它自己的名字。自己设 `trap … EXIT` 的套件收尾得调 `tmux_shim_cleanup`
#（判据 `every_e2e_suite_gets_a_private_tmux_from_the_sandbox_env`）。
# shellcheck source=tests/e2e/tmux-shim.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/tmux-shim.sh" --sandbox-default || exit 2
trap tmux_shim_cleanup EXIT

# 测试里「等多久算挂了」的唯一上限（秒），与 Rust 的 `guard_core::wall::CEILING_SECS` 同一个数（判据 `wall_ceiling` 逐份核）。
# 墙钟只许兜底：等一件事成立用 `wall_until`，给一条命令封顶用 `timeout "$WALL_CEILING_S"`；
# 不拿「几秒之内」判快慢 —— 门禁并发、机器忙时那种判据会误红。要分「自己回来」与「挂住被掐」，看退出码（124 = 被掐）。
export WALL_CEILING_S=300
# `wall_until '<条件>'`：条件成立就回 0，每 0.05 秒再问一次；到上限还不成立回 1（stderr 说一声是哪个条件）。
# 不睡固定秒数再看（判据 `wall_ceiling` 的 sh-sleep 那一形）：等一件事就等它成立；要看「没发生」，先等一个说明「定了」的正面迹象。条件串每次现 eval（单引号里的变量到时候才展开）。
wall_until() {
  local _wall_end=$((SECONDS + WALL_CEILING_S))
  until eval "$1"; do
    [ "$SECONDS" -lt "$_wall_end" ] || { echo "wall_until：到兜底上限 ${WALL_CEILING_S} 秒还不成立：$1" >&2; return 1; }
    sleep 0.05
  done
}

# `proc_start <pid>`：进程的起始时刻（`/proc/<pid>/stat` 第 22 格 starttime，时钟滴答），喂 pidfile 的 `procStart`。
# 第 2 格是带括号的进程名，名字里能有空格和 `)` ⇒ 先剥到最后一个 `) ` 再数（剥完第 3 格是 $1，第 22 格是 $20）；
# 直接 `awk '{print $22}'` 名字带空格时列会错位（判据 `every_shell_read_of_proc_stat_columns_strips_the_process_name_first`）。
proc_start() { sed 's/.*) //' "/proc/$1/stat" | awk '{print $20}'; }

# `e2e_leftovers <沙箱家目录>`：印还活着的、家目录是它的那些进程（`pid 命令行` 一行一个；本 shell 与这层子 shell 除外）。
# 按 `/proc/<pid>/environ` 里的 `HOME=` 认，不按名字 —— 同名的可能是别的路、主会话或用户自己的。套件收尾前问一句「这一趟起的都走了没」。
e2e_leftovers() {
  local want="HOME=$1" p pid
  for p in /proc/[0-9]*; do
    pid="${p#/proc/}"
    if [ "$pid" = "$$" ] || [ "$pid" = "$BASHPID" ]; then continue; fi
    grep -qzxF -- "$want" "$p/environ" 2>/dev/null || continue
    printf '%s %s\n' "$pid" "$(tr '\0' ' ' < "$p/cmdline" 2>/dev/null)"
  done
}
