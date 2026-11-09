//! **账号那几条别名叫什么**（纯）：每个号 `<名>cc` · `<名>cct` · `<名>cch`。按别名清单算加 / 删 / 认形住 `assets/aliases/accounts.rs`（帧面宿主建号删号时调它）。

use cmd_wire::session::Launch;

/// 一条别名起在哪种终端里的三形，照别名清单里排的先后：当前终端（直路）· tmux · 托管。
pub const IN_ORDER: [Launch; 3] = [Launch::Direct, Launch::Tmux, Launch::Managed];

/// 接在 wrapper 名后面的那一段（`cc` · `cct` · `cch`）。
pub const fn suffix(form: Launch) -> &'static str {
    match form {
        Launch::Direct => "",
        Launch::Tmux => "t",
        Launch::Managed => "h",
    }
}

/// 一个号那一条别名叫什么：账号名去掉 shell 函数名里放不下的字符（今天只有 `-`）＋ 账号库那一家的 wrapper 名
/// （`agents::wrapper_alias`，Claude：`cc`）＋ 那一形的后缀（[`suffix`]）；以数字打头 ⇒ 前面补 `_`。
/// 去完什么都不剩、或那一家没有 wrapper ⇒ `None`（不给它起别名）。
pub fn alias_name(account: &str, form: Launch) -> Option<String> {
    let cleaned: String = account
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    let wrapper = crate::agents::sole_kind(|a| a.accounts.is_some())
        .and_then(crate::agents::wrapper_alias)?;
    let name = format!("{cleaned}{wrapper}{}", suffix(form));
    Some(if name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{name}")
    } else {
        name
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/aliases_tests.rs"]
mod tests;
