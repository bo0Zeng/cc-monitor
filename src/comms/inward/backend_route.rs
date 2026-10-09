//! 「这条命令能不能回落」的唯一判定（monitor 侧走后端的所有控制命令共用）。`CallError`（问后端失败的原因）与在等应答的上限 [`MAX_PENDING`]
//! 也住这里：客户端（壳里 `inbound_client.rs`）造它们，分流规则读它们 —— 一套词一个家。
//!
//! 只能有一份：两份实现会漂，而漂开的后果是静默的权限旁路 —— 把后端的一次 `wrong_owner` 当成「backend 不可用」回落到另一条路再做一次，
//! 等于把一次被门拒绝洗成成功。由 `every_backend_sender_is_registered_and_uses_the_one_router` 钉住（它遍历目录发现发送端，不靠手写清单）。
//!
//! # 分界线是「能不能证明这条命令根本没发出去」
//!
//! | 档 | 在 `call` 里的位置 | 判定 |
//! |---|---|---|
//! | `client_for(origin) == None` | 连 client 都没有，一个字节没发 | 证明没发出去 ⇒ 可回落 |
//! | `Unsupported` | `call` 第一行 `if !self.accepts(cmd)`，早于 `next_id`/`register`/`send` | 证明没发出去 ⇒ 可回落（旧后端） |
//! | `TooManyPending` | `register(&id)` 失败，仍早于 `writes.send` | 证明没发出去 ⇒ 可回落 |
//! | `Disconnected` | 两个产地：写队列 send 失败（没入队）或 等应答时 `rx` 掉了（已发出） | 分不开 ⇒ 按最坏算 ⇒ 不回落 |
//! | `Timeout` | 两个产地：写入段超时或 等应答段超时（可能已执行） | 分不开 ⇒ 按最坏算 ⇒ 不回落 |
//! | `Cancelled` / `Remote{..}` | backend 说过话了 | 不回落 |
//!
//! `Timeout` 与 `Disconnected` 的两个产地类型上分不开 ⇒ 一次写入段超时会让用户拿到错误而不是回落；要修得在 `inbound_client` 那边分成两个变体。

use crate::chan::wire::{self as w, Withdraw};
use copy_core::copy_text;
use std::time::Duration;

/// 同一条连接上**同时在等应答**的命令数上限。
///
/// 超时**不摘登记**（见 [`InboundClient::call`]），所以一个死掉但没断连的 backend
/// 会让登记表只涨不落。这条上限把它变成「新命令快速失败」而不是「内存无界增长」。
/// 取值与后端侧应答通道容量同量级（后端侧应答通道容量
/// `REPLY_CHANNEL_CAPACITY = 256`）—— 那头一次也只缓 256 条应答。
pub const MAX_PENDING: usize = 256;

/// 一次调用失败的原因。**每一档都要能让调用方分辨「该重试」还是「别重试」。**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// backend 在 `hello.commands` 里没声明这条命令（含旧后端：无该字段 ⇒ 空集）。
    /// 客户端侧直接拒，省一次往返 + 一次超时。**别重试**。
    Unsupported { cmd: String, offered: Vec<String> },
    /// 同时在等的命令已达 [`MAX_PENDING`]。**可稍后重试**。
    TooManyPending,
    /// 连接（或写任务）已经没了。**重连后重试**。
    Disconnected,
    /// backend 回了 `{"kind":"cancelled"}`。
    Cancelled,
    /// 握手时那台说过「这条我接得下、这台做不到」（`hello.unavailable`）⇒ **不发**，事前就拒。
    /// `code` 与那台事后会回的同一个（如 `no_tmux`），调用方那张「码 → 人话」表不用另写。**别重试**（换台机器或装上再连）。
    Unavailable { cmd: String, code: String },
    /// 本地超时。`withdraw` 说对端那一半：没发出去 / 已补发撤单（best-effort）/ 对端不认撤单（要说出来）。
    Timeout { after: Duration, withdraw: Withdraw },
    /// backend 回了 `ok:false`。`code`/`message` 原样透出（形状对齐 `--resolve` 的错误契约）；
    /// `data` ＝ 那几个按码定了形的码带的 `data`（序列化好的 JSON 原文；别的码 ⇒ `None`）。
    Remote {
        code: String,
        message: String,
        /// 「复制详情」那几行（那台后端写好；老后端没有 ⇒ 空串）。
        detail: String,
        data: Option<String>,
    },
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // 这一层不知道那台叫什么：给人看的那句由 [`route_call_error`] 按调用方给的名字取。
            CallError::Unsupported { .. } => {
                write!(f, "{}", copy_core::backend_old(&copy_core::peer_machine()))
            }
            // 码不上屏（调用方要按码分支的读 `code` 本身）。
            CallError::Unavailable { .. } => {
                write!(f, "{}", copy_text("rsInboundClient.error.unavailable", &[]))
            }
            CallError::TooManyPending => write!(
                f,
                "{}",
                copy_text(
                    "rsInboundClient.error.tooMany",
                    &[("max", &MAX_PENDING.to_string())]
                )
            ),
            CallError::Disconnected => {
                write!(f, "{}", copy_text("rsInboundClient.error.closed", &[]))
            }
            CallError::Cancelled => {
                write!(f, "{}", copy_text("rsInboundClient.error.cancelled", &[]))
            }
            CallError::Timeout { after, withdraw } => {
                let dur = copy_core::format_elapsed(*after);
                let said = match withdraw {
                    Withdraw::Unsent | Withdraw::Asked => {
                        copy_text("rsInboundClient.error.timeout", &[("dur", &dur)])
                    }
                    // 对端不认撤单：本地照撤，结果里说出来。
                    Withdraw::NotOffered => {
                        copy_text("rsInboundClient.error.timeoutPeerRunsOn", &[("dur", &dur)])
                    }
                };
                write!(f, "{said}")
            }
            // 那台的原话；码不上屏（调用方要按码分支的读 `code` 本身）。
            CallError::Remote { message, .. } if message.trim().is_empty() => write!(
                f,
                "{}",
                copy_text("rsInboundClient.error.refusedNoReason", &[])
            ),
            CallError::Remote { message, .. } => write!(
                f,
                "{}",
                copy_text(
                    "rsInboundClient.error.refused",
                    &[("message", &message.to_string())]
                )
            ),
        }
    }
}

/// 一条走后端的控制命令**失败时**的结局，分界线见模块头注（成功那一态由调用方的 `Ok` 自己装，这里不另设）。
#[derive(Debug, PartialEq, Eq)]
pub enum Routed {
    /// 证明这条命令没发出去 ⇒ 调用方可以回落。带上原因只为诊断，不参与分流判断。
    NoChannel(String),
    /// backend 说了话（拒绝 / 失败），**或者**我们无法证明它没执行 ⇒
    /// **不许回落**，把这句话原样交给用户。
    Refused(String),
}

/// 分层判定的唯一一份：`inbound_client::CallError` ⇒ 三层（传输错 · 对端错 · 我们自己错）＋ `reach` 三档 ＋ `why`。
/// 全仓唯一 `match` `inbound_client::CallError` 的地方：三态的 [`route_call_error`] 从它收拢出来，通道的生产句柄（`chan/host.rs`）直接用它。
/// `hop` 是调用方那一侧给这一跳的编号（`HopId.idx`）。
///
/// | 进来的 | 分层结果 | 理由（与模块头注那张表逐档对应） |
/// |---|---|---|
/// | `Unsupported` | `Peer{Unsupported}` | 对端事前就说不认（`hello.commands`），一个字节没发 |
/// | `Unavailable{code}` | `Peer{Refused{body}}`，body = `{"code","message"}` | 对端握手时说过这台做不到（`hello.unavailable`），本侧没发；同对端事后回那个码 |
/// | `TooManyPending` | `Hop{write, NotSent, Overrun}` | 本侧在飞上限顶满，早于入队 |
/// | `Disconnected` | `Hop{read, Unknown, Dropped}` | 两个产地分不开 ⇒ 拿不准一律 `Unknown` |
/// | `Timeout` | `Hop{wait, Unknown, Overrun}` | 同上 |
/// | `Cancelled` | `Ours{Cancelled}` | 撤单源自本侧（后端只是确认了它）；副作用状态未知，不是回滚 |
/// | `Remote{code,message}` | `Peer{Refused{body}}`，body = `{"code","message"}` 的 JSON | 对端说了话；body 对通道不透明 |
///
/// 附带的 [`Detail`] 是给人看的那句话的原料，不参与任何分流判断（分流只看 `error`）。
pub fn layer_call_error(e: &CallError, hop: u8) -> Layered {
    let at = |tag: &'static str| w::HopId { idx: hop, tag };
    let text = |s: String| Detail::Text(s);
    match e {
        CallError::Unsupported { .. } => Layered {
            error: w::CallError::Peer {
                why: w::PeerFault::Unsupported,
            },
            detail: Detail::BackendOld,
        },
        // 那台握手时说过做不到、本侧没发：对端的话（只是来得早）⇒ 与它事后回同一个码时同形，
        //   调用方按码说人话；不回落（换条路也做不到）。
        CallError::Unavailable { code, .. } => Layered {
            error: w::CallError::Peer {
                why: w::PeerFault::Refused {
                    body: w::Body(
                        serde_json::to_vec(
                            &serde_json::json!({ "code": code, "message": e.to_string() }),
                        )
                        .unwrap_or_default(),
                    ),
                },
            },
            detail: Detail::Remote {
                code: code.clone(),
                message: e.to_string(),
            },
        },
        CallError::TooManyPending => Layered {
            error: w::CallError::Hop {
                at: at("write"),
                reach: w::Reach::NotSent,
                why: w::HopFault::Overrun,
            },
            detail: text(copy_text("rsBackendRoute.layer.tooMany", &[]).into()),
        },
        CallError::Disconnected => Layered {
            error: w::CallError::Hop {
                at: at("read"),
                reach: w::Reach::Unknown,
                why: w::HopFault::Dropped,
            },
            detail: text(e.to_string()),
        },
        CallError::Timeout { .. } => Layered {
            error: w::CallError::Hop {
                at: at("wait"),
                reach: w::Reach::Unknown,
                why: w::HopFault::Overrun,
            },
            detail: text(e.to_string()),
        },
        CallError::Cancelled => Layered {
            error: w::OursFault::Cancelled.into(),
            detail: text(e.to_string()),
        },
        CallError::Remote {
            code,
            message,
            detail,
            data,
        } => Layered {
            error: w::CallError::Peer {
                why: w::PeerFault::Refused {
                    body: w::Body(refusal_body(code, message, detail, data.as_deref())),
                },
            },
            detail: Detail::Remote {
                code: code.clone(),
                message: message.clone(),
            },
        },
    }
}

/// 对端拒绝体 `{code, message}`；带了 `data` 的那几个码再多一格 `data`；那台写了详情 ⇒ 再多一格 `detail`。
fn refusal_body(code: &str, message: &str, detail: &str, data: Option<&str>) -> Vec<u8> {
    let mut v = serde_json::json!({ "code": code, "message": message });
    if !detail.trim().is_empty() {
        v["detail"] = serde_json::Value::String(detail.to_string());
    }
    if let Some(d) = data.and_then(|d| serde_json::from_str::<serde_json::Value>(d).ok()) {
        v["data"] = d;
    }
    serde_json::to_vec(&v).unwrap_or_default()
}

/// [`layer_call_error`] 的产出：`05` 的分层错误 ＋ 给人看的那句话的原料。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layered {
    /// 三层。**分流只看它。**
    pub error: w::CallError,
    /// 那句话的原料 —— 不参与分流。
    pub detail: Detail,
}

/// 给人看的那句话的原料。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detail {
    /// 已经说成一句话了。
    Text(String),
    /// 对端的原话，由调用方的 `refusal` 翻成用户看的话。
    Remote { code: String, message: String },
    /// 那台不认这条命令：按码取那一句（`copy_core::backend_old`），名字由 [`route_call_error`] 的调用方给。
    BackendOld,
}

/// 没有控制通道（`client_for` 回 `None`）的分层结果 —— **一个字节都没发出去**。
pub fn layer_no_channel(hop: u8) -> w::CallError {
    w::CallError::Hop {
        at: w::HopId {
            idx: hop,
            tag: "open",
        },
        reach: w::Reach::NotSent,
        why: w::HopFault::Unreachable,
    }
}

/// `CallError` → 三态 —— **建在 [`layer_call_error`] 上的一层收拢**，自己不再看 inbound 的枚举。
/// `machine` 是问的那一台给人看的称呼（本机 ⇒ `copy_core::local_machine()`）：那台不认这条命令时说「{machine} 后端要更新」。
/// `refusal` 只负责把 `(code, message)` 翻成用户看的话 ——
/// **分流本身不许由调用方决定**，那就是本模块存在的全部意义。
///
/// 收拢规则只有一条：分层结果**能证明没发出去**（`reach: NotSent`，或对端事前就说不认）
/// ⇒ `NoChannel`（可回落）；其余一律 `Refused`（不回落）。
/// 收拢前后逐字节不变由 `the_collapse_to_three_states_is_byte_identical_to_the_table_before_layering` 钉着。
pub fn route_call_error(
    e: &CallError,
    machine: &str,
    refusal: impl Fn(&str, &str) -> String,
) -> Routed {
    let Layered { error, detail } = layer_call_error(e, 0);
    let provably_not_sent = match error {
        w::CallError::Hop { reach, .. } => match reach {
            w::Reach::NotSent => true,
            w::Reach::Sent | w::Reach::Unknown => false,
        },
        w::CallError::Peer { why } => match why {
            w::PeerFault::Unsupported => true,
            w::PeerFault::Refused { .. } => false,
        },
        w::CallError::Ours { .. } => false,
    };
    match (provably_not_sent, detail) {
        (_, Detail::BackendOld) => Routed::NoChannel(copy_core::backend_old(machine)),
        (true, Detail::Text(s)) => Routed::NoChannel(s),
        (true, Detail::Remote { code, message }) => Routed::NoChannel(refusal(&code, &message)),
        (false, Detail::Text(s)) => Routed::Refused(copy_text(
            "rsBackendRoute.route.unsure",
            &[("s", &s.to_string())],
        )),
        (false, Detail::Remote { code, message }) => Routed::Refused(refusal(&code, &message)),
    }
}

/// 没有控制通道（`client_for` 回 `None`）—— **一个字节都没发出去**，可回落。
pub fn no_channel(origin: &str) -> Routed {
    Routed::NoChannel(copy_text(
        "rsBackendRoute.noChannel.message",
        &[("origin", &origin.to_string())],
    ))
}

#[cfg(test)]
#[path = "../../../tests/comms/inward/backend_route_tests.rs"]
mod tests;
