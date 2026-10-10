//! **会话血缘：谁起的谁。** 只在这里认、只存这一份 `~/.cc-monitor/lineage.json`；轮换（跟随父会话）、帧面、
//! 日后的 cc-bus 名册与计划页都只经 [`Book::parent_of`] / [`Book::children_of`] 读它。
//!
//! # 怎么认（唯一认的地方：中转那一路，上游选择对中转的那个口里、换号之前）
//!
//! 起会话的地址尾上带一段来处 `~<来处>[~<父>]`（语法住 `relay_route_core::parse_origin`）：`ccm` 起会话时现铸来处，
//! 被一个会话的 shell 调用时把那个会话的编号写成父。这个会话里再起的任何东西（直接敲的 claude · 脚本 · cc-spawn）
//! 原样继承这条地址。一发请求：头里的会话 `X`、地址里的来处 `T`（可带父 `P`）——
//!
//! 1. `T` 还没绑 ⇒ 绑给 `X`（`X` 就是这条地址交给的那个会话：它里面的进程要等它先发过请求才可能被起）；带 `P` 且 `P ≠ X` ⇒ `X` 的父 ＝ `P`。
//! 2. `T` 绑的就是 `X` ⇒ 不记（subagent 带的也是父会话的编号，走这一格）。
//! 3. `T` 绑的是别的会话 `O` ⇒ `X` 的父 ＝ `O`。
//! 4. 一个会话的父只记第一次，之后不改。没有会话头 / 没有来处 ⇒ 不记。
//!
//! 不翻进程树：只看地址与请求头，Linux / Windows 一样。哪一家（`agent`）照路由第 1 段记下，跟随父会话只在同一家之间成立（轮换那边判）。
//!
//! # 落盘
//!
//! 只在有新事实时写（新来处第一次被用 · 新会话第一次带着父被看见 · 每天头一次被用到刷新时刻）；平常一发零写盘。
//! [`DROP_AFTER`] 没被用到的整条清掉（写的时候顺手清）。跨进程锁里读 → 改 → `own_state` 原子写；读不懂的那一份不覆盖。

use crate::common::said::Said;
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(crate) use relay_route_core::Origin;

pub(crate) const FILE_NAME: &str = relay_route_core::file_name_of(relay_route_core::LINEAGE_REL);

/// 一条被用到的时刻隔多久才刷新一次（秒）。
pub(crate) const SEEN_REFRESH: u64 = 86_400;
/// 多久没被用到就整条清掉（秒）。
pub(crate) const DROP_AFTER: u64 = 90 * 86_400;

/// 一个来处绑给了哪个会话。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Bound {
    pub(crate) sid: String,
    pub(crate) at: u64,
}

/// 一个会话的父。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Parent {
    pub(crate) parent: String,
    /// 这个会话是哪一家（路由第 1 段）。
    pub(crate) agent: String,
    pub(crate) at: u64,
}

/// 整份 `lineage.json`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Book {
    #[serde(default)]
    pub(crate) origins: BTreeMap<String, Bound>,
    #[serde(default)]
    pub(crate) parents: BTreeMap<String, Parent>,
}

impl Book {
    /// 一发请求被看见：`origin` 是地址里的来处段，`sid` 是头里的会话，`agent` 是路由第 1 段。有新事实 ⇒ `true`（要写盘）。
    pub(crate) fn saw(
        &mut self,
        origin: Option<Origin<'_>>,
        sid: &str,
        agent: &str,
        now: u64,
    ) -> bool {
        let Some(o) = origin else {
            return false;
        };
        if sid.is_empty() {
            return false;
        }
        let mut moved = false;
        let parent = match self.origins.get_mut(o.token) {
            None => {
                self.drop_stale(now);
                self.origins.insert(
                    o.token.to_string(),
                    Bound {
                        sid: sid.to_string(),
                        at: now,
                    },
                );
                moved = true;
                o.parent
            }
            Some(b) => {
                if now.saturating_sub(b.at) >= SEEN_REFRESH {
                    b.at = now;
                    moved = true;
                }
                (b.sid != sid).then_some(b.sid.as_str())
            }
        };
        let parent = parent.filter(|p| *p != sid).map(str::to_string);
        match self.parents.get_mut(sid) {
            Some(p) => {
                if now.saturating_sub(p.at) >= SEEN_REFRESH {
                    p.at = now;
                    moved = true;
                }
            }
            None => {
                if let Some(parent) = parent {
                    self.parents.insert(
                        sid.to_string(),
                        Parent {
                            parent,
                            agent: agent.to_string(),
                            at: now,
                        },
                    );
                    moved = true;
                }
            }
        }
        moved
    }

    /// 清掉 [`DROP_AFTER`] 没被用到的来处与父子。
    fn drop_stale(&mut self, now: u64) {
        self.origins
            .retain(|_, b| now.saturating_sub(b.at) < DROP_AFTER);
        self.parents
            .retain(|_, p| now.saturating_sub(p.at) < DROP_AFTER);
    }

    /// 这个会话的父（没记过 ⇒ `None`）。
    pub(crate) fn parent_of(&self, sid: &str) -> Option<&str> {
        self.parents.get(sid).map(|p| p.parent.as_str())
    }
}

// ── 落盘 ─────────────────────────────────────────────────────────────────

/// 这台的落点：`<家>/lineage.json`（家同额度账与轮换）。
pub(crate) fn path_from(get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    crate::accounts::quota::ledger::path_from(get).map(|p| p.with_file_name(FILE_NAME))
}

pub(crate) fn path_now() -> Option<PathBuf> {
    path_from(&|k| std::env::var(k).ok())
}

type Read = crate::common::own_state::Read<Book>;

/// 读盘的上限（90 天清一次，远到不了）。
const MAX_BYTES: u64 = 16 << 20;

pub(crate) fn read_at(path: &Path) -> Read {
    crate::common::own_state::read_json(path, MAX_BYTES)
}

/// 盘上那一份的戳：改动时刻 · 长度 · inode（同 `rotation.rs`）。
type Stamp = (std::time::SystemTime, u64, u64);

fn stamp(p: &Path) -> Option<Stamp> {
    let m = std::fs::metadata(p).ok()?;
    #[cfg(unix)]
    let ino = std::os::unix::fs::MetadataExt::ino(&m);
    #[cfg(not(unix))]
    let ino = 0;
    Some((m.modified().ok()?, m.len(), ino))
}

/// 盘上那一份 ＋ 一份缓存（盘上的戳变了才重读）。`path = None` ⇒ 家推不出来，只在内存里。
pub(crate) struct LineageStore {
    path: Option<PathBuf>,
    cache: Mutex<(Option<Stamp>, Book)>,
}

impl LineageStore {
    pub(crate) fn at(path: Option<PathBuf>) -> Self {
        Self {
            path,
            cache: Mutex::new((None, Book::default())),
        }
    }

    /// 此刻那一份（盘上动过就重读；读不懂 ⇒ 当作空的）。
    pub(crate) fn now(&self) -> Book {
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let Some(p) = self.path.as_deref() else {
            return g.1.clone();
        };
        let s = stamp(p);
        if s.is_none() || s != g.0 {
            g.1 = match read_at(p) {
                Read::Present(b) => b,
                Read::Absent | Read::Unreadable(_) => Book::default(),
            };
            g.0 = s;
        }
        g.1.clone()
    }

    /// 一发请求被看见。先按缓存判有没有新事实，有才拿锁读盘再判一次、写回。写不成只出声，不影响这一发（血缘不是请求的前提）。
    /// 外面只经 [`relay_saw`] 进来。
    fn saw(&self, origin: Option<Origin<'_>>, sid: &str, agent: &str, now: u64) {
        if origin.is_none() || sid.is_empty() || !self.now().clone().saw(origin, sid, agent, now) {
            return;
        }
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        match self.path.as_deref() {
            None => {
                g.1.saw(origin, sid, agent, now);
            }
            Some(path) => match write_locked(path, |b| b.saw(origin, sid, agent, now)) {
                Ok(after) => {
                    g.0 = stamp(path);
                    g.1 = after;
                }
                Err(e) => tracing::warn!("[lineage] {}", e.logged()),
            },
        }
    }
}

/// **唯一的写口**（门是上游选择对中转的那个口）：中转那一路每一发认一次「谁起的谁」。
pub(crate) fn relay_saw(
    store: &LineageStore,
    origin: Option<Origin<'_>>,
    sid: &str,
    agent: &str,
    now: u64,
) {
    store.saw(origin, sid, agent, now);
}

fn write_locked(path: &Path, f: impl FnOnce(&mut Book) -> bool) -> Result<Book, Said> {
    let shown = path.display().to_string();
    let dir = path
        .parent()
        .ok_or_else(|| Said::from(copy_text("beLineage.write.noParent", &[("path", &shown)])))?;
    crate::common::own_dir::ensure_private_dir(dir).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beLineage.write.failed",
                &[("path", &shown), ("why", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        )
    })?;
    let _lock = crate::platform::lock::hold(dir)?;
    let mut book = match read_at(path) {
        Read::Absent => Book::default(),
        Read::Present(b) => b,
        Read::Unreadable(e) => {
            return Err(e.wrap(|said| copy_text("beLineage.write.unreadable", &[("said", said)])))
        }
    };
    if f(&mut book) {
        crate::common::own_state::write_json(path, &book)?;
    }
    Ok(book)
}

#[cfg(test)]
#[path = "../../tests/backend/lineage_tests.rs"]
mod tests;
