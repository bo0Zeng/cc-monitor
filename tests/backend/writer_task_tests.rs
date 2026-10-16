//! `main.rs::writer_task` 的第三条来源（tap）排在最后：**tap 灌满时，出方向的内容帧一条不少、顺序不变**。
//!
//! 住址：设计住仓外；要求（「SSE 保快，jsonl 保对」——
//! SSE 那一路断 / 丢 / 挤都不许碰 jsonl 那条对的路）· 抄流跟不上绝不回推主路。

use super::*;
use cc_monitor_backend::observe::watcher::{frame_channel, CHANNEL_MAX_BYTES};

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
    Frame::LiveCard(cc_monitor_backend::wire::LiveCard {
        sid: "sid-w".into(),
        run: None,
        card: 0,
        n,
        op: cc_monitor_backend::wire::LiveOp::Awaiting,
    })
}

/// 出方向 200 条 `line`、tap 400 条同时排着；写者写完之后：写出去的 `line` 帧 **==** 灌进去的（逐条、同序），
/// 而且**每一条 `line` 都排在任何一条 tap 之前**（tap 最低优先：出方向有东西时轮不到它）。
#[tokio::test]
async fn a_flooded_tap_never_costs_or_reorders_a_content_frame() {
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES);
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

/// 出方向的帧写出去那一下按这条流看的那一台的时区写钟面（`Frame::stamp`）；应答通道里的不动（答的那一下已按请求的 `tz` 写好）。
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
            ended: Vec::new(),
        })
    };
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES);
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(4);
    let (_tap_tx, tap_rx) = tokio::sync::mpsc::channel::<Frame>(1);
    tx.try_send(runs(None)).unwrap();
    drop(tx);
    let mut out: Vec<u8> = Vec::new();
    let sh = Tz::named("Asia/Shanghai").unwrap();
    writer_task(&mut out, rx, reply_rx, Fed(tap_rx), sh, None).await;
    drop(reply_tx);
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains(r#""started_text":"04:30""#),
        "推出去的那一帧没按流的时区写钟面：{text}"
    );
}

/// 一页分页应答（`data` 里垫到一页那么大）。
fn page(id: &str) -> Frame {
    let pad = "p".repeat(cc_monitor_backend::REPLY_PAGE_BYTES);
    Frame::Reply(crate::wire::Reply {
        id: id.into(),
        ok: true,
        code: None,
        message: None,
        detail: None,
        data: Some(serde_json::json!({ "pad": pad })),
    })
}

/// 8a b：**大应答分页取到一半，同一台别的会话来了一行 ⇒ 它在下一页之前写出去**（按先后判，不按墙钟）。
/// 应答通道里第 1、2、3 页都排着，出方向排着一帧 `line`：写出去的次序是 第 1 页 · `line` · 第 2 页 · 第 3 页 ——
/// 应答优先只到一页为止（按条数让位要等 8 页之后）。
#[tokio::test]
async fn a_live_line_goes_out_before_the_next_page_of_a_paged_reply() {
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES);
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(10);
    let (_tap_tx, tap_rx) = tokio::sync::mpsc::channel::<Frame>(1);
    for i in 1..=3 {
        reply_tx.try_send(page(&format!("p{i}"))).expect("页灌");
    }
    tx.try_send(line(7)).expect("line 灌");
    drop(tx);
    let mut out: Vec<u8> = Vec::new();
    // 出方向关了 ⇒ 写者在出方向排空之后寿终；剩下的页在关之前写完与否不论，判的是写出去的那几帧的先后。
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
}
