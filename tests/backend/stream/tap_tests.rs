//! `tap.rs`：一条流连接最低优先那一路（活卡的发件箱 ＋ 几种 `changed` 推送）。活卡本身住 `live_tests.rs`。
//!
//! 经真中转走一遍的那几条住 `relay/host_tests.rs`（它们要中转的门与夹具）。

use super::*;
use crate::relay::{TapBody, TapEvent};

/// 中转交来的事件经 hub 进了活卡：这条连接的写者取得到 `live` 帧（名单只报 A ⇒ B 的不来）。
#[test]
fn what_the_relay_offers_reaches_the_connection_as_live_frames_for_its_list_only() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let hub = std::sync::Arc::new(TapHub::new(vec![crate::agents::StreamFamily {
            face: crate::agents::fake::runs::STREAM,
            owns: true,
        }]));
        let mut t = attach_rx(hub.clone());
        t.watch_handle().watch(&["A".to_string()]);
        for (sid, resp) in [("B", 2u64), ("A", 1u64)] {
            hub.offer(TapEvent {
                stream: sid.into(),
                owner: String::new(),
                resp,
                n: 0,
                body: TapBody::Data(format!(r#"{{"ev":"open","rid":"r{resp}"}}"#)),
            });
        }
        let f = tokio::time::timeout(guard_core::wall::CEILING, t.next())
            .await
            .expect("该推一帧");
        match f {
            Some(Frame::LiveCard(l)) => assert_eq!(l.sid, "A"),
            other => panic!("不是 A 的活卡：{other:?}"),
        }
    });
}

/// ★ 订阅计划：某个工作区的计划变了 ⇒ 这条流连接推一帧 `changed {plan, key: 工作区, rev, body: {needs}}`；没再变 ⇒ 不再推。
#[test]
fn a_plan_change_becomes_one_changed_frame() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let mut t = attach_rx(std::sync::Arc::new(TapHub::default()));
        let tx = tokio::sync::broadcast::channel::<crate::plan::watch::Change>(4).0;
        t.plan = Some(tx.subscribe());
        tx.send(("/w".to_string(), "r2".to_string(), 3)).unwrap();
        let f = tokio::time::timeout(guard_core::wall::CEILING, t.next())
            .await
            .expect("该推一帧");
        match f {
            Some(Frame::Changed {
                topic: Topic::Plan,
                key,
                rev,
                body,
            }) => assert_eq!(
                (key.as_deref(), rev.as_deref(), body),
                (
                    Some("/w"),
                    Some("r2"),
                    Some(serde_json::json!({"needs": 3}))
                )
            ),
            other => panic!("{other:?}"),
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), t.next())
                .await
                .is_err()
        );
    });
}

/// ★ 订阅节点：事实那几种 ⇒ 推 `changed {quota}` 与 `changed {accounts}`（清单每号带「5h 那一格」）；带会话的 ⇒ 再推一帧 `changed {rotation, key: sid}`；
/// 策略那一种 ⇒ `changed {rotation_strategies}`。没订那条通道的连接永远不推。
#[test]
fn each_kind_of_node_becomes_the_changed_frames_it_stands_for() {
    use crate::accounts::quota::nodes::{emit, Source};
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let mut t = attach_rx(std::sync::Arc::new(TapHub::default()));
        let tx = tokio::sync::broadcast::channel(16).0;
        t.nodes = Some(tx.subscribe());
        let mut got = Vec::new();
        for (source, sid, n) in [
            (Source::Reading, None, 2),
            (Source::QuotaFile, None, 2),
            (Source::Logins, None, 2),
            (Source::Login, None, 2),
            (Source::Refused, Some("s9"), 3),
            (Source::Book, Some("s1"), 1),
            (Source::Strategies, None, 1),
        ] {
            emit(&tx, source, sid);
            for _ in 0..n {
                match tokio::time::timeout(guard_core::wall::CEILING, t.next()).await {
                    Ok(Some(Frame::Changed { topic, key, .. })) => got.push((source, topic, key)),
                    other => panic!("{source:?} 该推一帧：{other:?}"),
                }
            }
        }
        let q = |s| (s, Topic::Quota, None);
        let a = |s| (s, Topic::Accounts, None);
        assert_eq!(
            got,
            vec![
                q(Source::Reading),
                a(Source::Reading),
                q(Source::QuotaFile),
                a(Source::QuotaFile),
                q(Source::Logins),
                a(Source::Logins),
                q(Source::Login),
                a(Source::Login),
                q(Source::Refused),
                a(Source::Refused),
                (Source::Refused, Topic::Rotation, Some("s9".to_string())),
                (Source::Book, Topic::Rotation, Some("s1".to_string())),
                (Source::Strategies, Topic::RotationStrategies, None),
            ]
        );
        // 没再来 ⇒ 不再推。
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), t.next())
                .await
                .is_err()
        );
    });
}

/// 帧里带不带小成品照主题表：轮换那一帧（`rotation`）不带（客户端照 `{target: {sid}}` 重问 `rotation-view`），
/// 即便这条连接的 `bodies` 现算得出东西也不带。
#[test]
fn a_rotation_change_carries_the_body_the_table_asks_for() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let mut t = attach_rx(std::sync::Arc::new(TapHub::default()));
        // 小成品按这条连接看的那一台的时区现算（同客户端带 `tz` 重问）。
        fn fake(topic: Topic, key: Option<&str>, tz: &crate::Tz) -> Option<serde_json::Value> {
            Some(serde_json::json!({ "topic": topic.name(), "key": key, "tz": tz.iana_name() }))
        }
        t.bodies = fake;
        t.tz = crate::Tz::named("Asia/Shanghai").unwrap();
        let tx = tokio::sync::broadcast::channel(4).0;
        t.nodes = Some(tx.subscribe());
        crate::accounts::quota::nodes::emit(
            &tx,
            crate::accounts::quota::nodes::Source::Book,
            Some("s1"),
        );
        let f = tokio::time::timeout(guard_core::wall::CEILING, t.next())
            .await
            .expect("该推一帧");
        match f {
            Some(Frame::Changed {
                topic: Topic::Rotation,
                key,
                body,
                ..
            }) => assert_eq!((key.as_deref(), body), (Some("s1"), None)),
            other => panic!("{other:?}"),
        }
    });
}
