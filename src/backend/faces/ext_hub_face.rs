//! 两台之间「装」那一件的枢纽那一对（typed）：`ext-hub-preview` / `ext-hub-apply`，界面只问本机一次（本体在 `assets/hub.rs`）。
//! 问这台自己的那一跳（`typed::Hub`）由命令表那一行递进来（同文件管理面那一份）；写句柄由分派那一层递。
//! 不与内层那 8 条同住：引用图按文件取，这一对够得着的（可达表 · 拨号）不该算到内层头上。

use crate::assets::hub_wire as wire;
use crate::wire::spec::Fail;
use crate::wire::typed::{self, codes, Command};

codes! {
    /// `ext-hub-preview` / `ext-hub-apply` 的码（各台的失败不在这里：进那一台的 `error`）。
    pub(crate) enum ExtHubErr {
        /// 表里没有这一行。
        Missing = "missing",
        /// 这台的资产目录读不出来。
        CatalogUnreadable = "catalog_unreadable",
        /// 盘上这一步没成。
        IoFailed = "io_failed",
    }
}

/// `ext-hub-preview`：装到勾上的几台之前那一张卡，本机后端当枢纽向各台问完并好。
pub(crate) struct ExtHubPreview;

impl Command for ExtHubPreview {
    const NAME: &'static str = "ext-hub-preview";
    type Req = wire::HubReq;
    type Reply = wire::HubPreview;
    type Err = ExtHubErr;
}

impl crate::wire::typed::Handle for ExtHubPreview {
    type Needs = crate::wire::typed::needs::OwnHub;
    fn run(
        req: wire::HubReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::HubPreview, ExtHubErr>> + Send + 'static
    {
        let (cx, needs) = cx.split();
        let (own, hub) = needs;

        async move {
            let current = |visit| crate::assets::asset_catalog::answer_current(visit, &own);
            crate::assets::hub::ext_preview_many(
                &hub(&cx.tz, &own),
                &req,
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
                &current,
            )
            .await
            .map_err(|p| typed::of_fail(Fail::from(p), ExtHubErr::IoFailed))
        }
    }
}

/// `ext-hub-apply`：装到勾上的几台，各台各自结局。
pub(crate) struct ExtHubApply;

impl Command for ExtHubApply {
    const NAME: &'static str = "ext-hub-apply";
    type Req = wire::HubApplyReq;
    type Reply = wire::HubApplied;
    type Err = ExtHubErr;
}

impl crate::wire::typed::Handle for ExtHubApply {
    type Needs = crate::wire::typed::needs::OwnHub;
    fn run(
        req: wire::HubApplyReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::HubApplied, ExtHubErr>> + Send + 'static
    {
        let (cx, needs) = cx.split();
        let (own, hub) = needs;

        async move {
            let current = |visit| crate::assets::asset_catalog::answer_current(visit, &own);
            crate::assets::hub::ext_apply_many(
                &hub(&cx.tz, &own),
                &req,
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
                &current,
            )
            .await
            .map_err(|p| typed::of_fail(Fail::from(p), ExtHubErr::IoFailed))
        }
    }
}
