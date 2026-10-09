//! 一次性远端 exec 与限长读行；远端后端在 shell 里的写法。

use super::*;
use tokio::io::AsyncBufReadExt;

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

/// backend 出方向单行的字节上限：一帧 = 一条 Claude jsonl 行。不等于 backend 入方向命令信封的 `inbound::MAX_LINE_BYTES`（1 MiB）—— 两者不是同一个量。
/// 真实的 jsonl 里最长一行接近 3 MiB、上百行超过 1 MiB；抄 1 MiB 会丢真实的行，用户还会看到一条「丢了帧」的健康提示。
/// 64 MiB 留足余量：帧内换行被后端转义成 `\n` 两字符（最坏接近翻倍），工具输出体量只会变大。
/// 超限语义：丢弃 + 带身份报告（`REMOTE_HEALTH` + `kind: "line_too_long"`，与 `overflow_health_message` 同一个出口），不许静默。
pub(crate) const BACKEND_FRAME_LINE_CAP: usize = 64 * 1024 * 1024;

/// 一次有界读行的结果。
#[derive(Debug)]
pub(crate) enum CappedLine {
    /// 读到一行（内容在 `buf` 里，**不含**行尾 `\n`；可能是 EOF 前的残行）。
    Line,
    /// 这一行超过 [`BACKEND_FRAME_LINE_CAP`]，**已整行丢弃**。
    /// 带上它到底有多少字节 —— 超限之后只数不存，所以这个数是准的而内存是 O(上限) 的。
    TooLong(u64),
    /// 对端关了写半边，且没有残行。
    Eof,
}

/// 按 `\n` 读一行，上限在读的时候生效：`fill_buf`/`consume`，超限之后只找换行、不再往 buf 里塞字节，整行的内存占用与行长无关
/// （读完再判的话，喂一条几百 MiB 无换行的流内存就跟着涨）。上限值与后端入方向不同、机制相同。
/// 不是 cancellation-safe：中途取消会丢掉 `overflowed` / 计数状态而 `buf` 里的半行留着 ⇒ 调用方要么放进独立 task，要么取消之后不再复用这个 reader。
/// `cap` 是参数：测试传一个小数，「超限之后内存不涨」直接判（`over_limit_stops_growing_the_buffer`）。
pub(crate) async fn read_capped_line<R>(
    rd: &mut R,
    buf: &mut Vec<u8>,
    cap: usize,
) -> std::io::Result<CappedLine>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    // `fill_buf`/`consume` 走文件顶部那条 `AsyncBufReadExt` 导入 ——
    // ⚠ 别在这里再本地 `use` 一次：本文件有一条判据把 `tokio::io` 导入清单钉死了
    // （`ALLOWED_IO_IMPORTS`，含反向锚点「放行清单不许留死行」），
    // 顶部那条一旦没人用就会被 `unused_imports` 逼着删，而删掉它那条判据当场红。
    buf.clear();
    let mut overflowed = false;
    let mut seen: u64 = 0;
    loop {
        let chunk = rd.fill_buf().await?;
        if chunk.is_empty() {
            // EOF。有残行就当一行交出去（`read_line` 旧行为逐字如此），否则报 EOF。
            return Ok(if seen == 0 {
                CappedLine::Eof
            } else if overflowed {
                CappedLine::TooLong(seen)
            } else {
                CappedLine::Line
            });
        }
        let (take, done) = match chunk.iter().position(|&c| c == b'\n') {
            Some(i) => (i, true),
            None => (chunk.len(), false),
        };
        seen += take as u64;
        if !overflowed {
            if buf.len() + take > cap {
                overflowed = true;
                buf.clear();
                buf.shrink_to_fit();
            } else {
                buf.extend_from_slice(&chunk[..take]);
            }
        }
        let consumed = if done { take + 1 } else { take };
        rd.consume(consumed);
        if done {
            return Ok(if overflowed {
                CappedLine::TooLong(seen)
            } else {
                CappedLine::Line
            });
        }
    }
}

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
