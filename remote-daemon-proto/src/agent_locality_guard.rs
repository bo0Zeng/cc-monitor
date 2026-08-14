//! `S2`：**一个 agent 的知识只许有一个住址，通用层里认它的地方逐条登记**〔用@08-14〕。
//!
//! # 它守的是什么 —— 病灶不是"散"，是"数不清"
//!
//! `S2` 开工前实测：Codex 的知识在三处，而**三处长得完全不一样**：
//! `observe/codex.rs` 是抽取器、`observe/usage_query.rs` 里是
//! `aggregate(claude_dir).and_then(|()| aggregate_codex())`、
//! `control/resolve_query.rs` 里是 `spec.agent_kind.trim() == "codex"`。
//! grep 出其中任意一处，**都找不到另外两处**。
//!
//! ⇒ 「接第三个 agent 要改哪几处」这份清单，今天只住在人的脑子里。
//! 本模块把它变成两张表 —— 而**表是会红的**。
//!
//! # 两条判据，分别守两种泄漏
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
//! # ⚠ 诚实边界（三条，都写在这里而不是只写在计划里）
//!
//! 1. 判据认的是**字面量**，不是语义。派发点里的人直接写
//!    `format!("{base} resume {sid}")` 仍然不会红 —— 堵死它要等 `S3` 凑齐两个实现后立接口（`D4`）。
//! 2. 只覆盖 **codex**。Claude 那半（`watcher`/`accounts_query`/`history_query` 里的
//!    `projects/`、`.jsonl`、账号布局）今天**一处都没被这条判据管**，归 `S3`。
//! 3. `production_code` 剥掉注释与 `#[cfg(test)] mod` ⇒ **文档与测试里怎么写都不红**。
//!    这是刻意的（判据自己的散文里就有这些词），代价是"只在测试里泄漏格式知识"逮不到。
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
}
