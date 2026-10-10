//! turn-end 判词（backend 侧 · TurnEnd 帧的检测核）。
//!
//! 契约 = 手机端 `src/mobile/core-claude/.../model/TurnDetector.kt` 逐字对拍：turn-end ⟺ `type=="assistant" && message.stop_reason=="end_turn" && !isApiError`，且这条记录属于主运行。
//! 字段坑：`isApiErrorMessage` → isApiError、`stop_reason` 嵌在 message 下。
//! 「属于主运行」不在本判词里：子运行的记录归属由本家 `runs::run_of` 答、由通用 watcher 在发 `TurnEnd` 之前排除（子运行的轮次收尾 ≠ 主运行一轮结束）。
//!
//! `process_jsonl` 每见一条 turn-end 记录发 `Frame::TurnEnd{sid,uuid}`（逐记录、backend 不去重）；去重在手机端
//! （`TurnEndDebouncer.kt` 的 rolling-latest + debounce(1200ms) ＋ `SshKeepAliveService.kt` 的 `baselineByPath`：首见吞历史不通知、offset 续拉重放 uuid≤基线不通知）。
//! backend 仍逐行转发每一条 Line（不因分类丢行）；turn-end 是在 raw 之外额外算的边沿信号，不替代、不过滤 Line。
//!
//! 改 `is_turn_end` 的合取项前先看这里：同一个判词在前端 `src/frontend/ui/turn-notify.ts` 还有第二份（通知那条路）。两份做不到「权威源恰好一个」
//! ⇒ 跨语言对拍住在 `tests/frontend/ui/turn-notify.vitest.ts`：它从本函数的合取项派生人群，新加一条而不在那张登记表里说明 TS 侧怎么办 ⇒ 当场红。

//!
//! # 每行先过子串闸，再读窄探针
//!
//! watcher 每交出一行就问一次这里（`RecordFace.turn_end`），绝大多数行根本不是一轮的结束：
//! ① 子串闸：原文里没有 `"end_turn"` 这个字面量 ⇒ 不可能是 turn-end，零解析直接回（CLI 写 jsonl 不转义 ASCII）；
//! ② 过了闸的才解析进 [`Probe`]（只收判词要的五格，其余一律 `IgnoredAny` 跳过、不建树）。

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
    /// 非 bool ⇒ 当缺（安全默认：不排除）。
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

/// 一条记录是否为 turn-end 边沿。**逐字对拍手机端 `TurnDetector.kt`**：
/// `assistant` && `message.stop_reason=="end_turn"` && !`isApiErrorMessage`（子运行的那几条由通用 watcher 先排除，见头注）。
/// 缺字段一律安全默认（stop_reason 缺→非 end_turn→false；error 缺→false→不排除）。
pub fn is_turn_end(p: &Probe) -> bool {
    is_assistant(p) && ends_turn(stop_reason(p), is_api_error(p))
}

/// 一条 assistant 记录是不是一轮的结束（帧 `turn_end` 与通用记录的 `endsTurn` 共用这一个判定）。
pub(crate) fn ends_turn(stop_reason: Option<&str>, api_error: bool) -> bool {
    stop_reason == Some("end_turn") && !api_error
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
