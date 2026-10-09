//! 一次失败**给人看的那一句 ＋ 下层原话**（可缺）。句子只带原因词（[`crate::io_reason`] 那一类）；原话不上句子，
//! 走到应答 / 收场帧那一层进「复制详情」（[`crate::detail`]）。
//!
//! 纯数据（后端 · 共享契约 crate 都用这一份）：记日志那一口（成不了应答、只成别处一句时把原话记一行）要日志库，
//! 住后端 `common/said.rs`，不在这里。

/// 那一句 ＋ 下层原话。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    pub said: String,
    pub raw: Option<String>,
}

impl From<String> for Said {
    /// 没有下层原话的那几形（参数 · 路径解析拒 · 撤了）。
    fn from(said: String) -> Said {
        Said { said, raw: None }
    }
}

impl From<(String, Option<String>)> for Said {
    /// 下面那一层（够不到本类型的）交上来的一对：那一句 ＋ 原话。
    fn from((said, raw): (String, Option<String>)) -> Said {
        Said { said, raw }
    }
}

impl From<&str> for Said {
    fn from(said: &str) -> Said {
        Said::from(said.to_string())
    }
}

impl std::fmt::Display for Said {
    /// 只出那一句（原话不跟着进任何拼出来的句子）。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.said)
    }
}

impl Said {
    /// 外层换一句（里层那一句给 `f` 当原因 / 对象用），原话原样跟着。
    pub fn wrap(self, f: impl FnOnce(&str) -> String) -> Said {
        Said {
            said: f(&self.said),
            raw: self.raw,
        }
    }

    /// 进日志的那一行：那一句 ＋ 原话（日志要原话；屏上的句子不走这里）。
    pub fn logged(&self) -> String {
        match &self.raw {
            Some(r) => format!("{}: {r}", self.said),
            None => self.said.clone(),
        }
    }

    /// 那一句（已带原因词）＋ 下层原话（空 ⇒ 不带）。
    pub fn with_raw(said: String, raw: impl std::fmt::Display) -> Said {
        let raw = raw.to_string();
        Said {
            said,
            raw: (!raw.trim().is_empty()).then_some(raw),
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/copy-core/said_tests.rs"]
mod tests;
