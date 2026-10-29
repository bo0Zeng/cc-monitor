//! **资产目录同步** —— monitor 这一侧：**只交事实，零判定**。
//!
//! # 要求（逐字）
//!
//! 「比如本机后端在本机看见一个skill并记录下来, 就会和远端后端同步, 这样远端后端也能在远端装skill或者mcp」·
//! 「目录自动同步，装要你点」。
//!
//! # 它做的那一件事
//!
//! **连上那一刻**（[`on_remote_ready`]，`stream_source/` 在远端那条流握手成功时调）：那台的后端认得资产目录
//! ⇒ 把那台的**名字**交给本机常驻后端 `assets-sync`，由**它**照自己的机器表组请求、沿池里那条 SSH
//! 拉 / 并 / 推（`src/backend/assets/asset_sync.rs`）。后台跑，不挡收帧。
//!
//! 怎么够到那台只看本机后端的机器表：monitor 不交拨号；**合并、推什么、扇不扇出，一条都不在这里**。
//! 目录本身的读（`assets-catalog`）前端经通道直接问那台（`chan.call`），不经本模块。
//!
//! # 买不到
//!
//! - 🔴 真远端：本模块的判据只到「交出去的是什么」「答回来的认不认得」；本机后端那一趟在后端判据里用替身对面验。
//! - 本机后端不在 ⇒ 报（`D11`，不回落）；连上那一刻它不在 ⇒ 这一次就没同步（下次连上 / 看机器页再来），只记一行 warn。

use cmd_wire::assets::{AssetsSyncReq, AssetsSynced};
use cmd_wire::Call;
use copy_core::copy_text;
use std::time::Duration;

use crate::backend_route::{route_call_error, Routed};

/// 远端那台要认得的命令（它不认 ⇒ 那台后端太旧，连上时不去碰它）。
pub(crate) const REMOTE_NEEDS: &str = "assets-catalog-merge";

/// 一趟的上限：拉 ＋ 推（每台几条 exec）＋ 扇出到其余各台。给足余量同时防卡死（后端零定时器，期限归调用方）。
const BUDGET: Duration = Duration::from_secs(180);

/// 本机后端那一跳（生产 = 本机常驻后端那条流；判据用替身）。
pub(crate) trait LocalBackend {
    async fn call(&self, req: &AssetsSyncReq) -> Result<AssetsSynced, String>;
}

pub(crate) struct ResidentBackend;

impl LocalBackend for ResidentBackend {
    async fn call(&self, req: &AssetsSyncReq) -> Result<AssetsSynced, String> {
        let client = crate::inbound_client::local_backend_accepting(AssetsSyncReq::NAME).await?;
        let deadline = tokio::time::Instant::now() + BUDGET;
        let got =
            client.ask(None, req, None, deadline).await.map_err(|e| {
                match route_call_error(&e, &copy_core::local_machine(), |_code, message| {
                    copy_text("rsAssetSync.call.failed", &[("said", &message.to_string())])
                }) {
                    Routed::NoChannel(s) | Routed::Refused(s) => s,
                }
            })?;
        got.map_err(|_| copy_core::reply_unreadable(&copy_core::local_machine()))
    }
}

/// 可测的那一半：那一台（流握手那一刻）→ 入参（只交那台的名字，怎么够到它由本机后端照机器表组）→ 本机后端；
/// 回包交回（只记日志，monitor 不解释它）。
pub(crate) async fn sync_with(
    origin: &str,
    local: &impl LocalBackend,
) -> Result<AssetsSynced, String> {
    local
        .call(&AssetsSyncReq {
            origin: Some(cmd_wire::Machine::named(origin)),
        })
        .await
}

/// 远端那条流握手成功那一刻（本机后端转来那台的 hello，`stream_source/local.rs::attached`）：那台认得资产目录 ⇒ 后台让本机后端对它做一趟同步；
/// 不认 ⇒ 不碰（老后端不认一次性子命令会进流模式）。
pub(crate) fn on_remote_ready(origin: &str, remote_accepts: bool) {
    let label = origin.to_string();
    if !remote_accepts {
        tracing::info!("资产目录：[{label}] 的后端不认 `{REMOTE_NEEDS}`，这次不同步");
        return;
    }
    tauri::async_runtime::spawn(async move {
        match sync_with(&label, &ResidentBackend).await {
            Ok(r) => tracing::info!("资产目录：[{label}] 连上即同步 {:?}", r.synced),
            Err(e) => tracing::warn!("资产目录：[{label}] 连上即同步没办成：{e}"),
        }
    });
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/asset_sync_tests.rs"]
mod tests;
