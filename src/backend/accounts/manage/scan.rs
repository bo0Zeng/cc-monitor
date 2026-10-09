//! **读盘那一半**：把账号库此刻的样子读成一份 [`Snapshot`]（只读；不跟链接地看每一项）。
//! 算计划、核对都只吃这份快照，不再碰盘 —— 判据拿临时家目录真建一套、读成快照再喂给它们。

use super::model::Manifest;
use crate::platform::acct_view::{self, Item};
use std::collections::BTreeMap;
use std::path::Path;

/// 清单读取上限（账号数有限，几 MB 足矣）。
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
/// 一份撤销清单的读取上限。
const MAX_UNDO_BYTES: u64 = 4 * 1024 * 1024;

/// 备份目录名的前缀（`<账号库>/.backup-<时间戳>`）。
pub(crate) const BACKUP_PREFIX: &str = ".backup-";
/// 备份目录里记「这一趟改了什么」的那份（每行 `RESTORE\t<路径>` 或 `DELETE\t<路径>`）。
pub(crate) const UNDO_NAME: &str = "undo.tsv";
/// 还原过之后留的标记（最近一份的挑法跳过它）。
pub(crate) const ROLLED_BACK_NAME: &str = ".rolled-back";

/// 清单的文件名（账号库目录下；住址只在契约 crate）。
pub(crate) const MANIFEST_FILE: &str = relay_route_core::ACCOUNTS_MANIFEST_NAME;

/// 这台的账号库目录：`<家目录>/.cc-monitor/accounts`。位置只跟着家走（契约 crate 那一段），这一族都经它。
pub(crate) fn accts_root(home: &str) -> String {
    join(home, relay_route_core::ACCOUNTS_DIR_REL)
}

/// 三个根：家目录 · 共享库（各号链回去的那个配置根）· 账号库。都是绝对 POSIX 路径、不带尾部 `/`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Roots {
    pub home: String,
    pub shared: String,
    pub accts: String,
}

impl Roots {
    pub(crate) fn manifest(&self) -> String {
        join(&self.accts, MANIFEST_FILE)
    }

    /// 能不能动它们：三个都要在家目录底下（写经家目录为根的文件管理面）、共享库与账号库互不包含。
    pub(crate) fn check(&self) -> Result<(), String> {
        for p in [&self.shared, &self.accts] {
            if !super::model::config_dir_ok(p) || !is_under(p, &self.home) || p == &self.home {
                return Err(copy_core::copy_text(
                    "beAcctScan.roots.outsideHome",
                    &[("path", p), ("home", &self.home)],
                ));
            }
        }
        if is_under(&self.accts, &self.shared) || is_under(&self.shared, &self.accts) {
            return Err(copy_core::copy_text(
                "beAcctScan.roots.nested",
                &[("shared", &self.shared), ("accts", &self.accts)],
            ));
        }
        Ok(())
    }

    /// 一个号的目录归不归本模块动：在账号库里，或者就是共享库本身（旧的 in-place 号）。
    pub(crate) fn controls(&self, dir: &str) -> bool {
        (is_under(dir, &self.accts) && dir != self.accts) || dir == self.shared
    }
}

/// `a/b`（`a` 的尾部 `/` 去掉）。
pub(crate) fn join(a: &str, b: &str) -> String {
    format!("{}/{}", a.trim_end_matches('/'), b)
}

/// `child` 是不是 `parent` 本身或在它里面（按路径段，不按字符前缀）。
pub(crate) fn is_under(child: &str, parent: &str) -> bool {
    let p = parent.trim_end_matches('/');
    child == p || child.starts_with(&format!("{p}/"))
}

/// 一个目录此刻的样子。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DirState {
    pub me: Option<Item>,
    /// 直接子项（名 → 那一项，不跟链接）。
    pub entries: BTreeMap<String, Item>,
}

impl DirState {
    pub(crate) fn exists(&self) -> bool {
        self.me.as_ref().is_some_and(Item::exists)
    }
    pub(crate) fn is_dir(&self) -> bool {
        matches!(self.me, Some(Item::Dir { .. }))
    }
    pub(crate) fn mode(&self) -> Option<u32> {
        match self.me {
            Some(Item::Dir { mode }) => mode,
            _ => None,
        }
    }
    pub(crate) fn get(&self, name: &str) -> &Item {
        self.entries.get(name).unwrap_or(&Item::Absent)
    }
}

/// 一份备份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Backup {
    /// `.backup-` 之后那一段（回滚时用的名字）。
    pub id: String,
    pub path: String,
    pub undo: Option<String>,
    pub rolled_back: bool,
}

/// 账号库此刻的样子。
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Snapshot {
    pub roots: Option<Roots>,
    /// 清单原文（写回时拿它做「读的时候就是这一份」的比对）；`None` = 不在。
    pub manifest_text: Option<String>,
    /// 读得出来 ⇒ `Some(Ok)`；在却读不动 / 解不开 ⇒ `Some(Err(那句话))`；不在 ⇒ `None`。
    pub manifest: Option<Result<Manifest, String>>,
    pub shared: DirState,
    /// 账号库目录本身在不在。
    pub accts: DirState,
    /// `$HOME` 下那几份原生根是家目录的身份文件。
    pub home_items: BTreeMap<String, Item>,
    /// 每个号的目录（清单里的 ＋ 调用方另点名要看的）。
    pub dirs: BTreeMap<String, DirState>,
    /// 目录 → 它的 `.claude.json` 里登录的邮箱（家目录那一格是账号 0 的）。
    pub emails: BTreeMap<String, String>,
    /// 调用方另点名要看的单个路径（导入的凭据文件）。
    pub files: BTreeMap<String, Item>,
    pub backups: Vec<Backup>,
    /// 这台 key 表里有哪几行（门递进来的，账号库不自己读那份文件）；`None` = 这一趟不看它。
    pub keys: Option<KeyRows>,
}

/// 这台 key 表（上游选择自己的状态）此刻的样子：它在哪 · 里面有哪几个号（账号 id）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct KeyRows {
    pub path: String,
    pub ids: Vec<String>,
}

impl Snapshot {
    pub(crate) fn roots(&self) -> &Roots {
        self.roots.as_ref().expect("快照总带着三个根")
    }
    pub(crate) fn manifest(&self) -> Option<&Manifest> {
        self.manifest.as_ref().and_then(|r| r.as_ref().ok())
    }
    pub(crate) fn dir(&self, p: &str) -> DirState {
        self.dirs.get(p).cloned().unwrap_or_default()
    }
    pub(crate) fn email_of(&self, dir: &str) -> Option<&str> {
        self.emails.get(dir).map(String::as_str)
    }
}

/// 读一遍。`home` 是这台机器的家目录（判据给临时目录）；`extra_dirs` / `extra_files` 是这一趟另要看的。
pub(crate) fn scan(home: &str, extra_dirs: &[String], extra_files: &[String]) -> Snapshot {
    let home = home.trim_end_matches('/').to_string();
    let accts = accts_root(&home);
    let mpath = join(&accts, MANIFEST_FILE);
    let (manifest_text, manifest) = match acct_view::item(Path::new(&mpath)) {
        Item::Absent => (None, None),
        _ => match manifest_text_at(Path::new(&mpath)) {
            Ok(t) => {
                let parsed = Manifest::parse(&t);
                (Some(t), Some(parsed))
            }
            Err(e) => (
                None,
                Some(Err(e
                    .wrap(|why| {
                        copy_core::copy_text(
                            "beAcctScan.manifest.unreadable",
                            &[("path", &mpath), ("why", why)],
                        )
                    })
                    .said_logging_raw())),
            ),
        },
    };
    let default_shared = super::layout::shared_root_in(&home);
    let shared = manifest
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .and_then(|m| m.shared_store.as_deref())
        .map(|s| {
            let t = s.trim_end_matches('/');
            if t.is_empty() {
                "/".to_string()
            } else {
                t.to_string()
            }
        })
        .unwrap_or(default_shared);
    let roots = Roots {
        home: home.clone(),
        shared: shared.clone(),
        accts: accts.clone(),
    };
    let mut snap = Snapshot {
        shared: dir_state(&shared),
        accts: dir_state(&accts),
        manifest_text,
        manifest,
        ..Snapshot::default()
    };
    for (name, home_rooted, _) in super::layout::identity() {
        if home_rooted {
            snap.home_items.insert(
                name.to_string(),
                acct_view::item(Path::new(&join(&home, name))),
            );
        }
    }
    let mut dirs: Vec<String> = snap
        .manifest()
        .map(|m| m.managed().map(|a| a.config_dir.clone()).collect())
        .unwrap_or_default();
    dirs.extend(extra_dirs.iter().cloned());
    for d in dirs {
        let st = dir_state(&d);
        snap.dirs.insert(d, st);
    }
    for d in std::iter::once(&home)
        .chain(std::iter::once(&shared))
        .chain(snap.dirs.keys())
        .cloned()
        .collect::<Vec<_>>()
    {
        if let Some(e) = super::layout::email_in(&d) {
            snap.emails.insert(d, e);
        }
    }
    for f in extra_files {
        snap.files.insert(f.clone(), acct_view::item(Path::new(f)));
    }
    snap.backups = backups_in(&accts, &snap.accts);
    snap.roots = Some(roots);
    snap
}

/// 清单全文（在却读不动 ⇒ `Err`，调用方把它记进快照、由用的那一步报出来）。
pub(crate) fn manifest_text_at(p: &Path) -> Result<String, crate::common::said::Said> {
    let bytes = crate::common::fs::read_regular_capped(p, MAX_MANIFEST_BYTES)?;
    String::from_utf8(bytes).map_err(|e| {
        crate::common::said::Said::with_raw(
            copy_core::copy_text("beAcctScan.manifest.notUtf8", &[]),
            e,
        )
    })
}

fn dir_state(p: &str) -> DirState {
    let me = acct_view::item(Path::new(p));
    let entries = match me {
        Item::Dir { .. } => acct_view::names(Path::new(p))
            .unwrap_or_default()
            .into_iter()
            .map(|n| {
                let it = acct_view::item(Path::new(&join(p, &n)));
                (n, it)
            })
            .collect(),
        _ => BTreeMap::new(),
    };
    DirState {
        me: Some(me),
        entries,
    }
}

fn backups_in(accts: &str, st: &DirState) -> Vec<Backup> {
    st.entries
        .iter()
        .filter(|(n, it)| n.starts_with(BACKUP_PREFIX) && matches!(it, Item::Dir { .. }))
        .map(|(n, _)| {
            let path = join(accts, n);
            let undo_path = join(&path, UNDO_NAME);
            let undo =
                match crate::common::fs::read_regular_capped(Path::new(&undo_path), MAX_UNDO_BYTES)
                {
                    Ok(b) => String::from_utf8(b).ok(),
                    Err(e) => {
                        tracing::warn!("备份 {path} 的撤销清单读不了，这一份不能用来回滚：{e}");
                        None
                    }
                };
            Backup {
                id: n[BACKUP_PREFIX.len()..].to_string(),
                rolled_back: acct_view::item(Path::new(&join(&path, ROLLED_BACK_NAME))).exists(),
                path,
                undo,
            }
        })
        .collect()
}

/// 备份里一项此刻是什么（回滚时按它的种类还原）。
pub(crate) fn item_at(p: &str) -> Item {
    acct_view::item(Path::new(p))
}
