//! **账号之间同步用户级 MCP 的那一半 IO**：读共享集合与各号此刻的样子 → 交 [`super::mcp_share`] 算 → 按计划落盘。
//!
//! - 整趟持账号库那把锁（与改账号库那几条命令互斥，监听器那一路与帧命令那一路也互斥）。
//! - 各号的配置文件只换顶层那一个键（[`super::json_key`]）：先把原文放进 `~/.cc-monitor/backups/accounts-mcp/<号>.claude.json`
//!   （每个号一份，下一次改写之前换成新的），再经文件管理面 `files-put` 写，`expect` = 读到的那一份（CAS）。
//!   被抢先改了 ⇒ 这个号这一趟不写、底不动；抢先的那一下自己会再触发一次同步。
//! - 共享集合那份文件与备份都 0600（里面有 MCP 的密钥）：新建时先落一份空的、改权限、再写内容。
//! - 说出去的每一句只有号名、名字与路径，不带任何定义里的值。

use super::json_key;
use super::layout;
use super::mcp_share::{self, Seen, Servers, Store};
use super::model::Manifest;
use super::scan::{item_at, join};
use crate::assets::door::{self, Door, Refused};
use crate::platform::acct_view::Item;
use acct_core::wire::{AccountMcpChoice, AccountMcpConflict, AccountMcpView};
use copy_core::copy_text;
use serde_json::Value;
use std::path::Path;

type Refusal = crate::stream::inbound::spec::Fail;

/// 清单 · 共享集合两份小文件的读取上限。
const MAX_SMALL_BYTES: u64 = 8 * 1024 * 1024;
/// 改写某个号的配置文件之前那份原文放在备份目录下的哪一层（每个号一份 `<号><配置文件名>`）。
const BACKUP_SUBDIR: &str = "accounts-mcp";
/// 各号配置文件的读取上限（它被项目历史与 MCP 配置撑大，同适配层读它时的那个量级）。
pub(crate) const MAX_CONFIG_BYTES: u64 = 32 * 1024 * 1024;

/// 一个号读出来的样子 ＋ 原文（落盘时当 `expect`；`None` = 文件不在）。
struct Account {
    seen: Seen,
    path: String,
    raw: Option<String>,
}

/// 这台此刻的样子（锁内读）。
struct Here {
    home: String,
    key: &'static str,
    accounts: Vec<Account>,
    store: Store,
    store_raw: Option<String>,
    notes: Vec<String>,
}

pub(crate) fn read_text(p: &str, cap: u64) -> Result<Option<String>, crate::common::said::Said> {
    match item_at(p) {
        Item::Absent => Ok(None),
        Item::File { .. } => {
            let b = crate::common::fs::read_regular_capped(Path::new(p), cap)?;
            String::from_utf8(b)
                .map(Some)
                .map_err(|_| copy_text("beAcctMcpShare.read.notText", &[]).into())
        }
        _ => Err(copy_text("beAcctMcpShare.read.notFile", &[]).into()),
    }
}

/// 这台的账号库清单里有哪几个号（`None` = 这台没有账号库 / 做不了多账号）。
pub(crate) fn accounts_in(home: &str) -> Result<Option<Vec<(String, String)>>, Refusal> {
    if layout::face().is_none() || crate::platform::acct_view::multi_account_supported().is_err() {
        return Ok(None);
    }
    let mpath = join(&super::scan::accts_root(home), super::scan::MANIFEST_FILE);
    // 那一句只带路径与原因词；原话进复制详情。
    let say = |w: &str| {
        copy_text(
            "beAcctScan.manifest.unreadable",
            &[("path", &mpath), ("why", w)],
        )
    };
    let text = read_text(&mpath, MAX_SMALL_BYTES)
        .map_err(|e| Refusal::from(("io_failed", e.wrap(say))))?;
    let Some(text) = text else {
        return Ok(None);
    };
    let m = Manifest::parse(&text).map_err(|e| ("refused", e))?;
    Ok(Some(
        m.managed()
            .map(|a| (a.name.clone(), a.config_dir.clone()))
            .collect(),
    ))
}

/// 这台账号库里各号共用的用户级 MCP 住哪份文件；没有账号库 ⇒ `None`（用户级 MCP 就是 agent 自己那一份）。
pub(crate) fn store_file_in(home: &str) -> Option<String> {
    matches!(accounts_in(home), Ok(Some(_)))
        .then(|| door::join_under(home, relay_route_core::ACCOUNTS_MCP_REL))
}

/// 这台的用户级 MCP 住哪份文件：有账号库 ⇒ 各号共用的那一份（扩展页读它）；没有 ⇒ agent 自己那一份。
pub(crate) fn user_mcp_file() -> Option<std::path::PathBuf> {
    let home = crate::platform::paths::home_dir()?;
    store_file_in(&home.display().to_string())
        .map(std::path::PathBuf::from)
        // 今天只管 Claude 的用户级 MCP 文件（唯一声明了资产面的那一家）。Codex 的 MCP 来了改这里。
        .or_else(|| crate::assets::asset_kind().and_then(crate::agents::user_mcp_file))
}

/// 一个号的配置文件 ⇒ 服务器表（文件不在 ⇒ 空表）。
fn servers_in(raw: Option<&str>, key: &str) -> Result<Servers, String> {
    let Some(t) = raw else {
        return Ok(Servers::new());
    };
    let v: Value = serde_json::from_str(t.trim_start_matches('\u{feff}')).map_err(|e| {
        copy_text(
            "beAcctMcpShare.read.badJson",
            &[("line", &e.line().to_string())],
        )
    })?;
    match v.get(key) {
        _ if !v.is_object() => Err(copy_text("beAcctMcpShare.read.notObject", &[])),
        None => Ok(Servers::new()),
        Some(Value::Object(m)) => Ok(m.clone()),
        Some(_) => Err(copy_text("beAcctMcpShare.read.serversNotObject", &[])),
    }
}

fn load(home: &str, list: Vec<(String, String)>) -> Result<Here, Refusal> {
    let key = layout::user_mcp_key();
    let file = layout::identity_config_file();
    let spath = door::join_under(home, relay_route_core::ACCOUNTS_MCP_REL);
    // 那一句只带路径与原因词；原话进复制详情。
    let say = |w: &str| {
        copy_text(
            "beAcctMcpShare.store.unreadable",
            &[("path", &spath), ("why", w)],
        )
    };
    let store_raw = read_text(&spath, MAX_SMALL_BYTES)
        .map_err(|e| Refusal::from(("io_failed", e.wrap(say))))?;
    let store = match &store_raw {
        Some(t) => Store::parse(t, key).map_err(|e| {
            (
                "refused",
                copy_text(
                    "beAcctMcpShare.store.cannotUse",
                    &[("path", &spath), ("why", &e)],
                ),
            )
        })?,
        None => Store::default(),
    };
    let mut notes = Vec::new();
    let mut accounts = Vec::new();
    for (name, dir) in list {
        let path = join(&dir, file);
        let got = read_text(&path, MAX_CONFIG_BYTES)
            .and_then(|raw| Ok((servers_in(raw.as_deref(), key)?, raw)));
        let (raw, now) = match got {
            Ok((s, raw)) => (raw, Some(s)),
            Err(e) => {
                let why = e.logged();
                tracing::warn!(
                    "账号之间同步 MCP：{name} 号的配置 {path} 读不出来，这一次跳过它：{why}"
                );
                notes.push(copy_text(
                    "beAcctMcpShare.note.unreadable",
                    &[("account", &name), ("path", &path), ("why", &e.said)],
                ));
                (None, None)
            }
        };
        accounts.push(Account {
            seen: Seen { name, dir, now },
            path,
            raw,
        });
    }
    Ok(Here {
        home: home.to_string(),
        key,
        accounts,
        store,
        store_raw,
        notes,
    })
}

/// 写一份只该自己读的文件：已在 ⇒ 带 `expect` 照常写（沿用原权限位）；不在 ⇒ 先落一份空的、改成 0600、再写内容。
pub(crate) fn put_private(
    d: &dyn Door,
    home: &str,
    rel: &str,
    text: &str,
    expect: Option<&str>,
) -> Result<(), Refused> {
    if expect.is_some() {
        return door::put(d, home, rel, text, expect, false, false).map(|_| ());
    }
    door::put(d, home, rel, "", None, false, true)?;
    door::chmod(d, home, rel, 0o600).map_err(|said| Refused::Peer {
        code: "io_failed".to_string(),
        said,
    })?;
    door::put(d, home, rel, text, Some(""), false, false).map(|_| ())
}

/// 改写之前把原文放进这个号那一份备份。
fn backup(d: &dyn Door, home: &str, name: &str, raw: &str) -> Result<(), String> {
    let rel = format!(
        "{}/{BACKUP_SUBDIR}/{name}{}",
        relay_route_core::EXT_BACKUPS_DIR_REL,
        layout::identity_config_file()
    );
    let abs = door::join_under(home, &rel);
    let before = read_text(&abs, MAX_CONFIG_BYTES)
        .map_err(|e| e.logged())
        .inspect_err(|e| {
            tracing::warn!(
                "账号之间同步 MCP：{name} 号上一份备份 {abs} 读不出来，这一次跳过它：{e}"
            );
        })?;
    put_private(d, home, &rel, raw, before.as_deref()).map_err(Refused::said)
}

/// 一个号的服务器表改成 `want`：先备份，再 CAS 写。回 `Ok(true)` = 落下了；`Ok(false)` = 被抢先改了（这一趟不写）。
fn write_account(d: &dyn Door, h: &Here, a: &Account, want: &Servers) -> Result<bool, String> {
    let rel = door::rel_under(&h.home, &a.path)?;
    let value = Value::Object(want.clone());
    let text = match &a.raw {
        Some(t) => json_key::set_top_key(t, h.key, &value)?,
        None => json_key::fresh_with(h.key, &value)?,
    };
    if let Some(t) = &a.raw {
        backup(d, &h.home, &a.seen.name, t)
            .map_err(|e| copy_text("beAcctMcpShare.note.backupFailed", &[("why", &e)]))?;
    }
    match put_private(d, &h.home, &rel, &text, a.raw.as_deref()) {
        Ok(()) => Ok(true),
        Err(Refused::Stale(_)) => Ok(false),
        Err(e) => Err(e.said()),
    }
}

fn view_of(store: &Store, conflicts: &[mcp_share::Conflict]) -> AccountMcpView {
    AccountMcpView {
        enabled: true,
        sync: !store.paused,
        servers: store.servers.keys().cloned().collect(),
        conflicts: conflicts
            .iter()
            .map(|c| AccountMcpConflict {
                name: c.name.clone(),
                choices: c
                    .choices
                    .iter()
                    .map(|x| AccountMcpChoice {
                        from: x.from.clone(),
                        holders: x.holders.clone(),
                        gone: x.gone,
                    })
                    .collect(),
            })
            .collect(),
        changed: Vec::new(),
        notes: Vec::new(),
    }
}

/// 账号库在就拿它那把锁（与改账号库那几条命令同一把：锁的是账号库目录本身）。
/// 不借命令那一层的那一份：那一层还认得 `ccm` 的命令行，扩展页读共享集合时会被连带引用进来。
pub(crate) fn lock(home: &str) -> Result<Option<crate::platform::lock::DirLock>, Refusal> {
    let accts = super::scan::accts_root(home);
    if item_at(&accts).exists() {
        crate::platform::lock::hold(Path::new(&accts))
            .map(Some)
            .map_err(|e| Refusal::from(("io_failed", crate::common::said::Said::from(e))))
    } else {
        Ok(None)
    }
}

/// 这一趟是谁要的：文件事件 / 加号之后的那一趟（[`Why::Auto`]），还是用户在 cc-monitor 里点的（[`Why::Asked`]）。
/// 停着同步时：前一种什么都不写、照实答此刻的样子；后一种拒（要先开回来，否则「从所有号删」就成了同步）。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Why {
    Auto,
    Asked,
}

/// 停着同步时那一份此刻的样子（不对照、不写；冲突不算 —— 各号各管各的）。
fn paused_view(h: &Here) -> AccountMcpView {
    let mut view = view_of(&h.store, &[]);
    view.notes = h.notes.clone();
    view
}

/// 一趟：锁 → 读 → `decide`（cc-monitor 里定的那一下；同步那一趟原样交回）→ 对照 → 落盘 → 写回共享集合。
fn run(
    d: &dyn Door,
    why: Why,
    decide: &dyn Fn(&Store, &[Seen]) -> Result<Store, Refusal>,
) -> Result<AccountMcpView, Refusal> {
    let home = door::home(d).map_err(|e| ("io_failed", e))?;
    let _held = lock(&home)?;
    let Some(list) = accounts_in(&home)? else {
        return Ok(AccountMcpView::default());
    };
    let h = load(&home, list)?;
    if h.store.paused {
        return match why {
            Why::Auto => Ok(paused_view(&h)),
            Why::Asked => Err(Refusal::from((
                "refused",
                copy_text("beAcctMcpShare.sync.paused", &[]),
            ))),
        };
    }
    let seen: Vec<Seen> = h.accounts.iter().map(|a| a.seen.clone()).collect();
    let start = decide(&h.store, &seen)?;
    let plan = mcp_share::plan(&start, &seen);
    let mut store = plan.store.clone();
    let mut notes = h.notes.clone();
    let mut changed = Vec::new();
    for (dir, want) in &plan.writes {
        let Some(a) = h.accounts.iter().find(|a| a.seen.dir == *dir) else {
            continue;
        };
        let landed = write_account(d, &h, a, want);
        match landed {
            Ok(true) => changed.push(a.seen.name.clone()),
            other => {
                // 没落下 ⇒ 这个号的底换回这一趟之前的（cc-monitor 里定的那一下照样算数：底取 `start` 里的）。
                match start.base.get(dir) {
                    Some(b) => store.base.insert(dir.clone(), b.clone()),
                    None => store.base.remove(dir),
                };
                notes.push(match other {
                    Err(e) => copy_text(
                        "beAcctMcpShare.note.writeFailed",
                        &[("account", &a.seen.name), ("why", &e)],
                    ),
                    _ => copy_text("beAcctMcpShare.note.stale", &[("account", &a.seen.name)]),
                });
            }
        }
    }
    let text = store.render(h.key);
    if h.store_raw.as_deref() != Some(text.as_str()) {
        put_private(
            d,
            &home,
            relay_route_core::ACCOUNTS_MCP_REL,
            &text,
            h.store_raw.as_deref(),
        )
        .map_err(|e| {
            (
                "io_failed",
                copy_text("beAcctMcpShare.store.writeFailed", &[("why", &e.said())]),
            )
        })?;
    }
    let mut view = view_of(&store, &plan.conflicts);
    view.changed = changed;
    view.notes = notes;
    Ok(view)
}

/// 同步一趟（文件事件 · 加号 · 建库之后）。
pub(crate) fn sync(d: &dyn Door) -> Result<AccountMcpView, Refusal> {
    run(d, Why::Auto, &|s, _| Ok(s.clone()))
}

/// `accounts-mcp-sync {on}`：停 / 开各号之间的同步。只改共享集合那份文件里那一格。
/// 停 ⇒ 之后各号各管各的，已经同步过去的一条不删；开 ⇒ 落下那一格之后立刻同步一趟（停着那段时间各号改的照三方对照采纳，
/// 两边都改了的照常列出来等挑）。已经是那一态 ⇒ 不写，照答。
pub(crate) fn set_sync(d: &dyn Door, on: bool) -> Result<AccountMcpView, Refusal> {
    let home = door::home(d).map_err(|e| ("io_failed", e))?;
    {
        let _held = lock(&home)?;
        let Some(list) = accounts_in(&home)? else {
            return Err(Refusal::from((
                "refused",
                copy_text("beAcctMcpShare.sync.noLibrary", &[]),
            )));
        };
        let h = load(&home, list)?;
        if h.store.paused == !on {
            return if on {
                Ok(view_after(&h))
            } else {
                Ok(paused_view(&h))
            };
        }
        let mut store = h.store.clone();
        store.paused = !on;
        put_private(
            d,
            &home,
            relay_route_core::ACCOUNTS_MCP_REL,
            &store.render(h.key),
            h.store_raw.as_deref(),
        )
        .map_err(|e| {
            (
                "io_failed",
                copy_text("beAcctMcpShare.store.writeFailed", &[("why", &e.said())]),
            )
        })?;
        if !on {
            let mut view = view_of(&store, &[]);
            view.notes = h.notes;
            return Ok(view);
        }
    }
    sync(d)
}

/// 开着时此刻的样子（只算不写；冲突照算）。
fn view_after(h: &Here) -> AccountMcpView {
    let seen: Vec<Seen> = h.accounts.iter().map(|a| a.seen.clone()).collect();
    let plan = mcp_share::plan(&h.store, &seen);
    let mut view = view_of(&plan.store, &plan.conflicts);
    view.notes = h.notes.clone();
    view
}

/// `accounts-mcp-read`：此刻的样子（只算不写；冲突照算）。
pub(crate) fn read(d: &dyn Door) -> Result<AccountMcpView, Refusal> {
    let home = door::home(d).map_err(|e| ("io_failed", e))?;
    let Some(list) = accounts_in(&home)? else {
        return Ok(AccountMcpView::default());
    };
    let h = load(&home, list)?;
    Ok(if h.store.paused {
        paused_view(&h)
    } else {
        view_after(&h)
    })
}

fn known(store: &Store, seen: &[Seen], name: &str) -> Result<(), Refusal> {
    let anywhere = store.servers.contains_key(name)
        || seen
            .iter()
            .any(|s| s.now.as_ref().is_some_and(|c| c.contains_key(name)));
    if anywhere {
        Ok(())
    } else {
        Err(Refusal::from((
            "not_found",
            copy_text("beAcctMcpShare.name.unknown", &[("name", name)]),
        )))
    }
}

/// `accounts-mcp-remove`：从共享集合里删一条，所有号一起撤（删除只有这一条路）。
pub(crate) fn remove(d: &dyn Door, name: &str) -> Result<AccountMcpView, Refusal> {
    run(d, Why::Asked, &|s, seen| {
        known(s, seen, name)?;
        Ok(mcp_share::decide(s, seen, name, None))
    })
}

/// `accounts-mcp-pick`：两边都改了的那一条用哪一版（`from` = 那个号里的；`None` = 共享的那一版）。
pub(crate) fn pick(
    d: &dyn Door,
    name: &str,
    from: Option<&str>,
) -> Result<AccountMcpView, Refusal> {
    run(d, Why::Asked, &|s, seen| {
        known(s, seen, name)?;
        let to = mcp_share::pick(s, seen, name, from).map_err(|e| ("refused", e))?;
        Ok(mcp_share::decide(s, seen, name, to))
    })
}

/// 装到全局：这一条写进共享集合、同步到所有号（名字空 / 定义不是对象 ⇒ 拒）。扩展页的确认卡接它。
pub(crate) fn put(d: &dyn Door, name: &str, def: &Value) -> Result<AccountMcpView, Refusal> {
    if name.trim().is_empty() || !def.is_object() {
        return Err(Refusal::from((
            "bad_args",
            crate::common::contract::malformed("a server needs a name and an object definition"),
        )));
    }
    run(d, Why::Asked, &|s, seen| {
        Ok(mcp_share::decide(s, seen, name, Some(def.clone())))
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/mcp_share_exec_tests.rs"]
mod tests;
