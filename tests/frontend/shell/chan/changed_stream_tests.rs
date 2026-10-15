//! 文件窗口订的「那台的某样东西变了」（`changed/<主题>`）：生产句柄 [`InboundBackends`] 把那台推来的 `changed` 帧按主题原样交出去。
//!
//! # 买到什么
//!
//! - 订了 `changed/plan` ⇒ 那台推的 `plan` 那一格原样出一格（序号从 1 连续），别的主题、别的机器不进。
//! - 形状不对的 `changed/`（空主题 · 带 `/`）⇒ 原位 `Closed{Peer}`，不装作订阅成功。
//!
//! # 买不到什么
//!
//! - 真后端推帧那一段（后端那一侧的判据另判）；窗口拿到之后重问 `plan-files`（`filewin` 的判据另判）。

use super::super::wire::{By, Item, Kind, Origin};
use super::*;
use futures::stream::StreamExt;
use std::time::Duration;

async fn next(s: &mut BoxStream<'static, Item>) -> Option<Item> {
    tokio::time::timeout(Duration::from_millis(300), s.next())
        .await
        .ok()
        .flatten()
}

#[tokio::test]
async fn a_changed_frame_of_that_topic_on_that_machine_comes_out_as_is() {
    let me = Origin("chan-changed-a".to_string());
    let mut s = InboundBackends.subscribe_raw(me.clone(), Kind("changed/plan".to_string()), None);
    let replay = crate::event_replay::EventReplay::new();
    replay.changed(
        &Origin("chan-changed-other".to_string()),
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
            (1, r#"{"key":"/w","rev":"r2"}"#.to_string()),
            (2, r#"{"key":"/w","rev":"r3"}"#.to_string())
        ]
    );
    assert!(next(&mut s).await.is_none(), "别的主题 / 别的机器漏进来了");
}

#[tokio::test]
async fn a_malformed_changed_kind_is_refused_in_place() {
    for kind in ["changed/", "changed/a/b"] {
        let mut s = InboundBackends.subscribe_raw(
            Origin("chan-changed-b".to_string()),
            Kind(kind.to_string()),
            None,
        );
        assert!(
            matches!(next(&mut s).await, Some(Item::Closed { by: By::Peer(_) })),
            "{kind} 没被原位拒"
        );
    }
}
