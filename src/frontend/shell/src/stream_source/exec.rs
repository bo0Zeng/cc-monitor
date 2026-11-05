//! 收帧用的限长读行。
//
// 界面进程不拨 SSH（`dial_move_judge` 钉着）：一次性远端命令都在本机常驻后端里跑，问那台经本机客户端 `call_at`。
// 远端后端在 shell 里的写法（`~/.cc-monitor/bin/ccm`）只住 `relay_route_core`：往那台拼命令的都在后端（停 · 装 · 接常驻）。
// 一次 exec 的完整结果是 `cmd_wire::dial::Captured`（本机那一趟 `--resident-stop` 同一形，`local_backend_host::local_captured`）。

// 有界读行（`read_capped_line` · `CappedLine`）住 `comms_inward::line`，出方向单行的上限（`BACKEND_FRAME_LINE_CAP`）住 `resident-client`（手机同一份），这里转用。
pub(crate) use comms_inward::line::{read_capped_line, CappedLine};
pub(crate) use resident_client::line::BACKEND_FRAME_LINE_CAP;

/// 收帧那几臂的源码判据（P8c）；有界读行的行为对拍住 `tests/comms/inward/line_tests.rs`。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/capped_line_tests.rs"]
mod capped_line_tests;
