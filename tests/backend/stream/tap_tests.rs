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
            run: Some("x-run"),
            side: None,
        }]));
        let mut t = attach_rx(hub.clone());
        t.watch_handle().watch(&["A".to_string()]);
        for (sid, resp) in [("B", 2u64), ("A", 1u64)] {
            hub.offer(TapEvent {
                stream: sid.into(),
                heads: Vec::new(),
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

/// 0 号那一件带来的请求头进会话的请求标记（会话事实定上下文上限时读）：带了扩展上下文那一项 ⇒ 真；
/// 后面的事件不再带头、也不把它抹掉。
#[test]
fn the_first_event_heads_land_in_the_session_marks() {
    let hub = TapHub::new(crate::agents::stream_families());
    let ev = |sid: &str, n: u64, heads: Vec<(&'static str, String)>| TapEvent {
        stream: sid.into(),
        heads,
        resp: 7,
        n,
        body: TapBody::Data("{}".into()),
    };
    hub.offer(ev(
        "tm-wide",
        0,
        vec![("anthropic-beta", "a-1, context-1m-2025-08-07".into())],
    ));
    hub.offer(ev("tm-wide", 1, Vec::new()));
    hub.offer(ev("tm-std", 0, vec![("anthropic-beta", "a-1".into())]));
    assert_eq!(crate::observe::relay_marks::seen("tm-wide"), Some(true));
    assert_eq!(crate::observe::relay_marks::seen("tm-std"), Some(false));
    assert_eq!(crate::observe::relay_marks::seen("tm-never"), None);
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
            Some(Frame::Changed(crate::wire::Changed::Plan { key, rev, body })) => assert_eq!(
                (key.as_str(), rev.as_deref(), body),
                (
                    "/w",
                    Some("r2"),
                    Some(cmd_wire::frame::PlanNeeds { needs: 3 })
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

/// ★ 正被浏览的某个目录有了动静 ⇒ 这条流连接推一帧 `changed {files_dir, key: 那个目录}`（不带小成品）；没再动 ⇒ 不再推。
#[test]
fn a_moved_directory_becomes_one_changed_frame() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let mut t = attach_rx(std::sync::Arc::new(TapHub::default()));
        let tx = tokio::sync::broadcast::channel::<String>(4).0;
        t.dirs = Some(tx.subscribe());
        tx.send("/w/src".to_string()).unwrap();
        let f = tokio::time::timeout(guard_core::wall::CEILING, t.next())
            .await
            .expect("该推一帧");
        match f {
            Some(Frame::Changed(crate::wire::Changed::FilesDir { key })) => {
                assert_eq!(key, "/w/src")
            }
            other => panic!("{other:?}"),
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), t.next())
                .await
                .is_err()
        );
    });
}

/// ★ 会话清单的节点响了 ⇒ 这条流连接推一帧 `changed {history_list}`（不带 `key` · 小成品）；
/// 连着响几下、还没取 ⇒ 只一帧（这条连接没在看清单、不重问，也只收这一帧）；没再响 ⇒ 不再推。
#[test]
fn list_moves_become_one_changed_frame() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let mut t = attach_rx(std::sync::Arc::new(TapHub::default()));
        let node = crate::history::list_node::ListNode::default();
        t.list = Some(node.subscribe());
        node.moved(Some("a"));
        node.moved(Some("b"));
        node.moved(None);
        let f = tokio::time::timeout(guard_core::wall::CEILING, t.next())
            .await
            .expect("该推一帧");
        match f {
            Some(Frame::Changed(crate::wire::Changed::HistoryList {})) => {}
            other => panic!("{other:?}"),
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), t.next())
                .await
                .is_err(),
            "连着响的几下只出一帧"
        );
        node.moved(Some("c"));
        assert!(
            tokio::time::timeout(guard_core::wall::CEILING, t.next())
                .await
                .is_ok(),
            "再响 ⇒ 再一帧"
        );
    });
}

/// ★ 节点 ⇒ 帧（`Node::changed`）：事实那几种 ⇒ `changed {quota}`；带会话的 ⇒ 再加 `changed {rotation, key: sid}`；
/// 策略那一种 ⇒ `changed {rotation_strategies}`（续令牌没成那一声需手动的铃由发端敲，不在这里）；落后丢件 ⇒ 不带会话的那两帧。
#[test]
fn each_kind_of_node_becomes_the_changed_frames_it_stands_for() {
    use crate::accounts::quota::nodes::{Node, Source};
    let topics = |cs: Vec<Changed>| -> Vec<(Topic, Option<String>)> {
        cs.iter()
            .map(|c| (c.topic(), c.key().map(str::to_string)))
            .collect()
    };
    let of = |source, sid: Option<&str>| {
        topics(
            Node {
                source,
                sid: sid.map(str::to_string),
            }
            .changed(),
        )
    };
    let q = vec![(Topic::Quota, None)];
    for s in [
        Source::Reading,
        Source::QuotaFile,
        Source::Logins,
        Source::Login,
    ] {
        assert_eq!(of(s, None), q, "{s:?}");
    }
    assert_eq!(
        of(Source::Refused, Some("s9")),
        vec![(Topic::Quota, None), (Topic::Rotation, Some("s9".into()))]
    );
    assert_eq!(
        of(Source::Book, Some("s1")),
        vec![(Topic::Rotation, Some("s1".into()))]
    );
    assert_eq!(
        of(Source::Strategies, None),
        vec![(Topic::RotationStrategies, None)]
    );
    assert_eq!(
        topics(Node::lagged()),
        vec![(Topic::Quota, None), (Topic::RotationStrategies, None)]
    );
}

/// ★ 一次机器变动推到 N 条连接：各台那几句没变（推的那一侧判过、项里没带）⇒ 每条 0 帧 `changed {empty}`，
/// 只有那台的 `changed {machine}`；变了（项里带着）⇒ 每条恰好再一帧 `changed {empty}`，体就是带来的那几句。
#[test]
fn a_machine_move_pushes_the_fleet_lines_only_when_they_changed() {
    use crate::control::machine_state::Moved;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let hub = std::sync::Arc::new(TapHub::default());
        let tx = tokio::sync::broadcast::channel::<Moved>(8).0;
        let mut conns: Vec<TapRx> = (0..3)
            .map(|_| {
                let mut t = attach_rx(hub.clone());
                t.machine = Some(tx.subscribe());
                t
            })
            .collect();
        let state = crate::control::machine_state::read()
            .remove(cmd_wire::machine::LOCAL)
            .expect("本机那一行");
        let fleet = crate::control::machine_state::fleet();
        for (with, want) in [(None, 0usize), (Some(fleet.clone()), 1)] {
            tx.send(Moved {
                machine: "m1".into(),
                state: state.clone(),
                fleet: with,
            })
            .unwrap();
            for t in &mut conns {
                match tokio::time::timeout(guard_core::wall::CEILING, t.next()).await {
                    Ok(Some(Frame::Changed(Changed::Machine { key, .. }))) => {
                        assert_eq!(key, "m1")
                    }
                    other => panic!("该先推那台：{other:?}"),
                }
                let mut empties = 0;
                loop {
                    match tokio::time::timeout(std::time::Duration::from_millis(50), t.next()).await
                    {
                        Ok(Some(Frame::Changed(Changed::Empty { body }))) => {
                            assert_eq!(body.as_ref(), Some(&fleet), "体不是带来的那几句");
                            empties += 1;
                        }
                        Ok(other) => panic!("多推了别的：{other:?}"),
                        Err(_) => break,
                    }
                }
                assert_eq!(empties, want, "这条连接推了几帧 changed {{empty}}");
            }
        }
    });
}

/// ★ 机器那一路落后丢了几件（带着新句子的那一件可能就在里面）⇒ 这条连接照此刻的各台那几句补一帧 `changed {empty}`，
/// 不等那几句再变一次。
#[test]
fn a_lagged_machine_channel_resends_the_fleet_lines_of_now() {
    use crate::control::machine_state::Moved;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let mut t = attach_rx(std::sync::Arc::new(TapHub::default()));
        let tx = tokio::sync::broadcast::channel::<Moved>(1).0;
        t.machine = Some(tx.subscribe());
        let state = crate::control::machine_state::read()
            .remove(cmd_wire::machine::LOCAL)
            .expect("本机那一行");
        for _ in 0..3 {
            tx.send(Moved {
                machine: "m1".into(),
                state: state.clone(),
                fleet: None,
            })
            .unwrap();
        }
        let mut empties = 0;
        loop {
            match tokio::time::timeout(std::time::Duration::from_millis(50), t.next()).await {
                Ok(Some(Frame::Changed(Changed::Empty { body }))) => {
                    assert!(body.is_some(), "补的那一帧没带句子");
                    empties += 1;
                }
                Ok(Some(Frame::Changed(Changed::Machine { .. }))) => {}
                Ok(other) => panic!("多推了别的：{other:?}"),
                Err(_) => break,
            }
        }
        assert_eq!(empties, 1, "落后之后该补一帧 changed {{empty}}");
    });
}

/// 现算得出小成品的主题（额度账）换成这条连接的 `bodies` 交的那一臂，时区照这条连接看的那一台；
/// 现算不出的（轮换那一帧：客户端照 `{target: {sid}}` 重问 `rotation-view`）照旧是发端造的那一臂。
#[test]
fn a_computed_topic_carries_the_arm_the_hook_gives() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let mut t = attach_rx(std::sync::Arc::new(TapHub::default()));
        // 只认额度账；时区写进一个看得见的格里（换成那条连接的时区 ⇒ 这一格跟着变）。
        fn fake(
            topic: Topic,
            key: Option<&str>,
            tz: &crate::common::time::Tz,
        ) -> Option<crate::wire::Changed> {
            (topic == Topic::Quota && key.is_none()).then(|| crate::wire::Changed::Tasks {
                key: tz.iana_name(),
                body: None,
            })
        }
        t.bodies = fake;
        t.tz = crate::common::time::Tz::named("Asia/Shanghai").unwrap();
        let tx = tokio::sync::broadcast::channel(4).0;
        t.nodes = Some(tx.subscribe());
        crate::accounts::quota::nodes::emit(
            &tx,
            crate::accounts::quota::nodes::Source::Refused,
            Some("s1"),
        );
        let mut got = Vec::new();
        for _ in 0..2 {
            match tokio::time::timeout(guard_core::wall::CEILING, t.next()).await {
                Ok(Some(Frame::Changed(c))) => got.push(c),
                other => panic!("该推一帧：{other:?}"),
            }
        }
        assert_eq!(
            got,
            vec![
                crate::wire::Changed::Tasks {
                    key: "Asia/Shanghai".to_string(),
                    body: None
                },
                crate::wire::Changed::Rotation {
                    key: "s1".to_string()
                },
            ]
        );
    });
}

/// ★ 流里说出这一发用量的那几件经 hub 记进进程里那一本每发用量（额度账量「1% ≈ 多少」只从这里数到用量）。
/// 号名与会话名每个进程各一个，不跟别的判据在那一本里串。
#[test]
fn usage_in_the_stream_reaches_the_process_spend_book() {
    let hub = TapHub::default();
    let sid = format!("s-tap-spend-{}", std::process::id());
    let acct = format!("tap-spend-{}", std::process::id());
    let book = crate::accounts::quota::spend::book();
    let w = |used: f64| crate::agents::QuotaWindow {
        name: "five_hour".into(),
        used: Some(used),
        resets_at: Some(2_000_000_000),
        warned_at: None,
    };
    let send = |input: u64| {
        book.answered(Some(&sid), "claude-code", &acct);
        hub.offer(TapEvent {
            stream: sid.clone(),
            heads: Vec::new(),
            resp: 0,
            n: 0,
            body: TapBody::Data(format!(
                r#"{{"type":"message_start","message":{{"usage":{{"input_tokens":{input}}}}}}}"#
            )),
        });
    };
    book.pace("claude-code", &acct, &w(0.10));
    send(1_000);
    book.pace("claude-code", &acct, &w(0.11));
    for _ in 0..5 {
        send(10_000);
    }
    assert_eq!(book.pace("claude-code", &acct, &w(0.15)), Some(12_500));
}
