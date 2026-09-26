//! 〔LOC1b · 第四波 4D〕本机活会话表（`session_map::LocalTable`）的判据。
//!
//! 要求住址：`INVARIANTS §40` 逐字「我的目的就是把本地当成不走 ssh 的远端」· `设计/01 §1.1` 逐字「一切判定都在后端」·
//! `设计/30 §3.5.7a` 逐字「`Unseen` 不许被显示成已结束」。
//! 从前这里钉的是 monitor 自己那份判活（pidfile 扫描 · `/proc` 身份 · 心跳 · diff），那份实现删了，判据随之换成
//! 「帧 ⇒ 表 ⇒ emitter」这一跳；「本机流断落说不清、与远端断连 flush 同一份期望」两形各跑一遍。
use super::*;

fn entry(cwd: &str, kind: Option<&str>, status: Option<&str>, pid: Option<u32>) -> LiveEntry {
    LiveEntry {
        cwd: Some(cwd.into()),
        kind: kind.map(str::to_string),
        name: None,
        status: status.map(str::to_string),
        waiting_for: None,
        pid,
    }
}

fn added(t: &mut LocalTable, sid: &str, e: LiveEntry) -> Vec<Out> {
    t.step(Lifecycle::Added {
        sid: sid.into(),
        entry: e,
    })
}

/// 宣告 ⇒ 进表 ＋ 交一件 `added`（带初始灯）；表的两个快照都看得见它，清单按 (cwd, sid) 排。
#[test]
fn an_announcement_enters_the_table_and_reaches_the_emitter_with_its_initial_light() {
    let mut t = LocalTable::default();
    let out = added(
        &mut t,
        "b",
        entry("/p", Some("interactive"), Some("busy"), Some(7)),
    );
    assert_eq!(
        out,
        vec![Out::Change(SessionChange {
            added: vec!["b".into()],
            removed: vec![],
            status_changed: vec![SessionActivity {
                session_id: "b".into(),
                status: Some("busy".into()),
                waiting_for: None,
            }],
            then_listed: None,
        })]
    );
    added(&mut t, "a", entry("/p", None, None, None));
    added(&mut t, "c", entry("/a", Some("bg"), None, None));
    assert!(t.is_active("b") && !t.is_active("zz"));
    assert_eq!(t.lookup("b").and_then(|e| e.pid), Some(7));
    let order: Vec<String> = t
        .snapshot_active()
        .into_iter()
        .map(|a| a.session_id)
        .collect();
    assert_eq!(order, vec!["c", "a", "b"], "按 (cwd, sid) 排，跨启动稳定");
    assert_eq!(t.snapshot_activity().len(), 3);
}

/// 灯只改表里有的；表里没有的（例：被藏起来的 bg 会话、流断之后迟到的）不交。
#[test]
fn a_light_change_only_lands_on_a_session_the_table_knows() {
    let mut t = LocalTable::default();
    assert!(t
        .step(Lifecycle::Status {
            sid: "x".into(),
            status: Some("idle".into()),
            waiting_for: None
        })
        .is_empty());
    added(&mut t, "x", entry("/p", None, Some("busy"), None));
    let out = t.step(Lifecycle::Status {
        sid: "x".into(),
        status: Some("waiting".into()),
        waiting_for: Some("permission prompt".into()),
    });
    assert_eq!(out.len(), 1);
    assert_eq!(t.lookup("x").unwrap().status.as_deref(), Some("waiting"));
}

/// 摘掉：cause 原样交（后端说了算，monitor 不猜）；表里没有的不交。
#[test]
fn a_removal_passes_the_backends_cause_through_untouched() {
    for cause in [RemovalCause::Gone, RemovalCause::Superseded] {
        let mut t = LocalTable::default();
        added(&mut t, "x", entry("/p", None, None, None));
        let out = t.step(Lifecycle::Removed {
            sid: "x".into(),
            cause,
        });
        assert_eq!(
            out,
            vec![Out::Change(SessionChange {
                removed: vec![RemovedSid {
                    sid: "x".into(),
                    cause
                }],
                ..Default::default()
            })]
        );
        assert!(!t.is_active("x"));
        assert!(t
            .step(Lifecycle::Removed {
                sid: "x".into(),
                cause
            })
            .is_empty());
    }
}

/// 「报完了清单」：交 `Listed`、表记下；流一断就不算数。
#[test]
fn the_list_is_complete_only_until_the_stream_ends() {
    let mut t = LocalTable::default();
    assert!(t.listed_active().is_none());
    assert_eq!(t.step(Lifecycle::Listed), vec![Out::Listed]);
    assert!(t.listed_active().is_some());
    t.step(Lifecycle::StreamEnded { idle: vec![] });
    assert!(t.listed_active().is_none());
}

/// 〔主会话 09-25 补〕**流断 ⇒ 说不清，两形同一份期望**：本机那条流断了（表的流断臂）与远端断连 flush
/// （`ssh_source::disconnect_removals`）交出来的东西，逐条都是 `Unseen`、人群 == 活会话 ∪ 可重连（两向相等）。
/// 反空真：两形都至少一条；正控：同一个识别器认得出一条 `Gone`（否则「全是 Unseen」是空真）。
#[test]
fn a_dropped_stream_leaves_every_session_unseen_the_same_way_on_both_sides() {
    fn expect_all_unseen(removed: &[RemovedSid], want: &[&str]) {
        let got: std::collections::BTreeSet<&str> =
            removed.iter().map(|r| r.sid.as_str()).collect();
        let want: std::collections::BTreeSet<&str> = want.iter().copied().collect();
        assert_eq!(got, want, "流断那一摞的人群不对");
        assert!(!removed.is_empty());
        assert!(
            removed.iter().all(|r| r.cause == RemovalCause::Unseen),
            "流断那一摞里有不是 `Unseen` 的 ⇒ 前端会把它显示成已结束（`设计/30 §3.5.7a` 禁）：{removed:?}"
        );
    }
    // 正控：识别器认得出一条 `Gone`。
    assert!(!matches!(RemovedSid::gone("g").cause, RemovalCause::Unseen));

    // 本机那一形：表里两条活的 ＋ 一条可重连。
    let mut t = LocalTable::default();
    added(&mut t, "a", entry("/p", None, None, None));
    added(&mut t, "b", entry("/p", None, None, None));
    let out = t.step(Lifecycle::StreamEnded {
        idle: vec!["c".into()],
    });
    let [Out::Change(c)] = out.as_slice() else {
        panic!("流断应当恰好交一件：{out:?}")
    };
    expect_all_unseen(&c.removed, &["a", "b", "c"]);
    assert!(c.added.is_empty() && c.status_changed.is_empty());
    assert!(
        !t.is_active("a") && !t.is_active("b"),
        "流断之后表清空（下一条流重新宣告）"
    );

    // 远端那一形：同一个函数、同一份期望。
    let remote = crate::ssh_source::disconnect_removals(
        vec!["a".to_string(), "b".to_string()],
        vec!["c".to_string()],
    );
    expect_all_unseen(&remote, &["a", "b", "c"]);
    assert_eq!(
        c.removed, remote,
        "本机流断与远端断连 flush 交出来的不是同一摞"
    );
}

/// 流断时表是空的、也没有可重连 ⇒ 什么都不交（不发一件空的 change）。
#[test]
fn a_dropped_stream_with_nothing_live_sends_nothing() {
    let mut t = LocalTable::default();
    assert!(t.step(Lifecycle::StreamEnded { idle: vec![] }).is_empty());
}

/// 本机判活的实现**不在 monitor 里了**：生产段零处 notify / `/proc` / 进程存活探测 / pidfile 目录。
/// 正控：同一组针认得出旧实现的形状（否则零命中是空真）。
#[test]
fn the_monitor_no_longer_judges_local_liveness_itself() {
    let needles = [
        "notify_debouncer_mini",
        "/proc/",
        "is_process_alive",
        "GetProcessTimes",
        "liveness_dir(",
        "load_with_changes(",
    ];
    let hits = |s: &str| needles.iter().filter(|n| s.contains(*n)).count();
    let me = guard_core::production_code(include_str!("../../src/bridge/src/session_map.rs"));
    let lib = guard_core::production_code(include_str!("../../src/bridge/src/lib.rs"));
    assert_eq!(hits(&me), 0, "session_map.rs 又长出了自己判活的那一形");
    assert_eq!(hits(&lib), 0, "lib.rs 又在起 monitor 自己的判活");
    let old = concat!(
        "use notify_debouncer_mini::new_debouncer; let alive = ",
        "is_process_alive(pid) && read(\"/proc/\"); let d = adapter::liveness_dir(&c); ",
        "SessionMap::load_with_changes(d)"
    );
    assert_eq!(hits(old), 5);
}

/// `list_active_sessions` 的答案：清单没报完 ⇒ 不给（`None`，命令明拒）；报完 ⇒ 给整份；流一断又不给。
/// 守的是 `设计/30 §3.5.7a`「那台还没报完清单 ⇒ 说不清」：半截的清单会把还没宣告到的活会话说成已结束。
#[test]
fn the_active_list_is_only_handed_out_once_it_is_complete() {
    let mut t = LocalTable::default();
    added(&mut t, "a", entry("/p", None, None, None));
    assert_eq!(t.listed_active(), None, "没报完就交了清单");
    t.step(Lifecycle::Listed);
    let got: Vec<String> = t
        .listed_active()
        .expect("报完了却不给")
        .into_iter()
        .map(|a| a.session_id)
        .collect();
    assert_eq!(got, vec!["a"]);
    t.step(Lifecycle::StreamEnded { idle: vec![] });
    assert_eq!(t.listed_active(), None, "流断之后还在交旧清单");
}

/// `feed` 这一跳（进程级那张表 ＋ 出口 ＋ 「报完了清单」那本账）：报完 ⇒ 账上有 `<local>`、出口收到 `Listed`；
/// 流断 ⇒ 账上摘掉、出口收到那一摞 `Unseen`。F5 对账按这本账分「已结束 / 说不清」（与远端同一本）。
/// ⚠ 用的是进程级的那一张与那一个出口：本条是全测试进程里唯一装出口、唯一喂 `<local>` 的地方。
#[test]
fn feeding_keeps_the_listed_book_and_the_emitter_in_step() {
    let local = crate::backend::control::inbound_client::LOCAL_ORIGIN;
    let (tx, rx) = std::sync::mpsc::channel();
    install_sink(tx);
    feed(Lifecycle::Added {
        sid: "feed-a".into(),
        entry: entry("/p", None, None, None),
    });
    feed(Lifecycle::Listed);
    assert!(
        crate::ssh_source::listed_origins().contains(local),
        "报完了，账上却没有本机"
    );
    assert!(local_table_has("feed-a"));
    feed(Lifecycle::StreamEnded { idle: vec![] });
    assert!(
        !crate::ssh_source::listed_origins().contains(local),
        "流断了，账上还记着本机报完了"
    );
    let got: Vec<Out> = rx.try_iter().collect();
    assert!(
        matches!(got.as_slice(), [Out::Change(_), Out::Listed, Out::Change(c)]
        if c.removed.len() == 1 && c.removed[0].cause == RemovalCause::Unseen),
        "{got:?}"
    );
}

fn local_table_has(sid: &str) -> bool {
    local().read().is_active(sid)
}

/// 接线：本机那条流的消费者真的把每件东西先交本机起停核、再喂表（各恰好一处）。
/// 本机起停核与表都是纯的、各有真值表 —— 这一跳要是断了，两边照绿、本机 tab 永远不结束（本仓「判据不在执行链上」）。
#[test]
fn the_local_consumer_feeds_the_table_exactly_once() {
    let ss = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    let at = ss
        .find("pub(crate) async fn consume_local(")
        .expect("找不到本机消费者");
    let body = &ss[at..at + ss[at..].find("\n}\n").expect("函数尾")];
    assert_eq!(
        body.matches("local_lifecycle(").count(),
        1,
        "本机消费者没（或不止一处）交本机起停核"
    );
    assert_eq!(
        body.matches("crate::session_map::feed(").count(),
        1,
        "本机消费者没（或不止一处）喂表"
    );
    let core = body.find("local_lifecycle(").unwrap();
    let step = body.find("local_step(").expect("本机消费者里没有内容分派");
    assert!(
        core < step,
        "起停核排在内容分派之后 ⇒ 它读到的 `hidden` 已被这一件改过"
    );
}
