//! 帧面宿主：改账号库的那几条命令（`accounts-*`，本体 `accounts::manage::wire`）＋ 两步装配 ——
//! ① 建 API 号 ⇒ key 写进这台的 apikey 表（`accounts::upstream_select::file_face`，与 `apikey-key-set` 同一个写口）；
//!    删号 ⇒ 表里这个号那一行跟着清掉、回滚 ⇒ 放回去（那两步由执行器排进同一份备份，写的仍是表自己那一口）；
//! ② 账号表变了 ⇒ 每个号那一条 `<名>cc` 并进这台的别名文件（`assets::aliases`，POSIX 那一份；用户自己的别名一条不动）；
//! ③ 账号库改过之后 ⇒ 各号共用的用户级 MCP 同步一趟（`accounts::manage::mcp_share_exec`），并让监听器的名单跟上。
//! 各号共用的 MCP 那几条（`accounts-mcp-*`）也从这里进。
//! 两块互不认识，接在这里（`accounts/mod.rs` 头注：账号库与上游选择零引用）。

use crate::accounts::manage::{aliases as acct_aliases, mcp_share_exec, mcp_share_watch, wire};
use crate::accounts::upstream_select::file_face;
use crate::assets::aliases::{self, Alias};
use crate::assets::door::Door;
use crate::platform::shell::dialect::Shell;
use acct_core::wire::{AccountChange, AccountKind, AliasChange};
use copy_core::copy_text;
use serde_json::{json, Value};
use std::path::PathBuf;

/// 这台 key 表的几口，由门递进来（生产：`inbound.rs` 递 `file_face` 那几个；判据给落在临时目录上的那一份）。
pub(crate) struct KeyDoor<'a> {
    /// 写一个号的 key（`{configDir, key, baseUrl}`）。
    pub(crate) set: &'a dyn Fn(&Value) -> file_face::FileFaceAnswer,
    /// 摘掉一个号那一行（`{configDir}`）。
    pub(crate) drop: &'a dyn Fn(&Value) -> file_face::FileFaceAnswer,
    /// 从备份那一份放回一个号那一行（`{configDir, from}`）。
    pub(crate) restore: &'a dyn Fn(&Value) -> file_face::FileFaceAnswer,
    /// 那份表在哪。
    pub(crate) path: &'a dyn Fn() -> Result<PathBuf, String>,
}

/// 本族的应答：`data` 或 `(code, message)`。
pub(crate) type Answer = Result<Value, (&'static str, String)>;

fn to_value<T: serde::Serialize>(v: &T) -> Answer {
    serde_json::to_value(v).map_err(|e| ("io_failed", e.to_string()))
}

/// 帧面入口。key 表那几口由门递进来（[`KeyDoor`]）。
pub(crate) fn answer(d: &dyn Door, cmd: &str, args: &Value, keys: &KeyDoor) -> Answer {
    if let Some(req) = wire::parse_mcp(cmd, args) {
        let view = wire::run_mcp(d, &req?)?;
        return to_value(&view);
    }
    let req = wire::parse(cmd, args)?;
    let key = match &req {
        wire::Request::Verify => return to_value(&wire::run_verify(d)?),
        wire::Request::LoginCmd(a) => return to_value(&wire::run_login_cmd(d, a)?),
        // 预演不带 key（界面填表时每改一格问一次，明文不必跟着走）；真建号之前先判 key 与地址写不写得进去，
        // 不然号建好了、key 却落不下。
        wire::Request::Add(a) if a.kind == AccountKind::ApiKey && a.dry_run != Some(true) => {
            let k = a.key.clone().unwrap_or_default();
            file_face::check_inputs(&k, a.base_url.as_deref())?;
            Some((k, a.base_url.clone()))
        }
        _ => None,
    };
    // 删号 · 回滚才碰 key 表那一行：先读表里有哪几个号（读不了 ⇒ 删号那一趟不清它、说一句）。
    let mut unreadable = None;
    let table = match &req {
        wire::Request::Remove(_) | wire::Request::Rollback(_) => match (keys.path)() {
            Ok(p) => match file_face::account_ids_at(&p) {
                Ok(ids) => Some((p.display().to_string(), ids)),
                Err(e) => {
                    unreadable = Some(copy_text("beAcctFace.key.unreadable", &[("e", &e)]));
                    None
                }
            },
            Err(_) => None,
        },
        _ => None,
    };
    let drop = |dir: &str| {
        (keys.drop)(&json!({ "configDir": dir }))
            .map(|_| ())
            .map_err(|(_, m)| m)
    };
    let restore = |dir: &str, from: &str| {
        (keys.restore)(&json!({ "configDir": dir, "from": from }))
            .map(|_| ())
            .map_err(|(_, m)| m)
    };
    let kt = table.map(|(path, ids)| wire::KeyTable {
        path,
        ids,
        drop: &drop,
        restore: &restore,
    });
    let done = wire::run_change(d, &req, kt.as_ref())?;
    let mut change = done.change;
    if let (Some(note), wire::Request::Remove(_)) = (unreadable, &req) {
        change.notes.push(note);
    }
    if change.applied {
        if let (Some((k, base)), Some(acct)) = (key, change.account.clone()) {
            put_key(&mut change, keys.set, &acct.config_dir, &k, base.as_deref());
        }
    }
    if let Some(names) = done.accounts_after {
        change.aliases = Some(sync_aliases(d, &names));
    }
    if change.applied {
        sync_mcp(d, &mut change);
    }
    to_value(&change)
}

/// 账号库改过之后（建库 · 加号 · 删号 …）各号共用的用户级 MCP 跟着同步一趟，监听器的名单也跟上（新建的目录此前没被盯着）。
/// 同步没做成不挡这一趟改动，说一句。
fn sync_mcp(d: &dyn Door, change: &mut AccountChange) {
    match mcp_share_exec::sync(d) {
        Ok(v) => {
            if !v.changed.is_empty() {
                change.notes.push(copy_text(
                    "beAcctFace.mcp.synced",
                    &[("accounts", &v.changed.join(" · "))],
                ));
            }
            change.notes.extend(v.notes);
            if !v.conflicts.is_empty() {
                let names: Vec<&str> = v.conflicts.iter().map(|c| c.name.as_str()).collect();
                change.notes.push(copy_text(
                    "beAcctFace.mcp.conflicts",
                    &[("names", &names.join(" · "))],
                ));
            }
        }
        Err((_, why)) => change
            .notes
            .push(copy_text("beAcctFace.mcp.failed", &[("e", &why)])),
    }
    mcp_share_watch::kick();
}

fn put_key(
    change: &mut AccountChange,
    key_set: &dyn Fn(&Value) -> file_face::FileFaceAnswer,
    config_dir: &str,
    key: &str,
    base_url: Option<&str>,
) {
    match key_set(&json!({ "configDir": config_dir, "key": key, "baseUrl": base_url })) {
        Ok(v) => change.key_masked = v.get("masked").and_then(Value::as_str).map(str::to_string),
        Err((_, why)) => {
            change.key_problem = Some(copy_text("beAcctFace.key.notSaved", &[("e", &why)]))
        }
    }
}

/// 账号表 ⇒ 别名文件。文件里有认不出的行 ⇒ 不动它（重写会把那几行丢掉），说一句。
fn sync_aliases(d: &dyn Door, accounts: &[String]) -> AliasChange {
    let shell = Shell::Posix;
    let listing = match aliases::read_via(d, shell, None) {
        Ok(l) => l,
        Err(e) => {
            return AliasChange {
                path: String::new(),
                changed: false,
                names: Vec::new(),
                note: Some(copy_text("beAcctFace.aliases.readFailed", &[("e", &e)])),
            }
        }
    };
    let current: Vec<acct_aliases::Entry> = listing
        .aliases
        .iter()
        .map(|a| (a.name.clone(), a.args.clone()))
        .collect();
    let rec = acct_aliases::reconcile(&current, accounts);
    let mut notes: Vec<String> = rec
        .skipped
        .iter()
        .map(|(acc, name)| {
            copy_text(
                "beAcctFace.aliases.nameTaken",
                &[("account", acc), ("alias", name)],
            )
        })
        .collect();
    if !listing.unparsed.is_empty() {
        notes.push(copy_text(
            "beAcctFace.aliases.unparsed",
            &[
                ("path", &listing.alias_path),
                ("n", &listing.unparsed.len().to_string()),
            ],
        ));
        return AliasChange {
            path: listing.alias_path,
            changed: false,
            names: Vec::new(),
            note: Some(notes.join("\n")),
        };
    }
    let list: Vec<Alias> = rec
        .list
        .into_iter()
        .map(|(name, args)| Alias { name, args })
        .collect();
    match aliases::install_in(d, &list, None, shell) {
        Ok(r) => AliasChange {
            path: r.alias_path,
            changed: r.wrote_alias_file,
            names: rec.names,
            note: (!notes.is_empty()).then(|| notes.join("\n")),
        },
        Err(e) => {
            notes.push(copy_text("beAcctFace.aliases.writeFailed", &[("e", &e)]));
            AliasChange {
                path: listing.alias_path,
                changed: false,
                names: Vec::new(),
                note: Some(notes.join("\n")),
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/accounts_face_tests.rs"]
mod tests;
