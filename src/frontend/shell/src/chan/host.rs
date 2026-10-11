//! 通道 · 宿主那一侧（monitor 进程里）：把文件窗口进程那一对管子交给路由器 · 注入后端句柄。
//!
//! 不是通信层成员（刻意的）：接管子、定帧长上限（策略值）、把 `op` 翻成 `inbound_client` 的命令
//! （路由器不许知道载荷长什么样）都归宿主，与 `relay/listen.rs` 那一份「语义上就该在外面」同形。
//!
//! # 谁连得上
//!
//! 没有监听口、没有钥匙：文件窗口进程**总是** monitor 起的子进程（`filewin/proc.rs`），通道就是它的 stdin / stdout ——
//! 父子管道只有起它的那一方拿得到。种子是 stdin 的第一行，之后同一对管子上走通道的帧（[`serve_window`]）。
//!
//! `call` 经注入的 [`InboundBackends`] 走既有的 `inbound_client`（本机与远端同一条路）。
//!
//! # 买不到的
//!
//! - 订阅只有一条流：`transfer/<id>`（传输台那一趟的进度，翻译在本文件末尾，判据 `transfer_stream_tests`）；别的 `kind`
//!   [`InboundBackends::subscribe`] 原位回 `Closed{Peer(…)}`，不装作订阅成功。
//! - 对端撤活是尽力的：外部前端撤单 ⇒ 路由器丢掉本 future（`router::run_call`）⇒ 那次 `inbound_client` 调用随之被丢 ⇒ 它的 `AbandonGuard`
//!   补发一条 `cancel{target}`（判据 `inbound_client_tests::abandoning_the_wait_fires_one_cancel_and_finishing_fires_none`）。可取消档真停下；阻塞档照跑完。
//! - 不接回：monitor 退了，管子断了，窗口里之后每一件都会出声说没走通（`client.rs` 头注）。

use super::router::{self, Backends, Ended, Terms};
use super::wire::{Body, By, CallError, CancelToken, Cursor, Item, Kind, Op, Origin, OursFault};
use crate::copy_table::copy_text;
use crate::{backend_route, inbound_client};
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use std::sync::Arc;
use std::time::Duration;

/// 帧头 / 帧体各自的上限：一屏目录（`filewin::source::LS_LIMIT` 条）的 JSON 在兆字节级，给 64 MiB。两端同一个数（随种子交给窗口进程）。
pub const FRAME_MAX_BYTES: usize = 64 << 20;

/// 把一个窗口进程的那一对管子交给路由器（`rd` = 它的 stdout，`wr` = 它的 stdin），直到它走了。要在 tokio 运行时里调。
pub fn serve_window<R, W>(rd: R, wr: W)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    serve_with(rd, wr, Arc::new(InboundBackends));
}

/// 同上，句柄是入参（判据挂合成句柄）。
pub fn serve_with<R, W>(rd: R, wr: W, backends: Arc<dyn Backends>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        match router::serve(
            rd,
            wr,
            Terms {
                frame: FRAME_MAX_BYTES,
            },
            backends,
        )
        .await
        {
            Ended::Left => {}
            Ended::Broken(why) => tracing::warn!("通道：窗口进程那一对管子坏了，已关（{why}）"),
        }
    });
}

// ════════════════════════════════════════════════════════════════════════════
//  生产那个后端句柄：`call` 走既有的 `inbound_client`
// ════════════════════════════════════════════════════════════════════════════

/// 生产的后端句柄 —— `origin` ⇒ `inbound_client::client_for`（本机与远端同一条路）。
pub struct InboundBackends;

/// 这一跳在面 A 上的编号：路由器 ↔ 后端（`wire::HopId` 头注那两跳里的第 1 跳）。
const HOP: u8 = 1;

/// 通道上**由 monitor 自己接**、不按 `origin` 转给那台后端的 op：传输台开单两条（本机常驻后端的传输台，经中继 `sftp_pool.rs`）·
/// 文件窗口「在此打开终端」（[`terminal_open`]）。其余一切照旧按 `origin` 去 `inbound_client`。
/// 传输那两条从传输台那一份名单取（`sftp_pool::TRANSFER_OPS`，一份名单一个家）。
pub(crate) const HOST_OPS: [&str; 6] = [
    crate::sftp_pool::TRANSFER_OPS[0],
    crate::sftp_pool::TRANSFER_OPS[1],
    filewin_contract::TERMINAL_OPEN_OP,
    filewin_contract::FILEWIN_OPEN_OP,
    filewin_contract::LINK_RETRY_OP,
    filewin_contract::PLAN_OPEN_OP,
];

/// 「在计划里看」交给主窗口的事件名（界面 `window-events.ts::PLAN_OPEN_EVENT`，两边逐字相同，`plan-signs.vitest.ts` 比）。
const PLAN_OPEN_EVENT: &str = "plan-open";

/// 开终端那一行由本机后端渲（`src/backend/dial/terminal.rs`，与主界面 `terminal-open.ts` 问的同一条）。
const TERMINAL_SSH: &str = "terminal-ssh";

impl Backends for InboundBackends {
    fn call(
        &self,
        origin: Origin,
        op: Op,
        payload: Body,
        view: Option<serde_json::Value>,
        left: Duration,
        _cancel: CancelToken,
    ) -> BoxFuture<'static, Result<Body, CallError>> {
        // 对端说「不行」⇒ 拒绝体里那份详情远端时补「本机」一行（主界面与文件窗口都经这一口，补一次、只在这里）。
        let fut = self.call_raw(origin.clone(), op, payload, view, left);
        Box::pin(async move {
            fut.await
                .map_err(|e| crate::detail::relay_refusal(&origin, e))
        })
    }

    fn subscribe(
        &self,
        origin: Origin,
        kind: Kind,
        from: Option<Cursor>,
    ) -> BoxStream<'static, Item> {
        self.subscribe_raw(origin, kind, from)
    }

    /// 那台的能力事实 = 它那条长连接握手时交出的那一份（`inbound_client` 登记表里，一个家）。
    fn offer(&self, origin: &Origin) -> Option<super::wire::Offer> {
        inbound_client::client_for(origin.as_wire_str()).map(|c| c.offer())
    }
}

impl InboundBackends {
    /// `call` 的本体（拒绝体补「本机」之前）。
    /// `view` 只给转去那台后端的那一路（原样进请求信封）；monitor 自己接的那几条（[`HOST_OPS`]）不认声明。
    fn call_raw(
        &self,
        origin: Origin,
        op: Op,
        payload: Body,
        view: Option<serde_json::Value>,
        left: Duration,
    ) -> BoxFuture<'static, Result<Body, CallError>> {
        // 🔴**传输台那两条开单命令不按 `origin` 去那台机器的后端**：
        // 传输台住**本机**常驻后端（SFTP 与其它 SSH 同一条连接），由中继 `sftp_pool.rs` 转过去；
        //    其余一切照旧按 `origin` 去 `inbound_client`。
        // 通道上由 monitor 自己接的那几条（[`HOST_OPS`]，两向登记在 `command_home_registry_tests::CHANNEL_OWN`）。
        if HOST_OPS.contains(&op.0.as_str()) {
            if crate::sftp_pool::is_transfer_op(&op.0) {
                return Box::pin(transfer_open(origin, op, payload));
            }
            if op.0 == filewin_contract::FILEWIN_OPEN_OP {
                return Box::pin(filewin_open(origin, payload));
            }
            if op.0 == filewin_contract::PLAN_OPEN_OP {
                return Box::pin(plan_open(origin, payload));
            }
            if op.0 == filewin_contract::LINK_RETRY_OP {
                // 叫醒那台的连接循环（它睡在退避里）；连没连上由 `link` 那条流说。
                inbound_client::kick(origin.as_wire_str());
                return Box::pin(async { Ok(Body(b"{}".to_vec())) });
            }
            return Box::pin(terminal_open(origin, payload, left));
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
            let deadline = tokio::time::Instant::now() + left;
            let answer = match &view {
                Some(v) => client.call_until_viewed(&op.0, args, v, deadline).await,
                None => client.call_until(&op.0, args, deadline).await,
            };
            match answer {
                Ok(v) => Ok(Body(
                    serde_json::to_vec(&v.unwrap_or(serde_json::Value::Null)).unwrap_or_default(),
                )),
                Err(e) => Err(backend_route::layer_call_error(&e, HOP).error),
            }
        })
    }

    fn subscribe_raw(
        &self,
        origin: Origin,
        kind: Kind,
        from: Option<Cursor>,
    ) -> BoxStream<'static, Item> {
        // 🔴**生产上的第一条流**：`transfer/<id>` ⇒ 传输台那一趟的进度。
        if let Some(id) = kind.0.strip_prefix(crate::sftp_pool::TRANSFER_KIND_PREFIX) {
            return transfer_stream(&origin, id, from);
        }
        // 第二条：那台此刻连没连着（连接循环的事实，`inbound_client::link_of`）。
        if kind.0 == filewin_contract::LINK_KIND {
            return link_stream(&origin);
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
}

/// `link` 那条流：先交此刻的样子，之后每变一次交一格（没变不重复说）。连接循环那一侧的事实只住 `inbound_client`。
fn link_stream(origin: &Origin) -> BoxStream<'static, Item> {
    use futures::StreamExt as _;
    let o = origin.as_wire_str().to_string();
    let rx = inbound_client::link_changes();
    futures::stream::unfold((rx, None, o), |(mut rx, last, o)| async move {
        loop {
            let now = inbound_client::link_of(&o);
            if last != Some(now) {
                return Some((link_item(now), (rx, Some(now), o)));
            }
            rx.changed().await.ok()?;
        }
    })
    .boxed()
}

/// 连接循环的一格 → 流上的一格。
fn link_item(l: inbound_client::Link) -> Item {
    use super::wire::{HopFault, HopId};
    let at = HopId {
        idx: HOP,
        tag: "open",
    };
    match l {
        inbound_client::Link::Up => Item::Seen { from: None },
        inbound_client::Link::Reconnecting => Item::Unseen {
            at,
            why: HopFault::Dropped,
        },
        inbound_client::Link::Down => Item::Unseen {
            at,
            why: HopFault::Unreachable,
        },
    }
}

//
// ══════════════════════════════════════════════════════════════════
// 传输台那一口：开单 ＋ 进度流
// ══════════════════════════════════════════════════════════════════
//
// 这里只做宿主那几件事：按 `origin` 找那台机器的配置、把载荷当 JSON 读、把传输台的领域话（`sftp_pool::Snap`）翻成通道的格子。
// 传输本身（借通道 · 写暂存区 · 续传 · 撤）不在这里。

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
    // 本机那一侧先分：本机没有 SFTP 传输这回事（文件窗口只开在远端上）⇒ 说真实原因，不让下一行答一句「未找到远端配置: <local>」。
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

/// **文件窗口「在此打开终端」**：窗口只交意图（寻址 ＝ 那台 · 参数 `{cwd}`），这里补那台的机器事实
/// （monitor 的机器表 ＋ 上次赢的那条，`dial_host::machine_facts`）、问本机后端 `terminal-ssh` 渲那一行、交 `open_terminal_window` 开窗 ——
/// 与主界面开终端同一条路（`src/frontend/ui/terminal-open.ts`：`terminal_dial` → `terminal-ssh` → `open_terminal_window`）。
/// 窗口不拼命令、不认识 monitor 的配置；成败作为这一次 `call` 的应答回去，那句话照旧画在窗口上。
async fn terminal_open(origin: Origin, payload: Body, left: Duration) -> Result<Body, CallError> {
    let Ok(args) = serde_json::from_slice::<serde_json::Value>(&payload.0) else {
        return Err(OursFault::Misuse.into());
    };
    let Some(cwd) = filewin_contract::terminal_open_cwd(&args) else {
        return Err(OursFault::Misuse.into());
    };
    // 文件窗口只开在远端上⇒ 本机这一问不存在，说真实原因。
    if origin.as_wire_str() == inbound_client::LOCAL_ORIGIN {
        return Err(refused(
            "local_has_no_file_window",
            copy_text("rsChanHost.terminal.localNone", &[]),
        ));
    }
    let Some(cfg) = crate::load_remote_config_by_label(origin.as_wire_str()) else {
        return Err(refused(
            "no_such_origin",
            copy_text(
                "rsChanHost.terminal.noConfig",
                &[("machine", &(origin.as_wire_str()).to_string())],
            ),
        ));
    };
    let mut ask = crate::dial_host::machine_facts(&cfg);
    ask["cwd"] = cwd.clone();
    let Some(local) = inbound_client::client_for(inbound_client::LOCAL_ORIGIN) else {
        return Err(backend_route::layer_no_channel(HOP));
    };
    let reply = match local.call(TERMINAL_SSH, ask, left).await {
        Ok(v) => v,
        Err(e) => return Err(backend_route::layer_call_error(&e, HOP).error),
    };
    let Some(line) = reply
        .as_ref()
        .and_then(|v| v.get("command"))
        .and_then(serde_json::Value::as_str)
    else {
        return Err(refused(
            "bad_reply",
            copy_text("rsChanHost.terminal.badReply", &[]),
        ));
    };
    match crate::launch::open_terminal_window(line.to_string()).await {
        Ok(crate::platform::terminal::TerminalOpen::Opened) => Ok(Body(b"{}".to_vec())),
        // 这台找不到终端：窗口那一侧把这句当原话画出来（说去设置里指定）。
        Ok(crate::platform::terminal::TerminalOpen::NoWindow) => Err(refused(
            "no_window",
            copy_text("rsLaunch.posix.noTerminalWindow", &[]),
        )),
        // 设置里指定的那个终端不在：同上，说去设置里改。
        Ok(crate::platform::terminal::TerminalOpen::SetMissing) => Err(refused(
            "no_window",
            copy_text("rsLaunch.posix.setTerminalMissing", &[]),
        )),
        Err(why) => Err(refused("terminal_failed", why.said)),
    }
}

/// 「开另一台的文件窗口」：载荷当 JSON 读，交给开窗入口（`filewin::entry::open_from_window`），它的拒绝原样回给窗口。
async fn filewin_open(origin: Origin, payload: Body) -> Result<Body, CallError> {
    let Ok(args) = serde_json::from_slice::<serde_json::Value>(&payload.0) else {
        return Err(OursFault::Misuse.into());
    };
    match crate::filewin::entry::open_from_window(origin.as_wire_str(), &args).await {
        Ok(()) => Ok(Body(b"{}".to_vec())),
        Err((code, why)) => Err(refused(code, why)),
    }
}

/// **文件窗口「在计划里看」**：窗口只交意图（寻址 ＝ 那台 · 参数 `{workspace, slice, id}`）；这里把主窗口拉到前面，
/// 发 [`PLAN_OPEN_EVENT`] 给它（带上那台的名字：片按机器区分），主窗口开计划页、选中那一格。主窗口不在 ⇒ 说真实原因。
async fn plan_open(origin: Origin, payload: Body) -> Result<Body, CallError> {
    use tauri::{Emitter as _, Manager as _};
    let Ok(args) = serde_json::from_slice::<serde_json::Value>(&payload.0) else {
        return Err(OursFault::Misuse.into());
    };
    let Some(t) = filewin_contract::plan_open_target(&args) else {
        return Err(OursFault::Misuse.into());
    };
    let Some(app) = crate::main_app() else {
        return Err(refused(
            "no_main_window",
            copy_text("rsChanHost.plan.noMain", &[]),
        ));
    };
    if app.get_webview_window(crate::MAIN_WINDOW_LABEL).is_none() {
        return Err(refused(
            "no_main_window",
            copy_text("rsChanHost.plan.noMain", &[]),
        ));
    }
    // 拉到前面走主窗口那一处（第二次启动 · 点通知同一条：还原 · 显示 · 聚焦，失败各留一行日志）。
    crate::platform::window::raise_main(&app, None, true);
    let said = serde_json::json!({
        "origin": origin.as_wire_str(),
        "workspace": t.workspace,
        "slice": t.slice,
        "id": t.id,
    });
    let to = tauri::EventTarget::webview_window(crate::MAIN_WINDOW_LABEL);
    app.emit_to(to, PLAN_OPEN_EVENT, said).map_err(|e| {
        tracing::warn!("plan-open not delivered to the main window: {e}");
        refused("emit_failed", copy_text("rsChanHost.plan.emitFailed", &[]))
    })?;
    Ok(Body(b"{}".to_vec()))
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
        // 码原样交给窗口（它按码换路）；复制详情原样交给窗口（空 ⇒ 窗口自己写）。缺的格不出。
        Some(End::Failed { why, code, detail }) => {
            let mut v = serde_json::json!({ "state": "failed", "why": why });
            if let Some(c) = code {
                v["code"] = serde_json::Value::from(c.as_str());
            }
            if !detail.is_empty() {
                v["detail"] = serde_json::Value::from(detail.as_str());
            }
            Item::Closed {
                by: By::Peer(json(v)),
            }
        }
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
#[path = "../../../../../tests/frontend/shell/chan/transfer_stream_tests.rs"]
mod transfer_stream_tests;
