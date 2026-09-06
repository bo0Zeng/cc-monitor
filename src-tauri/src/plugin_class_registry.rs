//! `EF01`〔`plugin-split`〕：把定框 `E4` 的**四候选 × 两轴**分类表落成**会红的登记表**。
//!
//! # ⚠⚠ 先读这一段：这张表钉的是「**今天实际是什么形态**」，不是「将来必须是什么形态」
//!
//! `plugin-split` 的定框（`E1`–`E10` · `E4b` · `E4c`）**用户还没批**。本模块由
//! `.claude/planned-build/plugin-split/DECISIONS.md#ED1` 自批开工，自批边界逐字只覆盖
//! 「**加判据**」这一档 —— 所以这里只做一件事：**把四个候选今天的实测形态钉住**。
//!
//! ⇒ 两轴的分类结论（[`Candidate::semantics`] / [`Candidate::shape`] 两列）在本模块里是
//! **数据，不是断言**：没有任何一条测试要求「现实必须长成分类说的样子」。
//! 恰恰相反 —— `code-picture` 那一格今天就**对不上**（提案说 sidecar，实测是编进 monitor），
//! 而本模块把这个对不上**如实登记成一列**（[`Candidate::gap`]），不把它抹平。
//!
//! 把本模块读成「定框已生效」是错的，这段话存在的唯一理由就是挡住那个读法。
//! 定框真获批之后要做的是 `EF02`–`EF06`（抽调用口 / 补能力协商 / 出插件 / 假插件验收），
//! **不是**把这张表改成「现实必须等于分类」的断言。
//!
//! # 它钉什么
//!
//! | 候选 | 今天的形态由什么钉住 |
//! |---|---|
//! | `cc-bus` | 脚本族条数 · daemon 命令表里 `bus-*` 的条数 · 起进程口**恰好一处**且住在**通用调用口**里（转调壳里**零处**）· 边界判据还在且针数没缩 |
//! | `code-picture` | monitor 的 `Cargo.toml` 那条 path 依赖**整行**存在 · daemon 的依赖清单与整棵源码树**零命中** |
//! | `cc-spawn` | 生产段里有 `CCM_BIN` 与能力协商 · 且**零**总线数据针（「它不碰总线目录了」这句话的机检形态） |
//! | `ccm` | `agent_*` 适配函数条数 · 每个函数两臂**是不是真的分叉** · `--ccm-probe` 的能力 token 数与 agent 数 · 它在受管工具表里 |
//!
//! # ⚠ 08-26：`cc-bus` 那格的起进程口**搬家了** —— 判据跟着事实走，不是放宽
//!
//! 到 08-25 为止，这里钉的是「**`control/cc_bus.rs` 里恰好一处** `Command::new(`」，
//! 理由逐字是 `E5` 的默认「插件调用**复用**这一处口」。
//! `K-W1A`（08-26）做的正是那句「复用」：把那一处口从 **cc-bus 专用的转调壳**里
//! 抽到**通用调用口** `remote-daemon-proto/src/plugin/invoke.rs`。
//! ⇒ 旧断言当场红了，panic 逐字是「`Command::new(` 一处都找不到 —— 事实没了，
//! 或者抽取面画错了」。**那是钉子干对了活**：它没让这次搬家在无声中发生
//!（同型先例：`guard_support` 的锚点在 U3 拆层时红过一次，账上判的也是「钉子干对了活」）。
//!
//! ★ 于是断言的**主语**跟着换了位置，而**性质一个字没松**：
//! 从「`cc_bus.rs` 恰好一处」变成「**`plugin/invoke.rs` 恰好一处，而 `cc_bus.rs` 零处**」。
//! 两条一起才等价于原来那一条 —— 少了后半条，「口搬走了但壳里又长回一处」就从缝里溜过去了
//!（那正是绕开通用口的形状）。⚠ **不许把这里改成 `>= 0` / `>= 1` 之类**：
//! 那不是跟着搬家，那是把判据换成安慰剂。
//!
//! # ⚠ 刻意**不钉行数**
//!
//! `E1`/`E4` 的读数里有一半是行数（1264 / 4378 / 652 / 1060 …）。行数**不进断言**：
//! 改一个错别字就红，而红了之后唯一的动作是「把数字改掉」—— 那是本仓反复记的
//! 「改数字了事」那一族，它把判据训练成噪音。
//! 进断言的是**结构量**（脚本几个 · 命令几条 · 起进程口几处 · 适配函数几个 · 能力几个）：
//! 这些数变一次，就真的有人加了/删了一样东西，那时**值得有人过一眼**。
//! 行数写在功能件的 Bx 读数里，带日期，谁引用谁自己重量。
//!
//! # 量具的坑（本轮各踩到或差点踩到一次，逐条写下来）
//!
//! 1. **判据会被自己的散文喂饱**：Rust 语料一律先过 `guard_core::production_code`
//!    （剥 `#[cfg(test)]` 段 + `//` 整行），shell 语料先过 `guard_core::strip_hash_comment_lines`。
//!    ⚠ 本轮的活样本：`panorama.rs` 裸 `grep -c` 数到 **22** 条 tauri 命令，其中一条
//!    写在注释里，真值 **21** —— `panorama_seam_registry` 早就把它钉成 21 并逐字写过
//!    「21 不是 22」，而 `E4`/`E10` 抄的仍是 22。
//! 2. **匹配单位不许比事实小**：一处性事实走 `guard_core::find_pinned`（恰好一处 + 两侧有边界），
//!    整行性事实走 `guard_core::pin_line`。裸 `contains("…")` 在磁盘语料上有递减棘轮
//!    （`needle_anchor_registry`），**不许为了本模块把上限调上去**。
//! 3. **读不到就静默变空串**：所有磁盘读走 [`tests::must_read`] —— 读不到直接 panic，
//!    再加字节地板。`unwrap_or_default()` 会让下面每一条零命中地绿。
//! 4. **判据在自己语料里找到自己**：扫树一律走 `guard_core::scan_tree!`（按构造摘除调用者）。
//!
//! # 与已有判据的分工（**不重造**）
//!
//! · daemon 不许碰 cc-bus 的**数据布局** → `remote-daemon-proto/src/cc_bus_boundary_guard.rs`；
//! · daemon 生产段的**起进程点总数** → `readonly_guard::spawn_registry`（相等断言，今天 9 处）；
//! · 全景引擎的**取用口恰好一处** → `panorama_seam_registry`；
//! · daemon 协议面**零全景** → `protocol_doc_guard`（它只扫 `inbound.rs` + `wire.rs`）。
//!
//! 本模块只在**上面这些判据管不到的接缝**上补：`E4` 那张表的**每一格**今天成不成立。
//! 唯一与既有判据重叠的是「daemon 零 code-picture」那条 —— 重叠是刻意的：
//! `protocol_doc_guard` 扫的是**协议面两个文件**，这里扫的是**整棵 daemon 源码树**。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空、零运行期开销。

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    // ───────────────────────────── 登记表 ─────────────────────────────

    /// 轴一（`C19` 三问 → **谁的语义**）。⚠ **提案，未获批**。
    #[derive(Debug, PartialEq, Eq)]
    enum Semantics {
        /// 缺一轴的结论不算结论（`E3`）—— 留这一档就是为了让「漏答」会红。
        Unanswered,
        /// 内建：它就是那件事本身，跨 skill 的原语。
        BuiltIn,
        /// 语义内建、**载体**出插件（`C20` 那一格）。
        CarrierIsAPlugin,
        /// 插件：它是某个 skill 的语义。
        Plugin,
    }

    /// 轴二（`C21` 三档 → **几个二进制**）。⚠ **提案，未获批**。
    ///
    /// ⚠ `BuiltIn` 今天**没有候选落在它上面** —— 那本身是一条读数
    /// （`C21` 那三档里，今天有真实实例的只有 `Plugin` 与 `Sidecar`（且 sidecar 那格还是提案）），
    /// 不是死代码。删掉它等于让「三档」在类型上悄悄变成两档，
    /// 而下一个人再想把某个候选归到「内建」时，会发现这个选项根本不存在。
    #[allow(dead_code)]
    #[derive(Debug, PartialEq, Eq)]
    enum Shape {
        /// 见 [`Semantics::Unanswered`]。
        Unanswered,
        /// 编进同一个二进制。
        BuiltIn,
        /// 我们自己出、随产品发的独立二进制。
        Sidecar,
        /// 用户自己装的外部命令。
        Plugin,
        /// ⚠ **不是三档中的任何一档**：`tool_registry` 里 `installable` 的受管工具。
        /// 它在表里是为了让「`ccm` 落不进三档」这件事**看得见**，而不是被默默归进某一档。
        ManagedTool,
    }

    /// 一个候选在 `E4` 那张表里的一整行。
    struct Candidate {
        /// 候选名（也是诊断里点名用的键）。
        id: &'static str,
        /// 它今天住哪（相对仓根）。**必须真实存在** —— 这是登记表的反向那半。
        home: &'static str,
        /// 轴一（提案）。
        semantics: Semantics,
        /// 轴二（提案）。
        shape: Shape,
        /// 今天的实测形态（一句话；具体的数由下面各自那条判据钉）。
        today: &'static str,
        /// 提案与今天的**差**。一致就写「无差」——但那句话也要有人写下来。
        gap: &'static str,
    }

    /// ★ `E4` 那张表的四行。**分类两列是提案；`today`/`gap` 两列才是本模块钉的东西。**
    const REGISTERED: &[Candidate] = &[
        Candidate {
            id: "cc-bus",
            home: "shared/cc-bus/scripts",
            semantics: Semantics::CarrierIsAPlugin,
            shape: Shape::Plugin,
            today: "一族 shell 脚本；daemon 只经命令面转调它，且那唯一一处起进程口\
                    自 08-26 起住在**通用调用口** `plugin/invoke.rs` 里，转调壳自己零处",
            gap: "还差「插件」这个名分（`EU3`：粒度是命令还是包）—— **通用口那一半 `K-W1A` 已经补上**：\
                  今天那处口不再是 cc-bus 专用的，壳只是它的第一个消费者",
        },
        Candidate {
            id: "code-picture",
            home: "src-tauri/vendor/code-picture-core",
            semantics: Semantics::Plugin,
            shape: Shape::Sidecar,
            today: "vendor crate 由 path 依赖**编进 monitor**；daemon 侧整棵树零命中",
            gap: "★ **今天对不上**：轴二说 sidecar，而 monitor 侧是内嵌。\
                  `E10` 裁「本区不做」、待决 `EU5` 记着这笔账 —— 本模块只如实登记，不去拆它",
        },
        Candidate {
            id: "cc-spawn",
            home: "shared/cc-bus/scripts/cc-spawn",
            semantics: Semantics::Plugin,
            shape: Shape::Plugin,
            today: "已经是 `ccm` 的前端：走 `CCM_BIN` + `--ccm-probe` 能力协商，且不碰总线目录",
            gap: "「出插件」的落点没定（待决 `EU3`：一条插件命令 vs 整个脚本族当一个包）",
        },
        Candidate {
            id: "ccm",
            home: "shared/ccm",
            semantics: Semantics::BuiltIn,
            shape: Shape::ManagedTool,
            today: "一套通用骨架 + 一张 per-agent 适配表（`E4b`），能力靠 `--ccm-probe` 报",
            gap: "无差 —— 本区只借它的协商形状（`E7`），不改它。\
                  ⚠ 但轴二那一格**落不进 `C21` 的三档**：它是受管工具，这件事本身就是读数",
        },
    ];

    // ───────────────────────────── 量具 ─────────────────────────────

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级 = 仓根")
            .to_path_buf()
    }

    /// 读一份**必须存在**的文件（相对仓根）。
    ///
    /// ⚠ 刻意不 `unwrap_or_default()`：读不到会静默变成空串，而空串让下面每一条
    /// 零命中地绿（`cross_half_edge_registry` 头注逐字记过这个坑）。字节地板同理 ——
    /// 文件被清空与文件内容变了，是两种完全不同的失败。
    fn must_read(rel: &str, min_bytes: usize) -> String {
        let p = repo_root().join(rel);
        let disk_text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}: {e}"));
        assert!(
            disk_text.len() >= min_bytes,
            "{rel} 只读到 {} 字节（地板 {min_bytes}）—— 文件被清空/搬走了，本条此刻是空转的",
            disk_text.len()
        );
        disk_text
    }

    /// 一份 Rust 源码的**生产段**（剥 `#[cfg(test)]` 段 + `//` 整行注释）。
    fn rust_production(rel: &str, min_bytes: usize) -> String {
        let prod = guard_core::production_code(&must_read(rel, min_bytes));
        guard_core::assert_no_test_code(rel, &prod);
        prod
    }

    /// 数一个子串出现几次。
    ///
    /// ⚠ 参数化（needle 不是字面量）是刻意的：它让本函数落在
    /// `needle_anchor_registry` 那条棘轮的人群之外 —— 但**代价要说清**：
    /// 子串计数的匹配单位比事实小。所以凡是「恰好一处」型的事实，
    /// 下面一律用 `guard_core::find_pinned` 而不是本函数；本函数只用于
    /// 「这一段里有几个同形的东西」（段界已经先被 [`segment_after`] 界定过）。
    fn occurrences(hay: &str, needle: &str) -> usize {
        hay.matches(needle).count()
    }

    /// 把一段**界定**出来：从 `anchor`（必须在 `hay` 里恰好一处）之后，到 `close` 为止。
    ///
    /// 「先界段、再在段内数」是本模块所有计数的形状。不界段的话，
    /// 同一个形态在文件别处出现一次，计数就悄悄地变了（坏法②：比的是任意一处）。
    fn segment_after(hay: &str, anchor: &str, close: &str) -> String {
        let at = guard_core::find_pinned(hay, anchor)
            .unwrap_or_else(|e| panic!("段界锚点 `{anchor}` 钉不住：{e}"));
        let tail = &hay[at + anchor.len()..];
        let end = tail.find(close).unwrap_or(tail.len());
        tail[..end].to_string()
    }

    /// daemon 命令注册表里的**每一条命令名**（`REGISTRY` 那张表内，段界之内）。
    fn daemon_command_names() -> Vec<String> {
        let prod = rust_production("remote-daemon-proto/src/inbound.rs", 10_000);
        let seg = segment_after(&prod, "const REGISTRY: &[CommandSpec] = &[", "\n];");
        let mut out = Vec::new();
        let key = format!("name{} \"", ':');
        let mut from = 0usize;
        while let Some(rel) = seg[from..].find(key.as_str()) {
            let at = from + rel + key.len();
            let Some(end) = seg[at..].find('"') else { break };
            out.push(seg[at..at + end].to_string());
            from = at + end;
        }
        out
    }

    /// `shared/ccm` 每个 `agent_*` 适配函数的 **claude 臂 / codex 臂**。
    ///
    /// 取法三步，每步都**界段**，不靠「从文件开头找第一个」：
    /// ① 行首 `agent_` 且带 `()` 的行是函数起点；
    /// ② 从起点切到**它自己的** `esac`（段外的 `claude)` 因此读不进来）；
    /// ③ 段内取 `claude)` / `codex)` 的臂体（到 `;;` 为止）；缺哪一臂就落到 `*)`。
    ///
    /// ⚠ ③ 那个「缺就落到 `*)`」不是将就：`agent_has_identity` 与 `agent_needs_bus_id`
    /// 今天就是这么写的（只列一个 agent，另一个走通配），照 shell 的真实语义取才对。
    fn ccm_agent_arms() -> Vec<(String, String, String)> {
        let ccm = guard_core::strip_hash_comment_lines(include_str!("../../shared/ccm"));
        let head = format!("\n{}_", "agent");
        let mut out = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = ccm[from..].find(head.as_str()) {
            let at = from + rel + 1;
            from = at + 1;
            let tail = &ccm[at..];
            let Some(paren) = tail.find("()") else { continue };
            let name = tail[..paren].trim().to_string();
            // 「行首那个词就是函数名」—— 带空格说明这行不是函数定义（是调用或散文）。
            if name.contains(' ') {
                continue;
            }
            let Some(esac) = tail.find("esac") else { continue };
            let block = &tail[..esac];
            out.push((name, case_arm(block, "claude"), case_arm(block, "codex")));
        }
        out
    }

    /// 一个 `case` 段里某个 agent 的臂体（空白归一化）。缺这一臂就取通配臂 `*)`。
    fn case_arm(case_block: &str, agent: &str) -> String {
        let pat = format!("{agent})");
        let seg = match case_block.find(pat.as_str()) {
            Some(i) => &case_block[i + pat.len()..],
            None => match case_block.find("*)") {
                Some(i) => &case_block[i + 2..],
                None => return String::new(),
            },
        };
        let end = seg.find(";;").unwrap_or(seg.len());
        seg[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// `--ccm-probe` 那一行里某个 `key=` 的值列表。
    ///
    /// 锚点带上 shell 源码里那个**两字符的 `\n` 转义**（`printf` 串里的换行是写出来的，
    /// 不是真换行）—— 不带它的话 `capabilities=` 的左边是字母 `n`，
    /// `find_pinned` 会判成「被撑大的命中」。这一条是写判据时当场撞出来的。
    fn ccm_probe_values(key: &str) -> Vec<String> {
        let ccm = guard_core::strip_hash_comment_lines(include_str!("../../shared/ccm"));
        let anchor = format!("\\n{key}=");
        segment_after(&ccm, &anchor, "\\n")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// cc-bus **专有**的数据布局针（与 daemon 的 `cc_bus_boundary_guard` 同一组）。
    ///
    /// ⚠ 运行时拼：写成字面量的话，本文件就成了下一个扫描型判据的假语料
    /// （`panorama_seam_registry` 头注逐字记过「判据的针不只会读到自己，还会喂给别人」）。
    /// ⚠ 只留 cc-bus **专有**的名字：那条判据第一版把 `.jsonl` 列进来，
    /// 误伤了读 Claude 转录的两个文件 —— 判据一旦误伤，它的诊断文案就成了假话。
    fn bus_data_needles() -> Vec<String> {
        vec![
            format!("agents{}tsv", "."),
            format!("spawned{}tsv", "."),
            format!("{}cc-bus", "."),
            format!("cc-bus{}inbox", "/"),
            format!("lastread{}", "-"),
        ]
    }

    // ───────────────────────────── 判据 ─────────────────────────────

    /// `EF01-Y1`：登记表就是 `E4` 点名的那四个候选，**一个不多一个不少**，且住址不烂。
    #[test]
    fn the_registry_covers_exactly_the_four_candidates_the_charter_names() {
        let mut ids: Vec<&str> = REGISTERED.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        assert_eq!(
            ids,
            vec!["cc-bus", "cc-spawn", "ccm", "code-picture"],
            "分类表的候选集变了。\n\
             ⇒ **别直接改这里**：`E4` 那张表是定框条款（且今天还没获批），\
             多一个候选意味着先去那张表里答完两轴，再回来登记它今天的形态。"
        );
        assert_eq!(ids.len(), REGISTERED.len(), "候选 id 有重复 —— 键不唯一，诊断会指不明");

        // ★ 反向那半：登记表**只许记真事**。住址没了 ⇒ 这一行已经烂了，不是「候选消失了」。
        let mut rotten: Vec<&str> = Vec::new();
        for c in REGISTERED {
            if !repo_root().join(c.home).exists() {
                rotten.push(c.home);
            }
        }
        assert!(
            rotten.is_empty(),
            "登记表里这些住址在仓里不存在了：{rotten:?}\n\
             ⇒ 搬家了就同轮改键；真没了就把整行删掉并回 `E4` 说明。\
             登记表腐烂比没有登记更糟 —— 它让「有东西守着」这句话变成假话。"
        );
    }

    /// `EF01-Y2`：**每个候选两轴都答了**（`E3`：缺一轴的结论不算结论），且差在哪写下来了。
    #[test]
    fn every_candidate_answers_both_axes_and_states_its_gap() {
        let mut unanswered: Vec<String> = Vec::new();
        let mut silent: Vec<&str> = Vec::new();
        for c in REGISTERED {
            if c.semantics == Semantics::Unanswered {
                unanswered.push(format!("{} 轴一（谁的语义）", c.id));
            }
            if c.shape == Shape::Unanswered {
                unanswered.push(format!("{} 轴二（几个二进制）", c.id));
            }
            // 「今天什么样」与「差在哪」都是这张表的正文，空着等于这一行没写。
            if c.today.chars().count() < 8 || c.gap.chars().count() < 4 {
                silent.push(c.id);
            }
        }
        assert!(
            unanswered.is_empty(),
            "这些格子没答：{unanswered:?}\n\
             ⇒ `E3` 逐字「每个候选必须**同时**答两轴，缺一轴的结论不算结论」。\
             实测过一次代价：`code-picture` 当初只答了轴二（sidecar）没答轴一，\
             于是 monitor 侧内嵌那笔账拖到 `E10` 才被量出来。"
        );
        assert!(
            silent.is_empty(),
            "这些候选的「今天什么样 / 差在哪」是空的：{silent:?}\n\
             ⇒ 分类是提案，**这两列才是本模块钉的东西**。空着的话，\
             读者会把提案那两列当成现状。"
        );
    }

    /// `EF01-Y3`：`cc-bus` 今天**只经命令面**被够到 —— 脚本族条数 · `bus-*` 命令条数 ·
    /// 起进程口**恰好一处**且住在通用调用口里（转调壳里**零处**）· 边界判据还在且针没缩。
    ///
    /// ⚠ 第 ③ 段的主语 08-26 换过位置（见模块头注那一节）：换的是**守在哪个文件**，
    /// 不是「恰好一处」这个性质本身 —— 它今天由**两条一起**守（一处 + 零处）。
    #[test]
    fn cc_bus_is_reached_only_through_its_command_surface_today() {
        // ① 脚本族：`shell_scripts` 走的是「`.sh` 或 shebang 带 sh」，不是按后缀一种取。
        let scripts = guard_core::shell_scripts(&repo_root().join("shared/cc-bus/scripts"));
        assert_eq!(
            scripts.len(),
            14,
            "cc-bus 脚本族从 14 个变成 {} 个：{scripts:?}\n\
             ⇒ 这不是要你改数字了事：加/删一条脚本 = 这个「插件包」的**命令面**变了，\
             而 `EU3` 正卡在「插件的粒度是命令还是包」上 —— 变了就该回去看那条待决。",
            scripts.len()
        );

        // ② daemon 命令表里的 `bus-*`。
        let names = daemon_command_names();
        assert!(
            names.len() >= 5,
            "只从 daemon 命令表里抠出 {} 条命令名 —— 抽取器坏了，本条此刻是空转的：{names:?}",
            names.len()
        );
        let bus: Vec<&String> = names.iter().filter(|n| n.starts_with("bus-")).collect();
        assert_eq!(
            bus.len(),
            3,
            "daemon 转调 cc-bus 的命令从 3 条变成 {} 条：{bus:?}\n\
             ⚠ 顺带纠一条读数：`C19` 的 ⚠ 里写的是「`bus-*` **四条**」，\
             而 `inbound::REGISTRY` 今天实测是**三条**（`bus-list`/`bus-send`/`bus-kill`），\
             刻意没有 `bus-recv`（`cc-recv` 有副作用，daemon 代读等于把消息从人那里偷走）。",
            bus.len()
        );

        // ③ 起进程口**恰好一处**，且那一处住在**通用调用口**里 —— `E5`/`EL1` 那条
        //    「两条命令共用这一处口」。⚠ 08-26 `K-W1A` 把这处口从转调壳搬到了通用层，
        //    所以这一格由**两条一起**守：口那边恰好一处 · 壳这边零处。少哪一条都会漏掉
        //    一种真实的坏形状（多起一处 / 壳里又长回一处 = 绕开通用口）。
        let port = rust_production("remote-daemon-proto/src/plugin/invoke.rs", 5_000);
        guard_core::find_pinned(&port, "Command::new(").unwrap_or_else(|e| {
            panic!(
                "通用调用口 `plugin/invoke.rs` 里的起进程口不是恰好一处：{e}\n\
                 ⇒ `E5` 的默认是「插件调用**复用**这一处口」，08-26 起那一处就住在这个文件里。\
                 一处都找不到 ⇒ 口又被搬走了（跟着改这里的文件名，别删断言）；\
                 多于一处 ⇒ 通用口自己开了第二条起进程的路。真要加，得同时做三件事：\
                 改 `readonly_guard::spawn_registry::SPAWN_SITES_TODAY`（相等断言）\
                 并在 `ALLOWED` 里写明理由 · 期限仍住子进程（`timeout` 前缀）· \
                 找不到 `timeout` 时如实降级并写进头注。"
            )
        });
        let shell = rust_production("remote-daemon-proto/src/control/cc_bus.rs", 5_000);
        assert_eq!(
            occurrences(&shell, "Command::new("),
            0,
            "cc-bus 转调壳里又长回了起进程口（{} 处）——那等于**绕开通用调用口**。\n\
             ⇒ 08-26 之前这处口就住在这个壳里，`K-W1A` 把它抽进了 `plugin/invoke.rs`；\
             壳今天的身份只是那处口的**第一个消费者**。壳里再起进程 = 通用口白抽了，\
             而且下一个插件会照着壳的样子再起一处（`SPAWN_SITES_TODAY` 9 → 10 → …）。\
             真有非走不可的理由，先去 `E5`/`EU3` 把账改了，再回来改这一条。",
            occurrences(&shell, "Command::new(")
        );

        // ④ 「零文件格式耦合」这句话**靠谁**成立 —— 那条判据还在，且针没缩。
        let boundary = guard_core::strip_comment_lines(&must_read(
            "remote-daemon-proto/src/cc_bus_boundary_guard.rs",
            1_000,
        ));
        let needles = segment_after(&boundary, "let needles = [", "];");
        assert_eq!(
            occurrences(&needles, "format!("),
            bus_data_needles().len(),
            "`cc_bus_boundary_guard` 的**专有针**数量与本表登记的对不上。\n\
             ⇒ 本表说 cc-bus 那格「零文件格式耦合」，而那句话完全压在那条判据上。\
             `E6` 说的是**每加一个插件加它自己那组专有针**（针只增不减，且不许用一根通用针盖住所有插件）\
             —— 所以这个数变大是好事，但要有人当场看见。"
        );
    }

    /// `EF01-Y4`：`code-picture` 今天**编在 monitor 里**、**不在 daemon 里** ——
    /// 这一格是本表唯一一处「提案与现状对不上」，把它钉成会红的，而不是抹平。
    #[test]
    fn code_picture_is_compiled_into_the_monitor_and_absent_from_the_daemon() {
        // ① monitor 侧：那条 path 依赖**整行**在（`pin_line` 而不是子串 —— 事实就是「有这么一行」）。
        let cargo = must_read("src-tauri/Cargo.toml", 1_000);
        let dep = format!("code-picture-core = {} path = \"vendor/code-picture-core\" {}", '{', '}');
        guard_core::pin_line(&cargo, &dep).unwrap_or_else(|e| {
            panic!(
                "monitor 的 `Cargo.toml` 里那条 vendor path 依赖不见了或变形了：{e}\n\
                 ⇒ 本表 `code-picture` 那一行的 `today` 逐字说「**编进 monitor**」。\
                 真改成了别的形态（sidecar / 可选 feature），那是待决 `EU5` 的答案落地 —— \
                 请先去 `E10`/`EU5` 把账改了，再回来改这一行，别反过来。\n\
                 ⚠ 只是**改了写法**（空格 / 换成表段形式）而语义没变的话，同轮把这里的\
                 期望串一起改 —— 用整行相等是刻意的：子串会被「path 改指别处」\
                 这种撑大式改动从缝里溜过去（`needle_anchor_registry` 那一族）。"
            )
        });

        // ② daemon 侧的依赖清单：零命中（**第一层**；③ 是第二层，扫源码树）。
        //    ⚠ 这两层守的是「**vendor 全景引擎不许进 daemon**」，**不是** `C18`「依赖树零 C」——
        //    后者 08-29 已被推翻，规矩没变、换的是理由，逐字与住址见下面两条的失败文案。
        let daemon_cargo =
            guard_core::strip_hash_comment_lines(&must_read("remote-daemon-proto/Cargo.toml", 500));
        assert!(
            !guard_core::contains_word(&daemon_cargo, "code-picture-core"),
            "daemon 的**依赖树**里出现了 vendor 全景引擎（`code-picture-core`）——\
             这一条今天仍是红线，不是权衡项。\n\
             ⚠ **别拿 `C18` 当它的理由**：那条「daemon 不引 C 生态链」**已被推翻** ——\
             盘上逐字「~~**C18** daemon 不引 C 生态链~~ **已被推翻（08-29）**」\
             （住址 backend-consolidation 的 `MASTERPLAN.md:58`；现行版本是 `K30`）。\
             **规矩没变，换的是理由**，而撑着它的两样就写在这里，不用去别处找：\n\
             ① `C21`〔用户 08-14 当面裁〕逐字「code-picture 走『一等公民 + 独立二进制』（sidecar），\
             不是第三方插件，**也不编进 daemon**」；\n\
             ② 实测代价：编进去 3.5 MB → 17.43 MB，且 aarch64 交叉编译当场失败\
             （`rusqlite(bundled)` 是整份 SQLite C 源码 + 9 门 tree-sitter grammar）。\
             ⚠ 这两个数是 `plugin-split` 的 `E10` 当年量的，而 `K30`（08-29）已判\
             「那个 `3.5 MB` 的分母馊了、增量要重测」⇒ 当**量级**读，别当今天的精确值。"
        );

        // ③ daemon 侧**整棵源码树**：零命中（第二层 —— 这一层 `protocol_doc_guard` 够不到，
        //    它只扫协议面那两个文件）。
        let root = repo_root();
        let files = guard_core::scan_tree!(&root.join("remote-daemon-proto/src"), &["rs"]);
        assert!(
            files.len() >= 30,
            "只遍历到 {} 个 daemon 源文件 —— 遍历坏了，本条此刻是空转的（08-14 实测 53）",
            files.len()
        );
        let needle = format!("code{}picture", '_');
        let mut hits: Vec<String> = Vec::new();
        for (path, src) in &files {
            if guard_core::contains_word(&guard_core::production_code(src), &needle) {
                hits.push(
                    path.strip_prefix(&root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
        assert!(
            hits.is_empty(),
            "daemon 的**源码树**（生产段）里出现了全景引擎：{hits:?}\n\
             ⇒ 本条钉的是「**vendor 全景引擎不许进 daemon**」—— 上面 ② 钉依赖树，这一条钉源码树。\n\
             ⚠ **别拿 `C18` 当它的理由**：那条「daemon 的依赖树里不许出现 C」**已被推翻** ——\
             盘上逐字「~~**C18** daemon 不引 C 生态链~~ **已被推翻（08-29）**」\
             （住址 backend-consolidation 的 `MASTERPLAN.md:58`；现行版本是 `K30`）。\
             **规矩没变，换的是理由**，而撑着它的两样就写在这里，不用去别处找：\n\
             ① `C21`〔用户 08-14 当面裁〕逐字「code-picture 走『一等公民 + 独立二进制』（sidecar），\
             不是第三方插件，**也不编进 daemon**」⇒ 要给 daemon 全景能力，答案是 sidecar，不是内嵌\
             （独立二进制，它自己那份 C 依赖跟着它走）；\n\
             ② 实测代价：编进去 3.5 MB → 17.43 MB，且 aarch64 交叉编译当场失败。\
             ⚠ 这两个数是 `plugin-split` 的 `E10` 当年量的，而 `K30`（08-29）已判\
             「那个 `3.5 MB` 的分母馊了、增量要重测」⇒ 当**量级**读，别当今天的精确值。"
        );
    }

    /// `EF01-Y5`：`cc-spawn` 今天**已经是 `ccm` 的前端** —— 走 `CCM_BIN` + 能力协商，
    /// 且「本脚本不碰总线目录了」这句话是真的。
    #[test]
    fn cc_spawn_is_a_frontend_of_ccm_and_touches_no_bus_data() {
        let spawn = guard_core::strip_hash_comment_lines(include_str!(
            "../../shared/cc-bus/scripts/cc-spawn"
        ));
        assert!(
            spawn.len() > 1_000,
            "剥完注释只剩 {} 字节 —— 剥法或路径坏了，下面两条此刻是空转的",
            spawn.len()
        );
        for token in ["CCM_BIN", "--ccm-probe"] {
            assert!(
                guard_core::contains_word(&spawn, token),
                "`cc-spawn` 的生产段里没有 `{token}` —— 它就不再是 `ccm` 的前端了。\n\
                 ⇒ 本表 `cc-spawn` 那一行、以及 `EU3` 默认取「粒度是包」的理由\
                 （「`cc-spawn` 里有多少是 cc-bus 之外的语义」实测是 0），\
                 都建立在这两件事上。真要改回自己起会话，先去 `EU3` 把账改了。\n\
                 ⚠ 尤其是 `--ccm-probe`：脚本头注逐字记着不检的后果 —— \
                 把「版本太旧」说成「建会话失败」。"
            );
        }
        let mut touched: Vec<String> = Vec::new();
        for n in bus_data_needles() {
            if spawn.contains(n.as_str()) {
                touched.push(n);
            }
        }
        assert!(
            touched.is_empty(),
            "`cc-spawn` 又碰总线数据了：{touched:?}\n\
             ⇒ `C15` 之后那三件 cc-bus 专属逻辑（命名避让 · 总线登记 · 台账）已经搬进 `ccm`，\
             脚本头注逐字写着「本脚本不碰总线目录了」。\
             `E6` 的通则同理：拿不到的东西**给它加一条命令**，不是绕到背后读文件。"
        );
    }

    /// `EF01-Y6`：`ccm` 今天是**一套骨架 + 一张 per-agent 适配表**（`E4b`），
    /// 而它在轴二上**落不进三档**（受管工具）。
    #[test]
    fn ccm_is_one_skeleton_with_a_per_agent_table() {
        let arms = ccm_agent_arms();
        let names: Vec<&str> = arms.iter().map(|(n, _, _)| n.as_str()).collect();
        assert_eq!(
            arms.len(),
            5,
            "`shared/ccm` 的 per-agent 适配函数从 5 个变成 {} 个：{names:?}\n\
             ⇒ `E4b` 裁的是「通用骨架不动，加一张表的一行」。多一个函数 = 分叉面变大，\
             那正是该有人过一眼的时刻；少一个 = 要么收敛了（好事，改这个数），\
             要么抽取器坏了（`agent_` 开头 + `()` + 到自己的 `esac`）。",
            arms.len()
        );

        // ★ E4b 那个「4 项不同」的口径：**取值不同**是 5/5，减掉「同名参数化」那一项才是 4。
        //   两个数都对，量的不是同一件事 —— 所以两个都断言，谁引用谁看得见口径。
        let mut diverging = 0usize;
        let mut parameterized: Vec<&str> = Vec::new();
        for (name, claude, codex) in &arms {
            if claude == codex {
                continue;
            }
            diverging += 1;
            // 「同名参数化」= 把 agent 名换成同一个占位符之后两臂逐字相同
            //（`echo claude` vs `echo codex`：那不是形状分叉，是同一个形状带自己的名字）。
            if claude.replace("claude", "<agent>") == codex.replace("codex", "<agent>") {
                parameterized.push(name);
            }
        }
        assert_eq!(
            diverging,
            5,
            "5 个适配函数里只有 {diverging} 个两臂取值不同 —— \
             `E4b` 的前提（「codex 是 ccm 但其实是不同形态的」）在这个口径下不再成立。\n\
             逐条：{arms:?}"
        );
        assert_eq!(
            parameterized.len(),
            1,
            "「同名参数化」的适配函数不是 1 个，而是 {}：{parameterized:?}\n\
             ⇒ `E4b` 写的是「5 项里 codex 有 **4** 项与 claude 不同」。\
             实测按「取值不同」是 **5** 项；那个 4 = 5 减掉 `agent_default_launcher`\
             （`echo claude` vs `echo codex` —— 同一个形状带自己的名字，不是形状分叉）。\
             两个数都对，量的不是同一件事，本条把口径钉下来免得下一个人再对一次。",
            parameterized.len()
        );

        // 能力协商面（`E7`/`EL3`）：token 是**集合**，判「会不会做某件事」问集合，不比版本号。
        let caps = ccm_probe_values("capabilities");
        assert_eq!(
            caps.len(),
            17,
            "`--ccm-probe` 的能力 token 从 17 个变成 {}：{caps:?}\n\
             ⇒ 这是插件协商的**样板**（`E7`：一条 probe 子命令 → `key=value` 行 → \
             消费者声明它要哪些 token）。加能力是好事，但今天已有两个真实消费者\
             （`shared/cc-bus/scripts/cc-spawn` 检 4 个 token · `src/launch-render-cli.ts` 的 \
             `CLI_REQUIRED_CAPS` 检 7 个），这个数变了要顺手看一眼它们。\n\
             ⚠ 16 → 17 是 `K-C1`（08-24）加的 `account-via-daemon`。**PM 落这一格前逐个读过那两个消费者**：\
             `cc-spawn` 是 `for _c in detach tmux-size tmux-base bus-register` 逐个查逗号列表（**子集检查**）；\
             `CLI_REQUIRED_CAPS` 是一个 7 元必需列表（**也是子集检查**）⇒ **加 token 安全，删/改名才危险**。\
             ⇒ 下一个人加 token 时不必重读这两处；**改名或删 token 时必须重读**。",
            caps.len()
        );
        assert_eq!(
            ccm_probe_values("agents"),
            vec!["claude".to_string(), "codex".to_string()],
            "`--ccm-probe` 报的 agent 集合变了 —— `E4b` 的 per-agent 表要跟着加行，\
             而 `E4c` 记着那张表今天有**三份副本**（`golden.tsv` 4 key · ccm 5 函数 · \
             daemon `agents/*/resume.rs`），真相源只覆盖一半。"
        );

        // 轴二那一格：它是**受管工具**，不是三档中的任何一档。
        let tools = rust_production("src-tauri/src/tool_registry.rs", 10_000);
        let key = format!("id{} \"ccm\"", ':');
        guard_core::find_pinned(&tools, &key).unwrap_or_else(|e| {
            panic!(
                "`ccm` 不在受管工具表里了：{e}\n\
                 ⇒ 本表给 `ccm` 的轴二填的是「受管工具」，并逐字写着它**落不进 `C21` 的三档**。\
                 那句话就靠这一条撑着。"
            )
        });
    }

    /// `EF01-Y7`：★ **分类结论今天只是判据语料，没有任何生产代码消费它。**
    ///
    /// 这条是 `ED1` 那句「提案 ≠ 生效」的机检形态：定框还没批，
    /// 谁想把这张表接进运行期行为（按分类决定走哪条路），会先撞到这里、先读到头注。
    ///
    /// ⚠ 它**不挡**「在别处另写一份同样的分类」—— 那是复制，不是消费，
    /// 本条看不见。诚实边界登记在功能件 §4。
    #[test]
    fn the_classification_is_guard_corpus_only_and_no_production_code_consumes_it() {
        // ① 本模块自己整体在 `#[cfg(test)]` 内 ⇒ 生产段为空。
        let me = guard_core::production_code(include_str!("plugin_class_registry.rs"));
        assert!(
            me.trim().is_empty(),
            "本模块的生产段不再是空的（{} 字节）—— 分类表变成了运行期数据结构。\n\
             ⇒ `E4` 那张表**还没获批**（`DECISIONS.md#ED1`）。它今天的身份是\
             「今天的形态长这样」的判据语料，不是「将来必须这样」的裁定。\n\
             真获批了要落地的是 `EF02`–`EF06`，不是把这张表搬进生产段。",
            me.trim().len()
        );

        // ② 两棵树的生产段里，没有任何文件引用本模块。
        let root = repo_root();
        let mut files = guard_core::scan_tree!(&root.join("src-tauri/src"), &["rs"]);
        files.extend(guard_core::scan_tree!(
            &root.join("remote-daemon-proto/src"),
            &["rs"]
        ));
        assert!(
            files.len() >= 100,
            "只扫到 {} 个源文件 —— 遍历坏了，本条此刻是空转的（08-14 实测 135+）",
            files.len()
        );
        let myself = format!("plugin_class{}registry", '_');
        let decl = format!("mod {myself};");
        let mut consumers: Vec<String> = Vec::new();
        for (path, src) in &files {
            for line in guard_core::production_code(src).lines() {
                // `lib.rs` 里那一行模块声明是它存在的方式，不是消费。
                if line.trim() == decl {
                    continue;
                }
                if guard_core::contains_word(line, &myself) {
                    consumers.push(format!(
                        "{}: {}",
                        path.strip_prefix(&root)
                            .unwrap_or(path)
                            .to_string_lossy()
                            .replace('\\', "/"),
                        line.trim()
                    ));
                }
            }
        }
        assert!(
            consumers.is_empty(),
            "有生产代码引用了这张**还没获批**的分类表：\n  {}\n\
             ⇒ 见上一条的诊断：分类是提案，落地归 `EF02`–`EF06`。",
            consumers.join("\n  ")
        );
    }

    /// 抽取器的**行为**自检：喂人造语料，臂体取法必须只取该取的那一段。
    ///
    /// 没有这条，上面那个 `5 / 1` 只是「今天碰巧数出来的两个数」——
    /// 取法坏掉（比如把整个 `case` 段当臂体）时它照样可能落在同一个数上。
    #[test]
    fn the_arm_extractor_takes_one_arm_not_the_whole_case() {
        let block = "case \"$1\" in claude) echo A ;; codex) echo B ;; esac";
        assert_eq!(case_arm(block, "claude"), "echo A");
        assert_eq!(case_arm(block, "codex"), "echo B");
        // 缺这一臂 ⇒ 落到通配臂（`agent_has_identity` 今天就是这个形状）。
        let wild = "case \"$1\" in claude) return 0 ;; *) return 1 ;; esac";
        assert_eq!(case_arm(wild, "codex"), "return 1");
        assert_eq!(case_arm(wild, "claude"), "return 0");
        // 段界自检：`;;` 之后的东西不许被吃进来。
        assert!(
            !case_arm(block, "claude").contains('B'),
            "臂体取过头了 —— 取到了下一臂，那样两臂永远「相同」，分叉计数会静默归零"
        );
    }
}
