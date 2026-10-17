//! 接常驻流那一段握手的两行（hello 之后、流之前）：客户端交的 attach 行 · 常驻后端（或它前面的小中继）回的那一行。
//! 后端 · 桌面 · 手机同一份；新旧由那台接 attach 行时判（[`Relation`]），客户端不另问。

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// attach 行：「我要流」＋ 看的这一台的时区 ＋ 手上这一版的身份。认不得的格 ⇒ 那台回 `refused`（认不得这一形的就是比这一形早的）。
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

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/attach_tests.rs"]
mod tests;
