//! 文件管理线上类型：路径字节两形往返无损、认不得的形状拒。

use super::*;

fn round(b: &[u8]) -> (String, WirePath) {
    let s = serde_json::to_string(&WirePath(b.to_vec())).unwrap();
    let back: WirePath = serde_json::from_str(&s).unwrap();
    (s, back)
}

#[test]
fn utf8_goes_out_as_a_string_and_other_bytes_as_b16_and_both_come_back_unchanged() {
    let (s, back) = round("目录/a.txt".as_bytes());
    assert_eq!(s, "\"目录/a.txt\"");
    assert_eq!(back.0, "目录/a.txt".as_bytes());
    let (s, back) = round(b"odd-\xff.bin");
    assert_eq!(s, r#"{"b16":"6f64642dff2e62696e"}"#);
    assert_eq!(back.0, b"odd-\xff.bin");
    // 是 UTF-8 也可以用 b16 那一形送进来。
    let p: WirePath = serde_json::from_str(r#"{"b16":"6162"}"#).unwrap();
    assert_eq!(p.0, b"ab");
}

#[test]
fn shapes_that_are_not_a_path_are_refused() {
    for bad in [
        "1",
        "null",
        "[]",
        r#"{"b16":"zz"}"#,
        r#"{"b16":"abc"}"#,
        r#"{"b16":"AB"}"#,
        r#"{"hex":"ab"}"#,
        r#"{"b16":"ab","x":1}"#,
        "{}",
    ] {
        assert!(
            serde_json::from_str::<WirePath>(bad).is_err(),
            "`{bad}` 不是一个路径，却解出来了"
        );
    }
}

#[test]
fn a_chunk_always_goes_out_as_b16_even_when_it_is_utf8() {
    let s = serde_json::to_string(&HexBytes(b"hi".to_vec())).unwrap();
    assert_eq!(s, r#"{"b16":"6869"}"#);
    let back: HexBytes = serde_json::from_str(&s).unwrap();
    assert_eq!(back.0, b"hi");
    assert!(serde_json::from_str::<HexBytes>("\"hi\"").is_err());
}
