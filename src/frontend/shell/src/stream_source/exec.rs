//! 一次性远端 exec 与限长读行；远端后端在 shell 里的写法。

use super::*;
use tokio::io::AsyncBufReadExt;

// ═══════════ 拨号归后端：接远端后端的每一跳都只经拨号代理 ═══════════
//
// 〔墓碑 —— `K-P6b` 那一段原话的要点逐字：「买到的是：**`backend 那条长连接流` 的那一跳 SSH 握手，
//  可以不发生在界面进程里**」「**界面进程仍然自己拨号 —— 7 处里搬走的是 1 处**」「回落有两条……
//  ① 拿不到代理二进制 ② 配置里没填 `keyPath`」。〕
//
// C2 之后这三句都不成立了：拨号**全部**在后端的拨号代理里（`src/backend/dial/`）；界面这一侧拿链路的
// 唯一入口是宿主 `dial_host`，读应答的是通信层成员 `ssh_link`。两条回落都删了：
// ② 的根因（代理不会 ssh-agent）在代理那侧补上了；① 按 `D11`「后端是给定的，不要退路」—— 找不到本机后端
// 二进制就**报**，不再进程内拨。**唯一还在界面进程里拨的是 SFTP**（`F7c` 独占的 `sftp.rs`，
// 用的是 `inproc_dial.rs` 那一份搬来的旧实现），登记在 `dial_move_judge::DIAL_SITES`。

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

/// backend **出方向单行**的字节上限。
///
/// # ★ 这个数**刻意不等于** backend 侧的 `inbound::MAX_LINE_BYTES`（1 MiB）
///
/// 那一条限的是**入方向命令信封**（`stream/inbound/` 逐字「命令信封比 `ResumeSpec` 还小，
/// 1 MiB 已是极宽松的上限」）。本条限的是**出方向内容帧** —— 一帧 = 一条 Claude jsonl 行。
/// **两者不是同一个量**，抄过来就是把不同的东西按数字凑到一起
/// （`byte_cap_registry` 头注对账本 S3 那次订正逐字写过这句）。
///
/// 实测本机 `~/.claude/projects/**/*.jsonl` 全量 **525,132 行**：最长一行
/// **3,117,370 字节（2.97 MiB）**，其中 **78 行超过 1 MiB**
/// （> 1 MiB 且 ≤ 2 MiB 有 75 条，> 2 MiB 有 3 条）。
/// ⇒ 抄 1 MiB 会在本机丢掉 78 条**真实**行，而超限语义是「丢弃 + 报告」——
/// 用户会看到一条「丢了帧」的健康提示，而那不是拥塞，是我们自己把上限设小了。
///
/// 取 64 MiB = 实测最长行的 21 倍。留这么大余量的理由有两条：
/// 帧内换行被后端转义成 `\n` 两字符（最坏接近翻倍），以及工具输出体量只会变大。
///
/// # 超限语义：**丢弃 + 带身份报告**，不许静默
///
/// 走 `REMOTE_HEALTH` + `kind: "line_too_long"`，与 `overflow_health_message` 那条
/// 现成的路同一个出口（定框 E4：静默失败要给身份、且抬到调用方能判定的那一层）。
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

/// 按 `\n` 读一行，**上限在读的时候生效**。
///
/// # ★ 为什么不是 `read_line` 加一句长度判断
///
/// 那是后端侧栽过的坑，逐字记在 `src/backend/stream/inbound/mod.rs` 头注里：
/// 第一版用无界 `read_until`、读完再看长度，D 审计实测**喂 512 MiB 无换行的流 ⇒
/// RSS 从 6 MiB 涨到 518 MiB**，而它照样回了一条 `line_too_long`「看起来对」。
/// ⇒ 机制必须是 `fill_buf`/`consume`：超限之后**只找换行、不再往 buf 里塞字节**，
/// 整行的内存占用与行长无关。本函数是那段机制在 monitor 侧的同构实现
/// （**上限值不同、机制相同** —— 见 [`BACKEND_FRAME_LINE_CAP`] 头注）。
///
/// ⚠ **不是 cancellation-safe**：中途取消会丢掉 `overflowed`/计数状态，
/// 而 `buf` 里的半行留着。调用方要么把它放进独立 task（主帧读那样），
/// 要么取消之后就**不再复用这个 reader**（探测那两处那样）。
///
/// ★ `cap` **是参数而不是直接读常量**：生产调用点全传 [`BACKEND_FRAME_LINE_CAP`]，
/// 而测试要能传一个小数。否则「超限之后内存不涨」这条性质就只能靠量 RSS 来证
/// （backend 侧当年正是那么发现问题的），而**那种证法进不了单测**。
/// 传小 cap 之后同一条性质可以直接判：见 `over_limit_stops_growing_the_buffer`。
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
) -> Result<RemoteExec, String> {
    // 〔C2 → SR1a〕收全三样的活在本机常驻后端里（`use: capture`）；stdout/stderr 各自的上限照旧由这里给。
    crate::dial_host::capture(cfg, cmd, abort_marker, EXEC_CAPTURE_MAX_BYTES).await
}

// 这里原有 POSIX 单引号转调壳（转 `shell_quote_core::posix_quote`）：最后一个生产调用方（monitor 侧 Gate 1 前检，
//   THIN 第 3 件删）走了 ⇒ 一起删；要 quote 直调 `shell_quote_core::posix_quote`。

/// 有界读行的**行为**对拍。
///
/// # 为什么必须有这一组
///
/// 判据那半（`byte_cap_registry` 的四条）钉的全是**形态**：常量进表了没、
/// 处置臂说话了没。它们**判不出**「上限到底生不生效」——
/// 而后端侧栽的那次正是这个缺口：上限写着 1 MiB、`line_too_long` 也回了，
/// 可它是**读完再判**，实测 RSS 从 6 MiB 涨到 518 MiB。头注逐字记着
/// 「常数抄了先例，**机制没抄**」。
///
/// ⇒ 本组直接喂 reader，用**小 cap**，把那条「超限之后内存不涨」判成断言。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/capped_line_tests.rs"]
mod capped_line_tests;
