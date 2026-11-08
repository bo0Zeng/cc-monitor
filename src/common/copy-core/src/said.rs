//! 一次失败**码 ＋ 给人看的那一句 ＋ 下层原话 ＋ 按码定形的 `data`**（除那一句外都可缺）。句子只带原因词（[`crate::io_reason`] 那一类）；原话不上句子，
//! 走到应答 / 收场帧那一层进「复制详情」（[`crate::detail`]）。
//!
//! 纯数据（后端 · 共享契约 crate 都用这一份）：记日志那一口（成不了应答、只成别处一句时把原话记一行）要日志库，
//! 住后端 `common/said.rs`，不在这里。

// 码缺着的那一次失败投成应答时补的码：线上那一格的值，住两端的契约（`cmd_wire::failed`）。
use cmd_wire::failed::FALLBACK_CODE;

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

// ── 契约错那一串（调用方是我们自己的代码、请求格式不对：线上 `bad_args` / `bad_request`）────────────────────────────
// 那一句只有一句（`beContract.malformed.say`）；`detail` 是给报 bug 的人看的英文诊断，不进表、不上句子，
// 隔一个 [`DIAG_SEP`] 跟在那一句后面走，由失败信封那一处（[`parts`]）拆进复制详情的「原话」。
// 记那一行 warn 日志的 `malformed` 在用它的那一层（拨号核心 `ssh_dial::malformed` · 后端 `common/contract.rs`）：本 crate 不链日志库。

/// 句子与跟在它后面的契约诊断之间那一个字符。
const DIAG_SEP: char = '\u{1f}';

/// 契约错那一串：那一句 ＋ [`DIAG_SEP`] ＋ 诊断。
pub fn malformed_said(detail: &str) -> String {
    format!(
        "{}{DIAG_SEP}{detail}",
        crate::copy_text("beContract.malformed.say", &[])
    )
}

/// 一条失败的那一句 ⇒ (给人看的那一句, 下层原话)：句子后面跟着契约诊断 ⇒ 拆出来、放在原话最前面；没有诊断 ⇒ 原样。
pub fn parts(message: &str, raw: Option<&str>) -> (String, Option<String>) {
    match message.split_once(DIAG_SEP) {
        Some((said, diag)) => (
            said.to_string(),
            Some(match raw {
                Some(r) => format!("{diag}\n{r}"),
                None => diag.to_string(),
            }),
        ),
        None => (message.to_string(), raw.map(str::to_string)),
    }
}

/// [`parts`] 的反方向：那一句 ＋ 原话 ⇒ 一串（原话隔一个 [`DIAG_SEP`] 跟在后面；没有原话 ⇒ 那一句原样）。
pub fn joined(said: String, raw: Option<String>) -> String {
    match raw {
        Some(r) => format!("{said}{DIAG_SEP}{r}"),
        None => said,
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/copy-core/said_tests.rs"]
mod tests;
