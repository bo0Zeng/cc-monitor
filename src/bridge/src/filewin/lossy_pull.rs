//! 〔FILES2 · 第四波 · 2026-09-27〕**有损名（非 UTF-8）的下载** —— `设计/60 §7` 第 9 条 Q4 · `§6.2`「有损名全寻址」的下载那一格。
//!
//! 主会话按通行做法裁：「落到 Linux 本机 ⇒ 字节原样当文件名；落到 Windows 本机 ⇒ 名字按有损形替换并在结局里说一句
//! 『名字里有认不出的字节，已改成 X』（Windows 文件名是 UTF-16，没法原样）」。
//!
//! # 远端那一头怎么寻址（本路补的手段，记录 `FILES2.md` 第二节）
//!
//! SFTP 库（`russh-sftp`）的路径是 `String`（`设计/60 §4.1`）⇒ 非 UTF-8 的字节结构上发不出去。于是：
//! ① 让那台机器的后端就地拷一份到我们自己的暂存区，名字是 UTF-8 的 `<32hex>.part`（`files-copy`，根 `/`、源按字节）——
//!    同机拷贝，零网络；② 照常 `transfer-download` 那份暂存件；③ 不论成败都删掉它（`files-delete`）。
//! 窗口在 ② 与 ③ 之间崩了 ⇒ 暂存件的名字就是孤儿扫认的形（`files_commit::sweep_stale`，七天）。
//! 暂存区不在（这台还没传过东西）⇒ 先 `files-mkdir` 建、`files-chmod` 收成 0700（与后端自建那一层同权限）。

use crate::copy_table::copy_text;

/// 暂存区（相对远端 home）。⚠ 与后端 `control/files_commit.rs::STAGING_DIR` 是同一个值的两份（两个 crate 互相引不到），
/// 判据 `lossy_pull_tests::the_staging_dir_is_the_backend_one` 读两侧源码钉相等。
pub const STAGING_DIR: &str = ".cc-monitor/staging";

/// 暂存区自建时收成的权限位（与后端 `own_dir` 建自家目录同一个 0700）。
pub const STAGING_MODE: u32 = 0o700;

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
    local_dest_kept(p, shown, raw)
}

#[cfg(unix)]
fn local_dest_kept(
    p: &std::path::Path,
    _shown: &str,
    raw: &[u8],
) -> (serde_json::Value, Option<String>) {
    use std::os::unix::ffi::OsStrExt as _;
    let mut b = p
        .parent()
        .map(|d| d.as_os_str().as_bytes().to_vec())
        .unwrap_or_default();
    if b.last() != Some(&b'/') {
        b.push(b'/');
    }
    b.extend_from_slice(raw);
    (super::source::wire_bytes(&b), None)
}

#[cfg(not(unix))]
fn local_dest_kept(
    p: &std::path::Path,
    shown: &str,
    _raw: &[u8],
) -> (serde_json::Value, Option<String>) {
    (
        serde_json::Value::String(p.to_string_lossy().to_string()),
        Some(copy_text(
            "rsFilewinLossyPull.note.renamed",
            &[("name", shown)],
        )),
    )
}

/// 线上那一形 → 本机路径（给「那儿已经有东西了吗」那一问用；判定本身住 `download::dest_exists_at`）。
pub fn local_path_of(v: &serde_json::Value) -> Option<std::path::PathBuf> {
    let b = super::find::decode_path(v)?;
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        Some(std::path::PathBuf::from(std::ffi::OsStr::from_bytes(&b)))
    }
    #[cfg(not(unix))]
    {
        Some(std::path::PathBuf::from(
            String::from_utf8_lossy(&b).to_string(),
        ))
    }
}

/// 远端暂存件的整条路径（显示形 == 真字节：home 与键都是 UTF-8）。
pub fn staged_path(home: &str, key: &str) -> String {
    format!("{}/{STAGING_DIR}/{key}.part", home.trim_end_matches('/'))
}

/// 整趟：暂存区就位 → 就地拷进暂存区 → 下载那份暂存件 → 删暂存件（不论成败）。
pub async fn pull_by_bytes(
    line: &super::source::Line,
    origin: &super::source::Origin,
    remote: &[u8],
    local: serde_json::Value,
    board: &super::download::DownloadBoard,
) -> Result<(), String> {
    use super::source::ask;
    let home = super::source::home_from_reply(
        &ask(
            line,
            origin,
            "files-home",
            &serde_json::json!({}),
            super::transfer::PROBE_BUDGET,
        )
        .await?,
    )?;
    let key = uuid::Uuid::new_v4().simple().to_string();
    let own = format!("{}/.cc-monitor", home.trim_end_matches('/'));
    let staging = format!("{}/{STAGING_DIR}", home.trim_end_matches('/'));
    // 暂存区不在 ⇒ 建、收成 0700；已在 ⇒ 建那一下失败，不管它（拷的那一步会照实说）。
    let made = ask(
        line,
        origin,
        "files-mkdir",
        &serde_json::json!({ "root": own, "rel": "staging" }),
        super::writeops::WRITE_BUDGET,
    )
    .await
    .is_ok();
    if made {
        ask(
            line,
            origin,
            "files-chmod",
            &serde_json::json!({ "root": own, "rel": "staging", "mode": STAGING_MODE }),
            super::writeops::WRITE_BUDGET,
        )
        .await?;
    }
    let from: Vec<u8> = remote.strip_prefix(b"/").unwrap_or(remote).to_vec();
    let to = staged_path(&home, &key);
    ask(
        line,
        origin,
        "files-copy",
        &serde_json::json!({
            "root": "/",
            "from": super::source::wire_bytes(&from),
            "to": to.trim_start_matches('/'),
        }),
        super::copy::COPY_BUDGET,
    )
    .await
    .map_err(|e| copy_text("rsFilewinLossyPull.stage.failed", &[("why", &e)]))?;
    let pulled = super::download::pull_one_at(line, origin, &to, local, board).await;
    // 不论成败都删掉暂存件（删不掉交孤儿扫；不盖住下载本身的结局）。
    let _ = ask(
        line,
        origin,
        "files-delete",
        &serde_json::json!({ "root": staging, "rel": format!("{key}.part") }),
        super::writeops::WRITE_BUDGET,
    )
    .await;
    pulled
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/lossy_pull_tests.rs"]
mod tests;
