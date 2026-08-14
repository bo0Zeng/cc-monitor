//! `S1`：**通用层不许知道任何一个 agent 的名字与文件格式**〔用@08-14〕。
//!
//! # 它守的是什么
//!
//! 用户 08-14 逐字：「**现在的daemon几乎都是兼容claudecode, 那就标清楚, 分离清楚,
//! 后面搞兼容其他agent的时候才方便**」。
//!
//! daemon 今天 21021 行里，agent 专属的知识（哪个目录、什么文件格式、账号叫什么）
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
    use std::path::{Path, PathBuf};

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
        "platform/pidwatch/mod.rs",
        "platform/fallback_guard.rs",
        "common/mod.rs",
        "wire.rs",
        "inbound.rs",
        // ── 以下由 `S3` 加入 ──────────────────────────────────────────
        // `platform/` 整层：`S3` 把 `proc_claude_config_dir` 参数化成 `proc_env_var(pid, name)`
        // 之后，这一层再没有任何一个 agent 的名字。⚠ 这是**整层**进表，不是挑干净的进 ——
        // 「唯一允许平台原语的层」恰恰最不该认识某个 agent 叫什么。
        "platform/proc.rs",
        "platform/liveness.rs",
        "platform/paths.rs",
        "platform/signal.rs",
        // `common/` 整层：`S3` 把 `projects_root` 搬进适配层之后 `common/paths.rs` 整个文件消失了
        //（它当初就违反 `common/` 三条门槛的第③条「无域知识」——`projects` 是 Claude 的布局）。
        "common/fs.rs",
    ];

    /// 人群下界：低于它说明取法坏了（路径写错 / 扩展名过滤掉）⇒ **红**，不是绿。
    ///
    /// ⚠ `S3` 把它从 5 抬到 10：**下界必须跟着覆盖面涨**，否则搬进来一批之后
    /// 「路径全写错」这种坏法仍然过得去（剩 5 个也满足旧下界）。
    const CORE_FILES_FLOOR: usize = 10;

    /// **已知欠账**（`文件:行内容片段`, 归哪一件, 为什么今天不动它）—— **递减棘轮**。
    ///
    /// 立表当天判据红出的就是这两条，它们**正是 `D3` 预言的那处**：协议把 agent 名字
    /// 焊进了字段名。改它要 bump 协议版本、两侧同改、动 `protocol_doc_guard` 的对拍
    /// ⇒ 归 `S4`，本件不顺手改（一功能一 commit）。
    ///
    /// 三条纪律，缺一条这张表就会变成许愿池：
    /// ① **命中不在表里 ⇒ 红**（新增违规立刻现形）；
    /// ② **表里的条目不再命中 ⇒ 也红**（修好了必须摘登记，否则表会攒成幽灵）；
    /// ③ 所以它**只会缩短**，不会变长 —— `S4` 落地那天这张表清零。
    const KNOWN_DEBT: &[(&str, &str, &str)] = &[
        (
            "wire.rs",
            "claude_dir",
            "协议字段名里带 agent 名（`D3`）⇒ 归 `S4`：换成 `agent_kind` + 通用 home。             改它要 bump 协议、两侧同改、动文档对拍，不在本件顺手做",
        ),
        (
            "wire.rs",
            "codex_dir",
            "同上；这两个并列字段正是 `D3` 说的「加第三个 agent 就要再加一个字段」的证据",
        ),
    ];

    fn src_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
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

    /// 一行是不是**真代码**（剥 `//` 注释）。
    ///
    /// ⚠ 只剥注释、**不剥字符串**：通用层里出现 `"claude"` 这个字符串字面量，
    /// 那正是要抓的东西（比如 `if kind == "claude"`）。
    fn production_lines(src: &str) -> Vec<(usize, String)> {
        crate::guard_support::production_code(src)
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
        // `文件:行号:命中的词:那一行` —— `S1-Y3` 要的是**逐条可核**，不是一个总数：
        // 只报数字的话，`S2`/`S3` 搬的时候没法对照，人会去改数字而不是搬代码。
        let mut hits: Vec<String> = Vec::new();
        for rel in CORE_FILES {
            let p = root.join(rel);
            let Ok(src) = std::fs::read_to_string(&p) else {
                continue; // 文件被挪走 ⇒ 下面的下界断言会红，比在这里 panic 诊断更清楚
            };
            scanned += 1;
            for (no, line) in production_lines(&src) {
                let low = line.to_lowercase();
                for (n, why) in needles() {
                    if low.contains(&n.to_lowercase()) {
                        hits.push(format!("  {rel}:{no}  [{why}] {}", line.trim()));
                        break; // 一行只报一次，免得同一行刷屏
                    }
                }
            }
        }
        assert!(
            scanned >= CORE_FILES_FLOOR,
            "只扫到 {scanned} 个通用层文件（下界 {CORE_FILES_FLOOR}）—— \
             取法坏了（路径写错？文件挪走了？），本断言此刻是**空转**的"
        );
        // 已知欠账挑出来（`S4` 会清掉它们）；剩下的才是**新增违规**。
        let (known, unknown): (Vec<String>, Vec<String>) = hits.into_iter().partition(|h| {
            KNOWN_DEBT
                .iter()
                .any(|(f, frag, _)| h.contains(f) && h.contains(frag))
        });
        // ★ 棘轮的第二条：**修好了必须摘登记**。否则这张表会攒成幽灵，
        //   而幽灵条目会让下一个人以为"这里还欠着"，进而不敢动。
        let ghosts: Vec<&str> = KNOWN_DEBT
            .iter()
            .filter(|(f, frag, _)| !known.iter().any(|h| h.contains(f) && h.contains(frag)))
            .map(|(frag, _, _)| *frag)
            .collect();
        assert!(
            ghosts.is_empty(),
            "`KNOWN_DEBT` 里这些条目**已经不再命中**了：{ghosts:?}\n\
             ⇒ 修好了就把它从表里摘掉（棘轮只许缩短）。留着 = 让下一个人以为这儿还欠着。"
        );
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
        // ⚠ 写两遍是**刻意的**：它让"把某个文件移出通用层"必须是一个**显式动作**
        //（同 `KNOWN_DEBT` 要手动摘登记）。
        //
        // ★ 这张表原名 `AT_BIRTH`，只装 `S1` 立表当天那 6 个 —— 于是 `S3` 后来加进
        // `CORE_FILES` 的 5 个**可以被静默删掉**（6 个元老还在、长度也够，两条断言全绿）。
        // **棘轮只棘到出生线，不棘到今天。** 这个洞是 `S3` 的变异台架逮出来的
        //（M3「把 platform/proc.rs 摘掉」原本零红），不是谁记起来的。
        const EVER_DECLARED_CORE: &[&str] = &[
            // S1 立表当天
            "platform/mod.rs",
            "platform/pidwatch/mod.rs",
            "platform/fallback_guard.rs",
            "common/mod.rs",
            "wire.rs",
            "inbound.rs",
            // S3 加入
            "platform/proc.rs",
            "platform/liveness.rs",
            "platform/paths.rs",
            "platform/signal.rs",
            "common/fs.rs",
        ];
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
            needles().iter().any(|(n, _)| low.contains(&n.to_lowercase()))
        });
        assert!(found, "判定认不出合成的违规样本 —— 正题那条此刻是空转的");
        assert!(!needles().is_empty(), "针表空了 ⇒ 正题恒绿");

        // 注释里的写法**不算**（判据的语料不许混进散文）
        let only_comment = "// 这里以前写的是 claude_dir.join(\"projects\")\nfn g() {}\n";
        let clean = production_lines(only_comment);
        let hit_in_comment = clean.iter().any(|(_, l)| {
            let low = l.to_lowercase();
            needles().iter().any(|(n, _)| low.contains(&n.to_lowercase()))
        });
        assert!(!hit_in_comment, "注释里的写法被当成了真代码");

        // ⚠ `@ccm_sid` **刻意不是针**：它是我们自己的 tmux 变量，不是任何 agent 的。
        let ours = "fn h() {\n    tmux(\"show-options\", \"@ccm_sid\");\n}\n";
        let ours_hit = production_lines(&ours.to_string()).iter().any(|(_, l)| {
            let low = l.to_lowercase();
            needles().iter().any(|(n, _)| low.contains(&n.to_lowercase()))
        });
        assert!(
            !ours_hit,
            "把我们自己的 `@ccm_sid` 误判成 agent 耦合了 —— 误伤会训练人绕过判据"
        );
    }
}
