//! `S1`：**通用层不许知道任何一个 agent 的名字与文件格式**〔用@08-14〕。
//!
//! # 它守的是什么
//!
//! 「**现在的后端几乎都是兼容claudecode, 那就标清楚, 分离清楚,
//! 后面搞兼容其他agent的时候才方便**」。
//!
//! backend 今天 21021 行里，agent 专属的知识（哪个目录、什么文件格式、账号叫什么）
//! **散在每一层**（Bx 实测：顶层 30 · `observe/` 111 · `control/` 23 · `platform/` 1 · `common/` 2）。
//! 目标形态是「通用内核 + 每个 agent 一份适配」，而**没有判据的重构会被稀释** ——
//! 搬完当天干净，下一个人往 `watcher.rs` 里加一行 `claude_dir.join("projects")` 就回去了，
//! 而**没有任何东西会红**。
//!
//! # 人群：**逐文件登记**，不是目录规则
//!
//! Bx 摸底否掉了「`observe/` 脏、其余干净」这个想法：`platform/` 与 `common/` 也有命中。
//! ⇒ 谁是"通用层"由 [`CORE_FILES`] 逐条列举。默认**不在表里的文件不扫** ——
//! 这不是漏洞，是**分阶段搬迁的把手**：`S2`/`S3` 每搬完一块，就把那块加进表里，
//! 判据的覆盖面**只增不减**（`the_core_file_list_only_grows` 钉住）。
//!
//! # 针：6 个，逐条判过，**不一把梭**
//!
//! | 针 | 为什么是它 |
//! |---|---|
//! | agent 名字 ×2 | 最强信号 |
//! | 目录/格式知识 ×4 | 「会话住哪个目录、什么后缀、账号变量叫什么」正是要赶出通用层的东西 |
//!
//! ⚠ **`@ccm_sid` 刻意不是针**：它是**我们自己**在 tmux 上设的变量（`control/` 里 10 处，
//! 是 §34 三道门用的），与 agent 无关。一把梭会把它误伤成"耦合"，
//! 而误伤会训练人绕过判据 —— 那比没有判据更坏（skill 铁律 18）。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    /// **已经宣称"通用"的文件**（相对 `src/`）。不在表里的今天不扫。
    ///
    /// ⚠ 这张表是**搬迁进度表**，不是白名单：`S2`/`S3` 每把一块的 agent 知识搬进
    /// `agents/*`，就把那块加进来。**只增不减**（下面那条判据钉住）。
    ///
    /// 今天先放 Bx 实测**最干净**的两层 —— 它们的命中数分别是 1 和 2，
    /// 是"随手就能清掉"的量级；先从它们开始，让判据从第一天就在真实地守着东西，
    /// 而不是挂一个全红的清单等人来看（全红 = 没人看 = 等于没有）。
    const CORE_FILES: &[&str] = &[
        "platform/mod.rs",
        // `platform/fallback_guard.rs` 出表：它是判据，整份搬去了 `tests/backend/platform/`（不是通用层的生产文件）。
        "common/mod.rs",
        "wire/mod.rs",
        "stream/inbound/mod.rs",
        // ── 以下由 `S3` 加入 ──────────────────────────────────────────
        // `platform/` 整层：读进程环境只经 `proc_env_var(pid, name)`，这一层没有任何一个 agent 的名字。⚠ 这是**整层**进表，不是挑干净的进 ——
        // 「唯一允许平台原语的层」恰恰最不该认识某个 agent 叫什么。
        "platform/proc.rs",
        "platform/liveness.rs",
        "platform/paths.rs",
        "platform/signal.rs",
        // `common/` 整层：`S3` 把 `projects_root` 搬进适配层之后 `common/paths.rs` 整个文件消失了
        //（它当初就违反 `common/` 三条门槛的第③条「无域知识」——`projects` 是 Claude 的布局）。
        "platform/fs/ro.rs",
        // ── `S4b`：**这一轮一个都没加，而这是量出来的，不是忘了** ────────────
        //
        // `S4`/`L1` 都写着「`claude_dir` 改完名，`watcher`/`accounts_query`/`history_query`
        // 就能进表」。`S4b` 改完名之后**实测这三个文件**，那句话**不成立**：
        // 六根针下还剩 **6 / 5 / 5** 处（共 16），其中
        //   · **12 处是 `crate::agents::claudecode::…`** —— 适配层的**地址**本身
        //     （`history_query` 那 5 处全是；它的 `claude` 一词只剩这一种来源）；
        //   · 4 处是散文（`watcher` 两句 warn 里的 `claude` 一词、`accounts_query` 两处
        //     `sessions/` 文案）。
        //
        // ⇒ **刻意不放宽针去凑这三个**。`S3` 设计的中间态（机器留在通用层、知识住适配层）
        // 让通用层用「一次函数调用」拿知识 —— 而那次调用**写死了一个 agent 的名字**，
        // 它是真耦合，不是假阳：加第三个 agent 时这 12 处每一处都要回来看。
        // 把它豁免掉，本判据的正题就从「通用层不认识 agent」滑成「通用层不认识 agent 的**目录布局**」。
        //
        // 卡点因此从 `S4b` 移交 `L2`/`S6`（立接口）。那 12 处 + 另外 15 处同族已由
        // `agent_locality_guard::ADAPTER_CALL_SITES` **逐条登记**（共 8 文件 / 27 处），
        // 收进接口一处、那张表短一条、这里就能进一个文件。
        //
        // ── **`plugin/` 整层在表里** ─────────────────────
        //
        // `CORE_FILES` 是 **opt-in** 的，不在表里的文件**不扫** ⇒ 新造一个自称「通用」的层
        // 而不进表，「通用层不许知道任何一个 agent 的名字」这条性质对它就是**空的**，
        // 而且**没有任何机检能逼人加** ⇒ 新造的层从第一天就进表。
        //
        // ⇒ 进表不是形式：它约束了接口形状。插件的候选安装路径里逐字带着
        // agent 的名字，所以**候选列表只能是入参**，不能住进 `plugin/` —— 这条判据是把它
        // 挡在门外的那道门（另一条独立理由是两个真实实现的查找顺序本来就不同）。
        "plugin/mod.rs",
        "plugin/discover.rs",
        "plugin/invoke.rs",
        // `common/tmux_utf8.rs`（「tmux 的打印通道必须是 UTF-8」那个口径的家）：
        // 同 `plugin/`，新造的东西从第一天就进表 —— 本表是 opt-in 的，不在表里就**不扫**。
        "common/tmux_utf8.rs",
        // 入方向是一个目录：其中每一份（含命令表各族）都在表里。
        "stream/inbound/doors.rs",
        "wire/drain.rs",
        "stream/inbound/sniff.rs",
        "wire/spec.rs",
        "stream/inbound/registry/accounts.rs",
        "stream/inbound/registry/aliases.rs",
        "stream/inbound/registry/assets.rs",
        "stream/inbound/registry/bus.rs",
        "stream/inbound/registry/file_manager.rs",
        "stream/inbound/registry/history.rs",
        "stream/inbound/registry/link.rs",
        "stream/inbound/registry/machine.rs",
        "stream/inbound/registry/terminals.rs",
    ];

    /// **整棵进表的子树**（相对 `src/`，按前缀认）：其下每一份 `.rs` 都是通用层，**新长出来的文件从第一天就在扫**
    ///（[`CORE_FILES`] 是逐条的，新文件不进表就不扫 —— `KY6` 那个洞；整棵进表把它堵上）。
    ///
    /// 这三块是「读会话」的核心：记录怎么读成清单行 · 搜索索引 · 骨架索引 · 判活 · 起会话 / 恢复。
    /// 它们只认适配层交来的那几种行（进程状态那一份 `ProcFact` · 一行记录里会话级那几格），**不认那一家的文件格式**。
    /// 同 [`CORE_FILES`]：只增不减（`the_core_file_list_only_grows` 钉住）。
    const CORE_TREES: &[&str] = &["observe/", "history/", "control/session_"];

    fn src_dir() -> PathBuf {
        guard_core::addr::backend_src_root()
    }

    /// 针 —— **运行时拼**。
    ///
    /// 本文件的头注里写着这些词（它得解释自己在防什么），直接写字面量会**读到判据自己**。
    /// 本仓栽过四次（`P4b §6` 那几条头注逐条记着）。
    fn needles() -> Vec<(String, &'static str)> {
        vec![
            (format!("clau{}", "de"), "agent 名字"),
            (format!("cod{}", "ex"), "agent 名字"),
            (format!("{}jsonl", "."), "会话文件后缀"),
            (format!("proje{}", "cts/"), "会话目录布局"),
            (format!("sessi{}", "ons/"), "会话目录布局"),
            (format!("CLAUDE_CONFIG{}", "_DIR"), "账号环境变量"),
        ]
    }

    /// 针之二：**那一家原文里的字段名**（进程状态文件与会话记录），按带引号的字面量认、大小写照原样 ——
    /// 读字段只有 `get("…")` · `["…"]` · `rename = "…"` · `json!` 键这几形，都带引号；
    /// 不带引号的同名标识符（线上类型 `ForkedFrom` · 蛇形的 `ai_title`）不是在读原文，不算。
    ///
    /// 通用层要这些事实，只经适配层交来的那一种行（`ProcFact` · `SessionLine`）拿；字段叫什么只住 `agents/<名>/`。
    fn field_needles() -> Vec<(String, &'static str)> {
        vec![
            (format!("\"proc{}\"", "Start"), "进程状态文件的字段名"),
            (format!("\"waiting{}\"", "For"), "进程状态文件的字段名"),
            (format!("\"statusUpdated{}\"", "At"), "进程状态文件的字段名"),
            (format!("\"ai{}\"", "Title"), "会话记录的字段名"),
            (format!("\"session{}\"", "Kind"), "会话记录的字段名"),
            (format!("\"forked{}\"", "From"), "会话记录的字段名"),
        ]
    }

    /// 这一行扎到哪一根针（两套针各自的认法）；一根都没扎到 ⇒ `None`。
    fn hit_of(line: &str) -> Option<&'static str> {
        let low = line.to_lowercase();
        needles()
            .into_iter()
            .find(|(n, _)| low.contains(&n.to_lowercase()))
            .or_else(|| {
                field_needles()
                    .into_iter()
                    .find(|(n, _)| line.contains(n.as_str()))
            })
            .map(|(_, why)| why)
    }

    /// 通用层今天的全部文件：[`CORE_FILES`] 逐条的 ＋ [`CORE_TREES`] 每棵底下的每一份 `.rs`（去重、排序）。
    /// 每棵树都得至少认出一份（认不出 ⇒ 前缀写错了，那一棵在空转）。
    fn core_files() -> Vec<String> {
        let all = guard_core::files_by_extension(&src_dir(), "rs");
        let mut out: Vec<String> = CORE_FILES.iter().map(|f| (*f).to_string()).collect();
        for tree in CORE_TREES {
            let under: Vec<&String> = all.iter().filter(|f| f.starts_with(tree)).collect();
            assert!(
                !under.is_empty(),
                "通用层子树 `{tree}` 底下一份 `.rs` 都没认出 —— 前缀写错了 / 目录挪走了，那一棵此刻在空转"
            );
            out.extend(under.into_iter().cloned());
        }
        out.sort();
        out.dedup();
        out
    }

    /// 一行是不是**真代码**（剥 `//` 注释）。
    ///
    /// ⚠ 只剥注释、**不剥字符串**：通用层里出现 `"claude"` 这个字符串字面量，
    /// 那正是要抓的东西（比如 `if kind == "claude"`）。
    fn production_lines(src: &str) -> Vec<(usize, String)> {
        guard_core::production_code(src)
            .lines()
            .enumerate()
            .map(|(i, l)| (i + 1, l.to_string()))
            .filter(|(_, l)| !l.trim().is_empty())
            .collect()
    }

    /// ★ 正题：**已宣称通用的文件里，一个 agent 专属字面量都不许有**。
    #[test]
    fn no_agent_specific_literal_in_the_core_layer() {
        let root = src_dir();
        let mut scanned = 0usize;
        // `文件:行号:命中的词:那一行` —— 要的是**逐条可核**，不是一个总数：
        // 只报数字的话，搬代码的时候没法对照，人会去改数字而不是搬代码。
        let mut hits: Vec<String> = Vec::new();
        let files = core_files();
        for rel in &files {
            let p = root.join(rel);
            let Ok(src) = std::fs::read_to_string(&p) else {
                continue; // 文件被挪走 ⇒ 下面那条「每一份都读到了」会红，比在这里 panic 诊断更清楚
            };
            scanned += 1;
            for (no, line) in production_lines(&src) {
                // 一行只报一次（扎到的第一根针），免得同一行刷屏
                if let Some(why) = hit_of(&line) {
                    hits.push(format!("  {rel}:{no}  [{why}] {}", line.trim()));
                }
            }
        }
        assert_eq!(
            scanned,
            files.len(),
            "通用层清单 {} 份里只读到 {scanned} 份 —— \
             取法坏了（路径写错？文件挪走了？），本断言此刻有一部分在**空转**",
            files.len()
        );
        let unknown = hits;
        assert!(
            unknown.is_empty(),
            "通用层里出现了**新的** agent 专属字面量（{} 处）：\n{}\n\n\
             ⇒ 通用层不许知道任何一个 agent 的名字、目录布局或文件格式。\n\
             正确做法是把这段搬进 `agents/<名>/`，让通用层只认抽象（`agent_kind` 那类**值**）。\n\
             ⚠ 实在搬不动的，**先别改这条判据** —— 先在功能件里写清为什么，再谈例外。",
            unknown.len(),
            unknown.join("\n")
        );
    }

    /// ★ 覆盖面**只增不减**：这张表是搬迁进度表。
    ///
    /// 没有这条的话，最省事的"修法"是把红了的文件从表里删掉 ——
    /// 那正是本仓一路在防的「改数字了事」（`readonly_guard` 用 `assert_eq!` 而不是地板，同理）。
    #[test]
    fn the_core_file_list_only_grows() {
        // **曾经被宣称为通用层的每一个文件**。加进 `CORE_FILES` 的同一轮要追加到这里。
        //
        // ⚠ 写两遍是**刻意的**：它让"把某个文件移出通用层"必须是一个**显式动作**。
        //
        // ★ 只装立表当天那几个的话，后来加进 `CORE_FILES` 的**可以被静默删掉**
        //（元老还在、长度也够，两条断言全绿）：棘轮要棘到今天，不只棘到出生线。
        const EVER_DECLARED_CORE: &[&str] = &[
            "platform/mod.rs",
            "common/mod.rs",
            "wire/mod.rs",
            "stream/inbound/mod.rs",
            "platform/proc.rs",
            "platform/liveness.rs",
            "platform/paths.rs",
            "platform/signal.rs",
            // 读盘的只读那一份
            "platform/fs/ro.rs",
            // `plugin/` 整层
            "plugin/mod.rs",
            "plugin/discover.rs",
            "plugin/invoke.rs",
            // tmux 打印通道的口径
            "common/tmux_utf8.rs",
            // 入方向那个目录与命令表各族
            "stream/inbound/doors.rs",
            "wire/drain.rs",
            "stream/inbound/sniff.rs",
            "wire/spec.rs",
            "stream/inbound/registry/accounts.rs",
            "stream/inbound/registry/aliases.rs",
            "stream/inbound/registry/assets.rs",
            "stream/inbound/registry/bus.rs",
            "stream/inbound/registry/file_manager.rs",
            "stream/inbound/registry/history.rs",
            "stream/inbound/registry/link.rs",
            "stream/inbound/registry/machine.rs",
            "stream/inbound/registry/terminals.rs",
        ];
        // 整棵进表的子树（[`CORE_TREES`]），同上只增不减、两张表同轮改。
        const EVER_DECLARED_TREES: &[&str] = &[
            // 读会话的核心三块整棵进表
            "observe/",
            "history/",
            "control/session_",
        ];
        let gone: Vec<&str> = EVER_DECLARED_TREES
            .iter()
            .filter(|t| !CORE_TREES.contains(t))
            .copied()
            .collect();
        assert!(
            gone.is_empty() && CORE_TREES.len() == EVER_DECLARED_TREES.len(),
            "通用层子树清单变了：少了 {gone:?}，或多了没追加进棘轮表的（{} vs {}）—— \
             两张表必须同轮改，删一棵是把判据的覆盖面缩小",
            CORE_TREES.len(),
            EVER_DECLARED_TREES.len()
        );
        let missing: Vec<&str> = EVER_DECLARED_CORE
            .iter()
            .filter(|f| !CORE_FILES.contains(f))
            .copied()
            .collect();
        assert!(
            missing.is_empty(),
            "这些文件被从通用层清单里**删掉了**：{missing:?}\n\
             ⇒ 删表是把判据的覆盖面缩小，等于把红变绿而问题还在。\n\
             真要移出（比如那个文件确实成了 agent 专属），在功能件里写清楚再动这条断言。"
        );
        assert_eq!(
            CORE_FILES.len(),
            EVER_DECLARED_CORE.len(),
            "\n`CORE_FILES` 与棘轮表长度对不上（{} vs {}）。\n\
             多出来 ⇒ 有人加了通用层文件却没追加进棘轮表，那一条从此可以被静默删掉；\n\
             少了 ⇒ 上面那条会先红。**两张表必须同轮改**。",
            CORE_FILES.len(),
            EVER_DECLARED_CORE.len()
        );
    }

    /// ★ 反向夹具：**这条判据真的会红**。
    ///
    /// 没有这一格的话，`needles()` 哪天被改坏（比如返回空 vec），正题那条会**安静地全绿**。
    #[test]
    fn the_detector_itself_catches_a_synthetic_violation() {
        let bad = "fn f() {\n    let p = home.join(\".claude\").join(\"projects\");\n}\n";
        let lines = production_lines(bad);
        let found = lines.iter().any(|(_, l)| {
            let low = l.to_lowercase();
            needles()
                .iter()
                .any(|(n, _)| low.contains(&n.to_lowercase()))
        });
        assert!(found, "判定认不出合成的违规样本 —— 正题那条此刻是空转的");

        // 字段名那一套针：读原文字段（带引号）要扎到；同名的线上类型 / 蛇形标识符不扎。
        let reads = format!("let t = v.get(\"ai{}\");", "Title");
        assert_eq!(
            hit_of(&reads),
            Some("会话记录的字段名"),
            "读原文字段名没认出来"
        );
        let typed = "let f = cmd_wire::listing::ForkedFrom { label }; let t = row.ai_title;";
        assert_eq!(
            hit_of(typed),
            None,
            "把线上类型 / 蛇形标识符误判成读原文字段"
        );
        assert!(!needles().is_empty(), "针表空了 ⇒ 正题恒绿");

        // 注释里的写法**不算**（判据的语料不许混进散文）
        let only_comment = "// 这里以前写的是 claude_dir.join(\"projects\")\nfn g() {}\n";
        let clean = production_lines(only_comment);
        let hit_in_comment = clean.iter().any(|(_, l)| {
            let low = l.to_lowercase();
            needles()
                .iter()
                .any(|(n, _)| low.contains(&n.to_lowercase()))
        });
        assert!(!hit_in_comment, "注释里的写法被当成了真代码");

        // ⚠ `@ccm_sid` **刻意不是针**：它是我们自己的 tmux 变量，不是任何 agent 的。
        let ours = "fn h() {\n    tmux(\"show-options\", \"@ccm_sid\");\n}\n";
        let ours_hit = production_lines(ours).iter().any(|(_, l)| {
            let low = l.to_lowercase();
            needles()
                .iter()
                .any(|(n, _)| low.contains(&n.to_lowercase()))
        });
        assert!(
            !ours_hit,
            "把我们自己的 `@ccm_sid` 误判成 agent 耦合了 —— 误伤会训练人绕过判据"
        );
    }
}
