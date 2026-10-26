//! [`Sid`]：会话 id（resume 的 sid · `@ccm_sid` 身份标记 · 按 sid 找会话）。线上就是一个字符串；
//! 值域只有一份（`shell_quote_core::session_id_ok`：1..=64 位 · 首字符 ASCII 字母数字 · 其余 `[A-Za-z0-9-]`），解码那一下就判，
//! 判不过的形状进不了这个类型 ⇒ 收它的地方不再各写一遍形状检查。

use serde::{Deserialize, Serialize};

/// 会话 id。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"), type = "string")
)]
pub struct Sid(String);

impl Sid {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 判不过值域的那个串（原样带着，报错时说出来）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotASid(pub String);

impl std::fmt::Display for NotASid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "not a session id (1-{} chars, starts with [A-Za-z0-9], then [A-Za-z0-9-]): {:?}",
            shell_quote_core::SESSION_ID_MAX,
            self.0
        )
    }
}

impl TryFrom<String> for Sid {
    type Error = NotASid;
    fn try_from(s: String) -> Result<Self, NotASid> {
        if shell_quote_core::session_id_ok(&s) {
            Ok(Sid(s))
        } else {
            Err(NotASid(s))
        }
    }
}

impl std::str::FromStr for Sid {
    type Err = NotASid;
    fn from_str(s: &str) -> Result<Self, NotASid> {
        Sid::try_from(s.to_string())
    }
}

impl std::ops::Deref for Sid {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for Sid {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for Sid {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/sid_tests.rs"]
mod tests;
