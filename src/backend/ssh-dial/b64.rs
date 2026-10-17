//! base64（RFC 4648 §4，标准字母表、带 `=` 补位）：链路字节进 JSON 串的那一种编码。
//! 后端的帧面经 `wire/mod.rs` 转一手用它。

/// base64 的字母表（RFC 4648 §4，标准字母表、带 `=` 补位）。
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// 不在字母表里的字节在 [`B64_DECODE`] 里的值。
const NOT_B64: u8 = 0xff;

/// 字节 ⇒ 它在 [`B64`] 里的位置（不在 ⇒ [`NOT_B64`]）：解码每个字符查一次表。
const B64_DECODE: [u8; 256] = {
    let mut t = [NOT_B64; 256];
    let mut i = 0;
    while i < 64 {
        t[B64[i] as usize] = i as u8;
        i += 1;
    }
    t
};

/// 链路字节进 JSON 串的编码（`link_data` 帧 / `link-data` 命令的 `data`）。
///
/// ⚠ **为什么手写而不加一条依赖**：本 crate 每加一条依赖都要过 `readonly_guard` 的依赖签字，
/// 而这一段是 20 行、无状态、有 RFC 4648 §10 的七条标准向量可对拍
/// （`wire_tests::b64_matches_the_rfc_4648_test_vectors` 与 monitor 侧那一份的判据
/// **各拿同一组 RFC 向量**核自己 —— 异源是 RFC，不是对面的实现）。
pub fn b64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(B64[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// [`b64_encode`] 的逆。**严格**：长度不是 4 的倍数、字母表外的字符、补位不在末尾 ⇒ `Err`
/// （坏的就是坏的，不猜）。
pub fn b64_decode(text: &str) -> Result<Vec<u8>, String> {
    let s = text.as_bytes();
    if !s.len().is_multiple_of(4) {
        return Err(crate::malformed(&format!(
            "base64 length {s_count} is not a multiple of 4",
            s_count = s.len()
        )));
    }
    let val = |c: u8| -> Option<u32> {
        (B64_DECODE[usize::from(c)] != NOT_B64).then(|| u32::from(B64_DECODE[usize::from(c)]))
    };
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let quads = s.len() / 4;
    for (qi, q) in s.chunks(4).enumerate() {
        let pad = q.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && qi + 1 != quads) {
            return Err(crate::malformed("base64 padding is not at the end"));
        }
        let mut n: u32 = 0;
        for &c in &q[..4 - pad] {
            let v = val(c).ok_or_else(|| {
                crate::malformed(&format!(
                    "base64 has a character outside the alphabet: {char:?}",
                    char = char::from(c)
                ))
            })?;
            n = (n << 6) | v;
        }
        n <<= 6 * pad as u32;
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - pad]);
    }
    Ok(out)
}
