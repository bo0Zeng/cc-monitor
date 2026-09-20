//! 〔步 24f · 2026-09-20〕**保鲜的另一半**：只给「用户正在浏览的那几个目录」挂 `inotify`。
//!
//! # 为什么只挂几十个，而不是全挂
//!
//! 这不是取舍，是两条现打把路堵死了（`真相源/98 §3.2` 两行 ＋ `man 2 fanotify_init` 一手）：
//!
//! | 现打 | 值 |
//! |---|---|
//! | 本机 `inotify` 每用户 watch 上限 | **262 144** |
//! | 我 home 底下的条目数 | **640 413** |
//!
//! ⇒ **全挂挂不住**（`inotify` 一个 watch 盯一个目录），而「全文件系统监听」那条路
//! 非特权拿不到：`fanotify_init(2)` 手册页逐字「非特权用户……不许用
//! `FAN_MARK_MOUNT` / `FAN_MARK_FILESYSTEM`」，而后端**是以用户身份经 SSH 起的、不是 root**。
//! Everything 那套（读 MFT ＋ 订阅 USN Journal）**两段都要管理员**（voidtools FAQ 逐字）。
//!
//! ⇒ 于是分工写死：**眼前那一块**（用户正在看的几十个目录）由本模块盯着、秒级；
//! **别处**由 `super::index::REWALK_INTERVAL_SECS` 那一档收敛。
//!
//! # 事件进来之后做什么：**重列那一个目录，不碰别人**
//!
//! 一个目录里有动静 ⇒ 把**那一个目录**重列一遍，结果放进 [`overlay_snapshot`] 那份小表。
//! 查询时 overlay 里那几个目录的直接子项**盖掉**大索引里的对应条目
//!（`super::index::find` 里那一句 `supersedes`）。
//!
//! 为什么不直接改那块大 blob：那是一块连续字节 ＋ 一排界桩，
//! 在中间插一条要搬 O(n) 字节（64 万条 ⇒ 77 MiB）。
//! 一个事件搬 77 MiB 显然不行，而 overlay 的大小上界是
//! 「几十个目录 × 各自的子项数」—— 它跟着**用户在看什么**走，不跟着盘的大小走。
//!
//! # ⚠ 它**没有**买到什么（逐条，别读宽）
//!
//! - **只盯直接子项**（`NonRecursive`）。浏览的目录**底下**那棵子树里新建的东西，
//!   仍然等重走那一档。递归挂会把 watch 数变成那棵子树的目录数 —— 上面那张表就是它的反例。
//! - **`inotify` 的 watch 绑在 inode 上，不绑路径**（本仓 08-13 一天内在三处踩过，
//!   `observe/watcher.rs` 有整段记录）。浏览的目录被删掉再重建成同名的新 inode，
//!   本模块的 watch **留在旧 inode 上**，那一格此后是瞎的，直到下一次 [`set_browsing`]。
//!   ⚠ **这一条今天没有补救**，如实登记。
//! - **事件可能丢**（内核 `inotify` 队列溢出、`IN_Q_OVERFLOW`）。溢出时本模块
//!   **不知道自己漏了什么**；兜底仍然是重走那一档。
//! - **平台不对等**，而这一格必须如实声明：Linux 走 `inotify`、Windows 走
//!   `ReadDirectoryChangesW`、macOS 走 `FSEvents` —— 三家的语义（合并、延迟、
//!   丢事件的条件）都不是一件事。逐 target 的声明住 `super::FRESHNESS`，
//!   而 `设计/96 §2.9` 边界② 逐字要求**不许把它们判成相等**。

use std::path::{Path, PathBuf};

/// 同时挂着的 watch 上限。
///
/// # 这个数是怎么定的
///
/// `设计/60 §3.5.2` 逐字「几十个 watch」。本机上限现打 **262 144**
///（`真相源/98 §3.2`）⇒ 64 相对它是 **1/4096**，也就是说：
/// 即使有 4096 个这样的后端同时跑在一台机器上，也吃不满那个上限。
///
/// ⚠ 判据那一侧**刻意用绝对量比**（`64 * 4096 <= 262144`），不写成百分比 ——
/// `设计/17 §6.9` 那条现打逼出来的纪律：**闸不许比量具自身的分辨率还细**，
/// 而 `max_user_watches` 是个可以被管理员随手改成 8192 的整数。
pub const MAX_BROWSE_WATCHES: usize = 64;

/// 一个被盯着的目录，以及它**最近一次重列**出来的直接子项（全路径原始字节）。
struct Watched {
    dir: Vec<u8>,
    children: Vec<Vec<u8>>,
}

/// 眼下盯着的那几个目录。**进程里只有这一份。**
static WATCHED: std::sync::RwLock<Vec<Watched>> = std::sync::RwLock::new(Vec::new());

/// 查询时拿的那一份 overlay（只读快照）。
pub struct Overlay {
    dirs: Vec<Vec<u8>>,
    children: Vec<Vec<u8>>,
}

impl Overlay {
    /// 大索引里这一条，是不是已经被 overlay 里某个目录的重列结果盖掉了。
    pub fn supersedes(&self, entry: &[u8]) -> bool {
        self.dirs
            .iter()
            .any(|d| super::raw::is_direct_child(entry, d))
    }

    /// overlay 自己那些条目。
    pub fn iter(&self) -> impl Iterator<Item = &[u8]> + '_ {
        self.children.iter().map(|c| c.as_slice())
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
            + self.children.iter().map(Vec::len).sum::<usize>()
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

/// 列一个目录的直接子项（全路径原始字节）。`None` = 打不开。
///
/// ⚠ 它**不递归**、不跟 symlink、不 `stat` —— 只要名字。
fn list_dir(dir: &Path) -> Option<Vec<Vec<u8>>> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut out = Vec::new();
    for e in rd {
        let Ok(e) = e else { continue };
        out.push(super::raw::path_bytes(&e.path()).to_vec());
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

/// 只给判据用：把名单清空，让每条判据从一个已知状态起跑。
pub fn forget_all() {
    if let Ok(mut g) = WATCHED.write() {
        g.clear();
    }
}

// ══════════════════════ 真的挂上去那一跳 ══════════════════════

/// 一个活着的监听器。**它一被丢掉，watch 就跟着没了** —— 所以调用方要拿住它。
///
/// 🔴 **为什么不用那个去抖器**（`observe/watcher.rs` 用的是
/// `notify-debouncer-mini`）：那一份要传一个**时间窗**进去，而本 crate 的
/// 零定时器护栏把秒级 `Duration` 构造钉成了**恰好等于登记表条数**
///（`no_timer_guard::REGISTERED_DURATION_USES`，那张表在另一棵树上、不在本件写区）。
/// ⇒ 本模块直接用裸 `notify`：**一个时间窗都不传**，事件来一条处理一条。
/// ⚠ 代价如实写：没有合并窗口 ⇒ 同一个目录连着改十次就重列十次。
/// 重列一个目录是一次 `read_dir`，代价与目录里的项数同阶、与盘的大小无关。
pub struct BrowseWatcher {
    inner: notify::RecommendedWatcher,
}

impl BrowseWatcher {
    /// 起一个监听器。事件进来 ⇒ 自动调 [`on_change`]。
    ///
    /// ⚠ 回调跑在 `notify` 自己的线程上（那条线程**不在** `no_timer_guard` 的人群里 ——
    /// 人群按「本 crate `src/` 的源码文本」画，同 `observe/watcher.rs` 那条已登记的先例）。
    pub fn start() -> Result<Self, String> {
        let inner = notify::recommended_watcher(|res: notify::Result<notify::Event>| {
            let Ok(ev) = res else { return };
            for p in ev.paths {
                let bytes = super::raw::path_bytes(&p).to_vec();
                if let Some(dir) = dir_for_event(&bytes) {
                    on_change(&dir);
                }
            }
        })
        .map_err(|e| e.to_string())?;
        Ok(Self { inner })
    }

    /// 把名单上的目录逐个挂上去。返回挂成功的个数与逐条失败原因。
    pub fn arm(&mut self) -> (usize, Vec<String>) {
        use notify::Watcher;
        let dirs: Vec<Vec<u8>> = match WATCHED.read() {
            Ok(g) => g.iter().map(|w| w.dir.clone()).collect(),
            Err(_) => Vec::new(),
        };
        let mut ok = 0usize;
        let mut failures = Vec::new();
        for d in dirs {
            let path = super::raw::to_path_buf(&d);
            match self.inner.watch(&path, notify::RecursiveMode::NonRecursive) {
                Ok(()) => ok += 1,
                Err(e) => failures.push(e.to_string()),
            }
        }
        (ok, failures)
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/files/browse_watch_tests.rs"]
mod tests;
