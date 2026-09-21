//! 〔步 24f〕常驻索引的行为判据 —— **在真盘上真走一遍**，不是源码扫描。
//!
//! # 🔴 主锚全部是**相等**断言，不是地板
//!
//! 「反空真」那条纪律逐字：绿必须来自相等断言，地板在「变少」方向上是瞎的。
//! 本族的两个主锚都算得出确切值，所以都写成相等：
//!
//! | 锚 | 相等的两侧 |
//! |---|---|
//! | 条目数 | 索引里的条数 == 夹具**按构造**造了多少条 |
//! | 常驻字节 | [`Snapshot::resident_bytes`] == 路径总长（算出来的）＋ 4×条数 |
//!
//! 时延那一类量不出确切值 ⇒ 它们**不在这里**，在 `秤 F2`（`scale_f2.rs`），
//! 而那边的主锚同样是上面这两个相等，墙钟只作为**读数**报出去。
//!
//! # 语料
//!
//! **全合成**，现造现删（`设计/17 §6` 的数据源纪律：测试夹具不许含真会话正文、
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
    super::super::browse_watch::forget_all();
    g
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
fn resident_bytes_equals_the_paths_plus_four_bytes_of_bookkeeping_per_entry() {
    let _lock = resident_lock();
    let fx = make_tree("bytes", 5, 9, 17);
    let snap = build(&fx.root);
    assert_eq!(
        snap.resident_bytes(),
        fx.path_bytes_total + 4 * fx.entries,
        "常驻字节不等于「路径总长 ＋ 4×条数」——\n\
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
    rebuild_once(&fx.root)
        .expect("本格独占跑，抢不到那个位就是并发保护写错了 —— 不许静默当成走过了");
    let s = status();
    assert_eq!(
        s.rewalk_interval_secs, REWALK_INTERVAL_SECS,
        "`files.index.status` 报的周期与声明的那个常量不是同一个数 ——\n\
         🔴 `设计/96 §2.9` 边界③ 逐字要求这个数**可查询**、不许只活在代码里"
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
        "年龄算错了 —— 而它就是 `设计/60 §3.5.3` 要求显示在界面上的那个延迟"
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
    rebuild_once(&fx.root)
        .expect("本格独占跑，抢不到那个位就是并发保护写错了 —— 不许静默当成走过了");

    let r = find(&FindArgs {
        needle: b"needle-here-and-nowhere-else".to_vec(),
        ignore_ascii_case: false,
        limit: 100,
    });
    assert_eq!(r.total_hits, 1, "命中数不对");
    assert_eq!(r.hits.len(), 1, "回送的条数不对");
    assert_eq!(
        r.hits[0],
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
    let miss = find(&FindArgs {
        needle: b"this-string-is-in-no-path".to_vec(),
        ignore_ascii_case: false,
        limit: 100,
    });
    assert_eq!(miss.total_hits, 0);
    assert_eq!(miss.hits, Vec::<Vec<u8>>::new(), "没命中却回送了东西");
    assert_eq!(miss.scanned, fx.entries + 1, "没命中那一趟把扫描面报丢了");
}

#[test]
fn the_limit_truncates_the_payload_but_never_the_count() {
    let _lock = resident_lock();
    let fx = make_tree("limit", 3, 10, 0);
    rebuild_once(&fx.root)
        .expect("本格独占跑，抢不到那个位就是并发保护写错了 —— 不许静默当成走过了");
    let all = find(&FindArgs {
        needle: b"f000".to_vec(),
        ignore_ascii_case: false,
        limit: 10_000,
    });
    assert_eq!(
        all.total_hits,
        3 * 10,
        "夹具里每个目录 10 个文件、名字都以 f000 开头？"
    );
    assert!(!all.truncated);

    let capped = find(&FindArgs {
        needle: b"f000".to_vec(),
        ignore_ascii_case: false,
        limit: 4,
    });
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
    let r = find(&FindArgs {
        needle: b"anything".to_vec(),
        ignore_ascii_case: false,
        limit: 10,
    });
    assert!(
        r.index_missing,
        "🔴 索引还没建过时回了一个「没命中」—— 那两件事在界面上一模一样，\n\
         而用户会去怀疑自己的搜索词，不会去怀疑索引"
    );
    assert_eq!(r.total_hits, 0);
    assert_eq!(r.scanned, 0);
}

/// 🔴 非 UTF-8 的文件名**真的搜得到、真的原样回来** —— `设计/60 §2 档②` 那条的活体。
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

        rebuild_once(&fx.root)
            .expect("本格独占跑，抢不到那个位就是并发保护写错了 —— 不许静默当成走过了");
        let r = find(&FindArgs {
            needle: name.clone(),
            ignore_ascii_case: false,
            limit: 10,
        });
        assert_eq!(
            r.total_hits, 1,
            "非 UTF-8 的名字搜不到 —— 那正是 `档②` 说的「寻址不到」"
        );
        assert_eq!(
            r.hits[0], full,
            "回送的字节与盘上那个名字不是逐字节相等 —— 有损解码溜进来了"
        );
        assert!(
            std::str::from_utf8(&r.hits[0]).is_err(),
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
/// `真相源/104 §4` 现打：「周期 = 单次耗时 × N」这种形式的推荐值**业界没有公认值**
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
    let fx = make_tree("rebuild-mutex", 3, 4, 8);

    // ── 阴性对照：**没人在跑的时候它必须能跑** ──────────────────
    //    没有这一条，下面那条「第二趟被拒」可以靠「它永远拒」全绿。
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
