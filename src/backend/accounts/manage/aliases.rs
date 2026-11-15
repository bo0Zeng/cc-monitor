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

/// 别名那一侧要一个号的两样：名字 · 它是哪一家的号（清单那一格原样；缺席 ⇒ 缺省那一家）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasOwner {
    pub name: String,
    pub agent: Option<String>,
}

impl AliasOwner {
    /// 清单里的一个号。
    pub fn of(a: &super::model::Account) -> AliasOwner {
        AliasOwner {
            name: a.name.clone(),
            agent: a.agent.clone(),
        }
    }
}

/// 一个号那一条别名叫什么：照那个号那一家（`agents::account_kind`）的 wrapper 起（[`alias_name_among`]）；认不出那一家 ⇒ `None`。
pub fn alias_name(owner: &AliasOwner, form: Launch) -> Option<String> {
    alias_name_among(
        crate::agents::REGISTRY,
        crate::agents::account_kind(owner.agent.as_deref())?,
        &owner.name,
        form,
    )
}

/// `kind` 那一家的号那一条别名叫什么：账号名去掉 shell 函数名里放不下的字符（今天只有 `-`）＋ 那一家的 wrapper 名
/// （`LaunchFace.launcher_alias`）＋ 那一形的后缀（[`suffix`]）；以数字打头 ⇒ 前面补 `_`。
/// 去完什么都不剩、或那一家没有 wrapper ⇒ `None`（不给它起别名）。
pub fn alias_name_among(
    registry: &[crate::agents::Adapter],
    kind: &str,
    account: &str,
    form: Launch,
) -> Option<String> {
    let cleaned: String = account
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    let wrapper = crate::agents::launch_face_among(registry, kind)?.launcher_alias?;
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
