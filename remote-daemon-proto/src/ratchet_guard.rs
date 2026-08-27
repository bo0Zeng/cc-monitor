//! `K-P1 KPY7`（daemon 侧那半）：**本件一行都不许放宽已有判据。**
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
             它已经摘出去成 `K-P3`，立件时要连这一格的代价一起写。",
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
             扫描面只增不减 ⇒ 这个地板没有任何理由被下调。**下调它就是让护栏在更小的面上绿着。**",
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
