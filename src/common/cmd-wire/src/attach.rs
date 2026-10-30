//! 接常驻流那一段握手的两行（hello 之后、流之前）：客户端交的 attach 行 · 常驻后端（或它前面的小中继）回的那一行。
//! 后端 · 桌面 · 手机同一份；新旧由那台接 attach 行时判（[`Relation`]），客户端不另问。
//! 另住手机聊天输入框上方那一排附件进出内嵌核心的两份（[`AttachWant`] ⇒ [`AttachFace`]）。

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// attach 行：「我要流」＋ 看的这一台的时区 ＋ 手上这一版的身份 ＋ 收不收 deflate ＋ 在看名单。认不得的格 ⇒ 那台回 `refused`（认不得这一形的就是比这一形早的）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachLine {
    pub attach: Yes,
    /// 看的这一台的时区（IANA 名；缺 · 认不得 ⇒ UTC）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tz: Option<String>,
    /// 手上这一版的身份（`BUILD_ID`）；手上没带后端字节 ⇒ 不带，那台就不回新旧。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mine: Option<String>,
    /// 我收 deflate：那台回话里认了（[`AttachReply::Ok`] 的 `deflate`）⇒ 回话那一行之后它写过来的字节整条是一个 raw deflate 流（每帧 sync flush）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deflate: bool,
    /// 在看名单（同 `stream-watch` 的入参）：那台从这条流的第一帧起照它发。不带 ＝ 从没报过 ＝ 全发（不报名单的载体）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch: Option<crate::watch::WatchAsk>,
}

impl AttachLine {
    /// 一整行（带换行）。
    pub fn line(&self) -> String {
        let mut s = serde_json::to_string(self).unwrap_or_default();
        s.push('\n');
        s
    }
}

/// 线上恒为 `true` 的那一格（别的值 ⇒ 解不开）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Yes;

impl Serialize for Yes {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bool(true)
    }
}

impl<'de> Deserialize<'de> for Yes {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Yes, D::Error> {
        match bool::deserialize(d)? {
            true => Ok(Yes),
            false => Err(serde::de::Error::custom("`attach` must be true")),
        }
    }
}

/// attach 回话（小中继断开时它也是第一行）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "attach", rename_all = "snake_case")]
pub enum AttachReply {
    /// 接上了；`relation` ＝ 那台相对 attach 行里的 `mine`（没带 `mine` ⇒ 缺）。
    Ok {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relation: Option<Relation>,
        /// 认了 attach 行里的 `deflate`：这一行之后整条压（客户端没要 ⇒ 不回、不压）。
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        deflate: bool,
    },
    /// 拒了：原因是闭集的常量（后端 `wire/listen.rs`）。
    Refused { reason: String },
}

impl AttachReply {
    /// 一整行（带换行）。
    pub fn line(&self) -> String {
        let mut s = serde_json::to_string(self).unwrap_or_default();
        s.push('\n');
        s
    }

    /// 一行 ⇒ 回话；不是这一形 ⇒ `None`。
    pub fn of(line: &str) -> Option<AttachReply> {
        serde_json::from_str(line.trim()).ok()
    }
}

/// 那一份构建相对手上这一版（比大小的函数是 `deploy_contract::build_relation`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "VersionRelation", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Same,
    Older,
    Newer,
    /// 任一边解不出序键 · 同序不同名 · 手上没带后端字节。
    Incomparable,
}

// ───── 手机聊天输入框上方那一排附件：Kotlin 交此刻每张卡走到哪，内嵌核心回那一排的字（`mobcore` 的 `attach::face`） ─────

/// 交核心的那一份：每张卡走到哪 ＋ 断着时那台离线那一句 ＋ 手机这边的错。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AttachWant {
    pub cards: Vec<AttachWantCard>,
    /// 那台离线那一句：只在有排队的、又断着时给。
    pub offline: Option<String>,
    /// 手机这边的错（那一条最先说它）。
    pub local: Option<AttachLocal>,
}

/// 一张卡：名字 ＋ 走到哪。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AttachWantCard {
    pub name: String,
    pub phase: AttachCardPhase,
}

/// 一张卡走到哪（传输中不带进度数字：卡上只转环）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttachCardPhase {
    /// 还没轮到 · 断着等连上。
    Queued,
    Moving,
    Landed,
    /// 开传前那台就不收：那台写好的那一句（写在卡上）。
    Refused { said: String },
    /// 传 · 落没成：那台那一句（进卡下那一条）。
    Failed { said: String },
    /// 本机那一份副本开不了。
    Lost,
}

/// 手机这边的错。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum AttachLocal {
    /// 拷不进本机（空间满 …）。
    NoRoom,
    /// 没有能拍照的系统相机。
    NoCamera,
}

/// 核心回的那一排的字：每张卡（与交进来的同序）＋ 卡下那一条。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AttachFace {
    pub cards: Vec<AttachFaceCard>,
    pub note: Option<AttachFaceNote>,
}

/// 一张卡上的字：第二行（传输中 · 落好了 ⇒ 不写）＋ ✕ 的读屏名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AttachFaceCard {
    pub line: Option<String>,
    pub remove: String,
}

/// 卡下那一条：那一句 ＋ 按钮（没有 ⇒ `None`）＋ 是不是断着那一档（虚线）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AttachFaceNote {
    pub text: String,
    pub act: Option<AttachAct>,
    pub dashed: bool,
}

/// 卡下那一条上的按钮。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum AttachAct {
    /// 那台的错：重传失败的那几份。
    Retry,
    /// 手机这边存不下：再挑一份。
    Repick,
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/attach_tests.rs"]
mod tests;
