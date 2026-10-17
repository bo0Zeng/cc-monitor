//! 流只发要看的会话（`stream-watch`）：这条流没报过在看哪几个 ⇒ 全发；报了 ⇒ 不在里面的会话，它的整行帧
//! （行 · 运行表 · 主线外清单）不上这条流，摘要那几格（宣告 · 状态 · 轮次边沿 · 去向）照发；
//! 看了以后回「这条流从第几行起发它」，之前那段由客户端按骨架补 —— 合起来不重复也不漏。
//! 语料是结构性的假行（`{"n":…}`），不含任何真会话正文。

use super::*;
use crate::wire::WatchFrom;

const A: &str = "s-wa";
const B: &str = "s-wb";

fn rig(tag: &str) -> (PathBuf, ReaderState, FrameSink, FrameRx) {
    let dir = std::env::temp_dir().join(format!("ccm-watch-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES);
    let mut state = ReaderState::new(dir.clone());
    state.active_sids.insert(A.to_string());
    state.active_sids.insert(B.to_string());
    (dir, state, FrameSink::new(tx), rx)
}

fn file(dir: &Path, sid: &str) -> PathBuf {
    dir.join(format!("{sid}.jsonl"))
}

/// 往一份记录后面追加 `n` 行假行（行号接着文件里已有的数）。
fn append(path: &Path, n: usize) {
    use std::io::Write;
    let have = std::fs::read(path)
        .map(|b| b.iter().filter(|&&c| c == b'\n').count())
        .unwrap_or(0);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    for i in have..have + n {
        writeln!(f, "{{\"n\":{i}}}").unwrap();
    }
}

/// 这一趟上了流的行：`(会话, 行号)`。
fn lines(rx: &mut FrameRx) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    while let Ok(f) = rx.try_recv() {
        if let Frame::Line(crate::wire::Line {
            session_id, seq, ..
        }) = f
        {
            out.push((session_id, seq));
        }
    }
    out
}

fn ids(v: &[&str]) -> crate::wire::WatchAsk {
    crate::wire::WatchAsk {
        sids: v.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

#[test]
fn a_stream_that_never_said_what_it_watches_carries_every_session() {
    let (dir, mut state, mut sink, mut rx) = rig("all");
    append(&file(&dir, A), 2);
    append(&file(&dir, B), 1);
    process_jsonl(&file(&dir, A), &mut state, &mut sink);
    process_jsonl(&file(&dir, B), &mut state, &mut sink);
    assert_eq!(
        lines(&mut rx),
        vec![(A.into(), 0), (A.into(), 1), (B.into(), 0)]
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_unwatched_session_puts_no_line_on_the_stream_and_keeps_counting() {
    let (dir, mut state, mut sink, mut rx) = rig("unwatched");
    watch_sessions(0, ids(&[A]), &mut state, &mut sink);
    append(&file(&dir, A), 2);
    append(&file(&dir, B), 3);
    process_jsonl(&file(&dir, A), &mut state, &mut sink);
    let n = process_jsonl(&file(&dir, B), &mut state, &mut sink);
    assert_eq!(n, 3, "没在看的会话照读（行号照数），只是不上流");
    assert_eq!(lines(&mut rx), vec![(A.into(), 0), (A.into(), 1)]);
    // 运行表 · 主线外清单也是那一行的内容：没在看就不上流；在看的照发。
    sink.send(state.book.frame(B));
    send_branch(&mut state, &mut sink, B, &file(&dir, B), true);
    sink.send(state.book.frame(A));
    let mut kinds = Vec::new();
    while let Ok(f) = rx.try_recv() {
        kinds.push(match f {
            Frame::SessionRuns(crate::wire::SessionRuns { sid, .. }) => format!("runs {sid}"),
            Frame::SessionBranch(crate::wire::SessionBranch { sid, .. }) => format!("branch {sid}"),
            _ => "other".into(),
        });
    }
    assert_eq!(kinds, vec![format!("runs {A}")]);
    // 摘要那几格（状态 · 文件重读 / 不在了）不挑：没在看的会话照发。
    sink.send(Frame::SessionFileGone(crate::wire::SessionFileGone {
        session_id: B.into(),
        path: "p".into(),
    }));
    let mut n = 0;
    while rx.try_recv().is_ok() {
        n += 1;
    }
    assert_eq!(n, 1, "摘要帧不按在看的名单挑");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn starting_to_watch_answers_where_live_lines_begin_and_nothing_is_lost_or_doubled() {
    let (dir, mut state, mut sink, mut rx) = rig("from");
    let pb = file(&dir, B);
    // 先在看 A；B 在后台写了三行。
    assert!(
        watch_sessions(0, ids(&[A]), &mut state, &mut sink).is_empty(),
        "A 没有记录文件 ⇒ 无从答"
    );
    append(&pb, 3);
    process_jsonl(&pb, &mut state, &mut sink);
    assert!(lines(&mut rx).is_empty());
    // 转去看 B：回 B 从第 3 行起上流。
    let from = watch_sessions(0, ids(&[B]), &mut state, &mut sink);
    assert_eq!(
        from,
        vec![WatchFrom {
            sid: B.into(),
            path: pb.to_string_lossy().into_owned(),
            seq: 3
        }]
    );
    // 之后又写两行（其中一行在回话之前就落盘、还没读到也一样）：上流的从 3 起。
    append(&pb, 2);
    process_jsonl(&pb, &mut state, &mut sink);
    let live: Vec<u64> = lines(&mut rx).into_iter().map(|(_, s)| s).collect();
    assert_eq!(live, vec![3, 4]);
    // 合起来：骨架补 [0, from) ＋ 实时 [from, …) ＝ 文件里每一行恰好一次。
    let mut all: Vec<u64> = (0..from[0].seq).chain(live).collect();
    all.sort_unstable();
    let on_disk = std::fs::read(&pb)
        .unwrap()
        .iter()
        .filter(|&&c| c == b'\n')
        .count() as u64;
    assert_eq!(all, (0..on_disk).collect::<Vec<_>>());
    // A 不看了 ⇒ A 再写也不上流。
    append(&file(&dir, A), 1);
    process_jsonl(&file(&dir, A), &mut state, &mut sink);
    assert!(lines(&mut rx).is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn only_sessions_that_just_came_into_view_get_a_start() {
    let (dir, mut state, mut sink, _rx) = rig("again");
    append(&file(&dir, A), 1);
    append(&file(&dir, B), 1);
    process_jsonl(&file(&dir, A), &mut state, &mut sink);
    process_jsonl(&file(&dir, B), &mut state, &mut sink);
    // 没报过 ＝ 全在流上 ⇒ 第一次报，谁都不是「新看的」。
    assert!(watch_sessions(0, ids(&[A, B]), &mut state, &mut sink).is_empty());
    assert!(watch_sessions(0, ids(&[A]), &mut state, &mut sink).is_empty());
    let from = watch_sessions(0, ids(&[A, B]), &mut state, &mut sink);
    assert_eq!(
        from.iter().map(|f| f.sid.as_str()).collect::<Vec<_>>(),
        vec![B]
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn coming_into_view_resends_the_run_table_and_the_branch_list() {
    let (dir, mut state, mut sink, mut rx) = rig("resend");
    let pb = file(&dir, B);
    append(&pb, 1);
    watch_sessions(0, ids(&[A]), &mut state, &mut sink);
    process_jsonl(&pb, &mut state, &mut sink);
    adopt_runs(&mut state, B, &pb);
    while rx.try_recv().is_ok() {}
    watch_sessions(0, ids(&[A, B]), &mut state, &mut sink);
    let mut kinds = Vec::new();
    while let Ok(f) = rx.try_recv() {
        if let Frame::SessionRuns(crate::wire::SessionRuns { sid, .. }) = f {
            kinds.push(sid);
        }
    }
    assert_eq!(kinds, vec![B.to_string()], "新看的会话补一帧运行表（整份）");
    std::fs::remove_dir_all(&dir).ok();
}

/// attach 行带了名单：这条连接从接上那一份起就照它发 —— 名单外的会话运行表 · 行一帧不收、名单里的照收；
/// 名单里要了主线外清单的接上即收它（不等 `stream-watch`）；宣告照发（不挑）。
#[test]
fn a_list_on_the_attach_line_holds_from_the_first_frame() {
    let (dir, mut state, mut sink, _rx0) = rig("join");
    let (pa, pb) = (file(&dir, A), file(&dir, B));
    append(&pa, 1);
    append(&pb, 1);
    process_jsonl(&pa, &mut state, &mut sink);
    process_jsonl(&pb, &mut state, &mut sink);
    for sid in [A, B] {
        crate::observe::runs::testing::seed_running(&state.book, sid, "r1", None);
        // 宣告那一帧的形状这里不看：join 原样转发留着的那一帧。
        state.added.insert(
            sid.to_string(),
            Frame::SessionFileGone(crate::wire::SessionFileGone {
                session_id: sid.into(),
                path: String::new(),
            }),
        );
    }
    let (tx, mut rx) = frame_channel(CHANNEL_MAX_BYTES);
    let ask = crate::wire::WatchAsk {
        sids: vec![A.into()],
        branch: vec![crate::wire::BranchHeld {
            sid: A.into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    join(
        Sub::new(7, tx, crate::Tz::default()),
        Some(ask),
        &mut state,
        &mut sink,
    );
    append(&pa, 1);
    append(&pb, 1);
    process_jsonl(&pa, &mut state, &mut sink);
    process_jsonl(&pb, &mut state, &mut sink);
    let mut got = Vec::new();
    while let Ok(f) = rx.try_recv() {
        got.push(match f {
            Frame::SessionRuns(crate::wire::SessionRuns { sid, .. }) => format!("runs {sid}"),
            Frame::SessionBranch(crate::wire::SessionBranch { sid, .. }) => format!("branch {sid}"),
            Frame::Line(crate::wire::Line {
                session_id, seq, ..
            }) => {
                format!("line {session_id} {seq}")
            }
            Frame::SessionFileGone(crate::wire::SessionFileGone { session_id, .. }) => {
                format!("added {session_id}")
            }
            _ => continue,
        });
    }
    assert_eq!(
        got,
        vec![
            format!("added {A}"),
            format!("runs {A}"),
            format!("added {B}"),
            format!("branch {A}"),
            format!("line {A} 1"),
        ],
        "名单外的 B：运行表 · 行一帧不收"
    );
    std::fs::remove_dir_all(&dir).ok();
}

// ── 手上留着的会话：那台报完清单、又不在说的 ⇒ 会话账答一句已结束（只答报它的那条连接）──

/// 生产那一份发帧口（带会话账本）；`listed` ⇒ 清单已报完、第一份 tmux 观测已到。
fn ledgered(listed: bool) -> FrameSink {
    let mut sink = FrameSink::hub();
    if listed {
        sink.send(Frame::SessionsReplayed(crate::wire::SessionsReplayed {}));
        sink.tmux(crate::observe::tmux_observe::TmuxObservation::NoServer);
    }
    sink
}

fn kept(v: &[&str]) -> crate::wire::WatchAsk {
    crate::wire::WatchAsk {
        kept: v.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

fn conn(
    sink: &mut FrameSink,
    state: &mut ReaderState,
    id: u64,
    ask: Option<crate::wire::WatchAsk>,
) -> FrameRx {
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES);
    join(Sub::new(id, tx, crate::Tz::default()), ask, state, sink);
    rx
}

/// 这一趟收到的会话账那几帧：`ended x` · `reconnectable x` · `listed`。
fn told(rx: &mut FrameRx) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(f) = rx.try_recv() {
        match f {
            Frame::SessionState(s) => {
                assert_eq!(
                    Frame::SessionState(s.clone()),
                    Frame::session_state(s.sid.clone(), s.state),
                    "与别处发的同一句"
                );
                out.push(format!("{:?} {}", s.state, s.sid));
            }
            Frame::SessionsReplayed(_) => out.push("listed".into()),
            _ => {}
        }
    }
    out
}

fn added_frame(sid: &str) -> Frame {
    Frame::SessionAdded(crate::wire::SessionAdded {
        sid: sid.into(),
        agent_kind: "claude".to_string(),
        background: false,
        attachable: None,
        cwd: None,
        project_dir: None,
        name: None,
        path: None,
        face: crate::wire::SessionFace::of(crate::wire::Life::Live {
            activity: None,
            tmux: None,
        }),
        name_face: crate::history::search_rules::name_face(&Default::default()),
        waiting_for: None,
        hidden: None,
        container: None,
        pid: 0,
    })
}

/// ① 带上来一个那台从没说过的 ⇒ 清单已报完时当场答已结束，只给这条连接。
#[test]
fn a_kept_session_the_backend_never_spoke_of_is_answered_ended_on_that_connection_only() {
    let mut state = ReaderState::new(std::env::temp_dir());
    let mut sink = ledgered(true);
    let mut other = conn(&mut sink, &mut state, 1, None);
    let mut mine = conn(&mut sink, &mut state, 2, Some(kept(&["x"])));
    assert_eq!(told(&mut mine), vec!["listed", "Ended x"]);
    assert_eq!(told(&mut other), vec!["listed"]);
}

/// ② 活的 · 可重连的 · 等裁的都不答（前两样接上那一份里已说过；等裁的裁了照常扇给每条连接）。
#[test]
fn kept_sessions_the_ledger_still_holds_are_not_answered() {
    let mut state = ReaderState::new(std::env::temp_dir());
    let mut sink = ledgered(true);
    sink.send(added_frame("live"));
    sink.send(added_frame("due"));
    sink.send(Frame::SessionRemoved(crate::wire::SessionRemoved {
        sid: "due".into(),
        cause: crate::wire::RemovalCause::Gone,
        by: None,
    }));
    sink.tmux(crate::observe::tmux_observe::TmuxObservation::Sessions(
        "n\t/w\tzsh\t0\t1\tback\t$1\tback \n".into(),
    ));
    let mut rx = conn(
        &mut sink,
        &mut state,
        1,
        Some(kept(&["live", "back", "due"])),
    );
    assert_eq!(told(&mut rx), vec!["Reconnectable back", "listed"]);
}

/// ③ 清单报完之前带上来 ⇒ 先不答；放出 `sessions_replayed` 那一刻答，排在它后面。
#[test]
fn a_kept_session_is_answered_right_after_the_list_is_released() {
    let mut state = ReaderState::new(std::env::temp_dir());
    let mut sink = ledgered(false);
    let mut rx = conn(&mut sink, &mut state, 1, Some(kept(&["x"])));
    sink.send(Frame::SessionsReplayed(crate::wire::SessionsReplayed {}));
    assert_eq!(told(&mut rx), Vec::<String>::new(), "清单还压着");
    sink.tmux(crate::observe::tmux_observe::TmuxObservation::NoServer);
    assert_eq!(told(&mut rx), vec!["listed", "Ended x"]);
}

/// ④ 再报一次（多了 y）⇒ 只答 y，x 不重答。
#[test]
fn reporting_again_answers_only_the_new_ones() {
    let mut state = ReaderState::new(std::env::temp_dir());
    let mut sink = ledgered(true);
    let mut rx = conn(&mut sink, &mut state, 1, Some(kept(&["x"])));
    assert_eq!(told(&mut rx), vec!["listed", "Ended x"]);
    watch_sessions(1, kept(&["x", "y"]), &mut state, &mut sink);
    assert_eq!(told(&mut rx), vec!["Ended y"]);
}

// ── 拥塞：通了先一句 overflow（那台写的）；丢过行以外的 ⇒ 原连接上就地重报，客户端不判、不重接 ──

/// 一条通道恰好装得下 8 帧填料（[`filler`]，比重报里哪一帧都大）的连接：在看 A、要 A 的主线外清单、手上留着 k（那台不在说）；接上那一份读掉。
fn narrow(
    tag: &str,
) -> (
    PathBuf,
    ReaderState,
    FrameSink,
    FrameRx,
) {
    let (dir, mut state, _, _rx0) = rig(tag);
    let mut sink = ledgered(true);
    let pa = file(&dir, A);
    append(&pa, 1);
    process_jsonl(&pa, &mut state, &mut sink);
    state.added.insert(A.to_string(), added_frame(A));
    let (tx, mut rx) = frame_channel(8 * wire_bytes(&filler()));
    let ask = crate::wire::WatchAsk {
        sids: vec![A.into()],
        branch: vec![crate::wire::BranchHeld {
            sid: A.into(),
            ..Default::default()
        }],
        kept: vec!["k".into()],
    };
    join(
        Sub::new(1, tx, crate::Tz::default()),
        Some(ask),
        &mut state,
        &mut sink,
    );
    while rx.try_recv().is_ok() {}
    (dir, state, sink, rx)
}

fn gone(sid: &str) -> Frame {
    Frame::SessionFileGone(crate::wire::SessionFileGone {
        session_id: sid.into(),
        path: String::new(),
    })
}

/// 填料：一帧「不见了」，路径垫长到比重报里哪一帧都大（通道按字节封顶，[`narrow`] 按它的大小开 8 格）。
fn filler() -> Frame {
    Frame::SessionFileGone(crate::wire::SessionFileGone {
        session_id: "f".into(),
        path: "x".repeat(4_000),
    })
}

/// 往通道里放 `n` 格填料。
fn fill(sink: &mut FrameSink, n: usize) {
    for _ in 0..n {
        sink.send_to(1, filler());
    }
}

/// 把通道余下那一点也用小填料塞满（再来一帧最小的也放不下）。
fn brim(sink: &mut FrameSink) {
    let small = wire_bytes(&gone("f"));
    loop {
        let tx = &sink.subs[0].tx;
        if tx.held.load(std::sync::atomic::Ordering::Acquire) + small > tx.cap {
            return;
        }
        sink.send_to(1, gone("f"));
    }
}

/// 读掉通道里的全部，记成一行一件。
fn heard(rx: &mut FrameRx) -> Vec<String> {
    use crate::wire::TopicExt as _;
    let mut out = Vec::new();
    while let Ok(f) = rx.try_recv() {
        out.push(match f {
            Frame::Overflow(o) => format!("overflow {}", o.said.0),
            Frame::SessionAdded(a) => format!("added {}", a.sid),
            Frame::SessionPrimed(p) => format!("primed {}", p.sid),
            Frame::SessionsReplayed(_) => "listed".into(),
            Frame::SessionState(s) => format!("{:?} {}", s.state, s.sid),
            Frame::Changed(c) => {
                format!("changed {} {}", c.topic.name(), c.key.unwrap_or_default())
            }
            Frame::SessionBranch(b) => format!("branch {} {}", b.sid, b.at),
            Frame::SessionFileGone(g) => format!("gone {}", g.session_id),
            Frame::Line(l) => format!("line {} {}", l.session_id, l.seq),
            other => format!("{other:?}"),
        });
    }
    out
}

fn realigned() -> String {
    format!(
        "overflow {}",
        copy_core::copy_text("beStream.overflow.realigned", &[])
    )
}

/// ⑩ 丢一帧会话状态 ⇒ 通了之后这条连接依次收到：那一句（已重新对齐）· 此刻的样子（宣告 · 读完了 · 清单报完）· 手上留着的重答。
#[test]
fn a_lost_session_state_is_reported_again_in_place_after_the_overflow() {
    let (dir, mut state, mut sink, mut rx) = narrow("realign");
    fill(&mut sink, 8);
    sink.send(Frame::session_state(
        "z".into(),
        crate::wire::SessionFate::Ended,
    ));
    while rx.try_recv().is_ok() {}
    sink.send_to(1, gone("t"));
    report_owed(&mut state, &mut sink);
    assert_eq!(
        heard(&mut rx),
        vec![
            realigned(),
            "gone t".into(),
            format!("added {A}"),
            format!("primed {A}"),
            "listed".into(),
            "Ended k".into(),
        ]
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// ⑪ 只丢了行 ⇒ 那一句是「实时消息」那句，后面没有重报。
#[test]
fn only_lost_lines_say_so_and_report_nothing_again() {
    let (dir, mut state, mut sink, mut rx) = narrow("lines");
    fill(&mut sink, 8);
    let pa = file(&dir, A);
    append(&pa, 2);
    process_jsonl(&pa, &mut state, &mut sink);
    while rx.try_recv().is_ok() {}
    sink.send_to(1, gone("t"));
    report_owed(&mut state, &mut sink);
    assert_eq!(
        heard(&mut rx),
        vec![
            format!(
                "overflow {}",
                copy_core::copy_text("beStream.overflow.lines", &[("n", "2")])
            ),
            "gone t".into(),
        ]
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// ⑫ 丢一帧 `changed(tasks, x)` ⇒ 重报里有它（不带体）；丢一帧主线外清单 ⇒ 重报给那份整份。
#[test]
fn a_lost_change_and_a_lost_branch_page_come_back_in_the_report() {
    let (dir, mut state, mut sink, mut rx) = narrow("changed");
    fill(&mut sink, 8);
    sink.send(Frame::changed(
        crate::wire::Topic::Tasks,
        Some("x".into()),
        None,
        None,
    ));
    let pa = file(&dir, A);
    sink.branch(A, &pa.to_string_lossy(), &[], 0);
    while rx.try_recv().is_ok() {}
    sink.send_to(1, gone("t"));
    report_owed(&mut state, &mut sink);
    assert_eq!(
        heard(&mut rx),
        vec![
            realigned(),
            "gone t".into(),
            format!("added {A}"),
            format!("primed {A}"),
            "listed".into(),
            "Ended k".into(),
            "changed tasks x".into(),
            format!("branch {A} 0"),
        ]
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// ⑬ 重报时通道还满 ⇒ 下一次照样 overflow ＋ 重报；通了之后不再有（数帧，不按墙钟）。
#[test]
fn a_report_that_jams_again_is_said_and_reported_again_until_it_gets_through() {
    let (dir, mut state, mut sink, mut rx) = narrow("jam");
    fill(&mut sink, 8);
    sink.send(Frame::session_state(
        "z".into(),
        crate::wire::SessionFate::Ended,
    ));
    while rx.try_recv().is_ok() {}
    fill(&mut sink, 7); // 头一格先补上那一句，后面七格填料
    brim(&mut sink); // 余下那一点也塞满，通道又满
    sink.send_to(1, gone("t")); // 这一帧丢
    report_owed(&mut state, &mut sink); // 重报整份又丢
    let first = heard(&mut rx);
    assert!(first.len() >= 8, "{first:?}");
    assert_eq!(first[0], realigned());
    assert!(first[1..].iter().all(|f| f == "gone f"), "{first:?}");
    sink.send_to(1, gone("t"));
    report_owed(&mut state, &mut sink);
    assert_eq!(
        heard(&mut rx),
        vec![
            realigned(),
            "gone t".into(),
            format!("added {A}"),
            format!("primed {A}"),
            "listed".into(),
            "Ended k".into(),
        ]
    );
    sink.send_to(1, gone("t"));
    report_owed(&mut state, &mut sink);
    assert_eq!(heard(&mut rx), vec!["gone t".to_string()], "通了之后不再有");
    std::fs::remove_dir_all(&dir).ok();
}
