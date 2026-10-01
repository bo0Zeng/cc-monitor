//! turn-end 判词（backend 侧 · phase② TurnEnd 帧的检测核）。
//!
//! **契约 = aterm `TurnDetector.kt:29` 逐字对拍**（golden-parity，同 `usage_query` 套路）：
//! turn-end ⟺ `type=="assistant" && message.stop_reason=="end_turn" && !isApiError`，且这条记录属于主运行。
//! 字段坑（master plan §0）：`isApiErrorMessage`→isApiError、`stop_reason` 嵌在 **message** 下。
//! 「属于主运行」不在本判词里：子运行的记录归属由本家 `runs::run_of` 答、由通用 watcher 在发 `TurnEnd` 之前排除
//! （子运行的轮次收尾 ≠ 主运行一轮结束），线上 `TurnEnd` 帧的人群与从前同一个。
//!
//! **已接线**（backend-09）：`process_jsonl` 每见一条 turn-end 记录发 `Frame::TurnEnd{sid,uuid}`
//! （raw-per-record、backend 不 dedup）；dedup 视界在 aterm 侧 rolling-latest + debounce(1200ms)
//! `baselineByPath`（首见吞历史不通知、offset 续拉重放 uuid≤基线不通知；transport-agnostic、同 β）。
//!
//! §2.1 不变量并存：backend 仍逐行 raw 转发**每一条** Line（不因分类丢行）；turn-end 是在 raw 之外
//! **额外**从解析内容算的边沿信号，不替代、不过滤 Line。
//!
//! ★ **改 `is_turn_end` 的合取项前先看这里**：同一个判词在前端
//! `src/frontend/ui/turn-notify.ts` 还有第二份（通知那条路）。两份天生做不到 E3 的「权威源恰好一个」
//! ⇒ 退而求其次是**跨语言对拍**，住在 `tests/frontend/ui/turn-notify.vitest.ts`：它**从本函数的合取项派生**
//! 人群，新加一条而不在那张登记表里说明 TS 侧怎么办 ⇒ 当场红。
//! ⚠ 这不是多余的礼节：报告 §4.1 记着这两份**已经漂过一次**（backend 有 `!isApiError`、TS 没有），
//! 而 08-07 实测「backend 加第五个合取项」时，改前的对拍表两侧共 17 条判据**一条都不会红**。

//! # 每行先过子串闸，再读窄探针
//!
//! watcher 每交出一行就问一次这里（`RecordFace.turn_end`）。原先每行整份解析成 `serde_json::Value` 再取四格 ——
//! 而绝大多数行根本不是一轮的结束。今天两步：① **子串闸**：原文里没有 `"end_turn"` 这个字面量 ⇒ 不可能是 turn-end，
//! 零解析直接回（CLI 写 jsonl 不转义 ASCII，`stop_reason` 的值只可能以这个字面量出现）；② 过了闸的才解析进
//! [`Probe`]（只收判词要的五格，其余一律 `IgnoredAny` 跳过、不建树）。判词与缺字段的安全默认逐字不变。

use serde::de::IgnoredAny;
use serde::Deserialize;

/// 判词要的那几格。其余字段跳过（serde 对未知字段默认忽略，不建 `Value`）。
#[derive(Debug, Default, Deserialize)]
pub struct Probe {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    message: Option<ProbeMessage>,
    /// 非 bool ⇒ 当缺（安全默认：不排除），与原先 `as_bool().unwrap_or(false)` 同口径。
    #[serde(rename = "isApiErrorMessage", default)]
    is_api_error: Option<LooseBool>,
}

/// `message` 里只要 `stop_reason`（嵌在 message 下 —— 字段坑）。
#[derive(Debug, Default, Deserialize)]
struct ProbeMessage {
    #[serde(default)]
    stop_reason: Option<LooseStr>,
}

/// 是 bool 就取它，别的形状一律当缺（不让一格形状不对把整行判成畸形）。
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum LooseBool {
    Bool(bool),
    Other(IgnoredAny),
}

/// 是字符串就取它，别的形状一律当缺。
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum LooseStr {
    Str(String),
    Other(IgnoredAny),
}

/// 子串闸：`stop_reason` 的值 `end_turn` 以这个字面量出现在原文里是 turn-end 的必要条件。
const END_TURN_NEEDLE: &str = "\"end_turn\"";

/// 一条记录是否为 turn-end 边沿。**逐字对拍 aterm `TurnDetector`**：
/// `assistant` && `message.stop_reason=="end_turn"` && !`isApiErrorMessage`（子运行的那几条由通用 watcher 先排除，见头注）。
/// 缺字段一律安全默认（stop_reason 缺→非 end_turn→false；error 缺→false→不排除）。
pub fn is_turn_end(p: &Probe) -> bool {
    is_assistant(p) && stop_reason(p) == Some("end_turn") && !is_api_error(p)
}

/// turn-end 边沿的 uuid（= 完成 assistant 记录 uuid），供 TurnEnd 帧 + 客户端幂等去重。
/// 非 turn-end / 无 uuid → None。
pub fn turn_end_uuid(p: &Probe) -> Option<&str> {
    if is_turn_end(p) {
        p.uuid.as_deref()
    } else {
        None
    }
}

/// 一行原文 ⇒ 它若是 turn-end 边沿，那条记录的 uuid（注册表 `RecordFace.turn_end` 那一格；watcher 每行问一次）。
/// 过不了子串闸 ⇒ 不解析；畸形 ⇒ `None`（不影响那一行的 `line` 帧）。
pub(crate) fn turn_end_uuid_of(raw: &str) -> Option<String> {
    if !raw.contains(END_TURN_NEEDLE) {
        return None;
    }
    let p: Probe = serde_json::from_str(raw).ok()?;
    turn_end_uuid(&p).map(str::to_string)
}

fn is_assistant(p: &Probe) -> bool {
    p.kind.as_deref() == Some("assistant")
}

/// `message.stop_reason`（嵌在 message 下——字段坑）。
fn stop_reason(p: &Probe) -> Option<&str> {
    match p.message.as_ref()?.stop_reason.as_ref()? {
        LooseStr::Str(s) => Some(s),
        LooseStr::Other(_) => None,
    }
}

/// `isApiErrorMessage`（→ isApiError；缺/非 bool → false）。
fn is_api_error(p: &Probe) -> bool {
    matches!(p.is_api_error, Some(LooseBool::Bool(true)))
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/turn_tests.rs"]
mod tests;
