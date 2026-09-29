//! 〔AS2 · 第四波 4B · V113〕**资产目录同步** —— monitor 这一侧：**只交事实，零判定**。
//!
//! # 用户裁决（2026-09-25，`99 §1` V113，逐字）
//!
//! 「比如本机后端在本机看见一个skill并记录下来, 就会和远端后端同步, 这样远端后端也能在远端装skill或者mcp」·
//! 「目录自动同步，装要你点」。
//!
//! # 它做的两件事
//!
//! 1. **连上那一刻**（[`on_remote_ready`]，`ssh_source.rs` 在远端那条流握手成功时调）：那台的后端认得资产目录
//!    ⇒ 把「怎么够到那台」（拨号请求 ＋ 那台后端的路径）交给本机常驻后端 `assets-sync`，由**它**沿池里那条 SSH
//!    拉 / 并 / 推（`src/backend/assets/asset_sync.rs`）。后台跑，不挡收帧。
//! 2. **界面看机器页前**：〔MIG-3a〕界面经通道直问本机后端 `assets-sync`（只报 `origin`，够到那台用握手时登记的那一行，
//!    `src/frontend/ui/assets-sync-reads.ts`）；这里那条 Tauri 命令〔散文墓碑〕删了。
//!
//! 拨号请求今天仍由 monitor 造（读 ssh 配置那一半归 MIG-1 进本机后端，`99 §2.1 ⑯`）；**合并、推什么、扇不扇出，一条都不在这里**。
//! 目录本身的读（`assets-catalog`）前端经通道直接问那台（`chan.call`），不经本模块。
//!
//! # 买不到
//!
//! - 🔴 真远端：本模块的判据只到「交出去的是什么」「答回来的认不认得」；本机后端那一趟在后端判据里用替身对面验。
//! - 本机后端不在 ⇒ 报（`D11`，不回落）；连上那一刻它不在 ⇒ 这一次就没同步（下次连上 / 看机器页再来），只记一行 warn。

use crate::copy_table::copy_text;
use std::time::Duration;

use serde_json::{json, Value};

use crate::backend_route::{route_call_error, Routed};

/// 本机后端那条命令的名字（与 `src/backend/stream/inbound.rs::REGISTRY` 同名，判据现抠对拍）。
pub(crate) const CMD: &str = "assets-sync";

/// 远端那台要认得的命令（它不认 ⇒ 那台后端太旧，连上时不去碰它）。
pub(crate) const REMOTE_NEEDS: &str = "assets-catalog-merge";

/// 一趟的上限：拉 ＋ 推（每台几条 exec）＋ 扇出到其余各台。给足余量同时防卡死（后端零定时器，期限归调用方）。
const BUDGET: Duration = Duration::from_secs(180);

// 〔MIG-3a〕`AssetsSynced` 那三个形状与 `parse_reply`〔散文墓碑〕随 Tauri 命令 `assets_sync` 删了：界面经通道直问本机后端、
//   按形状收（`src/frontend/ui/assets-sync-reads.ts`）；这里只剩流握手那一刻交事实（`on_remote_ready`），应答原样记日志。

/// 一台远端的入参：`origin` ＋ 拨号请求（`capture` 用法；后端会再钉一遍）。〔E2〕那台后端的路径不交：落点恒是 `relay_route_core::BACKEND_LANDING_SHELL`。
pub(crate) fn args_for(cfg: &crate::ssh_source::RemoteConfig) -> Result<Value, String> {
    let dial = crate::dial_host::request(cfg, "capture", json!({}))?;
    Ok(json!({
        "origin": cfg.origin_label(),
        "dial": dial,
    }))
}

/// 本机后端那一跳（生产 = 本机常驻后端那条流；判据用替身）。
pub(crate) trait LocalBackend {
    async fn call(&self, args: Value) -> Result<Value, String>;
}

pub(crate) struct ResidentBackend;

impl LocalBackend for ResidentBackend {
    async fn call(&self, args: Value) -> Result<Value, String> {
        let client = crate::dial_host::local_backend_accepting(CMD).await?;
        let data = client.call(CMD, args, BUDGET).await.map_err(|e| {
            match route_call_error(&e, |_code, message| {
                copy_text(
                    "rsAssetSync.call.failed",
                    &[("message", &message.to_string())],
                )
            }) {
                Routed::NoChannel(s) | Routed::Refused(s) => s,
                Routed::Done => copy_text("rsAssetSync.call.internal", &[]),
            }
        })?;
        data.ok_or_else(|| copy_text("rsAssetSync.call.emptyReply", &[]))
    }
}

/// 可测的那一半：那一台（流握手那一刻）→ 入参 → 本机后端；应答原样交回（只记日志，monitor 不解释它）。
pub(crate) async fn sync_with(
    target: &crate::ssh_source::RemoteConfig,
    local: &impl LocalBackend,
) -> Result<Value, String> {
    local.call(args_for(target)?).await
}

/// 〔C4d · 第四波 4B〕本机后端**可达表登记**那条命令（与 `src/backend/stream/inbound.rs::REGISTRY` 同名，判据现抠对拍）。
///
/// 「本机后端问远端后端」那一跳（后端 `remote_ask.rs`）有两路在用：资产目录同步（本模块）· 历史跨机 join（后端 `history_join.rs`）。
/// 前者只在那台认 `assets-catalog-merge` 时才交 `assets-sync`（它顺手登记）；后者问的是老子命令，**老远端也得够得着**
/// ⇒ 每台远端流握手那一刻**无条件**交一次「怎么够到那台」（与 `assets-sync` 同一份入参），只登记、不拨号。
pub(crate) const REACH_CMD: &str = "remote-reach";

/// 登记一次的期限：纯内存（后端那一侧一把锁、插一行），给得很短。
const REACH_BUDGET: Duration = Duration::from_secs(10);

/// 把「怎么够到那台」登记进本机后端的可达表（本机后端太旧不认 ⇒ 报，调用方只记一行 warn）。
async fn register_reach(cfg: &crate::ssh_source::RemoteConfig) -> Result<(), String> {
    let args = args_for(cfg)?;
    let client = crate::dial_host::local_backend_accepting(REACH_CMD).await?;
    client
        .call(REACH_CMD, args, REACH_BUDGET)
        .await
        .map(|_| ())
        .map_err(|e| {
            match route_call_error(&e, |_code, message| {
                copy_text(
                    "rsAssetSync.reach.said",
                    &[("message", &message.to_string())],
                )
            }) {
                Routed::NoChannel(s) | Routed::Refused(s) => s,
                Routed::Done => copy_text("rsAssetSync.reach.internal", &[]),
            }
        })
}

/// 远端那条流握手成功那一刻（`ssh_source.rs::stream_loop`）：
/// ①〔C4d〕**无条件**把「怎么够到那台」登记进本机后端的可达表（历史跨机 join 要它，老远端也要）；
/// ② 那台认得资产目录 ⇒ 后台让本机后端对它做一趟同步；不认 ⇒ 不碰（老后端不认一次性子命令会进流模式）。
pub(crate) fn on_remote_ready(cfg: &crate::ssh_source::RemoteConfig, remote_accepts: bool) {
    let label = cfg.origin_label();
    {
        let cfg = cfg.clone();
        let label = label.clone();
        tauri::async_runtime::spawn(async move {
            match register_reach(&cfg).await {
                Ok(()) => tracing::info!("可达表：[{label}] 已登记进本机后端"),
                Err(e) => tracing::warn!("可达表：[{label}] 没登记上（历史清单问不到这台）：{e}"),
            }
        });
    }
    if !remote_accepts {
        tracing::info!("资产目录：[{label}] 的后端不认 `{REMOTE_NEEDS}`，这次不同步");
        return;
    }
    let cfg = cfg.clone();
    tauri::async_runtime::spawn(async move {
        match sync_with(&cfg, &ResidentBackend).await {
            Ok(r) => tracing::info!("资产目录：[{label}] 连上即同步 {}", r["synced"]),
            Err(e) => tracing::warn!("资产目录：[{label}] 连上即同步没办成：{e}"),
        }
    });
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/asset_sync_tests.rs"]
mod tests;
