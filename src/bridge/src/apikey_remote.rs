//! 〔RM1a · 第四波〕上游选择那份凭据文件**按机器**读写 —— monitor 侧的分派与发送口。
//!
//! # 它补的是哪一格
//!
//! 设置里三条 apikey 命令（读状态 · 配 key · 问「这几个号在表里有没有行」）先前**只答本机**，
//! 而远端账号页调的也是它们 ⇒ 远端建的 apikey 号，key 落在**本机**那份表里，远端会话用不上
//! （`设计/70 §13.2` 第三条）。今天三条都收 `origin`：
//!
//! | origin | 谁写那份文件 | 谁读（界面状态 / 行） |
//! |---|---|---|
//! | 本机 | **本机常驻后端**（帧面 `apikey-key-set`，与远端同一条路）| monitor 自己（`creds_store::read_status` · 起会话那一侧同步读行）|
//! | 某台远端 | 那台机器的后端（帧面 `apikey-key-set` / `apikey-read`，`src/backend/accounts/upstream/file_face.rs`）| 同左（`apikey-read`）|
//!
//! ⇒ **每台机器上这份文件的程序写者恰好一个 ＝ 那台的后端**（主会话 09-25 裁；`调研/第四波记录/GP1.md §3`）。
//! 〔墓碑 —— RM1a 那一版本机那一臂进 `creds_store::write_key`〔散文墓碑〕、「从不把 `apikey-key-set` 发给本机后端」，
//!  判据 `the_local_arm_never_sends_the_key_to_a_backend`〔散文墓碑〕；RM1a / RL1 两次把「本机也走本机后端」交主会话，
//!  顾虑是 `CCM_DATA_DIR` 隔离跑时会写穿到真 profile。〕
//!
//! # 🔴 本机那一臂先核「写的是不是我这一份」
//!
//! 常驻后端按**家目录**认（`local_backend_host::listen_port_for` · token 在 `~/.cc-monitor`），**不按数据目录** ⇒
//! 一个 `CCM_DATA_DIR` 隔离跑的 monitor 会接上真 profile 那个 monitor 起的常驻后端，而那个后端的凭据路径
//! （起它时交的 `CCM_APIKEY_CREDENTIALS`）指的是**真数据目录**。直接交出去就写穿了。
//! ⇒ 发 `apikey-key-set` 之前先问一次 `apikey-read`：它回的 `path` 必须 == 本 monitor 的 `creds_store::resolve_path()`，
//! 不等 / 问不到 ⇒ **拒写**、两个路径都说出来。那个后端的路径由它起时的环境定死、进程一辈子不变 ⇒ 先核后写没有窗。
//!
//! # 归属（判清全文 `调研/第四波记录/RM1a.md §1`）
//!
//! 那份文件是**上游选择自己的状态**，不是用户文件 ⇒ 它不走文件管理那一面，也不走 SFTP
//! （`creds_core` crate 头注：远端那一侧**不许从 SFTP 的 mode 参数拿机密性**）。
//!
//! # 明文走哪
//!
//! 界面 → tauri 入参（IPC 那一跳，`K-H2a` 起就登记为「判不了」，原样）→ 装进 `apikey-key-set` 的 `args.key`，
//! 经那台机器那条长连接的入方向送过去（〔GP1〕本机 ＝ 本机常驻后端那条，与远端同一条路）。
//! **不进日志、不进报错文案、不进任何结构体字段**；逐跳由 `creds_store_tests::PLAINTEXT_HOPS` 钉着。
//!
//! # 它**不**做什么
//!
//! 不起中转、不问中转（那是中转的事，有它自己的命令）；不推账号 id 的第二份规则
//! （直接调 `history::apikey_account_id_of_dir`，起会话那一侧用的是同一个）。

use crate::backend::control::backend_route::{no_channel, route_call_error, Routed};
use crate::backend::control::inbound_client;
use crate::copy_table::copy_text;
use crate::creds_store::ApikeyCredentialsStatus;
use crate::origin::{Origin, Route};
use serde_json::{json, Value};

/// 后端那两条命令的名字（与 `src/backend/inbound.rs::REGISTRY` 同名，跨半边由判据现抠对拍）。
pub(crate) const CMD_KEY_SET: &str = "apikey-key-set";
pub(crate) const CMD_READ: &str = "apikey-read";

/// 一趟往返的上限。远端要走一趟长连接，给宽一点（同 `backend_policy::EXIT_POLICY_BUDGET`）。
const BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// 配一把 key（与可选的 Base URL）：**两臂同一条路** —— 交那台机器的后端（本机 ＝ 本机常驻后端）。
///
/// ⚠ 明文 `key` 在本函数里**恰好两处**，一臂一处，逐处登记在 `creds_store_tests::PLAINTEXT_HOPS`。
/// 本机那一臂多交一格「那份文件应当在哪」（`creds_store::resolve_path()`）：后端答的路径不是它 ⇒ 不写（本模块头注）。
/// 〔ST2 × RM1a〕`base_url` 跟着 key 走同一台机器：缺席 / 空 = 不碰那一格；形状关只在**写者**那一侧
/// （后端 `file_face` 调 `creds_core` 那一个函数），本函数不另判一遍。
pub(crate) async fn write_key_on(
    origin: &Origin,
    config_dir: &str,
    key: String,
    base_url: Option<String>,
) -> Result<(), String> {
    match origin.route("write_apikey_credentials_key")? {
        Route::Local => send_key(LOCAL, config_dir, key, base_url, Some(local_file()?)).await,
        Route::Remote(host) => send_key(host, config_dir, key, base_url, None).await,
    }
}

/// 本机那条长连接在客户端登记表里的名字（与 `backend_policy` 等同族一样，本机就是一个具名 origin）。
const LOCAL: &str = crate::backend::control::inbound_client::LOCAL_ORIGIN;

/// 本 monitor 认的那份本机凭据文件（`CCM_DATA_DIR` 隔离跑时跟着它走）。说不出 ⇒ 不写（不猜一个路径让后端去写）。
fn local_file() -> Result<std::path::PathBuf, String> {
    crate::creds_store::resolve_path()
        .ok_or_else(|| copy_text("rsApikeyRemote.local.noDataDir", &[]))
}

/// 推账号 id（全仓唯一那份规则）→（本机：先核那台后端写的就是 `same_file`）→ 装进 `args.key` → 交那台机器的后端。
///
/// ⚠ 明文绑定在这里叫 `plain`（不叫 `key`）：`args` 里那个字段名逐字是 `"key"`，
/// 同名的话「明文被碰了几次」那把按标识符数的尺子会把字段名也数进去。
/// ⚠ 顺序承重：说不出账号 id ⇒ **一次都不问**就拒（先于任何往返）；核路径那一问（`apikey-read`）不带明文。
pub(crate) async fn send_key(
    host: &str,
    config_dir: &str,
    plain: String,
    base_url: Option<String>,
    same_file: Option<std::path::PathBuf>,
) -> Result<(), String> {
    let account = crate::history::apikey_account_id_of_dir(config_dir).ok_or_else(|| {
        copy_text(
            "rsApikeyRemote.send.noAccount",
            &[
                ("dir", &format!("{:?}", config_dir)),
                ("host", &host.to_string()),
            ],
        )
    })?;
    if let Some(want) = same_file {
        let got = path_from_wire(host, &call(host, CMD_READ, json!({})).await?)?;
        if got != want {
            return Err(copy_text(
                "rsApikeyRemote.send.notSameFile",
                &[
                    ("host", &host.to_string()),
                    ("got", &got.display().to_string()),
                    ("want", &want.display().to_string()),
                ],
            ));
        }
    }
    call(
        host,
        CMD_KEY_SET,
        json!({ "account": account, "key": plain, "baseUrl": base_url }),
    )
    .await?;
    Ok(())
}

/// `apikey-read` 的应答里那份文件的路径（缺 / 不是字符串 ⇒ 报错：说不出写哪一份就不写）。
fn path_from_wire(host: &str, d: &Value) -> Result<std::path::PathBuf, String> {
    d.get("path")
        .and_then(Value::as_str)
        .map(std::path::PathBuf::from)
        .ok_or_else(|| copy_text("rsApikeyRemote.read.noPath", &[("host", &host.to_string())]))
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
    let bad = |what: &str| {
        copy_text(
            "rsApikeyRemote.wire.badShape",
            &[("host", &host.to_string()), ("what", &what.to_string())],
        )
    };
    let opt_str = |k: &str| -> Result<Option<String>, String> {
        match d.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(bad(&copy_text(
                "rsApikeyRemote.wire.notString",
                &[("k", &k.to_string())],
            ))),
        }
    };
    Ok(ApikeyCredentialsStatus {
        configured: d
            .get("configured")
            .and_then(Value::as_bool)
            .ok_or_else(|| bad(&copy_text("rsApikeyRemote.wire.noConfigured", &[])))?,
        masked: opt_str("masked")?
            .ok_or_else(|| bad(&copy_text("rsApikeyRemote.wire.noMasked", &[])))?,
        path: opt_str("path")?.ok_or_else(|| bad(&copy_text("rsApikeyRemote.wire.noPath", &[])))?,
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
            copy_text(
                "rsApikeyRemote.wire.rowsNotArray",
                &[("host", &host.to_string())],
            )
        })
}

/// 发送口（形状照 `backend_policy::exit_policy_call`）：没通道 / 旧后端不认 / 调用失败，各说各的话。
async fn call(host: &str, cmd: &str, args: Value) -> Result<Value, String> {
    let Some(client) = inbound_client::client_for(host) else {
        return Err(said(no_channel(host)));
    };
    if !client.accepts(cmd) {
        return Err(copy_text(
            "rsApikeyRemote.call.tooOld",
            &[("host", &host.to_string())],
        ));
    }
    let data = client.call(cmd, args, BUDGET).await.map_err(|e| {
        said(route_call_error(&e, |_code, message| {
            copy_text(
                "rsApikeyRemote.call.failed",
                &[
                    ("host", &host.to_string()),
                    ("message", &message.to_string()),
                ],
            )
        }))
    })?;
    data.ok_or_else(|| copy_text("rsApikeyRemote.call.noData", &[("host", &host.to_string())]))
}

/// 三态里给人看的那句话（同 `backend_policy::said`）。`Done` 在本族走不到。
fn said(r: Routed) -> String {
    match r {
        Routed::NoChannel(s) | Routed::Refused(s) => s,
        Routed::Done => copy_text("rsApikeyRemote.call.internal", &[]),
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/apikey_remote_tests.rs"]
mod tests;
