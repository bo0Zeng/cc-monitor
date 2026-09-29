//! 通道 · **webview 那一侧的宿主**：主界面（webview 里的 TS）经 Tauri IPC 说 `call` 的那一跳。
//!
//! # 为什么要有它（`设计/05 §3.3` · `§10` · C4a）
//!
//! 通道那一拍（`§10`）给了**进程外**的前端（文件窗口）一条路：回环 TCP ＋ 钥匙 ＋ 路由器。
//! 主界面**不在进程外** —— 它是 monitor 进程里的 webview，与后端之间隔着的是 Tauri IPC，
//! 不是一条 TCP 连接。⇒ 它说 `call` 用不着绑口、钥匙、拆帧、编号配对（那些是**那条连接**的事）；
//! 它要的只是 `§3.3` 那个签名在 Tauri IPC 这一跳上的样子：
//!
//! ```text
//! webview (src/ipc/chan.ts)  ──Tauri IPC──▶  chan_call（本文件）──▶ router::settle ──▶ 注入的 Backends
//!   第 0 跳：webview ↔ monitor                                      第 1 跳：monitor ↔ 后端
//! ```
//!
//! 跳号与回环那条同一张读法表（`wire::HopId` 头注）：第 0 跳是「前端 ↔ monitor」这一跳，第 1 跳是
//! 「monitor ↔ 后端」（经注入的句柄）。第 0 跳的期限在 TS 那一侧执行（过期就一个字节都不发）；
//! 第 1 跳的上界由 [`super::router::settle`] 执行 —— **与回环那条同一份**，不写第二份。
//!
//! # 🔴 为什么这一份**不是**通信层成员（与 `host.rs` 同一条理由）
//!
//! 它碰 Tauri（`#[tauri::command]` · `tauri::ipc::Response`）、按生产注入句柄（[`super::host::InboundBackends`]）、
//! 给「空白名」那一档当场拒 —— 那是**宿主**做的几件事（`C5` 的「绑口 / 注入在外」同形）。
//! 成员那一半是 TS 的 `src/ipc/chan.ts`（webview 手里那一半，与 `client.rs` 对称）。
//!
//! # 载荷原样
//!
//! 请求体在 IPC 上是一个字节数组（JSON 的数字数组 —— 查询的请求体都小），**应答**走
//! `tauri::ipc::Response`（原样字节，TS 拿 `ArrayBuffer`）—— 两个方向都不把载荷当 JSON 读，
//! 读它的是宿主注入的那个句柄（后端说 JSON，`host.rs` 那一格）与 TS 那侧的调用方。
//!
//! # 错误三层
//!
//! 失败时回 `{ err, body }`：`err` 是 [`super::wire::err_to_wire`] 给的**线上形状**（回环那条同一份），
//! `body` 是 `Refused` 那份不透明体。TS 那侧按它解回 `§3.3.1` 的三层。
//!
//! # 〔CF2 · 第四波 4B〕`subscribe` 那一半
//!
//! ```text
//! webview (src/ipc/chan.ts)  ── chan_subscribe(origin, kind, from, want, id) ──▶ 本文件 ──▶ 注入的句柄（event_replay·rs）
//!   events.ts 的流          ◀── Tauri 事件 `chan-items` {sub, items} ─────────── WebviewSink（emit_to 那个 webview）
//!                            ── chan_want(id, more) / chan_stop(id) ────────────▶
//! ```
//!
//! - **传输选 Tauri 事件，不选 `tauri::ipc::Channel`**（`调研/第四波记录/CF2.md §3.2`）：前端那一条 queue 的顺序
//!   （行 · `ended` 格 · 宣告 …）靠「同一个 webview 上按 emit 先后执行」；`Channel` 的大消息走「先存、再让 JS
//!   `fetch` 回来」，会被之后 `eval` 出去的起停事件超车（Tauri 2.11.6 `ipc/channel.rs`）。
//! - **编号由 webview 那一侧给**（每页从 1 起）：格可能先于 `chan_subscribe` 的应答到达，
//!   编号先登记在 TS 那侧才不丢。同一个 `(webview, 编号)` 再订一次 ⇒ 旧的那条作废（页面重载）。
//! - **体在这一跳是文本**：Tauri 事件是 JSON，二进制过不来；体原样按 UTF-8 装成字符串（不解析、不改写），
//!   不是 UTF-8 ⇒ 那一格换成 `Closed{Ours(Broken)}`（句柄坏了，不猜）。
//! - 句柄只有一种：会话内容（`kind` 前缀 `session-lines`，[`crate::event_replay::SESSION_LINES_KIND`]）。
//!   认不出的 `kind` 由句柄原位回 `Closed{Peer(no-such-stream)}`，不装作订阅成功。
//!
//! # 买不到
//!
//! - 〔MIG-3b 续 · 主会话 09-28 裁「撤单不许回退」〕**TS 那侧撤单传到这一跳了**：`Budget.cancel` 拨下 ⇒ TS 本地立即回
//!   `Ours{Cancelled}`，同时带着那一问的编号发 [`chan_cancel`] ⇒ 在飞表里那一格的撤单手柄拨下 ⇒ 路由器丢掉那次调用
//!   （`router::settle`）⇒ `inbound_client` 的 `AbandonGuard` 补发 `cancel`（〔RM1f〕，同回环那条）⇒ 后端可取消档停下。
//!   〔墓碑 —— 上一版这里写着「这一跳缺的只剩 TS → monitor 的撤单命令 ＋ 在飞编号表，本拍不开」：这一拍开了。〕
//!   撤单那一条先于 `chan_call` 本身到（两条 IPC 不保序）⇒ 记下编号，那一问登记时当场撤、一个字节不发。
//! - **webview 这一跳的 `subscribe` 没有续传**（`from` 给了就原位说用法错）与**回环那条上没有会话流**
//!   （进程外前端今天不订会话内容；`host.rs::InboundBackends::subscribe` 对它照旧回「没有这条流」）。

use super::host::InboundBackends;
use super::router::{self, Backends};
use super::wire::{
    err_to_wire, Body, By, CallError, CancelToken, Cursor, HopFault, Item, Op, OursFault, WireErr,
};
use serde::Serialize;
use std::time::Duration;

/// 失败时交回 webview 的那一格：线上形状 ＋ `Refused` 的不透明体。
#[derive(Debug, Serialize)]
pub struct Fail {
    err: WireErr,
    body: Vec<u8>,
}

fn fail(e: CallError) -> Fail {
    let (err, body) = err_to_wire(e);
    Fail { err, body }
}

/// 经给定句柄走一次 `call`（第 1 跳）。判据用它喂合成句柄；生产由 [`chan_call`] 喂 [`InboundBackends`]。
/// 〔MIG-3b 续〕`call_id` 给了 ⇒ 这一问登记进在飞表，[`chan_cancel`] 拨下它的撤单手柄（见头注「撤单」）。
pub(crate) async fn call_via(
    backends: &dyn Backends,
    origin: super::wire::Origin,
    op: String,
    payload: Vec<u8>,
    left: Duration,
    call_id: Option<String>,
) -> Result<Body, CallError> {
    let cancel = CancelToken::new();
    let _inflight = call_id.map(|id| Inflight::enter(id, cancel.clone()));
    // 撤单先于这一问到了（两条 IPC 不保序）⇒ 一个字节都不发。
    if cancel.is_cancelled() {
        return Err(OursFault::Cancelled.into());
    }
    let fut = backends.call(origin, Op(op), Body(payload), left, cancel.clone());
    router::settle(fut, left, cancel).await
}

/// 〔MIG-3b 续 · 主会话 09-28 裁「撤单不许回退」〕webview 这一跳在飞的调用：TS 给的编号 → 撤单手柄；
/// 外加「撤单先到、那一问还没登记」的编号（两条 IPC 不保序），留最近 [`EARLY_KEEP`] 个，那一问登记时当场撤。
/// 有结局 / 被撤 / 调用方被丢都摘掉（[`Inflight`] 的 `Drop`）。编号由 TS 那一侧现造（每一问一个 UUID），本侧只当不透明的键。
#[derive(Default)]
struct Table {
    live: std::collections::HashMap<String, CancelToken>,
    early: std::collections::VecDeque<String>,
}

/// 先到的撤单留几个（晚到的撤单 —— 那一问已经有结局 —— 也落在这里，所以要有上限；编号不复用，挤掉的只是最旧的）。
const EARLY_KEEP: usize = 64;

fn inflight() -> std::sync::MutexGuard<'static, Table> {
    use std::sync::{Mutex, OnceLock};
    static TABLE: OnceLock<Mutex<Table>> = OnceLock::new();
    TABLE
        .get_or_init(|| Mutex::new(Table::default()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// 在飞表里的一格，随这一问一起走。
pub(crate) struct Inflight(String);

impl Inflight {
    fn enter(id: String, cancel: CancelToken) -> Inflight {
        let mut t = inflight();
        if let Some(i) = t.early.iter().position(|e| *e == id) {
            t.early.remove(i);
            cancel.cancel();
        }
        t.live.insert(id.clone(), cancel);
        Inflight(id)
    }
}

impl Drop for Inflight {
    fn drop(&mut self) {
        inflight().live.remove(&self.0);
    }
}

/// 拨下编号 `id` 那一问的撤单手柄。回它此刻在不在飞（不在 ⇒ 记进「先到的撤单」，那一问随后登记时当场撤）。
pub(crate) fn cancel_inflight(id: &str) -> bool {
    let mut t = inflight();
    if let Some(c) = t.live.get(id).cloned() {
        c.cancel();
        return true;
    }
    if t.early.len() >= EARLY_KEEP {
        t.early.pop_front();
    }
    t.early.push_back(id.to_string());
    false
}

/// 〔MIG-3b 续〕主界面撤掉一问（`src/ipc/chan.ts`：`Budget.cancel` 拨下 ⇒ 带着那一问的编号发这一条）。
/// 撤单手柄拨下 ⇒ `router::settle` 丢掉那次调用 ⇒ `inbound_client` 的放弃守卫补发 `cancel{target}` ⇒ 后端可取消档停下
/// （全景：小程序连同 `timeout` 前缀那一组子进程被杀）。
#[tauri::command]
pub fn chan_cancel(id: String) -> bool {
    cancel_inflight(&id)
}

/// 主界面说 `call` 的那一条命令。`left_ms` = 这一跳还剩多少（TS 那侧由绝对时刻换算，过期的根本不发）。
///
/// 空白名当场按「用法错」拒（`Origin::route` 那道闸，同全仓吃 `Origin` 的命令）：
/// 空名交给句柄只会得到一句「没有控制通道」，那与真实原因（调用方没说哪台）毫无关系。
#[tauri::command]
pub async fn chan_call(
    origin: crate::origin::Origin,
    op: String,
    payload: Vec<u8>,
    left_ms: u64,
    call_id: Option<String>,
) -> Result<tauri::ipc::Response, Fail> {
    if origin.route("chan_call").is_err() {
        return Err(fail(OursFault::Misuse.into()));
    }
    match call_via(
        &InboundBackends,
        origin,
        op,
        payload,
        Duration::from_millis(left_ms),
        call_id,
    )
    .await
    {
        Ok(body) => Ok(tauri::ipc::Response::new(body.0)),
        Err(e) => Err(fail(e)),
    }
}

/// 〔NET2〕webview 手里那份能力事实：判断已在这边做完（`Offer` 的方法），TS 只查成员、不另算。
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct OfferView {
    /// 那台认的 op。
    ops: Vec<String>,
    /// `[op, 码]`：这台做不到（码与那台事后会回的同一个）。
    unavailable: Vec<(String, String)>,
    /// 本地撤掉之后那台停得下的 op（`Offer::withdraw == Asked`）；不在里面的 ⇒ 撤了它可能还在跑。
    stoppable: Vec<String>,
}

/// `Offer` ⇒ webview 那一份。
pub fn offer_view(o: &super::wire::Offer) -> OfferView {
    let ops = o.ops().to_vec();
    OfferView {
        unavailable: ops
            .iter()
            .filter_map(|op| o.unavailable(op).map(|c| (op.clone(), c.to_string())))
            .collect(),
        stoppable: ops
            .iter()
            .filter(|op| o.withdraw(op) == super::wire::Withdraw::Asked)
            .cloned()
            .collect(),
        ops,
    }
}

/// 〔NET2〕主界面要那台机器的能力事实。与回环那条同一个句柄（`InboundBackends::offer`）；
/// `None` = 今天没有控制通道 / 空白名。
#[tauri::command]
pub fn chan_offer(origin: crate::origin::Origin) -> Option<OfferView> {
    origin.route("chan_offer").ok()?;
    InboundBackends.offer(&origin).as_ref().map(offer_view)
}

// ════════════════════════════════════════════════════════════════════════════
//  〔CF2 · 第四波 4B〕`subscribe`
// ════════════════════════════════════════════════════════════════════════════

/// 交给 webview 的事件名（`src/ipc/chan.ts` 按窗口作用域听它）。
pub const ITEMS_EVENT: &str = "chan-items";

/// 流里一格在 webview 这一跳上的样子（`src/ipc/chan.ts::decodeItem` 按它解；跨语言金样
/// `tests/__fixtures__/chan-webview-items.golden.json`）。体是文本（见模块头注）。
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub(crate) enum WebviewItem {
    Frame {
        seq: u64,
        body: String,
    },
    /// 〔RENDER2〕`to_seq` 缺 = 知道丢了、不知道丢到哪（`Item::Gap` 头注）。
    Gap {
        from_seq: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        to_seq: Option<u64>,
    },
    Unseen {
        idx: u8,
        tag: String,
        why: HopFault,
    },
    Seen {
        from: Option<Vec<u8>>,
    },
    ClosedByPeer {
        body: String,
    },
    ClosedByOurs {
        why: OursFault,
    },
}

/// 一次投递：哪条订阅 ＋ 一串格。
#[derive(Debug, Serialize)]
pub(crate) struct Delivery {
    pub sub: u64,
    pub items: Vec<WebviewItem>,
}

/// `Item` ⇒ webview 这一跳的格。体不是 UTF-8 ⇒ `Closed{Ours(Broken)}`（**穷尽**，`X1` 同一条纪律）。
pub(crate) fn webview_item(i: Item) -> WebviewItem {
    let broken = WebviewItem::ClosedByOurs {
        why: OursFault::Broken,
    };
    match i {
        Item::Frame { seq, body } => match String::from_utf8(body.0) {
            Ok(body) => WebviewItem::Frame { seq, body },
            Err(_) => broken,
        },
        Item::Gap { from_seq, to_seq } => WebviewItem::Gap { from_seq, to_seq },
        Item::Unseen { at, why } => WebviewItem::Unseen {
            idx: at.idx,
            tag: at.tag.to_string(),
            why,
        },
        Item::Seen { from } => WebviewItem::Seen {
            from: from.map(|c: Cursor| c.0),
        },
        Item::Closed { by: By::Peer(b) } => match String::from_utf8(b.0) {
            Ok(body) => WebviewItem::ClosedByPeer { body },
            Err(_) => broken,
        },
        Item::Closed { by: By::Ours(why) } => WebviewItem::ClosedByOurs { why },
    }
}

/// 生产的出口：`emit_to` 那个 webview（窗口作用域），与起停事件同一条投递队列。
pub struct WebviewSink(pub tauri::AppHandle);

impl crate::event_replay::ItemSink for WebviewSink {
    fn deliver(&self, label: &str, sub: u64, items: Vec<Item>) {
        use tauri::Emitter;
        let d = Delivery {
            sub,
            items: items.into_iter().map(webview_item).collect(),
        };
        // 目标必须是 `EventTarget::webview_window`（`INVARIANTS §22` 第 2 条：裸串目标命不中窗口作用域的监听）。
        let target = tauri::EventTarget::webview_window(label.to_string());
        if let Err(e) = self.0.emit_to(target, ITEMS_EVENT, &d) {
            tracing::warn!("通道：交格给 [{label}] 第 {sub} 条订阅失败：{e}");
        }
    }
}

/// 主界面说 `subscribe` 的那一条命令。**不回错**（`05 §3.3.5`）：说不了的在流里原位说。
/// `id` 由 webview 那一侧给（见模块头注）；`from` 是不透明游标（本句柄不支持，原位说用法错）。
#[tauri::command]
pub fn chan_subscribe(
    webview: tauri::Webview,
    origin: crate::origin::Origin,
    kind: String,
    from: Option<Vec<u8>>,
    want: u32,
    id: u64,
    replay: tauri::State<'_, std::sync::Arc<crate::event_replay::EventReplay>>,
) {
    // 空白名：`route` 那道闸拒 ⇒ 交给句柄一个空名，它原位回 `Closed{Ours(Misuse)}`（同 `chan_call` 那一格）。
    let origin = match origin.route("chan_subscribe") {
        Ok(_) => origin,
        Err(_) => crate::origin::Origin(String::new()),
    };
    replay.subscribe(webview.label(), id, &origin, &kind, from.map(Cursor), want);
}

/// 订阅方的 credit（累加）。
#[tauri::command]
pub fn chan_want(
    webview: tauri::Webview,
    id: u64,
    more: u32,
    replay: tauri::State<'_, std::sync::Arc<crate::event_replay::EventReplay>>,
) {
    replay.want(webview.label(), id, more);
}

/// 撤订阅（本地撤单）。
#[tauri::command]
pub fn chan_stop(
    webview: tauri::Webview,
    id: u64,
    replay: tauri::State<'_, std::sync::Arc<crate::event_replay::EventReplay>>,
) {
    replay.stop(webview.label(), id);
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/chan/webview_tests.rs"]
mod tests;
