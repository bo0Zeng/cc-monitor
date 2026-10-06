//! **有损名（非 UTF-8）的下载** —— 「有损名全寻址」的下载那一格。
//!
//! 按通行做法：「落到 Linux 本机 ⇒ 字节原样当文件名；落到 Windows 本机 ⇒ 名字按有损形替换并在结局里说一句
//! 『名字里有认不出的字节，已改成 X』（Windows 文件名是 UTF-16，没法原样）」。
//!
//! # 远端那一头怎么寻址（用户 09-27）
//!
//! SFTP 库（`russh-sftp`）的路径是 `String`⇒ 非 UTF-8 的字节结构上发不出去 ⇒ **经那台后端链路按字节寻址分块读回**
//! （`files-read-chunk`，读族，远端只读 —— 「下载对远端只读」照旧）；本机那一头经本机后端落盘：
//! 逐块 `files-stage-chunk` 进本机暂存区 → `files-commit-upload`（带 `chunks` / `bytes` / 整份摘要，`rel` 收字节）。
//! monitor / 窗口进程对用户文件一个字节不写（`§3.8`）；不另造传输台。撤 ⇒ 下一块不读，已送的块交本机孤儿扫。
//! 不走「远端 `files-copy` 进暂存区 → SFTP 下 → 删暂存」：远端要只读。

use copy_core::copy_text;

/// 本机落点：`dest` 是框里那一串（有损显示形），`shown` / `raw` 是那一行的显示名与原始字节。
/// 回 `(线上那一形, 结局里要说的那一句)`。
/// - 用户改了名字（落点尾段 ≠ 显示名）⇒ 就用他敲的，什么都不说。
/// - Linux / unix：尾段没改 ⇒ 换成原始字节（`{"b16": …}`）—— 字节原样当文件名。
/// - Windows：尾段没改 ⇒ 有损形照用（`U+FFFD` 在 Windows 文件名里合法），结局里说一句改成了什么。
pub fn local_dest(dest: &str, shown: &str, raw: &[u8]) -> (serde_json::Value, Option<String>) {
    let p = std::path::Path::new(dest);
    let kept = p.file_name().and_then(|n| n.to_str()) == Some(shown);
    if !kept {
        return (serde_json::Value::String(dest.to_string()), None);
    }
    crate::platform::local_dest_kept(p, shown, raw)
}

// `local_dest_kept` 的两个平台臂（unix 按字节拼 · 别处用有损形并说一句）住 `platform.rs`。

/// 线上那一形 → 本机路径（那儿有没有同名由系统存盘框自己问，这里只换形）。
pub fn local_path_of(v: &serde_json::Value) -> Option<std::path::PathBuf> {
    let b = super::find::decode_path(v)?;
    Some(crate::platform::path_from_bytes(&b))
}

/// 一块读多少（== 后端 `files::READ_CHUNK_MAX_BYTES`，读两侧源码钉相等）。
pub const PULL_CHUNK: u64 = 256 * 1024;

/// 十六进制 → 字节（`{"b16": …}` 的内容；形状不对 ⇒ `None`）。
pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len() / 2)
        .map(|k| u8::from_str_radix(s.get(2 * k..2 * k + 2)?, 16).ok())
        .collect()
}

/// 本机落点（线上那一形）→ `(父目录那一形, 名字那一形)`：按最后一个 `/`（Windows 上还有 `\`）切，都保持字节。
pub fn split_local(local: &serde_json::Value) -> Option<(serde_json::Value, serde_json::Value)> {
    let b = super::find::decode_path(local)?;
    let cut = b
        .iter()
        .rposition(|c| *c == b'/' || (crate::platform::BACKSLASH_IS_SEP && *c == b'\\'))?;
    let (dir, name) = (&b[..cut.max(1)], &b[cut + 1..]);
    if name.is_empty() {
        return None;
    }
    Some((
        super::source::wire_bytes(dir),
        super::source::wire_bytes(name),
    ))
}

/// 整趟：远端逐块读回（按字节寻址）→ 本机逐块进暂存区 → 本机提交（摘要照核）。`overwrite` ＝ 人在「盖不盖」那一问里答了盖。
pub async fn pull_by_bytes(
    line: &super::source::Line,
    origin: &super::source::Origin,
    remote: &[u8],
    local: serde_json::Value,
    overwrite: bool,
    board: &super::download::DownloadBoard,
) -> Result<(), String> {
    use super::source::ask;
    use sha2::Digest as _;
    let (root, rel) =
        split_local(&local).ok_or_else(|| copy_text("rsFilewinLossyPull.local.badDest", &[]))?;
    let here = comms_inward::chan::wire::Origin(super::cross_copy::LOCAL_ORIGIN.to_string());
    let key = uuid::Uuid::new_v4().simple().to_string();
    let path = super::source::wire_bytes(remote);
    let mut sha = sha2::Sha256::new();
    let (mut off, mut seq) = (0u64, 0u64);
    loop {
        if board.cancels().is_cancelled() {
            return Err(super::transfer::CANCELLED.to_string());
        }
        let d = ask(
            line,
            origin,
            "files-read-chunk",
            &serde_json::json!({ "path": path, "offset": off, "len": PULL_CHUNK }),
            super::chunk_upload::CHUNK_BUDGET,
        )
        .await?;
        let body = d["content"]["b16"]
            .as_str()
            .and_then(unhex)
            .ok_or_else(|| copy_text("rsFilewinLossyPull.reply.badChunk", &[]))?;
        let size = d["size"].as_u64().unwrap_or(0);
        if !body.is_empty() {
            sha.update(&body);
            ask(
                line,
                &here,
                "files-stage-chunk",
                &serde_json::json!({ "key": key, "seq": seq, "content": { "b16": super::chunk_upload::hex(&body) } }),
                super::chunk_upload::CHUNK_BUDGET,
            )
            .await?;
            seq += 1;
            off += body.len() as u64;
        }
        board.progress(off, size);
        if d["eof"].as_bool() != Some(false) || body.is_empty() {
            break;
        }
    }
    let digest = super::chunk_upload::hex(&sha.finalize());
    ask(
        line,
        &here,
        super::transfer::CMD_COMMIT,
        &serde_json::json!({
            "key": key, "root": root, "rel": rel, "overwrite": overwrite,
            "expect": { "sha256": digest }, "chunks": seq, "bytes": off,
        }),
        super::transfer::COMMIT_BUDGET,
    )
    .await
    .map(|_| ())
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/lossy_pull_tests.rs"]
mod tests;
