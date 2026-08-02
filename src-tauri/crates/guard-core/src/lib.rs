//! 各条源码扫描型守卫共用的「只留生产段」剥法。
//!
//! # 为什么是一个共享 crate（U8a-2a，2026-08-02）
//!
//! 这份剥法原先住在 daemon 的 `guard_support`（`cfg(test)` 模块）。monitor 侧够不着它，
//! 于是 monitor 的守卫各自写了**便宜近似**，最常见的一种是：
//!
//! ```text
//! let prod = src.split("\n#[cfg(test)]").next().unwrap_or(src);
//! ```
//!
//! 它只在「文件里第一个测试模块之后再没有生产代码」时才是对的。`ssh_source.rs` 的
//! 第一个测试模块在 804 行，而 `parse_frame` 在 1771 行 —— 拿那个近似去扫它，
//! **扫描面直接归零到前 803 行**，守卫静默变瞎。
//!
//! 「抽取面画小了」这一族在 unified-backend 工作区里已经出现过四次。本 crate 是它的收口：
//! 两侧用**同一份**剥法，daemon 的 `guard_support` 改为再导出。
//!
//! # ⚠ 与三个兄弟 crate 的一处**不同**，别照抄错家族不变量
//!
//! `branch-core` / `usage-core` / `acct-core` 的头注都写着「无 IO、无平台依赖」。
//! 本 crate 零依赖、平台无关，但 [`assert_tree_strips_clean`] **做真的 `read_dir` 遍历**，
//! 而且以 panic 为错误模型 —— 因为它只供 `cfg(test)` 里的守卫调用（两侧都是
//! `[dev-dependencies]`，不进任何发布二进制）。新增函数请守住这条边界：
//! **纯文本处理放这里，需要 IO 的只能是守卫断言型（panic 语义、只在测试里跑）。**
//!
//! # 为什么剥法长这样（daemon 侧的两次实测教训，原样保留）
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
//! 1. **锚点写死了模块名 `tests`。** daemon `main.rs` 的测试模块叫 `mod stream_flag_tests`
//!    ⇒ 匹配不上 ⇒ `None` 分支 ⇒ **整个文件（含测试段）被当成生产段扫**。
//!
//! 2. **「第一个锚点之后全砍」这个形状本身就是错的。** 它假定测试模块是文件里最后一样
//!    东西。`main.rs` 不是：`mod stream_flag_tests` 在 182–247 行，而**真正的子命令
//!    dispatch 在 275–291 行，在它后面**。
//!
//! ⇒ 只把坑 1 修掉（放宽锚点）会**当场引爆坑 2**。所以本模块的剥法是
//! 「**逐个剥掉每个 `#[cfg(test)]` 模块**」，不是「第一个之后全砍」。
//!
//! # 自指陷阱（本仓连踩五次，别再踩）
//!
//! 锚点里的换行**必须**用转义写法（源码里是反斜杠 + `n` 两个字符），这样它与真正的换行
//! 不相等 ⇒ 扫源码时不会匹配到本行自己。同理，判「剥干净没」用的那个属性名要**运行时拼**。

/// 一条 `#[cfg(…)]` 属性是不是**测试期专属**。
///
/// # 为什么不能只认 `#[cfg(test)]` 这一种写法（U8a-2a 实测，坑 1 的变种）
///
/// monitor 的 `session_map.rs` 里有
/// `#[cfg(all(test, target_os = "linux"))] mod linux_liveness { … }`（U7d 加的）。
/// 只认逐字 `#[cfg(test)]` 的锚点匹配不上它 ⇒ 那 5 个 `#[test]` 会留在「生产段」里 ——
/// 又是一次「扫描面画错」。新加的 monitor 全树自检第一次跑就把它逮出来了。
///
/// 判据 = 属性里出现 `test` 这个**独立标识符**（前后都不是标识符字符或 `-`）。
/// 于是 `cfg(test)` / `cfg(all(test, …))` / `cfg(any(test, …))` 都算，
/// 而 `cfg(all(unix, not(target_os = "linux")))` 不算、`cfg(feature = "test-utils")` 也不算。
fn cfg_is_test_only(attr: &str) -> bool {
    let b = attr.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'-';
    attr.match_indices("test").any(|(k, _)| {
        let before_ok = k == 0 || !ident(b[k - 1]);
        let after = k + 4;
        let after_ok = after >= b.len() || !ident(b[after]);
        before_ok && after_ok
    })
}

/// 剥掉源码里**每一个带花括号体的** `#[cfg(test)] mod X { … }` 块，其余原样保留。
///
/// 收尾判据 = **列 0 的右大括号**（换行紧跟一个右大括号）。测试模块是顶层 item，rustfmt 保证它的收尾大括号
/// 在列 0，而块内任何嵌套大括号都是缩进的。刻意不做完整的大括号配对：那要连字符串、
/// 原始字符串、注释一起解析，复杂度远超收益，而列 0 判据在两侧的文件上实测干净
/// （daemon 侧 `every_daemon_file_strips_clean`、monitor 侧 `every_monitor_file_strips_clean`）。
///
/// # ★ 必须先确认那一行以左大括号收尾（本函数第一版栽在这里，Phase D 审计当场逮出）
///
/// `#[cfg(test)]` 底下也可能是一条**无花括号体的模块声明**：
///
/// ```text
/// #[cfg(test)]
/// mod guard_support;      ← 就是这份剥法自己在 daemon main.rs 里的声明
/// ```
///
/// 锚点照样匹配，但这里没有块可剥 —— 于是「列 0 的右大括号」会一路找到**下一个顶层 item 的
/// 收尾大括号**，把中间**全部生产代码**当测试段吞掉。实测：加上这条声明后，
/// daemon `main.rs:26–179` 整段消失（`const BUILD_ID` / `const CAPABILITIES` / `const EMITS` /
/// `fn split_stream_flags` 全没了），而 `no_timer_guard` 在那一段上**静默变瞎** ——
/// 把 `thread::sleep` 放进被吞区间**全绿**，放到测试模块之后才红。
///
/// 讽刺的是这正是本模块要消灭的那类 bug，且它**由本模块的引入本身**制造。
/// ⇒ 匹配到锚点后必须看那一行：以左大括号收尾才是模块体、才剥；否则原样保留、跳过继续找。
pub fn production_source(src: &str) -> String {
    // 转义写法 ⇒ 与真正的换行不相等 ⇒ 不会匹配到本行自己。
    let open = "\n#[cfg(";
    let close = "\n}";
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    loop {
        let Some(rel) = src[i..].find(open) else {
            out.push_str(&src[i..]);
            return out;
        };
        let j = i + rel;
        let line_end = |from: usize| {
            src[from..]
                .find('\n')
                .map(|k| from + k)
                .unwrap_or(src.len())
        };
        // 属性那一行（不含前导换行）。
        let attr_start = j + 1;
        let attr_end = line_end(attr_start);
        // 紧接着的那一行必须是 `mod X {` 才是「带花括号体的测试模块」。
        let mod_start = (attr_end + 1).min(src.len());
        let mod_end = line_end(mod_start);
        let mod_line = src[mod_start..mod_end].trim();
        let is_test_mod = cfg_is_test_only(&src[attr_start..attr_end])
            && mod_line.starts_with("mod ")
            && mod_line.ends_with('{');
        if !is_test_mod {
            // 不是测试模块（非 test 的 cfg / 无花括号体的 `mod x;` 声明 / cfg 挂在别的 item 上）
            // ⇒ 原样保留，从属性行之后继续找。
            //
            // ★「无花括号体」那一条是 Phase D 审计逮出来的：`#[cfg(test)] mod guard_support;`
            //   若被当成模块体，「列 0 的右大括号」会一路吞到下一个顶层 item 的收尾，
            //   把中间**全部生产代码**当测试段丢掉（daemon main.rs 曾整段 26–179 行消失）。
            out.push_str(&src[i..attr_end]);
            i = attr_end;
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
/// 剥注释是必需的：两侧的注释**大量**在解释「为什么这里没有定时器了 / 哪些写模式被
/// 禁了 / 有哪些子命令」，逐字提到那些字面量。不剥的话守卫会被**解释它自己的那段散文**
/// 喂饱（daemon 的 `no_timer_guard` P4 实测被打红过；`build_id_guard` 的指纹会被注释里的
/// 子命令名污染）。
pub fn production_code(src: &str) -> String {
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
pub fn assert_no_test_code(who: &str, prod: &str) {
    let attr = format!("#[{}]", "test");
    let n = prod.matches(attr.as_str()).count();
    assert_eq!(
        n, 0,
        "{who}：剥完仍残留 {n} 个测试属性 —— 剥法坏了，此刻这条守卫在扫测试代码"
    );
}

/// 遍历一棵源码树，对每个 `.rs` 文件断言 [`assert_no_test_code`]。
///
/// 抽出来是因为两侧各有一份一模一样的遍历（daemon `every_daemon_file_strips_clean`、
/// monitor `every_monitor_file_strips_clean`），而遍历本身也会坏 —— `min_files`
/// 就是那条计数自检：扫到的文件数低于它，说明**遍历坏了**，不是代码变干净了。
///
/// # Panics
///
/// 目录读不了、文件读不了、或扫到的文件数 `< min_files` 时 panic（守卫语义，只在测试里调）。
pub fn assert_tree_strips_clean(root: &std::path::Path, min_files: usize) {
    assert!(min_files > 0, "min_files 不得为 0 —— 那等于关掉计数自检");
    let mut n = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap_or_else(|e| panic!("读目录 {d:?} 失败: {e}"))
        {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let src =
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读 {path:?} 失败: {e}"));
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
            // 用 `production_code`（**连注释一起剥**）而不是 `production_source`：
            // 散文里逐字提到测试属性是**正常的**（本文件的头注就在解释它），
            // 那不是「剥法坏了」。U8a-2a 实测：两侧各有一个文件因此假红
            // （`guard-core/src/lib.rs` 自己 + monitor `ccm_cli_contract.rs:181`）。
            assert_no_test_code(name, &production_code(&src));
            n += 1;
        }
    }
    assert!(
        n >= min_files,
        "只扫到 {n} 个 .rs 文件（期望至少 {min_files}）—— **遍历坏了**，本条此刻是空转的"
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

    /// ★ U8a-2a 追加：把 monitor 侧那个便宜近似与本剥法**摆在同一份输入上对照**。
    ///
    /// 输入照抄 `ssh_source.rs` 的形状：第一个测试模块在中段，真正要扫的生产代码在它**后面**。
    /// 近似做法会把后半段整个漏掉 —— 这条把「为什么要有这个 crate」变成一句可执行的话。
    #[test]
    fn the_cheap_split_approximation_would_lose_the_later_production_code() {
        let src = "fn early() {}\n#[cfg(test)]\nmod early_tests {\n    fn t() {}\n}\n\
                   fn parse_frame() { let _ = 0; }\n";
        let cheap = src.split("\n#[cfg(test)]").next().unwrap_or(src);
        assert!(
            !cheap.contains("fn parse_frame()"),
            "近似做法居然没漏 —— 那这条对照失去意义，检查夹具形状"
        );
        let good = production_source(src);
        assert!(
            good.contains("fn parse_frame()"),
            "本剥法也漏了后半段，那它不比近似强：{good:?}"
        );
        assert!(!good.contains("fn t()"), "测试段没剥掉：{good:?}");
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
    /// 全部丢掉。这条回归钉直接照着真实病灶写（daemon `main.rs` 里 `mod guard_support;` 的形状）。
    ///
    /// ⚠ 夹具**必须写成单行 `\n` 转义**，不能用带真实换行的多行字符串 ——
    /// 否则夹具里那些列 0 的右大括号会在本文件被 `this_crate_strips_clean` 扫到时
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

    /// ★ U8a-2a 逮出的那条：`#[cfg(all(test, target_os = "linux"))]` 也是测试模块。
    ///
    /// 病灶原样照抄 `session_map.rs::linux_liveness`（U7d 加的）：只认逐字 `#[cfg(test)]`
    /// 的锚点会漏掉它，那 5 个 `#[test]` 就留在「生产段」里了。
    #[test]
    fn strips_test_modules_behind_a_compound_cfg() {
        let src = "fn a() {}\n#[cfg(all(test, target_os = \"linux\"))]\nmod linux_liveness {\n    fn t() {}\n}\nfn b() {}\n";
        let prod = production_source(src);
        assert!(
            !prod.contains("fn t()"),
            "复合 cfg 的测试模块没剥掉：{prod:?}"
        );
        assert!(prod.contains("fn b()"), "后面的生产代码丢了：{prod:?}");
    }

    /// ★ 反向：**不含 `test` 的 cfg 一律不许剥**。
    ///
    /// 放宽锚点最容易引进来的新 bug 就是这个 —— 平台 cfg 满地都是，
    /// 误剥一条就是把一整段生产代码从扫描面里抹掉。
    #[test]
    fn a_platform_cfg_module_is_never_mistaken_for_a_test_module() {
        let src = "#[cfg(all(unix, not(target_os = \"linux\")))]\nmod bsd_impl {\n    fn keep_me() {}\n}\nfn after() {}\n";
        let prod = production_source(src);
        assert!(
            prod.contains("fn keep_me()"),
            "平台模块被当测试段剥了：{prod:?}"
        );
        assert!(prod.contains("fn after()"), "后面的也丢了：{prod:?}");
        // `feature = "test-utils"` 里的 `test` 不是独立标识符。
        assert!(!cfg_is_test_only("#[cfg(feature = \"test-utils\")]"));
        assert!(cfg_is_test_only("#[cfg(test)]"));
        assert!(cfg_is_test_only("#[cfg(all(test, target_os = \"linux\"))]"));
        assert!(!cfg_is_test_only(
            "#[cfg(all(unix, not(target_os = \"linux\")))]"
        ));
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

    /// 剥注释：解释性散文里的字面量不许喂饱守卫。
    #[test]
    fn strips_line_comments_so_prose_cannot_feed_a_guard() {
        let src = "// 这里说明为什么不许 forbidden_token\nfn a() { let _ = 0; }\n";
        let prod = production_code(src);
        assert!(!prod.contains("forbidden_token"), "注释没剥掉：{prod:?}");
        assert!(prod.contains("fn a()"), "生产代码被剥掉了：{prod:?}");
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

    /// 本 crate 自己的源码也要剥得干净（吃自己的狗粮）。
    #[test]
    fn this_crate_strips_clean() {
        assert_tree_strips_clean(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            1,
        );
    }

    /// `assert_tree_strips_clean` 的计数自检真的会咬人。
    #[test]
    fn the_tree_walk_floor_actually_bites() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let r = std::panic::catch_unwind(|| assert_tree_strips_clean(&root, 9_999));
        assert!(r.is_err(), "地板远高于实际文件数却没红 —— 计数自检是空转的");
    }
}
