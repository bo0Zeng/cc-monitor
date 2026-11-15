//! 附件草稿那一圈：结局表（同一种没成，在哪一步都落同一结局）· 一圈 `pump`（整排重问 · 一份一份传 · 断了停 · 撤了不落 · 全落好才交编号）。
//! 替身那台在内存里，答的都是当场就有的结果。

use super::attach::*;
use cmd_wire::attachment::{AttachCardPhase, AttachItem, AttachLocal, AttachVerdict};
use cmd_wire::files::FileClass;
use cmd_wire::transfer::Staged;
use std::collections::VecDeque;
use std::future::Future;
use std::sync::Mutex;

/// 替身答的都是当场就有的结果 ⇒ 不要运行时，空唤醒器推一下就完。
fn run<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match f.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(v) => v,
        std::task::Poll::Pending => panic!("替身不该挂起"),
    }
}

fn yes() -> AttachVerdict {
    AttachVerdict {
        ok: true,
        class: FileClass::Image,
        class_text: "图样".into(),
        why: None,
        said: None,
    }
}

fn no(said: &str) -> AttachVerdict {
    AttachVerdict {
        ok: false,
        said: Some(said.into()),
        ..yes()
    }
}

/// 替身那台：问收不收 · 送块 · 提交各照一列排好的答（排完了 ⇒ 收 · 成）；记下每一问。
#[derive(Default)]
struct Far {
    checks: Mutex<VecDeque<Result<Vec<AttachVerdict>, Ended>>>,
    stages: Mutex<VecDeque<Result<(), Ended>>>,
    commits: Mutex<VecDeque<Result<(), Ended>>>,
    /// 每次问收不收交的那几份名字。
    asked: Mutex<Vec<Vec<String>>>,
    /// 送块 · 提交的那几份名字。
    staged: Mutex<Vec<String>>,
    committed: Mutex<Vec<String>>,
    dropped: Mutex<Vec<String>>,
    /// 送块那一下顺手做的事（撤掉 · 加一份 …），在答之前。
    during_stage: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

fn take<T>(q: &Mutex<VecDeque<T>>) -> Option<T> {
    q.lock().unwrap().pop_front()
}

impl Upload for Far {
    async fn check(&self, items: &[AttachItem]) -> Result<Vec<AttachVerdict>, Ended> {
        self.asked
            .lock()
            .unwrap()
            .push(items.iter().map(|i| i.name.clone()).collect());
        take(&self.checks).unwrap_or_else(|| Ok(items.iter().map(|_| yes()).collect()))
    }
    async fn stage(
        &self,
        card: &Card,
        moved: &mut (dyn FnMut(u64) + Send),
    ) -> Result<Staged, Ended> {
        self.staged.lock().unwrap().push(card.name.clone());
        if let Some(f) = self.during_stage.lock().unwrap().take() {
            f();
        }
        moved(card.bytes);
        take(&self.stages).unwrap_or(Ok(())).map(|()| Staged {
            key: card.key.clone(),
            chunks: 1,
            bytes: card.bytes,
            sha256: "00".into(),
        })
    }
    async fn commit(&self, card: &Card, _staged: &Staged) -> Result<String, Ended> {
        self.committed.lock().unwrap().push(card.name.clone());
        take(&self.commits)
            .unwrap_or(Ok(()))
            .map(|()| format!("id-{}", card.name))
    }
    async fn drop_landed(&self, id: &str) {
        self.dropped.lock().unwrap().push(id.to_string());
    }
    fn changed(&self, _tray: &Tray) {}
}

fn tray(names: &[&str]) -> Mutex<Tray> {
    let mut t = Tray::default();
    for n in names {
        t.add(format!("k-{n}"), n.to_string(), 10, false);
    }
    Mutex::new(t)
}

fn phases(t: &Mutex<Tray>) -> Vec<AttachCardPhase> {
    t.lock()
        .unwrap()
        .cards()
        .iter()
        .map(|c| c.phase.clone())
        .collect()
}

fn failed(said: &str) -> AttachCardPhase {
    AttachCardPhase::Failed { said: said.into() }
}

fn refused(said: &str) -> AttachCardPhase {
    AttachCardPhase::Refused { said: said.into() }
}

#[test]
fn the_outcome_table_is_one_answer_per_ending() {
    let cases = [
        (
            Ended::Refused {
                said: "超上限".into(),
            },
            Some(refused("超上限")),
            false,
        ),
        (Ended::Cut, Some(AttachCardPhase::Queued), true),
        (
            Ended::Failed {
                said: "盘满了呀".into(),
            },
            Some(failed("盘满了呀")),
            false,
        ),
        (Ended::Unreadable, Some(AttachCardPhase::Lost), false),
        (Ended::Stopped, None, false),
    ];
    for (end, phase, wait) in cases {
        assert_eq!(settle(&end), Settled { phase, wait }, "{end:?}");
    }
}

/// 同一种没成，落在送块那一步与提交那一步、问收不收那一步，卡上都是结局表那一格。
#[test]
fn the_same_ending_lands_the_same_wherever_it_happens() {
    for end in [
        Ended::Cut,
        Ended::Failed {
            said: "那台的错".into(),
        },
        Ended::Unreadable,
    ] {
        let want = settle(&end).phase.unwrap();
        let at_stage = Far::default();
        at_stage.stages.lock().unwrap().push_back(Err(end.clone()));
        let at_commit = Far::default();
        at_commit
            .commits
            .lock()
            .unwrap()
            .push_back(Err(end.clone()));
        let at_check = Far::default();
        at_check.checks.lock().unwrap().push_back(Err(end.clone()));
        for far in [at_stage, at_commit, at_check] {
            let t = tray(&["a.png"]);
            run(pump(&t, &far));
            assert_eq!(phases(&t), vec![want.clone()], "{end:?}");
            assert_eq!(t.lock().unwrap().cut(), settle(&end).wait, "{end:?}");
        }
    }
}

#[test]
fn refused_ones_are_marked_and_the_rest_still_go() {
    let far = Far::default();
    far.checks
        .lock()
        .unwrap()
        .push_back(Ok(vec![no("超上限"), yes()]));
    let t = tray(&["big.bin", "b.png"]);
    run(pump(&t, &far));
    assert_eq!(phases(&t), vec![refused("超上限"), AttachCardPhase::Landed]);
    assert_eq!(*far.staged.lock().unwrap(), vec!["b.png".to_string()]);
}

#[test]
fn the_whole_row_is_asked_before_each_upload_except_refused_ones() {
    let far = Far::default();
    far.checks
        .lock()
        .unwrap()
        .push_back(Ok(vec![no("超上限"), yes(), yes()]));
    let t = tray(&["x", "a", "b"]);
    run(pump(&t, &far));
    assert_eq!(
        *far.asked.lock().unwrap(),
        vec![
            vec!["x".to_string(), "a".into(), "b".into()],
            vec!["a".to_string(), "b".into()],
        ]
    );
    assert_eq!(
        *far.committed.lock().unwrap(),
        vec!["a".to_string(), "b".into()]
    );
}

#[test]
fn a_cut_stops_the_round_and_up_starts_it_over() {
    let far = Far::default();
    far.stages.lock().unwrap().push_back(Err(Ended::Cut));
    let t = tray(&["a", "b"]);
    run(pump(&t, &far));
    assert_eq!(
        phases(&t),
        vec![AttachCardPhase::Queued, AttachCardPhase::Queued]
    );
    assert_eq!(far.staged.lock().unwrap().len(), 1, "断了不再往下传");
    // 断着再叫一圈：不动。
    run(pump(&t, &far));
    assert_eq!(far.staged.lock().unwrap().len(), 1);
    t.lock().unwrap().up();
    run(pump(&t, &far));
    assert_eq!(
        phases(&t),
        vec![AttachCardPhase::Landed, AttachCardPhase::Landed]
    );
    assert_eq!(
        *far.staged.lock().unwrap(),
        vec!["a".to_string(), "a".into(), "b".into()]
    );
}

#[test]
fn a_failed_check_fails_the_queued_and_retry_requeues_them() {
    let far = Far::default();
    far.checks.lock().unwrap().push_back(Err(Ended::Failed {
        said: "那台的错".into(),
    }));
    let t = tray(&["a", "b"]);
    run(pump(&t, &far));
    assert_eq!(phases(&t), vec![failed("那台的错"), failed("那台的错")]);
    t.lock().unwrap().retry();
    run(pump(&t, &far));
    assert_eq!(
        phases(&t),
        vec![AttachCardPhase::Landed, AttachCardPhase::Landed]
    );
}

#[test]
fn a_card_removed_while_moving_settles_nothing_and_its_landed_id_is_dropped() {
    let far = Far::default();
    let t: &'static Mutex<Tray> = Box::leak(Box::new(tray(&["a", "b"])));
    *far.during_stage.lock().unwrap() = Some(Box::new(move || {
        let gone = t.lock().unwrap().remove("k-a").expect("在");
        assert_eq!(gone.phase, AttachCardPhase::Moving);
    }));
    run(pump(t, &far));
    let names: Vec<String> = t
        .lock()
        .unwrap()
        .cards()
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert_eq!(names, vec!["b".to_string()]);
    assert_eq!(phases(t), vec![AttachCardPhase::Landed]);
    assert_eq!(*far.dropped.lock().unwrap(), vec!["id-a".to_string()]);
}

#[test]
fn ids_are_handed_only_when_every_card_landed() {
    let far = Far::default();
    far.stages.lock().unwrap().push_back(Ok(()));
    far.stages.lock().unwrap().push_back(Err(Ended::Unreadable));
    let t = tray(&["a", "b"]);
    run(pump(&t, &far));
    assert!(!t.lock().unwrap().ready());
    assert_eq!(t.lock().unwrap().take(), None);
    t.lock().unwrap().retry();
    run(pump(&t, &far));
    assert_eq!(
        t.lock().unwrap().take(),
        Some(vec!["id-a".to_string(), "id-b".into()])
    );
    assert!(t.lock().unwrap().cards().is_empty());
    assert_eq!(Tray::default().take(), Some(vec![]));
}

#[test]
fn restored_cards_keep_their_ids_and_only_the_unlanded_go_again() {
    let far = Far::default();
    let mut t = Tray::default();
    t.restore("k-a".into(), "a".into(), 10, Some("id-old".into()));
    t.restore("k-b".into(), "b".into(), 10, None);
    let t = Mutex::new(t);
    run(pump(&t, &far));
    assert_eq!(*far.staged.lock().unwrap(), vec!["b".to_string()]);
    assert_eq!(
        t.lock().unwrap().take(),
        Some(vec!["id-old".to_string(), "id-b".into()])
    );
}

#[test]
fn the_want_says_offline_only_when_queued_ones_wait_on_a_cut() {
    let far = Far::default();
    far.stages.lock().unwrap().push_back(Err(Ended::Cut));
    let t = tray(&["a"]);
    assert_eq!(
        t.lock().unwrap().want(Some("devbox 离线")).offline,
        None,
        "没断着不说"
    );
    run(pump(&t, &far));
    let mut g = t.lock().unwrap();
    let w = g.want(Some("devbox 离线"));
    assert_eq!(w.offline.as_deref(), Some("devbox 离线"));
    assert_eq!(w.cards[0].name, "a");
    g.local(Some(AttachLocal::NoRoom));
    assert_eq!(g.want(None).local, Some(AttachLocal::NoRoom));
    g.add("k-c".into(), "c".into(), 1, false);
    assert_eq!(g.want(None).local, None, "加一份就收掉本机这边的错");
}

/// 已落好的照样列进整排一起问（总量照算），可那台这一次不收也不动它：编号还挂着、不落「不收」。
#[test]
fn a_landed_card_is_asked_along_but_never_unlanded() {
    let far = Far::default();
    let t = tray(&["a"]);
    run(pump(&t, &far));
    t.lock().unwrap().add("k-b".into(), "b".into(), 10, false);
    far.checks
        .lock()
        .unwrap()
        .push_back(Ok(vec![no("总量超了"), yes()]));
    run(pump(&t, &far));
    assert_eq!(
        far.asked.lock().unwrap()[1],
        vec!["a".to_string(), "b".into()]
    );
    assert_eq!(
        phases(&t),
        vec![AttachCardPhase::Landed, AttachCardPhase::Landed]
    );
    assert_eq!(
        t.lock().unwrap().take(),
        Some(vec!["id-a".to_string(), "id-b".into()])
    );
}

/// 关了那一排：在传的那一份落好了也让那台删掉，剩下的不再问、不再传。
#[test]
fn a_closed_tray_stops_the_round_and_drops_what_lands_after() {
    let far = Far::default();
    let t: &'static Mutex<Tray> = Box::leak(Box::new(tray(&["a", "b"])));
    *far.during_stage.lock().unwrap() = Some(Box::new(move || t.lock().unwrap().close()));
    run(pump(t, &far));
    assert!(t.lock().unwrap().cards().is_empty());
    assert_eq!(*far.staged.lock().unwrap(), vec!["a".to_string()]);
    assert_eq!(far.asked.lock().unwrap().len(), 1);
    assert_eq!(*far.dropped.lock().unwrap(), vec!["id-a".to_string()]);
}
