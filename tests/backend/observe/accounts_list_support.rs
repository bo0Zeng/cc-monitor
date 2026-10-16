//! 账号清单成品（`accounts-list` 那一份）的夹具：照类型造。一个号的缺省是能起会话的订阅号（隔离 · 目录在 · 已就绪），账号目录 `/h/<名>`。

use super::{AccountRow, AccountsList, Badge, ListMeta, ScannedAccount};
use crate::common::cells::Words;

/// 一个能起会话的订阅号。
pub(crate) fn row(name: &str) -> AccountRow {
    row_with(name, |_| {})
}

/// 同 [`row`]，再按 `f` 改几格。`selectable` 照改后的那几格按成品的规则（`acct_core::account_selectable`）重算。
pub(crate) fn row_with(name: &str, f: impl FnOnce(&mut ScannedAccount)) -> AccountRow {
    let mut a = ScannedAccount {
        name: name.to_string(),
        label: name.to_string(),
        email: format!("{name}@example.test"),
        config_dir: Some(format!("/h/{name}")),
        is_default: false,
        mode: "isolated".to_string(),
        exists: true,
        logged_in: true,
        auth_kind: acct_core::AUTH_KIND_SUBSCRIPTION.to_string(),
        auth_ready: true,
        selectable: true,
        color: 0,
    };
    f(&mut a);
    a.selectable =
        acct_core::account_selectable(&a.mode, a.auth_ready, a.exists, a.config_dir.is_some());
    AccountRow {
        plate: Words(a.name.clone()),
        scanned: a,
        key_masked: None,
        base_url: None,
        badge: Badge {
            text: Words::default(),
            warn: false,
            title: Words::default(),
        },
        quota: None,
        unselectable: None,
        login: false,
    }
}

/// 读得出的一份清单：默认号照成品的规则（`acct_core::effective_default`）从 `isDefault` 那一格算。
pub(crate) fn list(accounts: Vec<AccountRow>) -> AccountsList {
    let default = acct_core::effective_default(&accounts, |a| a.scanned.is_default)
        .map(|a| a.scanned.name.clone());
    AccountsList {
        meta: meta(true, default, None),
        accounts,
        notice: None,
    }
}

/// 读不出的那一份（`meta.enabled: false` ＋ 那一句）。
pub(crate) fn unreadable(error: &str) -> AccountsList {
    AccountsList {
        meta: meta(false, None, Some(error.to_string())),
        accounts: Vec::new(),
        notice: None,
    }
}

fn meta(enabled: bool, effective_default: Option<String>, error: Option<String>) -> ListMeta {
    ListMeta {
        enabled,
        accts_dir: "/h/accts".to_string(),
        manifest_path: "/h/accts/accounts.json".to_string(),
        updated_at: None,
        shared_store: None,
        count: 0,
        error,
        unsupported: None,
        next_default: None,
        effective_default,
        home: None,
    }
}
