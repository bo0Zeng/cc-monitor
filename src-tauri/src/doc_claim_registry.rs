//! F11：**耐久文档里「描述当下」的那些字段，与代码对拍。**
//!
//! # 病：文档寿命比「当下」长
//!
//! F07 顺出的一般化，F04b/F04c 各自又验证一次：**「状态列」与「实测答案」是耐久文档里
//! 最易腐的两种字段** —— 它们描述的是**当下**，而文档的寿命比「当下」长得多。
//! 六轮加总已经订正了 **14 处**，而其中最险的一处（`INVARIANTS §A5`「kill 无此白名单」）
//! **自 F04 起就假了**、连着好几轮没人发现。
//!
//! ⚠ 更要紧的是 **F07 自己就是这个病的受害者**：它订正了 §33b 三问的答案 ①，
//! 却漏了**同一节里 11 行之前那一行说着同一句话的单元格**（`control/launch.rs`
//! 「零生产调用方」）。**订正手头那一处，不等于订正那句话。**
//! 那与 F01 的四处「每 ~8s」是同一个病，只是这次犯在「订正」这个动作上。
//!
//! # 处置不是「以后记得更新」，是**把那个数搬回文档、让判据去读它**
//!
//! 本模块的核心手法：**判据不自己写那个数** —— 它从文档里把数抽出来，再与现场量的比。
//! 于是那个数**只有一个家（文档）**，而「文档与现实对不上」变成一条会红的机检。
//! 这同时满足定框 §4 那条「同一个数不许两侧各写一份」：代码里没有第二份。
//!
//! # 扫描面为什么是「状态列」而不是「所有实测句」
//!
//! 实测（F11 摸底）：`doc/` 十一个文件 4158 行里，**表头含「状态」的表只有一张、六行**
//! （`INVARIANTS.md §33b`）—— 可枚举、可穷尽、后果最重（它是「下一个执行 U8c-3 的人
//! 唯一会读的依据」）。而「实测句」那一族有 **63 句**、绝大多数是散文，
//! **钉不住**（见 §诚实边界）。⇒ 状态列**逐格登记**，可数的实测断言**挑出来登记**，
//! 其余如实记为诚实边界。

/// 耐久文档里**每一个**「状态列」单元格 → 它的**现场量法**。`(件名, 量法键)`。
///
/// # ⚠ 这里**刻意不存「文档写的状态」**
///
/// 第一版存了 —— 于是 `STATUS_CELLS` 成了文档那一列的**第二份副本**，而两份副本必漂：
/// **E4 变异（把文档里 `U8c-3` 的「待做」改成「已交付」）时五条判据全绿**，
/// 因为判据比的是「登记表里那份副本 ↔ 现场」，文档那份根本没参与。
///
/// ★ 那正是本模块开头声称要治的病，我自己在同一个文件里又犯了一次 ——
/// **而且只有变异复验能发现**（基线全绿时它看起来完全正常）。
/// ⇒ 状态**只从文档里读**，这里只留「怎么量」。判据 = 文档说的 ↔ 现场量的。
#[cfg(test)]
const STATUS_CELLS: &[(&str, &str)] = &[
    ("U8c-1", "payload-kernel-exists"),
    ("U8c-2a", "usage-probe-uses-the-kernel"),
    ("U8c-2b-0", "posix-quote-has-one-home"),
    ("U8c-2c-1", "ccm-invocation-kernel-exists"),
    ("U8c-2c-2", "production-ts-calls-the-rust-renderers"),
    ("U8c-3", "ts-renderer-still-there"),
];

#[cfg(test)]
mod tests {
    use super::STATUS_CELLS;
    use std::path::{Path, PathBuf};

    const INVARIANTS: &str = include_str!("../../doc/INVARIANTS.md");

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .to_path_buf()
    }

    fn doc_files() -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(repo_root().join("doc"))
            .expect("读不到 doc/")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        v.sort();
        v
    }

    /// 一张「表头含状态」的表：`(文件名, 表头行号, 各行 = (件名, 首格原文, 状态格原文))`。
    ///
    /// 件名剥掉 `*`/`✅` 便于对表；**首格原文留着**，因为本仓的约定是
    /// 「✅ 打在件名上、日期写在状态列」—— 判「文档说完没完」要同时看这两处。
    #[allow(clippy::type_complexity)]
    fn status_tables() -> Vec<(String, usize, Vec<(String, String, String)>)> {
        let mut out = Vec::new();
        for p in doc_files() {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let raw = std::fs::read_to_string(&p).unwrap_or_default();
            let lines: Vec<&str> = raw.lines().collect();
            let mut i = 0usize;
            while i < lines.len() {
                let is_head = lines[i].starts_with('|')
                    && lines[i].contains("状态")
                    && i + 1 < lines.len()
                    && lines[i + 1].starts_with('|')
                    && lines[i + 1]
                        .chars()
                        .all(|c| matches!(c, '|' | '-' | ':' | ' '));
                if !is_head {
                    i += 1;
                    continue;
                }
                let mut rows = Vec::new();
                let mut j = i + 2;
                while j < lines.len() && lines[j].starts_with('|') {
                    let cells: Vec<&str> = lines[j].trim_matches('|').split('|').collect();
                    let raw_first = cells.first().unwrap_or(&"").trim().to_string();
                    let item = raw_first.replace(['*', '✅'], "").trim().to_string();
                    let status = cells.last().unwrap_or(&"").trim().to_string();
                    rows.push((item, raw_first, status));
                    j += 1;
                }
                out.push((name.clone(), i + 1, rows));
                i = j;
            }
        }
        out
    }

    /// ★ 抽取器自检：扫描面没缩水。坏掉时下面几条会零命中零失败地绿。
    #[test]
    fn the_doc_scan_actually_reads_the_durable_docs() {
        let files = doc_files();
        assert!(
            files.len() >= 11,
            "`doc/` 只扫到 {} 个 .md —— 遍历坏了（F11 摸底实测 11 个）",
            files.len()
        );
        let total: usize = files
            .iter()
            .map(|p| {
                std::fs::read_to_string(p)
                    .map(|s| s.lines().count())
                    .unwrap_or(0)
            })
            .sum();
        assert!(
            total >= 4000,
            "`doc/` 总共只剩 {total} 行 —— 路径或读法坏了（摸底实测 4158 行）"
        );
        let tables = status_tables();
        assert_eq!(
            tables.len(),
            1,
            "「表头含状态」的表张数变了（实得 {:?}）—— **这不是让你改数字**：\n\
             新出现一张就把它的每一格登记进 `STATUS_CELLS` 并配一条现场量法；\n\
             少了一张就说明表被删了或表头措辞变了（那本条会零命中地绿，所以它必须红）。",
            tables
                .iter()
                .map(|(f, l, r)| format!("{f}:{l}（{} 行）", r.len()))
                .collect::<Vec<_>>()
        );
    }

    /// ★ **两个方向**：文档里每一格都登记了；登记表里没有文档里已经不存在的件。
    #[test]
    fn every_status_cell_is_registered() {
        let mut in_doc: Vec<String> = status_tables()
            .into_iter()
            .flat_map(|(_, _, rows)| rows)
            .map(|(item, _, _)| item)
            .collect();
        in_doc.sort();
        let mut registered: Vec<String> = STATUS_CELLS.iter().map(|(k, _)| k.to_string()).collect();
        registered.sort();
        assert_eq!(
            in_doc, registered,
            "耐久文档的「状态列」与登记表对不上。\n\
             多出来的格请登记进 `STATUS_CELLS` 并写一条**现场量法**（不是抄它的状态，\n\
             是写「怎么从代码里量出这个状态还对不对」）；\n\
             登记表里多出来的件说明文档改了而这里没跟。"
        );
    }

    /// 生产段里 `.call("launch")` 的处数（**不数测试段、不数本仓的说明文字**）。
    fn production_launch_calls() -> usize {
        let mut n = 0usize;
        let mut stack = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        // 运行时拼，免得命中本文件自己。
        let verb = format!(".call(\"{}\"", "launch");
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                    continue;
                }
                // `launch_wire.rs` 的头注里逐字写着那个串（F07 立的例外，沿用）。
                if p.file_name().is_some_and(|s| s == "launch_wire.rs") {
                    continue;
                }
                let src =
                    guard_core::production_code(&std::fs::read_to_string(&p).unwrap_or_default());
                n += src.matches(verb.as_str()).count();
            }
        }
        n
    }

    /// ★★ **核心手法：判据从文档里读那个数，不自己写一份。**
    ///
    /// # 它抓的是什么
    ///
    /// `INVARIANTS §33b` 有一格逐字写着「生产段 `.call("launch")` 今天 **N 处**」。
    /// 本条把那个 N 抽出来，与现场数的比。⇒ 那个数**只有一个家（文档）**，
    /// 而「有人加了第三处调用而文档还写着 2」变成一条会红的机检。
    ///
    /// ⚠ **F07 漏掉的正是这一格。** 它订正了三问的答案 ①（11 行之后那一处），
    /// 而这一行说着同一句话（「零生产调用方 —— 只有一处且在 `cfg(test)` 里」）**没被碰**。
    /// **订正手头那一处，不等于订正那句话。**
    #[test]
    fn the_doc_number_for_production_launch_calls_matches_reality() {
        let marker = "〔机检〕生产段 `.call(\"launch\")` 处数：";
        let at = INVARIANTS.find(marker).unwrap_or_else(|| {
            panic!(
                "`INVARIANTS.md` 里找不到锚点 {marker:?} —— 那句话被改写了。\n\
                 本条判据的整个价值就是「那个数只有一个家」；\n\
                 改措辞就把它变成零命中地绿，所以宁可让它红。"
            )
        });
        let tail = &INVARIANTS[at + marker.len()..];
        let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
        let want: usize = digits
            .parse()
            .unwrap_or_else(|_| panic!("锚点后面不是数字，实得 {:?}", &tail[..12.min(tail.len())]));
        let got = production_launch_calls();
        assert_eq!(
            want, got,
            "文档写着生产段有 {want} 处 `.call(\"launch\")`，实测 {got} 处。\n\
             ⚠ **这两个数只允许有一个家（文档那一处）** —— 别在代码里再写一份，\n\
             回去改文档那个数，并顺手想一想：多出来的那处是不是又切了一格？\n\
             （F04c 加的那处是 `send-keys`，**不是**又切了一格「起会话」——\n\
             那两件事很容易被混成一个数，`launch_wire` 那条判据就是为此改过度量的。）"
        );
    }

    /// ★ 「外层载荷有四个产出方，一个都没退役」—— 逐个存在性复核。
    ///
    /// 这条是「可数的实测断言」里第二条能钉的。⚠ 它**只钉住「四个都还在」**，
    /// 钉不住「它们各自还是不是生产在跑」—— 那需要真远端/真安装包（ROADMAP §5）。
    #[test]
    fn the_four_outer_layer_producers_are_all_still_there() {
        let root = repo_root();
        let checks: &[(&str, bool)] = &[
            (
                "session-backend.ts（TS 生产远端主路）",
                root.join("src/session-backend.ts").is_file(),
            ),
            (
                "control/launch.rs（daemon argv）",
                root.join("remote-daemon-proto/src/control/launch.rs")
                    .is_file(),
            ),
            (
                "account_usage.rs::build_usage_probe_cmd（用量探针 shell 串）",
                std::fs::read_to_string(root.join("src-tauri/src/account_usage.rs"))
                    .map(|s| s.contains("fn build_usage_probe_cmd"))
                    .unwrap_or(false),
            ),
            (
                "shared/ccm（用户终端那条路）",
                root.join("shared/ccm").is_file(),
            ),
        ];
        let missing: Vec<&str> = checks
            .iter()
            .filter(|(_, ok)| !ok)
            .map(|(n, _)| *n)
            .collect();
        assert!(
            missing.is_empty(),
            "「外层四个产出方」里这些已经没了：{missing:?} —— **这多半是好事**：\n\
             有产出方退役了 ⇒ `INVARIANTS §33b` 那句「四个产出方，一个都没退役」过期了，\n\
             回去把它和 U8c-3 的前置一起重裁。"
        );
    }

    /// ★★ 逐格跑「现场量法」：**文档里那一格**记的状态今天还对不对。
    ///
    /// ⚠ 状态**从文档读**，不从登记表读 —— 见 `STATUS_CELLS` 的头注：
    /// 第一版存了一份副本，E4 变异（只改文档里的状态）时五条判据**全绿**。
    ///
    /// ⚠ 量法都是**结构性**的（文件/符号在不在、生产段有没有在调），**不是**跑功能。
    /// 那是刻意的：这一族的失效形态是「事实变了而文档没跟」，
    /// 而结构性事实恰好是变了就一定能看见的那种。
    #[test]
    fn each_registered_status_still_matches_reality() {
        let root = repo_root();
        let read = |rel: &str| std::fs::read_to_string(root.join(rel)).unwrap_or_default();
        let prod = |rel: &str| guard_core::production_code(&read(rel));
        // 文档那张表：件名 → (首格原文, 状态格原文)
        let cells: std::collections::BTreeMap<String, (String, String)> = status_tables()
            .into_iter()
            .flat_map(|(_, _, rows)| rows)
            .map(|(item, raw, status)| (item, (raw, status)))
            .collect();
        for (item, how) in STATUS_CELLS {
            let (delivered, why) = match *how {
                "payload-kernel-exists" => (
                    prod("src-tauri/src/backend/control/payload.rs").contains("fn render_payload"),
                    "载荷内核在 `backend/control/payload.rs`",
                ),
                // ⚠ 第一版量法写的是 `render_payload`，**红了**——那是我选错了标的：
                // 用量探针走的是内核的另一个入口 `payload::usage_probe_payload`
                // （账号前缀 + 嵌套 env 清理 + 启动器，**无 cd**）。判据自己的报错文案
                // 逐字预言了这一种可能（「或者本条量法本身选错了标的」），照它改。
                "usage-probe-uses-the-kernel" => (
                    prod("src-tauri/src/account_usage.rs")
                        .contains("backend::control::payload::usage_probe_payload"),
                    "用量探针在调载荷内核的 `usage_probe_payload` 入口",
                ),
                "posix-quote-has-one-home" => (
                    prod("src-tauri/src/shell_quote.rs").contains("shell_quote_core::posix_quote")
                        || prod("src-tauri/src/ssh_source.rs")
                            .contains("shell_quote_core::posix_quote"),
                    "Rust 侧的 POSIX quote 收在共享 crate（另有 `quote_singleton_guard` 单点守卫）",
                ),
                "ccm-invocation-kernel-exists" => (
                    prod("src-tauri/src/backend/control/ccm_invocation.rs")
                        .contains("fn render_ccm_invocation"),
                    "ccm 调用行内核在 `backend/control/ccm_invocation.rs`",
                ),
                "production-ts-calls-the-rust-renderers" => (
                    read("src/remote-launch-run.ts").contains("commands.render_ccm_launch(")
                        && read("src/remote-launch-run.ts")
                            .contains("commands.render_launch_payload("),
                    "生产 TS 主路在调那两条 Rust 渲染命令",
                ),
                // 「待做」那一格：**反向**量法 —— TS 渲染器还在，就说明确实还没删。
                "ts-renderer-still-there" => (
                    !root.join("src/session-backend.ts").is_file(),
                    "TS 渲染器已经删了",
                ),
                other => panic!("`{item}` 的量法键 {other:?} 没有实现 —— 登记表与实现漂了"),
            };
            let (raw_first, status) = cells
                .get(*item)
                .unwrap_or_else(|| panic!("`{item}` 不在文档那张表里 —— 另一条判据会先红"));
            // 本仓的约定：**✅ 打在件名上、日期写在状态列**；或者状态列直接写「已交付」。
            //
            // ⚠ **「先出现的那个词算数」**，不是「含哪个词」——状态格里常常带订正叙事
            // （`U8c-2c-2` 那格逐字写着「本列此前写『待做』」），两个词都在里面。
            // 第一版用 `contains` 判，当场被它红了；「先出现的算数」既能容纳叙事，
            // 又不至于把判定权交给措辞。
            let done_at = status.find("已交付");
            let todo_at = status.find("待做").or_else(|| status.find("未做"));
            let says_done = match (done_at, todo_at) {
                _ if raw_first.contains('✅') => true,
                (Some(d), Some(t)) => d < t,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => panic!(
                    "`{item}` 的状态格里既没有「已交付」也没有「待做」（首格 {raw_first:?} / \
                     状态 {status:?}）—— 改措辞就把本条变成零命中地绿，所以宁可让它红。"
                ),
            };
            assert_eq!(
                delivered, says_done,
                "`{item}`：文档那一格说「已交付 = {says_done}」（状态列原文 {status:?}），\n\
                 而现场量法说「{why}」= {delivered}。\n\
                 ⚠ 两种可能，都要动手：\n\
                 · 事实前进了而状态列没跟（**这一族最常见**，六轮 14 处都是它）⇒ 改文档；\n\
                 · 或者本条量法本身选错了标的（U8c-2a 就这么红过一次）⇒ 改量法 + 写清为什么。"
            );
        }
    }
}
