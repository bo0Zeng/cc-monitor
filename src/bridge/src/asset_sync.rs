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
//!    拉 / 并 / 推（`src/backend/asset_sync.rs`）。后台跑，不挡收帧。
//! 2. **界面看机器页前**（Tauri 命令 [`assets_sync`]）：同一条命令 —— 远端那一页交那台的拨号请求；本机那一页什么都不交
//!    （本机后端对它可达表里每一台各做一趟）。
//!
//! 拨号请求只有 monitor 造得出来（读配置是宿主的事，`dial_host.rs` 头注 `C4`）；**合并、推什么、扇不扇出，一条都不在这里**。
//! 目录本身的读（`assets-catalog`）前端经通道直接问那台（`chan.call`），不经本模块。
//!
//! # 买不到
//!
//! - 🔴 真远端：本模块的判据只到「交出去的是什么」「答回来的认不认得」；本机后端那一趟在后端判据里用替身对面验。
//! - 本机后端不在 ⇒ 报（`D11`，不回落）；连上那一刻它不在 ⇒ 这一次就没同步（下次连上 / 看机器页再来），只记一行 warn。

use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};

use crate::backend::control::backend_route::{route_call_error, Routed};
use crate::origin::{Origin, Route};

/// 本机后端那条命令的名字（与 `src/backend/inbound.rs::REGISTRY` 同名，判据现抠对拍）。
pub(crate) const CMD: &str = "assets-sync";

/// 远端那台要认得的命令（它不认 ⇒ 那台后端太旧，连上时不去碰它）。
pub(crate) const REMOTE_NEEDS: &str = "assets-catalog-merge";

/// 一趟的上限：拉 ＋ 推（每台几条 exec）＋ 扇出到其余各台。给足余量同时防卡死（后端零定时器，期限归调用方）。
const BUDGET: Duration = Duration::from_secs(180);

/// 一趟对一台的结局（后端答的，原样转给界面）。
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct AssetsSyncRow {
    pub origin: String,
    /// 那台目录的 id（没拉成 ⇒ `null`）。
    pub peer: Option<String>,
    /// 本机目录因这一趟变了没有。
    pub changed: bool,
    /// 推过去几台快照。量纲是「机器台数」（后端可达表 ≤ 256 台，一台一份快照）⇒ 远在 2^53 之下，`number` 装得下。
    #[cfg_attr(test, ts(type = "number"))]
    pub pushed: u64,
    /// 哪里没办成（`null` = 全办成了）。
    pub error: Option<String>,
}

/// 可达表的一行：origin ↔ 那台目录的 id。界面据它把目录里的机器 id 对回 origin（「装到这台」要从来源那台现读）。
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct AssetsReach {
    pub origin: String,
    pub machine: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct AssetsSynced {
    /// 本机目录的 id（界面据它把目录里本机那一格对回 `<local>`）。本机后端没答出来 ⇒ `null`。
    #[serde(rename = "self")]
    pub self_id: Option<String>,
    pub synced: Vec<AssetsSyncRow>,
    pub reach: Vec<AssetsReach>,
}

/// 对面后端的应答认不出来时给人的那句话（多半是两边版本不一样）。**不猜默认值**。
const UNREADABLE_REPLY: &str = "本机后端答的同步结果认不出来，多半是两边版本不一样。";

/// 后端应答 → 线上形状。契约对不上 ⇒ 报错，不猜。
pub(crate) fn parse_reply(v: &Value) -> Result<AssetsSynced, String> {
    let broken = || UNREADABLE_REPLY.to_string();
    let opt_str = |x: &Value, k: &str| -> Result<Option<String>, String> {
        match x.get(k) {
            Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            _ => Err(broken()),
        }
    };
    let synced = v
        .get("synced")
        .and_then(Value::as_array)
        .ok_or_else(broken)?
        .iter()
        .map(|r| {
            Ok(AssetsSyncRow {
                origin: r
                    .get("origin")
                    .and_then(Value::as_str)
                    .ok_or_else(broken)?
                    .to_string(),
                peer: opt_str(r, "peer")?,
                changed: r
                    .get("changed")
                    .and_then(Value::as_bool)
                    .ok_or_else(broken)?,
                pushed: r.get("pushed").and_then(Value::as_u64).ok_or_else(broken)?,
                error: opt_str(r, "error")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let reach = v
        .get("reach")
        .and_then(Value::as_array)
        .ok_or_else(broken)?
        .iter()
        .map(|r| {
            Ok(AssetsReach {
                origin: r
                    .get("origin")
                    .and_then(Value::as_str)
                    .ok_or_else(broken)?
                    .to_string(),
                machine: opt_str(r, "machine")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(AssetsSynced {
        self_id: opt_str(v, "self")?,
        synced,
        reach,
    })
}

/// 一台远端的入参：`origin` ＋ 拨号请求（`capture` 用法；后端会再钉一遍）＋ 那台后端的路径。
pub(crate) fn args_for(cfg: &crate::ssh_source::RemoteConfig) -> Result<Value, String> {
    let dial = crate::dial_host::request(cfg, "capture", json!({}))?;
    Ok(json!({
        "origin": cfg.origin_label(),
        "dial": dial,
        "backend": cfg.backend_path,
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
            match route_call_error(&e, |_code, message| format!("本机后端：{message}")) {
                Routed::NoChannel(s) | Routed::Refused(s) => s,
                Routed::Done => "本机后端同步资产目录时出了内部错误，没有拿到结果".to_string(),
            }
        })?;
        data.ok_or_else(|| format!("本机后端对 `{CMD}` 回了一条空应答"))
    }
}

/// 可测的那一半：哪台（`None` = 本机那一页）→ 入参 → 本机后端 → 解析。
pub(crate) async fn sync_with(
    target: Option<&crate::ssh_source::RemoteConfig>,
    local: &impl LocalBackend,
) -> Result<AssetsSynced, String> {
    let args = match target {
        None => json!({}),
        Some(cfg) => args_for(cfg)?,
    };
    parse_reply(&local.call(args).await?)
}

/// 界面看机器页前调：远端那一页 ⇒ 让本机后端对那一台做一趟；本机那一页 ⇒ 对它够得到的每一台各一趟。
#[tauri::command]
pub async fn assets_sync(origin: Origin) -> Result<AssetsSynced, String> {
    match origin.route("assets_sync")? {
        Route::Local => sync_with(None, &ResidentBackend).await,
        Route::Remote(host) => {
            let cfg = crate::remote_history::require_cfg_by_label(host)?;
            sync_with(Some(&cfg), &ResidentBackend).await
        }
    }
}

/// 〔C4d · 第四波 4B〕本机后端**可达表登记**那条命令（与 `src/backend/inbound.rs::REGISTRY` 同名，判据现抠对拍）。
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
        .map_err(
            |e| match route_call_error(&e, |_code, message| format!("本机后端：{message}")) {
                Routed::NoChannel(s) | Routed::Refused(s) => s,
                Routed::Done => "本机后端登记那台远端时出了内部错误".to_string(),
            },
        )
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
        match sync_with(Some(&cfg), &ResidentBackend).await {
            Ok(r) => tracing::info!("资产目录：[{label}] 连上即同步 {:?}", r.synced),
            Err(e) => tracing::warn!("资产目录：[{label}] 连上即同步没办成：{e}"),
        }
    });
}

#[cfg(test)]
#[path = "../../../tests/bridge/asset_sync_tests.rs"]
mod tests;
