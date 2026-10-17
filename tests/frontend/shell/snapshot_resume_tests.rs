//! 手上那一份（`held`）的存取：快照做完立 · 实时行连续而且带末端才推 · 会话结束 / 不在看就忘；快照那条路真的交回、真的立。
//! 续不续得上的判定在后端（`history-tail`，后端 `history_bytes_tests.rs`）；只问一趟由 `stream_source/snapshot_tests.rs` 钉着。

use super::*;

fn held(path: &str, next: u64, next_byte: u64) -> Held {
    Held {
        path: path.to_string(),
        next,
        next_byte,
        mark: "m".to_string(),
    }
}

/// ★ 尾段几行只有一处口径：壳留存每会话留的那么多（`event_replay::REPLAY_TAIL_KEEP`）。
/// 快照多拉的行进了留存也会被修回这个数（F5 那一屏看不到），只在界面账本里多占着、往上翻才用得上 ⇒ 不多拉。
#[test]
fn the_snapshot_tail_is_the_replay_keep() {
    let prod = guard_core::addr::stream_source_production();
    for anchor in [
        "const SNAPSHOT_TAIL_LINES: u64 = crate::event_replay::REPLAY_TAIL_KEEP as u64;",
        "SNAPSHOT_TAIL_LINES,",
    ] {
        guard_core::find_pinned(&prod, anchor)
            .unwrap_or_else(|e| panic!("stream_source 生产段里 `{anchor}` 不是恰好一处：{e}"));
    }
}

/// 只在**连续**到达、而且带着末端时前推；做完一次不回退（实时行推得更远就留推远的）；会话真结束就忘掉。
#[test]
fn the_cursor_only_moves_on_contiguous_lines() {
    let o = &crate::origin::Origin("c2-test-origin".to_string());
    let s = "c2-test-sid";
    forget(o, s);
    note_flushed(o, [(s, 0, Some(5))]);
    assert_eq!(
        cursor_of(o, s),
        None,
        "没立之前不记（实时行之前那一段还没拿到）"
    );
    note_done(o, s, held("/p.jsonl", 10, 100));
    let at = |o, s| cursor_of(o, s).map(|c| (c.next, c.next_byte));
    assert_eq!(at(o, s), Some((10, 100)));
    note_flushed(o, [(s, 10, Some(110)), (s, 11, Some(120))]);
    assert_eq!(at(o, s), Some((12, 120)));
    note_flushed(o, [(s, 20, Some(200))]); // 跳号：中间那几行没拿到
    assert_eq!(at(o, s), Some((12, 120)), "跳号不许前推");
    note_flushed(o, [(s, 5, Some(50))]); // 重复的旧行
    assert_eq!(at(o, s), Some((12, 120)));
    note_flushed(o, [(s, 12, None)]); // 末端说不准
    assert_eq!(at(o, s), Some((12, 120)), "说不准末端的那一行不许前推");
    // 再做完一次（后端给的没超过已有）⇒ 不回退
    note_done(o, s, held("/p.jsonl", 10, 100));
    assert_eq!(at(o, s), Some((12, 120)));
    // 换了路径 ⇒ 照后端给的
    note_done(o, s, held("/q.jsonl", 3, 30));
    assert_eq!(at(o, s), Some((3, 30)));
    forget(o, s);
    assert_eq!(cursor_of(o, s), None);
}

/// ★ 接线：快照那条路真的取手上那一份交回去、做完真的立；发行那条真的推；会话结束真的忘。
/// 锚串各恰好一处（`find_pinned`：裸 `contains` 在锚被撑大时照样绿）。
#[test]
fn the_snapshot_path_is_wired_through_the_cursor() {
    let prod = guard_core::addr::stream_source_production();
    for anchor in [
        "let held = crate::snapshot_resume::cursor_of(&origin, sid);",
        "held.as_ref(),",
        "crate::snapshot_resume::note_done(&origin, sid, tail.held);",
        "crate::snapshot_resume::note_flushed(",
        // 「会话结束真的忘」在 `LineIntake::removed`（远端与本机两个帧源共用那一处）。
        "crate::snapshot_resume::forget(&crate::origin::Origin(self.origin_label.clone()), sid);",
    ] {
        guard_core::find_pinned(&prod, anchor)
            .unwrap_or_else(|e| panic!("stream_source 生产段里 `{anchor}` 不是恰好一处：{e}"));
    }
}

/// 报了名单 ⇒ 那台不在看的会话续点作废（重连时它只读尾段，不从停住的旧续点补读一段）；在看的与别台的照留。
#[test]
fn unwatched_sessions_lose_their_cursor() {
    let here = Origin("proj9-keep-only-a".into());
    let there = Origin("proj9-keep-only-b".into());
    for sid in ["seen", "unseen"] {
        note_done(&here, sid, held("/p.jsonl", 10, 100));
    }
    note_done(&there, "unseen", held("/p.jsonl", 10, 100));
    keep_only(&here, &["seen".to_string()].into());
    assert!(cursor_of(&here, "seen").is_some(), "在看的续点被作废了");
    assert!(cursor_of(&here, "unseen").is_none(), "不在看的续点还留着");
    assert!(
        cursor_of(&there, "unseen").is_some(),
        "别台的续点被连带作废"
    );
    forget(&here, "seen");
    forget(&there, "unseen");
}
