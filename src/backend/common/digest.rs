//! 内容摘要（SHA-256，64 位小写十六进制）：CAS 摘要形 `expect: {"sha256": …}` 的唯一算法住址。
//! 文件管理那一块（读交出去的摘要 · 写面比对 · 提交核暂存件）与原生那一块（传输台上传边传边算）都用它，
//! 两块之间零互相依赖（`files/module_boundary_guard.rs`），共用的只许住 `common/`。

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
