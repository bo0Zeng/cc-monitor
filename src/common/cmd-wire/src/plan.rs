//! 计划反查 `plan-files` 的回包：这个目录落在哪一片的仓库里、每份文件归哪一格、哪几份无主。
//! 两侧同一份：后端按这里的类型过一遍再发（拼在后端 `plan::owners`），文件窗口按这里的类型解（不手解 JSON）。

use serde::{Deserialize, Serialize};

/// 那一格的状态码（后端 `plan::product` 从 pb 的状态字 · 凭据类认出来的）。TS 那一侧在字段上写成字面量并集（不另出一份生成文件）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanStatusCode {
    Done,
    Open,
    Dropped,
    /// 凭据类的格（不论 pb 的状态字）。
    Evidence,
}

/// 一份文件的对账码（pb 的 在 · 缺 · 空 · 坏）。TS 那一侧同上，写在字段上。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanFileState {
    Ok,
    Missing,
    Empty,
    Broken,
}

/// `plan-files` 里一份文件（文件窗口「格」一列与预览头那一块）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanFileOwner {
    /// 文件名（目录里的那一项）。
    pub name: String,
    /// 作主的那一格（排期上先声明它的那一格）。
    pub id: String,
    /// 「编号 标题」（没有标题 ⇒ 只写编号），后端拼好。
    pub label: String,
    pub title: Option<String>,
    /// 认不出 pb 的状态字 ⇒ `null`（当没做完画）。
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"done\" | \"open\" | \"dropped\" | \"evidence\" | null")
    )]
    pub status_code: Option<PlanStatusCode>,
    pub status: Option<String>,
    /// 那一格住的那一块（块根格的标题）；住顶块 ⇒ `null`。
    pub block: Option<String>,
    /// 作数那一条签收的时刻（成了字）；没签过 ⇒ `null`。
    pub sign_at_text: Option<String>,
    /// 对账码；认不出 ⇒ `null`。
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"ok\" | \"missing\" | \"empty\" | \"broken\" | null")
    )]
    pub file_state: Option<PlanFileState>,
    pub file_note: Option<String>,
    /// 也声明它的那几格（pb 判据红「两个节点声明同一个文件」）。
    pub dup: Vec<PlanFileDup>,
}

/// 也声明同一份文件的另一格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanFileDup {
    pub id: String,
    /// 「编号 标题」，后端拼好。
    pub label: String,
    pub title: Option<String>,
}

/// `plan-files` 的回包：这个目录落在哪一片的仓库里（不在 ⇒ `slice: null`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanFiles {
    pub workspace: Option<String>,
    pub slice: Option<String>,
    /// 那一片此刻读不成的那一句（条目空）。
    pub unreadable: Option<String>,
    pub entries: Vec<PlanFileOwner>,
    /// 这个目录里无主的那几份（文件名）；不在任何一片里 · 那一片读不成 · 仓库读不出 ⇒ `null`。
    pub unowned: Option<Vec<String>>,
    /// 仓库读不出、判不了无主的那一句（pb 原样）；判得了 ⇒ `null`。
    #[serde(rename = "unownedWhy")]
    #[cfg_attr(any(test, feature = "ts"), ts(rename = "unownedWhy"))]
    pub unowned_why: Option<String>,
}
