//! `rotation-uses.json`（配置编辑层那份账本，第一版）：新会话默认用哪份。会话载入的副本存在基础层那份 `rotation.json` 里，
//! 载入只经这里（[`Setup::load`] · 新会话那一份 [`Adoptions::for_new`] · 起会话之前那一笔 [`preset`]）。
//! 存取同 `rotation.json` 一处（[`rotation::DocStore`]）。另住盯目录（[`watch_here`]）。

use crate::accounts::quota::nodes::{self, Nodes, Source};
use crate::accounts::quota::rotation::{self, DiskSeen, Doc, DocStore, RotationStore};
use crate::accounts::quota::rules::book::{self, Book, Copy};
use crate::accounts::quota::rules::Adoptions;
use crate::common::said::Said;
use crate::platform::fs::Own;
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub(crate) const FILE_NAME: &str =
    relay_route_core::file_name_of(relay_route_core::ROTATION_USES_REL);

/// 这一版的版本号。
pub(crate) const USES_VERSION: u64 = 1;

/// 整份：新会话载入哪份（策略目录里的文件名去掉 `.rhai`）；空 ⇒ 新会话不轮换，只用开会话的号。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UsesBook {
    pub(crate) version: u64,
    #[serde(default)]
    pub(crate) for_new: String,
}

impl Doc for UsesBook {
    const DISK: crate::common::own_state::Doc<Self> = crate::common::own_state::Doc::state(
        relay_route_core::ROTATION_USES_REL,
        rotation::MAX_BYTES,
    );
    fn version() -> u64 {
        USES_VERSION
    }
    fn fresh() -> Self {
        Self {
            version: USES_VERSION,
            ..Self::default()
        }
    }
    /// `forNew` 变了 ⇒ 策略那一侧发一件。
    fn ring(tx: &Nodes, before: &Self, after: &Self) {
        if before.for_new != after.for_new {
            nodes::emit(tx, Source::Strategies, None);
        }
    }
    fn seen() -> &'static Mutex<DiskSeen<Self>> {
        static SEEN: std::sync::OnceLock<Mutex<DiskSeen<UsesBook>>> = std::sync::OnceLock::new();
        SEEN.get_or_init(Default::default)
    }
}

/// 在用 `id` 那份的会话（按 sid；`older` ⇒ 只要副本版本不是 `rev` 的）。
pub(crate) fn in_use(book: &Book, id: &str, older_than: Option<&str>) -> Vec<String> {
    book.sessions
        .iter()
        .filter(|(_, s)| {
            s.uses
                .as_ref()
                .is_some_and(|c| c.id == id && older_than.is_none_or(|r| c.rev != r))
        })
        .map(|(sid, _)| sid.clone())
        .collect()
}

/// 这台的配置那一侧：本层的账本 ＋ 策略目录，另拿着基础层那份 `rotation.json`（载入副本 · 起之前那一笔写它）。
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

    /// 帧面那一路的写口（新会话用哪份）。
    pub(crate) fn change<R>(
        &self,
        own: &Own,
        f: impl FnOnce(&mut UsesBook) -> R,
    ) -> Result<R, Said> {
        self.uses.change(own, f)
    }

    /// 给这几个会话载入 `copy`（换了一份 ⇒ memo 与叫醒清掉：那是上一份记的；临时动作照留）。回载入了几个。
    pub(crate) fn load(
        &self,
        own: &Own,
        sids: &[String],
        copy: Option<&Copy>,
    ) -> Result<usize, Said> {
        rotation::face_change(&self.book, own, |b| {
            let mut n = 0;
            for sid in sids {
                if let Some(s) = b.sessions.get_mut(sid) {
                    s.uses = copy.cloned();
                    s.memo = None;
                    s.wake = Default::default();
                    n += 1;
                }
            }
            n
        })
    }
}

impl Adoptions for Setup {
    /// 〔继承表〕轮换那一格：新会话载入 `forNew` 那一份此刻的副本，不看起它的那条会话载入的是哪份。
    fn for_new(&self) -> Option<Copy> {
        super::copy_at(self.dir().as_deref(), &self.uses.now().for_new)
    }
}

/// 起之前那一笔：给定好的 `sid` 记一条、载入 `copy`（缺 ⇒ 不轮换），会话一报到前界面就看得见它。
/// 面上的 `session-open` 与 ccm 的 `--no-rotation` 都走这一下。
pub(crate) fn preset(
    setup: &Setup,
    own: &Own,
    sid: &str,
    route: crate::agents::RouteId,
    copy: Option<Copy>,
    now: u64,
) -> Result<(), Said> {
    rotation::face_change(&setup.book, own, |b| {
        b.sessions.insert(
            sid.to_string(),
            book::Session {
                uses: copy,
                ..book::Session::fresh(route.id(), acct_core::BARE_ACCOUNT, now)
            },
        );
    })
}

/// 起不成：撤掉 [`preset`] 记的那一条（只撤还没换过号的；写不成只出声）。
pub(crate) fn forget_preset(setup: &Setup, own: &Own, sid: &str) {
    let r = rotation::face_change(&setup.book, own, |b| {
        if b.sessions.get(sid).is_some_and(|s| s.log.is_empty()) {
            b.sessions.remove(sid);
        }
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

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/setup/uses_tests.rs"]
mod tests;
