//! **账号之间同步 MCP 登录的那一半 IO**：读各号凭据文件里那一节 → 交 [`super::mcp_logins`] 算 → 逐条落盘 → 底写回共享集合那份文件。
//!
//! - 整趟持账号库那把锁（与同步服务器定义那一半、改账号库那几条命令互斥）；停着同步（同一个开关）⇒ 什么都不做。
//! - 写一个号的一条：经订阅号凭据的窄句柄（[`crate::platform::fs::AccountCreds`]），先拿那个号的 claude 续这一条时拿的那把锁，
//!   再经凭据文件唯一的写口 [`crate::accounts::oauth::store::rewrite`]（那一家的整份写锁里重读 · 比对 · 整份原子写）：
//!   只换那一节里那一条，文件里别的一格不动；盘上那一条已经不是计划时看到的那一条 ⇒ 不写。
//!   锁有人持着（那个号自己正在续 / 存盘）⇒ 这一条这一趟不写、底不动：它存完盘会再来一个文件事件，下一趟再对照。
//! - 凭据文件不在 / 读不出来的号这一趟不参与。说出去的每一句只有号名与原因，不带令牌，也不带指纹。

use super::mcp_logins::{self, Cells, Entries, LoginBase, LoginWrite, SeenLogins};
use super::mcp_share_exec::{accounts_in, load_store, lock, put_private};
use crate::accounts::oauth::store::{self, Locked, Read};
use crate::agents::{EntryLogins, LoginFace};
use crate::assets::door::{self, Door};
use crate::platform::fs::Own;
use copy_core::copy_text;
use serde_json::Value;
use std::path::Path;

type Refusal = crate::stream::inbound::spec::Fail;

/// 一趟的结果：改写了哪几个号 · 没落下的那几句。
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Synced {
    pub changed: Vec<String>,
    pub notes: Vec<String>,
}

/// 一个号凭据文件里那一节（文件不在 · 读不出来 · 那一节不是对象 ⇒ `None`：这个号这一趟不参与）。
fn section_in(read: Read, section: &str) -> Option<Entries> {
    match read {
        Read::Present(doc) => match doc.get(section) {
            None => Some(Entries::new()),
            Some(Value::Object(m)) => Some(m.clone()),
            Some(_) => None,
        },
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
fn write_one(
    own: &Own,
    face: &LoginFace,
    entries: &EntryLogins,
    cells: &Cells,
    w: &LoginWrite,
) -> Result<(), Missed> {
    let failed = |s: crate::common::said::Said| Missed::Failed(s.said);
    let creds = store::creds_of(own, Path::new(&w.dir), face).map_err(failed)?;
    let done = store::with_entry_lock(&creds, entries, &w.key, |held| {
        store::rewrite(&creds, face, held, |read| {
            let Read::Present(mut doc) = read else {
                return (None, false);
            };
            let cur = doc.get(entries.section).and_then(|s| s.get(&w.key));
            if cur.map(|e| mcp_logins::fingerprint(e, cells)) != w.expect {
                return (None, false);
            }
            let sec = doc
                .entry(entries.section.to_string())
                .or_insert_with(|| Value::Object(Default::default()));
            let Value::Object(m) = sec else {
                return (None, false);
            };
            match &w.to {
                Some(v) => {
                    m.insert(w.key.clone(), v.clone());
                }
                None => {
                    m.remove(&w.key);
                }
            }
            (Some(doc), true)
        })
    })
    .map_err(failed)?;
    match done {
        Locked::Held(Ok(Locked::Held(true))) => Ok(()),
        Locked::Held(Err(e)) => Err(failed(e)),
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

/// 同步一趟（文件事件 · 加号 · 建库之后，跟在服务器定义那一半后面）。`own` = 写自家状态的句柄（铸各号的凭据窄句柄）。
pub(crate) fn sync(d: &dyn Door, own: &Own) -> Result<Synced, Refusal> {
    let home = door::home(d).map_err(|e| ("io_failed", e))?;
    let _held = lock(&home)?;
    let Some(list) = accounts_in(&home)? else {
        return Ok(Synced::default());
    };
    let Some(face) = super::layout::face().and_then(|f| f.login) else {
        return Ok(Synced::default());
    };
    let Some(entries) = face.entries else {
        return Ok(Synced::default());
    };
    let (store, store_raw) = load_store(&home)?;
    if store.paused {
        return Ok(Synced::default());
    }
    let cells = Cells {
        access: entries.access,
        refresh: entries.refresh,
        expires_ms: entries.expires_ms,
    };
    let seen: Vec<SeenLogins> = list
        .iter()
        .map(|(_, dir)| SeenLogins {
            dir: dir.clone(),
            now: section_in(store::read(Path::new(dir), &face), entries.section),
        })
        .collect();
    let plan = mcp_logins::plan(&store.logins, &seen, &cells);
    let mut base = plan.base.clone();
    let mut out = Synced::default();
    for w in &plan.writes {
        let name = list
            .iter()
            .find(|(_, dir)| *dir == w.dir)
            .map(|(n, _)| n.clone())
            .unwrap_or_default();
        match write_one(own, &face, &entries, &cells, w) {
            Ok(()) => {
                if !out.changed.contains(&name) {
                    out.changed.push(name);
                }
            }
            Err(m) => {
                revert(&mut base, &store.logins, &w.dir, &w.key);
                if let Missed::Failed(why) = m {
                    out.notes.push(copy_text(
                        "beAcctMcpLogins.note.writeFailed",
                        &[("account", &name), ("why", &why)],
                    ));
                }
            }
        }
    }
    if base != store.logins {
        let mut next = store.clone();
        next.logins = base;
        put_private(
            d,
            &home,
            relay_route_core::ACCOUNTS_MCP_REL,
            &next.render(super::layout::user_mcp_key()),
            store_raw.as_deref(),
        )
        .map_err(|e| {
            (
                "io_failed",
                copy_text("beAcctMcpShare.store.writeFailed", &[("why", &e.said())]),
            )
        })?;
    }
    Ok(out)
}
