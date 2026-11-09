//! Claude 的任务文件（`<家>/tasks/<sid>/<数字>.json`，注册表 `LocalFace.tasks` 那一格）：一个已解析的任务对象怎么读成一格任务。
//!
//! 字段名 · 状态那几个值只住这里；通用层（`observe/tasks_query.rs`）只走目录、照码写状态字与计数。
//! 状态是闭集：认得的四种各一码，别的值 ⇒ `Other`（原串不过边界）。`activeForm` 只在进行中给。

use cmd_wire::tasks::{Task, TaskStatus};
use serde_json::Value;

/// 注册表 `LocalFace.tasks` 那一格。
pub(crate) const FACE: crate::agents::TasksFace = crate::agents::TasksFace {
    dir: super::paths::tasks_root,
    task: task_of,
};

/// 一个任务对象 ⇒ 一格任务（`status_text` 留空，核心照 `status` 写）；缺必填格 · 类型不对 ⇒ `None`。
pub(crate) fn task_of(v: &Value) -> Option<Task> {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let (id, subject) = (s("id")?, s("subject")?);
    let status = match v.get("status").and_then(Value::as_str)? {
        "pending" => TaskStatus::Pending,
        "in_progress" => TaskStatus::InProgress,
        "completed" => TaskStatus::Completed,
        "deleted" => TaskStatus::Deleted,
        _ => TaskStatus::Other,
    };
    let opt = |k: &str| -> Option<Option<String>> {
        match v.get(k) {
            None | Some(Value::Null) => Some(None),
            Some(Value::String(x)) => Some(Some(x.clone())),
            Some(_) => None,
        }
    };
    let list = |k: &str| -> Option<Vec<String>> {
        match v.get(k) {
            None => Some(Vec::new()),
            Some(Value::Array(a)) => a.iter().map(|x| x.as_str().map(str::to_string)).collect(),
            Some(_) => None,
        }
    };
    let active_form = opt("activeForm")?;
    Some(Task {
        id,
        subject,
        status,
        status_text: String::new(),
        description: opt("description")?,
        active_form: active_form.filter(|_| status == TaskStatus::InProgress),
        blocks: list("blocks")?,
    })
}
