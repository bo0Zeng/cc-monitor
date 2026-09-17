//! 各条源码扫描型守卫共用的「只留生产段」剥法。**整个模块只在 `cfg(test)` 下存在。**
//!
//! # 剥法本体已搬进共享 crate（U8a-2a）
//!
//! 实现在 [`guard_core`]（`src-tauri/crates/guard-core`），本模块只是**再导出** +
//! 存放 daemon 专属的那两条语义钉。搬家的理由：monitor 侧够不着 daemon 的 `cfg(test)`
//! 模块，于是它的守卫各自写了便宜近似（`src.split("\n#[cfg(test)]").next()`）——
//! 那个近似在 `ssh_source.rs` 这种「第一个测试模块在 804 行、要扫的代码在 1771 行」的文件上
//! **把扫描面砍掉三分之二**。剥法的来龙去脉（两个互相掩盖的坑、无花括号体 mod 声明那条）
//! 全部留在 `guard-core` 的模块头注里，别在这里再写一份。
//!
//! 调用点一行不用改：下面三条 `pub(crate) use` 让 `crate::guard_support::production_code`
//! 等路径原样可用。

pub(crate) use guard_core::{assert_no_test_code, production_code, production_source};

/// 本 crate **源码树根**的唯一住址。
///
/// # 为什么是一个函数而不是 45 处各写一行
///
/// 这条形状此前在 33 个文件里出现 **45 次**（`Path::new(env!("CARGO_MANIFEST_DIR"))` 接
/// `.join("src")`；这里刻意**不写成完整的可替换形**，理由见下面那条⚠）。
/// 那在「源码树就住在 `Cargo.toml` 旁边」时成立 —— 而仓库重组要把这棵树搬到
/// `<repo>/src/backend/`，`Cargo.toml` 留在原处 ⇒ 那 45 处**同时失效**。
///
/// 🔴 **失效的形态比「报错」坏得多**：`CARGO_MANIFEST_DIR/src` 搬走后是个**不存在的目录**，
/// 而扫描型守卫拿不到文件时多半**扫了个空集 ⇒ 恒绿**，不是红。按本仓自己的说法
/// 「恒绿看起来和真绿一模一样」—— 33 个守卫会一起变成装饰品，而门禁全绿。
/// ⇒ 一个东西一个住址，搬树时只改这一行。前端侧同形同理（`tests/test-support/repo-root.ts`
/// 的 `srcDirOf`，那边是 14 个消费者）。
///
/// ⚠⚠ **建这个住址的那一轮，机械替换把本函数的函数体也换成了对自己的调用** ——
/// 无限递归，`cargo test` 以**栈溢出**（SIGABRT）现形，而不是断言失败。同一趟还把上面
/// 那句「此前的形状长什么样」一并替换了，于是那句话变成「旧形状 ＝ 新形状」的废话。
/// ⇒ 这是本仓「**尺子量到了自己**」那一族的又一例，而且是最容易中的一种：抽住址时，
/// **新住址的定义本身就是旧形状的最后一个实例**，任何按形状扫全树的改法都会吃掉它。
/// 教训落成纪律：**抽住址时把定义处排除在替换人群之外**；引用旧形状的文字也不写成
/// 能被同一条规则命中的完整形。
///
/// ⚠ 本函数**不覆盖** `.join("Cargo.toml")` 那 3 处 —— 它们跟着 **manifest** 走，
/// 不跟着源码树走，搬树时本来就不该动。两件事别混成一件。
pub(crate) fn src_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ 反空真：`src_root()` 必须指到**真的**源码树。
    ///
    /// 它是 45 处扫描的共同入口 —— 一旦它指错（搬树时漏改这一行就会），
    /// 下游那些守卫不会红，会**扫空集然后全绿**。这条把「指错」变成一条会红的机检。
    #[test]
    fn the_src_root_address_points_at_a_real_tree() {
        let root = src_root();
        assert!(root.is_dir(), "src_root() 不是目录：{}", root.display());
        assert!(
            root.join("main.rs").is_file(),
            "src_root() 下没有 main.rs —— 它指到了别的地方：{}",
            root.display()
        );
        let n = std::fs::read_dir(&root).expect("读 src_root 失败").count();
        assert!(n >= 20, "src_root() 下只有 {n} 个条目，扫描面疑似塌了");
    }

    /// ★ 语义钉：`main.rs` 的生产段必须含这几样东西。
    ///
    /// 字节数地板挡不住「单个文件被剥空/剥过头」（`no_timer_guard` 的 80_000 是**全体**总量，
    /// 实测最大的 `watcher.rs` 整个被吞它都照样绿）。这条用**语义锚点**直接钉住那一类失效：
    /// `guard-core` 头注里记的那个真实病灶如果没修，这里会立刻红。
    ///
    /// # 锚点会随重构变，**换锚点之前先问它为什么不见了**
    ///
    /// U3 拆 `observe/`/`control/` 时这条**红了一次**：锚点里有 `mod watcher;`，
    /// 而 `watcher` 正当地挪进了 `observe/mod.rs`。**这是钉子干对了活**（它就是要在
    /// 「main.rs 生产段少了东西」时叫），只是这次的原因是重构而不是剥过头。
    ///
    /// 处置纪律与「守卫钉死的计数」同一条：**不是把红的那条删掉了事**，
    /// 而是问「main.rs 今天还剩哪些东西是承重的」，换成那些。
    /// ⇒ `mod watcher;` 换成 `mod observe;` + `mod control;` —— 后两条恰恰是 U3 建出来的
    /// 两条解耦线在 `main.rs` 里的落点，比原来那条更承重。
    #[test]
    fn main_production_section_keeps_its_load_bearing_items() {
        let prod = production_code(include_str!("main.rs"));
        for anchor in [
            format!("const BUILD{}", "_ID"),
            format!("const CAPA{}", "BILITIES"),
            format!("fn split_stream{}", "_flags"),
            format!("mod obser{}", "ve;"),
            format!("mod contr{}", "ol;"),
        ] {
            assert!(
                prod.contains(&anchor),
                "main.rs 的生产段里找不到 `{anchor}` —— 剥过头了，扫描面正在静默缩水"
            );
        }
    }

    /// ★ 全 crate 实测：daemon 的每个源文件用列 0 收尾判据都能剥干净。
    ///
    // ⟦KR115D3 共用段·起⟧
    /// 这条同时是「列 0 收尾判据够不够用」的持续验证 —— **而它自己已经守不住那一形了**。
    ///
    /// ⚠ 〔`K-R115` 09-14〕上一版这里逐字写着「哪天有人在测试模块里写了一段列 0 含右
    /// 大括号的原始字符串，**这里会红**」。那句话**两天里假了两次**：
    /// ① `K-R110`（09-13）现打它**当时就没红** —— 那 31 行漏进生产段，而守门人看不见；
    /// ② `K-R110` 之后它**永远不会再因此红** —— 那一形已经被
    ///    `guard_core::assert_test_module_ranges_are_brace_balanced` 正确处理掉了
    ///    （匹配单位从「行」换成「计数」，区间在原始字符串里收尾会被它当场逮住）。
    /// ⇒ **今天真正守这一形的是那条判据**；本条经 `assert_tree_strips_clean` 调到它，
    ///    本条自己守的只剩「这棵树剥得干净 ＋ 文件数没缩水」。
    ///
    /// 🔴 **这一段在两棵树里各住一份，逐字必须相同**
    /// （`remote-daemon-proto/src/guard_support.rs` ＋ `src-tauri/src/structural_scan.rs`）——
    /// `K-R110` 交回时点名过这个形状：**两个住址、同一句话**，改一处漏一处，
    /// 下一次还是一处真一处假。钉着它的是
    /// `guard_support.rs::the_two_strip_clean_notes_stay_one_sentence`，**只改一处当场红**。
    // ⟦KR115D3 共用段·止⟧
    #[test]
    fn every_daemon_file_strips_clean() {
        // 地板 = **实测值**（2026-08-02：34 个 .rs）。原先是 10，松了 24 个文件。
        guard_core::assert_tree_strips_clean(
            &crate::guard_support::src_root(),
            // ★ audit-0805 F16：34 → **37**（今日实测）。余量 3 恰好等于
            // `platform/pidwatch/` 的文件数 —— 那一整个目录掉出去也不会红。
            37,
        );
    }

    /// 再导出没有把语义换掉：反向自检仍然会咬人。
    #[test]
    fn the_reexported_leak_check_still_bites() {
        let attr = format!("#[{}]", "test");
        let leaked = format!("{attr}\nfn t() {{}}\n");
        let r = std::panic::catch_unwind(|| assert_no_test_code("自检", &leaked));
        assert!(r.is_err(), "再导出之后判据形同虚设");
        assert!(production_source("fn a() {}\n").contains("fn a()"));
    }

    /// 🔴 `K-R115` `KR115D3`：**「两个住址、同一句话」不许再各说各话。**
    ///
    /// # 题面
    ///
    /// 本文件的 `every_daemon_file_strips_clean` 与 monitor 那一侧的同名判据
    /// （住 `src-tauri/src/structural_scan.rs`，扫的是另一棵树）头上挂着**同一段散文**。
    ///
    /// ⚠ 这里**刻意不写出对侧那个判据的函数名**：`structural_scan.rs` 里的
    /// `scan_tree!` 按构造摘掉调用者自己那一份 ⇒ 只住在那份文件里的符号**进不了**
    /// 死名判据的代码侧语料，散文一点名它就被当成「代码里根本不存在的名字」。
    /// 那是那条判据的一处盲区（`K-R115` 09-14 现打撞到），不是这一句写错了。
    /// `K-R110` 交回时点名过这个形状：那句话**昨天是假的**（现打它没红）、
    /// **今天变成另一种假**（那一形已被正确处理，它永远不会再因此红）——
    /// 而**订正只落在其中一处**的话，下一次仍是一处真一处假。
    ///
    /// ⇒ 本条把那一段钉成**逐字相同**：只改一处，当场红。
    ///
    /// # ⚠ 它买不到什么（诚实边界，别把绿读宽）
    ///
    /// - **不判那句话对不对** —— 两处一起改成同一句假话，本条照样绿（那要读语义）。
    ///   它买的是「**不会再一处真一处假**」，不是「说的是真话」。
    /// - **人群写死是两处**。第三处地方再抄一遍同一句话，本条看不见。
    /// - 抽取靠一对标记；标记被删掉 ⇒ 上面那条 `count() == 1` 先红，
    ///   **不会退化成「抽了个空串、两边相等、静默绿」**。
    #[test]
    fn the_two_strip_clean_notes_stay_one_sentence() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("remote-daemon-proto 的上级 = 仓根");
        // 🔴 **运行时拼**：写成字面量的话，本文件里这两行自己就会命中标记，
        //    下面那条 `count() == 1` 当场变成恒假 —— 而它正是防空真的那一条。
        let beg = format!("// ⟦KR115D3 共{}", "用段·起⟧");
        let end = format!("// ⟦KR115D3 共{}", "用段·止⟧");
        let sites = [
            "src-tauri/src/structural_scan.rs",
            "remote-daemon-proto/src/guard_support.rs",
        ];
        let mut blocks: Vec<(&str, String)> = Vec::new();
        for rel in sites {
            let src = std::fs::read_to_string(repo.join(rel))
                .unwrap_or_else(|e| panic!("{rel} 读不到：{e} —— 本条判不了，不许当成绿"));
            for (mark, which) in [(&beg, "起"), (&end, "止")] {
                assert_eq!(
                    src.matches(mark.as_str()).count(),
                    1,
                    "{rel} 里共用段的「{which}」标记出现 {} 次，应当恰好 1 次 —— \
                     抽取器指不到唯一那一段时，下面那条「两处相等」就是空真",
                    src.matches(mark.as_str()).count()
                );
            }
            let i = src.find(beg.as_str()).unwrap() + beg.len();
            let j = src.find(end.as_str()).unwrap();
            assert!(i < j, "{rel} 里共用段的两个标记次序反了");
            blocks.push((rel, src[i..j].to_string()));
        }
        // 🔴 运行时拼：写成字面量会被 `needle_anchor_registry` 那条棘轮
        //    （语料变量上的裸子串匹配，只许降）数进去 —— 那条棘轮不许为了今天好过而调高。
        let pointer = format!("assert_test_module_ranges_are_brace{}", "_balanced");
        // 反向自检：抽出来的不许是空的、也不许短到什么都不是（`"" == ""` 照样成立）。
        for (rel, b) in &blocks {
            let lines = b.trim().lines().count();
            assert!(
                lines >= 10,
                "{rel} 抽到的共用段只有 {lines} 行 —— 抽空了的话「两处相等」照样绿"
            );
            assert!(
                b.contains(pointer.as_str()),
                "{rel} 的共用段里没有点名今天真正守这一形的那条判据 —— \
                 `KR115D3` 要的就是这两处从「这里会红」改成指向它"
            );
        }
        assert_eq!(
            blocks[0].1, blocks[1].1,
            "**两个住址、同一句话，而今天它们说的不是同一句。**\n\
             {} 与 {} 的共用段逐字对不上 ⇒ 有人只改了一处。\n\
             两处一起改，或者把这一段挪到一处去派生。",
            blocks[0].0, blocks[1].0
        );
    }
}
