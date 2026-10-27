//! 〔一次性迁移 · 下一版整份删〕书签搬到各台后端（`bookmarks-read` / `-edit`，本体 `control/bookmarks.rs`）之后，
//! 桌面旧的那份 `<monitor 数据目录>/filewin-bookmarks.json`（`{ "<机器名>": ["/目录", …] }`）迁一次就删：
//!
//! - 连上一台那一刻（本机：入方向通道登记上；远端：那条流握手成功）⇒ 旧文件里那台那一格有东西 ⇒ 问那台 `bookmarks-edit {op: add}`；
//! - 那台答了 ok ⇒ 删掉旧文件里那一格；删空了整份（连锁旁件）删掉。没答 ok ⇒ 不动，下次连上再来，数据不丢；
//! - 旧文件读不懂 ⇒ 一格都不迁、一个字节不动（照实记一行日志），由人去看那份文件。
//!
//! 只读用户那份旧文件、写到那台后端那一份；旧文件本身只有「删一格 / 删整份」这两种写，在旁件 `.lock` 的独占锁里做。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 旧文件的名字（只有本迁移还认它）。
pub(crate) const OLD_FILE: &str = "filewin-bookmarks.json";

/// 旧文件那一形：机器名 ⇒ 那台上的目录。
type Book = BTreeMap<String, Vec<String>>;

fn lock_path(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_os_string();
    name.push(".lock");
    PathBuf::from(name)
}

/// 读整份：不在 ⇒ `Ok(None)`；读不懂 ⇒ `Err`。
fn read(file: &Path) -> Result<Option<Book>, String> {
    match std::fs::read_to_string(file) {
        Ok(raw) => serde_json::from_str(&raw)
            .map(Some)
            .map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// 旧文件里 `origin` 那一格（不在 · 空 · 读不懂 ⇒ `None`）。
pub(crate) fn pending_at(file: &Path, origin: &str) -> Option<Vec<String>> {
    match read(file) {
        Ok(book) => book?.remove(origin).filter(|v| !v.is_empty()),
        Err(e) => {
            tracing::warn!("书签迁移：旧文件 {} 读不懂，不迁不动：{e}", file.display());
            None
        }
    }
}

/// 那台答了 ok：在锁里删掉 `origin` 那一格；删空了整份连锁旁件删掉。
pub(crate) fn settled_at(file: &Path, origin: &str) -> std::io::Result<()> {
    let lock = std::fs::File::create(lock_path(file))?;
    lock.lock()?;
    let mut book = match read(file) {
        Ok(Some(b)) => b,
        Ok(None) => return Ok(()),
        Err(e) => return Err(std::io::Error::other(e)),
    };
    book.remove(origin);
    book.retain(|_, v| !v.is_empty());
    if book.is_empty() {
        std::fs::remove_file(file)?;
        drop(lock);
        return std::fs::remove_file(lock_path(file));
    }
    host_core::atomic_write_json(file, &book)
}

/// 连上 `origin` 那一刻（后台跑，不挡连接）：旧文件里有它那一格 ⇒ 加到那台后端，答 ok 才删那一格。
pub(crate) fn after_ready(origin: crate::origin::Origin) {
    let Some(file) = crate::config::resolve_monitor_data_dir().map(|d| d.join(OLD_FILE)) else {
        return;
    };
    let Some(dirs) = pending_at(&file, &origin.to_string()) else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let deadline = crate::frame_query::Deadline::within(std::time::Duration::from_secs(10));
        let args = serde_json::json!({ "op": "add", "dirs": dirs });
        let who = origin.to_string();
        match crate::frame_query::call(&origin, "bookmarks-edit", args, None, deadline).await {
            Ok(_) => match settled_at(&file, &who) {
                Ok(()) => tracing::info!("书签迁移：[{who}] 的旧书签已交给那台后端"),
                Err(e) => tracing::warn!("书签迁移：[{who}] 已交给那台，旧文件那一格没删掉：{e}"),
            },
            Err(e) => tracing::warn!("书签迁移：[{who}] 这次没交过去，下次连上再来：{e}"),
        }
    });
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/bookmarks_migration_tests.rs"]
mod tests;
