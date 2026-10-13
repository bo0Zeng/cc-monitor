//! `common/digest.rs` 的判据：流式摘要与公开的 SHA-256 向量逐字相同，分块喂与整份喂同一个答案。

use super::*;

/// FIPS 180-2 附录那两条向量（空串 · `abc`）：64 位小写十六进制逐字。
#[test]
fn the_streaming_digest_matches_the_published_vectors() {
    let hex = |b: &[u8]| {
        let mut d = ContentDigest::new();
        d.update(b);
        d.finish()
    };
    assert_eq!(
        hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(hex(b"abc").len(), SHA256_HEX_LEN);
}

/// 一边传一边算（传输台）与提交那一侧逐块读：块怎么切都不改答案。
#[test]
fn feeding_in_pieces_gives_the_same_digest_as_feeding_at_once() {
    let body: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
    let mut whole = ContentDigest::default();
    whole.update(&body);
    let mut pieces = ContentDigest::new();
    for chunk in body.chunks(37) {
        pieces.update(chunk);
    }
    assert_eq!(pieces.finish(), whole.finish());
}
