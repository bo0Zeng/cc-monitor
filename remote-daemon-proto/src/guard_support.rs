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

#[cfg(test)]
mod tests {
    use super::*;

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
    /// 这条同时是「列 0 大括号够不够用」的持续验证 —— 哪天有人在测试模块里写了一段
    /// 列 0 含右大括号的原始字符串，这里会红，那时再上真正的大括号配对。
    #[test]
    fn every_daemon_file_strips_clean() {
        // 地板 = **实测值**（2026-08-02：34 个 .rs）。原先是 10，松了 24 个文件。
        guard_core::assert_tree_strips_clean(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            34,
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
}
