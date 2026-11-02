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
//! - 订阅：本文件自己接三种流 `transfer/<id>`（传输台那一趟的进度，翻译在本文件末尾，判据 `transfer_stream_tests`）· `link`（那台连没连着）·
//!   `files-batch/<票>`（一组做到哪了，`files_batch_relay`）；别的 `kind` 交进程里那一份会话流句柄（[`crate::event_replay::EventReplay::subscribe_stream`]：
//!   `changed/<主题>` 等，与主界面同一张订阅表，判据 `changed_stream_tests`），它认不出的原位回 `Closed{Peer(…)}`，不装作订阅成功。
//! - 对端撤活是尽力的：外部前端撤单 ⇒ 路由器丢掉本 future（`router::run_call`）⇒ 那次 `inbound_client` 调用随之被丢 ⇒ 它的 `AbandonGuard`
//!   补发一条 `cancel{target}`（判据 `inbound_client_tests::abandoning_the_wait_fires_one_cancel_and_finishing_fires_none`）。可取消档真停下；阻塞档照跑完。
//! - 不接回：monitor 退了，管子断了，窗口里之后每一件都会出声说没走通（`client.rs` 头注）。

use super::router::{self, Backends, Ended, Terms};
use super::wire::{Body, By, CallError, CancelToken, Cursor, Item, Kind, Op, Origin, OursFault};
use crate::{backend_route, inbound_client};
use copy_core::copy_text;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use std::sync::Arc;
use std::time::Duration;

/// 帧头 / 帧体各自的上限：一屏目录（`filewin::source::LS_LIMIT` 条）的 JSON 在兆字节级，给 64 MiB。两端同一个数（随种子交给窗口进程）。
pub const FRAME_MAX_BYTES: relay_route_core::Cap = relay_route_core::Cap {
    bytes: 64 << 20,
    over: relay_route_core::Over::Reject,
};

/// 把一个窗口进程的那一对管子交给路由器（`rd` = 它的 stdout，`wr` = 它的 stdin），直到它走了。要在 tokio 运行时里调。
pub fn serve_window<R, W>(rd: R, wr: W)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    // 这扇窗在「会打断什么」那张表上的编号：它说的记在这一格，管子断了这一格删掉（[`busy`]）。
    let book = busy::book();
    let id = book.enrol();
    serve_then(rd, wr, Arc::new(WindowBackends { id }), move || {
        book.forget(id);
    });
}

/// 同上，句柄是入参（判据挂合成句柄）。
#[cfg(test)]
pub fn serve_with<R, W>(rd: R, wr: W, backends: Arc<dyn Backends>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    serve_then(rd, wr, backends, || {});
}

/// 同上，那一对管子走了之后再做 `after`（不论正常走还是坏了）。
fn serve_then<R, W>(
    rd: R,
    wr: W,
    backends: Arc<dyn Backends>,
    after: impl FnOnce() + Send + 'static,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        match router::serve(
            rd,
            wr,
            Terms {
                frame: FRAME_MAX_BYTES.as_len(),
            },
            backends,
        )
        .await
        {
            Ended::Left => {}
            Ended::Broken(why) => tracing::warn!("通道：窗口进程那一对管子坏了，已关（{why}）"),
        }
        after();
    });
}

/// 一扇文件窗口那条通道上的句柄：它说「手上有什么会被打断」（[`filewin_contract::FILEWIN_BUSY_OP`]）记进它那一格，别的一切照 [`InboundBackends`]。
struct WindowBackends {
    id: u64,
}

impl Backends for WindowBackends {
    fn call(
        &self,
        origin: Origin,
        op: Op,
        payload: Body,
        view: Option<serde_json::Value>,
        left: Duration,
        cancel: CancelToken,
    ) -> BoxFuture<'static, Result<Body, CallError>> {
        if op.0 == filewin_contract::FILEWIN_BUSY_OP {
            let said = busy::book().said(self.id, &payload.0);
            return Box::pin(async move {
                said.map(|()| Body(b"{}".to_vec()))
                    .map_err(|_| OursFault::Misuse.into())
            });
        }
        InboundBackends.call(origin, op, payload, view, left, cancel)
    }

    fn subscribe(
        &self,
        origin: Origin,
        kind: Kind,
        from: Option<Cursor>,
    ) -> BoxStream<'static, Item> {
        InboundBackends.subscribe(origin, kind, from)
    }

    fn offer(&self, origin: &Origin) -> Option<super::wire::Offer> {
        InboundBackends.offer(origin)
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  生产那个后端句柄：`call` 走既有的 `inbound_client`
// ════════════════════════════════════════════════════════════════════════════

/// 生产的后端句柄 —— 一律经本机后端那一个客户端（`inbound_client::local_client`）：本机直接问，别台经它 `call_at`（本机与远端同一条路）。
pub struct InboundBackends;

/// 这一跳在面 A 上的编号：路由器 ↔ 后端（`wire::HopId` 头注那两跳里的第 1 跳）。
const HOP: u8 = 1;

/// 通道上**由 monitor 自己接**、不按 `origin` 转给那台后端的 op：传输台开单两条（本机常驻后端的传输台，经中继 `transfer_relay.rs`）·
/// 文件窗口「在此打开终端」（[`terminal_open`]）。其余一切照旧按 `origin` 去 `inbound_client`。
/// 传输那两条从传输台那一份名单取（`transfer_relay::TRANSFER_OPS`，一份名单一个家）。
/// 窗口说「手上有什么会被打断」那一条由窗口那条通道的句柄（[`WindowBackends`]）接；从别处（主界面）来的这一条是用错了。
pub(crate) const HOST_OPS: [&str; 7] = [
    filewin_contract::FILEWIN_BUSY_OP,
    crate::transfer_relay::TRANSFER_OPS[0],
    crate::transfer_relay::TRANSFER_OPS[1],
    crate::transfer_relay::TRANSFER_COPY,
    filewin_contract::TERMINAL_OPEN_OP,
    filewin_contract::FILEWIN_OPEN_OP,
    filewin_contract::PLAN_OPEN_OP,
];

/// 「在计划里看」交给主窗口的事件名（界面 `window-events.ts::PLAN_OPEN_EVENT`，两边逐字相同，`plan-signs.vitest.ts` 比）。
const PLAN_OPEN_EVENT: &str = "plan-open";

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
        // 🔴**生产上的第一条流**：`transfer/<id>` ⇒ 传输台那一趟的进度。
        if let Some(id) = kind.0.strip_prefix(filewin_contract::TRANSFER_KIND_PREFIX) {
            return transfer_stream(&origin, id, from);
        }
        // 一组做到哪了：按票（窗口铸的）转后端推来的 `files_batch` 帧，那一组在哪台做都一样。`from` 没有意义（只留最新那一格）。
        if let Some(ticket) = kind.0.strip_prefix(crate::files_batch_relay::KIND_PREFIX) {
            use futures::StreamExt as _;
            let _ = from;
            return crate::files_batch_relay::watch(ticket)
                .enumerate()
                .map(|(i, cell)| Item::Frame {
                    seq: i as u64 + 1,
                    body: Body(cell.into_bytes()),
                })
                .boxed();
        }
        // 别的 `kind`（文件窗口订的 `changed/<主题>` …）⇒ 进程里那一份会话流句柄，与主界面同一张订阅表、同一套 credit；
        // 流名认不认得 · 续不续看由它答（认不出 ⇒ 原位 `Closed{Peer(no-such-stream)}`；`from` 给了 ⇒ 原位 `bad_args`）。
        match crate::event_replay::host() {
            Some(replay) => replay.subscribe_stream(&origin, &kind.0, from),
            // 会话流句柄还没起（进程起步那一下之前）：不装作订阅成功。
            None => Box::pin(futures::stream::iter([Item::Closed {
                by: By::Ours(OursFault::Broken),
            }])),
        }
    }

    /// 那台的能力事实 = 它此刻那条连接握手时交出的那一份（`inbound_client::offer_of`，一个家）。
    fn offer(&self, origin: &Origin) -> Option<super::wire::Offer> {
        inbound_client::offer_of(origin)
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
        // 传输台住**本机**常驻后端（SFTP 与其它 SSH 同一条连接），由中继 `transfer_relay.rs` 转过去；
        //    其余一切照旧按 `origin` 去 `inbound_client`。
        // 通道上由 monitor 自己接的那几条（[`HOST_OPS`]，两向登记在 `command_home_registry_tests::CHANNEL_OWN`）。
        if HOST_OPS.contains(&op.0.as_str()) {
            if op.0 == filewin_contract::FILEWIN_BUSY_OP {
                return Box::pin(async { Err(OursFault::Misuse.into()) });
            }
            if crate::transfer_relay::is_transfer_op(&op.0) {
                return Box::pin(transfer_open(origin, op, payload));
            }
            if op.0 == crate::transfer_relay::TRANSFER_COPY {
                return Box::pin(transfer_copy(origin, payload, left));
            }
            if op.0 == filewin_contract::FILEWIN_OPEN_OP {
                return Box::pin(filewin_open(origin, payload));
            }
            if op.0 == filewin_contract::PLAN_OPEN_OP {
                return Box::pin(plan_open(origin, payload));
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
            let Some(client) = inbound_client::local_client() else {
                return Err(backend_route::layer_no_channel(HOP));
            };
            // 这个后端说 JSON：载荷在这里（宿主这一侧）才第一次被当成 JSON 读。
            // 读不动是**调用方用错了**（`Ours{Misuse}`）—— 与 inbound 无关，不是分流判定。
            let Ok(args) = serde_json::from_slice::<serde_json::Value>(&payload.0) else {
                return Err(OursFault::Misuse.into());
            };
            let deadline = tokio::time::Instant::now() + left;
            // 问别台：经本机后端转给它手上那台的常驻流（信封带 `machine`）；认不认 · 形状对不对由那一端连那台的客户端判。
            let answer = if origin.is_local() {
                match &view {
                    Some(v) => client.call_until_viewed(&op.0, args, v, deadline).await,
                    None => client.call_until(&op.0, args, deadline).await,
                }
            } else {
                client
                    .call_at(
                        origin.host_name().unwrap_or_default(),
                        &op.0,
                        args,
                        view.as_ref(),
                        deadline,
                    )
                    .await
            };
            match answer {
                Ok(v) => Ok(Body(
                    serde_json::to_vec(&v.unwrap_or(serde_json::Value::Null)).unwrap_or_default(),
                )),
                Err(e) => Err(backend_route::layer_call_error(&e, HOP).error),
            }
        })
    }
}

//
// ══════════════════════════════════════════════════════════════════
// 传输台那一口：开单 ＋ 进度流
// ══════════════════════════════════════════════════════════════════
//
// 这里只做宿主那几件事：按 `origin` 找那台机器的配置、把载荷当 JSON 读、把传输台那一格（`transfer_relay::Step`）原样装进通道的格子（帧照 `cmd_wire::frame::Transfer` · 收场照 `TransferEnd` 序列化，不手写）。
// 传输本身（借通道 · 写暂存区 · 续传 · 撤）不在这里。

/// 拒绝的信封：与后端命令的拒绝**同一个形状**（交出去那一形 `cmd_wire::failed::Failed`，`backend_route::layer_call_error`
/// 逐字），窗口那一侧用同一个翻译（`filewin::source::said`）说人话。
///
/// ⚠ 经线上那一层的构造器（`wire::err_from_wire`）造，**不在本文件手写那个枚举的变体**：
/// 本文件登记在 `backend_route_tests::SENDERS` 里（走分流器的那一档），那一档的牙是
/// 「生产段不许自己拼 / 匹配那个错误枚举」—— 分层判定只许有一个家。这里不做任何分层判定：
/// 传输台说了「不行」，就是对端说了「不行」，原样装进那个信封。
fn refused(code: &str, said: String) -> CallError {
    refused_with(cmd_wire::failed::Failed {
        code: code.to_string(),
        said,
        ..Default::default()
    })
}

/// 同 [`refused`]，后端带了 `data` 的（`exists` 的同名清单）一并装进信封，窗口照它画冲突框。
fn refused_with(r: cmd_wire::failed::Failed) -> CallError {
    let body = serde_json::to_vec(&r).unwrap_or_default();
    super::wire::err_from_wire(super::wire::WireErr::Refused, body)
}

/// 开单：`transfer-upload` / `transfer-download`。
async fn transfer_open(origin: Origin, op: Op, payload: Body) -> Result<Body, CallError> {
    let Ok(args) = serde_json::from_slice::<serde_json::Value>(&payload.0) else {
        return Err(OursFault::Misuse.into());
    };
    // 本机那一侧先分：本机没有 SFTP 传输这回事（文件窗口只开在远端上）⇒ 说真实原因，不让下一行答一句「未找到远端配置: <local>」。
    if origin.is_local() {
        return Err(refused(
            "local_has_no_transfer",
            copy_text("rsChanHost.transfer.localNone", &[]),
        ));
    }
    let host = origin.host_name().unwrap_or_default();
    if !crate::machine_table::has(host) {
        return Err(refused(
            "no_such_origin",
            copy_text(
                "rsChanHost.transfer.noConfig",
                &[("machine", &host.to_string())],
            ),
        ));
    }
    match crate::transfer_relay::transfer_call(host.to_string(), &op.0, &args).await {
        Ok(v) => Ok(Body(serde_json::to_vec(&v).unwrap_or_default())),
        Err(r) => Err(refused_with(r)),
    }
}

/// 两台之间复制一组：源 ＝ 寻址的那台，落点 ＝ 载荷里 `to` 那台；两台都要在机器表里（本机不是一台能拨的远端 ⇒ 说真实原因）。
/// 本机后端做完整组才回 ⇒ 等的上限是窗口给的 `left`。
async fn transfer_copy(origin: Origin, payload: Body, left: Duration) -> Result<Body, CallError> {
    let Ok(args) = serde_json::from_slice::<serde_json::Value>(&payload.0) else {
        return Err(OursFault::Misuse.into());
    };
    let to = args
        .get("to")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    for machine in [origin.to_string(), to] {
        if machine == cmd_wire::LOCAL {
            return Err(refused(
                "local_has_no_transfer",
                copy_text("rsChanHost.transfer.localNone", &[]),
            ));
        }
        if !crate::machine_table::has(&machine) {
            return Err(refused(
                "no_such_origin",
                copy_text("rsChanHost.transfer.noConfig", &[("machine", &machine)]),
            ));
        }
    }
    match crate::transfer_relay::copy_call(origin.to_string(), &args, left).await {
        Ok(v) => Ok(Body(serde_json::to_vec(&v).unwrap_or_default())),
        Err(r) => Err(refused_with(r)),
    }
}

/// **文件窗口「在此打开终端」**：窗口只交意图（寻址 ＝ 那台 · 参数 `{cwd}`），这里只补那台的名字、
/// 问本机后端 `terminal-ssh` 渲那一行（怎么拨照它的机器表）、交 `open_terminal_window` 开窗 ——
/// 与主界面开终端同一条路（`src/frontend/ui/terminal-open.ts`：`terminal-ssh` → `open_terminal_window`）。
/// 窗口不拼命令、不认识 monitor 的配置；成败作为这一次 `call` 的应答回去，那句话照旧画在窗口上。
async fn terminal_open(origin: Origin, payload: Body, left: Duration) -> Result<Body, CallError> {
    let Ok(args) = serde_json::from_slice::<serde_json::Value>(&payload.0) else {
        return Err(OursFault::Misuse.into());
    };
    let Some(cwd) = filewin_contract::terminal_open_cwd(&args) else {
        return Err(OursFault::Misuse.into());
    };
    // 文件窗口只开在远端上⇒ 本机这一问不存在，说真实原因。
    if origin.is_local() {
        return Err(refused(
            "local_has_no_file_window",
            copy_text("rsChanHost.terminal.localNone", &[]),
        ));
    }
    let host = origin.host_name().unwrap_or_default();
    if !crate::machine_table::has(host) {
        return Err(refused(
            "no_such_origin",
            copy_text(
                "rsChanHost.terminal.noConfig",
                &[("machine", &host.to_string())],
            ),
        ));
    }
    // 开终端那一行由本机后端渲（`src/backend/dial/terminal.rs`，与主界面 `terminal-open.ts` 问的同一条）。
    let ask = cmd_wire::terminal::TerminalSshReq {
        machine: cmd_wire::Machine::named(host),
        command: None,
        cwd: Some(cwd.clone()),
        sid: None,
    };
    let Some(local) = inbound_client::local_client() else {
        return Err(backend_route::layer_no_channel(HOP));
    };
    let deadline = tokio::time::Instant::now() + left;
    let line = match local.ask(None, &ask, None, deadline).await {
        Ok(Ok(r)) => r.window,
        Ok(Err(_)) => {
            return Err(refused(
                "bad_reply",
                copy_text("rsChanHost.terminal.badReply", &[]),
            ))
        }
        Err(e) => return Err(backend_route::layer_call_error(&e, HOP).error),
    };
    match crate::launch::open_terminal_window(line).await {
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
    match crate::filewin::entry::open_from_window(&origin.to_string(), &args).await {
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
    // 拉到前面走主窗口那一处（与第二次启动同一条：还原 · 显示 · 聚焦，失败各留一行日志）。
    crate::platform::window::raise_main(&app, None, true);
    let said = serde_json::json!({
        "origin": origin,
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

/// 传输台的一格 ⇒ 流里的一格。进度是 `Frame{seq, Transfer}`（后端那一帧原样）；
/// 收场那一格是 `Closed{Peer(TransferEnd)}`（码 · 复制详情原样，窗口按码换路）。
fn step_item(seq: u64, step: crate::transfer_relay::Step) -> Item {
    let bytes = |v: Result<Vec<u8>, serde_json::Error>| Body(v.unwrap_or_default());
    match step {
        Ok(t) => match &t.end {
            None => Item::Frame {
                seq,
                body: bytes(serde_json::to_vec(&t)),
            },
            Some(end) => Item::Closed {
                by: By::Peer(bytes(serde_json::to_vec(end))),
            },
        },
        Err(end) => Item::Closed {
            by: By::Peer(bytes(serde_json::to_vec(&end))),
        },
    }
}

/// 进度流。**`from` 必须是 `None`**：每一格都是一整份快照，「从哪续」对它没有意义 ——
/// 给了就原位说用法错，不装作续上了。
fn transfer_stream(origin: &Origin, id: &str, from: Option<Cursor>) -> BoxStream<'static, Item> {
    use futures::stream::StreamExt;
    let closed = |code: &str, said: String| -> BoxStream<'static, Item> {
        let body = serde_json::to_vec(&cmd_wire::failed::Failed {
            code: code.to_string(),
            said,
            ..Default::default()
        })
        .unwrap_or_default();
        Box::pin(futures::stream::iter([Item::Closed {
            by: By::Peer(Body(body)),
        }]))
    };
    if from.is_some() {
        return closed("bad_args", copy_text("rsChanHost.transfer.noResume", &[]));
    }
    match crate::transfer_relay::watch_ticket(origin, id) {
        Ok(steps) => Box::pin(
            steps
                .enumerate()
                .map(|(i, step)| step_item(i as u64 + 1, step)),
        ),
        Err((code, e)) => closed(code, e),
    }
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/chan/transfer_stream_tests.rs"]
mod transfer_stream_tests;

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/chan/changed_stream_tests.rs"]
mod changed_stream_tests;

/// 退出那一问要的加总（[`busy`] 那张表现读出来的）：手上有事的窗口几扇 · 没存的几份 · 还在跑的几趟。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct FilewinTotals {
    pub windows: u32,
    pub unsaved: u32,
    pub running: u32,
}

/// 各文件窗口「手上有什么会被打断」那张表：窗口在数变了时说一句（[`filewin_contract::FILEWIN_BUSY_OP`]，发在窗口那头 `workspace.rs::settle_busy`），
/// 那扇窗的通道句柄（[`WindowBackends`]）记进它那一格，管子断了（[`serve_window`] 收尾）删掉；退出 / 重启 cc-monitor 那一问经 Tauri 命令
/// [`filewin_interrupts`] 现读加起来的数（界面 `settings/interrupts.ts`），界面那头不存一份。
/// 买不到：窗口说了之后、下一句到之前那一小段里的变化；窗口进程卡死、管子还连着 ⇒ 它最后说的那一份照算。
pub mod busy {
    pub use super::FilewinTotals;
    use filewin_contract::Busy;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};

    /// 那张表：窗口编号 → 它最后说的那一份。
    #[derive(Default)]
    pub struct Book {
        next: AtomicU64,
        rows: Mutex<BTreeMap<u64, Busy>>,
    }

    impl Book {
        /// 一扇新窗接上了：给它一个编号（一个进程内不重复）。
        pub fn enrol(&self) -> u64 {
            self.next.fetch_add(1, Ordering::Relaxed)
        }

        /// 那扇窗说了它此刻的数。
        pub fn set(&self, id: u64, busy: Busy) {
            if let Ok(mut rows) = self.rows.lock() {
                rows.insert(id, busy);
            }
        }

        /// 窗口在通道上说的那一句（[`filewin_contract::FILEWIN_BUSY_OP`] 的载荷）；解不出 ⇒ `Err`、表不动（不当成 0 记）。
        ///
        /// # Errors
        ///
        /// 载荷不是 [`Busy`] 那个形状。
        pub fn said(&self, id: u64, payload: &[u8]) -> Result<(), serde_json::Error> {
            let busy: Busy = serde_json::from_slice(payload)?;
            self.set(id, busy);
            Ok(())
        }

        /// 那扇窗的管子断了：它那一份不再算。
        pub fn forget(&self, id: u64) {
            if let Ok(mut rows) = self.rows.lock() {
                rows.remove(&id);
            }
        }

        /// 此刻加起来的数（说回 0 的窗口不算「手上有事」）。
        pub fn total(&self) -> FilewinTotals {
            let Ok(rows) = self.rows.lock() else {
                return FilewinTotals::default();
            };
            rows.values().filter(|b| **b != Busy::default()).fold(
                FilewinTotals::default(),
                |t, b| FilewinTotals {
                    windows: t.windows.saturating_add(1),
                    unsaved: t.unsaved.saturating_add(b.unsaved),
                    running: t.running.saturating_add(b.running),
                },
            )
        }
    }

    /// 进程里那一张。
    pub fn book() -> &'static Book {
        static BOOK: OnceLock<Book> = OnceLock::new();
        BOOK.get_or_init(Book::default)
    }
}

/// IPC：退出 / 重启 cc-monitor 之前，各文件窗口手上会被打断的加起来（现读 [`busy`] 那张表）。
#[tauri::command]
pub fn filewin_interrupts() -> busy::FilewinTotals {
    busy::book().total()
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/filewin/busy_tests.rs"]
mod busy_tests;
