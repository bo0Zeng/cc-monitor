//! **账号之间同步 MCP 登录的那一半 IO**：读各号那份 MCP 登录文件 → 交 [`super::mcp_logins`] 算 → 逐条落盘 → 底写回共享集合那份文件。
//! 文件名 · 格式 · 锁全照那一家账号面的 MCP 登录那一格（[`crate::agents::McpLogins`]）；订阅号登录那一面不管这件事。
//!
//! - 整趟持账号库那把锁（与同步服务器定义那一半、改账号库那几条命令互斥）；停着同步（同一个开关）⇒ 什么都不做。
//! - 写一个号的一条：经那份文件唯一的写口 [`crate::accounts::oauth::store::rewrite_login`]（照声明依次拿锁 · 锁里重读 · 比对 · 整份原子写），
//!   只经那一家的 `edit` 换那一条，文件里别的条与写法不动；盘上那一条已经不是计划时看到的那一条 ⇒ 不写。
//!   锁有人持着（那个号自己正在续 / 存盘）⇒ 这一条这一趟不写、底不动：它存完盘会再来一个文件事件，下一趟再对照。
//! - 那份文件不在 / 读不出来的号这一趟不参与。说出去的每一句只有号名与原因，不带令牌，也不带版本。

use super::mcp_logins::{self, Cells, Entries, LoginBase, LoginWrite, SeenLogins};
use super::mcp_share_exec::{accounts_of, load_store, lock, manifest_in, put_private, slots};
use crate::accounts::oauth::store::{self, Locked};
use crate::agents::McpLogins;
use crate::common::door::Door;
use crate::common::own_state::Read;
use crate::common::said::Said;
use crate::platform::fs::Own;
use copy_core::copy_text;
use std::path::Path;

/// 一趟的结果：改写了哪几个号 · 没落下的那几句。
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Synced {
    pub changed: Vec<String>,
    pub notes: Vec<String>,
}

/// 一个号此刻的各条（文件不在 · 读不出来 · 那一家读不懂 ⇒ `None`：这个号这一趟不参与）。
fn entries_in(read: Read<store::LoginsText>) -> Option<Entries> {
    match read {
        Read::Present(l) => Some(l.entries),
        Read::Absent | Read::Unreadable(_) => None,
    }
}

/// 一条怎么没落下。
enum Missed {
    /// 锁有人持着 / 盘上那一条已经变了：等下一个文件事件。
    Later,
    /// 出错了（一句原因）。
    Failed(String),
}

/// 落一条（见模块头注）。落下了 ⇒ `Ok(())`。
fn write_one(own: &Own, logins: &McpLogins, cells: &Cells, w: &LoginWrite) -> Result<(), Missed> {
    let done = store::rewrite_login(own, Path::new(&w.dir), logins, &w.key, |read| {
        let Read::Present(now) = read else {
            return (None, Ok(false));
        };
        let cur = now.entries.get(&w.key);
        if cur.map(|e| mcp_logins::rev_of(e, cells)) != w.if_rev {
            return (None, Ok(false));
        }
        match (logins.edit)(&now.text, &w.key, w.to.as_ref()) {
            Ok(text) => (Some(text), Ok(true)),
            Err(why) => (None, Err(why)),
        }
    })
    .map_err(|s| Missed::Failed(s.said))?;
    match done {
        Locked::Held(Ok(true)) => Ok(()),
        Locked::Held(Err(why)) => Err(Missed::Failed(why)),
        _ => Err(Missed::Later),
    }
}

/// 这一条没落下 ⇒ 那个号的底里这一条换回这一趟之前的。
fn revert(next: &mut LoginBase, old: &LoginBase, dir: &str, key: &str) {
    let was = old.get(dir).and_then(|m| m.get(key));
    let one = next.entry(dir.to_string()).or_default();
    match was {
        Some(fp) => {
            one.insert(key.to_string(), fp.clone());
        }
        None => {
            one.remove(key);
        }
    }
    if one.is_empty() {
        next.remove(dir);
    }
}

/// 这一家的 MCP 登录在不在号之间同步（判定只住这一处）：账号面声明了 MCP 登录那一格（`AccountsFace.mcp_logins`）⇒ 那一格；
/// 没有账号面 · 没声明 ⇒ `None`。
pub fn logins_shared(reg: &[crate::agents::Adapter], kind: &str) -> Option<McpLogins> {
    crate::agents::accounts_face_among(reg, kind)?.mcp_logins
}

/// 登录那一趟挑的家（注册序）：有账号面、MCP 登录可同步（[`logins_shared`]）的每一家 ⇒ 它的家 · 共享集合里那一格 · MCP 登录那一格。
/// 不看那一家声明没声明 MCP 那一格。
pub(crate) fn login_families(
    reg: &[crate::agents::Adapter],
) -> impl Iterator<Item = (&'static str, Option<&'static str>, McpLogins)> + '_ {
    slots(reg).filter_map(|(kind, slot)| Some((kind, slot, logins_shared(reg, kind)?)))
}

/// 同步一趟（文件事件 · 加号 · 建库之后，跟在服务器定义那一半后面；注册表可喂夹具）。`own` = 写自家状态的句柄（铸各号的 MCP 登录文件窄句柄）。
/// [`login_families`] 里清单中有号的每一家各一份、只在同一家的号之间；
/// 那一家不同步 MCP 登录（[`logins_shared`]）⇒ 不在里面：不动它的号、一句不说。
pub fn sync_among(d: &dyn Door, own: &Own, reg: &[crate::agents::Adapter]) -> Result<Synced, Said> {
    let home = super::scan::home(d).map_err(|e| Said::new("io_failed", e))?;
    let _held = lock(&home)?;
    let Some(m) = manifest_in(&home)? else {
        return Ok(Synced::default());
    };
    let (whole, store_raw) = load_store(&home, reg)?;
    if whole.paused {
        return Ok(Synced::default());
    }
    let mut next = whole.clone();
    let mut out = Synced::default();
    for (kind, slot, logins) in login_families(reg) {
        if accounts_of(&m, reg, kind).next().is_none() {
            continue;
        }
        let mut one = next.slot(slot);
        one.logins = sync_family(own, &m, reg, kind, &logins, &one.logins, &mut out);
        next.put_slot(slot, one);
    }
    if next != whole {
        put_private(
            d,
            &home,
            relay_route_core::ACCOUNTS_MCP_REL,
            &next.render(super::mcp_share_exec::store_key(reg)),
            store_raw.as_deref(),
        )
        .map_err(|e| {
            Said::new(
                "io_failed",
                copy_text("beAcctMcpShare.store.writeFailed", &[("why", &e.said())]),
            )
        })?;
    }
    Ok(out)
}

/// 一家一趟：读各号那份文件 → 对照 → 逐条落盘；交回这一家新的底（没落下的那几条换回原来的）。
fn sync_family(
    own: &Own,
    m: &super::model::Manifest,
    reg: &[crate::agents::Adapter],
    kind: &'static str,
    logins: &McpLogins,
    old: &LoginBase,
    out: &mut Synced,
) -> LoginBase {
    let cells = Cells {
        access: logins.access,
        refresh: logins.refresh,
        expires_ms: logins.expires_ms,
    };
    let seen: Vec<SeenLogins> = accounts_of(m, reg, kind)
        .map(|a| SeenLogins {
            dir: a.config_dir.clone(),
            now: entries_in(store::read_logins(Path::new(&a.config_dir), logins)),
        })
        .collect();
    let plan = mcp_logins::plan(old, &seen, &cells);
    let mut base = plan.base.clone();
    for w in &plan.writes {
        let name = accounts_of(m, reg, kind)
            .find(|a| a.config_dir == w.dir)
            .map(|a| a.name.clone())
            .unwrap_or_default();
        match write_one(own, logins, &cells, w) {
            Ok(()) => {
                if !out.changed.contains(&name) {
                    out.changed.push(name);
                }
            }
            Err(m) => {
                revert(&mut base, old, &w.dir, &w.key);
                if let Missed::Failed(why) = m {
                    out.notes.push(copy_text(
                        "beAcctMcpLogins.note.writeFailed",
                        &[("account", &name), ("why", &why)],
                    ));
                }
            }
        }
    }
    base
}
