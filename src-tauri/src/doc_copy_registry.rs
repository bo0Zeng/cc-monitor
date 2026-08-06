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
    ];

    /// V6 那张表的总行数（台账标题写「十八行」，实际列出 **16**）。
    const TOTAL_ROWS: usize = 16;

    /// ★ **棘轮的地板**：已处置行数只许涨。
    ///
    /// ⚠ 第一版写的是「未处置数 `STILL_PROSE_ROWS <= 11`」，**clippy 当场指出那是恒真断言**
    /// （`this assertion has a constant value`）—— 常量与自己比永远成立，
    /// 那不是棘轮，是一句装饰。与 F06 那次被 clippy 咬中的同义反复**同一个形状**。
    /// ⇒ 改成从**表**导出：`DONE_ROWS` 的真实条数与这个地板比，加一行才降得下未处置数。
    const DONE_FLOOR: usize = 5;

    /// V6 十六行里已处置的（本轮 5 行）。未处置数 = `TOTAL_ROWS` − 本表条数。
    const DONE_ROWS: &[&str] = &[
        "#1 门禁怎么跑（`--all` 缺 vendor 排除 → 已订正为 `--workspace --exclude`）",
        "#2 workspace 测试总数（删副本留指针）",
        "#4 vitest DOM 数（删副本留指针）",
        "#5 CI job 数（删副本留指针）",
        "#9 `backend/` 下 `.rs` 数（删副本留指针）",
    ];

    /// 本文件自己的路径 —— 扫符号时要摘出去（表里写着那些符号名）。
    const SELF: &str = "src-tauri/src/doc_copy_registry.rs";

    fn read(rel: &str) -> String {
        std::fs::read_to_string(repo_root().join(rel))
            .unwrap_or_else(|e| panic!("{rel} 读不到：{e} —— 文件搬了就把这条一起改"))
    }

    /// `prefix` 之后（跳过空格）紧跟阿拉伯数字 ⇒ 返回那一小段原文。
    fn digit_after(hay: &str, prefix: &str) -> Option<String> {
        let mut from = 0;
        while let Some(i) = hay[from..].find(prefix) {
            let at = from + i + prefix.len();
            let rest = hay[at..].trim_start_matches(' ');
            if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                let start = (from + i).saturating_sub(12);
                let end = (at + 14).min(hay.len());
                let snippet: String = hay[start..end].chars().filter(|c| *c != '\n').collect();
                return Some(snippet);
            }
            from = at;
        }
        None
    }

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
        for (fact, _, pointer, home) in POINTER_ONLY {
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
        assert!(
            done <= TOTAL_ROWS,
            "已处置 {done} 行 > 全表 {TOTAL_ROWS} 行 —— 表加行了就把 `TOTAL_ROWS` 一起改"
        );
        assert!(
            done >= DONE_FLOOR,
            "已处置的散文副本行数从 {DONE_FLOOR} 掉到 {done} 了 —— 这是**棘轮，只许涨**。\
             还剩 {} 行没处置；处置一行就把它写进 `DONE_ROWS` 并把 `DONE_FLOOR` 抬上去，\
             **不许把已处置的挪走来让数字好看**。",
            TOTAL_ROWS - done
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
