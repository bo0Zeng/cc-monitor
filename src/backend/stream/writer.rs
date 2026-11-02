//! 一条流的写者：应答 · 出方向 · tap 三路排空，一帧一行写进那一端（stdio 那条载体是 stdout，常驻那条是套接字的写半边）。
//! 两条载体都由 `main.rs` 起它；优先序与 flush 的口径只在这里一份。

use crate::observe;
use crate::stream::inbound::views::{to_line_viewed, StreamView};
use crate::stream::tap;
use crate::wire::{Frame, FrameRules as _};
use crate::Tz;
use tokio::io::AsyncWriteExt;

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
/// `reply_rx` 永远不会返回 `None`（`main` 自己留着一个 sender 到进程结束），
/// 所以这个 select 不会退化成 `None` 忙转。
///
/// `tz` ＝ 这条流看的那一台的时区（起流的 `--tz` / attach 行的 `tz`）：出方向的每一帧写出去之前按它写钟面（[`Frame::stamp`]）。
pub async fn writer_task<W: tokio::io::AsyncWrite + Unpin>(
    mut out: W,
    mut rx: observe::watcher::FrameRx,
    mut reply_rx: tokio::sync::mpsc::Receiver<Frame>,
    mut tap_rx: impl tap::TapSource,
    tz: Tz,
    view: Option<StreamView>,
) {
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
    //   一帧一 flush 时，分页应答与成批的行是一帧一次写系统调用，开了 deflate 还一帧一次 sync flush。
    let mut unflushed = 0usize;
    loop {
        let picked = {
            // 活卡那一路**排在最后**（`biased` 按书写顺序问）：只有出方向与应答此刻都没有东西时才轮到它。
            //   它可丢（SSE 只保快，jsonl 保对），而出方向里的 `line` 帧不许因为它晚到或被挤掉 —— 它走自己那个
            //   有界发件箱（`live::LIVE_OUTBOX_CAP`），满了合成整卡。
            let pick = async {
                if burst < REPLY_BURST && burst_bytes < crate::REPLY_PAGE_BYTES {
                    tokio::select! {
                        biased;
                        Some(f) = reply_rx.recv() => { burst += 1; Some((f, true)) }
                        f = rx.recv() => f.map(|mut f| { f.stamp(&tz); (f, false) }),
                        Some(f) = tap_rx.next() => Some((f, false)),
                    }
                } else {
                    burst = 0;
                    burst_bytes = 0;
                    tokio::select! {
                        biased;
                        f = rx.recv() => f.map(|mut f| { f.stamp(&tz); (f, false) }),
                        Some(f) = reply_rx.recv() => Some((f, true)),
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
                        if !flush_out(&mut out).await {
                            return;
                        }
                        unflushed = 0;
                        pick.await
                    }
                }
            }
        };
        let Some((frame, reply)) = picked else {
            // 出方向通道关了 = 寿终：攒着没 flush 的先发出去。
            flush_out(&mut out).await;
            return;
        };
        let wrote = match write_frame(&mut out, &frame, &tz, view.as_ref()).await {
            Ok(n) => n,
            Err(e) => {
                // Broken pipe (client gone) is the normal end-of-life; stop quietly.
                tracing::warn!("stdout write failed ({e}); stopping writer");
                return;
            }
        };
        if reply {
            burst_bytes += wrote;
        } else {
            burst = 0;
            burst_bytes = 0;
        }
        unflushed += wrote;
        if unflushed >= WRITE_FLUSH_BYTES {
            if !flush_out(&mut out).await {
                return;
            }
            unflushed = 0;
        }
    }
}

/// flush 写者那一端；失败（对端走了）⇒ 记一行、回 `false`，写者就此停。
async fn flush_out<W: tokio::io::AsyncWrite + Unpin>(out: &mut W) -> bool {
    match out.flush().await {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("stdout flush failed ({e}); stopping writer");
            false
        }
    }
}

/// Serialize one frame to its wire line and write it (no flush).
/// 回：写了几个字节。
/// 复制详情里那一行时刻的空位按这条流看的那一台的时区填好（应答 · 推送都过这里；`stamp_detail_slots`）；
/// 推送帧照这条流的出口声明裁格（`view`；应答的 kind 不在声明的住处表里，原样）。
async fn write_frame<W: tokio::io::AsyncWrite + Unpin>(
    out: &mut W,
    frame: &Frame,
    tz: &Tz,
    view: Option<&StreamView>,
) -> std::io::Result<usize> {
    let line = to_line_viewed(frame, view)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let line = crate::stamp_detail_slots(&line, tz);
    out.write_all(line.as_bytes()).await?;
    Ok(line.len())
}

#[cfg(test)]
#[path = "../../../tests/backend/writer_task_tests.rs"]
mod tests; // 写者的优先序：tap 灌满时内容帧一条不少、顺序不变
