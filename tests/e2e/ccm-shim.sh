# shellcheck shell=bash
# 起会话交给终端的只是一行 `ccm …` ⇒ 跑那一行的 e2e 要一个叫 `ccm` 的后端二进制在 PATH 上。
#
# 用法（在 tmux-shim.sh 之后、本趟第一条 tmux 命令之前 source；私有 tmux server 起来时的 PATH 就是 pane 里的 PATH）：
#   # shellcheck source=tests/e2e/ccm-shim.sh
#   . "$(cd "$(dirname "$0")" && pwd)/ccm-shim.sh"
#
# 只链不编：二进制由门禁 / CI 先 `cargo build`（`src/backend` 下）。找不到 ⇒ 响亮退出，不回落到 PATH 上碰巧有的那一份。
# 隔离：账号库 · 凭据表都指向本趟的临时文件（不读这台机器的真账号库与真凭据）；不等信任框（`CCM_NO_PRETRUST=1`）。

_ccm_shim_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)" || exit 2
CCM_E2E_BIN="${CCM_E2E_BACKEND_BIN:-${CARGO_TARGET_DIR:-$_ccm_shim_root/.build/backend}/debug/cc-monitor-backend}"
[ -x "$CCM_E2E_BIN" ] || {
  echo "找不到后端二进制 $CCM_E2E_BIN —— 先 \`cd src/backend && cargo build\`" >&2
  exit 2
}
CCM_SHIM_DIR="$(mktemp -d /tmp/e2e-ccm-shim.XXXXXX)"
ln -s "$CCM_E2E_BIN" "$CCM_SHIM_DIR/ccm"
export PATH="$CCM_SHIM_DIR:$PATH"
export CCM_ACCTS_MANIFEST="${CCM_ACCTS_MANIFEST:-$CCM_SHIM_DIR/accounts.json}"
export CCM_APIKEY_CREDENTIALS="$CCM_SHIM_DIR/apikey-credentials.json"
export CCM_NO_PRETRUST=1

ccm_shim_cleanup() { rm -rf "$CCM_SHIM_DIR"; }
