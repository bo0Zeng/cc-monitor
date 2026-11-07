//! 帧面 `diagnostics-report`（typed）：日志页「复制诊断信息」那一整段，本机后端出成品 —— 只读。
//!
//! 一段里有：monitor 这一版的版本号与本机后端的构建标识 · 各台的系统 · 状态码与原因码（与机器列表同一份成品，`machine_state::read`）·
//! 读不懂的数据（每台那本记录账的条数，[`crate::faces::read_face::drift_hub`] 经各台常驻流同时问 `drift-report`；读不到的那台照实说读不到）·
//! `config.json` 里认不出的顶层键（键表 `src/shared/config-keys.json`，界面 `config.ts` 与这里共读这一份）· 日志位置。不含会话内容与 key。
//! 壳只交它自己的三样事实（[`DiagnosticsReportReq`]：版本号 · 日志文件 · config 原文的顶层键）；拼、数、判都在这里。

use cmd_wire::diagnostics::{DiagnosticsReport, DiagnosticsReportReq, UnknownOnMachine};
use cmd_wire::machine::MachineState;
use copy_core::copy_text;
use std::collections::{BTreeMap, BTreeSet};

/// config.json 顶层键 → 它的主人（界面 `config.ts::CONFIG_KEY_OWNERS` 读同一份）。
const CONFIG_KEYS_JSON: &str = include_str!("../../shared/config-keys.json");

/// 登记过的顶层键（读一次）。
fn known_config_keys() -> &'static BTreeSet<String> {
    static K: std::sync::OnceLock<BTreeSet<String>> = std::sync::OnceLock::new();
    K.get_or_init(|| {
        serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(CONFIG_KEYS_JSON)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    })
}

/// 壳交来的顶层键里认不出的那几个（照它在盘上的先后）。**纯函数**。
pub(crate) fn unknown_config_keys(keys: &[String]) -> Vec<String> {
    let known = known_config_keys();
    keys.iter()
        .filter(|k| !known.contains(*k))
        .cloned()
        .collect()
}

/// 一台的事实（拼文本用）。
#[derive(Debug, Clone, PartialEq, Eq)]
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
    let none = copy_text("acct.val.none", &[]);
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
                    config_unknown.join(&copy_text("plan.page.listSep", &[]))
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

/// 那台记录账（`drift-report` 的应答）里认不出的条数：各面各条的 `count` 相加。形状不对 ⇒ `None`（不猜成 0）。
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

/// 一个码按 serde 的名字写进文本（状态 · 原因码与机器列表同一份成品）。
fn wire_word(v: impl serde::Serialize) -> Option<String> {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
}

/// 此刻接着的各台那条常驻流 hello 自报的构建标识（断着的那台 ⇒ 没有）。
fn remote_builds() -> BTreeMap<String, String> {
    crate::control::feeds::attached_hellos()
        .into_iter()
        .filter_map(|f| match f {
            crate::wire::Frame::Relayed(cmd_wire::frame::Relayed::Line { machine, line }) => {
                serde_json::from_str::<cmd_wire::frame::Hello>(&line)
                    .ok()
                    .map(|h| (machine, h.build_id))
            }
            _ => None,
        })
        .collect()
}

/// 机器列表那一份成品里，整段要写的那几格（状态 · 原因码按线上的名字）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct StateWords {
    pub os: Option<String>,
    pub state: String,
    pub reason: Option<String>,
    pub version: Option<String>,
}

fn words_of(m: &MachineState) -> StateWords {
    StateWords {
        os: m.os.clone(),
        state: wire_word(m.state).unwrap_or_default(),
        reason: m.reason.and_then(wire_word),
        version: m.version.clone(),
    }
}

/// 并好的事实 ⇒ 每台一行：本机在前，其余照 `drift` 的先后（`drift_hub` 已按机器表排好、去了本机与重复）。**纯函数**。
pub(crate) fn lines_of(
    states: &BTreeMap<String, StateWords>,
    builds: &BTreeMap<String, String>,
    drift: &[(String, Option<serde_json::Value>)],
) -> Vec<MachineLine> {
    let local = cmd_wire::machine::LOCAL;
    drift
        .iter()
        .map(|(origin, report)| {
            let m = states.get(origin).cloned().unwrap_or_default();
            let build = if origin == local {
                Some(crate::common::id::BUILD_ID.to_string())
            } else {
                builds.get(origin).cloned()
            };
            MachineLine {
                name: if origin == local {
                    copy_text("detail.label.local", &[])
                } else {
                    origin.clone()
                },
                os: m.os,
                state: m.state,
                reason: m.reason,
                version: m.version,
                build,
                records: report.as_ref().and_then(records_in),
            }
        })
        .collect()
}

crate::wire::typed::codes! {
    /// `diagnostics-report` 没有别的码。
    pub enum DiagnosticsReportErr {}
}

/// `diagnostics-report`：整段诊断文本 ＋ 那一行「未识别数据」用的数（同一份）。只读；读机器表进阻塞池，各台记录账同时问。
pub struct DiagnosticsReportCmd;

impl crate::wire::typed::Command for DiagnosticsReportCmd {
    crate::wire::typed::by_call!(DiagnosticsReportReq);
    type Err = DiagnosticsReportErr;
}

impl crate::wire::typed::Handle for DiagnosticsReportCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: DiagnosticsReportReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<DiagnosticsReport, DiagnosticsReportErr>>
           + Send
           + 'static {
        let until = cx.until;
        async move {
            let names = crate::faces::read_face::machine_names().await;
            let there = move |m: String| async move {
                crate::control::feeds::call::<crate::faces::read_face::DriftReportCmd>(
                    &m,
                    &crate::wire::typed::NoArgs {},
                    until,
                )
                .await
                .map_err(|f| tracing::warn!("diagnostics-report [{m}]: {}", f.said))
                .ok()
            };
            let merged =
                crate::faces::read_face::drift_hub(names, crate::agents::drift_report(), there)
                    .await;
            let drift: Vec<(String, Option<serde_json::Value>)> = merged
                .machines
                .into_iter()
                .map(|d| {
                    let report = d.report.and_then(|r| serde_json::to_value(r).ok());
                    (d.origin, report)
                })
                .collect();
            let states: BTreeMap<String, StateWords> = crate::control::machine_state::read()
                .iter()
                .map(|(k, m)| (k.clone(), words_of(m)))
                .collect();
            let lines = lines_of(&states, &remote_builds(), &drift);
            Ok(render(
                &req.version,
                Some(crate::common::id::BUILD_ID),
                &lines,
                &unknown_config_keys(&req.config_keys),
                req.log_file.as_deref(),
            ))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/diagnostics_face_tests.rs"]
mod tests;
