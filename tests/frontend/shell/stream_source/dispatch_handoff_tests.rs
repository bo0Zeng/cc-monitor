//! 分派第一层（`route`）把就地交的那三种直接交进程里那一份会话流句柄（`event_replay::host()`），中间没有转交的出口要装：
//! 本机的活卡（`live_card`）进 `<local>` 的 `session-live-card`（别台原样的不交）；测试连接的一格（`probe`，只在本机那条流上）进 `<local>` 的 `probe-progress/<票>`；
//! 终端画面（`terminal_screen` · `terminal_follow_end`）进那台的 `terminal-screen/<票>`。期望手写。
//!
//! 买不到什么：界面那一侧怎么画（`live-card` · `remote-probe` · `terminal-follow` 的 vitest 判）；credit 与 `Gap`（`event_replay_tests` 判）。

use super::*;
use crate::chan::wire::Item;
use futures::stream::{BoxStream, StreamExt};
use std::time::Duration;

/// 进程里那一份会话流句柄（本判据二进制里先装的那一份；各条判据用各自的机器名 · 票，不串）。
fn replay() -> Arc<EventReplay> {
    crate::event_replay::install(Arc::new(EventReplay::new()))
}

fn frame(line: &str) -> Frame {
    cmd_wire::frame::decode(line).expect("夹具那一行是一帧")
}

/// 流里下一格，跳过开头那一格「看不见」。
async fn next_frame(s: &mut BoxStream<'static, Item>) -> Option<serde_json::Value> {
    loop {
        let got = tokio::time::timeout(Duration::from_millis(500), s.next())
            .await
            .ok()
            .flatten()?;
        match got {
            Item::Frame { body, .. } => return Some(serde_json::from_slice(&body.0).unwrap()),
            Item::Unseen { .. } | Item::Seen { .. } => continue,
            other => panic!("不是一格：{other:?}"),
        }
    }
}

/// 本机那条流上的 `live_now`（本机后端折好的那一处）原样进 `<local>` 的活卡流；原样的 `live_card` 本机 · 别台都不交（壳不折）；
/// 别台流上来的 `live_now` 不交（别台的是 `relayed` 的 `live`）。
#[tokio::test]
async fn a_local_live_card_frame_goes_straight_into_the_local_live_card_stream() {
    let r = replay();
    let local = crate::origin::Origin::local();
    let box_a = crate::origin::Origin::named("handoff-live-a");
    let mut s = r.subscribe_stream(&local, "session-live-card", None);
    let mut theirs = r.subscribe_stream(&box_a, "session-live-card", None);
    let op = r#"{"kind":"live_card","sid":"handoff-s1","card":7,"n":0,"op":"open","rid":"m","phase":"awaiting","clipped":false,"blocks":[]}"#;
    let whole = serde_json::json!({"sid": "handoff-s1", "cards": [{"card": 7, "rid": "m", "phase": "awaiting", "clipped": false,
        "head": "后端写的头", "body": []}]});
    let now = serde_json::json!({"kind": "live_now", "change": {"t": "whole", "whole": whole}, "whole": whole}).to_string();
    for at in [&box_a, &local] {
        assert!(route(at, None, frame(op)).is_none());
    }
    assert!(route(&box_a, None, frame(&now)).is_none());
    assert!(
        next_frame(&mut theirs).await.is_none(),
        "别台流上的活卡交出去了"
    );
    assert!(
        route(&local, None, frame(&now)).is_none(),
        "活卡不是有序的那几种"
    );
    let got = next_frame(&mut s).await.expect("活卡成品没进本机的活卡流");
    assert_eq!(got["origin"], "<local>");
    assert_eq!(
        got["live"],
        serde_json::json!({"t": "whole", "whole": whole})
    );
    assert!(
        next_frame(&mut s).await.is_none(),
        "本机原样的 `live_card` 也交了（壳又在折）"
    );
}

#[tokio::test]
async fn a_probe_cell_from_the_local_stream_goes_straight_into_its_tickets_progress_stream() {
    let r = replay();
    let local = crate::origin::Origin::local();
    let mut s = r.subscribe_stream(&local, "probe-progress/handoff-p1", None);
    let line = r#"{"kind":"probe","ticket":"handoff-p1","cell":{"stage":"dial"}}"#;
    assert!(route(&local, None, frame(line)).is_none());
    assert_eq!(
        next_frame(&mut s).await,
        Some(serde_json::json!({"stage": "dial"})),
        "测试连接那一格没进那张票的进度流"
    );
}

#[tokio::test]
async fn a_terminal_follow_end_goes_straight_into_that_machines_screen_stream_without_its_ticket() {
    let r = replay();
    let box_b = crate::origin::Origin::named("handoff-screen-b");
    let mut s = r.subscribe_stream(&box_b, "terminal-screen/handoff-t1", None);
    let line = r#"{"kind":"terminal_follow_end","ticket":"handoff-t1","why":"gone","said":"那台写的一句"}"#;
    assert!(route(&box_b, None, frame(line)).is_none());
    assert_eq!(
        next_frame(&mut s).await,
        Some(serde_json::json!({"why": "gone", "said": "那台写的一句"})),
        "终端画面那一格没进那台那张票的画面流"
    );
}
