//! 〔RM1a · 第四波〕账号层那份凭据文件**按机器**读写 —— monitor 侧的分派与发送口。
//!
//! # 它补的是哪一格
//!
//! 设置里三条 apikey 命令（读状态 · 配 key · 问「这几个号在表里有没有行」）先前**只答本机**，
//! 而远端账号页调的也是它们 ⇒ 远端建的 apikey 号，key 落在**本机**那份表里，远端会话用不上
//! （`设计/70 §13.2` 第三条）。今天三条都收 `origin`：
//!
//! | origin | 谁读写那份文件 |
//! |---|---|
//! | 本机 | monitor 自己（`creds_store`，既有、一个字节没动）|
//! | 某台远端 | 那台机器的后端（帧面 `apikey-key-set` / `apikey-read`，`src/backend/accounts/apikey/file_face.rs`）|
//!
//! ⇒ **每台机器上这份文件的程序写者恰好一个**。本机那一臂**只进 `creds_store`**，从不把
//! `apikey-key-set` 发给本机后端（判据：`the_local_arm_never_sends_the_key_to_a_backend`）。
//!
//! # 归属（判清全文 `调研/第四波记录/RM1a.md §1`）
//!
//! 那份文件是**账号层自己的状态**，不是用户文件 ⇒ 它不走文件管理那一面，也不走 SFTP
//! （`creds_core` crate 头注：远端那一侧**不许从 SFTP 的 mode 参数拿机密性**）。
//!
//! # 明文走哪
//!
//! 界面 → tauri 入参（IPC 那一跳，`K-H2a` 起就登记为「判不了」，原样）→ 本机：`creds_store::write_key`；
//! 远端：装进 `apikey-key-set` 的 `args.key`，经那台机器那条长连接的入方向送过去。
//! **不进日志、不进报错文案、不进任何结构体字段**；逐跳由 `creds_store_tests::PLAINTEXT_HOPS` 钉着。
//!
//! # 它**不**做什么
//!
//! 不起中转、不问中转（那是层 1 的事，有它自己的命令）；不推账号 id 的第二份规则
//! （直接调 `history::apikey_account_id_of_dir`，起会话那一侧用的是同一个）。

use crate::backend::control::backend_route::{no_channel, route_call_error, Routed};
use crate::backend::control::inbound_client;
use crate::creds_store::ApikeyCredentialsStatus;
use crate::origin::{Origin, Route};
use serde_json::{json, Value};

/// 后端那两条命令的名字（与 `src/backend/inbound.rs::REGISTRY` 同名，跨半边由判据现抠对拍）。
pub(crate) const CMD_KEY_SET: &str = "apikey-key-set";
pub(crate) const CMD_READ: &str = "apikey-read";

/// 一趟往返的上限。远端要走一趟长连接，给宽一点（同 `backend_policy::EXIT_POLICY_BUDGET`）。
const BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// 配一把 key（与可选的 Base URL）：本机进 `creds_store`，远端交那台机器的后端。
///
/// ⚠ 明文 `key` 在本函数里**恰好两处**，一臂一处（两台机器、两个写者），逐处登记在
/// `creds_store_tests::PLAINTEXT_HOPS`。
/// 〔ST2 × RM1a〕`base_url` 跟着 key 走同一台机器：缺席 / 空 = 不碰那一格；形状关只在**写者**那一侧
/// （本机 `creds_store` · 远端后端 `file_face`，两处调的是 `creds_core` 同一个函数），本函数不另判一遍。
pub(crate) async fn write_key_on(
    origin: &Origin,
    config_dir: &str,
    key: String,
    base_url: Option<String>,
) -> Result<(), String> {
    match origin.route("write_apikey_credentials_key")? {
        Route::Local => crate::creds_store::write_key(config_dir, &key, base_url.as_deref()),
        Route::Remote(host) => send_key(host, config_dir, key, base_url).await,
    }
}

/// 远端那一臂：推账号 id（全仓唯一那份规则）→ 装进 `args.key` → 交那台机器的后端。
///
/// ⚠ 明文绑定在这里叫 `plain`（不叫 `key`）：`args` 里那个字段名逐字是 `"key"`，
/// 同名的话「明文被碰了几次」那把按标识符数的尺子会把字段名也数进去。
async fn send_key(
    host: &str,
    config_dir: &str,
    plain: String,
    base_url: Option<String>,
) -> Result<(), String> {
    let account = crate::history::apikey_account_id_of_dir(config_dir).ok_or_else(|| {
        format!(
            "说不出这是哪个账号（configDir 是 {config_dir:?}）—— 不往 [{host}] 的表里写：\
             写进去的那一行谁也命中不了"
        )
    })?;
    call(
        host,
        CMD_KEY_SET,
        json!({ "account": account, "key": plain, "baseUrl": base_url }),
    )
    .await?;
    Ok(())
}

/// 读那台机器上那份文件的状态（只回掩码）。
pub(crate) async fn status_on(origin: &Origin) -> Result<ApikeyCredentialsStatus, String> {
    match origin.route("read_apikey_credentials_status")? {
        Route::Local => crate::creds_store::read_status(),
        Route::Remote(host) => status_from_wire(host, &call(host, CMD_READ, json!({})).await?),
    }
}

/// 那台机器的表里有哪几条账号 id。本机走起会话那一侧**同一条缝**（`history::inject_facts`）——
/// 那个取值口只许经那条缝被够到（`payload_tests` 数着），绕过去就是第二个没人数得出来的调用点。
pub(crate) async fn rows_on(origin: &Origin) -> Result<Vec<String>, String> {
    match origin.route("apikey_routing_for")? {
        Route::Local => Ok((crate::history::inject_facts().rows)()),
        Route::Remote(host) => rows_from_wire(host, &call(host, CMD_READ, json!({})).await?),
    }
}

/// `apikey-read` 的应答 → 界面那份状态。形状不对 ⇒ 报错（**不许**退化成「没配」）。
pub(crate) fn status_from_wire(host: &str, d: &Value) -> Result<ApikeyCredentialsStatus, String> {
    let bad =
        |what: &str| format!("[{host}] `{CMD_READ}` 的应答形状不对：{what} —— 两端契约对不上");
    let opt_str = |k: &str| -> Result<Option<String>, String> {
        match d.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(bad(&format!("`{k}` 不是字符串"))),
        }
    };
    Ok(ApikeyCredentialsStatus {
        configured: d
            .get("configured")
            .and_then(Value::as_bool)
            .ok_or_else(|| bad("缺 `configured`"))?,
        masked: opt_str("masked")?.ok_or_else(|| bad("缺 `masked`"))?,
        path: opt_str("path")?.ok_or_else(|| bad("缺 `path`"))?,
        notice: opt_str("notice")?,
        problem: opt_str("problem")?,
    })
}

/// `apikey-read` 的应答 → 行 id 表。
pub(crate) fn rows_from_wire(host: &str, d: &Value) -> Result<Vec<String>, String> {
    d.get("rows")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter()
                .map(|v| v.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| {
            format!("[{host}] `{CMD_READ}` 的应答里 `rows` 不是字符串数组 —— 两端契约对不上")
        })
}

/// 发送口（形状照 `backend_policy::exit_policy_call`）：没通道 / 旧后端不认 / 调用失败，各说各的话。
async fn call(host: &str, cmd: &str, args: Value) -> Result<Value, String> {
    let Some(client) = inbound_client::client_for(host) else {
        return Err(said(no_channel(host)));
    };
    if !client.accepts(cmd) {
        return Err(format!(
            "[{host}] 的后端还不认 `{cmd}` —— 账号层那份凭据文件按机器读写之后才有这条命令，\
             重装那台机器的后端就有了"
        ));
    }
    let data = client.call(cmd, args, BUDGET).await.map_err(|e| {
        said(route_call_error(&e, |code, message| {
            format!("[{host}] `{cmd}` 失败（{code}）：{message}")
        }))
    })?;
    data.ok_or_else(|| format!("[{host}] `{cmd}` 的应答没有 data —— 两端契约对不上"))
}

/// 三态里给人看的那句话（同 `backend_policy::said`）。`Done` 在本族走不到。
fn said(r: Routed) -> String {
    match r {
        Routed::NoChannel(s) | Routed::Refused(s) => s,
        Routed::Done => "读写第三方 API key 时出了内部错误，没有拿到结果".to_string(),
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/apikey_remote_tests.rs"]
mod tests;
