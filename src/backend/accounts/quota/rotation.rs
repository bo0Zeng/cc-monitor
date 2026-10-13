//! 轮换的存取：每个会话那一侧的事，落 `~/.cc-monitor/rotation.json`（第三版，[`super::rules::book`]）；策略文件在旁边的
//! `strategies/`（[`super::rules::shelf`]）。
//!
//! - 第一版（没有 `version`）· 第二版：读的一侧在内存里照升级链换一遍来用；写一律拒，等接了中转的那个进程起来时在锁里一次写回
//!   （[`migrate_at`]：v1 → v2 → v3、写升级出来的策略文件、旧脚本挪进 `strategies/v3/`、原文留 `.v1` / `.v2`）。
//! - 读不懂的那一份不覆盖：照上一份读懂的接着用。
//! - 写只经两扇门：中转那一路（[`relay_change`]）与帧面那一路（[`face_change`]）；改到的会话、策略那一侧各发一件节点。

pub(crate) mod migrate;

use super::rules::book::{self, Book};
use super::rules::{shelf, upgrade};
use crate::common::said::Said;
use crate::platform::fs::{Own, Ro, Stamp};
use copy_core::copy_text;
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
const MAX_BYTES: u64 = 16 << 20;

/// 盘上那一份。
#[derive(Debug)]
pub(crate) enum Disk {
    Absent,
    Now(Book),
    /// 第一版 / 第二版，还没升级写回：`v1` 是第一版的原文（从第二版读到的 ⇒ 空）· `v2` 是第二版的形（第一版在内存里换好的）。
    Old {
        v1: Option<Value>,
        v2: Value,
    },
    Unreadable(Said),
}

/// ★ 读一次盘：第三版照形状读（多一格 ⇒ 读不懂）；第二版原样交出；没有 `version` ⇒ 第一版，在内存里换成第二版；别的版本 ⇒ 读不懂。
pub(crate) fn read_at(path: &Path) -> Disk {
    use crate::common::own_state::Read;
    let v: Value = match crate::common::own_state::read_json(path, MAX_BYTES) {
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
    match v.get("version").map(Value::as_u64) {
        Some(Some(book::BOOK_VERSION)) => match serde_json::from_value::<Book>(v) {
            Ok(b) => Disk::Now(b),
            Err(e) => bad(e.to_string()),
        },
        Some(Some(2)) => Disk::Old { v1: None, v2: v },
        None => {
            let local = crate::platform::local_tz::iana_name();
            match migrate::from_v1(&v, local.as_deref()) {
                Ok(v2) => Disk::Old { v1: Some(v), v2 },
                Err(e) => bad(e),
            }
        }
        Some(n) => bad(format!("unknown rotation.json version {n:?}")),
    }
}

/// 读的一侧把还没写回的旧版在内存里换成第三版：活会话的副本还没写成文件，先照它打开的那份（`live` 一律答否）；
/// 「其余的订阅号」展开不了（账号库在宿主手里）⇒ 不展开。写回由 [`migrate_at`] 照真的判活与账号库再做一遍。
fn in_memory(v2: &Value) -> Book {
    let local = crate::platform::local_tz::iana_name();
    let env = upgrade::Env {
        rest: &[],
        live: &|_| false,
        title: &|_| None,
        local_tz: local.as_deref(),
    };
    upgrade::from_v2(v2, &env).map_or_else(|_| Book::fresh(), |u| u.book)
}

struct Held {
    stamp: Option<Stamp>,
    book: Book,
    unreadable: Option<Said>,
    /// 盘上还是旧版（写一律拒，等升级）。
    old: bool,
}

/// 盘上那一份 ＋ 一份缓存（盘上的戳变了才重读）。`path = None` ⇒ 家推不出来，只在内存里。
pub(crate) struct RotationStore {
    path: Option<PathBuf>,
    /// 改到会话 · 策略那一侧往哪发节点（生产是进程里那一条；判据各给自己的一条）。
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
                book: Book::fresh(),
                unreadable: None,
                old: false,
            }),
        }
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// 本机策略目录（家推不出 ⇒ `None`）。
    pub(crate) fn strategy_dir(&self) -> Option<PathBuf> {
        self.path.as_deref().and_then(shelf::dir_from)
    }

    /// 此刻那一份（盘上动过就重读；读不懂 ⇒ 照上一份读懂的；旧版 ⇒ 内存里换好的那一份）。
    pub(crate) fn now(&self) -> Book {
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let Some(p) = self.path.as_deref() else {
            return g.book.clone();
        };
        let s = Ro::stamp(p);
        if s.is_some() && s == g.stamp {
            return g.book.clone();
        }
        let (b, old) = match read_at(p) {
            Disk::Now(b) => (b, false),
            Disk::Absent => (Book::fresh(), false),
            Disk::Old { v2, .. } => (in_memory(&v2), true),
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
                return g.book.clone();
            }
        };
        g.unreadable = None;
        g.old = old;
        g.book = b.clone();
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

    /// 在跨进程锁里读盘 → 改 → 原子写回 → 缓存跟上；改到的会话各发一件节点。读不懂 · 还是旧版的那一份不覆盖。
    fn change<R>(&self, own: &Own, f: impl FnOnce(&mut Book) -> R) -> Result<R, Said> {
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let (r, before, after) = match self.path.as_deref() {
            None => {
                let before = g.book.clone();
                let r = f(&mut g.book);
                (r, before, g.book.clone())
            }
            Some(path) => {
                let (r, before, after) = write_locked(own, f)?;
                g.stamp = Ro::stamp(path);
                g.book = after.clone();
                g.unreadable = None;
                g.old = false;
                let base = seen_swap(path, g.stamp, &after).unwrap_or(before);
                (r, base, after)
            }
        };
        drop(g);
        ring_changed(&self.nodes, &before, &after);
        Ok(r)
    }
}

/// 本进程最后见过的盘上那一份（按路径）。别的进程写了 ⇒ 戳对不上 ⇒ 重扫时比对它发节点。
type DiskSeen = std::collections::HashMap<PathBuf, (Option<Stamp>, Book)>;

fn seen() -> &'static Mutex<DiskSeen> {
    static SEEN: std::sync::OnceLock<Mutex<DiskSeen>> = std::sync::OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(DiskSeen::new()))
}

/// 记下这一份，回上一份（没见过 ⇒ `None`）。
fn seen_swap(path: &Path, st: Option<Stamp>, book: &Book) -> Option<Book> {
    let mut g = seen().lock().unwrap_or_else(|e| e.into_inner());
    g.insert(path.to_path_buf(), (st, book.clone()))
        .map(|(_, b)| b)
}

/// 重扫一次盘：戳与上回见过的不同（别的进程写了）⇒ 重读、比对、改到的各发一件节点。第一次 ⇒ 只记下。读不懂 · 旧版 ⇒ 不动。
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
        Disk::Old { .. } => return,
        Disk::Unreadable(e) => {
            tracing::warn!("[rotation] 重扫 {} 读不懂：{e}", path.display());
            return;
        }
    };
    if let Some(prev) = seen_swap(path, st, &now) {
        if known.is_some() {
            ring_changed(super::nodes::bus(), &prev, &now);
        }
    }
}

/// 盯 `rotation.json` 所在的目录（只认这个文件名与额度账）与策略目录：别的进程写了 ⇒ [`rescan`]；策略文件变了 ⇒ 策略那一侧的节点。
/// 返回的那一份活着就一直盯。那两层目录不在就经写句柄建。
pub(crate) fn watch(own: &Own) -> Result<crate::platform::watch_file::Watching, String> {
    let no_home = || copy_text("beRotation.read.noHome", &[]);
    let root = own.root().ok_or_else(no_home)?;
    let sub = own.path(shelf::DIR_NAME).map_err(|e| e.said)?;
    let file = own.path(FILE_NAME).map_err(|e| e.said)?;
    own.ensure_dir(&root).map_err(|e| e.to_string())?;
    own.ensure_dir(&sub).map_err(|e| e.to_string())?;
    let dir = root.as_path().to_path_buf();
    let sdir = sub.as_path().to_path_buf();
    let path = file.as_path();
    rescan(path);
    let target = path.to_path_buf();
    let sdir_for = sdir.clone();
    crate::platform::watch_file::watch(
        &[(dir, false), (sdir.clone(), false)],
        move |p| {
            let name = p.file_name().and_then(|n| n.to_str());
            name == Some(FILE_NAME)
                || name == Some(super::ledger::FILE_NAME)
                || (p.parent() == Some(sdir_for.as_path())
                    && p.extension().and_then(|x| x.to_str()) == Some("rhai"))
        },
        "rotation-watch",
        move |hit| {
            // 额度账被写了（多半是别的进程：CLI 的 quota-probe 等）：本进程的额度账只在用的时候比戳，不会自己响。
            if hit
                .iter()
                .any(|p| p.file_name().and_then(|n| n.to_str()) == Some(super::ledger::FILE_NAME))
            {
                super::nodes::emit(super::nodes::bus(), super::nodes::Source::QuotaFile, None);
            }
            if hit
                .iter()
                .any(|p| p.extension().and_then(|x| x.to_str()) == Some("rhai"))
            {
                super::nodes::emit(super::nodes::bus(), super::nodes::Source::Strategies, None);
            }
            rescan(&target)
        },
    )
}

/// 常驻 / 流那一路的后端起来时调一次：盯这台的 `rotation.json` 与策略目录，进程活着就一直盯。盯不上只出声。
pub(crate) fn watch_here(own: &Own) {
    static HELD: Mutex<Option<crate::platform::watch_file::Watching>> = Mutex::new(None);
    let mut g = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_some() {
        return;
    }
    match watch(own) {
        Ok(w) => *g = Some(w),
        Err(e) => {
            tracing::warn!("[rotation] 盯不上 {FILE_NAME}：{e}（别处写的轮换要等面板重开才看得见）")
        }
    }
}

/// 升级要的外部事实（宿主给）：会话还活着吗（问 pidfile）· 「其余的订阅号」展开成哪几个 · 会话的标题。
pub(crate) struct UpgradeEnv<'a> {
    pub(crate) live: &'a dyn Fn(&str) -> bool,
    pub(crate) rest: Vec<String>,
    pub(crate) title: &'a dyn Fn(&str) -> Option<String>,
}

/// 写回那一下做了什么。
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Migrated {
    /// 从第几版升上来的（`None` ＝ 本来就是第三版 / 没有）。
    pub(crate) from: Option<u8>,
    /// 升级写出来的策略文件。
    pub(crate) wrote: Vec<String>,
    /// 挪进 `strategies/v3/` 的旧脚本。
    pub(crate) moved: Vec<String>,
}

fn io_said(path: &Path, e: &std::io::Error) -> Said {
    Said::with_raw(
        copy_text(
            "beRotation.write.failed",
            &[
                ("path", &path.display().to_string()),
                ("why", &copy_core::io_reason(e.kind())),
            ],
        ),
        e,
    )
}

/// ★ 升级写回（一次；接了中转的那个进程起来时调，[`migrate_here`]）。在 `rotation.json` 那一层的锁里做：
/// 旧版 ⇒ v1 → v2 → v3 一次换完、写出升级来的策略文件（同名已有不覆盖）、第三版格式的脚本挪进 `strategies/v3/`、原文留 `.v1` / `.v2`（已有不覆盖）。
/// 第三版 · 没有文件 ⇒ 什么都不写。读不懂的那一份不动。
pub(crate) fn migrate_at(own: &Own, env: &UpgradeEnv<'_>) -> Result<Migrated, Said> {
    let file = own.path(FILE_NAME)?;
    let root = own
        .root()
        .ok_or_else(|| Said::from(copy_text("beRotation.read.noHome", &[])))?;
    let path = file.as_path();
    own.ensure_dir(&root)
        .map_err(|e| io_said(root.as_path(), &e))?;
    let sdir = own.path(shelf::DIR_NAME)?;
    own.ensure_dir(&sdir)
        .map_err(|e| io_said(sdir.as_path(), &e))?;
    let held = own.lock(&root)?;
    let mut out = Migrated::default();
    let (mut book, old) = match read_at(path) {
        Disk::Absent => (Book::fresh(), None),
        Disk::Now(b) => (b, None),
        Disk::Unreadable(e) => return Err(e),
        Disk::Old { v1, v2 } => (Book::fresh(), Some((v1, v2))),
    };
    let before = book.clone();
    let sheld = own.lock(&sdir)?;
    if let Some((v1, v2)) = &old {
        out.from = Some(if v1.is_some() { 1 } else { 2 });
        let local = crate::platform::local_tz::iana_name();
        let u = upgrade::from_v2(
            v2,
            &upgrade::Env {
                rest: &env.rest,
                live: env.live,
                title: env.title,
                local_tz: local.as_deref(),
            },
        )
        .map_err(|m| {
            Said::with_raw(
                copy_text(
                    "beOwnState.read.notJson",
                    &[("path", &path.display().to_string())],
                ),
                &m,
            )
        })?;
        // 第三版格式的脚本（没有第五版的头）挪开：原文写进 v3/、删掉原处。
        let odir = sdir.join(shelf::OLD_DIR)?;
        for s in shelf::all(Some(sdir.as_path())).iter() {
            if s.source.is_empty() || super::rules::script::has_head(&s.source) {
                continue;
            }
            own.ensure_dir(&odir)
                .map_err(|e| io_said(odir.as_path(), &e))?;
            let oheld = own.lock(&odir)?;
            let to = odir.join(&format!("{}.rhai", s.id))?;
            if !to.as_path().exists() {
                own.write(&to, &oheld, s.source.as_bytes())?;
            }
            let from = sdir.join(&format!("{}.rhai", s.id))?;
            own.delete_file(&from)
                .map_err(|e| io_said(from.as_path(), &e))?;
            out.moved.push(s.id.clone());
        }
        for (id, text) in &u.files {
            let to = sdir.join(&format!("{id}.rhai"))?;
            if !to.as_path().exists() {
                own.write(&to, &sheld, text.as_bytes())?;
                out.wrote.push(id.clone());
            }
        }
        let keep = |name: String, body: &Value| -> Result<(), Said> {
            let backup = own.path(&name)?;
            if !backup.as_path().exists() {
                let b = crate::common::own_state::json_body(backup.as_path(), body)?;
                own.write(&backup, &held, &b)?;
            }
            Ok(())
        };
        match v1 {
            Some(v1) => keep(format!("{FILE_NAME}.v1"), v1)?,
            None => keep(format!("{FILE_NAME}.v2"), v2)?,
        }
        book = u.book;
    }
    drop(sheld);
    if book != before || old.is_some() {
        let body = crate::common::own_state::json_body(path, &book)?;
        own.write(&file, &held, &body)?;
    }
    drop(held);
    if old.is_some() {
        super::nodes::emit(super::nodes::bus(), super::nodes::Source::Strategies, None);
    }
    Ok(out)
}

/// 这台的 `rotation.json` 升级写回（[`migrate_at`]）；写不成只出声。
pub(crate) fn migrate_here(own: &Own, env: &UpgradeEnv<'_>) {
    match migrate_at(own, env) {
        Ok(m) => {
            if let Some(n) = m.from {
                tracing::info!(
                    "[rotation] {FILE_NAME} 从第 {n} 版升到第三版：写了 {:?}，挪开 {:?}",
                    m.wrote,
                    m.moved
                );
            }
        }
        Err(e) => tracing::warn!("[rotation] {FILE_NAME} 升级写回不成：{}", e.logged()),
    }
}

/// 中转那一路的写口（第一次看见会话 · 换号 · 记一条 · 存 memo 与叫醒）。
pub(crate) fn relay_change<R>(
    store: &RotationStore,
    own: &Own,
    f: impl FnOnce(&mut Book) -> R,
) -> Result<R, Said> {
    store.change(own, f)
}

/// 帧面那一路的写口（草稿 · 存 · 下拉换一份 · 拖出来的顺序 · 现在用 · 新会话用哪份）。
pub(crate) fn face_change<R>(
    store: &RotationStore,
    own: &Own,
    f: impl FnOnce(&mut Book) -> R,
) -> Result<R, Said> {
    store.change(own, f)
}

/// 帧面那一路写一份策略文件：`<家>/strategies/<id>.rhai`（目录只建这一层，整份原子写）。读不读得懂由调用方先判。
pub(crate) fn strategy_file_write(own: &Own, id: &str, source: &str) -> Result<PathBuf, Said> {
    let dir = own
        .path(shelf::DIR_NAME)
        .map_err(|_| Said::from(copy_text("beRotation.read.noHome", &[])))?;
    own.ensure_dir(&dir)
        .map_err(|e| io_said(dir.as_path(), &e))?;
    let file = dir.join(&format!("{id}.rhai"))?;
    let held = own.lock(&dir)?;
    own.write(&file, &held, source.as_bytes())?;
    super::nodes::emit(super::nodes::bus(), super::nodes::Source::Strategies, None);
    Ok(file.as_path().to_path_buf())
}

/// 帧面那一路删一份策略文件（只删策略目录里的那一份）。
pub(crate) fn strategy_file_remove(own: &Own, id: &str) -> Result<(), Said> {
    let dir = own
        .path(shelf::DIR_NAME)
        .map_err(|_| Said::from(copy_text("beRotation.read.noHome", &[])))?;
    let file = dir.join(&format!("{id}.rhai"))?;
    own.delete_file(&file)
        .map_err(|e| io_said(file.as_path(), &e))?;
    super::nodes::emit(super::nodes::bus(), super::nodes::Source::Strategies, None);
    Ok(())
}

/// 改到的会话各发一件（它那一条变了）；`forNew` 变了 · 哪个会话用哪份变了 ⇒ 策略那一侧也发一件。
fn ring_changed(tx: &super::nodes::Nodes, before: &Book, after: &Book) {
    let mut uses_moved = before.for_new != after.for_new;
    for (sid, s) in &after.sessions {
        let b = before.sessions.get(sid);
        if b != Some(s) {
            super::nodes::emit(tx, super::nodes::Source::Book, Some(sid));
        }
        if b.map(|b| &b.strategy.id) != Some(&s.strategy.id) {
            uses_moved = true;
        }
    }
    for sid in before.sessions.keys() {
        if !after.sessions.contains_key(sid) {
            super::nodes::emit(tx, super::nodes::Source::Book, Some(sid));
            uses_moved = true;
        }
    }
    if uses_moved {
        super::nodes::emit(tx, super::nodes::Source::Strategies, None);
    }
}

fn write_locked<R>(own: &Own, f: impl FnOnce(&mut Book) -> R) -> Result<(R, Book, Book), Said> {
    let file = own.path(FILE_NAME)?;
    let dir = own
        .root()
        .ok_or_else(|| Said::from(copy_text("beRotation.read.noHome", &[])))?;
    let path = file.as_path();
    let shown = path.display().to_string();
    own.ensure_dir(&dir).map_err(|e| io_said(path, &e))?;
    let held = own.lock(&dir)?;
    let before = match read_at(path) {
        Disk::Absent => Book::fresh(),
        Disk::Now(b) => b,
        // 盘上还是旧版：只有起来时接了中转的那个进程升级（那时旧版的中转已经让出了口）；别的写一律不写。
        Disk::Old { .. } => {
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
