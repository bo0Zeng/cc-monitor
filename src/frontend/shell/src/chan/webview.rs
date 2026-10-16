//! 通道 · webview 那一侧的宿主：主界面（webview 里的 TS）经 Tauri IPC 说 `call` 的那一跳。主界面不在进程外，它与后端之间隔的是 Tauri IPC，
//! 用不着绑口、钥匙、拆帧、编号配对：
//!
//! ```text
//! webview (src/comms/inward/chan.ts)  ──Tauri IPC──▶  chan_call（本文件）──▶ router::settle ──▶ 注入的 Backends
//! 第 0 跳：webview ↔ monitor                                      第 1 跳：monitor ↔ 后端
//! ```
//!
//! 跳号与回环那条同一张读法表（`wire::HopId` 头注）。第 0 跳的期限 TS 那一侧换成还剩多少（过期交 0），本文件见 0 就不交句柄、一个字节都不发；第 1 跳的上界由 [`super::router::settle`] 执行（与回环那条同一份）。
//! 不是通信层成员（与 `host.rs` 同一条理由）：它碰 Tauri、按生产注入句柄（[`super::host::InboundBackends`]）、给「空白名」那一档当场拒；成员那一半是 TS 的 `src/comms/inward/chan.ts`。
//!
//! 载荷原样：请求体在 IPC 上是一个字节数组，应答走 `tauri::ipc::Response`（原样字节，TS 拿 `ArrayBuffer`）—— 两个方向都不把载荷当 JSON 读。
//! 失败时回 `{ err, body }`：`err` 是 [`super::wire::err_to_wire`] 给的线上形状（回环那条同一份），`body` 是 `Refused` 那份不透明体；TS 那侧按它解回三层。
//!
//! # `subscribe` 那一半
//!
//! ```text
//! webview (src/comms/inward/chan.ts)  ── chan_subscribe(origin, kind, from, want, id) ──▶ 本文件 ──▶ 注入的句柄（event_replay·rs）
//! events.ts 的流          ◀── Tauri 事件 `chan-items` {sub, items} ─────────── WebviewSink（emit_to 那个 webview）
//! ── chan_want(id, more) / chan_stop(id) ────────────▶
//! ```
//!
//! - 传输选 Tauri 事件，不选 `tauri::ipc::Channel`：前端那一条 queue 的顺序（行 · `ended` 格 · 宣告 …）靠「同一个 webview 上按 emit 先后执行」；
//!   `Channel` 的大消息走「先存、再让 JS `fetch` 回来」，会被之后 `eval` 出去的起停事件超车（Tauri 2.11.6 `ipc/channel.rs`）。
//! - 编号由 webview 那一侧给（每页从 1 起）：格可能先于 `chan_subscribe` 的应答到达，编号先登记在 TS 那侧才不丢。同一个 `(webview, 编号)` 再订一次 ⇒ 旧的那条作废（页面重载）。
//! - 体在这一跳是文本：Tauri 事件是 JSON，二进制过不来；体原样按 UTF-8 装成字符串（不解析、不改写），不是 UTF-8 ⇒ 那一格换成 `Closed{Ours(Broken)}`。
//! - 句柄只有一种：会话内容（`kind` 前缀 `session-lines`，[`crate::event_replay::SESSION_LINES_KIND`]）。认不出的 `kind` 原位回 `Closed{Peer(no-such-stream)}`。

use super::host::InboundBackends;
use super::router::{self, Backends};
use super::wire::{
    err_to_wire, Body, By, CallError, CancelToken, Cursor, HopFault, HopId, Item, Op, OursFault,
    Reach, WireErr,
};
use serde::Serialize;
use std::time::Duration;

/// 失败时交回 webview 的那一格：线上形状 ＋ `Refused` 的不透明体 ＋ 给人看的那一句（[`super::said::said`]）＋ 「复制详情」那几行（[`crate::detail`]：
/// 对端说「不行」⇒ 那台写的那份（远端时「本机」那一行通道宿主已经补上，`InboundBackends::call`）；别的层 ⇒ 壳写）。
#[derive(Debug, Serialize)]
pub struct Fail {
    err: WireErr,
    body: Vec<u8>,
    said: String,
    detail: String,
}

pub(crate) fn fail(origin: &crate::origin::Origin, op: &str, e: CallError) -> Fail {
    let detail = match &e {
        CallError::Peer {
            why: super::wire::PeerFault::Refused { body },
        } => crate::detail::of_refusal_body(&body.0),
        other => crate::detail::of_channel(origin, op, other),
    };
    let said = super::said::said(&e, origin);
    let (err, body) = err_to_wire(e);
    Fail {
        err,
        body,
        said,
        detail,
    }
}

/// 经给定句柄走一次 `call`（第 1 跳）。判据用它喂合成句柄；生产由 [`chan_call`] 喂 [`InboundBackends`]。
/// 期限已过（`left` 为 0）⇒ 不碰句柄，答第 0 跳没发出去就超时。
pub(crate) async fn call_via(
    backends: &dyn Backends,
    origin: super::wire::Origin,
    op: String,
    payload: Vec<u8>,
    view: Option<serde_json::Value>,
    left: Duration,
) -> Result<Body, CallError> {
    if left.is_zero() {
        return Err(CallError::Hop {
            at: HopId {
                idx: 0,
                tag: "write",
            },
            reach: Reach::NotSent,
            why: HopFault::Overrun,
        });
    }
    let cancel = CancelToken::new();
    let fut = backends.call(origin, Op(op), Body(payload), view, left, cancel.clone());
    router::settle(fut, left, cancel).await
}

/// 主界面说 `call` 的那一条命令。`left_ms` = 这一跳还剩多少（TS 那侧由绝对时刻换算，过期交 0）。
/// `view` = 出口的声明（`chan.ts::call` 的那一格），原样交给句柄、进后端的请求信封；不进载荷。
///
/// 空白名当场按「用法错」拒（`Origin::route` 那道闸，同全仓吃 `Origin` 的命令）：
/// 空名交给句柄只会得到一句「没有控制通道」，那与真实原因（调用方没说哪台）毫无关系。
#[tauri::command]
pub async fn chan_call(
    origin: crate::origin::Origin,
    op: String,
    payload: Vec<u8>,
    view: Option<serde_json::Value>,
    left_ms: u64,
) -> Result<tauri::ipc::Response, Fail> {
    if origin.route("chan_call").is_err() {
        return Err(fail(&origin, &op, OursFault::Misuse.into()));
    }
    match call_via(
        &InboundBackends,
        origin.clone(),
        op.clone(),
        payload,
        view,
        Duration::from_millis(left_ms),
    )
    .await
    {
        Ok(body) => Ok(tauri::ipc::Response::new(body.0)),
        Err(e) => Err(fail(&origin, &op, e)),
    }
}

/// webview 手里那份能力事实：判断已在这边做完（`Offer` 的方法），TS 只查成员、不另算。
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct OfferView {
    /// 那台认的 op。
    ops: Vec<String>,
    /// `[op, 码, 那一句]`：这台做不到（码与那台事后会回的同一个；那一句是置灰时说的）。
    unavailable: Vec<(String, String, String)>,
}

/// `Offer` ⇒ webview 那一份。`origin` 只用来称呼那台。
pub fn offer_view(o: &super::wire::Offer, origin: &crate::origin::Origin) -> OfferView {
    let ops = o.ops().to_vec();
    let machine = super::said::machine(origin);
    OfferView {
        unavailable: ops
            .iter()
            .filter_map(|op| {
                o.unavailable(op).map(|c| {
                    (
                        op.clone(),
                        c.to_string(),
                        copy_core::unavailable(c, &machine),
                    )
                })
            })
            .collect(),
        ops,
    }
}

/// 主界面要那台机器的能力事实。与回环那条同一个句柄（`InboundBackends::offer`）；
/// `None` = 今天没有控制通道 / 空白名。
#[tauri::command]
pub fn chan_offer(origin: crate::origin::Origin) -> Option<OfferView> {
    origin.route("chan_offer").ok()?;
    InboundBackends
        .offer(&origin)
        .as_ref()
        .map(|o| offer_view(o, &origin))
}

// ════════════════════════════════════════════════════════════════════════════
// `subscribe`
// ════════════════════════════════════════════════════════════════════════════

/// 交给 webview 的事件名（`src/comms/inward/chan.ts` 按窗口作用域听它）。
pub const ITEMS_EVENT: &str = "chan-items";

/// 流里一格在 webview 这一跳上的样子（`src/comms/inward/chan.ts::decodeItem` 按它解；跨语言金样
/// `tests/__fixtures__/chan-webview-items.golden.json`）。体是文本（见模块头注）。
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub(crate) enum WebviewItem {
    Frame {
        seq: u64,
        body: String,
    },
    /// `to_seq` 缺 = 知道丢了、不知道丢到哪（`Item::Gap` 头注）。
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

/// 主界面说 `subscribe` 的那一条命令。**不回错**：说不了的在流里原位说。
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
