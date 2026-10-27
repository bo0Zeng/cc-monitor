//! 设置「扩展」页那几条（typed）：`ext-list`（那张表；读可达表认出每台的名字 ⇒ 只在流面上）· `ext-uninstall-preview` / `-apply`
//! （从这台卸一个扩展：判与写都在这台，写经命令表递进来的文件管理面，装记录的写口从这里递进去）。本体在 `assets/ext.rs`。

use crate::assets::asset_catalog;
use crate::assets::ext;
use crate::faces::catalog_face::CatalogErr;
use crate::wire::typed::{self, codes, Command};

/// `ext-list`：这台现扫一次、记下（资产目录的写口从这里递进去），各台目录合成「条目 × 机器」；表里有 MCP 行 ⇒
/// 各台那一格 `mcp` 由这台问到再交（[`with_mcp`]：本机就地读，别的台经常驻流问 `mcp-read`），界面只问这一条。
/// 异步档：扫目录与本机那一份 `mcp-read` 进阻塞池，问各台在异步那一侧等。
pub(crate) struct ExtListCmd;

impl Command for ExtListCmd {
    const NAME: &'static str = "ext-list";
    type Req = ext::ExtListReq;
    type Reply = ext::ExtList;
    type Err = CatalogErr;
}

impl crate::wire::typed::Handle for ExtListCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: ext::ExtListReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ext::ExtList, CatalogErr>> + Send + 'static {
        let (cx, needs) = cx.split();
        let (until, tz) = (cx.until, cx.tz);
        async move {
            let list = tokio::task::spawn_blocking(move || {
                let own = &needs;
                ext::answer_list(
                    &req,
                    &|visit| asset_catalog::answer_current(visit, own),
                    &ext::reach_of(&crate::dial::remote_ask::REACH),
                )
            })
            .await
            .map_err(|_| CatalogErr::IoFailed(String::new()))?
            .map_err(|f| typed::of_fail(f, CatalogErr::IoFailed))?;
            let here = move || async move {
                let read = tokio::task::spawn_blocking(move || {
                    crate::faces::feature_face::mcp_read_here(&Default::default(), &tz)
                })
                .await
                .ok()?
                .map_err(|e| tracing::warn!("ext-list: 这台的 mcp-read 没成：{e:?}"))
                .ok()?;
                serde_json::to_value(read).ok()
            };
            let there = move |m: String| async move {
                crate::control::feeds::call_passing::<crate::faces::feature_face::McpReadCmd>(
                    &m,
                    &Default::default(),
                    until,
                )
                .await
                .map_err(|f| tracing::warn!("ext-list: mcp-read [{m}]: {}", f.message))
                .ok()
            };
            Ok(with_mcp(list, here, there).await)
        }
    }
}

/// [`ExtListCmd`] 填各台那一格 `mcp` 的那一层（这台怎么读 · 怎么问到那台由调用方给，判据喂假的）：
/// 表里没有 MCP 行 ⇒ 谁都不问；有 ⇒ 这台（`here`）就地读，别的够得着的台（有键）同时问，问不到的那一格留空。
pub(crate) async fn with_mcp<H, HF, T, TF>(mut list: ext::ExtList, here: H, there: T) -> ext::ExtList
where
    H: FnOnce() -> HF,
    HF: std::future::Future<Output = Option<serde_json::Value>>,
    T: Fn(String) -> TF,
    TF: std::future::Future<Output = Option<serde_json::Value>> + Send + 'static,
{
    if !list
        .rows
        .iter()
        .any(|r| r.kind == asset_catalog::ExtKind::Mcp)
    {
        return list;
    }
    let mut asks = tokio::task::JoinSet::new();
    for (i, m) in list.machines.iter().enumerate() {
        if let (false, true, Some(key)) = (m.here, m.reachable, m.key.clone()) {
            let ask = there(key);
            asks.spawn(async move { (i, ask.await) });
        }
    }
    let mine = if list.machines.iter().any(|m| m.here) {
        here().await
    } else {
        None
    };
    for m in list.machines.iter_mut().filter(|m| m.here) {
        m.mcp = mine.clone();
    }
    while let Some(got) = asks.join_next().await {
        if let Ok((i, Some(v))) = got {
            list.machines[i].mcp = Some(v);
        }
    }
    list
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

impl crate::wire::typed::Handle for ExtUninstallPreviewCmd {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: ext::ExtUninstallReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ext::ExtUninstallCard, UninstallPreviewErr>>
           + Send
           + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let files = needs;

            ext::answer_uninstall_preview(files, &ext::Env::here(), &req)
                .map_err(|f| typed::of_fail(f, UninstallPreviewErr::IoFailed))
        }
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

impl crate::wire::typed::Handle for ExtUninstallApplyCmd {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: ext::ExtUninstallApplyReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ext::ExtDone, UninstallApplyErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let (files, own) = (needs.0, &needs.1);

            ext::answer_uninstall_apply(
                files,
                &ext::Env::here(),
                &|a| crate::assets::skill_ledger::answer_record(a, own),
                &req,
            )
            .map_err(|f| typed::of_fail(f, UninstallApplyErr::IoFailed))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/ext_mcp_tests.rs"]
mod ext_mcp_tests;
