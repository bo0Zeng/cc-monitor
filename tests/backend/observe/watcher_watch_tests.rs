//! 流只发要看的会话（`stream-watch`）：这条流没报过在看哪几个 ⇒ 全发；报了 ⇒ 不在里面的会话，它的整行帧
//! （行 · 运行表 · 主线外清单）不上这条流，摘要那几格（宣告 · 状态 · 轮次边沿 · 去向）照发；
//! 看了以后回「这条流从第几行起发它」，之前那段由客户端按骨架补 —— 合起来不重复也不漏。
//! 语料是结构性的假行（`{"n":…}`），不含任何真会话正文。

use super::*;
use crate::stream::wire::WatchFrom;

const A: &str = "s-wa";
const B: &str = "s-wb";

fn rig(
    tag: &str,
) -> (
    PathBuf,
    ReaderState,
    FrameSink,
    tokio::sync::mpsc::Receiver<Frame>,
) {
    let dir = std::env::temp_dir().join(format!("ccm-watch-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel::<Frame>(256);
    let mut state = ReaderState::new(dir.join("projects"));
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
fn lines(rx: &mut tokio::sync::mpsc::Receiver<Frame>) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    while let Ok(f) = rx.try_recv() {
        if let Frame::Line {
            session_id, seq, ..
        } = f
        {
            out.push((session_id, seq));
        }
    }
    out
}

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
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
    watch_sessions(ids(&[A]), &mut state, &mut sink);
    append(&file(&dir, A), 2);
    append(&file(&dir, B), 3);
    process_jsonl(&file(&dir, A), &mut state, &mut sink);
    let n = process_jsonl(&file(&dir, B), &mut state, &mut sink);
    assert_eq!(n, 3, "没在看的会话照读（行号照数），只是不上流");
    assert_eq!(lines(&mut rx), vec![(A.into(), 0), (A.into(), 1)]);
    // 运行表 · 主线外清单也是那一行的内容：没在看就不上流；在看的照发。
    sink.send(state.runs.frame(B));
    sink.send(branch_frame(&state, B, &file(&dir, B)));
    sink.send(state.runs.frame(A));
    let mut kinds = Vec::new();
    while let Ok(f) = rx.try_recv() {
        kinds.push(match f {
            Frame::SessionRuns { sid, .. } => format!("runs {sid}"),
            Frame::SessionBranch { sid, .. } => format!("branch {sid}"),
            _ => "other".into(),
        });
    }
    assert_eq!(kinds, vec![format!("runs {A}")]);
    // 摘要那几格（状态 · 轮次边沿 · 文件重读 / 不在了）不挑：没在看的会话照发。
    sink.send(Frame::TurnEnd {
        session_id: B.into(),
        uuid: "u".into(),
    });
    sink.send(Frame::SessionFileGone {
        session_id: B.into(),
        path: "p".into(),
    });
    let mut n = 0;
    while rx.try_recv().is_ok() {
        n += 1;
    }
    assert_eq!(n, 2, "摘要帧不按在看的名单挑");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn starting_to_watch_answers_where_live_lines_begin_and_nothing_is_lost_or_doubled() {
    let (dir, mut state, mut sink, mut rx) = rig("from");
    let pb = file(&dir, B);
    // 先在看 A；B 在后台写了三行。
    assert!(
        watch_sessions(ids(&[A]), &mut state, &mut sink).is_empty(),
        "A 没有记录文件 ⇒ 无从答"
    );
    append(&pb, 3);
    process_jsonl(&pb, &mut state, &mut sink);
    assert!(lines(&mut rx).is_empty());
    // 转去看 B：回 B 从第 3 行起上流。
    let from = watch_sessions(ids(&[B]), &mut state, &mut sink);
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
    assert!(watch_sessions(ids(&[A, B]), &mut state, &mut sink).is_empty());
    assert!(watch_sessions(ids(&[A]), &mut state, &mut sink).is_empty());
    let from = watch_sessions(ids(&[A, B]), &mut state, &mut sink);
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
    watch_sessions(ids(&[A]), &mut state, &mut sink);
    process_jsonl(&pb, &mut state, &mut sink);
    state.runs.adopt(B, &pb);
    while rx.try_recv().is_ok() {}
    watch_sessions(ids(&[A, B]), &mut state, &mut sink);
    let mut kinds = Vec::new();
    while let Ok(f) = rx.try_recv() {
        if let Frame::SessionRuns { sid, .. } = f {
            kinds.push(sid);
        }
    }
    assert_eq!(kinds, vec![B.to_string()], "新看的会话补一帧运行表（整份）");
    std::fs::remove_dir_all(&dir).ok();
}

impl crate::guard_support::Shaped for crate::stream::wire::WatchReply {
    fn samples() -> Vec<Self> {
        vec![crate::stream::wire::WatchReply {
            from: vec![WatchFrom {
                sid: "s".into(),
                path: "/p/s.jsonl".into(),
                seq: 3,
            }],
        }]
    }
}
