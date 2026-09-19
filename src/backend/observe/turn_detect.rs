//! turn-end 判词（backend 侧 · phase② TurnEnd 帧的检测核）。
//!
//! **契约 = aterm `TurnDetector.kt:29` 逐字对拍**（golden-parity，同 `usage_query` 套路）：
//! turn-end ⟺ `type=="assistant" && message.stop_reason=="end_turn" && !isApiError && !isSidechain`。
//! 字段坑（master plan §0）：`isApiErrorMessage`→isApiError、`stop_reason` 嵌在 **message** 下、
//! `isSidechain` 是 **top-level**。在 `serde_json::Value` 上抽取（backend 无 `parse_line`/typed model）。
//!
//! **已接线**（backend-09）：`process_jsonl` 每见一条 turn-end 记录发 `Frame::TurnEnd{sid,uuid}`
//! （raw-per-record、backend 不 dedup）；dedup 视界在 aterm 侧 rolling-latest + debounce(1200ms)
//! `baselineByPath`（首见吞历史不通知、offset 续拉重放 uuid≤基线不通知；transport-agnostic、同 β）。
//!
//! §2.1 不变量并存：backend 仍逐行 raw 转发**每一条** Line（不因分类丢行）；turn-end 是在 raw 之外
//! **额外**从解析内容算的边沿信号，不替代、不过滤 Line。
//!
//! ★ **改 `is_turn_end` 的合取项前先看这里**〔audit-0805 08-07〕：同一个判词在前端
//! `src/turn-notify.ts` 还有第二份（通知那条路）。两份天生做不到 E3 的「权威源恰好一个」
//! ⇒ 退而求其次是**跨语言对拍**，住在 `tests/turn-notify.vitest.ts`：它**从本函数的合取项派生**
//! 人群，新加一条而不在那张登记表里说明 TS 侧怎么办 ⇒ 当场红。
//! ⚠ 这不是多余的礼节：报告 §4.1 记着这两份**已经漂过一次**（backend 有 `!isApiError`、TS 没有），
//! 而 08-07 实测「backend 加第五个合取项」时，改前的对拍表两侧共 17 条判据**一条都不会红**。

use serde_json::Value;

/// 一条已解析的 jsonl 记录是否为 turn-end 边沿。**逐字对拍 aterm `TurnDetector`**：
/// `assistant` && `message.stop_reason=="end_turn"` && !`isApiErrorMessage` && !`isSidechain`。
/// 缺字段一律安全默认（stop_reason 缺→非 end_turn→false；error/sidechain 缺→false→不排除）。
pub fn is_turn_end(v: &Value) -> bool {
    is_assistant(v) && stop_reason(v) == Some("end_turn") && !is_api_error(v) && !is_sidechain(v)
}

/// turn-end 边沿的 uuid（= 完成 assistant 记录 uuid），供 TurnEnd 帧 + 客户端幂等去重。
/// 非 turn-end / 无 uuid → None。
pub fn turn_end_uuid(v: &Value) -> Option<&str> {
    if is_turn_end(v) {
        v.get("uuid").and_then(Value::as_str)
    } else {
        None
    }
}

fn is_assistant(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("assistant")
}

/// `message.stop_reason`（嵌在 message 下——字段坑）。
fn stop_reason(v: &Value) -> Option<&str> {
    v.get("message")
        .and_then(|m| m.get("stop_reason"))
        .and_then(Value::as_str)
}

/// `isApiErrorMessage`（→ isApiError；缺/非 bool → false）。
fn is_api_error(v: &Value) -> bool {
    v.get("isApiErrorMessage")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// top-level `isSidechain`（缺/非 bool → false）。
fn is_sidechain(v: &Value) -> bool {
    v.get("isSidechain")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/turn_detect_tests.rs"]
mod tests;
