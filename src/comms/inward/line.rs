//! 带上限的异步读：读一行（[`read_capped_line`]）· 读整份（[`read_capped_all`]）。
//! 后端 · 常驻客户端 · 壳 · 手机异步地读对端的字节只经这一份；同步那一形住 `relay_route_core`（[`relay_route_core::read_capped_from`]）。
//!
//! 上限在读的时候生效：超了之后不再往缓冲里塞字节（读完再判的话，喂一条几百 MiB 无换行的流内存就跟着涨）。
//! 超了一律回「超限」，不截断：截下前 `cap.bytes` 字节当一行 · 一份用，正是上限要挡的。超了之后怎么办（拒收 · 丢弃并报告 …）
//! 是调用方照 `cap.over` 办的事，这里只把「超了」交出去。
//! 只用 `fill_buf` / `consume` 与一次次的 `read`：tokio 的整读（`read_line` · `read_until` · `read_to_end` · `read_to_string`）
//! 先读完再判，这里不用。

use relay_route_core::Cap;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt};

/// 一次带上限读一行的结果。
#[derive(Debug)]
pub enum CappedLine {
    /// 读到一行（内容在 `buf` 里，不含行尾 `\n`；可能是流尽前没换行的残行）。
    Line,
    /// 这一行超过 `cap.bytes`，已整行丢弃（`buf` 清空）。带上它的真实字节数：超了之后只数不存，数是准的、内存只到上限。
    TooLong(u64),
    /// 对端关了写半边，且没有残行。
    Eof,
}

/// 按 `\n` 读一行，至多 `cap.bytes` 字节（不含 `\n`）。超了的那一行读到换行为止整行丢掉，下一次调用接着读下一行。
/// 不是 cancellation-safe：中途取消会丢掉计数状态而 `buf` 里的半行留着 ⇒ 调用方要么放进独立 task，要么取消之后不再复用这个 reader。
pub async fn read_capped_line<R>(
    rd: &mut R,
    buf: &mut Vec<u8>,
    cap: Cap,
) -> std::io::Result<CappedLine>
where
    R: AsyncBufRead + Unpin,
{
    buf.clear();
    let mut overflowed = false;
    let mut seen: u64 = 0;
    loop {
        let chunk = rd.fill_buf().await?;
        if chunk.is_empty() {
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
            if seen > cap.bytes {
                overflowed = true;
                buf.clear();
                buf.shrink_to_fit();
            } else {
                buf.extend_from_slice(&chunk[..take]);
            }
        }
        rd.consume(if done { take + 1 } else { take });
        if done {
            return Ok(if overflowed {
                CappedLine::TooLong(seen)
            } else {
                CappedLine::Line
            });
        }
    }
}

/// 一次带上限读整份的结果：与同步那一形（`relay_route_core::read_capped_from`）同一个类型。
pub use relay_route_core::CappedAll;

/// 读到流尽，至多 `cap.bytes` 字节。多读一个字节分得出「正好到上限」与「超了」。
pub async fn read_capped_all<R>(mut rd: R, cap: Cap) -> std::io::Result<CappedAll>
where
    R: AsyncRead + Unpin,
{
    let mut out = Vec::new();
    let mut chunk = [0u8; 8 * 1024];
    loop {
        let room = (cap.bytes + 1).saturating_sub(out.len() as u64);
        let want = chunk.len().min(usize::try_from(room).unwrap_or(usize::MAX));
        let n = rd.read(&mut chunk[..want]).await?;
        if n == 0 {
            return Ok(CappedAll::All(out));
        }
        out.extend_from_slice(&chunk[..n]);
        if out.len() as u64 > cap.bytes {
            return Ok(CappedAll::OverCap);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/comms/inward/line_tests.rs"]
mod tests;
