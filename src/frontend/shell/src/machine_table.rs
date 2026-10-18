//! **机器表的最近一份**：机器表住本机后端（`~/.cc-monitor/machines.json`，后端 `control/machines.rs`），本壳不读盘，
//! 只记本机后端 `machines-read` 最近答的那一份，远端那几条流照它起、停（[`crate::reconcile_remote_streams`]）。
//!
//! - 什么时候问：本机入方向通道登记上那一刻问一次（[`refresh`]，两条载体各调一次），之后本机流每来一帧 `changed {machines}`
//!   （后端每次写完响一下）再问一次。问完当场对齐远端流 ⇒ 设置页存完不必另叫「对齐」。
//! - 本机后端连上之前表是空的 ⇒ 远端流不起（本机先起，今天也是这个顺序）。
//! - 读不懂 ⇒ 当空表（一台都不连），上屏一句（`machines-unreadable`）；问不到 ⇒ 留着上一份。

use serde_json::Value;
use std::sync::Mutex;

/// 最近一答里的那几台（原样的 JSON 对象，形状同后端 `Host`）。
static LATEST: Mutex<Vec<Value>> = Mutex::new(Vec::new());

/// 最近一答里的那几台（本机后端还没答过 ⇒ 空）。
pub(crate) fn hosts_now() -> Vec<Value> {
    LATEST.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// 那一答 ⇒ 照用的那几台 ＋ 读不懂时那一句（`(路径, 原因)`）。**纯函数**。
pub(crate) fn hosts_of(reply: &Value) -> (Vec<Value>, Option<(String, String)>) {
    let hosts = reply
        .get("hosts")
        .and_then(Value::as_array)
        .cloned()
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

/// 后台问本机后端一次、记下、对齐远端流（不挡调用方）。
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
                tracing::info!("没问到本机的机器表，远端流照上一份：{e}");
                return;
            }
        };
        let (hosts, unreadable) = hosts_of(&reply);
        if let Some((path, why)) = unreadable {
            tracing::error!("{path} 读不懂，远端一台都不连：{why}");
            crate::ui_error::tell(crate::ui_error::UiError::MachinesUnreadable { path, why });
        }
        *LATEST.lock().unwrap_or_else(|p| p.into_inner()) = hosts;
        let done = tauri::async_runtime::spawn_blocking(crate::reconcile_remote_streams).await;
        match done {
            Ok(d) if !d.started.is_empty() || !d.stopped.is_empty() => {
                tracing::info!(
                    "照机器表对齐了远端流：起 {:?} 停 {:?}",
                    d.started,
                    d.stopped
                )
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("照机器表对齐远端流没跑完：{e}"),
        }
        crate::prefs_migration::after_machines_read(&reply);
    });
}

/// 壳照机器表对齐完远端流之后在本机 `changed/machines` 上说一声：主窗口据它重问 `backend_machines`、补订新来的 · 退订没了的。
/// 本机后端自己那一帧 `changed {machines}` 不往界面转（它说的是表写完了，那一刻远端流还没对齐，界面这时重问拿到的是旧的）。
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
