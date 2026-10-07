//! 常驻文件名索引：建 · 保持新鲜 · 查询。
//!
//! | 段 | 做法 | 住这里的哪一处 |
//! |---|---|---|
//! | 建索引 | 后端在那台机器上走一遍，文件名索引留在它内存里 | [`build`] · [`rebuild_once`] |
//! | 保持新鲜 | 定期重走 ＋ 只给用户正在浏览的那几个目录挂 `inotify` | 周期见 [`REWALK_INTERVAL_SECS`]；watch 那一半住 [`super::browse_watch`] |
//! | 查询 | 一条命令 → 后端内存里扫 → 只回送命中的结果 | [`find`] —— 未命中的路径一个字节都不出现在结果里 |
//!
//! # 「定期重走」的节拍不在后端进程里
//!
//! 后端生产段一个会让线程自己醒来的构件都不许有（`tests/backend/no_timer_guard.rs`）⇒ 后端只给机制，不给节拍：
//! 1. 机制：[`rebuild_once`] —— 走一遍，就一遍，做完就返回。
//! 2. 偏好：[`REWALK_INTERVAL_SECS`] —— 后端声明它建议多久重走一次，摆在 [`status`] 的回答里。
//! 3. 谁出节拍：调用方（界面那一侧）。调用方不发那条重走命令，索引就不会自己变新 —— 本 crate 的判据钉不住那一侧。
//!
//! # 索引的形状：一块字节 ＋ 一排界桩 ＋ 一排类型
//!
//! 几十万条、每条一个 `Vec<u8>` 就是几十万次分配 ＋ 每条三个机器字的簿记 ⇒ 存成一块连续字节（[`Snapshot::blob`]）
//! 加一排结束偏移（[`Snapshot::ends`]，每条 4 字节）加一排类型字节（[`Snapshot::kinds`]，每条 1 字节，`file:` / `folder:` / `ext:` 与命中的 `kind` 要它）：
//! 常驻字节 ＝ 路径总长 ＋ 5×条数，算得出，所以判据写成相等而不是范围。
//! `u32` 的界桩把索引总字节钉在 4 GiB 以内；撞上限时 [`build`] 停下并说出来（[`Stats::truncated`]），不静默丢条目。

use std::path::Path;
use std::time::SystemTime;

/// 「定期重走」的周期：能力的一部分，不是实现细节（300 秒；界面显示的是 `files-find` 回的 `index_age_secs`，客户端照它画）。
/// 住址只有这一行：客户端树里这个数零命中，由 `filewin/find.rs` 的 `no_rewalk_period_literal_lives_on_this_side` 钉着。
///
/// - 下界（硬）：周期 ≥ 建索引耗时。秤 `F2 ①` 每趟把本机现打的速率换算过来，断言本常量至少是它的 10 倍（重走占空比不超过 10%）。
/// - 上界（软，取舍）：它等于「在没挂 watch 的地方新建一个文件，最久多久能搜到」。挂了 `inotify` 的目录（用户正在浏览的）是秒级的。
/// - 不靠「全挂 watch」消掉这个数：每用户 `inotify` watch 上限（几十万）撑不住整个 home，非特权也拿不到全文件系统监听
///   （`man 2 fanotify_init`：不许 `FAN_MARK_MOUNT` / `FAN_MARK_FILESYSTEM`）。
/// 冷缓存下走一遍在机械盘上会不会把盘吵起来，没有读数。
pub const REWALK_INTERVAL_SECS: u64 = 300;

/// 冷启动首建那一趟大约要多久 —— 与周期分开钉的另一个数：周期性重走缓存是热的，后端刚起来那一趟是冷的、用户看得见。
/// 10 秒来自 `find ~ -xdev` 在冷缓存（`drop_caches=3` 之后）下的读数取上整（代理指标，NVMe · ext4、七十多万条；真索引器的冷态没量过）。
/// 是有出处的估计，界面上说「约」。经 [`status`] 交出去（[`Status::cold_first_build_secs`]），客户端一个字节都不持有它。
pub const COLD_FIRST_BUILD_SECS: u64 = 10;

/// 一次遍历的读数 —— `秤 F2 ①` 与 [`status`] 共用同一份。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// 条目数（目录 ＋ 文件 ＋ 符号链接，根自己不算）。
    pub entries: usize,
    /// 索引的**常驻字节**：路径总长 ＋ 5×条数。`秤 F2 ②`。
    pub resident_bytes: usize,
    /// 打不开的目录数。**不静默吞** —— 权限不足 / 遍历途中被删都落这里。
    pub unreadable_dirs: usize,
    /// 撞上 `u32` 界桩、提前收摊了。真的发生时 [`Snapshot::entries`] 是**不完整**的。
    pub truncated: bool,
    /// 根底下挂着的**别的文件系统**，没走进去的个数（那个目录本身照样在索引里）。
    pub skipped_mounts: usize,
}

/// 一次遍历留下来的东西。**不可变** —— 重走产出一份新的，整份换掉。
pub struct Snapshot {
    /// 全部条目的原始字节，首尾相接。
    blob: Vec<u8>,
    /// 第 i 条在 [`Snapshot::blob`] 里的**结束**偏移；起点 = 上一条的结束（第 0 条从 0 起）。
    ends: Vec<u32>,
    /// 第 i 条的类型字节（`query::KIND_*`，不跟链接）。
    kinds: Vec<u8>,
    /// 这一趟走的根（原始字节）。
    root: Vec<u8>,
    /// 走完的那一刻。
    ///
    /// 🔴 用 `SystemTime` 而不是那个单调时钟：后者在本 crate 的禁用表上
    ///（`no_timer_guard` 逐字「本 crate 里它只会用来做『距上次多久了』的节流判断」），
    /// 而新鲜度要的正是一个**能跟别人对话**的时刻 —— 界面要显示「多久前更新的」。
    built_at: SystemTime,
    unreadable_dirs: usize,
    /// 读不进去的那几个目录（原始字节，前 [`UNREADABLE_PATHS_MAX`] 个；窗口状态行「n 个目录无权限［查看］」列它们）。
    unreadable_paths: Vec<Vec<u8>>,
    truncated: bool,
    skipped_mounts: usize,
}

/// 读不进去的目录最多记几个（数照旧记全，`unreadable_dirs`）。
pub const UNREADABLE_PATHS_MAX: usize = 20;

impl Snapshot {
    pub fn entries(&self) -> usize {
        self.ends.len()
    }

    /// `秤 F2 ②`。**算得出的量**，所以判据写成相等而不是范围。
    pub fn resident_bytes(&self) -> usize {
        self.blob.len() + self.ends.len() * core::mem::size_of::<u32>() + self.kinds.len()
    }

    pub fn built_at(&self) -> SystemTime {
        self.built_at
    }

    /// 读不进去的那几个目录（前 [`UNREADABLE_PATHS_MAX`] 个，按遍历先后）。
    pub fn unreadable_paths(&self) -> &[Vec<u8>] {
        &self.unreadable_paths
    }

    pub fn root(&self) -> &[u8] {
        &self.root
    }

    pub fn stats(&self) -> Stats {
        Stats {
            entries: self.entries(),
            resident_bytes: self.resident_bytes(),
            unreadable_dirs: self.unreadable_dirs,
            truncated: self.truncated,
            skipped_mounts: self.skipped_mounts,
        }
    }

    /// 第 i 条的原始字节。
    pub fn get(&self, i: usize) -> Option<&[u8]> {
        let end = *self.ends.get(i)? as usize;
        let start = if i == 0 { 0 } else { self.ends[i - 1] as usize };
        self.blob.get(start..end)
    }

    /// 逐条走一遍。
    pub fn iter(&self) -> impl Iterator<Item = &[u8]> + '_ {
        (0..self.entries()).filter_map(|i| self.get(i))
    }

    /// 逐条走一遍，连类型字节。
    pub fn iter_kinds(&self) -> impl Iterator<Item = (&[u8], u8)> + '_ {
        (0..self.entries()).filter_map(|i| Some((self.get(i)?, *self.kinds.get(i)?)))
    }

    /// 距 `now` 过了多少秒。时钟倒退（NTP 校时）时给 `0` —— **不给负数、不 panic**。
    pub fn age_secs(&self, now: SystemTime) -> u64 {
        now.duration_since(self.built_at)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// 走一遍 `root`，产出一份 [`Snapshot`]。
///
/// 显式栈而不是递归：目录深度由用户数据说了算，几万层的树会让递归栈溢出（`SIGSEGV`，接不住）。
///
/// symlink 不跟进 ⇒ 结构上没有环：目录项的类型由 `DirEntry::file_type()` 给（Linux 上来自 `d_type`，不跟 symlink），
/// 符号链接当一条条目收进索引但不往里走。代价：指向目录的 symlink 底下那些文件不在索引里，除非它们本来也在根底下。
///
/// 不跨文件系统边界（与 `find -xdev` 同口径）：设备号走 `platform::paths::device_of`；子目录的设备号与根不同 ⇒ 这一条照样收进索引，
/// 不往里走，`skipped_mounts` 记一个数（经 `files-index-status` / `files-index-rebuild` 交出去）。非 unix 上设备号问不出 ⇒ 那一判不开口。
pub fn build(root: &Path) -> Snapshot {
    build_with(root, crate::platform::paths::device_of)
}

/// [`build`] 的本体，设备号由调用方给（判据注入「这个子目录在另一个设备上」—— 真挂载点在测试里造不出来）。
pub fn build_with(root: &Path, device_of: impl Fn(&Path) -> Option<u64>) -> Snapshot {
    let mut blob: Vec<u8> = Vec::new();
    let mut ends: Vec<u32> = Vec::new();
    let mut kinds: Vec<u8> = Vec::new();
    let mut unreadable_dirs = 0usize;
    let mut unreadable_paths: Vec<Vec<u8>> = Vec::new();
    let mut truncated = false;
    let mut skipped_mounts = 0usize;
    let root_dev = device_of(root);
    let mut stack: Vec<std::path::PathBuf> = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let rd = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => {
                unreadable_dirs += 1;
                // 根自己读不进去不算（那是「拒」，`files-index-rebuild` 回 `unreadable`）；底下的记前几个。
                if dir.as_path() != root && unreadable_paths.len() < UNREADABLE_PATHS_MAX {
                    unreadable_paths.push(super::raw::path_bytes(&dir).to_vec());
                }
                continue;
            }
        };
        for entry in rd {
            let Ok(entry) = entry else {
                continue;
            };
            let path = entry.path();
            let bytes = super::raw::path_bytes(&path);
            if blob.len() + bytes.len() > u32::MAX as usize {
                truncated = true;
                stack.clear();
                break;
            }
            blob.extend_from_slice(bytes);
            ends.push(blob.len() as u32);
            let kind = entry
                .file_type()
                .map(|t| super::query::kind_of(t.is_dir(), t.is_symlink(), t.is_file()))
                .unwrap_or(super::query::KIND_OTHER);
            kinds.push(kind);
            if kind == super::query::KIND_DIR {
                if root_dev.is_some() && device_of(&path) != root_dev {
                    skipped_mounts += 1;
                    continue;
                }
                stack.push(path);
            }
        }
    }

    blob.shrink_to_fit();
    ends.shrink_to_fit();
    kinds.shrink_to_fit();
    Snapshot {
        blob,
        ends,
        kinds,
        root: super::raw::path_bytes(root).to_vec(),
        built_at: SystemTime::now(),
        unreadable_dirs,
        unreadable_paths,
        truncated,
        skipped_mounts,
    }
}

// ══════════════════════ 常驻的那一份 ══════════════════════

/// 常驻索引。进程里只有这一份；只在内存里，重启重建（冷首建约 10 秒，热约 1 秒；要不要落盘是一道取舍题）。
static RESIDENT: std::sync::RwLock<Option<Snapshot>> = std::sync::RwLock::new(None);

/// 有没有一趟重走正在跑：非阻塞互斥用的那一个位。调用方是「用户一点就发」的节奏，两趟同时走就是双倍走整棵树；
/// 互斥、不重叠是这一类周期性后台任务的通行约束（`plocate` 的 `flock --nonblock` · Kubernetes CronJob 的 `concurrencyPolicy: Forbid`）。
static REBUILDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 被抢占过几趟 —— **只报不禁**（同上，占空比这一类是读数不是闸）。
static REBUILD_SKIPPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 判据专用的那个口 —— **只在 `cfg(test)` 下存在**。
///
/// 🔴 为什么要它：被测的性质是「**那个位是 true 的时候第二趟进不来**」，
/// 而不是「两条线程谁先」。起线程去撞它会让判据的读数依赖调度 ⇒ 变飘。
/// ⇒ 把那个位直接按住，是同一件事的**可判形态**。
/// ⚠ 代价如实记：这样就**买不到**「两条真线程同时打进来」那一格。
///
/// ⚠ **体住 `tests/`，这里只留桩**（形状）——
/// 我第一版把它内联写在这儿，`structural_scan` 那条「剖分不许回来」当场红了，**红对了**。
#[cfg(test)]
#[path = "../../../tests/backend/files/index_testing.rs"]
pub mod testing;

/// 被抢占过几趟。判据与 `status` 用它。
pub fn rebuild_skipped() -> u64 {
    REBUILD_SKIPPED.load(std::sync::atomic::Ordering::Relaxed)
}

/// 走一遍，把常驻那一份整份换掉。**就走一遍，做完返回。**
///
/// 节拍由调用方给（理由整段住本文件头注那个 🔴）。
///
/// # 🔴 回值刻意是 `Option`：**「有一趟已经在跑」与「走完了」必须分得开**
///
/// `None` = 有一趟正在跑，这一趟**没走**（计入 [`rebuild_skipped`]）。
/// 做成 `Stats::default()` 之类的「空结果」会让**没走**与**走完了但树是空的**
/// 在类型上分不开 —— 那正是本仓反复禁的那一形（「跳过」与「过了」长得一样）。
pub fn rebuild_once(root: &Path) -> Option<Stats> {
    use std::sync::atomic::Ordering;
    // 非阻塞：抢不到就**当场返回**，不排队、不阻塞调用方那一帧。
    if REBUILDING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        REBUILD_SKIPPED.fetch_add(1, Ordering::Relaxed);
        return None;
    }
    // ⚠ 从这里到清掉那个位之间**不许提前返回** —— `build` 会 panic 的话那个位就漏了。
    //   `build` 本身不 panic（它自己把 IO 错误收成计数），而这一条靠的是那个事实，
    //   不是靠这里加一层守卫。哪天 `build` 会 panic 了，这里要改成 RAII 守卫。
    let snap = build(root);
    let stats = snap.stats();
    if let Ok(mut g) = RESIDENT.write() {
        *g = Some(snap);
    }
    REBUILDING.store(false, Ordering::Release);
    Some(stats)
}

/// 「该重走了」——**一条纯函数**，声明的周期是它唯一的参数。
///
/// 抬成函数（而不是在 [`status`] 里写一个 `>`）只为一件事：让判据能对**两侧**各喂一次
///（刚好等于周期 ⇒ 不该重走 · 超过一秒 ⇒ 该重走）。
/// 写在 `status` 里的话，那条判据只能拿「刚建完 ⇒ 不 stale」这一侧喂它，
/// 而**另一侧要等 5 分钟才测得到** ⇒ 实际上永远没人行使它，
/// 那正是本仓记过五次的「负向断言没有输入就等于没有」。
pub fn is_stale(age_secs: u64) -> bool {
    age_secs > REWALK_INTERVAL_SECS
}

/// 常驻那一份被别人毒到不可用时（`RwLock` 中毒）也要**说得出话**，不 panic。
/// 常驻那一份里读不进去的那几个目录（前 [`UNREADABLE_PATHS_MAX`] 个；还没建过 ⇒ 空）。
pub fn unreadable_paths() -> Vec<Vec<u8>> {
    with_resident(|s| s.map(|s| s.unreadable_paths().to_vec()).unwrap_or_default())
}

fn with_resident<T>(f: impl FnOnce(Option<&Snapshot>) -> T) -> T {
    match RESIDENT.read() {
        Ok(g) => f(g.as_ref()),
        Err(_) => f(None),
    }
}

/// 把常驻那一份丢掉 —— 只给判据用（每条判据要从一个已知状态起跑）。
pub fn forget_resident() {
    if let Ok(mut g) = RESIDENT.write() {
        *g = None;
    }
}

// ══════════════════════ 查询 ══════════════════════

/// 一次查询要什么。
pub struct FindArgs<'a> {
    /// 解析好的搜索词（[`super::query::parse`]）。
    pub query: &'a super::query::Matcher,
    /// 这一趟只看这个目录底下（不含它自己）；`None` ⇒ [`FindArgs::home`]。
    pub under: Option<&'a [u8]>,
    /// 这台机器的家目录（`under` 没给时的范围；拿不到 ⇒ 整份索引）。
    pub home: Option<&'a [u8]>,
    /// 从第几条命中起回（前面的只数不回）。
    pub offset: usize,
    /// 这一屏最多回几条。
    pub limit: usize,
    /// 带号的那一趟：同一个搜索框来了更新的号 ⇒ 收手（[`ticket`]）。
    pub ticket: Option<&'a Ticket>,
    /// 按什么排（翻页同一个序）。
    pub sort: Sort,
}

/// 命中按哪一列排。线上词见 [`SortKey::wire`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    /// 相关度档 → 路径浅的在前（[`super::query::Matcher::rank`]）。
    #[default]
    Relevance,
    /// 名字（ASCII 不分大小写）。
    Name,
    /// 所在目录（相对搜索起点），再按名字。
    Location,
    /// 修改时间（读不到的算最早）。
    Mtime,
    /// 大小（目录与读不到的算最小）。
    Size,
}

impl SortKey {
    /// 线上那个词的闭集（判据按它对拍协议文档）。
    pub const WIRE: &'static [&'static str] = &["relevance", "name", "location", "mtime", "size"];

    pub fn wire(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Name => "name",
            Self::Location => "location",
            Self::Mtime => "mtime",
            Self::Size => "size",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        Some(match s {
            "relevance" => Self::Relevance,
            "name" => Self::Name,
            "location" => Self::Location,
            "mtime" => Self::Mtime,
            "size" => Self::Size,
            _ => return None,
        })
    }

    /// 这一列要逐条读盘（索引里只有路径与类型）。
    fn needs_meta(self) -> bool {
        matches!(self, Self::Mtime | Self::Size)
    }
}

/// 一趟的排序：哪一列 ＋ 正反。同一列里打平的按全路径字节序（正反都正着），翻页才稳。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sort {
    pub key: SortKey,
    pub desc: bool,
}

/// 一条命中在盘上的那两格（跟链接读；读不到 ⇒ 两格都 `None`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Meta {
    /// 文件的字节数；目录 ⇒ `None`。
    pub size: Option<u64>,
    pub mtime_secs: Option<u64>,
}

fn read_meta(path: &[u8]) -> Meta {
    match std::fs::metadata(super::raw::to_path_buf(path)) {
        Ok(md) => Meta {
            size: (!md.is_dir()).then(|| md.len()),
            mtime_secs: md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
        },
        Err(_) => Meta::default(),
    }
}

/// 一条命中：全路径原始字节 ＋ 类型字节 ＋ 盘上那两格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub path: Vec<u8>,
    pub kind: u8,
    pub meta: Meta,
}

/// 一次查询的答案。
///
/// 🔴 **这里只有命中** —— 「**只回送命中的结果**」，
/// 而那正是「零流量搜索」那句话的全部内容：未命中的 64 万条路径
/// **一个字节都没有离开那台机器**。
#[derive(Debug, Clone)]
pub struct FindResult {
    /// 这一屏的命中（`offset` 起、最多 `limit` 条）。
    pub hits: Vec<Hit>,
    /// 一共命中几条（**不受分页影响**）。
    pub total_hits: usize,
    /// 这一屏之后还有。
    pub truncated: bool,
    /// 这一趟扫了几条（＝索引条目数）。反空真用：扫到 0 条的「没命中」与
    /// 「索引是空的」在界面上一模一样，所以这个数必须跟着回去。
    pub scanned: usize,
    /// 答这一趟用的索引，是多久以前建的。
    pub index_age_secs: u64,
    /// 索引还没建过 ⇒ 上面几个数全是 0，而那**不是**「没搜到」。
    pub index_missing: bool,
    /// 该重走了（[`is_stale`]）。
    pub stale: bool,
    /// 手上那份索引的根（没建过 ⇒ `None`）。
    pub index_root: Option<Vec<u8>>,
    /// 这一趟的范围不在手上那份索引里（换了根之后才搜得全）。
    pub out_of_index: bool,
    /// 要搜全这一趟、重走该走哪个根：手上那份盖得住 ⇒ 它的根；否则范围在家目录里 ⇒ 家目录；否则 ⇒ 范围本身。
    pub cover_root: Option<Vec<u8>>,
}

/// 一条命中在这一趟的序里的位置：主键（看 [`Sort::key`]）→ 全路径字节序。
/// 翻到哪一屏都是这一个序（每一屏都按它从头排、再切那一段）。
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Primary {
    Relevance(u8, usize),
    Name(Vec<u8>),
    Location(Vec<u8>, Vec<u8>),
    Mtime(Option<u64>),
    Size(Option<u64>),
}

#[derive(Debug, PartialEq, Eq)]
struct Ranked {
    primary: Primary,
    desc: bool,
    path: Vec<u8>,
    kind: u8,
    meta: Option<Meta>,
}

impl Ord for Ranked {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        let p = self.primary.cmp(&o.primary);
        let p = if self.desc { p.reverse() } else { p };
        p.then_with(|| self.path.cmp(&o.path))
    }
}

impl PartialOrd for Ranked {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

fn fold(b: &[u8]) -> Vec<u8> {
    b.to_ascii_lowercase()
}

/// `path` 的父目录相对 `start` 的那一段（不在 `start` 底下 ⇒ 父目录全路径；直接在 `start` 里 ⇒ 空）。
pub fn location_of<'p>(path: &'p [u8], start: Option<&[u8]>) -> &'p [u8] {
    let path = trim_sep(path);
    let parent = match path.iter().rposition(|&b| is_sep(b)) {
        Some(0) => &path[..1],
        Some(i) => &path[..i],
        None => &path[..0],
    };
    let Some(start) = start.map(trim_sep) else {
        return parent;
    };
    if trim_sep(parent) == start {
        return &parent[..0];
    }
    if strictly_under(parent, start) {
        let mut rest = &parent[start.len()..];
        while rest.first().is_some_and(|&b| is_sep(b)) {
            rest = &rest[1..];
        }
        return rest;
    }
    parent
}

impl Ranked {
    fn new(args: &FindArgs<'_>, start: Option<&[u8]>, path: &[u8], kind: u8, depth: usize) -> Self {
        let mut meta = None;
        let primary = match args.sort.key {
            SortKey::Relevance => Primary::Relevance(args.query.rank(path), depth),
            SortKey::Name => Primary::Name(fold(name_bytes(path))),
            SortKey::Location => {
                Primary::Location(fold(location_of(path, start)), fold(name_bytes(path)))
            }
            SortKey::Mtime => {
                let m = read_meta(path);
                meta = Some(m);
                Primary::Mtime(m.mtime_secs)
            }
            SortKey::Size => {
                let m = read_meta(path);
                meta = Some(m);
                Primary::Size(m.size)
            }
        };
        debug_assert_eq!(meta.is_some(), args.sort.key.needs_meta());
        Self {
            primary,
            desc: args.sort.desc,
            path: path.to_vec(),
            kind,
            meta,
        }
    }
}

fn name_bytes(path: &[u8]) -> &[u8] {
    let path = trim_sep(path);
    match path.iter().rposition(|&b| is_sep(b)) {
        Some(i) if i + 1 < path.len() => &path[i + 1..],
        _ => path,
    }
}

/// 这一趟被同一个搜索框更新的一趟顶掉了。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Superseded;

// ── 丢弃旧查询：每个搜索框一个「最新的号」，靠号不靠计时 ─────────────────────

/// 记几个搜索框的最新号。窗口关了不会来说一声 ⇒ 有上限，满了丢最久没来的那个
/// （丢掉的那个下次来照样登记；它在飞的那一趟只是不能被提前撤，答案照旧会被窗口按号丢掉）。
pub const MAX_STREAMS: usize = 64;

/// 扫多少条看一次号。
const CHECK_EVERY: usize = 8192;

static STREAMS: std::sync::Mutex<Vec<(String, std::sync::Arc<std::sync::atomic::AtomicU64>)>> =
    std::sync::Mutex::new(Vec::new());

/// 一趟带号查询的凭据。
pub struct Ticket {
    latest: std::sync::Arc<std::sync::atomic::AtomicU64>,
    seq: u64,
}

impl Ticket {
    /// 还是这个搜索框最新的那一趟。
    pub fn current(&self) -> bool {
        self.latest.load(std::sync::atomic::Ordering::Acquire) == self.seq
    }
}

/// 登记「搜索框 `stream` 发来了第 `seq` 趟」。已经来过更大的号 ⇒ [`Superseded`]（晚到的旧号当场丢）。
/// 同号再来（往下翻页）照常放行。
pub fn ticket(stream: &str, seq: u64) -> Result<Ticket, Superseded> {
    let latest = {
        let mut g = STREAMS.lock().unwrap_or_else(|e| e.into_inner());
        let latest = match g.iter().position(|(s, _)| s == stream) {
            Some(i) => {
                let e = g.remove(i);
                let a = e.1.clone();
                g.push(e);
                a
            }
            None => {
                if g.len() >= MAX_STREAMS {
                    g.remove(0);
                }
                let a = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
                g.push((stream.to_string(), a.clone()));
                a
            }
        };
        latest
    };
    let prev = latest.fetch_max(seq, std::sync::atomic::Ordering::AcqRel);
    if prev > seq {
        return Err(Superseded);
    }
    Ok(Ticket { latest, seq })
}

fn is_sep(b: u8) -> bool {
    b == b'/' || (cfg!(windows) && b == b'\\')
}

/// 去掉末尾的分隔符（根 `/` 自己留着）。
fn trim_sep(p: &[u8]) -> &[u8] {
    let mut n = p.len();
    while n > 1 && is_sep(p[n - 1]) {
        n -= 1;
    }
    &p[..n]
}

/// `a` 就是 `b`，或在 `b` 底下。
pub fn within(a: &[u8], b: &[u8]) -> bool {
    let (a, b) = (trim_sep(a), trim_sep(b));
    if b.last().is_some_and(|&c| is_sep(c)) {
        return a.starts_with(b);
    }
    a == b || (a.len() > b.len() && a.starts_with(b) && is_sep(a[b.len()]))
}

/// `a` 在 `b` 底下（不含 `b` 自己）。
fn strictly_under(a: &[u8], b: &[u8]) -> bool {
    within(a, b) && trim_sep(a) != trim_sep(b)
}

/// 在常驻索引里查。
///
/// ⚠ **它不重走、不阻塞**：拿的是手上这一份，并把它的年龄与「该不该重走、走哪个根」一起交回去。
/// 要更新的一方自己发重走那条命令 —— 见本文件头注那个 🔴。
pub fn find(args: &FindArgs<'_>) -> Result<FindResult, Superseded> {
    let now = SystemTime::now();
    let domain = args.under.or(args.home);
    let in_home = |d: &[u8]| args.home.is_some_and(|h| within(d, h));
    let fresh_root = |d: Option<&[u8]>| -> Option<Vec<u8>> {
        match d {
            Some(d) if !in_home(d) => Some(trim_sep(d).to_vec()),
            _ => args.home.map(|h| trim_sep(h).to_vec()),
        }
    };
    with_resident(|snap| {
        let Some(snap) = snap else {
            return Ok(FindResult {
                hits: Vec::new(),
                total_hits: 0,
                truncated: false,
                scanned: 0,
                index_age_secs: 0,
                index_missing: true,
                stale: false,
                index_root: None,
                out_of_index: false,
                cover_root: fresh_root(domain),
            });
        };
        let root = snap.root();
        let covered = domain.is_none_or(|d| within(d, root));
        // 范围就是根 ⇒ 不用逐条比前缀。
        let scope = domain.filter(|d| trim_sep(d) != trim_sep(root));
        let overlay = super::browse_watch::overlay_snapshot();
        // 按相关度排（[`Ranked`]），只留到这一屏的末尾那么多条（大顶堆，堆顶是留下的里面最靠后的那条）。
        let keep = args.offset.saturating_add(args.limit);
        let mut best: std::collections::BinaryHeap<Ranked> = std::collections::BinaryHeap::new();
        let mut total = 0usize;
        let mut scanned = 0usize;
        let start = domain.map(trim_sep);
        let mut take = |bytes: &[u8], kind: u8| {
            if scope.is_some_and(|d| !strictly_under(bytes, d)) {
                return;
            }
            if args.query.matches(bytes, kind) {
                total += 1;
                if keep == 0 {
                    return;
                }
                let depth = bytes.iter().filter(|&&b| is_sep(b)).count();
                let r = Ranked::new(args, start, bytes, kind, depth);
                if best.len() < keep {
                    best.push(r);
                } else if let Some(mut worst) = best.peek_mut() {
                    // 比留下的最靠后那条靠前 ⇒ 顶替它（放手时堆自己重排）。
                    if r < *worst {
                        *worst = r;
                    }
                }
            }
        };
        for (bytes, kind) in snap.iter_kinds() {
            scanned += 1;
            if scanned % CHECK_EVERY == 0 && args.ticket.is_some_and(|t| !t.current()) {
                return Err(Superseded);
            }
            // 被 watch 盖住的那几个目录：它们的直接子项由 overlay 那一份说话
            //（那一份是事件驱动的、比这一趟遍历新）。
            if overlay.supersedes(bytes) {
                continue;
            }
            take(bytes, kind);
        }
        for (bytes, kind) in overlay.iter() {
            scanned += 1;
            take(bytes, kind);
        }
        if args.ticket.is_some_and(|t| !t.current()) {
            return Err(Superseded);
        }
        let hits: Vec<Hit> = best
            .into_sorted_vec()
            .into_iter()
            .skip(args.offset)
            .map(|r| Hit {
                meta: r.meta.unwrap_or_else(|| read_meta(&r.path)),
                path: r.path,
                kind: r.kind,
            })
            .collect();
        let age = snap.age_secs(now);
        Ok(FindResult {
            truncated: total > args.offset.saturating_add(hits.len()),
            hits,
            total_hits: total,
            scanned,
            index_age_secs: age,
            index_missing: false,
            stale: is_stale(age),
            index_root: Some(root.to_vec()),
            out_of_index: !covered,
            cover_root: if covered {
                Some(root.to_vec())
            } else {
                fresh_root(domain)
            },
        })
    })
}

/// `files.index.status` 的答案。
///
/// 🔴 **[`Status::rewalk_interval_secs`] 在这里不是装饰** ——
/// 边界③ 逐字：「『定期重走』的周期是能力的一部分，不是实现细节……
/// 它必须**可查询**，不许只活在代码里」。这个字段就是那句话的兑现处。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub index_missing: bool,
    pub entries: usize,
    pub resident_bytes: usize,
    pub unreadable_dirs: usize,
    pub truncated: bool,
    /// 没走进去的挂载点个数（[`Stats::skipped_mounts`]）。
    pub skipped_mounts: usize,
    /// 距上次走完过了多少秒。界面拿它显示「多久前更新的」。
    pub age_secs: u64,
    /// 后端**声明**的重走周期（[`REWALK_INTERVAL_SECS`]）。
    pub rewalk_interval_secs: u64,
    /// `age_secs > rewalk_interval_secs` —— 「该重走了」。
    ///
    /// ⚠ 它是一句**判断**，不是一个动作：后端不会因为它变 `true` 就自己走
    ///（本文件头注那个 🔴）。
    pub stale: bool,
    /// 眼下挂着 `inotify` 的目录数（保鲜的另一半）。
    pub browse_watches: usize,
    /// 挂 watch 的上限（[`super::browse_watch::MAX_BROWSE_WATCHES`]）。
    pub browse_watch_cap: usize,
    /// 后端**声明**的冷启动首建大约要几秒（[`COLD_FIRST_BUILD_SECS`]）。界面在首建那一趟里显示它。
    pub cold_first_build_secs: u64,
}

pub fn status() -> Status {
    let now = SystemTime::now();
    with_resident(|snap| {
        let (missing, s, age) = match snap {
            Some(snap) => (false, snap.stats(), snap.age_secs(now)),
            None => (
                true,
                Stats {
                    entries: 0,
                    resident_bytes: 0,
                    unreadable_dirs: 0,
                    truncated: false,
                    skipped_mounts: 0,
                },
                0,
            ),
        };
        Status {
            index_missing: missing,
            entries: s.entries,
            resident_bytes: s.resident_bytes,
            unreadable_dirs: s.unreadable_dirs,
            truncated: s.truncated,
            skipped_mounts: s.skipped_mounts,
            age_secs: age,
            rewalk_interval_secs: REWALK_INTERVAL_SECS,
            stale: !missing && is_stale(age),
            browse_watches: super::browse_watch::watched_count(),
            browse_watch_cap: super::browse_watch::MAX_BROWSE_WATCHES,
            cold_first_build_secs: COLD_FIRST_BUILD_SECS,
        }
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/files/index_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/files/scale_f2.rs"]
mod scale_f2;
