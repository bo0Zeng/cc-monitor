use super::*;

/// 🔴 **`scan_tree!` 这个宏的自摘，剖分之后在本仓已经没有一个能生效的调用者了。**
///
/// # 它换掉了谁、为什么〔步 7c 2026-09-19 · `设计/16 §6.2` **D 类**〕
///
/// 原来这里住的是 `the_caller_never_gets_its_own_source_back`〔散文墓碑〕，断言
/// 「本 crate 的 `src/` 里只有 `lib.rs`（= **调用者自己**），摘除生效 ⇒ 结果为空」。
/// 那条断言的前提是**判据与被测代码同住一份文件** —— 判据搬进
/// `tests/bridge/crates/guard-core/lib_tests.rs` 之后，「调用者」不再是 `lib.rs`，
/// 前提**恒假**，它当场红。这正是 `§6.2` D 类那一形（`§6.3` 记过两条同形的）。
///
/// # 换成今天真的成立、而且仍然会红的那句话
///
/// `scan_tree!` 展开成 `scan_tree_excluding_self($root, $exts, file!())`，
/// 而本仓**所有**测试模块今天都是 `#[path]` 引进来的 ⇒ `file!()` 一律是带 `..`
/// 的折返路径 ⇒ 后缀比恒不命中 ⇒ **自摘在本仓恒空转**。
/// 本条就断言这件事，因此：
///
/// · 它是 [`a_path_attribute_caller_file_silently_fails_to_exclude`] 的**加强版** ——
///   那条喂的是手写的折返串（依赖「我猜 `file!()` 长这样」），
///   本条喂的是**真的 `file!()`**，不依赖任何猜测；
/// · 哪天有人给 `scan_tree!` 加上路径规范化（让自摘重新生效），**本条当场红**，
///   而红了该干什么写在下面的文案里 —— 那时要回去重读 `§5.4b` 纪律 4：
///   自摘一旦重新生效，94 处 `scan_tree!` 的语料面会**静默缩小一份**。
#[test]
fn the_scan_tree_macro_no_longer_excludes_its_caller_after_the_split() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // 对照组先行：遍历本身必须是活的（否则下面的「非空」也读不出信息）。
    let all = scan_tree_excluding(&root, &["rs"], &[]);
    assert!(
        all.iter().any(|(p, _)| p.ends_with("lib.rs")),
        "明写空名单时 `lib.rs` 都不在结果里 —— 遍历坏了，本条此刻不携带信息"
    );
    let files = scan_tree!(&root, &["rs"]);
    assert!(
        files.iter().any(|(p, _)| p.ends_with("lib.rs")),
        "🟢 `scan_tree!` 的自摘**重新生效了**（`lib.rs` 被摘掉了）。\n\
         \n\
         本条不是在报故障，是在报一个**纪律要重读**的信号：\n\
         `设计/16 §5.4b` 纪律 4 说「摘掉我自己」不许靠 `file!()`，理由之一是\n\
         它失效时**安静**。它一旦重新生效，本仓 94 处 `scan_tree!` 的语料面会\n\
         各自**静默少一份文件**，而少扫不会红。\n\
         ⇒ 先去核这 94 处里有几处的语料根含判据自己那棵树（`tests/`），\n\
         再决定是把它们改成 `scan_tree_excluding` 的明写名单，还是收掉本条。"
    );
}

/// ★ 对照组：把摘除条件换成一个匹配不上的名字 ⇒ `lib.rs` 必须回来。
///
/// 没有这一条，上面那条「结果为空」可能只是**遍历本来就坏了**。
#[test]
fn without_the_exclusion_the_caller_would_be_included() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = scan_tree_excluding_self(&root, &["rs"], "does-not-match-anything.rs");
    assert!(
        files.iter().any(|(p, _)| p.ends_with("lib.rs")),
        "换成匹配不上的摘除名之后 `lib.rs` **仍然**不在结果里 —— \
             说明遍历本来就坏了，上一条的「为空」是假信号（零命中地绿）"
    );
}

/// 🔴 **`file!()` 自摘在「判据被 `#[path]` 引进来」时是空转的** —— 条 73 的成因钉。
///
/// 仓库重组（`设计/16`）之后，测试模块长这样：
/// `#[cfg(test)] #[path = "../../../../src/bridge/tests/bridge/X_tests.rs"] mod tests;`
/// 被引进来的那份文件里 `file!()` 给的是 `src/../../../tests/bridge/X_tests.rs` ——
/// **带 `..` 的折返路径**，而草垛是规范化过的绝对路径 ⇒ 后缀比**恒不命中**。
///
/// ⚠ 这一条**不是**在要求把它修好（修好之后它仍然只是「看起来像在工作」）；
/// 它是把那个失效**钉成一条读得出来的事实**，好让条 73 那条纪律有据可依：
/// 靠 `file!()` 自摘的判据，一搬树就安静地把自己收进了自己的语料。
#[test]
fn a_path_attribute_caller_file_silently_fails_to_exclude() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let folded = "src/../../../crates/guard-core/src/lib.rs";
    let files = scan_tree_excluding_self(&root, &["rs"], folded);
    assert!(
        files.iter().any(|(p, _)| p.ends_with("lib.rs")),
        "折返形的 `caller_file` 居然摘掉了调用者 —— 那说明后缀比的口径变了，\
             条 73 那条纪律的成因描述要跟着重写"
    );
}

/// ★ 明写名单**真的摘得掉** —— [`scan_tree_excluding`] 的正控。
#[test]
fn a_named_exclusion_actually_drops_that_file() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = scan_tree_excluding(&root, &["rs"], &["crates/guard-core/src/lib.rs"]);
    assert!(
        files.is_empty(),
        "明写名单没把 `lib.rs` 摘掉，实得 {:?}",
        files.iter().map(|(p, _)| p).collect::<Vec<_>>()
    );
    // 对照组：名单为空 ⇒ 它必须回来（否则上面那个「为空」是遍历坏了）。
    assert!(
        scan_tree_excluding(&root, &["rs"], &[])
            .iter()
            .any(|(p, _)| p.ends_with("lib.rs")),
        "名单为空时 `lib.rs` 仍然不在结果里 —— 遍历坏了，上一格的「为空」是假信号"
    );
}

/// 🔴 **名单摘不到东西就当场红** —— 这正是 `file!()` 那条路从来没有过的那一格。
///
/// 排除悄悄失效之后，被排除的那份会**回到人群里**，而判据在自己的登记表或散文里
/// 找到自己 ⇒ 恒绿，读起来和「真的没有违规」一模一样。
#[test]
fn an_exclusion_that_matches_nothing_is_a_hard_error() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let err = std::panic::catch_unwind(|| {
        scan_tree_excluding(&root, &["rs"], &["crates/guard-core/src/gone.rs"]);
    });
    assert!(
        err.is_err(),
        "名单上写了一个盘上没有的文件，本函数却一声不吭 —— \
             那就退回成了 `file!()` 那条「关掉之后看起来和没关一模一样」的路"
    );
}

/// ★ 回归钉〔09-09 Windows CI 现打〕：`file!()` 在 Windows 上给的是**反斜杠**形。
///
/// 云端 windows-latest 的 panic 位置逐字是 `crates\guard-core\src\lib.rs`。
/// 归一化此前只做在扫出来的那一边 ⇒ 针和草垛不同形 ⇒ 后缀比恒不命中、摘除空转。
///
/// ⚠ **为什么不能只靠上面那条**：上面那条只在 Windows 上红，而本仓的 `cargo test`
/// 在 Windows 上今天才第一次跑起来（此前先卡 fmt、再卡 clippy）——
/// 也就是说「Linux 全绿」这件事**证明不了摘除是活的**。这一条把那个平台差
/// 拉成一个**两个平台都跑**的输入：直接喂反斜杠形的 `caller_file`，摘除必须照样生效。
#[test]
fn a_windows_shaped_caller_file_still_excludes_the_caller() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = scan_tree_excluding_self(&root, &["rs"], r"crates\guard-core\src\lib.rs");
    assert!(
        files.is_empty(),
        "反斜杠形的 `caller_file` 没摘掉调用者 —— 摘除在 Windows 上是空转的，实得 {:?}",
        files.iter().map(|(p, _)| p).collect::<Vec<_>>()
    );
}

/// 摘除的后缀比**断在路径分量边界上**：`a-src/lib.rs` 不许被当成 `src/lib.rs`。
///
/// 松匹配的后果是**摘多了**：多摘一份不是调用者的源码，判据的人群悄悄小一份而照样绿。
#[test]
fn the_exclusion_suffix_is_anchored_at_a_path_component_boundary() {
    assert!(path_suffix_matches("/repo/x/src/lib.rs", "src/lib.rs"));
    assert!(path_suffix_matches("src/lib.rs", "src/lib.rs"));
    assert!(
        !path_suffix_matches("/repo/x/a-src/lib.rs", "src/lib.rs"),
        "松匹配把 `a-src/lib.rs` 也认成了 `src/lib.rs` —— 那会摘掉一份不是调用者的源码"
    );
}

/// 空 `caller_file` 会退化成「摘除一切」⇒ 必须拒绝，不许静默给空集。
#[test]
#[should_panic(expected = "caller_file 为空")]
fn an_empty_caller_file_is_rejected_not_silently_swallowed() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let _ = scan_tree_excluding_self(&root, &["rs"], "");
}

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
/// 全部丢掉。这条回归钉直接照着真实病灶写（backend `main.rs` 里 `mod guard_support;` 的形状）。
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

/// ★★ `K-R3` 09-01：**行尾注释不许再喂饱存在型判据**。
///
/// 语料是本仓退出臂那一刀的最小形（09-01 现打过真值：改之前门禁七格一个数不动）：
/// 真调用被换掉、只在**行尾注释**里留下原来那句 —— 生产上功能没了，判据却绿。
#[test]
fn a_trailing_comment_can_no_longer_feed_an_existence_guard() {
    let holed = "fn exit() {\n    let _ = h.current_pid(); // h.stop();\n}\n";
    let prod = production_code(holed);
    assert!(
        !prod.contains(".stop()"),
        "行尾注释把 `.stop()` 带进了生产文本 —— 那正是「功能抽走、判据照绿」那个洞：{prod:?}"
    );
    // 非空对照：真调用还在时**必须**读得到（不然上面那条可能只是「什么都读不到」）。
    let real = "fn exit() {\n    h.stop(); // 勾了才收；缺省不收见 C8③\n}\n";
    assert!(
        production_code(real).contains(".stop()"),
        "真调用被误剥了 —— 这是拿假阳换真阳（铁律 18）"
    );
}

/// 🔴 `strip_comment_lines` 头注给「不剥行尾」写的理由是
/// 「按第一个 `//` 截断会砍坏 `\"http://host\"`」——
/// 本条钉住：新剥法**没有**落进那个坑，两侧都断。
#[test]
fn a_double_slash_inside_a_string_literal_is_not_a_comment() {
    let src = "fn a() {\n    let u = \"http://host/x\"; // 真注释在这儿\n}\n";
    let prod = production_code(src);
    assert!(
        prod.contains("\"http://host/x\""),
        "字符串字面量被当成注释砍掉了：{prod:?}"
    );
    assert!(
        !prod.contains("真注释"),
        "那一行真正的行尾注释没被剥掉：{prod:?}"
    );
}

/// 一个 `'\"'` 字符字面量不许把状态机整段带偏（此后所有 `//` 都被当成字符串内容）。
#[test]
fn a_quote_char_literal_does_not_derail_the_scanner() {
    let src = "fn a() {\n    if c == '\"' { let _ = 0; } // 尾注释\n}\n";
    let prod = production_code(src);
    assert!(!prod.contains("尾注释"), "字符字面量把剥法带瞎了：{prod:?}");
    assert!(
        prod.contains("if c == '\"'"),
        "字符字面量本身被砍了：{prod:?}"
    );
}

/// 保守边界的**正面兑现**：raw string 与跨行字符串里的 `//` 一个字都不许动。
///
/// 这两种情形上的洞**仍然开着**（[`strip_trailing_comments`] 头注逐条写了）——
/// 本条钉的不是「洞关了」，是「**没有为了关洞去砍字符串**」。
#[test]
fn raw_and_multiline_strings_are_left_alone() {
    let raw = "fn a() {\n    let s = r#\"keep // this\"#;\n}\n";
    assert!(
        production_code(raw).contains("keep // this"),
        "raw string 里的 `//` 被当注释砍了：{:?}",
        production_code(raw)
    );
    let multi = "fn a() {\n    let s = \"first\n    second // still inside\";\n}\n";
    assert!(
        production_code(multi).contains("second // still inside"),
        "跨行字符串里的 `//` 被当注释砍了：{:?}",
        production_code(multi)
    );
}

// ══════════════════════════════════════════════════════════════════
// `K-R9` 09-04：块注释那一半
// ══════════════════════════════════════════════════════════════════

/// ★★ **本件的正题**：多行块注释不许再喂饱存在型判据。
///
/// 形状照 09-04 在 `local_backend_host.rs` 幂等门上现打的那一刀：真代码包进
/// `/*` `*/`（两个符号各占一行 —— 编辑器「注释掉这几行」的默认产物）、换一个桩值。
/// 改之前 monitor 侧 **1275 条全绿 0 失败**，而生产上那道门已经没了。
#[test]
fn a_block_comment_can_no_longer_feed_an_existence_guard() {
    let holed = "fn start() {\n    /*\n    if g.is_some() {\n        return Already;\n    }\n    */\n    let _shim = g.is_some();\n}\n";
    let prod = production_code(holed);
    assert!(
        !prod.contains("return Already"),
        "块注释把真代码带进了生产文本 —— 那正是「功能抽走、判据照绿」那个洞：{prod:?}"
    );
    assert!(
        !prod.contains("if g.is_some() {"),
        "块注释里的 `if g.is_some() {{` 仍被数进生产段：{prod:?}"
    );
    // 非空对照：真代码还在时**必须**读得到（不然上面两条可能只是「什么都读不到」）。
    let real = "fn start() {\n    if g.is_some() {\n        return Already;\n    }\n}\n";
    assert!(
        production_code(real).contains("return Already"),
        "真代码被误剥了 —— 这是拿假阳换真阳（铁律 18）"
    );
}

/// 误伤一：**字符串字面量里的 `/*` 不是注释**。
///
/// 语料取自本仓真形：`shared_crate_registry.rs` 里逐字有 `crates/*/Cargo.toml`
/// 与 `src/shared/**`，`relay/server.rs` 里有 shell 的 `${rest%%/*}`。
#[test]
fn a_block_open_inside_a_string_literal_is_not_a_comment() {
    let src = "fn a() {\n    let g = \"crates/*/Cargo.toml\";\n    let h = \"src/shared/** files\";\n    keep_me();\n}\n";
    let prod = production_code(src);
    assert!(
        prod.contains("\"crates/*/Cargo.toml\""),
        "串里的 `/*` 被当成块注释开头：{prod:?}"
    );
    assert!(
        prod.contains("keep_me()"),
        "串里那个 `/*` 把它后面的真代码吃掉了 —— 这正是「剥狠了」那一格：{prod:?}"
    );
}

/// 误伤一的**变体**：`'\"'` 这种字符字面量不许把串状态整段带偏。
///
/// 实测语料：`shared_crate_registry_tests.rs::shared_crate_names` 里的 `.trim_end_matches('\"')`
/// —— 不掩码的话该文件扫到末尾深度停在 **5**，后面大片真代码会被当注释抹掉。
#[test]
fn a_quote_char_literal_does_not_derail_the_block_scanner() {
    let src = "fn a() {\n    let v = x.trim_end_matches('\"').to_string();\n    let g = \"crates/*/Cargo.toml\";\n    keep_me();\n}\n";
    assert!(
        block_comment_model_holds(src),
        "`'\"'` 把块注释词法带瞎了（走了兜底）"
    );
    assert!(
        production_code(src).contains("keep_me()"),
        "真代码被误剥了：{:?}",
        production_code(src)
    );
}

/// 误伤二：**嵌套块注释**要吃到最外层那个 `*/`，不是第一个。
#[test]
fn nested_block_comments_are_consumed_to_the_outer_close() {
    let src = "fn a() {\n    /* 外 /* 内 */ 还在注释里 hidden_token */\n    keep_me();\n}\n";
    let prod = production_code(src);
    assert!(
        !prod.contains("hidden_token"),
        "嵌套只吃到第一个 `*/`：{prod:?}"
    );
    assert!(prod.contains("keep_me()"), "嵌套吃过头了：{prod:?}");
}

/// 误伤三：**`/**` 与 `/*!` 文档注释**是散文，与 `///` 同等对待 —— 剥掉。
/// 附两个退化形 `/**/` 与 `/***/`（它们必须**恰好**收口，不许多吃一格）。
#[test]
fn block_doc_comments_are_prose_and_two_degenerate_forms_close_exactly() {
    let doc = "/** 这里说明为什么不许 forbidden_token */\nfn a() { keep_me(); }\n";
    let prod = production_code(doc);
    assert!(
        !prod.contains("forbidden_token"),
        "块文档注释没剥掉：{prod:?}"
    );
    assert!(
        prod.contains("keep_me()"),
        "块文档注释吃掉了真代码：{prod:?}"
    );
    for degenerate in [
        "fn a() { /**/ keep_me(); }\n",
        "fn a() { /***/ keep_me(); }\n",
    ] {
        assert!(
            production_code(degenerate).contains("keep_me()"),
            "`{degenerate}` 这个退化形没有恰好收口：{:?}",
            production_code(degenerate)
        );
    }
}

/// 误伤四：**行注释里的 `/*` 不开块** —— 否则一句散文能吃掉它后面整份文件。
#[test]
fn a_block_open_inside_a_line_comment_does_not_swallow_the_file() {
    let src = "fn a() {\n    let _ = 0; // 见 /* 那一段\n    keep_me();\n}\n";
    assert!(
        production_code(src).contains("keep_me()"),
        "行注释里的 `/*` 开了块、把后面的真代码吃了：{:?}",
        production_code(src)
    );
}

/// 误伤五：**跨行原始串内部**一个字都不许动。
///
/// 这条不是防御性编程 —— 语料是本仓 `relay/server_tests.rs::STUB_LAUNCHER` 那段 shell 的最小形，
/// 里面逐字有一个 `/*`（`${rest%%/*}`）和一个 `*/`（`${rest#*/}`）。
/// 按「含 `r#` 的**那一行**整行不动」的便宜办法，中间这两行会被当块注释抹掉 18 个字节，
/// 而它们是真的字符串内容。
#[test]
fn a_multiline_raw_string_holding_shell_globs_is_left_alone() {
    let src = "const S: &str = r#\"#!/usr/bin/env bash\nhostport=${rest%%/*}\npath=/${rest#*/}\nhost=${hostport%%:*}\n\"#;\nfn a() { keep_me(); }\n";
    let prod = production_code(src);
    assert!(
        prod.contains("hostport=${rest%%/*}") && prod.contains("path=/${rest#*/}"),
        "跨行原始串里的 shell 被当块注释抹了：{prod:?}"
    );
    assert!(
        prod.contains("keep_me()"),
        "原始串把后面的真代码吃了：{prod:?}"
    );
}

/// 抹成**等长空格** ⇒ 行数与字节数都不变 ⇒ [`pin_line`] 的行号、
/// [`find_pinned`] 的字节偏移一格都不漂。
#[test]
fn stripping_block_comments_moves_neither_a_line_nor_a_byte() {
    let src = "fn a() {\n    /* 多字节也要\n       等长抹掉 */\n    keep_me();\n}\n";
    let out = strip_block_comments(src);
    assert_eq!(
        out.len(),
        src.len(),
        "字节数变了 ⇒ `find_pinned` 的偏移全体漂"
    );
    assert_eq!(
        out.split('\n').count(),
        src.split('\n').count(),
        "行数变了 ⇒ `pin_line` 的行号全体错位"
    );
    assert!(!out.contains("等长抹掉"), "没剥干净：{out:?}");
}

/// 🔴 兜底：模型崩了就**一个字都不剥**（宁可留洞，不许造假红），
/// 而且这件事**说得出来**（`block_comment_model_holds` 为 `false`）。
#[test]
fn a_broken_model_strips_nothing_and_says_so() {
    let unterminated = "fn a() {\n    /* 开了不收口\n    keep_me();\n}\n";
    assert!(
        !block_comment_model_holds(unterminated),
        "没收口的块注释居然被判成模型站得住"
    );
    assert_eq!(
        strip_block_comments(unterminated),
        unterminated,
        "兜底没有原样返回 —— 那会把真代码剥掉、造一片假红"
    );
}

/// 本 crate 自己的源码上，块注释词法**不许走兜底**（吃自己的狗粮）。
/// 树级的那两份（monitor / backend）由两侧各自的判据钉着。
/// ⚠ 09-04（`K-R25`）起两个单位一起量：整份文件 ＋ 按 `#[test]` 切出的每一块。
#[test]
fn this_crate_never_falls_back_to_not_stripping() {
    // 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §6.2` B 类〕**语料根要跨两棵树。**
    //
    // 本条的块那一半数的是「按 `#[test]` 切出来的块」，而本 crate 的测试模块
    // 剖分之后整个住进了 `<repo>/tests/bridge/crates/guard-core/`。
    // 上一版只给 `<crate>/src` 一棵根 ⇒ 块数从 30+ 掉到 1，本条**当场红**
    // （红得对：它的自检逐字说「切法坏了，本条的块那一半此刻是空转的」）。
    // ⇒ 按 `§6.3` B 类的修法补上测试那一棵，**地板一格没往下拧**（仍是 30）。
    // ⚠ `min_files` 从 1 拧到 **2**：那是**往上**拧 —— 两棵根都必须真的读到东西，
    //   少读一棵就红。给 1 的话「测试树没读到」会零命中地绿。
    let crate_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // `<crate>` = `<repo>/src/bridge/crates/guard-core` ⇒ 上去四级是仓根。
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("从 crate 目录上去四级该是仓根")
        .to_path_buf();
    let crate_tests = repo.join("tests/bridge/crates/guard-core");
    assert!(
        crate_tests.is_dir(),
        "{crate_tests:?} 不在 —— 本 crate 的测试树住址变了，\
         别把下面那棵根删掉了事（删掉就回到「块那一半空转」那个状态）"
    );
    // 两棵根互不包含（`§5.4b` 纪律 1）：一棵在 `src/` 下、一棵在 `tests/` 下。
    assert_block_comment_model_holds(&[&crate_src, &crate_tests], 2, 30);
}

/// 🔴🔴 **`K-R25` 的正题：钉「剥法的输入单位 = 看门判据的输入单位」这条关系。**
///
/// # 它钉的**不是**一个形状
///
/// 不是「剥法必须按文件跑」这句话（那是一个形状，换个写法就绕过去了），
/// 而是**「先切块再剥」与「先剥再切块」这两条路答案不同**这件事本身。
/// 只要它们不同，「剥法在哪个单位上跑」就是一个**承重**的选择，
/// 而看门判据 [`assert_block_comment_model_holds`] 量的单位就必须把它包住。
///
/// # 活体夹具（**不是空真**：它今天真的分得开两条路）
///
/// `/*` 落在 A 块里、`*/` 落在 B 块里 —— **整份文件配平、单块不配平**。
/// 对 rustc 这就是一条普通的跨行块注释（编得过），所以
/// **看门判据的文件那一半在它上面是绿的**，而块那一半会红。
/// 这一形是 `K-R9` 落定拍在真仓上实打出来的（`tests/evidence/K-R9-R2-fallback-watch-scope.md §C 刀 2`：
/// 那一趟两条判据全绿、全量 monitor `1278 passed; 0 failed`）。
///
/// ⚠ **谁退掉哪一格会让本条红**：兜底改成「照剥」⇒ ② 断；
/// 词法不再跨行延续状态 ⇒ ① 或 ④ 断；剥法不再抹掉注释内容 ⇒ ③ 断；
/// 「剥完再切」不再收掉注释里那条边界 ⇒ ⑤ 断。
#[test]
fn cutting_first_and_stripping_first_do_not_give_the_same_answer() {
    // 自指：这两个串**运行时拼** —— 写死会让本文件自己多出一条边界 / 被判据数到。
    let attr = concat!("#[te", "st]");
    let gate = concat!("demand_tmux_", "shim(");
    // A 块开了块注释而没收口；收口的 `*/` 落在 B 块里。
    let straddling = format!(
        "{attr}\nfn a() {{\n    /*\n    let shim = {gate}\"x\");\n}}\n\n\
             {attr}\nfn b() {{\n    */\n    let _ = 0;\n}}\n"
    );
    // ① 整份文件那一档**是配平的** ⇒ 看门判据的文件那一半在它上面绿。
    assert!(
        block_comment_model_holds(&straddling),
        "夹具坏了：整份文件本该配平（一个 `/*` 一个 `*/`），否则下面证不出「文件级看不见」"
    );
    // ② 先切块再剥 ⇒ 单位是**块**，而块上模型崩了（兜底）。
    let cut_first = test_attr_chunks(&straddling);
    let broken: Vec<usize> = cut_first
        .iter()
        .enumerate()
        .filter(|(_, c)| !block_comment_model_holds(c))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        broken.len(),
        1,
        "先切块再剥：本该恰好 1 块掉进兜底（整份配平、单块不配平），实得 {broken:?}；\
             切出 {} 块。**这一格断了就说明两条路不再分得开** —— 那时本条钉的关系没了对象，\
             回来重新造夹具，别把本条删掉。",
        cut_first.len()
    );
    // ③ 兜底 = 一个字都不剥 ⇒ 注释里那句真调用**原样活着**，洞是开着的。
    assert!(
        strip_comment_lines(&cut_first[broken[0]]).contains(gate),
        "兜底那一块居然被剥了 —— 那么「宁可留洞，不许造假红」这条取舍变了，\
             本条的 ② 与它一起要重判"
    );
    // ④ 先剥整份再切块 ⇒ 每一块都干净，注释里那句话**一块都不剩**。
    let stripped_first = strip_comment_lines(&straddling);
    let after = test_attr_chunks(&stripped_first);
    for (i, c) in after.iter().enumerate() {
        assert!(
            block_comment_model_holds(c),
            "先剥再切之后第 {i} 块仍然掉进兜底 —— 剥法没把跨块的那条注释吃掉"
        );
        assert!(
            !c.contains(gate),
            "先剥再切之后第 {i} 块里还留着注释里那句真调用 —— 洞没关上：{c:?}"
        );
    }
    // ⑤ 前置问题的答案（`KR25D2` 甲的那一格）：**落在块注释里面的那条 `#[test]`
    //    边界会被剥掉**，于是前后两块合成一块。那是**对的** —— 注释里的 `#[test]`
    //    不是一条测试。而行数一个都不少（剥法等长抹空格 / 整行换空行）。
    assert_eq!(
        (cut_first.len(), after.len()),
        (2, 1),
        "边界数变了：剥之前该切出 2 块（第二条 `{attr}` 落在块注释里面）、\
             剥之后该只剩 1 块。这一格是「剥完再切，边界本身会不会被剥掉」那个前置问题的读数"
    );
    assert_eq!(
        stripped_first.lines().count(),
        straddling.lines().count(),
        "行数变了 ⇒ `pin_line` 的行号与 `.lines().take(n)` 的窗口全体错位"
    );
    // ⑥ 对照臂（**块内配平**那一形）：先切块再剥**照样剥得掉**
    //    ⇒ `K-R9` 买到的东西还在，本条治的是**作用域**不是词法。
    let contained = format!(
        "{attr}\nfn a() {{\n    /*\n    let shim = {gate}\"x\");\n    */\n    let _ = 0;\n}}\n\n\
             {attr}\nfn b() {{\n    let _ = 1;\n}}\n"
    );
    for (i, c) in test_attr_chunks(&contained).iter().enumerate() {
        assert!(
            block_comment_model_holds(c),
            "对照臂第 {i} 块本该配平（`/*` 与 `*/` 都在块内），夹具坏了"
        );
        assert!(
            !strip_comment_lines(c).contains(gate),
            "对照臂第 {i} 块：块内配平的块注释没被剥掉 —— **`K-R9` 买到的东西丢了**"
        );
    }
}

/// 行数不许变 —— [`pin_line`] 报的是行号，剥法改变行数就等于让它的读数全体漂一格。
#[test]
fn stripping_trailing_comments_keeps_the_line_count() {
    let src = "fn a() { let _ = 0; } // 尾\nfn b() {}\nlet u = \"a//b\";\n";
    assert_eq!(
        strip_trailing_comments(src).split('\n').count(),
        src.split('\n').count(),
        "行数变了 ⇒ pin_line 的行号全体错位"
    );
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

/// 一份「生产 ＋ 一个测试模块 ＋ 生产」的语料，可见性修饰由调用方给。
///
/// 三段都带**只在自己那一段出现**的锚点 ⇒ 剥对了、剥过头了、没剥都分得开。
fn vis_fixture(vis: &str) -> String {
    format!(
        "fn head_anchor() {{}}\n\
             #[cfg(test)]\n\
             {vis}mod t {{\n    \
                 fn inside_anchor() {{}}\n    \
                 #[{}]\n    \
                 fn x() {{}}\n\
             }}\n\
             fn tail_anchor() {{}}\n",
        "test"
    )
}

/// 🔴 `KR75D1`：剥法认得出**带任何可见性修饰**的测试模块 —— 认的是形状，不是前缀表。
///
/// # 这条判据凭什么杀得死「换成一张前缀表」那条退路
///
/// 表里前五档（无修饰 · `pub` · `pub(crate)` · `pub(super)` · `pub(self)`）任何一张
/// 前缀表都列得出来。承重的是后面那一档：`pub(in crate::…)` 的**路径任意长**，
/// 而这里逐档拉到 8 层 —— 前缀表要接住它就得写成无穷张表。
/// 再加两档**空白形状**（`pub  (crate)` / `pub(crate)   mod`）：Rust 允许，
/// 而按整段字面比对的写法接不住。
///
/// **死值验**：把 `test_module_ranges` 里那一句退回 `mod_line.starts_with("mod ")`
/// ⇒ 本条必须红（读数落 `tests/evidence/K-R75-剥法认形状与真静默读数.md`）。
#[test]
fn a_test_module_is_recognised_whatever_its_visibility() {
    let mut vis: Vec<String> = ["", "pub ", "pub(crate) ", "pub(super) ", "pub(self) "]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    for depth in 1..=8 {
        let path = (0..depth)
            .map(|k| format!("m{k}"))
            .collect::<Vec<_>>()
            .join("::");
        vis.push(format!("pub(in crate::{path}) "));
    }
    // 空白也是形状的一部分（`pub (crate) mod` / `pub(crate)   mod` 都是合法 Rust）。
    vis.push("pub  (crate) ".to_string());
    vis.push("pub(crate)   ".to_string());
    assert!(vis.len() >= 15, "语料退化了：只剩 {} 档", vis.len());

    for v in &vis {
        let src = vis_fixture(v);
        let prod = production_source(&src);
        let test = test_source(&src);
        assert!(
            !prod.contains("inside_anchor"),
            "可见性写成 {v:?} 时测试模块没被剥掉 —— 整段测试代码留在生产段里。\n生产段：{prod:?}"
        );
        for keep in ["head_anchor", "tail_anchor"] {
            assert!(
                prod.contains(keep),
                "可见性写成 {v:?} 时把 {keep} 也剥掉了（剥过头）：{prod:?}"
            );
        }
        assert!(
            test.contains("inside_anchor"),
            "可见性写成 {v:?} 时测试段里没有模块体：{test:?}"
        );
        assert_eq!(
            prod.len() + test.len(),
            src.len(),
            "可见性写成 {v:?} 时两半拼不回原文"
        );
    }
}

/// [`strip_visibility`] 的**反向那半**：不是可见性修饰的东西一个字都不许切。
///
/// 少了这一条，把它写成 `line.trim_start_matches(|c| c != 'm')` 之类的粗刀也能过上一条。
#[test]
fn strip_visibility_leaves_everything_else_alone() {
    for line in [
        "mod tests {",
        "pubsub_mod tests {", // `pub` 只是标识符的开头
        "publish mod x {",    // 同上
        "pub(crate mod x {",  // 括号不配平 ⇒ 不认，原样退回
        "fn pub_thing() {",
        "use super::*;",
        "",
    ] {
        assert_eq!(
            strip_visibility(line),
            line,
            "{line:?} 里没有可见性修饰，却被切了"
        );
    }
    assert_eq!(strip_visibility("pub mod x {"), "mod x {");
    assert_eq!(strip_visibility("pub(crate) mod x {"), "mod x {");
    assert_eq!(strip_visibility("pub(in crate::a::b) mod x {"), "mod x {");
}

/// 🔴 `KR75D2`：**「剥法认不出的测试模块」这一族有人在数** —— 而且它数得出下一形。
///
/// 三组语料，每组都是剥法今天**认不出**的写法（属性与 `mod` 之间夹了东西 / 同一行 /
/// 属性写成 `cfg(all(test, …))` 且夹了别的属性）⇒ [`assert_no_unstripped_test_module`]
/// 必须逐个咬住，**并把 `who` 印进去**（点名那份文件是这条判据的产出，不是附赠）。
///
/// ⚠ 反向那半同样承重：`#[cfg(test)]` 挂在**不是 `mod`** 的花括号体上（`fn`/`impl`/`enum`/
/// `thread_local!`）是本仓真盘上 31 处的现状，**不许红**；无花括号体的 `mod x;` 也不许红。
#[test]
fn the_unstripped_test_module_alarm_names_the_file() {
    let cfg = format!("#[{}({})]", "cfg", "test");
    // 这三条都是「剥法认不出」的形状：`production_source` 剥不掉它们。
    let unseen = [
        // 属性与 `mod` 之间夹了一行文档注释
        format!("fn a() {{}}\n{cfg}\n/// 说明\nmod later {{\n    fn z() {{}}\n}}\n"),
        // 同一行
        format!("fn a() {{}}\n{cfg} mod inline {{\n    fn z() {{}}\n}}\n"),
        // 复合 cfg ＋ 夹了另一条属性
        format!(
            "fn a() {{}}\n#[{}(all({}, unix))]\n#[allow(dead_code)]\nmod both {{\n    fn z() {{}}\n}}\n",
            "cfg", "test"
        ),
    ];
    for (k, src) in unseen.iter().enumerate() {
        let prod = production_source(src);
        assert!(
            prod.contains("fn z()"),
            "第 {k} 组语料本该是「剥法认不出」的，可它被剥掉了 —— 语料失效，本条在空转"
        );
        let r = std::panic::catch_unwind(|| {
            assert_no_unstripped_test_module("某棵树/某文件.rs", &prod)
        });
        let e = r.expect_err(&format!("第 {k} 组语料没红 —— 这就是那个不会响的闹钟"));
        let msg = e.downcast_ref::<String>().cloned().unwrap_or_else(|| {
            e.downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .unwrap_or_default()
        });
        assert!(
            msg.contains("某棵树/某文件.rs"),
            "第 {k} 组红了但没点名那份文件：{msg}"
        );
    }
    // 反向：这些**不许**红。
    let quiet = [
        format!("{cfg}\nmod decl_only;\nfn a() {{}}\n"),
        format!("{cfg}\nfn helper() {{}}\n"),
        format!("{cfg}\nimpl Drop for G {{\n    fn drop(&mut self) {{}}\n}}\n"),
        format!("{cfg}\nenum V {{\n    A,\n}}\n"),
        format!("{cfg}\nthread_local! {{\n    static X: u8 = 0;\n}}\n"),
        format!("{cfg}\npub(crate) use m::x;\n"),
        "fn a() {}\nlet s = \"model_id\";\n".to_string(),
    ];
    for (k, src) in quiet.iter().enumerate() {
        assert_no_unstripped_test_module("不该红", src);
        let _ = k;
    }
}

/// `assert_no_unstripped_test_module` 与剥法**不共用匹配单位** —— 这是它不会一起瞎掉的理由。
///
/// 用 `modern_thing` / `commodity` 这种**含 `mod` 子串但不是 `mod` 这个词**的行钉住：
/// 若哪天有人把它退回裸 `contains("mod")`，本条当场红。
#[test]
fn the_alarm_matches_mod_as_a_word_not_as_a_substring() {
    let cfg = format!("#[{}({})]", "cfg", "test");
    for line in [
        "fn modern_thing() {",
        "impl Commodity for X {",
        "struct Model {",
    ] {
        assert_no_unstripped_test_module("不该红", &format!("{cfg}\n{line}\n}}\n"));
    }
    // 而真的 `mod` 这个词必须咬住。
    let r = std::panic::catch_unwind(|| {
        assert_no_unstripped_test_module("该红", &format!("{cfg}\npub(crate) mod m {{\n}}\n"))
    });
    assert!(r.is_err(), "真的 `mod` 没咬住 —— 匹配单位缩得太小了");
}

/// ★ `production_source` 与 `test_source` 必须是**互补的两半**。
///
/// 它们共用 `test_module_ranges`，这条钉的是「共用」这件事真的成立：
/// 两半拼起来的长度等于原文（区间不重叠、不漏），且各自只装该装的东西。
#[test]
fn production_and_test_halves_partition_the_source() {
    let src = "fn a() {}\n#[cfg(test)]\nmod m1 {\n    fn t1() {}\n}\nfn b() {}\n\
                   #[cfg(test)]\nmod m2 {\n    fn t2() {}\n}\nfn c() {}\n";
    let prod = production_source(src);
    let test = test_source(src);
    assert_eq!(
        prod.len() + test.len(),
        src.len(),
        "两半拼不回原文 —— 区间重叠或漏了：\nprod={prod:?}\ntest={test:?}"
    );
    for t in ["fn t1()", "fn t2()"] {
        assert!(test.contains(t), "{t} 不在测试段里：{test:?}");
        assert!(!prod.contains(t), "{t} 漏进了生产段：{prod:?}");
    }
    for pcode in ["fn a()", "fn b()", "fn c()"] {
        assert!(prod.contains(pcode), "{pcode} 不在生产段里：{prod:?}");
        assert!(!test.contains(pcode), "{pcode} 漏进了测试段：{test:?}");
    }
}

// ── F24：匹配单位比事实小 ────────────────────────────────────────────
//
// 下面每一条都配一句**对照**：先证明裸 `contains` 在同一份输入上是绿的，
// 再证明 `find_pinned` / `pin_line` 是红的。没有对照那半，就分不清
// 「判据抓到了」和「输入本来就不含那个串」。

/// `contains_word` 不要求唯一，但仍然要求有边界。
#[test]
fn contains_word_needs_a_boundary_but_not_uniqueness() {
    assert!(
        contains_word("let ccm = x; ccm.len(); ccm", "ccm"),
        "多次出现不该拒绝"
    );
    assert!(
        !contains_word("let ccm_raw = 1;", "ccm"),
        "`ccm_raw` 不是 `ccm` 这个词"
    );
    assert!(
        "let ccm_raw = 1;".contains("ccm"),
        "对照组前提不成立：裸 contains 在这里是绿的"
    );
    assert!(!contains_word("anything", ""), "空 needle 一律否");
}

/// ★ F05 的形状：中文短语被撑大（`起流` ⊂ `起流程`）。
#[test]
fn a_cjk_needle_that_got_stretched_is_rejected() {
    let hay = "[perf] 起流程 耗时 12ms";
    assert!(
        hay.contains("起流"),
        "对照组前提不成立：输入里本来就没有那个子串"
    );
    let e = find_pinned(hay, "起流").expect_err("被撑大的命中必须红");
    assert!(e.contains("被撑大"), "诊断没点明是被撑大：{e}");
    // 反向：真的是 `起流` 时不许红。
    assert!(find_pinned("[perf] 起流 耗时", "起流").is_ok());
}

/// ★ F16 的形状：连字符续接（`src/backend` ⊂ `src/backend-X`）。
#[test]
fn a_hyphen_extension_is_rejected() {
    let hay = "working-directory: src/backend-X\n";
    assert!(hay.contains("src/backend"), "对照组前提不成立");
    assert!(find_pinned(hay, "src/backend").is_err());
    assert!(find_pinned("working-directory: src/backend\n", "src/backend").is_ok());
}

/// ★ `polling_registry` 的活样本：`sleep 1` ⊂ `sleep 10`。
///
/// 判据名与头注都写着「**每秒**」，而 `sleep 10` 让裸 `contains` 照样绿 ——
/// 事实（每秒）变了，判据不知道。
#[test]
fn sleep_one_does_not_match_sleep_ten() {
    let ten = "    while :; do\n      sleep 10\n    done\n";
    assert!(
        ten.contains("sleep 1"),
        "对照组前提不成立：裸 contains 在这里本该是绿的"
    );
    assert!(
        find_pinned(ten, "sleep 1").is_err(),
        "`sleep 10` 被当成了 `sleep 1`"
    );
    let one = "    while :; do\n      sleep 1\n    done\n";
    assert!(find_pinned(one, "sleep 1").is_ok(), "真的是每秒时不许红");
}

/// ★ F19 的形状：needle 不唯一 ⇒ 断言指不明是哪一处。
#[test]
fn a_needle_that_matches_two_places_is_rejected() {
    let hay = "fn a(exe_suffix: &str) {}\nfn b(exe_suffix: &str) {}\n";
    assert!(hay.contains("exe_suffix: &str"), "对照组前提不成立");
    let e = find_pinned(hay, "exe_suffix: &str").expect_err("两处命中必须红");
    assert!(e.contains("命中 2 处"), "诊断没说清有几处：{e}");
}

/// 空 needle 会匹配到任何地方 ⇒ 拒绝，不许静默 `Ok`。
#[test]
fn an_empty_needle_is_rejected() {
    assert!(find_pinned("whatever", "").is_err());
}

/// 一处都没有时的诊断要说「空转」，不能只说「不匹配」——
/// 抽取面画错和事实真没了是两回事，诊断得让人分得出来。
#[test]
fn a_missing_needle_says_the_guard_is_idling() {
    let e = find_pinned("nothing here", "absent_token").expect_err("找不到必须红");
    assert!(e.contains("空转"), "诊断没提醒可能是抽取面画错了：{e}");
}

/// ★ `pin_line`：`find_pinned` 的边界判据挡不住「同一行被加长」中分隔符非标识符的那类。
#[test]
fn pin_line_catches_what_the_boundary_check_cannot() {
    let hay = "defaults:\n  working-directory: src/backend/sub\n";
    // `/` 不是标识符字符 ⇒ 边界看起来是干净的，`find_pinned` 会放过。
    assert!(
        find_pinned(hay, "working-directory: src/backend").is_ok(),
        "本条的前提是 find_pinned 在这里放过 —— 前提变了就把这条一起改"
    );
    let e = pin_line(hay, "working-directory: src/backend").expect_err("整行判据必须红");
    assert!(e.contains("撑大"), "诊断没点明事实被撑大：{e}");
    assert!(
        pin_line(
            "  working-directory: src/backend\n",
            "working-directory: src/backend"
        )
        .is_ok(),
        "trim 之后相等的行不许红"
    );
}

/// `pin_line` 的两行同名情形。
#[test]
fn pin_line_rejects_two_identical_lines() {
    let e = pin_line("a\nx\nb\nx\n", "x").expect_err("两行相同必须红");
    assert!(e.contains("有 2 行"), "诊断没说有几行：{e}");
}

// ── KR110D1 夹具：区间在「不是代码」的字节上收尾 ──────────────────────
//
// 三形共一个病：收尾针「列 0 的右大括号」原先在**裸文本**上找。
// 三份夹具各自只装一形，撤掉修法（锚点改回裸 `src`）时**三条一起红**。

/// 一份「生产 ＋ 一个测试模块（里面藏着一个列 0 `}` 的 `x`）＋ 生产」的语料。
///
/// `head` / `tail` 两个锚点**只在自己那一段出现** ⇒ 剥对了、剥早了、剥过头了三种都分得开。
fn early_close_fixture(inner: &str) -> String {
    let cfg = format!("#[{}(test)]", "cfg");
    format!(
        "fn prod_head_anchor() {{}}\n\
             {cfg}\n\
             mod t {{\n    \
                 fn probe() {{\n\
             {inner}    \
                 }}\n    \
                 fn tail_anchor_only_in_tests() {{}}\n\
             }}\n\
             fn prod_tail_anchor() {{}}\n"
    )
}

/// 三形各判一次：区间**不许**在字符串/注释里的列 0 `}` 上收尾。
#[test]
fn a_column_zero_brace_that_is_not_code_does_not_close_the_test_module() {
    let shapes: [(&str, &str); 3] = [
        (
            "原始字符串",
            concat!(
                "        let s = r#\"\n",
                "fn inner() {\n",
                "}\"#;\n",
                "        let _ = s;\n",
            ),
        ),
        (
            "块注释",
            concat!(
                "        /*\n",
                "}\n",
                "        */\n",
                "        let _ = 0;\n",
            ),
        ),
        (
            "普通串里的真换行",
            concat!("        let s = \"\n", "}\";\n", "        let _ = s;\n"),
        ),
    ];
    for (what, inner) in shapes {
        let src = early_close_fixture(inner);
        let prod = production_source(&src);
        let test = test_source(&src);
        assert!(
            test.contains("tail_anchor_only_in_tests"),
            "[{what}] 测试段少了尾巴 —— 区间在字符串/注释里提前收尾了：\n{test}"
        );
        assert!(
            !prod.contains("tail_anchor_only_in_tests"),
            "[{what}] 测试代码漏进生产段 —— 每一条按生产段扫的判据在这份文件上人群都错了：\n{prod}"
        );
        for keep in ["fn prod_head_anchor()", "fn prod_tail_anchor()"] {
            assert!(
                prod.contains(keep),
                "[{what}] 剥过头了，生产段少了 `{keep}`：\n{prod}"
            );
        }
        assert_eq!(
            prod.len() + test.len(),
            src.len(),
            "[{what}] 两半拼不回原文 —— 区间重叠或漏了"
        );
    }
}

/// 掩码只抹「不是代码」的字节，**等长**，而且真的把那几档都抹掉了。
#[test]
fn the_lexical_mask_blanks_only_non_code_bytes_and_keeps_the_length() {
    let src = concat!(
        "let a = \"str{\";\n",
        "let b = '{';\n",
        "let c = r#\"raw{\"#;\n",
        "// line{\n",
        "/* block{ */\n",
        "let d = 1; // 尾{\n",
        "fn f() {}\n",
    );
    let m = mask_all_literals(src).expect("这份文本词法收得了口");
    assert_eq!(m.len(), src.len(), "掩码不等长 ⇒ 所有字节偏移全体错位");
    assert_eq!(
        m.matches('{').count(),
        1,
        "只该剩下 `fn f() {{}}` 那一个 `{{`，实得：\n{m}"
    );
    assert!(m.contains("let a = "), "把代码也抹掉了：\n{m}");
    assert!(m.contains("fn f() {}"), "把代码也抹掉了：\n{m}");
    // 兜底那一档：收不了口的原始串 ⇒ `None`，而不是乱抹一气。
    assert!(
        mask_all_literals("let x = r#\"never closed\n").is_none(),
        "词法收不了口却没走兜底"
    );
}

// ── KR110D1-selftest ──
/// 新判据**真的会咬人**：切出来的区间花括号不配平就红，配平就不红。
#[test]
fn the_brace_balance_check_actually_bites() {
    let cfg = format!("#[{}(test)]", "cfg");
    // 收不了尾（文件到头都没有列 0 的 `}`）⇒ 区间一路吞到 EOF ⇒ `{` 比 `}` 多。
    let unbalanced = format!("fn a() {{}}\n{cfg}\nmod t {{\n    fn x() {{}}\n");
    let r = std::panic::catch_unwind(|| {
        assert_test_module_ranges_are_brace_balanced("该红", &unbalanced)
    });
    assert!(r.is_err(), "不配平的区间没咬住 —— 本条是安慰剂");
    // 反向：正常语料不许误报，而且它**真的看过区间**（不是零命中地绿）。
    let ok = format!("fn a() {{}}\n{cfg}\nmod t {{\n    fn x() {{}}\n}}\nfn b() {{}}\n");
    assert_eq!(
        test_module_ranges(&ok).len(),
        1,
        "夹具本身没切出区间 ⇒ 下面那句「不许红」是空真"
    );
    assert_test_module_ranges_are_brace_balanced("不许红", &ok);
}

/// 词法收不了口时，新判据说的是「**判不了**」，不是静默通过。
#[test]
fn the_brace_balance_check_says_it_cannot_tell_when_the_lexer_gives_up() {
    let src = "fn a() {}\nlet x = r#\"never closed\n";
    let e =
        std::panic::catch_unwind(|| assert_test_module_ranges_are_brace_balanced("判不了", src))
            .expect_err("词法兜底时必须出声");
    let msg = e
        .downcast_ref::<String>()
        .map(String::as_str)
        .unwrap_or_default();
    assert!(msg.contains("判不了"), "诊断没说「判不了」：{msg}");
}
// ── /KR110D1-selftest ──

/// `assert_tree_strips_clean` 的计数自检真的会咬人。
#[test]
fn the_tree_walk_floor_actually_bites() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let r = std::panic::catch_unwind(|| assert_tree_strips_clean(&root, 9_999));
    assert!(r.is_err(), "地板远高于实际文件数却没红 —— 计数自检是空转的");
}
