//! **漂移账的通用形**（跟哪一家无关）：各家把「看不懂的记录」记在自己的账本里（Claude Code 那一份住
//! `claudecode/drift.rs`），注册表 [`super::RecordFace::drift`] 交出来的是这一形；帧命令 `drift-report` 把各家的面并成一份。
//!
//! 面的名字是那一家自己的词（`unknown_record_type` …），通用层不认，原样带着；后果那一句也是那一家写好的。

/// 一个键的记账。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct DriftEntry {
    /// 看不懂的那个值（记录 type / kind / status / token）。
    pub(crate) key: String,
    /// 见过多少次（本进程内的计数，不落盘）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) count: u64,
    /// **首见**的一条样例（截断）。后来的不再覆盖 —— 第一条最接近「它刚出现时长什么样」。
    pub(crate) first_sample: Option<String>,
}

/// 一个面的快照。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct DriftFaceReport {
    /// 那一家的面的名字（那一家自己的词）。
    pub(crate) face: String,
    /// 这个面「看不懂时会发生什么」（那一家写好的那一句）。
    pub(crate) consequence: String,
    /// 按 `count` 降序、同数按键名升序。
    pub(crate) entries: Vec<DriftEntry>,
    /// 该面是否已经溢出（键数触顶）。溢出后新键并进那一家的溢出键。
    pub(crate) overflowed: bool,
}

/// `drift-report` 的应答：注册表里各家的面并在一起（注册序；不记的那一家没有面）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct DriftReport {
    pub(crate) faces: Vec<DriftFaceReport>,
}
