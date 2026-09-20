//! 🔴 **秤 F2** —— `设计/17 §6.9` 表第二行，「搜索三段各自的代价」。
//!
//! 设计逐字要三样：
//!
//! > ① 建索引墙钟（**冷 / 热两档**）· ② 索引常驻字节 · ③ 单次查询时延（按命中数分桶）
//!
//! 以及一条落点：
//!
//! > ⚠ 它同时是**新鲜度**那个数的来源：重走周期 **≥** 建索引耗时
//!
//! # 这杆秤的**主锚是相等断言**，墙钟只是读数
//!
//! 「反空真」那条纪律逐字：绿必须来自相等断言；地板在「变少」方向上是瞎的。
//! 墙钟量不出确切值 ⇒ 它**不当锚**。当锚的是两个算得出的量：
//!
//! | 锚 | 相等的两侧 |
//! |---|---|
//! | 语料 | 索引条目数 == 语料**按构造**造了多少条 |
//! | ② 常驻字节 | [`Snapshot::resident_bytes`] == 路径字节总长（算出来的）＋ 4×条数 |
//! | ③ 分桶 | 每个桶的**命中数**是算出来的确切值，不是量出来的 |
//!
//! 唯一一条挂在墙钟上的**闸**是那条落点（周期 ≥ 建索引耗时），而它带着
//! **两个数量级**的余量（300 秒对约 1 秒）——「闸不许比量具自身的分辨率还细」
//! 那条纪律在这里是靠余量满足的，不是靠容差调参。
//!
//! # 🔴 冷档**判不了**（不编一个数）
//!
//! `设计/17 §6.9` 的 `F2` 那一栏逐字：「①的**冷档今天量不了**（`drop_caches` 要 root）
//! ⇒ 先量热档，冷档写『判不了』并说清缺什么」。
//!
//! **缺的证据，逐条：**
//!
//! 1. `echo 3 > /proc/sys/vm/drop_caches` 要 root，而后端（与本判据）都不是 root。
//! 2. 退而求其次的办法（`posix_fadvise(DONTNEED)` 逐文件丢页缓存）**也不够**：
//!    它丢的是**文件内容**的页，而建索引读的是**目录项与 inode 元数据** ——
//!    那两样在 dentry/inode 缓存里，不在 page cache 里，`fadvise` 碰不到。
//! 3. 机械盘那一档**另外还缺一台机械盘**（本机是 NVMe ＋ tmpfs）。
//!
//! ⇒ 所以「后端重启之后重建够不够快」这一问**今天答不了**，
//! 而 `设计/96 §2.9` 末尾那个「索引要不要落盘」正是压在它上面的。
//! **本件不答那一问，也不假装答了。**
//!
//! # 🔴 第一趟跑完就有一条发现，写在这里别丢：**「10 毫秒」那个数不是我们的数**
//!
//! `设计/60 §3.5.2` 那张三段表的「查询」那一行逐字写着「**10 毫秒**，且零流量」，
//! 脚注指向 `真相源/98 §3.3`。而那一格现打的原话是：
//!
//! > **查一次**（全 64 万条，`grep -F` **暴力线性扫、零索引结构**）：**0.01 秒**
//!
//! ⇒ 那是 **`grep -F`** 的读数，**不是我们这份实现的读数** ——
//! `设计/17 §6.9` 的 `F2` 那一栏其实已经预告过这件事，逐字：
//! 「今天关于**搜索**性能的任何说法（包括 `60 §3.5.2` 那张三段表里的
//! `0.99 秒` / `0.01 秒`）都只是 `find` ＋ `grep` 在一台机器上的**代理指标**」。
//!
//! **本秤把那个差距量出来了**（本机，**release** 档，语料 20 220 条，连跑三趟稳定）：
//! 单次查询 **0.62–1.57 毫秒** ⇒ 线性外推到 64 万条量纲是 **约 20–50 毫秒**。
//! ⇒ 与那个代理指标（10 毫秒）差 **2–5 倍**，**不是**一个数量级。
//! 差在哪里：`grep -F` 走 SIMD ＋ Boyer-Moore 一族，
//! 而本族是首字节筛 ＋ 整块比（`files::raw::contains_exact` 的头注记着前后两版）。
//!
//! 🔴🔴 **这段读数自己腐过一次，教训比读数值钱，逐字留着：**
//! 本节第一版写的是「约 3–9 毫秒 ⇒ 约 90–280 毫秒 ⇒ **慢一到两个数量级**」。
//! 那是**一趟**读数，而那一趟恰好跑在刚编译完、机器正忙的时刻 ——
//! 连跑三趟之后同一台机器给的是上面那个数，**差了五倍**。
//! ⇒ 纪律：**一趟不算读数。** 报一个性能结论之前连跑几趟看它稳不稳；
//! 这条本仓在 `设计/17 §6` 记过同形的一次（「语料 86→87 张、p90 100.8%→96.9%，
//! 而被测代码一行没改」）。
//!
//! ⚠ **门禁跑的是 debug 档**，那一档同一份语料是 5.7–8.6 毫秒
//! （外推约 180–270 毫秒）。两档差 **8 倍左右** ⇒ 读门禁里那几行读数时
//! 一定要连着「构建档」那一行读，否则会得出一个假结论。
//!
//! ⇒ **「打字即出结果，不等」（`设计/60 §3.5` 逐字的形状要求）在 64 万条这个
//! 量纲上今天仍然**没有被证明** —— 20–50 毫秒是「一次按键之内答得出」的量级，
//! 但它是**外推**，而且从没在一棵真的 64 万条的树上跑过。
//! 出路有（换一个子串搜索算法 / 加一层索引结构 / 边打字边增量过滤上一次的结果集），
//! **本件一条都没做，也不假装做了**。那是另一件活，而它现在有一杆秤盯着了。
//!
//! ⚠ 还有一格别混：上面那个外推是**线性**的，而它的前提（查询是 O(索引字节)）
//! 本件**只在一个规模上量过**。两个规模的对拍没做。
//!
//! # 语料的来历（一起报，别让下一个人把它当普适前提）
//!
//! 量纲取自 `真相源/98 §3.3` 的**现打**：我 home 底下 **640 413** 条目、
//! 平均路径长 **126 字节**、索引原始体积 **77 MiB**。
//! 🔴 那是**一台机器**的读数，不是「用户都这么多」——
//! 本文件的语料是按那个量纲**等比缩小**的合成树，读数里一起报缩放比。
//! 别人机器的条目数 / 盘型 / 文件系统**一格都没量**（`真相源/98 §3.5` 边界 3）。

use super::tests::resident_lock;

/// 语料规模。
///
/// # 为什么不是 640 413
///
/// 门禁每跑一趟都要造一遍这棵树再删掉。64 万个 inode 在门禁上是几十秒的代价，
/// 而 ⚠ `/tmp` 是挂了 `usrquota` 的 tmpfs（本机现打 `nr_inodes=1048576`）——
/// 往那儿堆 64 万个 inode 会把这台机器上别人的活一起拖下水。
/// ⇒ 取 **1/32** 的量纲（下面那三个常数算出来正好 20 220 条），
/// 并把「每条多少字节 / 每毫秒多少条」这两个**与规模无关**的比值报出去，
/// 由本文件把它们换算回 640 413 那个量纲。
/// 🔴 换算是**线性外推**，而它成立的前提（遍历是 O(条目数)）**本件没有在两个规模上验过** ——
/// 如实登记：这一格是外推，不是实测。
const L1_DIRS: usize = 20;
const L2_DIRS_EACH: usize = 10;
const FILES_EACH: usize = 100;

/// 叶子文件名的长度。配上两层目录名之后，**相对根**的平均路径长落在 126 字节附近
///（`真相源/98 §3.3` 那个现打值）。
const LEAF_NAME_LEN: usize = 101;

/// 一级 / 二级目录名的长度（定长，所以字节总量是算出来的）。
const DIR_NAME_LEN: usize = 11;

/// `真相源/98 §3.3` 的量纲 —— 外推的分母。
const REFERENCE_ENTRIES: usize = 640_413;

/// 一棵**按构造知道自己有多少条、多少字节**的三层合成树。
struct Corpus {
    root: std::path::PathBuf,
    entries: usize,
    /// 全部条目的路径字节总长（**含**根前缀）。
    path_bytes_total: usize,
    /// 根前缀的字节长 —— 语料自检要把它扣掉（它逐机不同，而量纲是**相对**路径长）。
    root_len: usize,
    /// 名字里含这个串的条目恰好有多少条 —— `③` 那几个桶的确切命中数。
    bucket_needles: Vec<(Vec<u8>, usize)>,
}

impl Drop for Corpus {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

/// 造语料。名字**全部定长**，所以下面每个数都是算出来的。
fn build_corpus() -> Corpus {
    let root = std::env::temp_dir().join(format!("ccm-f2-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("造语料根");
    let root_len = crate::files::raw::path_bytes(&root).len();

    let mut entries = 0usize;
    let mut total = 0usize;
    // 桶 `hit-1%` 与 `hit-10%`：靠叶子名字里的一个标记位控制命中数。
    let mut hits_1pct = 0usize;
    let mut hits_10pct = 0usize;

    for a in 0..L1_DIRS {
        let d1 = format!("l1-{a:08}");
        assert_eq!(
            d1.len(),
            DIR_NAME_LEN,
            "一级目录名不是定长 —— 下面的字节总量就不是算出来的了"
        );
        let p1 = root.join(&d1);
        std::fs::create_dir_all(&p1).expect("造一级目录");
        entries += 1;
        total += root_len + 1 + DIR_NAME_LEN;
        for b in 0..L2_DIRS_EACH {
            let d2 = format!("l2-{b:08}");
            assert_eq!(d2.len(), DIR_NAME_LEN);
            let p2 = p1.join(&d2);
            std::fs::create_dir_all(&p2).expect("造二级目录");
            entries += 1;
            total += root_len + 1 + DIR_NAME_LEN + 1 + DIR_NAME_LEN;
            for c in 0..FILES_EACH {
                // 标记位：`c` 落在 0 ⇒ 1%（每 100 个里 1 个）；落在 0..10 ⇒ 10%。
                let mark = if c == 0 {
                    hits_1pct += 1;
                    hits_10pct += 1;
                    "onepct-tenpct"
                } else if c < 10 {
                    hits_10pct += 1;
                    "zzzzzz-tenpct"
                } else {
                    "zzzzzz-zzzzzz"
                };
                let name = format!("{mark}-{c:03}{}", "p".repeat(LEAF_NAME_LEN - 17));
                assert_eq!(
                    name.len(),
                    LEAF_NAME_LEN,
                    "叶子名不是定长 —— 字节总量的算式就失效了"
                );
                std::fs::File::create(p2.join(&name)).expect("造叶子文件");
                entries += 1;
                total += root_len + 1 + DIR_NAME_LEN + 1 + DIR_NAME_LEN + 1 + LEAF_NAME_LEN;
            }
        }
    }

    // 「一个二级目录那一棵」这个桶的针：**从真路径取字节**，不手拼分隔符
    // （分隔符逐平台不同，手拼会让这一格在换平台时安静地变成 0 命中）。
    let one_subtree = crate::files::raw::path_bytes(
        &root
            .join(format!("l1-{:08}", 0))
            .join(format!("l2-{:08}", 0)),
    )
    .to_vec();

    Corpus {
        root,
        entries,
        path_bytes_total: total,
        root_len,
        bucket_needles: vec![
            (b"this-substring-is-in-no-path".to_vec(), 0),
            // 那个二级目录**自己** ＋ 它底下的全部叶子。
            (one_subtree, 1 + FILES_EACH),
            (b"onepct-".to_vec(), hits_1pct),
            (b"-tenpct-".to_vec(), hits_10pct),
            (b"".to_vec(), entries), // 空针 = 全命中
        ],
    }
}

/// 🔴🔴 **秤 F2 本体** —— 一趟跑完，三样读数一起报。
///
/// ⚠ 写成**一个** `#[test]` 而不是三个：语料造一遍要几千个 inode，
/// 拆成三条就是造三遍（而且它们会抢同一个常驻索引）。
/// 三样读数各自的断言在函数体里逐段分开，失败文案分得清是哪一样。
#[test]
fn f2_the_three_costs_of_the_search_family() {
    let _lock = resident_lock();

    // ── 语料 ─────────────────────────────────────────────────────────────
    let t_corpus = std::time::Instant::now();
    let corpus = build_corpus();
    let corpus_ms = t_corpus.elapsed().as_millis();
    let expected_entries = L1_DIRS + L1_DIRS * L2_DIRS_EACH + L1_DIRS * L2_DIRS_EACH * FILES_EACH;
    assert_eq!(
        corpus.entries, expected_entries,
        "语料的条数与算式对不上 —— 语料坏了，下面三样读数全部作废"
    );

    // ── ① 建索引墙钟（**热档**）─────────────────────────────────────────
    //
    // 「热」的意思写清楚：语料是**本判据刚刚造出来的**，dentry/inode 全在缓存里
    // ⇒ 这一趟量到的是**热缓存**那一档，与 `真相源/98 §3.3` 那个 0.99 秒同一档。
    // ⚠ 走 `rebuild_once` 而不是 `build`：**一趟遍历同时供给 ① ② ③**。
    //   分成两趟的话 ③ 那一段量的是另一次遍历的产物，而两趟之间语料没变、
    //   代价却白付一次 —— 门禁上那一次是几千个 inode 的重读。
    let t_build = std::time::Instant::now();
    let stats = crate::files::index::rebuild_once(&corpus.root);
    let build_ms = t_build.elapsed().as_millis().max(1);

    assert_eq!(
        stats.entries, corpus.entries,
        "索引条数与语料按构造的条数对不上 —— 遍历漏了或多了"
    );
    assert_eq!(stats.unreadable_dirs, 0, "刚造的语料不该有读不了的目录");
    assert!(!stats.truncated, "这个规模不可能撞上界桩上限");

    let entries_per_ms = stats.entries as f64 / build_ms as f64;
    let ref_build_ms = (REFERENCE_ENTRIES as f64 / entries_per_ms).ceil() as u64;

    // ── ② 索引常驻字节 ──────────────────────────────────────────────────
    assert_eq!(
        stats.resident_bytes,
        corpus.path_bytes_total + 4 * corpus.entries,
        "常驻字节不等于「路径总长 ＋ 4×条数」—— 这个量的口径变了（变了就回来改算式，别调松）"
    );
    let bytes_per_entry = stats.resident_bytes as f64 / stats.entries as f64;
    let ref_resident_mib = bytes_per_entry * REFERENCE_ENTRIES as f64 / (1024.0 * 1024.0);

    // ── ③ 单次查询时延（按命中数分桶）───────────────────────────────────
    let mut buckets: Vec<(String, usize, u128)> = Vec::new();
    for (needle, want) in &corpus.bucket_needles {
        let t = std::time::Instant::now();
        let r = crate::files::index::find(&crate::files::index::FindArgs {
            needle: needle.clone(),
            ignore_ascii_case: false,
            limit: usize::MAX,
        });
        let us = t.elapsed().as_micros();
        assert_eq!(
            r.total_hits,
            *want,
            "桶 `{}` 的命中数不对 —— **这一格是主锚**，它红了说明查询的判词变了，\n\
             而时延读数在一个判词变了的实现上没有意义",
            String::from_utf8_lossy(needle)
        );
        assert_eq!(
            r.scanned, corpus.entries,
            "这一趟扫过的条数不等于索引条数 —— 反空真：扫了 0 条的「没命中」与真没命中长得一样"
        );
        let label = if needle.is_empty() {
            "<空针·全命中>".to_string()
        } else {
            String::from_utf8_lossy(needle).to_string()
        };
        buckets.push((label, r.total_hits, us));
    }

    // ── 读数（**报出去，不当闸**）───────────────────────────────────────
    // 🔴 **读数必须说出它自己是哪个档编出来的。**
    //
    // 门禁跑的是 `cargo test`，也就是**未优化**的 debug 档。而 `真相源/98 §3.3`
    // 那两个对照数（0.99 秒建索引 / 0.01 秒查一次）是 `find` 与 `grep -F` ——
    // **优化过的原生代码**。两边不同档，直接并排读会得出一个假结论。
    // ⇒ 报读数时把档一起报；release 那一档要另跑一趟
    //   （`cargo test --release -- --nocapture files::index::scale_f2`）。
    let profile = if cfg!(debug_assertions) {
        "debug（未优化 —— 门禁跑的就是这一档）"
    } else {
        "release（优化过 —— 与 `98 §3.3` 那两个 `find`/`grep` 数同档）"
    };
    eprintln!("╔══ 秤 F2 · 搜索三段的代价（现打，一趟）══════════════════════════");
    eprintln!("║ 构建档：{profile}");
    eprintln!("║ 语料（合成，量纲取自 `真相源/98 §3.3` 的 640413 等比缩小）");
    eprintln!(
        "║   条目 {} 条（目录 {} · 文件 {}）· 造语料墙钟 {} ms",
        corpus.entries,
        L1_DIRS + L1_DIRS * L2_DIRS_EACH,
        L1_DIRS * L2_DIRS_EACH * FILES_EACH,
        corpus_ms
    );
    let mean_rel =
        (corpus.path_bytes_total - corpus.entries * corpus.root_len) as f64 / corpus.entries as f64;
    eprintln!(
        "║   平均路径长（**相对根**）{mean_rel:.2} 字节；`98 §3.3` 那台机器上是 126。\n\
         ║   含根前缀的平均值是 {:.1} 字节（前缀 {} 字节，逐机不同 ⇒ 不进闸）",
        corpus.path_bytes_total as f64 / corpus.entries as f64,
        corpus.root_len
    );
    eprintln!("║ ① 建索引墙钟 —— **热档**");
    eprintln!(
        "║   {} ms / {} 条 ⇒ {:.1} 条每毫秒",
        build_ms, stats.entries, entries_per_ms
    );
    eprintln!(
        "║   线性外推到 640413 条 ⇒ 约 {} ms（`98 §3.3` 的现打代理指标是 990 ms）",
        ref_build_ms
    );
    eprintln!("║   **冷档：判不了**（`drop_caches` 要 root）—— 缺什么见本文件头注那三条");
    eprintln!("║ ② 索引常驻字节");
    eprintln!(
        "║   {} 字节 / {} 条 ⇒ {:.1} 字节每条",
        stats.resident_bytes, stats.entries, bytes_per_entry
    );
    eprintln!(
        "║   线性外推到 640413 条 ⇒ 约 {:.1} MiB（`98 §3.3` 的原始体积是 77 MiB，那一份不含界桩）",
        ref_resident_mib
    );
    eprintln!("║ ③ 单次查询时延（按命中数分桶，暴力线性扫、零索引结构）");
    for (label, hits, us) in &buckets {
        eprintln!("║   命中 {hits:>6} 条 · {us:>6} µs · 针 `{label}`");
    }
    eprintln!("╚═════════════════════════════════════════════════════════════════");

    // ── 🔴 那条落点闸：**重走周期 ≥ 建索引耗时**（`设计/17 §6.9` F2 逐字）──
    //
    // 系数取 10 ⇒ 重走的**占空比不超过 10%**。
    // ⚠ 这是本文件唯一挂在墙钟上的闸，而它带着两个数量级的余量
    //   （300 000 ms 对约 1 000 ms）—— 机器慢 30 倍它才开口。
    let interval_ms = crate::files::index::REWALK_INTERVAL_SECS * 1000;
    assert!(
        interval_ms >= 10 * ref_build_ms,
        "🔴 声明的重走周期 {} 秒**撑不住**建索引的代价：\n\
         外推到 640413 条要 {} ms，10 倍占空比要求周期至少 {} ms。\n\
         ⇒ `设计/17 §6.9` 的 F2 逐字「重走周期 ≥ 建索引耗时」。\n\
         两条出路：把周期调大（并回 `index::REWALK_INTERVAL_SECS` 的头注里重写依据），\n\
         或者把遍历改快。**不许调小这里的系数** —— 那是在让重走悄悄吃掉这台机器。",
        crate::files::index::REWALK_INTERVAL_SECS,
        ref_build_ms,
        10 * ref_build_ms
    );

    // ── 语料自检：平均路径长要落在 `98 §3.3` 那个量纲附近 ──────────────
    //
    // 🔴 带**量化底**（`设计/17 §6.9` 那条现打逼出来的纪律：
    //    「相对偏差 ≤ 容差 **或** 绝对差 ≤ 1 个量化单位，两者取宽」）。
    //    这里的量化单位是**1 个字节**，而分母（根前缀长度）逐机不同 ⇒ 容差给得宽。
    let target = 126.0_f64;
    let rel = (mean_rel - target).abs() / target;
    assert!(
        rel <= 0.05 || (mean_rel - target).abs() <= 1.0,
        "语料的平均**相对**路径长 {mean_rel:.2} 字节离量纲 {target} 太远（相对偏差 {:.2}%）。\n\
         ⇒ 语料与 `真相源/98 §3.3` 那个量纲脱钩了，**本次读数作废** ——\n\
         回去调 `LEAF_NAME_LEN` / 目录名长度，**别改这条闸**。\n\
         ⚠ 这里量的是**扣掉根前缀之后**的长度：前缀逐机不同，把它算进来\n\
           就等于把这台机器的临时目录名烤进判据里。\n\
         ⚠ 那条 `或绝对差 ≤ 1 字节` 是**量化底**（`设计/17 §6.9` 现打逼出来的纪律）：\n\
           路径长是整数字节，均值的分辨率不可能优于 1 个字节。",
        rel * 100.0
    );
}
