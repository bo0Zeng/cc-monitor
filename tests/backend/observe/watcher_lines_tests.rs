//! 后端 jsonl 增量读（watcher D 块）的判据。
//!
//! 守的要求：「写端写完整 JSON 没写换行就被 kill ⇒ 该行 live 永不投递」。
//! 语料是结构性的假行（`{"n":…}`），不含任何真会话正文。

use super::*;

const SID: &str = "s-r2";

fn rig(
    tag: &str,
) -> (
    PathBuf,
    PathBuf,
    ReaderState,
    FrameSink,
    tokio::sync::mpsc::Receiver<Frame>,
) {
    let dir = std::env::temp_dir().join(format!("ccm-r2-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel::<Frame>(256);
    let mut state = ReaderState::new(dir.join("projects"), false, false);
    state.active_sids.insert(SID.to_string());
    let path = dir.join(format!("{SID}.jsonl"));
    (dir, path, state, FrameSink::new(tx), rx)
}

/// 这一趟收到的行帧：`(seq, byte_offset)`。帧上不再带原文（带的是成品；这里的假行没有读者 ⇒ 不带成品），
/// 哪一行由行号与它的末端字节认。
fn lines(rx: &mut tokio::sync::mpsc::Receiver<Frame>) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    while let Ok(f) = rx.try_recv() {
        if let Frame::Line {
            seq, byte_offset, ..
        } = f
        {
            out.push((seq, byte_offset));
        }
    }
    out
}

/// A6：写端死时游标之后那截是整个 JSON 对象 ⇒ 恰好一行、号 = 下一个行号；半行 / 活着 ⇒ 零行。
/// 经生产出口 `retire_sid_if_unreferenced`（会话退休只有这一个出口）。
#[test]
fn a_complete_final_line_without_newline_is_handed_out_once_the_writer_is_dead() {
    // 甲：整条 JSON 没 `\n`，写端死 ⇒ 恰好一行、seq 1。
    let (dir, path, mut state, mut sink, mut rx) = rig("a6-whole");
    std::fs::write(&path, b"{\"n\":0}\n{\"n\":1}").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(lines(&mut rx), vec![(0, 8)], "活着时残行不许发");
    retire_sid_if_unreferenced(SID, RemovalCause::Gone, &mut state, &mut sink);
    assert_eq!(lines(&mut rx), vec![(1, 15)]);
    std::fs::remove_dir_all(&dir).ok();

    // 乙：半条 JSON，写端死 ⇒ 零行。
    let (dir, path, mut state, mut sink, mut rx) = rig("a6-half");
    std::fs::write(&path, b"{\"n\":0}\n{\"n\":1,\"tor").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    let _ = lines(&mut rx);
    retire_sid_if_unreferenced(SID, RemovalCause::Gone, &mut state, &mut sink);
    assert_eq!(lines(&mut rx), Vec::<(u64, u64)>::new(), "半行永不误发");
    std::fs::remove_dir_all(&dir).ok();

    // 丙：死前最后一行的文件事件还没到（pidfd 先醒）⇒ 退休时补读到它，号接着走。
    let (dir, path, mut state, mut sink, mut rx) = rig("a6-late");
    std::fs::write(&path, b"{\"n\":0}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    let _ = lines(&mut rx);
    std::fs::write(&path, b"{\"n\":0}\n{\"n\":1}\n").unwrap();
    retire_sid_if_unreferenced(SID, RemovalCause::Gone, &mut state, &mut sink);
    assert_eq!(lines(&mut rx), vec![(1, 16)]);
    std::fs::remove_dir_all(&dir).ok();
}

/// 这一趟的帧摊平：重读出声给 `reread`，行帧给 `seq`。
fn frames(rx: &mut tokio::sync::mpsc::Receiver<Frame>) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(f) = rx.try_recv() {
        match f {
            Frame::Line { seq, .. } => out.push(format!("{seq}")),
            Frame::SessionFileReread { .. } => out.push("reread".to_string()),
            _ => {}
        }
    }
    out
}

/// 「要后端给『seq 跨截断稳定』的保证 …… seq ＝ 当前文件里的行号，截断即换代，
/// 先发一帧『这份文件重写了』再从 0 重投」。截短 / 原地改写 / 删了又长回来三种从 0 重读 ⇒ 出声在前、行号从 0 数。
#[test]
fn a_reread_restarts_the_line_numbers_after_saying_so() {
    let (dir, path, mut state, mut sink, mut rx) = rig("seq-gen");
    std::fs::write(&path, b"{\"n\":0,\"uuid\":\"aaaaaaaa-0000\"}\n{\"n\":1,\"uuid\":\"aaaaaaaa-0001\"}\n{\"n\":2,\"uuid\":\"aaaaaaaa-0002\"}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(frames(&mut rx), vec!["0", "1", "2"]);
    // 截短
    std::fs::write(&path, b"{\"n\":9}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(frames(&mut rx), vec!["reread", "0"], "截短");
    // 原地改写且变长（末尾指纹对不上）
    std::fs::write(
        &path,
        b"{\"n\":9,\"edited-and-longer\":true,\"uuid\":\"bbbbbbbb-0000\"}\n{\"n\":10}\n",
    )
    .unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(frames(&mut rx), vec!["reread", "0", "1"], "改写");
    // 删了又长回来
    std::fs::remove_file(&path).unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    std::fs::write(&path, b"{\"n\":20}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(frames(&mut rx), vec!["reread", "0"], "删了又长回来");
    // 阴性：纯追加 ⇒ 不出声、号接着走。
    std::fs::write(&path, b"{\"n\":20}\n{\"n\":21}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(frames(&mut rx), vec!["1"], "纯追加");
    std::fs::remove_dir_all(&dir).ok();
}
