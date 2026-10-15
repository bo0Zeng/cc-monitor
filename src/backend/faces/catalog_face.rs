//! 资产目录那一族的四条（typed）：`assets-catalog` · `assets-catalog-merge` · `ext-note-set` · `skill-install-record`。
//! 本体在 `assets/`（`asset_catalog` · `ext` · `skill_ledger`），这里只接上名字 · 入参 · 回包 · 码表与写句柄档：
//! 四条都只写后端自家状态（`~/.cc-monitor/` 底下的目录 / 装记录）⇒ 只要写句柄那一档，一个用户文件都不写。

use crate::assets::asset_catalog;
use crate::assets::ext;
use crate::assets::skill_ledger;
use crate::wire::typed::{self, codes, Command, NoArgs};

codes! {
    /// 资产目录那三条的码（`assets-catalog` · `-merge` · `ext-note-set`）。
    pub(crate) enum CatalogErr {
        /// 盘上那一份读不懂 / 是更新的版本（不覆盖）。
        CatalogUnreadable = "catalog_unreadable",
        /// 家目录 · 建目录 · 拿锁 · 写没成。
        IoFailed = "io_failed",
    }
}

/// `assets-catalog`：这台现扫一次、记下（变了才写），回整份目录 ＋「这台缺什么」。
pub(crate) struct AssetsCatalogCmd;

impl Command for AssetsCatalogCmd {
    const NAME: &'static str = "assets-catalog";
    const TAKES_INPUT: bool = false;
    type Req = NoArgs;
    type Reply = asset_catalog::CatalogReply;
    type Err = CatalogErr;
}

impl crate::wire::typed::Handle for AssetsCatalogCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        _req: NoArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<asset_catalog::CatalogReply, CatalogErr>>
           + Send
           + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let own = &needs;

            asset_catalog::answer_catalog(own).map_err(|f| typed::of_fail(f, CatalogErr::IoFailed))
        }
    }
}

/// `assets-catalog-merge`：同上，再把另一台后端的整份并进来（同一台取 `gen` 大的整份）。
pub(crate) struct AssetsCatalogMergeCmd;

impl Command for AssetsCatalogMergeCmd {
    const NAME: &'static str = "assets-catalog-merge";
    type Req = asset_catalog::CatalogMergeReq;
    type Reply = asset_catalog::CatalogReply;
    type Err = CatalogErr;
}

impl crate::wire::typed::Handle for AssetsCatalogMergeCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: asset_catalog::CatalogMergeReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<asset_catalog::CatalogReply, CatalogErr>>
           + Send
           + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let own = &needs;

            asset_catalog::answer_merge(req, own)
                .map_err(|f| typed::of_fail(f, CatalogErr::IoFailed))
        }
    }
}

/// `ext-note-set`：写 / 改 / 清一个扩展的备注（记进本机目录自己那一格，随目录同步）。
pub(crate) struct ExtNoteSetCmd;

impl Command for ExtNoteSetCmd {
    const NAME: &'static str = "ext-note-set";
    type Req = ext::ExtNoteReq;
    type Reply = ext::ExtNote;
    type Err = CatalogErr;
}

impl crate::wire::typed::Handle for ExtNoteSetCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: ext::ExtNoteReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ext::ExtNote, CatalogErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let own = &needs;

            ext::answer_note(&req, &|k, t| asset_catalog::answer_note(k, t, own))
                .map_err(|f| typed::of_fail(f, CatalogErr::IoFailed))
        }
    }
}

codes! {
    /// `skill-install-record` 的码。
    pub(crate) enum RecordErr {
        /// 家目录 · 建目录 · 拿锁 · 写没成 · skill 根算不出。
        IoFailed = "io_failed",
        /// 盘上那份装记录读不懂（不覆盖）。
        LedgerUnreadable = "ledger_unreadable",
        /// `drop` 的那个目录不在记录里。
        NotFound = "not_found",
        /// 一趟 `add` 的文件超过上限。
        TooLarge = "too_large",
    }
}

/// `skill-install-record`：装记录的写口（skill 装完记 `add` · 卸掉的摘 `drop`；MCP 那一条 `mcp-add` / `mcp-drop`）。
pub(crate) struct SkillInstallRecordCmd;

impl Command for SkillInstallRecordCmd {
    const NAME: &'static str = "skill-install-record";
    type Req = skill_ledger::RecordReq;
    type Reply = skill_ledger::RecordReply;
    type Err = RecordErr;
}

impl crate::wire::typed::Handle for SkillInstallRecordCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: skill_ledger::RecordReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<skill_ledger::RecordReply, RecordErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let own = &needs;

            skill_ledger::answer_record(&req, own)
                .map_err(|f| typed::of_fail(f, RecordErr::IoFailed))
        }
    }
}

/// `ext-list-here`：扩展页那张表的**本机那一半**（同一个本体，目录先裁到这台自己那一格；不问可达表 ⇒ 上 CLI 面）。
pub(crate) struct ExtListHereCmd;

impl Command for ExtListHereCmd {
    const NAME: &'static str = "ext-list-here";
    type Req = ext::ExtListReq;
    type Reply = ext::ExtList;
    type Err = CatalogErr;
}

impl crate::wire::typed::Handle for ExtListHereCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: ext::ExtListReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ext::ExtList, CatalogErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let own = &needs;

            // 裁到这台那一格再交给同一个本体。`answer_current` 仍是那扇门（现扫 ＋ 记下整份目录，别的台那几格一个字节不动）。
            let here = |visit: bool| {
                let (mut cat, problems) = asset_catalog::answer_current(visit, own)?;
                let me = cat.self_id.clone();
                cat.machines.retain(|id, _| *id == me);
                Ok((cat, problems))
            };
            ext::answer_list(&req, &here, &[]).map_err(|f| typed::of_fail(f, CatalogErr::IoFailed))
        }
    }
}
