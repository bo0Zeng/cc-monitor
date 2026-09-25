//! 〔RM1a · 第四波〕中转（层 1）**按机器**：那台机器上的 `--relay` 进程在不在 · 起一个 —— monitor 侧的分派与发送口。
//!
//! # 两台机器，两个监护者
//!
//! | origin | 中转谁起、谁答「在不在」 |
//! |---|---|
//! | 本机 | monitor 自己：起本机后端那一刻顺手监护一个（`local_backend_host::start_local_relay`）；「在不在」走起会话那一侧**同一条缝**（`history::inject_facts`）|
//! | 某台远端 | 那台机器的后端：帧面 `relay-status`（口上有没有人在听）· `relay-ensure`（没有就起一个脱离的）|
//!
//! ⇒ monitor **从不**对本机发 `relay-ensure`：本机那一个有监护者，第二个去抢口只会让监护者的那一个
//! 起不来、三次之后放弃 —— 那之后本机的 api-key 号起会话全被拒（判据：`the_local_arm_never_starts_a_second_relay`）。
//!
//! # 🔴 本模块一个账号层的名字都没有
//!
//! 只有端口（[`crate::backend::control::payload::RELAY_PORT`]，注入侧与中转侧用同一个值）。
//! 凭据文件在哪、表里有哪几行，是 `apikey_remote.rs` 的事；两个模块互不引用。
//!
//! # 它今天**没有**自动触发点（如实登记，交主会话）
//!
//! 本机那一个的触发点是「起本机后端那一刻」。远端的同位点是「那台机器的长连接握手完成那一刻」，
//! 或「起远端会话、且要注入那一刻」—— 两处都不在本拍写区（链路宿主 / 起会话那一侧）。
//! ⇒ 本拍交的是**能力**：tauri 命令 `relay_ensure(origin)` ＋ 读口 [`running_on`]（设置里的 `apikey_routing_for` 在用）。

use crate::backend::control::backend_route::{no_channel, route_call_error, Routed};
use crate::backend::control::inbound_client;
use crate::origin::{Origin, Route};
use serde_json::{json, Value};

/// 后端那两条命令的名字（与 `src/backend/inbound.rs::REGISTRY` 同名，跨半边由判据现抠对拍）。
pub(crate) const CMD_STATUS: &str = "relay-status";
pub(crate) const CMD_ENSURE: &str = "relay-ensure";

/// 一趟往返的上限（同 `apikey_remote::BUDGET` 的理由：远端要走一趟长连接）。
const BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// `relay-ensure` 的结局（原样转给界面）。
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RelayEnsured {
    /// 起之前那一刻口上有没有人。
    pub(crate) listening: bool,
    /// 这一趟起了一个进程（**不等它 bind**：要知道口上有没有人，再问一次 [`running_on`]）。
    pub(crate) started: bool,
}

/// 那台机器上的中转在不在。本机：monitor 起过它而且没停过（缝里那个取值口）；远端：那个口上有没有人在听。
pub(crate) async fn running_on(origin: &Origin) -> Result<bool, String> {
    match origin.route("relay_running_on")? {
        Route::Local => Ok((crate::history::inject_facts().running)()),
        Route::Remote(host) => {
            listening_from_wire(host, &call(host, CMD_STATUS, port_args()).await?)
        }
    }
}

/// 让那台机器上有一个中转在跑。**本机拒**（本机那一个由 monitor 监护）。
pub(crate) async fn ensure_on(origin: &Origin) -> Result<RelayEnsured, String> {
    match origin.route("relay_ensure")? {
        Route::Local => Err(LOCAL_HAS_ITS_OWN.to_string()),
        Route::Remote(host) => ensured_from_wire(host, &call(host, CMD_ENSURE, port_args()).await?),
    }
}

/// 本机那一臂的说法：本机的中转有监护者，不经这条路起。
pub(crate) const LOCAL_HAS_ITS_OWN: &str =
    "本机的中转由 monitor 自己看着（起本机后端时一起起），不在这里另起一个";

/// 两条命令共用的入参：只有端口。
fn port_args() -> Value {
    json!({ "port": crate::backend::control::payload::RELAY_PORT })
}

/// `relay-status` 的应答 → 口上有没有人。
pub(crate) fn listening_from_wire(host: &str, d: &Value) -> Result<bool, String> {
    d.get("listening").and_then(Value::as_bool).ok_or_else(|| {
        format!("[{host}] `{CMD_STATUS}` 的应答里没有布尔 `listening` —— 两端契约对不上")
    })
}

/// `relay-ensure` 的应答 → 结局。
pub(crate) fn ensured_from_wire(host: &str, d: &Value) -> Result<RelayEnsured, String> {
    let flag = |k: &str| {
        d.get(k).and_then(Value::as_bool).ok_or_else(|| {
            format!("[{host}] `{CMD_ENSURE}` 的应答里没有布尔 `{k}` —— 两端契约对不上")
        })
    };
    Ok(RelayEnsured {
        listening: flag("listening")?,
        started: flag("started")?,
    })
}

/// 发送口（形状照 `apikey_remote::call`）。
async fn call(host: &str, cmd: &str, args: Value) -> Result<Value, String> {
    let Some(client) = inbound_client::client_for(host) else {
        return Err(said(no_channel(host)));
    };
    if !client.accepts(cmd) {
        return Err(format!(
            "[{host}] 的后端还不认 `{cmd}` —— 远端也起中转之后才有这条命令，重装那台机器的后端就有了"
        ));
    }
    let data = client.call(cmd, args, BUDGET).await.map_err(|e| {
        said(route_call_error(&e, |code, message| {
            format!("[{host}] `{cmd}` 失败（{code}）：{message}")
        }))
    })?;
    data.ok_or_else(|| format!("[{host}] `{cmd}` 的应答没有 data —— 两端契约对不上"))
}

/// 三态里给人看的那句话。`Done` 在本族走不到。
fn said(r: Routed) -> String {
    match r {
        Routed::NoChannel(s) | Routed::Refused(s) => s,
        Routed::Done => "问中转时出了内部错误，没有拿到结果".to_string(),
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/remote_relay_tests.rs"]
mod tests;
