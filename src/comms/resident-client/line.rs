//! 按行读帧：出方向单行的上限与有界读行（桌面 monitor 与手机核心同一份）。

use tokio::io::AsyncBufReadExt;

/// backend 出方向单行的字节上限：一帧 = 一条 Claude jsonl 行。不等于 backend 入方向命令信封的 `inbound::MAX_LINE_BYTES`（1 MiB）—— 两者不是同一个量。
/// 真实的 jsonl 里最长一行接近 3 MiB、上百行超过 1 MiB；抄 1 MiB 会丢真实的行，用户还会看到一条「丢了帧」的健康提示。
/// 64 MiB 留足余量：帧内换行被后端转义成 `\n` 两字符（最坏接近翻倍），工具输出体量只会变大。
/// 超限语义：丢弃 + 带身份报告（`REMOTE_HEALTH` + `kind: "line_too_long"`，与 `overflow_health_message` 同一个出口），不许静默。
pub const BACKEND_FRAME_LINE_CAP: usize = 64 * 1024 * 1024;

/// 一次有界读行的结果。
#[derive(Debug)]
pub enum CappedLine {
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
pub async fn read_capped_line<R>(
    rd: &mut R,
    buf: &mut Vec<u8>,
    cap: usize,
) -> std::io::Result<CappedLine>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    // `fill_buf`/`consume` 走文件顶部那条 `AsyncBufReadExt` 导入。
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
