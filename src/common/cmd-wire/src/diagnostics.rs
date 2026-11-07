//! `diagnostics-report`：日志页「复制诊断信息」那一整段，本机后端出成品（拼、数、判都在后端 `faces/diagnostics_face.rs`）。
//! 壳只交它自己的三样事实：monitor 这一版的版本号 · 日志文件在哪 · `config.json` 原文的顶层键。

use serde::{Deserialize, Serialize};

/// `diagnostics-report` 的入参：壳自己的事实（后端看不见的那几样）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagnosticsReportReq {
    /// monitor 这一版的版本号。
    pub version: String,
    /// 今天的日志文件（没写文件 ⇒ 没有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub log_file: Option<String>,
    /// `config.json` 原文的顶层键（照盘上的先后；读不到 ⇒ 空）。
    #[serde(default)]
    pub config_keys: Vec<String>,
}

/// 一台的未识别数据：`records` ＝ 那台记录账里认不出的条数；`None` ＝ 读不到（断开 / 那台答不了）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase")]
pub struct UnknownOnMachine {
    /// 那台的名字（本机 ⇒ 文案表里那个词）。
    pub machine: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub records: Option<u64>,
}

/// `diagnostics-report` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReport {
    /// 整段可复制的诊断文本。
    pub text: String,
    /// 每台一行（本机在前，其余照机器表的顺序）。
    pub unknown: Vec<UnknownOnMachine>,
    /// config.json 里认不出的项数。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub config_unknown: u64,
}
