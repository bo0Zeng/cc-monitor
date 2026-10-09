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
//! ## ② 通用层里**一个 agent 名字面量都没有**
//!
//! 用户逐字：「思考怎么解耦, 不要硬适配claude code」「如果是其他agent呢, 比如codex」。
//! 从前这里是「拿 kind **值**做判别的地方逐条登记」（`resolve_query` 与 `ccm` 各一处）；
//! 那两处问的能力（默认那一家 · resume 命令形 · 会话名前缀 · cc-bus 身份 · 身份面 · pidfile · 信任框）
//! 收进了适配层起会话事实的各格，通用层只问那一格 ⇒ 登记表收成零，判据改成**零命中**。
//!
//! ## ③通用层的**标识符**里不许出现 `<agent 名>_dir`
//!
//! `D3` 逐字管的是**协议字段名**。`S4b` 把同一条道理往仓内推一格：
//! `fn run(claude_dir: &Path, …)` 这种签名让「谁是通用层」只能靠人记得 ——
//! 协议干净了，代码里仍然是一句 agent 名。
//!
//! ⇒ 通用层的 home 参数一律叫 `agent_home`（`S4b` 改了 8 个文件；生产段 `claude_dir` 64 行 → 3 行）。
//! 例外只有**冻结的 wire 字段名**（Rust 侧标识符必须与线上字段名同名，改名 = 破坏契约），
//! 逐条登记进 [`tests::AGENT_NAMED_WIRE_FIELDS`]，**每条带解锁条件、条数有天花板**。
//!
//! ## ④通用层**零处**直呼某个适配层
//!
//! `G1` 成功标准②逐字：「加一个新 agent 只需新增 `agents/<名>/`，**通用层零改动**」。
//! 通用层里一处 `agents::<名>::…` 都没有：每一种 agent 知识都收进注册表那一格（[`tests::CAPABILITY_FACES`]），
//! 通用层按「这一家的那一格是什么」问。判据是**零命中**，命中一处就红。
//!
//! ## ⑤**注册表**那类被拆了出去 —— 它与④性质相反
//!
//! `S5` 往 `agents/mod.rs` 放了适配层注册表（`REGISTRY`：这个后端认得哪几个 agent）。
//! 它也是「直呼适配层」，但**方向反了**：④ 要零处，而注册表那几行是「加一个 agent **本来就该**动的一行」，
//! 随 agent 数增长。⇒ 拆成 [`tests::AGENT_REGISTRY_SITES`]，④ 的人群里扣掉它。
//! **扣除的对价**是判据⑦：注册表文件的处数被钉死成 `REGISTRY.len()`（一家一行），藏不进第二样东西。
//! ④ 的针有全路径 `agents::<名>::` 与相对路径 `<名>::` 两种写法（`agents/mod.rs` 里写的是后一种）。
//!
//! # ⚠ 诚实边界（四条，都写在这里而不是只写在计划里）
//!
//! 1. 判据认的是**字面量**，不是语义。派发点里的人直接写
//!    `format!("{base} resume {sid}")` 仍然不会红 —— 堵死它要等 `S3` 凑齐两个实现后立接口（`D4`）。
//! 2. 判据①的针是一张固定的格式词表：词表外的 agent 知识（新造的目录名、新的环境变量名）它认不出。
//! 3. `production_code` 剥掉注释与 `#[cfg(test)] mod` ⇒ **文档与测试里怎么写都不红**。
//!    这是刻意的（判据自己的散文里就有这些词），代价是"只在测试里泄漏格式知识"逮不到。
//! 4. 判据③的针是 `<agent 名>_dir` 这一形，**只认得已知的两个名字**（同 `S1` 的 `4a`）。
//!    有人在通用层写 `claude_home` / `gemini_root`，它一条都不会红。
//!    今天的兜底是判据④：那种代码迟早要去调某个 `agents::<名>::`，那一步当场红。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    /// **所有** agent 家的前缀（相对 `src/`）。加一个 agent = 加一行。
    ///
    /// 扫描时整体排除它们 —— 判据因此不需要回答「这是谁的知识」，
    /// 只需要回答「这是不是 agent 知识」（`S3` §1b-4）。
    ///
    /// ⚠这里面有一家是**夹具家**（见 [`FIXTURE_HOMES`]）：它在文件树里、
    /// 在本表里，但**不在** [`crate::agents::REGISTRY`] 里，模块声明还带着 `#[cfg(test)]`。
    /// 生产家与夹具家的条数各自有账，[`every_agent_adapter_has_exactly_one_registry_entry`]
    /// 算的就是那笔账。
    const HOMES: &[&str] = &["agents/codex/", "agents/claudecode/", "agents/fake/"];

    /// **夹具家**：住在 `agents/<名>/`、进 [`HOMES`]，但**不进生产注册表**。
    ///
    /// # 它为什么必须单独登记，而不是"反正 `HOMES` 里多一行"
    ///
    /// [`HOMES`] 是**四条判据共用的排除表** —— 进了它，那一整层的格式知识就不被判据①扫。
    /// 对一个真 agent 那是对的；对一个夹具，那立刻开出一条捷径：
    /// **把通用层里洗不掉的 agent 知识挪进 `agents/fake/`**，判据①当场安静。
    ///
    /// ⇒ 三条对价，缺一条这张表就是逃生舱：
    /// ① **只许 `fake` 那一家**——「再加一个夹具家」得先改判据、说清第一家为什么不够；
    /// ② **必须真在文件树里**（幽灵检查，见判据⑥）；
    /// ③ **绝不许进 [`crate::agents::REGISTRY`]**，且模块声明必须带 `#[cfg(test)]`
    ///    —— 那两条由 `crate::agents::fake::tests::the_fixture_agent_never_ships` 双向钉住。
    const FIXTURE_HOMES: &[(&str, &str)] = &[(
        "fake",
        "`S6` 的最小假 agent —— 本区**验收件**的量具。它存在的唯一理由是回答\
         「加一个新 agent 到底要动通用层几处」，所以它必须长得**像一个真 agent**\
         （住 `agents/<名>/`、12 种能力齐全、进 `HOMES`），又必须**永远上不了生产**\
         （`#[cfg(test)]` + 不进 `REGISTRY`）。⚠ 它的每一种能力都**刻意与 Claude 不同形**：\
         同形的话，通用层拿 Claude 的知识去解释它的 home 恰好也能读出东西，\
         `S6` 的正题就退化成一个粉饰的通过。",
    )];

    /// 针：**agent 的目录布局与文件格式**，运行时拼（本文件的散文里就有这些词）。
    ///
    /// 前六根是 `S2` 立的（Codex 专有），后五根是 `S3` 加的 ——
    /// 其中 `"sessions"` 与 `"jsonl"` **两个 agent 都用**，按住址分区之后才敢加。
    ///
    /// ⚠ 带引号的那几根是**刻意的**：`jsonl` 裸词会打中 `jsonl_path` / `read_jsonl` /
    /// `newest_jsonl` 这一大片**变量名与函数名** —— 那是通用的流式读取机器，不是知识。
    /// 同一个教训在 monitor 侧那条「用量口径只有一个家」的判据头注里也记着
    /// （「匹配单位比事实**大**」）。〔：那条判据随用量 ② 轴整轴退役 ——
    /// 教训还在，出处没了。〕
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
            (
                format!(".js{}\"", "onl"),
                "会话文件后缀（拼在字符串字面量末尾，`format!` 那一形）",
            ),
            (
                format!("\"mcp{}\"", "Servers"),
                "MCP 配置里装 server 表的键",
            ),
            (format!(".mcp{}\"", ".json"), "项目级 MCP 配置文件名"),
            (format!("settings{}\"", ".json"), "设置文件名"),
            (format!("join(\"ta{}\")", "sks"), "任务列表目录"),
            (format!("ANTHROPIC_BASE{}", "_URL"), "上游地址环境变量"),
            (
                format!("\"cc{}\"", "t"),
                "别名名字（由那一家的 wrapper 名派生）",
            ),
            (
                format!("\"cc{}\"", "a"),
                "别名名字（由那一家的 wrapper 名派生）",
            ),
        ]
    }

    /// **不是 agent 知识、但会被针打中**的地方，逐条登记 + 写清"它到底是什么"。
    ///
    /// ⚠ 这张表与 `S1` 的 `KNOWN_DEBT` **性质相反**，别混：
    /// 那张是「欠账，将来要清零」；**这张是「判据看走眼了，永远留着」**。
    /// 把假阳塞进欠账表的后果是它**永远清不掉**，于是"只会缩短"的那张表里
    /// 长出永久居民 —— 递减棘轮就此失去意义（`S3-Y3`）。
    const NOT_AGENT_KNOWLEDGE: &[(&str, &str, &str)] = &[
        (
            "control/cc_bus.rs",
            ".claude",
            "这是 **cc-bus 的门牌号**（`~/.claude/skills/cc-bus/scripts`），不是 agent 知识 —— \
             cc-bus 恰好装在那个目录下而已。`cc_bus_boundary_guard` 的头注逐字写着这条分界：\
             「允许**命令的地址**，禁**数据布局**」。同 `S1` 的 `@ccm_sid`、`S2` 的 `sessions/` 那一族。",
        ),
        // `control/ccm/argv.rs` 那一条摘了：账号库的门牌号搬进后端的家（契约 crate 的 `ACCOUNTS_DIR_REL`），那里不再有 `.claude` 字样。
        (
            "observe/search_query.rs",
            "\"sessions\"",
            "这是 **`history-search-merge` 线上的字段名**（各台搜索结果的会话行那一摞，界面 `SearchResult.sessions`），\
             不是哪个 agent 的 `sessions/` 目录布局 —— 合并只读 `updatedAt` / `hitCount` / `hitsTruncated` 三格。",
        ),
        (
            "faces/rotation_face.rs",
            "\"sessions\"",
            "这是换号那一族**线上的字段名**（一批会话各自的结果 / 那一份，`rotation-session-*` · `rotation-switch`），\
             不是哪个 agent 的 `sessions/` 目录布局。",
        ),
        (
            "faces/rotation_switch_face.rs",
            "\"sessions\"",
            "同上一条：「现在就换」的入参与每个会话的结果那一格（`rotation-switch`），不是目录布局。",
        ),
        (
            "stream/inbound/registry/accounts.rs",
            "\"sessions\",",
            "同上一条：换号那一族命令登记的入参 / 应答字段名（`fields` 表），不是目录布局。",
        ),
        (
            "stream/inbound/registry/history.rs",
            "\"sessions\",",
            "同上一条：`history-search-merge` 那条命令登记的应答字段名（`fields` 表），不是目录布局。",
        ),
    ];

    /// ② 的针：每一家 agent 的**名字**（wire kind · 适配器 id · 中转路由名），外加夹具家的 kind，**带引号的整串**。
    /// 从注册表派生，不另写一份 —— 加一家，针跟着长。
    ///
    /// ⚠ 带引号是刻意的：`== "claude"` · `match … "codex" =>` · `const X: &str = "claude-code"` 都是这一形，
    /// 而 `".claude-alt/…"` 这类住址、`is_codex` 这类标识符不是（前者归判据①的格式针，后者不是值）。
    fn agent_name_literals() -> Vec<String> {
        let mut names: Vec<&str> = Vec::new();
        for a in crate::agents::REGISTRY {
            names.push(a.kind);
            if let Some(f) = a.launch {
                names.push(f.adapter_id);
            }
            if let Some(u) = &a.upstream {
                names.push(u.route_id);
            }
        }
        for (fixture, _) in FIXTURE_HOMES {
            names.push(fixture);
        }
        names.sort_unstable();
        names.dedup();
        names.into_iter().map(|n| format!("\"{n}\"")).collect()
    }

    /// **冻结的 wire 字段名**（`文件`, `片段`, 为什么改不动, **解锁条件**）——
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
            "stream/wire.rs",
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

    /// **适配层注册表**住哪几个文件（`文件`, 为什么它不该被压到零）。
    ///
    /// 通用层直呼适配层要零处（判据④），注册表那几行却是「加一个 agent **本来就该**动的一行」，随 agent 数增长
    /// ⇒ 从④的人群里扣掉。⚠ **排除是有对价的**：
    /// [`the_adapter_registry_is_one_line_per_agent`] 把注册表文件的处数钉死成
    /// **恰好 `REGISTRY.len()`** —— 想往这个文件里藏一处别的直呼，当场超出、当场红。
    /// 没有那条对价，本表就成了「把东西挪进来就不算数」的逃生舱。
    const AGENT_REGISTRY_SITES: &[(&str, &str)] = &[(
        "agents/mod.rs",
        "适配层自己的索引：不写 `pub(crate) mod <名>;` 新适配层根本编不进来 ⇒ \
         它**本来就**在「加一个 agent 必改」的清单里。`S5` 把注册表（每家的 `agent_kind` 值 + \
         home 解析入口）放在这里，是为了不让必改的文件从 1 个变成 2 个。**一家一行**。",
    )];

    /// 假 agent 的每一种能力住注册表哪一格（`能力`, `那一格`）—— 通用层只问那一格，不直呼某一家（判据④零命中）。
    /// 与 `agents::fake::CAPABILITIES` 双向对账：多一种 / 少一种都红。
    const CAPABILITY_FACES: &[(&str, &str)] = &[
        ("会话记录根", "RecordFace.tree.root"),
        ("会话文件判定", "RecordFace.is_session_file"),
        ("会话文件命名", "RecordFace.tree.file_name"),
        ("pidfile 目录", "LocalFace.pidfile_dir"),
        ("账号环境变量名", "AccountsFace.session_env"),
        ("账号信任判定", "AccountsFace.trust_in"),
        ("判活 cmdline", "LocalFace.cmdline_may_be_agent"),
        ("解析本机 home", "Adapter.home · LocalFace.home_at"),
        ("resume 默认命令", "LaunchFace.default_launcher"),
        ("resume 命令形", "LaunchFace.resume_command"),
        ("resume 会话名前缀", "LaunchFace.session_name_prefix"),
    ];

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
    /// # ⚠相对写法那半是补上的 —— 此前它是判据④的**盲区**
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

    /// 整棵 `src/` 的 `(相对路径, 生产段)`。
    ///
    /// ⚠ 先前这一行写着「`scan_tree!` **按构造摘掉调用者自己**」——
    /// 那一刀**在这一处不生效**：判据一律由 `#[path]` 挂进生产树，`file!()` 给的是
    /// 带 `..` 的折返路径，后缀比不命中
    /// （`the_scan_tree_macro_no_longer_excludes_its_caller_after_the_split` 守着这件事）。
    /// 本条的人群里没有本文件，靠的是**住址**：判据住 `tests/backend/`，不在被扫的 `src/` 下。
    fn sources() -> Vec<(String, String)> {
        let src_dir = crate::guard_support::src_root();
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

    /// ① 任何 agent 的目录/格式知识都只许住 `agents/<名>/`：通用层**零命中**（[`NOT_AGENT_KNOWLEDGE`] 登记的假阳除外）。
    /// 欠账已清零（`ccm` 的上游变量名收进 `DefaultUpstream.base_url_env` · 项目级 MCP 的足迹申报挪进 Claude 的足迹面），
    /// 不再设欠账表 ⇒ 新出现的一处当场红。
    #[test]
    fn agent_format_knowledge_lives_only_in_agent_homes() {
        let files = sources();
        let unseen: Vec<&&str> = HOMES
            .iter()
            .filter(|h| !files.iter().any(|(rel, _)| rel.starts_with(**h)))
            .collect();
        assert!(
            unseen.is_empty(),
            "这几个登记的 agent 家在遍历里一份文件都没有：{unseen:?} —— 有人删了或改名了某一家，\n\
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

    /// ② 通用层（`src/backend/` 的 `.rs` 生产段，扣掉 [`HOMES`]）里**零个** agent 名字面量。
    ///
    /// 人群之外、可以写名字的地方：各家适配层与注册表（`agents/`）· 夹具（`.json` / `.tsv`）·
    /// 给用户看的文案（文案表 `.json`）· 注释与测试段（`production_code` 剥掉）。
    /// 正控两道：每根针在适配层的家里**真有命中**（针对准了真写法）；合成的三种分叉形都被认出、住址不被认出。
    #[test]
    fn the_general_layer_names_no_agent() {
        let files = sources();
        let needles = agent_name_literals();
        let mut hits: Vec<String> = Vec::new();
        let mut aimed: Vec<&String> = Vec::new();
        for (rel, prod) in &files {
            let home = HOMES.iter().any(|h| rel.starts_with(h));
            for (i, line) in prod.lines().enumerate() {
                for n in needles.iter().filter(|n| line.contains(n.as_str())) {
                    if home {
                        aimed.push(n);
                    } else {
                        hits.push(format!("{rel}:{}  {}", i + 1, line.trim()));
                    }
                }
            }
        }
        aimed.sort();
        aimed.dedup();
        assert_eq!(
            aimed,
            needles.iter().collect::<Vec<_>>(),
            "有几根名字针在适配层自己的家里一处都没命中 —— 针拼错了，下面的零命中是空转"
        );
        assert_eq!(
            hits,
            Vec::<String>::new(),
            "\n通用层又按 agent 的名字认人了。\n\
             ⇒ 这一处问的是哪一种能力？把它做成适配层上的一格（`agents::LaunchFace` 或合适的那一面；\
             没声明 = 不支持），这里改成问那一格。默认那一家由注册表里声明 `is_default` 的那一家给出。"
        );
        for bad in [
            format!("if agent == \"clau{}\" {{}}", "de"),
            format!("match kind {{ \"cod{}\" => 1, _ => 0 }}", "ex"),
            format!("const A: &str = \"claude-{}\";", "code"),
        ] {
            assert!(
                needles.iter().any(|n| bad.contains(n.as_str())),
                "合成的按名分叉没被认出：{bad}"
            );
        }
        for good in [
            "let m = under_home(&home, \".cc-monitor/accounts/accounts.json\");",
            "if face.is_some_and(|f| f.needs_bus_id) {}",
        ] {
            assert!(
                !needles.iter().any(|n| good.contains(n.as_str())),
                "住址 / 问能力的写法被当成了按名分叉：{good}"
            );
        }
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
        // 值判别归第二条判据管（通用层里一处都不许有），那条**应该**认得它。
        assert!(
            agent_name_literals()
                .iter()
                .any(|n| legal.contains(n.as_str())),
            "kind 值判别的形状变了，第二条判据的针要跟着改"
        );
    }

    /// ③通用层的**标识符**里不许出现 `<agent 名>_dir`，例外只有冻结的 wire 字段。
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
        let (known, unknown): (Vec<String>, Vec<String>) = hits.into_iter().partition(|h| {
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
                known
                    .iter()
                    .any(|h| h.starts_with(&format!("{f}:")) && h.contains(frag)),
                "`AGENT_NAMED_WIRE_FIELDS` 里登记的 `{f}`（片段 `{frag}`）已经不再命中 —— \
                 修好了就摘登记（这张表只许缩短）"
            );
        }
        // 只许那一个历史字段（`claude_dir`）留着：登记一个别的 `<名>_dir` 字段 = 走 `D3` 逐字排除掉的那条路，
        // 终点是 hello 里五个并列的目录字段。往 `homes` 里加一项，不要加字段。
        for (f, frag, ..) in AGENT_NAMED_WIRE_FIELDS {
            assert_eq!(
                *frag, "claude_dir",
                "`AGENT_NAMED_WIRE_FIELDS` 登记了 `{f}` 的 `{frag}` —— 只许那一个历史字段，往 `homes` 里加一项，不要加字段"
            );
        }
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
            HOMES.iter().all(|h| needles
                .iter()
                .any(|n| n.starts_with(&h.trim_end_matches('/').replace('/', "::")))),
            "有一家没从 `HOMES` 派生出适配层路径针 —— 派生坏了，本条在空转：{needles:?}"
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

    /// ④通用层直呼适配层的地方，登记表与实得**逐条对齐**（多一处红、少一处也红）。
    ///
    /// 这张表**短了是好事、长了是坏事**。
    ///
    /// ⚠人群里**扣掉注册表文件**（[`AGENT_REGISTRY_SITES`]）——
    /// 那类是「加一个 agent 本来就该改的一行」，性质与本表相反，混进来 `S6` 就没法拿
    /// 这个数当成绩（降了不知道是真收进接口，还是有人把调用点挪进注册表文件刷了数）。
    /// **扣掉的对价**是判据⑦：注册表文件的处数被钉死成 `REGISTRY.len()`，藏不进第二样东西。
    /// ④ 通用层（扣掉各家与注册表文件）里直呼适配层的行：**零**。
    #[test]
    fn the_general_layer_calls_no_adapter_directly() {
        let got: Vec<(String, usize)> = adapter_call_sites_measured()
            .into_iter()
            .filter(|(rel, _)| !AGENT_REGISTRY_SITES.iter().any(|(f, _)| rel == f))
            .collect();
        assert!(
            got.is_empty(),
            "\n通用层直呼了适配层（`agents::<名>::…`）：{got:?}\n\
             ⇒ 那一家的这一件知识收进注册表对应那一格（`CAPABILITY_FACES` 列着已有的几格），通用层按 kind 问那一格。"
        );
    }

    #[test]
    fn the_adapter_registry_is_one_line_per_agent() {
        let measured = adapter_call_sites_measured();
        let agents = crate::agents::REGISTRY.len();
        assert!(agents > 0, "注册表空了");
        for (file, why) in AGENT_REGISTRY_SITES {
            assert!(
                !why.trim().is_empty(),
                "{file} 没写「它为什么不该被压到零」"
            );
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

    /// ⑥**每个 agent 家在注册表里恰好一条** —— 多一家红、少一家也红。
    ///
    /// # 它守的是「建了家却没人认得」这种最安静的坏法
    ///
    /// `S5` 之后「backend 看得见哪些 agent」的答案由 [`crate::agents::REGISTRY`] 给。
    /// 建了 `agents/<名>/`、知识也搬进去了、判据①②③④**全绿** —— 但忘了往注册表加一行，
    /// 那家就**永远不会出现在 `hello.homes` 里**，而没有任何东西会说。
    /// 这正是 `S6`（最小假 agent 走全流程）会一头撞上的坑，所以判据先立在这儿。
    ///
    /// ⚠ 人群取自**文件树**（`agents/<名>/` 真的存在几个）而不是 [`HOMES`] 那个常量 ——
    /// 否则「加了一家但两处常量都忘了改」会全绿。顺带把 `HOMES` 自己也对了一次账：
    /// 它此前只有下界，**多写一家、写错一个名字都不会红**。
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
            !in_tree.is_empty(),
            "文件树里一个 agent 家都没找到 —— 遍历坏了，本条在空转"
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

        // **夹具家不该进生产注册表** —— 先把这笔账单独结清，再对生产家的数。
        // 夹具家只许那一个最小假 agent（`fake`）：夹具家享受判据①的整层豁免，多一家就多一处可以往里塞 agent 知识而不会响的地方。
        for (name, _) in FIXTURE_HOMES {
            assert_eq!(
                *name, "fake",
                "又登记了一个夹具家 `{name}` —— 只许那一个最小假 agent"
            );
        }
        for (name, why) in FIXTURE_HOMES {
            assert!(
                in_tree.iter().any(|n| n == name),
                "`FIXTURE_HOMES` 里登记的夹具家 `{name}` 在文件树里不存在 —— \n\
                 目录删了而登记没跟（幽灵条目），或者名字写错了。实得：{in_tree:?}"
            );
            assert!(
                why.trim().len() >= 20,
                "夹具家 `{name}` 没写「它为什么可以住在 `agents/` 里却不算一家」"
            );
        }

        let reg = crate::agents::REGISTRY;
        let production_homes = in_tree.len() - FIXTURE_HOMES.len();
        assert_eq!(
            reg.len(),
            production_homes,
            "\n有 {} 个 agent 家（其中 {} 家是夹具），生产注册表却有 {} 条。\n\
             ⚠ 少一条 = 那家**永远不会出现在 `hello.homes` 里**，而编译、判据①②③④全绿 —— \n\
             `S6` 的最小假 agent 会一头撞上它。\n\
             ⚠ 多一条 = 注册表在指一个不存在的适配层，**或者夹具家混进了生产**\n\
             （后者更坏：真填 `homes` 那天后端会向仓外消费方声明一个不存在的 agent）。\n\
             家：{in_tree:?}｜夹具：{:?}｜注册的 kind：{:?}",
            in_tree.len(),
            FIXTURE_HOMES.len(),
            reg.len(),
            FIXTURE_HOMES.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            reg.iter().map(|a| a.kind).collect::<Vec<_>>()
        );
    }

    /// 假 agent 的每一种能力都在注册表里有一格（[`CAPABILITY_FACES`]），两边的名字集合一样。
    #[test]
    fn every_fake_capability_lives_in_a_registry_face() {
        use std::collections::BTreeSet;
        let known: BTreeSet<&str> = crate::agents::fake::CAPABILITIES.iter().copied().collect();
        let faced: BTreeSet<&str> = CAPABILITY_FACES.iter().map(|(c, _)| *c).collect();
        assert_eq!(
            faced.len(),
            CAPABILITY_FACES.len(),
            "`CAPABILITY_FACES` 里有重名的能力"
        );
        assert_eq!(
            faced, known,
            "\n`CAPABILITY_FACES` 与 `agents::fake::CAPABILITIES` 对不上。\n\
             ⚠ 多一种 = 登记了假 agent 没有的能力；少一种 = 有一种能力没说住注册表哪一格。"
        );
        for (cap, face) in CAPABILITY_FACES {
            assert!(!face.trim().is_empty(), "{cap} 没写住注册表哪一格");
        }
    }

    #[test]
    fn the_s4b_detectors_catch_synthetic_violations() {
        // ③ 正向：合成的违规签名必须被逮到。
        let bad = format!(
            "fn run(clau{}_dir: &Path, args: &[String]) -> i32 {{}}",
            "de"
        );
        assert!(
            agent_named_dir_needles()
                .iter()
                .any(|n| bad.contains(n.as_str())),
            "③ 的判定认不出合成的违规签名 —— 它此刻是空转的"
        );
        // ③ 反向：改名之后的正确写法**不许**被打中（假阳会训练人绕过判据）。
        let good = "fn run(agent_home: &Path, args: &[String]) -> i32 {}";
        assert!(
            !agent_named_dir_needles()
                .iter()
                .any(|n| good.contains(n.as_str())),
            "③ 把改名之后的正确写法判成了违规 —— 那是假阳"
        );
        // ④ 正向：适配层路径针认得出真调用。
        let call = format!(
            "crate::agents::clau{}code::records::is_session_file(p)",
            "de"
        );
        assert!(
            adapter_path_needles()
                .iter()
                .any(|n| call.contains(n.as_str())),
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
    /// ⑨ 通用层零处「按注册序取第一家」：`agents/mod.rs` 生产段里每一次在注册表上 `find` / `find_map`，
    /// 闭包都得按键比（`kind` · 路由名 · 名字）；不按键比的只许是下面登记的那几形（各带理由与处数，处数对不上就红 ——
    /// 收掉一处要同轮改这张表，多出一处要先说清楚为什么不是「取第一家」）。
    const FIRST_PICK_ALLOWED: &[(&str, usize, &str)] = &[
        (
            "is_default",
            2,
            "声明的缺省那一家（`LaunchFace::is_default`，`agents_tests` 钉着恰一家）",
        ),
        (
            "is_some_and(under)",
            1,
            "按路径：记录落在那一家合成历史的根下（各家的根不相交）",
        ),
    ];

    /// 按键比的那几形：闭包里有其一 ⇒ 是「问那一家」，不是「取第一家」。
    const KEYED: &[&str] = &["== kind", ".kind ==", "== route_id", "== name"];

    /// `src` 里每一次 `.find(` / `.find_map(` 的整个实参（括号配平）。
    fn find_calls(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        for pat in [".find(", ".find_map("] {
            let mut from = 0;
            while let Some(i) = src[from..].find(pat) {
                let open = from + i + pat.len() - 1;
                let mut depth = 0usize;
                let mut end = src.len();
                for (j, c) in src[open..].char_indices() {
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                end = open + j + 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                out.push(
                    src[open..end]
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                from = open + 1;
            }
        }
        out
    }

    /// 不按键比、也不在登记表里的那几次（＝ 取第一家）；登记表那几形各数到几处。
    fn first_picks(src: &str) -> (Vec<String>, Vec<usize>) {
        let mut bad = Vec::new();
        let mut seen = vec![0; FIRST_PICK_ALLOWED.len()];
        for call in find_calls(src) {
            if KEYED.iter().any(|k| call.contains(k)) {
                continue;
            }
            match FIRST_PICK_ALLOWED
                .iter()
                .position(|(m, _, _)| call.contains(m))
            {
                Some(i) => seen[i] += 1,
                None => bad.push(call),
            }
        }
        (bad, seen)
    }

    #[test]
    fn the_general_layer_never_takes_the_first_family_by_registry_order() {
        let (_, src) = sources()
            .into_iter()
            .find(|(rel, _)| rel == "agents/mod.rs")
            .expect("agents/mod.rs 不在扫描的人群里");
        let (bad, seen) = first_picks(&src);
        assert!(
            bad.is_empty(),
            "`agents/mod.rs` 里按注册序取了第一家（闭包不按 kind 比）：{bad:#?}\n\
             ⇒ 改成按 kind 取（入参带 kind，调用方说是哪一家），或逐家问。"
        );
        for ((marker, want, why), got) in FIRST_PICK_ALLOWED.iter().zip(&seen) {
            assert_eq!(
                got, want,
                "登记的「{marker}」（{why}）数到 {got} 处、登记 {want} 处 —— 收掉了就同轮改登记表"
            );
        }
    }

    /// ⑨ 的正控：判定真认得出「取第一家」，也不误伤按 kind 取。
    #[test]
    fn the_first_pick_detector_catches_a_synthetic_first_pick() {
        let first = "REGISTRY.iter().find_map(|a| a.assets.and_then(|f| (f.skills_root)()))\n\
                     registry.iter().find(|a| a.history.is_none()).and_then(|a| a.records)";
        let (bad, _) = first_picks(first);
        assert_eq!(bad.len(), 2, "认不出取第一家：{bad:?}");
        let keyed = "registry.iter().find(|a| a.kind == kind).and_then(|a| a.records)";
        let (bad, _) = first_picks(keyed);
        assert!(bad.is_empty(), "把按 kind 取判成了取第一家：{bad:?}");
    }
}
