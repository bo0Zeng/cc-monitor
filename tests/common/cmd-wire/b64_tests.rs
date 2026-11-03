use super::*;

/// RFC 4648 §10 的七条标准向量（异源是 RFC），两向。
#[test]
fn rfc_4648_vectors() {
    let vectors = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ];
    for (plain, enc) in vectors {
        assert_eq!(b64_encode(plain.as_bytes()), enc, "编 {plain:?}");
        assert_eq!(b64_decode(enc).as_deref(), Ok(plain.as_bytes()), "解 {enc:?}");
    }
}

/// 字母表最后两位（`+` `/`，§4 表 1 的 62 · 63）七条向量用不到：另钉一条；全 256 个字节值来回一趟。
#[test]
fn the_alphabet_tail_and_every_byte_round_trip() {
    assert_eq!(b64_encode(&[0xfb, 0xff]), "+/8=");
    assert_eq!(b64_decode("+/8="), Ok(vec![0xfb, 0xff]));
    let all: Vec<u8> = (0..=255u8).collect();
    assert_eq!(b64_decode(&b64_encode(&all)), Ok(all));
}

/// 坏形一律拒，且说得出是哪一种。
#[test]
fn malformed_text_says_which_way_it_is_bad() {
    assert_eq!(b64_decode("A"), Err(B64Error::Length(1)));
    assert_eq!(b64_decode("AA="), Err(B64Error::Length(3)));
    assert_eq!(b64_decode("A==="), Err(B64Error::Padding));
    assert_eq!(b64_decode("===="), Err(B64Error::Padding));
    assert_eq!(b64_decode("Zg==Zg=="), Err(B64Error::Padding));
    assert_eq!(b64_decode("Zm9v!A=="), Err(B64Error::Char('!')));
    assert_eq!(b64_decode("Zm9-"), Err(B64Error::Char('-')), "URL 安全那一版的字母不认");
}
