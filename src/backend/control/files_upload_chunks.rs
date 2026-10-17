//! 上传的块形：把 `files-stage-chunk` 送进暂存区的块拼成一份暂存件 `<key>.part`，交给原来那条提交。
//!
//! 给非标准 SFTP 起始目录（chroot / `internal-sftp -d`）的机器用：连上时 SFTP `realpath(".")` 与那台后端的 `$HOME` 不一致 ⇒
//! 这台的上传改走后端链路分块写。块的形状与存盘分块同一个（`files_commit::chunk_name`，`O_EXCL` 一块一份）；拼出来之后与 SFTP 那条路
//! 同一条提交（整份摘要核 · 改名上位 · 不覆盖时先占位）⇒ 落进用户目录的那一下只有一份。
//!
//! 流式拼（一块一块读、写进暂存件），不整份读进内存：上传件没有存盘那条 8 MiB 读上限。
//! 失败 ⇒ 删掉自己刚建的暂存件；不论成败都删掉这一键的全部块（与 `commit_text` 同一条）。

use super::files_commit::{chunk_name, drop_chunks, is_key, STAGING_DIR};
use super::files_write::{walk_in_root, Last, WriteRefusal};
use crate::platform::fs::{Face, Ro};
use copy_core::{copy_text, io_reason};
use std::path::Path;

/// 把 `<key>.0.chunk` … `<key>.<chunks-1>.chunk` 依次拼成 `<key>.part`（`O_EXCL` 新建），总长必须恰好 `bytes`、不多一块。
/// 这一键已经拼过（暂存件在、块都收了：上一次提交在上位那一步撞了同名）⇒ 不再拼，照那一份提交（整份摘要照核）。
pub fn assemble_part(
    face: &Face,
    home: &Path,
    key: &str,
    chunks: u64,
    bytes: u64,
) -> Result<(), WriteRefusal> {
    let dir = home.join(STAGING_DIR);
    let regular = |name: String| {
        is_key(key)
            && walk_in_root(Last::Link, &dir, name)
                .ok()
                .is_some_and(|p| Ro::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_file()))
    };
    if regular(format!("{key}.part")) && !regular(chunk_name(key, 0)) {
        return Ok(());
    }
    let r = assemble(face, home, key, chunks, bytes);
    drop_chunks(face, home, key);
    r
}

fn assemble(
    face: &Face,
    home: &Path,
    key: &str,
    chunks: u64,
    bytes: u64,
) -> Result<(), WriteRefusal> {
    if !is_key(key) {
        return Err(WriteRefusal::Refused(copy_text(
            "beUploadChunks.args.bad",
            &[("key", key), ("chunks", &chunks.to_string())],
        )));
    }
    let dir = home.join(STAGING_DIR);
    let part =
        walk_in_root(Last::Link, &dir, format!("{key}.part")).map_err(WriteRefusal::Refused)?;
    let io = |what: &Path, e: std::io::Error| {
        WriteRefusal::io(
            copy_text(
                "reason.fs.writeFailed",
                &[
                    ("path", &what.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    };
    let mut out = face.create_new(&part).map_err(|e| io(&part, e))?;
    let mut total = 0u64;
    let mut fail: Option<WriteRefusal> = None;
    for seq in 0..chunks {
        let at = match walk_in_root(Last::Link, &dir, chunk_name(key, seq)) {
            Ok(a) => a,
            Err(m) => {
                fail = Some(WriteRefusal::Refused(m));
                break;
            }
        };
        // 不跟链接地看一眼：块必须是一份普通文件（链接不是我们放的一块）。
        if !Ro::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_file()) {
            fail = Some(WriteRefusal::Io(copy_text(
                "beUploadChunks.chunk.missing",
                &[("seq", &seq.to_string()), ("chunks", &chunks.to_string())],
            )));
            break;
        }
        let copied = Ro::open_nofollow(&at).and_then(|mut f| std::io::copy(&mut f, &mut out));
        match copied {
            Ok(n) => total += n,
            Err(e) => {
                fail = Some(io(&at, e));
                break;
            }
        }
    }
    drop(out);
    let extra = walk_in_root(Last::Link, &dir, chunk_name(key, chunks))
        .is_ok_and(|p| Ro::symlink_metadata(p).is_ok());
    if fail.is_none() && (total != bytes || extra) {
        fail = Some(WriteRefusal::Io(copy_text(
            "beUploadChunks.total.mismatch",
            &[
                ("chunks", &chunks.to_string()),
                ("got", &total.to_string()),
                ("bytes", &bytes.to_string()),
            ],
        )));
    }
    match fail {
        None => Ok(()),
        Some(e) => {
            // 只删**我们自己刚建的那一份**（`O_EXCL` 保证它此前不存在）。
            if let Ok(p) = walk_in_root(Last::Link, &dir, format!("{key}.part")) {
                face.delete_file(&p).ok();
            }
            Err(e)
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/files_upload_chunks_tests.rs"]
mod tests;
