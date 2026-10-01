# shellcheck shell=bash
# 起会话交给终端的只是一行 `ccm …` ⇒ 跑那一行的 e2e 要一个叫 `ccm` 的后端二进制在 PATH 上。
#
# 用法（在 tmux-shim.sh 之后、本趟第一条 tmux 命令之前 source；私有 tmux server 起来时的环境就是 pane 里的环境）：
#   # shellcheck source=tests/e2e/ccm-shim.sh
#   . "$(cd "$(dirname "$0")" && pwd)/ccm-shim.sh"
#
# 只链不编：二进制由门禁 / CI 先 `cargo build`（`src/backend` 下）。找不到 ⇒ 响亮退出，不回落到 PATH 上碰巧有的那一份。
#
# 🔴 隔离一律**无条件**指向本趟沙箱，不继承开发机上的值：开发者的 shell rc 里常 export 着指向真账号库的
#   `CCM_ACCTS_MANIFEST`，`${…:-沙箱}` 那种写法会照用它、再往里写测试号 ⇒ 真账号清单被改写（实发过一次）。
#   ccm 会读 / 写的家目录、账号库、凭据表、cc-bus 目录都换成沙箱；开发机的账号目录变量摘掉。

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
mkdir -p "$HOME"
export CCM_ACCTS_MANIFEST="$CCM_SHIM_DIR/accounts.json"
export CCM_APIKEY_CREDENTIALS="$CCM_SHIM_DIR/apikey-credentials.json"
export CC_BUS_HOME="$CCM_SHIM_DIR/cc-bus"
export CCM_NO_PRETRUST=1
unset CLAUDE_CONFIG_DIR CC_BUS_SCRIPTS CCM_CONFIG CCM_ENV CCM_LAUNCH_ID CCM_RBIND_TOKEN \
  ANTHROPIC_BASE_URL CCM_RELAY_PORT CCM_RELAY_ALL_SESSIONS

ccm_shim_cleanup() { rm -rf "$CCM_SHIM_DIR"; }
