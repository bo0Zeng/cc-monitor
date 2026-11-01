//! **成品格的两种标记类型**：写好的字（[`Words`]）与语气（[`Tone`]）—— 两端同一份（后端写、手机嵌的 Rust 核心按它解）；
//! 另有上次读数那一格（[`SeenAt`]：时刻 ＋ 写好的那一句）。
//!
//! 核心出的每件成品由「值」与「写好的字」组成。哪一格是写好的字、哪一格是语气，不靠格名猜，由这一格的 Rust 类型说：
//! 后端的格目录（`faces/cells_catalog.rs`）认的是这两个类型名（序列化时交给序列化器的那个名字），所以它俩住这里、不换成 `String`。
//! 线上形状不受影响：[`Words`] 上线就是一个字符串，[`Tone`] 上线就是一个小写词。

use serde::{Deserialize, Serialize};

/// 核心写好的一句 / 一段给人看的字（走文案表、按这台的语言与本地钟）。出口照抄，不拼不改。
///
/// 是一个**非透明**的单字段元组结构：serde 序列化它时经 `serialize_newtype_struct("Words", …)`，
/// 格目录的走查器认这个名字；`serde_json` 照样只写里面那个字符串。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"), type = "string")
)]
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
/// 后台有事在跑（不要人、也不是这会儿在干的那一步）· 没连上（那一端此刻够不着：画离线的灰，与常规不同色，也不算出错）。
/// 出口按它选颜色，不按业务码自己判。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Plain,
    Fail,
    Now,
    Need,
    Warn,
    Busy,
    Offline,
}

/// 上次读数那一格：记下的时刻 ＋ 后端按看的那一台的时区写好的那一句（「采样于 12:03」）。
/// 离线时画上次读数的那几处（账号页 · 「文件与数据」页 · 计划条）照摆，不自己算时长、不自己写时刻。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct SeenAt {
    /// 记下的时刻（毫秒）。
    #[serde(rename = "atMs")]
    #[cfg_attr(any(test, feature = "ts"), ts(rename = "atMs", type = "number"))]
    pub at_ms: u64,
    /// 「采样于 12:03」（后端照文案表与那一台的钟写）。
    pub said: Words,
}
