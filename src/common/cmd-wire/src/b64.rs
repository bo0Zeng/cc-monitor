//! base64（RFC 4648 §4：标准字母表、带 `=` 补位）的唯一一份：后端 CLI 面入参（`--args-b64`）· 出网口的代理鉴权
//! （`Proxy-Authorization: Basic`）· 壳给 PowerShell 的 `-EncodedCommand` 都调它。
//!
//! 手写而不引包：两棵 workspace 里现成的 base64 只是传递进来的、还不是同一个包，直接引哪一个都要多签一条依赖；
//! 这一段无状态、有 RFC 4648 §10 的七条标准向量可对拍（判据拿 RFC 的向量核，异源是 RFC）。

/// 字母表。
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// 不在字母表里的字节在 [`DECODE`] 里的值。
const NOT_B64: u8 = 0xff;

/// 字节 ⇒ 它在 [`ALPHABET`] 里的位置（不在 ⇒ [`NOT_B64`]）。
const DECODE: [u8; 256] = {
    let mut t = [NOT_B64; 256];
    let mut i = 0;
    while i < 64 {
        t[ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    t
};

/// 解不开的那几种（写成给人看的那一句由用的那一侧做）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum B64Error {
    /// 长度不是 4 的倍数（带着那个长度）。
    Length(usize),
    /// 补位不在末尾，或一组里补了三个以上。
    Padding,
    /// 字母表外的字。
    Char(char),
}

/// 字节 ⇒ base64。
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
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// [`b64_encode`] 的逆。**严格**：长度不是 4 的倍数、字母表外的字、补位不在末尾 ⇒ `Err`（坏的就是坏的，不猜）。
pub fn b64_decode(text: &str) -> Result<Vec<u8>, B64Error> {
    let s = text.as_bytes();
    if !s.len().is_multiple_of(4) {
        return Err(B64Error::Length(s.len()));
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let quads = s.len() / 4;
    for (qi, q) in s.chunks(4).enumerate() {
        let pad = q.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && qi + 1 != quads) {
            return Err(B64Error::Padding);
        }
        let mut n: u32 = 0;
        for &c in &q[..4 - pad] {
            let v = DECODE[usize::from(c)];
            if v == NOT_B64 {
                return Err(B64Error::Char(char::from(c)));
            }
            n = (n << 6) | u32::from(v);
        }
        n <<= 6 * pad as u32;
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - pad]);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/b64_tests.rs"]
mod tests;
