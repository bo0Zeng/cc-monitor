//! **多客户的流：整台一份 watcher、每条连接一份投影 —— 留一个触发器，不留一句话。**
//!
//! **整台一份** watcher（会话账本 · 读者 · 运行簿整台一份），每条连接经 `watcher::WatcherPoke::attach` 拿一份
//! **投影**：它自己的有界出方向通道 ＋ 名单 ＋ 欠的丢帧账（丢帧账按连接记：「这条连接那一条通道」，见 `watcher::Sub`）。
//! 观测通道只有一个诞生点（在 `attach` 里，按连接实例化）；下面几根针钉的是
//! 「源码里恰好一份，按连接实例化」。
//!
//! # 它是什么形状
//!
//! **今天绿、那天故意红**：它会在那天故意变红，那是提醒，不是障碍。
//!
//! 「写成『以后记得』= **没有触发器**」这一族本仓治过一次，逐字住当时 monitor 侧 backend 目录那份头注
//! （随目录删了）：「那是个**没有触发器的等待**…
//! **别再把这句读成「等某个人想起来」**」。⇒ 明确不做的那一半必须有东西钉着。
//!
//! # 它钉的是哪三处「恰好一个客户端」
//!
//! 逐条点了名，本模块把它们做成**带次数的锚点**（
//! 每一刀先断言锚点恰好命中 N 次）：
//!
//! 1. **`HelloFlushed` 是类型级见证** —— 换载体那一格是 drop-in（两边本来就是泛型），
//!    但**多连接那天要么每连接一份见证，要么把见证降级 —— 降级是放宽，不许**。
//!    那一格由 `wire::tests::the_hello_witness_has_exactly_one_way_to_exist` 钉着，
//!    本模块**不重复钉它**，只在这里指住它的住址。
//! 2. **观测帧每条连接一个消费者** —— `observe::watcher::WatcherPoke::attach` 里那条通道**只有这一个诞生点**（每接一条连接走一次）。
//!    N 个连接的 fan-out 在 watcher 里，丢帧账因此**按连接**记
//!    （状态增量帧「是一次差分的结果，**别处不存在**」——
//!    一条连接丢了，别的连接收到了也不算它收到）。★ **那是语义变更，不是搬运**：账重定义成「这条连接那一条通道」。
//!    ⚠ 这一格的针钉在通道的诞生点上 —— 为什么、买到什么、买不到什么，见下一节。
//! 3. **`writer_task` 的让位预算是「两条通道对一个 writer」** —— `REPLY_BURST = 8`，
//!    那个数是 D 审计实测「500ms 内应答 70789 条 / 实时行 4 条」之后加的。
//!    每连接一个 writer 之后，这条预算要**按连接算**。
//!
//! # ★ 第 2 格那根针钉的是「通道的诞生点」，两条
//!
//! **不数类型名。** 数 `mpsc::Receiver<Frame>` 在生产段里出现几次的话，`stream/writer.rs::writer_task` 的形参
//! 也算进去 —— 形参是「**同一条**通道的消费端被传进来」，不是「第二条通道」⇒ 说法（「只有一个消费者」，
//! 说的是**谁在收**）比针宽了一格：在 `writer_task` 前插一个借用同一条通道的 helper
//! （**零语义变更的纯重构**）也会红。
//! ★★ **假阳会训练人绕过判据，比没有判据更坏。** 治法也不是把数改大 —— 下一个 helper 又红。
//!
//! ## ⚠⚠ 只钉 turbofish 不够：一个**正常拼法**就能塞进第二个真消费者
//!
//! ```text
//! let (_tx2, mut _rx2): (tokio::sync::mpsc::Sender<Frame>, tokio::sync::mpsc::Receiver<Frame>) =
//!     tokio::sync::mpsc::channel(8);
//! tokio::spawn(async move { while let Some(_f) = _rx2.recv().await {} });
//! ```
//!
//! 一个**真的**第二消费者（真 task、真 `recv`），只是靠**左边的类型标注**定型而不是 turbofish ——
//! 只有 ① 的话它静默走过去。
//!
//! ⇒ 治法**不是把 ① 的数改大**（`§0a` 那条承重线仍然写死），而是**再钉一条**：
//! **② 生产段里不许有「带路径前缀、却不带 turbofish」的通道诞生点**（锚点逐字 `::channel(`）。
//! 两条合起来，「造一条搬 `Frame` 的 mpsc 通道」在**这两条锚点够得着的那批拼法内**
//! 只剩 turbofish 一种写法，而那一种由 ① 数着。
//! ⚠ **够不着的那批逐条列在下面「保证不了什么」1–4** —— 别把上面这句读成「只剩一种拼法」。
//!
//! - **它数的是什么**（两条针，各数各的）：
//!   ① **搬 `Frame` 的 mpsc 通道的诞生点**，不是类型名的出现处（`PINS` 里两行）：
//!      观测通道 `mpsc::unbounded_channel::<(Frame, usize)>(` = 1（`observe/watcher.rs`，按字节记账的那条）；
//!      应答通道 `mpsc::channel::<Frame>(` = 2（`main.rs`：stdio 那条 + 常驻那条，与 `writer_task(` 那条针的 2 同源）。
//!   ② 生产段里 `::channel(` 出现几次 = **0**
//!      （[`tests::no_qualified_channel_call_leaves_its_payload_type_to_inference`]）——
//!      一处「**带路径前缀而把载荷类型交给推导**」的诞生点都没有。
//!      ⚠ 它是一条**关于拼法**的棘轮，不是关于 `Frame` 的：谁的载荷都算。
//! - **它因此能保证什么**：`tokio` 的 `mpsc::Receiver` **不可 clone**（类型层面挡着）
//!   ⇒ 一条通道**至多**一个消费端 ⇒ **通道诞生数是 `Frame` 消费端数的上界**
//!   （是上界不是等式：造了通道而把 receiver 立刻扔掉，消费端就是 0）。
//!   ② 把「**带路径前缀的**诞生点必须自报载荷类型」变成硬约束 ⇒ ① 在**它够得着的那批拼法内**
//!   是完整的（够不着的逐条写在下面）。于是「再造一条 `Frame` 通道」（fan-out 落地的必经一步）
//!   **当场红**：turbofish 那一形被 ① 逮，类型标注 / 全推导那一形被 ② 逮。
//!   而**传递 / 借用 / move 一个已有的 `Receiver<Frame>`**（抽 helper、把 rx 交给另一个
//!   task、把返回元组改成 struct）**两条针一处都不增** —— 那正是旧针分不开的那一格。
//! - **② 的代价，先说清楚它是哪一种**：它**过宽** —— 新造一条与 `Frame` 毫无关系的通道，
//!   只要不写 turbofish 也会红。⚠ 但这笔代价与旧针那笔**方向相反**：旧针的假阳出在
//!   **零语义变更的纯重构**上（`§0a` 的承重线正是这一条），而 ② 只在**真的新造了一条通道**
//!   时出声，且它的解法是「**把载荷类型写出来**」——**不是放宽判据**。纯重构一处都不碰它。
//! - **它保证不了什么**（诚实边界，别读成证明）：
//!   1. **② 要求 `::` 紧挨着 `channel(`。** `use tokio::sync::mpsc::channel;` 之后裸调
//!      `channel(8)` / `channel::<Frame>(8)` —— ①②**都看不见**。
//!      （本模块在 `tests/backend/`，不在分母里 —— 见 [`crate_sources`] 头注。
//!      ★ **分母的单位与量法要与判据自己印的那个数同源**：
//!      字节 = `corpus.iter().map(|(_, c)| c.len()).sum()`，也就是本条那道字节下限
//!      （`bytes >= 150_000`）的报错模板印的**同一个数** —— 把那个下限临时调到 `9_000_000`
//!      逼判据自己印出真值，是复跑这个分母的权威路子。
//!      `c.len()` 是 **UTF-8 字节数**，不是字符数（这份语料里中文注释很多，两者差几千）。
//!      量法 = 与 `crate_sources()` 同一份剥法逐文件数子串：今天全 crate 没有一处 import 到
//!      `mpsc` 的函数那一级（`use tokio::sync::mpsc` 都停在 `::mpsc;`）。
//!      **这是读数，不是不变量** —— 引用前重打。）
//!   2. **别的构造函数名**：`std::sync::mpsc::sync_channel(4)` 里 `::` 不紧挨 `channel(`
//!      ⇒ ② 看不见。今天那一处（`relay/tee.rs`）写着 turbofish 且与 `Frame` 无关。
//!   3. **给 `Frame` 起个类型别名再造通道**（`type F = Frame; mpsc::channel::<F>(8)`）：
//!      有 turbofish ⇒ ② 不红；字面不是 `Frame` ⇒ ① 也不数。**两条都看不见。**
//!   4. **`broadcast` / `watch` 这类多消费者原语**带 turbofish 写出来时同理：② 不红、① 不数。
//!      而多消费者那天真正会走的路，很可能就是这一条。
//!   5. 它数的是**造了几条通道**，不是**几个人在收**：一条通道被 `Arc<Mutex<_>>` 包起来给
//!      两个 task 轮流 `recv()`，计数不变，而消费者真的是两个。
//!   6. 剥法的边界在 `src/common/guard-core/src/lib.rs`：`production_code` 剥 `#[cfg(test)] mod`、
//!      整行与行尾 `//`、块注释 `/* … */` ⇒ 在注释里写一句 `mpsc::channel::<Frame>(` 不计入本模块的任何计数。
//!      ⚠ 边界在那条**静默兜底**：剥法的词法与某份文件对不上时
//!      （`depth` 不收口 / 停在字符串里）它**一个字都不剥**，那份文件上的块注释洞当场重开。
//!      ⇒ 那件事由 [`tests::no_backend_file_falls_back_to_leaving_block_comments_in`] 看着。
//!      **别把这条读成「注释都剥干净了」。**
//!   7. 它只管「观测帧那条通道有没有第二个消费端」；丢帧那本账**该怎么**重新
//!      定义，判据一个字都不管 —— 那要立件。
//!
//! # 它**挡不住**什么（说清楚，别让人以为它是证明）
//!
//! 它认的是下面那张表里的**字面锚点与次数**。有人把 fan-out 写成一个新模块、
//! 而这几处字面一个都不动 ⇒ 本条照样绿。⇒ 它买的是「**动到这三处时有人会被拦一下**」，
//! 不是「fan-out 不可能被实现」。这一句是照 `tests/common/guard-core/rules/bind_guard.rs` 头注那条订正写的：
//! 那条规则**只认 4 个拼写，别的一律看不见**，说法不许比这更强。
//!
//! ⚠ **本模块刻意与被扫的代码不同住一个文件**（理由：
//! 判据在自己的登记表 / 注释 / 常量里找到自己 ⇒ 恒绿）。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#![cfg(test)]

#[cfg(test)]
mod tests {
    use guard_core::production_code;

    /// `(锚点住的那个文件, 锚点, **那个文件里**恰好几处, **全 crate** 恰好几处, 它一变就意味着什么)`。
    /// **次数钉死**。
    ///
    /// # ⚠⚠ 为什么有「全 crate」这一列
    ///
    /// 好几条的**说法**是「全 crate 只此一处」，只数「那个文件里几处」就对不上 ——
    /// 例如 `Admit::Stream` 那条：语料只扫 `wire/listen.rs` 的话，
    /// 「有人在 `main.rs` 里直接构造一个 `listen::Admit::Stream` 绕过 `admit()`」这条针看不见。
    /// ⇒ **守卫范围 ≠ 性质范围**，那是这里自己命名的那一族的**第五形**。
    ///
    /// ★ 处置是**扩人群到它自称的那个范围**（不是把说法改窄）——
    /// 理由：改窄会把真正要挡的东西（「在别处新开一条更松的路」）整个放掉；
    /// 而且是所有同职的条目一起扩，不是只扩被点名的那一条。
    /// ⚠ 通道那一条扩人群治不了：它钉的是通道的**诞生点**（`mpsc::channel::<Frame>(`），不数类型名。
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
             而两条流各自记各自的丢帧账 之后，那本账就不再是「这台机丢了几帧」。\
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
        // `busy.swap(true` 那一行摘了：多客户之后不再有「谁拿到那一张牌」。
        (
            "observe/watcher.rs",
            "mpsc::unbounded_channel::<(Frame, usize)>(",
            1,
            1,
            "观测帧**每条连接一个消费者**，钉在那条通道的**诞生点**上：`watcher::frame_channel` 里那一句\
             （按字节记账，`WatcherPoke::attach` 每接一条连接走一次），**全 crate 只此一处造观测通道**。\
             `tokio` 的 `Receiver` 不可 clone ⇒ 一条通道**至多**一个消费端 ⇒ 一条连接至多一个消费端；\
             扇出在 watcher 里（`FrameSink::fan`：整台那一层过一次，再逐条连接投），丢帧账按连接记（`watcher::Sub`）。\
             再造第二处 `Frame` 通道 = 有一条绕开整台那一层（会话账本 · 预读拦帧）的路 ⇒ 回来重判。\
             ⚠ 本行数**诞生点**，不数类型名 `mpsc::Receiver<Frame>` 出现几处：\
             那样会把 `writer_task` 的两个**形参**算进去，抽一个借用同一条通道的 helper\
             这种**零语义变更的纯重构**也会红。传递 / 借用 / move 一个**已有的**\
             `Receiver<Frame>` 不增本行计数；射程与诚实边界逐条写在本模块头注。\
             ★★ **本行一个人挡不住第二条通道** —— 它只认 turbofish 那一形，\
             而靠左边类型标注定型的写法（`… : (Sender<Frame>, Receiver<Frame>) = mpsc::channel(8)`）\
             它一处都不数（静默过）。\
             ⇒ 与 `no_qualified_channel_call_leaves_its_payload_type_to_inference` **成对**：\
             那条钉着「生产段里 `::channel(` 恰好 0 处」，把**带路径前缀的**诞生点逼回 turbofish\
             这一种写法，本行在**那两条锚点够得着的范围内**才数得全（够不着的四条列在模块头注）。\
             **改本行之前先读那一条。**",
        ),
        (
            "main.rs",
            "mpsc::channel::<Frame>(",
            2,
            2,
            "应答通道**一条载体一条**：stdio 那条 + 常驻那条（与 `writer_task(` 那条针的 2 同源）。\
             第 3 处 = 又造了一条搬 `Frame` 的通道（turbofish 那一形）⇒ 回来重判它是哪一种。",
        ),
        (
            "wire/listen.rs",
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
            "stream/writer.rs",
            "REPLY_BURST",
            2,
            2,
            "让位预算是「两条通道对**一个** writer」（常量 1 处 + 比较 1 处）。\
             每连接一个 writer 之后这条预算要按连接算 —— 那时本行的数会变，正好逼人回来重判。\
             ⇒ 全 crate 也是 2：这条预算**不许有第二个家**。",
        ),
    ];

    /// 「**不带 turbofish 的通道诞生点**」的锚点。
    ///
    /// `PINS` 里那条观测针只认 `mpsc::channel::<Frame>(`。少了本条，
    /// 一个**正常拼法**（靠左边的类型标注定型）就能塞进第二个真消费者而它一声不吭 ——
    /// `PM` 09-01 现打过：backend `489 passed` · 0 failed（那一刀的逐字形状见模块头注）。
    ///
    /// ★ 本锚点要求 `::` **紧挨着** `channel(`：
    /// - 逮得到：`mpsc::channel(8)` · `tokio::sync::mpsc::channel(N)` · `std::sync::mpsc::channel()`
    ///   · `oneshot::channel()` · `broadcast::channel(16)`；
    /// - 逮不到（**故意**）：`mpsc::channel::<Frame>(8)`（有 turbofish，那是 `PINS` 那条的活）
    ///   · `sync_channel::<String>(4)` 与任何 `xxx_channel(` （`::` 不紧挨）
    ///   · `fn reply_channel(cap: usize)` 与 `inbound::reply_channel(8)`
    ///     —— **抽一个叫 `…_channel` 的 helper 是纯重构，它一处都不许红**，
    ///     这正是那条承重线；用 `channel(` 当锚点就会在这里出假阳。
    const BARE_BIRTH: &str = "::channel(";

    /// 全 crate 语料：`src/` 下**全部**（含子目录）`.rs` 的生产段。
    ///
    /// # ⚠ 为什么走 `scan_tree!` 而不是自己 `read_dir`
    ///
    /// 〔08-27，回修当天被本仓自己的判据逮到，如实留档〕本函数第一版是裸递归 `read_dir`
    /// + 一张 `SKIPPED_BY_NAME` 自摘表，**当场被 monitor 侧的
    /// `scanning_guard_registry::no_new_guard_walks_the_tree_without_excluding_itself` 打红**
    /// （它是一条**递减棘轮**：存量清单只许变短）。
    /// 那条判据的理由逐字：「判据在自己的登记表 / 注释 / 常量里找到自己 ⇒ **恒绿**，
    /// audit-0805 实测五次，**五次都不是被判据变红发现的**」。
    /// ⚠ **本模块今天够不上那一形的后果，这句话别写成现在时**：
    /// `PINS` 那六个锚点确实逐字住在本文件里，但它们住在 `#[cfg(test)] mod tests` 内 ——
    /// `production_code` 把整块剥掉，本模块的生产段只剩 **15 字节**、六个锚点**各 0 处**（现打）。
    /// ⇒ 走 `scan_tree!` 的理由**不是**「它今天会拿自己的登记表把自己喂饱」，而是：
    /// **摘除这件事不许靠人记得去写那张自摘表**（⚠ 而宏的按构造自摘在本仓也**不生效** ——
    /// 判据一律由 `#[path]` 挂进生产树 ⇒ `file!()` 给的是带 `..` 的折返路径 ⇒ 后缀比不命中）。
    /// ⇒ 让本模块不进自己语料的是**住址**：它在 `tests/backend/`，而这里扫的是
    /// `src/backend`。走 `scan_tree!` 买到的是「遍历口径只有一份」＋「住址错时当场 panic」。
    ///
    /// ⚠ 把自己加进那张存量清单是**放宽**（一行都不许放宽已有判据）—— 那边一个字都不加。
    ///
    /// 递归这件事由 `scan_tree!` 自己保证；`no_timer_guard::backend_sources` 头注记着
    /// 不递归的实测后果：**目录没有扩展名于是被整个跳过**，护栏一行业务代码都没扫还全绿。
    fn crate_sources() -> Vec<(String, String)> {
        let root = guard_core::addr::backend_src_root();
        guard_core::scan_tree!(&root, &["rs"])
            .into_iter()
            .map(|(path, src)| {
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                // 🔴 同 `no_timer_guard`：按住哪棵树分流，
                // 理由在 `guard_core::addr::production_side_of` 的头注里。
                (rel, guard_core::addr::production_side_of(&path, &src))
            })
            .collect()
    }

    fn source_of(rel: &str) -> String {
        let p = guard_core::addr::backend_src_root().join(rel);
        production_code(
            &std::fs::read_to_string(&p)
                .unwrap_or_else(|e| panic!("读不到 {} —— 文件搬家了就来改本表：{e}", p.display())),
        )
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
        // 反空真②：本护栏自己不许在语料里。
        // ⚠ 本格**按住址成立**，不是在复核 `scan_tree!` 的自摘（那在本仓一处都不生效：
        //   判据由 `#[path]` 挂载 ⇒ 折返路径 ⇒ 后缀比不命中）：本模块在 `tests/backend/`，
        //   不在这里扫的 `src/backend` 那棵树里。
        // ★ 它认得出的一形：有人把语料根扩到含 `tests/` 而没想过本模块会跟着进来。
        //   本模块整份内容在 `#[cfg(test)] mod tests` 里，`production_code` 整块剥掉，
        //   生产段里 `PINS` 那六个锚点**各 0 处**
        //   ⇒ 就算它进了语料，下面那个循环一处也不会多数出来，**会出声的只有本格**。
        assert!(
            !corpus
                .iter()
                .any(|(rel, _)| rel.ends_with("single_stream_guard.rs")),
            "本护栏自己进了语料。\
             ⚠ **别去查 `scan_tree!` 的自摘** —— 那一刀在这一处不生效，\
             本格从来不靠它。要查的是**语料根**：本模块住 `tests/backend/`，\
             而 [`crate_sources`] 扫的是 `src/backend` ⇒ 多半是有人把根扩到含 `tests/` 了。\
             ★ 本格认的是**语料面把判据自己收进来了**这件事本身，\
             不是「`PINS` 里那六个锚点会多数出来」：\
             本模块的生产段今天只有 15 字节、六个锚点各 0 处（09-01 现打），\
             进了语料也一处都不会多算 —— 会出声的只有本格。"
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
                 ★ **这多半不是 bug，是提醒**：明确不做「多客户端的流」。\n\
                 ⚠ 本条与上面那条的差别就是**人群**：那条只看主场文件，本条看全 crate ——\n\
                 「在别处新开一条更松的路」只有本条看得见（`D1` `重-5` 的那一格）。"
            );
        }
    }

    /// ★★ **本模块所有计数的分母，是被真的剥过的文本** —— 不许是兜底原样返回的。
    ///
    /// 上面那两条都在数 `production_code(..)` 出来的字符串。共用剥法剥块注释，
    /// 它带一条**静默兜底**：词法与文本对不上（`depth` 不收口 / 停在字符串里）时
    /// **一个字都不剥**，把原文原样交回来 —— 那是「宁可留洞，不许造假红」的取舍。
    ///
    /// ⇒ 兜底一旦在本 crate 的哪份文件上触发，那份文件的**块注释又能喂饱 `PINS` 了**
    /// （在块注释里写一句 `mpsc::channel::<Frame>(` 就把计数顶上去 / 把真的那处注掉却仍计数），
    /// 而上面两条判据一个数都不会动。**本条就是看着那件事的那道判据。**
    ///
    /// 地板是计数自检（遍历坏了要红，不是静默扫 0 份通过）；人群真的变小时（删了整份 `.rs`）地板跟着降。
    ///
    /// # ★★ **上面那句「本模块所有计数的分母」要带单位**
    ///
    /// 兜底触发不触发，**由交进剥法的那段文本自己配不配平决定**，不由它所属的文件决定。
    /// 只按**整份文件**喂的话，凡是「先把文件切小、再交进剥法」的调用点，
    /// 它一个字都看不见（整份配平、单块不配平时，看门判据绿、被喂的那条守卫判「合规」）。
    /// ⇒ `guard_core::assert_block_comment_model_holds` 量**两个单位**：
    /// 整份文件 ＋ `guard_core::test_attr_chunks` 按 `#[test]` 切出的每一块。
    /// 两个地板各自量（**块数不是文件数**）：`min_files` 挡「遍历坏了」，`min_blocks` 挡「切法坏了」。
    ///
    /// ⚠ **backend 这一侧的读数**（不写 sha：那是每轮都变的量；要重打就跑量具，它自己会报它量的是哪一棵）：
    /// `src/backend` 下把「不是整份文件」的文本交进剥法的调用点 **0 处**
    /// —— 这一侧**没有**块级的活体洞，本条的块那一半在这一侧是**预防**，不是修补。
    /// ⚠ **这是读数，不是不变量** —— 引用前重跑量具。
    /// ⚠ 那个 0 的分母是**那把尺子的射程**（它自己判不了的逐条列在输出的 `PARAM` 段里，
    /// 当时逐条手核）—— 不是「所有写法」的全称。
    #[test]
    fn no_backend_file_falls_back_to_leaving_block_comments_in() {
        // 🔴 **两棵树一起喂，地板一格不动。**
        //
        // backend 这半边今天也住两处：生产段 `src/backend`、测试段 `<repo>/tests/backend`。
        // 本条第二个单位（`#[test]` 块）**整个住在测试段** ⇒ 只喂 `src/` 那一棵的话，
        // `min_blocks` 会在一个已经掉空的分母上通过。两条地板（62 / 500）逐字照旧。
        let roots = guard_core::addr::backend_code_roots();
        guard_core::assert_block_comment_model_holds(&[&roots[0], &roots[1]], 62, 500);
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
                 ★ **这多半不是 bug，是提醒**：明确不做「多客户端的流」——\n\
                 fan-out 与 丢帧账的重新定义留给**第二个要「流」的客户端**出现那天。\n\
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

    /// ★★ **带路径前缀的通道诞生点必须自报载荷类型** —— 生产段里 `::channel(` 恰好 0 处。
    ///
    /// ⚠ **名字与它数的东西对齐过一次**（这里治的正是这一族）：它叫 `qualified`，
    /// 因为锚点要求 `::` **紧挨** `channel(` ⇒ `use …::channel;` 之后裸调的 `channel(8)`
    /// 它**看不见**。别把它读成「每一条通道」。
    ///
    /// # 它为什么在这儿（它是 `PINS` 那条观测针的另一半）
    ///
    /// `PINS` 那条只认 `mpsc::channel::<Frame>(`。`PM` 09-01 现打的那一刀
    /// （逐字形状在模块头注）用**类型标注**定型而不是 turbofish ⇒ 那条针 0 命中，
    /// **backend `489 passed` · 0 failed 静默走过去**，而**旧针（数类型名）在同一形上会红**。
    /// ⇒ 那一版在这一形上是相对旧针的**回退**，不是「一个看不见的角落」。
    ///
    /// 本条把那条缝堵上的办法**不是**把那个数改大一格（`§0a` 写死了不许），而是**换个方向**：
    /// 逼所有诞生点回到 turbofish 那一种拼法，`PINS` 那条才数得全。
    ///
    /// # ⚠ 它是一条 `== 0` 的断言 —— 三道反空真在下面，别删
    ///
    /// 「今天该是空的」那种格天然会空转。所以本条带着：
    /// ① 语料下限（文件数 / 字节数）；② 本护栏自己真的被摘掉了；
    /// ③ **匹配器自检** —— 一组**独立手写**的样本，正反两面各断一次
    /// （`stream/inbound/` 那条 `tokio::main` 自检的纪律：**不用锚点自己拼样本**）。
    /// 而这条判据真有牙的活体证据在 `§3` 死值验：把 `PM` 那一刀插进 `main.rs` ⇒ 它当场红。
    ///
    /// # 它的代价（写在这儿，别只写在头注里）
    ///
    /// **过宽**：新造一条与 `Frame` 毫无关系的通道，只要不写 turbofish 也会红。
    /// ⚠ 但它**对零语义变更的纯重构一处都不红**（那是这里的承重线），
    /// 而且它的解法是「把载荷类型写出来」——**不是放宽判据**。这笔换是选它的理由。
    #[test]
    fn no_qualified_channel_call_leaves_its_payload_type_to_inference() {
        // 反空真③·匹配器自检：**独立手写**的样本，不用 `BARE_BIRTH` 自己拼。
        for should_hit in [
            "let (tx, rx) = tokio::sync::mpsc::channel(8);",
            "        mpsc::channel(REPLY_CHANNEL_CAPACITY)",
            "let (events_tx, events_rx) = std::sync::mpsc::channel();",
            "let (gate_tx, gate_rx) = tokio::sync::oneshot::channel();",
            "let (tx, rx) = tokio::sync::broadcast::channel(16);",
        ] {
            assert!(
                should_hit.contains(BARE_BIRTH),
                "锚点 `{BARE_BIRTH}` 认不出这个**不带 turbofish 的诞生点**：{should_hit:?}\n\
                 ⇒ 本条此刻是空转的，`PINS` 那条观测针也就少了另一半。"
            );
        }
        for should_miss in [
            // 有 turbofish ⇒ 归 `PINS` 那条数，不归本条。
            "let (tx, rx) = tokio::sync::mpsc::channel::<Frame>(8);",
            "let (tx, rx) = std::sync::mpsc::sync_channel::<String>(TEE_QUEUE_LINES);",
            // ★ 这两行是本条的**假阳护栏**：抽一个叫 `…_channel` 的 helper 是纯重构。
            "fn reply_channel(cap: usize) -> (Sender<Frame>, Receiver<Frame>) {",
            "let (tx, rx) = inbound::reply_channel(8);",
        ] {
            assert!(
                !should_miss.contains(BARE_BIRTH),
                "锚点 `{BARE_BIRTH}` 在这一行上有命中：{should_miss:?}\n\
                 ★ 它逮到的要么是 turbofish 那一形（那是 `PINS` 那条的活、会重复计数），\n\
                 要么是一次**纯重构**（抽一个 `…_channel` helper）—— 后者正是这里在治的假阳。"
            );
        }

        let corpus = crate_sources();
        // 反空真①：语料塌了 ⇒ 下面是一句 0 == 0 的空真。
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
        // 反空真②：本护栏自己不许在语料里。
        // ⚠ 原文是「`scan_tree!` 的自摘那一刀**真的落下了**」——
        //   那一刀在本仓**一处都不生效**；本格今天按**住址**成立（本模块住 `tests/backend/`，
        //   而语料根是 `src/backend`），跟那一刀无关。下面那对 09-01 的刀是在
        //   剖分**之前**打的（那时本模块与被扫的树同住一棵），读数照留，别读成现在时。
        // ★ 本格认得出的是「**语料面把判据自己收进来了**」这件事本身，而且它是**本条判据里唯一**认得出这件事的
        //   东西 —— 09-01 现打的一对刀：把本模块塞回语料 ⇒ 本格红（`488 passed; 2 failed`）；
        //   同一刀下把本格退掉 ⇒ 本条**绿着走过去**（`489 passed; 1 failed`，红的只剩另一条判据）。
        // ⚠ **别**把它读成「防自己拿自己的样本把自己打红」——那个后果今天不会发生，理由在报错里。
        assert!(
            !corpus
                .iter()
                .any(|(rel, _)| rel.ends_with("single_stream_guard.rs")),
            "本护栏自己进了语料。\
             ⚠ **别去查 `scan_tree!` 的自摘** —— 那一刀在这一处不生效，本格不靠它；\
             要查的是**语料根**（本模块住 `tests/backend/`，而语料根应当只是 `src/backend`）。\
             ★ 本格认的是**语料面把判据自己收进来了**这件事本身，不是「它会拿自己的样本把自己打红」：\
             本模块整份内容都住在 `#[cfg(test)] mod tests` 里，`production_code` 把它整块剥掉，\
             生产段今天只有 15 字节（09-01 现打），里面 `{BARE_BIRTH}` 0 处 —— \
             就算它进了语料，也一口都喂不进去。"
        );

        let dist: Vec<(&str, usize)> = corpus
            .iter()
            .map(|(rel, code)| (rel.as_str(), code.matches(BARE_BIRTH).count()))
            .filter(|(_, n)| *n > 0)
            .collect();
        let got: usize = dist.iter().map(|(_, n)| n).sum();
        assert_eq!(
            got, 0,
            "\n全 crate 生产段里有 {got} 处**不带 turbofish 的通道诞生点**（锚点 `{BARE_BIRTH}`）。\n\
             分布：{dist:?}\n\
             ★ 本条要的不是「别造通道」，是「**造的时候把载荷类型写出来**」：\n\
             把 `…::channel(N)` 改成 `…::channel::<那个类型>(N)` 就绿了 —— 这是一次\n\
             **零语义变更**的改动，而且它让 `PINS` 里那条观测针看得见你造的是不是 `Frame` 通道。\n\
             ⚠ **别把本条的数从 0 调上去**：那等于把「诞生点自报类型」这条约束放掉，\n\
             而明确不做「多客户端的流」，靠的正是「`Frame` 通道只有一条」这句话数得准。\n\
             ⇒ 真要造第二条 `Frame` 通道：先立件把 丢帧那本账重新定义清楚。"
        );
    }

    /// ★★ 观测帧那根针认的是**通道的诞生点**，不是**类型名出现几处**。
    ///
    /// 本条把那件事的**双向**死值验钉进判据自己 —— 不然它只是头注里的一段散文，
    /// 下一个人把针改回类型名（或者把那个数往上调一格）时，**没有任何东西会红**。
    ///
    /// - **假阳那一侧**（这是这里买的东西）：三种**零语义变更的纯重构**（`D2` 逐字那一刀 ·
    ///   把 rx move 进另一个 task · 把返回元组改成 struct）⇒ **两条针都必须 0 命中**。
    ///   旧针在第一种上是 1，于是全 crate 计数 3→4 当场假红。
    /// - **真违规那一侧**（这是没被拔牙的证据）：真的再造一条 `Frame` 通道 ——
    ///   `Receiver` 不可 clone，第二个消费端**只能这么来** ⇒ **两个拼法各断一次**，
    ///   turbofish 那一形归 `PINS` 那条、类型标注那一形归 `BARE_BIRTH` 那条。
    ///   ⚠ 09-01 之前本条**只断了 turbofish 那一形** —— 于是「消假阳」在另一形上
    ///   把牙一起拔了却没有东西出声，而**恒绿判据 = 假绿**。
    #[test]
    fn the_observation_pin_counts_channel_births_not_type_name_mentions() {
        let needle: &str = PINS
            .iter()
            .find(|(home, _, _, _, _)| *home == "observe/watcher.rs")
            .expect("`PINS` 里 `observe/watcher.rs` 那一行不见了 —— 观测帧那一格的针没了")
            .1;
        // ── 假阳那一侧（这是这里买的东西）：三种**零语义变更的纯重构**，两条针都必须 0 命中。
        //
        // ⚠ 只验第一种是不够的：`D2` 那一刀是**借用**，而「把 rx move 进另一个 task」
        // 与「把返回元组改成 struct」同样是纯重构，同样**一个消费者都没多**。
        for (what, refactored) in [
            (
                "`D2` 逐字那一刀：借用同一条通道的 helper",
                "#[allow(dead_code)] async fn d2_drain_one(\
                 rx: &mut tokio::sync::mpsc::Receiver<Frame>) \
                 -> Option<Frame> { rx.recv().await }",
            ),
            (
                "把 rx move 进另一个 task（同一条通道，仍然只有一个人在收）",
                "tokio::spawn(async move { while let Some(f) = rx.recv().await { let _ = f; } });",
            ),
            (
                "把 `watcher::spawn` 的返回元组改成 struct",
                "pub struct WatcherHandle { \
                 pub rx: tokio::sync::mpsc::Receiver<Frame>, pub poke: WatcherPoke }",
            ),
        ] {
            assert_eq!(
                refactored.matches(needle).count(),
                0,
                "针 `{needle}` 在一次**零语义变更的纯重构**（{what}）上有命中 ——\n\
                 它又在数「类型名出现几处」了。\n\
                 ★ 写死：治法**不是**把那个数改大一格（下一个 helper 又红 = 同一个病\n\
                 推迟一轮），而是让它数**产出 / 诞生点**：谁在造通道，不是谁提到了那个类型名。"
            );
            assert_eq!(
                refactored.matches(BARE_BIRTH).count(),
                0,
                "锚点 `{BARE_BIRTH}` 在一次**零语义变更的纯重构**（{what}）上有命中 ——\n\
                 09-01 补的那条针本来只该管**诞生点**，现在它把纯重构也一起打红了，\n\
                 那就是把旧针的病换个地方长回来。"
            );
        }

        // ── 真违规那一侧（这是没被拔牙的证据）：**两个拼法各一刀**。
        //
        // ★★ 09-01 之前这里只有拼法 ① —— 而拼法 ② 是个**正常写法**，
        //    实测能塞进一个真的第二消费者（真 task、真 `recv`）而门禁 `489 passed` 静默过去。
        //    「只验一个拼法」正是那一版被打回的原因。
        let caught = |probe: &str| {
            (
                PINS.iter()
                    .map(|p| probe.matches(p.1).count())
                    .sum::<usize>(),
                probe.matches(BARE_BIRTH).count(),
            )
        };
        for (spelling, probe) in [
            (
                "① turbofish：`mpsc::channel::<Frame>(8)`（由 `PINS` 那条数）",
                "let (tx2, mut rx2) = tokio::sync::mpsc::channel::<Frame>(8);",
            ),
            (
                "② 靠左边的类型标注定型（`PM` 09-01 那一刀，由 `BARE_BIRTH` 那条数）",
                "let (_pm_tx2, mut _pm_rx2): (tokio::sync::mpsc::Sender<Frame>, \
                 tokio::sync::mpsc::Receiver<Frame>) = tokio::sync::mpsc::channel(8);",
            ),
        ] {
            let (by_pin, by_bare) = caught(probe);
            assert!(
                by_pin + by_bare > 0,
                "「再造一条 `Frame` 通道」的拼法 {spelling} **两条针都没认出来**\n\
                 （`PINS` 各锚点 {by_pin} 处 · `{BARE_BIRTH}` {by_bare} 处）：\n\
                 {probe:?}\n\
                 ★ 这个拼法是**真的**第二个消费端（`Receiver` 不可 clone，第二个消费端只能这么来）。\n\
                 消假阳把牙一起拔了，而恒绿的判据就是假绿。"
            );
        }
    }

    /// ★ 那条**不由本模块钉**的（`HelloFlushed` 见证）要指得住住址。
    ///
    /// `§0b-2㈢`-1 逐字：多连接那天「要么每连接一份见证，**要么把见证降级 —— 降级是放宽，不许**」。
    /// 那一格的判据住 `wire.rs`；本条只保证**那条判据还在**，不重复实现它
    /// （两份实现迟早分叉 —— 本仓最贵的那一族）。
    #[test]
    fn the_hello_witness_pin_still_lives_where_this_module_says_it_does() {
        // 🔴 指的是**判据住的那份文件**。
        // `fn the_hello_witness_has_exactly_one_way_to_exist` 这一轮从
        // `src/backend/wire/mod.rs` 的测试段搬进了 `tests/backend/wire/wire_tests.rs`。
        let wire = include_str!("wire/wire_tests.rs");
        assert!(
            wire.contains("fn the_hello_witness_has_exactly_one_way_to_exist"),
            "本模块头注指着 `wire.rs` 的那条见证判据，而它已经不在了 ——\n\
             ⇒ 「hello 先于 reader」在换了载体之后就只剩一句散文。"
        );
    }
}
