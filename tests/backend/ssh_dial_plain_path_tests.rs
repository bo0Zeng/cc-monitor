//! 普通段那一道：三处调用方各自的形状都在这一张表里。

use super::{plain_segments, Dots, Fault};

fn segs(p: &str, bs: bool, dots: Dots) -> Result<Vec<String>, Fault> {
    plain_segments(p.as_bytes(), bs, dots)
        .map(|v| v.iter().map(|s| String::from_utf8_lossy(s).into_owned()).collect())
}

#[test]
fn plain_relative_paths_split_into_their_segments() {
    for dots in [Dots::Reject, Dots::Strip] {
        assert_eq!(segs("a/b/c.txt", false, dots).unwrap(), ["a", "b", "c.txt"]);
        assert_eq!(segs("a//b/", false, dots).unwrap(), ["a", "b"]);
        assert_eq!(segs("", false, dots).unwrap(), Vec::<String>::new());
        assert_eq!(segs("..a/b..", false, dots).unwrap(), ["..a", "b.."]);
    }
}

#[test]
fn parent_steps_and_absolute_forms_are_refused_either_way() {
    for dots in [Dots::Reject, Dots::Strip] {
        assert_eq!(segs("../x", false, dots), Err(Fault::Parent));
        assert_eq!(segs("a/../../x", false, dots), Err(Fault::Parent));
        assert_eq!(segs("/etc/passwd", false, dots), Err(Fault::Absolute));
        assert_eq!(segs("\\x", true, dots), Err(Fault::Absolute));
        assert_eq!(segs("C:\\x", true, dots), Err(Fault::Absolute));
        assert_eq!(segs("c:x", true, dots), Err(Fault::Absolute));
        assert_eq!(segs("a\\..\\x", true, dots), Err(Fault::Parent));
    }
}

#[test]
fn dots_are_refused_or_stripped_as_asked() {
    assert_eq!(segs("./a", false, Dots::Reject), Err(Fault::Dot));
    assert_eq!(segs("a/./b", false, Dots::Reject), Err(Fault::Dot));
    assert_eq!(segs("./a/./b", false, Dots::Strip).unwrap(), ["a", "b"]);
    assert_eq!(segs(".", false, Dots::Strip).unwrap(), Vec::<String>::new());
}

/// `\` 不是分隔符时它是名字的一部分，盘符那一形也只是名字（unix 上的 tar · 本机写面）。
#[test]
fn backslash_and_drive_are_plain_name_bytes_when_backslash_is_no_separator() {
    assert_eq!(segs("a\\..\\b", false, Dots::Reject).unwrap(), ["a\\..\\b"]);
    assert_eq!(segs("C:/x", false, Dots::Reject).unwrap(), ["C:", "x"]);
    assert_eq!(segs("a/C:", true, Dots::Reject).unwrap(), ["a", "C:"]);
}

#[test]
fn non_utf8_segments_pass_through_byte_for_byte() {
    let got = plain_segments(b"\xff\xfe/x", false, Dots::Reject).unwrap();
    assert_eq!(got, [&b"\xff\xfe"[..], &b"x"[..]]);
}
