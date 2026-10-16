//! FNV-1a：全仓唯一的一份。只求跨进程 · 跨版本稳定（摘要会存盘、会跨连接交回），不防篡改。

/// 64 位的起点。
pub const FNV64_BASIS: u64 = 0xcbf2_9ce4_8422_2325;

/// 32 位只是同一个式子取低 32 位：乘法与异或的低 32 位只由低 32 位决定。
fn fold(h: u64, prime: u64, bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(h, |h, &b| (h ^ u64::from(b)).wrapping_mul(prime))
}

/// 从 `h` 接着喂 `bytes`（分段喂 · 换起点）。
pub fn fnv1a64_from(h: u64, bytes: &[u8]) -> u64 {
    fold(h, 0x0100_0000_01b3, bytes)
}

/// FNV-1a 64。
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    fnv1a64_from(FNV64_BASIS, bytes)
}

/// FNV-1a 32。
pub fn fnv1a32(bytes: &[u8]) -> u32 {
    fold(0x811c_9dc5, 0x0100_0193, bytes) as u32
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/fnv_tests.rs"]
mod tests;
