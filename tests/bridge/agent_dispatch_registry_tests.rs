use std::path::{Path, PathBuf};

/// 一处耦合的**脸**。四张脸的性质不同，前三张该压到零，第四张方向相反。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Face {
    /// 调用点说不出它指哪个 agent（`active()`）。
    Active,
    /// 门面替调用者把 kind 写死了（`records_dir` 那一族）。
    Facade,
    /// 拿 `AgentKind::<名>` 字面量当实参 —— 写死，不是分派。
    KindLiteral,
    /// 传运行时 kind —— 接口正在被正确使用。**不上棘轮。**
    RuntimeDispatch,
}

impl Face {
    /// 上不上递减棘轮。第四张脸是好方向，混进去棘轮就会奖励「把真分派改回写死」。
    fn on_the_ratchet(self) -> bool {
        self != Face::RuntimeDispatch
    }
}

/// **通用层认得出某个 adapter 的地方**，逐条登记（`文件`, `脸`, `处数`, 它在向适配层要什么）。
///
/// ⚠ 这不是白名单：**表里有几处，就意味着「把 agent 收进接口要回来改几处」**。
/// 它长了是设计在退化；短了才说明真收进接口了（那一轮同时摘登记）。
///
/// ⚠ `adapter.rs` 自己那几行**也在表里** —— 见模块头注「本条比后端侧那条少一个洞」。
const AGENT_COUPLING_SITES: &[(&str, Face, usize, &str)] = &[
    // 〔MIG-3b · `99 §2.1 ㉓②`〕`adapter.rs` 的 `Active` 那一行（1 处：「任务追踪目录」那个门面的实现体）摘了：
    //   它唯一的调用方（monitor 自己那条任务 notify）随监视进后端删了，门面零调用方、一起删。
    // 〔CF1 · 第四波 09-24〕`adapter.rs` 的 `Facade` 那一行（1 处：「是不是顶层会话记录文件」把两个门面再合成一个）摘了 ——
    //   它与它合成的那个跳过段门面，唯一的调用方都是 monitor 自己那套 jsonl watcher，随它一起删了。
    (
        "adapter.rs",
        Face::KindLiteral,
        1,
        "`active()` 的定义体写死 Claude（**这一处是刻意的**，它就是「活跃那个是谁」的答案）。\
             ⇒ **不是欠账，是接口的边界条件**；登记它是为了「加第三家时这里必改」有人知道。\
             〔LOC1b · 第四波 4D〕**2 → 1**：`enabled_kinds()` 探 Codex 会话根那一处随它删了（本机不再按根找会话）",
    ),
    (
        "history.rs",
        Face::Active,
        3,
        "resume flag / 默认启动器 / 中转注入的 agent id ⇒ **两处读面 + 一处写面**。\
             ⚠ 写面那一处（`relay_prefix_for` 里的 `active().id()`）是 08-28 审计\
             点名「写面没有分派」之后**又长出来的**，十天没有任何东西红过 —— 本条买的就是它",
    ),
    // 〔LOC1b · 第四波 4D〕`history.rs` 的 `Facade` 那一行（1 → 0：本机冷读那一支取记录根 `records_dir_for` 的
    //   门面 `has_record_ext`）摘了 —— 本机冷读改经本机后端的 `history-read`，围栏归后端，桌面侧不再问「这个文件是不是会话记录」。〔散文墓碑〕
    //   ⚠ 与前几笔同形：**不是「收进接口了」**，是整段搬去后端。
    // 〔C4d · 第四波 4B〕`history.rs` 的 `KindLiteral` 那一行（2 处：Codex 枚举里写死 `AgentKind::Codex` 取数据根 ＋ 取 layout）摘了 ——
    //   Codex 合成历史搬进后端的适配层（`src/backend/agents/codex/history.rs`，经注册表 `Adapter.history` 那一格给通用层）。
    (
        "lib.rs",
        Face::Active,
        2,
        "起会话面：resume 前清洗嵌套 env（`nested_env_to_scrub`）+ setup 里取数据根",
    ),
    // 〔MIG-3b · ㉓②〕`lib.rs` 的 `Facade` 那一行（1 处：setup 里拼 tasks 目录喂 monitor 自己那条任务 notify）摘了 ——
    //   任务变更的监视进了后端（`tasks_changed` 帧），桌面侧不再问任务目录住哪。
    // 〔LOC1b · 第四波 4D〕`search.rs` 的 `Facade` 那一行（3 处：记录根 ＋ 记录判定 ＋ 从路径取 sid）摘了 ——
    //   本机全文搜索改问本机后端，monitor 那份内存索引整个删了。主会话 09-25 裁（按 `00 §2.5 ①` · `90 §4 F`），
    //   那一行自己写的退役条件（「backend 侧也有索引」）**没有兑现**：本机从此每次现扫（读数 `第四波记录/LOC1b.md §3`）。
    // 〔RM1b · 第四波〕`tasks.rs` 那一处门面（1 处，「任务追踪目录（`tasks_dir`）」）摘了：
    //   任务列表整段改问那台机器的后端，monitor 这一侧不再自己解析任务目录。
    // 〔CF1 · 第四波 09-24〕`watcher.rs` 那一行（门面 5 处：「是不是记录文件 ×3 ＋ 从路径取 sid ×2」）摘了：
    //   monitor 自己那套 jsonl watcher 整个删了，本机会话内容改走本机后端的 `line` 帧（后端那一份早就在做同一件事）。
    // ── 第四张脸：方向相反，登记但不上棘轮 ──────────────────────────
    (
        "adapter.rs",
        Face::RuntimeDispatch,
        1,
        "`agent_profile_facts(kind)` 里那一句 `for_kind(kind)`（`K-R93` 09-12）—— 前端那份 agent 画像的取数口，\
             收 kind、按 kind 取适配器，**没有 `active()`**。\
             〔LOC1b · 第四波 4D〕**3 → 1**：`records_dir_for` 与 `records_roots` 那两处随它们删了（零调用方）",
    ),
    (
        "history.rs",
        Face::RuntimeDispatch,
        1,
        "按文件名形态判出 kind 之后取 layout（`kind_of_record_name` → `for_kind(kind)`）—— 好方向。\
             〔LOC1b · 第四波 4D〕**2 → 1**：本机那一支取根那一次（`for_kind(kind).data_root()`）随那一支删了",
    ),
];

/// 立表那天的读数（前三张脸的总数）。**只许降。**
///
/// ⚠ 它是「桌面侧那一半差多少」的头条数字。件计划 `§0b` 此前登记的是
/// 「`active()` 硬编码 6 处 + `for_kind` 真分派 4 处」——**那个读数只盖住 `active()` 那一张脸，
/// 而门面那一族（立表时 24 处）一根针都没数到**。本条立表时把四张脸一起量了。
///
/// 🔴 〔`K-R97` 09-12〕**40 → 37**：`history.rs` 的门面那张脸 11 → 8，
/// 因为本机项目列表整段改走后端（详见那条登记）。这是这张棘轮第一次真往下走。
///
/// 🔴 〔`K-R88` 09-13〕**37 → 36**：`history.rs` 的门面那张脸 8 → 7，
/// 建分支那条路的源守卫随「找文件」一起进了共享 crate（详见那条登记）。
///
/// 🔴 〔`设计/50` 删用量〕**36 → 35**：`account_usage.rs` 那张 `Active` 脸（1 处，
/// 「用量探针要 `nested_env_to_scrub` + `default_launcher`」）随用量 ③ 轴整轴退役。
/// ⚠ **这一格不是「收进接口了」，是「那件事不做了」** —— 两种都让棘轮往下走，
/// 但只有前者算把耦合还清；如实写在这里，别让下一个人把它读成进展。
///
/// ⚠ 与后端侧那个 27 **不是同一把尺子，不许相加**（两侧机制不同：那边直呼
/// `agents::<名>::`，这边走 trait + 门面）。要比较请各自报各自的尺子。
///
/// 🔴 〔RM1b · 第四波〕**35 → 34**：`tasks.rs` 那张门面脸（1 处）随任务列表改走后端摘掉。
/// ⚠ 如实写：这一格是**搬走**不是**收进接口** —— 读任务目录那一步搬去了后端，
/// 而后端侧那把尺子（`agent_locality_guard`）今天没数它（住址理由在 `observe/tasks_query.rs` 头注）。
///
/// 🔴 〔RW1 · 第四波 09-24〕**−3**：`history.rs` 的门面那张脸 7 → 4 —— 本机删会话那道路径守卫（−2）与本机分叉那一支取记录根（−1）
/// 整段搬去后端（详见那条登记）。⚠ 与 `K-R97` 同形：**不是「收进接口了」**，是桌面侧不再问这件事。
/// 〔合并〕RM1b −1 与 RW1 −3 两边各自减，35 → 31。
///
/// 🔴 〔CF1 · 第四波 09-24〕**31 → 23**：`watcher.rs` 那张门面脸（5 处）整份删了，`lib.rs` 门面 3 → 2（records 那一处），
/// `adapter.rs` 的 `Active` 6 → 5、`Facade` 1 → 0（只有那条 watcher 用的两个门面一起删了）。
/// ⚠ 同样**不是「收进接口了」**：本机会话内容改走本机后端的 `line` 帧，桌面侧从此不自己 watch、不自己判记录文件。
///
/// 🔴 〔C4d · 第四波 4B〕**23 → 18**：`history.rs` 门面 4 → 1、`KindLiteral` 2 → 0 —— 本机历史清单（展开项目 · Codex 枚举）
/// 整段搬去本机常驻后端。⚠ 同上几笔：**不是「收进接口了」**，是桌面侧不再问这件事（后端那把尺子
/// `agent_locality_guard` 没涨：Codex 那半进了 `agents/codex/`，经注册表那一格够到）。
///
/// 🔴 〔LOC1b · 第四波 4D〕**18 → 14**：`history.rs` 门面 1 → 0 —— 本机冷读改经本机后端的 `history-read`（本机远端一条路）；
/// `search.rs` 门面 3 → 0 —— 本机全文搜索改问本机后端，monitor 内存索引删了。⚠ 同上几笔：不是「收进接口了」，是桌面侧不再做这件事。
/// 🔴 〔LOC1b · 第四波 4D〕**14 → 13**：`lib.rs` 门面 2 → 1 —— 本机判活改由本机后端的帧来，monitor 不再拼 `sessions/` 目录。
/// 🔴 〔LOC1b · 第四波 4D〕**13 → 8**：`adapter.rs` 门面实现体 5 → 1 · 字面量 2 → 1 —— 只为本机读盘服务的那几个门面零调用方、删了。
const COUPLING_BASELINE: usize = 6; // 〔MIG-3b〕8 → 6：`adapter.rs` 的 `Active`（任务门面的实现体）· `lib.rs` 的 `Facade`（拼 tasks 目录）两处随监视进后端删了

/// **抹除 kind 的门面**：`adapter.rs` 里那几个「替调用者把 agent 写死」的自由函数。
///
/// 判准是一条**可复述的规则**，不是逐个品味：**它的实现体里调 `active()`，
/// 而它的签名里没有 kind**（每一个都有一个带 kind 的兄弟 `*_for` / `*_with`）。
///
/// ⚠ 手写闭集 ⇒ 加一个新门面不会自动进人群。对价是
/// [`every_registered_facade_really_erases_the_kind`]：每一条必须真在 `adapter.rs` 里、
/// 且那个函数**真的**经 `active()` 取适配器。
///
/// ⚠ 门面也可以自己不调 `active()`、而是调另外的门面（CF1 之前有过一个：「是不是顶层会话记录文件」）—— 单独一行说明，
/// 那条判据按「调 `active()` **或** 调另一个已登记门面」放行它。
const KIND_ERASING_FACADES: &[(&str, &str)] = &[
    // 〔MIG-3b〕「任务追踪目录」那个门面删了（唯一调用方——monitor 那条任务 notify——随监视进后端删了）⇒ 本表空了。
    // 〔CF1 · 第四波 09-24〕「这个路径要不要跳过」与「上面两个的合成」那两个门面随 monitor 的 jsonl watcher 一起删了。
    // 〔LOC1b · 第四波 4D〕会话记录目录 · 活性 pidfile 目录 · 「是不是会话记录」· 从路径取 sid 四个门面删了（零调用方：
    //   冷读 · 判活 · 搜索都改问本机后端）。
];

/// 人群下界：`src/bridge/src` 今天 105 份 `.rs`，**105 份全部进扫描**。
///
/// ⚠ 〔`P4` 2026-09-21〕先前这一行写着「`scan_tree!` 摘掉本文件 ⇒ 104 进扫描」——
/// 那一刀**在这一处不生效**，而且本文件住 `tests/bridge/`、根本不在这棵树里。
///
/// 低于它说明**遍历坏了**，不是代码变干净了 —— 那是最坏的一种绿。
const TREE_FLOOR: usize = 90;

/// 针：**运行时拼**，免得命中本文件自己的散文（本模块头注里逐字写着这些形态）。
///
/// 三根针的形状理由逐条住模块头注「针怎么不重蹈 `.active(`」那一节，改这里之前先读那一段。
fn needle_active() -> String {
    // 不带前导点（三种写法一网打尽）+ 带闭括号（不打中 `watcher.rs` 那个同名函数）
    format!("acti{}()", "ve")
}

fn needle_kind_literal() -> String {
    format!("for_kind(Agent{}::", "Kind")
}

fn needle_for_kind() -> String {
    format!("for_{}(", "kind")
}

fn facade_needles() -> Vec<String> {
    KIND_ERASING_FACADES
        .iter()
        .map(|(name, _)| format!("{name}("))
        .collect()
}

/// 整棵 `src/bridge/src` 的 `(相对路径, 生产段)`。
///
/// 走 `scan_tree!` 是 `scanning_guard_registry` 那条递减棘轮逼的（它逐字禁止新写的
/// 扫描型判据裸遍历 —— 那族病的默认结局是恒绿）。
///
/// ⚠ 〔`P4` 2026-09-21〕先前这里写着「`scan_tree!` **按构造摘掉调用者自己**」——
/// 那一刀**在这一处不生效**（判据一律由 `#[path]` 挂载 ⇒ `file!()` 给的是带 `..`
/// 的折返路径 ⇒ 后缀比不命中，
/// `the_scan_tree_macro_no_longer_excludes_its_caller_after_the_split` 守着这件事）。
/// 本条不在自己的语料里靠两样：**住址**（判据住 `tests/bridge/`，扫的是 `src/bridge/src`）
/// ＋ 三根**运行时拼**的针。
fn sources() -> Vec<(String, String)> {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files: Vec<(PathBuf, String)> = guard_core::scan_tree!(&src_dir, &["rs"]);
    files
        .into_iter()
        .map(|(p, s)| {
            let rel = p
                .strip_prefix(&src_dir)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            (rel, guard_core::production_code(&s))
        })
        .collect()
}

/// 一行落在哪张脸上（`None` = 不是耦合点）。**四张脸互斥**，判定序在下面逐条注明。
///
/// ⚠ **定义行一律不算** —— `pub fn active()` / `pub fn records_dir(…)` 那几行本身
/// 就是接口，不是「谁在用它」。判「是不是定义行」按 `fn ` 打头（`production_code`
/// 之后缩进仍在，所以要 `trim_start`）。
fn face_of(line: &str) -> Option<Face> {
    let t = line.trim_start();
    let is_def =
        t.starts_with("fn ") || t.starts_with("pub fn ") || t.starts_with("pub(crate) fn ");
    // ① `active()` 优先：`session_id_from_path_with(active().layout(), p)` 这种
    //    一行里两样都有的，算「说不出指哪个 agent」那张脸（更根本的那一层）。
    if !is_def && guard_core::contains_word(line, &needle_active()) {
        return Some(Face::Active);
    }
    // ② 门面。⚠ `records_dir_for(` / `session_id_from_path_with(` **不算** ——
    //    针带闭括号 + `contains_word` 的右边界让它们结构上打不中（那是好方向）。
    if !is_def
        && facade_needles()
            .iter()
            .any(|n| guard_core::contains_word(line, n))
    {
        return Some(Face::Facade);
    }
    // ③ 字面量实参。定义行不可能长这样，不必再判一次。
    if guard_core::contains_word(line, &needle_kind_literal()) {
        return Some(Face::KindLiteral);
    }
    // ④ 剩下的 `for_kind(` = 传运行时 kind。⚠ `parse_for_kind(` 是 parser 的另一个
    //    派发器、不是适配层 —— `contains_word` 的**左边界**把它挡在外面（前一个字符是 `_`）。
    if !is_def && guard_core::contains_word(line, &needle_for_kind()) {
        return Some(Face::RuntimeDispatch);
    }
    None
}

/// 逐 `(文件, 脸)` 的实得处数（**行粒度**：一行里出现两次只算一次）。
///
/// 判据①②③共用这一份抽取 —— 抽两遍必然漂开。
fn measured() -> Vec<(String, Face, usize)> {
    let mut got: Vec<(String, Face, usize)> = Vec::new();
    for (rel, prod) in sources() {
        for face in [
            Face::Active,
            Face::Facade,
            Face::KindLiteral,
            Face::RuntimeDispatch,
        ] {
            let n = prod.lines().filter(|l| face_of(l) == Some(face)).count();
            if n > 0 {
                got.push((rel.clone(), face, n));
            }
        }
    }
    got.sort();
    got
}

/// ★ **抽取器自检**：扫的真是那棵树，而且针真的够得着东西。
///
/// 没有这一条，下面三条都是「找不到东西就绿」的形状 —— 遍历坏了 / 针拼成空串
/// 都会**安静地全绿**，而那正是本件 `§0c①` 现打的那个失败形（读数 0 看起来像问题没了）。
#[test]
fn the_scan_actually_reads_the_monitor_tree_and_the_needles_reach() {
    let files = sources();
    assert!(
        files.len() >= TREE_FLOOR,
        "只扫到 {} 份 `.rs`（下界 {TREE_FLOOR}）—— **遍历坏了**，不是代码变干净了。\n\
             ⚠ 这一格是本条最坏的失败形：人群塌成空集，下面三条判据全绿。",
        files.len()
    );
    // 本条的锚点住 `adapter.rs`：接口自己那一份必须在人群里，且真的有那个自由函数。
    let adapter = files
        .iter()
        .find(|(rel, _)| rel == "adapter.rs")
        .map(|(_, s)| s.clone())
        .expect("`adapter.rs` 不在扫描面里 —— 遍历或路径剥法坏了");
    let define = format!("pub fn acti{}()", "ve");
    assert!(
        guard_core::contains_word(&adapter, &define),
        "`adapter.rs` 里找不到 `{define}` —— 那个自由函数被改名或删了，\
             本条的针从此**恒零命中**，而零命中在这一族里长得跟「已经收干净了」一模一样。"
    );
    assert!(
        !needle_active().is_empty() && !needle_for_kind().is_empty(),
        "针拼出了空串 —— `contains_word` 对空 needle 返 false ⇒ 本条整条在空转"
    );
    let got = measured();
    assert!(
        !got.is_empty(),
        "四张脸一处都没量到 —— 抽取坏了（今天盘上是 {} 条登记）",
        AGENT_COUPLING_SITES.len()
    );
}

/// ① **逐文件逐脸相等** —— 多一处红、**少一处也红**。
///
/// 形态照后端侧 `agent_locality_guard::general_layer_adapter_call_sites_are_enumerated_one_by_one`。
/// 只比总数的话「从 A 文件挪一处到 B 文件」会全绿，而那是**把改动面藏起来，不是消掉**。
#[test]
fn agent_coupling_sites_are_enumerated_one_by_one() {
    let got = measured();
    let mut want: Vec<(String, Face, usize)> = AGENT_COUPLING_SITES
        .iter()
        .map(|(f, face, n, _)| ((*f).to_string(), *face, *n))
        .collect();
    want.sort();
    assert_eq!(
        got, want,
        "\n桌面侧「通用层认得出某个 adapter」的地方与登记表对不上。\n\
             实得：{got:?}\n登记：{want:?}\n\
             ⚠ **多出来的那处 = 又一个「加 agent 时要回来改」的地方**：先登记 + 写清它在向\n\
             适配层要什么，然后问一句 —— 这一处能不能改成把 kind 穿进来（门面都有 `*_for` 兄弟）？\n\
             ⚠ 少掉的那处 = 真收进接口了，恭喜，同轮摘登记并把 `COUPLING_BASELINE` 调下来\n\
             （这张表短了是好事）。\n\
             ⚠ `RuntimeDispatch` 那张脸**方向相反**：它变多是好事，但也要登记 ——\n\
             不登记就分不清「多了一处好的」和「多了一处坏的」。"
    );
    for (f, face, n, what) in AGENT_COUPLING_SITES {
        assert!(*n > 0, "{f} 的 {face:?} 登记了 0 处 —— 那它不该在表里");
        assert!(
            what.trim().len() >= 10,
            "{f} 的 {face:?} 没写「它在向适配层要什么」—— 而那正是收接口那轮要的规格"
        );
    }
}

/// ② **递减棘轮**：前三张脸的总数只许往下走。
///
/// ⚠ 第四张脸（`RuntimeDispatch`）不在这个数里 —— 见模块头注。
#[test]
fn the_coupling_total_only_ever_goes_down() {
    let total: usize = measured()
        .iter()
        .filter(|(_, face, _)| face.on_the_ratchet())
        .map(|(_, _, n)| *n)
        .sum();
    assert!(
        total <= COUPLING_BASELINE,
        "\n桌面侧「加一个 agent 通用层要改几处」从 {COUPLING_BASELINE} **涨到了** {total}。\n\
             ⚠ 这个数只该往下走。件计划现打过一次真涨（`history.rs` 的 `relay_prefix_for`\n\
             在 08-28 审计点名「写面没有分派」的**同一天**长出一处，十天没有任何东西红过）——\n\
             本条就是为了让下一次那样写的时候会红。\n\
             ⚠ **不许把 `COUPLING_BASELINE` 调上去让今天好过。**"
    );
    // 反向：棘轮不许被悄悄放松成一个够不着的天花板。
    let registered: usize = AGENT_COUPLING_SITES
        .iter()
        .filter(|(_, face, _, _)| face.on_the_ratchet())
        .map(|(_, _, n, _)| *n)
        .sum();
    assert_eq!(
        registered, COUPLING_BASELINE,
        "登记表前三张脸合计 {registered}，而棘轮写着 {COUPLING_BASELINE} —— \
             两个数漂开了。真收进接口了就**同轮**把两处一起调低；\
             ⚠ 只调棘轮不摘登记，本条就从「只许降」退化成一个永远够不着的天花板。"
    );
}

/// ③ **每一个登记的门面都真的抹除了 kind** —— 手写闭集的对价。
///
/// [`KIND_ERASING_FACADES`] 是手写的，加一个新门面不会自动进人群。这一条钉住表与事实
/// 不漂：每条必须真在 `adapter.rs` 里，且**真的**经 `active()`（或另一个已登记门面）取适配器。
#[test]
fn every_registered_facade_really_erases_the_kind() {
    let files = sources();
    let adapter = files
        .iter()
        .find(|(rel, _)| rel == "adapter.rs")
        .map(|(_, s)| s.clone())
        .expect("`adapter.rs` 不在扫描面里");
    // 〔LOC1b · 第四波 4D〕地板 5 → 1：四个门面零调用方删了（冷读 · 判活 · 搜索都改问本机后端），表里只剩 `tasks_dir`。
    // 〔MIG-3b〕地板 1 → 0（整条删了）：最后那个门面（任务追踪目录）零调用方删了 —— 门面真的退役了，`AGENT_COUPLING_SITES` 的 `Facade` 行同轮摘；
    //   「表是不是被削了」那一问今天由 `the_detectors_catch_synthetic_violations` 里「表是空的」那一格接住。
    for (name, what) in KIND_ERASING_FACADES {
        assert!(
            what.trim().len() >= 6,
            "门面 `{name}` 没写「它替调用者定死了什么」"
        );
        let define = format!("fn {name}(");
        assert!(
            guard_core::contains_word(&adapter, &define),
            "门面 `{name}` 在 `adapter.rs` 里找不到（找的是 `{define}`）—— \
                 它被改名 / 挪走 / 删了，而**本表没跟着改** ⇒ 那一根针从此恒零命中。"
        );
        // 它的实现体真的抹 kind：调 `active()`，或调另一个已登记门面（「合成门面」那形）。
        let body = body_of(&adapter, &define);
        let via_active = guard_core::contains_word(&body, &needle_active());
        let via_peer = KIND_ERASING_FACADES
            .iter()
            .any(|(peer, _)| peer != name && guard_core::contains_word(&body, &format!("{peer}(")));
        assert!(
            via_active || via_peer,
            "门面 `{name}` 的实现体里既没有 `{}`、也没调另一个已登记门面 —— \
                 它**不再抹除 kind 了**（多半是好事：签名里加了 kind？）⇒ 同轮把它从本表摘掉，\
                 并把 `AGENT_COUPLING_SITES` 里对应的行一起改。实得体：{body:?}",
            needle_active()
        );
    }
}

/// 从 `define` 那一行起、到下一个列 0 的 `}` 为止的函数体（粗，够本条用）。
///
/// ⚠ 粗在哪里、为什么够：它只服务于「这个门面调没调 `active()`」这一问，
/// 而归错的表现是**红在名字对不上**（上面那条会说「实得体」是什么），不是静默放行。
fn body_of(src: &str, define: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in src.lines() {
        if !inside {
            if guard_core::contains_word(line, define) {
                inside = true;
            }
            continue;
        }
        if line.starts_with('}') {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// ④ **反向夹具：判定真的会红** —— 件计划 `§3` 刀 D2 的 `C` 那一格。
///
/// 上面三条都是「找不到东西就绿」的形状。针拼坏了（拼成 `.active(`、去掉边界、
/// 去掉闭括号）会**安静地全绿**，而 `§0c①` 现打证明过那个失败形长得跟
/// 「问题已经没有了」一模一样。⇒ 每一道处置在这里各有一格正、一格反。
///
/// 形状照后端侧 `agent_locality_guard::the_s4b_detectors_catch_synthetic_violations`。
#[test]
fn the_detectors_catch_synthetic_violations() {
    // ── 处置 1：不带前导点 ⇒ 三种写法都认得 ────────────────────────
    for form in [
        format!("    let agent = crate::adapter::acti{}();", "ve"),
        format!("    let agent = adapter::acti{}();", "ve"),
        format!(
            "    data_root.join(acti{}().layout().sessions_subdir)",
            "ve"
        ),
    ] {
        assert_eq!(
            face_of(&form),
            Some(Face::Active),
            "针认不出这一种调用写法 —— 它此刻对这一形是空转的：{form:?}\n\
                 ★ 这一格就是 `§0c①` 那个活体的反向钉：**带前导点的针（`.active(`）\
                 结构上够不着自由函数**，而它给出的读数是 0，看起来像问题已经没有了。"
        );
    }
    // ── 处置 2：带闭括号 ⇒ `watcher.rs` 那个同名函数不许被数进来 ──
    let other_active = "        if !active(&session_id) {";
    assert_eq!(
        face_of(other_active),
        None,
        "针打中了 `watcher.rs` 里那个**同名**的会话活性函数 —— 那是假阳。\n\
             ★ 「同一个词装了两件事」：裸 `active(` 会把它一起数进来。针必须带闭括号。"
    );
    // ── 处置 3：匹配单位带边界 ⇒ 撑大的名字不许被数进来 ────────────
    let stretched = format!("        map.snapshot_acti{}()", "ve");
    assert_eq!(
        face_of(&stretched),
        None,
        "针打中了 `snapshot_active()` —— 那是「匹配单位比事实小」那一族的假阳。\n\
             ★ 这是**现打的活体**：不带边界时它把 `lib.rs` 那一行数成一处耦合，读数 12 虚高成 13。"
    );
    // ── 定义行不算（那是接口自己，不是「谁在用它」）───────────────
    for def in [
        format!("pub fn acti{}() -> &'static dyn AgentAdapter {{", "ve"),
        "pub fn tasks_dir(data_root: &Path) -> Option<PathBuf> {".to_string(),
    ] {
        assert_eq!(face_of(&def), None, "定义行被数成了调用点：{def:?}");
    }
    // ── 门面：正向认得出，而 per-kind 兄弟**不许**被打中（那是好方向）──
    // 〔MIG-3b〕门面表空了（最后那个「任务追踪目录」零调用方删了）⇒ 门面这张脸整张退役：那一句旧调用不再被认成门面。
    //   表里哪天再长出一个门面，这一格要换回「正向认得出」的正控。
    assert!(
        KIND_ERASING_FACADES.is_empty(),
        "门面表又长出来了 —— 把这一格换回「门面针认得出真调用」的正控"
    );
    assert_eq!(
        face_of("    let tasks_dir = crate::adapter::tasks_dir(&claude_dir);"),
        None,
        "门面表是空的，门面针却还打中了东西 —— 针不是从表里来的"
    );
    for good in [
        "    let root = crate::adapter::records_dir_for(kind, &dr);",
        "    crate::adapter::session_id_from_path_with(layout, &target)",
    ] {
        assert_ne!(
            face_of(good),
            Some(Face::Facade),
            "门面针打中了 per-kind 那个**兄弟** —— 那正是本条希望人们改成的写法，\n\
                 打中它就是在训练人绕过判据（假阳比漏判更贵）：{good:?}"
        );
    }
    // ── 字面量 vs 运行时：两张脸不许互串 ──────────────────────────
    let literal = format!("    let codex = for_kind(Agent{}::Codex);", "Kind");
    assert_eq!(
        face_of(&literal),
        Some(Face::KindLiteral),
        "字面量实参没被认成写死 —— `§0c②` 逐字：那不是分派，那是写死"
    );
    assert_eq!(
        face_of("    let root = crate::adapter::for_kind(kind).data_root();"),
        Some(Face::RuntimeDispatch),
        "传运行时 kind 的那一形没被认出来 —— 好方向也要登记，否则分不清多的是好的还是坏的"
    );
    // ── `parse_for_kind(` 是 parser 的另一个派发器，不是适配层 ────
    let parser = "        let rec = match crate::parser::parse_for_kind(kind, trimmed) {";
    assert_eq!(
        face_of(parser),
        None,
        "打中了 `parse_for_kind(` —— 那是 parser 的派发器，不是适配层。\n\
             ★ `contains_word` 的**左边界**（前一个字符是 `_`）是唯一挡住它的东西；\
             裸 `contains` 在这里会把 7 行噪音数进来（件计划 `§0c②` 现打）。"
    );
}
