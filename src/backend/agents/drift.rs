//! **漂移账的通用形**（跟哪一家无关）：各家把「看不懂的记录」记在自己的账本里（Claude Code 那一份住
//! `claudecode/drift.rs`），注册表 [`super::RecordFace::drift`] 交出来的是这一形；帧命令 `drift-report` 把各家的面并成一份。
//!
//! 面的名字是那一家自己的词（`unknown_record_type` …），通用层不认，原样带着；后果那一句也是那一家写好的。
//! 也收得进来（`Deserialize`）：本机后端经常驻流问到别的台那一份（诊断信息 `diagnostics-report`）。

/// 一个键的记账。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct DriftEntry {
    /// 看不懂的那个值（记录 type / kind / status / token）。
    pub(crate) key: String,
    /// 见过多少次（本进程内的计数，不落盘）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub(crate) count: u64,
}

/// 一个面的快照。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct DriftFaceReport {
    /// 那一家的面的名字（那一家自己的词）。
    pub face: String,
    /// 按 `count` 降序、同数按键名升序（键数触顶后新键并进那一家的溢出键）。
    pub entries: Vec<DriftEntry>,
}

/// `drift-report` 的应答：注册表里各家的面并在一起（注册序；不记的那一家没有面）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct DriftReport {
    pub faces: Vec<DriftFaceReport>,
}
