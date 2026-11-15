//! **号名与语义位名的字**（唯一一处）：起会话时没说是哪个号的那一个（`_`）· 账号 0（不设配置目录的那一份登录，叫那一家默认家目录的短形）叫什么、
//! `5h` / `7d` 两个语义位叫什么。`_` 只说「没表态」：落在哪一份登录说不准（建了账号库的机器上可能是 shell rc 导出的那个配置根），
//! 所以它不叫 `~/.claude`，叫「未指定账号」（与额度行那一格同一条键）。
//!
//! 后端自己拼句子时用 [`account_text_of`]（照号那一家问）/ [`slot_text`]；出口画号名照 `accounts-list` 每号那一行的 `label`（[`account_text_among`] 写），
//! 画位名照 `quota-read` 成品上的 `names`（[`names`]：`{slots: {码: 字}}`）。出口不认 `_`、不认 `5h` / `7d`。

use crate::common::cells::Words;
use cmd_wire::quota::{SLOT_5H, SLOT_7D};
use copy_core::copy_text;

/// 起会话时没说是哪个号 ⇒ 记成这个（是哪一份登录说不准；不等于账号 0）。
pub(crate) const HOME_ACCOUNT: &str = "_";

/// 一个号能不能当换号 · 加号的候选（唯一一处）：`_`（起会话时没说是哪个号）不是 —— 它不顶任何一份登录，放上去只能回「需登录」。
/// `quota-read` 的 `candidates` 与会话那一份的候选行都调它，界面只照抄。
pub fn is_candidate(account: &str) -> bool {
    account != HOME_ACCOUNT
}

/// `agent`（线上的 kind 或路由名都认）那一家的一个号在人眼里叫什么：`_` ⇒「未指定账号」·
/// 账号 0 ⇒ 那一家默认家目录的短形（[`bare_account_text_among`]）；其余照原名。
pub fn account_text_of(agent: &str, name: &str) -> String {
    account_text_among(crate::agents::REGISTRY, Some(agent), name)
}

/// [`account_text_of`] 照给定注册表（账号库扫清单照它手里那张表问）。认不出号是哪一家的（`agent` 缺）：
/// 账号 0 的号名要问那一家的家目录 ⇒ 照实说认不出是哪一家的（[`bare_account_text_among`] 判）；别的号名不看家，照常写。
pub(crate) fn account_text_among(
    registry: &[crate::agents::Adapter],
    agent: Option<&str>,
    name: &str,
) -> String {
    if name == HOME_ACCOUNT {
        copy_text("acct.tag.home", &[])
    } else if name == acct_core::BARE_ACCOUNT {
        bare_account_text_among(
            registry,
            agent.map(|a| crate::agents::kind_of_route_among(registry, a)),
            crate::platform::paths::home_dir().as_deref(),
        )
    } else {
        name.to_string()
    }
}

/// 账号 0（不设配置目录的那一份登录）的名字：`kind` 那一家不给配置目录时的家目录（[`crate::agents::LocalFace::home_at`]），
/// 在这台家目录 `user_home` 底下 ⇒ 写成 `~/…`。不知道是哪一家（`kind` 缺）· 认不出 · 那一家没有本机布局 ⇒
/// 照实说认不出是哪一家的（只这一处判）。
pub(crate) fn bare_account_text_among(
    registry: &[crate::agents::Adapter],
    kind: Option<&str>,
    user_home: Option<&std::path::Path>,
) -> String {
    kind.and_then(|k| crate::agents::local_face_among(registry, k))
        .map_or_else(
            || copy_text("rsAgents.family.unknown", &[]),
            |f| copy_core::home_short(&(f.home_at)(None, false).to_string_lossy(), user_home),
        )
}

/// 号牌上那一两个字（号色那一格里画的）：字母数字开头 ⇒ 前两个字；别的 ⇒ 第一个字；空名 ⇒ `?`。
pub fn plate_text(name: &str) -> String {
    let mut cs = name.chars();
    match cs.next() {
        None => "?".to_string(),
        Some(c) if c.is_ascii_alphanumeric() => name.chars().take(2).collect(),
        Some(c) => c.to_string(),
    }
}

/// ★ 号的类型字（订阅 · 按量）：唯一一处；额度卡首行（`faces/quota_rows.rs` 的 `head`）与账号行第二行（[`kind_line`]）都照它。
pub fn kind_text(kind: cmd_wire::quota::Kind) -> String {
    if kind == cmd_wire::quota::Kind::Api {
        copy_text("acct.kind.api", &[])
    } else {
        copy_text("acct.kind.sub", &[])
    }
}

/// ★ 账号行第二行那一句：类型字 · 这个号的那一格（订阅号：没登录 ⇒ 需登录、有邮箱 ⇒ 邮箱；按量号：配了端点 ⇒ 端点的主机，
/// 即 [`endpoint_host`] 写好的那一格）；那一格没有 ⇒ 只有类型字。`accounts-list` 每个号的 `kindLine`，两端照摆。
pub fn kind_line(
    kind: cmd_wire::quota::Kind,
    logged_in: bool,
    email: &str,
    endpoint_host: Option<&Words>,
) -> Words {
    let detail = match kind {
        cmd_wire::quota::Kind::Api => endpoint_host.map(|h| h.0.clone()),
        cmd_wire::quota::Kind::Sub if !logged_in => Some(copy_text("acct.tag.login", &[])),
        cmd_wire::quota::Kind::Sub => (!email.is_empty()).then(|| email.to_string()),
    };
    let word = kind_text(kind);
    Words(match detail {
        Some(d) => format!("{word}{}{d}", copy_text("kit.text.sep", &[])),
        None => word,
    })
}

/// ★ 端点的主机（带端口），唯一一处：去掉 `scheme://` 与路径 · 查询 · 片段；认不出 scheme ⇒ 原样。
/// `accounts-list` 每个号的 `endpointHost` 与 [`kind_line`] 里的主机都是它。
pub fn endpoint_host(url: &str) -> Words {
    Words(
        url.split_once("://")
            .map_or(url, |(_, rest)| {
                rest.split(['/', '?', '#']).next().unwrap_or(rest)
            })
            .to_string(),
    )
}

/// 一个语义位在人眼里叫什么：`5h` · `7d` 各一个字；认不出的照原样。
pub fn slot_text(w: &str) -> String {
    match w {
        SLOT_5H => copy_text("acct.slot.fiveHour", &[]),
        SLOT_7D => copy_text("acct.slot.sevenDay", &[]),
        other => other.to_string(),
    }
}

/// 成品上的 `names` 那一格（线上形住 `cmd-wire`）。
pub(crate) use cmd_wire::quota::Names;

/// 唯一一处造 [`Names`]。
pub fn names() -> Names {
    Names {
        slots: [SLOT_5H, SLOT_7D]
            .iter()
            .map(|w| (w.to_string(), Words(slot_text(w))))
            .collect(),
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/name_words_tests.rs"]
mod tests;
