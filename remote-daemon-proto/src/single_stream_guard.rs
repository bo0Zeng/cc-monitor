//! `K-P1 KPY8`：**「多客户端的流」本件明确不做 —— 留一个触发器，不留一句话。**
//!
//! # 它是什么形状
//!
//! **今天绿、那天故意红**（形状抄 `wire::tests::production_hello_leaves_homes_empty_so_claude_bytes_stay_frozen`
//! 的那一条：「它会在那天**故意变红**，那是提醒，不是障碍」）。
//!
//! 「写成『以后记得』= **没有触发器**」这一族本仓治过一次，逐字住
//! `src-tauri/src/backend/mod.rs:22-26`：「那是个**没有触发器的等待**…
//! **别再把这句读成「等某个人想起来」**」。⇒ 本件明确不做的那一半必须有东西钉着。
//!
//! # 它钉的是哪三处「恰好一个客户端」
//!
//! `K-P1 §0b-2㈢` 逐条点了名，本模块把它们做成**带次数的锚点**（`brief` 第 7 条：
//! 每一刀先断言锚点恰好命中 N 次）：
//!
//! 1. **`HelloFlushed` 是类型级见证** —— 换载体那一格是 drop-in（两边本来就是泛型），
//!    但**多连接那天要么每连接一份见证，要么把见证降级 —— 降级是放宽，不许**。
//!    那一格由 `wire::tests::the_hello_witness_has_exactly_one_way_to_exist` 钉着，
//!    本模块**不重复钉它**，只在这里指住它的住址。
//! 2. **观测帧只有一个消费者** —— `observe::watcher::spawn` 里那条通道**只诞生一次**。
//!    N 个连接要 fan-out，而 `Overflow.lost` 的丢帧账是**按那一个通道**记的
//!    （`wire.rs` 的 `LostFrame` 头注逐字：状态增量帧「是一次差分的结果，**别处不存在**」）
//!    ⇒ 分流之后「丢了哪几帧」这本账要**重新定义**。★ **那是语义变更，不是搬运。**
//!    ⚠ 这一格的针 08-28 挪过位置（`K-G5`）——怎么挪的、买到什么、买不到什么，见下一节。
//! 3. **`writer_task` 的让位预算是「两条通道对一个 writer」** —— `REPLY_BURST = 8`，
//!    那个数是 D 审计实测「500ms 内应答 70789 条 / 实时行 4 条」之后加的。
//!    每连接一个 writer 之后，这条预算要**按连接算**。
//!
//! # ★ 第 2 格那根针挪到了「通道的诞生点」〔`K-G5`，08-28〕
//!
//! **为什么挪 —— 旧针钉错了地方。** 旧针数的是**类型名** `mpsc::Receiver<Frame>` 在生产段里
//! 出现几次（登记 3），而那个 3 里有 **2 处是 `main.rs::writer_task` 的形参**。形参是
//! 「**同一条**通道的消费端被传进来」，不是「第二条通道」⇒ 说法（「只有一个消费者」，说的是
//! **谁在收**）比针宽了一格。`K-G2` 的 `D2` 实打这个洞：在 `writer_task` 前插一个借用同一条
//! 通道的 helper（`#[allow(dead_code)] async fn d2_drain_one(rx: &mut …Receiver<Frame>)`，
//! **零语义变更的纯重构**）⇒ 计数 3→4，**当场红**（`D2` 当轮判定行 464/0 → 463/1）。
//! ★★ **假阳会训练人绕过判据，比没有判据更坏。**
//! ⚠ 治法**不是把 3 改成 4** —— 下一次插第二个 helper 又红，那是同一个病推迟一轮。
//!
//! - **它数的是什么**：生产段里 `mpsc::channel::<Frame>(` 出现几次 —— **按这一个拼法**，
//!   就是**这个进程里造出过几条搬 `Frame` 的 mpsc 通道**（通道的**诞生点**，不是类型名的
//!   出现处）。⚠ 拼法之外的造法它一处都看不见，见下面「保证不了什么」①。
//!   今天 3 = 观测 1（`observe/watcher.rs`）+ 应答 2（`main.rs`：stdio 那条 + 常驻那条，
//!   与 `writer_task(` 那条针的 2 同源）。
//! - **它因此能保证什么**：`tokio` 的 `mpsc::Receiver` **不可 clone**（类型层面挡着）
//!   ⇒ 一条通道**至多**一个消费端 ⇒ **通道诞生数是 `Frame` 消费端数的上界**
//!   （是上界不是等式：造了通道而把 receiver 立刻扔掉，消费端就是 0）。于是
//!   「再造一条 `Frame` 通道」（fan-out 落地的必经一步）**当场红**；而**传递 / 借用 /
//!   move 一个已有的 `Receiver<Frame>`**（抽 helper、把 rx 交给另一个 task、把返回元组
//!   改成 struct）**一处都不增本针的计数** —— 那正是旧针分不开的那一格。
//! - **它保证不了什么**（诚实边界，别读成证明）：
//!   1. **只认 `mpsc::channel::<Frame>(` 这一个拼法。** 不带 turbofish（靠左边的类型标注
//!      定型）、换成 `broadcast` / `watch` 这类多消费者原语、或给 `Frame` 起个类型别名再造
//!      通道 —— 本条**一处都看不见**。
//!   2. 它数的是**造了几条通道**，不是**几个人在收**：一条通道被 `Arc<Mutex<_>>` 包起来给
//!      两个 task 轮流 `recv()`，计数不变，而消费者真的是两个。
//!   3. 剥法的边界在 `src-tauri/crates/guard-core/src/lib.rs`：`production_code` 只剥
//!      `#[cfg(test)] mod` 与**整行** `//` 注释 —— **行尾注释与块注释不剥**，
//!      在那里写一句这个字面量照样计入。**别把它读成「注释都剥干净了」。**
//!   4. 它只管「观测帧那条通道有没有第二个消费端」；`Overflow.lost` 那本账**该怎么**重新
//!      定义，判据一个字都不管 —— 那要立件。
//!
//! # 它**挡不住**什么（说清楚，别让人以为它是证明）
//!
//! 它认的是下面那张表里的**字面锚点与次数**。有人把 fan-out 写成一个新模块、
//! 而这几处字面一个都不动 ⇒ 本条照样绿。⇒ 它买的是「**动到这三处时有人会被拦一下**」，
//! 不是「fan-out 不可能被实现」。这一句是照 `relay/bind_guard.rs` 头注那条订正写的：
//! 那条守卫**只认 4 个拼写，别的一律看不见**，而它先前把自己写得比这更强。
//!
//! ⚠ **本模块刻意与被扫的代码不同住一个文件**（`relay/bind_guard.rs` 头注逐字给的理由：
//! 判据在自己的登记表 / 注释 / 常量里找到自己 ⇒ 恒绿）。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#![cfg(test)]

#[cfg(test)]
mod tests {
    use crate::guard_support::production_code;

    /// `(锚点住的那个文件, 锚点, **那个文件里**恰好几处, **全 crate** 恰好几处, 它一变就意味着什么)`。
    /// **次数钉死**（`KP4` 第四轮买牙的两个价钱之一）。
    ///
    /// # ⚠⚠ 为什么多了「全 crate」这一列〔`K-P1-D1` `重-5`，08-27 回修〕
    ///
    /// 回修前本表只有「那个文件里几处」这一个数，而好几条的**说法**写的是「全 crate 只此一处」——
    /// 审计员点名 `Admit::Stream` 那条：语料是 `source_of("listen.rs")`，**只扫 `listen.rs`**，
    /// 于是「有人在 `main.rs` 里直接构造一个 `listen::Admit::Stream` 绕过 `admit()`」这条针看不见。
    /// ⇒ **守卫范围 ≠ 性质范围**，那是本件自己命名的那一族的**第五形**。
    ///
    /// ★ 本轮的处置是**扩人群到它自称的那个范围**（不是把说法改窄）——
    /// 理由：改窄会把真正要挡的东西（「在别处新开一条更松的路」）整个放掉。
    /// ⚠ 而且**六条一起改**，不是只改被点名的那一条（`brief` 第 15 条那一问：
    /// 我治的是这一处，还是所有同职的地方？）—— 现打，六条里有 **4 条**的说法都越了它的语料：
    /// `busy.swap(true` 逐字写着「全 crate 只此一处」、`writer_task(` / `inbound::spawn(` 写「同一个进程里」、
    /// `mpsc::Receiver<Frame>` 写「只有一个消费者」。
    /// ⚠ **上面这一段是 08-27 那一刻的快照**：`mpsc::Receiver<Frame>` 那条针 08-28 已经被
    /// `K-G5` 换成了通道的**诞生点**（`mpsc::channel::<Frame>(`）—— 扩人群治不了它，
    /// 因为它的病不在人群而在**针本身钉错了地方**（它把 `writer_task` 的形参算进那个 3）。
    ///
    /// ⚙ 全 crate 那个数**可以大于**文件内那个数，而且大得有名有姓 —— 逐条在说法里写清多出来的是谁。
    const PINS: &[(&str, &str, usize, usize, &str)] = &[
        (
            "main.rs",
            "writer_task(",
            2,
            2,
            "出方向的写者**一条载体一个**：stdio 那条 + 常驻那条。\
             第三处意味着同一个进程里同时有两条流在写 —— 那正是 fan-out 的入口，\
             而两条流各自记各自的 `Overflow.lost` 之后，那本账就不再是「这台机丢了几帧」。\
             ⇒ 全 crate 也是 2：`main.rs` 之外一处都没有，**换个文件写第三条也会红**。",
        ),
        (
            "main.rs",
            "inbound::spawn(",
            2,
            2,
            "入方向 reader 同样**一条载体一个**。多一处 = 多一个能发 `launch`/`kill` 的对端，\
             而 `inbound::REGISTRY` 的取消登记表是**一份**（按 id 去重），两个对端会撞 id。\
             ⇒ 全 crate 也是 2（`wire.rs` 头注里那一处提及是 `///` 行，被 `production_code` 剥掉）。",
        ),
        (
            "main.rs",
            "busy.swap(true",
            1,
            1,
            "「谁拿到那一条流」由**一次原子操作**决出来，**全 crate 只此一处**。\
             改成「先查后写」就有窗口（两条连接同时握手都拿到流）；\
             多一处则意味着有第二个地方在发牌。\
             ⚠ 这条的说法从第一版起就写着「全 crate」，而语料到今天才真的是全 crate。",
        ),
        (
            "observe/watcher.rs",
            "mpsc::channel::<Frame>(",
            1,
            3,
            "观测帧**只有一个消费者**，钉在那条通道的**诞生点**上：`watcher::spawn` 里\
             `mpsc::channel::<Frame>(CHANNEL_CAPACITY)` 那一句，**全 crate 只此一次造观测通道**。\
             `tokio` 的 `Receiver` 不可 clone ⇒ 一条通道恰好一个消费端 ⇒ 再造一条 `Frame` 通道\
             就是 fan-out 落地，而 `Overflow.lost` 的丢帧账是按那**一个**通道记的\
             ⇒ 语义变更，不是搬运。\
             ⇒ 全 crate 3 = 观测 1 + `main.rs` 的应答通道 2（stdio 那条 + 常驻那条，\
             与 `writer_task(` 那条针的 2 同源）。第 4 处出现时回来重判它是哪一种。\
             ⚠ 〔`K-G5` 08-28〕本行从 `mpsc::Receiver<Frame>`（数**类型名出现几处**）换成**诞生点**：\
             旧针把 `writer_task` 的两个**形参**算进那个 3，于是抽一个借用同一条通道的 helper\
             这种**零语义变更的纯重构**会红（`K-G2 D2` 实打）。传递 / 借用 / move 一个**已有的**\
             `Receiver<Frame>` 不增本行计数；射程与诚实边界逐条写在本模块头注。",
        ),
        (
            "listen.rs",
            "Admit::Stream",
            1,
            2,
            "「交出流」这一档**全 crate 只此一处产出**（`listen::admit` 的那一臂）。\
             ⇒ 全 crate 2 = 那一处产出 + `main.rs` 里 `match` 的**模式**那一处。\
             第 3 处 = 有人绕过 `admit()` 自己造了一个 `Admit::Stream`，也就是有第二条路能拿到流。\
             ★ 这正是 `D1` `重-5` 点名的那一格：回修前语料只有 `listen.rs`，`main.rs` 里\
             直接构造一个它**看不见**。",
        ),
        (
            "main.rs",
            "REPLY_BURST",
            2,
            2,
            "让位预算是「两条通道对**一个** writer」（常量 1 处 + 比较 1 处）。\
             每连接一个 writer 之后这条预算要按连接算 —— 那时本行的数会变，正好逼人回来重判。\
             ⇒ 全 crate 也是 2：这条预算**不许有第二个家**。",
        ),
    ];

    /// 全 crate 语料：`src/` 下**全部**（含子目录）`.rs` 的生产段。
    ///
    /// # ⚠ 为什么走 `scan_tree!` 而不是自己 `read_dir`
    ///
    /// 〔08-27，回修当天被本仓自己的判据逮到，如实留档〕本函数第一版是裸递归 `read_dir`
    /// + 一张 `SKIPPED_BY_NAME` 自摘表，**当场被 monitor 侧的
    /// `scanning_guard_registry::no_new_guard_walks_the_tree_without_excluding_itself` 打红**
    /// （它是一条**递减棘轮**：存量清单只许变短）。
    /// 那条判据的理由逐字：「判据在自己的登记表 / 注释 / 常量里找到自己 ⇒ **恒绿**，
    /// audit-0805 实测五次，**五次都不是被判据变红发现的**」——
    /// 而本模块的 `PINS` 里逐字带着那六个锚点，正是那一形。
    /// ⇒ `scan_tree!` **按构造**摘除调用者自己那份，摘不摘得掉不靠我记得写那张表。
    ///
    /// ⚠ 把自己加进那张存量清单是**放宽**（`KPY7` 逐字：本件一行都不许放宽已有判据）——
    /// 一个字都没往那边加。
    ///
    /// 递归这件事由 `scan_tree!` 自己保证；`no_timer_guard::daemon_sources` 头注记着
    /// 不递归的实测后果：**目录没有扩展名于是被整个跳过**，护栏一行业务代码都没扫还全绿。
    fn crate_sources() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        guard_core::scan_tree!(&root, &["rs"])
            .into_iter()
            .map(|(path, src)| {
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, production_code(&src))
            })
            .collect()
    }

    fn source_of(rel: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join(rel);
        production_code(&std::fs::read_to_string(&p).unwrap_or_else(|e| {
            panic!("读不到 {} —— 文件搬家了就来改本表：{e}", p.display())
        }))
    }

    /// ★★ `重-5`：**没有一个锚点在别处还有第二个家。**
    ///
    /// 上面那条只扫「锚点住的那个文件」；本条把人群扩到**它们自称的那个范围**（全 crate）。
    /// 少了本条，「全 crate 只此一处」这句话就只是一句散文 ——
    /// `D1` `重-5` 实测：有人在 `main.rs` 里直接构造一个 `listen::Admit::Stream`
    /// 绕过 `admit()`，回修前那条针**看不见**。
    #[test]
    fn none_of_these_anchors_has_a_second_home_anywhere_in_the_crate() {
        let corpus = crate_sources();
        // 反空真①：语料塌了 ⇒ 下面全是 0 == 0 的空真。
        assert!(
            corpus.len() >= 30,
            "全 crate 语料只有 {} 个文件（下限 30）—— 遍历坏了（多半是没递归进子目录）",
            corpus.len()
        );
        let bytes: usize = corpus.iter().map(|(_, c)| c.len()).sum();
        assert!(
            bytes >= 150_000,
            "全 crate 语料只有 {bytes} 字节（下限 150_000）—— 剥过头了，本条此刻在空转"
        );
        // 反空真②：本护栏自己**真的**被摘掉了。
        // `scan_tree!` 是按构造摘的，本条是**复核那一刀真的落下了** ——
        // 摘除退化成空集或匹配不上时，`PINS` 里那六个锚点会把自己各喂一口，六条一起假红。
        assert!(
            !corpus
                .iter()
                .any(|(rel, _)| rel.ends_with("single_stream_guard.rs")),
            "本护栏自己进了语料 —— 它的 `PINS` 里逐字带着这六个锚点，那样每条都会多数出来"
        );

        for (home, needle, _file_want, crate_want, why) in PINS {
            let per_file: Vec<(String, usize)> = corpus
                .iter()
                .map(|(rel, code)| (rel.clone(), code.matches(needle).count()))
                .filter(|(_, n)| *n > 0)
                .collect();
            let got: usize = per_file.iter().map(|(_, n)| n).sum();
            assert_eq!(
                got, *crate_want,
                "\n全 crate 里 `{needle}` 有 {got} 处，登记的是 {crate_want} 处（主场 `{home}`）。\n\
                 分布：{per_file:?}\n\
                 登记说法：{why}\n\
                 ★ **这多半不是 bug，是提醒**：`K-P1 §2` 明确不做「多客户端的流」。\n\
                 ⚠ 本条与上面那条的差别就是**人群**：那条只看主场文件，本条看全 crate ——\n\
                 「在别处新开一条更松的路」只有本条看得见（`D1` `重-5` 的那一格）。"
            );
        }
    }

    /// ★ 正题：三处「恰好一个客户端」的锚点**逐个按次数**对上。
    #[test]
    fn the_single_stream_shape_is_still_exactly_one_client() {
        for (file, needle, want, _crate_want, why) in PINS {
            let src = source_of(file);
            // 反空真：切出来的必须是真代码，不是一个空串。
            assert!(
                src.len() > 500,
                "{file} 的生产段只有 {} 字节 —— 抽取坏了，本条此刻在空转",
                src.len()
            );
            let got = src.matches(needle).count();
            assert_eq!(
                got, *want,
                "\n`{file}` 里 `{needle}` 有 {got} 处，登记的是 {want} 处。\n\
                 登记说法：{why}\n\
                 ★ **这多半不是 bug，是提醒**：`K-P1 §2` 明确不做「多客户端的流」——\n\
                 fan-out 与 `Overflow.lost` 丢帧账的重新定义留给**第二个要「流」的客户端**出现那天。\n\
                 真到了那天：先立件，把那本账重新定义清楚，再回来改这张表；\n\
                 只是重构动到了这几处：把新的数与理由一起写进来。"
            );
        }
    }

    /// ★ 反向锚点：**每个锚点今天都真的找得到**。
    ///
    /// 少了这条，把 `PINS` 里某一行的 `want` 改成 0 就能让它「绿着通过」——
    /// 而 0 处恰恰是「抽取器坏了」与「代码被删了」共同的样子。
    #[test]
    fn every_pin_is_a_live_anchor_not_a_zero() {
        for (file, needle, want, crate_want, _) in PINS {
            assert!(
                *want > 0,
                "`{file}` / `{needle}` 登记成 0 处 —— 零命中的锚点钉不住任何东西"
            );
            // 全 crate 那个数**不许小于**主场那个数 —— 小于就说明这两列有一列是编的。
            assert!(
                *crate_want >= *want,
                "`{file}` / `{needle}`：全 crate 登记 {crate_want} 处 < 主场 {want} 处 —— \
                 两列里必有一列不是量出来的"
            );
            assert!(
                source_of(file).contains(needle),
                "`{file}` 里找不到锚点 `{needle}` —— 锚点腐烂了，本表在假绿"
            );
        }
    }

    /// ★★ `K-G5`：观测帧那根针认的是**通道的诞生点**，不是**类型名出现几处**。
    ///
    /// 本条把那件事的**双向**死值验钉进判据自己 —— 不然它只是头注里的一段散文，
    /// 下一个人把针改回类型名（或者把那个数往上调一格）时，**没有任何东西会红**。
    ///
    /// - **假阳那一侧**（这是本件买的东西）：`K-G2` `D2` 那一刀的形状 —— 一个**借用同一条
    ///   通道**的 helper（零语义变更的纯重构）⇒ 本针**必须 0 命中**。
    ///   旧针在这一格上是 1，于是全 crate 计数 3→4 当场假红。
    /// - **真违规那一侧**（这是没被拔牙的证据）：真的再造一条 `Frame` 通道 ——
    ///   `Receiver` 不可 clone，第二个消费端**只能这么来** ⇒ 本针**必须命中**。
    ///   缺了这一格，「消假阳」最省事的走法就是把针改成恒不命中，而**恒绿判据 = 假绿**。
    #[test]
    fn the_observation_pin_counts_channel_births_not_type_name_mentions() {
        let needle: &str = PINS
            .iter()
            .find(|(home, _, _, _, _)| *home == "observe/watcher.rs")
            .expect("`PINS` 里 `observe/watcher.rs` 那一行不见了 —— 观测帧那一格的针没了")
            .1;
        // 假阳那一侧：`D2-M4` 逐字那一刀（**借用**同一条通道，没造第二条）。
        let pure_refactor = "#[allow(dead_code)] async fn d2_drain_one(\
                             rx: &mut tokio::sync::mpsc::Receiver<Frame>) \
                             -> Option<Frame> { rx.recv().await }";
        assert_eq!(
            pure_refactor.matches(needle).count(),
            0,
            "针 `{needle}` 在一次**零语义变更的纯重构**上有命中 —— 它又在数「类型名出现几处」了。\n\
             ★ `K-G5 §0a` 写死：治法**不是**把那个数改大一格（下一个 helper 又红 = 同一个病\n\
             推迟一轮），而是让它数**产出 / 诞生点**：谁在造通道，不是谁提到了那个类型名。"
        );
        // 真违规那一侧：真的第二条 `Frame` 通道 = 真的第二个消费端。
        let real_second_consumer = "let (tx2, rx2) = tokio::sync::mpsc::channel::<Frame>(8);";
        assert!(
            real_second_consumer.contains(needle),
            "针 `{needle}` 认不出「再造一条 `Frame` 通道」——\n\
             消假阳把牙一起拔了，而恒绿的判据就是假绿。"
        );
    }

    /// ★ 那条**不由本模块钉**的（`HelloFlushed` 见证）要指得住住址。
    ///
    /// `§0b-2㈢`-1 逐字：多连接那天「要么每连接一份见证，**要么把见证降级 —— 降级是放宽，不许**」。
    /// 那一格的判据住 `wire.rs`；本条只保证**那条判据还在**，不重复实现它
    /// （两份实现迟早分叉 —— 本仓最贵的那一族）。
    #[test]
    fn the_hello_witness_pin_still_lives_where_this_module_says_it_does() {
        let wire = include_str!("wire.rs");
        assert!(
            wire.contains("fn the_hello_witness_has_exactly_one_way_to_exist"),
            "本模块头注指着 `wire.rs` 的那条见证判据，而它已经不在了 ——\n\
             ⇒ 「hello 先于 reader」在换了载体之后就只剩一句散文。"
        );
    }
}
