//! 通道 · **线上词汇与拆帧**：`call` / `subscribe` 两个动作要用到的全部类型，以及它们在一条字节流上的样子。
//!
//! # 🔴 通信层成员 `COMM-LAYER-MEMBER`〔面 A 第一个外部客户端，2026-09-24〕
//!
//! 这一枚标记是**盘上那一侧**的凭据（登记那一侧在
//! `tests/frontend/shell/comm_boundary_registry_tests.rs` 的 `REGISTERED`，两向集合相等）。
//! 盖上它 = **上锁**：本文件从此被 `C1`–`C5` ＋ `X1`–`X6` 十一条一起管着。
//!
//! **凭什么它属于通信层**：那「五个不透明类型 ＋ 一个手柄 ＋ 一个跳号」
//! 与 `§3.3.1` 的三层错误、`§3.3.4` 的 `Item` 就住在这里 —— 它们**就是**这一层的词汇，
//! 每一个名字都是位置名或传输词，一个业务词都没有（用**位置**称呼它搬的东西）。
//!
//! # 线上长什么样
//!
//! ```text
//! 一帧 = [u32 头长 BE][头：一段 JSON][u32 体长 BE][体：原样字节]
//! ```
//!
//! - **头**只装地址、名字、编号、错误分层这些**小东西**；
//! - **体**装那份不透明载荷 —— 它从不进 JSON，一个字节都不改写（`§2`：载荷是不透明字节）。
//! - 两段各有一个上限，**上限的值由宿主交进来**（`C4`：配置由后端交给它），本文件一个尺寸常量都没有。
//!
//! # 期限在线上是「还剩多少」
//!
//! `§3.3.2` 逐字：「跨机那一段线上传的当然是"还剩多少"（对端没有我们的钟），
//! 但**总时限由我们这侧的绝对时刻兜底**」。⇒ 客户端手里的 [`Budget`] 是绝对时刻，
//! 过线时换成 [`Budget::remaining`]；对端拿到的是一段时长，它**不许**把它当成新的绝对时刻往下传放宽。
//!
//! # 买到什么
//!
//! - 通道的签名在盘上有了类型：`Budget`（绝对时刻 ＋ 撤单手柄）·
//!   `CallError` 三层 · `Reach` 三档 · `Item` 五个变体（`Unseen` 与 `Closed` 是两个不同的变体）。
//! - 载荷不进 JSON ⇒ 通道对载荷的形状**零假设**。
//! - 能力协商的家（`Offer` · `Withdraw`）：对端握手交出的 op 集只在这里被问；
//!   撤单也是其中一条能力，不认就本地照撤并说出来（`§3.3.3`）。
//!
//! # 买不到什么
//!
//! - **不买「对端一定按这份协议说话」**：解不出来的头一律是 [`OursFault::Broken`]，
//!   不猜、不补默认值。
//! - **不买版本协商**：头里没有协议版本号。今天两端都编自同一个 `chan-core`（从前是同一个 `monitor_lib`），
//!   将来两端能分开升级那一天要补 —— 登记为欠账，不假装已有。
//! - **不买 `HopId.tag` 的开放集合**：线上只认 `open | auth | write | read | wait` 这五个
//!   （`§3.3.0` 逐字），认不出的标签当作协议坏了，不当作新标签收下。

use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::watch;

pub use crate::origin::Origin;

// ════════════════════════════════════════════════════════════════════════════
//  一、五个不透明类型—— 通信层自己也不解释它们的内容
// ════════════════════════════════════════════════════════════════════════════

/// 操作名 —— 不透明串。路由器只拿它当键转交，从不比较它的内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op(pub String);

/// 流标签 —— 不透明串。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kind(pub String);

/// 载荷 —— 不透明字节。线上走帧体，不进 JSON。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Body(pub Vec<u8>);

/// 续传游标 —— 上层给、原样带回（「按字节偏移续传」在这里是不透明的）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor(pub Vec<u8>);

// ════════════════════════════════════════════════════════════════════════════
//  二、期限与撤单：一次调用一个手柄
// ════════════════════════════════════════════════════════════════════════════

/// **本地撤单**的手柄。克隆出去的每一份都指同一个开关，任何一份拨下去全体可见。
///
/// ⚠ 它只管**本地**：拨下去 ⇒ 在飞的那一格当场回收、调用方拿到 `Ours{Cancelled}`。
/// 「对端停没停」是另一件事（`§3.3.3`：对端撤活只是尽力），不在这个类型里。
#[derive(Clone, Debug)]
pub struct CancelToken(Arc<watch::Sender<bool>>);

impl Default for CancelToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancelToken {
    /// 一个还没拨下去的开关。
    pub fn new() -> Self {
        Self(Arc::new(watch::channel(false).0))
    }

    /// 拨下去。重复拨无副作用。
    pub fn cancel(&self) {
        self.0.send_replace(true);
    }

    /// 拨下去了没有。
    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }

    /// 等到它被拨下去。**拨之前永不返回**（发送端在 `self` 里握着，不会先于它消失）。
    pub async fn cancelled(&self) {
        let mut rx = self.0.subscribe();
        // `wait_for` 只在发送端没了时出错 —— 而发送端就在 `self` 里，这一支到不了。
        rx.wait_for(|c| *c).await.ok();
    }
}

/// 一次调用的**全部**时间与撤单预算。
///
/// 🔴 `until` 是**绝对时刻**，不是 `Duration`（`§3.3.2`：`Duration` 跨跳传递时每一跳都会
/// 重新开始计时 —— 那正是「无总时限」那条病的机制）。**调用者给，通信层不造**。
#[derive(Clone, Debug)]
pub struct Budget {
    /// 过了这一刻，这次调用一定已经回来了（成功或 `Overrun`）。
    pub until: Instant,
    /// 本地撤单。
    pub cancel: CancelToken,
}

impl Budget {
    /// 离期限还剩多少（已过则为零）。**只在过线那一下用**：线上传的是「还剩多少」。
    pub fn remaining(&self) -> Duration {
        self.until.saturating_duration_since(Instant::now())
    }

    /// 同一个绝对时刻，换成 tokio 的钟面（给 `timeout_at` 用）。**不改时刻，只换类型。**
    pub fn deadline(&self) -> tokio::time::Instant {
        tokio::time::Instant::from_std(self.until)
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  三、每一跳的身份 ＋ 错误三层
// ════════════════════════════════════════════════════════════════════════════

/// 每一跳的身份 —— 位置 ＋ 传输动作，没有业务名。
///
/// `idx` 在本通道上的读法：`0` = 外部前端 ↔ 路由器那一跳（这条回环连接）；
/// `1` = 路由器 ↔ 后端那一跳（经宿主注入的句柄）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HopId {
    /// 第几跳。
    pub idx: u8,
    /// 卡在这一跳的哪个动作上：`open | auth | write | read | wait`。
    pub tag: &'static str,
}

/// `HopId.tag` 的全部合法取值（那五个）。
pub const HOP_TAGS: [&str; 5] = ["open", "auth", "write", "read", "wait"];

/// 线上的标签串 ⇒ 静态串。认不出 ⇒ `None`（协议坏了，不是一个新标签）。
fn tag_of(s: &str) -> Option<&'static str> {
    HOP_TAGS.into_iter().find(|t| *t == s)
}

/// 一次调用失败的原因 —— **三层，穷尽 `match`，不许 `_ =>`**（`X1`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// ① 传输错 —— 我们与那台机器之间的通道出了事。
    Hop {
        at: HopId,
        reach: Reach,
        why: HopFault,
    },
    /// ② 对端错 —— 通道是通的；它收到了并答了「不行」，或它事前就说不认。
    Peer { why: PeerFault },
    /// ③ 我们自己错 —— 与对面无关。
    /// `runs_on`：本地撤单（`Cancelled`）时那台对这一条不认撤 ⇒ 那件事可能还在跑（「在结果里说明」）。
    /// 由手里有那台 `Offer` 的一方填（回环客户端 · TS）；别的 `why` 恒 `false`。
    Ours { why: OursFault, runs_on: bool },
}

/// 🔴 只答「我们发没发出去」，不答「对面做没做」。**拿不准一律 `Unknown`。**
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reach {
    NotSent,
    Sent,
    Unknown,
}

/// 传输错的三种。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HopFault {
    /// 连不上 / 没有这条路。
    Unreachable,
    /// 路断在半途。
    Dropped,
    /// 期限到了（或容量顶满）。
    Overrun,
}

/// 对端错的两种。`body` 不透明，原样上交 —— 通信层一解释就撞 `C1`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerFault {
    Unsupported,
    Refused { body: Body },
}

/// 我们自己错的三种。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OursFault {
    /// 本地撤单。**不是回滚**：那次 `op` 的副作用状态是 `Unknown`。
    Cancelled,
    /// 用法错。
    Misuse,
    /// 内部不变量破了（含：对端说的话解不出来）。
    Broken,
}

/// 「我们自己错」那一层的简写：`OursFault::Misuse.into()`。
impl From<OursFault> for CallError {
    fn from(why: OursFault) -> Self {
        CallError::Ours {
            why,
            runs_on: false,
        }
    }
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // 照 CP1 台账改：不再把「第 N 跳」与枚举名（{:?}）原样上屏，每一格给一句人话；
            // 跳号与标签留给日志（调用方要细节时自己拿 `at` / `reach`）。
            CallError::Hop {
                why,
                reach: _,
                at: _,
            } => f.write_str(&match why {
                HopFault::Unreachable => copy_text("rsChanWire.hop.unreachable", &[]),
                HopFault::Dropped => copy_text("rsChanWire.hop.dropped", &[]),
                HopFault::Overrun => copy_text("rsChanWire.hop.overrun", &[]),
            }),
            CallError::Peer { why } => f.write_str(&match why {
                PeerFault::Unsupported => copy_text("rsChanWire.peer.unsupported", &[]),
                PeerFault::Refused { .. } => copy_text("rsChanWire.peer.refused", &[]),
            }),
            CallError::Ours { why, runs_on } => f.write_str(&match why {
                OursFault::Cancelled if *runs_on => {
                    copy_text("rsChanWire.ours.cancelledRunsOn", &[])
                }
                OursFault::Cancelled => copy_text("rsChanWire.ours.cancelled", &[]),
                OursFault::Misuse => copy_text("rsChanWire.ours.misuse", &[]),
                OursFault::Broken => copy_text("rsChanWire.ours.broken", &[]),
            }),
        }
    }
}

impl std::error::Error for CallError {}

// ════════════════════════════════════════════════════════════════════════════
//  四、流里走的东西
// ════════════════════════════════════════════════════════════════════════════

/// 订阅流里的一格。🔴 `Unseen` **不是终点**，通信层里没有任何一条路径把它变成 `Closed`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// 不透明载荷。
    Frame { seq: u64, body: Body },
    /// 丢在哪两个序号之间 —— **原位**出现，不是旁路通知。
    /// `to_seq == None`：知道这里丢了、不知道丢到哪（例：一行超长、整行没读进来，
    /// 说不出是哪个会话的哪一行）；不占序号。接收侧的补法：从已见最大 seq ＋ 1 往后取。
    Gap { from_seq: u64, to_seq: Option<u64> },
    /// 那台机器现在看不见。
    Unseen { at: HopId, why: HopFault },
    /// 又看得见了，以及它从哪续上。
    Seen { from: Option<Cursor> },
    /// 真的结束了。
    Closed { by: By },
}

/// 谁把流关了。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum By {
    Peer(Body),
    Ours(OursFault),
}

// ════════════════════════════════════════════════════════════════════════════
//  四b、前端只有两个动作（`Comms` / `Sub`，参数名与顺序一字不改）
// ════════════════════════════════════════════════════════════════════════════

/// 前端对通信层的**全部**说法。
///
/// ⚠ 与对照：参数名与顺序一字不改；`async fn` 写成
/// 「返回一个 `Send` 的 `Future`」—— 同一个签名，只是让它能被多线程运行时驱动。
pub trait Comms: Send + Sync {
    /// 一次性请求。失败一定是一个 `CallError`，永远不会是「返回一个空答案」。
    fn call(
        &self,
        origin: &Origin,
        op: &Op,
        payload: Body,
        budget: Budget,
    ) -> impl std::future::Future<Output = Result<Body, CallError>> + Send;

    /// 订阅。🔴 **它不返回 `Result`** —— 订阅一台「现在看不见」的机器是合法的。
    /// ⚠ 返回的流**不借**任何实参（`use<Self>`）：订阅是长期意向，不该被一次调用的借用拴住。
    fn subscribe(
        &self,
        origin: &Origin,
        kind: &Kind,
        from: Option<Cursor>,
        want: u32,
    ) -> impl Sub + use<Self>;
}

/// 一条订阅：一条 `Item` 流 ＋ 两个往回说的动作。
pub trait Sub: futures::Stream<Item = Item> + Send {
    /// 背压信号：订阅方报「我还能吃多少」（credit，累加）。
    fn want(&self, more: u32);
    /// 撤订阅 —— 与 `call` 的撤单同义。
    fn stop(&self);
}

// ════════════════════════════════════════════════════════════════════════════
//  五、认证钥匙
// ════════════════════════════════════════════════════════════════════════════

/// 连接者要出示的那把钥匙。**本类型不打印内容**（`Debug` 手写成占位），
/// 为的是它永远不会被一条 `tracing!("{:?}")` 顺手带进日志。
///
/// ⚠ 钥匙从哪来、怎么交到外部前端手里，不归本文件（`C4`：它由宿主交给通信层）。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Key(pub String);

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(<不打印>)")
    }
}

impl Key {
    /// 比对两把钥匙。**逐字节全比完再答**，不在第一个不等处提前返回
    /// —— 回环口上任何本机进程都能来试，提前返回会把「前几个字节对了」泄给计时。
    pub fn matches(&self, other: &str) -> bool {
        let a = self.0.as_bytes();
        let b = other.as_bytes();
        let mut diff = u8::from(a.len() != b.len());
        for (i, x) in a.iter().enumerate() {
            diff |= x ^ b.get(i).copied().unwrap_or(0);
        }
        diff == 0
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  五b、能力协商—— 面 A 上「对端认不认」的唯一答处
// ════════════════════════════════════════════════════════════════════════════

/// 撤单那条 op 的名字（对端握手时交出的 op 集里有它 ⇒ 认撤单）。
pub const WITHDRAW_OP: &str = "cancel";

/// 对端对补发的撤单回这个码 ⇒ 那一条它停不下来（阻塞档），会照跑完。
pub const WITHDRAW_REFUSED: &str = "not_cancellable";

/// 对端握手时交出的能力事实：接哪些 op · 哪几条在这台做不到（附码）· 哪几条撤不动。
/// 一条连接一份；「认不认 / 做不做得到 / 撤不撤得动」只问它。外部前端与 webview 拿的是它的拷贝（序列化形）。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Offer {
    ops: Vec<String>,
    /// `(op, 码)`：接得下、这台做不到；码与对端事后会回的同一个。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    unavailable: Vec<(String, String)>,
    /// 撤不动的 op（对端开跑之后停不下）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    uncancellable: Vec<String>,
}

/// 本地撤单之后，对端那一半怎样了（本地照撤；对端不认要说出来）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Withdraw {
    /// 请求还没出本侧，对端没见过它 —— 没什么可撤。
    Unsent,
    /// 对端认撤这一条，已补发（尽力；旧对端没说哪几条撤不动，停不下的那一条由它回 [`WITHDRAW_REFUSED`] 说）。
    Asked,
    /// 🔴 对端不认撤这一条（没交出撤单 op，或它在撤不动那一栏）⇒ 一帧都不补，那件事可能照跑完。
    NotOffered,
}

impl Offer {
    /// 对端握手时交出的三格，原样收下。
    pub fn new(
        ops: Vec<String>,
        unavailable: Vec<(String, String)>,
        uncancellable: Vec<String>,
    ) -> Self {
        Self {
            ops,
            unavailable,
            uncancellable,
        }
    }

    /// 对端认不认这个 op。
    pub fn admits(&self, op: &str) -> bool {
        self.ops.iter().any(|o| o == op)
    }

    /// 对端交出的全部 op（报「不认」时附上，给人看它认什么）。
    pub fn ops(&self) -> &[String] {
        &self.ops
    }

    /// 这台做不到这个 op ⇒ 那个码（与对端事后会回的同一个）；做得到 / 没把握 ⇒ `None`。
    pub fn unavailable(&self, op: &str) -> Option<&str> {
        self.unavailable
            .iter()
            .find(|(o, _)| o == op)
            .map(|(_, code)| code.as_str())
    }

    /// 这个 op 的请求**已发出**、被本地撤掉之后，对端那一半的处置。
    pub fn withdraw(&self, op: &str) -> Withdraw {
        if self.admits(WITHDRAW_OP) && !self.uncancellable.iter().any(|o| o == op) {
            Withdraw::Asked
        } else {
            Withdraw::NotOffered
        }
    }

    /// JSON 对象里 `unavailable`（`[{command, code}]`）/ `uncancellable`（`[op]`）两格 —— hello 与 `resync` 应答同形，
    /// 读法只此一份。缺格 ⇒ 空（没把握）；坏项逐项丢，不丢整份。
    pub fn facts_of(
        obj: &serde_json::Map<String, serde_json::Value>,
    ) -> (Vec<(String, String)>, Vec<String>) {
        let items = |k: &str| {
            obj.get(k)
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default()
        };
        let unavailable = items("unavailable")
            .iter()
            .filter_map(|x| {
                let o = x.as_object()?;
                Some((
                    o.get("command")?.as_str()?.to_string(),
                    o.get("code")?.as_str()?.to_string(),
                ))
            })
            .collect();
        let uncancellable = items("uncancellable")
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect();
        (unavailable, uncancellable)
    }

    /// 「重新对齐」时那台交回的**当下**能力事实换掉握手那一刻的（例：握手后才装上 tmux）。
    /// 接哪些 op 不变（还是同一个二进制、同一条连接）。
    pub fn refresh_facts(
        &mut self,
        unavailable: Vec<(String, String)>,
        uncancellable: Vec<String>,
    ) {
        self.unavailable = unavailable;
        self.uncancellable = uncancellable;
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  六、帧头（线上形状）—— 只在本通道两半之间用
// ════════════════════════════════════════════════════════════════════════════

/// 一帧的头。体（若有）跟在后面，见模块头注。
///
/// ⚠ 编号（`id`）由**客户端**发，一条连接内单调；路由器只拿它配对，不解释。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum Head {
    // ── 客户端 → 路由器 ──
    Hello {
        key: String,
    },
    Call {
        id: u64,
        origin: Origin,
        op: String,
        left: Duration,
    },
    Cancel {
        id: u64,
    },
    Subscribe {
        id: u64,
        origin: Origin,
        kind: String,
        from: Option<Cursor>,
        want: u32,
    },
    Want {
        id: u64,
        more: u32,
    },
    Stop {
        id: u64,
    },
    /// 要那台机器的能力事实（`Offer`）；路由器回 `Done{id}`，体是 `Option<Offer>` 的 JSON。
    OfferOf {
        id: u64,
        origin: Origin,
    },
    // ── 路由器 → 客户端 ──
    Welcome,
    Denied,
    Done {
        id: u64,
    },
    Failed {
        id: u64,
        err: WireErr,
    },
    Next {
        id: u64,
        item: WireItem,
    },
}

/// `CallError` 的线上形状。`Refused` 的那份不透明体走帧体，不进 JSON。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireErr {
    Hop {
        idx: u8,
        tag: String,
        reach: Reach,
        why: HopFault,
    },
    Unsupported,
    Refused,
    Ours(OursFault),
    /// `Ours` 且那台对这一条不认撤（`CallError::Ours.runs_on`）。
    OursRunsOn(OursFault),
}

/// `Item` 的线上形状。`Frame` 与 `Closed{by: Peer}` 的体走帧体。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireItem {
    Frame {
        seq: u64,
    },
    Gap {
        from_seq: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_seq: Option<u64>,
    },
    Unseen {
        idx: u8,
        tag: String,
        why: HopFault,
    },
    Seen {
        from: Option<Cursor>,
    },
    ClosedByPeer,
    ClosedByOurs(OursFault),
}

/// `CallError` ⇒ 线上头 ＋ 帧体。**穷尽**（`X1`）。
pub fn err_to_wire(e: CallError) -> (WireErr, Vec<u8>) {
    match e {
        CallError::Hop { at, reach, why } => (
            WireErr::Hop {
                idx: at.idx,
                tag: at.tag.to_string(),
                reach,
                why,
            },
            Vec::new(),
        ),
        CallError::Peer {
            why: PeerFault::Unsupported,
        } => (WireErr::Unsupported, Vec::new()),
        CallError::Peer {
            why: PeerFault::Refused { body },
        } => (WireErr::Refused, body.0),
        CallError::Ours {
            why,
            runs_on: false,
        } => (WireErr::Ours(why), Vec::new()),
        CallError::Ours { why, runs_on: true } => (WireErr::OursRunsOn(why), Vec::new()),
    }
}

/// 线上头 ＋ 帧体 ⇒ `CallError`。认不出的跳号标签 ⇒ `Ours{Broken}`（协议坏了，不猜）。
pub fn err_from_wire(w: WireErr, body: Vec<u8>) -> CallError {
    match w {
        WireErr::Hop {
            idx,
            tag,
            reach,
            why,
        } => match tag_of(&tag) {
            Some(tag) => CallError::Hop {
                at: HopId { idx, tag },
                reach,
                why,
            },
            None => OursFault::Broken.into(),
        },
        WireErr::Unsupported => CallError::Peer {
            why: PeerFault::Unsupported,
        },
        WireErr::Refused => CallError::Peer {
            why: PeerFault::Refused { body: Body(body) },
        },
        WireErr::Ours(why) => why.into(),
        WireErr::OursRunsOn(why) => CallError::Ours { why, runs_on: true },
    }
}

/// `Item` ⇒ 线上头 ＋ 帧体。**穷尽**（`X1`）。
pub fn item_to_wire(i: Item) -> (WireItem, Vec<u8>) {
    match i {
        Item::Frame { seq, body } => (WireItem::Frame { seq }, body.0),
        Item::Gap { from_seq, to_seq } => (WireItem::Gap { from_seq, to_seq }, Vec::new()),
        Item::Unseen { at, why } => (
            WireItem::Unseen {
                idx: at.idx,
                tag: at.tag.to_string(),
                why,
            },
            Vec::new(),
        ),
        Item::Seen { from } => (WireItem::Seen { from }, Vec::new()),
        Item::Closed { by: By::Peer(b) } => (WireItem::ClosedByPeer, b.0),
        Item::Closed { by: By::Ours(why) } => (WireItem::ClosedByOurs(why), Vec::new()),
    }
}

/// 线上头 ＋ 帧体 ⇒ `Item`。认不出的跳号标签 ⇒ `None`（调用方按协议坏了处置）。
pub fn item_from_wire(w: WireItem, body: Vec<u8>) -> Option<Item> {
    Some(match w {
        WireItem::Frame { seq } => Item::Frame {
            seq,
            body: Body(body),
        },
        WireItem::Gap { from_seq, to_seq } => Item::Gap { from_seq, to_seq },
        WireItem::Unseen { idx, tag, why } => Item::Unseen {
            at: HopId {
                idx,
                tag: tag_of(&tag)?,
            },
            why,
        },
        WireItem::Seen { from } => Item::Seen { from },
        WireItem::ClosedByPeer => Item::Closed {
            by: By::Peer(Body(body)),
        },
        WireItem::ClosedByOurs(why) => Item::Closed { by: By::Ours(why) },
    })
}

// ════════════════════════════════════════════════════════════════════════════
//  七、拆帧 / 封帧
// ════════════════════════════════════════════════════════════════════════════

/// 读一帧失败的三种。
#[derive(Debug)]
pub enum ReadFault {
    /// 在帧边界上干净地读到了 EOF —— 对面走了。
    Eof,
    /// 读到一半断了 / 读出错。
    Io(std::io::Error),
    /// 长度超过宿主交进来的上限，或头解不出来 —— 对面没按协议说话。
    Bad(String),
}

/// 读一段「u32 长度 ＋ 那么多字节」。`cap` 由调用方给（宿主交进来的值）。
async fn read_segment<R: AsyncRead + Unpin>(
    r: &mut R,
    cap: usize,
    first: bool,
) -> Result<Vec<u8>, ReadFault> {
    let mut len = [0u8; 4];
    if let Err(e) = r.read_exact(&mut len).await {
        return Err(if first && e.kind() == std::io::ErrorKind::UnexpectedEof {
            ReadFault::Eof
        } else {
            ReadFault::Io(e)
        });
    }
    let n = u32::from_be_bytes(len) as usize;
    if n > cap {
        return Err(ReadFault::Bad(copy_text(
            "rsChanWire.segment.tooLong",
            &[("n", &n.to_string()), ("cap", &cap.to_string())],
        )));
    }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).await.map_err(ReadFault::Io)?;
    Ok(buf)
}

/// 读一整帧：头 ＋ 体。
pub async fn read_frame<R: AsyncRead + Unpin>(
    r: &mut R,
    cap: usize,
) -> Result<(Head, Vec<u8>), ReadFault> {
    let head = read_segment(r, cap, true).await?;
    let body = read_segment(r, cap, false).await?;
    let head: Head = serde_json::from_slice(&head).map_err(|e| {
        ReadFault::Bad(copy_text(
            "rsChanWire.frame.badHead",
            &[("e", &e.to_string())],
        ))
    })?;
    Ok((head, body))
}

/// 写一整帧并冲刷。
pub async fn write_frame<W: AsyncWrite + Unpin>(
    w: &mut W,
    head: &Head,
    body: &[u8],
) -> std::io::Result<()> {
    let h = serde_json::to_vec(head).map_err(std::io::Error::other)?;
    let too_long = |n: usize| {
        u32::try_from(n).map_err(|_| {
            std::io::Error::other(copy_text(
                "rsChanWire.frame.tooLong",
                &[("n", &n.to_string())],
            ))
        })
    };
    let mut out = Vec::with_capacity(8 + h.len() + body.len());
    out.extend_from_slice(&too_long(h.len())?.to_be_bytes());
    out.extend_from_slice(&h);
    out.extend_from_slice(&too_long(body.len())?.to_be_bytes());
    out.extend_from_slice(body);
    w.write_all(&out).await?;
    w.flush().await
}
