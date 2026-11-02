//! 〔一次性迁移 · 下一版整份删〕书签搬到各台后端（`bookmarks-read` / `-edit`，本体 `control/bookmarks.rs`）之后，
//! 桌面旧的那份 `<monitor 数据目录>/filewin-bookmarks.json`（`{ "<机器名>": ["/目录", …] }`）迁一次就删：
//!
//! - 连上一台那一刻（本机：入方向通道登记上；远端：那条流握手成功）⇒ 旧文件里那台那一格有东西 ⇒ 问那台 `bookmarks-edit {op: add}`；
//! - 那台答了 ok ⇒ 删掉旧文件里那一格；删空了整份（连锁旁件）删掉。没答 ok ⇒ 不动，下次连上再来，数据不丢；
//! - 旧文件读不懂 ⇒ 一格都不迁、一个字节不动（照实记一行日志），由人去看那份文件。
//!
//! 只读用户那份旧文件、写到那台后端那一份；旧文件本身只有「删一格 / 删整份」这两种写，在旁件 `.lock` 的独占锁里做，
//! 写与删都经 `platform::own_fs`（旧文件在数据目录底下，是 monitor 自己的状态）。

use crate::platform::own_fs::OwnState;
use std::collections::BTreeMap;
use std::path::Path;

/// 旧文件的名字（只有本迁移还认它）。
pub(crate) const OLD_FILE: &str = "filewin-bookmarks.json";

/// 旧文件的锁旁件（同一层，旧文件名后接 `.lock`）。
pub(crate) const LOCK_FILE: &str = "filewin-bookmarks.json.lock";

/// 旧文件那一形：机器名 ⇒ 那台上的目录。
type Book = BTreeMap<String, Vec<String>>;

/// 读整份，上限照新的那份书签的声明（`relay_route_core::BOOKMARKS_FILE`：装的是同一批书签）：
/// 不在 ⇒ `Ok(None)`；读不懂 · 超了上限 ⇒ `Err`（一格都不迁、一个字节不动）。
fn read(file: &Path) -> Result<Option<Book>, String> {
    let cap = relay_route_core::BOOKMARKS_FILE.row.cap;
    match host_core::read_capped(file, cap) {
        Ok(host_core::Capped::Whole(raw)) => serde_json::from_str(&raw)
            .map(Some)
            .map_err(|e| e.to_string()),
        Ok(host_core::Capped::Absent) => Ok(None),
        Ok(host_core::Capped::TooBig) => Err(format!("> {} bytes", cap.bytes)),
        Err(e) => Err(e.to_string()),
    }
}

/// 旧文件里 `origin` 那一格（不在 · 空 · 读不懂 ⇒ `None`）。
pub(crate) fn pending_at(own: &OwnState, origin: &str) -> Option<Vec<String>> {
    let file = own.path(OLD_FILE).ok()?;
    match read(&file) {
        Ok(book) => book?.remove(origin).filter(|v| !v.is_empty()),
        Err(e) => {
            tracing::warn!("书签迁移：旧文件 {} 读不懂，不迁不动：{e}", file.display());
            None
        }
    }
}

/// 那台答了 ok：在锁里删掉 `origin` 那一格；删空了整份连锁旁件删掉。
pub(crate) fn settled_at(own: &OwnState, origin: &str) -> std::io::Result<()> {
    let file = own.path(OLD_FILE)?;
    let lock = own.create(LOCK_FILE)?;
    lock.lock()?;
    let mut book = match read(&file) {
        Ok(Some(b)) => b,
        Ok(None) => return Ok(()),
        Err(e) => return Err(std::io::Error::other(e)),
    };
    book.remove(origin);
    book.retain(|_, v| !v.is_empty());
    if book.is_empty() {
        own.remove(OLD_FILE)?;
        drop(lock);
        return own.remove(LOCK_FILE);
    }
    let body = serde_json::to_vec_pretty(&book).map_err(std::io::Error::other)?;
    own.write(OLD_FILE, &body)
}

/// 连上 `origin` 那一刻（后台跑，不挡连接）：旧文件里有它那一格 ⇒ 加到那台后端，答 ok 才删那一格。
pub(crate) fn after_ready(origin: crate::origin::Origin) {
    let Some(own) = OwnState::issue() else {
        return;
    };
    let Some(dirs) = pending_at(&own, &origin.to_string()) else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let deadline = crate::frame_query::Deadline::within(std::time::Duration::from_secs(10));
        let args = serde_json::json!({ "op": "add", "dirs": dirs });
        let who = origin.to_string();
        match crate::frame_query::call(&origin, "bookmarks-edit", args, None, deadline).await {
            Ok(_) => match settled_at(&own, &who) {
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
