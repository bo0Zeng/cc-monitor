//! `tasks-list` 的回包线上形（一个会话的任务列表；也是 `changed {tasks}` 带的那一份体）。那一家任务文件的字段怎么读住适配层
//! （Claude：`agents/claudecode/tasks.rs`），成品句住后端 `observe/tasks_query.rs`。

use serde::{Deserialize, Serialize};

/// `tasks-list` 的回包（成品）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct TasksList {
    /// `<tasks>/<sid>/<数字>.json` 里每个任务一格，按那个数字升序。
    pub tasks: Vec<Task>,
    /// 状态栏那一枚的字（「任务 完成数/总数」）；没有任务 ⇒ 空串。
    pub chip: String,
    /// 那一枚的悬停分项（完成 · 进行 · 未开始）；没有任务 ⇒ 空串。
    pub hint: String,
}

/// 一个任务在哪一态（闭集；那一家写的值认不出 ⇒ `Other`，原串不过边界）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Deleted,
    Other,
}

/// `tasks-list` 回包 `tasks[]` 里的一格（成品：适配层从那一家的任务文件收）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Task {
    /// 每格必有、是串（缺 / 不是串的那个对象不算任务，跳过）。
    pub id: String,
    /// 同上。
    pub subject: String,
    /// 哪一态（原文缺 / 不是串 ⇒ 那个对象不算任务）。
    pub status: TaskStatus,
    /// `status` 写给人看的那一个字（读屏念它；界面照摆，不自己映）。
    pub status_text: String,
    /// 串的数组；原文缺 ⇒ `[]`（原文是 `null` / 别的类型 ⇒ 那个对象不算任务）。
    pub blocks: Vec<String>,
    /// 可缺（原文缺或 `null` ⇒ 这一格不出现），出现就是串。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub description: Option<String>,
    /// 正在做的那一句：只在进行中给（别的态原文带着也不出）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub active_form: Option<String>,
}
