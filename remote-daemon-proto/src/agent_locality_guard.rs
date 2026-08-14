//! `S2`：**一个 agent 的知识只许有一个住址，通用层里认它的地方逐条登记**〔用@08-14〕。
//!
//! # 它守的是什么 —— 病灶不是"散"，是"数不清"
//!
//! `S2` 开工前实测：Codex 的知识在三处，而**三处长得完全不一样**：
//! `observe/codex.rs` 是抽取器、`observe/usage_query.rs` 里是
//! `aggregate(<home>).and_then(|()| aggregate_codex())`、
//! `control/resolve_query.rs` 里是 `spec.agent_kind.trim() == "codex"`。
//! grep 出其中任意一处，**都找不到另外两处**。
//!
//! ⇒ 「接第三个 agent 要改哪几处」这份清单，今天只住在人的脑子里。
//! 本模块把它变成**四张表** —— 而**表是会红的**。
//!
//! # 四条判据，分别守四种泄漏
//!
//! ## ① 格式/布局知识只许住 `agents/<某个名字>/`
//!
//! ⚠ **`S3` 把这条从"按 agent 分针"改成了"按住址分区"** —— 这是本判据形态上最要紧的一次变化：
//!
//! `S2` 时它写死 `HOME = "agents/codex/"`，针必须**专有**（能答"这是谁的知识"）。
//! 代价是**含糊的词全用不了**：`sessions/` 两个 agent 各用各的（Claude 是 pidfile 目录、
//! Codex 是会话记录根）、`.jsonl` 两边都用 —— 它们**确实是 agent 知识**，却因为
//! 指不出主人而被排除在外。
//!
//! `S3` 之后人群改成「**排除所有 agent 家**」，判据要答的问题也跟着换成
//! 「这是不是 agent 知识」——**后者是确定的**。于是 `"sessions"` / `"jsonl"` 这类
//! 含糊的针从此可用，覆盖面反而更大。
//!
//! ★ 一般化的教训：**判据答不了的问题，先看能不能换一个更弱、但够用的问题**。
//!
//! ## ② 通用层的 kind 派发点**逐条登记**
//!
//! 形态照 [`crate::layering_guard`]：那条判据不禁止 `observe → control`，它**数**跨层的边
//! （今天恰好 1 条），逼下一个人把理由也写出来。这里同理 —— `D3` 逐字允许
//! 「agent 维度出现在**值**里」，所以 `agent_kind == "codex"` 是合法的；
//! **不合法的是它出现在第二个地方**（那意味着又一处需要跟着改而没人知道）。
//!
//! ## ③〔`S4b`〕通用层的**标识符**里不许出现 `<agent 名>_dir`
//!
//! `D3` 逐字管的是**协议字段名**。`S4b` 把同一条道理往仓内推一格：
//! `fn run(claude_dir: &Path, …)` 这种签名让「谁是通用层」只能靠人记得 ——
//! 协议干净了，代码里仍然是一句 agent 名。
//!
//! ⇒ 通用层的 home 参数一律叫 `agent_home`（`S4b` 改了 8 个文件；生产段 `claude_dir` 64 行 → 3 行）。
//! 例外只有**冻结的 wire 字段名**（Rust 侧标识符必须与线上字段名同名，改名 = 破坏契约），
//! 逐条登记进 [`tests::AGENT_NAMED_WIRE_FIELDS`]，**每条带解锁条件、条数有天花板**。
//!
//! ## ④〔`S4b`〕通用层**直呼某个适配层**的地方逐条登记
//!
//! `G1` 成功标准②逐字：「加一个新 agent 只需新增 `agents/<名>/`，**通用层零改动**」。
//! 今天那句话**还不成立**，而没人数得出差多少。⇒ [`tests::ADAPTER_CALL_SITES`] 把它数出来：
//! 通用层里每一处 `agents::<名>::…` 都是「接第三个 agent 时要回来看一眼」的地方。
//!
//! ★ 这张表同时是**为什么 `watcher`/`accounts_query`/`history_query` 进不了
//! `agent_boundary_guard::CORE_FILES`** 的答案（`S4b` 实测）：把 `claude_dir` 改名之后，
//! 这三个文件在 `S1` 六根针下的残留**全是**这些适配层地址（外加两处日志散文）。
//! ⇒ 卡点不是参数名（`S4b` 已清），是**还没有接口**（`L2`），归 `S6`。
//!
//! ## ⑤〔`S5`〕**注册表**那类被拆了出去 —— 它与④性质相反
//!
//! `S5` 往 `agents/mod.rs` 放了适配层注册表（`REGISTRY`：这个 daemon 认得哪几个 agent）。
//! 它也是「直呼适配层」，但**方向反了**：④ 里的每一条都该被压到零（收进接口），
//! 而注册表那几行是「加一个 agent **本来就该**动的一行」，应该**随 agent 数增长**。
//!
//! 两类混进同一个计数，`S6` 就没法拿那个数当成绩 —— 降了不知道是真收进接口了，
//! 还是有人把调用点挪进注册表文件去刷数。⇒ 拆成 [`tests::AGENT_REGISTRY_SITES`]，
//! ④ 的人群里扣掉它。**扣除的对价**是判据⑦：注册表文件的处数被钉死成 `REGISTRY.len()`
//! （一家一行），藏不进第二样东西。
//!
//! ⚠ 同轮还补了一件事：④ 的针此前**只有全路径** `agents::<名>::`，
//! 而 `agents/mod.rs` 引用两家写的是相对路径 `codex::` —— 新增的注册表**一处都没被数到**。
//! 那是本区第二次「量具的作用域比事实**小**」（第一次是 `S4b §0b-2`）。针已扩到两种写法，
//! 实测扩针**不改动任何既有文件的读数**（8 文件 / 27 处一字未变）。
//!
//! # ⚠ 诚实边界（四条，都写在这里而不是只写在计划里）
//!
//! 1. 判据认的是**字面量**，不是语义。派发点里的人直接写
//!    `format!("{base} resume {sid}")` 仍然不会红 —— 堵死它要等 `S3` 凑齐两个实现后立接口（`D4`）。
//! 2. 只覆盖 **codex**。Claude 那半（`watcher`/`accounts_query`/`history_query` 里的
//!    `projects/`、`.jsonl`、账号布局）今天**一处都没被这条判据管**，归 `S3`。
//! 3. `production_code` 剥掉注释与 `#[cfg(test)] mod` ⇒ **文档与测试里怎么写都不红**。
//!    这是刻意的（判据自己的散文里就有这些词），代价是"只在测试里泄漏格式知识"逮不到。
//! 4. 判据③的针是 `<agent 名>_dir` 这一形，**只认得已知的两个名字**（同 `S1` 的 `4a`）。
//!    有人在通用层写 `claude_home` / `gemini_root`，它一条都不会红。
//!    今天的兜底是判据④：那种代码迟早要去调某个 `agents::<名>::`，那一步会被数出来。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// **所有** agent 家的前缀（相对 `src/`）。加一个 agent = 加一行。
    ///
    /// 扫描时整体排除它们 —— 判据因此不需要回答「这是谁的知识」，
    /// 只需要回答「这是不是 agent 知识」（`S3` §1b-4）。
    const HOMES: &[&str] = &["agents/codex/", "agents/claudecode/"];

    /// 人群下界：agent 家的数量。少于它说明有人把某个 agent 的家删了或改了名，
    /// 而**判据会因此静默放行那一整家的知识** —— 那是最坏的一种绿。
    const HOMES_FLOOR: usize = 2;

    /// **通用层里允许出现 `"codex"` 这个值的地方**，逐条登记 —— 形态照
    /// `layering_guard::ALLOWED_OBSERVE_TO_CONTROL`。
    ///
    /// **加一条之前先回答**：这个 kind 派发非得在这里做吗？能不能让调用方把已经解析好的
    /// agent 送进来？（`resolve_query` 的答案：wire 上收到的就是一个字符串 `agentKind`，
    /// 派发必须发生在最接近入口的地方，而 `CommandPlan` 的骨架两个 agent 共用 ——
    /// 派发点在这里，两条分支各自去自己的适配层取知识。）
    ///
    /// ⚠ 这不是白名单：**表里有几条，就意味着"加一个 agent 要动几处"**。
    /// 它长了就是设计在退化，短不了才说明适配层真的兜住了。
    const KIND_DISPATCH_SITES: &[(&str, &str)] = &[(
        "control/resolve_query.rs",
        "wire 上的 `agentKind` 是个字符串，派发必须在最接近入口处做；两条分支之后共用 CommandPlan 骨架",
    )];

    /// 针：**agent 的目录布局与文件格式**，运行时拼（本文件的散文里就有这些词）。
    ///
    /// 前六根是 `S2` 立的（Codex 专有），后五根是 `S3` 加的 ——
    /// 其中 `"sessions"` 与 `"jsonl"` **两个 agent 都用**，按住址分区之后才敢加。
    ///
    /// ⚠ 带引号的那几根是**刻意的**：`jsonl` 裸词会打中 `jsonl_path` / `read_jsonl` /
    /// `newest_jsonl` 这一大片**变量名与函数名** —— 那是通用的流式读取机器，不是知识。
    /// 同一个教训 `usage.rs::the_usage_kou_jing_has_exactly_one_home` 的头注也记着
    /// （「匹配单位比事实**大**」）。
    fn needles() -> Vec<(String, &'static str)> {
        vec![
            (format!("rollo{}", "ut-"), "会话文件命名"),
            (format!("token_{}", "count"), "用量事件名"),
            (format!("turn_{}", "context"), "记录类型名"),
            (format!("session_{}", "meta"), "记录类型名"),
            (format!("event_{}", "msg"), "记录类型名"),
            (format!(".cod{}", "ex"), "会话目录名"),
            (format!("\"js{}\"", "onl"), "会话文件后缀"),
            (format!("\"proj{}\"", "ects"), "会话目录布局"),
            (format!("\"sessi{}\"", "ons"), "会话目录布局"),
            (format!(".clau{}", "de"), "配置目录名"),
            (format!("CLAUDE_CONFIG{}", "_DIR"), "账号环境变量"),
        ]
    }

    /// **不是 agent 知识、但会被针打中**的地方，逐条登记 + 写清"它到底是什么"。
    ///
    /// ⚠ 这张表与 `S1` 的 `KNOWN_DEBT` **性质相反**，别混：
    /// 那张是「欠账，将来要清零」；**这张是「判据看走眼了，永远留着」**。
    /// 把假阳塞进欠账表的后果是它**永远清不掉**，于是"只会缩短"的那张表里
    /// 长出永久居民 —— 递减棘轮就此失去意义（`S3-Y3`）。
    const NOT_AGENT_KNOWLEDGE: &[(&str, &str, &str)] = &[(
        "control/cc_bus.rs",
        ".claude",
        "这是 **cc-bus 的门牌号**（`~/.claude/skills/cc-bus/scripts`），不是 agent 知识 —— \
         cc-bus 恰好装在那个目录下而已。`cc_bus_boundary_guard` 的头注逐字写着这条分界：\
         「允许**命令的地址**，禁**数据布局**」。同 `S1` 的 `@ccm_sid`、`S2` 的 `sessions/` 那一族。",
    )];

    /// kind 值判别的形状：带引号的 `"codex"`。
    ///
    /// ⚠ **必须带引号**——照 `usage.rs::the_usage_kou_jing_has_exactly_one_home` 头注记下的
    /// 那次教训：第一版只比裸名字，`let is_codex = …` 这种**局部变量名**当场把判据打红。
    /// 判据要认的是「谁在拿这个**值**做判别」，不是「谁提到过这个词」。
    fn kind_literal() -> String {
        format!("\"cod{}\"", "ex")
    }

    /// 〔`S4b`〕**冻结的 wire 字段名**（`文件`, `片段`, 为什么改不动, **解锁条件**）——
    /// 判据③唯一的例外表。
    ///
    /// 这三处都是**同一个 wire 字段名在 Rust 侧的落点**：两处声明 + 一处构造。
    /// serde 直接拿 Rust 标识符当线上字段名（`rename_all` 只改大小写风格），
    /// ⇒ 在这里改名 = **改线上契约**，而两条契约都有仓外消费方（aterm）。
    ///
    /// ⚠ 与 `agent_boundary_guard::FROZEN_COMPAT` **不是一张表，别合并**：
    /// 那张的作用域是「已宣称通用的文件（`CORE_FILES`）里的 agent **字面量**」，
    /// 本张的作用域是「**整棵 `src/`** 里 `<名>_dir` 这一形的**标识符**」。
    /// 两者恰好在 `wire.rs` 重叠一行 —— 那是同一个事实被两个不同问题各问了一次，
    /// 不是重复登记。★ 真正的证据在这里：`control/resolve_query.rs` 那条
    /// **`S4` 当时没看见**（它不在 `CORE_FILES` 里，`S1` 的判据扫不到它），
    /// 是本判据把它逼出来的 —— 两张表的作用域确实不一样。
    ///
    /// 两条纪律同 `FROZEN_COMPAT`：① 每条必须有非空解锁条件；② 条数有天花板且只许降。
    const AGENT_NAMED_WIRE_FIELDS: &[(&str, &str, &str, &str)] = &[
        (
            "wire.rs",
            "claude_dir",
            "`Hello` 帧里今天**真在线上**的目录字段。`S4` 走 additive 迁移（新字段 `homes` \
             承载 agent 维度），这个字段原地冻结 ⇒ 线上字节零变化。",
            "monitor 与 aterm **都**改读 `homes` 之后删掉它。monitor 那半 `S4` 已做完，\
             只剩仓外 aterm。删的那天本条同轮摘登记（下面的幽灵检查会逼着摘）。",
        ),
        (
            "control/resolve_query.rs",
            "claude_dir",
            "`--resolve` 的 **stdin 入参** `ResumeSpec.claudeDir`（`rename_all=camelCase`）。\
             契约与仓外 aterm 冻结在 2026-07-18，改 Rust 标识符 = 改线上字段名。\
             ⚠ 它今天是 `#[allow(dead_code)]`（MVP 不做 pidfile 消解），**但「没用到」不等于\
             「可以改名」** —— 别人在往里发。",
            "aterm 那侧 `ResumeSpec` 不再发 `claudeDir` 之后（或整个 `--resolve` 契约重谈）。\
             与上一条同源：都卡在同一个仓外消费方身上。",
        ),
        (
            "main.rs",
            "claude_dir",
            "上面那个 `Hello.claude_dir` 的**构造点**（`claude_dir: agent_home.to_string_lossy()…`）——\
             左边是冻结的字段名、右边已经是 `S4b` 改过的 `agent_home`。\
             ⚠ 这一行左右刻意不一致，**不是笔误**：字段名归契约，变量名归架构。",
            "随 `wire.rs` 那条一起走：字段删了，这一行自然没了。",
        ),
    ];

    /// 〔`S4b`〕通用层里**直呼某个适配层**的地方（`文件`, 处数, 它在向适配层要什么）。
    ///
    /// # 它数的是什么 —— `G1` 成功标准②今天差多少
    ///
    /// 成功标准②逐字：「加一个新 agent 只需新增 `agents/<名>/`，**通用层零改动**」。
    /// 今天不成立：下面这 8 个文件里的每一处都写死了**某一个** agent 的名字。
    /// `S3` 的形状（「机器留在原地，通过适配层的一次函数调用拿知识」）是对的中间态，
    /// 但它把「加一个 agent 要改哪几处」从**格式知识**挪成了**调用点** —— 数量没有归零。
    ///
    /// ⇒ 本表**就是那份清单**。它长了是设计在退化；`S6` 立起接口之后它应该整体缩短。
    ///
    /// ⚠ 与 [`KIND_DISPATCH_SITES`] 分工：那张数的是「拿 kind **值**做判别」（`D3` 允许的形状，
    /// 今天 1 处）；本张数的是「**不判别、直接写死一个 agent**」（`D3` 管不着，因为它不在协议里）。
    /// 两者加起来才是「接第三个 agent 的改动面」。
    /// ⚠〔`S5`〕**注册表那类不在本表里** —— 见 [`AGENT_REGISTRY_SITES`]。
    /// 本表只装「**该被压到零**」的那一类，`S6` 的靶子就是它的总数。
    const ADAPTER_CALL_SITES: &[(&str, usize, &str)] = &[
        ("control/fork_write.rs", 3, "会话记录根 + 会话文件命名"),
        (
            "control/resolve_query.rs",
            6,
            "resume 的默认命令与会话名前缀（**两家各三处** —— 它同时也是唯一登记的 kind 派发点）",
        ),
        ("main.rs", 1, "解析本机 home（`resolve_agent_home` 里唯一那句）"),
        ("observe/accounts_query.rs", 3, "pidfile 目录 + 账号环境变量名 + `.claude.json` 信任判定"),
        ("observe/history_query.rs", 5, "会话记录根 + 「这个文件是不是会话记录」×4"),
        ("observe/search_query.rs", 2, "会话记录根 + 会话文件判定"),
        ("observe/usage_query.rs", 3, "会话记录根 + 会话文件判定 + codex 侧聚合的另一半"),
        ("observe/watcher.rs", 4, "会话记录根 + pidfile 目录 + 判活 cmdline + 会话文件判定"),
    ];

    /// 〔`S5`〕**适配层注册表**住哪几个文件（`文件`, 为什么它不该被压到零）。
    ///
    /// # 它为什么是**另一张表**，而不是 [`ADAPTER_CALL_SITES`] 里的一条
    ///
    /// 那张表里每一条的含义是**统一的**：「通用层直呼适配层 = 加一个 agent 要回来改的地方」，
    /// 而 `S6` 的成绩就是**把那个数压下去**。
    ///
    /// 注册表是**性质相反**的东西：`agents/mod.rs` 里那几行是「加一个 agent **本来就该**
    /// 动的一行」—— 它不该被压下去，反而应该**随 agent 数增长**。
    /// 两类塞进同一个计数里，`S6` 就没法拿那个数当成绩了：降了不知道是真收进接口了，
    /// 还是有人把调用点挪进注册表文件里去刷数。
    ///
    /// ⇒ 分两张表。⚠ **排除是有对价的**：
    /// [`the_adapter_registry_is_one_line_per_agent`] 把注册表文件的处数钉死成
    /// **恰好 `REGISTRY.len()`** —— 想往这个文件里藏一处别的直呼，当场超出、当场红。
    /// 没有那条对价，本表就成了「把东西挪进来就不算数」的逃生舱。
    const AGENT_REGISTRY_SITES: &[(&str, &str)] = &[(
        "agents/mod.rs",
        "适配层自己的索引：不写 `pub(crate) mod <名>;` 新适配层根本编不进来 ⇒ \
         它**本来就**在「加一个 agent 必改」的清单里。`S5` 把注册表（每家的 `agent_kind` 值 + \
         home 解析入口）放在这里，是为了不让必改的文件从 1 个变成 2 个。**一家一行**。",
    )];

    /// 判据③的针：`<agent 名>_dir` 这一形的**标识符**。**运行时拼**（本文件散文里就有这些词）。
    ///
    /// ⚠ 蛇形与驼峰**都要**：serde 的 `rename_all` 会把 `claude_dir` 变成 `claudeDir`，
    /// 只认一种就等于放过另一种写法。
    fn agent_named_dir_needles() -> Vec<String> {
        let mut out = Vec::new();
        for name in [format!("clau{}", "de"), format!("cod{}", "ex")] {
            out.push(format!("{name}_dir"));
            let mut camel = name.clone();
            camel.push_str("Dir");
            out.push(camel);
        }
        out
    }

    /// 判据④的针：每个 agent 家的**模块路径**（`agents/codex/` → `agents::codex::`），
    /// **外加它的相对写法**（`codex::`）。
    ///
    /// 由 [`HOMES`] 派生而不是另写一份 —— 加一个 agent 只改 `HOMES` 一处。
    ///
    /// # ⚠〔`S5` 08-14〕相对写法那半是补上的 —— 此前它是判据④的**盲区**
    ///
    /// `S4b` 立这条时只拼了全路径 `agents::<名>::`，因为当时所有调用点都在
    /// `observe/`/`control/`/`main.rs`，那里只能写全路径。**但 `agents/mod.rs` 不是** ——
    /// 它是 `agents::` 的父模块，引用两家写的是 `claudecode::home` / `codex::home`，
    /// 全路径针一条都打不中。
    ///
    /// `S5` 往 `agents/mod.rs` 加注册表时**当场撞上**这个盲区：新增的 4 处直呼适配层，
    /// 判据④**全绿**。⇒ 补针。这是本区第二次「量具的作用域比事实**小**」
    ///（第一次是 `S4b §0b-2`：`S1` 的判据只扫 `CORE_FILES`，看不见 `resolve_query` 那条冻结字段）。
    ///
    /// ★ 教训与那次同型、方向相反于"作用域比事实大"那三次：**作用域小的表现不是虚高的读数，
    /// 是一条真实的耦合根本没进视野** —— 而它偏偏出现在"表恰好全绿"的时候。
    fn adapter_path_needles() -> Vec<String> {
        let mut out = Vec::new();
        for h in HOMES {
            let rel = h.trim_end_matches('/');
            // 全路径：`agents/codex/` → `agents::codex::`
            out.push(rel.replace('/', "::") + "::");
            // 相对写法：`agents/codex/` → `codex::`（`agents/mod.rs` 里的写法）
            if let Some(seg) = rel.rsplit('/').next() {
                out.push(format!("{seg}::"));
            }
        }
        out
    }

    /// 整棵 `src/` 的 `(相对路径, 生产段)`。`scan_tree!` **按构造摘掉调用者自己**。
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

    /// ① 任何 agent 的目录/格式知识都只许住 `agents/<名>/`。
    #[test]
    fn agent_format_knowledge_lives_only_in_agent_homes() {
        let files = sources();
        assert!(
            files.len() >= 20,
            "只遍历到 {} 个源文件 —— 遍历坏了，本断言在空转",
            files.len()
        );
        let homes_seen = HOMES
            .iter()
            .filter(|h| files.iter().any(|(rel, _)| rel.starts_with(**h)))
            .count();
        assert!(
            homes_seen >= HOMES_FLOOR,
            "只找到 {homes_seen} 个 agent 家（下界 {HOMES_FLOOR}）—— 有人删了或改名了某一家，\n\
             而本判据会因此**静默放行那一整家的知识**"
        );
        let needles = needles();
        let mut leaks: Vec<String> = Vec::new();
        let mut excused: Vec<&str> = Vec::new();
        for (rel, prod) in &files {
            if HOMES.iter().any(|h| rel.starts_with(h)) {
                continue; // agent 自己的家
            }
            for (line_no, line) in prod.lines().enumerate() {
                for (n, what) in &needles {
                    if !line.contains(n.as_str()) {
                        continue;
                    }
                    if let Some((f, _, _)) = NOT_AGENT_KNOWLEDGE
                        .iter()
                        .find(|(f, frag, _)| rel == f && line.contains(frag))
                    {
                        excused.push(f);
                        continue;
                    }
                    leaks.push(format!("{rel}:{}  [{what}] {}", line_no + 1, line.trim()));
                }
            }
        }
        assert!(
            leaks.is_empty(),
            "agent 的目录/格式知识跑到 `agents/*/` 之外去了（{} 处）：\n  {}\n\
             ⚠ 那些名字是**某个 agent 的文件格式或目录布局本身**。散在通用层里，\n\
             「接第三个 agent 要改哪几处」就又只住在人的脑子里了 —— 那正是本区要消灭的病。\n\
             搬进 `agents/<名>/`，通用层通过适配层取；真是判据看走眼了就登记进 \n\
             `NOT_AGENT_KNOWLEDGE`（**那张表不是欠账表**，进去的东西永远留着）。",
            leaks.len(),
            leaks.join("\n  ")
        );
        // 反向：登记的假阳必须**真的还在命中**，否则它就是幽灵条目。
        for (f, frag, _) in NOT_AGENT_KNOWLEDGE {
            assert!(
                excused.contains(f),
                "`NOT_AGENT_KNOWLEDGE` 里登记的 `{f}`（片段 `{frag}`）已经不再命中 —— \n\
                 代码变了而登记没跟，摘掉它"
            );
        }
    }

    /// ② 通用层里的 kind 派发点，登记表与实得**逐条对齐**（多一处红、少一处也红）。
    #[test]
    fn kind_dispatch_sites_are_enumerated_one_by_one() {
        let files = sources();
        let lit = kind_literal();
        let mut hits: Vec<String> = Vec::new();
        for (rel, prod) in &files {
            if HOMES.iter().any(|h| rel.starts_with(h)) {
                continue;
            }
            if prod.contains(&lit) {
                hits.push(rel.clone());
            }
        }
        hits.sort();
        let mut registered: Vec<String> =
            KIND_DISPATCH_SITES.iter().map(|(f, _)| f.to_string()).collect();
        registered.sort();
        assert_eq!(
            hits, registered,
            "\n通用层里拿 agent kind **值**做判别的地方与登记表对不上。\n\
             实得：{hits:?}\n登记：{registered:?}\n\
             ⚠ 多出来的那处 = **又一个「加 agent 时要跟着改」的地方**，先把理由写进 \
             `KIND_DISPATCH_SITES` 再说；\n\
             少掉的那处 = 派发搬走了而登记没跟，摘掉它（这张表短了是好事，长了是坏事）。"
        );
        assert!(
            !KIND_DISPATCH_SITES.is_empty(),
            "登记表空了 —— 要么派发真没了（那 `S3`/`S6` 该更新本条），要么抽取坏了"
        );
    }

    /// ③ 反向夹具：喂合成样本，正题的判定必须**认得出违规**、且**认不出合法值判别**。
    ///
    /// 上面两条都是「找不到东西就绿」的形状 —— 判定一旦坏掉（永远回 false），
    /// 它们会**安静地全绿**。本条同时钉住两个方向：漏报与误报。
    #[test]
    fn the_detector_catches_violations_and_spares_legal_value_dispatch() {
        let needles = needles();

        // 正向：合成的违规代码，必须被逮到。
        // ⚠ 样本里**故意**放了 `sessions/` 那一形：它是"看起来像针但刻意不是针"的那个词。
        // 只有样本里真有它，`caught == 1` 才同时钉住两件事 ——
        // ① 判定认得出真违规；② 谁把 `sessions/` 加成针，这条会红（它会变成 2）。
        let violation = format!(
            "let root = base.join(\"sess{}\");\nif name.starts_with(\"rollo{}\") {{}}",
            "ions/", "ut-"
        );
        let caught = needles
            .iter()
            .filter(|(n, _)| violation.contains(n.as_str()))
            .count();
        assert_eq!(
            caught, 1,
            "判定认不出合成的违规样本（命中 {caught}，应为 1：只该打中 `rollout-`，\
             `sessions/` 刻意不是针）—— 正题此刻是空转的"
        );

        // 反向：`D3` 明说合法的**值判别**，不许被格式针打中。
        let legal = format!("let is_cx = spec.agent_kind.trim() == \"cod{}\";", "ex");
        let false_positives: Vec<&str> = needles
            .iter()
            .filter(|(n, _)| legal.contains(n.as_str()))
            .map(|(_, w)| *w)
            .collect();
        assert!(
            false_positives.is_empty(),
            "格式针打中了合法的值判别：{false_positives:?}\n\
             ⚠ `D3` 逐字：agent 维度**只许出现在值里** —— 把它判成违规就是假阳，\n\
             而假阳会训练人绕过判据，比没有判据更坏。"
        );
        // 值判别归第二条判据管，那条**应该**认得它。
        assert!(
            legal.contains(&kind_literal()),
            "kind 值判别的形状变了，第二条判据的抽取要跟着改"
        );
    }

    /// ③〔`S4b`〕通用层的**标识符**里不许出现 `<agent 名>_dir`，例外只有冻结的 wire 字段。
    ///
    /// 没有这一条的话，`S4b` 改的那 60 多行会**安静地长回来**：下一个人照着旧调用点复制一个
    /// `fn run(claude_dir: &Path, …)`，编译过、测试全绿、判据一条不响 —— 那正是 `D2` 头一句
    /// 「没有判据的重构，正确性只能靠人记得」说的事。
    #[test]
    fn no_agent_named_dir_identifier_outside_agent_homes() {
        let files = sources();
        assert!(
            files.len() >= 20,
            "只遍历到 {} 个源文件 —— 遍历坏了，本断言在空转",
            files.len()
        );
        let needles = agent_named_dir_needles();
        assert!(!needles.is_empty(), "针表空了 ⇒ 本条恒绿");

        let mut hits: Vec<String> = Vec::new();
        for (rel, prod) in &files {
            if HOMES.iter().any(|h| rel.starts_with(h)) {
                continue; // agent 自己的家：在 `agents/codex/` 里叫 `codex_dir` 是冗余、不是越界
            }
            for (i, line) in prod.lines().enumerate() {
                if needles.iter().any(|n| line.contains(n.as_str())) {
                    hits.push(format!("{rel}:{}  {}", i + 1, line.trim()));
                }
            }
        }
        let (known, unknown): (Vec<String>, Vec<String>) = hits
            .into_iter()
            .partition(|h| {
                AGENT_NAMED_WIRE_FIELDS
                    .iter()
                    .any(|(f, frag, _, _)| h.starts_with(&format!("{f}:")) && h.contains(frag))
            });
        assert!(
            unknown.is_empty(),
            "通用层的标识符里出现了 `<agent 名>_dir`（{} 处）：\n  {}\n\n\
             ⇒ 通用层的 home 参数一律叫 `agent_home`（`S4b`）。协议面 `D3` 已经裁过\n\
             「agent 维度只许出现在**值**里」；仓内同理 —— 签名里写死一个 agent 的名字，\n\
             「谁是通用层」就又只能靠人记得了。\n\
             ⚠ 真是冻结的 wire 字段（改名 = 破坏跨仓契约），登记进 `AGENT_NAMED_WIRE_FIELDS`，\n\
             **并写解锁条件**；没有解锁条件的冻结只是「永久豁免」的好听说法。",
            unknown.len(),
            unknown.join("\n  ")
        );
        // 幽灵检查：登记的必须**还在命中**。字段真删掉了却不摘登记，
        // 下一个人会以为这里还冻着、进而不敢动。
        for (f, frag, _, _) in AGENT_NAMED_WIRE_FIELDS {
            assert!(
                known.iter().any(|h| h.starts_with(&format!("{f}:")) && h.contains(frag)),
                "`AGENT_NAMED_WIRE_FIELDS` 里登记的 `{f}`（片段 `{frag}`）已经不再命中 —— \
                 修好了就摘登记（这张表只许缩短）"
            );
        }
        /// 今天恰好 3 条（同一个 wire 字段名的两处声明 + 一处构造）。**只许降不许升**。
        const CEILING: usize = 3;
        assert!(
            AGENT_NAMED_WIRE_FIELDS.len() <= CEILING,
            "`AGENT_NAMED_WIRE_FIELDS` 涨到 {} 条了（天花板 {CEILING}）。\n\
             ⚠ 如果你正在加第三个 `<名>_dir` 字段 —— 那正是 `D3` 逐字排除掉的那条路，\n\
             终点是 hello 里五个并列的目录字段。往 `homes` 里加一项，不要加字段。",
            AGENT_NAMED_WIRE_FIELDS.len()
        );
        for (f, frag, why, unlock) in AGENT_NAMED_WIRE_FIELDS {
            assert!(!why.trim().is_empty(), "{f}:{frag} 没写「为什么改不动」");
            assert!(
                unlock.trim().len() >= 20,
                "{f}:{frag} 没写**解锁条件**（实得 {} 字节）—— 要写的是「什么条件满足之后\
                 这一条就能删」，不是「为什么现在不能删」",
                unlock.trim().len()
            );
        }
    }

    /// 通用层里每个文件**直呼适配层**的处数（`agents/<名>/` 自己的家不算）。
    ///
    /// 判据④与⑦共用这一份抽取 —— 抽两遍必然漂开。
    fn adapter_call_sites_measured() -> Vec<(String, usize)> {
        let files = sources();
        let needles = adapter_path_needles();
        assert!(
            needles.len() >= HOMES_FLOOR,
            "从 `HOMES` 只派生出 {} 根适配层路径针（下界 {HOMES_FLOOR}）—— 派生坏了，本条在空转",
            needles.len()
        );
        let mut got: Vec<(String, usize)> = Vec::new();
        for (rel, prod) in &files {
            if HOMES.iter().any(|h| rel.starts_with(h)) {
                continue;
            }
            let n = prod
                .lines()
                .filter(|l| needles.iter().any(|nd| l.contains(nd.as_str())))
                .count();
            if n > 0 {
                got.push((rel.clone(), n));
            }
        }
        got.sort();
        got
    }

    /// ④〔`S4b`〕通用层直呼适配层的地方，登记表与实得**逐条对齐**（多一处红、少一处也红）。
    ///
    /// 形态照 [`kind_dispatch_sites_are_enumerated_one_by_one`]：这张表**短了是好事、长了是坏事**。
    ///
    /// ⚠〔`S5`〕人群里**扣掉注册表文件**（[`AGENT_REGISTRY_SITES`]）——
    /// 那类是「加一个 agent 本来就该改的一行」，性质与本表相反，混进来 `S6` 就没法拿
    /// 这个数当成绩（降了不知道是真收进接口，还是有人把调用点挪进注册表文件刷了数）。
    /// **扣掉的对价**是判据⑦：注册表文件的处数被钉死成 `REGISTRY.len()`，藏不进第二样东西。
    #[test]
    fn general_layer_adapter_call_sites_are_enumerated_one_by_one() {
        let got: Vec<(String, usize)> = adapter_call_sites_measured()
            .into_iter()
            .filter(|(rel, _)| !AGENT_REGISTRY_SITES.iter().any(|(f, _)| rel == f))
            .collect();
        let mut want: Vec<(String, usize)> = ADAPTER_CALL_SITES
            .iter()
            .map(|(f, n, _)| ((*f).to_string(), *n))
            .collect();
        want.sort();
        assert_eq!(
            got, want,
            "\n通用层直呼适配层的地方与登记表对不上。\n实得：{got:?}\n登记：{want:?}\n\
             ⚠ **多出来的那处 = 又一个「加 agent 时要回来改」的地方** —— 先登记 + 写清它在向\n\
             适配层要什么，然后问一句：这一处能不能改成走接口（`L2`/`S6`）？\n\
             少掉的那处 = 真收进接口了，恭喜，摘登记（这张表短了是好事）。"
        );
        for (f, n, what) in ADAPTER_CALL_SITES {
            assert!(*n > 0, "{f} 登记了 0 处 —— 那它不该在表里");
            assert!(!what.trim().is_empty(), "{f} 没写「它在向适配层要什么」");
        }
    }

    /// ⑦〔`S5`〕**注册表文件里一家恰好一行** —— 判据④把它扣出人群之后的**对价**。
    ///
    /// # 没有这一条，[`AGENT_REGISTRY_SITES`] 就是逃生舱
    ///
    /// 判据④为了不稀释 `S6` 的靶子，把注册表文件整个扣出了人群。
    /// 那立刻开出一条捷径：**把通用层的直呼挪进 `agents/mod.rs`**，④ 的数就掉下去了，
    /// 而「加一个 agent 要改几处」一点没少 —— 只是换了个地方藏。
    ///
    /// ⇒ 本条把注册表文件的处数钉死成 **恰好 `REGISTRY.len()`**：
    /// 一家一行，多一行就是藏了别的东西，当场红。
    ///
    /// ★ 与 ④ 的**方向也相反**：④ 的数应该**降到零**；本条的数应该**随 agent 数一起涨**。
    /// 这正是拆成两张表的全部理由。
    #[test]
    fn the_adapter_registry_is_one_line_per_agent() {
        let measured = adapter_call_sites_measured();
        let agents = crate::agents::REGISTRY.len();
        assert!(agents >= HOMES_FLOOR, "注册表只剩 {agents} 家，下界 {HOMES_FLOOR}");
        for (file, why) in AGENT_REGISTRY_SITES {
            assert!(!why.trim().is_empty(), "{file} 没写「它为什么不该被压到零」");
            let n = measured
                .iter()
                .find(|(rel, _)| rel == file)
                .map(|(_, n)| *n)
                .unwrap_or(0);
            assert_eq!(
                n, agents,
                "\n`{file}` 里直呼适配层的行数是 {n}，而注册表有 {agents} 家 —— 应当**一家一行**。\n\
                 ⚠ 多出来 = 有人把别的直呼藏进了注册表文件（判据④已经把这个文件扣出人群，\n\
                 藏进来就等于从 `S6` 的靶子上抹掉一处），或者注册表被排版成了每字段一行；\n\
                 ⚠ 少了（尤其是 0）= 注册表没了 / 抽取坏了 ⇒ 本条与判据④的扣除都在空转。"
            );
        }
    }

    /// ⑥〔`S5`〕**每个 agent 家在注册表里恰好一条** —— 多一家红、少一家也红。
    ///
    /// # 它守的是「建了家却没人认得」这种最安静的坏法
    ///
    /// `S5` 之后「daemon 看得见哪些 agent」的答案由 [`crate::agents::REGISTRY`] 给。
    /// 建了 `agents/<名>/`、知识也搬进去了、判据①②③④**全绿** —— 但忘了往注册表加一行，
    /// 那家就**永远不会出现在 `hello.homes` 里**，而没有任何东西会说。
    /// 这正是 `S6`（最小假 agent 走全流程）会一头撞上的坑，所以判据先立在这儿。
    ///
    /// ⚠ 人群取自**文件树**（`agents/<名>/` 真的存在几个）而不是 [`HOMES`] 那个常量 ——
    /// 否则「加了一家但两处常量都忘了改」会全绿。顺带把 `HOMES` 自己也对了一次账：
    /// 它此前只有下界（`HOMES_FLOOR`），**多写一家、写错一个名字都不会红**。
    #[test]
    fn every_agent_adapter_has_exactly_one_registry_entry() {
        let files = sources();
        // 文件树里真实存在的 agent 家：`agents/<名>/…` 的第二段。
        let mut in_tree: Vec<String> = files
            .iter()
            .filter_map(|(rel, _)| rel.strip_prefix("agents/"))
            .filter_map(|rest| rest.split_once('/').map(|(name, _)| name.to_string()))
            .collect();
        in_tree.sort();
        in_tree.dedup();
        assert!(
            in_tree.len() >= HOMES_FLOOR,
            "文件树里只找到 {} 个 agent 家（下界 {HOMES_FLOOR}）—— 遍历坏了，本条在空转：{in_tree:?}",
            in_tree.len()
        );

        let mut declared: Vec<String> = HOMES
            .iter()
            .map(|h| h.trim_start_matches("agents/").trim_end_matches('/'))
            .map(str::to_string)
            .collect();
        declared.sort();
        assert_eq!(
            in_tree, declared,
            "\n`HOMES` 与文件树里的 agent 家对不上。\n实得：{in_tree:?}\n登记：{declared:?}\n\
             ⚠ 多出来的那家**整层知识都不被判据①扫**（`HOMES` 是排除表）；\n\
             少掉的那家反过来 —— 它自己的格式知识会被当成通用层的泄漏。"
        );

        let reg = crate::agents::REGISTRY;
        assert_eq!(
            reg.len(),
            in_tree.len(),
            "\n有 {} 个 agent 家，注册表却有 {} 条。\n\
             ⚠ 少一条 = 那家**永远不会出现在 `hello.homes` 里**，而编译、判据①②③④全绿 —— \n\
             `S6` 的最小假 agent 会一头撞上它。\n\
             ⚠ 多一条 = 注册表在指一个不存在的适配层。\n\
             家：{in_tree:?}｜注册的 kind：{:?}",
            in_tree.len(),
            reg.len(),
            reg.iter().map(|a| a.kind).collect::<Vec<_>>()
        );
    }

    /// ⑤〔`S4b`〕反向夹具：③④两条的判定**真的会红**。
    ///
    /// 两条都是「找不到东西就绿」的形状 —— 针拼坏了（比如 `format!` 拼出空串）会**安静地全绿**。
    #[test]
    fn the_s4b_detectors_catch_synthetic_violations() {
        // ③ 正向：合成的违规签名必须被逮到。
        let bad = format!("fn run(clau{}_dir: &Path, args: &[String]) -> i32 {{}}", "de");
        assert!(
            agent_named_dir_needles().iter().any(|n| bad.contains(n.as_str())),
            "③ 的判定认不出合成的违规签名 —— 它此刻是空转的"
        );
        // ③ 反向：改名之后的正确写法**不许**被打中（假阳会训练人绕过判据）。
        let good = "fn run(agent_home: &Path, args: &[String]) -> i32 {}";
        assert!(
            !agent_named_dir_needles().iter().any(|n| good.contains(n.as_str())),
            "③ 把改名之后的正确写法判成了违规 —— 那是假阳"
        );
        // ④ 正向：适配层路径针认得出真调用。
        let call = format!("crate::agents::clau{}code::records::is_session_file(p)", "de");
        assert!(
            adapter_path_needles().iter().any(|n| call.contains(n.as_str())),
            "④ 的判定认不出适配层调用 —— 它此刻是空转的"
        );
        // ④ 反向：**值**上的派发（`D3` 明说合法）不许被④打中 —— 那是判据②的活。
        let value_dispatch = format!("let is_cx = spec.agent_kind.trim() == \"cod{}\";", "ex");
        assert!(
            !adapter_path_needles()
                .iter()
                .any(|n| value_dispatch.contains(n.as_str())),
            "④ 打中了合法的值判别 —— `D3` 逐字：agent 维度**只许出现在值里**"
        );
    }
}
