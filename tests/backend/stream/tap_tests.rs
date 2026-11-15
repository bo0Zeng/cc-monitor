//! `tap.rs`：一条流连接最低优先那一路（活卡的发件箱 ＋ 几种 `changed` 推送）。活卡本身住 `live_tests.rs`。
//!
//! 经真中转走一遍的那几条住 `relay/host_tests.rs`（它们要中转的门与夹具）。

use super::*;
use crate::relay::{TapBody, TapEvent};

/// 判据自己造的一条连接：只订 `src` 里给了的那几格（其余由 [`Sources::bare`] 留空）。
fn conn(src: Sources) -> TapRx {
    attach_with(src, crate::common::time::Tz::default(), None, false)
}

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
        let mut t = conn(Sources::bare(hub.clone()));
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
        let tx = tokio::sync::broadcast::channel::<crate::plan::watch::Change>(4).0;
        let mut t = conn(Sources {
            plan: Some(tx.subscribe()),
            ..Sources::bare(std::sync::Arc::new(TapHub::default()))
        });
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
        let tx = tokio::sync::broadcast::channel::<String>(4).0;
        let mut t = conn(Sources {
            dirs: Some(tx.subscribe()),
            ..Sources::bare(std::sync::Arc::new(TapHub::default()))
        });
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
fn an_attach_row_becomes_one_changed_frame_keyed_by_its_draft() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let (tx, rx) = tokio::sync::broadcast::channel(4);
        let mut t = conn(Sources {
            attach: Some(rx),
            ..Sources::bare(std::sync::Arc::new(TapHub::default()))
        });
        let shown = crate::control::attach_upload::Desk::nothing("d-1");
        tx.send(shown.clone()).expect("有人订");
        let f = tokio::time::timeout(guard_core::wall::CEILING, t.next())
            .await
            .expect("该推一帧");
        match f {
            Some(Frame::Changed(crate::wire::Changed::Attach { key, body })) => {
                assert_eq!(key, "d-1", "key 是草稿号");
                assert_eq!(body, Some(shown), "体就是那一排");
            }
            other => panic!("{other:?}"),
        }
    });
}

#[test]
fn list_moves_become_one_changed_frame() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let node = crate::history::list_node::ListNode::default();
        let mut t = conn(Sources {
            list: Some(node.subscribe()),
            ..Sources::bare(std::sync::Arc::new(TapHub::default()))
        });
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
                conn(Sources {
                    machine: Some(tx.subscribe()),
                    ..Sources::bare(hub.clone())
                })
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
        let tx = tokio::sync::broadcast::channel::<Moved>(1).0;
        let mut t = conn(Sources {
            machine: Some(tx.subscribe()),
            ..Sources::bare(std::sync::Arc::new(TapHub::default()))
        });
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
        let tx = tokio::sync::broadcast::channel(4).0;
        let mut t = attach_with(
            Sources {
                nodes: Some(tx.subscribe()),
                bodies: fake,
                ..Sources::bare(std::sync::Arc::new(TapHub::default()))
            },
            crate::common::time::Tz::named("Asia/Shanghai").unwrap(),
            None,
            false,
        );
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

/// ★ 装板那一下在 [`attach_with`] 里：attach 行带了 `folded` 的连接（桌面壳）取到的是 `live_now`（那一处整份带着那张卡），
/// 收不到原样的 `live_card`；没带的照旧是 `live_card`。连接接在判据自己造的 [`Sources`] 上：只有判据的 hub
/// （进程级 hub 的流照真家的协议折，判据喂不了），几种 `changed` 推送一条不订。
#[test]
fn attach_with_folded_installs_the_board_and_without_it_does_not() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        for folded in [true, false] {
            let hub = std::sync::Arc::new(TapHub::new(vec![crate::agents::StreamFamily {
                face: crate::agents::fake::runs::STREAM,
                run: Some("x-run"),
                side: None,
            }]));
            let mut t = attach_with(
                Sources::bare(hub.clone()),
                crate::common::time::Tz::default(),
                None,
                folded,
            );
            hub.offer(TapEvent {
                stream: "A".into(),
                heads: Vec::new(),
                resp: 1,
                n: 0,
                body: TapBody::Data(r#"{"ev":"open","rid":"r1"}"#.to_string()),
            });
            let f = tokio::time::timeout(guard_core::wall::CEILING, t.next())
                .await
                .expect("该推一帧");
            match (folded, f) {
                (true, Some(Frame::LiveNow(l))) => {
                    assert_eq!(l.whole.sid, "A");
                    assert_eq!(l.whole.cards.len(), 1, "那一处整份带着那张卡：{l:?}");
                    assert_eq!(l.change.at().0, "A");
                }
                (false, Some(Frame::LiveCard(l))) => assert_eq!(l.sid, "A"),
                (folded, other) => panic!("folded={folded} 取到的不对：{other:?}"),
            }
        }
    });
}

/// ★ 一条连接订的东西只列在 [`Sources`] 一处、整份交进 [`TapRx`]：生产那一口 [`attach`] 接上的连接，每一格都是进程级那一条
/// （hub · 额度节点 · 计划 · 机器表 · 机器状态 · 目录变动 · 清单节点 · 小成品）。两处解构都不带 `..`：
/// `Sources` 或 `TapRx` 多一格、这里不跟就编不过 —— 连接上多订一条只能进 `Sources`，进了就得在这里对上进程级那一条。
#[test]
fn a_production_connection_holds_exactly_the_process_sources() {
    let t = attach(crate::common::time::Tz::default(), None, false);
    let TapRx {
        src,
        id: _,
        wake: _,
        tz: _,
        out: _,
        board: _,
    } = &t;
    let Sources {
        hub: h,
        nodes,
        plan,
        machines,
        machine,
        dirs,
        list,
        attach,
        bodies,
    } = src;
    let same = |got: bool, what: &str| assert!(got, "{what} 不是进程级那一条");
    same(std::sync::Arc::ptr_eq(h, &hub()), "hub");
    same(
        nodes
            .as_ref()
            .is_some_and(|r| r.same_channel(&crate::accounts::quota::nodes::bus().subscribe())),
        "nodes",
    );
    same(
        plan.as_ref()
            .is_some_and(|r| r.same_channel(&crate::plan::watch::changes().subscribe())),
        "plan",
    );
    same(
        machines
            .as_ref()
            .is_some_and(|r| r.same_channel(&crate::control::machines::changes().subscribe())),
        "machines",
    );
    same(
        machine
            .as_ref()
            .is_some_and(|r| r.same_channel(&crate::control::machine_state::changes().subscribe())),
        "machine",
    );
    same(
        dirs.as_ref()
            .is_some_and(|r| r.same_channel(&crate::stream::inbound::files_face::dir_changes())),
        "dirs",
    );
    same(
        list.as_ref()
            .is_some_and(|r| r.same_channel(&crate::history::list_node::node().subscribe())),
        "list",
    );
    same(
        attach
            .as_ref()
            .is_some_and(|r| r.same_channel(&crate::control::attach_upload::changes().subscribe())),
        "attach",
    );
    same(
        std::ptr::fn_addr_eq(
            *bodies,
            crate::wire::topic_hook::body as crate::wire::topic_hook::Bodies,
        ),
        "bodies",
    );
}

/// ★ 判据那一形（[`Sources::bare`]）只有给的那个 hub，别的一条不订：别的判据往进程级通道推的帧插不进来。
#[test]
fn bare_sources_subscribe_nothing_but_the_hub() {
    let given = std::sync::Arc::new(TapHub::default());
    let Sources {
        hub: h,
        nodes,
        plan,
        machines,
        machine,
        dirs,
        list,
        attach,
        bodies,
    } = Sources::bare(given.clone());
    assert!(std::sync::Arc::ptr_eq(&h, &given));
    assert!(
        nodes.is_none()
            && plan.is_none()
            && machines.is_none()
            && machine.is_none()
            && dirs.is_none()
            && list.is_none()
            && attach.is_none()
    );
    assert!(std::ptr::fn_addr_eq(
        bodies,
        no_bodies as crate::wire::topic_hook::Bodies
    ));
}

/// ★ 生产那一口 [`attach`] 把 attach 行的 `folded` 接进装板那一下：带了的连接（桌面壳）把一张原样的 `live_card`
/// 交成 `live_now`（那一处整份带着那张卡）、不交原样的；没带的照旧交 `live_card`。进程级 hub 喂不了，
/// 那张卡从判据自己的 hub 上一条不折的连接取来，再过生产连接交帧的那一下（[`TapRx::fold`]）。
#[test]
fn the_production_attach_wires_folded_into_the_board() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    let card = rt.block_on(async {
        let hub = std::sync::Arc::new(TapHub::new(vec![crate::agents::StreamFamily {
            face: crate::agents::fake::runs::STREAM,
            run: Some("x-run"),
            side: None,
        }]));
        let mut t = conn(Sources::bare(hub.clone()));
        hub.offer(TapEvent {
            stream: "A".into(),
            heads: Vec::new(),
            resp: 1,
            n: 0,
            body: TapBody::Data(r#"{"ev":"open","rid":"r1"}"#.to_string()),
        });
        match tokio::time::timeout(guard_core::wall::CEILING, t.next()).await {
            Ok(Some(f @ Frame::LiveCard(_))) => f,
            other => panic!("该先取到一张原样的活卡：{other:?}"),
        }
    });
    for folded in [true, false] {
        let mut t = attach(crate::common::time::Tz::default(), None, folded);
        match (folded, t.fold(card.clone())) {
            (true, Some(Frame::LiveNow(l))) => {
                assert_eq!(l.whole.sid, "A");
                assert_eq!(l.whole.cards.len(), 1, "那一处整份带着那张卡：{l:?}");
            }
            (false, Some(Frame::LiveCard(l))) => assert_eq!(l.sid, "A"),
            (folded, other) => panic!("attach(.., {folded}) 交的不对：{other:?}"),
        }
    }
}

/// ★ 机器表那一路（[`Sources::machines`]）：铃响一下 ⇒ 订了的那条连接恰好收一帧 `changed {machines}`，没订的那条收不到；
/// 铃那头没了 ⇒ 那一格摘成 `None`，连接照常睡着等（不空转：空转的话这条线程回不来，看门的那一头当场报）。
#[test]
fn a_machines_bell_reaches_only_the_subscribed_connection_and_a_gone_bell_is_dropped() {
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        rt.block_on(async {
            let hub = std::sync::Arc::new(TapHub::default());
            let tx = tokio::sync::broadcast::channel::<()>(4).0;
            let mut on = conn(Sources {
                machines: Some(tx.subscribe()),
                ..Sources::bare(hub.clone())
            });
            let mut off = conn(Sources::bare(hub.clone()));
            tx.send(()).unwrap();
            match tokio::time::timeout(guard_core::wall::CEILING, on.next()).await {
                Ok(Some(Frame::Changed(Changed::Machines {}))) => {}
                other => panic!("订了的那条该收一帧 changed {{machines}}：{other:?}"),
            }
            let quiet = std::time::Duration::from_millis(50);
            assert!(
                tokio::time::timeout(quiet, on.next()).await.is_err(),
                "响一下只该一帧"
            );
            assert!(
                tokio::time::timeout(quiet, off.next()).await.is_err(),
                "没订的那条收到了"
            );
            drop(tx);
            assert!(
                tokio::time::timeout(quiet, on.next()).await.is_err(),
                "铃没了不该推帧"
            );
            assert!(on.src.machines.is_none(), "铃没了那一格该摘掉");
        });
        let _ = done_tx.send(());
    });
    match done_rx.recv_timeout(guard_core::wall::CEILING * 2) {
        Ok(()) => {}
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("铃没了以后这条连接在空转"),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            panic!("判据那条线程失败了（见上面）")
        }
    }
}

/// ★ 要成品的连接断了，它在活卡上的那一份跟着摘：板（[`TapRx::board`]）是这条连接自己的一格、随 [`TapRx`] 放掉；
/// hub 那一侧这条连接的发件箱（活卡上的一条连接，攥着它的叫醒口）由 `Drop` 摘。一条 `folded` 的连接连上 · 板上折进一张卡 · 断开，
/// 来三回 ⇒ 每回断开后 hub 上还攥着它叫醒口的条数是零（没摘 ⇒ 壳开关 tab / 重连一回多一条，越积越多）。
#[test]
fn a_folded_connection_that_comes_and_goes_three_times_leaves_no_board_behind() {
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
        let mut left = 0usize;
        for round in 1..=3u64 {
            let mut t = attach_with(
                Sources::bare(hub.clone()),
                crate::common::time::Tz::default(),
                None,
                true,
            );
            hub.offer(TapEvent {
                stream: format!("S{round}"),
                heads: Vec::new(),
                resp: round,
                n: 0,
                body: TapBody::Data(format!(r#"{{"ev":"open","rid":"r{round}"}}"#)),
            });
            match tokio::time::timeout(guard_core::wall::CEILING, t.next()).await {
                Ok(Some(Frame::LiveNow(_))) => {}
                other => panic!("第 {round} 回该先折进板、交一帧 live_now：{other:?}"),
            }
            assert!(t.board.is_some(), "folded 的连接该有板");
            let wake = std::sync::Arc::clone(&t.wake);
            assert_eq!(
                std::sync::Arc::strong_count(&wake),
                3,
                "连着时：这条连接 · hub 上它那一条 · 这里各攥一份"
            );
            drop(t);
            left += std::sync::Arc::strong_count(&wake) - 1;
        }
        assert_eq!(
            left, 0,
            "连上又断开三回，hub 上还留着 {left} 条这几条连接的东西"
        );
    });
}
