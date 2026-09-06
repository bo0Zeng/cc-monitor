//! **扫描型判据的自匹配元判据**〔audit-0805 F23，F+#3 开〕。
//!
//! # 它治的不是一个 bug，是一个**族**
//!
//! 症状永远一样：**判据在自己的登记表 / 注释 / 常量里找到了自己要找的东西 ⇒ 恒绿**。
//! audit-0805 实测五次：
//!
//! | 何处 | 判据在自己的什么东西里找到了自己 |
//! |---|---|
//! | F12 | 跨语言对拍匹配到自己的**注释** |
//! | F13 | 原子替换登记表匹配到自己的 `RULE` **常量**（写着两个符号的调用示例） |
//! | F18 | 文档副本判据匹配到自己的**登记表**（`HAS_A_GUARD` 里写着那些判据名） |
//! | F05 | 函数体抽取的**锚点**命中了自己 `PHASES` 表里的字符串 |
//! | F05 | 棘轮 `matches()` **数到自己**：6 vs 真实 4 |
//!
//! ★ **五次没有一次是被「判据变红」发现的** —— 四次靠变异、一次靠 clippy。
//! 这一类缺陷的**默认结局是恒绿**，所以「以后小心点」不是修法。
//!
//! # 修法：让它写不出来，而不是再检测一遍
//!
//! `guard_core::scan_tree!` 按构造摘除调用者自己那一份（用 `file!()`，调用方改不错）。
//! 本模块要求：**测试段里不许再出现裸的目录遍历** —— 要么走 `scan_tree!`，
//! 要么在下面这张存量清单里，而清单**只许变短**。
//!
//! # ★ 存量已**逐条判过真伪**（08-06 第二刀）
//!
//! 判准是**「它靠什么读不到自己」**，四类穷尽（下面 `PENDING` 每一行都属其一）：
//!
//! | 类 | 数 | 它凭什么安全 |
//! |---|---|---|
//! | **生产 / 夹具 IO** | 5 | 遍历的根本不是源码树（`.claude/projects` 的 jsonl · 标注池 · tempdir · `.ssh` · PowerShell profile）。**自匹配这个概念对它们不成立** |
//! | **剥生产段（构造性摘除）** | 21 | 只扫 `production_code`/`production_source` 的产物；判据自己住在 `#[cfg(test)]` 里 ⇒ **按构造读不到自己** |
//! | **显式摘除自身** | 2 | `SELF` 常量 / `replace(&own, "")`（`atomic_replace_registry` · `doc_copy_registry`） |
//! | **扫的树不含自己** | 2 | `frame_cadence_guard` 只扫 `.md`（自己是 `.rs`）· `shared_crate_registry` 扫 `crates/` 与 `Cargo.toml`（自己在 `src-tauri/src`） |
//!
//! ⇒ **30 个存量里没有一个是「会自匹配却没防住」的**。它们不是 30 个待修的 bug，
//! 是 30 个**已分类的、各有安全理由的**遍历。棘轮继续挡**新增**，而不再暗示这里有一堆债。
//!
//! ⚠ 第二刀唯一动过的一个是 `parity_ledger.rs`：它原来靠**写死文件名**跳过自己
//! （`== Some("parity_ledger.rs")`）—— 那种摘除**改名即静默失效**，而失效后看起来和没失效
//! 一模一样（`scan_tree!` 头注逐字警告过这个形态）。已换成 `scan_tree!`（按 `file!()` 摘除）。
//! ★ **但要如实说**：把那个摘除关掉，**没有任何判据变红** —— 真正挡住它自匹配的是
//! 另一招（`attr` 运行时拼 `format!("#[tauri::{}]", "command")`，于是字面量不在自己源码里）。
//! ⇒ 这一改**去掉的是一个改名即失效的形态，不是修了一个活缺陷**。别把它读成后者。
//!
//! 本条的契约仍是两句：**新增的不许出现；存量只许降。**
//!
//! # ★ 登记一个**第五形**（`K-R31` `D2`，09-06）—— 只登记，不加判据
//!
//! 上面那张四类表说的是「这份遍历**凭什么读不到自己**」。`K-R31` 新立的那条判据
//! （`local_backend.rs::nothing_in_the_production_path_runs_code_between_fork_and_exec`）
//! 走的是一个上面没有的形状：**`scan_tree!` 摘掉自己那份之后，
//! 又用 `include_str` 那个宏把自己那一份显式加回来**。
//!
//! 为什么它非这么写不可：那条判据扫的是「生产段里有没有 `pre_exec`」，
//! 而**最可能长出那种写法的恰恰是它自己所在的文件**（起进程那一跳就在那儿）——
//! 按构造摘掉自己 = 在最该看的那一份上瞎掉。
//! 它挡住自匹配靠的是**第二类**（剥生产段）：形态表住 `#[cfg(test)]`、被守的那句话住 `///`，
//! 逐份过 `guard_core::production_code` 之后**按构造都不在扫描面里**，
//! 并配了一条**阴性对照**（同一段包进注释 ⇒ 必须不命中）把「剥法真的在跑」钉住。
//!
//! ⚠ **这一段只是登记，不是判据**，而且**这一形有几处我没数出来** —— 说清查了哪条路：
//! 09-06 现打的是一把**文件级**的尺子（`scan_tree!` 与「取自己那一份的 `include_str`」
//! 同现于一份文件 ⇒ **16 份**），而事实的单位是「**同一条判据里**摘掉自己又加回来」——
//! 尺子比事实粗，那 16 份里多半是「两条不同的判据各用各的」。⇒ **那个数没人量过。**
//! 而这一形一旦被复制而**没带剥法**，本模块**一个字都看不见**。
//! **别把这句读成「已经守住了」，也别读成「全仓就这一处」。**
//!
//! # 🔴 `K-R33`（09-06）—— 上面那一段登记完的**第二天**就发现：它根本没被判到
//!
//! `every_registry_guard_keeps_its_reverse_half` 靠 `TABLE_DECLS` **按名字**认表，
//! 而 `K-R31` 那条判据的表叫 `FORMS` —— 名字不在那个闭集里 ⇒ 它**恒真地过**。
//!
//! 🔴 **要紧的不是漏了一条，是「过了」与「它没扫到你」在输出上一模一样**（都是静默的绿）。
//! ★ 这正是本模块头注治的那一族的**镜像**：那一族是「判据在自己的登记表里找到了自己」，
//! 这一格是「**登记表根本没去找它**」。两边的默认结局都是恒绿。
//!
//! ## 走的是「闭集 ＋ 点名钉住」，不是「按形状认」—— 为什么
//!
//! 两条路，实测之后选了前者：
//!
//! - **按形状认**（口径改成「测试段里既有 `const … : &[…]`、又有一处树遍历」）：
//!   09-06 现打，人群从 8 涨到 33，而其中**两份当场红** ——
//!   `exec_site_registry.rs` 真的有反向那半，只是措辞是「已经不存在」而 `REVERSE` 只认另两句
//!   （⇒ 要落地它，还得**再放宽一次** `REVERSE`）；
//!   `tmux_daemon_gate_guard.rs` 压根没有登记表，它那几张全是 needle 表（⇒ 那是**误采**）。
//!   ⇒ 这条路要么连着放宽两处，要么配一张豁免清单，两样都是在拆这条守卫。
//! - **闭集 ＋ 点名钉住**（今天这一条）：`TABLE_DECLS` 里加一个名字，人群 8 → 9，
//!   新进来的**恰好**是被漏掉的那一份（`const FORMS:` 全树现打**只有一处**）。
//!
//! ⚠ **代价是真的，写在这儿别让下一个人重新发现**：闭集按名字认 ⇒
//! **改名即静默失效**，而本模块前面那一段（`parity_ledger`）逐字警告过这个形态。
//! ⇒ 对价是 `every_registry_guard_keeps_its_reverse_half` 里那条 `MUST_BE_RECOGNISED`：
//! 它拿**真实住址**把这一形钉住，于是「表改了名 ⇒ 掉出人群」从**静默**变成**当场红并点名**。
//! ★ 但它只钉住**被点名的那几份**：别的判据改表名，本模块仍然看不见。
//!
//! ## 📌 纪律（这一条是本节存在的理由，不是附注）
//!
//! **新写一条「扫描面 ＋ 常量表」型的判据，那张表要起成 `TABLE_DECLS` 里已有的名字之一。**
//! 起了别的名字 ⇒ 这条元判据看不见你，而你会以为它在守着 —— 那就是 `K-R31` 那一轮的实况。
//! 非要用新名字的话：往 `TABLE_DECLS` 里加，**并且同拍往 `MUST_BE_RECOGNISED` 里加一行**
//! （只加前者的话，下次谁把名字改回去就又是静默的绿）。
//!
//! ## ⚠ 它**没有**买到什么（`K-R33` 只治一形）
//!
//! - **粒度是「文件」，不是「那条判据」**：一份文件只要测试段里有任意一处 `REVERSE` 形态就算过。
//!   `local_backend.rs` 的测试段里另有 **27 处** `assert_eq!(`
//!（09-06 现打：全文 28 处、全部落在测试段内，其中只有 1 处是 `K-R31` 的阳性对照）
//!   ⇒ **把 `K-R31` 那条判据的阳性对照整个删掉，本条照样绿**（09-06 变异实打，读数在件文件里）。
//!   本条买到的是「它进了人群、数得着」，**不是**「它的反向那半被逐条守着」。
//! - **射程只有 `src-tauri/src` 一棵树**（`scan_tree!` 的实参就写在那儿）：
//!   `remote-daemon-proto/src` 里的登记表这条元判据一份也没看。
//! - **`D1②` 现打的真数**（09-06，分母 = 三棵树 `*_registry.rs`/`*_guard.rs` 共 47 份、
//!   剔掉本文件自己）：那 138 条 `const X: &[` 里**逐条读过**，真的是「一条判据自带的登记表」
//!   的有 63 条，而今天认得出的只有 8 条 —— 其中 20 条住在 daemon 树（射程之外，另一族病）。
//!   **剩下的这一族本件刻意不治**，读数与逐条判词落在 `K-R33` 件文件里。

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// 裸遍历的形态。
    const RAW_WALKS: &[&str] = &["read_dir(", "WalkDir", "collect_rs(", "collect_ts("];

    /// **存量**：测试段里仍在裸遍历的文件（08-06 实测 31 个）。
    ///
    /// ⚠ **只许变短。** 迁一个就从这里删一行并把 `PENDING_CEILING` 调下来。
    /// 不许往里加 —— 新写的扫描型判据必须走 `guard_core::scan_tree!`。
    const PENDING: &[&str] = &[
        "src-tauri/src/account_usage.rs",
        "src-tauri/src/atomic_replace_registry.rs",
        "src-tauri/src/backend/control/daemon_kill.rs",
        "src-tauri/src/backend/control/launch_wire.rs",
        "src-tauri/src/backend/control/local_query.rs",
        "src-tauri/src/backend/mod.rs",
        "src-tauri/src/cross_half_edge_registry.rs",
        "src-tauri/src/doc_claim_registry.rs",
        "src-tauri/src/doc_copy_registry.rs",
        "src-tauri/src/frame_cadence_guard.rs",
        "src-tauri/src/gate_singleton_guard.rs",
        "src-tauri/src/local_read_surface_registry.rs",
        "src-tauri/src/panorama.rs",
        "src-tauri/src/parser.rs",
        "src-tauri/src/polling_registry.rs",
        "src-tauri/src/profile_installer.rs",
        "src-tauri/src/quote_singleton_guard.rs",
        "src-tauri/src/rust_timer_registry.rs",
        "src-tauri/src/session_name_registry.rs",
        "src-tauri/src/shared_crate_registry.rs",
        "src-tauri/src/ssh_source.rs",
        "src-tauri/src/tmux_daemon_gate_guard.rs",
        "src-tauri/src/utils.rs",
        "remote-daemon-proto/src/layering_guard.rs",
        "remote-daemon-proto/src/no_timer_guard.rs",
        "remote-daemon-proto/src/observe/watcher.rs",
        "remote-daemon-proto/src/platform/fallback_guard.rs",
        "remote-daemon-proto/src/protocol_doc_guard.rs",
        "remote-daemon-proto/src/readonly_guard.rs",
    ];

    /// 存量上限（**递减棘轮**）。
    // 08-08：`daemon_route.rs` 的裸遍历迁到了 `guard_core::scan_tree!`（那一轮把它的
    // 发现面从一个目录扩到整棵树，顺带就该换掉手写遍历）⇒ 清单少一行，上限一起降。
    const PENDING_CEILING: usize = 29;

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

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf()
    }

    /// 抠出所有 `#[cfg(test)]` 段（到下一个顶层 `}` 为止）。
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
        /// 反向那半的两种合法形态。
        const REVERSE: &[&str] = &["assert_eq!(", "已经不在了", "已经没有"];
        let mut population: Vec<String> = Vec::new();
        let mut missing: Vec<String> = Vec::new();
        for (f, src) in guard_core::scan_tree!(&root.join("src-tauri/src"), &["rs"]) {
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            let regs = test_regions(&src);
            if !declares_a_guard_table(&regs) {
                continue;
            }
            population.push(rel.clone());
            if !REVERSE.iter().any(|m| regs.contains(m)) {
                missing.push(rel);
            }
        }
        // 🔴 `K-R33`：**把采到了谁印出来。**
        //
        // 在此之前，本条对一份「它压根没扫到」的文件与一份「它采到了、而且过了」的文件
        // **输出完全相同**（都是静默的绿）。`K-R31` 新加的那条判据整整一轮落在前一格里
        // 而没有任何人看得出来 —— 那不是漏了一条，是这条判据**说不出自己看了谁**。
        // 只在 `--nocapture` 下可见；判红时那几条断言的文案里另有一份。
        // ⚠ 标签刻意**不写判据的函数名** —— 抄一份名字进字符串，改名那天它就是一句假话
        //（本模块头注里的 `parity_ledger` 就是这个形态）。判据名 cargo 自己会打在上一行。
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
            "src/backend/control/local_backend.rs",
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
        // 量具 `evidence/K-R33-table-decl-census.py`）：那 138 条里三者合计比登记表还多。
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

    /// 今天仍在裸遍历的文件（相对仓根）。
    fn raw_walkers() -> Vec<String> {
        let root = repo_root();
        let mut out = Vec::new();
        for sub in ["src-tauri/src", "remote-daemon-proto/src"] {
            // ★ 本模块自己也走 `scan_tree!` —— 它就是那条规矩的第一个遵守者。
            //
            // ⚠ **摘除在这里今天不是承重的**（变异实测）：把 `scan_tree!` 换成一个匹配不上的
            // 摘除名，本条**照样绿** —— 因为真正让本文件不被标记的是下面那个
            // `!regs.contains("scan_tree!")`：本模块的测试段里就写着 `scan_tree!`。
            // 留着摘除是**纵深防御**：哪天本模块多写一个不走 `scan_tree!` 的扫描助手，
            // 没有摘除就会拿 `RAW_WALKS` 里那四个字面量把自己算进去。
            // ★ 「哪一行在真正干活」这种断言**必须变异验过再写** —— 本区第三次
            //（F14 第六刀 `[ -r ]` 不能省 · F12 `uiStrings` 两道都不能省 · 本条）。
            for (f, src) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
                let regs = test_regions(&src);
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
}
