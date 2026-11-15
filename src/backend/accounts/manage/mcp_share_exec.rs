//! **账号之间同步用户级 MCP 的那一半 IO**：读共享集合与各号此刻的样子 → 交 [`super::mcp_share`] 算 → 按计划落盘。
//!
//! - 整趟持账号库那把锁（与改账号库那几条命令互斥，监听器那一路与帧命令那一路也互斥）。
//! - 按家走：注册表里账号面声明了 MCP 那一格（[`crate::agents::McpCells`]）的每一家各一趟，只在同一家的号之间同步；
//!   共享集合那份文件里各家各占一格（[`Store::slot`]）。通用层不认配置文件的格式：读出各服务器、改一个服务器都经那一家的那一格。
//! - 各号的配置文件只动变了的那几个服务器那一节：先把原文放进 `~/.cc-monitor/backups/accounts-mcp/<号><那一家的文件名>`
//!   （每个号一份，下一次改写之前换成新的），再经文件管理面 `Door::put` 写，`expect` = 读到的那一份（CAS）。
//!   被抢先改了 ⇒ 这个号这一趟不写、底不动；抢先的那一下自己会再触发一次同步。
//! - 共享集合那份文件与备份都 0600（里面有 MCP 的密钥）：新建时先落一份空的、改权限、再写内容。
//! - 说出去的每一句只有号名、名字与路径，不带任何定义里的值。

use super::layout;
use super::mcp_share::{self, Seen, Servers, Store};
use super::model::Manifest;
use super::scan::{item_at, join, Unread};
use crate::agents::{Adapter, McpCells, REGISTRY};
use crate::common::door::{self, Door, Refused};
use crate::common::said::Said;
use crate::platform::acct_view::Item;
use cap_core::{Cap, Over};
use cmd_wire::accounts::{AccountMcpChoice, AccountMcpConflict, AccountMcpServer, AccountMcpView};
use copy_core::copy_text;
use serde_json::Value;
use std::path::Path;

/// 共享集合那份小文件的读取上限：超了这一趟同步整个失败，照实回错。
const MAX_SMALL_BYTES: Cap = Cap {
    bytes: 8 * 1024 * 1024,
    over: Over::HardError,
};
/// 改写某个号的配置文件之前那份原文放在备份目录下的哪一层（每个号一份 `<号><配置文件名>`）。
const BACKUP_SUBDIR: &str = "accounts-mcp";
/// 各号配置文件的读取上限（它被项目历史与 MCP 配置撑大，同适配层读它时的那个量级）：超了跳过那个号、说是哪一份。
pub(crate) const MAX_CONFIG_BYTES: Cap = Cap {
    bytes: 32 * 1024 * 1024,
    over: Over::SkipAndSay,
};

/// 一个号读出来的样子 ＋ 原文（落盘时当 `expect`；`None` = 文件不在）。
struct Account {
    seen: Seen,
    path: String,
    raw: Option<String>,
}

/// 同步 MCP 的一家：家名 · 它的 MCP 那一格 · 它在共享集合里那一格（缺省那一家 ⇒ `None`，住顶层）。
#[derive(Clone, Copy)]
pub(crate) struct Family {
    pub kind: &'static str,
    pub cells: McpCells,
    pub slot: Option<&'static str>,
}

/// 注册表里有账号面的每一家 ＋ 它在共享集合里那一格（缺省那一家 ⇒ `None`，住顶层；注册序）。登录那一半走它。
/// 缺省那一家不写格：判在 [`crate::agents::unless_default`]，默认交账号默认那一家。
pub(crate) fn slots(
    reg: &[Adapter],
) -> impl Iterator<Item = (&'static str, Option<&'static str>)> + '_ {
    let main = crate::agents::account_kind_among(reg, None);
    reg.iter()
        .filter(|a| a.accounts.is_some())
        .map(move |a| (a.kind, crate::agents::unless_default(a.kind, main)))
}

/// 注册表里账号面声明了 MCP 那一格的每一家（注册序）。
pub(crate) fn families(reg: &[Adapter]) -> Vec<Family> {
    slots(reg)
        .filter_map(|(kind, slot)| {
            let cells = crate::agents::accounts_face_among(reg, kind)?.mcp?;
            Some(Family { kind, cells, slot })
        })
        .collect()
}

/// 缺省那一家（帧命令没说是哪一家时作用的那一家）：住顶层那一格的；没有 ⇒ `None`（不按注册序挑）。
fn main_family(reg: &[Adapter]) -> Option<Family> {
    let all = families(reg);
    all.iter().find(|f| f.slot.is_none()).copied()
}

/// 清单里这一家的号（号是哪一家照 [`crate::agents::account_kind_among`]）。
pub(crate) fn accounts_of<'m>(
    m: &'m Manifest,
    reg: &'m [Adapter],
    kind: &'m str,
) -> impl Iterator<Item = &'m super::model::Account> + 'm {
    m.managed()
        .filter(move |a| crate::agents::account_kind_among(reg, a.agent.as_deref()) == Some(kind))
}

/// 这台此刻的样子（锁内读）：一家的。
struct Here {
    home: String,
    /// 共享集合里服务器表那一格的键（[`store_key`]）。
    key: &'static str,
    fam: Family,
    accounts: Vec<Account>,
    /// 这一家那一格。
    store: Store,
    /// 整份共享集合（落盘时把这一家那一格放回去，别家的原样）。
    whole: Store,
    store_raw: Option<String>,
    notes: Vec<String>,
}

pub(crate) fn read_text(p: &str, cap: Cap) -> Result<Option<String>, crate::common::said::Said> {
    match item_at(p) {
        Item::Absent => Ok(None),
        Item::File { .. } => {
            let b = crate::platform::fs::Ro::read_capped(Path::new(p), cap)?;
            String::from_utf8(b)
                .map(Some)
                .map_err(|_| copy_text("beSkillInstall.text.binary", &[]).into())
        }
        _ => Err(copy_text("reason.fs.notRegular", &[]).into()),
    }
}

/// 这台的账号库清单（各号之间同步 · 预标信任 · 别名都照它的具名号 `Manifest::managed`：名字与号目录过了写侧那一关）。
/// `None` = 这台没有账号库 / 做不了多账号。家照原样拼（家名不是 UTF-8 也不转字符串一道）；家说不出 ⇒ 照实拒（判只在 `under_home`）。
pub fn manifest_in(home: impl AsRef<Path>) -> Result<Option<Manifest>, Said> {
    if layout::face().is_none() || crate::platform::acct_view::multi_account_supported().is_err() {
        return Ok(None);
    }
    let mpath =
        crate::platform::paths::under_home(Some(home.as_ref()), relay_route_core::ACCOUNTS_DIR_REL)
            .map_err(|s| crate::wire::spec::fail("refused", s))?
            .join(super::scan::MANIFEST_FILE);
    // 读不动那一句只带路径与原因词；原话进复制详情。
    let m = match super::scan::manifest_at(&mpath) {
        Ok(None) => return Ok(None),
        Ok(Some(m)) => m,
        Err(Unread::Io(s)) => return Err(crate::wire::spec::fail("io_failed", s)),
        Err(Unread::Bad(s)) => return Err(crate::wire::spec::fail("refused", s)),
    };
    Ok(Some(m))
}

/// 这台账号库里各号共用的用户级 MCP 住哪份文件；没有账号库 ⇒ `None`（用户级 MCP 就是 agent 自己那一份）。
/// 照家原样拼（家名不是 UTF-8 也不转字符串一道）。⚠ 收字的 [`load_store`] 还另拼一份，尚未收成只留这一形。
pub fn store_path_in(home: &Path) -> Option<std::path::PathBuf> {
    matches!(manifest_in(home), Ok(Some(_)))
        .then(|| crate::platform::paths::home_join(home, relay_route_core::ACCOUNTS_MCP_REL))
}

/// `kind` 那一家这台的用户级 MCP 住哪份文件：有账号库、它又是住共享集合顶层那一家 ⇒ 各号共用的那一份（扩展页读它，
/// 服务器表那一格与那一家自己那份同名）；否则 ⇒ 那一家自己那一份。
pub fn user_mcp_file(kind: &str) -> Option<std::path::PathBuf> {
    let home = crate::platform::paths::home_dir()?;
    let top = main_family(REGISTRY).is_some_and(|f| f.kind == kind);
    top.then(|| store_path_in(&home))
        .flatten()
        .or_else(|| crate::agents::user_mcp_file(kind))
}

/// 一个号的配置文件 ⇒ 服务器表（文件不在 ⇒ 空表），经那一家的读法。
fn servers_in(raw: Option<&str>, cells: &McpCells) -> Result<Servers, String> {
    match raw {
        None => Ok(Servers::new()),
        Some(t) => (cells.read)(t),
    }
}

/// 共享集合里服务器表那一格的键：账号库缺省那一家资产面声明的那个（[`crate::agents::AssetFace::servers_key`]；
/// 有账号库时它的扩展页把这份当用户级 MCP 读）。那一家没有资产面 ⇒ 空串（照样读写得通，只是没人当用户级 MCP 读它）。
/// 这一格的键只判这一处。
pub fn store_key(reg: &[Adapter]) -> &'static str {
    crate::agents::account_kind_among(reg, None)
        .and_then(|k| crate::agents::adapter_among(reg, k))
        .and_then(|a| a.assets)
        .map_or("", |f| f.servers_key)
}

/// 共享集合那份文件（整份，各家各一格）：解好的 ＋ 原文（落盘时当 `expect`；`None` = 文件不在）。
/// 登录那一半（[`super::mcp_logins_exec`]）也读它。
pub(crate) fn load_store(home: &str, reg: &[Adapter]) -> Result<(Store, Option<String>), Said> {
    let spath = door::join_under(home, relay_route_core::ACCOUNTS_MCP_REL);
    // 那一句只带路径与原因词；原话进复制详情。
    let say = |w: &str| {
        copy_text(
            "beAcctMcpShare.store.unreadable",
            &[("path", &spath), ("why", w)],
        )
    };
    let store_raw = read_text(&spath, MAX_SMALL_BYTES)
        .map_err(|e| crate::wire::spec::fail("io_failed", e.wrap(say)))?;
    let store = match &store_raw {
        Some(t) => Store::parse(t, store_key(reg)).map_err(|e| {
            Said::new(
                "refused",
                copy_text(
                    "beAcctMcpShare.store.cannotUse",
                    &[("path", &spath), ("why", &e)],
                ),
            )
        })?,
        None => Store::default(),
    };
    Ok((store, store_raw))
}

fn load(home: &str, m: &Manifest, reg: &[Adapter], fam: Family) -> Result<Here, Said> {
    let (whole, store_raw) = load_store(home, reg)?;
    let store = whole.slot(fam.slot);
    let mut notes = Vec::new();
    let mut accounts = Vec::new();
    let accts = super::scan::accts_root(home);
    for a in accounts_of(m, reg, fam.kind) {
        let (name, dir) = (a.name.clone(), a.config_dir.clone());
        // 号目录不在账号库里（用户自己的配置目录）⇒ 不读不写，不进这一趟（需手动那一件由信任那一侧的 `outside_library` 出）。
        if !super::scan::in_library_on_disk(&dir, &accts) {
            tracing::warn!(
                "账号之间同步 MCP：{name} 号的目录 {dir} 不在账号库里，不读也不写它（需手动改清单）"
            );
            continue;
        }
        let path = join(&dir, fam.cells.file);
        let got = read_text(&path, MAX_CONFIG_BYTES)
            .and_then(|raw| Ok((servers_in(raw.as_deref(), &fam.cells)?, raw)));
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
        key: store_key(reg),
        fam,
        accounts,
        store,
        whole,
        store_raw,
        notes,
    })
}

impl Here {
    /// 这一家那一格换成 `store` 之后整份共享集合的原文。
    fn render_with(&self, store: &Store) -> String {
        let mut whole = self.whole.clone();
        whole.put_slot(self.fam.slot, store.clone());
        whole.render(self.key)
    }
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
    super::exec::set_mode(d, home, rel, 0o600)
        .map_err(|said| Refused::Peer(Said::new("io_failed", said)))?;
    door::put(d, home, rel, text, Some(""), false, false).map(|_| ())
}

/// 改写之前把原文放进这个号那一份备份。
fn backup(d: &dyn Door, home: &str, name: &str, file: &str, raw: &str) -> Result<(), String> {
    let rel = format!(
        "{}/{BACKUP_SUBDIR}/{name}{file}",
        relay_route_core::EXT_BACKUPS_DIR_REL,
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
    let text = edited(&h.fam.cells, a.raw.as_deref(), a.seen.now.as_ref(), want)?;
    if let Some(t) = &a.raw {
        backup(d, &h.home, &a.seen.name, h.fam.cells.file, t)
            .map_err(|e| copy_text("beAcctMcpShare.note.backupFailed", &[("why", &e)]))?;
    }
    match put_private(d, &h.home, &rel, &text, a.raw.as_deref()) {
        Ok(()) => Ok(true),
        Err(Refused::Stale(_)) => Ok(false),
        Err(e) => Err(e.said()),
    }
}

/// 号此刻那份原文（不在 ⇒ 空串）里，`now` 与 `want` 不一样的那几个服务器逐个经那一家的改法换掉（`want` 里没有 ⇒ 删）；
/// 别的服务器那几节与别的键原样。
fn edited(
    cells: &McpCells,
    raw: Option<&str>,
    now: Option<&Servers>,
    want: &Servers,
) -> Result<String, String> {
    let empty = Servers::new();
    let now = now.unwrap_or(&empty);
    let mut text = raw.unwrap_or("").to_string();
    let names: std::collections::BTreeSet<&String> = now.keys().chain(want.keys()).collect();
    for n in names {
        if now.get(n) != want.get(n) {
            text = (cells.edit)(&text, n, want.get(n))?;
        }
    }
    Ok(text)
}

/// `fam` 那一家此刻的样子：每条带上家（住顶层的缺省那一家不写，判法就是 [`slots`] 那一处，同 `mcp-read`）。
fn view_of(store: &Store, conflicts: &[mcp_share::Conflict], fam: Family) -> AccountMcpView {
    let agent = fam.slot.map(cmd_wire::AgentKind::from);
    AccountMcpView {
        enabled: true,
        sync: !store.paused,
        servers: store
            .servers
            .keys()
            .map(|name| AccountMcpServer {
                name: name.clone(),
                agent: agent.clone(),
                agent_said: None,
            })
            .collect(),
        conflicts: conflicts
            .iter()
            .map(|c| AccountMcpConflict {
                name: c.name.clone(),
                agent: agent.clone(),
                agent_said: None,
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
#[expect(
    clippy::disallowed_methods,
    reason = "各号共用的用户级 MCP 那一趟读—改—写：监听器那一路、帧命令那一路、改账号库的命令同时来，后写的会拿旧的对照底算错。与账号库同一把锁"
)]
pub(crate) fn lock(home: &str) -> Result<Option<crate::platform::lock::DirLock>, Said> {
    let accts = super::scan::accts_root(home);
    if item_at(&accts).exists() {
        crate::platform::lock::hold(Path::new(&accts))
            .map(Some)
            .map_err(|e| crate::wire::spec::fail("io_failed", e))
    } else {
        Ok(None)
    }
}

/// 这一趟是谁要的：文件事件 / 加号之后的那一趟（[`Asker::Auto`]），还是用户在 cc-monitor 里点的（[`Asker::Asked`]）。
/// 停着同步时：前一种什么都不写、照实答此刻的样子；后一种拒（要先开回来，否则「从所有号删」就成了同步）。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Asker {
    Auto,
    Asked,
}

/// 停着同步时那一份此刻的样子（不对照、不写；冲突不算 —— 各号各管各的）。
fn paused_view(h: &Here) -> AccountMcpView {
    let mut view = view_of(&h.store, &[], h.fam);
    view.notes = h.notes.clone();
    view
}

/// cc-monitor 里定的那一下：照共享集合与各号读到的，交回新的共享集合。
type StoreDecide<'a> = dyn Fn(&Store, &[Seen]) -> Result<Store, Said> + 'a;

/// 一家一趟：锁 → 读 → `decide`（cc-monitor 里定的那一下；同步那一趟原样交回）→ 对照 → 落盘 → 写回共享集合里那一家那一格。
fn run(
    d: &dyn Door,
    reg: &[Adapter],
    fam: Family,
    asker: Asker,
    decide: &StoreDecide<'_>,
) -> Result<AccountMcpView, Said> {
    let home = super::scan::home(d).map_err(|e| Said::new("io_failed", e))?;
    let _held = lock(&home)?;
    let Some(m) = manifest_in(&home)? else {
        return Ok(AccountMcpView::default());
    };
    let h = load(&home, &m, reg, fam)?;
    if h.store.paused {
        return match asker {
            Asker::Auto => Ok(paused_view(&h)),
            Asker::Asked => Err(crate::wire::spec::fail(
                "refused",
                copy_text("beAcctMcpShare.sync.paused", &[]),
            )),
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
    let text = h.render_with(&store);
    if h.store_raw.as_deref() != Some(text.as_str()) {
        put_private(
            d,
            &home,
            relay_route_core::ACCOUNTS_MCP_REL,
            &text,
            h.store_raw.as_deref(),
        )
        .map_err(|e| {
            Said::new(
                "io_failed",
                copy_text("beAcctMcpShare.store.writeFailed", &[("why", &e.said())]),
            )
        })?;
    }
    let mut view = view_of(&store, &plan.conflicts, h.fam);
    view.changed = changed;
    view.notes = notes;
    Ok(view)
}

/// 同步一趟（文件事件 · 加号 · 建库之后）：每一家各一趟，只在同一家的号之间。
pub fn sync(d: &dyn Door) -> Result<AccountMcpView, Said> {
    sync_among(d, REGISTRY)
}

/// [`sync`] 的可喂夹具那一半：几家的应答并成一份（服务器 · 冲突 · 改了的号 · 备注照家序接着排，见 [`merged`]）。
pub fn sync_among(d: &dyn Door, reg: &[Adapter]) -> Result<AccountMcpView, Said> {
    let v = sync_each(d, reg)?
        .into_iter()
        .fold(None, |out, (_, v)| Some(merged(out, v)))
        .unwrap_or_default();
    Ok(said_when_several(reg, v))
}

/// 同步一趟、各家的应答分开交（家名 · 那一家的应答；注册序）：后台那一趟的日志照它逐家说。
pub fn sync_each(
    d: &dyn Door,
    reg: &[Adapter],
) -> Result<Vec<(&'static str, AccountMcpView)>, Said> {
    families(reg)
        .into_iter()
        .map(|fam| {
            Ok((
                fam.kind,
                run(d, reg, fam, Asker::Auto, &|s, _| Ok(s.clone()))?,
            ))
        })
        .collect()
}

/// 并成的那一份每条写上家的叫法：缺家的是住共享集合顶层的那一家（打标签时省掉的那一家，[`slots`]），
/// 不止一家才写 —— 判在 `agents::said_when_several`（同扩展页 `ExtRow.agentSaid`）。
fn said_when_several(reg: &[Adapter], mut v: AccountMcpView) -> AccountMcpView {
    let agents: Vec<Option<&str>> = v
        .servers
        .iter()
        .map(|s| s.agent.as_deref())
        .chain(v.conflicts.iter().map(|c| c.agent.as_deref()))
        .collect();
    let mut said = crate::agents::said_when_several(
        reg,
        crate::agents::account_kind_among(reg, None),
        &agents,
    )
    .into_iter();
    for s in &mut v.servers {
        s.agent_said = said.next().flatten();
    }
    for c in &mut v.conflicts {
        c.agent_said = said.next().flatten();
    }
    v
}

/// 几家的应答并成一份：服务器 · 冲突 · 改了的号 · 备注照家序接着排（每条带着它的家，同名不相撞）。
fn merged(out: Option<AccountMcpView>, v: AccountMcpView) -> AccountMcpView {
    match out {
        None => v,
        Some(mut o) => {
            o.enabled |= v.enabled;
            o.servers.extend(v.servers);
            o.conflicts.extend(v.conflicts);
            o.changed.extend(v.changed);
            o.notes.extend(v.notes);
            o
        }
    }
}

/// 帧命令作用的那一家（缺省那一家）；没有一家同步 MCP ⇒ 拒（同没有账号库）。
fn main_or_refuse(reg: &[Adapter]) -> Result<Family, Said> {
    main_family(reg).ok_or_else(|| {
        crate::wire::spec::fail("refused", copy_text("beAcctMcpShare.sync.noLibrary", &[]))
    })
}

/// `accounts-mcp-sync {on}`：停 / 开各号之间的同步。只改共享集合那份文件里那一格。
/// 停 ⇒ 之后各号各管各的，已经同步过去的一条不删；开 ⇒ 落下那一格之后立刻同步一趟（停着那段时间各号改的照三方对照采纳，
/// 两边都改了的照常列出来等挑）。已经是那一态 ⇒ 不写，照答。
pub fn set_sync(d: &dyn Door, on: bool) -> Result<AccountMcpView, Said> {
    let home = super::scan::home(d).map_err(|e| Said::new("io_failed", e))?;
    {
        let _held = lock(&home)?;
        let Some(m) = manifest_in(&home)? else {
            return Err(crate::wire::spec::fail(
                "refused",
                copy_text("beAcctMcpShare.sync.noLibrary", &[]),
            ));
        };
        // 停不停是整份共享集合的一格（各家一起），照缺省那一家那一格读、写。
        let h = load(&home, &m, REGISTRY, main_or_refuse(REGISTRY)?)?;
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
            &h.render_with(&store),
            h.store_raw.as_deref(),
        )
        .map_err(|e| {
            Said::new(
                "io_failed",
                copy_text("beAcctMcpShare.store.writeFailed", &[("why", &e.said())]),
            )
        })?;
        if !on {
            let mut view = view_of(&store, &[], h.fam);
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
    let mut view = view_of(&plan.store, &plan.conflicts, h.fam);
    view.notes = h.notes.clone();
    view
}

/// `accounts-mcp-read`：此刻的样子（只算不写；冲突照算）。
pub fn read(d: &dyn Door) -> Result<AccountMcpView, Said> {
    read_among(d, REGISTRY)
}

/// [`read`] 的可喂夹具那一半：同步 MCP 的每一家各读它住的那一格（[`Family::slot`]），并成一份（同 [`sync_among`]）。
pub fn read_among(d: &dyn Door, reg: &[Adapter]) -> Result<AccountMcpView, Said> {
    whole(d, reg, None)
}

/// 完整成品：同步 MCP 的每一家各一份并成一份、写上叫法。`done` ＝ 刚跑过一趟的那一家与它的应答（删 · 挑之后：
/// 那一家用这一份，连这一趟改了的号与备注；别家照此刻读，读不出 · 重读账号清单读不出或读成没有账号库 ⇒ 那一家一句备注、
/// 不推翻也不丢落下的那一笔）；
/// `None` ⇒ 每一家都照此刻读，读不出整笔报。
fn whole(
    d: &dyn Door,
    reg: &[Adapter],
    mut done: Option<(&'static str, AccountMcpView)>,
) -> Result<AccountMcpView, Said> {
    // 删 · 挑已经落盘：别家读不出不推翻那一笔，那一家照实一句备注、不列条目 —— 重读家目录 · 账号清单读不出、读成没有账号库
    // 也一样（落下的那一家照留）。只读那一问照常整笔报，没有账号库 ⇒ 空。
    let written = done.is_some();
    let here = super::scan::home(d)
        .map_err(|e| Said::new("io_failed", e))
        .and_then(|home| Ok(manifest_in(&home)?.map(|m| (home, m))));
    let here = match here {
        Ok(Some(h)) => Ok(h),
        Ok(None) if !written => return Ok(AccountMcpView::default()),
        Ok(None) => Err(Said::new(
            "io_failed",
            copy_text("beAcctMcpShare.sync.noLibrary", &[]),
        )),
        Err(e) if !written => return Err(e),
        Err(e) => Err(e),
    };
    let mut out: Option<AccountMcpView> = None;
    for fam in families(reg) {
        let v = match done.take_if(|(k, _)| *k == fam.kind) {
            Some((_, v)) => v,
            None => match here
                .as_ref()
                .map_err(Said::clone)
                .and_then(|(home, m)| load(home, m, reg, fam))
            {
                Ok(h) if h.store.paused => paused_view(&h),
                Ok(h) => view_after(&h),
                Err(e) if written => {
                    tracing::warn!(
                        "账号之间同步 MCP：落盘之后 {} 那一家读不出来，这一次不列它：{}",
                        fam.kind,
                        e.logged()
                    );
                    AccountMcpView {
                        notes: vec![copy_text(
                            "beAcctMcpShare.note.familyUnread",
                            &[
                                ("agent", &crate::agents::family_said_among(reg, fam.kind)),
                                ("why", &e.said),
                            ],
                        )],
                        ..AccountMcpView::default()
                    }
                }
                Err(e) => return Err(e),
            },
        };
        out = Some(merged(out, v));
    }
    debug_assert!(done.is_none(), "跑过的那一家不在同步 MCP 的那几家里");
    Ok(said_when_several(reg, out.unwrap_or_default()))
}

fn known(store: &Store, seen: &[Seen], name: &str) -> Result<(), Said> {
    let anywhere = store.servers.contains_key(name)
        || seen
            .iter()
            .any(|s| s.now.as_ref().is_some_and(|c| c.contains_key(name)));
    if anywhere {
        Ok(())
    } else {
        Err(crate::wire::spec::fail(
            "not_found",
            copy_text("beAcctMcpShare.name.unknown", &[("name", name)]),
        ))
    }
}

/// 用户在 cc-monitor 里点的那一下：作用在 `agent` 那一家那一格（缺 ⇒ 缺省那一家；没有一家同步 MCP ⇒ 同没有账号库）。
/// 说了家却不在同步 MCP 的那几家（[`families`]）里 ⇒ `bad_args`，不退回缺省那一家。回作用的那一家与它这一趟的应答。
fn asked(
    d: &dyn Door,
    reg: &[Adapter],
    agent: Option<&str>,
    decide: &StoreDecide<'_>,
) -> Result<Option<(&'static str, AccountMcpView)>, Said> {
    let fam = match agent {
        None => main_family(reg),
        Some(k) => Some(
            families(reg)
                .into_iter()
                .find(|f| f.kind == k)
                .ok_or_else(|| {
                    crate::wire::spec::fail(
                        "bad_args",
                        crate::common::contract::malformed(&format!(
                            "agent `{k}` does not share MCP servers here"
                        )),
                    )
                })?,
        ),
    };
    fam.map(|f| Ok((f.kind, run(d, reg, f, Asker::Asked, decide)?)))
        .transpose()
}

/// 删 · 挑那一下：作用在那一家（[`asked`]），回完整成品 —— 各家的条目与叫法都在（[`whole`]，同 `read`），不只作用的那一家。
fn asked_whole(
    d: &dyn Door,
    reg: &[Adapter],
    agent: Option<&str>,
    decide: &StoreDecide<'_>,
) -> Result<AccountMcpView, Said> {
    match asked(d, reg, agent, decide)? {
        Some(done) => whole(d, reg, Some(done)),
        None => Ok(AccountMcpView::default()),
    }
}

/// `accounts-mcp-remove`：从共享集合里 `agent` 那一家那一格删一条，那一家所有号一起撤（删除只有这一条路）。
pub fn remove(d: &dyn Door, name: &str, agent: Option<&str>) -> Result<AccountMcpView, Said> {
    remove_among(d, REGISTRY, name, agent)
}

/// [`remove`] 的可喂夹具那一半。
pub fn remove_among(
    d: &dyn Door,
    reg: &[Adapter],
    name: &str,
    agent: Option<&str>,
) -> Result<AccountMcpView, Said> {
    asked_whole(d, reg, agent, &|s, seen| {
        known(s, seen, name)?;
        Ok(mcp_share::decide(s, seen, name, None))
    })
}

/// `accounts-mcp-pick`：`agent` 那一家两边都改了的那一条用哪一版（`from` = 那个号里的；`None` = 共享的那一版）。
pub fn pick(
    d: &dyn Door,
    name: &str,
    from: Option<&str>,
    agent: Option<&str>,
) -> Result<AccountMcpView, Said> {
    pick_among(d, REGISTRY, name, from, agent)
}

/// [`pick`] 的可喂夹具那一半。
pub fn pick_among(
    d: &dyn Door,
    reg: &[Adapter],
    name: &str,
    from: Option<&str>,
    agent: Option<&str>,
) -> Result<AccountMcpView, Said> {
    asked_whole(d, reg, agent, &|s, seen| {
        known(s, seen, name)?;
        let to = mcp_share::pick(s, seen, name, from).map_err(|e| Said::new("refused", e))?;
        Ok(mcp_share::decide(s, seen, name, to))
    })
}

/// 装到全局：这一条写进共享集合、同步到所有号（名字空 / 定义不是对象 ⇒ 拒）。扩展页的确认卡接它。
/// 回缺省那一家这一趟的应答（装记录那一步照它的备注说装没装上，别家的备注不进来）。
pub fn put(d: &dyn Door, name: &str, def: &Value) -> Result<AccountMcpView, Said> {
    if name.trim().is_empty() || !def.is_object() {
        return Err(crate::wire::spec::fail(
            "bad_args",
            crate::common::contract::malformed("a server needs a name and an object definition"),
        ));
    }
    let done = asked(d, REGISTRY, None, &|s, seen| {
        Ok(mcp_share::decide(s, seen, name, Some(def.clone())))
    })?;
    Ok(done.map(|(_, v)| v).unwrap_or_default())
}

/// 判 MCP 状态要看的那几个家：那一家没设账号时的家 ＋ 账号库里这一家的号的家（同一个目录只算一次，带名字的那一条留下）。
/// 没设账号时的家另记一格 `bare`：某个号的家恰是它时 `homes` 里不另列，没设账号那一份配置照样照它认缓存。
/// 账号库读不出 ⇒ 只看没设账号的那一份（状态少说，不说错）。
pub fn mcp_look(kind: &str) -> crate::agents::McpLook {
    mcp_look_among(REGISTRY, kind)
}

/// [`mcp_look`] 照给定注册表（足迹那一趟交它取的那一份）：号与没设账号时的家都照它取。
pub fn mcp_look_among(registry: &[Adapter], kind: &str) -> crate::agents::McpLook {
    let mut homes = crate::platform::paths::home_dir()
        .and_then(|h| manifest_in(h).ok().flatten())
        .map(|m| account_homes(&m, registry, kind))
        .unwrap_or_default();
    let bare = crate::agents::adapter_among(registry, kind).and_then(|a| (a.home)());
    if let Some(b) = &bare {
        if !homes.iter().any(|(_, d)| d == b) {
            homes.push((None, b.clone()));
        }
    }
    let now_ms = crate::common::time::now_ms();
    crate::agents::McpLook {
        homes,
        bare,
        now_ms,
    }
}

/// [`mcp_look`] 里账号库那一半：清单里 `kind` 那一家的号各一条（号名 · 号目录），别家的号不算（注册表可喂夹具）。
pub fn account_homes(
    m: &Manifest,
    reg: &[Adapter],
    kind: &str,
) -> Vec<(Option<String>, std::path::PathBuf)> {
    accounts_of(m, reg, kind)
        .map(|a| {
            (
                Some(a.name.clone()),
                std::path::PathBuf::from(&a.config_dir),
            )
        })
        .collect()
}

/// 一个要登录的 MCP 服务器：名字 · 哪一段（user / project / local）· 要登录的是哪几位（同 [`crate::agents::McpEntry::login_in`]：
/// 号名，`None` ＝ 没设账号的那一份）· 项目级 · 本地级那一条所在的项目目录（用户级 ⇒ `None`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpNeed {
    pub name: String,
    pub scope: &'static str,
    pub login_in: Vec<Option<String>>,
    pub project: Option<String>,
}

/// ★ MCP 需登录只在这一处判（待办 · 需手动都照它，各自成行）：几份读面 —— 用户级一份（`None`）＋ 活会话所在项目各一份（`Some(目录)`）
/// ⇒ 要登录的那几条。项目那一份里的用户级不重复算（用户级那一份已经说了）；项目级 · 本地级那一条带上它所在的项目。
/// 读面是逐号读的：几个号里的同一服务器先经核心 `mcp_held` 合成一条（要登录的那几位并起来，没设账号那一位也在里面），不按号重复出项。
pub fn needs_login_of(reads: &[(Option<String>, crate::agents::McpRead)]) -> Vec<McpNeed> {
    let mut out = Vec::new();
    for (dir, read) in reads {
        for h in crate::agents::mcp_held(&read.entries, &[]) {
            let e = h.entry;
            if e.status != crate::agents::McpStatus::NeedsLogin
                || (dir.is_some() && e.scope == "user")
            {
                continue;
            }
            out.push(McpNeed {
                name: e.name,
                scope: e.scope,
                login_in: e.login_in,
                project: dir.clone(),
            });
        }
    }
    out
}

/// 这台 `kind` 那一家要登录的 MCP（[`needs_login_of`] 的口径）：`projects` 是这台此刻活着的会话所在的项目（宿主递进来：
/// 判活住 history，accounts 不认识它）。那一家不认得 MCP ⇒ 空。
pub fn needs_login(kind: &str, projects: &[String]) -> Vec<McpNeed> {
    needs_login_among(REGISTRY, kind, projects)
}

/// [`needs_login`] 照给定注册表（足迹那一趟交它取的那一份）：号 · 家 · 读面都照它取。
pub fn needs_login_among(registry: &[Adapter], kind: &str, projects: &[String]) -> Vec<McpNeed> {
    let look = mcp_look_among(registry, kind);
    let dirs = std::iter::once(None).chain(projects.iter().cloned().map(Some));
    let reads: Vec<(Option<String>, crate::agents::McpRead)> = dirs
        .filter_map(|dir| {
            let read = crate::agents::mcp_read_among(
                registry,
                kind,
                dir.as_deref().map(Path::new),
                &look,
            )?;
            Some((dir, read))
        })
        .collect();
    needs_login_of(&reads)
}
