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

impl std::fmt::Display for Said {
    /// 只出那一句（原话不跟着进任何拼出来的句子）。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.said)
    }
}

impl Said {
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
