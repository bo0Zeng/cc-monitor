//! **常驻文件名索引** —— 那张三段表的实现。
//!
//! # 三段，逐段对着设计
//!
//! | 段 | 设计逐字 | 住这里的哪一处 |
//! |---|---|---|
//! | **建索引** | 「后端在那台机器上走一遍，文件名索引留在它内存里」 | [`build`] · [`rebuild_once`] |
//! | **保持新鲜** | 「定期重走 ＋ 只给用户正在浏览的那几个目录挂 `inotify`」 | 周期那一半见下面那个 🔴；挂 watch 那一半住 [`super::browse_watch`] |
//! | **查询** | 「一条命令 → 后端内存里扫 → **只回送命中的结果**」 | [`find`] —— 它交出去的是命中，**未命中的路径一个字节都不出现在结果里** |
//!
//! # 🔴 「定期重走」那一半：**节拍不在后端进程里，而这不是偷懒**
//!
//! 写的是「后端**定期重走**」。而本 crate 有一条更硬的铁律
//! 与它正面相撞：**后端生产段一个会让线程自己醒来的构件都不许有**
//! （`tests/backend/no_timer_guard.rs`，之前就在盘上）。
//! 睡到点自己醒、节拍器、带超时的等待 —— 逐条在那张禁用表上。
//!
//! ⇒ 本族的处置与 `capture-pane` 那两条原语**逐字同一条**（`inbound.rs` 那段注释）：
//! 「**只做一次**……『隔多久再抓一次』留在调用方（`K37`：后端只给机制，不给偏好）」。
//!
//! 具体落成三件，缺一件这条边界就会被读宽：
//!
//! 1. **机制**：[`rebuild_once`] —— 走一遍，就一遍，做完就返回。
//! 2. **偏好**：[`REWALK_INTERVAL_SECS`] —— 后端**声明**它建议多久重走一次，
//!    并把这个数摆在 [`status`] 的回答里（边界③ 逐字要求它可查询）。
//! 3. **谁出节拍**：调用方（界面那一侧）。那一侧本来就有正当的周期行为，
//!    `no_timer_guard` 的射程也逐字写着「**只钉 backend crate**……monitor 侧另有自己的轮询纪律」。
//!
//! ⚠ **用户看得见的那条性质一个字没少**：「新建的文件最多 `REWALK_INTERVAL_SECS`
//! 秒之后能搜到」仍然成立，而且 [`status`] 让界面**算得出**已经过了多久
//!（要求那个延迟显示在界面上）。
//! ⚠ **少掉的那一格如实说**：如果调用方**不发**那条重走命令，索引就不会自己变新 ——
//! 而「调用方到底发不发」这件事**本 crate 的判据钉不住**（它在另一棵树上）。
//! 这是一条真实的边界，不是纸面上的。
//!
//! # 索引的形状：一块字节 ＋ 一排界桩
//!
//! 64 万条、平均路径长 126 字节（现打）⇒ 每条一个 `Vec<u8>`
//! 就是 64 万次分配 ＋ 每条三个机器字的簿记。这里存成**一块连续字节**
//!（[`Snapshot::blob`]）加**一排结束偏移**（[`Snapshot::ends`]，每条 4 字节）：
//! 常驻字节 ＝ 路径总长 ＋ 4×条数，**算得出、量得准**，所以 `秤 F2 ②` 那一格
//! 能写成一条**相等**断言而不是一个范围。
//!
//! ⚠ `u32` 的界桩把索引总字节钉在 4 GiB 以内。撞上限时 [`build`] **停下并说出来**
//!（[`Stats::truncated`]），不静默丢条目 —— 静默丢会让「搜不到」变成一个查不出原因的现象。

use std::path::Path;
use std::time::SystemTime;

/// 🔴 **「定期重走」的周期 —— 它是能力的一部分，不是实现细节**（边界③ 逐字）。
///
/// # 🔴用户拍板：**按现值 300 秒发**
///
/// 用户 2026-09-22 那一轮裁「按推荐来」，推荐原文逐字：
/// 「**先按现值 300 秒发，界面上把它显示出来**」。
/// ⇒ 下面那一整节（下界现打 · 上界取舍 · 为什么不靠全挂 watch）**就是那个「现值」的依据**，
/// 它从「本件自己定的一个数」升格成**已裁的产品参数**。逐字没改一个字，因为它一条都没过期。
///
/// ⚠ 裁决的另一半（「界面上把它显示出来」）落在**客户端**：
/// `src/frontend/filewin/src/find.rs::freshness_line`，由那一侧一条从 egui 的 galley 里
/// 把数读回来、而且**喂两组不同的数**的判据钉着。
/// ⚠ 而「只有一个住址」是**这一行**：客户端树里那个数零命中、后端树里这个声明恰好一处，
/// 两向由 `filewin/find.rs` 那条 `no_rewalk_period_literal_lives_on_this_side` 钉着
/// （不许前后端各写一份）。
/// ⇒ **改这个数只要改这一行**，客户端一个字节都不用动。
///
/// # 这个数是怎么定的（「**没定**」，本件定它）
///
/// **下界（硬）**：「重走周期 **≥** 建索引耗时」。
/// 建一次索引的现打代理指标是：64 万条 **0.99 秒**（热缓存）。
/// 秤 `F2 ①` 每跑一趟都会把本机的**现打**速率换算到那个量纲上，
/// 并断言本常量至少是它的 10 倍 —— 也就是**重走的占空比不超过 10%**。
/// ⇒ 300 秒对 ~1 秒的下界留了两个数量级余量；哪天机器慢 30 倍，那条断言才会开口。
///
/// **上界（软，是取舍不是事实）**：它等于「在**没挂 watch** 的地方新建一个文件，
/// 最久多久能搜到」。挂了 `inotify` 的那几个目录（用户正在浏览的，见
/// [`super::browse_watch`]）是**秒级**的，本常量只作用于**别处**。
/// ⇒ 5 分钟买的是「眼前那一块实时、远处那一块 5 分钟内收敛」。
///
/// **为什么不靠「全挂 watch」把这个数消掉** —— 现打，不是推论：
/// 本机 `inotify` 的每用户 watch 上限是 **262 144**，而 home 底下 **640 413** 条目
///（两行现打）⇒ 光目录数就可能撑爆，而且非特权拿不到
/// 全文件系统监听（`man 2 fanotify_init` 一手：不许 `FAN_MARK_MOUNT` /
/// `FAN_MARK_FILESYSTEM`）。**所以这一档必须存在。**
///
/// ⚠ **它今天判不了的那一格**：冷缓存下走一遍要多久没人量过（`drop_caches` 要 root）。
/// 如果冷档真的是几十秒，这个数的**下界**仍然成立（300 ≫ 30），但「重走会不会
/// 在机械盘上把盘吵起来」这件事**没有读数**。`秤 F2` 的冷档那一格逐字登记着这件事。
pub const REWALK_INTERVAL_SECS: u64 = 300;

/// 🔴 **冷启动首建那一趟大约要多久** —— 与上面那个周期**分开钉的另一个数**（用户 2026-09-24 裁
/// 「单列一个数并在搜索界面显示」）。
///
/// # 为什么要单列
///
/// 上面那个周期的下界（秤 `F2`）用的是**热**缓存的数 —— 周期性重走每 300 秒一趟，缓存不会凉。
/// 而**后端刚起来那一趟**是冷的：目录项与 inode 元数据都不在缓存里。那一趟用户**看得见**
/// （搜索框里敲了字、结果要等它走完），所以界面要能先说一句「大概要等多久」。
///
/// # 这个数是怎么来的（逐条，别读宽）
///
/// - 冷缓存现打（`drop_caches=3` 之后）：**8.88 / 9.44 / 9.63 秒**，
///   739 782 条 · NVMe · ext4；同日热基线 0.56–0.73 秒。取最大那个的上整 ⇒ **10**。
/// - ⚠ 那是 `find ~ -xdev` 的读数（**代理指标**），不是本文件 [`build`] 的读数；
///   真索引器的冷态**没量过**（`drop_caches` 要 root）。
/// - ⚠ 只量了一台机器、一个 home；机械盘 / 别的文件系统 / 条目多一个数量级 —— 都没有读数。
///
/// ⇒ 它是一个**有出处的估计**，界面上说「约」；**不是**这台机器的实测。
/// ⚠ 它经 [`status`] 交出去（[`Status::cold_first_build_secs`]），客户端一个字节都不持有它
/// （住址唯一由 `filewin/find.rs` 那条 `no_rewalk_period_literal_lives_on_this_side` 一并钉着）。
pub const COLD_FIRST_BUILD_SECS: u64 = 10;

/// 一次遍历的读数 —— `秤 F2 ①` 与 [`status`] 共用同一份。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// 条目数（目录 ＋ 文件 ＋ 符号链接，根自己不算）。
    pub entries: usize,
    /// 索引的**常驻字节**：路径总长 ＋ 4×条数。`秤 F2 ②`。
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
    /// 这一趟走的根（原始字节）。
    root: Vec<u8>,
    /// 走完的那一刻。
    ///
    /// 🔴 用 `SystemTime` 而不是那个单调时钟：后者在本 crate 的禁用表上
    ///（`no_timer_guard` 逐字「本 crate 里它只会用来做『距上次多久了』的节流判断」），
    /// 而新鲜度要的正是一个**能跟别人对话**的时刻 —— 界面要显示「多久前更新的」。
    built_at: SystemTime,
    unreadable_dirs: usize,
    truncated: bool,
    skipped_mounts: usize,
}

impl Snapshot {
    pub fn entries(&self) -> usize {
        self.ends.len()
    }

    /// `秤 F2 ②`。**算得出的量**，所以判据写成相等而不是范围。
    pub fn resident_bytes(&self) -> usize {
        self.blob.len() + self.ends.len() * core::mem::size_of::<u32>()
    }

    pub fn built_at(&self) -> SystemTime {
        self.built_at
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

    /// 距 `now` 过了多少秒。时钟倒退（NTP 校时）时给 `0` —— **不给负数、不 panic**。
    pub fn age_secs(&self, now: SystemTime) -> u64 {
        now.duration_since(self.built_at)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// 走一遍 `root`，产出一份 [`Snapshot`]。
///
/// # 为什么是显式栈而不是递归
///
/// 递归的深度等于目录深度，而目录深度是**用户数据**说了算的 ——
/// 一棵被谁 `mkdir -p` 出几万层的树会让这条路 stack overflow，
/// 而栈溢出在 Rust 里是 `SIGSEGV`/`SIGABRT`，**不是一个能被上层接住的错误**。
///
/// # symlink 不跟进 ⇒ 结构上没有环
///
/// 目录项的类型由 `DirEntry::file_type()` 给（Linux 上直接来自 `d_type`，
/// **不跟 symlink**）。符号链接**当一条条目收进索引**，但不往里走
/// ⇒ 不需要 inode 去重，也不可能绕环。
/// ⚠ 代价如实写：指向目录的 symlink 底下那些文件**不在索引里**，
/// 除非它们本来也在根底下。
///
/// # 不跨文件系统边界
///
/// 这里原来写着「本实现**没有**那一档 —— 拿到设备号要走平台扩展 trait，而那是 `platform/` 的地盘」。
/// 今天有了：设备号走 `platform::paths::device_of`，与那趟现打的 `find ~ -xdev` 同口径 ——
/// 子目录的设备号与根不同 ⇒ **这一条照样收进索引**（它是根底下的一个名字），**不往里走**，`skipped_mounts` 记一个数
/// （经 `files-index-status` / `files-index-rebuild` 交出去，不静默少走）。
/// ⚠ 非 unix 上设备号问不出 ⇒ 那一判不开口（全当同一个文件系统），如实登记。
pub fn build(root: &Path) -> Snapshot {
    build_with(root, crate::platform::paths::device_of)
}

/// [`build`] 的本体，设备号由调用方给（判据注入「这个子目录在另一个设备上」—— 真挂载点在测试里造不出来）。
pub fn build_with(root: &Path, device_of: impl Fn(&Path) -> Option<u64>) -> Snapshot {
    let mut blob: Vec<u8> = Vec::new();
    let mut ends: Vec<u32> = Vec::new();
    let mut unreadable_dirs = 0usize;
    let mut truncated = false;
    let mut skipped_mounts = 0usize;
    let root_dev = device_of(root);
    let mut stack: Vec<std::path::PathBuf> = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let rd = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => {
                unreadable_dirs += 1;
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
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
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
    Snapshot {
        blob,
        ends,
        root: super::raw::path_bytes(root).to_vec(),
        built_at: SystemTime::now(),
        unreadable_dirs,
        truncated,
        skipped_mounts,
    }
}

// ══════════════════════ 常驻的那一份 ══════════════════════

/// 常驻索引。**进程里只有这一份** —— 「常驻」就是它的全部意思。
///
/// ⚠ **只在内存里，重启重建**（末尾那一问今天的答案）。
///
/// 🔴 **〔2026-09-21 订正：上一版那句「今天量不了」已经假了〕**
/// 原文写着「它要的证据是**冷缓存**下重建要多久，而 `drop_caches` 要 root
/// ⇒ 秤 `F2` 的冷档今天量不了」。**那一格量到了**：
/// 冷 **8.88 / 9.44 / 9.63 秒**（739 782 条 · NVMe · ext4），同日热基线 **0.56–0.73 秒**
/// ⇒ **缓存效应 13–17 倍**。
/// ⇒ 「要不要落盘」那一问现在**有证据可依**了，但**本件仍然没答它** ——
/// 它是一道取舍题（省 9 秒的首建 vs 多一份要保鲜的盘上状态），归设计。
/// ⚠ 另记一条外部对照：Everything 在**拿不到变更流**的那一档
/// （网络共享 / 任意目录 ＝ **我们这一档**）也是**索引全在内存、只在退出时落盘**。
static RESIDENT: std::sync::RwLock<Option<Snapshot>> = std::sync::RwLock::new(None);

/// 🔴 **有没有一趟重走正在跑。** 非阻塞互斥用的那一个位。
///
/// # 为什么要它（2026-09-21 外部调研逼出来的）
///
/// 本函数走的是**一整棵树**。在它之前**零并发保护** —— 两趟同时调就是两趟都走整棵树，
/// 而调用方（界面那一侧）恰恰是「用户一点就发」那种节奏 ⇒ 连点两下就双倍开销。
///
/// 而「**互斥、不重叠**」正是这一类周期性后台任务在业界**唯一被代码硬校验**的那条约束
///（现打：`plocate`/`mlocate` 用 `flock --nonblock` ·
/// Kubernetes CronJob 的 `concurrencyPolicy: Forbid` ·
/// Prometheus 直接把「单次预算 > 周期」判成配置错误）。
/// ⚠ 那一篇同时查实：**「周期 = 单次耗时 × N」这种形式的推荐值，业界没有公认值**
///（六个方向各自查完都没有）⇒ 所以本族**不再**拿那个比值当闸，见 `scale_f2` 里那三条。
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
#[derive(Debug, Clone)]
pub struct FindArgs {
    /// 要找的**字节**子串。空串 = 匹配一切（用来数总条目）。
    pub needle: Vec<u8>,
    /// ASCII 段大小写不敏感。
    pub ignore_ascii_case: bool,
    /// 最多回送几条。
    pub limit: usize,
}

/// 一次查询的答案。
///
/// 🔴 **这里只有命中** —— 「**只回送命中的结果**」，
/// 而那正是「零流量搜索」那句话的全部内容：未命中的 64 万条路径
/// **一个字节都没有离开那台机器**。
#[derive(Debug, Clone)]
pub struct FindResult {
    /// 命中的原始字节，最多 [`FindArgs::limit`] 条。
    pub hits: Vec<Vec<u8>>,
    /// 一共命中几条（**不受 `limit` 影响**）。
    pub total_hits: usize,
    /// 因为 `limit` 而没回送全部。
    pub truncated: bool,
    /// 这一趟扫了几条（＝索引条目数）。反空真用：扫到 0 条的「没命中」与
    /// 「索引是空的」在界面上一模一样，所以这个数必须跟着回去。
    pub scanned: usize,
    /// 答这一趟用的索引，是多久以前建的。
    pub index_age_secs: u64,
    /// 索引还没建过 ⇒ 上面几个数全是 0，而那**不是**「没搜到」。
    pub index_missing: bool,
}

/// 在常驻索引里查。
///
/// ⚠ **它不重走、不阻塞**：拿的是手上这一份，并把它的年龄一起交回去
///（「那个延迟要显示在界面上」）。
/// 要更新的一方自己发重走那条命令 —— 见本文件头注那个 🔴。
pub fn find(args: &FindArgs) -> FindResult {
    let now = SystemTime::now();
    with_resident(|snap| {
        let Some(snap) = snap else {
            return FindResult {
                hits: Vec::new(),
                total_hits: 0,
                truncated: false,
                scanned: 0,
                index_age_secs: 0,
                index_missing: true,
            };
        };
        let overlay = super::browse_watch::overlay_snapshot();
        let mut hits: Vec<Vec<u8>> = Vec::new();
        let mut total = 0usize;
        let mut scanned = 0usize;
        let take = |bytes: &[u8], hits: &mut Vec<Vec<u8>>, total: &mut usize| {
            if super::raw::contains(bytes, &args.needle, args.ignore_ascii_case) {
                *total += 1;
                if hits.len() < args.limit {
                    hits.push(bytes.to_vec());
                }
            }
        };
        for bytes in snap.iter() {
            scanned += 1;
            // 被 watch 盖住的那几个目录：它们的直接子项由 overlay 那一份说话
            //（那一份是事件驱动的、比这一趟遍历新）。
            if overlay.supersedes(bytes) {
                continue;
            }
            take(bytes, &mut hits, &mut total);
        }
        for bytes in overlay.iter() {
            scanned += 1;
            take(bytes, &mut hits, &mut total);
        }
        FindResult {
            truncated: total > hits.len(),
            hits,
            total_hits: total,
            scanned,
            index_age_secs: snap.age_secs(now),
            index_missing: false,
        }
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
