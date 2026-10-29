//! 书签那两条（typed）：`bookmarks-read` · `bookmarks-edit`。本体在 `control/bookmarks.rs`（判定与写都在那里），
//! 这里只接上名字 · 入参 · 回包 · 码表与写句柄档：两条都读写后端自家状态（`~/.cc-monitor/`）⇒ 只要写句柄那一档。

use crate::control::bookmarks;
use crate::wire::typed::{self, codes, Command};
use cmd_wire::bookmarks::{Bookmarks, BookmarksEditReq, BookmarksReadReq};

codes! {
    /// `bookmarks-read` 的码。
    pub(crate) enum BookmarksReadErr {
        /// 这台的家目录解析不出来。
        IoFailed = "io_failed",
        /// 盘上那一份读不懂（不覆盖）。
        BookmarksUnreadable = "bookmarks_unreadable",
    }
}

/// `bookmarks-read`：这台此刻的书签。
pub(crate) struct BookmarksReadCmd;

impl Command for BookmarksReadCmd {
    typed::by_call!(BookmarksReadReq);
    const TAKES_INPUT: bool = false;
    type Err = BookmarksReadErr;
}

impl crate::wire::typed::Handle for BookmarksReadCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        _req: BookmarksReadReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Bookmarks, BookmarksReadErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            bookmarks::answer_read(&needs)
                .map_err(|f| typed::of_fail(f, BookmarksReadErr::IoFailed))
        }
    }
}

codes! {
    /// `bookmarks-edit` 的码。
    pub(crate) enum BookmarksEditErr {
        /// 建那一层目录 · 拿锁 · 写没成。
        IoFailed = "io_failed",
        /// 盘上那一份读不懂（不覆盖）。
        BookmarksUnreadable = "bookmarks_unreadable",
    }
}

/// `bookmarks-edit`：切换 · 删 · 加几条，回改完的整份。
pub(crate) struct BookmarksEditCmd;

impl Command for BookmarksEditCmd {
    typed::by_call!(BookmarksEditReq);
    type Err = BookmarksEditErr;
}

impl crate::wire::typed::Handle for BookmarksEditCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: BookmarksEditReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Bookmarks, BookmarksEditErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            bookmarks::answer_edit(&needs, &req)
                .map_err(|f| typed::of_fail(f, BookmarksEditErr::IoFailed))
        }
    }
}
