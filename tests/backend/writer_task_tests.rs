//! 〔TAP · V124〕`main.rs::writer_task` 的第三条来源（tap）排在最后：**tap 灌满时，出方向的内容帧一条不少、顺序不变**。
//!
//! 住址：设计住仓外 `调研/第四波记录/TAP.md §1.1 · §7 T3`；要求出处 `设计/20 §8`（「SSE 保快，jsonl 保对」——
//! SSE 那一路断 / 丢 / 挤都不许碰 jsonl 那条对的路）· `设计/05 §4.5.3` ③（抄流跟不上绝不回推主路）。

use super::*;

/// 判据那一侧的 tap 来源：一条手喂的帧通道（生产是 hub 那条，见 `tap::TapRx`）。
struct Fed(tokio::sync::mpsc::Receiver<Frame>);

impl tap::TapSource for Fed {
    async fn next(&mut self) -> Option<Frame> {
        self.0.recv().await
    }
}

fn line(seq: u64) -> Frame {
    Frame::Line {
        session_id: "sid-w".into(),
        path: "/p.jsonl".into(),
        seq,
        message: Some(serde_json::json!({ "i": seq })),
        cwd: None,
        byte_offset: 0,
        rid: None,
    }
}

fn tapf(n: u64) -> Frame {
    Frame::Tap {
        stream: "sid-w".into(),
        run: None,
        resp: 0,
        n,
        ev: Some(cc_monitor_backend::agents::StreamEv::Stop { ok: true }),
        end: None,
    }
}

/// 出方向 200 条 `line`、tap 400 条同时排着；写者写完之后：写出去的 `line` 帧 **==** 灌进去的（逐条、同序），
/// 而且**每一条 `line` 都排在任何一条 tap 之前**（tap 最低优先：出方向有东西时轮不到它）。
#[tokio::test]
async fn a_flooded_tap_never_costs_or_reorders_a_content_frame() {
    let (tx, rx) = tokio::sync::mpsc::channel::<Frame>(1000);
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
    writer_task(&mut out, rx, reply_rx, Fed(tap_rx)).await;
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
    let first_tap = kinds.iter().position(|(k, _)| k == "tap");
    let last_line = kinds.iter().rposition(|(k, _)| k == "line");
    assert!(
        matches!((first_tap, last_line), (Some(t), Some(l)) if t > l) || first_tap.is_none(),
        "tap 帧插到了内容帧前面（它该最低优先）：first_tap={first_tap:?} last_line={last_line:?}"
    );
}
