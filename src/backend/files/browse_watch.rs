//! 保鲜的另一半：只给「用户正在浏览的那几个目录」挂 `inotify`。
//!
//! 全挂挂不住：`inotify` 一个 watch 盯一个目录，每用户上限（几十万）撑不住整个 home；全文件系统监听非特权拿不到
//! （`fanotify_init(2)`：非特权用户不许用 `FAN_MARK_MOUNT` / `FAN_MARK_FILESYSTEM`；后端以用户身份经 SSH 起）。
//! ⇒ 眼前那一块（用户正在看的几十个目录）由本模块盯着、秒级；别处由 `super::index::REWALK_INTERVAL_SECS` 那一档收敛。
//!
//! # 事件进来之后：重列那一个目录，不碰别人
//!
//! 一个目录里有动静 ⇒ 把那一个目录重列一遍，结果放进 [`overlay_snapshot`] 那份小表；查询时 overlay 里那几个目录的直接子项
//! 盖掉大索引里的对应条目（`super::index::find` 里那一句 `supersedes`）。不直接改那块大 blob：在中间插一条要搬 O(n) 字节；
//! overlay 的大小跟着用户在看什么走，不跟着盘的大小走。
//!
//! # 买不到的
//!
//! - 只盯直接子项（`NonRecursive`）：浏览目录底下那棵子树里新建的东西仍然等重走那一档。
//! - watch 绑在 inode 上，不绑路径：浏览的目录被删掉再重建成同名的新 inode，watch 留在旧 inode 上，直到下一次 [`set_browsing`]。
//! - 事件可能丢（内核队列溢出、`IN_Q_OVERFLOW`）：溢出时不知道自己漏了什么，兜底仍然是重走那一档。
//! - 平台不对等：Linux `inotify` · Windows `ReadDirectoryChangesW` · macOS `FSEvents` 的语义（合并、延迟、丢事件的条件）不是一件事，
//!   逐 target 的声明住 `super::FRESHNESS`，不许判成相等。

use std::path::{Path, PathBuf};

/// 同时挂着的 watch 上限。64 相对每用户上限（默认 262 144）是 1/4096；判据用绝对量比（`64 * 4096 <= 262144`），
/// 不写成百分比（`max_user_watches` 是个可以被管理员改小的整数）。
pub const MAX_BROWSE_WATCHES: usize = 64;

/// 一个被盯着的目录，以及它**最近一次重列**出来的直接子项（全路径原始字节 ＋ 类型字节）。
pub(super) struct Watched {
    dir: Vec<u8>,
    children: Vec<(Vec<u8>, u8)>,
}

/// 眼下盯着的那几个目录。**进程里只有这一份。**
pub(super) static WATCHED: std::sync::RwLock<Vec<Watched>> = std::sync::RwLock::new(Vec::new());

/// 查询时拿的那一份 overlay（只读快照）。
pub struct Overlay {
    dirs: Vec<Vec<u8>>,
    children: Vec<(Vec<u8>, u8)>,
}

impl Overlay {
    /// 大索引里这一条，是不是已经被 overlay 里某个目录的重列结果盖掉了。
    pub fn supersedes(&self, entry: &[u8]) -> bool {
        self.dirs
            .iter()
            .any(|d| super::raw::is_direct_child(entry, d))
    }

    /// overlay 自己那些条目（路径 ＋ 类型字节）。
    pub fn iter(&self) -> impl Iterator<Item = (&[u8], u8)> + '_ {
        self.children.iter().map(|(c, k)| (c.as_slice(), *k))
    }

    pub fn dirs(&self) -> usize {
        self.dirs.len()
    }

    pub fn entries(&self) -> usize {
        self.children.len()
    }

    /// overlay 自己的常驻字节 —— `秤 F2 ②` 要把它算进总数。
    pub fn resident_bytes(&self) -> usize {
        self.dirs.iter().map(Vec::len).sum::<usize>()
            + self
                .children
                .iter()
                .map(|(c, _)| c.len() + 1)
                .sum::<usize>()
    }
}

/// 取一份 overlay 快照。
pub fn overlay_snapshot() -> Overlay {
    match WATCHED.read() {
        Ok(g) => Overlay {
            dirs: g.iter().map(|w| w.dir.clone()).collect(),
            children: g.iter().flat_map(|w| w.children.clone()).collect(),
        },
        Err(_) => Overlay {
            dirs: Vec::new(),
            children: Vec::new(),
        },
    }
}

pub fn watched_count() -> usize {
    WATCHED.read().map(|g| g.len()).unwrap_or(0)
}

/// 列一个目录的直接子项（全路径原始字节 ＋ 类型字节）。`None` = 打不开。
///
/// ⚠ 它**不递归**、不跟 symlink、不 `stat` —— 只要名字与目录项自带的类型。
fn list_dir(dir: &Path) -> Option<Vec<(Vec<u8>, u8)>> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut out = Vec::new();
    for e in rd {
        let Ok(e) = e else { continue };
        let kind = e
            .file_type()
            .map(|t| super::query::kind_of(t.is_dir(), t.is_symlink(), t.is_file()))
            .unwrap_or(super::query::KIND_OTHER);
        out.push((super::raw::path_bytes(&e.path()).to_vec(), kind));
    }
    out.sort();
    Some(out)
}

/// 一次 [`set_browsing`] 的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Applied {
    /// 本次新挂上的。
    pub added: usize,
    /// 本次卸掉的（用户不再看它了）。
    pub removed: usize,
    /// 超过 [`MAX_BROWSE_WATCHES`] 被**拒掉**的。
    ///
    /// 🔴 它必须是个数、必须回给调用方：静默截断会让「这个目录我明明在看、
    /// 新建的文件却要 5 分钟才出现」变成一个查不出原因的现象。
    pub rejected: usize,
}

/// 换一批「用户正在浏览的目录」。**差分**：新增的挂上、离开的卸掉。
///
/// 超过上限的部分**拒掉并报数**（见 [`Applied::rejected`]），不静默丢。
pub fn set_browsing(dirs: &[PathBuf]) -> Applied {
    let mut keep: Vec<Vec<u8>> = Vec::new();
    let mut rejected = 0usize;
    for d in dirs {
        let bytes = super::raw::path_bytes(d).to_vec();
        if keep.contains(&bytes) {
            continue;
        }
        if keep.len() >= MAX_BROWSE_WATCHES {
            rejected += 1;
            continue;
        }
        keep.push(bytes);
    }

    let Ok(mut g) = WATCHED.write() else {
        return Applied {
            added: 0,
            removed: 0,
            rejected,
        };
    };
    let before: Vec<Vec<u8>> = g.iter().map(|w| w.dir.clone()).collect();
    let removed = before.iter().filter(|d| !keep.contains(d)).count();
    let mut added = 0usize;
    let mut next: Vec<Watched> = Vec::new();
    for dir in keep {
        if let Some(old) = g.iter().position(|w| w.dir == dir) {
            next.push(Watched {
                dir: g[old].dir.clone(),
                children: g[old].children.clone(),
            });
            continue;
        }
        added += 1;
        let children = list_dir(&super::raw::to_path_buf(&dir)).unwrap_or_default();
        next.push(Watched { dir, children });
    }
    *g = next;
    Applied {
        added,
        removed,
        rejected,
    }
}

/// 一个被盯着的目录里有动静了 ⇒ **重列那一个**。
///
/// 返回 `true` = 这个目录在名单上、已经重列过。`false` = 不在名单上（事件与我们无关）。
pub fn on_change(dir_bytes: &[u8]) -> bool {
    let Ok(mut g) = WATCHED.write() else {
        return false;
    };
    let Some(i) = g.iter().position(|w| w.dir == dir_bytes) else {
        return false;
    };
    g[i].children = list_dir(&super::raw::to_path_buf(dir_bytes)).unwrap_or_default();
    true
}

/// 事件里那个路径属于名单上哪个目录 —— 事件给的可能是**目录本身**，
/// 也可能是它的某个直接子项。
pub fn dir_for_event(path_bytes: &[u8]) -> Option<Vec<u8>> {
    let g = WATCHED.read().ok()?;
    g.iter()
        .find(|w| w.dir == path_bytes || super::raw::is_direct_child(path_bytes, &w.dir))
        .map(|w| w.dir.clone())
}
// ══════════════════════ 真的挂上去那一跳 ══════════════════════

/// 一个活着的监听器（盯盘原语 [`crate::platform::watch_file`] 那一份）。它一被丢掉，watch 就跟着没了 —— 调用方要拿住它。
///
/// 一个时间窗都不传：去抖器要传时间窗，而零定时器护栏把秒级 `Duration` 构造钉成恰好等于登记表条数
/// （`no_timer_guard::REGISTERED_DURATION_USES`）。原语把一阵动静并成一次回调；同一阵里同一个目录只重列一次（一次 `read_dir`，与盘的大小无关）。
pub struct BrowseWatcher {
    inner: crate::platform::watch_file::Watching,
}

impl BrowseWatcher {
    /// 起一个监听器（名单空着，[`Self::sync`] 再挂）。有动静 ⇒ 动过的路径各归到名单上的目录、每个目录调一次 [`on_change`]。
    ///
    /// ⚠ 回调跑在原语收的那条线程上（那条线程**不在** `no_timer_guard` 的人群里 ——
    /// 人群按「本 crate `src/` 的源码文本」画，同 `observe/watcher.rs` 那条已登记的先例）。
    pub fn start() -> Result<Self, String> {
        let inner = crate::platform::watch_file::watch(
            &[],
            |_| true,
            "browse-watch",
            |paths| {
                let mut dirs: Vec<Vec<u8>> = Vec::new();
                for p in paths {
                    if let Some(dir) = dir_for_event(super::raw::path_bytes(p)) {
                        if !dirs.contains(&dir) {
                            dirs.push(dir);
                        }
                    }
                }
                for d in dirs {
                    on_change(&d);
                }
            },
        )?;
        Ok(Self { inner })
    }

    /// 〔「要有人在后端进程里长期持有那个监听器」〕**跟着名单走**：
    /// 名单上新来的挂上、离开的卸掉（留下的那几个不重挂）。
    /// 返回 `(此刻真挂着的个数, 这一趟挂不上的逐条原因)`。卸不掉的不算失败（那个目录多半已经没了）。
    pub fn sync(&mut self) -> (usize, Vec<String>) {
        let want: Vec<crate::platform::watch_file::Dir> = match WATCHED.read() {
            Ok(g) => g
                .iter()
                .map(|w| (super::raw::to_path_buf(&w.dir), false))
                .collect(),
            Err(_) => Vec::new(),
        };
        let failures = self.inner.rearm(&want);
        (self.inner.armed(), failures)
    }
}

/// **进程里那一个监听器**（第一次 [`keep_watching`] 时起，此后一直持有 —— 它一被丢掉 watch 就没了）。
static LIVE: std::sync::Mutex<Option<BrowseWatcher>> = std::sync::Mutex::new(None);

/// 一趟 [`keep_watching`] 的读数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watching {
    /// 此刻真挂着 `inotify`（或那个平台的对应物）的目录数。
    pub watching: usize,
    /// 这一趟没挂上的条数（名单上登记了、当场重列过，但**不会**跟着新）。
    pub failed: usize,
    /// 第一条挂不上的原因（监听器本身起不来也落这里）。`None` ＝ 都挂上了。
    pub error: Option<String>,
}

/// 〔仍开着第一条〕让进程里那一个监听器跟上此刻的名单（[`set_browsing`] 之后调）。
///
/// 监听器起不来（例如 `inotify` 实例数到顶）⇒ `watching: 0`、`error` 说原因，下一趟再试 —— 不静默。
/// ⚠ 买不到的与本模块头注同：watch 绑 inode 不绑路径 · 内核队列溢出 · 非 Linux 平台没量。
pub fn keep_watching() -> Watching {
    let Ok(mut g) = LIVE.lock() else {
        return Watching {
            watching: 0,
            failed: 0,
            error: Some("watcher lock poisoned".to_string()),
        };
    };
    if g.is_none() {
        match BrowseWatcher::start() {
            Ok(w) => *g = Some(w),
            Err(e) => {
                return Watching {
                    watching: 0,
                    failed: watched_count(),
                    error: Some(e),
                }
            }
        }
    }
    let w = g.as_mut().expect("刚放进去");
    let (watching, failures) = w.sync();
    Watching {
        watching,
        failed: failures.len(),
        error: failures.into_iter().next(),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/files/browse_watch_tests.rs"]
mod tests;
