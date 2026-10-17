//! `rotation-uses.json`（配置编辑层那份账本，第一版）：每个会话用哪份策略（加改过的设置）· 新会话默认用哪份。
//! 存取同 `rotation.json` 一处（[`rotation::DocStore`]）；写只经 [`Setup::change`]。另住两份账本的升级写回（[`migrate_at`]）· 盯目录（[`watch_here`]）·
//! 起会话之前那一笔（[`preset`]）。

use crate::accounts::quota::nodes::{self, Nodes, Source};
use crate::accounts::quota::rotation::{self, io_said, DiskSeen, Doc, DocStore, RotationStore};
use crate::accounts::quota::rules::book::{self, Book};
use crate::accounts::quota::rules::{Adopted, Adoptions};
use crate::common::said::Said;
use crate::platform::fs::Own;
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub(crate) const FILE_NAME: &str =
    relay_route_core::file_name_of(relay_route_core::ROTATION_USES_REL);

/// 这一版的版本号。
pub(crate) const USES_VERSION: u64 = 1;

/// 整份。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UsesBook {
    pub(crate) version: u64,
    /// 新会话用哪份（策略目录里的文件名去掉 `.rhai`）；空 ⇒ 新会话不轮换，只用开会话的号。
    #[serde(default)]
    pub(crate) for_new: String,
    #[serde(default)]
    pub(crate) sessions: BTreeMap<String, Uses>,
}

/// 会话用的那份策略：文件 id（空 ⇒ 这个会话不轮换）· 打开（或跟上）那一刻的内容版本 · 改过的设置（整份，只在改过时有）· 几时记下的。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Uses {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) rev: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) draft: Option<Value>,
    #[serde(default)]
    pub(crate) since: u64,
}

impl Uses {
    pub(crate) fn of(id: &str, now: u64) -> Self {
        Self {
            id: id.to_string(),
            rev: String::new(),
            draft: None,
            since: now,
        }
    }
}

impl UsesBook {
    /// 用这份策略的会话（按 sid）。
    pub(crate) fn in_use(&self, id: &str) -> Vec<String> {
        self.sessions
            .iter()
            .filter(|(_, s)| s.id == id)
            .map(|(sid, _)| sid.clone())
            .collect()
    }

    /// 第一次问到这个会话 ⇒ 照 `forNew` 记下，顺手清旧的：没草稿 · `rotation.json` 里已经没有 · 记下超过 [`book::DROP_AFTER`]。
    ///
    /// 〔继承表〕轮换那一格：新会话照 `forNew` 那一份，不看起它的那条会话用的是哪份。
    pub(crate) fn pick(&mut self, sid: &str, base: &Book, now: u64) -> Uses {
        if let Some(u) = self.sessions.get(sid) {
            return u.clone();
        }
        self.sessions.retain(|k, u| {
            u.draft.is_some()
                || base.sessions.contains_key(k)
                || now.saturating_sub(u.since) <= book::DROP_AFTER
        });
        let u = Uses::of(&self.for_new, now);
        self.sessions.insert(sid.to_string(), u.clone());
        u
    }
}

impl Doc for UsesBook {
    const FILE: &'static str = FILE_NAME;
    fn version() -> u64 {
        USES_VERSION
    }
    fn fresh() -> Self {
        Self {
            version: USES_VERSION,
            ..Self::default()
        }
    }
    /// 改到的会话各发一件；`forNew` 变了 · 哪个会话用哪份变了 ⇒ 策略那一侧也发一件。
    fn ring(tx: &Nodes, before: &Self, after: &Self) {
        let mut moved = before.for_new != after.for_new;
        for (sid, s) in &after.sessions {
            let b = before.sessions.get(sid);
            if b != Some(s) {
                nodes::emit(tx, Source::Book, Some(sid));
            }
            moved |= b.map(|b| &b.id) != Some(&s.id);
        }
        for sid in before.sessions.keys() {
            if !after.sessions.contains_key(sid) {
                nodes::emit(tx, Source::Book, Some(sid));
                moved = true;
            }
        }
        if moved {
            nodes::emit(tx, Source::Strategies, None);
        }
    }
    fn seen() -> &'static Mutex<DiskSeen<Self>> {
        static SEEN: std::sync::OnceLock<Mutex<DiskSeen<UsesBook>>> = std::sync::OnceLock::new();
        SEEN.get_or_init(Default::default)
    }
}

/// 这台的配置那一侧：本层的账本 ＋ 策略目录，另拿着基础层那份 `rotation.json`（清旧会话 · 起之前那一笔要看它、写它）。
pub(crate) struct Setup {
    pub(crate) uses: DocStore<UsesBook>,
    pub(crate) book: Arc<RotationStore>,
}

impl Setup {
    /// 跟着这份 `rotation.json`（同一层家，节点发到同一条）。
    pub(crate) fn of(book: Arc<RotationStore>) -> Self {
        let path = book.path().map(|p| p.with_file_name(FILE_NAME));
        Self {
            uses: DocStore::with_nodes(path, book.nodes().clone()),
            book,
        }
    }

    /// 本机策略目录（家推不出 ⇒ `None`）。
    pub(crate) fn dir(&self) -> Option<PathBuf> {
        self.book.path().and_then(super::shelf::dir_from)
    }

    /// 帧面那一路的写口（草稿 · 存 · 下拉换一份 · 新会话用哪份）。
    pub(crate) fn change<R>(
        &self,
        own: &Own,
        f: impl FnOnce(&mut UsesBook) -> R,
    ) -> Result<R, Said> {
        self.uses.change(own, f)
    }
}

impl Adoptions for Setup {
    fn adopted(&self, own: &Own, sid: &str, now: u64) -> Adopted {
        let mut b = self.uses.now();
        let u = match b.sessions.get(sid) {
            Some(u) => u.clone(),
            None => {
                let base = self.book.now();
                self.change(own, |x| x.pick(sid, &base, now))
                    .unwrap_or_else(|e| {
                        // 写不成（盘上读不懂）：照内存里那一份接着判，不记。
                        tracing::warn!("[rotation] {}", e.logged());
                        b.pick(sid, &base, now)
                    })
            }
        };
        super::adopted_at(self.dir().as_deref(), &u.id, u.draft.as_ref())
    }
}

/// 起之前那一笔：给定好的 `sid` 记下用 `strategy`（空 ⇒ 不轮换），基础层那份也记一条（会话一报到前界面就看得见它）。
/// 面上的 `session-open` 与 ccm 的 `--no-rotation` 都走这一下。
pub(crate) fn preset(
    setup: &Setup,
    own: &Own,
    sid: &str,
    route: crate::agents::RouteId,
    strategy: &str,
    now: u64,
) -> Result<(), Said> {
    setup.change(own, |b| {
        b.sessions.insert(sid.to_string(), Uses::of(strategy, now));
    })?;
    rotation::face_change(&setup.book, own, |b| {
        b.sessions.insert(
            sid.to_string(),
            book::Session::fresh(route.id(), acct_core::BARE_ACCOUNT, now),
        );
    })
}

/// 起不成：撤掉 [`preset`] 记的那一条（只撤还没换过号的；写不成只出声）。
pub(crate) fn forget_preset(setup: &Setup, own: &Own, sid: &str) {
    let r = rotation::face_change(&setup.book, own, |b| {
        if b.sessions.get(sid).is_some_and(|s| s.log.is_empty()) {
            b.sessions.remove(sid);
            return true;
        }
        false
    })
    .and_then(|gone| {
        if gone {
            setup.change(own, |b| {
                b.sessions.remove(sid);
            })?;
        }
        Ok(())
    });
    if let Err(e) = r {
        tracing::warn!("[rotation] 撤不掉起之前记的那一条 {sid}：{}", e.logged());
    }
}

// ── 盯目录 ─────────────────────────────────────────────────────────────────

/// 盯两份账本所在的目录（只认这两个文件名与额度账）与策略目录：别的进程写了 ⇒ 重扫；策略文件变了 ⇒ 策略那一侧的节点。
/// 返回的那一份活着就一直盯。那两层目录不在就经写句柄建。
pub(crate) fn watch(own: &Own) -> Result<crate::platform::watch_file::Watching, String> {
    let no_home = || copy_text("beRotation.read.noHome", &[]);
    let root = own.root().ok_or_else(no_home)?;
    let sub = own.path(super::shelf::DIR_NAME).map_err(|e| e.said)?;
    let base = own.path(rotation::FILE_NAME).map_err(|e| e.said)?;
    let mine = own.path(FILE_NAME).map_err(|e| e.said)?;
    own.ensure_dir(&root).map_err(|e| e.to_string())?;
    own.ensure_dir(&sub).map_err(|e| e.to_string())?;
    let dir = root.as_path().to_path_buf();
    let sdir = sub.as_path().to_path_buf();
    let (base, mine) = (base.as_path().to_path_buf(), mine.as_path().to_path_buf());
    rotation::rescan::<Book>(&base);
    rotation::rescan::<UsesBook>(&mine);
    let sdir_for = sdir.clone();
    let ledger = crate::accounts::quota::ledger::FILE_NAME;
    crate::platform::watch_file::watch(
        &[(dir, false), (sdir.clone(), false)],
        move |p| {
            let name = p.file_name().and_then(|n| n.to_str());
            [rotation::FILE_NAME, FILE_NAME, ledger]
                .iter()
                .any(|n| name == Some(n))
                || (p.parent() == Some(sdir_for.as_path())
                    && p.extension().and_then(|x| x.to_str()) == Some("rhai"))
        },
        "rotation-watch",
        move |hit| {
            let named = |n: &str| {
                hit.iter()
                    .any(|p| p.file_name().and_then(|x| x.to_str()) == Some(n))
            };
            // 额度账被写了（多半是别的进程：CLI 的 quota-probe 等）：本进程的额度账只在用的时候比戳，不会自己响。
            if named(ledger) {
                nodes::emit(nodes::bus(), Source::QuotaFile, None);
            }
            if hit
                .iter()
                .any(|p| p.extension().and_then(|x| x.to_str()) == Some("rhai"))
            {
                nodes::emit(nodes::bus(), Source::Strategies, None);
            }
            rotation::rescan::<Book>(&base);
            rotation::rescan::<UsesBook>(&mine);
        },
    )
}

/// 常驻 / 流那一路的后端起来时调一次：盯这台的轮换账本与策略目录，进程活着就一直盯。盯不上只出声。
pub(crate) fn watch_here(own: &Own) {
    static HELD: Mutex<Option<crate::platform::watch_file::Watching>> = Mutex::new(None);
    let mut g = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_some() {
        return;
    }
    match watch(own) {
        Ok(w) => *g = Some(w),
        Err(e) => {
            tracing::warn!("[rotation] 盯不上轮换账本：{e}（别处写的轮换要等面板重开才看得见）")
        }
    }
}

// ── 升级写回 ───────────────────────────────────────────────────────────────

/// 升级要的外部事实（宿主给）：会话还活着吗（问 pidfile）· 「其余的订阅号」展开成哪几个 · 会话的标题。
pub(crate) struct UpgradeEnv<'a> {
    pub(crate) live: &'a dyn Fn(&str) -> bool,
    pub(crate) rest: Vec<String>,
    pub(crate) title: &'a dyn Fn(&str) -> Option<String>,
}

/// 写回那一下做了什么。
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Migrated {
    /// 从第几版升上来的（`None` ＝ 本来就是这一版 / 没有）。
    pub(crate) from: Option<u8>,
    /// 升级写出来的策略文件。
    pub(crate) wrote: Vec<String>,
    /// 挪进 `strategies/v3/` 的旧脚本。
    pub(crate) moved: Vec<String>,
}

/// ★ 升级写回（一次；接了中转的那个进程起来时调，[`migrate_here`]）。在 `rotation.json` 那一层的锁里做：
/// 旧版 ⇒ v1 → v2 → 新格式一次换完、写出两份账本与升级来的策略文件（同名已有不覆盖；`rotation-uses.json` 里已有的会话照它的）、
/// 第三版格式的脚本挪进 `strategies/v3/`、原文留 `.v1` / `.v2`（已有不覆盖）。这一版 · 没有文件 ⇒ 什么都不写。读不懂的那一份不动。
pub(crate) fn migrate_at(own: &Own, env: &UpgradeEnv<'_>) -> Result<Migrated, Said> {
    use crate::common::own_state::Read;
    let shelf_dir = super::shelf::DIR_NAME;
    let file = own.path(rotation::FILE_NAME)?;
    let root = own
        .root()
        .ok_or_else(|| Said::from(copy_text("beRotation.read.noHome", &[])))?;
    let path = file.as_path();
    own.ensure_dir(&root)
        .map_err(|e| io_said(root.as_path(), &e))?;
    let sdir = own.path(shelf_dir)?;
    own.ensure_dir(&sdir)
        .map_err(|e| io_said(sdir.as_path(), &e))?;
    let held = own.lock(&root)?;
    let mut out = Migrated::default();
    let bad = |m: String| {
        Said::with_raw(
            copy_text(
                "beOwnState.read.notJson",
                &[("path", &path.display().to_string())],
            ),
            &m,
        )
    };
    let raw: Value = match crate::common::own_state::read_json(path, rotation::MAX_BYTES) {
        Read::Absent => return Ok(out),
        Read::Unreadable(e) => return Err(e),
        Read::Present(v) => v,
    };
    let (v1, v2) = match raw.get("version").map(Value::as_u64) {
        None => {
            let local = crate::platform::local_tz::iana_name();
            (
                Some(raw.clone()),
                super::migrate::from_v1(&raw, local.as_deref()).map_err(bad)?,
            )
        }
        Some(Some(2)) => (None, raw),
        _ => return Ok(out),
    };
    out.from = Some(if v1.is_some() { 1 } else { 2 });
    let local = crate::platform::local_tz::iana_name();
    let u = super::upgrade::from_v2(
        &v2,
        &super::upgrade::Env {
            rest: &env.rest,
            live: env.live,
            title: env.title,
            local_tz: local.as_deref(),
        },
    )
    .map_err(bad)?;
    let sheld = own.lock(&sdir)?;
    // 第三版格式的脚本（没有第五版的头）挪开：原文写进 v3/、删掉原处。
    let odir = sdir.join(super::shelf::OLD_DIR)?;
    for s in super::shelf::all(Some(sdir.as_path())).iter() {
        if s.source.is_empty() || super::script::has_head(&s.source) {
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
    drop(sheld);
    let keep = |name: String, body: &Value| -> Result<(), Said> {
        let backup = own.path(&name)?;
        if !backup.as_path().exists() {
            let b = crate::common::own_state::json_body(backup.as_path(), body)?;
            own.write(&backup, &held, &b)?;
        }
        Ok(())
    };
    match &v1 {
        Some(v1) => keep(format!("{}.v1", rotation::FILE_NAME), v1)?,
        None => keep(format!("{}.v2", rotation::FILE_NAME), &v2)?,
    }
    let ufile = own.path(FILE_NAME)?;
    let mut uses = match rotation::read_at::<UsesBook>(ufile.as_path()) {
        rotation::Disk::Now(b) => b,
        _ => UsesBook::fresh(),
    };
    for (sid, x) in u.uses.sessions {
        uses.sessions.entry(sid).or_insert(x);
    }
    let body = crate::common::own_state::json_body(ufile.as_path(), &uses)?;
    own.write(&ufile, &held, &body)?;
    let body = crate::common::own_state::json_body(path, &u.book)?;
    own.write(&file, &held, &body)?;
    drop(held);
    nodes::emit(nodes::bus(), Source::Strategies, None);
    Ok(out)
}

/// 这台的轮换账本升级写回（[`migrate_at`]）；写不成只出声。
pub(crate) fn migrate_here(own: &Own, env: &UpgradeEnv<'_>) {
    match migrate_at(own, env) {
        Ok(m) => {
            if let Some(n) = m.from {
                tracing::info!(
                    "[rotation] {} 从第 {n} 版升到新格式：写了 {:?}，挪开 {:?}",
                    rotation::FILE_NAME,
                    m.wrote,
                    m.moved
                );
            }
        }
        Err(e) => tracing::warn!(
            "[rotation] {} 升级写回不成：{}",
            rotation::FILE_NAME,
            e.logged()
        ),
    }
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/setup/uses_tests.rs"]
mod tests;
