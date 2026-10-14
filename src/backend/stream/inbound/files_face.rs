//! 文件管理那一族里读的那几条（`files-ls` · `-stat` · `-find` …）：入参 · 回包是 `cmd_wire::files` 的类型，本体在 `files/`（只经 [`crate::files::answer`] 那一扇门）。
//!
//! 住入方向而不住 `faces/`：文件管理后端只许命令那一层够到（`files/module_boundary_guard.rs` 的门表）。
//! 也不住命令表那一份族文件：那一份还递删会话的窄口（够得到原生那一侧），本体所在的文件要只够得到文件管理那一块。

use crate::stream::inbound::spec::Fail;
use crate::stream::inbound::typed::{self, codes, Async, Blocking, Codes, Command, Cx};
use cmd_wire::files as wire;

/// 读的那几条共用：问本体，失败收进这一条的码表（原话跟着那一句走）。
fn ask<A: crate::files::Ask, E: Codes>(
    req: A,
    cx: &Cx,
    fallback: fn(String) -> E,
) -> Result<A::Reply, E> {
    crate::files::answer(req, &cx.tz).map_err(|f| {
        typed::of_fail(
            Fail::new(f.code, f.said).with_raw(f.raw.as_deref()),
            fallback,
        )
    })
}

/// 资产域那扇门（`doors::LocalFiles`）问读的那几样：同一个本体，失败只交码与那一句（不过这一条的码表：门不是帧面）。
pub(crate) fn in_process<A: crate::files::Ask>(
    req: A,
) -> Result<A::Reply, crate::common::door::Said> {
    crate::files::answer(req, &Default::default())
        .map_err(|f| crate::common::door::said_of(f.code, &f.said))
}

codes! {
    /// `files-ls` 的码。
    pub(crate) enum FilesLsErr {
        /// 路径是空的。
        BadPath = "bad_path",
        /// 没有权限列这个目录。
        Denied = "denied",
        /// 那不是一个目录。
        NotDir = "not_dir",
        /// 那个目录不在。
        NotFound = "not_found",
        /// 打不开（别的原因，原话进详情）。
        Unreadable = "unreadable",
    }
}

/// `files-ls`：列一个目录的直接子项。
pub(crate) struct FilesLs;

impl Command for FilesLs {
    const NAME: &'static str = "files-ls";
    type Req = wire::LsReq;
    type Reply = wire::LsReply;
    type Err = FilesLsErr;
}

impl Blocking for FilesLs {
    fn run(req: wire::LsReq, cx: &Cx) -> Result<wire::LsReply, FilesLsErr> {
        ask(req, cx, FilesLsErr::BadArgs)
    }
}

codes! {
    /// `files-stat` 的码。
    pub(crate) enum FilesStatErr {
        /// 路径是空的。
        BadPath = "bad_path",
        /// 读不到元数据。
        Unreadable = "unreadable",
    }
}

/// `files-stat`：一个路径的元数据（跟链接）。
pub(crate) struct FilesStat;

impl Command for FilesStat {
    const NAME: &'static str = "files-stat";
    type Req = wire::StatReq;
    type Reply = wire::StatReply;
    type Err = FilesStatErr;
}

impl Blocking for FilesStat {
    fn run(req: wire::StatReq, cx: &Cx) -> Result<wire::StatReply, FilesStatErr> {
        ask(req, cx, FilesStatErr::BadArgs)
    }
}

codes! {
    /// `files-find` 的码。
    pub(crate) enum FilesFindErr {
        /// `under` 是空的。
        BadPath = "bad_path",
        /// 搜索词解析不了。
        BadQuery = "bad_query",
        /// 同一个搜索框来了更新的一趟。
        Superseded = "superseded",
    }
}

/// `files-find`：在常驻索引里查。
pub(crate) struct FilesFind;

impl Command for FilesFind {
    const NAME: &'static str = "files-find";
    type Req = wire::FindReq;
    type Reply = wire::FindReply;
    type Err = FilesFindErr;
}

impl Blocking for FilesFind {
    fn run(req: wire::FindReq, cx: &Cx) -> Result<wire::FindReply, FilesFindErr> {
        ask(req, cx, FilesFindErr::BadArgs)
    }
}

codes! {
    /// `files-index-status` 的码。
    pub(crate) enum FilesIndexStatusErr {
    }
}

/// `files-index-status`：索引的新鲜度 · 条目数 · 常驻字节 · 声明的重走周期。
pub(crate) struct FilesIndexStatus;

impl Command for FilesIndexStatus {
    const NAME: &'static str = "files-index-status";
    const TAKES_INPUT: bool = false;
    type Req = wire::IndexStatusReq;
    type Reply = wire::IndexStatusReply;
    type Err = FilesIndexStatusErr;
}

impl Blocking for FilesIndexStatus {
    fn run(
        req: wire::IndexStatusReq,
        cx: &Cx,
    ) -> Result<wire::IndexStatusReply, FilesIndexStatusErr> {
        ask(req, cx, FilesIndexStatusErr::BadArgs)
    }
}

codes! {
    /// `files-index-rebuild` 的码。
    pub(crate) enum FilesIndexRebuildErr {
        /// 另一趟正在走。
        AlreadyRebuilding = "already_rebuilding",
        /// 路径是空的。
        BadPath = "bad_path",
        /// 没给路径，这台也说不出家目录。
        NoHome = "no_home",
        /// 根读不进去（常驻那一份不动）。
        Unreadable = "unreadable",
    }
}

/// `files-index-rebuild`：重建常驻文件名索引：走一遍，做完返回。
pub(crate) struct FilesIndexRebuild;

impl Command for FilesIndexRebuild {
    const NAME: &'static str = "files-index-rebuild";
    type Req = wire::RebuildReq;
    type Reply = wire::RebuildReply;
    type Err = FilesIndexRebuildErr;
}

impl Blocking for FilesIndexRebuild {
    fn run(req: wire::RebuildReq, cx: &Cx) -> Result<wire::RebuildReply, FilesIndexRebuildErr> {
        ask(req, cx, FilesIndexRebuildErr::BadArgs)
    }
}

codes! {
    /// `files-browse` 的码。
    pub(crate) enum FilesBrowseErr {
        /// 名单里有一项是空的。
        BadPath = "bad_path",
    }
}

/// `files-browse`：用户此刻在看哪几个目录（整份名单）。
pub(crate) struct FilesBrowse;

impl Command for FilesBrowse {
    const NAME: &'static str = "files-browse";
    type Req = wire::BrowseReq;
    type Reply = wire::BrowseReply;
    type Err = FilesBrowseErr;
}

impl Blocking for FilesBrowse {
    fn run(req: wire::BrowseReq, cx: &Cx) -> Result<wire::BrowseReply, FilesBrowseErr> {
        ask(req, cx, FilesBrowseErr::BadArgs)
    }
}

codes! {
    /// `files-read-text` 的码。
    pub(crate) enum FilesReadTextErr {
        /// 路径是空的。
        BadPath = "bad_path",
        /// 不是普通文件 · 含 NUL · 不是 UTF-8。
        NotText = "not_text",
        /// 超过这一趟给的上限（整趟拒、不截断）。
        TooLarge = "too_large",
        /// 打不开 · 读到一半断了。
        Unreadable = "unreadable",
    }
}

/// `files-read-text`：读一份文本进编辑器。
pub(crate) struct FilesReadText;

impl Command for FilesReadText {
    const NAME: &'static str = "files-read-text";
    type Req = wire::ReadTextReq;
    type Reply = wire::ReadTextReply;
    type Err = FilesReadTextErr;
}

impl Blocking for FilesReadText {
    fn run(req: wire::ReadTextReq, cx: &Cx) -> Result<wire::ReadTextReply, FilesReadTextErr> {
        ask(req, cx, FilesReadTextErr::BadArgs)
    }
}

codes! {
    /// `files-read-chunk` 的码。
    pub(crate) enum FilesReadChunkErr {
        /// 路径是空的。
        BadPath = "bad_path",
        /// 不是普通文件。
        NotText = "not_text",
        /// 打不开 · 读到一半断了。
        Unreadable = "unreadable",
    }
}

/// `files-read-chunk`：按字节寻址分块读回。
pub(crate) struct FilesReadChunk;

impl Command for FilesReadChunk {
    const NAME: &'static str = "files-read-chunk";
    type Req = wire::ReadChunkReq;
    type Reply = wire::ReadChunkReply;
    type Err = FilesReadChunkErr;
}

impl Blocking for FilesReadChunk {
    fn run(req: wire::ReadChunkReq, cx: &Cx) -> Result<wire::ReadChunkReply, FilesReadChunkErr> {
        ask(req, cx, FilesReadChunkErr::BadArgs)
    }
}

codes! {
    /// `files-size` 的码。
    pub(crate) enum FilesSizeErr {
        /// 路径是空的。
        BadPath = "bad_path",
        /// 起点读不到。
        Unreadable = "unreadable",
    }
}

/// `files-size`：算一个目录（或文件）有多大。
pub(crate) struct FilesSize;

impl Command for FilesSize {
    const NAME: &'static str = "files-size";
    type Req = wire::SizeReq;
    type Reply = wire::SizeReply;
    type Err = FilesSizeErr;
}

impl Blocking for FilesSize {
    fn run(req: wire::SizeReq, cx: &Cx) -> Result<wire::SizeReply, FilesSizeErr> {
        ask(req, cx, FilesSizeErr::BadArgs)
    }
}

codes! {
    /// `files-home` 的码。
    pub(crate) enum FilesHomeErr {
        /// 这台说不出家目录，或者说出来的不是绝对路径。
        NoHome = "no_home",
    }
}

/// `files-home`：后端这个用户的家目录。
pub(crate) struct FilesHome;

impl Command for FilesHome {
    const NAME: &'static str = "files-home";
    const TAKES_INPUT: bool = false;
    type Req = wire::HomeReq;
    type Reply = wire::FilesPathReply;
    type Err = FilesHomeErr;
}

impl Blocking for FilesHome {
    fn run(req: wire::HomeReq, cx: &Cx) -> Result<wire::FilesPathReply, FilesHomeErr> {
        ask(req, cx, FilesHomeErr::BadArgs)
    }
}

codes! {
    /// `files-grep` 的码。
    pub(crate) enum FilesGrepErr {
        /// 路径是空的。
        BadPath = "bad_path",
        /// 起点读不进去。
        Unreadable = "unreadable",
    }
}

/// `files-grep`：在一个目录底下按内容搜。真异步：`cancel` 丢掉这一份，搜的那一头在下一份文件前收手。
pub(crate) struct FilesGrep;

impl Command for FilesGrep {
    const NAME: &'static str = "files-grep";
    type Req = wire::GrepReq;
    type Reply = wire::GrepReply;
    type Err = FilesGrepErr;
}

impl Async for FilesGrep {
    fn run(
        req: wire::GrepReq,
        _cx: Cx,
    ) -> impl std::future::Future<Output = Result<wire::GrepReply, FilesGrepErr>> + Send + 'static
    {
        async move {
            crate::files::answer_grep_cancellable(req)
                .await
                .map_err(|f| {
                    typed::of_fail(
                        Fail::new(f.code, f.said).with_raw(f.raw.as_deref()),
                        FilesGrepErr::BadArgs,
                    )
                })
        }
    }
}
