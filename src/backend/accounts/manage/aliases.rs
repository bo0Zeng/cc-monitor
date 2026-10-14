//! **账号那两条别名叫什么**（纯）：每个号 `<名>cc` 与 `<名>cct`。按别名清单算加 / 删 / 认形住 `assets/aliases/accounts.rs`（帧面宿主建号删号时调它）。

/// 一个号那一条别名叫什么：账号名去掉 shell 函数名里放不下的字符（今天只有 `-`）＋ 账号库那一家的 wrapper 名
/// （`agents::wrapper_alias`，Claude：`cc`；`tmux` ⇒ 再加 `t`）；以数字打头 ⇒ 前面补 `_`。
/// 去完什么都不剩、或那一家没有 wrapper ⇒ `None`（不给它起别名）。
pub(crate) fn alias_name(account: &str, tmux: bool) -> Option<String> {
    let cleaned: String = account
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    let wrapper = crate::agents::sole_kind(|a| a.accounts.is_some())
        .and_then(crate::agents::wrapper_alias)?;
    let name = format!("{cleaned}{wrapper}{}", if tmux { "t" } else { "" });
    Some(if name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{name}")
    } else {
        name
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/aliases_tests.rs"]
mod tests;
