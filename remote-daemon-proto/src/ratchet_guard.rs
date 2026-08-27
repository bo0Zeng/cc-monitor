//! `K-P1`（daemon 侧）：**两条只许收紧的棘轮。**
//!
//! 1. `KPY7`：**本件一行都不许放宽已有判据**（下面 `PINS` 那张表）。
//! 2. 〔08-27 回修第 7 处〕**已经删掉的构件，散文里不许还替它说话**
//!    （下面 `STALE_FALLBACK_PHRASES` + 那张**只许变短**的存量清单）。
//!
//! 两条同住一个模块，是因为它们是同一种东西：**存量只许变短的账**。
//! 第二条的来历见它自己那段头注。
//!
//! # 为什么只对**断言那几行**取指纹，而不是整张表
//!
//! 「加行是收紧、动断言是放宽」这句话**本身不是机检**。
//! 若对整张登记表取 md5 ⇒ **合法加行也会红** ⇒ 下一个人就会把它调松 ——
//! 而「把判据调松」是这一族里最坏的结局。
//! ⇒ 指纹只覆盖那几行**断言**，**表体排除在外**。
//!
//! # 量法（`brief` 第 11 条）
//!
//! 判「动没动某个闭集」不许用 `grep` 数加行。这里的量法是**整行相等 + 次数钉死**：
//! 每条锚点都要求在那份源码里**恰好出现 N 次**。
//! 它比 md5 好的地方是红的时候能说出**哪一行变了**；比 `contains` 强的地方是
//! 「撑大一格」也会红（`ssh_source` 那条 `pin_line` 的实测教训：`find_pinned` 放得过
//! `.map(|s| s)` 这种撑大，整行相等放不过）。
//!
//! # ⚠⚠ 口径：每条棘轮都要钉到**断言实参那一行**，常量声明行**顶不了它的班**
//!
//! 〔`K-P1-D1` `阻-2`，08-27 回修〕本表第一版的 4 条针里有 2 条钉的是**常量声明行**
//! （`const SPAWN_SITES_TODAY: usize = 9;` · `const MIN_SCANNED_CODE_BYTES: usize = 80_000;`），
//! 而**用**那个常量的断言**一行都没钉**。审计员据此切了一刀：把
//! `readonly_guard.rs` 那条 `assert_eq!(found.len(), SPAWN_SITES_TODAY,` **原地**换成
//! `assert!(found.len() >= SPAWN_SITES_TODAY,` ——「相等」当场变成「地板」，
//! 而实测 **460 passed / 0 failed，一条都没红**。
//! 逃得掉的原因是**那一刀一个字节都没碰被钉的那两行**：放宽发生在**原地**。
//!
//! ⇒ **这与下面「换个写法它会以『那一行不见了』的形式红」不是同一回事** ——
//! 只有当被钉的**就是断言自己那几行**时那句话才成立。
//! ⇒ 口径统一为：**凡钉一条棘轮，必须钉到它的断言实参行**；常量声明行只是配套的第二针
//! （它挡的是「不动断言、只把那个数调小」）。
//! ⚙ 这一格 monitor 侧那半（`local_daemon.rs::this_item_loosened_none_of_the_ratchets_it_touched`）
//! **本来就是对的** —— 它那 4 条针钉的全是断言实参行（`PLATFORM_EXCEPTIONS.len() <= 1,` ·
//! `missing.is_empty(),` · `found.len() >= 5,` · `found.len() >= 15,`）。
//! 松的是 daemon 侧这一份，**两侧口径今天对齐了**。
//!
//! ⚠ 修的是「**漏钉了哪一行**」，**不是换一种钉法** —— 上面那条「不对整张表取 md5」的
//! 理由今天一个字都没变。
//!
//! # 它**挡不住**什么
//!
//! 换个写法表达同一条断言（比如把 `assert_eq!` 拆成 `if … panic!`）它看不见 ——
//! 那时它会以「那一行不见了」的形式红，而那**正是要的**：结构变了就该有人回来重判。
//! 但反过来，**在别处新开一条更松的路**它看不见。如实登记，不假装它是证明。
//!
//! ⚠ 本模块刻意**不与被扫的代码同住一个文件**（`relay/bind_guard.rs` 头注给的理由：
//! 判据在自己的登记表 / 注释 / 常量里找到自己 ⇒ 恒绿）。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#![cfg(test)]

#[cfg(test)]
mod tests {
    /// `(文件, 那一行, 恰好几处, 它在守什么 · 本件与它什么关系)`
    const PINS: &[(&str, &str, usize, &str)] = &[
        (
            "readonly_guard.rs",
            "const SPAWN_SITES_TODAY: usize = 9;",
            1,
            "daemon 侧起进程登记表的**相等断言**（不是地板）。\
             `K-P1` 走的是 `K14` 裁的第一档（**不重起**）⇒ daemon 侧**一处都没加**，这个 9 必须原封不动。\
             ⚠ 走「甲：daemon 自己看住自己」那条路才会 9→10，而那是**动断言 = 放宽** —— \
             它已经摘出去成 `K-P3`，立件时要连这一格的代价一起写。\
             ⚠ 这一行**只挡「不动断言、只把 9 调小」**；「把断言原地调松」由下面两条实参针挡。",
        ),
        (
            "readonly_guard.rs",
            "found.len(),",
            1,
            "★★〔`D1` `阻-2` 回修〕那条**相等断言的第一个实参行**。\
             它与下面那条 `SPAWN_SITES_TODAY,` 合起来钉的是「这里用的是 `assert_eq!` 而不是地板」——\
             审计员实测：把 `assert_eq!(found.len(), SPAWN_SITES_TODAY,` 原地换成 \
             `assert!(found.len() >= SPAWN_SITES_TODAY,`，旧表 **460/0 一条都不红**。\
             ⇒ 这一行不见了 = 相等被换成了别的比较，或断言整个没了。\
             ★ 被守的那条断言**自己的报错文案里逐字写着**「**不许改回地板** —— \
             地板在变大方向上是瞎的」；今天这句话终于有人在钉。",
        ),
        (
            "readonly_guard.rs",
            "SPAWN_SITES_TODAY,",
            1,
            "同上：那条相等断言的**第二个实参行**。两行一起钉，是因为「换成地板」这一刀\
             会把它俩**同时**改掉，而单钉一行读起来会像「只是换了个变量名」。\
             ⚠ 常量**声明**行（上面第一条）与这一行是两件事：前者挡「把 9 调小」，\
             后者挡「不动 9、把 `==` 换成 `>=`」。",
        ),
        (
            "readonly_guard.rs",
            "unregistered.is_empty(),",
            1,
            "起进程的**默认拒绝**：不在 `ALLOWED` 里就红。本件没往它加行，这一行也不许动。",
        ),
        (
            "no_timer_guard.rs",
            "REGISTERED_DURATION_USES.len(),",
            1,
            "零定时器护栏的**恰好相等**断言那一行（`assert_eq!` 的第二个实参）。\
             ⚠ 那个名字在同一条断言的报错文案实参里还出现一次，但**那一行没有尾逗号** ——\
             整行相等的量法把两者分得开，这里钉的是**断言**那一处。\
             `K-P1` **一个 `Duration::from_*` 都没加**：`accept` 阻塞在内核事件上、\
             读一行也阻塞在内核事件上，没有任何会「自己醒过来」的构件。\
             ⇒ 这张表的条数与断言都必须原封不动。",
        ),
        (
            "no_timer_guard.rs",
            "const MIN_SCANNED_CODE_BYTES: usize = 80_000;",
            1,
            "零定时器护栏的**反空真地板**。本件往 daemon 加了两个文件（`listen.rs` / 两条守卫），\
             扫描面只增不减 ⇒ 这个地板没有任何理由被下调。**下调它就是让护栏在更小的面上绿着。**\
             ⚠ 同上：这一行**只挡「把 80_000 调小」**；「不动这个数、把断言原地调松」由下面那条实参针挡。",
        ),
        (
            "no_timer_guard.rs",
            "bytes >= MIN_SCANNED_CODE_BYTES,",
            1,
            "★〔`D1` `阻-2` 同族，收工前自查补的〕那条反空真地板的**断言实参行**。\
             `阻-2` 那一刀的形状是「**不碰被钉的常量，只把用它的那句断言调松**」——\
             这张表里凡是钉常量声明行的都有这个洞，本条补的是第二处。\
             把它写成 `bytes >= MIN_SCANNED_CODE_BYTES / 2,` 或 `bytes >= 1,` 时这一行会不见 ⇒ 红。",
        ),
    ];

    fn source_of(rel: &str) -> &'static str {
        match rel {
            "readonly_guard.rs" => include_str!("readonly_guard.rs"),
            "no_timer_guard.rs" => include_str!("no_timer_guard.rs"),
            other => panic!("本表里出现了没接语料的文件：{other}"),
        }
    }

    /// ★ 正题：那几行断言逐条按次数对上。
    #[test]
    fn this_item_loosened_none_of_the_daemon_side_ratchets() {
        for (file, line, want, why) in PINS {
            let src = source_of(file);
            // 反空真：语料读不到就下面全空转。
            assert!(src.len() > 2000, "{file} 只读到 {} 字节 —— 语料坏了", src.len());
            let n = src.lines().filter(|l| l.trim() == *line).count();
            assert_eq!(
                n, *want,
                "\n`{file}` 里 `{line}` 出现 {n} 次（应恰好 {want} 次）。\n\
                 说法：{why}\n\
                 ★ **变少 = 那条棘轮被改写或删掉了（放宽）**；变多 = 结构变了，回来重判。\n\
                 ⚠ 本条只对**断言那几行**取指纹 —— 往登记表里**加行是收紧**，本条不会因此红。"
            );
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // 棘轮二：**已经删掉的构件，散文里不许还替它说话**〔08-27 回修第 7 处〕
    // ══════════════════════════════════════════════════════════════════

    /// 「承诺一条周期性退路」的**闭集**。
    ///
    /// # 病根不是打错字
    ///
    /// `main.rs` 那句 warn 逐字写着「⇒ tmux hook 通路不可用，**退回定时探测**」——
    /// 而 `P5` 删掉 8s ticker 之后 daemon **零定时器**（`no_timer_guard` 钉着它，
    /// `observe/watcher.rs` 那条 `P0b-Y2` 头注逐字：「之后每一拍都靠事件」）。
    /// **那条退路不存在。**
    ///
    /// ★ **`P5` 删掉了一个构件，而替那个构件说话的散文散在别处，没人回去改** ——
    /// 然后它们继续以权威口吻骗下一个读者。这与本件逮到的「人群画在了错的文件上」同一根：
    /// **代码变了，而替代码说话的那些句子没跟着变。**
    ///
    /// # 人群：**生产段**（`production_code`）——说清它排除了什么
    ///
    /// 本条扫的是**会到日志/用户眼前的那些话**（字符串活下来，`//` 注释被剥掉）。
    /// ⇒ 它**管不到注释里的同一句谎**（今天已知 3 处，见下面清单的 ⚠ 段）。
    /// 这是刻意的：注释那一族要连 `//` 一起扫，而那会把本模块自己的说明文字也喂进去
    /// （`relay/bind_guard.rs` 头注那条：判据在自己的注释里找到自己 ⇒ 恒绿）。
    /// **如实登记为射程之外，不假装它覆盖了。**
    const STALE_FALLBACK_PHRASES: &[&str] = &["退回定时探测", "ticker 兜", "轮询兜"];

    /// **存量清单：只许变短。**`(相对 src 的路径, 还剩几处, 谁来退役它)`
    ///
    /// ⚠ 这不是豁免清单 —— 清单上的每一条都是**今天仍在说谎的一句话**。
    /// 它们不在 `K-P1` 回修那一轮的写区里（写区只有
    /// `ratchet_guard` / `single_stream_guard` / `listen` / `main`），
    /// **已上报 PM 立跟进件**。
    /// ⇒ 修掉一处就把这里的数改小；**改大 = 又新增了一句**，那正是本条要拦的。
    const STALE_FALLBACK_BACKLOG: &[(&str, usize, &str)] = &[
        (
            "observe/watcher.rs",
            2,
            "两句 warn 串：`:223`「拿不到自身 exe/starttime ⇒ 跳过装 tmux hook（退回定时探测）」·\
             `:1061`「监视 tmux socket 目录失败 …（复活仍由 ticker 兜）」。\
             两句都在承诺一条 `P5` 已经删掉的退路。**本轮写区之外，交 PM 立跟进件。**",
        ),
    ];

    /// ★★ 正题（棘轮二）：**生产段里承诺「周期性退路」的话只许变少。**
    ///
    /// 反向锚点在下面 `every_stale_fallback_entry_is_still_real` ——
    /// 没有它，把清单里的数改大就能「绿着过」，而那恰恰是本条要拦的方向。
    #[test]
    fn no_new_prose_promises_a_fallback_that_no_longer_exists() {
        let corpus = crate_production_sources();
        // 反空真①：语料塌了 ⇒ 下面全是 0 <= N 的空真。
        assert!(
            corpus.len() >= 30,
            "语料只有 {} 个文件（下限 30）—— 遍历坏了",
            corpus.len()
        );
        let bytes: usize = corpus.iter().map(|(_, c)| c.len()).sum();
        assert!(
            bytes >= 150_000,
            "语料只有 {bytes} 字节（下限 150_000）—— 剥过头了，本条此刻在空转"
        );
        // 反空真②：那三个短语**今天真的还在某处**（全 0 = 短语表腐烂了，本条恒绿）。
        let total: usize = corpus
            .iter()
            .map(|(_, c)| {
                STALE_FALLBACK_PHRASES
                    .iter()
                    .map(|p| c.matches(p).count())
                    .sum::<usize>()
            })
            .sum();
        assert!(
            total > 0,
            "全 crate 生产段一句都没命中那三个短语 —— 短语表腐烂了（或者真的清干净了：\
             那就把 `STALE_FALLBACK_BACKLOG` 清空并删掉本条这半个断言）"
        );

        for (rel, code) in &corpus {
            let n: usize = STALE_FALLBACK_PHRASES
                .iter()
                .map(|p| code.matches(p).count())
                .sum();
            let allowed = STALE_FALLBACK_BACKLOG
                .iter()
                .find(|(f, _, _)| f == rel)
                .map(|(_, k, _)| *k)
                .unwrap_or(0);
            assert!(
                n <= allowed,
                "\n`{rel}` 的生产段里有 {n} 句在承诺一条周期性退路（存量清单登记 {allowed} 句）。\n\
                 短语闭集：{STALE_FALLBACK_PHRASES:?}\n\
                 ★ **`P5` 删掉 8s ticker 之后 daemon 零定时器**（`no_timer_guard` 钉着）——\n\
                 「不可用但有兜底」与「不可用而且没兜底」是两件完全不同的事，\n\
                 而写着前者的那句话会以权威口吻骗下一个读者。\n\
                 ⇒ 要么把那句话改成真话（说清**什么事会变得发现不了** + **下一步能做什么**），\n\
                 要么先真的把那条退路做回来。**这张清单只许变短。**"
            );
        }
    }

    /// ★ 反向锚点（棘轮二）：存量清单上的每一条**今天真的还在**。
    ///
    /// 少了它，把清单里的数写大就能永远绿着 —— 而「数写大」正是这条棘轮要拦的那个方向。
    #[test]
    fn every_stale_fallback_entry_is_still_real() {
        let corpus = crate_production_sources();
        for (rel, want, why) in STALE_FALLBACK_BACKLOG {
            let code = corpus
                .iter()
                .find(|(f, _)| f == rel)
                .map(|(_, c)| c.as_str())
                .unwrap_or_else(|| panic!("存量清单指着 `{rel}`，而语料里没有它 —— 它搬家了"));
            let n: usize = STALE_FALLBACK_PHRASES
                .iter()
                .map(|p| code.matches(p).count())
                .sum();
            assert_eq!(
                n, *want,
                "`{rel}` 实测 {n} 句、清单登记 {want} 句。\n说法：{why}\n\
                 ★ 少了 ⇒ **有人修好了**，把这个数改小（清单只许变短）；\n\
                 多了 ⇒ 上面那条会先红。**别把数改大让今天好过。**"
            );
        }
    }

    /// 全 crate 生产段。`scan_tree!` **按构造**摘除调用者自己那份 ——
    /// 本模块的短语表逐字带着那三个短语，不摘就是「判据在自己的登记表里找到自己 ⇒ 恒绿」。
    /// ⚠ 不许改回裸 `read_dir`：monitor 侧
    /// `scanning_guard_registry::no_new_guard_walks_the_tree_without_excluding_itself`
    /// 是一条**递减棘轮**，08-27 当场把一份裸遍历打红过。
    fn crate_production_sources() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        guard_core::scan_tree!(&root, &["rs"])
            .into_iter()
            .map(|(path, src)| {
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, crate::guard_support::production_code(&src))
            })
            .collect()
    }

    /// ★ 反向锚点：这几条针**不是随手写的字符串**，它们今天真的都在。
    ///
    /// 少了这一条，把某一行的 `want` 改成 0 就能让它「绿着通过」——
    /// 而 0 次恰恰是「语料坏了」与「断言被删了」共同的样子。
    #[test]
    fn every_pin_points_at_a_line_that_exists_today() {
        for (file, line, want, _) in PINS {
            assert!(*want > 0, "`{file}` / `{line}` 登记成 0 处 —— 零命中的锚点钉不住任何东西");
            assert!(
                source_of(file).lines().any(|l| l.trim() == *line),
                "`{file}` 里一行都不等于 `{line}` —— 锚点腐烂了，本表在假绿"
            );
        }
    }
}
