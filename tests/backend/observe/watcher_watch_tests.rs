//! 流只发要看的会话（`stream-watch`）：这条流没报过在看哪几个 ⇒ 全发；报了 ⇒ 不在里面的会话，它的整行帧
//! （行 · 运行表 · 主线外清单）不上这条流，摘要那几格（宣告 · 状态 · 轮次边沿 · 去向）照发；
//! 看了以后回「这条流从第几行起发它」，之前那段由客户端按骨架补 —— 合起来不重复也不漏。
//! 语料是结构性的假行（`{"n":…}`），不含任何真会话正文。

use super::*;
use crate::wire::SessionFaceExt as _;
use crate::wire::WatchFrom;

const A: &str = "s-wa";
const B: &str = "s-wb";

fn rig(tag: &str) -> (PathBuf, ReaderState, FrameSink, FrameRx) {
    let dir = std::env::temp_dir().join(format!("ccm-watch-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
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
        said: Default::default(),
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
        // 宣告那一帧的形状这里不看：join 转发留着的那一帧（只认 sid）。
        state.added.insert(sid.to_string(), added_row(sid));
    }
    let (tx, mut rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    let ask = crate::wire::WatchAsk {
        sids: vec![A.into()],
        branch: vec![crate::wire::BranchHeld {
            sid: A.into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    join(
        Sub::new(7, tx, crate::common::time::Tz::default()),
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
            Frame::SessionAdded(crate::wire::SessionAdded { sid, .. }) => format!("added {sid}"),
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
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    join(
        Sub::new(id, tx, crate::common::time::Tz::default()),
        ask,
        state,
        sink,
    );
    rx
}

/// 离开那一帧说的去向（`Ended` · `Reconnectable`，同改形之前的写法）。
fn went(s: &crate::wire::SessionLeft) -> &'static str {
    match s.face.life {
        crate::wire::WireLife::Reconnectable => "Reconnectable",
        _ => "Ended",
    }
}

/// 这一趟收到的会话账那几帧：`ended x` · `reconnectable x` · `listed`。
fn told(rx: &mut FrameRx) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(f) = rx.try_recv() {
        match f {
            Frame::SessionLeft(s) => {
                let life = match s.face.life {
                    crate::wire::WireLife::Reconnectable => crate::wire::Life::Reconnectable,
                    _ => crate::wire::Life::Ended,
                };
                assert_eq!(
                    Frame::SessionLeft(s.clone()),
                    Frame::session_left(s.sid.clone(), life, None, None),
                    "与别处发的同一句"
                );
                out.push(format!("{} {}", went(&s), s.sid));
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
        cwd: None,
        project_dir: None,
        name: None,
        path: None,
        face: crate::wire::SessionFace::of(crate::wire::Life::Live {
            activity: None,
            host: None,
        }),
        name_face: crate::history::search_rules::name_face(&Default::default()),
        hidden: None,
        container: None,
        pid: 0,
    })
}

/// watcher 记着的那一份宣告（`attachable` 没说）。
fn added_row(sid: &str) -> Added {
    let Frame::SessionAdded(wire) = added_frame(sid) else {
        unreachable!()
    };
    Added {
        wire,
        attachable: None,
        kind: "claude",
    }
}

/// 宣告时 pidfile 说接进去没有意义（这一格不上线，宣告那一下经 `FrameSink::announce` 另交会话账本）⇒ 程序退了、
/// tmux 会话还在那一张脸照旧不给「看它的终端」；没说的那一条照给。
#[test]
fn an_announced_unattachable_session_stays_unattachable_once_reconnectable() {
    let mut state = ReaderState::new(std::env::temp_dir());
    let mut sink = ledgered(true);
    let mut rx = conn(&mut sink, &mut state, 1, None);
    let both = [("s-ua", Some(false), "$1"), ("s-ub", None, "$2")];
    for (sid, attachable, _) in both {
        let Frame::SessionAdded(wire) = added_frame(sid) else {
            unreachable!()
        };
        sink.announce(Added {
            wire,
            attachable,
            kind: "claude",
        });
    }
    let rows: String = both
        .iter()
        .map(|(sid, _, id)| format!("t{sid}\t/w\tzsh\t0\t1\t{sid}\t{id}\t{sid} \n"))
        .collect();
    sink.tmux(TmuxObservation::Sessions(rows.clone()));
    for (sid, _, _) in both {
        sink.send(Frame::session_left(
            sid.into(),
            crate::wire::Life::Ended,
            None,
            None,
        ));
    }
    sink.observing();
    sink.tmux(TmuxObservation::Sessions(rows));
    let mut view = std::collections::BTreeMap::new();
    while let Ok(f) = rx.try_recv() {
        if let Frame::SessionLeft(s) = f {
            if s.face.life == crate::wire::WireLife::Reconnectable {
                view.insert(s.sid.clone(), s.face.can.view.clone());
            }
        }
    }
    assert_eq!(
        view.get("s-ua"),
        Some(&crate::wire::CanView::No {
            why: crate::common::cells::Words(copy_core::copy_text(
                "sessionState.unattachable.why",
                &[]
            ))
        }),
        "{view:?}"
    );
    assert!(
        matches!(view.get("s-ub"), Some(crate::wire::CanView::Yes { .. })),
        "{view:?}"
    );
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
    sink.send(Frame::session_left(
        "due".into(),
        crate::wire::Life::Ended,
        None,
        None,
    ));
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
fn narrow(tag: &str) -> (PathBuf, ReaderState, FrameSink, FrameRx) {
    let (dir, mut state, _, _rx0) = rig(tag);
    let mut sink = ledgered(true);
    let pa = file(&dir, A);
    append(&pa, 1);
    process_jsonl(&pa, &mut state, &mut sink);
    state.added.insert(A.to_string(), added_row(A));
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
        Sub::new(1, tx, crate::common::time::Tz::default()),
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
        said: Default::default(),
    })
}

/// 填料：一帧「不见了」，路径垫长到比重报里哪一帧都大（通道按字节封顶，[`narrow`] 按它的大小开 8 格）。
fn filler() -> Frame {
    Frame::SessionFileGone(crate::wire::SessionFileGone {
        session_id: "f".into(),
        path: "x".repeat(4_000),
        said: Default::default(),
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
    let tx = &sink.subs[0].tx;
    let room = tx
        .cap
        .saturating_sub(tx.held.load(std::sync::atomic::Ordering::Acquire));
    if room < small {
        return;
    }
    // 一帧「不见了」把余下的整个占掉（路径垫到刚好）：余下的不按一帧的大小取整，比它小的帧（清单报完了那一帧）也塞不进。
    sink.send_to(
        1,
        Frame::SessionFileGone(crate::wire::SessionFileGone {
            session_id: "f".into(),
            path: "x".repeat(room - small),
            said: Default::default(),
        }),
    );
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
            Frame::SessionLeft(s) => format!("{} {}", went(&s), s.sid),
            Frame::Changed(c) => {
                format!(
                    "changed {} {}",
                    c.topic().name(),
                    c.key().unwrap_or_default()
                )
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
fn a_lost_session_left_is_reported_again_in_place_after_the_overflow() {
    let (dir, mut state, mut sink, mut rx) = narrow("realign");
    fill(&mut sink, 8);
    sink.send(Frame::session_left(
        "z".into(),
        crate::wire::Life::Ended,
        None,
        None,
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
    sink.send(Frame::changed(crate::wire::Changed::Tasks {
        key: "x".into(),
        body: None,
    }));
    let pa = file(&dir, A);
    sink.branch(A, &pa.to_string_lossy(), (&[], &[]), 0);
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
    sink.send(Frame::session_left(
        "z".into(),
        crate::wire::Life::Ended,
        None,
        None,
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

/// ★ ④a：别台那条流转过来的一行挤丢了 ⇒ 这条连接在下一帧之前先收那台一格 `gap`（不说成本机丢了行）；
/// 那台断了的那一帧挤丢了 ⇒ 补的是 `ended`（断了比丢了一行大：下一条从 hello 起）。
#[test]
fn a_relayed_line_lost_on_a_full_channel_is_owed_as_that_machines_gap() {
    use cmd_wire::frame::Relayed;
    let line = |n: u32| {
        Frame::Relayed(Relayed::Line {
            machine: "m".into(),
            line: format!("{{\"n\":{n}}}"),
        })
    };
    let (tx, mut rx) = frame_channel(2 * wire_bytes(&line(1)));
    let mut sink = FrameSink::new(tx);
    sink.fan(line(1));
    sink.fan(line(2));
    sink.fan(line(3)); // 满了：丢
    let mut got = Vec::new();
    while let Ok(f) = rx.try_recv() {
        got.push(f);
    }
    assert_eq!(got, vec![line(1), line(2)]);
    sink.fan(line(4));
    let mut got = Vec::new();
    while let Ok(f) = rx.try_recv() {
        got.push(f);
    }
    assert_eq!(
        got,
        vec![
            Frame::Relayed(Relayed::Gap {
                machine: "m".into()
            }),
            line(4)
        ],
        "丢的那一行在那台的流上原位留一格 gap，不说成 overflow"
    );
    sink.fan(line(5));
    sink.fan(line(6));
    sink.fan(Frame::Relayed(Relayed::Ended {
        machine: "m".into(),
    })); // 满了：丢
    while rx.try_recv().is_ok() {}
    sink.fan(line(7));
    let mut got = Vec::new();
    while let Ok(f) = rx.try_recv() {
        got.push(f);
    }
    assert_eq!(
        got,
        vec![
            Frame::Relayed(Relayed::Ended {
                machine: "m".into()
            }),
            line(7)
        ]
    );
}

/// 打标托管那一臂的替身：照那个托管终端盘上此刻的事实判 —— 那一跳（`hop_pid`）就是这个进程 ⇒ 打上（写进 sid、回真写了）；
/// 那一跳还没落盘 ⇒ 说不清、不打（同读不到终端的平台上 `host::managed::seat_in` 那一判）。事实在哪由本线程的判据交进来。
fn hop_judged(pid: u32, sid: &str) -> Option<crate::host::managed::Tagged> {
    use crate::host::managed::{home, Tagged};
    let dir = HOP_FACTS.with(|d| d.borrow().clone())?;
    let mut f = home::read_facts(&dir)?;
    if f.hop_pid != Some(pid) {
        return Some(Tagged::seated(None, false));
    }
    let wrote = f.sid != sid;
    f.sid = sid.to_string();
    put_facts(&dir, &f);
    let container = crate::wire::SessionContainer::Managed {
        terminal: Some(f.id.clone()),
    };
    Some(Tagged::seated(Some(container), wrote))
}

thread_local! {
    static HOP_FACTS: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    static SEEN: std::cell::RefCell<Vec<(u32, String)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// 照托管进程的写法落事实：写旁名、换名上位。
fn put_facts(dir: &Path, f: &crate::host::managed::home::Facts) {
    let tmp = dir.join(".facts.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(f).unwrap()).unwrap();
    std::fs::rename(&tmp, dir.join(crate::host::managed::home::FACTS)).unwrap();
}

/// ★ 托管终端里那一跳与会话 pidfile 谁先落盘没有保证：pidfile 先到（打标那一判此刻说不清、不打），那一跳后到 ⇒
/// 那个托管终端的事实一改，耳朵当场听见、在跟的会话重对一次打标，打上了 —— 不等下一次 pidfile 改写 · `resync`。
/// 托管终端的家在耳朵挂上之后才建（它的事实可能在挂上子目录之前就写了）也算一改。真 notify 耳朵。
#[cfg(target_os = "linux")]
#[test]
fn the_hop_landing_after_the_pidfile_tags_the_session_at_once() {
    use crate::host::managed::home::Facts;
    let data = std::env::temp_dir().join(format!("ccm-hosts-ear-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    // 那一层已在（这台起过托管终端）：新来的托管终端的家是它里面的一项。
    std::fs::create_dir_all(data.join("hosts")).unwrap();
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let (etx, erx) = std::sync::mpsc::channel::<WatchEvent>();
    let (mut fs_watch, pending) = fs_ears(etx).expect("文件耳朵起不来");
    let mut ear = HostsEar::new(&own).expect("托管终端那一层说得出在哪");
    ear.arm(&mut fs_watch);
    let (_d, mut state, mut sink, _rx) = rig("hosts-ear");
    state.hosted = hop_judged;
    const PID: u32 = 4242;
    state.sessions.insert(
        data.join("4242.json"),
        SessionEntry {
            pid: PID,
            sid: A.into(),
            status: None,
            activity: None,
            since: None,
            facts: Default::default(),
            path: None,
            beat: std::time::SystemTime::now(),
            frozen: false,
            attachable: None,
            held: Held::Writer,
        },
    );
    // 同一个托管终端里另一个会话（不是那一跳，`claude -p` 那一形）：事实一改照样问打标，便条不随之再认。
    const OTHER: u32 = 5353;
    state.sessions.insert(
        data.join("5353.json"),
        SessionEntry {
            pid: OTHER,
            sid: B.into(),
            status: None,
            activity: None,
            since: None,
            facts: Default::default(),
            path: None,
            beat: std::time::SystemTime::now(),
            frozen: false,
            attachable: None,
            held: Held::Writer,
        },
    );
    // 便条那一手（`sid → 号 / 标签页` 认的是那一跳的 pid）：事实一改也跟着认一次，记下问过的 (pid, sid)。
    fn seen(_: &crate::platform::fs::Own, pid: u32, sid: &str) {
        SEEN.with(|s| s.borrow_mut().push((pid, sid.to_string())));
    }
    state.launch = Some((
        Reactions {
            tmux_server_up: || {},
            session_seen: seen,
            tag: untagged,
            account_of: |_, _| None,
            quota_said: |_, _, _, _| {},
        },
        own.clone(),
    ));
    let home = data.join("hosts").join("t-1");
    let facts_file = home.join(crate::host::managed::home::FACTS);
    // 做一件事，等到耳朵交来含 `must` 那一格的那一批（之前排着的批照样交给耳朵）：回那一批重对时真打上了几条。
    let mut heard = |act: &dyn Fn(), must: &Path| -> Option<usize> {
        act();
        let deadline = std::time::Instant::now() + guard_core::wall::CEILING;
        while std::time::Instant::now() < deadline {
            if let Ok(WatchEvent::Notify) = erx.recv_timeout(std::time::Duration::from_millis(300))
            {
                let evs = pending.take();
                let n = hosts_moved(&mut ear, &mut fs_watch, &evs, &mut state, &mut sink);
                if evs.iter().any(|p| p == must) {
                    return n;
                }
            }
        }
        None
    };
    HOP_FACTS.with(|d| *d.borrow_mut() = Some(home.clone()));
    let started = Facts {
        id: "t-1".into(),
        child_pid: Some(1),
        ..Default::default()
    };
    // 托管终端起来：家（耳朵挂上它之前事实就可能写了）⇒ 对了一次，还没有那一跳 ⇒ 说不清、不打。
    assert_eq!(
        heard(
            &|| {
                std::fs::create_dir_all(&home).unwrap();
                put_facts(&home, &started);
            },
            &home
        ),
        Some(0),
        "新来的托管终端的家没听见（或还没那一跳就打上了）"
    );
    // pidfile 早到过、标没打上；那一跳此刻落盘 ⇒ 听见的就是这个托管终端的事实那一格，当场打上、便条也认一次。
    let hopped = Facts {
        hop_pid: Some(PID),
        ..started.clone()
    };
    SEEN.with(|s| s.borrow_mut().clear());
    assert_eq!(
        heard(&|| put_facts(&home, &hopped), &facts_file),
        Some(1),
        "那一跳落了盘，打标没当场重对"
    );
    assert_eq!(
        crate::host::managed::home::read_facts(&home).map(|f| f.sid),
        Some(A.to_string())
    );
    assert!(
        SEEN.with(|s| s.borrow().contains(&(PID, A.to_string()))),
        "那一跳落了盘，起会话便条没跟着认"
    );
    assert!(
        !SEEN.with(|s| s.borrow().iter().any(|(p, _)| *p == OTHER)),
        "不是前台的那一个也跟着事实一改再认便条"
    );
    // 打标自己写了事实 ⇒ 又是一改：再对一次，已是这个 sid、不写 —— 到此为止，不连环。
    assert_eq!(
        heard(&|| {}, &facts_file),
        Some(0),
        "打标写的那一改又打了一次（会连环）"
    );
    std::fs::remove_dir_all(&data).ok();
}

/// 托管终端那一层里别的动静（套接字 · 旁名 · 层外的文件）不算事实改了；那一层里的 `facts.json` 才算。
#[test]
fn only_a_hosted_terminals_facts_count_as_moved() {
    let data = std::env::temp_dir().join(format!("ccm-hosts-paths-{}", std::process::id()));
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let mut ear = HostsEar::new(&own).expect("托管终端那一层说得出在哪");
    let hosts = data.join("hosts");
    let mut w = notify::NullWatcher;
    let mut hit = |p: PathBuf| ear.touched(&mut w, std::iter::once(p.as_path()));
    assert!(hit(hosts.join("t-1").join("facts.json")));
    assert!(!hit(hosts.join("t-1").join("sock")));
    assert!(!hit(hosts.join("t-1").join(".facts.json.tmp")));
    assert!(!hit(data.join("facts.json")));
    assert!(!hit(hosts.join("t-1").join("x").join("facts.json")));
}

thread_local! {
    /// [`housed_by_hand`] 这一回交的打标结局。
    static HOUSED: std::cell::Cell<Option<(bool, bool)>> = const { std::cell::Cell::new(None) };
}

/// 打标托管那一臂的替身：交判据手里那一回的结局（`(容器说得出, 真写了)`）。
/// 宣告的是 pid 7 那一份；同一个 sid 另一份 pidfile（别的 pid，resume 时原进程还活着那一形）恒说不清。
fn housed_by_hand(pid: u32, _: &str) -> Option<crate::host::managed::Tagged> {
    if pid != 7 {
        return Some(crate::host::managed::Tagged::seated(None, false));
    }
    let (housed, wrote) = HOUSED.with(std::cell::Cell::get)?;
    let container = housed.then(|| crate::wire::SessionContainer::Managed {
        terminal: Some("t-1".into()),
    });
    Some(crate::host::managed::Tagged::seated(container, wrote))
}

/// 这一批里给 `sid` 的那几帧脸（`session_status`）。
fn faces_of(rx: &mut FrameRx, sid: &str) -> Vec<crate::wire::SessionFace> {
    let mut got = Vec::new();
    while let Ok(f) = rx.try_recv() {
        if let Frame::SessionStatus(s) = f {
            if s.sid == sid {
                got.push(s.face);
            }
        }
    }
    got
}

/// ★ 已宣告的会话，标晚补上（托管终端的事实一改那一路）⇒ 记着的那份宣告改成那个容器、推一帧脸照它（标签页那张脸不等 resync 就亮）；
/// 之后再对一次说不清 ⇒ 照实落说不清（不留上一次的容器）、再推一帧；结局没变 ⇒ 一帧不推。
/// 容器只照宣告的那一份 pidfile 的进程改：同一个 sid 另一份 pidfile 的结局不来回翻它。
#[test]
fn a_tag_landing_late_moves_the_announced_face_and_unsure_says_unsure() {
    let (_d, mut state, mut sink, mut rx) = rig("rehouse");
    state.hosted = housed_by_hand;
    for pid in [7, 8] {
        state.sessions.insert(
            PathBuf::from(format!("/x/{pid}.json")),
            SessionEntry {
                pid,
                sid: A.into(),
                status: None,
                activity: None,
                since: None,
                facts: Default::default(),
                path: None,
                beat: std::time::SystemTime::now(),
                frozen: false,
                attachable: None,
                held: Held::Writer,
            },
        );
    }
    state.active_sids.insert(A.into());
    let mut announced = added_row(A);
    announced.wire.pid = 7;
    state.added.insert(A.into(), announced);
    let unhoused = state.live_face(A);
    let managed = crate::wire::SessionContainer::Managed {
        terminal: Some("t-1".into()),
    };
    // 标补上了：容器说得出。
    HOUSED.with(|h| h.set(Some((true, true))));
    assert_eq!(retag_hosted(&mut state, &mut sink), 1);
    assert_eq!(state.added[A].wire.container, Some(managed));
    let faces = faces_of(&mut rx, A);
    assert_eq!(faces, vec![state.live_face(A)], "补上的标没推那张脸");
    assert_ne!(faces[0], unhoused, "推的那张脸没照新容器");
    // 同一个结局再来一次：一帧不推。
    HOUSED.with(|h| h.set(Some((true, false))));
    retag_hosted(&mut state, &mut sink);
    assert!(faces_of(&mut rx, A).is_empty(), "容器没变也推了一帧");
    // 说不清：照实落说不清，不留上一次的容器。
    HOUSED.with(|h| h.set(Some((false, false))));
    retag_hosted(&mut state, &mut sink);
    assert_eq!(
        state.added[A].wire.container, None,
        "说不清还挂着上一次的容器"
    );
    assert_eq!(faces_of(&mut rx, A), vec![unhoused.clone()]);
    // 宣告的那个进程先退了、sid 还挂在另一份 pidfile 上 ⇒ 宣告改认活着的那一个、当场照它认一次身份（它说不清 ⇒ 落说不清）。
    HOUSED.with(|h| h.set(Some((true, false))));
    retag_hosted(&mut state, &mut sink);
    let _ = faces_of(&mut rx, A);
    state.sessions.remove(&PathBuf::from("/x/7.json"));
    retire_sid_if_unreferenced(A, None, &mut state, &mut sink);
    assert_eq!(state.added[A].wire.pid, 8, "宣告还认着退了的那个进程");
    assert_eq!(
        state.added[A].wire.container, None,
        "退了的那个进程的容器还挂着"
    );
    assert_eq!(faces_of(&mut rx, A), vec![unhoused]);
}

/// ★ 「先打标、再认便条」只此一处：生产段里认便条只有它那一处调用（[`identify`]），各条重对的路都经它，次序不会两处各写一份。
#[test]
fn the_launch_note_is_adopted_in_one_place_only() {
    let src = include_str!("../../../src/backend/observe/watcher.rs");
    // 生产段 = 测试模块之前（同 `every_watch_site_answers_the_inode_swap_question` 的切法）。
    let cut = src
        .find("\n#[cfg(test)]")
        .map(|i| src[i..].find("\nmod ").map(|j| i + j).unwrap_or(i));
    let prod = &src[..cut.unwrap_or(src.len())];
    assert_eq!(
        prod.matches("adopt_launch_note(").count(),
        2,
        "认便条该只有定义那一处与 `identify` 里那一处调用"
    );
}

/// 真判法那一臂的数据目录（判据起的那个托管终端的；跨线程读，同一个测试进程只装这一回）。
static REAL_OWN: std::sync::OnceLock<crate::platform::fs::Own> = std::sync::OnceLock::new();

/// 打标托管那一臂：真判法（[`crate::host::managed::tag_pid_at`]），数据目录是判据起的那一份。
fn real_hosted(pid: u32, sid: &str) -> Option<crate::host::managed::Tagged> {
    crate::host::managed::tag_pid_at(REAL_OWN.get()?, pid, sid)
}

/// ★ 打个没完挡在真判法里：真起一个托管终端（假子进程 `sh`），会话就是它的子进程。重对打上标（托管进程写事实）⇒
/// 那一改又叫一次重对 ⇒ 事实里已是这个 sid、不写（`host::managed` 打标那一判），到此为止。不用替身。
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retagging_after_its_own_write_writes_nothing_on_the_real_judge() {
    use crate::host::managed::run::test_support::{
        hello, read_until_out, sandbox_own, spec_ready, start,
    };
    use crate::host::managed::wire::{write_frame, Frame as HostFrame, Want};
    let whole = tokio::time::timeout(guard_core::wall::CEILING, async {
        let (own, data) = sandbox_own("retag-real");
        let (sock, task) = start(&own, spec_ready("t-real")).await;
        let mut w = hello(&sock, Want::View).await;
        read_until_out(&mut w, "ready").await;
        drop(w);
        let home = data.join("hosts").join("t-real");
        let pid = crate::host::managed::home::read_facts(&home)
            .and_then(|f| f.child_pid)
            .expect("子进程 pid");
        assert!(
            REAL_OWN.set(own.clone()).is_ok(),
            "真判法的数据目录只装这一回"
        );
        let facts_file = home.join(crate::host::managed::home::FACTS);
        let watch_own = own.clone();
        let (first, again) = tokio::task::spawn_blocking(move || {
            let (etx, erx) = std::sync::mpsc::channel::<WatchEvent>();
            let (mut fs_watch, pending) = fs_ears(etx).expect("文件耳朵起不来");
            let mut ear = HostsEar::new(&watch_own).expect("托管终端那一层说得出在哪");
            ear.arm(&mut fs_watch);
            let (_d, mut state, mut sink, _rx) = rig("retag-real");
            state.hosted = real_hosted;
            state.sessions.insert(
                PathBuf::from(format!("/x/{pid}.json")),
                SessionEntry {
                    pid,
                    sid: "0b9c2f4e-1111-4222-8333-944455556677".into(),
                    status: None,
                    activity: None,
                    since: None,
                    facts: Default::default(),
                    path: None,
                    beat: std::time::SystemTime::now(),
                    frozen: false,
                    attachable: None,
                    held: Held::Writer,
                },
            );
            let first = retag_hosted(&mut state, &mut sink);
            // 打标那一下写了事实 ⇒ 耳朵听见那一格，再对一次。
            let deadline = std::time::Instant::now() + guard_core::wall::CEILING;
            let mut again = None;
            while again.is_none() && std::time::Instant::now() < deadline {
                if let Ok(WatchEvent::Notify) =
                    erx.recv_timeout(std::time::Duration::from_millis(300))
                {
                    let evs = pending.take();
                    let n = hosts_moved(&mut ear, &mut fs_watch, &evs, &mut state, &mut sink);
                    if evs.iter().any(|p| *p == facts_file) {
                        again = n;
                    }
                }
            }
            (first, again)
        })
        .await
        .expect("没崩");
        assert_eq!(first, 1, "头一回重对该打上标");
        assert_eq!(again, Some(0), "打标自己写的那一改又写了一次（会打个没完）");
        let mut s = hello(&sock, Want::Input).await;
        write_frame(&mut s, &HostFrame::In(b"\r".to_vec()))
            .await
            .expect("送字");
        task.await.expect("任务没崩").expect("跑完");
    });
    whole.await.expect("兜底上限之内跑完");
}

thread_local! {
    /// [`housed_when_told`] 此刻交不交容器。
    static TOLD_HOUSED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// 打标托管那一臂的替身：判据说交 ⇒ 容器是托管终端 `t-9`；否则说不清。
fn housed_when_told(_: u32, _: &str) -> Option<crate::host::managed::Tagged> {
    let container =
        TOLD_HOUSED
            .with(std::cell::Cell::get)
            .then(|| crate::wire::SessionContainer::Managed {
                terminal: Some("t-9".into()),
            });
    Some(crate::host::managed::Tagged::seated(container, false))
}

/// 两份 pidfile 那几条判据的会话 id。
const ONE: &str = "one-frame-1";

/// 同一个 sid 挂两份 pidfile 的夹具：宣告认定的是先来的那一份 `kid`；后来那一份 `other` 是 resume 时原进程还活着那一形。
/// 两份起先各说 `first` · `second`（`other` 那一份落下时推的帧留着，[`TwoOfOne::told`] 读）；托管那一臂此刻交不交容器照
/// `TOLD_HOUSED`（起先不交）。收尾掐掉两个进程、删掉目录。
struct TwoOfOne {
    dir: PathBuf,
    sessions: PathBuf,
    kid: std::process::Child,
    other: std::process::Child,
    state: ReaderState,
    sink: FrameSink,
    rx: FrameRx,
}

impl TwoOfOne {
    fn new(tag: &str, first: &str, second: &str) -> Self {
        use super::test_support::write_pidfile;
        let dir = std::env::temp_dir().join(format!("ccm-two-of-one-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sessions = dir.join("sessions");
        let kid = crate::platform::proc::test_support::spawn_settled_sleep(|_| {});
        let other = crate::platform::proc::test_support::spawn_settled_sleep(|_| {});
        let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
        let mut t = Self {
            state: ReaderState::new(dir.join("projects")),
            sink: FrameSink::new(tx),
            rx,
            sessions,
            dir,
            kid,
            other,
        };
        t.state.hosted = housed_when_told;
        TOLD_HOUSED.with(|h| h.set(false));
        let p = write_pidfile(&t.sessions, t.kid.id(), ONE, first);
        process_session_added(&p, &mut t.state, &mut t.sink);
        while t.rx.try_recv().is_ok() {}
        let q = write_pidfile(&t.sessions, t.other.id(), ONE, second);
        process_session_added(&q, &mut t.state, &mut t.sink);
        t
    }

    fn pid(&self, announced: bool) -> u32 {
        if announced {
            self.kid.id()
        } else {
            self.other.id()
        }
    }

    /// 那一份 pidfile 在 watcher 账上的键。
    fn key(&self, announced: bool) -> PathBuf {
        path_key(&self.sessions.join(format!("{}.json", self.pid(announced))))
    }

    /// 把那一份 pidfile 改写成 `status`、交 watcher 读一回。
    fn rewrite(&mut self, announced: bool, status: &str) {
        let at =
            super::test_support::write_pidfile(&self.sessions, self.pid(announced), ONE, status);
        process_session_added(&at, &mut self.state, &mut self.sink);
    }

    /// 上次读过之后推出的 `session_status`（按先后）。
    fn told(&mut self) -> Vec<crate::wire::SessionStatus> {
        std::iter::from_fn(|| self.rx.try_recv().ok())
            .filter_map(|f| match f {
                Frame::SessionStatus(s) if s.sid == ONE => Some(s),
                _ => None,
            })
            .collect()
    }

    /// 活会话表照 watcher 发布的那一造法现扫（[`table_of`]），说这条在干什么。
    fn table(&self) -> Option<crate::agents::SessionActivity> {
        table_of(&self.state)
            .doing_one(ONE)
            .expect("活会话表里该有这一条")
    }
}

/// ★ 冻住按进程，两边同一个喂口：另一个进程挂着两份 pidfile、都说在跑，只一份过了时限没再听见 ⇒
/// 那个进程冻住，watcher 与活会话表都不再照它说在跑（认定那份闲 ⇒ 都说闲）。
#[test]
fn a_frozen_process_with_two_pidfiles_feeds_both_sides_alike() {
    let mut t = TwoOfOne::new("freeze-proc", "idle", "busy");
    let second = t.dir.join("sessions2");
    let q = super::test_support::write_pidfile(&second, t.other.id(), ONE, "busy");
    process_session_added(&q, &mut t.state, &mut t.sink);
    let later = std::time::SystemTime::now() + crate::observe::runs::STALE_AFTER * 2;
    t.state
        .sessions
        .get_mut(&path_key(&q))
        .expect("第二份在账上")
        .beat = later;
    judge_beats(&mut t.state, &mut t.sink, later);
    assert!(t.state.sessions[&t.key(false)].frozen, "夹具没把那一份冻住");
    assert_eq!(
        t.state.heard(ONE),
        t.table(),
        "watcher 与活会话表喂的冻住不一样"
    );
    assert_eq!(t.table(), Some(crate::agents::SessionActivity::Idle));
}

/// ★ 只算活着的，两边同一个喂口：另一个进程已经退了、看守还没来得及摘它 ⇒ watcher 也不再照它说在跑（同活会话表）。
#[test]
fn a_dead_process_not_yet_let_go_feeds_neither_side() {
    let mut t = TwoOfOne::new("dead-other", "idle", "busy");
    let _ = t.other.kill();
    let _ = t.other.wait();
    assert_eq!(t.table(), Some(crate::agents::SessionActivity::Idle));
    assert_eq!(
        t.state.heard(ONE),
        t.table(),
        "watcher 算上了已经死了的那一份"
    );
}

impl Drop for TwoOfOne {
    fn drop(&mut self) {
        for k in [&mut self.kid, &mut self.other] {
            let _ = k.kill();
            let _ = k.wait();
        }
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

/// 这张脸说的那一态。
fn life_of(s: &crate::wire::SessionStatus) -> Option<crate::agents::SessionActivity> {
    match s.face.life {
        crate::wire::WireLife::Live { activity } => activity,
        ref other => panic!("脸不是活着的：{other:?}"),
    }
}

/// 这张脸接进终端时接的是哪个托管终端（不给接 ⇒ `None`）。
fn terminal_of(s: &crate::wire::SessionStatus) -> Option<String> {
    match &s.face.can.view {
        crate::wire::CanView::Yes {
            open: crate::wire::TermOpen::Attach { terminal },
            ..
        } => terminal.clone(),
        _ => None,
    }
}

/// ★ 一次认定一帧 · 状态照活会话表：两份都在跑，认定的那一份改写成闲、托管那一臂此刻交容器 `t-9` ⇒ 只推一帧；
/// 那一态照两份里最要紧的那一个（另一份还在跑 ⇒ 在跑），与活会话表说的一样；容器跟认定的进程换成 `t-9`；记录路径没变不上线。
#[test]
fn the_announced_going_idle_while_the_other_runs_says_running_like_the_table_in_one_frame() {
    let mut t = TwoOfOne::new("idle-announced", "busy", "busy");
    let _ = t.told();
    TOLD_HOUSED.with(|h| h.set(true));
    t.rewrite(true, "idle");
    let said = t.told();
    assert_eq!(said.len(), 1, "一次认定推了 {} 帧：{said:?}", said.len());
    assert_eq!(
        life_of(&said[0]),
        Some(crate::agents::SessionActivity::Working),
        "另一份还在跑，标签页却不说在跑"
    );
    assert_eq!(life_of(&said[0]), t.table(), "标签页与活会话表说的不一样");
    assert_eq!(
        terminal_of(&said[0]).as_deref(),
        Some("t-9"),
        "脸没照新容器"
    );
    assert_eq!(said[0].path, None, "记录路径没变也上了线");
}

/// ★ 另一份改写照常推帧：认定的那一份在跑，另一份改写成闲 ⇒ 推一帧，那一态照两份里最要紧的（在跑，同活会话表）；
/// 身份那一类只跟认定的进程：托管那一臂此刻交 `t-9`，可改写的不是认定那一份 ⇒ 容器不动。
#[test]
fn a_rewrite_of_the_other_pidfile_is_told_with_the_state_of_both_and_the_identity_of_the_announced()
{
    let mut t = TwoOfOne::new("idle-other", "busy", "busy");
    let _ = t.told();
    TOLD_HOUSED.with(|h| h.set(true));
    t.rewrite(false, "idle");
    let said = t.told();
    assert_eq!(said.len(), 1, "另一份改写推了 {} 帧：{said:?}", said.len());
    assert_eq!(
        life_of(&said[0]),
        Some(crate::agents::SessionActivity::Working)
    );
    assert_eq!(life_of(&said[0]), t.table(), "标签页与活会话表说的不一样");
    assert_eq!(
        terminal_of(&said[0]),
        None,
        "容器跟着没被认定的那个进程换了"
    );
}

/// ★ 认定那份闲、另一份后来落下说在跑 ⇒ 当场推一帧说在跑（同活会话表）。
#[test]
fn a_second_pidfile_that_runs_while_the_announced_idles_is_told_at_once() {
    let mut t = TwoOfOne::new("second-busy", "idle", "busy");
    let said = t.told();
    assert_eq!(said.len(), 1, "另一份落下推了 {} 帧：{said:?}", said.len());
    assert_eq!(
        life_of(&said[0]),
        Some(crate::agents::SessionActivity::Working)
    );
    assert_eq!(life_of(&said[0]), t.table(), "标签页与活会话表说的不一样");
}

/// ★ 另一份解冻 ⇒ 脸当场跟上：认定那份闲、另一份在跑；过了时限没再听见另一份 ⇒ 冻住，脸改说闲（同活会话表）；
/// 另一份改写（状态字没变、只是又听见了）⇒ 当场推一帧回到在跑。
#[test]
fn the_other_pidfile_thawing_brings_the_face_back_at_once() {
    let mut t = TwoOfOne::new("thaw-other", "idle", "busy");
    let _ = t.told();
    let later = std::time::SystemTime::now() + crate::observe::runs::STALE_AFTER * 2;
    judge_beats(&mut t.state, &mut t.sink, later);
    let froze = t.told();
    assert_eq!(
        froze.len(),
        1,
        "另一份冻住推了 {} 帧：{froze:?}",
        froze.len()
    );
    assert_eq!(
        life_of(&froze[0]),
        Some(crate::agents::SessionActivity::Idle)
    );
    assert_eq!(life_of(&froze[0]), t.table(), "标签页与活会话表说的不一样");
    let key = t.key(false);
    t.state.sessions.get_mut(&key).expect("另一份在账上").beat = std::time::UNIX_EPOCH;
    t.rewrite(false, "busy");
    let thawed = t.told();
    assert_eq!(
        thawed.len(),
        1,
        "另一份解冻推了 {} 帧：{thawed:?}",
        thawed.len()
    );
    assert_eq!(
        life_of(&thawed[0]),
        Some(crate::agents::SessionActivity::Working)
    );
    assert_eq!(life_of(&thawed[0]), t.table(), "标签页与活会话表说的不一样");
}

/// ★ 冻住 · 回来不推空帧：两份都在跑，只有另一份冻住 ⇒ 认定那份还在跑、脸不变，一帧不推；它又听见了（记录读到新字节）⇒ 也不推。
#[test]
fn the_other_pidfile_freezing_or_thawing_without_moving_the_face_says_nothing() {
    let mut t = TwoOfOne::new("freeze-other", "busy", "busy");
    let _ = t.told();
    let later = std::time::SystemTime::now() + crate::observe::runs::STALE_AFTER * 2;
    let key = t.key(true);
    t.state.sessions.get_mut(&key).expect("认定那份在账上").beat = later;
    judge_beats(&mut t.state, &mut t.sink, later);
    assert!(t.state.sessions[&t.key(false)].frozen, "夹具没把另一份冻住");
    assert!(t.told().is_empty(), "脸没变也推了帧");
    heard_from(&mut t.state, &mut t.sink, ONE, later);
    assert!(t.told().is_empty(), "回来了、脸没变也推了帧");
}

/// ★ 另一份退了、认定那份还在 ⇒ 那一态照剩下的重判，推一帧（另一份在跑、认定那份闲 ⇒ 改说闲）。
#[test]
fn the_other_process_leaving_retells_the_state_of_what_is_left() {
    let mut t = TwoOfOne::new("other-leaves", "idle", "busy");
    let _ = t.told();
    let key = t.key(false);
    let gone = t.state.sessions.remove(&key).expect("另一份在账上");
    let_go(&gone, None, &mut t.state, &mut t.sink);
    let said = t.told();
    assert_eq!(said.len(), 1, "另一份退了推了 {} 帧：{said:?}", said.len());
    assert_eq!(
        life_of(&said[0]),
        Some(crate::agents::SessionActivity::Idle)
    );
}

/// ★ 认定的那个进程退了、改认之后身份那几格都没变 ⇒ 不靠改认推帧；那一态照剩下的那一份重判变了 ⇒ 推一帧（认定那份在跑、另一份闲 ⇒ 改说闲）。
#[test]
fn the_announced_process_leaving_retells_the_state_even_when_the_identity_stays() {
    let mut t = TwoOfOne::new("announced-leaves", "busy", "idle");
    let _ = t.told();
    let key = t.key(true);
    let gone = t.state.sessions.remove(&key).expect("认定那份在账上");
    let_go(&gone, None, &mut t.state, &mut t.sink);
    assert_eq!(
        t.state.added[ONE].wire.pid,
        t.other.id(),
        "没改认活着的那一份"
    );
    let said = t.told();
    assert_eq!(
        said.len(),
        1,
        "认定那份退了推了 {} 帧：{said:?}",
        said.len()
    );
    assert_eq!(
        life_of(&said[0]),
        Some(crate::agents::SessionActivity::Idle)
    );
}

/// ★ pidfile 改写、状态字没变、只是那一态的时刻挪到钩子事件之后 ⇒ 那一态改照 pidfile（同活会话表），脸当场推一帧。
/// 钩子事件是整个进程共用的一张表：用自己的 sid，不碰别的判据。
#[test]
fn a_rewrite_that_only_moves_the_state_time_past_the_heard_event_is_told() {
    let sid = "since-moves-1";
    let dir = std::env::temp_dir().join(format!("ccm-since-moves-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sessions = dir.join("sessions");
    let mut kid = crate::platform::proc::test_support::spawn_settled_sleep(|_| {});
    let write = |since: u64| {
        let p = super::test_support::write_pidfile(&sessions, kid.id(), sid, "busy");
        let body = std::fs::read_to_string(&p).unwrap();
        let body = format!(
            "{},\"statusUpdatedAt\":{since}}}",
            body.trim_end().trim_end_matches('}')
        );
        std::fs::write(&p, body).unwrap();
        p
    };
    let (tx, mut rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"));
    let p = write(1);
    process_session_added(&p, &mut state, &mut sink);
    crate::history::live_table::hear(sid, crate::agents::AgentEvent::TurnEnded);
    assert_eq!(
        state.heard(sid),
        Some(crate::agents::SessionActivity::Idle),
        "夹具：钩子事件比 pidfile 新，该照事件"
    );
    while rx.try_recv().is_ok() {}
    let p = write(crate::common::time::now_ms() + 60_000);
    process_session_added(&p, &mut state, &mut sink);
    let said: Vec<crate::wire::SessionStatus> = std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|f| match f {
            Frame::SessionStatus(s) if s.sid == sid => Some(s),
            _ => None,
        })
        .collect();
    let _ = kid.kill();
    let _ = kid.wait();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(
        said.len(),
        1,
        "那一态的时刻挪了推了 {} 帧：{said:?}",
        said.len()
    );
    assert_eq!(
        life_of(&said[0]),
        Some(crate::agents::SessionActivity::Working)
    );
}

/// 手搭的两份：认定的进程 7 闲着，另一份 8 在跑（`facts` · `path` 照给的）。
fn idle_seven_busy_eight(
    state: &mut ReaderState,
    eight: crate::history::search_rules::LabelFacts,
    eight_path: Option<String>,
) {
    use crate::agents::SessionActivity as S;
    for (pid, activity, facts, path) in [
        (7, S::Idle, Default::default(), None),
        (8, S::Working, eight, eight_path),
    ] {
        state.sessions.insert(
            PathBuf::from(format!("/x/{pid}.json")),
            SessionEntry {
                pid,
                sid: A.into(),
                status: None,
                activity: Some(activity),
                since: None,
                facts,
                path,
                beat: std::time::SystemTime::now(),
                frozen: false,
                attachable: None,
                held: Held::Writer,
            },
        );
    }
    let mut announced = added_row(A);
    announced.wire.pid = 7;
    state.added.insert(A.into(), announced);
}

/// ★ 改名那一帧的那一态照两份里最要紧的（不照改到的那一份）：只有认定那一份（闲）的名字还要改，另一份在跑 ⇒ 推一帧、说在跑。
#[test]
fn a_rename_tells_the_state_of_both_not_of_the_pidfile_it_touched() {
    let (_d, mut state, mut sink, mut rx) = rig("rename-two");
    let named = crate::history::search_rules::LabelFacts {
        custom: Some("x".into()),
        ..Default::default()
    };
    idle_seven_busy_eight(&mut state, named, None);
    state.renamed(
        A,
        |f| {
            let moved = f.custom.is_none();
            f.custom = Some("x".into());
            moved
        },
        &mut sink,
    );
    let faces = faces_of(&mut rx, A);
    assert_eq!(faces.len(), 1, "改名推了 {} 帧", faces.len());
    assert_eq!(
        faces[0].life,
        crate::wire::WireLife::Live {
            activity: Some(crate::agents::SessionActivity::Working)
        },
        "改名那一帧照改到的那一份说那一态"
    );
}

/// ★ 记录路径换了那一帧的那一态照两份里最要紧的：另一份（在跑）已记着这条路径、只有认定那一份（闲）跟上 ⇒ 推一帧带路径、说在跑。
#[test]
fn a_new_record_path_tells_the_state_of_both_not_of_the_pidfile_it_touched() {
    let (_d, mut state, mut sink, mut rx) = rig("path-two");
    let p = "/x/r.jsonl".to_string();
    idle_seven_busy_eight(&mut state, Default::default(), Some(p.clone()));
    note_path(A, &p, &mut state, &mut sink);
    let said: Vec<crate::wire::SessionStatus> = std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|f| match f {
            Frame::SessionStatus(s) if s.sid == A => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(said.len(), 1, "换路径推了 {} 帧", said.len());
    assert_eq!(said[0].path.as_deref(), Some(p.as_str()));
    assert_eq!(
        life_of(&said[0]),
        Some(crate::agents::SessionActivity::Working),
        "换路径那一帧照改到的那一份说那一态"
    );
}

/// ★ `session_status` 那一帧只拼一处（[`tell`]）：生产段里 `Frame::SessionStatus(` 除了认帧的模式（`|` 起头 · 在 `=>` 左边），只构造一次。
#[test]
fn the_session_status_frame_is_built_in_one_place_only() {
    let src = include_str!("../../../src/backend/observe/watcher.rs");
    let cut = src
        .find("\n#[cfg(test)]")
        .map(|i| src[i..].find("\nmod ").map(|j| i + j).unwrap_or(i));
    let prod = &src[..cut.unwrap_or(src.len())];
    let built = prod
        .lines()
        .filter(|l| l.contains("Frame::SessionStatus("))
        .map(str::trim_start)
        .filter(|l| {
            let pattern = l
                .find("=>")
                .is_some_and(|a| l.find("Frame::SessionStatus(").is_some_and(|f| f < a));
            !l.starts_with("//") && !l.starts_with('|') && !pattern
        })
        .count();
    assert_eq!(built, 1, "拼 `session_status` 那一帧的不止一处");
}

/// 号照进程判（替身）：换号重启那一形 —— 旧进程 7 用旧号，新进程 8 用新号。
fn account_by_pid(pid: u32, _: &'static str) -> Option<String> {
    Some(if pid == 8 { "new" } else { "old" }.to_string())
}

/// ★ 换号重启、旧进程后退：宣告认的是旧进程（旧号 · 接得进去），它一退、改认新进程 ⇒ 脸上跟进程走的那几格一起照新进程换
/// （号、接进去有没有意义、容器），一次只推一帧。
#[test]
fn when_the_announced_process_leaves_every_cell_that_follows_it_moves_together() {
    let (_d, mut state, mut sink, mut rx) = rig("heir-cells");
    state.hosted = housed_by_hand;
    state.account_of = account_by_pid;
    for (pid, attachable) in [(7, None), (8, Some(false))] {
        state.sessions.insert(
            PathBuf::from(format!("/x/{pid}.json")),
            SessionEntry {
                pid,
                sid: A.into(),
                status: None,
                activity: None,
                since: None,
                facts: Default::default(),
                path: None,
                beat: std::time::SystemTime::now(),
                frozen: false,
                attachable,
                held: Held::Writer,
            },
        );
    }
    state.active_sids.insert(A.into());
    let mut announced = added_row(A);
    announced.wire.pid = 7;
    announced.wire.face.account = Some("old".into());
    state.added.insert(A.into(), announced);
    HOUSED.with(|h| h.set(Some((true, false))));
    retag_hosted(&mut state, &mut sink);
    let _ = faces_of(&mut rx, A);
    state.sessions.remove(&PathBuf::from("/x/7.json"));
    retire_sid_if_unreferenced(A, None, &mut state, &mut sink);
    let a = &state.added[A];
    assert_eq!(a.wire.pid, 8);
    assert_eq!(
        a.wire.face.account.as_deref(),
        Some("new"),
        "号还是退掉那个进程的"
    );
    assert_eq!(
        a.attachable,
        Some(false),
        "接进去有没有意义还是退掉那个进程的"
    );
    assert_eq!(a.wire.container, None, "容器还是退掉那个进程的");
    assert_eq!(faces_of(&mut rx, A).len(), 1, "换认那一下该只推一帧");
}

/// 打标托管那一臂的替身：新进程 8 住在托管终端 `t-8` 里（一退终端不留），旧进程 7 说不清。
fn heir_housed(pid: u32, _: &str) -> Option<crate::host::managed::Tagged> {
    let container = (pid == 8).then(|| crate::wire::SessionContainer::Managed {
        terminal: Some("t-8".into()),
    });
    Some(crate::host::managed::Tagged::seated(container, false))
}

/// ★ 改认之后会话账本跟上：换号重启、旧进程 7 先退 ⇒ 改认新进程 8（新号、托管终端里）；它也退了 ⇒ 离开那一帧当场就来
/// （托管终端跟着收，不等 tmux 观测），带的是新进程的号。
#[test]
fn after_the_heir_takes_over_the_ledger_says_its_cells_when_it_leaves() {
    let (_d, mut state, _, _) = rig("heir-ledger");
    let mut sink = ledgered(true);
    let mut rx = conn(&mut sink, &mut state, 1, None);
    state.hosted = heir_housed;
    state.account_of = account_by_pid;
    for pid in [7, 8] {
        state.sessions.insert(
            PathBuf::from(format!("/x/{pid}.json")),
            SessionEntry {
                pid,
                sid: A.into(),
                status: None,
                activity: None,
                since: None,
                facts: Default::default(),
                path: None,
                beat: std::time::SystemTime::now(),
                frozen: false,
                attachable: None,
                held: Held::Writer,
            },
        );
    }
    state.active_sids.insert(A.into());
    let mut announced = added_row(A);
    announced.wire.pid = 7;
    announced.wire.face.account = Some("old".into());
    state.added.insert(A.into(), announced.clone());
    sink.announce(announced);
    state.sessions.remove(&PathBuf::from("/x/7.json"));
    retire_sid_if_unreferenced(A, None, &mut state, &mut sink);
    while rx.try_recv().is_ok() {}
    state.sessions.remove(&PathBuf::from("/x/8.json"));
    retire_sid_if_unreferenced(A, None, &mut state, &mut sink);
    let left: Vec<crate::wire::SessionLeft> = std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|f| match f {
            Frame::SessionLeft(s) if s.sid == A => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(
        left.len(),
        1,
        "新进程住在托管终端里，退了该当场说已结束：{left:?}"
    );
    assert_eq!(
        left[0].face.account.as_deref(),
        Some("new"),
        "离开那一帧带的还是退掉那个进程的号"
    );
}
