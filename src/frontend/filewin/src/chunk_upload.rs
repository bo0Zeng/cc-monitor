//! **上传的块形** —— 非标准 SFTP 起始目录那台机器上的上传。
//!
//! 按通行做法：「连上时比 SFTP `realpath(".")` 与那台后端的 `$HOME`；不一致 ⇒ 这台的上传改走后端链路分块写
//! （`files-stage-chunk` 那一族，已有），不走 SFTP 暂存；出声一次说原因」。
//!
//! - 比那一下住传输台（它连上 SFTP 才知道起始目录）：窗口开单时带上后端的 `$HOME`，不一致 ⇒ 传输台一个字节不写、
//!   以 `sftp_home_mismatch` 收场（`transfer::upload_remote` 据码换路，这一窗之后的上传都走这里）。
//! - 这里：本机那份逐块读（每块 [`CHUNK_RAW`] 字节，发 `{"b16": …}`，一行装得下）→ 顺序 `files-stage-chunk` → 边读边算整份
//!   SHA-256 → `files-commit-upload` 带 `chunks` / `bytes`（后端先拼成暂存件，再走与 SFTP 那条路**同一条**提交，摘要照核）。
//! - 进度照上传那一格报；按「取消传输」⇒ 下一块不发（已送的块交孤儿扫，同存盘分块）。

use copy_core::copy_text;
use sha2::Digest as _;
use std::io::Read as _;

/// 一块多少原始字节。b16 翻倍 ＋ 信封 ⇒ 一行远小于后端入方向那一行的上限（1 MiB，`inbound::MAX_LINE_BYTES`）。
pub const CHUNK_RAW: usize = 256 * 1024;

/// 一块的往返上限（与写面一趟同一个上界）。
pub const CHUNK_BUDGET: std::time::Duration = super::writeops::WRITE_BUDGET;

/// 字节 → 小写十六进制（`{"b16": …}` 那一形的内容）。
pub fn hex(b: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push(H[(x >> 4) as usize] as char);
        s.push(H[(x & 15) as usize] as char);
    }
    s
}

/// 一件上传走块形：逐块送 → 提交（带 `chunks` / `bytes` / 整份摘要）。失败带复制详情（后端拒了取那台写的；本机读不到窗口自己写）。
pub async fn upload_by_chunks(
    line: &super::source::Line,
    origin: &super::source::Origin,
    p: &super::transfer::Pending,
    board: &super::transfer::DropBoard,
) -> Result<(), super::source::Failed> {
    use super::source::{ask_coded as ask, Failed};
    let key = uuid::Uuid::new_v4().simple().to_string();
    // 本机那份读不到：句子只说原因词，系统原话进复制详情。
    let local_err = |e: std::io::Error| {
        Failed::here(
            copy_text(
                "beTransfer.local.readFailed",
                &[
                    ("path", &p.local_path),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            origin,
            "files-stage-chunk",
            Some(&e.to_string()),
        )
    };
    let mut f = std::fs::File::open(&p.local_path).map_err(local_err)?;
    let total = f.metadata().map_err(local_err)?.len();
    let mut sha = sha2::Sha256::new();
    let mut buf = vec![0u8; CHUNK_RAW];
    let (mut seq, mut got) = (0u64, 0u64);
    board.progress(&p.name, 0, total);
    loop {
        if board.cancels().is_cancelled() {
            return Err(Failed::from(super::transfer::CANCELLED.as_str()));
        }
        let n = f.read(&mut buf).map_err(local_err)?;
        if n == 0 {
            break;
        }
        sha.update(&buf[..n]);
        ask(
            line,
            origin,
            "files-stage-chunk",
            &serde_json::json!({ "key": key, "seq": seq, "content": { "b16": hex(&buf[..n]) } }),
            CHUNK_BUDGET,
        )
        .await?;
        seq += 1;
        got += n as u64;
        board.progress(&p.name, got, total);
    }
    let digest = hex(&sha.finalize());
    ask(
        line,
        origin,
        super::transfer::CMD_COMMIT,
        &serde_json::json!({
            "key": key,
            "root": p.root_wire(),
            "rel": super::source::remote_basename(&p.remote_path),
            "overwrite": p.overwrite,
            "expect": { "sha256": digest },
            "chunks": seq,
            "bytes": got,
        }),
        super::transfer::COMMIT_BUDGET,
    )
    .await
    .map(|_| ())
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/chunk_upload_tests.rs"]
mod tests;
