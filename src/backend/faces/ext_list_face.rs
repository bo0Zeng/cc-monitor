//! 设置「扩展」页那几条（typed）：`ext-list`（那张表；读可达表认出每台的名字 ⇒ 只在流面上）· `ext-uninstall-preview` / `-apply`
//! （从这台卸一个扩展：判与写都在这台，写经命令表递进来的文件管理面，装记录的写口从这里递进去）。本体在 `assets/ext.rs`。

use crate::assets::asset_catalog;
use crate::assets::ext;
use crate::faces::catalog_face::CatalogErr;
use crate::wire::typed::{self, codes, Command, Cx, Files, Owning, WithFiles, WithOwn};

/// `ext-list`：这台现扫一次、记下（资产目录的写口从这里递进去），各台目录合成「条目 × 机器」。
pub(crate) struct ExtListCmd;

impl Command for ExtListCmd {
    const NAME: &'static str = "ext-list";
    type Req = ext::ExtListReq;
    type Reply = ext::ExtList;
    type Err = CatalogErr;
}

impl Owning for ExtListCmd {
    fn run(
        req: ext::ExtListReq,
        _cx: &Cx,
        own: &crate::platform::Own,
    ) -> Result<ext::ExtList, CatalogErr> {
        ext::answer_list(
            &req,
            &|visit| asset_catalog::answer_current(visit, own),
            &ext::reach_of(&crate::dial::remote_ask::REACH),
        )
        .map_err(|f| typed::of_fail(f, CatalogErr::IoFailed))
    }
}

codes! {
    /// `ext-uninstall-preview` 的码。
    pub(crate) enum UninstallPreviewErr {
        /// 那份配置文件读不懂。
        BadFile = "bad_file",
        /// 项目目录不是这台上的绝对路径 · 名字带分隔符。
        BadPath = "bad_path",
        /// 家目录 · 读盘没成。
        IoFailed = "io_failed",
        /// 装记录读不懂（不覆盖）。
        LedgerUnreadable = "ledger_unreadable",
        /// 这台那一级没有它。
        NotFound = "not_found",
        /// 这一级在这台只读（用户级 MCP 而这台没有账号库）。
        Refused = "refused",
    }
}

/// `ext-uninstall-preview`：从这台卸一个扩展之前那张卡。
pub(crate) struct ExtUninstallPreviewCmd;

impl Command for ExtUninstallPreviewCmd {
    const NAME: &'static str = "ext-uninstall-preview";
    type Req = ext::ExtUninstallReq;
    type Reply = ext::ExtUninstallCard;
    type Err = UninstallPreviewErr;
}

impl WithFiles for ExtUninstallPreviewCmd {
    fn run(
        req: ext::ExtUninstallReq,
        _cx: &Cx,
        files: Files,
    ) -> Result<ext::ExtUninstallCard, UninstallPreviewErr> {
        ext::answer_uninstall_preview(files, &ext::Env::here(), &req)
            .map_err(|f| typed::of_fail(f, UninstallPreviewErr::IoFailed))
    }
}

codes! {
    /// `ext-uninstall-apply` 的码。
    pub(crate) enum UninstallApplyErr {
        /// 那份配置文件读不懂。
        BadFile = "bad_file",
        /// 项目目录不是这台上的绝对路径 · 名字带分隔符。
        BadPath = "bad_path",
        /// 家目录 · 挪 · 删没成。
        IoFailed = "io_failed",
        /// 装记录读不懂（不覆盖）。
        LedgerUnreadable = "ledger_unreadable",
        /// 要用户点头的那几个没勾。
        NeedsConsent = "needs_consent",
        /// 这台那一级没有它。
        NotFound = "not_found",
        /// 这一级在这台只读 · 写口拒了。
        Refused = "refused",
        /// 卡上那一份记号现在对不上（看过之后又变了）：一个字节不动。
        Stale = "stale",
    }
}

/// `ext-uninstall-apply`：从这台卸一个扩展。
pub(crate) struct ExtUninstallApplyCmd;

impl Command for ExtUninstallApplyCmd {
    const NAME: &'static str = "ext-uninstall-apply";
    type Req = ext::ExtUninstallApplyReq;
    type Reply = ext::ExtDone;
    type Err = UninstallApplyErr;
}

impl WithOwn for ExtUninstallApplyCmd {
    fn run(
        req: ext::ExtUninstallApplyReq,
        _cx: &Cx,
        files: Files,
        own: &crate::platform::Own,
    ) -> Result<ext::ExtDone, UninstallApplyErr> {
        ext::answer_uninstall_apply(
            files,
            &ext::Env::here(),
            &|a| crate::assets::skill_ledger::answer_record(a, own),
            &req,
        )
        .map_err(|f| typed::of_fail(f, UninstallApplyErr::IoFailed))
    }
}
