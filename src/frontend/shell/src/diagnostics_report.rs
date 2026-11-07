//! 「复制诊断信息」那一段（日志页的主按钮）—— 只读。整段由本机后端出（`diagnostics-report`，`faces/diagnostics_face.rs`）：
//! 各台状态与原因码 · 构建标识 · 每台记录账的条数 · config.json 里认不出的项都在那边并、数、判。
//! 壳只交它自己的三样事实（[`req_of`]：monitor 这一版的版本号 · 今天的日志文件 · config.json 原文的顶层键），回包原样交界面。
//! 本机后端答不了 ⇒ 照实说（这件事就不发生，壳不另拼一份）。日志页那一行「未识别数据 · …」读同一份答复里的数。

use crate::detail::Shell as _;
use cmd_wire::diagnostics::{DiagnosticsReport, DiagnosticsReportReq};
use cmd_wire::Call as _;
use copy_core::copy_text;
use copy_core::said::Said;

/// 一趟的上限：本机后端要经各台常驻流各问一次记录账（同时问），给足余量同时防卡死。
const BUDGET: std::time::Duration = std::time::Duration::from_secs(15);

/// 壳自己的事实 ⇒ 入参（纯函数）。`config` 是盘上那份 config.json（读不到 ⇒ `None`）；不是对象 ⇒ 没有顶层键可交。
pub(crate) fn req_of(
    version: &str,
    log_file: Option<String>,
    config: Option<&serde_json::Value>,
) -> DiagnosticsReportReq {
    DiagnosticsReportReq {
        version: version.to_string(),
        log_file,
        config_keys: config
            .and_then(serde_json::Value::as_object)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default(),
    }
}

/// 日志页「复制诊断信息」：整段文本 ＋ 那一行用的数（本机后端出的成品，原样交界面）。
#[tauri::command]
pub async fn diagnostics_report(
    app: tauri::AppHandle,
) -> Result<DiagnosticsReport, cmd_wire::failed::Failed> {
    use crate::backend_route::{route_call_error, Routed};
    const NAME: &str = DiagnosticsReportReq::NAME;
    // 状态经 `AppHandle` 取、不收 `tauri::State<'_, …>` 参数：理由见 `lib.rs::bring_terminal_to_front` 那一句。
    let state = tauri::Manager::state::<std::sync::Arc<crate::logging::LoggingState>>(&app)
        .inner()
        .clone();
    let own = |s: Said| {
        s.retold(copy_text("rsDiagReport.call.failed", &[]))
            .named("diagnostics_report")
    };
    let config = crate::config::load_config().ok();
    let req = req_of(
        crate::PRODUCT_VERSION,
        state.log_file_info().current_file,
        config.as_ref(),
    );
    let client = crate::inbound_client::local_backend_accepting(NAME)
        .await
        .map_err(|said| own(Said::from(said)))?;
    let deadline = tokio::time::Instant::now() + BUDGET;
    let got = client.ask(None, &req, None, deadline).await.map_err(|e| {
        let said = match route_call_error(&e, &copy_core::local_machine(), |_code, message| {
            message.to_string()
        }) {
            Routed::NoChannel(s) | Routed::Refused(s) => s,
        };
        crate::detail::of_call(said, NAME, &e)
    })?;
    got.map_err(|_| {
        own(Said::from(copy_core::reply_unreadable(
            &copy_core::local_machine(),
        )))
    })
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/diagnostics_report_tests.rs"]
mod tests;
