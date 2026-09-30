//! 帧面宿主：改账号库的那几条命令（`accounts-*`，本体 `accounts::manage::wire`）＋ 两步装配 ——
//! ① 建 API 号 ⇒ key 写进这台的 apikey 表（`accounts::upstream_select::file_face`，与 `apikey-key-set` 同一个写口）；
//! ② 账号表变了 ⇒ 每个号那一条 `<名>cc` 并进这台的别名文件（`assets::aliases`，POSIX 那一份；用户自己的别名一条不动）。
//! 两块互不认识，接在这里（`accounts/mod.rs` 头注：账号库与上游选择零引用）。

use crate::accounts::manage::{aliases as acct_aliases, wire};
use crate::accounts::upstream_select::file_face;
use crate::assets::aliases::{self, Alias};
use crate::assets::door::Door;
use crate::platform::shell::dialect::Shell;
use acct_core::wire::{AccountChange, AccountKind, AliasChange};
use copy_core::copy_text;
use serde_json::{json, Value};

/// 本族的应答：`data` 或 `(code, message)`。
pub(crate) type Answer = Result<Value, (&'static str, String)>;

fn to_value<T: serde::Serialize>(v: &T) -> Answer {
    serde_json::to_value(v).map_err(|e| ("io_failed", e.to_string()))
}

/// 帧面入口。写 key 那一口由门递进来（生产：`inbound.rs` 递 `file_face::answer_set`；判据给落在临时目录上的那一份）。
pub(crate) fn answer(
    d: &dyn Door,
    cmd: &str,
    args: &Value,
    key_set: &dyn Fn(&Value) -> file_face::FileFaceAnswer,
) -> Answer {
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
    let done = wire::run_change(d, &req)?;
    let mut change = done.change;
    if change.applied {
        if let (Some((k, base)), Some(acct)) = (key, change.account.clone()) {
            put_key(&mut change, key_set, &acct.config_dir, &k, base.as_deref());
        }
    }
    if let Some(names) = done.accounts_after {
        change.aliases = Some(sync_aliases(d, &names));
    }
    to_value(&change)
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
