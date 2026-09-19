use std::path::{Path, PathBuf};

/// 裸遍历的形态。
const RAW_WALKS: &[&str] = &["read_dir(", "WalkDir", "collect_rs(", "collect_ts("];

/// **存量**：测试段里仍在裸遍历的文件（08-06 实测 31 个）。
///
/// ⚠ **只许变短。** 迁一个就从这里删一行并把 `PENDING_CEILING` 调下来。
/// 不许往里加 —— 新写的扫描型判据必须走 `guard_core::scan_tree!`。
///
/// # 🔴 「只许变短」今天**谁在守它**（`K-R38` 09-06，`D3①`）
///
/// [`the_pending_ratchet_never_turns_backwards`] —— 它拿**git 历史**当权威，
/// 断「今天这个数不许比它在历史上出现过的**最低档**还高」。
/// ⇒ **在此之前这句话只是一行注释里的纪律**：守它的
/// [`the_pending_inventory_only_shrinks`] 断的是 `n <= PENDING_CEILING`，
/// 而那个上限就在下面几行 ⇒ 抬一下就过（`K-R33` 的 `R33M3` 实打不红）。
///
/// ## ⚠ 它买到的比这句话的名字**小**，逐字写明没买到什么
///
/// - 守的是**这个数不许涨回去**，**不是**「清单真的在变短」——
///   一年一条没迁，本条照样绿。
/// - **删一行来腾余量**不归它管：接住那一形的是
///   [`no_new_guard_walks_the_tree_without_excluding_itself`]
///   （删掉的那份文件还在裸遍历 ⇒ 当场以 `newcomers` 红）。
/// - 它**挡不住把那条判据本身删掉** —— 买的是**留痕**，不是不可能。
const PENDING: &[&str] = &[
    // 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §6.2` C 类〕**下面 15 行换了住址，条数一格没变。**
    //    整份是判据的那批 registry 文件这一轮剖分了 ⇒ 它们的裸遍历跟着测试段
    //    搬进了 `tests/bridge/`。`PENDING_CEILING` **没有动** —— 一个裸遍历都没少。
    //    逐份点名（`src/bridge/src/X.rs` → `tests/bridge/…`）：
    //      `atomic_replace_registry.rs`      → `atomic_replace_registry_tests.rs`
    //      `backend/mod.rs`                  → `backend_layering.rs`〔见下面那条注〕
    //      `backend/observe/local_query.rs`  → `backend/observe/local_query_tests.rs`
    //      `cross_half_edge_registry.rs`     → `cross_half_edge_registry_tests.rs`
    //      `doc_claim_registry.rs`           → `doc_claim_registry_tests.rs`
    //      `doc_copy_registry.rs`            → `doc_copy_registry_tests.rs`
    //      `frame_cadence_guard.rs`          → `frame_cadence_guard_tests.rs`
    //      `gate_singleton_guard.rs`         → `gate_singleton_guard_tests.rs`
    //      `local_read_surface_registry.rs`  → `local_read_surface_registry_tests.rs`
    //      `polling_registry.rs`             → `polling_registry_tests.rs`
    //      `quote_singleton_guard.rs`        → `quote_singleton_guard_tests.rs`
    //      `rust_timer_registry.rs`          → `rust_timer_registry_tests.rs`
    //      `session_name_registry.rs`        → `session_name_registry_tests.rs`
    //      `shared_crate_registry.rs`        → `shared_crate_registry_tests.rs`
    //      `tmux_daemon_gate_guard.rs`       → `tmux_daemon_gate_guard_tests.rs`
    //
    // ⚠ **`backend/mod.rs` 那一条差点顶破棘轮，如实记**：它剖成了**两份**
    //    （`backend_tests.rs` ＋ `backend_layering.rs`），两份里都有裸遍历
    //    ⇒ 按文件数的这张清单会从 1 条变 2 条。抬上限是被明文禁止的
    //    （而且 `the_pending_ratchet_never_turns_backwards` 对着 git 历史比，
    //    抬了也不会绿）⇒ **真迁掉一个**：`backend_tests.rs::backend_files` 那个
    //    手写递归改走 `guard_core::scan_tree_excluding`（语义逐字相同，是纯死重）。
    //    ⇒ 清单里只留 `backend_layering.rs` 一条，条数与上限都不变。
    // 🔴 〔搬树 2026-09-18 · `设计/16 §6.2` C 类〕**下面 7 行换了住址，条数一格没变**
    //    （`PENDING_CEILING` 因此**没有动** —— 一个裸遍历都没少，只是它们跟着
    //    自己那条判据搬进了 `tests/`）。逐份点名：
    //      `backend/control/daemon_kill.rs`  → `tests/bridge/backend/control/daemon_kill_tests.rs`
    //      `backend/control/launch_wire.rs`  → `tests/bridge/backend/control/launch_wire_f07_main_path_tests.rs`
    //      `panorama.rs`                     → `tests/bridge/panorama_tests.rs`
    //      `parser.rs`                       → `tests/bridge/parser_tests.rs`
    //      `profile_installer.rs`            → `tests/bridge/profile_installer_tests.rs`
    //      `ssh_source.rs`                   → `tests/bridge/ssh_source_f032_idle_tests.rs`
    //      `utils.rs`                        → `tests/bridge/utils_tests.rs`
    "tests/bridge/atomic_replace_registry_tests.rs",
    "tests/bridge/backend/control/daemon_kill_tests.rs",
    "tests/bridge/backend/control/launch_wire_f07_main_path_tests.rs",
    "tests/bridge/backend_layering.rs",
    "tests/bridge/backend/observe/local_query_tests.rs",
    "tests/bridge/cross_half_edge_registry_tests.rs",
    "tests/bridge/doc_claim_registry_tests.rs",
    "tests/bridge/doc_copy_registry_tests.rs",
    "tests/bridge/frame_cadence_guard_tests.rs",
    "tests/bridge/gate_singleton_guard_tests.rs",
    "tests/bridge/local_read_surface_registry_tests.rs",
    "tests/bridge/panorama_tests.rs",
    "tests/bridge/parser_tests.rs",
    "tests/bridge/polling_registry_tests.rs",
    "tests/bridge/profile_installer_tests.rs",
    "tests/bridge/quote_singleton_guard_tests.rs",
    "tests/bridge/rust_timer_registry_tests.rs",
    "tests/bridge/session_name_registry_tests.rs",
    "tests/bridge/shared_crate_registry_tests.rs",
    "tests/bridge/ssh_source_f032_idle_tests.rs",
    "tests/bridge/tmux_daemon_gate_guard_tests.rs",
    "tests/bridge/utils_tests.rs",
    "tests/backend/layering_guard.rs",
    "tests/backend/no_timer_guard.rs",
    "src/backend/observe/watcher.rs",
    "tests/backend/platform/fallback_guard.rs",
    "tests/backend/protocol_doc_guard.rs",
    "tests/backend/readonly_guard.rs",
];

/// 存量上限（**递减棘轮**）。
///
/// 🔴 **只许往下调。** 守这句话的是 [`the_pending_ratchet_never_turns_backwards`]
/// （`K-R38` 09-06）：它对着 **git 历史**比，把这个数抬上去**当场红，而且提交了也不会绿**
/// —— 历史里那个更低的档还在。⇒ 别在这里试「先抬一格让今天好过」，那正是它挡的动作。
/// ⚠ 它守的是这个**数**；「删一行腾余量」那一形归
/// [`no_new_guard_walks_the_tree_without_excluding_itself`]。
// 08-08：`daemon_route.rs` 的裸遍历迁到了 `guard_core::scan_tree!`（那一轮把它的
// 发现面从一个目录扩到整棵树，顺带就该换掉手写遍历）⇒ 清单少一行，上限一起降。
const PENDING_CEILING: usize = 28; // `设计/50`：`account_usage.rs` 整删 ⇒ 存量少一条，上限同拍往下拧一格

/// 判定「这是一个带登记表的判据文件」的声明形态。**闭集，按名字认。**
///
/// 🔴 **这是一个闭集，不是一族形状** —— 往里加一个名字就是在**放宽**一条守卫，
/// 加之前先读模块头注那一节（`K-R33`）：它写着为什么这里走「闭集 ＋ 点名钉住」
/// 而不是「按形状认」，以及那条纪律要求新写的扫描型判据把表**起成这里的名字之一**。
const TABLE_DECLS: &[&str] = &[
    "const REGISTERED:",
    "const SITES:",
    "const SCHEDULING_SITES:",
    // `K-R33` 09-06：`K-R31` 那条判据的形态表（`local_backend.rs`）。
    // 全树现打：三棵树的 `.rs` 里 `const FORMS:` **恰好一处**，就是它 ⇒ 这一条不引入误采。
    "const FORMS:",
];

/// 识别器：这份**测试段**里有没有一张登记表。
///
/// 单独成函数（`K-R33`），是为了让下面那条反向自检能拿**合成文本**正反各喂一遍。
/// 直接在真树上判的话，「采到了它、而它过了」与「压根没扫到它」在输出上一模一样 ——
/// 那正是 `K-R33` 立件的那一格。
fn declares_a_guard_table(regs: &str) -> bool {
    TABLE_DECLS.iter().any(|d| regs.contains(d))
}

/// 反向那半的合法形态。**闭集。**
///
/// 🔴 **不许为了让新红变绿往里加成员**（`K-R36` `D2①` 明禁）：第一形 `assert_eq!(`
/// 几乎每一份测试段里都有 —— 闭集再宽一点，下面那两层判定就双双恒空，
/// 而输出与今天一模一样（绿）。真有第四种正当写法 ⇒ **单独论证 + 给读数**
/// （哪几条在用、为什么它也算「反向那半」），别顺手加。
///
/// 〔`K-R36` 09-06 把它从 [`every_registry_guard_keeps_its_reverse_half`] 的函数体里
/// 提到模块级，**成员一个没加、一个没减**。提出来的唯一理由：新加的那条反向自检
/// 要拿**同一份**闭集去判合成文本，而一个闭集只许有一个住址（`brief` 13b）。
/// 顺带删掉了原先那句「反向那半的**两种**合法形态」—— 那个基数写死在散文里，
/// 而成员早已是三个：13b 治的正是这一形，而它就长在这条判据自己头上。〕
const REVERSE: &[&str] = &["assert_eq!(", "已经不在了", "已经没有"];

/// 一个 `#[test]` 块里**那个 fn 项本身** —— 从 `fn` 那一行到**同缩进**的收尾 `}`。
///
/// # 为什么不能整块拿去判〔`K-R36` 09-06 现打的活体〕
///
/// [`guard_core::test_attr_chunks`] 切出来的块是「这条 `#[test]` 到下一条 `#[test]`」，
/// 它**还带着这条判据之后、下一条判据之前的模块级代码**（辅助函数 · 常量 ·
/// 下一条判据的文档注释）。拿整块判「表声明在谁体内」会把一张**模块级**的表
/// 算成「它上面那条判据自带的」：`polling_registry.rs` 的 `SCHEDULING_SITES`
/// 声明在 `every_data_poll_names_its_event_source_and_owner` **之后**、
/// 真正用它的 `every_scheduling_call_site_is_classified` **之前**
/// ⇒ 按块判会把它记到前者头上，而前者没有反向那半 ⇒ **一条假红**。
///
/// # 它认什么、认不出什么（认不出的是**漏判**，不是假绿）
///
/// 认：跳过块首的空行 / 属性行 / 注释行之后，**紧接着就是 `fn <名字>`**。
/// 这一条顺带把 [`guard_core::test_attr_chunks`] 的**模块级前言**那一块挡在外面
/// （前言跳过属性之后是 `mod … {`，不是 `fn`），而且**不按块的下标跳** ——
/// 文本以 `#[test]` 打头时前言那一块根本不存在，按下标跳会跳掉第一条真判据。
///
/// 认不出（整条跳过，不进人群）：`#[test]` 与 `fn` 之间夹着**折行**的属性
/// （`tmux.rs` 那个折行的 `#[cfg_attr(` 是这一形），以及 `pub fn` / `async fn`。
/// ⚠ **单行**的 `#[ignore = "…"]` / `#[cfg(…)]` **不是**漏判面 —— 块首的属性行会被跳过。
/// 09-06 现打：人群那几份文件里，本函数认不出的块 **0 块**
/// ⇒ 今天一条都没漏。分母与逐处住址在 `tests/evidence/K-R36-per-guard-census.py` 的输出里
/// （分母只取人群那几份就够：块里真有登记表声明 ⇒ 那份文件必定在人群里）。
///
/// 收尾靠**缩进配对**（`rustfmt` 的产物上成立），**不是花括号配平**：配平要解析字符串与
/// 字符字面量，而本仓的判据语料里满是**合成 Rust 源码串**（还有 raw string），
/// 一次失步就整段跟着错，而错的方向是**静默的绿**。宁可用一把粗而稳的尺子。
fn guard_fn_item(chunk: &str) -> Option<(String, String)> {
    let lines: Vec<&str> = chunk.lines().collect();
    let head = lines.iter().position(|l| {
        let t = l.trim_start();
        !(t.is_empty() || t.starts_with('#') || t.starts_with("//"))
    })?;
    let first = lines[head];
    let indent = &first[..first.len() - first.trim_start().len()];
    let name: String = first
        .trim_start()
        .strip_prefix("fn ")?
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        return None;
    }
    let close = format!("{indent}}}");
    let end = lines[head..]
        .iter()
        .position(|l| *l == close)
        .map_or(lines.len(), |k| head + k + 1);
    Some((name, lines[head..end].join("\n")))
}

/// 一段**测试段**文本里，每一条**把登记表声明在自己体内**的判据 —— `(判据名, 判据体)`。
///
/// 🔴 **单独成函数，与 [`declares_a_guard_table`] 同一个理由**：直接在真树上判的话，
/// 「每条判据各判各的」与「口径其实把整份文件的文本喂给了每一条」
/// 在输出上**一模一样**（都是绿）。
/// [`the_per_guard_split_does_not_hand_every_guard_the_whole_file`] 拿合成文本把这一格钉住。
fn guards_declaring_a_table(test_src: &str) -> Vec<(String, String)> {
    guard_core::test_attr_chunks(test_src)
        .iter()
        .filter_map(|c| guard_fn_item(c))
        .filter(|(_, item)| declares_a_guard_table(item))
        .collect()
}

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// 抠出所有 `#[cfg(test)]` 段（到下一个顶层 `}` 为止）。
/// 🔴 〔搬树 2026-09-18 · `设计/16 §5.4b` 纪律 3〕**住 `tests/` 的文件整份就是测试段。**
///
/// [`test_regions`] 与 `guard_core::test_source` 都靠 `#[cfg(test)]` 这个**标记**
/// 切出测试段 —— 那个标记是「生产段与测试段同住一份文件」那个年代的产物。
/// 剖分之后 `<repo>/tests/` 那棵树**整棵不进 `cargo build`**，里面的文件
/// 一个 `#[cfg(test)]` 都不需要写 ⇒ 两个切法对它们一律交回**空串**，
/// 而空串会让本模块两层判据（文件级 · 判据级）对那棵树**恒真地绿**
/// —— 本模块从头到尾治的正是「没红与没看在输出上一模一样」这个形状。
fn test_side_of(rel: &str, src: &str) -> String {
    if rel.starts_with("tests/") {
        return src.to_string();
    }
    test_regions(src)
}

fn test_regions(src: &str) -> String {
    let mut out = String::new();
    let mut i = 0usize;
    while let Some(j) = src[i..].find("#[cfg(test)]") {
        let at = i + j;
        let end = src[at..].find("\n}\n").map_or(src.len(), |e| at + e);
        out.push_str(&src[at..end]);
        out.push('\n');
        i = at + "#[cfg(test)]".len();
    }
    out
}

/// # 〔audit-0805 08-06〕横扫结论：「多条判据一起瞎」这一族**全仓已清**
///
/// 起因是两次实测：`no_timer_guard` 的两道针被同一种改写一次穿两层；
/// `inbound` 的两条判据开头是**同一句** `if spec.fields.is_empty() { continue; }`
/// —— 一个声明就能同时关掉两条。由此命名了这个形态并横扫全仓，两轮口径：
///
/// ① **判据之间共用同一个提前返回**（文本相同的 `if … { continue/return }`）：
///    全仓 4 种，其中三种是各自独立的文件过滤 / 自排除（`rel == SOLE_HOME` 在两个
///    单例守卫里各指各的常量），**真正的共用开关只有 `spec.fields.is_empty()` 那一处**，
///    已由 `inbound::declaring_zero_fields_needs_a_reason` 补上。
/// ② **多条判据共用同一个采集器**（它一坏就集体失明）：逐个核过
///    `rust_files` / `collect_ts` / `doc_files` / `daemon_sources` / `layer_sources` /
///    `scan_files` / `platform_cfgs` / `backend_files` / `daemon_control_production` …
///    —— **每一个都有自检**（地板、或与登记表的数量相等对拍）。**零发现。**
///
/// ⚠ 为什么不把这条横扫做成判据：我写的两版探测器**都不可靠** ——
/// 第一版按单行匹配 `if … { continue; }`，而 rustfmt 把它拆成两行 ⇒ 全仓零命中；
/// 第二版按「调用点后 400 字符内有 assert 地板」判自检，把 `collect_ts` / `layer_sources` /
/// `backend_files` 三个**有自检**的误报成没有（它们的自检写在变量上、或是等数对拍）。
/// ⇒ 依它建判据 = 把一个测不准的量具钉进门禁。**登记为已核事实，不做成机检。**
///
/// 「扫描面 + 登记表」型判据的**反向那半**必须在（〔audit-0805 08-06〕裁决件产出）。
///
/// # 它钉的是一个被实测证明**今天成立**的前提，不是一个缺陷
///
/// 本轮怀疑过两件事，量下去**两件都不成立**：
///
/// 1. **「自检重建了扫描面副本」是不是缺陷** —— 不是。`polling_registry` 与
///    `session_name_registry` 的自检确实各自又走了一遍遍历器，但
///    ① 把 `scan()` 的根整个打瞎 ⇒ 棘轮的**反向那半**当场红
///    （逐字「登记表里的 `src/session-accounts-poll.ts` 已经没有周期唤醒了」）；
///    ② 局部缩水（静默跳过一个子目录）⇒ 自检的地板红（`118 < 170`），
///    因为自检与 `scan()` **共用同一个 walker 函数**，函数体坏了两边一起坏。
/// 2. **是不是有登记表只做单向对拍** —— 没有。六个登记表逐个变异验过，
///    模拟「某个调用点退役了」都会红（`atomic_replace_registry` 逐字
///    「登记表里的 `config.rs [MoveFileExW]` 已经不在了 —— 删掉这条」）。
///
/// ⇒ 于是**第 1 条的安全性整个压在第 2 条上**：反向那半一旦被削成单向，
/// 「打瞎扫描面」就再没有人接住，而自检那份副本**照样绿**。
/// 这正是本区反复记的形状：**一条纪律的成立依赖另一条，而那条依赖没人盯。**
///
/// # 它查得动什么、查不动什么
///
/// 查的是**存在性**：每个带登记表的判据文件里，必须至少有一种反向那半的形态
/// （`assert_eq!` 双向对拍，或显式的「登记了但实测没有」诊断）。
/// ⚠ **查不动「反向那半是否真的在被执行」** —— 有人可以留着字样却把逻辑绕开。
/// 这是**前提触发器**，不是证明：它挡的是「顺手删掉反向那半」，
/// 那是实际会发生的动作（改判据时嫌它啰嗦），而不是蓄意伪装。
#[test]
fn every_registry_guard_keeps_its_reverse_half() {
    let root = repo_root();
    let mut population: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    // 🔴 `K-R36`：**判据级**那一层。与文件级那层**各扫各的** —— 它刻意不挂在
    // `declares_a_guard_table(&regs)` 那个 `continue` 后面：挂上去的话，
    // 文件级采不到就把判据级一起带瞎，而两层同时瞎与两层都过**输出完全相同**。
    let mut per_guard: Vec<String> = Vec::new();
    let mut missing_guards: Vec<String> = Vec::new();
    // 🔴 `K-R37` 09-06：**逐子树循环**。上一版这里的实参逐字只有
    // `&root.join("src/bridge/src")` 一棵树 ⇒ `src/backend` 那 77 份 `.rs`
    // 本条**一份也没打开过**，而它的失败文案**只点它采到的文件、没采到的一个字都不提**
    // ⇒ 「没红」与「没看」在输出上一模一样（本模块从头到尾在治的正是这个形状）。
    // ★ 形状不是发明的：本模块 [`raw_walkers`] 从一开始就是这么写的（逐字同一份清单）。
    let mut scanned: Vec<(&str, usize)> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    // 🔴 〔搬树 2026-09-18 补 `"tests"`〕19 份纯测试文件从后端树搬到了
    // `<repo>/tests/backend/` ⇒ 原来那两棵树**一份也够不着它们**，
    // 而本模块治的正是「没红与没看在输出上一模一样」那个形状。
    for sub in ["src/bridge/src", "src/backend", "tests"] {
        let files = guard_core::scan_tree!(&root.join(sub), &["rs"]);
        scanned.push((sub, files.len()));
        for (f, src) in files {
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            seen.push(rel.clone());
            let test_side = test_side_of(&rel, &src);
            for (name, item) in guards_declaring_a_table(&test_side) {
                per_guard.push(format!("{rel}::{name}"));
                if !REVERSE.iter().any(|m| item.contains(m)) {
                    missing_guards.push(format!("{rel}::{name}"));
                }
            }
            let regs = test_side;
            if !declares_a_guard_table(&regs) {
                continue;
            }
            population.push(rel.clone());
            if !REVERSE.iter().any(|m| regs.contains(m)) {
                missing.push(rel);
            }
        }
    }
    // 抽取器自检⑤（`K-R37`）：**采集量地板，逐子树一个** —— 不是只看总数。
    //
    // 🔴 它买的是 `D2` 的 acceptor 逐字点名的那一格：「把新树加进实参而**实际上采不到
    // 东西**」（路径拼错 / 后缀过滤掉了）。为什么必须**逐子树**：daemon 那棵今天对
    // `population` 的贡献是 **0**（闭集里那几个名字在那棵树上一处都没有，`D1` 现打），
    // ⇒ 把它的实参改坏，下面那些断言**一条都不会红**，而总数地板也顶得过去
    //（`src/bridge/src` 那 100 多份自己就够）。**一个只看总数的地板在这里等于没有。**
    //
    // ⚠ 诚实边界：路径**不存在**那一形其实不靠本格 —— `scan_tree_excluding_self`
    // 自己会 `panic!("读目录 … 失败")`。本格接的是**存在、但采不到东西**那一形
    //（后缀写错 · 指到一个几乎空的子目录）。两形各有各的接手人，别把本格读大。
    // 🔴 **地板按子树各给一个**，不是一个数管三棵。
    // 〔2026-09-18〕`"tests"` 是新加的那棵，而它只有 **19 份 `.rs`**
    // （另外 150 份是 `.ts`，不在本条的后缀里）⇒ 一个 40 的通用地板会**假红**，
    // 而假红正是本仓记过账的那件事：「假阳会训练人绕过判据」。
    // 现打：`src/bridge/src` 111 · `src/backend` 72 · `tests` 19。
    let floor_of = |sub: &str| -> usize {
        match sub {
            "tests" => 15,
            _ => 40,
        }
    };
    let starved: Vec<String> = scanned
        .iter()
        .filter(|(sub, n)| *n < floor_of(sub))
        .map(|(sub, n)| format!("  {sub} —— 只采到 {n} 份（地板 {}）", floor_of(sub)))
        .collect();
    assert!(
        starved.is_empty(),
        "这几棵子树的采集量低于它自己那条地板：\n{}\n\
             ⇒ 那个实参此刻**几乎什么都没采到**，而本条对它「全绿」——\n\
             那正是「没红」与「没看」在输出上一模一样的那一格。\n\
             ⇒ 先核实参（路径拼对了吗 · 后缀过滤对吗），别调地板让今天好过。\n\
             （本趟逐子树的采集量：{scanned:?}）",
        starved.join("\n")
    );
    // 抽取器自检⑥（`K-R37`，**点名**）：**射程本身**也要被钉住。
    //
    // 没有它，把实参改回一棵树是**静默**的：daemon 那棵今天对人群的贡献是 0
    // ⇒ 删掉它，上面那个地板（只看还剩的那几棵）与下面所有断言**全部照旧绿**，
    // 而输出与今天一模一样。那正是本件立件的那一格，只是方向反过来。
    //
    // 🔴 **为什么钉的是一个真实住址，而不是把上面那份子树清单再抄一遍**：
    // 抄一份的话，「清单少一棵」与「钉子少一条」会被同一次编辑一起改掉 ⇒ 恒真。
    // 形状照上面的 [`MUST_BE_RECOGNISED`]：拿**盘上真有的那一份**当见证。
    const MUST_BE_IN_REACH: &[(&str, &str)] = &[(
        "src/backend/wire.rs",
        "daemon 那棵树的见证 —— `K-R37` 之前本条的实参逐字只有 `src/bridge/src`，\
             那棵树的 `.rs` 一份也没被打开过",
    )];
    let out_of_reach: Vec<String> = MUST_BE_IN_REACH
        .iter()
        .filter(|(p, _)| !seen.iter().any(|q| q == p))
        .map(|(p, why)| format!("  {p} —— {why}"))
        .collect();
    assert!(
        out_of_reach.is_empty(),
        "这几份**应当**在本条的射程里，而本趟一份都没扫到：\n{}\n\n\
             本趟逐子树的采集量：{scanned:?}\n\n\
             🔴 别把这条读成「那份文件没了」—— 它红的多半是**射程被改窄了**：\n\
             上面那个 `for sub in [...]` 少了一棵树，或者那棵树的实参指到了别处。\n\
             ★ 射程改窄在本条上是**静默**的：被删掉那棵树对人群的贡献可以是 0，\n\
             于是下面每一条断言都照旧绿，输出与改窄之前逐字相同 ——\n\
             「没红」与「没看」在输出上一模一样，那正是 `K-R37` 立件的那一格。\n\
             ⇒ 处置：把那棵树加回实参；真要缩射程，先回答「那边从此谁看」。",
        out_of_reach.join("\n")
    );
    // 🔴 `K-R33`：**把采到了谁印出来。**
    //
    // 在此之前，本条对一份「它压根没扫到」的文件与一份「它采到了、而且过了」的文件
    // **输出完全相同**（都是静默的绿）。`K-R31` 新加的那条判据整整一轮落在前一格里
    // 而没有任何人看得出来 —— 那不是漏了一条，是这条判据**说不出自己看了谁**。
    // 只在 `--nocapture` 下可见；判红时那几条断言的文案里另有一份。
    // ⚠ 标签刻意**不写判据的函数名** —— 抄一份名字进字符串，改名那天它就是一句假话
    //（本模块头注里的 `parity_ledger` 就是这个形态）。判据名 cargo 自己会打在上一行。
    // 🔴 `K-R37`：**射程也是一个读数**。人群数说不出「我看了哪几棵树、各几份」——
    // 而本件治的正好是那一格：一棵树整个没被打开，人群数与「那边全合规」同值。
    // ⇒ 每趟**现算**并印出来（`brief` 13b：别把清单/基数写死在散文里）。
    eprintln!(
        "〔登记表型判据 · 本趟的射程〕{} 棵子树 · 逐棵采集量 {scanned:?}",
        scanned.len()
    );
    eprintln!(
        "〔登记表型判据 · 本趟采到的人群〕{} 个：\n  {}",
        population.len(),
        population.join("\n  ")
    );
    // 抽取器自检①：人群不能空 —— 空了下面那条会零命中地绿。
    assert!(
        population.len() >= 5,
        "只认出 {} 个带登记表的判据文件（08-06 实测 6 · 09-06 实测 9）—— 抽取器坏了，本条此刻是空转的：{population:?}",
        population.len()
    );
    // 抽取器自检③（`K-R33`，**点名**）：闭集是**按名字**认的，而名字是会被改的。
    // 改一个名字 ⇒ 那一形当场退回「没被扫到」，而「没被扫到」与「过了」在上面那条
    // 断言上**输出完全相同**（都不红）。⇒ 拿真实住址把至少一形钉住，让它改名即红。
    const MUST_BE_RECOGNISED: &[(&str, &str)] = &[(
        // 〔搬树 2026-09-18 · `设计/16 §6.2` C 类〕住址跟着判据搬：`K-R31` 那条判据
        // 与它那张 `FORMS` 一起从 `src/backend/control/local_backend.rs` 搬到了
        // `tests/bridge/backend/control/local_backend_tests.rs`（现打：全树 `const FORMS:`
        // 仍然**恰好一处**，就是它）。**表名与判据名一个字都没改。**
        "tests/bridge/backend/control/local_backend_tests.rs",
        "`K-R31` 的 `nothing_in_the_production_path_runs_code_between_fork_and_exec`，\
             它的表叫 `FORMS`",
    )];
    let unseen: Vec<String> = MUST_BE_RECOGNISED
        .iter()
        .filter(|(p, _)| !population.iter().any(|q| q.ends_with(p)))
        .map(|(p, why)| format!("  {p} —— {why}"))
        .collect();
    assert!(
        unseen.is_empty(),
        "这几份**应当**被采进人群，而本趟一个候选都没采到它们：\n{}\n\n\
             本趟采到的是这 {} 个：\n  {}\n\n\
             🔴 别把这条读成「那份文件坏了」—— 它红的是**本条自己瞎了**：\n\
             上面那个 `TABLE_DECLS` 是**按名字**认表的闭集，被点名的那份文件把表改了个名字\n\
             （或换了写法）⇒ 它从此掉出人群，而掉出去之后本条对它**恒真地绿**。\n\
             ★ `K-R33` 立件的正是这一格：`K-R31` 那条判据的表叫 `FORMS`，闭集里没有这个名字，\n\
             于是它整整一轮**没被判到**，而输出与「判到了并且过了」一模一样。\n\
             ⇒ 处置：把新表名加进 `TABLE_DECLS`（那是**放宽**，先读模块头注 `K-R33` 那一节），\n\
                或者把那张表改回闭集里的名字。",
        unseen.join("\n"),
        population.len(),
        population.join("\n  ")
    );
    // 抽取器自检②（负向）：**不带登记表的文件不许进人群**，
    // 否则「人群够大」这个自检可以靠把整棵树算进来而恒真。
    assert!(
        !population.iter().any(|p| p.ends_with("src/tmux.rs")),
        "`tmux.rs` 没有登记表却被算进人群 —— 判别式太松，人群数就不再说明任何事"
    );
    assert!(
        missing.is_empty(),
        "这些登记表型判据**没有反向那半**（登记了但实测已经没有 ⇒ 也该红）：\n  {}\n\
             ★ 单向对拍只挡「多一处」，挡不住「登记表腐烂」——\n\
             而本仓另有一条纪律**整个压在它上面**：扫描面被打瞎时，\n\
             接住的正是反向那半（自检那份副本照样绿，实测过）。\n\
             ⇒ 删它之前先想清楚谁来接「扫描面悄悄不扫了」这件事。",
        missing.join("\n  ")
    );

    // ── 🔴 `K-R36`：判据级那一层（上面那几条判的单位是**文件**）───────────────
    //
    // 只在 `--nocapture` 下可见。标签刻意**不写判据的函数名**，理由同上面那一处。
    eprintln!(
        "〔登记表判据 · 判据级射程〕{} 条（**表声明在判据体内**的那一档；\
             模块级前言里的表仍在文件级那一档，见模块头注 `K-R36` 那一节）：\n  {}",
        per_guard.len(),
        per_guard.join("\n  ")
    );
    // 抽取器自检④（`K-R36`，**点名**）：切法一坏，这一层**采到 0 条**，
    // 而 0 条时下面那两条断言**恒真地绿** —— 与 `K-R33` 那一格同形：
    // 「没扫到你」与「判过你了」在输出上一模一样。⇒ 拿真实住址把**本件的题眼**钉住。
    const MUST_BE_JUDGED_PER_GUARD: &[(&str, &str)] = &[(
        // 〔搬树 2026-09-18 · `16 §6.2` C 类〕住址跟着判据搬（表名与判据名没改）。
        "tests/bridge/backend/control/local_backend_tests.rs::nothing_in_the_production_path_runs_code_between_fork_and_exec",
        "`K-R36` 的题眼：它的表叫 `FORMS`、声明在它自己体内，\
             而同一份文件的测试段里另有几十处 `assert_eq!(` ——\
             文件级那一层对它恒真，判据级这一层才判得到它",
    )];
    let unjudged: Vec<String> = MUST_BE_JUDGED_PER_GUARD
        .iter()
        .filter(|(p, _)| !per_guard.iter().any(|q| q.ends_with(p)))
        .map(|(p, why)| format!("  {p} —— {why}"))
        .collect();
    assert!(
        unjudged.is_empty(),
        "这几条**应当**被判据级那一层判到，而本趟一条都没采到：\n{}\n\n\
             本趟判据级采到的是这 {} 条：\n  {}\n\n\
             🔴 别把这条读成「那条判据坏了」—— 它红的是**判据级那一层自己瞎了**：\n\
             切法（`guard_fn_item` ＋ `guard_core::test_attr_chunks`）一坏，这一层采到 0 条，\n\
             而 0 条时下面那两条断言恒真地绿。\n\
             ⇒ 先看被点名那条判据是不是把表**挪出了函数体** —— 挪出去就掉回文件级那一档，\n\
                模块头注 `K-R36` 那一节逐字写着这两档的分界。",
        unjudged.join("\n"),
        per_guard.len(),
        per_guard.join("\n  ")
    );

    // `K-R36` `D2③` 在**甲（棘轮）**与**乙（逐条豁免表）**里选了**乙**，
    // 两条路各自的代价与选它的理由写在模块头注 `K-R36` 那一节，这里不写第二遍。
    /// 今天**确实缺**反向那半、而本件**不去补**的那几条（`D2②`：补是别人的活）。
    ///
    /// 三列：**住址**（`路径::判据名`）· 为什么今天不补 · **解锁条件**。
    /// 起名 `REGISTERED` 是照本模块头注那条纪律（新写的「扫描面 ＋ 常量表」型判据，
    /// 表要起成 `TABLE_DECLS` 里已有的名字之一）。
    /// ⚠ **诚实边界**：本文件被 `scan_tree!` 按构造摘除 ⇒ 这条元判据**看不见自己这张表**，
    /// 起对名字在这里买到的只是**纪律的一致性**，不是「它真被判到了」。
    /// 真正接住这张表腐烂的是紧跟着的那条**幽灵检查**。
    const REGISTERED: &[(&str, &str, &str)] = &[(
        // 〔搬树 2026-09-18〕住址订正：那条判据住 `tests/bridge/`，不在 `src/bridge/src/`。
        "tests/bridge/structural_scan_tests.rs::comment_stripping_has_exactly_one_shared_implementation",
        "它自带扫描面（`scan_tree!`）与登记表，但只断了「扫到的里有没有没登记的」这一向；\
             「登记了却已经不在」那一向没人接 —— 而那正是本条要治的族。\
             本件按 `D2②` 只让它红出来，不代补",
        "给它补上双向对拍（`assert_eq!(found, want)`），\
             或一条「登记表里的 X 已经不在了」的诊断；补完把这一行删掉 —— \
             不删的话下面那条幽灵检查会逼你删",
    )];

    // 这张豁免表自己那半**反向**：登记了却**已经不在了**的豁免，必须红。
    // 没有它，豁免表就是一张只会长草的免检名单（`D2③乙` 逐字写着的那个代价）。
    let stale: Vec<String> = REGISTERED
        .iter()
        .filter(|(addr, ..)| !missing_guards.iter().any(|m| m.as_str() == *addr))
        .map(|(addr, ..)| format!("  {addr}"))
        .collect();
    assert!(
        stale.is_empty(),
        "豁免表里这几条**已经不在了** —— 它们要么补上了反向那半，要么改名/被删了：\n{}\n\
             ⇒ 把这几行从上面那张 `REGISTERED` 里删掉。留着就是把这条守卫的余量白送出去：\n\
             下一条同名的判据一进来，就自动带着一张谁也没签过的免检章。\n\
             （本趟判据级实缺的是这 {} 条：{missing_guards:?}）",
        stale.join("\n"),
        missing_guards.len()
    );

    let unexcused: Vec<String> = missing_guards
        .iter()
        .filter(|m| !REGISTERED.iter().any(|(addr, ..)| *addr == m.as_str()))
        .map(|m| format!("  {m}"))
        .collect();
    assert!(
        unexcused.is_empty(),
        "这几条判据**自带登记表、却没有自己那半反向**：\n{}\n\
             ★ 单位是**这一条判据**，不是这份文件 —— 同一份文件里别的判据有多少\n\
             `assert_eq!(` 都接不住它。`K-R36` 立件的正是这一格：在一份 28 处断言的文件上，\n\
             「每条判据」与「这份文件」相差 27 条判据。\n\
             ⇒ 处置**二选一**：\n\
               ① 给它补上反向那半 —— 双向对拍（`assert_eq!`），\n\
                  或一条「登记表里的 X 已经不在了」式的诊断；\n\
               ② 真有理由今天不补 ⇒ 写进上面那张 `REGISTERED`，**三列都要填**\n\
                  （住址 · 为什么不补 · 解锁条件），幽灵检查会盯着它别长草。\n\
             🔴 **没有第三条路**：往 `REVERSE` 里加一个成员把它变绿 ——\n\
                那是把一条本来就近乎空真的守卫弄得更空（`K-R36` `D2①` 明禁）。",
        unexcused.join("\n")
    );
}

/// ★ `K-R33` 的**反向那半**：识别器不许「什么表都算」。
///
/// # 没有它，把上面那条判据关掉只需要一次「放宽」
///
/// [`declares_a_guard_table`] 的口径一旦宽到「测试段里有个 `const … : &[…]` 就算」，
/// 人群就会**恒真地**吃下几乎每一份判据文件；而 `REVERSE` 的第一形是 `assert_eq!(`，
/// 几乎每一份测试段里都有 ⇒ `missing` 恒空 ⇒ 上面那条判据变成一场仪式，
/// **而它的输出与今天一模一样（绿）**。
/// ⇒ 放宽口径必须同时买一条「**这些不许被采**」，否则买到的只是一个更大的空转。
///
/// ⚠ **诚实边界（别把这条读大）**：今天的口径是**按名字的闭集**，
/// 按构造就不会过采 ⇒ 下面那几格**此刻是廉价的**，它们不是在证明今天的口径准。
/// 它们承的是**将来**那一拍：谁把闭集换成一族形状，这几格当场红。
#[test]
fn the_registry_table_recogniser_does_not_say_yes_to_every_const_slice() {
    // 正：闭集里的每一个名字都要真的被认出来 ——
    // 识别器与闭集脱钩（比如把名字写死进函数体）时，这一格逮它。
    for d in TABLE_DECLS {
        let synthetic = format!("    {d} &[&str] = &[\"x\"];");
        assert!(
            declares_a_guard_table(&synthetic),
            "闭集里写着 `{d}`，识别器却认不出这一形 —— 识别器与 `TABLE_DECLS` 脱钩了：{synthetic}"
        );
    }
    // 负：needle 表 / skip 表 / 扫描面表**都不是登记表**，不许被采进人群。
    // 三种都是本仓真实存在的形态 —— `K-R33` `D1②` 09-06 逐条判过（分母与逐条判词住件文件，
    // 量具 `tests/evidence/K-R33-table-decl-census.py`）：那 138 条里三者合计比登记表还多。
    const NOT_A_REGISTRY_TABLE: &[(&str, &str)] = &[
        (
            "    const NEEDLES: &[&str] = &[\"read_dir(\", \"WalkDir\"];",
            "needle 表 —— 拿去在文本里搜的串，它没有「登记了却已经不在了」这一半",
        ),
        (
            "    const EXEMPT: &[(&str, &str)] = &[(\"a.rs\", \"为什么豁免\")];",
            "skip 表 —— 豁免清单，它腐的方式与登记表不同（该由各自的幽灵检查治）",
        ),
        (
            "    const EXTS: &[&str] = &[\"rs\", \"ts\"];",
            "扫描面表 —— 说的是「去哪儿找」，不是「找到了谁」",
        ),
        (
            "    let sites = collect_sites();",
            "压根不是常量声明（口径一旦按「出现 sites 字样」认，这一行就会被采）",
        ),
    ];
    for (synthetic, what) in NOT_A_REGISTRY_TABLE {
        assert!(
            !declares_a_guard_table(synthetic),
            "{what}\n  —— 它被当成登记表采进来了。\n\
                 🔴 口径宽到「什么表都算」= **关掉**上面那条判据，而不是扩大它的覆盖：\n\
                 人群恒真地满，而 `REVERSE` 里的 `assert_eq!(` 几乎人人都有 ⇒ `missing` 恒空。\n\
                 逐字：{synthetic}"
        );
    }
}

/// ★ `K-R36` 的**反向那半**：判据级那一层不许**恒真地**采到反向那半。
///
/// # 没有它，把「按判据判」悄悄写回「按文件判」看不出来
///
/// [`guards_declaring_a_table`] 只要有一处把**整份文件**（或整块）的文本交给每一条判据，
/// 「逐条判」当场退回「按文件判」，而**真树上的输出与今天一模一样（绿）** ——
/// 那正是 `K-R36` 立件的那一格，也是 `K-R33` 那一格的同形：
/// **「判过了」与「压根没判到」在输出上不可区分。**
/// ⇒ 拿一份合成文本正反各喂一遍：前一条自带反向那半、后一条**确实没有**，
/// 断言**红且只红后一条**（`D2` 的 acceptor 逐字要的就是这一格）。
///
/// ⚠ **夹具名取中性名，且断言不取自夹具的名字**（`brief` 12 的 `6g`）：
/// 两个判据名在这里是**变量** —— 喂进去的与断出来的是同一个值，
/// 改夹具名不会让这一格恒真，也不会让它假红。
#[test]
fn the_per_guard_split_does_not_hand_every_guard_the_whole_file() {
    // 🔴 锚点**运行时拼**，而且夹具每一行都缩进 —— 两条都是承重的：
    //   ① 写成字面量的话，这份夹具会变成**本文件源码里一个真的 `#[test]` 边界**
    //      （`guard_core::test_attr_chunks` 按「整行 trim 之后逐字等于那条属性」认）；
    //   ② 顶格的 `}` 会被 `guard_core` 的 `test_module_ranges` 当成本文件测试段的收尾，
    //      把本文件自己的测试段**提前截断**，于是剥法把后半段当生产代码。
    // ★ 本模块头注治的正是「判据在自己的源码里找到了自己」这一族 —— 这里不许复发。
    let attr = concat!("#[te", "st]");
    let (kept, lost) = ("keeps_its_reverse_half", "lost_its_reverse_half");
    let synthetic = [
        "    mod fixture {".to_string(),
        "        const SITES: &[&str] = &[\"住在前言里，不算任何一条判据自带\"];".to_string(),
        format!("        {attr}"),
        format!("        fn {kept}() {{"),
        "            const REGISTERED: &[&str] = &[\"甲\"];".to_string(),
        "            assert_eq!(REGISTERED.len(), 1, \"双向对拍\");".to_string(),
        "        }".to_string(),
        format!("        {attr}"),
        format!("        fn {lost}() {{"),
        "            const REGISTERED: &[&str] = &[\"乙\"];".to_string(),
        // ⚠ 阴性那一条**刻意一个断言都不写**〔`R36M3` 自查逮到，09-06〕：
        // 第一版写的是 `assert!(!REGISTERED.is_empty(), …)`，于是**往 `REVERSE` 里加
        // `assert!(` 这个成员会把本格弄红** —— 那等于本条顺手把一个不归它管的闭集钉死了
        // （`R36M3` 逐字：那是 `KRF2` 那一族的形状，不是本件射程）。
        // 阴性对照要的只是「这条判据没有反向那半」，写成**不含任何断言**最不易被牵连。
        "            let _only_the_forward_half = REGISTERED.len();".to_string(),
        "        }".to_string(),
        "    }".to_string(),
    ]
    .join("\n");

    let judged = guards_declaring_a_table(&synthetic);
    let names: Vec<&str> = judged.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        vec![kept, lost],
        "判据级的切法采错了人群 —— 期望**恰好这两条**：\n\
             前言里那张 `SITES` 不属于任何一条判据（它是模块级的，归文件级那一档），\n\
             把它算进来就等于又退回「按文件判」。\n\
             合成夹具逐字：\n{synthetic}"
    );

    let no_reverse: Vec<&str> = judged
        .iter()
        .filter(|(_, item)| !REVERSE.iter().any(|m| item.contains(m)))
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(
        no_reverse,
        vec![lost],
        "**红且只红它**这一格没买到。两个方向各说明一次：\n\
             · **一条都没点**（后一条也算「有反向那半」）⇒ 口径把**整份文本喂给了每一条**：\n\
               前一条的 `assert_eq!(` 漏进了后一条 ⇒ 「逐条判」退回「按文件判」，\n\
               而那时真树上的输出与今天**一模一样（绿）**。这是本条存在的全部理由。\n\
             · **多点了名**（把前一条也算缺）⇒ 切块把某一条自己那半反向切丢了。\n\
             合成夹具逐字：\n{synthetic}"
    );
}

/// 今天仍在裸遍历的文件（相对仓根）。
fn raw_walkers() -> Vec<String> {
    let root = repo_root();
    let mut out = Vec::new();
    // 🔴 〔搬树 2026-09-18 补 `"tests"`〕19 份纯测试文件从后端树搬到了
    // `<repo>/tests/backend/` ⇒ 原来那两棵树**一份也够不着它们**，
    // 而本模块治的正是「没红与没看在输出上一模一样」那个形状。
    for (sub, excluded) in [
        ("src/bridge/src", &[] as &[&str]),
        ("src/backend", &[]),
        ("tests", &["bridge/scanning_guard_registry_tests.rs"]),
    ] {
        // 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §5.4b` 纪律 4〕
        //    **摘除从 `file!()` 改成明写名单，而且它现在是真承重的。**
        //
        // 上一版的注释说「摘除今天不是承重的，真正让本文件不被标记的是
        // `!regs.contains("scan_tree!")`」—— **那句话已经过期了**：F23 第二刀
        // 就把 `!regs.contains("scan_tree!")` 那半删掉了（理由写在下面）。
        // 于是今天**只剩摘除在挡**，而 `file!()` 那条路在剖分之后是空转的
        // （本文件被 `#[path]` 引进来 ⇒ `file!()` 是带 `..` 的折返路径 ⇒ 后缀比恒不命中）。
        // 现打后果：本文件的测试段里写着 `RAW_WALKS` 那四个字面量当语料，
        // 于是它**把自己算进了裸遍历人群**（`newcomers` 里多出一条
        // `tests/bridge/scanning_guard_registry_tests.rs`）。
        // ⇒ 排除明写成名单，`scan_tree_excluding` 摘不到就 panic ——
        //   本文件改名/搬家会当场出声，而不是安静地把自己收进语料。
        for (f, src) in guard_core::scan_tree_excluding(&root.join(sub), &["rs"], excluded) {
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            let regs = test_side_of(&rel, &src);
            // ★ F23 第二刀：**去掉了 `&& !regs.contains("scan_tree!")` 那半**。
            //
            // 它是**整份文件级的豁免**：只要测试段里出现过一次 `scan_tree!`，
            // 这个文件里**再多裸遍历也不会被标记**。豁免的粒度是「文件」，
            // 而事实的粒度是「那一处遍历」—— 又一次**匹配单位与事实不同级**
            // （F24 那一族的反面：这次是单位比事实**大**）。
            //
            // 变异实测：给 `byte_cap_registry`（它用 `scan_tree!`）的测试段加一处裸
            // `read_dir`，**本条照样绿**。去掉那半之后当场红。
            // ⚠ 先证明它恒绿再删（E11）：去掉后**一个文件都没被新标记** ——
            // 说明今天没有「既用 `scan_tree!` 又裸遍历」的文件，那半是纯死重。
            // 而 `scan_tree!` 的调用文本里本来就不含 `RAW_WALKS` 的四个字面量，
            // 所以只用 `scan_tree!` 的文件本来也不会被标记 —— 那半从来没起过作用。
            if RAW_WALKS.iter().any(|w| regs.contains(w)) {
                out.push(
                    f.strip_prefix(&root)
                        .unwrap_or(&f)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    out.sort();
    out
}

/// ★ 正题：**测试段里不许新增裸遍历**。
#[test]
fn no_new_guard_walks_the_tree_without_excluding_itself() {
    let found = raw_walkers();
    // 抽取器自检：扫不到时下面的对拍会两边都空、静默变绿。
    assert!(
        found.len() >= 20,
        "只扫到 {} 个裸遍历文件（08-06 实测 31）—— 抽取器坏了",
        found.len()
    );
    let newcomers: Vec<&String> = found
        .iter()
        .filter(|f| !PENDING.contains(&f.as_str()))
        .collect();
    assert!(
        newcomers.is_empty(),
        "有扫描型判据在测试段里**裸遍历目录**，且不在存量清单里：\n{}\n\n\
             ⇒ 改走 `guard_core::scan_tree!(&root, &[\"rs\"])` —— 它按构造摘除调用者自己那份。\n\
             ★ 为什么非要这条：判据在自己的登记表/注释/常量里找到自己 ⇒ **恒绿**，\n\
             audit-0805 实测五次，**五次都不是被判据变红发现的**（四次靠变异、一次靠 clippy）。\n\
             「以后小心点」对这一族无效，所以修法是**让它写不出来**。",
        newcomers
            .iter()
            .map(|s| format!("  {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// ★ **递减棘轮**：存量只许降。
#[test]
fn the_pending_inventory_only_shrinks() {
    let n = PENDING.iter().filter(|s| !s.is_empty()).count();
    assert!(
        n <= PENDING_CEILING,
        "存量清单涨到 {n}（上限 {PENDING_CEILING}）—— **只许降**。\
             迁一个就删一行并把上限调下来；**不许把上限调上去让今天好过**。"
    );
    // 清单不许长草：登记的文件必须真的还在裸遍历。
    let found = raw_walkers();
    let stale: Vec<&&str> = PENDING
        .iter()
        .filter(|p| !found.iter().any(|f| f == *p))
        .collect();
    assert!(
        stale.is_empty(),
        "存量清单里这些已经不裸遍历了（迁完了或文件没了）：{stale:?}\n\
             ⇒ 删掉它们并把 `PENDING_CEILING` 一起调下来 —— 留着就是把棘轮的余量白送出去。"
    );
}

// ── 🔴 `K-R38` 09-06：给那个「只许降」的棘轮装闸 ─────────────────────────
//
// 上面那条断的是 `n <= PENDING_CEILING`，而 `PENDING_CEILING` 就住在这份文件里
// ⇒ **抬上限只会让它更容易过**。选路（乙 · 对着 git 历史面比）、它的代价、
// 以及它**没有**买到什么，全写在模块头注 `K-R38` 那一节，这里不写第二遍。

/// 本文件在仓里的相对住址 —— 下面要拿它去问 git 历史。
///
/// 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §6.2` C 类〕**从 `src/bridge/src/…` 换到
/// `tests/bridge/…`**：`PENDING` 与 `PENDING_CEILING` 这一轮跟着测试段搬过来了，
/// 而这两个解析器要跑在**住着那两个常量的那份文件**的历史版本上。
/// 没跟着改的后果现打过：两个解析器在「本文件此刻的源码」上都回 `None`
/// ⇒ 对拍自检当场红（`left: (None, None)` / `right: (Some(28), Some(28))`）——
/// 红得对，而它红的正是「住址馊了」这一格（上面第 ③ 条来路逐字写着）。
const SELF_REL: &str = "tests/bridge/scanning_guard_registry_tests.rs";

/// 那两个常量在 git 历史上住过的**全部**住址，`(相对路径, 要不要 --follow)`。
///
/// 🔴 为什么是一张表而不是一个字符串：**剖分不是改名**，`--follow` 跨不过去。
/// 逐条写明每一个住址买到哪一段历史（缺一段 = 历史面变短 = 棘轮变松）：
/// · `tests/bridge/scanning_guard_registry_tests.rs` —— 步 7c 剖分**之后**的提交。
///   不带 `--follow`：它是新增文件，没有可跟的改名链。
/// · `src/bridge/src/scanning_guard_registry.rs` —— 剖分**之前**的整条历史。
///   带 `--follow`：它自己还跨着 2026-09-17 那次搬树（`src-tauri/src/…` → `src/bridge/src/…`）。
const SELF_HOMES: &[(&str, bool)] = &[
    (SELF_REL, false),
    ("src/bridge/src/scanning_guard_registry.rs", true),
];

/// 找 `PENDING_CEILING` 那行声明的针 —— 🔴 **运行时拼，别写成字面量**。
///
/// 承重，理由是本模块头注治的那一族：下面两个解析器要跑在**本文件自己的历史版本**上，
/// 而针一旦写成字面量，每一份历史 blob 里它就有**两处**（真声明 ＋ 这行字面量），
/// 于是「解析到的是哪一处」由两者在文件里的先后决定 —— 一次挪动就能让它悄悄解析错，
/// **而错的方向是静默的绿**。★「判据在自己的常量里找到了自己」在这里不许复发。
/// ★ 拼法照本文件已有的那一处（`concat!("#[te", "st]")`）—— 同一个理由，别改回字面量。
fn ceiling_needle() -> &'static str {
    concat!("const PENDING_", "CEILING", ": usize = ")
}

/// 找 `PENDING` 那张表表头的针 —— 同上，**拼出来的，不写字面量**。
/// ⚠ 必须带冒号：`const PENDING_CEILING` 也以 `const PENDING` 打头。
fn pending_needle() -> &'static str {
    concat!("const ", "PENDING", ": &[&str] = &[")
}

/// 一份**本文件源码文本**里的 `PENDING_CEILING` 值。找不到 ⇒ `None`（不许默默当 0）。
fn ceiling_in(src: &str) -> Option<usize> {
    let needle = ceiling_needle();
    let at = src.find(needle)? + needle.len();
    src[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

/// 一份**本文件源码文本**里 `PENDING` 的条数。找不到那张表 ⇒ `None`。
///
/// 口径与判据自己数的那个对齐（`PENDING.iter().filter(|s| !s.is_empty()).count()`）：
/// 从表头那一行起、到同层 `];` 为止，数**以引号打头**的行。
fn pending_count_in(src: &str) -> Option<usize> {
    let at = src.find(pending_needle())?;
    let mut n = 0usize;
    for line in src[at..].lines().skip(1) {
        let t = line.trim_start();
        if t.starts_with("];") {
            return Some(n);
        }
        if t.starts_with('"') {
            n += 1;
        }
    }
    None
}

/// 跑一条**只读**的 git，回它的 stdout。
///
/// 🔴 **两种「问不到」分开报**，它们在类型上不是一回事（照 `skill_host::git_common_dir`
/// 那条逐字记着的实测）：机器上没有 `git` 时 [`std::process::Command`] 给的是
/// `io::Error(NotFound)`，**不是**一个非零退出码。
///
/// ⚠ 两支都 **fail-closed（panic）**，这是刻意的：读不到历史时必须红，不许静默地绿 ——
/// 「历史面是空的」与「棘轮没被倒着转」在输出上一模一样，那正是本条要治的形状。
fn git_read(root: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "起不来 `git`（{e}）—— 本条拿 git 历史当权威，问不到就**不许猜一个出来**。\n\
                     ⚠ 这一支不是「git 说不知道」，是**进程都没起来**（PATH 里没有它）。\n\
                     ⇒ 本条刻意 fail-closed：读不到历史时红，而不是绿。"
            )
        });
    assert!(
        out.status.success(),
        "`git {}` 在 {} 上退出码 {:?} —— 这一支是「git 起来了、但它说不行」。\n\
             git 自己说：{}\n\
             ⇒ 常见来路：这棵树不在版本控制里 · 浅克隆（`--depth`）把历史截掉了。\n\
                两种都要修环境，**不许把本条改成读不到就跳过**（那等于把闸拆了）。",
        args.join(" "),
        root.display(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// 本文件在 git 历史上每一个版本的读数 —— `(短 sha, 上限, 条数)`，外加**没解析出来**的份数。
///
/// ⚠ 解析不出来的**不静默丢掉**：份数一起回，由调用方连读数印出来。
/// （合法的一形：某个提交早于这两个常量存在。今天 9 份全解析得出，实测。）
fn ratchet_history(root: &Path) -> (Vec<(String, usize, usize)>, usize) {
    let mut rows = Vec::new();
    let mut unparsed = 0usize;
    // 🔴 `--follow` ＋ **按「当时的路径」取 blob** —— 两件都是必须的：
    // 搬树（2026-09-17）给本文件改了名，于是
    //   ① `git log -- <新路径>` 只看得到改名之后的提交（现打：1 份 vs 92 份）；
    //   ② 即便用 `--follow` 拿回了 sha，`git show <老sha>:<新路径>` 也读不到
    //      —— 那些提交里它叫**旧名字**（现打：92 份里 91 份读不出来）。
    // ⇒ 用 `--name-only` 让 git 顺带报出每个提交里**当时的**路径，成对取。
    // 少任何一半，这条棘轮都会「历史面一空 ⇒ 下面几格恒真地绿」。
    //
    // 🔴 〔步 7c 剖分 2026-09-19〕**再加一件：历史面要跨「剖分」这一刀。**
    //
    // `--follow` 认的是**改名**（git 的相似度检测）。而剖分不是改名：
    // `PENDING`/`PENDING_CEILING` 从 `src/bridge/src/scanning_guard_registry.rs`
    // 里被**切出一段**放进新文件 `tests/bridge/scanning_guard_registry_tests.rs`
    // ⇒ git 眼里那是一个**新增文件**，`--follow` 一步都跨不过去。
    // 现打：只把 `SELF_REL` 改成新住址 ⇒ 历史面从 9 份掉到 **1 份**，
    // 地板（5）当场红 —— 红得对，而**不许靠调低地板让今天好过**（上面那句逐字）。
    // ⇒ 历史面改成**两个住址并起来**：新住址查剖分之后的提交，
    //   旧住址带 `--follow` 查剖分之前的整条历史（它自己还跨着 09-17 那次搬树改名）。
    //   按 sha 去重。少任何一个住址，这条棘轮都会退回「历史面一空 ⇒ 恒真地绿」。
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (rel, follow) in SELF_HOMES {
        let mut args: Vec<&str> = vec!["log"];
        if *follow {
            args.push("--follow");
        }
        args.extend(["--format=%h", "--name-only", "--", rel]);
        let log = git_read(root, &args);
        let mut cur: Option<String> = None;
        for line in log.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match cur.take() {
                None => cur = Some(line.to_string()),
                Some(sha) => pairs.push((sha, line.to_string())),
            }
        }
    }
    pairs.sort();
    pairs.dedup();
    for (sha, path_then) in pairs {
        let spec = format!("{sha}:{path_then}");
        let blob = git_read(root, &["show", &spec]);
        match (ceiling_in(&blob), pending_count_in(&blob)) {
            (Some(c), Some(p)) => rows.push((sha.to_string(), c, p)),
            _ => unparsed += 1,
        }
    }
    (rows, unparsed)
}

/// 纯算子：今天的读数 `today` 对着历史面 `hist`，棘轮有没有**被倒着转**。
///
/// 回**历史上最低的那一档**当见证；没被倒转 ⇒ `None`。
///
/// 🔴 **单独成函数**，与 [`declares_a_guard_table`] 同一个理由：真树上今天 `today`
/// 恰好**等于**历史最低档 ⇒ 把 `>` 写成 `<`、或者把 `hist` 传成空的，
/// **输出与判对了一模一样（绿）**。
/// [`the_ratchet_reader_can_tell_a_raise_from_a_drop`] 拿合成读数把这一格钉住。
///
/// ⚠ **诚实边界**：`hist` 为空时它回 `None`（= 绿）。**空历史那一格不归它**，
/// 归 [`the_pending_ratchet_never_turns_backwards`] 里那条**地板**。
/// 两格刻意分开：并成一格的话，「历史读不到」与「棘轮没被倒转」又会同形。
fn ratchet_backslide(today: usize, hist: &[(String, usize)]) -> Option<(String, usize)> {
    let low = hist.iter().min_by_key(|(_, v)| *v)?;
    if today > low.1 {
        Some(low.clone())
    } else {
        None
    }
}

/// ★ `K-R38` 的正题：**那两个数不许比它们在历史上出现过的最低档还高。**
///
/// 选路理由（乙，不是甲）· packfile 那条风险怎么证掉的 · 它**没有**买到什么，
/// 全在模块头注 `K-R38` 那一节，**这里不复述**（复述就会漂）。
#[test]
fn the_pending_ratchet_never_turns_backwards() {
    let root = repo_root();
    let n = PENDING.iter().filter(|s| !s.is_empty()).count();
    let (hist, unparsed) = ratchet_history(&root);

    // 抽取器自检①：**历史面不许是空的 / 短的**。
    //
    // 🔴 这一格是本条的地基：`ratchet_backslide` 拿到空历史时回 `None`（绿），
    // 于是「git 读不到历史」与「棘轮没被倒着转」**输出完全相同** ——
    // 那正是本模块从头到尾在治的形状，只是这次长在本条自己头上。
    // 会把历史面弄空的真实来路：浅克隆（`--depth 1`）· 两个常量被改了名
    //（针是按名字认的）· 本文件被挪了地方（`SELF_REL` 就馊了）。
    const HISTORY_FLOOR: usize = 5;
    assert!(
        hist.len() >= HISTORY_FLOOR,
        "只从 git 历史里读出 {} 份本文件的旧版本（地板 {HISTORY_FLOOR}，09-06 实测 9 份，\
             另有 {unparsed} 份解析不出来）——\n\
             ⇒ **本条此刻是空转的**：历史面一空，下面那两格恒真地绿。\n\
             常见来路：① 浅克隆把历史截掉了（要 `fetch-depth: 0`）；\n\
                       ② `PENDING` / `PENDING_CEILING` 被改了名（针是按名字认的）；\n\
                       ③ 本文件挪了位置 ⇒ `SELF_REL`（`{SELF_REL}`）馊了。\n\
             🔴 **不许靠调低地板让今天好过** —— 那是把闸拆了，而拆完输出还是绿的。",
        hist.len()
    );

    // 抽取器自检②：**解析器与真常量对拍。**
    //
    // 上面那两个针是拿文本认的，而下面比的是**真常量**（`PENDING_CEILING` / `n`）。
    // 解析器要是系统性偏了（比如总是多数一行、或总回一个大数），历史最低档跟着偏，
    // 而**真树上照样绿**。⇒ 拿本文件此刻的源码喂一遍解析器，逼它复现那两个真值。
    // 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §6.2` C 类〕**嵌的是「本文件」，不是那份生产文件。**
    //
    // 这一行的意思逐字是「拿**本文件此刻的源码**喂一遍解析器」。剖分之前本条住在
    // `src/bridge/src/scanning_guard_registry.rs` 的 `#[cfg(test)]` 段里，那份文件就是本文件；
    // 剖分之后那两个常量跟着本条搬来了 `tests/bridge/`，而生产段那份里一个都没有了。
    // ⚠ 剖分器把这条相对路径**按原语义重定向**过（它仍然指向那份生产文件）——
    //   路径是对的，指错的是**对象**。这一格正是「机械正确、语义失效」那一形。
    let me = include_str!("scanning_guard_registry_tests.rs");
    assert_eq!(
        (ceiling_in(me), pending_count_in(me)),
        (Some(PENDING_CEILING), Some(n)),
        "解析器在**本文件此刻的源码**上复现不出那两个真常量 —— 它偏了。\n\
             ⇒ 历史面上的读数跟着一起偏，而真树上本条**照样绿**（两边同向偏）。\n\
             这一格就是为了不让那种偏法静默通过。"
    );

    // 只在 `--nocapture` 下可见 —— 射程与历史面本身也是读数（`brief` 13b：现算，别写死）。
    eprintln!(
        "〔存量棘轮 · 本趟的历史面〕{} 份旧版本（解析不出 {unparsed} 份）· \
             今天 上限={PENDING_CEILING} 条数={n}\n  {}",
        hist.len(),
        hist.iter()
            .map(|(s, c, p)| format!("{s} 上限={c} 条数={p}"))
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    let ceilings: Vec<(String, usize)> = hist.iter().map(|(s, c, _)| (s.clone(), *c)).collect();
    let counts: Vec<(String, usize)> = hist.iter().map(|(s, _, p)| (s.clone(), *p)).collect();

    if let Some((sha, was)) = ratchet_backslide(PENDING_CEILING, &ceilings) {
        panic!(
            "🔴 **棘轮被倒着转了**：`PENDING_CEILING` 今天是 {PENDING_CEILING}，\
                 而它在 `{sha}` 上是 {was}。\n\
                 上面那行头注写着「只许变短」——**这一条从今天起是机器在守，不再是纪律**。\n\
                 ⇒ 处置：把上限调回 {was} 或更低。\n\
                 ★ 想「先抬一格让今天好过」的话，本条正是来挡这个动作的：\n\
                   在它之前，抬这个数是**改一个字符、零阻力、零留痕、零人知道**\n\
                  （`n <= PENDING_CEILING` 里那个上限就在同一份文件里 ⇒ 抬它只会更容易过）。\n\
                 ⚠ 提交了也不会变绿：本条比的是**历史上出现过的最低档**，那个更低的档还在。\n\
                 ⚠ 真有一条新的非进 `PENDING` 不可 ⇒ 先答「为什么它不能走 `scan_tree!`」，\
                   那是一次要被人看见的讨论，不是一个字符。"
        );
    }
    if let Some((sha, was)) = ratchet_backslide(n, &counts) {
        panic!(
            "🔴 **存量清单涨回去了**：`PENDING` 今天 {n} 条，而它在 `{sha}` 上是 {was} 条。\n\
                 头注逐字写着「**只许变短**」——今天守它的是本条。\n\
                 ⇒ 处置：把新加的那几行拿掉，改走 `guard_core::scan_tree!`。\n\
                 ⚠ 别去抬 `PENDING_CEILING` —— 上面那一格会当场逮住它。"
        );
    }
}

/// ★ `K-R38` 的**反向那半**：那把比较尺子，得真的分得开「抬上去」与「降下来」。
///
/// # 没有它，本条是一场仪式
///
/// 真树上今天 `PENDING_CEILING` 与 `n` **恰好等于**历史最低档（9 个提交上余量都是 0）。
/// ⇒ 把 [`ratchet_backslide`] 里的 `>` 写成 `<`、把 `min_by_key` 写成 `max_by_key`、
/// 或者让历史面传成空的 —— **真树上的输出与判对了一模一样（绿）**。
/// 那正是本模块从头到尾在治的形状：**「判过了」与「压根没判」不可区分。**
///
/// ⚠ **夹具里的 sha 与数字都取中性值**，断言比的是**喂进去的那个值本身**
/// （`brief` 12 的 `6g`：别让断言取自夹具的名字）。
#[test]
fn the_ratchet_reader_can_tell_a_raise_from_a_drop() {
    let hist: Vec<(String, usize)> = [("aaa", 31), ("bbb", 30), ("ccc", 29)]
        .iter()
        .map(|(s, v)| ((*s).to_string(), *v))
        .collect();
    let low = ("ccc".to_string(), 29);

    // 正：抬上去 ⇒ 必须逮到，而且点的是**历史最低**那一档（不是最近那一档）。
    // 🔴 「点最低那一档」是承重的：点最近那一档的话，抬上去之后只要**提交一次**，
    // 最近那一档就变成抬过的值 ⇒ 下一趟当场变绿，棘轮咬完就松。
    assert_eq!(
        ratchet_backslide(30, &hist),
        Some(low.clone()),
        "比 30 高于历史最低档 29 —— 这一格没逮到，说明比较写反了或者点错了档"
    );
    assert_eq!(
        ratchet_backslide(99, &hist),
        Some(low),
        "点的必须是**历史最低**那一档，不是最近的那一档"
    );

    // 平 / 降：棘轮正着转，一格都不许红。
    assert_eq!(
        ratchet_backslide(29, &hist),
        None,
        "与历史最低档持平，不许红"
    );
    assert_eq!(
        ratchet_backslide(28, &hist),
        None,
        "降下去正是要买的动作，不许红"
    );
    assert_eq!(ratchet_backslide(0, &hist), None, "降到底，仍然不许红");

    // 🔴 **空历史 ⇒ 它回 `None`（绿）**，这一格是**故意钉住的诚实边界**，不是缺陷：
    // 接住「历史面读不到」的是 `the_pending_ratchet_never_turns_backwards` 里那条**地板**。
    // 钉在这里，是为了不让谁把这一支改成 panic 之后顺手把那条地板删掉 ——
    // 那样一来两格并成一格，而并完之后**没有任何输出会变**。
    assert_eq!(
        ratchet_backslide(usize::MAX, &[]),
        None,
        "空历史这一支归**地板**管，不归这把尺子管；两格刻意分开，别并"
    );

    // 解析器那一半：针是运行时拼的，拿它自己拼出来的文本正反各喂一遍。
    let synthetic = format!("    {}{};\n", ceiling_needle(), 7);
    assert_eq!(
        ceiling_in(&synthetic),
        Some(7),
        "解析器认不出自己那根针拼出来的声明"
    );
    assert_eq!(
        ceiling_in("没有这根针的一段文本"),
        None,
        "认不出就要回 None，不许默默当 0"
    );

    let table = format!(
        "    {}\n        \"a.rs\",\n        \"b.rs\",\n    ];\n",
        pending_needle()
    );
    assert_eq!(pending_count_in(&table), Some(2), "表里两行，数不出 2");
    assert_eq!(
        pending_count_in(&format!("    {}\n    ];\n", pending_needle())),
        Some(0),
        "空表要回 Some(0)，与「找不到那张表」（None）**不是一回事**"
    );
    assert_eq!(
        pending_count_in("没有那张表的一段文本"),
        None,
        "找不到表就回 None"
    );
}
