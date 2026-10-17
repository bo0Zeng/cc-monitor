//! 传输台的 **monitor 这一侧：只剩中继**（用户「SFTP 进本机常驻后端，只写暂存区」）。
//!
//! # 它今天是什么
//!
//! 窗口进程经通道说 `call(origin, "transfer-upload" | "transfer-download", …)` 开单、`subscribe(origin, "transfer/<id>")`
//! 起跑并看进度、停订即撤（**一个字没变**）。那一面的宿主 `chan/host.rs` 把这两条交到这里；
//! 这里把它们**原样转给本机常驻后端**（`transfer-upload` / `-download` / `-start` / `-stop`，后端 `control/transfer.rs`），
//! 再把后端推上来的 `transfer` 帧（经本机那条流的吸收点，[`deliver`]）翻成窗口认的那几格（[`Snap`]）。
//!
//! # 🔴 没有退路（`D11`）
//!
//! 本机后端那条流不在 ⇒ 报「本机后端不在」；它不认 `transfer-*` ⇒ 报「太旧」。**不进程内开 SFTP**（`inproc_dial.rs` 那条路
//! 不再为传输服务）。
//!
//! # 撤与收场
//!
//! - 停订（流被丢）⇒ 发 `transfer-stop`（后端：上传删暂存件、下载留 `.part`）；
//! - 本机那条流断了 ⇒ 后端的票表随它一起丢、在册的一律撤；这一侧开在那条流上的中继一律收场（`failed`，原因说清），
//!   不让看的人干等（[`fail_owned_by`]，两条本机读循环结束时各调一次，同 `link_mux::fail_owned_by`）。

use crate::copy_table::copy_text;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use tokio::sync::watch;

use crate::backend_route::{route_call_error, Routed};
use crate::inbound_client::InboundClient;
use crate::stream_source::RemoteConfig;

/// 开单：上传。载荷 `cmd_wire::transfer::TransferUploadAsk`，回 `TransferOpened`（逐项；上传那几项带 `staged` ＝ 暂存件的键，提交时交给远端后端）。
pub const TRANSFER_UPLOAD: &str =
    <cmd_wire::transfer::TransferUploadAsk as cmd_wire::files::Call>::NAME;
/// 开单：下载。载荷 `cmd_wire::transfer::TransferDownloadAsk`，回 `TransferOpened`。
pub const TRANSFER_DOWNLOAD: &str =
    <cmd_wire::transfer::TransferDownloadAsk as cmd_wire::files::Call>::NAME;
/// 🔴 **窗口经通道说得出的传输命令：恰好这两条。** 取消不是命令（停订即撤）。
pub const TRANSFER_OPS: [&str; 2] = [TRANSFER_UPLOAD, TRANSFER_DOWNLOAD];
/// 两台之间复制一组。载荷 `cmd_wire::transfer::TransferCopyAsk`（寻址 ＝ 源那台，`to` ＝ 落点那台的名字），回 `BatchReply`。
/// 不开单：本机后端做完整组才回，进度走 `files_batch` 帧（[`crate::files_batch_relay`]），撤是 `files-batch-stop`。
pub const TRANSFER_COPY: &str =
    <cmd_wire::transfer::TransferCopyAsk as cmd_wire::files::Call>::NAME;
/// 进度流的 `kind`：`transfer/<id>`。
pub const TRANSFER_KIND_PREFIX: &str = "transfer/";

/// 起跑（后端命令名；窗口那一侧没有这条 —— 订阅即起跑）。
const TRANSFER_START: &str = "transfer-start";
/// 撤（后端命令名；窗口那一侧没有这条 —— 停订即撤）。
const TRANSFER_STOP: &str = "transfer-stop";

/// 开单 / 起跑 / 撤这几条等本机后端应答的上限。它们都是后端**就地记账**的命令（不等拨号、不等传输），
/// 本机那条流上毫秒级回；给 60 s 与 `dial_host::LINK_CALL_BUDGET` 同值（流被别的大应答占着时也不误判）。
const CALL_BUDGET: Duration = Duration::from_secs(60);

/// 这个操作名是不是传输台的。
pub fn is_transfer_op(op: &str) -> bool {
    TRANSFER_OPS.contains(&op)
}

/// 一趟传输**此刻**的样子（流里的每一格都是一整份快照，不是增量 ⇒ 合并掉中间几格不丢信息）。
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Snap {
    pub got: u64,
    pub total: u64,
    /// `Some` ⇒ 收场了（流的最后一格）。
    pub end: Option<End>,
}

/// 怎么收场的（传输台的那一形，`transfer.end`）。
pub use cmd_wire::frame::TransferEnd as End;

/// 壳这一侧收场的失败（流断了 · 开跑那一问没成）：只有那一句，详情由窗口进程自己写。
pub(crate) fn failed(why: String) -> End {
    End::Failed {
        why,
        code: None,
        detail: None,
    }
}

/// 一趟在本机后端册上的传输，在 monitor 这一侧的影子。
struct Relay {
    origin: crate::origin::Origin,
    /// 开它的那条本机流（断了就收场；撤的时候经它发 `transfer-stop`）。
    owner: Weak<InboundClient>,
    state: Arc<watch::Sender<Snap>>,
    watched: bool,
}

fn relays() -> std::sync::MutexGuard<'static, HashMap<String, Relay>> {
    static R: std::sync::OnceLock<Mutex<HashMap<String, Relay>>> = std::sync::OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// 本机后端答「不行」：码 · 那一句 · 它带的 `data`（`exists` 时是同名清单，窗口照它画冲突框）。
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    pub code: String,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl Refusal {
    fn of(code: &str, message: String) -> Refusal {
        Refusal {
            code: code.to_string(),
            message,
            data: None,
        }
    }

    /// 一次调用没成：码与那一句照 [`said`]；`data` 从分层那一份拒绝体里取（不在这里匹配错误枚举）。
    fn from_call(e: &crate::inbound_client::CallError) -> Refusal {
        let (code, message) = said(e);
        let (_, body) =
            crate::chan::wire::err_to_wire(crate::backend_route::layer_call_error(e, 0).error);
        let data = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("data").cloned());
        Refusal {
            code,
            message,
            data,
        }
    }
}

/// 本机后端回的失败 ⇒ `(码, 话)`。**分层判定不在这里**：走分流器（`backend_route::route_call_error`，全仓唯一
/// match 那个错误枚举的地方）；后端自己说的码（`busy` / `refused` / `bad_args` …）原样带回，窗口按它画那一行。
fn said(e: &crate::inbound_client::CallError) -> (String, String) {
    let code = std::cell::RefCell::new(String::from("backend"));
    let text = match route_call_error(e, &copy_core::local_machine(), |c, m| {
        *code.borrow_mut() = c.to_string();
        m.to_string()
    }) {
        Routed::NoChannel(s) | Routed::Refused(s) => s,
    };
    (code.into_inner(), text)
}

/// **开单**那一面（宿主 `call` 调）：窗口那一份按类型解（`cmd_wire::transfer::*Ask`，多一个键就拒）、补上那台远端的拨号请求，
/// 按后端那一份类型（`*Req`）转给本机后端，回包按类型解、登记一个中继。回应答的 JSON。
///
/// 失败回 [`Refusal`]（码 · 话 · 后端带的 `data`），宿主原样翻成「对端答不行」（与后端命令的拒绝同一个信封）。
pub async fn transfer_call(
    cfg: RemoteConfig,
    op: &str,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, Refusal> {
    use cmd_wire::transfer::{TransferDownloadAsk, TransferUploadAsk};
    match op {
        // `home`（可缺席）：窗口问那台后端拿到的 `$HOME`，传输台连上之后与 SFTP 起始目录比。
        TRANSFER_UPLOAD => {
            let ask: TransferUploadAsk = ask_of(op, payload)?;
            relay(&cfg, ask.dialed(crate::dial_host::transfer_dial(&cfg))).await
        }
        // 这里原来先判一次本机落点是不是 Claude 会话数据（开单时出声早）。用户「文件管理器全部都可以改.
        //   不需要任何围栏」⇒ 删了；落点的路径解析（绝对路径 · 父目录在盘上）在本机常驻后端 `transfer-download` 开单那一判。
        // 下载的本机落点是原始字节（字符串或 `{"b16": …}`：有损名在 Linux 上按原始字节落名），原样转给本机后端判。
        TRANSFER_DOWNLOAD => {
            let ask: TransferDownloadAsk = ask_of(op, payload)?;
            relay(&cfg, ask.dialed(crate::dial_host::transfer_dial(&cfg))).await
        }
        other => Err(Refusal::of(
            "bad_args",
            copy_text(
                "rsSftpPool.call.notTransfer",
                &[("other", &other.to_string())],
            ),
        )),
    }
}

/// **两台之间复制一组**（宿主 `call` 调）：窗口那一份按类型解、补上两台的拨号请求（源 ＝ `cfg`，落点 ＝ `to_cfg`），
/// 转给本机后端；它做完整组才回 ⇒ 等的上限是窗口给的 `left`。回包按类型解（`BatchReply`）原样回。
pub async fn copy_call(
    cfg: RemoteConfig,
    to_cfg: RemoteConfig,
    payload: &serde_json::Value,
    left: Duration,
) -> Result<serde_json::Value, Refusal> {
    let ask: cmd_wire::transfer::TransferCopyAsk = ask_of(TRANSFER_COPY, payload)?;
    let from = crate::dial_host::transfer_dial(&cfg);
    let to = crate::dial_host::transfer_dial(&to_cfg);
    let req = ask.dialed(from, to);
    let args = serde_json::to_value(&req).map_err(|e| Refusal::of("bad_args", e.to_string()))?;
    let client = crate::dial_host::local_backend_accepting(TRANSFER_COPY)
        .await
        .map_err(|e| Refusal::of("backend_unavailable", e))?;
    let v = client
        .call(TRANSFER_COPY, args, left)
        .await
        .map_err(|e| Refusal::from_call(&e))?
        .unwrap_or(serde_json::Value::Null);
    let reply: cmd_wire::batch::BatchReply = serde_json::from_value(v.clone()).map_err(|_| {
        Refusal::of(
            "backend",
            copy_text("rsSftpPool.call.noTicket", &[("v", &v.to_string())]),
        )
    })?;
    serde_json::to_value(&reply).map_err(|e| Refusal::of("backend", e.to_string()))
}

/// 窗口那一份载荷按类型解（缺一格 · 多一格 · 型不对 ⇒ `bad_args`）。解码器那一句不上句子（原话不进话里），记一行日志。
fn ask_of<A: serde::de::DeserializeOwned>(
    op: &str,
    payload: &serde_json::Value,
) -> Result<A, Refusal> {
    match serde_json::from_value(payload.clone()) {
        Ok(a) => Ok(a),
        Err(e) => {
            tracing::warn!("传输：窗口说的 {op} 载荷解不出来（{e}）");
            Err(Refusal::of(
                "bad_args",
                copy_text("rsSftpPool.call.badArgs", &[("op", &op.to_string())]),
            ))
        }
    }
}

/// 按后端那一份类型转给本机后端，回包按类型解（解不出 ⇒ 没回传输编号），登记一个中继。
async fn relay<R: cmd_wire::files::Call<Reply = cmd_wire::transfer::TransferOpened>>(
    cfg: &RemoteConfig,
    req: R,
) -> Result<serde_json::Value, Refusal> {
    let args = serde_json::to_value(&req).map_err(|e| Refusal::of("bad_args", e.to_string()))?;
    let client = crate::dial_host::local_backend_accepting(R::NAME)
        .await
        .map_err(|e| Refusal::of("backend_unavailable", e))?;
    let v = client
        .call(R::NAME, args, CALL_BUDGET)
        .await
        .map_err(|e| Refusal::from_call(&e))?
        .unwrap_or(serde_json::Value::Null);
    let opened: R::Reply = serde_json::from_value(v.clone()).map_err(|_| {
        Refusal::of(
            "backend",
            copy_text("rsSftpPool.call.noTicket", &[("v", &v.to_string())]),
        )
    })?;
    // 一组里开了单的每一项各登记一个中继（没开成 · 跳过的那几项没有票）。
    for id in opened.items.iter().filter_map(|it| it.id.as_deref()) {
        let (tx, _rx) = watch::channel::<Snap>(Snap::default());
        relays().insert(
            id.to_string(),
            Relay {
                origin: crate::origin::Origin(cfg.origin_label()),
                owner: Arc::downgrade(&client),
                state: Arc::new(tx),
                watched: false,
            },
        );
    }
    serde_json::to_value(&opened).map_err(|e| Refusal::of("backend", e.to_string()))
}

/// 订阅那一格的守卫：**流被丢掉 = 停订**。没收场就经开它的那条流发 `transfer-stop`；无论如何把中继摘掉 ——
/// 只摘**自己那一份**：本机后端重起之后票号从头数，同一个号可能已经是新开的那一趟（旧流的守卫晚一步才落）。
struct WatchGuard {
    id: String,
    state: Arc<watch::Sender<Snap>>,
    owner: Weak<InboundClient>,
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        {
            let mut g = relays();
            if g.get(&self.id)
                .is_some_and(|r| Arc::ptr_eq(&r.state, &self.state))
            {
                g.remove(&self.id);
            }
        }
        if self.state.borrow().end.is_some() {
            return;
        }
        let (Some(client), Ok(rt)) = (self.owner.upgrade(), tokio::runtime::Handle::try_current())
        else {
            return;
        };
        let id = self.id.clone();
        rt.spawn(async move {
            if let Err(e) = client
                .call(TRANSFER_STOP, serde_json::json!({ "id": id }), CALL_BUDGET)
                .await
            {
                tracing::warn!("传输：撤 {id} 没送到本机后端（{}）", said(&e).1);
            }
        });
    }
}

/// **起跑并看**：`id` 那一趟的快照流（第一格是此刻，之后每变一次出一格，收场那一格是最后一格）。
///
/// - 没有这张票 / 票不是这台机器的 ⇒ `Err(("no-such-transfer", …))`（宿主原位说出来）；
/// - 已经有人在看（第二次订阅）⇒ `Err(("already-watched", …))` —— 一趟传输一个看的人，撤的语义才不含糊。
///
/// ⚠ 必须在 tokio 运行时里调（起跑是一次 `spawn`）；宿主的订阅口恒在路由器的任务里。
pub fn watch_ticket(
    origin: &crate::origin::Origin,
    id: &str,
) -> Result<futures::stream::BoxStream<'static, Snap>, (&'static str, String)> {
    let (state, owner) = {
        let mut g = relays();
        let Some(r) = g.get_mut(id).filter(|r| r.origin == *origin) else {
            return Err((
                "no-such-transfer",
                copy_text("rsSftpPool.watch.unknown", &[("id", &id.to_string())]),
            ));
        };
        if r.watched {
            return Err((
                "already-watched",
                copy_text("rsSftpPool.watch.taken", &[("id", &id.to_string())]),
            ));
        }
        r.watched = true;
        (Arc::clone(&r.state), r.owner.clone())
    };
    let rx = state.subscribe();
    // ── 起跑 ──
    {
        let state = Arc::clone(&state);
        let owner = owner.clone();
        let id = id.to_string();
        tokio::spawn(async move {
            let Some(client) = owner.upgrade() else {
                state.send_modify(|s| {
                    s.end = Some(failed(copy_text("reason.local.linkLost", &[])))
                });
                return;
            };
            if let Err(e) = client
                .call(TRANSFER_START, serde_json::json!({ "id": id }), CALL_BUDGET)
                .await
            {
                let why = said(&e).1;
                state.send_modify(|s| s.end = Some(failed(why)));
            }
        });
    }
    // ── 看 ──
    let guard = WatchGuard {
        id: id.to_string(),
        state,
        owner,
    };
    Ok(Box::pin(futures::stream::unfold(
        (rx, guard, true, false),
        |(mut rx, guard, first, over)| async move {
            if over {
                return None;
            }
            if !first && rx.changed().await.is_err() {
                // 发送端在守卫里 ⇒ 这一支到不了；到了就当它收场了。
                let snap = Snap {
                    end: Some(failed(copy_text("rsSftpPool.watch.lost", &[]))),
                    ..Snap::default()
                };
                return Some((snap, (rx, guard, false, true)));
            }
            let snap = rx.borrow_and_update().clone();
            let over = snap.end.is_some();
            Some((snap, (rx, guard, false, over)))
        },
    )))
}

/// 本机那条流上来了一帧 `transfer`（吸收点调）。**从不阻塞**：只改那一格的快照（`watch` 合并）。
/// 不在册的 id（早就停订了）⇒ 丢掉。
pub(crate) fn deliver(id: &str, got: u64, total: u64, end: Option<End>) {
    if let Some(r) = relays().get(id) {
        r.state.send_modify(|s| {
            s.got = got;
            s.total = total;
            if end.is_some() {
                s.end = end;
            }
        });
    }
}

/// 本机那条流断了：经它开的中继一律收场（后端那一侧的票随那条流一起撤了）。只结束**经它开的**
/// （本机后端重连之后，新流上开的不许被旧流的收尾带走 —— 同 `link_mux::fail_owned_by`）。
pub(crate) fn fail_owned_by(client: &Arc<InboundClient>, why: &str) {
    for r in relays().values() {
        if r.owner.upgrade().is_some_and(|o| Arc::ptr_eq(&o, client)) || r.owner.strong_count() == 0
        {
            r.state.send_modify(|s| {
                if s.end.is_none() {
                    s.end = Some(failed(why.to_string()));
                }
            });
        }
    }
}

/// 判据 ＋ 台架（假本机后端）。`pub(crate)`：通道那一侧的判据（`chan/transfer_stream_tests`）借同一台台架。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/sftp_pool_tests.rs"]
pub(crate) mod tests;
