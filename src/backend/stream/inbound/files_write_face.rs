//! 文件管理那一族里写的那几条（写面 · 上传提交 · 解压）：入参 · 回包是 `cmd_wire::files` 的类型，本体在 `control/files_write` ·
//! `files_commit` · `files_extract`（每条一个收入参类型、出回包类型的函数）。写经分派那一层递进来的 `Face`（`typed::Facing` 那一档）。
//!
//! 不住命令表那一份族文件：同读的那一份（`files_face.rs`）的理由，族文件只放登记那几行。

use crate::platform::fs::Face;
use crate::wire::spec::Fail;
use crate::wire::typed::{self, codes, Codes, Command};
use cmd_wire::batch::{BatchReply, Conflicts, FilesBatchStopReply, FilesBatchStopReq};
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
) -> Result<A::Reply, crate::common::door::Said> {
    crate::wire::spec::with_face(|face| crate::control::files_write::answer(req, face))
        .map_err(|f| crate::common::door::said_of(f.code, &f.said))
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

impl crate::wire::typed::Handle for FilesCreate {
    type Needs = crate::wire::typed::needs::Face;
    fn run(
        req: wire::FilesCreateReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::FilesPathBytesReply, FilesCreateErr>>
           + Send
           + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let face = &needs;

            ask(req, face, FilesCreateErr::BadArgs)
        }
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

impl crate::wire::typed::Handle for FilesMkdir {
    type Needs = crate::wire::typed::needs::Face;
    fn run(
        req: wire::FilesMkdirReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::FilesPathReply, FilesMkdirErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let face = &needs;

            ask(req, face, FilesMkdirErr::BadArgs)
        }
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

impl crate::wire::typed::Handle for FilesRename {
    type Needs = crate::wire::typed::needs::Face;
    fn run(
        req: wire::FilesRenameReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::FilesPathReply, FilesRenameErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let face = &needs;

            ask(req, face, FilesRenameErr::BadArgs)
        }
    }
}

codes! {
    /// `files-delete` 的码（一项没成不是整组没成：那一项的码在它的结局里）。
    pub(crate) enum FilesDeleteErr {
        /// 目标根是空的。
        BadPath = "bad_path",
    }
}

/// `files-delete`：删一组（文件 · 空目录 · 显式递归的整棵）。
pub(crate) struct FilesDelete;

impl Command for FilesDelete {
    const NAME: &'static str = "files-delete";
    type Req = wire::FilesDeleteReq;
    type Reply = BatchReply;
    type Err = FilesDeleteErr;
}

impl crate::wire::typed::Handle for FilesDelete {
    type Needs = crate::wire::typed::needs::Face;
    fn run(
        req: wire::FilesDeleteReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BatchReply, FilesDeleteErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let face = &needs;

            ask(req, face, FilesDeleteErr::BadArgs)
        }
    }
}

codes! {
    /// `files-chmod` 的码（同 `files-delete`：一项的码在它的结局里）。
    pub(crate) enum FilesChmodErr {
        /// 目标根是空的。
        BadPath = "bad_path",
        /// 这个平台没有 unix 权限位：一组里每一项都回它（那一项的结局里），整条不回 —— 登记在这里，平台那一维照它现推。
        NoUnixMode = "no_unix_mode",
    }
}

/// `files-chmod`：一组改成同一个 unix 权限位。
pub(crate) struct FilesChmod;

impl Command for FilesChmod {
    const NAME: &'static str = "files-chmod";
    type Req = wire::FilesChmodReq;
    type Reply = BatchReply;
    type Err = FilesChmodErr;
}

impl crate::wire::typed::Handle for FilesChmod {
    type Needs = crate::wire::typed::needs::Face;
    fn run(
        req: wire::FilesChmodReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BatchReply, FilesChmodErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let face = &needs;

            ask(req, face, FilesChmodErr::BadArgs)
        }
    }
}

codes! {
    /// `files-copy` 的码（同 `files-delete`：一项的码在它的结局里）。
    pub(crate) enum FilesCopyErr {
        /// 目标根是空的。
        BadPath = "bad_path",
        /// 有同名的、还没答怎么办（一项都没做）：`data` 是同名清单。
        Exists = "exists" => Conflicts,
    }
}

/// `files-copy`：同根内复制一组。
pub(crate) struct FilesCopy;

impl Command for FilesCopy {
    const NAME: &'static str = "files-copy";
    type Req = wire::FilesCopyReq;
    type Reply = BatchReply;
    type Err = FilesCopyErr;
}

impl crate::wire::typed::Handle for FilesCopy {
    type Needs = crate::wire::typed::needs::Face;
    fn run(
        req: wire::FilesCopyReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BatchReply, FilesCopyErr>> + Send + 'static {
        let (cx, needs) = cx.split();
        async move {
            let cx = &cx;
            let face = &needs;

            use crate::control::files_batch::CopyRefused;
            crate::control::files_batch::copy(face, req, &cx.tz).map_err(|e| match e {
                CopyRefused::Clash(said, data) => FilesCopyErr::Exists(said, data),
                CopyRefused::Fail(f) => typed::of_fail(
                    Fail::new(f.code, f.said).with_raw(f.raw.as_deref()),
                    FilesCopyErr::BadArgs,
                ),
            })
        }
    }
}

codes! {
    /// `files-batch-stop` 的码。
    pub(crate) enum FilesBatchStopErr {}
}

/// `files-batch-stop`：撤一组（做完手上那一项就停，剩下的回 `cancelled`）。
pub(crate) struct FilesBatchStop;

impl Command for FilesBatchStop {
    const NAME: &'static str = "files-batch-stop";
    type Req = FilesBatchStopReq;
    type Reply = FilesBatchStopReply;
    type Err = FilesBatchStopErr;
}

impl crate::wire::typed::Handle for FilesBatchStop {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: FilesBatchStopReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<FilesBatchStopReply, FilesBatchStopErr>> + Send + 'static
    {
        let stopping = crate::common::batch_book::stop(&req.ticket);
        async move { Ok(FilesBatchStopReply { stopping }) }
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

impl crate::wire::typed::Handle for FilesExtract {
    type Needs = crate::wire::typed::needs::Face;
    fn run(
        req: wire::FilesExtractReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::FilesTreeReply, FilesExtractErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let face = &needs;

            ask(req, face, FilesExtractErr::BadArgs)
        }
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

impl crate::wire::typed::Handle for FilesWriteText {
    type Needs = crate::wire::typed::needs::Face;
    fn run(
        req: wire::FilesWriteTextReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::FilesWrittenReply, FilesWriteTextErr>>
           + Send
           + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let face = &needs;

            ask(req, face, FilesWriteTextErr::BadArgs)
        }
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

impl crate::wire::typed::Handle for FilesStageChunk {
    type Needs = crate::wire::typed::needs::Face;
    fn run(
        req: wire::FilesStageChunkReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::FilesStageChunkReply, FilesStageChunkErr>>
           + Send
           + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let face = &needs;

            ask(req, face, FilesStageChunkErr::BadArgs)
        }
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

impl crate::wire::typed::Handle for FilesCommitText {
    type Needs = crate::wire::typed::needs::Face;
    fn run(
        req: wire::FilesCommitTextReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::FilesWrittenReply, FilesCommitTextErr>>
           + Send
           + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let face = &needs;

            ask(req, face, FilesCommitTextErr::BadArgs)
        }
    }
}

codes! {
    /// `files-commit-upload` 的码。
    pub(crate) enum FilesCommitUploadErr {
        /// 目标根或落点是空的。
        BadPath = "bad_path",
        /// 盘上这一步没成。
        IoFailed = "io_failed",
        /// 路径解析拒了。
        Refused = "refused",
        /// 暂存件的摘要对不上。
        Stale = "stale",
        /// 落点有同名、还没答怎么办（一个字节没动）：`data` 是同名清单（一项）。
        Exists = "exists" => Conflicts,
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

impl crate::wire::typed::Handle for FilesCommitUpload {
    type Needs = crate::wire::typed::needs::Face;
    fn run(
        req: wire::FilesCommitUploadReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::FilesPathBytesReply, FilesCommitUploadErr>>
           + Send
           + 'static {
        let (cx, needs) = cx.split();
        async move {
            let face = &needs;
            use crate::control::files_commit::CommitRefused;
            crate::control::files_commit::commit_upload_of(face, req, &cx.tz).map_err(|e| match e {
                CommitRefused::Clash(said, data) => FilesCommitUploadErr::Exists(said, data),
                CommitRefused::Fail(f) => typed::of_fail(
                    Fail::new(f.code, f.said).with_raw(f.raw.as_deref()),
                    FilesCommitUploadErr::BadArgs,
                ),
            })
        }
    }
}
