//! 活卡（`live_card` 帧）的线上形 —— 两侧同一份：后端按这里的类型发（`stream::live` 出的成品，`wire::Frame::LiveCard`），
//! 手机嵌的 Rust 核心与界面按这里的类型认（界面用从这里导出的类型，`src/frontend/ui/generated/`）。
//! 归一流里一块是什么（[`BlockKind`]）也住这里：卡里每一块的种类就是它。

use serde::{Deserialize, Serialize};

/// 归一流里一块是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "lowercase")]
pub enum BlockKind {
    Text,
    Thinking,
    Tool,
    Other,
}

/// [`Frame::Live`] 的体：哪个会话哪个运行的哪张卡、这条连接上它的第几帧、这一帧做什么。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct LiveCard {
    /// 会话 id。
    pub sid: String,
    /// 归哪个子运行；缺 ＝ 主运行（主 tab 只画主运行的）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub run: Option<String>,
    /// 卡号（本进程的段号，整台唯一）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub card: u64,
    /// 这条连接上这张卡的第几帧：`open` 从 0 起，之后连续。不接上一帧 ⇒ 这张卡先不画，等下一张 `open` / `close`。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub n: u64,
    /// 这一帧做什么。
    #[serde(flatten)]
    pub op: LiveOp,
}

/// 活卡的一帧做什么（`op` 是标签）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum LiveOp {
    /// 整张卡（新开 · 这条连接刚开始看这个会话 · 发件箱满过 · 正文截过一次头）：照它整张换。
    Open {
        /// 对账键（与那一轮记录的 `rid` 同值）。
        rid: String,
        phase: LivePhase,
        /// 正文截过头（只留了尾巴）。
        clipped: bool,
        /// 各块，按块号。
        blocks: Vec<LiveBlock>,
    },
    /// 第 `i` 块开始了（种类叫 `block`：`kind` 已是帧的标签）。
    Block {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        i: u64,
        block: BlockKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        tool: Option<String>,
    },
    /// 第 `i` 块（文字块）接着一段字。
    Text {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        i: u64,
        s: String,
    },
    /// 上游说完了，等写入记录。
    Awaiting,
    /// 撤卡（为什么见 [`LiveWhy`]；客户端对哪种都一样：撤）。
    Close { why: LiveWhy },
}

/// 活卡此刻在哪一相。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum LivePhase {
    /// 上游还在说。
    Streaming,
    /// 上游说完了，那一轮的记录还没到。
    Awaiting,
}

/// 活卡里的一块。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct LiveBlock {
    pub kind: BlockKind,
    /// 文字块已到的正文（只有文字块有；思考不展开、工具入参不显示）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub text: Option<String>,
    /// 工具块的工具名。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub tool: Option<String>,
}

/// 为什么撤卡。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum LiveWhy {
    /// 那一轮的记录到了（整轮覆盖）。
    Record,
    /// 上游断在半路（Esc 打断 · 上游断 · 抄流那一跳缺了号）。
    Broken,
    /// 上游报错收尾。
    Error,
    /// 满了被挤掉（同一运行 2 张 · 整台 16 张）。
    Evicted,
    /// 会话走了 / 子运行收场了。
    Gone,
    /// 出了这条连接的名单。
    Unwatched,
}
