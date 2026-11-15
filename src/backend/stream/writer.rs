//! 一条流的写者：应答 · 出方向 · tap 三路排空，一帧一行写进那一端（stdio 那条载体是 stdout，常驻那条是套接字的写半边）。
//! 两条载体都由 `stream/entry.rs` 起它；优先序与 flush 的口径只在这里一份。

use crate::common::time::Tz;
use crate::observe;
use crate::stream::inbound::views::{to_line_viewed, StreamView};
use crate::stream::tap;
use crate::wire::{Frame, FrameExit as _};
use tokio::io::AsyncWriteExt;

/// 写者为什么停（一条流收场照它分，`stream/entry.rs::stream_ends`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriterEnd {
    /// 对端走了：写 · flush 报对端已断（断管 · 被重置 · 中止 · 没连着）。入方向随即读到 EOF。
    PeerGone,
    /// 别的缘故停的（出方向通道关了 · 一帧写不出 · 写报了别的错）：对端可能还连着。
    Closed,
}

/// 写出错 ⇒ 写者为什么停。
fn end_of(e: &std::io::Error) -> WriterEnd {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::BrokenPipe | K::ConnectionReset | K::ConnectionAborted | K::NotConnected => {
            WriterEnd::PeerGone
        }
        _ => WriterEnd::Closed,
    }
}

/// 写者自上次 flush 起写满这么多字节就照 flush 一次：帧一直排着时首帧的延迟有界。
pub const WRITE_FLUSH_BYTES: usize = 64 * 1024;

/// The stdout writer half of the §5.4 split: drain frames and write one wire
/// line each, flushing whenever nothing is queued so a connected client sees them promptly.
///
/// Awaiting `recv()` here is what back-pressures the bounded channel when the
/// SSH pipe is slow; that back-pressure never reaches the inotify reader.
/// 排空两条通道写 stdout。
///
/// **`biased` + 应答在前**：出方向的实时行可以有洪峰（通道按字节封顶 `CHANNEL_MAX_BYTES`），
/// 公平轮询会让应答排在成千上万行后面，客户端那头看起来就是「命令没反应」。
/// 应答量极小（一条命令一两帧），优先它不会饿死行。
///
/// `reply_rx` 返回 `None` ＝ 应答那一路没人再送了（常驻那条连接读到 EOF、在飞的命令都交了应答，`stream/entry.rs::stream_ends`）
/// ⇒ 同出方向关了一样收尾：排着的写完、flush、返回。stdio 那条起它的那一处自己留着一个 sender，那里不会回 `None`。
///
/// `tz` ＝ 这条流看的那一台的时区（起流的 `--tz` / attach 行的 `tz`）：每一帧写出去之前按它写钟面（[`crate::wire::FrameExit::stamp`]）。
/// 带钟面的那几种帧（`line` · `session_runs`）只走出方向；应答 · tap 里的帧没有钟面可写，原样出。
pub async fn writer_task<W: tokio::io::AsyncWrite + Unpin>(
    mut out: W,
    mut rx: observe::watcher::FrameRx,
    mut reply_rx: tokio::sync::mpsc::Receiver<Frame>,
    mut tap_rx: impl tap::TapSource,
    tz: Tz,
    view: Option<StreamView>,
) -> WriterEnd {
    // ★ 应答优先，但**有预算**。
    //
    // 第一版是无条件 `biased` + 应答在前，注释写着「应答量极小，优先它不会饿死行」——
    // **那个前提由不可信输入决定，不成立**。D 审计端到端实测（客户端只是往 stdin 灌
    // `{"id":"n","cmd":"n"}`）：500ms 内应答 70 789 条、出方向实时行 **4 条**，队列里一直排着。
    //
    // 机理是闭环的：读循环在应答通道满时阻塞（`send` 是 `.await`），于是通道**恒满**；
    // `biased` 每次都命中 `reply_rx`，`rx` 永远轮不到。生产里出方向按字节封顶，
    // 会先堆满再 `Overflow` 丢实时行 —— 一行命令就能让远端会话看起来「卡住」。
    //
    // 现在：连发 `REPLY_BURST` 条应答之后**强制让位一次**给出方向。
    // 仍然偏向应答（正常场景一条命令一两帧，够不到预算），但偏置是**有界**的。
    const REPLY_BURST: u32 = 8;
    // 另一道：自上一帧出方向起写出去的应答**字节**到了一页（`REPLY_PAGE_BYTES`）⇒ 也让位一次。
    //   分页取一份大会话（index · turns …）时应答是一页接一页来的；按条数，那台别的会话的实时行要等 8 页（几 MB）之后；
    //   按字节，至多再等一页 —— 「大应答分页取到一半，别的会话来了一行，它在下一页之前上线」就靠这一道（8a b）。
    let mut burst = 0u32;
    let mut burst_bytes = 0usize;
    // flush 只在三路此刻都没有排着的时候做（排空了再一次），另有 `WRITE_FLUSH_BYTES` 封顶；不用计时器。
    //   一帧一 flush 时，分页应答与成批的行是一帧一次写系统调用。开了 deflate，每帧一个压缩上下文由 `wire::listen::Down` 按行切，不靠这里 flush。
    let mut unflushed = 0usize;
    // 收场（`wire::drain::exit_after_drain`）等写者都返回才退：这里登记，返回时落。叫收尾了 ⇒ 走下面「出方向关了」那条路
    //   （排着的应答写完、flush、返回）；它排在三路之前问，出方向一直有行来也拖不住收尾。
    let _slot = crate::wire::drain::DRAIN.writer();
    let finishing = crate::wire::drain::DRAIN.finishing();
    tokio::pin!(finishing);
    loop {
        let picked = {
            // 活卡那一路**排在最后**（`biased` 按书写顺序问）：只有出方向与应答此刻都没有东西时才轮到它。
            //   它可丢（SSE 只保快，jsonl 保对），而出方向里的 `line` 帧不许因为它晚到或被挤掉 —— 它走自己那个
            //   有界发件箱（`live::LIVE_OUTBOX_CAP`），满了合成整卡。
            let pick = async {
                if burst < REPLY_BURST && burst_bytes < crate::wire::REPLY_PAGE_BYTES.as_len() {
                    tokio::select! {
                        biased;
                        () = &mut finishing => None,
                        f = reply_rx.recv() => f.map(|f| { burst += 1; (f, true) }),
                        f = rx.recv() => f.map(|f| (f, false)),
                        Some(f) = tap_rx.next() => Some((f, false)),
                    }
                } else {
                    burst = 0;
                    burst_bytes = 0;
                    tokio::select! {
                        biased;
                        () = &mut finishing => None,
                        f = rx.recv() => f.map(|f| (f, false)),
                        f = reply_rx.recv() => f.map(|f| (f, true)),
                        Some(f) = tap_rx.next() => Some((f, false)),
                    }
                }
            };
            tokio::pin!(pick);
            if unflushed == 0 {
                pick.await
            } else {
                // 先问一下三路；此刻一路都没有 ⇒ 写出去的先 flush，再接着等**同一个** `pick`（没取消，什么都不丢）。
                tokio::select! {
                    biased;
                    p = &mut pick => p,
                    () = std::future::ready(()) => {
                        if let Err(end) = flush_out(&mut out).await {
                            return end;
                        }
                        unflushed = 0;
                        pick.await
                    }
                }
            }
        };
        let Some((frame, reply)) = picked else {
            // 出方向通道关了 · 应答那一路没人再送 · 收场叫收尾了 = 寿终：应答那一路此刻排着的照写完（对端在等它们；预算用完那一轮先问的是出方向，
            //   它关了不等于写端断了），攒着没 flush 的再一起发出去。写端本身断了 ⇒ `put` 当场交回停因。
            //   先关上应答那一路的收件口：此刻排着的（连已拿到名额、还没落进队的）照样取得出，新的送不进 ——
            //   读循环一直在送也拖不住收尾；取完了 `recv` 回 `None`。
            reply_rx.close();
            while let Some(f) = reply_rx.recv().await {
                if let Err(end) = put(&mut out, f, &tz, view.as_ref(), &mut unflushed).await {
                    return end;
                }
            }
            return flush_out(&mut out).await.err().unwrap_or(WriterEnd::Closed);
        };
        let wrote = match put(&mut out, frame, &tz, view.as_ref(), &mut unflushed).await {
            Ok(n) => n,
            Err(end) => return end,
        };
        if reply {
            burst_bytes += wrote;
        } else {
            burst = 0;
            burst_bytes = 0;
        }
    }
}

/// 写一帧、记进攒着没 flush 的字节，攒满 `WRITE_FLUSH_BYTES` 就 flush 一次。回：这一帧写了几个字节；
/// 写或 flush 失败 ⇒ 记一行、交回写者为什么停（[`end_of`]）。
async fn put<W: tokio::io::AsyncWrite + Unpin>(
    out: &mut W,
    frame: Frame,
    tz: &Tz,
    view: Option<&StreamView>,
    unflushed: &mut usize,
) -> Result<usize, WriterEnd> {
    let wrote = write_frame(out, frame, tz, view).await.map_err(|e| {
        // Broken pipe (client gone) is the normal end-of-life; stop quietly.
        tracing::warn!("stdout write failed ({e}); stopping writer");
        end_of(&e)
    })?;
    *unflushed += wrote;
    if *unflushed >= WRITE_FLUSH_BYTES {
        flush_out(out).await?;
        *unflushed = 0;
    }
    Ok(wrote)
}

/// flush 写者那一端；失败 ⇒ 记一行、交回写者为什么停（[`end_of`]），写者就此停。
async fn flush_out<W: tokio::io::AsyncWrite + Unpin>(out: &mut W) -> Result<(), WriterEnd> {
    out.flush().await.map_err(|e| {
        tracing::warn!("stdout flush failed ({e}); stopping writer");
        end_of(&e)
    })
}

/// Serialize one frame to its wire line and write it (no flush).
/// 回：写了几个字节。
/// 先按这条流看的那一台的时区写钟面、交出线上那一形（[`crate::wire::FrameExit::stamp`]；只给后端看的那几格就此丢掉，序列化只在这之后）；
/// 复制详情里那一行时刻的空位按同一个时区填好（应答 · 推送都过这里；`common::time::fill_at`）；
/// 推送帧照这条流的出口声明裁格（`view`；应答的 kind 不在声明的住处表里，原样）。
async fn write_frame<W: tokio::io::AsyncWrite + Unpin>(
    out: &mut W,
    frame: Frame,
    tz: &Tz,
    view: Option<&StreamView>,
) -> std::io::Result<usize> {
    let line = to_line_viewed(&frame.stamp(tz), view)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let line = crate::common::time::fill_at(&line, tz);
    out.write_all(line.as_bytes()).await?;
    Ok(line.len())
}

#[cfg(test)]
#[path = "../../../tests/backend/writer_task_tests.rs"]
mod tests; // 写者的优先序：tap 灌满时内容帧一条不少、顺序不变
