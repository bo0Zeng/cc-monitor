//! 内容摘要（SHA-256，64 位小写十六进制）：CAS 摘要形 `expect: {"sha256": …}` 的唯一算法住址。
//! 文件管理那一块（读交出去的摘要 · 写面比对 · 提交核暂存件）与原生那一块（传输台上传边传边算）都用它，
//! 两块之间零互相依赖（`files/module_boundary_guard.rs`），共用的只许住 `common/`。
//! 另一形是不防篡改、只答相同 / 不同的 FNV-1a 64（[`Fnv`] · [`fnv_digest`]）：资产目录 · skill 装记录 · cc-bus 装件 · 各号 MCP 登录的指纹都用它（原住 `assets/`）。

/// CAS 摘要形里十六进制串的长度（SHA-256 = 32 字节）。
pub const SHA256_HEX_LEN: usize = 64;

/// SHA-256 的**流式**那一形（`files::content_sha256` 同一个算法、同一种十六进制）：上传一边传一边算整份的摘要
/// （`control/transfer.rs::upload_to_staging`），提交那一侧对暂存件逐块读着算（`files::file_sha256`）—— 几个 G 的文件不整份进内存。
pub struct ContentDigest(ring::digest::Context);

impl ContentDigest {
    pub fn new() -> Self {
        ContentDigest(ring::digest::Context::new(&ring::digest::SHA256))
    }
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
    /// 64 位小写十六进制。
    pub fn finish(self) -> String {
        use std::fmt::Write as _;
        let d = self.0.finish();
        let mut out = String::with_capacity(SHA256_HEX_LEN);
        for b in d.as_ref() {
            let _ = write!(out, "{b:02x}");
        }
        out
    }
}

impl Default for ContentDigest {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/common/digest_tests.rs"]
mod tests;

/// FNV-1a 64。每段先喂长度再喂字节 ⇒ 段与段之间不会粘成同一串。
#[derive(Clone, Copy)]
pub struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    fn byte(&mut self, b: u8) {
        self.0 ^= u64::from(b);
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }
    /// 喂一段（带长度前缀）。
    pub fn part(&mut self, bytes: &[u8]) -> &mut Self {
        for b in (bytes.len() as u64).to_le_bytes() {
            self.byte(b);
        }
        for &b in bytes {
            self.byte(b);
        }
        self
    }
    pub fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

/// 一段原文的 FNV 摘要（16 位十六进制）：资产装时记、卸时比 · 各号 MCP 登录那一条的指纹，都是它（原 `assets::skill_ledger::digest_of`）。**纯**。
pub fn fnv_digest(text: &str) -> String {
    let mut f = Fnv::default();
    f.part(text.as_bytes());
    f.hex()
}
