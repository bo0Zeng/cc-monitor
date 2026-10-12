//! 一次性远端 exec 与限长读行；远端后端在 shell 里的写法。

use super::*;

//
// ═══════════ 拨号归后端：接远端后端的每一跳都只经拨号代理 ═══════════
//
// 拨号全部在后端的拨号代理里（`src/backend/dial/`）；界面这一侧拿链路的唯一入口是宿主 `dial_host`，读应答的是通信层成员 `ssh_link`。
// 没有回落：找不到本机后端二进制就报（`D11`），不在进程内拨（`dial_move_judge::DIAL_SITES` 登记着拨号点）。

/// 远端后端在 shell 里的写法：恒是那台的 `~/.cc-monitor/bin/ccm`（后端二进制本身），
/// 可填的 `backendPath` 删了。常量一份住 `relay_route_core`（后端往远端拼命令也读它）。
pub(crate) const BACKEND_CMD: &str = relay_route_core::BACKEND_LANDING_SHELL;

// 一次性远端命令只有 [`connect_and_exec_capture`]（收全、有上限、带退出码）。

/// 一次远端 exec 的**完整**结果：stdout、stderr、退出码。
///
/// # 为什么需要它（而不是继续用 `connect_and_exec_cmd`）
///
/// `Channel::into_stream()` 只搬 `ChannelMsg::Data` —— **`ExtendedData`（= stderr）
/// 与 `ExitStatus` 都被丢掉**（russh 0.61 `channels/io/mod.rs`）。所以既有的
/// 只读 stdout 的路，远端命令失败时读到 0 行，与「查询成功但结果为空」**在类型上不可区分**。
///
/// 列举类查询忍得了（空结果本来就合法），但 `--fork-session` 忍不了 ——
/// 分叉失败必须让用户看见原因，而后端恰恰把原因写在 **stderr + exit 2** 上。
/// 所以这里直接驱动 `channel.wait()` 收全三样。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteExec {
    pub stdout: String,
    pub stderr: String,
    /// `None` = 远端没送 exit-status（连接被掐 / 服务端不守规矩）。
    /// **不许把 `None` 当成 0** —— 那正好会把「没跑成」读成「跑成了」。
    pub exit_status: Option<u32>,
}

/// stdout/stderr 各自的收集上限。查询类输出都是一行 JSON 量级；
/// 上限只是防「远端吐无穷字节」吃爆内存，正常路径远够不到。
const EXEC_CAPTURE_MAX_BYTES: usize = 4 * 1024 * 1024;

// 出方向单行的上限（`BACKEND_FRAME_LINE_CAP`）与有界读行（`read_capped_line` · `CappedLine`）住 `resident-client`（手机同一份），这里转用。
pub(crate) use resident_client::line::{read_capped_line, CappedLine, BACKEND_FRAME_LINE_CAP};

/// exec 一条命令并**收全** stdout / stderr / 退出码（见 [`RemoteExec`]）。
///
/// 与 `connect_and_exec_cmd` 一样每次独立连接（一次性查询语义），
/// 不影响长连接流路径。超时由调用方套 `tokio::time::timeout`。
///
/// `abort_marker`：stdout 里一出现这个子串就**立刻收工返回**。存在的理由只有一个 ——
/// **不认参数的旧后端会掉进流模式**（长连接、永不 EOF）。老老实实收到通道关闭，
/// 就只能等调用方的超时兜底，而超时会把「backend 版本过旧」这条最有用的诊断吞成
/// 一句「超时」。给调用方一个字符串就能提前抽身。`None` = 收到底。
pub async fn connect_and_exec_capture(
    cfg: &RemoteConfig,
    cmd: &str,
    abort_marker: Option<&str>,
) -> Result<RemoteExec, crate::detail::Said> {
    // 收全三样的活在本机常驻后端里（`use: capture`）；stdout / stderr 各自的上限由这里给。
    crate::dial_host::capture(cfg, cmd, abort_marker, EXEC_CAPTURE_MAX_BYTES).await
}

// 这里原有 POSIX 单引号转调壳（转 `shell_quote_core::posix_quote`）：最后一个生产调用方（monitor 侧 Gate 1 前检，
//   THIN 第 3 件删）走了 ⇒ 一起删；要 quote 直调 `shell_quote_core::posix_quote`。

/// 有界读行的行为对拍：`byte_cap_registry` 那几条钉的是形态（常量进表了没、处置臂说话了没），判不出上限到底生不生效 ⇒ 本组用小 cap 直接喂 reader。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/capped_line_tests.rs"]
mod capped_line_tests;
