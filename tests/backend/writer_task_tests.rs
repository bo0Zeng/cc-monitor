//! `stream/writer.rs::writer_task` 的第三条来源（tap）排在最后：**tap 灌满时，出方向的内容帧一条不少、顺序不变**。
//!
//! 住址：设计住仓外；要求（「SSE 保快，jsonl 保对」——
//! SSE 那一路断 / 丢 / 挤都不许碰 jsonl 那条对的路）· 抄流跟不上绝不回推主路。

use super::*;
use crate::observe::watcher::{frame_channel, CHANNEL_MAX_BYTES};
use crate::wire;

/// 判据那一侧的 tap 来源：一条手喂的帧通道（生产是 hub 那条，见 `tap::TapRx`）。
struct Fed(tokio::sync::mpsc::Receiver<Frame>);

impl tap::TapSource for Fed {
    async fn next(&mut self) -> Option<Frame> {
        self.0.recv().await
    }
}

fn line(seq: u64) -> Frame {
    Frame::Line(crate::wire::Line {
        session_id: "sid-w".into(),
        path: "/p.jsonl".into(),
        seq,
        record: None,
        cwd: None,
        byte_offset: 0,
        rid: None,
    })
}

fn tapf(n: u64) -> Frame {
    Frame::LiveCard(crate::wire::LiveCard {
        sid: "sid-w".into(),
        run: None,
        card: 0,
        n,
        op: crate::wire::LiveOp::Awaiting,
    })
}

/// 出方向 200 条 `line`、tap 400 条同时排着；写者写完之后：写出去的 `line` 帧 **==** 灌进去的（逐条、同序），
/// 而且**每一条 `line` 都排在任何一条 tap 之前**（tap 最低优先：出方向有东西时轮不到它）。
#[tokio::test]
async fn a_flooded_tap_never_costs_or_reorders_a_content_frame() {
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(8);
    let (tap_tx, tap_rx) = tokio::sync::mpsc::channel::<Frame>(1000);
    for i in 0..400 {
        tap_tx.try_send(tapf(i)).expect("tap 灌");
    }
    for i in 0..200 {
        tx.try_send(line(i)).expect("line 灌");
    }
    drop(tx); // 出方向关了 ⇒ 写者排完就寿终
    drop(tap_tx);
    let mut out: Vec<u8> = Vec::new();
    writer_task(
        &mut out,
        rx,
        reply_rx,
        Fed(tap_rx),
        Default::default(),
        None,
    )
    .await;
    drop(reply_tx);

    let text = String::from_utf8(out).expect("utf8");
    let kinds: Vec<(String, u64)> = text
        .lines()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).expect("一行一帧");
            let k = v["kind"].as_str().expect("kind").to_string();
            let key = if k == "line" {
                v["seq"].as_u64()
            } else {
                v["n"].as_u64()
            };
            (k, key.expect("序号"))
        })
        .collect();
    let lines: Vec<u64> = kinds
        .iter()
        .filter(|(k, _)| k == "line")
        .map(|(_, s)| *s)
        .collect();
    assert_eq!(lines, (0..200).collect::<Vec<_>>(), "内容帧少了或乱了");
    let first_tap = kinds.iter().position(|(k, _)| k == "live");
    let last_line = kinds.iter().rposition(|(k, _)| k == "line");
    assert!(
        matches!((first_tap, last_line), (Some(t), Some(l)) if t > l) || first_tap.is_none(),
        "tap 帧插到了内容帧前面（它该最低优先）：first_tap={first_tap:?} last_line={last_line:?}"
    );
}

/// 出方向的帧写出去那一下按这条流看的那一台的时区写钟面（`FrameExit::stamp`）；应答 · tap 里没有带钟面的帧（答的那一下已按请求的 `tz` 写好），过出口原样。
#[tokio::test]
async fn pushed_frames_get_their_clock_faces_in_the_stream_zone() {
    let runs = |text: Option<&str>| {
        Frame::SessionRuns(crate::wire::SessionRuns {
            sid: "s".into(),
            runs: vec![wire::RunInfo {
                run: "r".into(),
                // 2026-10-07T20:30:00Z
                started_ms: Some(1_791_405_000_000),
                started_text: text.map(str::to_string),
                ..Default::default()
            }],
            ..Default::default()
        })
    };
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(4);
    let (_tap_tx, tap_rx) = tokio::sync::mpsc::channel::<Frame>(1);
    tx.try_send(runs(None)).unwrap();
    drop(tx);
    let mut out: Vec<u8> = Vec::new();
    let sh = Tz::named("Asia/Shanghai").unwrap();
    writer_task(&mut out, rx, reply_rx, Fed(tap_rx), sh, None).await;
    drop(reply_tx);
    let text = String::from_utf8(out).unwrap();
    let started = copy_core::copy_text("agentWindow.facts.started", &[("time", "04:30")]);
    assert!(
        text.contains(&format!(r#""started_text":"{started}""#)),
        "推出去的那一帧没按流的时区写钟面：{text}"
    );
}

/// 出方向一帧 `line`（记录那一格是给的那一行）过写者，回写出去的那一行的 `record`。
async fn written_record(rec: crate::agents::record::BackRecord) -> serde_json::Value {
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(4);
    let (_tap_tx, tap_rx) = tokio::sync::mpsc::channel::<Frame>(1);
    let Frame::Line(mut l) = line(0) else {
        unreachable!()
    };
    l.record = Some(rec);
    tx.try_send(Frame::Line(l)).unwrap();
    drop(tx);
    let mut out: Vec<u8> = Vec::new();
    let sh = Tz::named("Asia/Shanghai").unwrap();
    writer_task(&mut out, rx, reply_rx, Fed(tap_rx), sh, None).await;
    drop(reply_tx);
    let text = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(text.lines().next().expect("一帧")).unwrap();
    v["record"].clone()
}

/// 后端的行带着重试那几格进出方向 ⇒ 写者那一下照流的时区写好那一行。
#[tokio::test]
async fn the_writer_writes_the_retry_line_from_the_back_facts() {
    let facts = crate::agents::record::RetryFacts {
        reason: crate::agents::ApiReason::Network,
        attempt: Some(1),
        max: Some(3),
    };
    let rec = written_record(crate::agents::record::retry(
        "fake".into(),
        "r".into(),
        // 2026-10-07T20:30:00Z ⇒ 上海 04:30
        Some("2026-10-07T20:30:00.000Z".into()),
        facts,
    ))
    .await;
    let want = crate::wire::stamp::retry_line(facts.reason, facts.attempt, facts.max, "04:30");
    assert_eq!(rec["text"], serde_json::json!(want), "{rec}");
}

/// 不带私有格、那一行已写好的重试那一条（线上那一形包成后端的行）过写者：那一行原样出，不再写一遍。
#[tokio::test]
async fn a_retry_line_already_written_goes_out_as_is() {
    let mut r = crate::agents::record::record(
        "fake".into(),
        "r".into(),
        Some("2026-10-07T20:30:00.000Z".into()),
        crate::agents::record::Body::Retry { text: None },
    );
    r.body = crate::agents::record::Body::Retry {
        text: Some(crate::common::cells::Words("那边写好的一行".into())),
    };
    let rec = written_record(crate::agents::record::BackRecord::from(r)).await;
    assert_eq!(rec["text"], serde_json::json!("那边写好的一行"), "{rec}");
}

/// 一页分页应答（`data` 里垫到一页那么大）。
fn page(id: &str) -> Frame {
    let pad = "p".repeat(crate::wire::REPLY_PAGE_BYTES.as_len());
    Frame::Reply(crate::wire::Reply {
        id: id.into(),
        ok: true,
        code: None,
        said: None,
        detail: None,
        data: Some(serde_json::json!({ "pad": pad })),
    })
}

/// 8a b：**大应答分页取到一半，同一台别的会话来了一行 ⇒ 它在下一页之前写出去**（按先后判，不按墙钟）。
/// 应答通道里第 1、2、3 页都排着，出方向排着一帧 `line`：写出去的次序是 第 1 页 · `line` · 第 2 页 · 第 3 页 ——
/// 应答优先只到一页为止（按条数让位要等 8 页之后）。
#[tokio::test]
async fn a_live_line_goes_out_before_the_next_page_of_a_paged_reply() {
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(10);
    let (_tap_tx, tap_rx) = tokio::sync::mpsc::channel::<Frame>(1);
    for i in 1..=3 {
        reply_tx.try_send(page(&format!("p{i}"))).expect("页灌");
    }
    tx.try_send(line(7)).expect("line 灌");
    drop(tx);
    let mut out: Vec<u8> = Vec::new();
    // 出方向关了 ⇒ 写者在出方向排空之后把排着的页写完再寿终；判的是写出去的那几帧的先后。
    writer_task(
        &mut out,
        rx,
        reply_rx,
        Fed(tap_rx),
        Default::default(),
        None,
    )
    .await;
    drop(reply_tx);
    let seen: Vec<String> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            match v["kind"].as_str() {
                Some("line") => "line".to_string(),
                _ => v["id"].as_str().unwrap_or("?").to_string(),
            }
        })
        .collect();
    assert_eq!(
        &seen[..2],
        ["p1", "line"],
        "实时行排在了第 2 页之后：{seen:?}"
    );
    assert_eq!(
        seen,
        ["p1", "line", "p2", "p3"],
        "出方向关了那一下排着的页没写完：{seen:?}"
    );
}

/// 判据那一侧的写端：写进来的先攒着，**flush 那一下才算发出去**（像 `BufWriter` · deflate 那一层）；数 flush 几次、每次带出去多少字节。
#[derive(Clone, Default)]
struct Flushed(std::sync::Arc<std::sync::Mutex<FlushLog>>);

#[derive(Default)]
struct FlushLog {
    pending: Vec<u8>,
    sent: Vec<u8>,
    flushes: Vec<usize>,
}

impl Flushed {
    fn sent_lines(&self) -> Vec<String> {
        String::from_utf8(self.0.lock().unwrap().sent.clone())
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect()
    }
    fn flushes(&self) -> Vec<usize> {
        self.0.lock().unwrap().flushes.clone()
    }
}

impl tokio::io::AsyncWrite for Flushed {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.0.lock().unwrap().pending.extend_from_slice(buf);
        std::task::Poll::Ready(Ok(buf.len()))
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let mut g = self.0.lock().unwrap();
        let chunk = std::mem::take(&mut g.pending);
        g.flushes.push(chunk.len());
        g.sent.extend_from_slice(&chunk);
        std::task::Poll::Ready(Ok(()))
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.poll_flush(cx)
    }
}

fn seqs(lines: &[String]) -> Vec<u64> {
    lines
        .iter()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).expect("一行一帧");
            v["seq"].as_u64().expect("seq")
        })
        .collect()
}

/// 出方向一次排着 40 帧、随后关掉：40 帧**全部发出去**（最后几帧不许攒在写端里没 flush 就寿终），
/// 而且只 flush **一次**（排着的时候不 flush，排空了才 flush）。
#[tokio::test]
async fn a_queued_burst_goes_out_whole_in_one_flush() {
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(4);
    let (_tap_tx, tap_rx) = tokio::sync::mpsc::channel::<Frame>(1);
    for i in 0..40 {
        tx.try_send(line(i)).expect("line 灌");
    }
    drop(tx);
    let out = Flushed::default();
    let mut w = out.clone();
    writer_task(&mut w, rx, reply_rx, Fed(tap_rx), Default::default(), None).await;
    drop(reply_tx);
    assert_eq!(
        seqs(&out.sent_lines()),
        (0..40).collect::<Vec<_>>(),
        "排空之前寿终，尾巴没 flush 出去"
    );
    assert_eq!(
        out.flushes().len(),
        1,
        "排着的帧一帧一 flush：{:?}",
        out.flushes()
    );
}

/// 只来一帧、通道还开着：这一帧写完**立刻**发出去（不等下一帧、不等计时器）。
#[tokio::test]
async fn a_lone_frame_goes_out_at_once() {
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(4);
    let (_tap_tx, tap_rx) = tokio::sync::mpsc::channel::<Frame>(1);
    let out = Flushed::default();
    let w = out.clone();
    let task = tokio::spawn(async move {
        writer_task(w, rx, reply_rx, Fed(tap_rx), Default::default(), None).await;
        drop(reply_tx);
    });
    tx.try_send(line(1)).expect("line 灌");
    let mut sent = Vec::new();
    for _ in 0..1000 {
        sent = out.sent_lines();
        if !sent.is_empty() {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(seqs(&sent), vec![1], "通道还开着时这一帧没发出去");
    drop(tx);
    task.await.unwrap();
}

/// 洪峰：一直排着的时候也不能无限攒 —— 自上次 flush 起写满 64 KB（`WRITE_FLUSH_BYTES`）就照 flush 一次（首帧延迟有界）。
#[tokio::test]
async fn a_long_flood_is_flushed_every_64k() {
    let fat = |seq: u64| {
        Frame::Line(crate::wire::Line {
            session_id: "sid-w".into(),
            path: format!("/{}", "p".repeat(8 * 1024)),
            seq,
            record: None,
            cwd: None,
            byte_offset: 0,
            rid: None,
        })
    };
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(4);
    let (_tap_tx, tap_rx) = tokio::sync::mpsc::channel::<Frame>(1);
    for i in 0..40 {
        tx.try_send(fat(i)).expect("line 灌");
    }
    drop(tx);
    let out = Flushed::default();
    let mut w = out.clone();
    writer_task(&mut w, rx, reply_rx, Fed(tap_rx), Default::default(), None).await;
    drop(reply_tx);
    assert_eq!(
        seqs(&out.sent_lines()),
        (0..40).collect::<Vec<_>>(),
        "洪峰里少了帧"
    );
    // 上限照设计定的 64 KB 写死在这里：写者那一边的常量改了，这一条跟着改。
    const CAP: usize = 64 * 1024;
    let one = crate::wire::to_line(&crate::wire::FrameExit::stamp(fat(0), &Tz::default()))
        .unwrap()
        .len();
    let flushes = out.flushes();
    assert!(
        flushes.iter().all(|&n| n < CAP + one),
        "一次 flush 攒过了 64 KB 上限：{flushes:?}"
    );
    assert!(flushes.len() < 40, "洪峰里仍是一帧一 flush：{flushes:?}");
}

/// 写什么都报对端已断的那一端（套接字那一头的对端走了）。
struct PeerLeft;

impl tokio::io::AsyncWrite for PeerLeft {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        _: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into()))
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into()))
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

/// ★ 写者交回它为什么停：写报对端已断 ⇒ 对端走了；出方向关了（对端可能还连着）⇒ 不是对端走了。
/// 一条流收场照它分：对端走了才等读循环把已收的读完（`stream/entry.rs::stream_ends`）。
#[tokio::test]
async fn the_writer_says_whether_it_stopped_because_the_peer_left() {
    let sh = Tz::named("Asia/Shanghai").unwrap();
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    let (_reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(4);
    let (_tap_tx, tap_rx) = tokio::sync::mpsc::channel::<Frame>(1);
    tx.try_send(line(0)).unwrap();
    assert_eq!(
        writer_task(PeerLeft, rx, reply_rx, Fed(tap_rx), sh.clone(), None).await,
        WriterEnd::PeerGone
    );
    drop(tx);

    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    let (_reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(4);
    let (_tap_tx, tap_rx) = tokio::sync::mpsc::channel::<Frame>(1);
    drop(tx);
    let mut out: Vec<u8> = Vec::new();
    assert_eq!(
        writer_task(&mut out, rx, reply_rx, Fed(tap_rx), sh, None).await,
        WriterEnd::Closed
    );
}

/// 判据那一侧会断的写端：写 · flush 各照给的那一形答（写可以先好好写 `good` 次再照 `write` 答）；数写了几次、flush 了几次（停了之后还写不写）。
#[derive(Clone)]
struct Breaks {
    good: usize,
    write: Answer,
    flush: Answer,
    log: std::sync::Arc<std::sync::Mutex<(usize, usize)>>,
}

/// 写端那一下怎么答。
#[derive(Clone, Copy, Debug)]
enum Answer {
    Ok,
    /// 写进去零个字节（`write_all` 报 `WriteZero`：这一帧写不出）。
    Zero,
    Fail(std::io::ErrorKind),
}

impl Breaks {
    fn new(write: Answer, flush: Answer) -> Self {
        Self {
            good: 0,
            write,
            flush,
            log: Default::default(),
        }
    }
    /// 头 `n` 次写好好写，之后照 `write` 答。
    fn after(self, n: usize) -> Self {
        Self { good: n, ..self }
    }
    /// （写了几次, flush 了几次）
    fn counts(&self) -> (usize, usize) {
        *self.log.lock().unwrap()
    }
}

impl tokio::io::AsyncWrite for Breaks {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let nth = {
            let mut g = self.log.lock().unwrap();
            g.0 += 1;
            g.0
        };
        let answer = if nth <= self.good {
            Answer::Ok
        } else {
            self.write
        };
        std::task::Poll::Ready(match answer {
            Answer::Ok => Ok(buf.len()),
            Answer::Zero => Ok(0),
            Answer::Fail(k) => Err(k.into()),
        })
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.log.lock().unwrap().1 += 1;
        std::task::Poll::Ready(match self.flush {
            Answer::Fail(k) => Err(k.into()),
            _ => Ok(()),
        })
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

/// 出方向排着这几帧（`close` ⇒ 随即关掉通道，否则一直开着），写者跑到停 ⇒ 交回的停因。
/// 写者不停就是这一形没收场：上限内等不到 ⇒ 红。
async fn ends_with(out: Breaks, frames: Vec<Frame>, close: bool) -> WriterEnd {
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    let (_reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(4);
    let (_tap_tx, tap_rx) = tokio::sync::mpsc::channel::<Frame>(1);
    for f in frames {
        tx.try_send(f).expect("出方向灌");
    }
    let keep = (!close).then_some(tx);
    let end = tokio::time::timeout(
        guard_core::wall::CEILING,
        writer_task(out, rx, reply_rx, Fed(tap_rx), Default::default(), None),
    )
    .await
    .expect("写者该停却没停");
    drop(keep);
    end
}

/// ★ 写报对端已断的另外几形（被重置 · 中止 · 没连着；断管见上一条）⇒ 交回「对端走了」（这条流随后等读循环把已收的读完再收场：
/// `main_startup_tests.rs::a_stream_whose_writer_ends_first_still_reads_what_came_in`）；写者自己当场停，排着的后几帧不再写。
#[tokio::test]
async fn a_write_the_peer_dropped_ends_as_peer_gone() {
    use std::io::ErrorKind as K;
    for kind in [K::ConnectionReset, K::ConnectionAborted, K::NotConnected] {
        let out = Breaks::new(Answer::Fail(kind), Answer::Ok);
        let end = ends_with(out.clone(), vec![line(0), line(1), line(2)], false).await;
        assert_eq!(end, WriterEnd::PeerGone, "写报 {kind:?}");
        assert_eq!(out.counts(), (1, 0), "写报 {kind:?} 之后还在写 / flush");
    }
}

/// ★ 一帧写不出（写进去零个字节）· 写报了别的错 ⇒ 交回「别的缘故」（对端可能还连着：这条流随后当场掐掉读循环收场，
/// `main_startup_tests.rs::a_stream_whose_writer_stops_with_the_peer_still_there_ends_at_once`）；写者当场停。
#[tokio::test]
async fn a_frame_that_cannot_be_written_ends_as_closed() {
    use std::io::ErrorKind as K;
    for write in [
        Answer::Zero,
        Answer::Fail(K::Other),
        Answer::Fail(K::TimedOut),
    ] {
        let out = Breaks::new(write, Answer::Ok);
        let end = ends_with(out.clone(), vec![line(0), line(1), line(2)], false).await;
        assert_eq!(end, WriterEnd::Closed, "写答 {write:?}");
        assert_eq!(out.counts(), (1, 0), "写答 {write:?} 之后还在写 / flush");
    }
}

/// 一帧 8 KB 多的 `line`（攒够 64 KB 要好几帧）。
fn fat(seq: u64) -> Frame {
    Frame::Line(crate::wire::Line {
        session_id: "sid-w".into(),
        path: format!("/{}", "p".repeat(8 * 1024)),
        seq,
        record: None,
        cwd: None,
        byte_offset: 0,
        rid: None,
    })
}

/// ★ flush 失败 ⇒ 照 flush 报的那一形交回停因：报对端已断 ⇒ 对端走了，报别的 ⇒ 别的缘故。
/// 写者 flush 的三处各走一遍 —— 排空了那一下（通道开着、只来一帧）· 攒满 64 KB 那一下（洪峰、通道开着）·
/// 出方向关了收尾那一下 —— 每处 flush 一失败写者就停：只 flush 那一次（洪峰那一处另验此后一帧不写）。
#[tokio::test]
async fn a_failed_flush_ends_the_writer_with_its_reason() {
    use std::io::ErrorKind as K;
    for (kind, want) in [
        (K::BrokenPipe, WriterEnd::PeerGone),
        (K::ConnectionReset, WriterEnd::PeerGone),
        (K::Other, WriterEnd::Closed),
    ] {
        // 排空了那一下
        let out = Breaks::new(Answer::Ok, Answer::Fail(kind));
        let end = ends_with(out.clone(), vec![line(0)], false).await;
        assert_eq!(end, want, "排空时 flush 报 {kind:?}");
        assert_eq!(out.counts(), (1, 1), "排空时 flush 报 {kind:?}");

        // 攒满 64 KB 那一下：40 帧排着（不会先排空），flush 一失败就停，后面那些帧不写
        let out = Breaks::new(Answer::Ok, Answer::Fail(kind));
        let end = ends_with(out.clone(), (0..40).map(fat).collect(), false).await;
        assert_eq!(end, want, "64 KB 那一下 flush 报 {kind:?}");
        // 写满 64 KB 的那一帧（上限照设计写死，同洪峰那一条）一写完就 flush、就停：写了正好那么几帧
        const CAP: usize = 64 * 1024;
        let one = crate::wire::to_line(&crate::wire::FrameExit::stamp(fat(0), &Tz::default()))
            .unwrap()
            .len();
        assert_eq!(
            out.counts(),
            (CAP.div_ceil(one), 1),
            "64 KB 那一下 flush 报 {kind:?}：不是在写满 64 KB 那一帧之后 flush、停下"
        );

        // 出方向关了收尾那一下
        let out = Breaks::new(Answer::Ok, Answer::Fail(kind));
        let end = ends_with(out.clone(), vec![line(0)], true).await;
        assert_eq!(end, want, "收尾时 flush 报 {kind:?}");
        assert_eq!(out.counts(), (1, 1), "收尾时 flush 报 {kind:?}");
    }
}

/// ★ 出方向通道关了、写端好好的 ⇒ 交回「别的缘故」（应答那一路还开着、对端可能还连着：这条流随后当场收场），
/// 停之前排着的那几帧全写完、攒着的 flush 出去一次。
#[tokio::test]
async fn a_closed_outbound_channel_ends_as_closed_after_the_tail() {
    let out = Breaks::new(Answer::Ok, Answer::Ok);
    let end = ends_with(out.clone(), vec![line(0), line(1), line(2)], true).await;
    assert_eq!(end, WriterEnd::Closed);
    assert_eq!(out.counts(), (3, 1), "尾巴没写完或没 flush 出去");
}

/// 一条小应答（`id` 认得出是哪一条）。
fn reply(id: &str) -> Frame {
    Frame::Reply(crate::wire::Reply {
        id: id.into(),
        ok: true,
        code: None,
        said: None,
        detail: None,
        data: None,
    })
}

/// 应答这一路排着 `n` 条（发件那一头一直留着，同生产：应答通道不会读到关），出方向一帧不排就关掉；写者跑到停 ⇒ 停因。
async fn replies_then_closed(out: impl tokio::io::AsyncWrite + Unpin, n: usize) -> WriterEnd {
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(n.max(1));
    let (_tap_tx, tap_rx) = tokio::sync::mpsc::channel::<Frame>(1);
    for i in 0..n {
        reply_tx.try_send(reply(&format!("r{i}"))).expect("应答灌");
    }
    drop(tx);
    let end = tokio::time::timeout(
        guard_core::wall::CEILING,
        writer_task(out, rx, reply_rx, Fed(tap_rx), Default::default(), None),
    )
    .await
    .expect("写者该停却没停");
    drop(reply_tx);
    end
}

/// ★ 应答连发的预算用完那一轮（`REPLY_BURST` 之后让位给出方向）出方向恰好关了、写端好好的 ⇒
/// 排着的应答照样全写出去、一次 flush 带走，再交回「别的缘故」—— 不许在让位那一下就收尾、丢下后几条（对端那几问就只能等到期限）。
#[tokio::test]
async fn replies_queued_past_the_burst_still_go_out_when_the_outbound_closes() {
    let out = Flushed::default();
    let end = replies_then_closed(out.clone(), 20).await;
    assert_eq!(end, WriterEnd::Closed);
    let ids: Vec<String> = out
        .sent_lines()
        .iter()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).expect("一行一帧");
            v["id"].as_str().expect("id").to_string()
        })
        .collect();
    assert_eq!(
        ids,
        (0..20).map(|i| format!("r{i}")).collect::<Vec<_>>(),
        "出方向关了那一下排着的应答少了或乱了"
    );
    assert_eq!(
        out.flushes().len(),
        1,
        "收尾不止 flush 一次：{:?}",
        out.flushes()
    );
}

/// ★ 同一形但收尾排着应答的那一段里写端断了 ⇒ 当场收尾：预算内 8 条写好，让位之后排空的头一条写报对端已断就停，
/// 后面排着的应答不再写、不 flush。
#[tokio::test]
async fn replies_queued_when_the_outbound_closes_stop_at_a_dead_writer() {
    let out = Breaks::new(Answer::Fail(std::io::ErrorKind::BrokenPipe), Answer::Ok).after(8);
    let end = replies_then_closed(out.clone(), 20).await;
    assert_eq!(end, WriterEnd::PeerGone);
    assert_eq!(
        out.counts(),
        (9, 0),
        "写端断了之后还在写 / flush，或排着的没写到断那一下"
    );
}

/// 每写一下先让出一回的写端（像套接字那样写一下就可能挂起）：让出那一下别的任务（发件那一头）跑得上。
struct Yields(bool);

impl tokio::io::AsyncWrite for Yields {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.0 = !self.0;
        if self.0 {
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        } else {
            std::task::Poll::Ready(Ok(buf.len()))
        }
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

/// ★ 收尾排着应答那一段不许被一直在送的那一头拖住：出方向关了那一下起应答那一路不再收新的，
/// 排着的写完就收尾（发件那一头一直在送也一样）。
#[tokio::test]
async fn the_tail_of_queued_replies_is_bounded_while_replies_keep_coming() {
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(16);
    let (_tap_tx, tap_rx) = tokio::sync::mpsc::channel::<Frame>(1);
    for i in 0..16 {
        reply_tx.try_send(reply(&format!("r{i}"))).expect("应答灌");
    }
    drop(tx);
    // 发件那一头一直在送（同生产的读循环：通道一空出一格就补上），直到通道关了送不进。
    let feeder = tokio::spawn(async move {
        let mut i = 16u64;
        while reply_tx.send(reply(&format!("r{i}"))).await.is_ok() {
            i += 1;
            tokio::task::yield_now().await;
        }
    });
    let end = tokio::time::timeout(
        guard_core::wall::CEILING,
        writer_task(
            Yields(false),
            rx,
            reply_rx,
            Fed(tap_rx),
            Default::default(),
            None,
        ),
    )
    .await
    .expect("一直有应答送进来，写者就停不下来");
    assert_eq!(end, WriterEnd::Closed);
    feeder.await.unwrap();
}
