//! 编辑器那一份（`src/shared/rotation/editor.rhai`，随后端带、只读）：界面上「新建策略」照它存成一份策略文件（换上号列表与时区），
//! 设置页照它的声明画格。不是随包策略：不进策略目录、不当任何会话的默认。代码或声明跟它此刻的不一样的那几份，界面标「手写」。

use super::script;
use serde_json::Value;

/// 编辑器那一份的原文。
pub(crate) const SOURCE: &str = include_str!("../../../../shared/rotation/editor.rhai");

/// 读懂的那一份（随包的原文，判据钉着它读得懂；读不懂 ⇒ 缺）。
fn parsed() -> Option<&'static script::File> {
    static F: std::sync::OnceLock<Option<script::File>> = std::sync::OnceLock::new();
    F.get_or_init(|| script::parse(SOURCE).ok()).as_ref()
}

/// ★ 照编辑器那一份新建：号列表 ＝ `accounts`（照这个先后；空 ⇒ 开会话的号那一行）· 时区 ＝ `tz`，别的照声明默认。
pub(crate) fn fresh(accounts: &[String], tz: Option<String>) -> String {
    let Some(e) = parsed() else {
        return SOURCE.to_string();
    };
    let mut head = e.head.clone();
    let rows: Vec<Value> = if accounts.is_empty() {
        vec![serde_json::json!({"start": true})]
    } else {
        accounts
            .iter()
            .map(|a| serde_json::json!({"id": a}))
            .collect()
    };
    head.settings[super::form::ACCOUNTS] = Value::Array(rows);
    head.tz = tz;
    script::with_head(SOURCE, &head).unwrap_or_else(|_| SOURCE.to_string())
}

/// ★ 这一份是不是照编辑器那一份存出来的：代码与声明跟它此刻的一字不差（设置 · 时区 · 说明怎么改都还算）。
pub(crate) fn made_by(f: &script::File) -> bool {
    parsed().is_some_and(|e| f.head.form == e.head.form && f.code_text.trim() == e.code_text.trim())
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/setup/editor_tests.rs"]
mod tests;
