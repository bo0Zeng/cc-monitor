//! 常驻索引的行为判据 —— **在真盘上真走一遍**，不是源码扫描。
//!
//! # 🔴 主锚全部是**相等**断言，不是地板
//!
//! 「反空真」那条纪律逐字：绿必须来自相等断言，地板在「变少」方向上是瞎的。
//! 本族的两个主锚都算得出确切值，所以都写成相等：
//!
//! | 锚 | 相等的两侧 |
//! |---|---|
//! | 条目数 | 索引里的条数 == 夹具**按构造**造了多少条 |
//! | 常驻字节 | [`Snapshot::resident_bytes`] == 路径总长（算出来的）＋ 5×条数（界桩 4 ＋ 类型 1）|
//!
//! 时延那一类量不出确切值 ⇒ 它们**不在这里**，在 `秤 F2`（`scale_f2.rs`），
//! 而那边的主锚同样是上面这两个相等，墙钟只作为**读数**报出去。
//!
//! # 语料
//!
//! **全合成**，现造现删（数据源纪律：测试夹具不许含真会话正文、
//! 不许锚在活体上）。本族一个字节都不读用户的真目录。

use super::*;

/// ★ 常驻那一份是**进程级的一个 static** ⇒ 同一进程里并行跑的判据会互相踩。
///
/// 🔴 不加这把锁会怎样（这不是假想，是这一族的结构性事实）：`cargo test` 默认多线程，
/// 两条判据各自 [`rebuild_once`] 一棵不同的树 ⇒ 后一条把前一条的索引整份换掉，
/// 前一条的条目数断言拿到的是**别人那棵树**的数。而那种红是**间歇性**的，
/// 一次绿一次红，最省事的消法是把断言调松 —— 正是本仓反复治的那条路。
///
/// ⇒ 凡是碰常驻那一份的判据，**第一行就拿这把锁**。
///
/// ⚠ 它顺手把**两份**常驻状态都清空（索引 ＋ 浏览目录的 overlay）：
/// 那两份一起构成「后端此刻手上的那份真相」，只清一半会让上一条判据挂的 watch
/// 在下一条判据的查询里冒出来 —— 那种红同样是间歇性的。
pub(crate) fn resident_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    // 中毒了也要能接着跑：一条判据 panic 不该让其余的全部变成「毒锁」红。
    let g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    forget_resident();
    if let Ok(mut w) = super::super::browse_watch::WATCHED.write() {
        w.clear();
    }
    g
}

/// 把常驻那一份清掉（判据之间复位用）。
fn forget_resident() {
    if let Ok(mut g) = RESIDENT.write() {
        *g = None;
    }
}

/// 在常驻索引里按一条搜索词查（不带号、不限范围、从头起）。
pub(crate) fn query(q: &str, limit: usize) -> FindResult {
    let m = super::super::query::parse(q).expect("判据里的搜索词本该解析得了");
    find(&FindArgs {
        query: &m,
        under: None,
        home: None,
        offset: 0,
        limit,
        ticket: None,
        sort: Sort::default(),
    })
    .expect("没带号的一趟不会被顶掉")
}

/// 一棵**按构造知道自己有多少条**的合成树。
pub(crate) struct Fixture {
    pub root: std::path::PathBuf,
    /// 条目数（目录 ＋ 文件，根自己不算）。
    pub entries: usize,
    /// 全部条目的**路径字节总长**（含根前缀）。
    pub path_bytes_total: usize,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

/// 造一棵树：`dirs` 个子目录，每个里面 `files` 个文件。
///
/// 名字**定长**，所以字节总长是算出来的、不是量出来的 ——
/// 这一条正是上面那张表里「常驻字节」那个相等断言成立的前提。
pub(crate) fn make_tree(tag: &str, dirs: usize, files: usize, name_pad: usize) -> Fixture {
    let root = std::env::temp_dir().join(format!("ccm-24f-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("造夹具根");
    let root_len = super::super::raw::path_bytes(&root).len();

    let mut entries = 0usize;
    let mut total = 0usize;
    for d in 0..dirs {
        let dname = format!("d{d:04}");
        let dir = root.join(&dname);
        std::fs::create_dir_all(&dir).expect("造夹具子目录");
        entries += 1;
        total += root_len + 1 + dname.len();
        for f in 0..files {
            let fname = format!("f{f:04}{}", "x".repeat(name_pad));
            std::fs::File::create(dir.join(&fname)).expect("造夹具文件");
            entries += 1;
            total += root_len + 1 + dname.len() + 1 + fname.len();
        }
    }
    Fixture {
        root,
        entries,
        path_bytes_total: total,
    }
}

// ══════════════════════ 建索引那一段 ══════════════════════

#[test]
fn a_walk_indexes_exactly_the_entries_the_fixture_built() {
    let _lock = resident_lock();
    let fx = make_tree("count", 7, 11, 0);
    let snap = build(&fx.root);
    assert_eq!(
        snap.entries(),
        fx.entries,
        "索引里的条数与夹具按构造造的条数对不上 —— 遍历漏了或多了"
    );
    assert_eq!(
        snap.stats().unreadable_dirs,
        0,
        "这棵树是我们自己刚造的，不该有打不开的目录"
    );
    assert!(!snap.stats().truncated, "这么小一棵树不可能撞上界桩上限");
}

/// 🔴 `秤 F2 ②` 的**判据那一半**：常驻字节是算得出的量。
#[test]
fn resident_bytes_equals_the_paths_plus_five_bytes_of_bookkeeping_per_entry() {
    let _lock = resident_lock();
    let fx = make_tree("bytes", 5, 9, 17);
    let snap = build(&fx.root);
    assert_eq!(
        snap.resident_bytes(),
        fx.path_bytes_total + 5 * fx.entries,
        "常驻字节不等于「路径总长 ＋ 5×条数」——\n\
         要么遍历少收了条目，要么这个量的口径变了（变了就回来改这条断言，别调松它）"
    );
}

#[test]
fn an_unreadable_root_is_counted_and_not_swallowed() {
    let _lock = resident_lock();
    let missing = std::env::temp_dir().join(format!(
        "ccm-24f-nope-{}-{}",
        std::process::id(),
        "does-not-exist"
    ));
    std::fs::remove_dir_all(&missing).ok();
    let snap = build(&missing);
    assert_eq!(snap.entries(), 0, "一个不存在的根不该产出条目");
    assert_eq!(
        snap.stats().unreadable_dirs,
        1,
        "打不开的目录没有被数进去 —— 静默吞掉会让「搜不到」变成查不出原因的现象"
    );
}

/// 读不进去的子目录：数照记，**名字也记**（前 [`UNREADABLE_PATHS_MAX`] 个，窗口「n 个目录无权限［查看］」列它们）；
/// 根自己读不进去不进名单（那是「拒」）。期望手写：造 23 个锁死的子目录 ⇒ 数 23、名单 20 个、都在根底下。
#[cfg(unix)]
#[test]
fn unreadable_subdirs_are_named_up_to_the_cap() {
    use std::os::unix::fs::PermissionsExt as _;
    let _lock = resident_lock();
    let root = std::env::temp_dir().join(format!("ccm-unreadable-names-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let mut locked = Vec::new();
    for k in 0..23 {
        let d = root.join(format!("locked-{k:02}"));
        std::fs::create_dir_all(&d).unwrap();
        locked.push(d);
    }
    std::fs::create_dir_all(root.join("open")).unwrap();
    std::fs::write(locked[0].join("probe"), b"x").unwrap();
    for d in &locked {
        std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    // 前提：这一趟不是以 root 跑（root 进得了 000 的目录，这一格就判不了）—— 看里面那一份还 stat 不 stat 得到。
    let can_read = std::fs::metadata(locked[0].join("probe")).is_ok();
    let snap = build(&root);
    for d in &locked {
        std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o755)).ok();
    }
    std::fs::remove_dir_all(&root).ok();
    assert!(
        !can_read,
        "以 root 跑的：000 的目录照样读得进，这一格判不了"
    );
    assert_eq!(snap.stats().unreadable_dirs, 23);
    let names = snap.unreadable_paths();
    assert_eq!(names.len(), UNREADABLE_PATHS_MAX, "名单没按上限截");
    assert_eq!(UNREADABLE_PATHS_MAX, 20);
    let root_bytes = root.to_string_lossy().to_string();
    for n in names {
        let n = String::from_utf8_lossy(n);
        assert!(
            n.starts_with(&root_bytes) && n.contains("/locked-"),
            "名单里有不是那几个锁死目录的：{n}"
        );
    }
    // 根读不进去（不存在）⇒ 数一个、名单空。
    let missing = root.join("gone");
    let snap = build(&missing);
    assert_eq!(snap.stats().unreadable_dirs, 1);
    assert!(snap.unreadable_paths().is_empty(), "根自己进了名单");
}

#[test]
fn a_symlink_is_indexed_but_not_followed() {
    let _lock = resident_lock();
    // 只在 unix 上造 symlink：这一格是**平台行为**，Windows 上造链接要另一套权限。
    // ⚠ 本文件住 `tests/backend/`，不在 `cfgless_guard` 的人群里（它扫 `src/`）。
    #[cfg(unix)]
    {
        let fx = make_tree("symlink", 2, 2, 0);
        let link = fx.root.join("loop-back");
        std::os::unix::fs::symlink(&fx.root, &link).expect("造 symlink");
        let snap = build(&fx.root);
        // 夹具的条目 ＋ 那一条链接自己。跟进去的话条数会爆（或者死循环）。
        assert_eq!(
            snap.entries(),
            fx.entries + 1,
            "symlink 被跟进去了 —— 那一跳会让指回自己的链接把遍历带进环里"
        );
    }
    #[cfg(not(unix))]
    {
        // 🔴 这一格在非 unix 上**判不了**，而「判不了」与「过了」必须分得开。
        // 缺的证据：一台 Windows 机器上造一条目录联结（junction）再走一遍。
        eprintln!("[24f] symlink 那一格在本 target 上判不了：造链接要另一套权限，本轮没有真机");
    }
}

// ══════════════════════ 保鲜那一段（周期那一半）══════════════════════

#[test]
fn the_declared_rewalk_interval_is_what_status_reports() {
    let _lock = resident_lock();
    let fx = make_tree("status", 2, 3, 0);
    // 🔴 这个位是**进程级**的，而 `cargo test` 默认并行 ⇒ 先串行（理由住 `index_testing.rs`）
    let _serial = crate::files::index::testing::serial();
    rebuild_once(&fx.root)
        .expect("本格独占跑，抢不到那个位就是并发保护写错了 —— 不许静默当成走过了");
    let s = status();
    assert_eq!(
        s.rewalk_interval_secs, REWALK_INTERVAL_SECS,
        "`files.index.status` 报的周期与声明的那个常量不是同一个数 ——\n\
         🔴 边界③ 逐字要求这个数**可查询**、不许只活在代码里"
    );
    assert_eq!(
        s.entries, fx.entries,
        "status 报的条目数与刚建的那一份对不上"
    );
    assert!(!s.index_missing, "刚建完就说索引不在");
    assert!(!s.stale, "刚建完就说该重走了 —— 那个判断的方向反了");
    forget_resident();
    let s = status();
    assert!(
        s.index_missing,
        "索引丢了之后 status 仍然说它在 —— 界面会把「没建过」显示成「0 条」"
    );
    assert!(
        !s.stale,
        "索引不存在时不该说「该重走了」；它该说「还没建过」"
    );
}

/// 🔴 「多久前更新的」那个数必须**真的是个数**，而不是个占位。
#[test]
fn the_age_is_a_real_number_and_a_backwards_clock_does_not_blow_it_up() {
    let _lock = resident_lock();
    let fx = make_tree("age", 1, 1, 0);
    let snap = build(&fx.root);
    let t0 = snap.built_at();
    assert_eq!(
        snap.age_secs(t0 + std::time::Duration::from_secs(1234)),
        1234,
        "年龄算错了 —— 而它就是要求显示在界面上的那个延迟"
    );
    assert_eq!(
        snap.age_secs(t0 - std::time::Duration::from_secs(9)),
        0,
        "时钟倒退（NTP 校时）时年龄没有被夹到 0 —— 那会让界面显示一个负的延迟或者直接 panic"
    );
}

/// ★★ 「该重走了」这个判断是一条**纯函数**，两侧都要有输入。
#[test]
fn the_stale_predicate_flips_exactly_at_the_declared_interval() {
    assert!(
        !is_stale(REWALK_INTERVAL_SECS),
        "刚好等于周期就说该重走了 —— 边界差一格"
    );
    assert!(
        is_stale(REWALK_INTERVAL_SECS + 1),
        "超过周期一秒还说不用重走 —— 那这个周期声明就是假的"
    );
}

// ══════════════════════ 查询那一段 ══════════════════════

/// 🔴 **「零流量搜索」那句话的判据**：出去的只有命中。
#[test]
fn a_query_hands_back_the_hits_and_nothing_else() {
    let _lock = resident_lock();
    let fx = make_tree("find", 4, 6, 0);
    let one = fx.root.join("d0002").join("needle-here-and-nowhere-else");
    std::fs::File::create(&one).expect("造那一条命中");
    // 🔴 这个位是**进程级**的，而 `cargo test` 默认并行 ⇒ 先串行（理由住 `index_testing.rs`）
    let _serial = crate::files::index::testing::serial();
    rebuild_once(&fx.root)
        .expect("本格独占跑，抢不到那个位就是并发保护写错了 —— 不许静默当成走过了");

    let r = query("needle-here-and-nowhere-else", 100);
    assert_eq!(r.total_hits, 1, "命中数不对");
    assert_eq!(r.hits.len(), 1, "回送的条数不对");
    assert_eq!(
        r.hits[0].path,
        super::super::raw::path_bytes(&one),
        "回送的那一条不是逐字节相等的原路径"
    );
    assert_eq!(
        r.scanned,
        fx.entries + 1,
        "扫过的条数与索引里的条数对不上 —— 这个数是反空真用的：\n\
         「扫了 0 条所以没命中」与「真的没命中」在界面上一模一样"
    );
    assert!(!r.truncated, "只有一条命中却说被截断了");
    assert!(!r.index_missing);

    // 反向那半：不命中的那一趟**一条都不回**，而且 `scanned` 仍然说得出它扫了多少。
    let miss = query("this-string-is-in-no-path", 100);
    assert_eq!(miss.total_hits, 0);
    assert_eq!(miss.hits, Vec::<Hit>::new(), "没命中却回送了东西");
    assert_eq!(miss.scanned, fx.entries + 1, "没命中那一趟把扫描面报丢了");
}

#[test]
fn the_limit_truncates_the_payload_but_never_the_count() {
    let _lock = resident_lock();
    let fx = make_tree("limit", 3, 10, 0);
    // 🔴 这个位是**进程级**的，而 `cargo test` 默认并行 ⇒ 先串行（理由住 `index_testing.rs`）
    let _serial = crate::files::index::testing::serial();
    rebuild_once(&fx.root)
        .expect("本格独占跑，抢不到那个位就是并发保护写错了 —— 不许静默当成走过了");
    let all = query("f000", 10_000);
    assert_eq!(
        all.total_hits,
        3 * 10,
        "夹具里每个目录 10 个文件、名字都以 f000 开头？"
    );
    assert!(!all.truncated);

    let capped = query("f000", 4);
    assert_eq!(capped.hits.len(), 4, "上限没生效");
    assert_eq!(
        capped.total_hits, all.total_hits,
        "🔴 上限把**总数**也砍了 —— 那会让界面显示「只有 4 个结果」而事实是 30 个"
    );
    assert!(capped.truncated, "被截断了却没说");
}

#[test]
fn a_query_against_a_missing_index_says_so_instead_of_saying_no_hits() {
    let _lock = resident_lock();
    forget_resident();
    let r = query("anything", 10);
    assert!(
        r.index_missing,
        "🔴 索引还没建过时回了一个「没命中」—— 那两件事在界面上一模一样，\n\
         而用户会去怀疑自己的搜索词，不会去怀疑索引"
    );
    assert_eq!(r.total_hits, 0);
    assert_eq!(r.scanned, 0);
}

/// 🔴 非 UTF-8 的文件名**真的搜得到、真的原样回来** —— 那条的活体。
#[test]
fn a_non_utf8_filename_is_indexed_searchable_and_returned_byte_for_byte() {
    let _lock = resident_lock();
    #[cfg(unix)]
    {
        let fx = make_tree("rawname", 1, 1, 0);
        let mut name = b"weird-".to_vec();
        name.extend_from_slice(&[0xff, 0xfe]);
        name.extend_from_slice(b"-name");
        let mut full = super::super::raw::path_bytes(&fx.root).to_vec();
        full.push(b'/');
        full.extend_from_slice(&name);
        let target = super::super::raw::to_path_buf(&full);
        std::fs::File::create(&target).expect("造非 UTF-8 名字的文件");

        // 🔴 这个位是**进程级**的，而 `cargo test` 默认并行 ⇒ 先串行（理由住 `index_testing.rs`）
        let _serial = crate::files::index::testing::serial();
        rebuild_once(&fx.root)
            .expect("本格独占跑，抢不到那个位就是并发保护写错了 —— 不许静默当成走过了");
        let r = query("weird-*-name", 10);
        assert_eq!(
            r.total_hits, 1,
            "非 UTF-8 的名字搜不到 —— 那正是 `档②` 说的「寻址不到」"
        );
        assert_eq!(
            r.hits[0].path, full,
            "回送的字节与盘上那个名字不是逐字节相等 —— 有损解码溜进来了"
        );
        assert!(
            std::str::from_utf8(&r.hits[0].path).is_err(),
            "这份夹具本该**不是**有效 UTF-8 —— 夹具坏了，本条在测别的东西"
        );
    }
    #[cfg(not(unix))]
    {
        // 🔴 **判不了**，缺的证据：一台 Windows 机器上造一个非 WTF-8 名字的文件。
        // 那不只是「本轮没做」——`OsStr` 在那个平台上**不允许**非 WTF-8，
        // 所以这一格在那里可能根本不存在。本族对那一维**不出声**。
        eprintln!(
            "[24f] 非 UTF-8 文件名那一格在本 target 上判不了：缺一台真机 ＋ 一个能造出来的名字"
        );
    }
}

/// 🔴 **两趟重走不许重叠** —— 第二趟当场被拒，而且**拒得出声**。
///
/// # 它治的是什么（2026-09-21 现打逼出来的）
///
/// `rebuild_once` 走的是**一整棵树**，而在这一拍之前它**零并发保护** ——
/// 两趟同时调就是两趟都走整棵树。调用方（界面那一侧）恰恰是「用户一点就发」那种节奏
/// ⇒ 连点两下就双倍开销，而且没有任何东西会出声。
///
/// # 为什么是「互斥」而不是「周期 ≥ 耗时 × N」
///
/// 现打：「周期 = 单次耗时 × N」这种形式的推荐值**业界没有公认值**
/// （六个方向各自查完都没有）。而**互斥不重叠**是那一类周期性任务真正的工业做法，
/// 四家一致：`plocate`/`mlocate` 的 `flock --nonblock` ·
/// Kubernetes CronJob 的 `concurrencyPolicy: Forbid` ·
/// Prometheus 把「单次预算 > 周期」判成配置错误 · rclone 硬规定 poll < cache。
///
/// # ⚠ 它买不到什么
///
/// - **不买「两条真线程同时打进来」** —— 本条是**同一条线程**上连着调两次
///   （第一趟没结束时第二趟进不来，是靠那个位，不是靠调度）。
///   真并发那一格要起线程，而那会让判据变飘；**这一条如实登记为未买**。
/// - **不买「那个位在 panic 之后会被清掉」** —— `rebuild_once` 里那一段之间没有 RAII 守卫，
///   它靠的是「`build` 不 panic」这个事实（`build` 把 IO 错误收成计数）。
///   **哪天 `build` 会 panic 了，这一格就得换成守卫，而本条看不见那一天。**
#[test]
fn a_second_rebuild_is_refused_while_one_is_running_and_says_so() {
    let _lock = resident_lock();
    let fx = make_tree("rebuild-mutex", 3, 4, 8);

    // ── 阴性对照：**没人在跑的时候它必须能跑** ──────────────────
    //    没有这一条，下面那条「第二趟被拒」可以靠「它永远拒」全绿。
    // 🔴 这个位是**进程级**的，而 `cargo test` 默认并行 ⇒ 先串行（理由住 `index_testing.rs`）
    let _serial = crate::files::index::testing::serial();
    let first = rebuild_once(&fx.root);
    assert!(
        first.is_some(),
        "没有任何一趟在跑，第一趟却被拒了 —— 那个位没被清掉，或者它初值就是 true。\n\
         ⇒ 那样这条能力**永远**走不了，而线上看起来只是「已经在跑」。"
    );

    // ── 正题：把那个位按住，再调一次 ────────────────────────────
    //    ⚠ 这里**刻意不起线程**：起线程会让这条判据的读数依赖调度。
    //      按住那个位是同一件事的**可判形态** —— 被测的性质是
    //      「那个位是 true 的时候第二趟进不来」，而不是「两条线程谁先」。
    crate::files::index::testing::hold_rebuilding(true);
    let before = crate::files::index::rebuild_skipped();
    let second = rebuild_once(&fx.root);
    let after = crate::files::index::rebuild_skipped();
    crate::files::index::testing::hold_rebuilding(false);

    assert!(
        second.is_none(),
        "有一趟正在跑，第二趟却走了 —— 互斥没生效，两趟会各走一遍整棵树。"
    );
    // 🔴 **相等断言，不是「至少多了一个」** —— 被抢占的次数要恰好多一。
    assert_eq!(
        after,
        before + 1,
        "被抢占的计数没有恰好加一（{before} → {after}）。\n\
         ⇒ 那个数是「只报不禁」那一栏唯一的读数来源（`scale_f2` 在印它），\n\
         数不准的话占空比那一行印出来的是假话。"
    );

    // ── 收场：那个位真的还回去了，下一趟走得了 ──────────────────
    //    这一格买的是「互斥不是一次性的闸」。
    assert!(
        rebuild_once(&fx.root).is_some(),
        "上一趟走完了，那个位却没还回去 ⇒ 这条能力从此永久被拒。"
    );
}

// ══════════════════════ 分页 · 范围 · 丢旧号 · 类型 ══════════════════════

/// 只回这一屏：`offset`/`limit` 切出来的 == 全量那一摞的同一段，总数照旧是全量。
#[test]
fn a_page_is_exactly_that_slice_of_the_full_answer_and_the_count_stays_whole() {
    let _lock = resident_lock();
    let fx = make_tree("page", 3, 10, 0);
    let _serial = crate::files::index::testing::serial();
    rebuild_once(&fx.root).expect("本格独占跑");
    let m = super::super::query::parse("f000").unwrap();
    let page = |offset: usize, limit: usize| {
        find(&FindArgs {
            query: &m,
            under: None,
            home: None,
            offset,
            limit,
            ticket: None,
            sort: Sort::default(),
        })
        .unwrap()
    };
    let all = page(0, 1000);
    assert_eq!(all.total_hits, 30);
    let mid = page(7, 5);
    assert_eq!(
        mid.hits,
        all.hits[7..12].to_vec(),
        "中间那一屏不是全量的那一段"
    );
    assert_eq!(mid.total_hits, 30, "翻页把总数也切了");
    assert!(mid.truncated, "后面还有却没说");
    let tail = page(28, 5);
    assert_eq!(tail.hits, all.hits[28..30].to_vec());
    assert!(!tail.truncated, "到底了还说有更多");
}

/// `under` 只留那个目录底下的；范围不在索引里 ⇒ 说出来，并给出该走的根（家目录里 ⇒ 家目录，否则 ⇒ 它自己）。
#[test]
fn under_keeps_only_that_subtree_and_says_which_root_would_cover_it() {
    let _lock = resident_lock();
    let fx = make_tree("under", 3, 4, 0);
    let _serial = crate::files::index::testing::serial();
    rebuild_once(&fx.root).expect("本格独占跑");
    let root = super::super::raw::path_bytes(&fx.root).to_vec();
    let d1 = super::super::raw::path_bytes(&fx.root.join("d0001")).to_vec();
    let all = super::super::query::parse("").unwrap();
    let run = |under: Option<&[u8]>, home: Option<&[u8]>| {
        find(&FindArgs {
            query: &all,
            under,
            home,
            offset: 0,
            limit: 1000,
            ticket: None,
            sort: Sort::default(),
        })
        .unwrap()
    };
    let inside = run(Some(&d1), None);
    let want: std::collections::BTreeSet<Vec<u8>> = (0..4)
        .map(|f| {
            super::super::raw::path_bytes(&fx.root.join("d0001").join(format!("f{f:04}"))).to_vec()
        })
        .collect();
    let got: std::collections::BTreeSet<Vec<u8>> =
        inside.hits.iter().map(|h| h.path.clone()).collect();
    assert_eq!(
        got, want,
        "只搜那个目录：挑出来的不是它底下那几条（它自己也不该在）"
    );
    assert!(!inside.out_of_index);
    assert_eq!(inside.cover_root.as_deref(), Some(root.as_slice()));
    // 家目录就是索引的根 ⇒ 不给 `under` 就是整份。
    assert_eq!(run(None, Some(&root)).total_hits, fx.entries);

    let outside = b"/ccm-no-such-dir-outside".to_vec();
    let away = run(Some(&outside), None);
    assert!(away.out_of_index, "范围不在索引里却没说");
    assert_eq!(away.total_hits, 0);
    assert_eq!(
        away.cover_root.as_deref(),
        Some(outside.as_slice()),
        "家目录外 ⇒ 该走它自己"
    );
    let in_home = run(Some(&d1), Some(b"/"));
    assert!(!in_home.out_of_index);
    let home_cover = run(Some(&outside), Some(b"/"));
    assert_eq!(
        home_cover.cover_root.as_deref(),
        Some(b"/".as_slice()),
        "在家目录里 ⇒ 该走家目录"
    );
}

/// 同一个搜索框来了更大的号 ⇒ 旧那一趟被顶掉、晚到的旧号当场丢；同号（翻页）照常；别的搜索框不受牵连。
#[test]
fn a_newer_seq_supersedes_only_its_own_stream() {
    let _lock = resident_lock();
    let fx = make_tree("seq", 1, 3, 0);
    let _serial = crate::files::index::testing::serial();
    rebuild_once(&fx.root).expect("本格独占跑");
    let m = super::super::query::parse("f").unwrap();
    let with = |t: &Ticket| {
        find(&FindArgs {
            query: &m,
            under: None,
            home: None,
            offset: 0,
            limit: 10,
            ticket: Some(t),
            sort: Sort::default(),
        })
    };
    let t5 = ticket("idx-seq-a", 5).expect("第一个号");
    assert!(with(&t5).is_ok());
    assert!(ticket("idx-seq-a", 3).is_err(), "晚到的旧号没被丢");
    let again = ticket("idx-seq-a", 5).expect("同号再来（翻页）该放行");
    let other = ticket("idx-seq-b", 1).expect("别的搜索框");
    let _t6 = ticket("idx-seq-a", 6).expect("新号");
    assert_eq!(
        with(&t5).unwrap_err(),
        Superseded,
        "来了新号，旧那一趟没收手"
    );
    assert!(with(&again).is_err());
    assert!(with(&other).is_ok(), "别的搜索框被牵连撤了");
}

/// 索引记住每条的类型：`folder:` 挑中的 == 夹具造的目录，`file:` == 夹具造的文件。
#[test]
fn the_index_keeps_each_entry_kind() {
    let _lock = resident_lock();
    let fx = make_tree("kinds", 4, 5, 0);
    let _serial = crate::files::index::testing::serial();
    rebuild_once(&fx.root).expect("本格独占跑");
    let dirs = query("folder:", 1000);
    assert_eq!(dirs.total_hits, 4);
    assert!(dirs
        .hits
        .iter()
        .all(|h| h.kind == super::super::query::KIND_DIR));
    assert_eq!(query("file:", 1000).total_hits, 4 * 5);
}

/// 文件名搜索按相关度排：名字就是它 ＞ 以它开头 ＞ 名字里有它；同一档里路径浅的在前、再按路径字节序。
/// 翻页也是这一个序（每一屏都是全序里的那一段）。此前按索引走到的顺序给，`report` 埋在一堆 `old-report-2.txt` 后面。
#[test]
fn name_hits_come_back_most_relevant_first_and_pages_keep_that_order() {
    let _lock = resident_lock();
    let root = std::env::temp_dir().join(format!("ccm-24f-rank-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    // 名字故意让「走到的先后」与相关度反着来。
    let made = [
        "a/b/report",
        "a/my-report.txt",
        "a/report.txt",
        "report",
        "z/xreport",
        "z/zz/report",
    ];
    for p in made {
        let at = root.join(p);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(&at, b"").unwrap();
    }
    let _serial = crate::files::index::testing::serial();
    rebuild_once(&root).expect("本格独占跑");
    let m = super::super::query::parse("REPORT").unwrap();
    let page = |offset: usize, limit: usize| {
        find(&FindArgs {
            query: &m,
            under: None,
            home: None,
            offset,
            limit,
            ticket: None,
            sort: Sort::default(),
        })
        .unwrap()
    };
    let rel = |hits: &[Hit]| -> Vec<String> {
        hits.iter()
            .map(|h| {
                let s = String::from_utf8_lossy(&h.path).to_string();
                s[root.to_string_lossy().len() + 1..].to_string()
            })
            .collect()
    };
    let all = page(0, 100);
    assert_eq!(
        rel(&all.hits),
        vec![
            "report",
            "a/b/report",
            "z/zz/report",
            "a/report.txt",
            "a/my-report.txt",
            "z/xreport",
        ],
        "命中不是按相关度排的"
    );
    assert_eq!(all.total_hits, 6);
    for (offset, limit) in [(0, 2), (2, 2), (4, 5)] {
        let p = page(offset, limit);
        let end = (offset + limit).min(6);
        assert_eq!(
            p.hits,
            all.hits[offset..end].to_vec(),
            "第 {offset} 条起那一屏与全序里那一段不一样"
        );
    }
    std::fs::remove_dir_all(&root).ok();
}

/// 按名称 / 位置 / 修改时间 / 大小排、正反都行，每一屏都是那个全序里的那一段；每条带盘上的大小与修改时间（目录没有大小）。
#[test]
fn hits_sort_by_each_column_both_ways_and_pages_keep_that_order() {
    let _lock = resident_lock();
    let root = std::env::temp_dir().join(format!("ccm-24f-sort-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    // (相对路径, 字节数, 修改时间)：名字、位置、大小、时间四个序互不相同。
    let made: [(&str, usize, u64); 5] = [
        ("b/Delta.log", 30, 4_000),
        ("a/x/beta.log", 10, 1_000),
        ("c/alpha.log", 50, 3_000),
        ("a/gamma.log", 20, 5_000),
        ("b/a/Echo.log", 40, 2_000),
    ];
    for (p, n, t) in made {
        let at = root.join(p);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(&at, vec![b'x'; n]).unwrap();
        let f = std::fs::File::options().write(true).open(&at).unwrap();
        f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(t))
            .unwrap();
    }
    let _serial = crate::files::index::testing::serial();
    rebuild_once(&root).expect("本格独占跑");
    let m = super::super::query::parse("ext:log").unwrap();
    let rootb = super::super::raw::path_bytes(&root).to_vec();
    let page = |key: SortKey, desc: bool, offset: usize, limit: usize| {
        find(&FindArgs {
            query: &m,
            under: Some(&rootb),
            home: None,
            offset,
            limit,
            ticket: None,
            sort: Sort { key, desc },
        })
        .unwrap()
    };
    let names = |hits: &[Hit]| -> Vec<String> {
        hits.iter()
            .map(|h| String::from_utf8_lossy(name_bytes(&h.path)).to_string())
            .collect()
    };
    let cases: [(SortKey, [&str; 5]); 4] = [
        (
            SortKey::Name,
            [
                "alpha.log",
                "beta.log",
                "Delta.log",
                "Echo.log",
                "gamma.log",
            ],
        ),
        // 位置：a · a/x · b · b/a · c
        (
            SortKey::Location,
            [
                "gamma.log",
                "beta.log",
                "Delta.log",
                "Echo.log",
                "alpha.log",
            ],
        ),
        (
            SortKey::Mtime,
            [
                "beta.log",
                "Echo.log",
                "alpha.log",
                "Delta.log",
                "gamma.log",
            ],
        ),
        (
            SortKey::Size,
            [
                "beta.log",
                "gamma.log",
                "Delta.log",
                "Echo.log",
                "alpha.log",
            ],
        ),
    ];
    for (key, want) in cases {
        let up = page(key, false, 0, 100);
        assert_eq!(names(&up.hits), want, "{key:?} 正序不对");
        let down = page(key, true, 0, 100);
        let mut rev = want.to_vec();
        rev.reverse();
        assert_eq!(names(&down.hits), rev, "{key:?} 倒序不对");
        for (offset, limit) in [(0, 2), (2, 2), (4, 3)] {
            let p = page(key, true, offset, limit);
            let end = (offset + limit).min(5);
            assert_eq!(
                p.hits,
                down.hits[offset..end].to_vec(),
                "{key:?} 第 {offset} 条起那一屏不是全序里那一段"
            );
        }
    }
    // 每条带盘上的两格（与造的一致），不论按哪一列排。
    for key in [SortKey::Relevance, SortKey::Name, SortKey::Size] {
        for h in page(key, false, 0, 100).hits {
            let name = String::from_utf8_lossy(name_bytes(&h.path)).to_string();
            let (_, n, t) = made.iter().find(|(p, _, _)| p.ends_with(&name)).unwrap();
            assert_eq!(
                h.meta,
                Meta {
                    size: Some(*n as u64),
                    mtime_secs: Some(*t)
                },
                "{name} 的大小 / 修改时间不对"
            );
        }
    }
    let dirs = super::super::query::parse("folder:").unwrap();
    let d = find(&FindArgs {
        query: &dirs,
        under: Some(&rootb),
        home: None,
        offset: 0,
        limit: 100,
        ticket: None,
        sort: Sort {
            key: SortKey::Size,
            desc: false,
        },
    })
    .unwrap();
    assert!(d.total_hits > 0);
    assert!(
        d.hits.iter().all(|h| h.meta.size.is_none()),
        "目录不该有大小"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// 位置 = 父目录相对搜索起点那一段：直接在起点里 ⇒ 空；不在起点底下 ⇒ 父目录全路径。
#[test]
fn location_is_the_parent_relative_to_the_search_start() {
    assert_eq!(location_of(b"/h/u/p/src/a.rs", Some(b"/h/u/p")), b"src");
    assert_eq!(
        location_of(b"/h/u/p/src/x/a.rs", Some(b"/h/u/p/")),
        b"src/x"
    );
    assert_eq!(location_of(b"/h/u/p/a.rs", Some(b"/h/u/p")), b"");
    assert_eq!(
        location_of(b"/elsewhere/a.rs", Some(b"/h/u/p")),
        b"/elsewhere"
    );
    assert_eq!(location_of(b"/a.rs", Some(b"/")), b"");
    assert_eq!(location_of(b"/etc/a.rs", Some(b"/")), b"etc");
    assert_eq!(location_of(b"/h/u/p/a.rs", None), b"/h/u/p");
}

// ══════════════════════ 碰常驻状态只有一道门 ══════════════════════

/// 一个 `#[test]` 块里会不会写常驻那两份（索引 · 浏览名单）。
///
/// 写它们的口：`rebuild_once` · `set_browsing` 直调；或经这一族的入口（`files::answer` / `answer_wire`）
/// 调 `files.index.rebuild` / `files.browse` —— 能力名写成字面量以外的东西（循环变量、拼出来的）
/// 就当它可能是那两条（「每条能力真调一次」那一形）。
/// `files/` 底下的判据 `use super::*`，入口是裸名 `answer(` / `answer_wire(`；别处要带 `files::`。
fn touches_resident_state(chunk: &str, bare_entry: bool) -> bool {
    // 针拼出来：本文件自己的源码不该被当成一处调用。
    let writers = [concat!("rebuild", "_once("), concat!("set_", "browsing(")];
    if writers.iter().any(|w| chunk.contains(w)) {
        return true;
    }
    let touching = [
        "files.index.rebuild",
        "files-index-rebuild",
        "files.browse",
        "files-browse",
    ];
    let mut entries = vec![
        concat!("files::", "answer("),
        concat!("files::", "answer_wire("),
    ];
    if bare_entry {
        entries.push(concat!(" ", "answer("));
        entries.push(concat!("(", "answer("));
        entries.push(concat!(" ", "answer_wire("));
        entries.push(concat!("(", "answer_wire("));
    }
    entries.iter().any(|e| {
        chunk.match_indices(e).any(|(at, _)| {
            // 定义（`fn answer(`，合成夹具里的那种）不是调用。
            if chunk[..at].trim_end().ends_with("fn") {
                return false;
            }
            let first = chunk[at + e.len()..].trim_start();
            match first.strip_prefix('"') {
                Some(lit) => touching.iter().any(|t| lit.starts_with(&format!("{t}\""))),
                None => true,
            }
        })
    })
}

fn takes_the_lock(chunk: &str) -> bool {
    chunk.contains(concat!("resident", "_lock()"))
}

/// ★ 后端测试树里，凡是会写常驻那两份的 `#[test]` 块，都拿 [`resident_lock`] —— 碰这份进程级状态只有这一道门。
///
/// 起因（mg41 CI 现红）：`a_query_against_a_missing_index_says_so_instead_of_saying_no_hits` 拿锁、清空、查，
/// 查到的却是一份索引 —— 同一进程里 `inbound_structure_guards::the_files_read_family_is_online_exactly_as_it_is_declared`
/// 用空入参把每一条读能力真调一次，`files-index-rebuild` 不给 `path` 就重走家目录、把常驻那一份装上，而它没拿这把锁。
/// 单跑、本机跑都绿：要两条恰好并发、而那一趟重走恰好落在「清空」与「查」之间。
///
/// 量法：按 `#[test]` 切块（先剥注释再切），块里有写口（[`touches_resident_state`]）⇒ 块里得有 `resident_lock()`。
#[test]
fn every_test_that_can_touch_the_resident_state_takes_the_one_lock() {
    let root = crate::guard_support::tests_root();
    let mut touching = 0usize;
    let mut unlocked: Vec<String> = Vec::new();
    for (path, src) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let inner = path.strip_prefix(&root).unwrap_or(&path);
        let rel = inner.to_string_lossy().replace('\\', "/");
        // 第一段目录是 `files` ⇒ 这一族自己的判据（按路径段比，不按串前缀）。
        let bare =
            inner.components().next() == Some(std::path::Component::Normal("files".as_ref()));
        let stripped = guard_core::strip_comment_lines(&src);
        // 第一块是第一条 `#[test]` 之前的那段（工具函数），不是判据。
        for chunk in guard_core::test_attr_chunks(&stripped).into_iter().skip(1) {
            if !touches_resident_state(&chunk, bare) {
                continue;
            }
            touching += 1;
            if !takes_the_lock(&chunk) {
                let name = chunk
                    .split("fn ")
                    .nth(1)
                    .and_then(|s| s.split('(').next())
                    .unwrap_or("?")
                    .trim()
                    .to_string();
                unlocked.push(format!("{rel}::{name}"));
            }
        }
    }
    assert!(
        touching >= 10,
        "只认出 {touching} 条会写常驻状态的判据 —— 切块或认法坏了，本条在空转"
    );
    assert!(
        unlocked.is_empty(),
        "这几条判据会写常驻索引 / 浏览名单，却没拿 `resident_lock()`：{unlocked:?}\n\
         它们与拿了锁的那几条并发时，会在别人「清空」与「查」之间把状态换掉（间歇红，单跑绿）。"
    );
    // 反向自检：这把尺子认得出那一形，也认得出拿了锁的、只碰别的能力的。
    let loop_call =
        "fn x() {\n    for n in names { let _ = crate::files::answer_wire(&n, &a, &t); }\n}";
    assert!(
        touches_resident_state(loop_call, false),
        "能力名是变量的那一形没认出来"
    );
    assert!(!takes_the_lock(loop_call));
    assert!(touches_resident_state(
        "    let v = answer(\"files.index.rebuild\", &a, &t);",
        true
    ));
    assert!(!touches_resident_state(
        "    let v = answer(\"files.ls\", &a, &t);",
        true
    ));
    assert!(!touches_resident_state(
        "    let v = answer(\"files.ls\", &a, &t);",
        false
    ));
    assert!(
        !touches_resident_state("pub fn answer(name: &str) -> u32 {", true),
        "把函数定义当成了一处调用"
    );
}
