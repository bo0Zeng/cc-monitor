//! **成品格的两种标记类型**：写好的字（[`Words`]）与语气（[`Tone`]）。
//!
//! 核心出的每件成品由「值」与「写好的字」组成（`src/doc/ARCHITECTURE.md` §「核心与适配层」）。哪一格是写好的字、哪一格是语气，
//! 不靠格名猜（`blocks[].text` 是原文、`cost.text` 是写好的字，名字一样），由这一格的 Rust 类型说：
//! 字段类型是 [`Words`] ⇒ 格目录（`faces/cells_catalog.rs`）记它为 `text`；是 [`Tone`] ⇒ 记为 `tone`；别的都是值。
//! 线上形状不受影响：[`Words`] 上线就是一个字符串，[`Tone`] 上线就是一个小写词。

use serde::{Deserialize, Serialize};

/// 核心写好的一句 / 一段给人看的字（走文案表、按这台的语言与本地钟）。出口照抄，不拼不改。
///
/// 是一个**非透明**的单字段元组结构：serde 序列化它时经 `serialize_newtype_struct("Words", …)`，
/// 格目录的走查器认这个名字；`serde_json` 照样只写里面那个字符串。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Words(pub String);

impl From<&str> for Words {
    fn from(s: &str) -> Self {
        Words(s.to_string())
    }
}

impl From<String> for Words {
    fn from(s: String) -> Self {
        Words(s)
    }
}

/// 一格字的语气（闭集）：常规 · 失败 · 现在在做的那一步 · 需手动 · 该留意了（还没出错，如上下文快满）·
/// 后台有事在跑（不要人、也不是这会儿在干的那一步）。
/// 出口按它选颜色，不按业务码自己判。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Plain,
    Fail,
    Now,
    Need,
    Warn,
    Busy,
}

/// 格目录认这两个类型名（序列化时 serde 交给序列化器的名字）。
pub(crate) const WORDS_TYPE: &str = "Words";
pub(crate) const TONE_TYPE: &str = "Tone";
