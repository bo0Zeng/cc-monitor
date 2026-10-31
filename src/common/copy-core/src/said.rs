//! 一次失败**码 ＋ 给人看的那一句 ＋ 下层原话 ＋ 按码定形的 `data`**（除那一句外都可缺）。句子只带原因词（[`crate::io_reason`] 那一类）；原话不上句子，
//! 走到应答 / 收场帧那一层进「复制详情」（[`crate::detail`]）。
//!
//! 纯数据（后端 · 共享契约 crate 都用这一份）：记日志那一口（成不了应答、只成别处一句时把原话记一行）要日志库，
//! 住后端 `common/said.rs`，不在这里。

/// 码缺着的那一次失败投成应答时补的码（协议文档里的兜底码）。
pub const FALLBACK_CODE: &str = "failed";

/// 码 ＋ 那一句 ＋ 下层原话 ＋ `data`。各处只经构造函数造它（[`Said::new`] · `From` · [`Said::with_raw`]），不写结构字面量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    /// 缺 ＝ 下层还说不出是哪一种；投成应答那一下补成 [`FALLBACK_CODE`]。
    pub code: Option<String>,
    /// 给人看的那一句（只带原因词）。
    pub said: String,
    /// 下层原话：进复制详情 · 日志，不上句子。
    pub raw: Option<String>,
    /// 只给协议文档里按码定了形的那几个码。
    pub data: Option<serde_json::Value>,
}

/// 原话去掉首尾空白；空 ⇒ 不带。
fn kept(raw: &str) -> Option<String> {
    let raw = raw.trim();
    (!raw.is_empty()).then(|| raw.to_string())
}

impl From<String> for Said {
    /// 没有下层原话的那几形（参数 · 路径解析拒 · 撤了）。
    fn from(said: String) -> Said {
        Said {
            code: None,
            said,
            raw: None,
            data: None,
        }
    }
}

impl From<(String, Option<String>)> for Said {
    /// 下面那一层（够不到本类型的）交上来的一对：那一句 ＋ 原话。
    fn from((said, raw): (String, Option<String>)) -> Said {
        Said {
            raw,
            ..Said::from(said)
        }
    }
}

impl From<(&'static str, String)> for Said {
    /// 码 ＋ 那一句（码在前）。
    fn from((code, said): (&'static str, String)) -> Said {
        Said::new(code, said)
    }
}

impl From<(String, String)> for Said {
    /// 码 ＋ 那一句（码在前）。
    fn from((code, said): (String, String)) -> Said {
        Said::new(&code, said)
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
    /// 码 ＋ 那一句（没有原话、没有 `data`）。
    pub fn new(code: &str, said: String) -> Said {
        Said::from(said).coded(code)
    }

    /// 外层定码（已有码照样换掉）。
    pub fn coded(mut self, code: &str) -> Said {
        self.code = Some(code.to_string());
        self
    }

    /// 投成应答那一下的码：缺着 ⇒ [`FALLBACK_CODE`]。
    pub fn code_or_fallback(&self) -> &str {
        self.code.as_deref().unwrap_or(FALLBACK_CODE)
    }

    /// 带上按码定了形的 `data`。
    pub fn with_data(mut self, data: serde_json::Value) -> Said {
        self.data = Some(data);
        self
    }

    /// 补一段下层原话（空 / 缺 ⇒ 不动；已有原话 ⇒ 新的一段接在下一行）。
    pub fn and_raw(mut self, raw: Option<&str>) -> Said {
        self.raw = match (self.raw.take(), raw.and_then(kept)) {
            (Some(had), Some(r)) => Some(format!("{had}\n{r}")),
            (a, b) => b.or(a),
        };
        self
    }

    /// 换成给人看的那一句：原来那句进原话（已有原话 ⇒ 照旧）。
    pub fn retold(mut self, said: String) -> Said {
        let was = std::mem::replace(&mut self.said, said);
        if self.raw.is_none() && !was.trim().is_empty() {
            self.raw = Some(was);
        }
        self
    }

    /// 外层换一句（里层那一句给 `f` 当原因 / 对象用），码 · 原话 · `data` 原样跟着。
    pub fn wrap(self, f: impl FnOnce(&str) -> String) -> Said {
        Said {
            said: f(&self.said),
            ..self
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
            raw: (!raw.trim().is_empty()).then_some(raw),
            ..Said::from(said)
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/copy-core/said_tests.rs"]
mod tests;
