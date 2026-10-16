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
    FrameRx,
) {
    let dir = std::env::temp_dir().join(format!("ccm-r2-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES);
    let mut state = ReaderState::new(dir.clone());
    state.active_sids.insert(SID.to_string());
    let path = dir.join(format!("{SID}.jsonl"));
    (dir, path, state, FrameSink::new(tx), rx)
}

/// 这一趟收到的行帧：`(seq, byte_offset)`。帧上不再带原文（带的是成品；这里的假行没有读者 ⇒ 不带成品），
/// 哪一行由行号与它的末端字节认。
fn lines(rx: &mut FrameRx) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    while let Ok(f) = rx.try_recv() {
        if let Frame::Line(crate::wire::Line {
            seq, byte_offset, ..
        }) = f
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
    retire_sid_if_unreferenced(SID, RemovalCause::Gone, None, &mut state, &mut sink);
    assert_eq!(lines(&mut rx), vec![(1, 15)]);
    std::fs::remove_dir_all(&dir).ok();

    // 乙：半条 JSON，写端死 ⇒ 零行。
    let (dir, path, mut state, mut sink, mut rx) = rig("a6-half");
    std::fs::write(&path, b"{\"n\":0}\n{\"n\":1,\"tor").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    let _ = lines(&mut rx);
    retire_sid_if_unreferenced(SID, RemovalCause::Gone, None, &mut state, &mut sink);
    assert_eq!(lines(&mut rx), Vec::<(u64, u64)>::new(), "半行永不误发");
    std::fs::remove_dir_all(&dir).ok();

    // 丙：死前最后一行的文件事件还没到（pidfd 先醒）⇒ 退休时补读到它，号接着走。
    let (dir, path, mut state, mut sink, mut rx) = rig("a6-late");
    std::fs::write(&path, b"{\"n\":0}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    let _ = lines(&mut rx);
    std::fs::write(&path, b"{\"n\":0}\n{\"n\":1}\n").unwrap();
    retire_sid_if_unreferenced(SID, RemovalCause::Gone, None, &mut state, &mut sink);
    assert_eq!(lines(&mut rx), vec![(1, 16)]);
    std::fs::remove_dir_all(&dir).ok();
}

/// 这一趟的帧摊平：重读出声给 `reread`，行帧给 `seq`。
fn frames(rx: &mut FrameRx) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(f) = rx.try_recv() {
        match f {
            Frame::Line(crate::wire::Line { seq, .. }) => out.push(format!("{seq}")),
            Frame::SessionFileReread(crate::wire::SessionFileReread { lines, .. }) => {
                out.push(format!("reread:{lines}"))
            }
            _ => {}
        }
    }
    out
}

/// 「要后端给『seq 跨截断稳定』的保证 …… seq ＝ 当前文件里的行号，截断即换代」。截短 / 原地改写 / 删了又长回来三种从 0 重读
/// ⇒ 照读照数、一帧行都不出，只出声一帧带重读完的行数（客户端照宣告那样从尾段取）；之后新长的行号接着数。
#[test]
fn a_reread_restarts_the_line_numbers_after_saying_so() {
    let (dir, path, mut state, mut sink, mut rx) = rig("seq-gen");
    std::fs::write(&path, b"{\"n\":0,\"uuid\":\"aaaaaaaa-0000\"}\n{\"n\":1,\"uuid\":\"aaaaaaaa-0001\"}\n{\"n\":2,\"uuid\":\"aaaaaaaa-0002\"}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(frames(&mut rx), vec!["0", "1", "2"]);
    // 截短
    std::fs::write(&path, b"{\"n\":9}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(frames(&mut rx), vec!["reread:1"], "截短");
    // 原地改写且变长（末尾指纹对不上）
    std::fs::write(
        &path,
        b"{\"n\":9,\"edited-and-longer\":true,\"uuid\":\"bbbbbbbb-0000\"}\n{\"n\":10}\n",
    )
    .unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(frames(&mut rx), vec!["reread:2"], "改写");
    // 删了又长回来
    std::fs::remove_file(&path).unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    std::fs::write(&path, b"{\"n\":20}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(frames(&mut rx), vec!["reread:1"], "删了又长回来");
    // 阴性：纯追加 ⇒ 不出声、号接着走。
    std::fs::write(&path, b"{\"n\":20}\n{\"n\":21}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(frames(&mut rx), vec!["1"], "纯追加");
    std::fs::remove_dir_all(&dir).ok();
}

/// d：从 0 重读按块 —— 一份记录被改写成 8 MiB，那一趟读的时候本线程手里最多两块多一点（今天整份读进一个 `Vec`）；
/// 出声在前、行号从 0 数、一行不少。
#[test]
fn a_reread_from_zero_holds_about_a_block_not_the_file() {
    use crate::common::append_read::test_support::{big_record, BIG, HELD_BOUND};
    let (dir, path, mut state, mut sink, mut rx) = rig("held");
    std::fs::write(&path, b"{\"n\":0,\"uuid\":\"aaaaaaaa-0000\"}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(frames(&mut rx), vec!["0"]);
    let users = big_record(&dir.join("big.jsonl"), BIG, true);
    std::fs::rename(dir.join("big.jsonl"), &path).unwrap();
    let base = crate::alloc_probe::reset_peak();
    let n = process_jsonl(&path, &mut state, &mut sink);
    let held = crate::alloc_probe::peak_since(base);
    let got = frames(&mut rx);
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(
        (n, got),
        (0, vec![format!("reread:{users}")]),
        "改写之后从 0 重读：照读照数、一行都不上流，出声带行数"
    );
    assert!(
        held <= HELD_BOUND,
        "从 0 重读 {BIG} 字节，本线程手里最多 {held} 字节，上界 {HELD_BOUND}"
    );
}

/// d：同一份记录从同一个游标起按 1 · 7 · 64 字节一块扫，出的行（号 · 原文 · 起止字节）、新游标、末尾指纹
/// 与整截读进来一次扫（[`read_new_lines_at`]，改前的读法）逐个相等；从 0 · 从中间一行 · 截短 · 改写四种起法都比。
#[test]
fn scanning_in_blocks_matches_one_whole_scan() {
    let dir = std::env::temp_dir().join(format!("ccm-r2-blocks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rec.jsonl");
    let body = "{\"n\":0,\"t\":\"中文跨块\"}\r\n\u{feff}{\"n\":1}\n   \n\n{\"n\":2,\"uuid\":\"aaaaaaaa-0002\",\"pad\":\"0123456789012345678901234567890123456789012345678901234567890123456789\"}\n{\"n\":3,\"t\":\"残";
    std::fs::write(&path, body).unwrap();
    let bytes = body.as_bytes();
    let line_end = bytes.iter().position(|&b| b == b'\n').unwrap() as u64 + 1;
    let tail_at = |c: u64| {
        let k = TAIL_PROBE.min(c);
        bytes[(c - k) as usize..c as usize].to_vec()
    };
    // （起读游标, 指纹）→ 改前那一形的答案：整截从 0 读进来一次扫（改写 / 截短从 0、行号从 0；续读从游标、号接着）。
    let starts: Vec<(&str, ReadCursor, Option<Vec<u8>>)> = vec![
        ("从 0", ReadCursor::default(), None),
        (
            "从中间一行",
            ReadCursor {
                consumed: line_end,
                seen_len: line_end,
            },
            Some(tail_at(line_end)),
        ),
        (
            "截短",
            ReadCursor {
                consumed: 1 << 20,
                seen_len: 1 << 20,
            },
            None,
        ),
        (
            "改写",
            ReadCursor {
                consumed: line_end,
                seen_len: line_end,
            },
            Some(vec![b'#'; TAIL_PROBE.min(line_end) as usize]),
        ),
    ];
    let shape = |l: &ReadLine| (l.seq, l.raw.clone(), l.start, l.byte_offset);
    for (name, cursor, tail) in &starts {
        let want = {
            let from = if *name == "改写" {
                ReadCursor::default()
            } else {
                *cursor
            };
            let mut seqs = SeqCounter::new();
            if *name == "从中间一行" {
                seqs.set("k", 1);
            }
            let (lines, next) =
                read_new_lines_at(bytes, 0, bytes.len() as u64, from, "k", &mut seqs);
            (
                lines.iter().map(shape).collect::<Vec<_>>(),
                next,
                tail_at(next.consumed),
            )
        };
        for block in [1usize, 7, 64, 1 << 20] {
            let Look::Read(mut new) = read_tail_in_blocks(&path, *cursor, tail.as_deref(), block)
            else {
                panic!("[{name} · 块 {block}] 该读得到");
            };
            let mut seqs = SeqCounter::new();
            if *name == "从中间一行" {
                seqs.set("k", 1);
            }
            let mut got = Vec::new();
            while let Some(lines) = new.next("k", &mut seqs) {
                got.extend(lines.iter().map(shape));
            }
            let (next, t) = new.finish();
            assert_eq!(
                (got, next, t),
                want,
                "[{name} · 块 {block}] 与整截一次扫的不一样"
            );
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}
