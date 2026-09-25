//! 〔RM1a · 第四波〕中转**按机器**：那台机器上的 `--relay` 进程在不在 · 起一个 —— monitor 侧的分派与发送口。
//!
//! # 两台机器，中转各住哪
//!
//! | origin | 中转谁起、谁答「在不在」 |
//! |---|---|
//! | 本机 | 〔RL1 · V107〕**本机常驻后端进程里**（monitor 起后端时交端口，`local_backend_host::relay_host_envs`）；「在不在」走起会话那一侧**同一条缝**（`history::inject_facts` → 回环上连一次那个口）|
//! | 某台远端 | 那台机器的后端：帧面 `relay-status`（口上有没有人在听）· `relay-ensure`（没有就起一个脱离的）|
//!
//! ⇒ monitor **从不**对本机发 `relay-ensure`：本机那一个住在常驻后端里，第二个去抢口只会让后端里那一个
//! 下次起不来（判据：`the_local_arm_never_starts_a_second_relay`）。
//! ⚠ 远端**不**并进远端后端：远端后端随 SSH 退，远端会话活在 tmux 里、比 SSH 长 ⇒ 远端中转必须是脱离的那一个
//! （`调研/第四波记录/RL1.md §1.2`）。
//!
//! # 🔴 本模块一个上游选择的名字都没有
//!
//! 只有端口（[`crate::backend::control::payload::RELAY_PORT`]，注入侧与中转侧用同一个值）。
//! 凭据文件在哪、表里有哪几行，是 `apikey_remote.rs` 的事；两个模块互不引用。
//!
//! # 触发点〔RL1〕：**起远端会话、且要注入那一刻**（用到才起）
//!
//! 远端那台的中转只在「这次拉起要往 `ANTHROPIC_BASE_URL` 里写地址」时才起：`history::relay_endpoint_on`
//! 先按「假如在跑」问一次判断口，要注入才经本模块 `listening_or_started`（在不在 → 起 → 有界等）。
//! 不挂在「长连接握手完成」上：那会让从不用中转的远端也常驻一个进程。
//! RM1a 那条单独暴露给界面的 tauri `relay_ensure(origin)`（零调用方）随之退役。

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

/// 那台机器上的中转在不在（两台都是「那个口上有没有人在听」）。本机：缝里那个取值口；远端：`relay-status`。
pub(crate) async fn running_on(origin: &Origin) -> Result<bool, String> {
    match origin.route("relay_running_on")? {
        Route::Local => Ok((crate::history::inject_facts().running)()),
        Route::Remote(host) => {
            listening_from_wire(host, &call(host, CMD_STATUS, port_args()).await?)
        }
    }
}

/// 让那台机器上有一个中转在跑。**本机拒**（本机那一个住在本机常驻后端里）。
pub(crate) async fn ensure_on(origin: &Origin) -> Result<RelayEnsured, String> {
    match origin.route("relay_ensure")? {
        Route::Local => Err(LOCAL_HAS_ITS_OWN.to_string()),
        Route::Remote(host) => ensured_from_wire(host, &call(host, CMD_ENSURE, port_args()).await?),
    }
}

/// 本机那一臂的说法：本机的中转住在本机常驻后端里，不经这条路起。
pub(crate) const LOCAL_HAS_ITS_OWN: &str =
    "本机的中转住在本机后端进程里（随它起、随它按「退出行为」留或退），不在这里另起一个";

// ═════════════════════════════════════════════════════════════════════════════
// 〔RL1〕远端「用到才起」的中转那一半：在不在 → 起 → 有界地等（组装在 `history::relay_endpoint_on`）
// ═════════════════════════════════════════════════════════════════════════════

/// 远端「中转在不在」那一问的等法：`relay-ensure` 起了进程却不等它 bind（后端零定时器）⇒ 这里**有界地**再问几次。
/// 等的是**一次性条件**（那个口起没起来），上限 `ENSURE_WAIT_TRIES × ENSURE_WAIT_INTERVAL`（≈ 2 s：
/// 回环 bind 是毫秒级，远端多一趟 SSH 往返）；等不到就如实答「没在听」，不无限重试。
/// 登记住 `rust_timer_registry`（`wait-for-condition`）。
const ENSURE_WAIT_TRIES: u32 = 10;
const ENSURE_WAIT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

/// 那台后端说「口上没人，也没起进程」时的那句为什么（`relay-ensure` 回 `started:false`）。
pub(crate) const RELAY_NOT_STARTED: &str = "那台的后端没有起中转（口上没人在听，它也没起进程）";
/// 起了进程、有界地等完了口上仍没人时的那句为什么。
pub(crate) const RELAY_NEVER_LISTENED: &str =
    "那台的后端起了一个中转，但等了一会儿它还没在那个口上听";

/// 那台机器的回环口上有没有人在听；没有就起一个、有界地等它。**只对远端**（本机那一个住在常驻后端里）。
/// `Ok(())` = 在听；`Err(为什么)` = 不在（包括问不到：那台后端太旧 / 没通道）。
pub(crate) async fn listening_or_started(origin: &Origin) -> Result<(), String> {
    if running_on(origin).await? {
        return Ok(());
    }
    let e = ensure_on(origin).await?;
    if e.listening {
        return Ok(());
    }
    if !e.started {
        return Err(RELAY_NOT_STARTED.to_string());
    }
    for _ in 0..ENSURE_WAIT_TRIES {
        tokio::time::sleep(ENSURE_WAIT_INTERVAL).await;
        if running_on(origin).await? {
            return Ok(());
        }
    }
    Err(RELAY_NEVER_LISTENED.to_string())
}

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
