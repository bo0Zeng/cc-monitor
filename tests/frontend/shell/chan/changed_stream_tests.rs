//! 文件窗口订的「那台的某样东西变了」（`changed/<主题>`）：生产句柄 [`InboundBackends`] 把这条订阅交给进程里那一份会话流句柄
//! （[`crate::event_replay::EventReplay::subscribe_stream`]，与主界面同一张订阅表），那台推来的 `changed` 帧按主题原样交出去。
//!
//! # 买到什么
//!
//! - 订了 `changed/plan` ⇒ 那台推的 `plan` 那一格原样出一格，别的主题、别的机器不进；那台看不见 ⇒ 先一格 `Unseen`。
//! - 形状不对的 `changed/`（空主题 · 带 `/`）· 认不出的流名 · 带了 `from` ⇒ 原位 `Closed{Peer}`，不装作订阅成功。
//!
//! # 买不到什么
//!
//! - 真后端推帧那一段（后端那一侧的判据另判）；窗口拿到之后重问 `plan-files`（`filewin` 的判据另判）；
//!   跟不上 · 丢掉流（`event_replay_tests` 判）。

use super::super::wire::{By, Cursor, Item, Kind, Origin};
use super::*;
use futures::stream::StreamExt;
use std::time::Duration;

async fn next(s: &mut BoxStream<'static, Item>) -> Option<Item> {
    tokio::time::timeout(Duration::from_millis(300), s.next())
        .await
        .ok()
        .flatten()
}

/// 进程里那一份会话流句柄（本判据二进制里先装的那一份；各条判据用各自的机器名，不串）。
fn replay() -> std::sync::Arc<crate::event_replay::EventReplay> {
    crate::event_replay::install(std::sync::Arc::new(crate::event_replay::EventReplay::new()))
}

#[tokio::test]
async fn a_changed_frame_of_that_topic_on_that_machine_comes_out_as_is() {
    let replay = replay();
    let me = Origin::named("chan-changed-a".to_string());
    let mut s = InboundBackends.subscribe(me.clone(), Kind("changed/plan".to_string()), None);
    assert!(
        matches!(next(&mut s).await, Some(Item::Unseen { .. })),
        "那台还没接上 ⇒ 先说看不见"
    );
    replay.changed(
        &Origin::named("chan-changed-other".to_string()),
        "plan",
        "{}".into(),
    );
    replay.changed(&me, "quota", "{}".into());
    replay.changed(&me, "plan", r#"{"key":"/w","rev":"r2"}"#.into());
    replay.changed(&me, "plan", r#"{"key":"/w","rev":"r3"}"#.into());
    let got: Vec<(u64, String)> = [next(&mut s).await, next(&mut s).await]
        .into_iter()
        .map(|i| match i {
            Some(Item::Frame { seq, body }) => (seq, String::from_utf8(body.0).unwrap()),
            other => panic!("不是一格：{other:?}"),
        })
        .collect();
    assert_eq!(
        got,
        [
            (0, r#"{"key":"/w","rev":"r2"}"#.to_string()),
            (1, r#"{"key":"/w","rev":"r3"}"#.to_string())
        ]
    );
    assert!(next(&mut s).await.is_none(), "别的主题 / 别的机器漏进来了");
}

#[tokio::test]
async fn a_malformed_changed_kind_is_refused_in_place() {
    replay();
    let o = || Origin::named("chan-changed-b".to_string());
    for kind in ["changed/", "changed/a/b", "no-such-kind"] {
        let mut s = InboundBackends.subscribe(o(), Kind(kind.to_string()), None);
        assert!(
            matches!(next(&mut s).await, Some(Item::Closed { by: By::Peer(_) })),
            "{kind} 没被原位拒"
        );
    }
    let mut s = InboundBackends.subscribe(
        o(),
        Kind("changed/plan".to_string()),
        Some(Cursor(b"1".to_vec())),
    );
    assert!(
        matches!(next(&mut s).await, Some(Item::Closed { by: By::Peer(_) })),
        "带了 from 没被原位拒"
    );
}
