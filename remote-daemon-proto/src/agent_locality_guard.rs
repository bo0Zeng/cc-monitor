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
//! ## ① 格式知识只许住 `agents/<名>/`
//!
//! 针取 **codex 专有**的六个（`rollout-` / `token_count` / `turn_context` /
//! `session_meta` / `event_msg` / `.codex`）。人群是**整棵 `src/`**（`scan_tree!` 自动摘掉本文件）。
//!
//! ⚠ **`sessions/` 刻意不是针**〔Bx 08-14 实测〕：`<claude_dir>/sessions/<PID>.json` 是
//! **Claude 的 pidfile 目录**（`watcher` 10 处 · `accounts_query` 5 处），
//! 而 Codex 的 `sessions/` 是它的会话记录根。**同一个词、两套语义**，它区分不了谁是谁。
//! （它在 `S1` 的判据里当针仍然对 —— 那条针问的是「通用层知不知道会话住哪个目录」，
//! **不问是谁的目录**。★ 针由那条判据要答的问题定，不由词定。）
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

    /// codex 知识的唯一合法住址（相对 `src/`，前缀匹配）。
    const HOME: &str = "agents/codex/";

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

    /// 针：**codex 专有**，运行时拼（本文件的散文里就有这些词）。
    fn needles() -> Vec<(String, &'static str)> {
        vec![
            (format!("rollo{}", "ut-"), "会话文件命名"),
            (format!("token_{}", "count"), "用量事件名"),
            (format!("turn_{}", "context"), "记录类型名"),
            (format!("session_{}", "meta"), "记录类型名"),
            (format!("event_{}", "msg"), "记录类型名"),
            (format!(".cod{}", "ex"), "会话目录名"),
        ]
    }

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

    /// ① codex 的格式知识只许住 `agents/codex/`。
    #[test]
    fn codex_format_knowledge_lives_in_exactly_one_place() {
        let files = sources();
        assert!(
            files.len() >= 20,
            "只遍历到 {} 个源文件 —— 遍历坏了，本断言在空转",
            files.len()
        );
        let needles = needles();
        let mut leaks: Vec<String> = Vec::new();
        for (rel, prod) in &files {
            if rel.starts_with(HOME) {
                continue; // 唯一住址
            }
            for (line_no, line) in prod.lines().enumerate() {
                for (n, what) in &needles {
                    if line.contains(n.as_str()) {
                        leaks.push(format!("{rel}:{}  [{what}] {}", line_no + 1, line.trim()));
                    }
                }
            }
        }
        assert!(
            leaks.is_empty(),
            "codex 的格式知识跑到 `{HOME}` 之外去了（{} 处）：\n  {}\n\
             ⚠ 那些名字是 **Codex 的文件格式本身**（信封/事件名/文件命名/目录名）。\n\
             它们散在通用层里，「接第三个 agent 要改哪几处」就又只住在人的脑子里了 —— \n\
             那正是 `S2` 要消灭的病。搬进 `{HOME}`，通用层通过适配层取。",
            leaks.len(),
            leaks.join("\n  ")
        );
    }

    /// ② 通用层里的 kind 派发点，登记表与实得**逐条对齐**（多一处红、少一处也红）。
    #[test]
    fn kind_dispatch_sites_are_enumerated_one_by_one() {
        let files = sources();
        let lit = kind_literal();
        let mut hits: Vec<String> = Vec::new();
        for (rel, prod) in &files {
            if rel.starts_with(HOME) {
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
