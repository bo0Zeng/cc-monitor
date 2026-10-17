//! 策略文件：标题 ＝ 文件名；文件最开头连着的 `//` 行是说明；接着一段块注释是 JSON 头（格式 · `timeVarying` · 时区 · 设置 · 声明），
//! 后面是 Rhai 代码。文件仍是合法的 Rhai；存设置只重写头那一块，说明和代码一个字不动。代码怎么编、怎么跑在基础层（[`code`]）。

use crate::accounts::quota::rules::code::{self, fail, Code};
use crate::accounts::quota::strategy::{Fail, FailKind};
/// 头里能写成一句、也能分语言写的字（线上形住 `cmd-wire`）。
pub(crate) use cmd_wire::rotation::StrategyText as Text;
use serde::{Deserialize, Serialize};

/// 头那一段的开头与结尾。
const HEAD_OPEN: &str = "/*--- rotation\n";
const HEAD_CLOSE: &str = "\n---*/";

/// JSON 头。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Head {
    pub(crate) format: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) time_varying: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tz: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) tz_unknown: bool,
    /// 这份策略自己的设置（形状它自己定，后端原样交给它）。
    pub(crate) settings: serde_json::Value,
    /// 设置怎么画、怎么校验：界面照它画，后端照它校验形状，不认意思。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) form: Vec<super::form::Cell>,
}

/// 这一版的格式号。
pub(crate) const FORMAT: u32 = 5;

/// 一份读懂了的策略文件：说明 ＋ 头 ＋ 编好的代码。
pub(crate) struct File {
    /// 文件开头那段 `//` 注释：去掉行首的 `// `，一行一行用 `\n` 接起来，空行是分段。界面只显示它。
    pub(crate) about: String,
    pub(crate) head: Head,
    pub(crate) code: std::sync::Arc<Code>,
    /// 代码那一段原文，前面垫好空行（副本存的就是它）。
    pub(crate) code_text: String,
}

impl std::fmt::Debug for File {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("File")
            .field("about", &self.about)
            .field("head", &self.head)
            .finish_non_exhaustive()
    }
}

/// 拆成 说明那一段原文（含它后面的空行）· 头 JSON 原文 · 代码。说明只能是连着的 `//` 行，跟头之间只许空行。
fn split(source: &str) -> Result<(&str, &str, &str), Fail> {
    let mut at = 0;
    let mut line_no = 1;
    for l in source.split_inclusive('\n') {
        let s = l.trim_end_matches(['\n', '\r']);
        if s.starts_with("//") || s.trim().is_empty() {
            at += l.len();
            line_no += 1;
            continue;
        }
        break;
    }
    let rest = source[at..].strip_prefix(HEAD_OPEN).ok_or_else(|| {
        fail(
            FailKind::BadShape,
            Some(line_no),
            "after the leading `//` lines the file must go on with the `/*--- rotation` head",
        )
    })?;
    let end = rest.find(HEAD_CLOSE).ok_or_else(|| {
        fail(
            FailKind::BadShape,
            Some(line_no),
            "the head is not closed with `---*/`",
        )
    })?;
    Ok((&source[..at], &rest[..end], &rest[end + HEAD_CLOSE.len()..]))
}

/// 说明那一段原文 ⇒ 显示用的字：去行首 `// `（或 `//`），首尾的空行去掉，连着几个空行算一个。
fn about_of(lead: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for l in lead.lines() {
        let Some(c) = l.trim_end().strip_prefix("//") else {
            continue;
        };
        let c = c.strip_prefix(' ').unwrap_or(c);
        if c.trim().is_empty() && out.last().is_none_or(|x| x.is_empty()) {
            continue;
        }
        out.push(if c.trim().is_empty() { "" } else { c });
    }
    while out.last().is_some_and(|x| x.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// 这一份是不是第五版的策略文件（说明之后是那一段头）：不是的（第三版脚本）升级时挪开。
pub(crate) fn has_head(source: &str) -> bool {
    split(source).is_ok()
}

/// ★ 读一份策略文件：头照形状解、代码编一次、要有 `decide`。
pub(crate) fn parse(source: &str) -> Result<File, Fail> {
    let (lead, head, code) = split(source)?;
    let about = about_of(lead);
    let head: Head = serde_json::from_str(head).map_err(|e| {
        let base = lead.matches('\n').count() + 1;
        fail(FailKind::BadShape, Some(base + e.line()), e.to_string())
    })?;
    if head.format != FORMAT {
        return Err(fail(
            FailKind::BadShape,
            Some(2),
            format!("format {} is not {FORMAT}", head.format),
        ));
    }
    super::form::check_form(&head.form).map_err(|m| fail(FailKind::BadShape, Some(2), m))?;
    if let Some(b) = super::form::check(&head.form, &head.settings).first() {
        if !head.form.is_empty() {
            return Err(fail(
                FailKind::BadShape,
                Some(2),
                format!("settings.{} does not fit the form ({})", b.cell, b.code),
            ));
        }
    }
    // 代码的行号接在头后面（报第几行时与文件对得上）。
    let lead = source.len() - code.len();
    let padded = format!(
        "{}{code}",
        "\n".repeat(source[..lead].matches('\n').count())
    );
    let code = std::sync::Arc::new(code::compile(&padded)?);
    Ok(File {
        about,
        head,
        code,
        code_text: padded,
    })
}

/// ★ 换掉头那一块；说明和代码原样接回去，一个字节不动（存设置那一下）。
pub(crate) fn with_head(source: &str, head: &Head) -> Result<String, Fail> {
    let (lead, _, code) = split(source)?;
    let json = serde_json::to_string_pretty(head)
        .map_err(|e| fail(FailKind::BadShape, None, e.to_string()))?;
    Ok(format!("{lead}{HEAD_OPEN}{json}{HEAD_CLOSE}{code}"))
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/setup/script_tests.rs"]
mod tests;
