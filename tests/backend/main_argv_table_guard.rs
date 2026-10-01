use super::{STREAM_FLAGS, SUBCOMMANDS, SUBCOMMAND_OPTIONS};

/// 4 个分派文件里出现的每个 `--token`。
///
/// **直接复用 U6a 的抽取**（`protocol_doc_guard` 的 `DISPATCH_FILES` +
/// `dispatched_subcommands`），而不是在这里重抄一份文件名单 —— 两份名单必然漂移，
/// 而 U6a 那份已经有 `dispatch_registry_is_complete` 反向核对它没漏文件。
fn dispatched() -> Vec<String> {
    crate::protocol_doc_guard::dispatched_subcommands()
}

/// 一次性查询那个 `match` 块的**块内**文本 —— 分派臂那一侧唯一的取法。
///
/// 🔴 **它必须是块内的，这不是省事**：[`SUBCOMMANDS`] 那张表住在同一份文件的
/// 生产段里，扫描面一旦放大到整份文件，表就把臂喂饱了 —— 那正是
/// [`every_listed_subcommand_is_actually_dispatched`] 今天瞎掉的成因。
fn dispatch_block(src: &str) -> &str {
    let beg = src
        .find("let code = match args.first()")
        .expect("找不到一次性查询的调度块 —— 块界锚点变了");
    let end = src[beg..]
        .find("std::process::exit(code);")
        .expect("找不到调度块的结尾锚点")
        + beg;
    &src[beg..end]
}

/// 一段 `match` 块里的**臂 token**（按出现序去重）—— 只认**模式那一侧**。
///
/// 🔴 **只扫「trim 之后以 `Some("` 或 `| Some("` 打头」的行**，不扫整块：
/// 臂**体**里要是碰巧也出现一个 `Some("--x")`，整块扫法会把它当成一条真臂收进来
/// ⇒ 那条 token 于是「有落点」，而实际上没有 —— **这是一个假绿方向**，不是假红。
///
/// 运行时拼 `Some("`，免得本函数自己的文本被别的扫描器当成一条分派臂。
fn arm_tokens(block: &str) -> Vec<&str> {
    let needle = format!("{}(\"", "Some");
    let mut out: Vec<&str> = Vec::new();
    for line in block.lines() {
        let l = line.trim_start();
        let l = l.strip_prefix("| ").unwrap_or(l);
        if !l.starts_with(needle.as_str()) {
            continue;
        }
        let rest = &l[needle.len()..];
        if let Some(end) = rest.find('"') {
            let t = &rest[..end];
            if t.starts_with("--") && !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

/// ★★ **每条调度臂都必须真的调到一个实现**〔G1，Phase G 变异抽样 A2 的产物〕。
///
/// # 为什么需要这条：隔壁那条名字里写着它该抓这个，但它没验
///
/// [`every_listed_subcommand_is_actually_dispatched`] 靠
/// [`crate::protocol_doc_guard::dispatched_subcommands`] 抽 **臂上的 token**
/// （`"--resolve"` 这个字面量）。变异 A2 把
/// `Some("--resolve") => control::resolve_query::run(…)` 换成 `Some("--resolve") => 2,`
/// —— **token 还在，那条判据全绿**，而 `--resolve` 已经彻底不工作了。
/// ⇒ 名字里的 **"actually dispatched"** 它没验；它验的是「有一条臂」。
///
/// ★ 一般化（判据覆盖面**第④格·性质面**）：**判据比的必须是它声称的那个性质本身。**
/// 「臂上有 token」与「臂真的调了实现」是两件事，正如「同一个值」≠「同一个来源」。
///
/// # 本条钉什么
///
/// 逐条取调度块里每一条臂 `=>` 右边的**臂体**，断言它是**一次调用**
/// （含 `::` 与 `(`），而不是常量/裸表达式。
///
/// ⚠ 抽取器自检：臂数必须 ≥ 7（**按实测写**，今天恰好 7 —— 6 条具名 + 1 条 `_`）。
/// 少于它就是块界找错或剥测试剥过头，本条会零命中地绿。
#[test]
fn every_dispatch_arm_actually_calls_an_implementation() {
    let src = crate::guard_support::production_code(include_str!("../../src/backend/main.rs"));
    let block = dispatch_block(&src);
    let arms: Vec<&str> = block
        .lines()
        .filter_map(|l| l.split_once("=>"))
        .map(|(_, body)| body.trim())
        .collect();
    assert!(
        arms.len() >= 7,
        "只抽到 {} 条调度臂（应 ≥7：6 条具名 + 1 条 `_`）—— 块界找错或剥过头，本条在空转：{arms:?}",
        arms.len()
    );
    for body in &arms {
        let b = body.trim_end_matches(',').trim();
        assert!(
            b.contains("::") && b.contains('('),
            "调度臂的臂体不是一次调用：{b:?}\n\
                 ⇒ 这条子命令的 token 还挂在那儿，但它已经不调任何实现了 —— \n\
                 v3.4.0 出过同型事故（`--account-trust-zero` 漏列 ⇒ 落进 `_` 臂）。\n\
                 ⚠ 隔壁 `every_listed_subcommand_is_actually_dispatched` **看不见这种错**：\n\
                 它抽的是臂上的 token，不是臂体。"
        );
    }
}

/// ★ 每个被分派的 token 都必须在三分表里。
#[test]
fn every_dispatched_token_is_classified() {
    let tokens = dispatched();
    assert!(
        tokens.len() >= 14,
        "只抽到 {} 个 token —— 抽取坏了，本断言在空转：{tokens:?}",
        tokens.len()
    );
    let unclassified: Vec<&String> = tokens
        .iter()
        .filter(|t| {
            let t = t.as_str();
            !STREAM_FLAGS.contains(&t)
                && !SUBCOMMANDS.contains(&t)
                && !SUBCOMMAND_OPTIONS.contains(&t)
        })
        .collect();
    assert!(
        unclassified.is_empty(),
        "这些 token 被分派了但不在 argv 三分表里：{unclassified:?}\n\
             ⚠ 后果**不是**报错退出，而是：`args[0]` 认不出来 ⇒ 当成流模式 ⇒ \n\
             那条子命令**静默变成起了个流**，调用方拿到 jsonl 行而不是查询结果。\n\
             把它加进 STREAM_FLAGS / SUBCOMMANDS / SUBCOMMAND_OPTIONS 之一。"
    );
}

/// ★ 三类**两两不交**。同一个 token 分两类 = 判据自相矛盾。
#[test]
fn the_three_classes_do_not_overlap() {
    for (an, a) in [
        ("STREAM_FLAGS", STREAM_FLAGS),
        ("SUBCOMMANDS", SUBCOMMANDS),
        ("SUBCOMMAND_OPTIONS", SUBCOMMAND_OPTIONS),
    ] {
        for (bn, b) in [
            ("STREAM_FLAGS", STREAM_FLAGS),
            ("SUBCOMMANDS", SUBCOMMANDS),
            ("SUBCOMMAND_OPTIONS", SUBCOMMAND_OPTIONS),
        ] {
            if an == bn {
                continue;
            }
            let both: Vec<&&str> = a.iter().filter(|t| b.contains(t)).collect();
            assert!(both.is_empty(), "{an} 与 {bn} 同时含有：{both:?}");
        }
    }
}

/// ★ 表里登记的子命令必须**真的被分派**（防表里堆死条目，让上面那条越来越松）。
///
/// # ⚠ 它在「**臂删了、表还在**」这一形上**不红** —— 这不是缺陷登记，是它的构造
///
/// 它拿 [`SUBCOMMANDS`] 去比 [`dispatched`]，而后者是**整份文件**的 `"--` 字面量扫描
/// （`protocol_doc_guard::DISPATCH_FILES` 的第一项就是 `main.rs`）——
/// 而 [`SUBCOMMANDS`] **自己就住在那份被扫的生产段里** ⇒ 「表里有这个串」与
/// 「源码里有这个串」在同一次扫描里互相喂饱，摘掉任何一条分派臂它都照样全绿。
/// ⇒ 那一形今天由 [`every_listed_subcommand_has_a_live_dispatch_route`] 接住（`K-R102`）。
#[test]
fn every_listed_subcommand_is_actually_dispatched() {
    let tokens = dispatched();
    let ghosts: Vec<&&str> = SUBCOMMANDS
        .iter()
        .filter(|t| !tokens.iter().any(|d| d == *t))
        .collect();
    assert!(
        ghosts.is_empty(),
        "SUBCOMMANDS 里这些 token 没有任何分派点：{ghosts:?}（删掉，别让表虚胖）"
    );
}

/// **同时**落在「字面量臂」与「派生臂」两条路上的那几条 —— `(token, 它为什么非要留自己那条臂)`。
///
/// # 为什么这一张是手写的，而上面那三条路是遍历出来的
///
/// 「今天谁在双路上」遍历得出来；「**它为什么必须留着自己那条臂**」遍历不出来 ——
/// 那是一次裁定。**两种角色的发现机制不同 ⇒ 分两张表**，
/// 抄 `src/frontend/shell/src/backend_kill.rs` 的 `CREATION_PATHS`／`VALIDATORS`
/// （逐字：「一张表混装两种角色是它自己会红的那种错」）。
///
/// 没有这一张，[`every_listed_subcommand_has_a_live_dispatch_route`] 在双路那几条上
/// **有一个洞**：摘掉它们的字面量臂，派生臂会静默接住 ⇒ 主断言不红，
/// 而那是一次**换路**（`--resolve` 那条臂上方逐字写着它「故意留在前面」，
/// 理由是仓外 aterm 的冻结契约「不拿『实际上一样』去赌」）。
const DUAL_ROUTE_ARMS: &[(&str, &str)] = &[
    (
        "--capture-pane",
        "`K-R86` 的只读抓屏原语：一次性 exec 直接调本体，不绕 CLI 面那层信封",
    ),
    (
        "--resolve",
        "信封与仓外 aterm 冻结在 2026-07-18，走原路一个字节都不动 —— \
             两条路的输出实为同一个 `CommandPlan`，而冻结的契约不拿「实际上一样」去赌",
    ),
];

/// ★★ `KR102D2` 甲：**表里每一条子命令都有一条活的分派落点。**
///
/// # 它买的是哪一形：「子命令表里有、分派臂没有」
///
/// 上面 [`every_listed_subcommand_is_actually_dispatched`] 的头注写清了它为什么看不见
/// 这一形。这一形不是假想：`K-P6b` 实打过一次（摘掉 `--dial` 那条臂，backend 侧
/// 一条判据都不红，见那时 `dial/dial_tests.rs` 里的 `the_dial_arm_is_actually_wired_into_the_dispatch`〔散文墓碑〕 ——
/// `--dial` 删了，那条判据换了入口、改名 `dial_tests.rs::the_link_arms_are_actually_wired_into_the_dispatch`），
/// `K-R86` 又撞了一次（`--capture-pane`）。两次都是**一件一件地各补一把伞**。
/// 本条是那把**总伞**：人群不是手写的，是 [`SUBCOMMANDS`] 自己。
///
/// # 🔴 两侧的发现机制刻意不同 —— 这是本条的承重要求，不是排版
///
/// - **表侧** = `SUBCOMMANDS` 这个**编译后的常量值**。没有抽取器可坏，改名增删自动反映。
/// - **臂侧** = 三处**块内**取法，每一处的扫描面都**不含那张表**：
///   ① `main.rs` 一次性查询 `match` 块（[`dispatch_block`]）里的 `Some("--x")` 字面量臂；
///   ② 那条**派生臂** —— 它**在不在**由块内文本判，它**认哪几条**由
///      [`crate::control::cli_control::spec_for`] 在**运行期**从 `inbound::REGISTRY` 派生；
///   ③ `_` 兜底臂交给 `observe::history_query::run`，它认哪几条由**那份文件里
///      它自己那个 `match` 块**判。
///
/// 分表这一形抄 `src/frontend/shell/src/backend_kill.rs` 的
/// `CREATION_PATHS`／`VALIDATORS`（逐字：「一张表混装两种角色是它自己会红的那种错，
/// 因为两种角色的发现机制不同」），不自己重发明。
///
/// ⚠ 头三条 `assert` 是**反空真地板**，它们只问「抽取有没有整个塌掉」，
/// **不问某一条在不在** —— 后者是下面那条主断言的活。地板刻意压在实测值以下，
/// 免得「摘掉一条臂」这一刀先撞上地板，红出来的话变成「抽取器可能坏了」
/// （红对了位置、讲错了成因，`inbound.rs` 那条 runtime 判据记过同一个坑）。
///
/// # ⚠ 它买不到什么（如实登记）
///
/// - 只判**够不够得到**，不判「调过去之后做得对不对」—— 那是各条命令自己的测试。
/// - 臂体是不是一次真调用由 [`every_dispatch_arm_actually_calls_an_implementation`] 判。
/// - 走**派生臂**那几条是**同生共死**的：摘掉那一条臂它们一起红，
///   摘不掉其中单独一条（那条臂上根本没有它们各自的字面量）。
/// - 它读的仍是**源码文本**（只是收窄到块内）⇒ token 连 `"--` 字面量都不出现的写法
///   （`concat!` 拼）一样看不见，与 `protocol_doc_guard` 头注登记的是同一条边界。
/// - **反方向**（表里删一条、臂还在）不在本条射程：那一形由
///   [`every_dispatched_token_is_classified`] 接住 —— 臂上那个 token 仍被扫到，
///   而它已经不在三分表里的任何一张 ⇒ 那条当场红。
#[test]
fn every_listed_subcommand_has_a_live_dispatch_route() {
    let src = crate::guard_support::production_code(include_str!("../../src/backend/main.rs"));
    let block = dispatch_block(&src);
    // 反喂饱自检：那张表**不许**落进臂侧的扫描面，否则本条退化成上面那条瞎子。
    assert!(
        !block.contains(&format!("SUB{}", "COMMANDS")),
        "分派块的扫描面里出现了那张表 —— 表与臂又回到同一次扫描里，本条此刻是个瞎子"
    );
    let literal = arm_tokens(block);
    let derived_arm = block.contains("cli_control::handles(");
    let catch_all = block.lines().any(|l| l.trim_start().starts_with("_ =>"));

    let hist_src = include_str!("../../src/backend/observe/history_query.rs");
    let hist = crate::guard_support::production_code(hist_src);
    let hbeg = hist
        .find("let result = match args.first()")
        .expect("`history_query::run` 的分派块起点锚点变了");
    let hend = hist[hbeg..]
        .find("Some(other) =>")
        .expect("`history_query::run` 的分派块收尾锚点变了")
        + hbeg;
    let fallback = arm_tokens(&hist[hbeg..hend]);

    // ── 反空真地板（只问「抽取塌没塌」，不问「某一条在不在」）──────────────
    assert!(
        literal.len() >= 6,
        "分派块里只抠到 {} 条字面量臂（地板 6，实测 14）—— 块界找错或抽取塌了：{literal:?}",
        literal.len()
    );
    assert!(
        derived_arm,
        "分派块里没有那条派生臂（`cli_control::handles`）—— 走 CLI 面的子命令一条都调不到了"
    );
    assert!(
        catch_all,
        "分派块里没有 `_` 兜底臂 —— 历史查询那一族一条都调不到了"
    );
    assert!(
        fallback.len() >= 3,
        "`history_query::run` 的块里只抠到 {} 条（地板 3，实测 5）—— 块界找错或抽取塌了：{fallback:?}",
        fallback.len()
    );

    // ★ 表侧也要有反空真地板：`SUBCOMMANDS` 被掏瘪 ⇒ 下面那个循环跑零圈、
    //   主断言**零命中地绿**（`testing.md` 硬规则 7：先证够得到，再问有没有违例）。
    //   地板取 **10**，与 `build_id_guard::subcommand_fingerprint` 那条**同一个数**
    //   （不另发明一个），而今天实测 26 —— 压得低是刻意的：删掉一两条子命令时
    //   要让上面/下面那几条真判据先说话，别先撞上这道自检。
    assert!(
        SUBCOMMANDS.len() >= 10,
        "`SUBCOMMANDS` 只剩 {} 条（地板 10，实测 26）—— 登记表被掏了，本条此刻在空转",
        SUBCOMMANDS.len()
    );

    // ── 主断言：表里每一条都得落在三条路之一上 ──────────────────────────
    let orphans: Vec<&str> = SUBCOMMANDS
        .iter()
        .copied()
        .filter(|tok| {
            if literal.contains(tok) {
                return false;
            }
            if derived_arm
                && (crate::control::cli_control::spec_for(tok).is_some()
                    || *tok == crate::control::cli_control::PROBE_FLAG)
            {
                return false;
            }
            !(catch_all && fallback.contains(tok))
        })
        .collect();
    assert!(
        orphans.is_empty(),
        "这些子命令**登记在表里，却没有任何一条分派路够得到**：{orphans:?}\n\
             ⇒ `is_query_mode` 那道闸门照旧放它们进查询模式（表还在），而下面没人接 ⇒\n\
             它们落进 `_` 臂走历史查询、回 `unknown argument` + exit 2 —— \n\
             v3.4.0 `--account-trust-zero` 那次事故的形状，只是方向反过来（那次是表里漏）。\n\
             三条合法落点：① 块内写一条 `Some(\"--x\") => …` 的字面量臂；\n\
             ② 让它上 `inbound::REGISTRY` 的 CLI 面（走派生臂）；\n\
             ③ 在 `observe::history_query::run` 里给它一条臂（走 `_` 兜底）。"
    );

    // ── 第二格：**双路**那几条的字面量臂不许悄悄消失（[`DUAL_ROUTE_ARMS`] 的活）──
    //
    // 上面那条主断言在它们身上**有个洞**：摘掉字面量臂，派生臂会静默接住 ⇒ 不红。
    // 而那是一次**换路**，不是等价重构。两个方向都判。
    let mut vanished: Vec<&str> = Vec::new();
    for (tok, _why) in DUAL_ROUTE_ARMS {
        if !literal.contains(tok) {
            vanished.push(tok);
        }
    }
    assert!(
        vanished.is_empty(),
        "这几条**登记过「必须走自己那条臂」**的子命令，字面量臂不见了：{vanished:?}\n\
             ⇒ 派生臂（CLI 面）会静默接住它们 —— 主断言因此不红，而**路已经换了**。\n\
             理由逐条写在 `DUAL_ROUTE_ARMS` 里。真要换路，改那张表并说清谁来承接旧信封。"
    );
    let mut unexplained: Vec<&str> = Vec::new();
    for tok in SUBCOMMANDS.iter().copied() {
        let on_derived = derived_arm && crate::control::cli_control::spec_for(tok).is_some();
        if on_derived && literal.contains(&tok) && !DUAL_ROUTE_ARMS.iter().any(|(t, _)| *t == tok) {
            unexplained.push(tok);
        }
    }
    assert!(
        unexplained.is_empty(),
        "这几条**同时**落在字面量臂与派生臂上，而 `DUAL_ROUTE_ARMS` 里没有它们：{unexplained:?}\n\
             ⇒ 两条路谁先谁后决定了调用方拿到哪一种信封，这件事必须有人写下理由。\n\
             要么把它加进那张表（连理由一起），要么把它那条字面量臂删掉、只走派生。"
    );
}

/// ★ 未知 `--flag` 不许把后端踢出流模式。
///
/// 变异回旧行为（「非空即查询」）⇒ 本测试红。
#[test]
fn an_unknown_flag_does_not_kick_the_backend_out_of_stream_mode() {
    let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(
        !super::is_query_mode(&v(&["--some-future-flag"])),
        "未知 flag 把后端踢进了查询模式 ⇒ exit 2、无 hello ⇒ monitor 重连死循环\n\
             （2026-07-09 事故的形状；实测过 `--some-future-flag` 会 rc=2）"
    );
    assert!(
        !super::is_query_mode(&v(&["--a", "--b"])),
        "多个未知 flag 同理"
    );
}

/// ★ 但未知**裸参数**仍要报错 —— 那是明确的调用错误，静默吞掉只会让人查半天。
#[test]
fn an_unknown_bare_argument_still_goes_to_the_error_path() {
    let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(super::is_query_mode(&v(&["/some/path"])));
    assert!(super::is_query_mode(&v(&["--future", "bare"])));
}

/// 已登记的子命令照旧进查询模式（回归）。
#[test]
fn listed_subcommands_still_enter_query_mode() {
    for c in SUBCOMMANDS {
        assert!(
            super::is_query_mode(&[c.to_string()]),
            "{c} 不再进查询模式了 —— 它会静默变成起了个流"
        );
    }
}

/// **argv 只在 `main.rs` 取一次**：后端生产树里读进程 argv 的地方（`env::args(` · `env::args_os(`）
/// 恰好一处，住 `main.rs::main`；`control/ccm/plan.rs` 那一格由调用方交（`ccm::run` 的 `process_argv`）。
#[test]
fn x15_the_process_argv_is_read_once_in_main() {
    let root = crate::guard_support::src_root();
    let needles = [format!("env::{}(", "args"), format!("env::{}(", "args_os")];
    let mut found: Vec<(String, String)> = Vec::new();
    let mut scanned = 0usize;
    for (path, src) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        scanned += 1;
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let prod = crate::guard_support::production_code(&src);
        for n in &needles {
            for (i, _) in prod.match_indices(n.as_str()) {
                let head = &prod[..i];
                let f = head.rfind("fn ").map_or(String::new(), |j| {
                    head[j + 3..]
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect()
                });
                found.push((rel.clone(), f));
            }
        }
    }
    assert!(
        scanned >= 100,
        "只扫到 {scanned} 份后端源文件 —— 遍历坏了，本条在空转"
    );
    assert_eq!(
        found,
        vec![("main.rs".to_string(), "main".to_string())],
        "读进程 argv 的地方不再恰是 `main.rs::main` 那一处（⑮）—— 要 argv 的模块由调用方交"
    );
}
