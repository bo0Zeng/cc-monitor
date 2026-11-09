//! 账号清单成品（`accounts-list` 那一份）的夹具：照类型造。一个号的缺省是能起会话的订阅号（隔离 · 目录在 · 已就绪），账号目录 `/h/<名>`。
//! 另有盘上一份账号库的造法（清单 · 照跨三处金样那一格造号目录与凭据）。本包的判据与上层判据经 `test-support` 共用。

use super::{AccountRow, AccountsList, Badge, ListMeta, ScannedAccount};
use crate::common::cells::Words;
use acct_core::CREDENTIALS_NAME;
use std::path::{Path, PathBuf};

/// 判据用的一份空临时目录（`tag` 各条判据各取各的）。
pub fn fresh_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-acct-{tag}-{}", std::process::id()));
    if p.exists() {
        std::fs::remove_dir_all(&p).unwrap();
    }
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// 在账号库目录 `accts` 写一份清单（`accounts.json`）。
pub fn write_manifest(accts: &Path, body: &str) {
    std::fs::create_dir_all(accts).unwrap();
    std::fs::write(accts.join("accounts.json"), body).unwrap();
}

/// 跨三处金样（`tests/__fixtures__/account-default.golden.json`）：能不能选 · 默认号是谁。
pub fn account_default_golden() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../__fixtures__/account-default.golden.json"
    ))
    .unwrap()
}

/// 金样那一格 ⇒ 盘上一份账号库（号目录 · 凭据文件 · 清单），回（临时根，清单目录）。
/// 凭据在、目录不在的那一形（订阅号做不出来）写成 API 号（鉴权那一维恒就绪）。
pub fn golden_library(tag: &str, case: &serde_json::Value) -> (PathBuf, PathBuf) {
    let root = fresh_dir(tag);
    let accts = root.join("accts");
    let shared = root.join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    let mut rows = Vec::new();
    for a in case["accounts"].as_array().unwrap() {
        let name = a["name"].as_str().unwrap();
        let ready = a["authReady"].as_bool().unwrap();
        let exists = a["exists"].as_bool().unwrap();
        let mut row = serde_json::json!({
            "name": name,
            "isDefault": a["isDefault"],
            "mode": a["mode"],
        });
        if a["hasDir"].as_bool().unwrap() {
            let dir = accts.join(format!("acct-{name}"));
            if exists {
                std::fs::create_dir_all(&dir).unwrap();
                if ready {
                    std::fs::write(dir.join(CREDENTIALS_NAME), "{}").unwrap();
                }
            } else if ready {
                row["authKind"] = serde_json::json!("api-key");
            }
            row["configDir"] = serde_json::json!(dir.to_string_lossy());
        } else if ready {
            std::fs::write(shared.join(CREDENTIALS_NAME), "{}").unwrap();
        }
        rows.push(row);
    }
    write_manifest(
        &accts,
        &serde_json::json!({
            "version": 1,
            "sharedStore": shared.to_string_lossy(),
            "accounts": rows,
        })
        .to_string(),
    );
    (root, accts)
}

/// 一个能起会话的订阅号。
pub fn row(name: &str) -> AccountRow {
    row_with(name, |_| {})
}

/// 同 [`row`]，再按 `f` 改几格。`selectable` 照改后的那几格按成品的规则（`acct_core::account_selectable`）重算。
pub fn row_with(name: &str, f: impl FnOnce(&mut ScannedAccount)) -> AccountRow {
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
        color: Some(0),
    };
    f(&mut a);
    a.selectable =
        acct_core::account_selectable(&a.mode, a.auth_ready, a.exists, a.config_dir.is_some());
    let new_session = super::new_session_of(&a);
    AccountRow {
        plate: Words(a.name.clone()),
        kind_line: Words::default(),
        config_dir_shown: a.config_dir.clone(),
        scanned: a,
        key_masked: None,
        endpoint_host: None,
        badge: Badge {
            text: Words::default(),
            warn: false,
        },
        slots: Vec::new(),
        unselectable: None,
        login: false,
        new_session,
    }
}

/// 读得出的一份清单：默认号照成品的规则（`acct_core::effective_default`）从 `isDefault` 那一格算。
pub fn list(accounts: Vec<AccountRow>) -> AccountsList {
    let default = acct_core::effective_default(&accounts, |a| a.scanned.is_default)
        .map(|a| a.scanned.name.clone());
    AccountsList {
        meta: meta(true, default, None),
        accounts,
        notice: None,
    }
}

/// 读不出的那一份（`meta.enabled: false` ＋ 那一句）。
pub fn unreadable(error: &str) -> AccountsList {
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
    }
}
