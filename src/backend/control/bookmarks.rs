//! 这台的书签：文件窗口与手机收藏的、这台上的目录。住 `~/.cc-monitor/bookmarks.json`（`relay_route_core::BOOKMARKS_REL`），
//! 后端自己的状态（删了会丢）；本机与远端同一形 —— 问哪台就是哪台的书签，文件里不带机器名。
//!
//! - 全仓唯一的写口是 [`answer_edit`]（只从 `faces/bookmarks_face.rs` 进）：读—改—写在那个目录的跨进程锁里、原子写
//!   ⇒ 两个窗口 / 一个窗口和手机同时加，两边加的都在。
//! - 切换 · 删 · 加怎么比（按归一之后的形，`cmd_wire::bookmarks::normalize_dir`）只在这里判。
//! - 读三态：不在 ⇒ 空的一份；读不懂 ⇒ `bookmarks_unreadable`，**不覆盖**（照实说出来，由人去看那份文件）。

use crate::common::own_state::{Doc, Read};
use crate::common::said::Said;
use crate::platform::fs::Own;
use crate::wire::spec::Fail;
use cmd_wire::bookmarks::{normalize_dir, BookmarkOp, Bookmarks, BookmarksEditReq};
use copy_core::copy_text;

/// 那一份：状态（读不懂 · 比这一版新 ⇒ 不覆盖；落点 · 上限 · 版本照声明表）。
const DOC: Doc<Bookmarks> = Doc::of(&relay_route_core::BOOKMARKS_FILE);

fn no_home(_: Said) -> Fail {
    Fail::new("io_failed", copy_text("beBookmarks.store.noHome", &[]))
}

fn read_in(own: &Own) -> Result<Bookmarks, Fail> {
    let file = own.at_home(DOC.rel).map_err(no_home)?;
    match DOC.read(file.as_path()) {
        Read::Absent => Ok(Bookmarks::default()),
        Read::Present(b) => Ok(b),
        Read::Unreadable(why) => Err(Fail::from(("bookmarks_unreadable", why))),
    }
}

/// 在 `dirs` 里照 `op` 改一条（按归一之后的形比；空路径不动）。
fn apply(dirs: &mut Vec<String>, op: BookmarkOp, dir: &str) {
    let d = normalize_dir(dir);
    if d.is_empty() {
        return;
    }
    let at = dirs.iter().position(|x| normalize_dir(x) == d);
    match (op, at) {
        (BookmarkOp::Toggle | BookmarkOp::Remove, Some(i)) => {
            dirs.remove(i);
        }
        (BookmarkOp::Toggle | BookmarkOp::Add, None) => dirs.push(d),
        (BookmarkOp::Remove, None) | (BookmarkOp::Add, Some(_)) => {}
    }
}

/// `bookmarks-read`：这台此刻的书签。
pub(crate) fn answer_read(own: &Own) -> Result<Bookmarks, Fail> {
    read_in(own)
}

/// `bookmarks-edit`：上锁 → 现读 → 改 → 原子换，回改完的整份。
pub(crate) fn answer_edit(own: &Own, req: &BookmarksEditReq) -> Result<Bookmarks, Fail> {
    let dir = own.home_dir().map_err(no_home)?;
    own.ensure_dir(&dir).map_err(|e| {
        Fail::from((
            "io_failed",
            Said::with_raw(
                copy_text(
                    "beBookmarks.store.mkdirFailed",
                    &[("dir", &dir.as_path().display().to_string())],
                ),
                e,
            ),
        ))
    })?;
    let held = own
        .lock(&dir)
        .map_err(|e| Fail::from(("io_failed", Said::from(e))))?;
    let mut book = read_in(own)?;
    for d in &req.dirs {
        apply(&mut book.dirs, req.op, d);
    }
    DOC.write(own, &dir, &held, &book)
        .map_err(|e| Fail::from(("io_failed", e)))?;
    Ok(book)
}

#[cfg(test)]
#[path = "../../../tests/backend/control/bookmarks_tests.rs"]
mod tests;
