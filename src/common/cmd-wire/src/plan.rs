//! 计划反查 `plan-files` 的回包：这个目录落在哪一片的仓库里、每份文件归哪一格、哪几份无主。
//! 两侧同一份：后端直接造这里的类型（在 `plan::owners`），文件窗口按这里的类型解（不手解 JSON）。

use serde::{Deserialize, Serialize};

/// 一个节点算不算活、签收了没有（后端 `plan::product` 照领域表算：收的值 ＝ 表里 update 规则加 · 去的那个值，
/// 节点的类许记它 ⇒ 算活）：记着它 ⇒ `Done`，没记 ⇒ `Open`；类不许记它（测试 · 规矩 · 原话 …）⇒ 字段为 `null`。
/// TS 那一侧在字段上写成字面量并集（不另出一份生成文件）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanStatusCode {
    Done,
    Open,
}

/// 一份文件的对账码（pb 的 `file_check`：present · missing · empty · invalid）。TS 那一侧同上，写在字段上。
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
    /// 类不许记收的值 ⇒ `null`（不算活）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "\"done\" | \"open\" | null"))]
    pub status_code: Option<PlanStatusCode>,
    /// 状态那一行字（后端照 pb 记着的值写好，如「已签收 2」）；什么都没记 ⇒ `null`。
    pub status: Option<String>,
    /// 那一格住的那一块（块根格的标题）；住顶块 ⇒ `null`。
    pub block: Option<String>,
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

/// `plan-files` 的入参：文件窗口在看的那个目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanFilesReq {
    /// 那台机器上的绝对路径。
    pub dir: String,
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
