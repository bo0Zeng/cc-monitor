//! 一次失败**给人看的那一句 ＋ 下层原话**（可缺）。句子只带原因词（`copy_core::io_reason` 那一类）；原话不上句子，
//! 走到应答 / 收场帧那一层进「复制详情」（`stream::detail`）。
//!
//! 后端里原来回 `String` 的失败要带原话时换成它：`?` 从 `String` 进来（没有原话的那几形）不改调用点。

/// 那一句 ＋ 下层原话。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Said {
    pub(crate) said: String,
    pub(crate) raw: Option<String>,
}

impl From<String> for Said {
    /// 没有下层原话的那几形（参数 · 路径解析拒 · 撤了）。
    fn from(said: String) -> Said {
        Said { said, raw: None }
    }
}

impl From<(String, Option<String>)> for Said {
    /// 下面那一层（`platform/`，够不到本类型）交上来的一对：那一句 ＋ 原话。
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
    pub(crate) fn wrap(self, f: impl FnOnce(&str) -> String) -> Said {
        Said {
            said: f(&self.said),
            raw: self.raw,
        }
    }

    /// 这次失败**不成应答**、只成别处的一句（成功应答里的提示格 · 状态格 · 日志行里拼的一段，那一格没有详情位）：
    /// 那一句交出去，原话记一行日志（不上句子、也不丢）。成应答的那几形走 `Fail::from((码, Said))`。
    pub(crate) fn into_note(self) -> String {
        if let Some(r) = &self.raw {
            tracing::warn!("{}: {r}", self.said);
        }
        self.said
    }

    /// 进日志的那一行：那一句 ＋ 原话（日志要原话；屏上的句子不走这里）。
    pub(crate) fn logged(&self) -> String {
        match &self.raw {
            Some(r) => format!("{}: {r}", self.said),
            None => self.said.clone(),
        }
    }

    /// 那一句（已带原因词）＋ 下层原话（空 ⇒ 不带）。
    pub(crate) fn with_raw(said: String, raw: impl std::fmt::Display) -> Said {
        let raw = raw.to_string();
        Said {
            said,
            raw: (!raw.trim().is_empty()).then_some(raw),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/common/said_tests.rs"]
mod tests;
