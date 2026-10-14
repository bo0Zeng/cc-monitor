//! **成品格的两种标记类型**：写好的字（[`Words`]）与语气（[`Tone`]）。本体住 `cmd-wire::cells`（两端同一份）。
//!
//! 字段类型是 [`Words`] ⇒ 格目录（`faces/cells_catalog.rs`）记它为 `text`；是 [`Tone`] ⇒ 记为 `tone`；别的都是值。

pub(crate) use cmd_wire::cells::{Tone, Words};

/// 格目录认这两个类型名（序列化时 serde 交给序列化器的名字）。
pub(crate) const WORDS_TYPE: &str = "Words";
pub(crate) const TONE_TYPE: &str = "Tone";
