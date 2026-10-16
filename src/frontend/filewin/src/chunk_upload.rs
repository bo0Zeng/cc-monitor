//! **上传的块形** —— 非标准 SFTP 起始目录那台机器上的上传。
//!
//! 按通行做法：「连上时比 SFTP `realpath(".")` 与那台后端的 `$HOME`；不一致 ⇒ 这台的上传改走后端链路分块写
//! （`files-stage-chunk` 那一族，已有），不走 SFTP 暂存；出声一次说原因」。
//!
//! - 比那一下住传输台（它连上 SFTP 才知道起始目录）：窗口开单时带上后端的 `$HOME`，不一致 ⇒ 传输台一个字节不写、
//!   以 `sftp_home_mismatch` 收场（`transfer::upload_remote` 据码换路，这一窗之后的上传都走这里）。
//! - 这里：逐块送 ＋ 整份摘要那一圈是两端共用的 `chunk_core::stage`（手机同一份），送完交 `files-commit-upload` 带 `chunks` / `bytes`
//!   （后端先拼成暂存件，再走与 SFTP 那条路**同一条**提交，摘要照核）。
//! - 进度照上传那一格报；按「取消传输」⇒ 下一块不发（已送的块交孤儿扫，同存盘分块）。

use copy_core::copy_text;

/// 一块的往返上限（与写面一趟同一个上界）。
pub const CHUNK_BUDGET: std::time::Duration = super::writeops::WRITE_BUDGET;

/// 经通道问那台（[`super::source::ask_typed`]，每问 [`CHUNK_BUDGET`]）：分块那一圈（`chunk_core`）问那台的那一口。
pub struct Over<'a> {
    pub line: &'a super::source::Line,
    pub origin: &'a super::source::Origin,
}

impl chunk_core::Ask for Over<'_> {
    type Fail = super::source::Failed;
    async fn ask<C: cmd_wire::files::Call + Sync>(&self, req: &C) -> Result<C::Reply, Self::Fail> {
        super::source::ask_typed(self.line, self.origin, req, CHUNK_BUDGET).await
    }
}

/// 上传那一格的撤与进度。
struct Board<'a> {
    board: &'a super::transfer::DropBoard,
    name: &'a str,
}

impl chunk_core::Watch for Board<'_> {
    fn stopped(&self) -> bool {
        self.board.cancels().is_cancelled()
    }
    fn moved(&mut self, done: u64, total: Option<u64>) {
        self.board.progress(self.name, done, total.unwrap_or(0));
    }
}

/// 一件上传走块形：逐块送 → 提交（带 `chunks` / `bytes` / 整份摘要）。失败带复制详情（后端拒了取那台写的；本机读不到窗口自己写）。
pub async fn upload_by_chunks(
    line: &super::source::Line,
    origin: &super::source::Origin,
    p: &super::transfer::Pending,
    board: &super::transfer::DropBoard,
) -> Result<(), super::source::Failed> {
    use super::source::{ask_typed, Failed};
    let key = chunk_core::key_of(*uuid::Uuid::new_v4().as_bytes());
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
            <cmd_wire::files::FilesStageChunkReq as cmd_wire::files::Call>::NAME,
            Some(&e.to_string()),
        )
    };
    let f = std::fs::File::open(&p.local_path).map_err(local_err)?;
    let total = f.metadata().map_err(local_err)?.len();
    let over = Over { line, origin };
    let s = chunk_core::stage(
        &over,
        key,
        f,
        Some(total),
        &mut Board {
            board,
            name: &p.name,
        },
        || Failed::from(super::transfer::CANCELLED.as_str()),
        local_err,
    )
    .await?;
    let req = p.commit_req(&s.key, s.sha256, Some((s.chunks, s.bytes)));
    ask_typed(line, origin, &req, super::transfer::COMMIT_BUDGET)
        .await
        .map(|_| ())
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/chunk_upload_tests.rs"]
mod tests;
