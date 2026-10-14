//! 帧面 `assets-sync`（typed）：本机常驻后端沿池里那条 SSH 同步资产目录，本体在 `assets/asset_sync.rs`。
//! 真异步（拨号 / 等远端）、要写自家状态（资产目录的写口 `answer_merge` 由这里递进去，`asset_sync.rs` 自己不直呼它）⇒ 异步 ＋ 写句柄那一档。

use crate::assets::asset_catalog;
use crate::assets::asset_sync::{self, AssetsSyncReq, AssetsSynced, Fold};
use crate::wire::typed::{self, codes, AsyncOwn, Command, Cx};

codes! {
    /// `assets-sync` 的码。
    pub(crate) enum AssetsSyncErr {
        /// 开头那一次本机现扫 · 并没成。
        IoFailed = "io_failed",
        /// 只给 `origin`（界面直问）而可达表里还没有那一台。
        Unreachable = "unreachable",
    }
}

/// `assets-sync`：拉远端的目录、并进本机、把远端缺的推过去。
pub(crate) struct AssetsSyncCmd;

impl Command for AssetsSyncCmd {
    const NAME: &'static str = "assets-sync";
    type Req = AssetsSyncReq;
    type Reply = AssetsSynced;
    type Err = AssetsSyncErr;
}

impl AsyncOwn for AssetsSyncCmd {
    fn run(
        req: AssetsSyncReq,
        _cx: Cx,
        own: crate::platform::Own,
    ) -> impl std::future::Future<Output = Result<AssetsSynced, AssetsSyncErr>> + Send + 'static
    {
        async move {
            let fold: Fold = std::sync::Arc::new(move |m| asset_catalog::answer_merge(m, &own));
            asset_sync::answer(&req, fold, &crate::dial::remote_ask::DialRemote)
                .await
                .map_err(typed::of_pair(AssetsSyncErr::IoFailed))
        }
    }
}
