# shellcheck shell=bash
# 起会话交给终端的只是一行 `ccm …` ⇒ 跑那一行的 e2e 要一个叫 `ccm` 的后端二进制在 PATH 上。
#
# 用法（在 tmux-shim.sh 之后、本趟第一条 tmux 命令之前 source；私有 tmux server 起来时的环境就是 pane 里的环境）：
#   # shellcheck source=tests/e2e/ccm-shim.sh
#   . "$(cd "$(dirname "$0")" && pwd)/ccm-shim.sh"
#
# 只链不编：二进制由门禁 / CI 先 `cargo build`（`src/backend` 下）。找不到 ⇒ 响亮退出，不回落到 PATH 上碰巧有的那一份。
#
# 🔴 隔离一律**无条件**指向本趟沙箱，不继承开发机上的值：`${…:-沙箱}` 那种写法在开发机上会照用 shell rc 里的真值、
#   再往里写测试数据（实发过一次：真账号清单被改写）。ccm 会读 / 写的家目录（账号库就在它底下
#   `~/.cc-monitor/accounts/`）、凭据表、cc-bus 目录都换成沙箱；开发机的账号目录变量摘掉。

_ccm_shim_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)" || exit 2
CCM_E2E_BIN="${CARGO_TARGET_DIR:-$_ccm_shim_root/.build/backend}/debug/cc-monitor-backend"
[ -x "$CCM_E2E_BIN" ] || {
  echo "找不到后端二进制 $CCM_E2E_BIN —— 先 \`cd src/backend && cargo build\`" >&2
  exit 2
}
CCM_SHIM_DIR="$(mktemp -d /tmp/e2e-ccm-shim.XXXXXX)"
ln -s "$CCM_E2E_BIN" "$CCM_SHIM_DIR/ccm"
export PATH="$CCM_SHIM_DIR:$PATH"
# 换家目录之前先把工具链钉住（取串那一跳要跑 cargo；工具链只读、与被测的账号 / 会话数据无关）。
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export HOME="$CCM_SHIM_DIR/home"
# 账号库只跟着家走：本趟的那一份就是沙箱家目录底下这一份（写它的套件先核它在沙箱里）。
CCM_SHIM_ACCOUNTS="$HOME/.cc-monitor/accounts"
mkdir -p "$CCM_SHIM_ACCOUNTS"
# 后端在真部署里的落点（起会话那一行直接叫它的绝对路径，不靠 PATH）：沙箱家里也放一份。
CCM_ENTRY="$HOME/.cc-monitor/bin/ccm"
mkdir -p "$HOME/.cc-monitor/bin"
ln -s "$CCM_E2E_BIN" "$CCM_ENTRY"
export CCM_ENTRY
export CC_BUS_HOME="$CCM_SHIM_DIR/cc-bus"
unset CLAUDE_CONFIG_DIR CC_BUS_SCRIPTS CCM_ENV \
  ANTHROPIC_BASE_URL CCM_RELAY_PORT CCM_RELAY_ALL_SESSIONS

ccm_shim_cleanup() { rm -rf "$CCM_SHIM_DIR"; }
