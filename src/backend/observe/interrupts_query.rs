//! **会打断什么** —— `session-interrupts` 那条只读帧命令的本体：重启切换 / 结束一个会话之前，界面先问这一句，
//! 后端按族答出此刻会被打断的东西（界面只画；什么都没有 ⇒ 界面直接做）。
//!
//! 三族，都是这台此刻的事实：
//! - `turn`：那个会话有一轮在跑（活着的 pidfile 里 `status` 是 `busy`）；
//! - `agent`：它派出去、还在跑的子运行（本进程各条流连接的运行簿合起来）；
//! - `task`：它的任务表里 `in_progress` 的那几条（任务主题作名字）。
//!
//! 空族不出现；一族都没有 ⇒ `families: []`。读不到（目录没有 / 读失败）按「没有」答，不报错 —— 界面那一侧
//! 等不到答复时本来就按「有东西在跑」处理，这里不再替它猜。

use serde::Serialize;
use serde_json::Value;
use std::path::Path;

/// 一族会被打断的东西。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct InterruptFamily {
    pub family: InterruptKind,
    /// 显示名（`turn` 那一族没有名字 ⇒ 空表）。
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub enum InterruptKind {
    Turn,
    Agent,
    Task,
}

/// `session-interrupts` 的应答。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SessionInterrupts {
    pub families: Vec<InterruptFamily>,
}

/// 三族事实的来处（判据拿夹具喂它）。
pub(crate) struct Sources<'a> {
    pub(crate) busy: &'a dyn Fn(&str) -> bool,
    pub(crate) agents: &'a dyn Fn(&str) -> Vec<String>,
    pub(crate) tasks: &'a dyn Fn(&str) -> Vec<String>,
}

/// 纯组装：三族事实 ⇒ 应答（空族不出）。
pub(crate) fn interrupts_of(sid: &str, src: &Sources<'_>) -> SessionInterrupts {
    let mut families = Vec::new();
    if (src.busy)(sid) {
        families.push(InterruptFamily {
            family: InterruptKind::Turn,
            names: Vec::new(),
        });
    }
    for (family, names) in [
        (InterruptKind::Agent, (src.agents)(sid)),
        (InterruptKind::Task, (src.tasks)(sid)),
    ] {
        if !names.is_empty() {
            families.push(InterruptFamily { family, names });
        }
    }
    SessionInterrupts { families }
}

/// 任务表里在做的那几条的主题（读不到 ⇒ 空）。
fn tasks_in_progress(home: &Path, sid: &str) -> Vec<String> {
    crate::observe::tasks_query::session_tasks(home, sid)
        .unwrap_or_default()
        .iter()
        .filter(|t| t.get("status").and_then(Value::as_str) == Some("in_progress"))
        .filter_map(|t| t.get("subject").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// 帧面入口（宿主 `faces/feature_face.rs` 给家目录）。
pub(crate) fn answer_at(home: &Path, args: &Value) -> Result<Value, (String, String)> {
    let sid = args
        .get("sid")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            (
                "bad_args".to_string(),
                crate::common::contract::malformed("missing `sid` (a non-empty string)"),
            )
        })?;
    let busy = |s: &str| crate::observe::watcher::session_busy(home, s);
    let agents = |s: &str| crate::observe::runs::running_names(s);
    let tasks = |s: &str| tasks_in_progress(home, s);
    let out = interrupts_of(
        sid,
        &Sources {
            busy: &busy,
            agents: &agents,
            tasks: &tasks,
        },
    );
    serde_json::to_value(out).map_err(|e| {
        (
            "failed".to_string(),
            crate::common::contract::malformed(&e.to_string()),
        )
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/interrupts_query_tests.rs"]
mod tests;
