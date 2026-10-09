//! 计划读面的线上形状（`plan-list` · `plan-read` · `plan-command` 的回包）：界面的类型由这里经 ts-rs 生成，不手写。
//!
//! 成品在 [`super::product`] · [`super::book`] · [`super::review`] 里按 JSON 拼（pb 的形状原样往下传最省事）；
//! 帧面出口那一下（`faces/plan_face.rs`）过一遍这里的类型（[`checked`]）⇒ 线上那一份就是这些类型的序列化，界面拿到的与生成的类型对得上。
//! 判据 `tests/backend/plan/wire_tests.rs` 钉「夹具出的成品过一遍不丢不添」。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 一个 id（块的接手 · 签收的「由」· 在长它的 · 签它的）对到的会话。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanWho {
    pub id: String,
    #[cfg_attr(test, ts(type = "\"session\" | \"subagent\" | \"unknown\""))]
    pub kind: String,
    /// 主会话 ⇒ 它自己；子 agent ⇒ 父会话；认不出 ⇒ `null`。
    pub sid: Option<String>,
    pub alive: bool,
    #[cfg_attr(test, ts(type = "\"working\" | \"needs_you\" | \"idle\" | null"))]
    pub activity: Option<String>,
    /// 在等你时等的是哪一类（批准 · 回答 · 计划 · 说不清）。
    pub needs: Option<String>,
}

/// 一格管的一份文件（对账）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanFile {
    pub path: String,
    /// pb 的原话（在 · 缺 · 空 · 坏）。
    pub state: Option<String>,
    /// 对账的码；认不出 ⇒ `null`。
    #[cfg_attr(
        test,
        ts(type = "\"ok\" | \"missing\" | \"empty\" | \"broken\" | null")
    )]
    pub state_code: Option<String>,
    pub note: Option<String>,
    /// 同一份文件还挂在哪几格（编号）。
    pub also_by: Vec<String>,
}

/// 话里提到的格：`[start, end)` 按字数，`ids` 是那几格（简写已展开）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanRef {
    pub start: u32,
    pub end: u32,
    pub ids: Vec<String>,
}

/// 标题与正文里各提到哪几格。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanRefs {
    pub title: Vec<PlanRef>,
    pub body: Vec<PlanRef>,
}

/// 一次签收。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanSign {
    /// pb 的 ISO 时刻。
    pub at: Option<String>,
    /// 那一刻写成给人看的字（那台本地钟）；读不出时刻 ⇒ 没有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub at_text: Option<String>,
    pub by: Option<PlanWho>,
    pub reason: Option<String>,
    pub refs: Vec<PlanRef>,
}

/// 四种边（指向 · 连着 · 排在后面 · 顶掉），各一列编号。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanEdges {
    pub to: Vec<String>,
    #[serde(rename = "with")]
    #[cfg_attr(test, ts(rename = "with"))]
    pub with_: Vec<String>,
    pub after: Vec<String>,
    pub replaces: Vec<String>,
}

/// 没做完的三种原因（后端从 pb 的原话拆好）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) enum PlanWhyCode {
    Nosign,
    Inside { done: u32, of: u32 },
    Upper,
}

/// 退回落地时多出来的那一格。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanReturnedChild {
    pub id: String,
    pub title: Option<String>,
}

/// 一格退回过的状态（后端记、后端判落地）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanReturned {
    /// 退回那一刻（epoch ms）。
    #[cfg_attr(test, ts(type = "number"))]
    pub at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub at_text: Option<String>,
    pub to: Option<PlanWho>,
    #[cfg_attr(test, ts(type = "\"returned\" | \"unsure\" | \"landed\""))]
    pub state: String,
    /// 落地的那一种：底下多出新格 · 正文改了。
    #[cfg_attr(test, ts(type = "\"child\" | \"body\" | null"))]
    pub by: Option<String>,
    pub child: Option<PlanReturnedChild>,
}

/// 一格。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanCell {
    pub id: String,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub body: Option<String>,
    pub parent: Option<String>,
    pub children: Vec<String>,
    pub edges: PlanEdges,
    pub pointed_by: PlanEdges,
    pub files: Vec<PlanFile>,
    /// pb 的原话（做完了 · 没做完 · 不做了）。
    pub status: Option<String>,
    /// 状态的码；pb 给了别的字 ⇒ `null`。
    #[cfg_attr(test, ts(type = "\"done\" | \"open\" | \"dropped\" | null"))]
    pub status_code: Option<String>,
    /// 没做完时的原因（pb 原话）。
    pub why: Option<String>,
    /// 原因拆好的那一份；认不出 ⇒ `null`（界面照出 `why` 原话）。
    pub why_code: Option<PlanWhyCode>,
    pub signs: Vec<PlanSign>,
    pub owner: Option<PlanWho>,
    /// 签它的（上一级）；pb 没给 ⇒ `null`。
    pub signer: Option<PlanWho>,
    pub refs: PlanRefs,
    pub has_view: bool,
    /// 退回过 ⇒ 那一次的状态；没有 ⇒ `null`。
    #[serde(default)]
    pub returned: Option<PlanReturned>,
}

/// 要你看的一条（后端判的四种）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanNeed {
    pub key: String,
    #[cfg_attr(test, ts(type = "\"top\" | \"red\" | \"ended\" | \"ask\""))]
    pub kind: String,
    pub block: Option<String>,
    pub cell: Option<String>,
    /// 那一条牵着的会话（问人 · 停了的那一个）。
    pub sid: Option<String>,
    /// 判据红那一条在 `check.red` 里的位置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub red: Option<u32>,
    pub acked: bool,
}

/// 派出去的一块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanBlock {
    pub id: String,
    pub cells: Vec<String>,
    pub dir: Option<String>,
    pub owner: Option<PlanWho>,
    pub phase: Option<String>,
    /// 站在哪一格（编号）。
    pub at: Option<String>,
    pub row: bool,
}

/// 四种边在这一类里各叫什么（pb 的领域表）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanEdgeWords {
    pub to: Option<String>,
    #[serde(rename = "with")]
    #[cfg_attr(test, ts(rename = "with"))]
    pub with_: Option<String>,
    pub after: Option<String>,
    pub replaces: Option<String>,
}

/// 领域表里的一类。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanKind {
    pub name: Option<String>,
    pub edge_words: PlanEdgeWords,
}

/// 阶段表里的一步。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanPhase {
    pub name: Option<String>,
    pub does: Option<String>,
    pub marks: Vec<String>,
}

/// 判据红的一条。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanRed {
    pub rule: Option<String>,
    pub what: Option<String>,
    pub block: Option<String>,
    pub fix: Option<String>,
}

/// 判不了的一条。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanUndecidable {
    pub rule: Option<String>,
    pub why: Option<String>,
}

/// 机检那一段。`unreadable` 是 pb 原样（这一版界面不读）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanCheck {
    #[cfg_attr(test, ts(type = "unknown[]"))]
    pub unreadable: Vec<Value>,
    pub red: Vec<PlanRed>,
    pub undecidable: Vec<PlanUndecidable>,
}

/// 顶层进度。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanProgress {
    pub done: u32,
    pub open: u32,
    pub dropped: u32,
}

/// 读不成、给的是上一次那一份：那一句 ＋ 原话 ＋ 那一份读到的时刻。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanStale {
    pub said: Option<String>,
    pub raw: Option<String>,
    #[cfg_attr(test, ts(type = "number"))]
    pub since: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub since_text: Option<String>,
}

/// 收起来的格（被顶掉的）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanArchived {
    pub id: String,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub replaced_by: Option<String>,
}

/// 一片。读不成又没读好过 ⇒ `bare`（只有头几格有字，其余是空的）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanSlice {
    pub name: String,
    pub domain: Option<String>,
    pub current: bool,
    /// 这一刻读不成的那一句（pb 原样）。
    pub error: Option<String>,
    pub stale: Option<PlanStale>,
    pub bare: bool,
    pub kinds: Vec<PlanKind>,
    pub phases: Vec<PlanPhase>,
    pub done: bool,
    pub top: Vec<String>,
    pub progress: Option<PlanProgress>,
    pub blocks: Vec<PlanBlock>,
    pub check: PlanCheck,
    pub cells: Vec<PlanCell>,
    pub archived: Vec<PlanArchived>,
    pub needs: Vec<PlanNeed>,
    /// 这一片要你看的数（没认可的，不含问人）。
    pub need_count: u32,
}

/// 会话 ⇒ 它接手的那一块（会话头那一枚标）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanSessionBlock {
    pub slice: String,
    pub block: String,
    /// 块根格（编号；点会话头那一枚标选中它）。
    pub cell: Option<String>,
    /// 块根格的标题；顶块 ⇒ `null`。
    pub title: Option<String>,
    pub top: bool,
    pub phase: Option<String>,
    pub at: Option<String>,
    pub at_title: Option<String>,
    #[cfg_attr(test, ts(type = "\"session\" | \"subagent\""))]
    pub via: String,
}

/// `plan-read` 的回包：一个工作区的成品。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanRead {
    pub pb: Option<String>,
    pub workspace: String,
    pub repo: Option<String>,
    pub auto: bool,
    pub slices: Vec<PlanSlice>,
    pub by_session: std::collections::BTreeMap<String, PlanSessionBlock>,
    /// 这一份输出的摘要：变了才算计划变了。
    pub rev: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub read_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub read_at_text: Option<String>,
    pub stale: Option<PlanStale>,
    pub need_count: u32,
}

/// `plan-list` 里一片的一行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanListSlice {
    pub name: String,
    pub domain: Option<String>,
    pub current: bool,
    pub progress: Option<PlanProgress>,
    pub need_count: u32,
    pub error: Option<String>,
    pub stale: Option<PlanStale>,
}

/// `plan-list` 里的一个工作区。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanListWorkspace {
    pub workspace: String,
    pub repo: Option<String>,
    pub auto: bool,
    pub rev: String,
    pub stale: Option<PlanStale>,
    pub need_count: u32,
    pub by_session: std::collections::BTreeMap<String, PlanSessionBlock>,
    pub slices: Vec<PlanListSlice>,
}

/// pb 装没装、认不认得它的输出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanPb {
    #[cfg_attr(test, ts(type = "\"ok\" | \"missing\" | \"unsupported\""))]
    pub state: String,
    pub said: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub version: Option<String>,
}

/// `plan-list` 的回包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanList {
    pub pb: PlanPb,
    pub workspaces: Vec<PlanListWorkspace>,
}

/// `plan-command` 的回包：pb 的退出码 · 那一句 · `view` 写的页面路径。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanCmdReply {
    pub rc: i32,
    pub said: Option<String>,
    pub path: Option<String>,
}

/// `plan-ack` / `plan-unack` 的回包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanAckReply {
    pub acked: bool,
    pub key: String,
    /// 这个工作区此刻要人过目的数。
    pub need_count: u32,
}

/// `plan-return` 的回包：送出的那一行（后端拼）· 送给谁 · 结局。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanReturnReply {
    pub line: String,
    pub to: Option<PlanWho>,
    /// `delivered` · `unsure`（送达未知，别重发）· `refused` · `copy`（送不了，只给这一行去复制）。
    #[cfg_attr(test, ts(type = "\"delivered\" | \"unsure\" | \"refused\" | \"copy\""))]
    pub result: String,
    pub why: Option<String>,
    pub said: Option<String>,
    pub screen: Option<String>,
}

/// 帧面出口：一份按 JSON 拼的回包过一遍 `T`（线上那一份就是 `T` 的序列化）。对不上 ⇒ 拼的那一侧有错（程序员错误），回 `failed`。
pub(crate) fn checked<T: Serialize + serde::de::DeserializeOwned>(
    v: Value,
) -> Result<Value, String> {
    let t: T = serde_json::from_value(v).map_err(|e| e.to_string())?;
    serde_json::to_value(t).map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/wire_tests.rs"]
mod tests;
