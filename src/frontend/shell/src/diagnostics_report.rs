//! 「复制诊断信息」那一段：一个命令出整段可复制的诊断文本（日志页的主按钮）—— 只读。
//!
//! 一段里有：这一版的版本号与构建标识 · 各台的系统 · 状态码与原因码（与机器列表同一份成品）· 读不懂的数据
//!（每台那本记录账的条数；读不到的那台照实说读不到）· config.json 里认不出的项 · 日志位置。不含会话内容与 key。
//! 记录账与认不出的键由界面交来（记录账界面经通道问各台 `drift-report`，壳不另开发送点；键表住界面），条数在这里数。
//! 日志页那一行「未识别数据 · …」读同一份答复里的数（不另起一个数）。

use crate::copy_table::copy_text;
use serde::{Deserialize, Serialize};

/// 界面交来的一台的记录账（`drift-report` 的应答原样；读不到 ⇒ `null`）。
#[derive(Debug, Clone, Deserialize)]
pub struct DriftIn {
    pub origin: String,
    pub report: Option<serde_json::Value>,
}

/// 一台的未识别数据：`records` ＝ 那台记录账里认不出的条数；`None` ＝ 读不到（连不上 / 那台答不了）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct UnknownOnMachine {
    /// 那台的名字（本机 ⇒ 文案表里那个词）。
    pub machine: String,
    #[cfg_attr(test, ts(type = "number | null"))]
    pub records: Option<u64>,
}

/// 整份答复。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReport {
    /// 整段可复制的诊断文本。
    pub text: String,
    /// 每台一行（本机在前，其余照机器表的顺序）。
    pub unknown: Vec<UnknownOnMachine>,
    /// config.json 里认不出的项数。
    #[cfg_attr(test, ts(type = "number"))]
    pub config_unknown: u64,
}

/// 一台的事实（拼文本用）。
pub(crate) struct MachineLine {
    pub name: String,
    pub os: Option<String>,
    pub state: String,
    pub reason: Option<String>,
    pub version: Option<String>,
    pub build: Option<String>,
    pub records: Option<u64>,
}

/// 拼整段（纯函数：判据拿事实喂）。
pub(crate) fn render(
    version: &str,
    build: Option<&str>,
    machines: &[MachineLine],
    config_unknown: &[String],
    log_file: Option<&str>,
) -> DiagnosticsReport {
    let none = copy_text("rsDiagReport.text.none", &[]);
    let mut out = vec![
        copy_text("rsDiagReport.text.head", &[]),
        copy_text(
            "rsDiagReport.text.version",
            &[("version", version), ("build", build.unwrap_or(&none))],
        ),
        copy_text("rsDiagReport.text.machines", &[]),
    ];
    for m in machines {
        let reason = m
            .reason
            .as_deref()
            .map(|r| copy_text("rsDiagReport.text.reason", &[("reason", r)]))
            .unwrap_or_default();
        out.push(copy_text(
            "rsDiagReport.text.machine",
            &[
                ("machine", &m.name),
                ("os", m.os.as_deref().unwrap_or(&none)),
                ("state", &m.state),
                ("reason", &reason),
                ("version", m.version.as_deref().unwrap_or(&none)),
                ("build", m.build.as_deref().unwrap_or(&none)),
            ],
        ));
    }
    out.push(copy_text("rsDiagReport.text.unknown", &[]));
    for m in machines {
        out.push(match m.records {
            Some(n) => copy_text(
                "rsDiagReport.text.records",
                &[("machine", &m.name), ("n", &n.to_string())],
            ),
            None => copy_text("rsDiagReport.text.unread", &[("machine", &m.name)]),
        });
    }
    out.push(copy_text(
        "rsDiagReport.text.config",
        &[
            ("n", &config_unknown.len().to_string()),
            (
                "keys",
                &if config_unknown.is_empty() {
                    none.clone()
                } else {
                    config_unknown.join(&copy_text("rsDiagReport.text.keySep", &[]))
                },
            ),
        ],
    ));
    out.push(match log_file {
        Some(p) => copy_text("rsDiagReport.text.log", &[("path", p)]),
        None => copy_text("rsDiagReport.text.noLog", &[]),
    });
    DiagnosticsReport {
        text: out.join("\n"),
        unknown: machines
            .iter()
            .map(|m| UnknownOnMachine {
                machine: m.name.clone(),
                records: m.records,
            })
            .collect(),
        config_unknown: config_unknown.len() as u64,
    }
}

/// 那台记录账（`drift-report`）里认不出的条数：各面各条的 `count` 相加。形状不对 ⇒ `None`（不猜成 0）。
pub(crate) fn records_in(v: &serde_json::Value) -> Option<u64> {
    let faces = v.get("faces")?.as_array()?;
    let mut n = 0u64;
    for f in faces {
        for e in f.get("entries")?.as_array()? {
            n = n.saturating_add(e.get("count")?.as_u64()?);
        }
    }
    Some(n)
}

/// 一台的状态码（与机器列表同一份成品，按 serde 的名字写进文本）。
fn state_word(m: &crate::machine_state::MachineState) -> String {
    serde_json::to_value(m.state)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// 日志页「复制诊断信息」：整段文本 ＋ 那一行用的数。`config_unknown` 是界面那份键表认不出的顶层键；
/// `drift` 是界面经通道问回来的各台记录账（没交的那台 ⇒ 读不到）。
#[tauri::command]
pub async fn diagnostics_report(
    config_unknown: Vec<String>,
    drift: Vec<DriftIn>,
    state: tauri::State<'_, std::sync::Arc<crate::logging::LoggingState>>,
) -> Result<DiagnosticsReport, String> {
    let log_file = state.log_file_info().current_file;
    let local = crate::inbound_client::LOCAL_ORIGIN;
    let mut origins: Vec<(String, String)> =
        vec![(local.to_string(), copy_text("rsDiagReport.text.local", &[]))];
    for (cfg, _) in crate::load_all_remote_configs() {
        let label = cfg.origin_label();
        origins.push((label.clone(), label));
    }
    let mine = crate::byte_table::my_backend_id();
    let machines: Vec<MachineLine> = origins
        .into_iter()
        .map(|(origin, name)| {
            let m = crate::backend_control::machine_now(&origin);
            let build = if origin == local {
                mine.map(str::to_string)
            } else {
                crate::machine_state::build_of(&origin)
            };
            let records = drift
                .iter()
                .find(|d| d.origin == origin)
                .and_then(|d| d.report.as_ref())
                .and_then(records_in);
            MachineLine {
                name,
                os: m.os.clone(),
                state: state_word(&m),
                reason: m.reason.clone(),
                version: m.version.clone(),
                build,
                records,
            }
        })
        .collect();
    Ok(render(
        crate::machine_state::PRODUCT_VERSION,
        mine,
        &machines,
        &config_unknown,
        log_file.as_deref(),
    ))
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/diagnostics_report_tests.rs"]
mod tests;
