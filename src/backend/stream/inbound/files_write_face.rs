//! 文件管理那一族里写的那几条（写面 · 上传提交 · 解压）：入参 · 回包是 `cmd_wire::files` 的类型，本体在 `control/files_write` ·
//! `files_commit` · `files_extract`（每条一个收入参类型、出回包类型的函数）。写经分派那一层递进来的 `Face`（`typed::Facing` 那一档）。
//!
//! 不住命令表那一份族文件：同读的那一份（`files_face.rs`）的理由，族文件只放登记那几行。

use crate::platform::fs::Face;
use crate::stream::inbound::spec::Fail;
use crate::stream::inbound::typed::{self, codes, Codes, Command, Cx, Facing};
use cmd_wire::files as wire;

/// 写的那几条共用：问本体，失败收进这一条的码表（原话跟着那一句走）。
fn ask<A: crate::control::files_write::Act, E: Codes>(
    req: A,
    face: &Face,
    fallback: fn(String) -> E,
) -> Result<A::Reply, E> {
    crate::control::files_write::answer(req, face).map_err(|f| {
        typed::of_fail(
            Fail::new(f.code, f.said).with_raw(f.raw.as_deref()),
            fallback,
        )
    })
}

/// 资产域那扇门（`doors::LocalFiles`）问写的那几样：同一个本体，写句柄经分派那一层（`spec::with_face`）领一份、只活这一问；
/// 失败只交码与那一句（不过这一条的码表：门不是帧面）。
pub(crate) fn in_process<A: crate::control::files_write::Act>(
    req: A,
) -> Result<A::Reply, crate::assets::door::Said> {
    crate::stream::inbound::spec::with_face(|face| crate::control::files_write::answer(req, face))
        .map_err(|f| crate::assets::door::said_of(f.code, &f.said))
}

codes! {
    /// `files-create` 的码。
    pub(crate) enum FilesCreateErr {
        /// 名字不合这台的规则。
        BadName = "bad_name",
        /// 目标根是空的。
        BadPath = "bad_path",
        /// 那儿已经有东西了。
        Exists = "exists",
        /// 盘上这一步没成。
        IoFailed = "io_failed",
        /// 路径解析拒了。
        Refused = "refused",
    }
}

/// `files-create`：在文件管理目标根底下新建一份此前不存在的文件。
pub(crate) struct FilesCreate;

impl Command for FilesCreate {
    const NAME: &'static str = "files-create";
    type Req = wire::FilesCreateReq;
    type Reply = wire::FilesPathBytesReply;
    type Err = FilesCreateErr;
}

impl Facing for FilesCreate {
    fn run(
        req: wire::FilesCreateReq,
        _cx: &Cx,
        face: &Face,
    ) -> Result<wire::FilesPathBytesReply, FilesCreateErr> {
        ask(req, face, FilesCreateErr::BadArgs)
    }
}

codes! {
    /// `files-mkdir` 的码。
    pub(crate) enum FilesMkdirErr {
        /// 名字不合这台的规则。
        BadName = "bad_name",
        /// 目标根是空的。
        BadPath = "bad_path",
        /// 那儿已经有东西了。
        Exists = "exists",
        /// 盘上这一步没成。
        IoFailed = "io_failed",
        /// 路径解析拒了。
        Refused = "refused",
    }
}

/// `files-mkdir`：新建一个目录。
pub(crate) struct FilesMkdir;

impl Command for FilesMkdir {
    const NAME: &'static str = "files-mkdir";
    type Req = wire::FilesMkdirReq;
    type Reply = wire::FilesPathReply;
    type Err = FilesMkdirErr;
}

impl Facing for FilesMkdir {
    fn run(
        req: wire::FilesMkdirReq,
        _cx: &Cx,
        face: &Face,
    ) -> Result<wire::FilesPathReply, FilesMkdirErr> {
        ask(req, face, FilesMkdirErr::BadArgs)
    }
}

codes! {
    /// `files-rename` 的码。
    pub(crate) enum FilesRenameErr {
        /// 名字不合这台的规则。
        BadName = "bad_name",
        /// 目标根是空的。
        BadPath = "bad_path",
        /// 目标已经在了（不覆盖）。
        Exists = "exists",
        /// 盘上这一步没成。
        IoFailed = "io_failed",
        /// 路径解析拒了。
        Refused = "refused",
    }
}

/// `files-rename`：改名 / 同根内移动。
pub(crate) struct FilesRename;

impl Command for FilesRename {
    const NAME: &'static str = "files-rename";
    type Req = wire::FilesRenameReq;
    type Reply = wire::FilesPathReply;
    type Err = FilesRenameErr;
}

impl Facing for FilesRename {
    fn run(
        req: wire::FilesRenameReq,
        _cx: &Cx,
        face: &Face,
    ) -> Result<wire::FilesPathReply, FilesRenameErr> {
        ask(req, face, FilesRenameErr::BadArgs)
    }
}

codes! {
    /// `files-delete` 的码。
    pub(crate) enum FilesDeleteErr {
        /// 目标根是空的。
        BadPath = "bad_path",
        /// 盘上这一步没成。
        IoFailed = "io_failed",
        /// 路径解析拒了。
        Refused = "refused",
        /// 盘上那一份跟读到的不一样了（一个字节没动）。
        Stale = "stale",
    }
}

/// `files-delete`：删一个文件或一个空目录。
pub(crate) struct FilesDelete;

impl Command for FilesDelete {
    const NAME: &'static str = "files-delete";
    type Req = wire::FilesDeleteReq;
    type Reply = wire::FilesDeleteReply;
    type Err = FilesDeleteErr;
}

impl Facing for FilesDelete {
    fn run(
        req: wire::FilesDeleteReq,
        _cx: &Cx,
        face: &Face,
    ) -> Result<wire::FilesDeleteReply, FilesDeleteErr> {
        ask(req, face, FilesDeleteErr::BadArgs)
    }
}

codes! {
    /// `files-chmod` 的码。
    pub(crate) enum FilesChmodErr {
        /// 目标根是空的。
        BadPath = "bad_path",
        /// 盘上这一步没成。
        IoFailed = "io_failed",
        /// 这个平台没有 unix 权限位。
        NoUnixMode = "no_unix_mode",
        /// 路径解析拒了 · 超出低 12 位。
        Refused = "refused",
    }
}

/// `files-chmod`：改 unix 权限位。
pub(crate) struct FilesChmod;

impl Command for FilesChmod {
    const NAME: &'static str = "files-chmod";
    type Req = wire::FilesChmodReq;
    type Reply = wire::FilesChmodReply;
    type Err = FilesChmodErr;
}

impl Facing for FilesChmod {
    fn run(
        req: wire::FilesChmodReq,
        _cx: &Cx,
        face: &Face,
    ) -> Result<wire::FilesChmodReply, FilesChmodErr> {
        ask(req, face, FilesChmodErr::BadArgs)
    }
}

codes! {
    /// `files-copy` 的码。
    pub(crate) enum FilesCopyErr {
        /// 目标根是空的。
        BadPath = "bad_path",
        /// 盘上这一步没成。
        IoFailed = "io_failed",
        /// 路径解析拒了 · 目标已在。
        Refused = "refused",
    }
}

/// `files-copy`：同根内复制一份普通文件。
pub(crate) struct FilesCopy;

impl Command for FilesCopy {
    const NAME: &'static str = "files-copy";
    type Req = wire::FilesCopyReq;
    type Reply = wire::FilesTreeReply;
    type Err = FilesCopyErr;
}

impl Facing for FilesCopy {
    fn run(
        req: wire::FilesCopyReq,
        _cx: &Cx,
        face: &Face,
    ) -> Result<wire::FilesTreeReply, FilesCopyErr> {
        ask(req, face, FilesCopyErr::BadArgs)
    }
}

codes! {
    /// `files-extract` 的码。
    pub(crate) enum FilesExtractErr {
        /// 目标根或包是空的。
        BadPath = "bad_path",
        /// 落点目录已经在了。
        Exists = "exists",
        /// 盘上 / 包里这一步没成。
        IoFailed = "io_failed",
        /// 包里有一项过不了路径解析（整包不动）。
        Refused = "refused",
        /// 不认这种包。
        Unsupported = "unsupported",
    }
}

/// `files-extract`：解压到一个新目录。
pub(crate) struct FilesExtract;

impl Command for FilesExtract {
    const NAME: &'static str = "files-extract";
    type Req = wire::FilesExtractReq;
    type Reply = wire::FilesTreeReply;
    type Err = FilesExtractErr;
}

impl Facing for FilesExtract {
    fn run(
        req: wire::FilesExtractReq,
        _cx: &Cx,
        face: &Face,
    ) -> Result<wire::FilesTreeReply, FilesExtractErr> {
        ask(req, face, FilesExtractErr::BadArgs)
    }
}

codes! {
    /// `files-link` 的码。
    pub(crate) enum FilesLinkErr {
        /// 目标根 · 链接 · 目标有一格是空的。
        BadPath = "bad_path",
        /// 盘上这一步没成。
        IoFailed = "io_failed",
        /// 路径解析拒了 · 那儿已经有东西。
        Refused = "refused",
    }
}

/// `files-link`：建一条符号链接。
pub(crate) struct FilesLink;

impl Command for FilesLink {
    const NAME: &'static str = "files-link";
    type Req = wire::FilesLinkReq;
    type Reply = wire::FilesPathReply;
    type Err = FilesLinkErr;
}

impl Facing for FilesLink {
    fn run(
        req: wire::FilesLinkReq,
        _cx: &Cx,
        face: &Face,
    ) -> Result<wire::FilesPathReply, FilesLinkErr> {
        ask(req, face, FilesLinkErr::BadArgs)
    }
}

codes! {
    /// `files-write-text` 的码。
    pub(crate) enum FilesWriteTextErr {
        /// 目标根是空的。
        BadPath = "bad_path",
        /// 盘上这一步没成。
        IoFailed = "io_failed",
        /// 路径解析拒了 · 不是普通文件。
        Refused = "refused",
        /// 盘上那一份的摘要对不上（一个字节没写）。
        Stale = "stale",
    }
}

/// `files-write-text`：覆盖写一份已经在的普通文件。
pub(crate) struct FilesWriteText;

impl Command for FilesWriteText {
    const NAME: &'static str = "files-write-text";
    type Req = wire::FilesWriteTextReq;
    type Reply = wire::FilesWrittenReply;
    type Err = FilesWriteTextErr;
}

impl Facing for FilesWriteText {
    fn run(
        req: wire::FilesWriteTextReq,
        _cx: &Cx,
        face: &Face,
    ) -> Result<wire::FilesWrittenReply, FilesWriteTextErr> {
        ask(req, face, FilesWriteTextErr::BadArgs)
    }
}

codes! {
    /// `files-peek` 的码。
    pub(crate) enum FilesPeekErr {
        /// 目标根是空的。
        BadPath = "bad_path",
        /// 读不出来。
        IoFailed = "io_failed",
        /// 不是 UTF-8 文本。
        NotText = "not_text",
        /// 路径解析拒了。
        Refused = "refused",
        /// 太大，不读。
        TooLarge = "too_large",
    }
}

/// `files-peek`：读改写的读那一半。
pub(crate) struct FilesPeek;

impl Command for FilesPeek {
    const NAME: &'static str = "files-peek";
    type Req = wire::FilesPeekReq;
    type Reply = wire::FilesPeekReply;
    type Err = FilesPeekErr;
}

impl Facing for FilesPeek {
    fn run(
        req: wire::FilesPeekReq,
        _cx: &Cx,
        face: &Face,
    ) -> Result<wire::FilesPeekReply, FilesPeekErr> {
        ask(req, face, FilesPeekErr::BadArgs)
    }
}

codes! {
    /// `files-put` 的码。
    pub(crate) enum FilesPutErr {
        /// 目标根是空的。
        BadPath = "bad_path",
        /// 盘上这一步没成（已回滚）。
        IoFailed = "io_failed",
        /// 路径解析拒了。
        Refused = "refused",
        /// 盘上那一份跟读到的不一样了（一个字节没动）。
        Stale = "stale",
    }
}

/// `files-put`：整份替换一份文本文件。
pub(crate) struct FilesPut;

impl Command for FilesPut {
    const NAME: &'static str = "files-put";
    type Req = wire::FilesPutReq;
    type Reply = wire::FilesPutReply;
    type Err = FilesPutErr;
}

impl Facing for FilesPut {
    fn run(
        req: wire::FilesPutReq,
        _cx: &Cx,
        face: &Face,
    ) -> Result<wire::FilesPutReply, FilesPutErr> {
        ask(req, face, FilesPutErr::BadArgs)
    }
}

codes! {
    /// `files-stage-chunk` 的码。
    pub(crate) enum FilesStageChunkErr {
        /// 暂存区写不进去 · 这台说不出家目录。
        IoFailed = "io_failed",
        /// 键不对 · 这一块已经在了。
        Refused = "refused",
    }
}

/// `files-stage-chunk`：存盘的一块进暂存区。
pub(crate) struct FilesStageChunk;

impl Command for FilesStageChunk {
    const NAME: &'static str = "files-stage-chunk";
    type Req = wire::FilesStageChunkReq;
    type Reply = wire::FilesStageChunkReply;
    type Err = FilesStageChunkErr;
}

impl Facing for FilesStageChunk {
    fn run(
        req: wire::FilesStageChunkReq,
        _cx: &Cx,
        face: &Face,
    ) -> Result<wire::FilesStageChunkReply, FilesStageChunkErr> {
        ask(req, face, FilesStageChunkErr::BadArgs)
    }
}

codes! {
    /// `files-commit-text` 的码。
    pub(crate) enum FilesCommitTextErr {
        /// 目标根是空的。
        BadPath = "bad_path",
        /// 盘上这一步没成。
        IoFailed = "io_failed",
        /// 路径解析拒了 · 块拼不起来。
        Refused = "refused",
        /// 盘上那一份的摘要对不上（目标一个字节没动）。
        Stale = "stale",
    }
}

/// `files-commit-text`：按块读回、拼起来、覆盖写。
pub(crate) struct FilesCommitText;

impl Command for FilesCommitText {
    const NAME: &'static str = "files-commit-text";
    type Req = wire::FilesCommitTextReq;
    type Reply = wire::FilesWrittenReply;
    type Err = FilesCommitTextErr;
}

impl Facing for FilesCommitText {
    fn run(
        req: wire::FilesCommitTextReq,
        _cx: &Cx,
        face: &Face,
    ) -> Result<wire::FilesWrittenReply, FilesCommitTextErr> {
        ask(req, face, FilesCommitTextErr::BadArgs)
    }
}

codes! {
    /// `files-commit-upload` 的码。
    pub(crate) enum FilesCommitUploadErr {
        /// 目标根或落点是空的。
        BadPath = "bad_path",
        /// 盘上这一步没成。
        IoFailed = "io_failed",
        /// 路径解析拒了 · 目标已在而没让覆盖。
        Refused = "refused",
        /// 暂存件的摘要对不上。
        Stale = "stale",
    }
}

/// `files-commit-upload`：把暂存区里一份传完的上传件挪进目标。
pub(crate) struct FilesCommitUpload;

impl Command for FilesCommitUpload {
    const NAME: &'static str = "files-commit-upload";
    type Req = wire::FilesCommitUploadReq;
    type Reply = wire::FilesPathBytesReply;
    type Err = FilesCommitUploadErr;
}

impl Facing for FilesCommitUpload {
    fn run(
        req: wire::FilesCommitUploadReq,
        _cx: &Cx,
        face: &Face,
    ) -> Result<wire::FilesPathBytesReply, FilesCommitUploadErr> {
        ask(req, face, FilesCommitUploadErr::BadArgs)
    }
}
