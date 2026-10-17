//! 轮换的账本存取，两份账本共用这一处：基础功能层那份 `rotation.json`（[`book`]）· 配置编辑层那份（那一侧定形，经 [`Doc`] 接进来）。
//!
//! - 盘上不是这一版（第一版 · 第二版）⇒ 当没有、写一律拒，等接了中转的那个进程起来时一次升级写回（配置编辑层做）。
//! - 读不懂的那一份不覆盖：照上一份读懂的接着用。
//! - 写在跨进程锁里读 → 改 → 原子写回；改到的各发一件节点（[`Doc::ring`]）。`rotation.json` 的写只经两扇门：
//!   中转那一路（[`relay_change`]）与帧面那一路（[`face_change`]）。

use super::rules::book::{self, Book};
use crate::common::said::Said;
use crate::platform::fs::{Own, Ro, Stamp};
use copy_core::copy_text;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(crate) const FILE_NAME: &str = relay_route_core::file_name_of(relay_route_core::ROTATION_REL);

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

/// 读盘的上限。
pub(crate) const MAX_BYTES: u64 = 16 << 20;

/// 本进程最后见过的盘上那一份（按路径）。别的进程写了 ⇒ 戳对不上 ⇒ 重扫时比对它发节点。
pub(crate) type DiskSeen<D> = std::collections::HashMap<PathBuf, (Option<Stamp>, D)>;

/// 一份账本的形：文件名 · 版本 · 改了发哪几件节点。
pub(crate) trait Doc:
    Clone + PartialEq + Serialize + DeserializeOwned + Send + 'static
{
    /// 那一份的声明（落点 · 读上限 · 状态）。
    const DISK: crate::common::own_state::Doc<Self>;
    const FILE: &'static str = relay_route_core::file_name_of(Self::DISK.rel);
    /// 这一版的版本号。
    fn version() -> u64;
    /// 没有文件时那一份。
    fn fresh() -> Self;
    /// 盘上的 `version` 是还没升级写回的旧版。
    fn old(_version: Option<u64>) -> bool {
        false
    }
    /// 改了以后发哪几件。
    fn ring(tx: &super::nodes::Nodes, before: &Self, after: &Self);
    /// 本进程最后见过的盘上那一份（每种一张表）。
    fn seen() -> &'static Mutex<DiskSeen<Self>>;
}

impl Doc for Book {
    const DISK: crate::common::own_state::Doc<Self> =
        crate::common::own_state::Doc::state(relay_route_core::ROTATION_REL, MAX_BYTES);
    fn version() -> u64 {
        book::BOOK_VERSION
    }
    fn fresh() -> Self {
        Book::fresh()
    }
    fn old(version: Option<u64>) -> bool {
        matches!(version, None | Some(2))
    }
    /// 改到的会话各发一件（它那一条变了）。
    fn ring(tx: &super::nodes::Nodes, before: &Self, after: &Self) {
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
    }
    fn seen() -> &'static Mutex<DiskSeen<Self>> {
        static SEEN: std::sync::OnceLock<Mutex<DiskSeen<Book>>> = std::sync::OnceLock::new();
        SEEN.get_or_init(Default::default)
    }
}

/// 盘上那一份。
#[derive(Debug)]
pub(crate) enum Disk<D> {
    Absent,
    Now(D),
    /// 还没升级写回的旧版。
    Old,
    Unreadable(Said),
}

/// ★ 读一次盘：这一版照形状读（多一格 ⇒ 读不懂）；旧版 ⇒ [`Disk::Old`]；别的版本 ⇒ 读不懂。
pub(crate) fn read_at<D: Doc>(path: &Path) -> Disk<D> {
    use crate::common::own_state::Read;
    let v: Value = match D::DISK.read_as(path) {
        Read::Absent => return Disk::Absent,
        Read::Unreadable(e) => return Disk::Unreadable(e),
        Read::Present(v) => v,
    };
    let bad = |m: String| {
        Disk::Unreadable(Said::with_raw(
            copy_text(
                "beOwnState.read.notJson",
                &[("path", &path.display().to_string())],
            ),
            &m,
        ))
    };
    let version = v.get("version").map(Value::as_u64);
    match version {
        Some(Some(n)) if n == D::version() => match serde_json::from_value::<D>(v) {
            Ok(b) => Disk::Now(b),
            Err(e) => bad(e.to_string()),
        },
        _ if D::old(version.flatten()) && version != Some(None) => Disk::Old,
        n => bad(format!("unknown {} version {n:?}", D::FILE)),
    }
}

struct Held<D> {
    stamp: Option<Stamp>,
    doc: D,
    unreadable: Option<Said>,
}

/// 盘上那一份 ＋ 一份缓存（盘上的戳变了才重读）。`path = None` ⇒ 家推不出来，只在内存里。
pub(crate) struct DocStore<D: Doc> {
    path: Option<PathBuf>,
    /// 改到的往哪发节点（生产是进程里那一条；判据各给自己的一条）。
    nodes: super::nodes::Nodes,
    cache: Mutex<Held<D>>,
}

/// 基础功能层那份账本。
pub(crate) type RotationStore = DocStore<Book>;

impl<D: Doc> DocStore<D> {
    pub(crate) fn at(path: Option<PathBuf>) -> Self {
        Self::with_nodes(path, super::nodes::bus().clone())
    }

    pub(crate) fn with_nodes(path: Option<PathBuf>, nodes: super::nodes::Nodes) -> Self {
        Self {
            path,
            nodes,
            cache: Mutex::new(Held {
                stamp: None,
                doc: D::fresh(),
                unreadable: None,
            }),
        }
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub(crate) fn nodes(&self) -> &super::nodes::Nodes {
        &self.nodes
    }

    /// 此刻那一份（盘上动过就重读；读不懂 ⇒ 照上一份读懂的；旧版 ⇒ 当没有）。
    pub(crate) fn now(&self) -> D {
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let Some(p) = self.path.as_deref() else {
            return g.doc.clone();
        };
        let s = Ro::stamp(p);
        if s.is_some() && s == g.stamp {
            return g.doc.clone();
        }
        let b = match read_at::<D>(p) {
            Disk::Now(b) => b,
            Disk::Absent | Disk::Old => D::fresh(),
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

    /// 在跨进程锁里读盘 → 改 → 原子写回 → 缓存跟上；改到的各发一件节点。读不懂 · 还是旧版的那一份不覆盖。
    pub(crate) fn change<R>(&self, own: &Own, f: impl FnOnce(&mut D) -> R) -> Result<R, Said> {
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
        D::ring(&self.nodes, &before, &after);
        Ok(r)
    }
}

/// 记下这一份，回上一份（没见过 ⇒ `None`）。
fn seen_swap<D: Doc>(path: &Path, st: Option<Stamp>, doc: &D) -> Option<D> {
    let mut g = D::seen().lock().unwrap_or_else(|e| e.into_inner());
    g.insert(path.to_path_buf(), (st, doc.clone()))
        .map(|(_, b)| b)
}

/// 重扫一次盘：戳与上回见过的不同（别的进程写了）⇒ 重读、比对、改到的各发一件节点。第一次 ⇒ 只记下。读不懂 · 旧版 ⇒ 不动。
pub(crate) fn rescan<D: Doc>(path: &Path) {
    let st = Ro::stamp(path);
    let known = {
        let g = D::seen().lock().unwrap_or_else(|e| e.into_inner());
        g.get(path).map(|(s, _)| *s)
    };
    if known == Some(st) {
        return;
    }
    let now = match read_at::<D>(path) {
        Disk::Now(b) => b,
        Disk::Absent => D::fresh(),
        Disk::Old => return,
        Disk::Unreadable(e) => {
            tracing::warn!("[rotation] 重扫 {} 读不懂：{e}", path.display());
            return;
        }
    };
    if let Some(prev) = seen_swap(path, st, &now) {
        if known.is_some() {
            D::ring(super::nodes::bus(), &prev, &now);
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

/// 帧面那一路的写口（拖出来的顺序 · 现在用 · 换了策略清 memo 与叫醒）。
pub(crate) fn face_change<R>(
    store: &RotationStore,
    own: &Own,
    f: impl FnOnce(&mut Book) -> R,
) -> Result<R, Said> {
    store.change(own, f)
}

fn write_locked<D: Doc, R>(own: &Own, f: impl FnOnce(&mut D) -> R) -> Result<(R, D, D), Said> {
    let file = own.path(D::FILE)?;
    let dir = own
        .root()
        .ok_or_else(|| Said::from(copy_text("beRotation.read.noHome", &[])))?;
    let path = file.as_path();
    let shown = path.display().to_string();
    own.ensure_dir(&dir).map_err(|e| io_said(path, &e))?;
    let held = own.lock(&dir)?;
    let before = match read_at::<D>(path) {
        Disk::Absent => D::fresh(),
        Disk::Now(b) => b,
        // 盘上还是旧版：只有起来时接了中转的那个进程升级（那时旧版的中转已经让出了口）；别的写一律不写。
        Disk::Old => {
            return Err(Said::from(copy_text(
                "beRotation.write.v1Pending",
                &[("path", &shown)],
            )))
        }
        Disk::Unreadable(e) => {
            return Err(e.wrap(|said| copy_text("beRotation.write.unreadable", &[("said", said)])))
        }
    };
    let mut after = before.clone();
    let r = f(&mut after);
    if after == before {
        return Ok((r, before, after));
    }
    let body = crate::common::own_state::json_body(path, &after)?;
    own.write(&file, &held, &body)?;
    Ok((r, before, after))
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/rotation_tests.rs"]
mod tests;
