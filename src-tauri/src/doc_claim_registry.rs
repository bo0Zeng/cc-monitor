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
    // 〔F19〕`doc/ARCHITECTURE.md` §2.1 的「backend 四层在 monitor 侧落地到哪一步」表。
    // ⚠ 它是**本条判据族第一次被一张新表触发**：F19 往 ARCHITECTURE 写下这张表时，
    // `the_doc_scan_actually_reads_the_durable_docs` 当场红（表张数 1 → 2），
    // 逼着这四格各配一条现场量法 —— 那正是这条判据存在的目的。
    ("`control/`", "monitor-backend-control-landed"),
    ("`observe/`", "monitor-backend-observe-landed"),
    ("`platform/`", "monitor-backend-platform-landed"),
    ("`common/`", "monitor-backend-common-landed"),
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
        // 〔F19〕1 → **2**：`doc/ARCHITECTURE.md` §2.1 新增「backend 四层落地」表。
        // ⚠ 这不是「为了绿而改数字」——改数字的**前提**是那张表的每一格都已登记进
        // `STATUS_CELLS` 并配了现场量法（本条的报错文案逐字要求的就是这件事）。
        assert_eq!(
            tables.len(),
            2,
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
        // ⚠ **F12 扩扫描面**：第一版只读 `INVARIANTS.md` 一份 ⇒
        // `/full-audit` 逮到**第四份副本**住在 `doc/IPC-PROTOCOL.md:584`（「生产路径今天还没切过来」），
        // 而它**结构上永远不会红**。⇒ 先扫全 `doc/**`，任何一份里出现同一句旧断言都要红。
        {
            let stale = "生产路径今天还没切过来";
            let mut offenders: Vec<String> = Vec::new();
            for p in doc_files() {
                if std::fs::read_to_string(&p)
                    .map(|t| t.contains(stale))
                    .unwrap_or(false)
                {
                    offenders.push(p.file_name().unwrap().to_string_lossy().to_string());
                }
            }
            assert!(
                offenders.is_empty(),
                "这些耐久文档还写着「{stale}」：{offenders:?} —— 那句话在 U8a-2c-1 之后就假了。\n\
                 ⚠ **它是同一句话的第四份副本**，而 F11 的机检只读 `INVARIANTS.md` ⇒ 它结构上够不着。\n\
                 这正是本模块头注那条纪律的反例：**订正一句假话时先把它的全部副本找出来。**"
            );
        }
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
                // ⚠ **F12 订正**：第一版左支读的是 `src-tauri/src/shell_quote.rs` —— **那个文件不存在**
                // ⇒ 左支恒 false，整条判据只靠右支撑着（`/full-audit` 逮到的）。
                // 这正是「判据自己会不会错」那一问要问的东西：**读一个不存在的文件不会报错，
                // 只会静默返回空串**，而 `||` 让它看起来像「两条都在查」。
                // ⇒ 改成读真正的家，并加一条「文件必须存在」的断言，杜绝同样的静默空转。
                "posix-quote-has-one-home" => (
                    {
                        let host = "src-tauri/src/ssh_source.rs";
                        assert!(
                            repo_root().join(host).is_file(),
                            "量法读的 {host} 不存在 —— 读不到的文件只会静默返回空串"
                        );
                        prod(host).contains("shell_quote_core::posix_quote")
                    },
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
                // 〔F19〕四层落地量法。⚠ 量的是「**这一层在 monitor 侧落地了没有**」，
                // **不是**「它内部已经干净了」——后者各有各的判据（`platform/` 是 C10 的
                // 跨 target 编译，今天不成立）。把两件事混进一格会让这一格永远说不清。
                //
                // ⚠ `control/` 那格刻意不是裸 `is_dir`：目录空着也算「有目录」，
                // 而这一格要主张的是**控制面真的住进来了** ⇒ 钉住那个唯一的分流器在里面。
                "monitor-backend-control-landed" => (
                    root.join("src-tauri/src/backend/control/daemon_route.rs")
                        .is_file(),
                    "monitor 侧 `backend/control/` 在，且那个唯一的回落分流器住在里面",
                ),
                "monitor-backend-observe-landed" => (
                    root.join("src-tauri/src/backend/observe").is_dir(),
                    "monitor 侧 `backend/observe/` 目录存在",
                ),
                "monitor-backend-platform-landed" => (
                    root.join("src-tauri/src/backend/platform").is_dir(),
                    "monitor 侧 `backend/platform/` 目录存在",
                ),
                "monitor-backend-common-landed" => (
                    root.join("src-tauri/src/backend/common").is_dir(),
                    "monitor 侧 `backend/common/` 目录存在",
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

    /// ★★ **F12 全局变异抽样抓到的缺口**：文档里**枚举的一组标识符**没人对拍。
    ///
    /// # 它是怎么被发现的
    ///
    /// Phase G 的全局变异抽样里，把 daemon `inbound.rs` 的 `unknown_command`
    /// **三处一起改名**成 `unknown_cmd` —— **daemon 253 条全绿**。
    /// 而 `doc/IPC-PROTOCOL.md` 逐条列着六个**协议级**错误码，语义是
    /// 「客户端代码写错了，别重试」—— 那是**仓外可见的契约**（`resolve` 那条已经与 aterm 冻结）。
    ///
    /// ⚠ F11 建这个登记表时扫的是「状态列」与「可数的实测断言」两族，
    /// **漏了第三种形状：文档里枚举的一组标识符**。
    /// 那不是「F11 做漏了」——是**摸底时的分族本身不完整**，
    /// 而**只有跨模块的变异抽样能发现这种「整族缺口」**（本工作区自己的判据都在
    /// 各自那件的范围里看，看不到「有一族根本没人管」）。
    ///
    /// ⇒ 这条也是 skill 那句「**变异存活分布不会说谎**」在本工作区拿到的实货。
    #[test]
    fn the_protocol_level_error_codes_in_the_doc_are_the_ones_the_daemon_uses() {
        const IPC: &str = include_str!("../../doc/IPC-PROTOCOL.md");
        let marker = "**协议级**由 `inbound.rs` 独占 ——";
        let at = IPC.find(marker).unwrap_or_else(|| {
            panic!(
                "`IPC-PROTOCOL.md` 里找不到锚点 {marker:?} —— 那句话被改写了。\n\
                 本条判据的价值是「那份清单只有一个家（文档）」，改措辞就把它变成零命中地绿。"
            )
        });
        // ⚠ 收尾锚点是**清单本身的收尾**（`，语义是`），不是段落结束 ——
        // 第一版切到空行，把后面几句里的 `invalid_args`/`launch`/`resolve` 也收进来了
        // （抽到 9 个而真值 6 个）。**抽取器自检当场把它拦下来了**，这就是它的岗位。
        let seg = &IPC[at..];
        let end = seg
            .find("，语义是")
            .expect("找不到清单的收尾锚点「，语义是」—— 那句话被改写了");
        let seg = &seg[..end];
        let mut in_doc: Vec<&str> = Vec::new();
        let mut rest = seg;
        while let Some(a) = rest.find('`') {
            rest = &rest[a + 1..];
            let Some(b) = rest.find('`') else { break };
            let word = &rest[..b];
            rest = &rest[b + 1..];
            // 只收「像错误码」的词：全小写 + 下划线，且不是模块名。
            if !word.is_empty()
                && word.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                && word != "inbound"
            {
                in_doc.push(word);
            }
        }
        in_doc.sort();
        in_doc.dedup();
        // ★ 抽取器自检：这一段里就该有六个码；抽不到就说明剥法坏了。
        assert_eq!(
            in_doc.len(),
            6,
            "从文档那一段只抽到 {} 个协议级错误码（{in_doc:?}）—— 剥法坏了，\n\
             下面那条会零命中地绿。文档实测是六个：bad_request · line_too_long ·\n\
             unknown_command · duplicate_id · handler_panicked · not_cancellable。",
            in_doc.len()
        );
        let daemon =
            std::fs::read_to_string(repo_root().join("remote-daemon-proto/src/inbound.rs"))
                .expect("读不到 daemon 的 inbound.rs");
        let prod = guard_core::production_code(&daemon);
        let missing: Vec<&&str> = in_doc
            .iter()
            .filter(|c| !prod.contains(&format!("\"{c}\"")))
            .collect();
        assert!(
            missing.is_empty(),
            "`IPC-PROTOCOL.md` 列着这些协议级错误码，而 daemon 生产段里**找不到**：{missing:?}\n\
             ⚠ 它是**仓外可见的契约**（`resolve` 那条已经与仓外 aterm 冻结）——\n\
             改名 = 静默毁约：对端拿到一个它不认识的码，而两侧的测试都不会红\n\
             （F12 的全局变异抽样就是这么把这个缺口逮出来的：三处一起改名，daemon 253 条全绿）。\n\
             要改就两侧一起改，并想清楚仓外消费方。"
        );
    }

    /// 〔audit-0805 08-06〕**`doc/` 里点名的代码符号必须解析得到，且住在文档说的那个文件里。**
    ///
    /// **为什么建它**：08-06 把本会话逮到的每一处文档/计划腐坏按机制归了族，主力是
    /// **停滞式** —— 世界变了、文本一个字没动（改代码的那个提交**碰过**那份文件，
    /// 却把已假的那句原样留着）。这一族**结构上救不了**靠「改法纪律」：留痕是「怎么改」的规矩，
    /// 而这族的定义就是没人来改。能接住它的只有一样：**一条会红的判据读到那句散文**（E12 ①）。
    ///
    /// `file.rs::symbol` 是 `doc/` 里**最可机检**的一族散文：改名 / 删除 / 搬家都让它变假，
    /// 而**没有任何东西会红**。建判据当天实测 **73 处引用、真腐 0**——
    /// ⚠ **「今天全对」正是建它的理由，不是不建的理由**：干净是纪律攒出来的，
    /// 而纪律不在门禁里就只是运气，`doc/` 这四个文件此前**一条判据都没读过**。
    ///
    /// **口径**（比「符号存在」严一档，建时实测不误红）：文档写 `a.rs::foo`，
    /// 就要求 `foo` 的声明**出现在 `a.rs` 里** —— 只对符号名会放过「搬到别的文件」，
    /// 而搬家恰恰是本仓重构的常见形态。
    ///
    /// ⚠ ~~抽取器摘除调用者自己（`scan_tree!` 按构造如此）⇒ 只在 `doc/` 引用本文件里的符号时
    /// 才会误红~~ —— **08-06 当天就误红了一次**（`DEVELOPMENT.md` 指向本文件里的一条判据）。
    /// 现在本文件自己也进扫描面（见下方 `srcs.push`）。**「只在极少数情况下会错」不是边界，是欠账。**
    #[test]
    fn every_code_symbol_named_in_the_docs_still_resolves() {
        /// 例外表：**每条都写清「为什么它解析不到却是对的」**。
        /// 下面有一条自检把「已经不需要的例外」揪出来 —— 例外表自己也会腐。
        const EXCEPTIONS: &[(&str, &str)] = &[
            (
                "setup",
                "tauri 的 `.setup(move |app| …)` 钩子闭包 —— 是真东西，但不是一处声明",
            ),
            (
                "monitor_get_active_ids",
                "`CONTRIBUTING.md` 里的**示例占位符**（教人「照这样加一行」），本就不指向真符号",
            ),
            (
                "run_tmux_reconcile_poller",
                "`INVARIANTS.md` 那句逐字写着它**已删**（audit-fixes F03.2）—— 历史句，\
                 删掉反而丢掉「为什么今天没有 poller」的解释",
            ),
        ];
        const KW: &[&str] = &[
            "fn", "struct", "enum", "const", "static", "trait", "mod", "type",
        ];

        // ── 收全仓声明：符号名 → 它出现在哪些文件名里
        let mut srcs: Vec<(PathBuf, String)> = Vec::new();
        for root in [
            "src-tauri/src",
            "src-tauri/crates",
            "remote-daemon-proto/src",
        ] {
            srcs.extend(guard_core::scan_tree!(&repo_root().join(root), &["rs"]));
        }
        // 〔08-06 第二次补扫描面〕**把本文件自己也收进来**。
        // `scan_tree!` 按构造摘除调用者（那是 F23「判据读到自己」的防护），
        // 但这里收的是**声明**不是语料 —— 摘掉自己会让「`doc/` 指向本文件里的符号」被误判成腐。
        // ⚠ 这不是假设：头注原本写着「只在这种情况下才会误红」，而 08-06 当天就发生了
        // （`DEVELOPMENT.md` 指向本文件的 `the_backend_test_command_in_the_docs_matches_ci`）。
        // ⇒ 把「已知的例外」变成「已修的缺陷」，头注那句警告随之删掉。
        srcs.push((
            PathBuf::from("doc_claim_registry.rs"),
            include_str!("doc_claim_registry.rs").to_string(),
        ));
        // `build.rs` 是单文件、不在任何被扫的目录下 —— 第一版就漏了它，
        // 于是 `build.rs::emit_daemon_build_id` 被当成「腐了」。**抽取器的扫描面要自己说清楚。**
        let br = repo_root().join("src-tauri/build.rs");
        let br_src = std::fs::read_to_string(&br).expect("读不到 src-tauri/build.rs");
        srcs.push((br, br_src));

        let mut decl: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
            std::collections::BTreeMap::new();
        for (p, raw) in &srcs {
            let fname = p
                .file_name()
                .expect("源文件名")
                .to_string_lossy()
                .to_string();
            // 注释里的 `fn foo` 不算声明 —— 否则「注掉一个函数」这种变异会被判据放过。
            let stripped = guard_core::strip_comment_lines(raw);
            for line in stripped.lines() {
                let mut it = line.split_whitespace().peekable();
                while let Some(tok) = it.next() {
                    if !KW.contains(&tok) {
                        continue;
                    }
                    let Some(next) = it.peek() else { continue };
                    let ident: String = next
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    if !ident.is_empty() {
                        decl.entry(ident).or_default().insert(fname.clone());
                    }
                }
            }
        }
        // ★ 抽取器自检 1：收不到足够多的声明 ⇒ 遍历坏了，下面整条会零命中地绿。
        assert!(
            decl.len() > 2000,
            "全仓只抽到 {} 个声明符号 —— 遍历或剥法坏了（建判据当天实测 3073 个 / 133 个源文件）",
            decl.len()
        );

        // ── 收 doc/ 里的 `file.rs::symbol`
        let mut refs: Vec<(String, usize, String, String)> = Vec::new();
        for p in doc_files() {
            let fname = p
                .file_name()
                .expect("doc 文件名")
                .to_string_lossy()
                .to_string();
            let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {p:?} 失败: {e}"));
            for (i, line) in text.lines().enumerate() {
                let b = line.as_bytes();
                let mut from = 0usize;
                while let Some(k) = line[from..].find(".rs::") {
                    let at = from + k;
                    // 往前收路径：只走 ASCII 路径字符 ⇒ 遇到中文（多字节）自然停在字符边界上。
                    let mut s = at;
                    while s > 0 && {
                        let c = b[s - 1] as char;
                        c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '/' || c == '-'
                    } {
                        s -= 1;
                    }
                    let base = line[s..at + 3]
                        .rsplit('/')
                        .next()
                        .unwrap_or_default()
                        .to_string();
                    let mut e = at + 5;
                    while e < b.len() && {
                        let c = b[e] as char;
                        c.is_ascii_alphanumeric() || c == '_'
                    } {
                        e += 1;
                    }
                    let sym = line[at + 5..e].to_string();
                    if !sym.is_empty() && !base.is_empty() {
                        refs.push((fname.clone(), i + 1, base, sym));
                    }
                    from = at + 5;
                }
            }
        }
        // ★ 抽取器自检 2：`doc/` 里本来就有几十处 —— 抽到个位数就是剥法坏了。
        assert!(
            refs.len() >= 60,
            "`doc/` 里只抽到 {} 处 `file.rs::symbol` —— 剥法坏了（建判据当天实测 73 处）",
            refs.len()
        );

        // ★ 自检 3：例外表保鲜。例外是**欠账**，不是免检章。
        for (sym, why) in EXCEPTIONS {
            let used: Vec<&(String, usize, String, String)> =
                refs.iter().filter(|r| r.3 == *sym).collect();
            assert!(
                !used.is_empty(),
                "例外表里的 `{sym}` 在 `doc/` 里已经没人写了 —— 删掉这一行。\n\
                 （它当初的理由：{why}）"
            );
            let still_needed = used
                .iter()
                .any(|r| !matches!(decl.get(&r.3), Some(fs) if fs.contains(&r.2)));
            assert!(
                still_needed,
                "例外 `{sym}` 现在**解析得到了** —— 删掉这条例外，别让例外表替真判据挡枪。\n\
                 （它当初的理由：{why}）"
            );
        }

        let bad: Vec<String> = refs
            .iter()
            .filter(|r| !EXCEPTIONS.iter().any(|(s, _)| *s == r.3))
            .filter_map(|(f, ln, base, sym)| match decl.get(sym) {
                None => Some(format!(
                    "doc/{f}:{ln}  `{base}::{sym}` —— **全仓找不到这个符号**（改名或删了）"
                )),
                Some(fs) if !fs.contains(base) => Some(format!(
                    "doc/{f}:{ln}  `{base}::{sym}` —— 符号还在，但**搬家了**：现住 {:?}",
                    fs.iter().collect::<Vec<_>>()
                )),
                _ => None,
            })
            .collect();
        assert!(
            bad.is_empty(),
            "`doc/` 点名了这些代码符号，而它们今天对不上：\n{}\n\n\
             ⚠ 这是**停滞式腐坏**的典型形态：改代码的人不会回来改文档，而在本判据之前
             **没有任何东西会因此变红**。两条修法（E12）：① 把文档改对；\
             ② 那句话若只是历史，就写清「已删 / 已改名」并进本条的例外表（带理由）。",
            bad.join("\n")
        );
    }

    /// 〔audit-0805 08-06〕**`doc/` 里点名的仓内文件路径必须解析得到。**
    ///
    /// 与上一条（`file.rs::symbol`）同族、更宽一档：符号那条只看得见 `.rs`，
    /// 而 `doc/` 里点名的还有 `.ts` / `.sh` / `.mjs` / `.json` / `.yml`。
    /// 建判据当天实测 **119 处**带目录的路径引用，逐条核完**真腐 1 处**：
    /// `INVARIANTS.md` 里的 `remote-daemon-proto/src/accounts_query.rs`
    /// —— 那个文件早已搬进 `observe/`，而**没有任何东西会因此变红**（本条即为此建）。
    ///
    /// ⚠ **解析口径用 `git ls-files` 而不是磁盘**：磁盘会把「本机生成、CI 里还不存在」的
    /// 生成物也算成解析得到（`src-tauri/gen/schemas/**` 就是），那样判据在两个环境里结论不同 ——
    /// 而**结论随环境变的判据比没有判据更坏**。生成物走例外表，理由写明。
    ///
    /// ⚠ 匹配用**后缀**：文档常按「隐含根」写（`control/gate.rs` 指的是
    /// `remote-daemon-proto/src/control/gate.rs`）。第一版用全路径相等，
    /// 一口气误报 36 处 —— 又一次**匹配单位比事实小**。
    #[test]
    fn every_repo_path_named_in_the_docs_still_resolves() {
        /// 例外：**解析不到却是对的**。三种形状，每种都在本仓真实出现过。
        const EXCEPTIONS: &[(&str, &str)] = &[
            (
                "src-tauri/gen/schemas/acl-manifests.json",
                "tauri 构建生成物 + gitignore：磁盘上有、`git ls-files` 里没有，且不同环境有无不定",
            ),
            (
                "shared/ccm-wrapper.sh",
                "**历史句**：原文逐字写着「取代已删除的 …」——删掉它反而丢掉「今天为什么没有 wrapper」",
            ),
            (
                "src/cards/memory-recall.ts",
                "**示例占位**：原文是「通常新建 `…`」，教人照着建一个，本就不指向现存文件",
            ),
            (
                "code-picture/doc/agents/claude-code.md",
                "**跨仓引用**：另一个仓的语料，本仓解析不到是正常的",
            ),
            (
                "agents/claude-code.md",
                "同上（同一句里的简写形）",
            ),
            (
                "account-ux/MASTERPLAN.md",
                "**计划工作区**住在 `.claude/planned-build/`（另一个 git 仓）",
            ),
            (
                "unify-launch/MASTERPLAN.md",
                "同上",
            ),
            (
                ".claude/planned-build/account-isolation/DESIGN-account-switching.md",
                "同上：计划仓里的设计稿，不在本仓",
            ),
            (
                "/.mcp.json",
                "指的是**用户项目目录**下的 `.mcp.json`（MCP 项目配置），不是本仓文件",
            ),
        ];
        const EXTS: &[&str] = &["rs", "ts", "sh", "mjs", "json", "yml", "toml", "md", "py"];

        let tracked: Vec<String> = {
            let out = std::process::Command::new("git")
                .args(["ls-files"])
                .current_dir(repo_root())
                .output()
                .expect("跑不动 `git ls-files` —— 本判据的解析口径就是它");
            assert!(out.status.success(), "`git ls-files` 非零退出");
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|s| s.to_string())
                .collect()
        };
        // ★ 自检 1：文件清单太短 ⇒ 口径坏了，下面会把一切都判成「指不到」。
        assert!(
            tracked.len() > 300,
            "`git ls-files` 只列出 {} 个文件 —— 口径坏了（本仓实测上千个）",
            tracked.len()
        );

        // ── 抽 `doc/` 里反引号包着、**带目录**的路径
        let mut refs: Vec<(String, usize, String)> = Vec::new();
        for p in doc_files() {
            let fname = p
                .file_name()
                .expect("doc 文件名")
                .to_string_lossy()
                .to_string();
            let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {p:?} 失败: {e}"));
            for (i, line) in text.lines().enumerate() {
                for chunk in line.split('`').skip(1).step_by(2) {
                    let c = chunk.trim();
                    if !c.contains('/') || c.contains(' ') || c.contains("::") {
                        continue;
                    }
                    let Some(ext) = c.rsplit('.').next() else {
                        continue;
                    };
                    if !EXTS.contains(&ext) || c.starts_with("http") {
                        continue;
                    }
                    if !c
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || "_./-".contains(ch))
                    {
                        continue;
                    }
                    refs.push((fname.clone(), i + 1, c.to_string()));
                }
            }
        }
        // ★ 自检 2：数量地板 **+ 锚点**。
        //
        // ⚠ 只有数量地板是**不够**的，这一条是变异当场量出来的：把「带目录才算」那个条件反过来
        // （于是收的是 `lib.rs` 这类**不带目录**的名字），`refs.len()` 照样过 90 ——
        // **地板对「收的是不是同一类东西」完全是瞎的**，它只数个数。
        // 补一个必须在场的锚点，人群换了就当场红。
        assert!(
            refs.len() >= 90,
            "`doc/` 里只抽到 {} 处带目录的路径引用 —— 剥法坏了（建判据当天实测 119 处）",
            refs.len()
        );
        const CANARY: &str = "src/session-backend.ts";
        assert!(
            refs.iter().any(|(_, _, c)| c == CANARY),
            "抽到了 {} 条，但**锚点 `{CANARY}` 不在里面** —— 收的多半不是「带目录的仓内路径」这一类了。\n\
             （数量地板只数个数，换一群东西照样能喂饱它。）",
            refs.len()
        );

        // `doc/` 在仓根下一层 ⇒ 文中的 `../src/README.md` 说的就是仓根的 `src/README.md`。
        // 不归一化就会把五处**完全正确**的相对写法判成腐 —— 判据误报比漏报更快被人关掉。
        let resolves = |c: &str| {
            let c = c.trim_start_matches("../");
            tracked
                .iter()
                .any(|t| t == c || t.ends_with(&format!("/{c}")))
        };

        // ★ 自检 3：例外表保鲜 —— 例外是欠账不是免检章。
        for (path, why) in EXCEPTIONS {
            assert!(
                refs.iter().any(|(_, _, c)| c == path),
                "例外表里的 `{path}` 在 `doc/` 里已经没人写了 —— 删掉这一行。（当初的理由：{why}）"
            );
            assert!(
                !resolves(path),
                "例外 `{path}` 现在**解析得到了** —— 删掉这条例外，别让例外表替真判据挡枪。\n\
                 （当初的理由：{why}）"
            );
        }

        let bad: Vec<String> = refs
            .iter()
            .filter(|(_, _, c)| !EXCEPTIONS.iter().any(|(e, _)| e == c))
            .filter(|(_, _, c)| !resolves(c))
            .map(|(f, ln, c)| format!("  doc/{f}:{ln}  `{c}`"))
            .collect();
        assert!(
            bad.is_empty(),
            "`doc/` 点名了这些仓内路径，而 `git ls-files` 里找不到（含后缀匹配）：\n{}\n\n\
             ⚠ 同 `file.rs::symbol` 那条：**搬家 / 改名 / 删除都让它变假，而此前没有东西会红**。\n\
             修法（E12）：① 把路径改对；② 若那句只是历史或示例，写清楚并进例外表（带理由）。",
            bad.join("\n")
        );
    }

    /// 〔audit-0805 08-06〕**发版版本号六处必须一致**（`package.json` 是权威源，其余对拍）。
    ///
    /// **为什么建它**：`doc/RELEASING.md` 自己逐字记着 ——
    /// 「v3.1→v3.4 **连续四次**发版漏改 README，于是 README 的『当前版本』长期落后一个大版本；
    /// BACKLOG 早把『checklist 里没有 README 这一条』点名为**机制性根因**，
    /// 而根因没修 ⇒ 第四次照样复发」。
    ///
    /// 那次的修法是**在 checklist 里加一行散文**。散文接不住它：第四次复发时 checklist 已经在了。
    /// ⇒ 这正是 E12 ①「送进一条会红的判据」该管的形状，也是 E3 的标准解
    /// （一个事实六个副本 ⇒ 定权威源 + 其余对拍）。
    ///
    /// ⚠ 本条**故意在发版中途也会红**：改了 `package.json` 而 README 还没跟上时它就红 ——
    /// 那不是误报，那是它的岗位（`RELEASING` 的 checklist 要求这几处一起改）。
    ///
    /// ⚠ 建判据当天六处全部是 `3.6.0`，**一处不差** —— 又是「今天干净、但没人守着」：
    /// 这个位置**已经腐过四次**，靠的是人记得，不是机制。
    ///
    /// # 与 `release.yml` 那道 guard 的关系（E3：别造第二个权威源）
    ///
    /// 建完本条的**第二天**才查到：`release.yml` 里早有一道
    /// 「Verify version consistency with tag」，查**四处**（`package.json` / `tauri.conf.json` /
    /// `Cargo.toml` / **`Cargo.lock`**），CHANGELOG 记着它是「防 v2.4.2 漂移事故复发」加的。
    ///
    /// 两者**不是一个事实两个权威源**，因为比的东西不同：
    /// 那道 guard 的权威是 **git tag**（发版时「代码里的版本 == tag」），
    /// 本条的权威是 `package.json`（任何时候「所有副本彼此一致」）。
    ///
    /// ★ 但它给出一条更要紧的教训：**「已经有 guard」不等于「有信号」**。
    /// `release.yml` 逐字写着 `on: push: tags: ['v*']` —— 而〔用 08-05〕已裁定不再 push
    /// ⇒ 那道 guard **今天结构上一次都不会跑**。README 那四次复发也正是发生在
    /// 「代码侧四处有人查、README 没人查」的缝里。
    ///
    /// # 诚实边界：`Cargo.lock` **钉不进本条**（试过，是恒绿的）
    ///
    /// 先把它当第七处加了进来，然后按 E11 造变异（把 lock 里 monitor 包改成 `3.5.0`）——
    /// **测试照样绿**。查下去才明白：`cargo test` 启动时会先解析依赖，
    /// **把 `Cargo.lock` 自动改回与 `Cargo.toml` 一致**（实测：变异后 `3.5.0` → 跑完 `3.6.0`，exit 0）。
    /// ⇒ 任何住在 `cargo test` 里的判据都**看不见** lock 漂移：它在被观察之前就被治好了。
    ///
    /// ⚠ 也不能改成读 `git show HEAD:` 那一份：那样「bump 版本」这个提交本身会被自己挡住
    /// （提交前跑测试 → HEAD 里还是旧版本 → 红 → 提交不了），**造出一个解不开的死结**。
    ///
    /// ⇒ 结论如实记着：lock 这一处**本地钉不住**，唯一能查它的是 `release.yml` 那道
    /// PowerShell 步骤（它不经 cargo 读文件），而那道今天不会跑。这是一条**真的诚实边界**，
    /// 不是「以后补」—— 记进 `ROADMAP §5`。
    ///
    /// ★ 它差一点就成了本仓最讨厌的那种东西：**一条永远不会红的判据**。
    /// 逮住它的不是「测试失败」，是**变异之后诊断栏一个字都没有** —— 只看 exit code 会当它绿了。
    #[test]
    fn the_release_version_is_the_same_in_all_six_places() {
        /// 从 `hay` 里按 `needle` 抠出紧随其后的 `X.Y.Z`。
        /// **needle 必须恰好命中一次** —— 命中零次（那行被改写）或多次（抠错地方）都当场红，
        /// 否则这条判据会在「读不到东西」的时候安静地绿。
        fn pick(who: &str, hay: &str, needle: &str) -> String {
            let n = hay.matches(needle).count();
            assert_eq!(
                n, 1,
                "在 {who} 里，锚点 {needle:?} 命中 {n} 次（要求恰好 1 次）——\n\
                 那一行被改写或挪走了。**先修锚点再谈版本对不对**，否则本条会零命中地绿。"
            );
            let at = hay.find(needle).expect("上面已断言命中一次") + needle.len();
            let rest = &hay[at..];
            let end = rest
                .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                .unwrap_or(rest.len());
            let v = &rest[..end];
            assert!(
                v.split('.').count() == 3 && v.split('.').all(|s| !s.is_empty()),
                "{who} 在锚点之后抠到的是 {v:?} —— 形状不像 `X.Y.Z`"
            );
            v.to_string()
        }

        let root = repo_root();
        let rd = |p: &str| {
            std::fs::read_to_string(root.join(p)).unwrap_or_else(|e| panic!("读 {p} 失败: {e}"))
        };
        let (pkg, cargo, conf, readme, readme_en) = (
            rd("package.json"),
            rd("src-tauri/Cargo.toml"),
            rd("src-tauri/tauri.conf.json"),
            rd("README.md"),
            rd("README.en.md"),
        );

        // 权威源（E3）：npm 包清单。其余五处都只是它的副本。
        let authority = pick("package.json", &pkg, "\n  \"version\": \"");
        let others: [(&str, String); 5] = [
            (
                "src-tauri/Cargo.toml",
                pick("src-tauri/Cargo.toml", &cargo, "\nversion = \""),
            ),
            (
                "src-tauri/tauri.conf.json",
                pick("src-tauri/tauri.conf.json", &conf, "\n  \"version\": \""),
            ),
            (
                "README.md 抬头那行",
                pick("README.md 抬头那行", &readme, "当前版本: v"),
            ),
            (
                "README.md 「项目当前状态」块",
                pick("README.md 「项目当前状态」块", &readme, "- **版本**：v"),
            ),
            (
                "README.en.md 抬头那行",
                pick("README.en.md 抬头那行", &readme_en, "| Current: v"),
            ),
        ];

        let off: Vec<String> = others
            .iter()
            .filter(|(_, v)| *v != authority)
            .map(|(who, v)| format!("  {who}：{v}"))
            .collect();
        assert!(
            off.is_empty(),
            "版本号对不上。权威源 `package.json` = {authority}，而这几处是别的数：\n{}\n\n\
             ⚠ 这个位置**已经连续腐过四次**（v3.1→v3.4 每次发版都漏改 README，\n\
             `doc/RELEASING.md` 自己记着这件事）。当时的修法是往 checklist 里加一行散文，\n\
             而第四次复发时那行散文已经在了 —— 所以现在由本条判据接着。\n\
             修法：把落后的那几处改成 {authority}（`RELEASING.md § 1` 的 checklist 列了全部落点）。",
            off.join("\n")
        );
    }

    /// 〔audit-0805 08-06〕**文档里写成 `CONST = 数` 的，代码里那个常量必须真是这个数。**
    ///
    /// **这是定框 E12 自己点名的洞**：E12 的 ⚠ 逐字写着「那四个准确的细节数
    /// （`CHUNK_SIZE=600` 等）**一个都不在它的扫描面里**」—— 本模块此前只管
    /// 「状态列」与几种极窄形态，`CONST = 数` 这一族**没人读**。
    ///
    /// **变异实证（先红后信的反面：它当时是绿的）**：把 daemon 生产常量
    /// `REPLY_BURST` 从 8 改成 3 —— daemon 全套 + monitor 全套**都绿**，
    /// 而 `IPC-PROTOCOL.md` 逐字写着「`main.rs::REPLY_BURST = 8`，连发 8 条后强制让位一次」。
    /// 也就是说：**协议文档里的一个行为常量，代码改了不会有任何东西红。**
    ///
    /// 手法沿用本模块的核心做法：**判据不自己写那个数**，从文档里抽出来再与代码比 ——
    /// 于是那个数只有一个家（文档），改代码不改文档就红，改文档不改代码也红。
    #[test]
    fn every_constant_value_quoted_in_the_docs_matches_the_code() {
        /// 长得像常量、其实不是 Rust 常量的。逐条写清它是什么。
        const EXCEPTIONS: &[(&str, &str)] = &[
            (
                "CLAUDECODE",
                "**环境变量**（`CLAUDECODE=1`），不是 Rust 常量",
            ),
            ("CLAUDE_CODE_CHILD_SESSION", "同上，环境变量"),
        ];

        /// 从一行里抽 `NAME = 数`（大写标识符 ≥4 字符）。反引号可有可无。
        fn scan_line(line: &str) -> Vec<(String, String)> {
            let b: Vec<char> = line.chars().collect();
            let mut out = Vec::new();
            let mut i = 0usize;
            while i < b.len() {
                if !(b[i].is_ascii_uppercase()) {
                    i += 1;
                    continue;
                }
                let s = i;
                while i < b.len()
                    && (b[i].is_ascii_uppercase() || b[i].is_ascii_digit() || b[i] == '_')
                {
                    i += 1;
                }
                let name: String = b[s..i].iter().collect();
                if name.len() < 4 {
                    continue;
                }
                let mut j = i;
                while j < b.len() && (b[j] == '`' || b[j] == ' ') {
                    j += 1;
                }
                if j >= b.len() || (b[j] != '=' && b[j] != '：' && b[j] != ':') {
                    continue;
                }
                j += 1;
                while j < b.len() && (b[j] == '`' || b[j] == ' ') {
                    j += 1;
                }
                let vs = j;
                while j < b.len() && (b[j].is_ascii_digit() || b[j] == '_') {
                    j += 1;
                }
                if j > vs {
                    let v: String = b[vs..j].iter().filter(|c| **c != '_').collect();
                    out.push((name, v));
                }
                i = j;
            }
            out
        }

        let mut claims: Vec<(String, usize, String, String)> = Vec::new();
        for p in doc_files() {
            let fname = p
                .file_name()
                .expect("doc 文件名")
                .to_string_lossy()
                .to_string();
            let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {p:?} 失败: {e}"));
            for (i, line) in text.lines().enumerate() {
                for (n, v) in scan_line(line) {
                    claims.push((fname.clone(), i + 1, n, v));
                }
            }
        }
        // ★ 自检 1 + 锚点：只有数量地板不够（本会话实测过「人群被换掉、地板照样过」）。
        assert!(
            claims.len() >= 5,
            "`doc/` 里只抽到 {} 处 `CONST = 数` —— 剥法坏了（建判据当天实测 6 处）",
            claims.len()
        );
        // ⚠ 锚点**只认名字、不认值** —— 第一版把 `&& v == "8"` 也写进来了，
        // 于是判据自己成了那个数的第二份副本（正是本模块头注警告的形态）：
        // 合法地把常量改成别的数时，红的会是锚点而不是对拍，诊断指错方向。
        assert!(
            claims.iter().any(|(_, _, n, _)| n == "REPLY_BURST"),
            "锚点 `REPLY_BURST` 不在抽到的清单里 —— 收的多半不是这一类了。\n\
             （它是本条的立项样本：改代码不改文档时，全仓一条都不会红。）"
        );

        // ── 代码侧：常量名 → 它被定义成的那些值
        let mut defined: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
            std::collections::BTreeMap::new();
        let mut srcs: Vec<(PathBuf, String)> = Vec::new();
        for root in [
            "src-tauri/src",
            "src-tauri/crates",
            "remote-daemon-proto/src",
        ] {
            srcs.extend(guard_core::scan_tree!(&repo_root().join(root), &["rs"]));
        }
        for (_, raw) in &srcs {
            for line in guard_core::strip_comment_lines(raw).lines() {
                let t = line.trim();
                let rest = match t
                    .strip_prefix("const ")
                    .or_else(|| t.strip_prefix("static "))
                {
                    Some(r) => r,
                    None => match t
                        .strip_prefix("pub const ")
                        .or_else(|| t.strip_prefix("pub static "))
                    {
                        Some(r) => r,
                        None => continue,
                    },
                };
                let Some((name, tail)) = rest.split_once(':') else {
                    continue;
                };
                let name = name.trim();
                if name.is_empty()
                    || !name
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                {
                    continue;
                }
                let Some((_, val)) = tail.split_once('=') else {
                    continue;
                };
                let v: String = val
                    .trim()
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '_')
                    .filter(|c| *c != '_')
                    .collect();
                if !v.is_empty() {
                    defined.entry(name.to_string()).or_default().insert(v);
                }
            }
        }
        // ★ 自检 2：代码侧一个常量都收不到 ⇒ 下面会把每条都判成「代码里没有」。
        assert!(
            defined.len() >= 20,
            "全仓只收到 {} 个数值常量定义 —— 剥法坏了",
            defined.len()
        );

        // ★ 自检 3：例外保鲜。
        for (name, why) in EXCEPTIONS {
            assert!(
                claims.iter().any(|(_, _, n, _)| n == name),
                "例外 `{name}` 在 `doc/` 里已经没人写了 —— 删掉这一行。（当初的理由：{why}）"
            );
            assert!(
                !defined.contains_key(*name),
                "例外 `{name}` 现在**真是一个 Rust 常量了** —— 删掉这条例外，让它进对拍。（当初的理由：{why}）"
            );
        }

        let bad: Vec<String> = claims
            .iter()
            .filter(|(_, _, n, _)| !EXCEPTIONS.iter().any(|(e, _)| e == n))
            .filter_map(|(f, ln, n, v)| match defined.get(n) {
                None => None, // 代码里没有同名常量：可能是别的语言/外部约定，不在本条管辖内
                Some(vs) if !vs.contains(v) => Some(format!(
                    "  doc/{f}:{ln}  文档说 `{n} = {v}`，代码里实为 {:?}",
                    vs.iter().collect::<Vec<_>>()
                )),
                _ => None,
            })
            .collect();
        assert!(
            bad.is_empty(),
            "文档写死的常量值与代码对不上：\n{}\n\n\
             ⚠ 立项样本就是这么溜掉的：`REPLY_BURST` 8→3，daemon 与 monitor **两套全绿**。\n\
             修法（E12）：① 把文档改对；② 或者那句话本就不该写死数字 —— 改成指常量名。",
            bad.join("\n")
        );
    }

    /// 〔audit-0805 08-06〕**`scripts/` 里的每个文件都要在它自己的 README 里登记。**
    ///
    /// # 逮到的是一条「找不到」的缺陷
    ///
    /// 08-06 实测：`scripts/` 有三个脚本，而 `scripts/README.md` 的表**只列了 `run.ps1`**。
    /// 漏掉的两个里有 `verify-committed-state.sh` —— 它的头注逐字写着
    /// 「本仓不 push ⇒ CI 从来没见过这些 commit，**所以这道门必须在本机跑**」。
    /// ⇒ 一个「必须本机跑」的门，**照目录 README 是找不到的**；
    /// 本会话是靠 grep 撞见它的，而那不是别人会走的路。
    ///
    /// 这类缺陷不会让任何测试变红，也不会让任何人报错 —— 它只是让**下一个人找不到**。
    /// 本条把「找得到」变成机检。
    ///
    /// ⚠ 只钉**存在性**，不钉描述内容：描述会随脚本演进，钉了就是下一个假陈述
    /// （本模块头注记着 `STATUS_CELLS` 那次教训）。要读细节去看脚本自己的头注。
    #[test]
    fn every_script_in_the_directory_is_listed_in_its_readme() {
        let dir = repo_root().join("scripts");
        let readme =
            std::fs::read_to_string(dir.join("README.md")).expect("读不到 scripts/README.md");
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .expect("读不到 scripts/")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n != "README.md")
            .collect();
        files.sort();
        // ★ 抽取器自检：目录空了或读法坏了 ⇒ 下面会零命中地绿。
        assert!(
            files.len() >= 3,
            "`scripts/` 只扫到 {} 个文件（README 之外）—— 遍历坏了（08-06 实测 3 个）",
            files.len()
        );
        let missing: Vec<&String> = files
            .iter()
            .filter(|n| !readme.lines().any(|l| l.contains(n.as_str())))
            .collect();
        assert!(
            missing.is_empty(),
            "`scripts/` 里这些文件在 `scripts/README.md` 里查不到：{missing:?}\n\n\
             ⚠ 这不会让任何测试变红，也不会让任何人报错 —— 它只是让**下一个人找不到**。\n\
             08-06 实测：那张表当时只有 `run.ps1`，于是照它找不到 `verify-committed-state.sh`，\n\
             而那是全仓**唯一量「提交状态」且必须在本机跑**的门。\n\
             ⇒ 加一行就行；细节写在脚本自己的头注里，别在 README 里抄第二份。"
        );
    }

    /// 〔audit-0805 08-06〕**开发者入口文档里的后端测试命令，必须与 `ci.yml` 逐字相同。**
    ///
    /// # 逮到的是「照它做会少测」
    ///
    /// `doc/DEVELOPMENT.md` 的「跑测试」节此前逐字写着
    /// `cargo test --lib          # 全部单元测试` —— 而 `--lib` **只覆盖根包**，
    /// 六个共享 crate 一条都不跑。新人照入口文档做，得到的是一个**少测**的读数，
    /// 而它长得和全量读数一模一样（都是「ok. N passed」）。
    ///
    /// 这一条属于本会话新命名的那类缺陷：**产物没错，通往它的路是错的**
    /// —— 没有任何测试会因为文档里写错命令而变红。
    ///
    /// # 手法：不在判据里写那个命令
    ///
    /// 命令的**唯一的家是 `ci.yml`**。本条从两边各抽一次再比 ——
    /// 于是改 CI 而不改文档会红，改文档而不改 CI 也会红，
    /// 而判据自己**不持有第三份副本**（`STATUS_CELLS` 那次的教训，见本模块头注）。
    #[test]
    fn the_backend_test_command_in_the_docs_matches_ci() {
        let ci = std::fs::read_to_string(repo_root().join(".github/workflows/ci.yml"))
            .expect("读不到 ci.yml");
        // `rust` job 里那条 `cargo test …` —— 剔注释，只认真会跑的行。
        let cmd = ci
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('#'))
            .find_map(|l| l.strip_prefix("run: "))
            .into_iter()
            .chain(
                ci.lines()
                    .map(str::trim)
                    .filter(|l| !l.starts_with('#'))
                    .filter_map(|l| l.strip_prefix("run: ")),
            )
            .find(|c| c.starts_with("cargo test --workspace"))
            .unwrap_or_else(|| {
                panic!(
                    "`ci.yml` 里找不到 `cargo test --workspace …` 那一步 —— \n\
                     命令的家变了，本条的读法要跟着改（否则它会零命中地绿）。"
                )
            })
            .to_string();

        let dev = std::fs::read_to_string(repo_root().join("doc/DEVELOPMENT.md"))
            .expect("读不到 doc/DEVELOPMENT.md");
        assert!(
            dev.lines().any(|l| l.contains(cmd.as_str())),
            "`doc/DEVELOPMENT.md` 的「跑测试」节里没有 CI 那条命令：\n  {cmd}\n\n\
             ⚠ 它此前写的是 `cargo test --lib` 并标成「全部单元测试」——\n\
             而 `--lib` **只覆盖根包**，六个共享 crate 一条都不跑。\n\
             新人照入口文档做会得到一个**少测**的读数，而它长得和全量读数一模一样。\n\
             ⇒ 命令的唯一的家是 `ci.yml`，文档要与它逐字一致（本条不持有第三份副本）。"
        );
        // ★ 反向：那条会误导的旧写法不许再回来。
        //
        // ⚠ 针**只认代码块里的注释形态**（`--lib` 后跟 `# 全部…`），不是「同一行出现两个词」。
        // 第一版就是后者，于是它**当场命中了我自己写的那句订正**
        // （「别用 `cargo test --lib` 当『全部』」）—— F23「更正时引用旧措辞 = 把它复制一份」
        // 在本仓已是**第三次**（前两次见 F08 下半、F10 §6）。
        // ⇒ 收紧到真正的缺陷形态：它被当成命令的**自我说明**写在代码块里。
        assert!(
            !dev.lines()
                .any(|l| l.contains("cargo test --lib") && l.contains("# 全部")),
            "`doc/DEVELOPMENT.md` 的代码块里又把 `cargo test --lib` 注释成「全部」了 ——\n\
             那正是 08-06 订正掉的那句：它只覆盖根包，六个共享 crate 一条都不跑。"
        );
    }
}
