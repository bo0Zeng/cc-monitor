//! `K-W1B D2`：**桌面侧「通用层认得出某个 agent」的地方逐条登记** —— 那一半今天零判据。
//!
//! # 名字的来历 —— 它**不叫** `agent_boundary_guard`，而那是刻意的
//!
//! 本模块初版就叫那个名字（派工时的预批名），落地当轮撞出一处真伤害，PM 裁定改名。
//! 把来历留在这儿，是为了让下一个人**不要再把它改回去**：
//!
//! 1. **daemon 那棵树里已经有一个 `agent_boundary_guard`**
//!    （`remote-daemon-proto/src/agent_boundary_guard.rs`，`S1`：通用层不许知道任何 agent 的
//!    名字与文件格式，人群由它的 `CORE_FILES` **opt-in** 列举）。**本模块不是它的桌面版**：
//!    那一条判「通用层提没提 agent 的名字/布局」，本条数「通用层认不认得出**具体哪一个**
//!    adapter」—— 形态照的是 daemon 侧**另一条**，`agent_locality_guard` 的判据④
//!    （`general_layer_adapter_call_sites_are_enumerated_one_by_one`）。
//! 2. **同名会当场把一处既有引用指错**：`src-tauri/src/ssh_source.rs` 里有一处逐字写着
//!    「`agent_boundary_guard::FROZEN_COMPAT`」，指的是 **daemon 那一个**。本模块若同名，
//!    那一处在本树里就变成一个**指得到、但指错**的名字（本模块没有 `FROZEN_COMPAT`）。
//!    ⚠ 那一处**不用改**：改名之后它在本树里重新变成唯一解 —— 只指 daemon 那个模块，而那是对的。
//! 3. **`K-W3` 正要把两棵树并成一个 crate** ⇒ 同名同 crate 时必须改，早改比晚改便宜。
//!
//! 名字按 monitor 侧「清账 + 递减棘轮」那族的既有命名取（`local_read_surface_registry` ·
//! `launcher_identity_registry` · `session_name_registry` · `exec_site_registry`），
//! 并说出它**数什么**（派发/调用点），不用含糊的「boundary」。
//!
//! # 它数什么 —— 四张脸，前三张该压到零，第四张方向相反
//!
//! 桌面侧的机制与 daemon 侧**不同**：这边**有** trait（`adapter::AgentAdapter`，9 个方法）
//! ＋ `for_kind()` 派发。⇒ 「直呼 `agents::<名>::`」那根针在这边一处都打不中，
//! 而耦合**换了形状**住在别处。四张脸，逐张一句话：
//!
//! | 脸 | 一行长什么样 | 为什么它是耦合 | 方向 |
//! |---|---|---|---|
//! | [`Face::Active`] | `crate::adapter::active()` · `active().layout()` | 调用点**说不出**它指哪个 agent —— 「当前活跃的那个」今天恒等于 Claude | 压到零 |
//! | [`Face::Facade`] | `crate::adapter::records_dir(&d)` · `has_record_ext(p)` | 门面替调用者把 kind 写死了（每一个都有 per-kind 兄弟 `*_for` / `*_with`）⇒ **调用点连「我要活跃那个」都没说** | 压到零 |
//! | [`Face::KindLiteral`] | `for_kind(AgentKind::Codex)` | 拿**字面量**当实参 —— 那不是分派，那是写死 | 压到零 |
//! | [`Face::RuntimeDispatch`] | `for_kind(kind)` | 传的是**运行时** kind ⇒ 接口正在被正确使用 | **不上棘轮** |
//!
//! ★ 第四张脸单独登记、**刻意不上棘轮**：它是好方向。混进同一个计数，棘轮就会奖励
//! 「把真分派改回写死」。这条分家的理由与 daemon 侧 `AGENT_REGISTRY_SITES` 的头注同族
//! （两类性质相反的东西不许共用一个数），但方向反过来。
//!
//! # 为什么是**逐文件相等**，而不是只比总数
//!
//! daemon 侧那条判据的头注已经写死并实测过：只比总数的话「从 A 文件挪一处到 B 文件」
//! 会**全绿**，而那是把改动面藏起来、不是消掉。⇒ 本条也是**多一处红、少一处也红**。
//!
//! ⚠ **本条比 daemon 侧那条少一个洞**：那边为了不稀释靶子，把注册表文件整个**扣出人群**，
//! 于是得再立一条判据⑦（注册表文件处数钉死 = `REGISTRY.len()`）当对价，堵「挪进去刷数」。
//! 本条**一个文件都不扣** —— `adapter.rs`（接口自己那一份）也在人群里，按文件分行；
//! 唯一不算的是**定义行**（`pub fn active()` / `pub fn records_dir(…)` 那几行本身就是接口）。
//! ⇒ 把通用层的一处挪进 `adapter.rs`：那一行照样被数，总数不掉，两张登记同时红。**不需要对价。**
//!
//! # 🔴 针怎么不重蹈 `.active(` 的覆辙（件文件 `§0c①` 那个活体）
//!
//! 那次的读数是 **0**，而 0 看起来像「问题已经没有了」—— 实际是 `active` 是**自由函数**、
//! 调用形一律 `adapter::active()`（前面是 `::`，不是 `.`）⇒ **带前导点的针结构上够不着它**。
//!
//! 本条的三道处置，缺一道就会退回那个失败形：
//!
//! 1. **针不带前导点**，只写 `active()` —— 全路径 `crate::adapter::active()`、
//!    相对 `adapter::active()`、模块内裸 `active()` **三种写法一网打尽**；
//! 2. **带闭括号**（`active()` 而不是 `active(`）—— `watcher.rs` 里另有一个**同名**的
//!    自由函数 `active(&session_id)` 判**会话活性**，与适配层无关。
//!    「同一个词装了两件事」这一族在本仓有账：裸 `active(` 会把它一起数进来，那是假阳；
//! 3. **匹配单位带边界**（走 [`guard_core::contains_word`]，`needle_anchor_registry` 头注
//!    逐字要求的三个原语之一）—— 现打的活体：不带边界时 `map.snapshot_active()`
//!    会被数成一处耦合（`lib.rs` 那一行），读数 12 虚高成 13。
//!
//! ⇒ 而这三道**都由 [`tests::the_detectors_catch_synthetic_violations`] 反向钉着**：
//! 把针拼坏成 `.active(`、或去掉边界、或去掉闭括号，那条会**红在「针空转」上**，
//! 不是安静地全绿。形状照 daemon 侧的 `the_s4b_detectors_catch_synthetic_violations`。
//!
//! # ⚠ 诚实边界（四条，写在这里而不是只写在件计划里）
//!
//! 1. **改名/别名躲得过**：`use crate::adapter::active as f;` 然后调 `f()` ⇒ 本条一处都不红。
//!    今天全树**零处**这种写法（本条落地时现打），但它**没有判据看着**，是真开着的洞。
//! 2. 认的是**字面形态**，不是语义。有人自己 `PathBuf::from(root).join("projects")`
//!    绕开门面 ⇒ 本条看不见。那一格归 `local_read_surface_registry`（**那本账数的是
//!    「桌面端还在自己读 `~/.claude`」，与本条是两把尺子，别互相报数** —— 件计划 `§2` 逐字）。
//! 3. `production_code` 剥掉注释与 `#[cfg(test)] mod` ⇒ **文档与测试段里怎么写都不红**
//!    （刻意的：本模块自己的散文里就有这些形态），代价是「只在测试里写死一个 agent」逮不到。
//! 4. 门面表 [`tests::KIND_ERASING_FACADES`] 是**手写的闭集**，加一个新门面不会自动进人群。
//!    对价是 [`tests::every_registered_facade_really_erases_the_kind`]：每一个登记的门面
//!    必须真的在 `adapter.rs` 里、且真的经 `active()` 取适配器 —— 表与事实漂开会红。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
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
    /// ⚠ `adapter.rs` 自己那几行**也在表里** —— 见模块头注「本条比 daemon 侧那条少一个洞」。
    const AGENT_COUPLING_SITES: &[(&str, Face, usize, &str)] = &[
        (
            "account_usage.rs",
            Face::Active,
            1,
            "用量探针要 `nested_env_to_scrub` + `default_launcher` ⇒ 拿「活跃那个」的键表与启动器",
        ),
        (
            "adapter.rs",
            Face::Active,
            6,
            "接口自己那 6 个**门面**的实现体（`records_dir` / `liveness_dir` / `tasks_dir` / \
             `has_record_ext` / `is_skipped_path` / `session_id_from_path`）—— 每一个都替调用者\
             把 kind 写死。⚠ 它们**不是**适配层的实现，是给通用层用的门面，\
             退役方式与通用层那几处一样是「把 kind 穿进来」（各自已有 `*_for` / `*_with` 兄弟）",
        ),
        (
            "adapter.rs",
            Face::Facade,
            1,
            "`is_record_file` 把两个抹除 kind 的门面**再合成一个门面** —— 同一处耦合，换了层皮",
        ),
        (
            "adapter.rs",
            Face::KindLiteral,
            2,
            "`active()` 的定义体写死 Claude（**这一处是刻意的**，它就是「活跃那个是谁」的答案）\
             + `enabled_kinds()` 探 Codex 的会话根（也是刻意的，那是发现逻辑）。\
             ⇒ 这两处**不是欠账，是接口的边界条件**；登记它们是为了「加第三家时这里必改」有人知道",
        ),
        (
            "history.rs",
            Face::Active,
            3,
            "resume flag / 默认启动器 / 中转注入的 agent id ⇒ **两处读面 + 一处写面**。\
             ⚠ 写面那一处（`relay_prefix_for` 里的 `active().id()`）是 08-28 审计\
             点名「写面没有分派」之后**又长出来的**，十天没有任何东西红过 —— 本条买的就是它",
        ),
        (
            "history.rs",
            Face::Facade,
            11,
            "会话记录根 ×4（`records_dir`）+「这个文件是不是会话记录」×6（`has_record_ext`）\
             + 从路径取 sid ×1 ⇒ 全树最重的一处，而它一根针都不在件计划 `§0` 的四把尺子里",
        ),
        (
            "history.rs",
            Face::KindLiteral,
            2,
            "`enumerate_codex_sessions` 里两处写死 `AgentKind::Codex`（取数据根 + 取 layout）\
             —— 通用层里的一条 Codex 专属分支",
        ),
        (
            "lib.rs",
            Face::Active,
            2,
            "起会话面：resume 前清洗嵌套 env（`nested_env_to_scrub`）+ setup 里取数据根",
        ),
        (
            "lib.rs",
            Face::Facade,
            3,
            "setup 里一次性拼出 records / liveness / tasks 三个目录 ⇒ 整条发现链的起点在这里定死",
        ),
        (
            "search.rs",
            Face::Facade,
            3,
            "本地全文索引的扫描面：记录根 + 记录判定 + 从路径取 sid。\
             ⚠ `search.rs` 的退役条件另有裁定（`K7` 乙块逐字：daemon 侧没索引，迁它是拿性能换账面）\
             —— 但**耦合处数照样要数**，两件事",
        ),
        (
            "tasks.rs",
            Face::Facade,
            1,
            "任务追踪目录（`tasks_dir`）—— 该 agent 没这个概念时返 `None`，而调用点按 Claude 兜底",
        ),
        (
            "watcher.rs",
            Face::Facade,
            5,
            "流式 watcher 的过滤器：是不是记录文件 ×3 + 从路径取 sid ×2。\
             ⚠ 本文件里另有一个**同名**自由函数 `active(&session_id)` 判会话活性，\
             与适配层无关 —— 针带闭括号正是为了不把它数进来（见模块头注第 2 道处置）",
        ),
        // ── 第四张脸：方向相反，登记但不上棘轮 ──────────────────────────
        (
            "adapter.rs",
            Face::RuntimeDispatch,
            2,
            "`records_dir_for` 与 `records_roots` —— per-kind 那一半，**接口正在被正确使用**",
        ),
        (
            "history.rs",
            Face::RuntimeDispatch,
            2,
            "按路径判出 kind 之后取根与 layout（`kind_of_path` → `for_kind(kind)`）—— 好方向",
        ),
    ];

    /// 立表那天的读数（前三张脸的总数）。**只许降。**
    ///
    /// ⚠ 它是「桌面侧那一半差多少」的头条数字。件计划 `§0b` 此前登记的是
    /// 「`active()` 硬编码 6 处 + `for_kind` 真分派 4 处」——**那个读数只盖住 `active()` 那一张脸，
    /// 而门面那一族（今天 24 处）一根针都没数到**。本条立表时把四张脸一起量了。
    ///
    /// ⚠ 与 daemon 侧那个 27 **不是同一把尺子，不许相加**（两侧机制不同：那边直呼
    /// `agents::<名>::`，这边走 trait + 门面）。要比较请各自报各自的尺子。
    const COUPLING_BASELINE: usize = 40;

    /// **抹除 kind 的门面**：`adapter.rs` 里那几个「替调用者把 agent 写死」的自由函数。
    ///
    /// 判准是一条**可复述的规则**，不是逐个品味：**它的实现体里调 `active()`，
    /// 而它的签名里没有 kind**（每一个都有一个带 kind 的兄弟 `*_for` / `*_with`）。
    ///
    /// ⚠ 手写闭集 ⇒ 加一个新门面不会自动进人群。对价是
    /// [`every_registered_facade_really_erases_the_kind`]：每一条必须真在 `adapter.rs` 里、
    /// 且那个函数**真的**经 `active()` 取适配器。
    ///
    /// ⚠ `is_record_file` 自己不调 `active()`，它调另外两个门面 —— 单独一行说明，
    /// 那条判据按「调 `active()` **或** 调另一个已登记门面」放行它。
    const KIND_ERASING_FACADES: &[(&str, &str)] = &[
        ("records_dir", "会话记录目录（`<root>/projects`）"),
        ("liveness_dir", "活性 pidfile 目录（`<root>/sessions`）"),
        ("tasks_dir", "任务追踪目录（`<root>/tasks`）"),
        ("has_record_ext", "「这个扩展名是不是会话记录」（`jsonl`）"),
        ("is_skipped_path", "「这个路径要不要跳过」（`subagents`）"),
        (
            "is_record_file",
            "上面两个的合成 —— **它自己不调 `active()`**，靠两个门面间接抹除",
        ),
        (
            "session_id_from_path",
            "从记录文件路径取 sid（Claude = file_stem）",
        ),
    ];

    /// 人群下界：`src-tauri/src` 今天 105 份 `.rs`（`scan_tree!` 摘掉本文件 ⇒ 104 进扫描）。
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

    /// 整棵 `src-tauri/src` 的 `(相对路径, 生产段)`。
    ///
    /// `scan_tree!` **按构造摘掉调用者自己**（`scanning_guard_registry` 的递减棘轮
    /// 逐字禁止新写的扫描型判据裸遍历 —— 那族病的默认结局是恒绿）。
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
    /// 形态照 daemon 侧 `agent_locality_guard::general_layer_adapter_call_sites_are_enumerated_one_by_one`。
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
        assert!(
            KIND_ERASING_FACADES.len() >= 5,
            "门面表只剩 {} 条 —— 少于立表时的规模，表被削了还是门面真的退役了？\
             真退役了就同轮把 `AGENT_COUPLING_SITES` 的 `Facade` 行一起改",
            KIND_ERASING_FACADES.len()
        );
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
            // 它的实现体真的抹 kind：调 `active()`，或调另一个已登记门面（`is_record_file` 那形）。
            let body = body_of(&adapter, &define);
            let via_active = guard_core::contains_word(&body, &needle_active());
            let via_peer = KIND_ERASING_FACADES.iter().any(|(peer, _)| {
                peer != name && guard_core::contains_word(&body, &format!("{peer}("))
            });
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
    /// 形状照 daemon 侧 `agent_locality_guard::the_s4b_detectors_catch_synthetic_violations`。
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
            "pub fn records_dir(data_root: &Path) -> PathBuf {".to_string(),
        ] {
            assert_eq!(face_of(&def), None, "定义行被数成了调用点：{def:?}");
        }
        // ── 门面：正向认得出，而 per-kind 兄弟**不许**被打中（那是好方向）──
        assert_eq!(
            face_of("    let projects_dir = crate::adapter::records_dir(&claude_dir);"),
            Some(Face::Facade),
            "门面针认不出真调用 —— 它此刻是空转的"
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
}
