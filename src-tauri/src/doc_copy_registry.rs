//! **散文里的数字副本清账**〔audit-0805 F18 下半，报告 §4.2〕。
//!
//! # 定框 E12 说的两条路，这里把第二条变成机检
//!
//! E12 逐字：文档修法只有两种 —— ① **送进一条会红的判据**，或 ② **删掉副本只留指针**。
//! 「改对一份散文数字」**不算修完**，因为下一次数字变了它照样会腐。
//!
//! V6 逐行核完 16 行：**准确的读数背后都有一条会红的判据**
//! （`BACKEND_FILES` · `NODE_SUITES` · `readers==7` · `ALLOWED` · `panel-groups` 逐页清单 …），
//! **已假的 13 处背后一条都没有**。⇒ 分类不是品味问题，是可判定的：**有没有机检读它**。
//!
//! # 本模块钉的是走第二条路的那些
//!
//! 「只剩指针」这件事本身**需要一条判据**，否则副本随时会被写回来 ——
//! `src-tauri/README.md` 就是活样本：它上一段刚写「本文件**刻意不复制**那份清单」，
//! 下一段就存了个 `11`（实际 15）。**声明纪律不等于纪律。**
//!
//! ⚠ 本模块**不判「数字对不对」**（那是走第一条路的那些的事），
//! 只判「**这个数字根本不该出现在这里**」。

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf()
    }

    /// 散文文件集 —— 数字副本只在这些地方治（源码注释另有各自的判据）。
    const PROSE_FILES: &[&str] = &[
        "README.md",
        "README.en.md",
        "src-tauri/README.md",
        "doc/CONTRIBUTING.md",
        "doc/DEVELOPMENT.md",
        "doc/ARCHITECTURE.md",
        // F18 下半新纳入的四份 —— 上半只覆盖了 V6 十六行里前五行涉及的文件，
        // 而剩下十一行的副本大半住在这四份里（`doc_claim_registry` 的文件集
        // 只有 `doc/` **直接子层**，`e2e/README.md` 连它都够不着）。
        "doc/INVARIANTS.md",
        "doc/IPC-PROTOCOL.md",
        "doc/REMOTE-PHASE0-DEPLOY.md",
        "e2e/README.md",
    ];

    /// 已按 **E12 第二条路**处置（删副本、只留指针）的事实。
    ///
    /// `(事实, 不许再出现的形态, 必须留着的指针片段, 家在哪)`
    ///
    /// 「不许再出现的形态」是**前缀**：命中之后若紧跟（跳过空格的）阿拉伯数字 ⇒ 红。
    /// ⚠ 用前缀而不是正则，是因为本仓没有 `regex` 直接依赖 ——
    /// **不为一条判据加供应链**（同 `ts-rs` 只进 dev 侧的那条论证）。
    #[allow(clippy::type_complexity)]
    const POINTER_ONLY: &[(&str, &[&str], &str, &str)] = &[
        (
            "各套测试的条数",
            &[
                "后端 cargo ",
                "远端 daemon ",
                "vitest ",
                "DOM 单测（",
                "DOM tests (",
                "code-picture-core (",
                "code-picture-core ",
            ],
            "条数以实跑为准",
            "实跑 + `doc/DEVELOPMENT.md` 那张表给命令；CI 侧的地板行在 `ci.yml`（那些有判据看着）",
        ),
        (
            "CI job 数",
            &["CI 共 ", "CI green across "],
            "CI 全绿",
            "`.github/workflows/ci.yml` 的 job 列表本身",
        ),
        (
            "daemon 生产段起进程的处数",
            &["今天清单上有 ", "清单上共 "],
            "清单本身就是家",
            "`readonly_guard.rs` 的 `ALLOWED` + 那条 `SPAWN_SITES_TODAY` 相等断言（不是地板）",
        ),
        (
            "e2e 各套件的断言数地板",
            &[
                "tmux-target ",
                "ccm-cli ",
                "ccm-acceptance ",
                "usage-probe ",
                "daemon-gate2 ",
                "graylight-frames ",
            ],
            "套数与地板值一律不抄在这里",
            "`ci.yml` 里 `run: bash e2e/assert-pass-floor.sh <套件> <地板>` 那 19 行",
        ),
        (
            "`backend/` 下 `.rs` 的个数",
            &["`backend/` 下今天有 "],
            "刻意不写它有几个",
            "`backend/mod.rs` 的 `BACKEND_FILES`（`every_file_under_backend_is_registered_with_a_reason` 看着）",
        ),
    ];

    /// 已按 **E12 第一条路**处置（有一条会红的判据读它）的事实 —— 判据名必须真的还在。
    ///
    /// V6 逐条对上的那批。这里不重复它们的值，只钉「**那条判据还活着**」。
    const HAS_A_GUARD: &[(&str, &str)] = &[
        (
            "`backend/` 下 `.rs` 清单",
            "every_file_under_backend_is_registered_with_a_reason",
        ),
        ("node 套件组数", "NODE_SUITES"),
        ("本机读取面 reader 文件数", "local_read_surface_registry"),
        ("daemon 生产 `Command::new` 处数", "ALLOWED"),
        (
            "`tmux ls` 格式串双写点",
            "tmux_ls_fmt_double_write_point_stays_in_sync",
        ),
        (
            "wire 帧 kind 清单",
            "every_wire_frame_kind_has_a_row_in_the_frame_table",
        ),
        // ⚠ 「e2e 套件名单」那条判据 **住在本文件里**，而本文件在扫描时被摘除
        // （否则表里写着的符号名会让每一条都在自己身上找到自己 —— F23 那一族）。
        // ⇒ 它不进这张表：住在本文件里的判据由**编译**保证还在，不需要再查一遍。
        ("daemon 生产段起进程的处数", "SPAWN_SITES_TODAY"),
        ("设置面板逐页清单", "pageTitles"),
    ];

    /// V6 那张表的总行数（台账标题写「十八行」，实际列出 **16**）。
    const TOTAL_ROWS: usize = 16;

    /// V6 十六行里**刻意不做**的，连理由一起登记。
    ///
    /// ⚠ 这张表存在的意义是：**「没处置」与「决定不处置」是两回事**。
    /// 少了它，棘轮就得靠「已处置数 == 全表数」收尾，而那会逼人去改不该改的东西。
    const NOT_DOING: &[(&str, &str)] = &[(
        "#10 `build_local_ps_command` 行号的第三份副本",
        "那份副本住在 `项目审阅报告-PhaseG-2026-07-29.md` —— **带日期的历史报告**。\
         改它等于篡改当时的记录；报告是快照，不是活文档。\
         另两份活副本已在 F18 上半处置。",
    )];

    /// ★ **棘轮的地板**：已处置行数只许涨。
    ///
    /// ⚠ 第一版写的是「未处置数 `STILL_PROSE_ROWS <= 11`」，**clippy 当场指出那是恒真断言**
    /// （`this assertion has a constant value`）—— 常量与自己比永远成立，
    /// 那不是棘轮，是一句装饰。与 F06 那次被 clippy 咬中的同义反复**同一个形状**。
    /// ⇒ 改成从**表**导出：`DONE_ROWS` 的真实条数与这个地板比，加一行才降得下未处置数。
    const DONE_FLOOR: usize = 15;

    /// V6 十六行里已处置的。未处置数 = `TOTAL_ROWS` − 本表条数 − `NOT_DOING` 条数。
    const DONE_ROWS: &[&str] = &[
        "#1 门禁怎么跑（`--all` 缺 vendor 排除 → 已订正为 `--workspace --exclude`）",
        "#2 workspace 测试总数（删副本留指针）",
        "#3 node 套件组数（★ F18 上半顺手做掉但没登记 —— 下半复核时才发现，台账是筛子不是免检章）",
        "#4 vitest DOM 数（删副本留指针）",
        "#5 CI job 数（删副本留指针）",
        "#6 e2e 套数与逐套地板（删副本留指针 + 新增套件名单机检）",
        "#7 reader 文件数（`local_read_surface_registry` 头注 11 vs 同文件机检 7 —— 一个文件内部自相矛盾；已删副本留指针）",
        "#8 daemon 生产 `Command::new` 处数（散文删副本；★ 判据从地板 `>= 4` 收紧为相等 —— 地板在变大方向上是瞎的）",
        "#9 `backend/` 下 `.rs` 数（删副本留指针）",
        "#11 主题 token 数（README 与 IPC-PROTOCOL 两处 13 → 删副本，家在 `theme.ts` 的 `TOKENS`，实为 14）",
        "#12 `__ccm_rbind`（`REMOTE-PHASE0-DEPLOY.md` 仍在教用户调一个全仓没有定义的函数 → 已改写）",
        "#13 远端 `↺` 行为（「monitor 无法在远端开交互 TTY」已假 → 已改写并指向 README 权威条）",
        "#14 远端分叉支持（README「仅本地会话」已假 → 已订正）",
        "#15 设置面板结构（「5 大折叠分组」在 v3.5.0 IA 重做后已不存在 → 整节重写为指针）",
        "#16 Tauri State 矩阵（**本来就是做对了的样本** —— 无副本、只有指针，复核后如实登记）",
    ];

    /// 本文件自己的路径 —— 扫符号时要摘出去（表里写着那些符号名）。
    /// 「事实 → 关键词」：**数量形态**出现即红，不管连接词怎么写。
    ///
    /// # 〔audit-0805 08-06〕它补的是 [`POINTER_ONLY`] 的**锚点腐坏**那一面
    ///
    /// 那张表按**精确前缀**取样（`"CI 共 "` / `"今天清单上有 "` …）。实测：
    /// 把同一个副本写成 `CI 一共 7 个 job` / `CI 目前有 7 个 job` / `本仓 CI 是 7 个 job`，
    /// **三种自然改写全部逃逸**，而 `CI 共 7 个 job` 当场被逮。
    ///
    /// ⇒ 这正是本区刚归纳出的那个形状：**被测对象正常演进的方向会系统性地把成员移出人群。**
    /// 而散文的正常演进就是**改写措辞** —— 按措辞取样的判据，注定随改写静默失效。
    ///
    /// # 人群怎么定的（先量后定，两次）
    ///
    /// 第一版想按「关键词 ±14 字符内出现数字」取样 —— 量完**否掉**：
    /// `e2e` / `Batch7` / `jsdom` / `14 项` 这类无关数字全被卷进来，噪声压过信号。
    /// 收紧成**数量形态**（数字与关键词紧邻，中间只许量词/连接符）后，
    /// 12 个候选关键词在今天的 10 份散文里**零误红**（逐个量过），三种改写仍全部命中。
    const QUANTITY_KEYWORDS: &[(&str, &str)] = &[
        ("CI job 数", "job"),
        ("各套测试的条数", "vitest"),
        ("各套测试的条数", "cargo"),
        ("e2e 各套件的断言数地板", "tmux-target"),
        ("e2e 各套件的断言数地板", "ccm-cli"),
        ("e2e 各套件的断言数地板", "ccm-acceptance"),
        ("e2e 各套件的断言数地板", "usage-probe"),
        ("e2e 各套件的断言数地板", "daemon-gate2"),
        ("e2e 各套件的断言数地板", "graylight-frames"),
        ("reader 文件数", "reader"),
        ("主题 token 数", "token"),
    ];

    /// 关键词紧邻数字（前：`7 个 job`；后：`job：7`）⇒ 返回那一小段上下文。
    fn quantity_form_hit(hay: &str, kw: &str) -> Option<String> {
        let ident = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
        let mut from = 0usize;
        while let Some(i) = hay[from..].find(kw) {
            let at = from + i;
            from = at + kw.len();
            // 词边界：`jobId` / `subtoken` 不算。
            //
            // ⚠ **英文复数要放行**（`13 tokens` / `7 jobs`）。第一版没放行，
            // 于是 `README.en.md` 里那句 `**Appearance**: 13 tokens` 逃了 ——
            // 而**整份英文文档本来就是盲区**：上面那张前缀表的锚点全是中文措辞
            //（`"CI 共 "` / `" 个 token"`），英文散文里一条都对不上。
            // 是造变异时读诊断才看见的：那次红的不是我的自检，是这处真副本。
            let mut tail_at = from;
            if hay[tail_at..].starts_with('s') {
                let nxt = hay[tail_at + 1..].chars().next();
                if !nxt.is_some_and(ident) {
                    tail_at += 1;
                }
            }
            if hay[..at].chars().next_back().is_some_and(ident)
                || hay[tail_at..].chars().next().is_some_and(ident)
            {
                continue;
            }
            // 往前：跳空白 → 可选量词 → 跳空白 → 必须是数字。
            let mut head: Vec<char> = hay[..at].chars().collect();
            while head.last().is_some_and(|c| c.is_whitespace()) {
                head.pop();
            }
            if head.last().is_some_and(|c| "个条套项".contains(*c)) {
                head.pop();
                while head.last().is_some_and(|c| c.is_whitespace()) {
                    head.pop();
                }
            }
            let before = head.last().is_some_and(|c| c.is_ascii_digit());
            // 往后：跳空白 → 可选连接符 → 跳空白 → 必须是数字。
            let tail: Vec<char> = hay[tail_at..].chars().collect();
            let mut k = 0usize;
            while tail.get(k).is_some_and(|c| c.is_whitespace()) {
                k += 1;
            }
            if tail.get(k).is_some_and(|c| "共计：:".contains(*c)) {
                k += 1;
                while tail.get(k).is_some_and(|c| c.is_whitespace()) {
                    k += 1;
                }
            }
            let after = tail.get(k).is_some_and(|c| c.is_ascii_digit());
            if before || after {
                let start = snap_down(hay, at.saturating_sub(18));
                let end = snap_up(hay, tail_at + 10);
                return Some(hay[start..end].chars().filter(|c| *c != '\n').collect());
            }
        }
        None
    }

    /// ★ 数量形态的副本一律不许进散文 —— **锚点不再是「我当时写的那句话」**。
    #[test]
    fn no_quantity_form_of_a_pointer_only_fact_appears_in_prose() {
        // 匹配器自检（先证明它两个方向都认、且不乱认）：
        for (s, kw) in [
            ("CI 一共 7 个 job。", "job"),
            ("本仓 CI 是 7 个 job", "job"),
            ("套件 tmux-target：45", "tmux-target"),
        ] {
            assert!(
                quantity_form_hit(s, kw).is_some(),
                "数量形态没被认出来：{s:?} / {kw}"
            );
        }
        for (s, kw) in [
            ("CI job 全绿（rust / node）", "job"),
            ("响应里带 jobId 3 号", "job"),
            ("先看 job 的日志再说", "job"),
        ] {
            assert!(
                quantity_form_hit(s, kw).is_none(),
                "把不是计数的写法当成了副本：{s:?} / {kw}"
            );
        }
        let mut offenders = Vec::new();
        for rel in PROSE_FILES {
            if *rel == SELF {
                continue;
            }
            let hay = read(rel);
            for (fact, kw) in QUANTITY_KEYWORDS {
                if let Some(sn) = quantity_form_hit(&hay, kw) {
                    offenders.push(format!("  {rel}：「…{sn}…」（事实：{fact}）"));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "散文里又出现了这些事实的**数量形态**副本：\n{}\n\
             ⚠ 与上面那条按前缀取样的判据不同，本条**不管连接词怎么写** ——\n\
             因为散文的正常演进就是改写措辞，而按措辞取样的判据会随改写静默失效。\n\
             改法同 E12：删掉这个数、只留指针（各事实的家见 `POINTER_ONLY` 那一列）。",
            offenders.join("\n")
        );
    }

    const SELF: &str = "src-tauri/src/doc_copy_registry.rs";

    fn read(rel: &str) -> String {
        std::fs::read_to_string(repo_root().join(rel))
            .unwrap_or_else(|e| panic!("{rel} 读不到：{e} —— 文件搬了就把这条一起改"))
    }

    /// 数字前后可能垫着的**排版字符** —— markdown 粗体星号、反引号、空格。
    ///
    /// ⚠ 这一条是 F18 下半补的洞：上一版只跳空格，于是 `今天清单上有 **4 处**`
    /// （粗体包着数字）**探不到** —— 而那正是本轮要治的副本之一。
    /// 与 F24 同一个族：**匹配单位比事实小**，这里小在「没算上排版字符」。
    const PAD: &[char] = &[' ', '*', '`'];

    /// 把字节下标往前挪到最近的字符边界（截取上下文用，宁可多取一点）。
    fn snap_down(hay: &str, i: usize) -> usize {
        let i = i.min(hay.len());
        (0..=i)
            .rev()
            .find(|k| hay.is_char_boundary(*k))
            .unwrap_or(0)
    }

    /// 把字节下标往后挪到最近的字符边界。
    fn snap_up(hay: &str, i: usize) -> usize {
        let i = i.min(hay.len());
        (i..=hay.len())
            .find(|k| hay.is_char_boundary(*k))
            .unwrap_or(hay.len())
    }

    /// `prefix` 之后（跳过排版字符）紧跟阿拉伯数字 ⇒ 返回那一小段原文。
    fn digit_after(hay: &str, prefix: &str) -> Option<String> {
        let mut from = 0;
        while let Some(i) = hay[from..].find(prefix) {
            let at = from + i + prefix.len();
            let rest = hay[at..].trim_start_matches(PAD);
            if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                // ⚠ **必须回到字符边界再切**：这里是字节算术，中文文档里
                // `saturating_sub(12)` 会落在一个汉字中间，`hay[start..end]` 当场 panic。
                // F18 上半这段一直没炸，只是因为当时的文件集里没有触发它的位置 ——
                // 下半把文件集从 6 份扩到 10 份，第一次跑就炸在 `INVARIANTS.md` 的「铁」字上。
                // ⇒ 一条判据在**没跑到的输入**上是什么行为，不能靠「它一直是绿的」推断。
                let start = snap_down(hay, (from + i).saturating_sub(12));
                let end = snap_up(hay, at + 14);
                let snippet: String = hay[start..end].chars().filter(|c| *c != '\n').collect();
                return Some(snippet);
            }
            from = at;
        }
        None
    }

    /// `suffix` **之前**（跳过排版字符）紧挨阿拉伯数字 ⇒ 返回那一小段原文。
    ///
    /// 中文里数量词多半是「**数字在前**」（`14 个 token` / `15 套` / `5 大折叠分组`），
    /// 只有 [`digit_after`] 一个方向时这类副本一条都探不到。
    fn digit_before(hay: &str, suffix: &str) -> Option<String> {
        let mut from = 0;
        while let Some(i) = hay[from..].find(suffix) {
            let at = from + i;
            let head = hay[..at].trim_end_matches(PAD);
            if head.chars().next_back().is_some_and(|c| c.is_ascii_digit()) {
                let start = snap_down(hay, head.len().saturating_sub(14));
                let end = snap_up(hay, at + suffix.len() + 6);
                return Some(hay[start..end].chars().filter(|c| *c != '\n').collect());
            }
            from = at + suffix.len();
        }
        None
    }

    /// 同 [`POINTER_ONLY`]，但形态是「**数字在前**」。
    ///
    /// `(事实, 不许再出现的后缀, 必须留着的指针片段, 家在哪)`
    #[allow(clippy::type_complexity)]
    const POINTER_ONLY_SUFFIX: &[(&str, &[&str], &str, &str)] = &[
        (
            "主题 token 数",
            &[" 个 token"],
            "以 `src/theme.ts` 的 `TOKENS` 为准",
            "`src/theme.ts` 的 `TOKENS` 数组本身（**刻意不抄条数** —— 原写「今天 14 条」，实为 15，本模块 police 的正是这个形状）",
        ),
        (
            "reader 文件数",
            &[" 个 reader"],
            "以 `local_read_surface_registry` 的机检为准",
            "`local_read_surface_registry` 里那条 `assert_eq!(readers, …)`（机器数 7）",
        ),
        (
            "设置面板的折叠分组数",
            &[" 大折叠分组"],
            "家在 `src/settings/panel-groups.vitest.ts`",
            "`panel-groups.vitest.ts` 的逐页完整清单（完整相等断言，搬丢一块会红）",
        ),
        (
            "进 CI 的 e2e 套数",
            &[" 套带断言", " 套真机套件"],
            "套数与地板值一律不抄在这里",
            "`ci.yml` 里的 `assert-pass-floor.sh` 调用行；名单一致性由 `the_e2e_readme_suite_list_matches_ci` 机检",
        ),
    ];

    /// ★ 正题：**声明「只剩指针」的事实，数字副本不许再回来**。
    #[test]
    fn a_fact_declared_pointer_only_has_no_number_copy_left() {
        // 抽取器自检：文件读不到 / 空的时候下面全是零命中地绿。
        let total: usize = PROSE_FILES.iter().map(|f| read(f).len()).sum();
        assert!(
            total > 50_000,
            "散文文件集只读到 {total} 字节 —— 抽取器坏了，下面每条都会零命中地绿"
        );

        let mut back = Vec::new();
        for (fact, prefixes, _, home) in POINTER_ONLY {
            for rel in PROSE_FILES {
                let body = read(rel);
                for p in *prefixes {
                    if let Some(snip) = digit_after(&body, p) {
                        back.push(format!(
                            "  {rel}：「…{snip}…」（事实：{fact}；家在 {home}）"
                        ));
                    }
                }
            }
        }
        for (fact, suffixes, _, home) in POINTER_ONLY_SUFFIX {
            for rel in PROSE_FILES {
                let body = read(rel);
                for q in *suffixes {
                    if let Some(snip) = digit_before(&body, q) {
                        back.push(format!(
                            "  {rel}：「…{snip}…」（事实：{fact}；家在 {home}）"
                        ));
                    }
                }
            }
        }
        assert!(
            back.is_empty(),
            "有已经删掉的数字副本又被写回散文里了：\n{}\n\n\
             ★ 定框 **E12**：文档修法只有两种 —— ① 送进一条**会红**的判据，② **删副本只留指针**。\n\
             这几个事实走的是第二条路。要写数字，先给它一条会红的判据，然后从这张表里挪走。\n\
             ⚠ `src-tauri/README.md` 是活样本：它上一段刚写「本文件**刻意不复制**那份清单」，\n\
             下一段就存了个 11（实际 15）—— **声明纪律不等于纪律**。",
            back.join("\n")
        );
    }

    /// 指针句必须**还在** —— 否则「删副本」会退化成「把整段删了」，读者连去哪查都不知道。
    #[test]
    fn the_pointer_that_replaced_the_copy_is_still_there() {
        for (fact, _, pointer, home) in POINTER_ONLY.iter().chain(POINTER_ONLY_SUFFIX.iter()) {
            let found = PROSE_FILES.iter().any(|rel| read(rel).contains(*pointer));
            assert!(
                found,
                "「{fact}」的指针句「{pointer}」在散文里一处都找不到了。\n\
                 删副本不等于删整段 —— 得留一句告诉读者**去哪查**（家在 {home}）。"
            );
        }
    }

    /// 走第一条路的那些，**那条判据得还活着**（判据没了，准确读数立刻变成下一处会腐的散文）。
    #[test]
    fn the_guards_that_keep_the_accurate_numbers_accurate_still_exist() {
        let root = repo_root();
        let mut all = String::new();
        for sub in ["src-tauri/src", "remote-daemon-proto/src", "src"] {
            collect(&root.join(sub), &mut all);
        }
        assert!(
            all.len() > 200_000,
            "只读到 {} 字节源码 —— 抽取器坏了",
            all.len()
        );
        // ★ **本文件自己要摘出去** —— `HAS_A_GUARD` 表里就写着这些符号名，
        // 不摘的话每一条都能在自己的表里找到自己 ⇒ **恒绿**。
        // 变异实测：把 `backend/mod.rs` 里那个判据改名，本条**照样绿**。
        // ⚠ 这是本区第三次「判据匹配到自己」（F12 匹配到注释 · F13 匹配到 `RULE` 常量 · 本条）。
        let own = std::fs::read_to_string(root.join(SELF)).unwrap_or_default();
        assert!(
            own.contains("HAS_A_GUARD"),
            "`{SELF}` 读不到或不含 `HAS_A_GUARD` —— 下面那句摘除成了死规则"
        );
        let all = all.replace(&own, "");
        for (fact, symbol) in HAS_A_GUARD {
            assert!(
                all.contains(symbol),
                "「{fact}」靠 `{symbol}` 看着，而它已经不在源码里了。\n\
                 ★ V6 逐条对上过：**准确的读数背后都有一条会红的判据，已假的 13 处背后一条都没有**。\n\
                 判据一没，那个准确读数就是下一处会腐的散文 —— 要么补一条新的，要么把它挪进\n\
                 `POINTER_ONLY`（删副本只留指针）。"
            );
        }
    }

    /// ★ **棘轮**：已处置的行数只许涨（＝未处置的只许降）。
    #[test]
    fn the_treated_prose_rows_only_go_up() {
        // `.iter().filter().count()` 而不是 `.len()` —— 后者会被常量折叠成恒真断言，
        // 那正是 clippy 咬掉第一版的原因。这里的值来自**表本身**，加一行才动得了它。
        let done = DONE_ROWS.iter().filter(|s| !s.is_empty()).count();
        let parked = NOT_DOING.len();
        assert!(
            done + parked <= TOTAL_ROWS,
            "已处置 {done} + 明确不做 {parked} > 全表 {TOTAL_ROWS} 行 —— \
             表加行了就把 `TOTAL_ROWS` 一起改"
        );
        assert!(
            done >= DONE_FLOOR,
            "已处置的散文副本行数从 {DONE_FLOOR} 掉到 {done} 了 —— 这是**棘轮，只许涨**。\
             还剩 {} 行既没处置也没登记为不做；处置一行就把它写进 `DONE_ROWS` 并把 \
             `DONE_FLOOR` 抬上去，**不许把已处置的挪走来让数字好看**。",
            TOTAL_ROWS - done - parked
        );
        // 「明确不做」必须**带理由**，否则它就成了「没做」的体面写法。
        for (row, why) in NOT_DOING {
            assert!(
                why.len() > 30,
                "`{row}` 登记为不做，但理由只有 {} 字节 —— 一句话的理由挡不住下一个人重开它",
                why.len()
            );
        }
    }

    /// ★ 把 `e2e/README.md` 那句「**只能靠这条提醒**」变成一条会红的判据〔F18 下半〕。
    ///
    /// 那份表原先连**套数**带**逐套地板**一起抄，并在旁边逐字写着
    /// 「副本漂了不会让任何东西变红，所以只能靠这条提醒」—— 然后它漂了三次
    /// （套数 15→19 · `ccm-cli` 44→53 · `usage-probe` 9→11），
    /// 而且上一轮 E82 订正时**也是这么写的**。**散文纪律等于没有纪律**（定框 E12）。
    ///
    /// 地板值走 **E12 第二条路**（删副本、只留指针，由上面那张表看着不许回来）；
    /// **套件名单**走第一条路 —— 就是本条：与 `ci.yml` 的调用行**集合相等**。
    #[test]
    fn the_e2e_readme_suite_list_matches_ci() {
        let ci = read(".github/workflows/ci.yml");
        let mark = "run: bash e2e/assert-pass-floor.sh ";
        let in_ci: BTreeSet<String> = ci
            .lines()
            .filter_map(|l| l.trim().strip_prefix(mark))
            .filter_map(|rest| rest.split_whitespace().next())
            .map(str::to_string)
            .collect();
        // 抽取器自检：抓不到调用行时两边都会是空集，本条就成了一句废话。
        assert!(
            in_ci.len() >= 15,
            "只从 `ci.yml` 抓到 {} 条 `assert-pass-floor` 调用行（08-06 实测 19）—— \
             抽取器坏了，本条此刻是空转的：{in_ci:?}",
            in_ci.len()
        );

        let readme = read("e2e/README.md");
        let in_doc: BTreeSet<String> = readme
            .lines()
            .filter(|l| l.starts_with("| `e2e-tmux"))
            .flat_map(|l| {
                l.split('|')
                    .nth(2)
                    .unwrap_or("")
                    .split('·')
                    .map(|c| c.trim().trim_matches('`').trim().to_string())
                    .collect::<Vec<_>>()
            })
            .filter(|c| !c.is_empty())
            .collect();
        assert!(
            in_doc.len() >= 15,
            "从 `e2e/README.md` 的表里只解析出 {} 个套件名 —— 表的形状变了就把本条一起改：{in_doc:?}",
            in_doc.len()
        );

        let missing: Vec<&String> = in_ci.difference(&in_doc).collect();
        let extra: Vec<&String> = in_doc.difference(&in_ci).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "`e2e/README.md` 的套件表与 `ci.yml` 的调用行对不上。\n\
             CI 有而文档没有：{missing:?}\n\
             文档有而 CI 没有：{extra:?}\n\
             ★ 地板值**不在**本条管辖内 —— 那些已按 E12 第二条路删掉副本，\
             单一事实源就是 `ci.yml` 的调用行。本条只钉**名单**。"
        );
    }

    fn collect(dir: &Path, out: &mut String) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs" || x == "ts") {
                out.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
            }
        }
    }
}
