//! 文件窗口的**书签**：书签归那台后端（`bookmarks-read` · `bookmarks-edit`，本体 `control/bookmarks.rs`），
//! 窗口只按这台的机器名问、照回包画 —— 本机与远端同一形，窗口进程不碰任何书签文件。
//!
//! - 读：开窗时与每次换目录时问一次 `bookmarks-read`（别的窗口 · 手机刚加的那几条从这里进来；不上定时器、不 watch）。
//! - 写：★ / × 各发一次 `bookmarks-edit`（切换 / 删），回包是改完的整份，照它换。两个写者同时写不丢，锁在那台后端。
//! - 几问同时在路上：只认最后发出的那一问的回包（先发后到的不盖掉后发的）。
//! - 问不成 ⇒ 那一句画在书签栏上（[`Shelf::notice`]），手上那一份不动。
//! - 判据那一形不接通道（`line == None`）：书签栏照画、一条都没有。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cmd_wire::bookmarks::{
    normalize_dir, BookmarkOp, Bookmarks, BookmarksEditReq, BookmarksReadReq,
};
use copy_core::copy_text;

use super::source::{Line, Origin};

/// 一问等多久。
const ASK: std::time::Duration = std::time::Duration::from_secs(10);

/// 窗口里的书签 —— **一个窗口一份**，所有标签页 / 两栏共用（克隆便宜）。
#[derive(Clone)]
pub struct Shelf(Arc<Inner>);

struct Inner {
    rt: Option<tokio::runtime::Handle>,
    line: Option<Line>,
    /// 哪台机器（问的就是它的后端）。
    origin: Origin,
    /// 发出去的第几问（从 1 起）。
    sent: AtomicU64,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// 这台上的书签（最近认下的那一份回包）。
    list: Vec<String>,
    /// 最近那一问没问成时那句话（`None` ＝ 没话说）。
    notice: Option<String>,
    /// 认下的是第几问的回包（比它早的不再认）。
    took: u64,
}

/// 书签栏上那颗切换按钮：当前目录不在书签里时。
pub static ADD_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinBookmarks.label.add", &[]));
/// 当前目录已经在书签里时。
pub static DROP_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinBookmarks.label.drop", &[]));

impl Shelf {
    /// 开窗时建：问一次那台机器的书签。
    pub fn open(rt: Option<tokio::runtime::Handle>, line: Option<Line>, origin: &Origin) -> Self {
        let s = Shelf(Arc::new(Inner {
            rt,
            line,
            origin: origin.clone(),
            sent: AtomicU64::new(0),
            state: Mutex::new(State::default()),
        }));
        s.refresh();
        s
    }

    /// 这台机器上的书签（最近认下的那一份）。
    pub fn list(&self) -> Vec<String> {
        self.0.state.lock().unwrap().list.clone()
    }

    /// 要画在书签栏上的那句话（`None` ＝ 没话说）。
    pub fn notice(&self) -> Option<String> {
        self.0.state.lock().unwrap().notice.clone()
    }

    /// `dir` 在不在书签里（按归一之后的形比）。
    pub fn contains(&self, dir: &str) -> bool {
        let d = normalize_dir(dir);
        self.0
            .state
            .lock()
            .unwrap()
            .list
            .iter()
            .any(|x| normalize_dir(x) == d)
    }

    /// 现问一次（换目录时调）。
    pub fn refresh(&self) {
        self.send(Ask::Read);
    }

    /// 切换 `dir`（★）。
    pub fn toggle(&self, dir: &str) {
        self.edit(BookmarkOp::Toggle, dir);
    }

    /// 删掉 `dir`（×）。
    pub fn remove(&self, dir: &str) {
        self.edit(BookmarkOp::Remove, dir);
    }

    fn edit(&self, op: BookmarkOp, dir: &str) {
        self.send(Ask::Edit(BookmarksEditReq {
            op,
            dirs: vec![dir.to_string()],
        }));
    }

    fn send(&self, ask: Ask) {
        let (Some(rt), Some(line)) = (self.0.rt.clone(), self.0.line.clone()) else {
            return;
        };
        let me = self.0.sent.fetch_add(1, Ordering::SeqCst) + 1;
        let inner = self.0.clone();
        rt.spawn(async move {
            let got = match &ask {
                Ask::Read => {
                    super::source::ask_typed(&line, &inner.origin, &BookmarksReadReq {}, ASK).await
                }
                Ask::Edit(req) => super::source::ask_typed(&line, &inner.origin, req, ASK).await,
            };
            inner.land(me, got.map_err(|f| f.said));
        });
    }
}

impl Inner {
    /// 第 `me` 问回来了：比已认下的那一问早 ⇒ 不认。
    fn land(&self, me: u64, got: Result<Bookmarks, String>) {
        let mut s = self.state.lock().unwrap();
        if me < s.took {
            return;
        }
        s.took = me;
        match got {
            Ok(b) => {
                s.list = b.dirs;
                s.notice = None;
            }
            Err(said) => s.notice = Some(said),
        }
    }
}

enum Ask {
    Read,
    Edit(BookmarksEditReq),
}

// 判据里别的模块（工作区 · 悬停扫）借它的合成书签后端与「手上已有几条」的那一形。
#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/bookmarks_tests.rs"]
pub(crate) mod tests;
