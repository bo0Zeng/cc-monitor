//! 问后端一条命令失败了怎么分层、交什么码、说哪一句的唯一一份（monitor 侧走后端的所有控制命令 · 手机核心 · 常驻转发共用）。
//! `CallError`（问后端失败的原因）、它的码（[`CallError::code`]，只取 `cmd_wire` 常量）与在等应答的上限 [`MAX_PENDING`]
//! 也住这里：客户端（`resident-client`）造它们，分层与那一句读它们 —— 一套词一个家。
//! 码不另写表：桌面 · 手机 · 常驻转发都读 [`CallError::code`]；壳里的发送端只走这里的出口，由 `every_backend_sender_is_registered_and_uses_the_one_router` 钉住（它遍历目录发现发送端，不靠手写清单）。
//!
//! # 分界线是「能不能证明这条命令根本没发出去」（[`provably_not_sent`]）
//!
//! | 档 | 在 `call` 里的位置 | 判定 |
//! |---|---|---|
//! | `local_client() == None` | 连 client 都没有，一个字节没发 | 证明没发出去 |
//! | `Unsupported` | `call` 第一行 `if !self.accepts(cmd)`，早于 `next_id`/`register`/`send` | 证明没发出去（旧后端） |
//! | `TooManyPending` | `register(&id)` 失败，仍早于 `writes.send` | 证明没发出去 |
//! | `Disconnected` | 两个产地：写队列 send 失败（没入队）或 等应答时 `rx` 掉了（已发出） | 分不开 ⇒ 按最坏算 ⇒ 说不准 |
//! | `Timeout` | 两个产地：写入段超时或 等应答段超时（可能已执行） | 分不开 ⇒ 按最坏算 ⇒ 说不准 |
//! | `Cancelled` / `Remote(..)` | backend 说过话了 | 那台的话照交 |
//!
//! 说不准的那几档，本侧写的那一句外面套「说不准」（[`route_call_error`]）；界面那一份 `chan-caller.ts::provablyNotSent` 照同一条线判。

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
    /// `code` 与那台事后会回的同一个（如 `no_tmux`），调用方那张「码 → 人话」表不用另写；`said` 是给人看的那一句：照那个码、按这条连接登记的那台的名字写（`copy_core::unavailable`，同那台置灰句一个函数）。
    /// **别重试**（换台机器或装上再连）。
    Unavailable {
        cmd: String,
        code: String,
        said: String,
    },
    /// typed 命令两端形状对不上（握手时那台交的形状指纹跟本侧编进来的那一份不一样，或它没交：比这一版早）⇒ **不发**，事前就拒。
    /// `said` 是给人看的那一句，由比指纹的这一层写好、带上那台的称呼（`peerVersion.said.differs`）。**别重试**（那台换了版本再连）。
    Differs { cmd: String, said: String },
    /// 本地超时。`withdraw` 说对端那一半：没发出去 / 已补发撤单（best-effort）/ 对端不认撤单（要说出来）。
    Timeout { after: Duration, withdraw: Withdraw },
    /// backend 回了 `ok:false`：它那一份失败原样透出（码 · 那一句 · 复制详情 · 按码定形的 `data`，同帧面失败应答）。
    Remote(cmd_wire::failed::Failed),
}

impl CallError {
    /// 这次失败交出去的码：本侧的那几种只取 `cmd_wire` 常量；那台交来码的（握手时说做不到 · 事后答失败）原样交那台的码。
    pub fn code(&self) -> &str {
        match self {
            CallError::Unsupported { .. } => cmd_wire::UNKNOWN_COMMAND,
            CallError::Differs { .. } => cmd_wire::SHAPE_DIFFERS,
            CallError::Unavailable { code, .. } => code,
            CallError::TooManyPending => cmd_wire::TOO_MANY_PENDING,
            CallError::Disconnected => cmd_wire::DISCONNECTED,
            CallError::Timeout { .. } => cmd_wire::TIMEOUT,
            CallError::Cancelled => cmd_wire::CANCELLED,
            CallError::Remote(r) => &r.code,
        }
    }
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // 这一层不知道那台叫什么：给人看的那句由 [`route_call_error`] 按调用方给的名字取。
            CallError::Unsupported { .. } => {
                write!(f, "{}", copy_core::backend_old(&copy_core::peer_machine()))
            }
            // 那一句是比指纹的那一层写好的（带那台的称呼）。
            CallError::Differs { said, .. } => write!(f, "{said}"),
            // 那一句是客户端事前拒时按那台的名字写好的（带缘由）；码不上屏（调用方要按码分支的读 `code` 本身）。
            CallError::Unavailable { said, .. } => write!(f, "{said}"),
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
                write!(f, "{}", copy_text("reason.op.cancelled", &[]))
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
            CallError::Remote(r) if r.said.trim().is_empty() => write!(
                f,
                "{}",
                copy_text("rsInboundClient.error.refusedNoReason", &[])
            ),
            CallError::Remote(r) => write!(
                f,
                "{}",
                copy_text("rsInboundClient.error.refused", &[("said", &r.said)])
            ),
        }
    }
}

/// 分层判定的唯一一份：`inbound_client::CallError` ⇒ 三层（传输错 · 对端错 · 我们自己错）＋ `reach` 三档 ＋ `why`。
/// 分层只此一份：给人看的那一句 [`route_call_error`] 建在它上面，通道的生产句柄（`chan/host.rs`）直接用它。
/// `hop` 是调用方那一侧给这一跳的编号（`HopId.idx`）。
///
/// | 进来的 | 分层结果 | 理由（与模块头注那张表逐档对应） |
/// |---|---|---|
/// | `Unsupported` | `Peer{Unsupported}` | 对端事前就说不认（`hello.commands`），一个字节没发 |
/// | `Unavailable{code}` | `Peer{Refused{body}}`，body = `{"code","said","detail"}`（详情空） | 对端握手时说过这台做不到（`hello.unavailable`），本侧没发；同对端事后回那个码 |
/// | `Differs{said}` | `Peer{Refused{body}}`，body = `{"code": "backend_differs", "said", "detail"}`（详情空） | typed 命令两端形状指纹对不上，本侧没发；那一句本层写好 |
/// | `TooManyPending` | `Hop{write, NotSent, Full}` | 本侧在等的满了，早于入队 |
/// | `Disconnected` | `Hop{read, Unknown, Dropped}` | 两个产地分不开 ⇒ 拿不准一律 `Unknown` |
/// | `Timeout` | `Hop{wait, Unknown, Overrun}` | 同上 |
/// | `Cancelled` | `Ours{Cancelled}` | 撤单源自本侧（后端只是确认了它）；副作用状态未知，不是回滚 |
/// | `Remote(失败)` | `Peer{Refused{body}}`，body = 那一份失败（`cmd_wire::failed::Failed`）的 JSON | 对端说了话；body 对通道不透明 |
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
        //   调用方按码说人话。
        CallError::Unavailable { code, .. } => Layered {
            error: w::CallError::Peer {
                why: w::PeerFault::Refused {
                    body: w::Body(refusal_body(&cmd_wire::failed::Failed {
                        code: code.clone(),
                        said: e.to_string(),
                        ..Default::default()
                    })),
                },
            },
            detail: Detail::Remote {
                code: code.clone(),
                said: e.to_string(),
            },
        },
        // 两端形状对不上：同 `Unavailable` 那一形（对端的话来得早、本侧没发），码 `cmd_wire::SHAPE_DIFFERS`，那一句是本层写好的。
        CallError::Differs { said, .. } => Layered {
            error: w::CallError::Peer {
                why: w::PeerFault::Refused {
                    body: w::Body(refusal_body(&cmd_wire::failed::Failed {
                        code: cmd_wire::SHAPE_DIFFERS.to_string(),
                        said: said.clone(),
                        ..Default::default()
                    })),
                },
            },
            detail: Detail::Remote {
                code: cmd_wire::SHAPE_DIFFERS.to_string(),
                said: said.clone(),
            },
        },
        CallError::TooManyPending => Layered {
            error: w::CallError::Hop {
                at: at("write"),
                reach: w::Reach::NotSent,
                why: w::HopFault::Full,
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
        CallError::Remote(r) => Layered {
            error: w::CallError::Peer {
                why: w::PeerFault::Refused {
                    body: w::Body(refusal_body(r)),
                },
            },
            detail: Detail::Remote {
                code: r.code.clone(),
                said: r.said.clone(),
            },
        },
    }
}

/// 对端拒绝体：就是那一份失败（`{code, said, detail, data?}`）。
fn refusal_body(f: &cmd_wire::failed::Failed) -> Vec<u8> {
    serde_json::to_vec(f).unwrap_or_default()
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
    Remote { code: String, said: String },
    /// 那台不认这条命令：按码取那一句（`copy_core::backend_old`），名字由 [`route_call_error`] 的调用方给。
    BackendOld,
}

/// 没有控制通道（`local_client` 回 `None`）的分层结果 —— **一个字节都没发出去**。
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

/// 分层结果**能不能证明这条命令没发出去**（`reach: NotSent`，或对端事前就说不认）。与界面那一份 `chan-caller.ts::provablyNotSent` 对拍。
pub fn provably_not_sent(error: &w::CallError) -> bool {
    match error {
        w::CallError::Hop { reach, .. } => match reach {
            w::Reach::NotSent => true,
            w::Reach::Sent | w::Reach::Unknown => false,
        },
        w::CallError::Peer { why } => match why {
            w::PeerFault::Unsupported => true,
            w::PeerFault::Refused { .. } => false,
        },
        w::CallError::Ours { .. } => false,
    }
}

/// `CallError` → 给人看的那一句 —— **建在 [`layer_call_error`] 上**，自己不再看 inbound 的枚举。
/// `machine` 是问的那一台给人看的称呼（本机 ⇒ `copy_core::local_machine()`）：那台不认这条命令时说「{machine} 后端要更新」。
/// `refusal` 只负责把对端的 `(code, said)` 翻成用户看的话。
///
/// 本侧写好的那一句：能证明没发出去（[`provably_not_sent`]）⇒ 原样；拿不准发没发出去 ⇒ 套「说不准」那一句。
pub fn route_call_error(
    e: &CallError,
    machine: &str,
    refusal: impl Fn(&str, &str) -> String,
) -> String {
    let Layered { error, detail } = layer_call_error(e, 0);
    match detail {
        Detail::BackendOld => copy_core::backend_old(machine),
        Detail::Text(s) if provably_not_sent(&error) => s,
        Detail::Text(s) => copy_text("rsBackendRoute.route.unsure", &[("s", &s)]),
        Detail::Remote { code, said } => refusal(&code, &said),
    }
}

/// 没有控制通道（`local_client` 回 `None`）—— **一个字节都没发出去**：那台离线那一句。
pub fn no_channel(origin: &str) -> String {
    copy_core::offline(origin)
}

#[cfg(test)]
#[path = "../../../tests/comms/inward/backend_route_tests.rs"]
mod tests;
