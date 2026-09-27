//! 通道 · **宿主那一侧**（monitor 进程里）：绑回环 · 造钥匙 · `accept` · 把连接交给路由器 · 注入后端句柄。
//!
//! # 🔴 为什么这一份**不是**通信层成员（刻意的）
//!
//! 它做的正是 `C4` / `C5` 不许通信层做的那几件事：
//!
//! | 它做的 | 为什么归宿主 |
//! |---|---|
//! | 绑 `127.0.0.1:0` | `01 §2.1 C5` 逐字：「由**后端** `bind`/`listen`……把 `accept` 到的连接交给面 B」—— 面 A 这里照同一个先例 |
//! | 造钥匙（OS 随机源） | `C4`：凭据由后端交给通信层 |
//! | 定帧长上限、认证等待时长 | `C4`：配置与**期限值**都是策略值，归后端 |
//! | 把 `op` 翻成 `inbound_client` 的一条命令、把载荷当 JSON 读 | 那是后端那一半的实现 —— 路由器不许知道载荷长什么样（`C1`） |
//!
//! ⇒ 圈进来的话 `C4`/`C5` 当场破；**不圈它就是它的归属**，与 `relay/listen.rs` 那一份
//! 「语义上就该在外面」同形。
//!
//! # 🔴 「一个对外端口」仍然成立（`01 §4`）
//!
//! 本文件只绑**回环**（`Ipv4Addr::LOCALHOST`），端口由内核挑 ⇒ 不新开任何对外端口。
//! 判据里有一条直接看 [`start_with`] 交回来的地址是不是回环。
//!
//! # 🔴 钥匙怎么交接
//!
//! 1. monitor 起来时 [`start`] 造一把钥匙（两枚 v4 UUID 的 244 位 OS 随机数，写成 64 位十六进制）；
//! 2. [`handoff`] 把「地址 ＋ 钥匙 ＋ 帧长上限」交给要起外部前端的那一方；
//! 3. 那一方**照 `filewin/proc.rs` 的先例把它写进子进程的 stdin**（不走 argv —— `/proc/<pid>/cmdline`
//!    世界可读；不走环境变量 —— `/proc/<pid>/environ` 同用户可读、且会被孙进程继承）；
//! 4. 外部前端从 stdin 读到它，用 [`super::dial::dial`] 连上并出示钥匙。
//!
//! ✅〔F2 · 2026-09-24〕**第 3 步接上了**：`filewin/entry.rs` 取 [`handoff`]，`filewin/proc.rs` 把它连同
//! 开窗种子一起写进窗口进程的 stdin（`proc::OpenRequest::handoff`），窗口进程用 [`super::dial::dial`] 拨回来。
//! ⚠ **钥匙不进日志**：[`Handoff`] 与 `Key` 的 `Debug` 都手写成不打印内容；本文件的日志只印端口。
//!
//! # 买到什么
//!
//! - monitor 进程里有一个**只听回环、只认一把钥匙**的通道口，外部前端经它说 `call`。
//! - `call` 经注入的 [`InboundBackends`] 走**既有**的 `inbound_client`（本机与远端同一条路），
//!   一行新业务都没写。
//!
//! # 买不到什么
//!
//! - ✅〔F7c · 第三波 · 2026-09-24〕**生产上有了第一条流**：`transfer/<id>`（传输台那一趟的进度，
//!   `设计/60 §13.2 ①`；翻译住本文件末尾那一节，判据 `transfer_stream_tests` 走真回环 ＋ 真钥匙 ＋ 本句柄）。
//!   🔴 **其余 `kind` 照旧一条流都没有**：`inbound_client` 只有「一问一答」，后端推上来的帧今天走
//!   `ssh_source` 的 Tauri 事件那条路 ⇒ [`InboundBackends::subscribe`] 对别的 `kind` 仍原位回
//!   `Closed{Peer(…)}`（对端说「没有这条流」），**不装作订阅成功**。
//! - **对端撤活是尽力的**：外部前端撤单 ⇒ 路由器丢掉本 future（`router::run_call`）⇒ 那次 `inbound_client`
//!   调用随之被丢 ⇒ 它的 `AbandonGuard` 补发一条 `cancel{target}`（〔RM1f〕best-effort、不等应答；判据
//!   `inbound_client_tests::abandoning_the_wait_fires_one_cancel_and_finishing_fires_none`）。后端可取消档
//!   （`Run::Async`）真停下；阻塞档回 `not_cancellable`、照跑完。〔AR1 订正〕上一版写「补发 `cancel` 那一手是
//!   `inbound_client` 的私有函数，今天够不着 ⇒ 后端可能照跑完」—— RM1f 之后丢 future 就会补发，不用够着它。
//! - **不买同机其它用户的隔离之外的东西**：回环口上同一台机器的任何进程都能**连**，
//!   挡它们的只有那把钥匙；钥匙在子进程 stdin 管子里走一次，之后只在两边内存里。
//!   能读 monitor 进程内存的人（同用户 root / ptrace）本来就能直接驱动后端，不在本文件射程。
//! - **不买连接数上界**：没出示钥匙的连接最多挂 `hello_within` 那么久；出示了钥匙的连接不设上限。

use super::router::{self, Backends, Ended, Terms};
use super::wire::{
    Body, By, CallError, CancelToken, Cursor, Item, Key, Kind, Op, Origin, OursFault,
};
use crate::backend::control::{backend_route, inbound_client};
use crate::copy_table::copy_text;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// 外部前端连上通道要的全部东西：地址 ＋ 钥匙 ＋ 帧长上限。
///
/// 它整份过一次进程边界（走子进程的 stdin，见模块头注），所以能序列化。
#[derive(Clone, Serialize, Deserialize)]
pub struct Handoff {
    /// 回环上的那个口。
    pub addr: SocketAddr,
    /// 那把钥匙。
    pub key: Key,
    /// 帧头 / 帧体各自的字节上限（两端必须同一个数）。
    pub frame: usize,
}

impl std::fmt::Debug for Handoff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handoff")
            .field("addr", &self.addr)
            .field("key", &self.key)
            .field("frame", &self.frame)
            .finish()
    }
}

/// 造一把钥匙：两枚 v4 UUID（各 122 位来自 OS 随机源）拼成 64 位十六进制。
pub fn mint_key() -> Key {
    Key(format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    ))
}

/// 在回环上起一个通道口，把每条接进来的连接交给路由器。回交接件。
///
/// `frame` 与 `hello_within` 由调用方给（它们是策略值）。判据用它起一个挂着合成句柄的口。
///
/// # Errors
///
/// 回环口绑不上。
pub async fn start_with(
    backends: Arc<dyn Backends>,
    key: Key,
    frame: usize,
    hello_within: Duration,
) -> std::io::Result<Handoff> {
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let addr = listener.local_addr()?;
    let terms = Terms {
        key: key.clone(),
        frame,
        hello_within,
    };
    tokio::spawn(async move {
        loop {
            // `accept` 是内核事件，不是定时器；单次失败（fd 顶满之类）不许把整个口带走。
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("通道：accept 失败（{e}），这一条放过，口照开");
                    continue;
                }
            };
            let terms = terms.clone();
            let backends = Arc::clone(&backends);
            tokio::spawn(async move {
                match router::serve(stream, terms, backends).await {
                    Ended::Left => {}
                    Ended::Denied => tracing::warn!("通道：一条连接没过认证，已关"),
                    Ended::Broken(why) => tracing::warn!("通道：一条连接坏了，已关（{why}）"),
                }
            });
        }
    });
    Ok(Handoff { addr, key, frame })
}

/// 本进程那个通道口的交接件（[`start`] 成功之后才有）。
static HANDOFF: OnceLock<Handoff> = OnceLock::new();

/// **生产入口**：monitor 起来时调一次。绑回环、造钥匙、挂上 [`InboundBackends`]。
///
/// # Errors
///
/// 回环口绑不上 / 已经起过一次。
pub async fn start() -> Result<(), String> {
    let handoff = start_with(
        Arc::new(InboundBackends),
        mint_key(),
        // 帧头 / 帧体各自的上限：一屏目录（`filewin::source::LS_LIMIT` 条）的 JSON 在兆字节级，给 64 MiB。
        64 << 20,
        // 没出示钥匙的连接最多挂这么久。
        Duration::from_secs(5),
    )
    .await
    .map_err(|e| copy_text("rsChanHost.start.bindFailed", &[("e", &e.to_string())]))?;
    tracing::info!("通道：在 {} 上听（只认回环、只认一把钥匙）", handoff.addr);
    HANDOFF
        .set(handoff)
        .map_err(|_| copy_text("rsChanHost.start.twice", &[]))
}

/// 交给要起外部前端的那一方。`None` = 通道没起来 —— **不许**因此退回别的路（`D11`）。
pub fn handoff() -> Option<Handoff> {
    HANDOFF.get().cloned()
}

// ════════════════════════════════════════════════════════════════════════════
//  生产那个后端句柄：`call` 走既有的 `inbound_client`
// ════════════════════════════════════════════════════════════════════════════

/// 生产的后端句柄 —— `origin` ⇒ `inbound_client::client_for`（本机与远端同一条路）。
pub struct InboundBackends;

/// 这一跳在面 A 上的编号：路由器 ↔ 后端（`wire::HopId` 头注那两跳里的第 1 跳）。
const HOP: u8 = 1;

impl Backends for InboundBackends {
    fn call(
        &self,
        origin: Origin,
        op: Op,
        payload: Body,
        left: Duration,
        _cancel: CancelToken,
    ) -> BoxFuture<'static, Result<Body, CallError>> {
        // 🔴〔F7c · 第三波 09-24〕**传输台那两条开单命令不按 `origin` 去那台机器的后端**：
        //    〔SR1b〕传输台住**本机**常驻后端（SFTP 与其它 SSH 同一条连接），由中继 `sftp_pool.rs` 转过去；
        //    其余一切照旧按 `origin` 去 `inbound_client`。
        if crate::sftp_pool::is_transfer_op(&op.0) {
            return Box::pin(transfer_open(origin, op, payload));
        }
        // 撤单**不在这里接**：路由器在撤单手柄拨下时直接丢掉本 future（`router::run_call`），
        // `inbound_client` 那次调用随之被丢 —— 本句柄再接一次就是第二份「撤了怎么说」。
        Box::pin(async move {
            // 🔴 分层判定**不在本文件**：「那台机器没有控制通道」与 inbound 的每一种失败
            //    怎么分层、`reach` 是哪一档，唯一的家是 `backend_route` 的
            //    `layer_no_channel` / `layer_call_error`（它们与旧三态同出一源）。
            //    本文件只做搬运，不 match inbound 的错误枚举。
            let Some(client) = inbound_client::client_for(origin.as_wire_str()) else {
                return Err(backend_route::layer_no_channel(HOP));
            };
            // 这个后端说 JSON：载荷在这里（宿主这一侧）才第一次被当成 JSON 读。
            // 读不动是**调用方用错了**（`Ours{Misuse}`）—— 与 inbound 无关，不是分流判定。
            let Ok(args) = serde_json::from_slice::<serde_json::Value>(&payload.0) else {
                return Err(OursFault::Misuse.into());
            };
            match client.call(&op.0, args, left).await {
                Ok(v) => Ok(Body(
                    serde_json::to_vec(&v.unwrap_or(serde_json::Value::Null)).unwrap_or_default(),
                )),
                Err(e) => Err(backend_route::layer_call_error(&e, HOP).error),
            }
        })
    }

    fn subscribe(
        &self,
        origin: Origin,
        kind: Kind,
        from: Option<Cursor>,
    ) -> BoxStream<'static, Item> {
        // 🔴〔F7c · 第三波 09-24〕**生产上的第一条流**：`transfer/<id>` ⇒ 传输台那一趟的进度。
        if let Some(id) = kind.0.strip_prefix(crate::sftp_pool::TRANSFER_KIND_PREFIX) {
            return transfer_stream(&origin, id, from);
        }
        // 别的 `kind` 今天照旧没有流。对端原位说「没有这条流」，不装作订阅成功、也不静默挂着。
        let _ = (origin, from);
        let body = serde_json::to_vec(&serde_json::json!({
            "code": "no-such-stream",
            "kind": kind.0,
        }))
        .unwrap_or_default();
        Box::pin(futures::stream::iter([Item::Closed {
            by: By::Peer(Body(body)),
        }]))
    }

    /// 〔NET2〕那台的能力事实 = 它那条长连接握手时交出的那一份（`inbound_client` 登记表里，一个家）。
    fn offer(&self, origin: &Origin) -> Option<super::wire::Offer> {
        inbound_client::client_for(origin.as_wire_str()).map(|c| c.offer().clone())
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  〔F7c · 第三波 · 2026-09-24〕传输台那一口：开单 ＋ 进度流（`设计/60 §13.2 ①`）
// ════════════════════════════════════════════════════════════════════════════
//
// 本文件在这里做的仍然只是**宿主**那几件事：按 `origin` 找那台机器的配置（读配置归宿主，`C4`）、
// 把载荷当 JSON 读、把传输台的领域话（`sftp_pool::Snap`）翻成通道的格子。
// 传输本身（借通道 · 写暂存区 · 续传 · 撤）一行都不在这里。

/// 拒绝的信封：与后端命令的拒绝**同一个形状**（`{"code","message"}`，`backend_route::layer_call_error`
/// 逐字），窗口那一侧用同一个翻译（`filewin::source::said`）说人话。
///
/// ⚠ 经线上那一层的构造器（`wire::err_from_wire`）造，**不在本文件手写那个枚举的变体**：
/// 本文件登记在 `backend_route_tests::SENDERS` 里（走分流器的那一档），那一档的牙是
/// 「生产段不许自己拼 / 匹配那个错误枚举」—— 分层判定只许有一个家。这里不做任何分层判定：
/// 传输台说了「不行」，就是对端说了「不行」，原样装进那个信封。
fn refused(code: &str, message: String) -> CallError {
    let body = serde_json::to_vec(&serde_json::json!({ "code": code, "message": message }))
        .unwrap_or_default();
    super::wire::err_from_wire(super::wire::WireErr::Refused, body)
}

/// 开单：`transfer-upload` / `transfer-download`。
async fn transfer_open(origin: Origin, op: Op, payload: Body) -> Result<Body, CallError> {
    let Ok(args) = serde_json::from_slice::<serde_json::Value>(&payload.0) else {
        return Err(OursFault::Misuse.into());
    };
    // 本机那一侧**先分**：本机没有 SFTP 传输这回事（用户逐字「本地不需要文件管理器」，
    // 文件窗口只开在远端上）⇒ 说真实原因，不让下一行答一句「未找到远端配置: <local>」。
    if origin.as_wire_str() == inbound_client::LOCAL_ORIGIN {
        return Err(refused(
            "local_has_no_transfer",
            copy_text("rsChanHost.transfer.localNone", &[]),
        ));
    }
    let Some(cfg) = crate::load_remote_config_by_label(origin.as_wire_str()) else {
        return Err(refused(
            "no_such_origin",
            copy_text(
                "rsChanHost.transfer.noConfig",
                &[("machine", &(origin.as_wire_str()).to_string())],
            ),
        ));
    };
    match crate::sftp_pool::transfer_call(cfg, &op.0, &args).await {
        Ok(v) => Ok(Body(serde_json::to_vec(&v).unwrap_or_default())),
        Err((code, message)) => Err(refused(&code, message)),
    }
}

/// 一格快照 ⇒ 流里的一格。收场那一格是 `Closed{Peer(结局)}`，其余是 `Frame{seq, {"got","total"}}`。
fn snap_item(seq: u64, snap: &crate::sftp_pool::Snap) -> Item {
    use crate::sftp_pool::End;
    let json = |v: serde_json::Value| Body(serde_json::to_vec(&v).unwrap_or_default());
    match &snap.end {
        None => Item::Frame {
            seq,
            body: json(serde_json::json!({ "got": snap.got, "total": snap.total })),
        },
        Some(End::Done { bytes, sha256 }) => Item::Closed {
            by: By::Peer(json(
                serde_json::json!({ "state": "done", "bytes": bytes, "sha256": sha256 }),
            )),
        },
        Some(End::Failed(why)) => Item::Closed {
            by: By::Peer(json(serde_json::json!({ "state": "failed", "why": why }))),
        },
        Some(End::Cancelled) => Item::Closed {
            by: By::Peer(json(serde_json::json!({ "state": "cancelled" }))),
        },
    }
}

/// 进度流。**`from` 必须是 `None`**：每一格都是一整份快照，「从哪续」对它没有意义 ——
/// 给了就原位说用法错，不装作续上了。
fn transfer_stream(origin: &Origin, id: &str, from: Option<Cursor>) -> BoxStream<'static, Item> {
    use futures::stream::StreamExt;
    let closed = |code: &str, message: String| -> BoxStream<'static, Item> {
        let body = serde_json::to_vec(&serde_json::json!({ "code": code, "message": message }))
            .unwrap_or_default();
        Box::pin(futures::stream::iter([Item::Closed {
            by: By::Peer(Body(body)),
        }]))
    };
    if from.is_some() {
        return closed("bad_args", copy_text("rsChanHost.transfer.noResume", &[]));
    }
    match crate::sftp_pool::watch_ticket(origin, id) {
        Ok(snaps) => Box::pin(
            snaps
                .enumerate()
                .map(|(i, snap)| snap_item(i as u64 + 1, &snap)),
        ),
        Err((code, e)) => closed(code, e),
    }
}

#[cfg(test)]
#[path = "../../../../tests/bridge/chan/transfer_stream_tests.rs"]
mod transfer_stream_tests;
