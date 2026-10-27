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
