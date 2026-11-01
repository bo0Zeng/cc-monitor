//! 按行读帧：出方向单行的上限与有界读行 · 握完手的读半边（原样 / 那台认了 deflate 就边读边解）。桌面 monitor 与手机核心同一份。

use std::pin::Pin;
use std::task::{ready, Context, Poll};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, BufReader, ReadBuf};

/// 握完手的读半边：原样，或那台在 attach 回话里认了 deflate ⇒ 回话那一行之后的字节整条是一个 raw deflate 流，边读边解。
/// 对外都是 `AsyncBufRead`：读行（[`read_capped_line`]）· 心跳数字节那几层不分两种。
pub enum Downstream<R> {
    Plain(BufReader<R>),
    Deflate(BufReader<Inflate<BufReader<R>>>),
}

impl<R: AsyncRead + Unpin> Downstream<R> {
    /// `r` 里已经读进来、还没交出去的字节（紧跟回话那一行的）照样算进流里。
    pub fn new(r: BufReader<R>, deflate: bool) -> Self {
        if deflate {
            Downstream::Deflate(BufReader::new(Inflate::new(r)))
        } else {
            Downstream::Plain(r)
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for Downstream<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        out: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Downstream::Plain(r) => Pin::new(r).poll_read(cx, out),
            Downstream::Deflate(r) => Pin::new(r).poll_read(cx, out),
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncBufRead for Downstream<R> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
        match self.get_mut() {
            Downstream::Plain(r) => Pin::new(r).poll_fill_buf(cx),
            Downstream::Deflate(r) => Pin::new(r).poll_fill_buf(cx),
        }
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        match self.get_mut() {
            Downstream::Plain(r) => Pin::new(r).consume(amt),
            Downstream::Deflate(r) => Pin::new(r).consume(amt),
        }
    }
}

/// 解一个 raw deflate 流（那台每帧 sync flush ⇒ 每帧一到就解得出来）。对端关了 ⇒ 到头（流本来就不收尾）；解不开 ⇒ `InvalidData`。
pub struct Inflate<R> {
    inner: R,
    z: flate2::Decompress,
    /// 上一次是把 `out` 填满才停的 ⇒ 解压器手里可能还压着已经吃进去的输入解出的字节（停在一段回指拷贝的半中间）。
    /// 这时先不等网络、空输入再解一次把它们吐出来：一帧的压缩字节已经全到、那台随后静下来时，这一帧当场整行交出。
    backlog: bool,
}

impl<R> Inflate<R> {
    pub fn new(inner: R) -> Self {
        Inflate {
            inner,
            z: flate2::Decompress::new(false),
            backlog: false,
        }
    }
}

impl<R: AsyncBufRead + Unpin> AsyncRead for Inflate<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        out: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if out.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if this.backlog {
            let was_out = this.z.total_out();
            let space = out.remaining();
            // 空输入：只吐压着的；解不开的错留给下一批真输入去报。
            let _ = this.z.decompress(
                &[],
                out.initialize_unfilled(),
                flate2::FlushDecompress::None,
            );
            let made = (this.z.total_out() - was_out) as usize;
            out.advance(made);
            this.backlog = made == space;
            if made > 0 {
                return Poll::Ready(Ok(()));
            }
        }
        loop {
            let input = ready!(Pin::new(&mut this.inner).poll_fill_buf(cx))?;
            let eof = input.is_empty();
            let (was_in, was_out) = (this.z.total_in(), this.z.total_out());
            let space = out.remaining();
            let status = this.z.decompress(
                input,
                out.initialize_unfilled(),
                flate2::FlushDecompress::None,
            );
            let used = (this.z.total_in() - was_in) as usize;
            let made = (this.z.total_out() - was_out) as usize;
            Pin::new(&mut this.inner).consume(used);
            out.advance(made);
            this.backlog = made == space;
            match status {
                Err(e) if !eof => {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        e,
                    )))
                }
                _ if made > 0 || eof => return Poll::Ready(Ok(())),
                Ok(flate2::Status::StreamEnd) => return Poll::Ready(Ok(())),
                _ if used == 0 => {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "deflate stream made no progress",
                    )))
                }
                // 吃进去了、还没解出字节（一段 sync flush 的空块）⇒ 接着读。
                _ => {}
            }
        }
    }
}

/// backend 出方向单行的字节上限：一帧 = 一条 Claude jsonl 行。不等于 backend 入方向命令信封的 `inbound::MAX_LINE_BYTES`（1 MiB）—— 两者不是同一个量。
/// 真实的 jsonl 里最长一行接近 3 MiB、上百行超过 1 MiB；抄 1 MiB 会丢真实的行，用户还会看到一条「丢了帧」的健康提示。
/// 64 MiB 留足余量：帧内换行被后端转义成 `\n` 两字符（最坏接近翻倍），工具输出体量只会变大。
/// 超限语义：丢弃 + 带身份报告（`REMOTE_HEALTH` + `kind: "line_too_long"`，与拥塞那一句同一个出口），不许静默。
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

#[cfg(test)]
#[path = "../../../tests/comms/resident-client/line_tests.rs"]
mod tests;
