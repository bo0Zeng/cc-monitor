//! `tasks-list` 的回包线上形（一个会话的任务列表；也是 `changed {tasks}` 带的那一份体）。字段语义住后端 `observe/tasks_query.rs::task_entry`。

use serde::{Deserialize, Serialize};

/// `tasks-list` 的回包（成品）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct TasksList {
    /// `<tasks>/<sid>/<数字>.json` 里每个任务一格，按那个数字升序。
    pub tasks: Vec<Task>,
}

/// `tasks-list` 回包 `tasks[]` 里的一格（成品：字段语义只住这里，[`task_entry`] 从原文对象收）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Task {
    /// 每格必有、是串（缺 / 不是串的那个对象不算任务，跳过）。
    pub id: String,
    /// 同上。
    pub subject: String,
    /// 同上。
    pub status: String,
    /// 串的数组；原文缺 ⇒ `[]`（原文是 `null` / 别的类型 ⇒ 那个对象不算任务）。
    pub blocks: Vec<String>,
    /// 同上。
    pub blocked_by: Vec<String>,
    /// 可缺（原文缺或 `null` ⇒ 这一格不出现），出现就是串。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub description: Option<String>,
    /// 同上。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub active_form: Option<String>,
}
