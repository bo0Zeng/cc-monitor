//! **分块传的循环**：桌面文件窗口与手机嵌的 Rust 核心同一份（两端各写一份 ⇒ 块多大 · 摘要怎么算 · 一块失败之后怎样各走各的）。
//!
//! - 上传 [`stage`]：本机那份逐块读满（每块 [`CHUNK`] 字节）→ 依次 `files-stage-chunk` 进那台的暂存区（同一个键）→ 边送边算整份
//!   SHA-256 → 交回 [`Staged`]（块数 · 字节数 · 摘要），提交（`files-commit-upload`，同名照 `on_exists`）由调用方交。
//! - 下载 [`pull`]：`files-read-chunk` 从 0 起逐块读回（按字节寻址），每块交调用方的 [`Sink`]，读到头为止；交回总字节。
//! - 问那台的那一下是 [`Ask`]（桌面经通道、手机经常驻流）；撤 · 进度 · 本机读不动怎么说都由调用方给，这里不写字。

use cmd_wire::files::{Call, ChunkBytes, FilesStageChunkReq, ReadChunkReq, WirePath};
use sha2::Digest as _;
use std::future::Future;

/// 一块多少原始字节（== 后端 `files::READ_CHUNK_MAX_BYTES`；b16 翻倍 ＋ 信封仍远小于那台入方向一行的上限）。
pub const CHUNK: usize = 256 * 1024;

/// 问那台一条文件命令：成了 ⇒ 那条命令的回包；没成 ⇒ 调用方那一形的失败。
pub trait Ask {
    type Fail;
    fn ask<C: Call + Sync>(&self, req: &C) -> impl Future<Output = Result<C::Reply, Self::Fail>>;
}

/// 读回来的一块交给谁（写本机文件 · 再进本机的暂存区 …）。
pub trait Sink {
    type Fail;
    fn put(&mut self, bytes: Vec<u8>) -> impl Future<Output = Result<(), Self::Fail>>;
}

/// 撤与进度那两口（每一块之前问一次撤；每送 / 收完一块报一次进度）。
pub trait Watch {
    /// 撤了没有。
    fn stopped(&self) -> bool;
    /// 已经过去的字节 · 总数（不知道 ⇒ `None`）。
    fn moved(&mut self, done: u64, total: Option<u64>);
}

/// 送完了的那一份：提交时照交（`chunks` · `bytes` · `expect.sha256`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staged {
    pub key: String,
    pub chunks: u64,
    pub bytes: u64,
    /// 整份 SHA-256，小写十六进制。
    pub sha256: String,
}

/// 一块一块往那台的暂存区里送（同一个键、块号从 0 连着来），边送边算整份摘要。
pub struct Stager {
    key: String,
    sha: sha2::Sha256,
    chunks: u64,
    bytes: u64,
}

impl Stager {
    pub fn new(key: String) -> Self {
        Stager {
            key,
            sha: sha2::Sha256::new(),
            chunks: 0,
            bytes: 0,
        }
    }

    /// 送一块（空块不送）。
    pub async fn put<A: Ask>(&mut self, ask: &A, bytes: Vec<u8>) -> Result<(), A::Fail> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.sha.update(&bytes);
        let n = bytes.len() as u64;
        let req = FilesStageChunkReq {
            key: self.key.clone(),
            seq: self.chunks,
            content: ChunkBytes::Hex(bytes),
        };
        ask.ask(&req).await?;
        self.chunks += 1;
        self.bytes += n;
        Ok(())
    }

    /// 已送的字节。
    pub fn sent(&self) -> u64 {
        self.bytes
    }

    pub fn finish(self) -> Staged {
        Staged {
            key: self.key,
            chunks: self.chunks,
            bytes: self.bytes,
            sha256: hex(&self.sha.finalize()),
        }
    }
}

/// 一份暂存件的键（那台只收 32 位小写十六进制）：由调用方现造的 16 个随机字节写成。
pub fn key_of(random: [u8; 16]) -> String {
    hex(&random)
}

/// 上传那一半：`src` 逐块读满送进 `key` 那一份暂存件。撤了 ⇒ `stop()`（已送的块交那台的孤儿扫）；本机读不动 ⇒ `local(e)`。
pub async fn stage<A: Ask, R: std::io::Read>(
    ask: &A,
    key: String,
    mut src: R,
    total: Option<u64>,
    watch: &mut impl Watch,
    stop: impl Fn() -> A::Fail,
    local: impl Fn(std::io::Error) -> A::Fail,
) -> Result<Staged, A::Fail> {
    let mut stager = Stager::new(key);
    watch.moved(0, total);
    loop {
        if watch.stopped() {
            return Err(stop());
        }
        let block = fill(&mut src).map_err(&local)?;
        if block.is_empty() {
            break;
        }
        let last = block.len() < CHUNK;
        stager.put(ask, block).await?;
        watch.moved(stager.sent(), total);
        if last {
            break;
        }
    }
    Ok(stager.finish())
}

/// 下载那一半：`path` 从 0 起逐块读回、每块交 `sink`，读到头为止；回总字节。撤了 ⇒ `stop()`。
pub async fn pull<A: Ask, S: Sink<Fail = A::Fail>>(
    ask: &A,
    path: WirePath,
    sink: &mut S,
    watch: &mut impl Watch,
    stop: impl Fn() -> A::Fail,
) -> Result<u64, A::Fail> {
    let mut off = 0u64;
    loop {
        if watch.stopped() {
            return Err(stop());
        }
        let d = ask
            .ask(&ReadChunkReq {
                path: path.clone(),
                offset: off,
                len: CHUNK as u64,
            })
            .await?;
        let body = d.content.0;
        let empty = body.is_empty();
        off += body.len() as u64;
        if !empty {
            sink.put(body).await?;
        }
        watch.moved(off, Some(d.size));
        if d.eof || empty {
            return Ok(off);
        }
    }
}

/// 读满一块（或读到头）。
fn fill<R: std::io::Read>(src: &mut R) -> std::io::Result<Vec<u8>> {
    let mut buf = vec![0u8; CHUNK];
    let mut n = 0;
    while n < CHUNK {
        match src.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    buf.truncate(n);
    Ok(buf)
}

/// 字节 → 小写十六进制。
pub fn hex(b: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push(H[(x >> 4) as usize] as char);
        s.push(H[(x & 15) as usize] as char);
    }
    s
}

#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

#[cfg(test)]
#[path = "../../../../tests/common/chunk-core/chunk_core_tests.rs"]
mod tests;
