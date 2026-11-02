//! 轮换账本 `rotation.json` 的存取（[`book`] 定形；声明从表取 `relay_route_core::ROTATION_FILE`）。
//!
//! - 读不懂的那一份（连比这一版新的）不覆盖、写一律拒：照上一份读懂的接着用。比下限老的当没有（写的那一下原文留成 `.v<N>`）。
//! - 写在跨进程锁里读 → 改 → 原子写回；改到的各发一件节点（[`ring`]）。写只经两扇门：
//!   中转那一路（[`relay_change`]）与帧面那一路（[`face_change`]）。

use super::rules::book::Book;
use crate::common::said::Said;
use crate::platform::fs::{Own, Ro, Stamp};
use copy_core::copy_text;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(crate) const FILE_NAME: &str = relay_route_core::file_name_of(relay_route_core::ROTATION_REL);

/// 那一份的声明（落点 · 读上限 · 状态 · 版本）。
const DOC: crate::common::own_state::Doc<Book> =
    crate::common::own_state::Doc::of(&relay_route_core::ROTATION_FILE);

static ID_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 起新会话先定好的 sid（UUID v4 的样子：那一家 `--session-id` 只收这一形）。
pub(crate) fn new_session_id() -> String {
    use std::hash::{BuildHasher, Hash, Hasher};
    let seed = std::collections::hash_map::RandomState::new();
    let half = |salt: u8| {
        let mut h = seed.build_hasher();
        salt.hash(&mut h);
        crate::common::time::now().hash(&mut h);
        std::process::id().hash(&mut h);
        ID_SEQ
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .hash(&mut h);
        h.finish()
    };
    let (a, b) = (half(1), half(2));
    let a = (a & !0xF000) | 0x4000; // 版本 4
    let b = (b & !(0xC000 << 48)) | (0x8000 << 48); // 变体 10xx
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        a >> 32,
        (a >> 16) & 0xFFFF,
        a & 0xFFFF,
        b >> 48,
        b & 0xFFFF_FFFF_FFFF
    )
}

// ── 落盘 ─────────────────────────────────────────────────────────────────

/// 这台的落点：`<家>/rotation.json`（家同额度账）。
pub(crate) fn path_from(get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    super::ledger::path_from(get).map(|p| p.with_file_name(FILE_NAME))
}

pub(crate) fn path_now() -> Option<PathBuf> {
    path_from(&|k| std::env::var(k).ok())
}

/// 本进程最后见过的盘上那一份（按路径）。别的进程写了 ⇒ 戳对不上 ⇒ 重扫时比对它发节点。
type DiskSeen = std::collections::HashMap<PathBuf, (Option<Stamp>, Book)>;

fn seen() -> &'static Mutex<DiskSeen> {
    static SEEN: std::sync::OnceLock<Mutex<DiskSeen>> = std::sync::OnceLock::new();
    SEEN.get_or_init(Default::default)
}

/// 改了以后发哪几件：改到的会话各发一件（它那一条变了）；`forNew` 变了 ⇒ 策略那一侧一件。
fn ring(tx: &super::nodes::Nodes, before: &Book, after: &Book) {
    let gone = before
        .sessions
        .keys()
        .filter(|sid| !after.sessions.contains_key(*sid));
    let moved = after
        .sessions
        .iter()
        .filter(|(sid, s)| before.sessions.get(*sid) != Some(s))
        .map(|(sid, _)| sid);
    for sid in moved.chain(gone) {
        super::nodes::emit(tx, super::nodes::Source::Book, Some(sid));
    }
    if before.for_new != after.for_new {
        super::nodes::emit(tx, super::nodes::Source::Strategies, None);
    }
}

/// 盘上那一份。
#[derive(Debug)]
pub(crate) enum Disk {
    Absent,
    Now(Book),
    Unreadable(Said),
}

/// ★ 读一次盘：照声明（这一版照形状读，多一格 ⇒ 读不懂；比这一版新 ⇒ 读不懂；比下限老 ⇒ 当没有）。
pub(crate) fn read_at(path: &Path) -> Disk {
    use crate::common::own_state::Read;
    match DOC.read(path) {
        Read::Absent => Disk::Absent,
        Read::Present(b) => Disk::Now(b),
        Read::Unreadable(e) => Disk::Unreadable(e),
    }
}

struct Held {
    stamp: Option<Stamp>,
    doc: Book,
    unreadable: Option<Said>,
}

/// 盘上那一份 ＋ 一份缓存（盘上的戳变了才重读）。`path = None` ⇒ 家推不出来，只在内存里。
pub(crate) struct RotationStore {
    path: Option<PathBuf>,
    /// 改到的往哪发节点（生产是进程里那一条；判据各给自己的一条）。
    nodes: super::nodes::Nodes,
    cache: Mutex<Held>,
}

impl RotationStore {
    pub(crate) fn at(path: Option<PathBuf>) -> Self {
        Self::with_nodes(path, super::nodes::bus().clone())
    }

    pub(crate) fn with_nodes(path: Option<PathBuf>, nodes: super::nodes::Nodes) -> Self {
        Self {
            path,
            nodes,
            cache: Mutex::new(Held {
                stamp: None,
                doc: Book::fresh(),
                unreadable: None,
            }),
        }
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// 此刻那一份（盘上动过就重读；读不懂 ⇒ 照上一份读懂的）。
    pub(crate) fn now(&self) -> Book {
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let Some(p) = self.path.as_deref() else {
            return g.doc.clone();
        };
        let s = Ro::stamp(p);
        if s.is_some() && s == g.stamp {
            return g.doc.clone();
        }
        let b = match read_at(p) {
            Disk::Now(b) => b,
            Disk::Absent => Book::fresh(),
            Disk::Unreadable(e) => {
                if g.stamp != s {
                    tracing::warn!(
                        "[rotation] {} 读不懂，照上一份读懂的接着用：{}",
                        p.display(),
                        e.logged()
                    );
                }
                g.stamp = s;
                g.unreadable = Some(e);
                return g.doc.clone();
            }
        };
        g.unreadable = None;
        g.doc = b.clone();
        g.stamp = s;
        b
    }

    /// 盘上那份此刻读不懂 ⇒ 为什么；读得懂 · 没有 ⇒ `None`。
    pub(crate) fn unreadable(&self) -> Option<Said> {
        self.now();
        self.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .unreadable
            .clone()
    }

    /// 在跨进程锁里读盘 → 改 → 原子写回 → 缓存跟上；改到的各发一件节点。读不懂的那一份不覆盖。
    fn change<R>(&self, own: &Own, f: impl FnOnce(&mut Book) -> R) -> Result<R, Said> {
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let (r, before, after) = match self.path.as_deref() {
            None => {
                let before = g.doc.clone();
                let r = f(&mut g.doc);
                (r, before, g.doc.clone())
            }
            Some(path) => {
                let (r, before, after) = write_locked(own, f)?;
                g.stamp = Ro::stamp(path);
                g.doc = after.clone();
                g.unreadable = None;
                let base = seen_swap(path, g.stamp, &after).unwrap_or(before);
                (r, base, after)
            }
        };
        drop(g);
        ring(&self.nodes, &before, &after);
        Ok(r)
    }
}

/// 记下这一份，回上一份（没见过 ⇒ `None`）。
fn seen_swap(path: &Path, st: Option<Stamp>, doc: &Book) -> Option<Book> {
    let mut g = seen().lock().unwrap_or_else(|e| e.into_inner());
    g.insert(path.to_path_buf(), (st, doc.clone()))
        .map(|(_, b)| b)
}

/// 重扫一次盘：戳与上回见过的不同（别的进程写了）⇒ 重读、比对、改到的各发一件节点。第一次 ⇒ 只记下。读不懂 ⇒ 不动。
pub(crate) fn rescan(path: &Path) {
    let st = Ro::stamp(path);
    let known = {
        let g = seen().lock().unwrap_or_else(|e| e.into_inner());
        g.get(path).map(|(s, _)| *s)
    };
    if known == Some(st) {
        return;
    }
    let now = match read_at(path) {
        Disk::Now(b) => b,
        Disk::Absent => Book::fresh(),
        Disk::Unreadable(e) => {
            tracing::warn!("[rotation] 重扫 {} 读不懂：{e}", path.display());
            return;
        }
    };
    if let Some(prev) = seen_swap(path, st, &now) {
        if known.is_some() {
            ring(super::nodes::bus(), &prev, &now);
        }
    }
}

pub(crate) fn io_said(path: &Path, e: &std::io::Error) -> Said {
    Said::with_raw(
        copy_text(
            "reason.fs.writeFailed",
            &[
                ("path", &path.display().to_string()),
                ("why", &copy_core::io_reason(e.kind())),
            ],
        ),
        e,
    )
}

/// 中转那一路的写口（第一次看见会话 · 换号 · 记一条 · 存 memo 与叫醒）。
pub(crate) fn relay_change<R>(
    store: &RotationStore,
    own: &Own,
    f: impl FnOnce(&mut Book) -> R,
) -> Result<R, Said> {
    store.change(own, f)
}

/// 帧面那一路的写口（拖出来的顺序 · 现在用 · 新会话用哪份 · 载入策略、换了策略清 memo 与叫醒）。
pub(crate) fn face_change<R>(
    store: &RotationStore,
    own: &Own,
    f: impl FnOnce(&mut Book) -> R,
) -> Result<R, Said> {
    store.change(own, f)
}

fn write_locked<R>(own: &Own, f: impl FnOnce(&mut Book) -> R) -> Result<(R, Book, Book), Said> {
    let file = own.path(FILE_NAME)?;
    let dir = own
        .root()
        .ok_or_else(|| Said::from(copy_text("beRotation.read.noHome", &[])))?;
    let path = file.as_path();
    own.ensure_dir(&dir).map_err(|e| io_said(path, &e))?;
    let held = own.lock(&dir)?;
    let before = match read_at(path) {
        Disk::Absent => Book::fresh(),
        Disk::Now(b) => b,
        Disk::Unreadable(e) => {
            return Err(e.wrap(|said| copy_text("beLineage.write.unreadable", &[("said", said)])))
        }
    };
    let mut after = before.clone();
    let r = f(&mut after);
    if after == before {
        return Ok((r, before, after));
    }
    DOC.write(own, &dir, &held, &after)?;
    Ok((r, before, after))
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/rotation_tests.rs"]
mod tests;
