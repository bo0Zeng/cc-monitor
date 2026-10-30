//! 文件窗口的**书签**：书签归那台后端（`bookmarks-read` · `bookmarks-edit`，本体 `control/bookmarks.rs`），
//! 窗口只按这台的机器名问、照回包画 —— 本机与远端同一形，窗口进程不碰任何书签文件。
//!
//! - 读：开窗时与每次换目录时问一次 `bookmarks-read`（别的窗口 · 手机刚加的那几条从这里进来；不上定时器、不 watch）。
//! - 写：★ / × 各发一次 `bookmarks-edit`（切换 / 删），回包是改完的整份，照它换。两个写者同时写不丢，锁在那台后端。
//! - 「此刻这个目录在不在书签里、是第几条」由那台答：一个窗口的几栏 · 几个标签页共用这一份，它记着各栏到过的目录
//!   （[`TRACKED`] 个，最近的在前），每一问都把这一串带上（`at`），回包的 `here` 按目录各答一个下标；窗口不自己比路径。
//!   一栏改了书签 ⇒ 那一问的回包把别的栏一起答了，各栏照各自的答画。
//!   认下的那一问没问到的目录 ⇒ 当没答（[`Shelf::here`] 回 `None`），并把它记上、现问一次。
//! - 几问同时在路上：只认最后发出的那一问的回包（先发后到的不盖掉后发的）。
//! - 问不成 ⇒ 那一句画在书签栏上（[`Shelf::notice`]），手上那一份不动。
//! - 判据那一形不接通道（`line == None`）：书签栏照画、一条都没有。

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cmd_wire::bookmarks::{BookmarkOp, Bookmarks, BookmarksEditReq, BookmarksReadReq};
use copy_core::copy_text;

use super::source::{Line, Origin};

/// 一问等多久。
const ASK: std::time::Duration = std::time::Duration::from_secs(10);

/// 每一问至多带几个目录（各栏到过的，最近的在前）。
pub const TRACKED: usize = 32;

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
    /// 每一问带上的那一串目录（各栏到过的，最近的在前，至多 [`TRACKED`] 个）。
    tracked: Mutex<Vec<String>>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// 这台上的书签（最近认下的那一份回包）。
    list: Vec<String>,
    /// 认下的那一问带的是哪几个目录。
    asked: Vec<String>,
    /// 那一问的回包：目录 ⇒ 它在 `list` 里的第几条（不在的不列）。
    here: BTreeMap<String, u32>,
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
    /// 开窗时建：问一次那台机器的书签（带上开窗的那个目录）。
    pub fn open(
        rt: Option<tokio::runtime::Handle>,
        line: Option<Line>,
        origin: &Origin,
        at: &str,
    ) -> Self {
        let s = Shelf(Arc::new(Inner {
            rt,
            line,
            origin: origin.clone(),
            sent: AtomicU64::new(0),
            tracked: Mutex::new(Vec::new()),
            state: Mutex::new(State::default()),
        }));
        s.refresh(at);
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

    /// 那台答的：目录 `dir` 是 [`Shelf::list`] 里的第几条。不在 ⇒ `None`；
    /// 认下的那一问没问到 `dir` ⇒ 也是 `None`，并把它记上、现问一次（同一个目录只问这一次）。
    pub fn here(&self, dir: &str) -> Option<usize> {
        {
            let s = self.0.state.lock().unwrap();
            if s.asked.iter().any(|a| a == dir) {
                return s.here.get(dir).map(|&i| i as usize);
            }
        }
        if self.0.track(dir) {
            self.send(Ask::Read);
        }
        None
    }

    /// 现问一次（换目录时调，`at` ＝ 换到的那个目录）。
    pub fn refresh(&self, at: &str) {
        self.0.track(at);
        self.send(Ask::Read);
    }

    /// 切换 `dir`（★：切的就是那一栏此刻这个目录）。
    pub fn toggle(&self, dir: &str) {
        self.0.track(dir);
        self.edit(BookmarkOp::Toggle, dir);
    }

    /// 删掉 `dir`（×）。
    pub fn remove(&self, dir: &str) {
        self.edit(BookmarkOp::Remove, dir);
    }

    fn edit(&self, op: BookmarkOp, dir: &str) {
        self.send(Ask::Edit(op, dir.to_string()));
    }

    fn send(&self, ask: Ask) {
        let (Some(rt), Some(line)) = (self.0.rt.clone(), self.0.line.clone()) else {
            return;
        };
        let at = self.0.tracked.lock().unwrap().clone();
        let me = self.0.sent.fetch_add(1, Ordering::SeqCst) + 1;
        let inner = self.0.clone();
        rt.spawn(async move {
            let got = match ask {
                Ask::Read => {
                    let req = BookmarksReadReq { at: at.clone() };
                    super::source::ask_typed(&line, &inner.origin, &req, ASK).await
                }
                Ask::Edit(op, dir) => {
                    let req = BookmarksEditReq {
                        op,
                        dirs: vec![dir],
                        at: at.clone(),
                    };
                    super::source::ask_typed(&line, &inner.origin, &req, ASK).await
                }
            };
            inner.land(me, at, got.map_err(|f| f.said));
        });
    }
}

impl Inner {
    /// 把 `dir` 记进要带上的那一串（挪到最前）；原来没有 ⇒ `true`。
    fn track(&self, dir: &str) -> bool {
        let mut w = self.tracked.lock().unwrap();
        let had = match w.iter().position(|d| d == dir) {
            Some(i) => {
                w.remove(i);
                true
            }
            None => false,
        };
        w.insert(0, dir.to_string());
        w.truncate(TRACKED);
        !had
    }

    /// 第 `me` 问（带的是 `asked` 那几个目录）回来了：比已认下的那一问早 ⇒ 不认。
    fn land(&self, me: u64, asked: Vec<String>, got: Result<Bookmarks, String>) {
        let mut s = self.state.lock().unwrap();
        if me < s.took {
            return;
        }
        s.took = me;
        match got {
            Ok(b) => {
                s.list = b.dirs;
                s.asked = asked;
                s.here = b.here;
                s.notice = None;
            }
            Err(said) => s.notice = Some(said),
        }
    }
}

enum Ask {
    Read,
    Edit(BookmarkOp, String),
}

// 判据里别的模块（工作区 · 悬停扫）借它的合成书签后端与「手上已有几条」的那一形。
#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/bookmarks_tests.rs"]
pub(crate) mod tests;
