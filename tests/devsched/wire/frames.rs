//! 每种帧写一行再读回原样；坏行 · 未知帧 · 超长行报错不 panic。

use devsched_wire::{samples, write_frame, Frame, FrameReader, WireError};
use std::io::Cursor;

fn read_all(bytes: &[u8]) -> Vec<Result<Frame, WireError>> {
    let mut r = FrameReader::new(Cursor::new(bytes.to_vec()));
    let mut out = Vec::new();
    loop {
        match r.read_frame() {
            Ok(None) => break,
            Ok(Some(f)) => out.push(Ok(f)),
            Err(e) => out.push(Err(e)),
        }
    }
    out
}

#[test]
fn every_frame_round_trips() {
    let all = samples();
    // 每一种帧名都有样本
    for name in Frame::NAMES {
        assert!(all.iter().any(|f| f.name() == *name), "{name} 没有样本");
    }
    let mut buf = Vec::new();
    for f in &all {
        write_frame(&mut buf, f).unwrap();
    }
    // 一帧一行
    assert_eq!(buf.iter().filter(|b| **b == b'\n').count(), all.len());
    let back: Vec<Frame> = read_all(&buf).into_iter().map(|r| r.unwrap()).collect();
    assert_eq!(back, all);
}

#[test]
fn bad_and_unknown_lines_are_errors() {
    let input = b"not json\n{\"t\":\"warp\",\"x\":1}\n[1,2]\n{\"t\":\"stop\"}\n{\"t\":\"beat\"}\n";
    let got = read_all(input);
    assert_eq!(got.len(), 5);
    assert!(matches!(got[0], Err(WireError::Bad { .. })), "{:?}", got[0]);
    match &got[1] {
        Err(WireError::Unknown { tag }) => assert_eq!(tag, "warp"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(got[2], Err(WireError::Bad { .. })));
    // 认得的帧名、少了字段 ⇒ 坏行
    assert!(matches!(got[3], Err(WireError::Bad { .. })));
    // 坏行之后照常读得下去
    assert_eq!(got[4].as_ref().unwrap(), &Frame::Beat);
}

#[test]
fn too_long_line_is_skipped_with_error() {
    let mut input = vec![b'x'; devsched_wire::MAX_LINE + 10];
    input.push(b'\n');
    input.extend_from_slice(b"{\"t\":\"beat\"}\n");
    let got = read_all(&input);
    assert_eq!(got.len(), 2);
    assert!(matches!(got[0], Err(WireError::TooLong)));
    assert_eq!(got[1].as_ref().unwrap(), &Frame::Beat);
}

#[test]
fn eof_without_newline_still_reads_last_frame() {
    let got = read_all(b"{\"t\":\"beat\"}");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].as_ref().unwrap(), &Frame::Beat);
}
