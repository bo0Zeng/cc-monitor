//! **机器表的最近一份**：机器表住本机后端（`~/.cc-monitor/machines.json`，后端 `control/machines.rs`），本壳不读盘，
//! 只记本机后端 `machines-read` 最近答的那一份（各台的常驻流由本机后端自己照它起停，`control/feeds.rs`）。
//!
//! - 什么时候问：本机入方向通道登记上那一刻问一次（[`refresh`]，两条载体各调一次），之后本机流每来一帧 `changed {machines}`
//!   （后端每次写完响一下）再问一次。问完当场记账（会话成品账按新表算「各台都报完」）、告诉主窗口。
//! - 本机后端连上之前表是空的。
//! - 读不懂 ⇒ 当空表（一台都不连），上屏一句（`machines-unreadable`）；问不到 ⇒ 留着上一份。

use serde_json::Value;
use std::sync::Mutex;

/// 最近一答里的一台：本壳只拿两样 —— 名字（后端算好的）· 连不连。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub(crate) name: String,
    pub(crate) connect: bool,
}

impl Row {
    /// 后端那一行（`{name, host}`）⇒ 一台；没有名字 ⇒ 不认。
    fn of(v: &Value) -> Option<Row> {
        let name = v
            .get("name")
            .and_then(Value::as_str)
            .filter(|n| !n.is_empty())?;
        let host = v.get("host").cloned().unwrap_or(Value::Null);
        Some(Row {
            name: name.to_string(),
            connect: host.get("connect").and_then(Value::as_bool) != Some(false),
        })
    }
}

/// 最近一答里的那几台。
static LATEST: Mutex<Vec<Row>> = Mutex::new(Vec::new());

/// 最近一答里的那几台（本机后端还没答过 ⇒ 空）。
pub(crate) fn rows_now() -> Vec<Row> {
    LATEST.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// 机器表里有没有叫这个名字的那一台（连不连都算：卸载 · 测试连接照样要它）。
pub(crate) fn has(name: &str) -> bool {
    rows_now().iter().any(|r| r.name == name)
}

/// 那一答 ⇒ 照用的那几台 ＋ 读不懂时那一句（`(路径, 原因)`）。**纯函数**。
pub(crate) fn hosts_of(reply: &Value) -> (Vec<Row>, Option<(String, String)>) {
    let hosts = reply
        .get("hosts")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Row::of).collect())
        .unwrap_or_default();
    let text = |k: &str| {
        reply
            .get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    match reply.get("state").and_then(Value::as_str) {
        Some("unreadable") => (Vec::new(), Some((text("path"), text("reason")))),
        _ => (hosts, None),
    }
}

/// 机器表里要连的那几台（机器页那几行：本机不在里面）。
pub(crate) fn connected_names() -> Vec<String> {
    let mut names: Vec<String> = rows_now()
        .into_iter()
        .filter(|r| r.connect)
        .map(|r| r.name)
        .collect();
    names.sort();
    names.dedup();
    names
}

/// 后台问本机后端一次、记下、记账并告诉主窗口（不挡调用方）。
pub(crate) fn refresh() {
    tauri::async_runtime::spawn(async {
        let deadline = crate::frame_query::Deadline::within(std::time::Duration::from_secs(10));
        let reply = crate::frame_query::call(
            &crate::origin::Origin::local(),
            "machines-read",
            serde_json::json!({}),
            None,
            deadline,
        )
        .await;
        let reply = match reply {
            Ok(r) => r,
            Err(e) => {
                tracing::info!("没问到本机的机器表，照上一份：{e}");
                return;
            }
        };
        let (hosts, unreadable) = hosts_of(&reply);
        if let Some((path, why)) = unreadable {
            tracing::error!("{path} 读不懂，远端一台都不连：{why}");
            crate::ui_error::tell(crate::ui_error::UiError::MachinesUnreadable { path, why });
        }
        *LATEST.lock().unwrap_or_else(|p| p.into_inner()) = hosts;
        // 机器表换了 ⇒ 会话成品账按新表重算「各台都报完」（同 `backend_machines` 那一份：本机恒第一，远端按名字排）。
        let mut table = vec![crate::inbound_client::LOCAL_ORIGIN.to_string()];
        table.extend(connected_names());
        crate::session_book::machines_changed(table);
        if let Some(replay) = crate::main_replay() {
            tell_followed(&replay);
        }
    });
}

/// 壳记下新的机器表之后在本机 `changed/machines` 上说一声：主窗口据它重问 `backend_machines`、补订新来的 · 退订没了的。
/// 本机后端自己那一帧 `changed {machines}` 不往界面转（它说的是表写完了，那一刻壳还没记下新的，界面这时重问拿到的是旧的）。
pub(crate) fn tell_followed(replay: &crate::event_replay::EventReplay) {
    let topic = serde_json::to_value(cmd_wire::frame::Topic::Machines)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    replay.changed(&crate::origin::Origin::local(), &topic, "{}".into());
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/machine_table_tests.rs"]
mod tests;
