//! 各条源码扫描型守卫共用的「只留生产段」剥法。**整个模块只在 `cfg(test)` 下存在。**
//!
//! # 为什么要抽出来（2026-08-01，unified-backend Phase A 计划自审）
//!
//! 原先**八处**各写一份：
//!
//! ```text
//! let marker = "\n#[cfg(test)]\nmod tests";
//! let prod = match src.find(marker) { Some(i) => &src[..i], None => src };
//! ```
//!
//! 它有**两个**独立的坑，而且互相掩盖：
//!
//! 1. **锚点写死了模块名 `tests`。** `main.rs` 的测试模块叫 `mod stream_flag_tests`
//!    ⇒ 匹配不上 ⇒ `None` 分支 ⇒ **整个文件（含测试段）被当成生产段扫**。
//!    受害的是三条守卫：`no_timer_guard`、`build_id_guard`、`accounts_query` 的跨文件
//!    dispatch 守卫。
//!
//! 2. **「第一个锚点之后全砍」这个形状本身就是错的。** 它假定测试模块是文件里最后一样
//!    东西。`main.rs` 不是：`mod stream_flag_tests` 在 182–247 行，而**真正的子命令
//!    dispatch 在 275–291 行，在它后面**。
//!
//! ⇒ 只把坑 1 修掉（放宽锚点）会**当场引爆坑 2**：`main.rs` 的生产段被砍到只剩前 182 行，
//! dispatch 整个消失，`build_id_guard` 的指纹变成空集。**实测确认过这一点**——
//! 所以本模块的剥法是「**逐个剥掉每个 `#[cfg(test)]` 模块**」，不是「第一个之后全砍」。
//!
//! 顺带说明坑 1 与坑 2 为什么互相掩盖：在 `main.rs` 上，坑 1（不剥）恰好让 dispatch
//! 留在了「生产段」里，于是 `build_id_guard` 一直是绿的 —— 但它数到的九个子命令
//! **来自测试模块里那份副本**，不是真 dispatch。两个 bug 凑成了一个看起来正确的结果。
//!
//! # 自指陷阱（本仓连踩五次，别再踩）
//!
//! 锚点里的换行**必须**用转义写法（源码里是反斜杠 + `n` 两个字符），这样它与真正的换行
//! 不相等 ⇒ 扫源码时不会匹配到本行自己。同理，判「剥干净没」用的那个属性名要**运行时拼**。

/// 剥掉源码里**每一个带花括号体的** `#[cfg(test)] mod X { … }` 块，其余原样保留。
///
/// 收尾判据 = **列 0 的右大括号**（换行紧跟一个右大括号）。测试模块是顶层 item，rustfmt 保证它的收尾大括号
/// 在列 0，而块内任何嵌套大括号都是缩进的。刻意不做完整的大括号配对：那要连字符串、
/// 原始字符串、注释一起解析，复杂度远超收益，而列 0 判据在本 crate 的文件上实测干净
/// （见 `every_daemon_file_strips_clean`）。
///
/// # ★ 必须先确认那一行以左大括号收尾（本函数第一版栽在这里，Phase D 审计当场逮出）
///
/// `#[cfg(test)]` 底下也可能是一条**无花括号体的模块声明**：
///
/// ```text
/// #[cfg(test)]
/// mod guard_support;      ← 就是本模块自己在 main.rs 里的声明
/// ```
///
/// 锚点照样匹配，但这里没有块可剥 —— 于是「列 0 的右大括号」会一路找到**下一个顶层 item 的
/// 收尾大括号**，把中间**全部生产代码**当测试段吞掉。实测：加上这条声明后，
/// `main.rs:26–179` 整段消失（`const BUILD_ID` / `const CAPABILITIES` / `const EMITS` /
/// `fn split_stream_flags` 全没了），而 `no_timer_guard` 在那一段上**静默变瞎** ——
/// 把 `thread::sleep` 放进被吞区间**全绿**，放到测试模块之后才红。
///
/// 讽刺的是这正是本模块要消灭的那类 bug，且它**由本模块的引入本身**制造。
/// ⇒ 匹配到锚点后必须看那一行：以左大括号收尾才是模块体、才剥；否则原样保留、跳过继续找。
pub(crate) fn production_source(src: &str) -> String {
    // 转义写法 ⇒ 与真正的换行不相等 ⇒ 不会匹配到本行自己。
    let open = "\n#[cfg(test)]\nmod ";
    let close = "\n}";
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    loop {
        let Some(rel) = src[i..].find(open) else {
            out.push_str(&src[i..]);
            return out;
        };
        let j = i + rel;
        // `mod ` 这一行的范围。
        let mod_line_start = j + open.len() - "mod ".len();
        let mod_line_end = src[mod_line_start..]
            .find('\n')
            .map(|k| mod_line_start + k)
            .unwrap_or(src.len());
        if !src[mod_line_start..mod_line_end].trim_end().ends_with('{') {
            // 无花括号体 ⇒ 是模块**声明**不是测试模块 ⇒ 原样保留，从这一行之后继续找。
            out.push_str(&src[i..mod_line_end]);
            i = mod_line_end;
            continue;
        }
        out.push_str(&src[i..j]);
        match src[j..].find(close) {
            // 没收尾 ⇒ 文件结束前都算测试段，丢弃剩余。
            None => return out,
            Some(rel_end) => i = j + rel_end + close.len(),
        }
    }
}

/// `production_source` + 剥掉行注释。
///
/// 剥注释是必需的：本 crate 的注释**大量**在解释「为什么这里没有定时器了 / 哪些写模式被
/// 禁了 / 有哪些子命令」，逐字提到那些字面量。不剥的话守卫会被**解释它自己的那段散文**
/// 喂饱（`no_timer_guard` P4 实测被打红过；`build_id_guard` 的指纹会被注释里的子命令名污染）。
pub(crate) fn production_code(src: &str) -> String {
    production_source(src)
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 反向自检：剥完的文本里**不许再出现测试属性**。
///
/// 这条是本模块存在的第二个理由。原先各处的自检是 `prod.len() < raw.len()` ——
/// 它**光靠剥注释就满足**，与测试段有没有剥掉毫无关系，所以三条守卫扫了几个月的测试代码
/// 都没人发现。判据字符串运行时拼，避免命中本文件自己。
pub(crate) fn assert_no_test_code(who: &str, prod: &str) {
    let attr = format!("#[{}]", "test");
    let n = prod.matches(attr.as_str()).count();
    assert_eq!(
        n, 0,
        "{who}：剥完仍残留 {n} 个测试属性 —— 剥法坏了，此刻这条守卫在扫测试代码"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 坑 2 的回归钉：测试模块在**文件中段**时，它后面的生产代码必须留下。
    #[test]
    fn keeps_production_code_that_follows_a_mid_file_test_module() {
        let src = "fn a() {}\n#[cfg(test)]\nmod some_tests {\n    fn t() {}\n}\nfn b() {}\n";
        let prod = production_source(src);
        assert!(prod.contains("fn a()"), "前半段丢了：{prod:?}");
        assert!(
            prod.contains("fn b()"),
            "**测试模块之后的生产代码被砍掉了** —— 这正是「第一个锚点之后全砍」的病：{prod:?}"
        );
        assert!(!prod.contains("fn t()"), "测试段没剥掉：{prod:?}");
    }

    /// 坑 1 的回归钉：模块名不叫 `tests` 也必须被剥掉。
    #[test]
    fn strips_test_modules_whose_name_is_not_tests() {
        let src = "fn a() {}\n#[cfg(test)]\nmod stream_flag_tests {\n    fn t() {}\n}\n";
        let prod = production_source(src);
        assert!(
            !prod.contains("fn t()"),
            "只认 `mod tests` 的老毛病还在：{prod:?}"
        );
    }

    /// ★ Phase D 审计逮出的那条：`#[cfg(test)] mod x;`（**无花括号体的声明**）不许被当成
    /// 测试模块 —— 否则「列 0 的右大括号」会一路吞到下一个顶层 item 的收尾，把中间的生产代码
    /// 全部丢掉。这条回归钉直接照着真实病灶写（`main.rs` 里 `mod guard_support;` 的形状）。
    ///
    /// ⚠ 夹具**必须写成单行 `\n` 转义**，不能用带真实换行的多行字符串 ——
    /// 否则夹具里那些列 0 的右大括号会在本文件被 `every_daemon_file_strips_clean` 扫到时
    /// 提前触发收尾，让本模块自己的测试段漏进「生产段」。
    /// （**实测过**：第一版就是多行写法，当场把那条自检打红、漏 4 个测试属性。
    ///  那条自检因此也证明了自己不是安慰剂。）
    #[test]
    fn a_bodyless_cfg_test_mod_declaration_swallows_nothing() {
        let src = "#[cfg(test)]\nmod helper;\nconst KEEP_ME: u8 = 1;\n\
                   fn important() {\n    let _ = 0;\n}\n\
                   #[cfg(test)]\nmod tests {\n    fn t() {}\n}\nfn after() {}\n";
        let prod = production_source(src);
        for keep in [
            "mod helper;",
            "const KEEP_ME",
            "fn important()",
            "fn after()",
        ] {
            assert!(
                prod.contains(keep),
                "`{keep}` 被吞了 —— 无体 mod 声明又被当成测试模块了：{prod:?}"
            );
        }
        assert!(!prod.contains("fn t()"), "真测试模块没剥掉：{prod:?}");
    }

    /// ★ 语义钉：`main.rs` 的生产段必须含这几样东西。
    ///
    /// 字节数地板挡不住「单个文件被剥空/剥过头」（`no_timer_guard` 的 80_000 是**全体**总量，
    /// 实测最大的 `watcher.rs` 整个被吞它都照样绿）。这条用**语义锚点**直接钉住那一类失效：
    /// 上面那个真实病灶如果没修，这里会立刻红。
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

    /// 多个测试模块要逐个剥。
    #[test]
    fn strips_every_test_module_not_just_the_first() {
        let src = "fn a() {}\n#[cfg(test)]\nmod m1 {\n    fn t1() {}\n}\nfn b() {}\n\
                   #[cfg(test)]\nmod m2 {\n    fn t2() {}\n}\nfn c() {}\n";
        let prod = production_source(src);
        for keep in ["fn a()", "fn b()", "fn c()"] {
            assert!(prod.contains(keep), "{keep} 丢了：{prod:?}");
        }
        for drop in ["fn t1()", "fn t2()"] {
            assert!(!prod.contains(drop), "{drop} 没剥掉：{prod:?}");
        }
    }

    /// `assert_no_test_code` 真的会咬人（否则它是安慰剂）。
    #[test]
    fn the_leak_check_actually_bites() {
        let attr = format!("#[{}]", "test");
        let leaked = format!("{attr}\nfn t() {{}}\n");
        let r = std::panic::catch_unwind(|| assert_no_test_code("自检", &leaked));
        assert!(r.is_err(), "喂进带测试属性的文本却没红 —— 判据形同虚设");
        // 反向：干净文本不许误报
        assert_no_test_code("自检", "fn a() {}\n");
    }

    /// ★ 全 crate 实测：15 个文件用列 0 收尾判据都能剥干净。
    ///
    /// 这条同时是「列 0 大括号够不够用」的持续验证 —— 哪天有人在测试模块里写了一段
    /// 列 0 含右大括号的原始字符串，这里会红，那时再上真正的大括号配对。
    #[test]
    fn every_daemon_file_strips_clean() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut n = 0usize;
        let mut stack = vec![dir];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).expect("read src dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let src = std::fs::read_to_string(&path).expect("read rs file");
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
                assert_no_test_code(name, &production_source(&src));
                n += 1;
            }
        }
        assert!(n >= 10, "只扫到 {n} 个文件 —— 遍历坏了，本条此刻是空转的");
    }
}
