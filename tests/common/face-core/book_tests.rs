//! 会话簿的折法：同一串帧（`book-frames.jsonl`）折出的整份就是 `book.golden.json` —— 壳与手机两头的判据对着同一份。
use super::*;
use cmd_wire::frame::decode;

const FRAMES: &str = include_str!("book-frames.jsonl");
const GOLDEN: &str = include_str!("book.golden.json");

fn folded(frames: &str) -> SessionBook {
    let mut b = SessionBook::default();
    for line in frames.lines().filter(|l| !l.trim().is_empty()) {
        let f = decode::<serde_json::Value>(line).expect("夹具里每一行都是一帧");
        b.fold(&f);
    }
    b
}

#[test]
fn the_fixture_frames_fold_into_the_golden_book() {
    let got = serde_json::to_value(folded(FRAMES)).expect("整份写得出");
    let want: serde_json::Value = serde_json::from_str(GOLDEN).expect("golden 是 JSON");
    assert_eq!(got, want);
}

#[test]
fn a_session_sits_in_one_place_and_reads_its_face_from_there() {
    let b = folded(FRAMES);
    for sid in b.live.keys() {
        assert!(!b.left.contains_key(sid), "{sid} 同时在活的与走了的里");
    }
    assert_eq!(
        b.face("a").map(|f| f.order),
        Some(0),
        "活的 ⇒ 最新状态那一张"
    );
    assert_eq!(
        b.face("c").map(|f| f.settled),
        Some(true),
        "走了的 ⇒ 去向那一张"
    );
    assert!(b.face("zz").is_none(), "没宣告过的状态帧不进簿");
    let order: Vec<&str> = b.seats().map(|(s, _)| s.as_str()).collect();
    assert_eq!(order, ["a", "b", "c", "d"]);
    assert!(matches!(b.seat("b"), Some(Seat::Left(_))));
}

#[test]
fn a_left_session_announced_again_is_live_again_and_other_frames_do_not_touch_the_book() {
    let mut b = folded(FRAMES);
    let again = FRAMES
        .lines()
        .next()
        .unwrap()
        .replace("\"sid\":\"a\"", "\"sid\":\"b\"");
    assert!(b.fold(&decode::<serde_json::Value>(&again).unwrap()));
    assert!(b.live.contains_key("b") && !b.left.contains_key("b"));
    let before = b.clone();
    let other = r#"{"kind":"changed","topic":"tasks"}"#;
    if let Ok(f) = decode::<serde_json::Value>(other) {
        assert!(!b.fold(&f), "不是会话帧 ⇒ 不动");
    }
    assert_eq!(b, before);
    b.forget("c");
    assert!(b.seat("c").is_none());
}

/// ★ 断的那一刻（桌面后端那一路 · 手机核心换流那一下都调它）：活的 · 可重连的落成看不见那一张、记在簿上，已结束的照旧；
/// 那台接上、报完活会话清单 ⇒ 摘掉。
#[test]
fn going_unseen_folds_the_open_ones_into_the_one_unseen_face_and_the_replay_clears_it() {
    let mut b = folded(FRAMES);
    let ended = b.face("c").cloned();
    let open: Vec<String> = b
        .seats()
        .filter(|(_, p)| match p {
            Seat::Live(_) => true,
            Seat::Left(s) => s.face.life == Life::Reconnectable,
        })
        .map(|(s, _)| s.clone())
        .collect();
    assert!(!open.is_empty(), "夹具里得有活的");
    let left = b.unseen_now(cmd_wire::cells::Words("X 离线".into()));
    let face = unseen(cmd_wire::cells::Words("X 离线".into()));
    assert_eq!(left.iter().map(|l| l.sid.clone()).collect::<Vec<_>>(), open);
    assert!(left.iter().all(|l| l.face == face));
    for sid in &open {
        assert_eq!(b.face(sid), Some(&face), "{sid} 落成看不见那一张");
    }
    assert_eq!(b.face("c").cloned(), ended, "已结束的照旧作数");
    assert_eq!(b.unseen.as_ref(), Some(&face));
    assert_eq!(
        (face.name.0.as_str(), face.line.0.as_str()),
        (
            copy_core::copy_text("runs.state.unknown", &[]).as_str(),
            "X 离线"
        )
    );
    b.fold(&decode::<serde_json::Value>(r#"{"kind":"sessions_replayed"}"#).unwrap());
    assert!(b.unseen.is_none(), "报完清单 ⇒ 看得见了");
}

/// ★ 记录文件那两帧带的那一句照记（两端照摆，不照 `why` 自己映）：后来的盖前面的；关掉那一条 ⇒ 不再记。
#[test]
fn the_record_file_frames_leave_their_written_sentence_on_the_session() {
    let mut b = folded(FRAMES);
    let gone = r#"{"kind":"session_file_gone","session_id":"d","path":"/p/d.jsonl","said":"甲句"}"#;
    assert!(b.fold(&decode::<serde_json::Value>(gone).unwrap()));
    assert_eq!(b.file_said.get("d").map(|w| w.0.as_str()), Some("甲句"));
    let reread = r#"{"kind":"session_file_reread","session_id":"d","path":"/p/d.jsonl","why":"rewritten","lines":3,"said":"乙句"}"#;
    assert!(b.fold(&decode::<serde_json::Value>(reread).unwrap()));
    assert_eq!(
        b.file_said.get("d").map(|w| w.0.as_str()),
        Some("乙句"),
        "后来的那一句盖前面的"
    );
    b.forget("d");
    assert!(!b.file_said.contains_key("d"), "关掉的那一条不再记");
}

/// ★ 折一帧回该推的那一格（各处推成品只调它）：改整条的那几种 ⇒ 那一条此刻整条；清单报完了 ⇒ 只带 `listed`；别的会话帧折进簿、不推。
#[test]
fn a_step_hands_back_the_seat_it_changed_and_the_whole_is_the_book() {
    use cmd_wire::frame::{SessionSeat, SessionSeats};
    let mut b = SessionBook::default();
    let mut lines = FRAMES.lines().filter(|l| !l.trim().is_empty());
    let first = decode::<serde_json::Value>(lines.next().unwrap()).unwrap();
    let got = b.step(&first).expect("宣告改整条");
    assert_eq!(
        got,
        SessionSeats {
            seats: vec![SessionSeat::of("a", b.live.get("a"), None, None, None)],
            listed: false,
            whole: false,
        }
    );
    assert!(b.live.contains_key("a"), "step 也折进簿");
    let gone = r#"{"kind":"session_file_gone","session_id":"a","path":"/p/a.jsonl","said":"甲句"}"#;
    assert_eq!(b.step(&decode::<serde_json::Value>(gone).unwrap()), None);
    assert_eq!(b.file_said.get("a").map(|w| w.0.as_str()), Some("甲句"));
    let replayed = b
        .step(&decode::<serde_json::Value>(r#"{"kind":"sessions_replayed"}"#).unwrap())
        .expect("清单报完了推一格");
    assert!(replayed.seats.is_empty() && replayed.listed && !replayed.whole);
    let changed = r#"{"kind":"changed","topic":"tasks"}"#;
    if let Ok(f) = decode::<serde_json::Value>(changed) {
        assert_eq!(b.step(&f), None, "不是会话帧 ⇒ 不推");
    }
    let all = folded(FRAMES);
    let whole = all.whole();
    assert!(whole.whole);
    assert_eq!(whole.listed, all.listed);
    let want: Vec<SessionSeat> = all.seats().map(|(sid, _)| all.seat_now(sid)).collect();
    assert_eq!(whole.seats, want);
    assert_eq!(
        whole
            .seats
            .iter()
            .map(|s| s.sid.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c", "d"]
    );
    assert!(whole.seats.iter().all(|s| s.seat.is_some()));
}

/// ★ 未读：`session_unread` 折进簿、那一条此刻整条带写好的字（只剩未读的那一条整本里也有）；不带字的那一帧摘掉；
/// 连接断的那一刻整份作废（说不清不留数）；关掉的那一条不再记。
#[test]
fn unread_folds_into_the_seat_and_goes_with_the_connection() {
    let mut b = folded(FRAMES);
    let unread = |sid: &str, said: Option<&str>| {
        let mut v = serde_json::json!({"kind": "session_unread", "sid": sid});
        if let Some(s) = said {
            v["said"] = s.into();
        }
        decode::<serde_json::Value>(&v.to_string()).expect("一帧")
    };
    let got = b.step(&unread("a", Some("3"))).expect("未读变了推那一条");
    assert_eq!(got.seats.len(), 1);
    assert_eq!(
        got.seats[0].unread.as_ref().map(|w| w.0.as_str()),
        Some("3")
    );
    assert!(got.seats[0].seat.is_some(), "整条照带");
    b.step(&unread("zz", Some("99+")));
    let whole = b.whole();
    assert_eq!(
        whole
            .seats
            .iter()
            .filter_map(|s| s.unread.as_ref().map(|w| (s.sid.as_str(), w.0.as_str())))
            .collect::<Vec<_>>(),
        [("a", "3"), ("zz", "99+")],
        "只剩未读的那一条整本里也有"
    );
    let cleared = b.step(&unread("a", None)).expect("推那一条");
    assert_eq!(cleared.seats[0].unread, None);
    b.step(&unread("a", Some("1")));
    b.unseen_now(cmd_wire::cells::Words("断了".into()));
    assert!(b.unread.is_empty(), "连接断了 ⇒ 未读整份作废");
    b.step(&unread("b", Some("2")));
    b.forget("b");
    assert!(!b.unread.contains_key("b"), "关掉的那一条不再记");
}
