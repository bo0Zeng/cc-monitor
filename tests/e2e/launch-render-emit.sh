#!/usr/bin/env bash
# 生产的那一行 `ccm …`（后端帧命令 `launch-render-cli`）的真输出 —— e2e 取「app 真正会跑的那一行」的 Rust 那一跳。
#
# 调用方：`tests/e2e/launch-render-driver.ts`（它用生产 TS 的 `plan*` ＋ `buildCliRenderRequest`
# 产请求 JSON，放进环境变量 `CCM_E2E_RENDER_REQ` 再调本脚本）。本脚本只做一件事：跑那个
# `#[ignore]` 数据出口（`launch_cli_parity_tests.rs::emit_launch_render_for_e2e`），
# 把它吐的那一行标记原样转出来：`LAUNCH_RENDER<<<命令>>>` 或 `LAUNCH_RENDER_ERR<<<理由>>>`。
#
# 「没输出」不是绿：编译失败、过滤器打错、`--ignored` 拼错都会得到 0 个测试、退出码 0 ——
# 所以取不到标记行就以 2 退出，并把 cargo 的尾巴打到 stderr。
set -uo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
[ -n "${CCM_E2E_RENDER_REQ:-}" ] || { echo "缺 CCM_E2E_RENDER_REQ（请求 JSON）" >&2; exit 2; }
# 渲染住在后端（`src/backend/control/launch_render/`）⇒ 在后端那个工程里跑出口。
OUT="$(cd "$REPO/src/backend" && cargo test --lib -- --ignored --nocapture emit_launch_render_for_e2e 2>&1)"
LINE="$(printf '%s\n' "$OUT" | grep -E 'LAUNCH_RENDER(_ERR)?<<<' | head -1)"
if [ -z "$LINE" ]; then
  printf '%s\n' "$OUT" | tail -40 >&2
  echo "取不到生产渲染器的输出 —— e2e 无对象可跑（别当绿过）" >&2
  exit 2
fi
printf '%s\n' "$LINE"
