//! cc-bus 信封里过线的那几样值：[`BusAct`]（这一封是做什么的）。线上是英文码，给人看的词由各端照码取文案表。

use serde::{Deserialize, Serialize};

/// 这一封是做什么的（线上码；显示为 问 · 答 · 请 · 告知 · 交回 · 更正，词住文案表 `ccBus.act.*`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum BusAct {
    /// 问。
    Ask,
    /// 答（接在一条问后面）。
    Answer,
    /// 请（要对方做一件事）。
    Request,
    /// 告知。
    Inform,
    /// 交回（做完了，接在那一条请后面）。
    Handback,
    /// 更正（改前面说过的那一条）。
    Correct,
}

impl BusAct {
    /// 这一种必须写明接在哪一条后面（`re`）：答 · 交回 · 更正。判定只在这一处（后端信封校验调它）。
    pub fn needs_re(self) -> bool {
        matches!(self, BusAct::Answer | BusAct::Handback | BusAct::Correct)
    }
}
