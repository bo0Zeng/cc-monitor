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
//! 抽到**通用调用口** `src/backend/plugin/invoke.rs`。
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
//! · daemon 不许碰 cc-bus 的**数据布局** → `src/backend/cc_bus_boundary_guard.rs`；
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
            home: "src/shared/cc-bus/scripts",
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
            home: "src/shared/cc-bus/scripts/cc-spawn",
            semantics: Semantics::Plugin,
            shape: Shape::Plugin,
            today: "已经是 `ccm` 的前端：走 `CCM_BIN` + `--ccm-probe` 能力协商，且不碰总线目录",
            gap: "「出插件」的落点没定（待决 `EU3`：一条插件命令 vs 整个脚本族当一个包）",
        },
        Candidate {
            id: "ccm",
            // 🔴 〔`K-R48` 第二拍 09-11〕住址从那份已删的 bash `ccm` 换到这里：脚本删了，
            //    `ccm` 今天是后端二进制的一次性模式（`K33`：「不要有什么单独的 ccm」）。
            home: "src/backend/control/ccm",
            semantics: Semantics::BuiltIn,
            shape: Shape::ManagedTool,
            today: "一套通用骨架 + 一张 per-agent 适配表（`E4b`），能力靠 `--ccm-probe` 报。\
                    〔`K-R48` 09-11〕它**就是后端本体**的一种跑法，不再是一个独立脚本",
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
        let prod = rust_production("src/backend/inbound.rs", 10_000);
        let seg = segment_after(&prod, "const REGISTRY: &[CommandSpec] = &[", "\n];");
        let mut out = Vec::new();
        let key = format!("name{} \"", ':');
        let mut from = 0usize;
        while let Some(rel) = seg[from..].find(key.as_str()) {
            let at = from + rel + key.len();
            let Some(end) = seg[at..].find('"') else {
                break;
            };
            out.push(seg[at..at + end].to_string());
            from = at + end;
        }
        out
    }

    /// 🔴 〔`K-R48` 第二拍 09-11〕**这里原来有三个从那份已删的 bash `ccm` 里抠的取法**
    /// （`ccm_agent_arms` 〔散文墓碑〕 逐个切 `agent_*` 函数的 `case` 臂 · `case_arm` · `ccm_probe_values` 〔散文墓碑〕）。
    /// 〔用@09-11 `K33`〕那个脚本删了，per-agent 适配表与 probe 那一行搬进了
    /// `src/backend/control/ccm/`（Rust）⇒ 取法整块换成读那份源码的 `const`。
    ///
    /// **为什么运行期读文件而不是 `include_str!`**：后者会在 monitor 与 daemon 之间造一条
    /// **编译期**跨 crate 边（`cross_half_edge_registry` 那族要单独登记），而本条要的只是一份文本。
    fn ccm_module_source(file: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .join("src/backend/control/ccm")
            .join(file);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {}：{e}", p.display()))
    }

    /// `control/ccm/mod.rs` 里某个 `&[&str]` 常量的成员。
    ///
    /// 🔴 抠不到 / 抠出空表就 **panic**：回空表会让上层的计数断言变成「零命中地绿」。
    fn ccm_const_list(name: &str) -> Vec<String> {
        let src = ccm_module_source("mod.rs");
        let head = format!("const {name}: &[&str] = &[");
        assert_eq!(
            src.matches(head.as_str()).count(),
            1,
            "`{name}` 在 `control/ccm/mod.rs` 里不是恰好一处 —— 抠法坏了"
        );
        let at = src.find(head.as_str()).unwrap() + head.len();
        let tail = &src[at..];
        let end = tail.find("];").expect("那个常量没有收尾 `];`");
        let out: Vec<String> = tail[..end]
            .split(',')
            .filter_map(|s| {
                let s = s.trim();
                s.strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .map(str::to_string)
            })
            .collect();
        assert!(!out.is_empty(), "`{name}` 抠出来是空表 —— 本条此刻是空转的");
        out
    }

    /// per-agent 适配函数的名字：`control/ccm/mod.rs` 里**按 agent 分支**的那几个。
    ///
    /// 取法：行首 `pub(crate) fn <名>(agent: &str)` —— 它们的共同形状是
    /// 「吃一个 agent 名，回这个 agent 的那一份」。⚠ 抠不到就 panic（免得零命中地绿）。
    ///
    /// 🔴 **人群只到 `control/ccm/mod.rs` 为止，`agents/mod.rs::account_env_of` 刻意不算**：
    /// 后者是 daemon **早就有**的东西（「切账号靠改哪个环境变量」），`ccm` 只是**问它要**
    /// （`mod.rs` 头注逐字「本文件不认识任何 agent 的名字」）。把它数进来，
    /// 这个数就从「`ccm` 的 per-agent 表有多大」变成「全仓有几个吃 agent 名的函数」——
    /// **那是另一个量**，而 `E4b` 裁的是前者。〔本拍现打时它真的混进来过一次，读数 6 vs 5。〕
    fn ccm_per_agent_fns() -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for line in guard_core::production_code(&ccm_module_source("mod.rs")).lines() {
            let l = line.trim();
            let Some(rest) = l.strip_prefix("pub(crate) fn ") else {
                continue;
            };
            let Some((name, args)) = rest.split_once('(') else {
                continue;
            };
            if args.starts_with("agent: &str)") {
                out.push(format!("mod.rs::{name}"));
            }
        }
        assert!(
            !out.is_empty(),
            "一个 per-agent 适配函数都抠不到 —— 抠法坏了"
        );
        out
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
        assert_eq!(
            ids.len(),
            REGISTERED.len(),
            "候选 id 有重复 —— 键不唯一，诊断会指不明"
        );

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
        let scripts = guard_core::shell_scripts(&repo_root().join("src/shared/cc-bus/scripts"));
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
            4,
            "daemon 转调 cc-bus 的命令从 4 条变成 {} 条：{bus:?}\n\
             今天这四条是 `bus-list` / `bus-send` / `bus-kill` / `bus-state`\
             （`bus-state` 是 `K-R113` 09-13 补的**具名读命令**：总线名单 ＋ spawn 台账一次回全）。\n\
             ⚠ **`C19` 的 ⚠ 与实测今天对上了**：那里写的是「`bus-*` 四条」，实测也是四条 ——\
             ⚠⚠ 而这一句此前**反过来是过期的**（它逐字写着「`C19` 说四条、实测三条」，\
             `K-R113` 之后实测就是四条了）⇒ 本行是那次订正的订正，别再照旧读。\n\
             ★ **仍然刻意没有 `bus-recv`**：`cc-recv` 有副作用（推进已读位置），\
             daemon 代读等于把消息从人那里偷走 —— 这句今天仍是真的，理由全文在\
             `src/backend/control/cc_bus.rs` 的模块头注 ①。\n\
             🔴 **变了要去看什么：不是 `EU3`。** 本行原先写着「变了就该回去看那条待决\
             （`EU3`：插件的粒度是命令还是包）」，而 `EU3` **今天已经作废** ——\
             `backend-consolidation/OPEN-PREMISES.md` 逐字「`plugin-split` `EF04` 已撤件、\
             `EU3` 同时作废」。⇒ 别去读一份不存在的待决。今天这条绊线买到的是\
             「**daemon 的 cc-bus 命令面长了一条，而 monitor 这张登记表没人看见**」——\
             改这个数之前，先回本表 `cc-bus` 那一行看它的「今天什么样 / 差在哪」还成不成立。",
            bus.len()
        );

        // ③ 起进程口**恰好一处**，且那一处住在**通用调用口**里 —— `E5`/`EL1` 那条
        //    「两条命令共用这一处口」。⚠ 08-26 `K-W1A` 把这处口从转调壳搬到了通用层，
        //    所以这一格由**两条一起**守：口那边恰好一处 · 壳这边零处。少哪一条都会漏掉
        //    一种真实的坏形状（多起一处 / 壳里又长回一处 = 绕开通用口）。
        let port = rust_production("src/backend/plugin/invoke.rs", 5_000);
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
        let shell = rust_production("src/backend/control/cc_bus.rs", 5_000);
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
            "src/backend/cc_bus_boundary_guard.rs",
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
        let dep = format!(
            "code-picture-core = {} path = \"vendor/code-picture-core\" {}",
            '{', '}'
        );
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
        //    后者**已被推翻（08-29）**，盘上逐字「~~**C18** daemon 不引 C 生态链~~ 已被推翻（08-29）」
        //    （住址 backend-consolidation 的 `MASTERPLAN.md:58`；现行版本是 `K30`）。
        //    **规矩没变，换的是理由** —— 新理由（`C21` ＋ 实测代价）写在下面两条的失败文案里，
        //    刻意各写一份：谁踩到哪一条，就只看得到哪一段字。
        let daemon_cargo =
            guard_core::strip_hash_comment_lines(&must_read("src/backend/Cargo.toml", 500));
        assert!(
            !guard_core::contains_word(&daemon_cargo, "code-picture-core"),
            "daemon 的**依赖树**里出现了 vendor 全景引擎（`code-picture-core`）——\
             这一条今天仍是红线，不是权衡项。\n\
             ⚠ **别拿 `C18` 当它的理由**：那条「daemon 不引 C 生态链」**已被推翻** ——\
             盘上逐字「~~**C18** daemon 不引 C 生态链~~ **已被推翻（08-29）**」\
             （住址 backend-consolidation 的 `MASTERPLAN.md:58`；现行版本是 `K30`）。\
             **规矩没变，换的是理由**，而撑着它的两样就写在这里，不用去别处找：\n\
             ① `C21`〔用户 08-14 当面裁〕逐字「code-picture 走『一等公民 + 独立二进制』（sidecar），\
             不是第三方插件，**也不编进 daemon**」—— 这一条是主理由；\n\
             ② 实测代价（`K-R2` 的 `§0b`，09-04 沙箱现打，量于 `e1944e8`，\
             与发版同版本的 zig 0.14.0 + cargo-zigbuild 0.23.0，同一趟同一提交的基线）：\
             daemon 二进制**每架构 +18.87～18.88 MB**（x86_64 4.55→23.43 · aarch64 4.07→22.94，×5.15–5.64），\
             而安装包内嵌两个架构 ⇒ **+37.75 MB**（`rusqlite(bundled)` 是整份 SQLite C 源码 + 9 门 tree-sitter grammar）。\
             而 `K30③`「体积在预算内」的**那个预算今天没有人定过** ⇒ 今天没有账能证明它在预算内。\n\
             ⚠ **盘上流传的两个旧数别再抄**：「3.5 MB → 17.43 MB」**分母与增量都馊了**\
             （旧增量 +13.93 MB，少算约 5 MB）；「aarch64 交叉编译当场失败」是**裸 `cargo build`** 在\
             没装 `aarch64-linux-musl-gcc` 的机器上量的，换成发版真正在用的 zigbuild，\
             `K-R2` 实测**两个架构都 EXIT=0** ⇒ **交叉编译今天不是拦路虎**，是尺子换了，不是结论翻了。"
        );

        // ③ daemon 侧**整棵源码树**：零命中（第二层 —— 这一层 `protocol_doc_guard` 够不到，
        //    它只扫协议面那两个文件）。
        let root = repo_root();
        let files = guard_core::scan_tree!(&root.join("src/backend"), &["rs"]);
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
             ② 实测代价（`K-R2` 的 `§0b`，09-04 沙箱现打，量于 `e1944e8`，与发版同版本的 zigbuild）：\
             daemon 二进制**每架构 +18.87～18.88 MB**（×5.15–5.64），安装包内嵌两个架构 ⇒ **+37.75 MB**；\
             而 `K30③`「体积在预算内」的**那个预算今天没有人定过** ⇒ 今天没有账能证明它在预算内。\n\
             ⚠ **盘上流传的两个旧数别再抄**：「3.5 MB → 17.43 MB」**分母与增量都馊了**（旧增量少算约 5 MB）；\
             「aarch64 交叉编译当场失败」是**裸 `cargo build`** 在没装 `aarch64-linux-musl-gcc` 的机器上量的，\
             换成发版真正在用的 zigbuild，`K-R2` 实测**两个架构都 EXIT=0** ⇒ **交叉编译今天不是拦路虎**。"
        );
    }

    /// `EF01-Y5`：`cc-spawn` 今天**已经是 `ccm` 的前端** —— 走 `CCM_BIN` + 能力协商，
    /// 且「本脚本不碰总线目录了」这句话是真的。
    #[test]
    fn cc_spawn_is_a_frontend_of_ccm_and_touches_no_bus_data() {
        let spawn = guard_core::strip_hash_comment_lines(include_str!(
            "../../src/shared/cc-bus/scripts/cc-spawn"
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
        // 🔴 〔`K-R48` 第二拍 09-11〕**取法换了一次，口径两条都保住，第三条如实降级。**
        //
        // 从前这三格读的是那份已删的 bash `ccm` 脚本：5 个 `agent_*` 函数、
        // 每个函数 `claude)` / `codex)` 两臂的**取值**、`--ccm-probe` 那一行的 token 列表。
        // 〔用@09-11 `K33`〕脚本删了，这三样搬进了 `control/ccm/`（Rust）。
        //
        // ⚠ **「两臂取值不同 5 项 / 同名参数化 1 项」那两格丢了，写清楚**：
        //   那是 shell `case` **文本**才有的形状（两臂各是一段可比较的字面量）。
        //   Rust 侧是 `match` 里的表达式 —— 逐字比它们的文本是在比实现细节，不是在比性质。
        //   ⇒ 本条今天只保住「**per-agent 适配面有几个函数**」这一格（`E4b` 那句
        //   「通用骨架不动，加一张表的一行」靠的正是它），另两格**如实作废，不假装还在**。
        //   那条「codex 与 claude 到底哪几项不同」今天由 daemon 侧
        //   `control::ccm::tests::the_agent_set_has_one_address_and_every_member_is_wired` 逐项钉。
        let fns = ccm_per_agent_fns();
        assert_eq!(
            fns.len(),
            5,
            "per-agent 适配函数从 5 个变成 {} 个：{fns:?}\n\
             ⇒ `E4b` 裁的是「通用骨架不动，加一张表的一行」。多一个函数 = 分叉面变大，\
             那正是该有人过一眼的时刻；少一个 = 要么收敛了（好事，改这个数），\
             要么抽取器坏了（`pub(crate) fn <名>(agent: &str)`，只扫 `control/ccm/mod.rs`）。",
            fns.len()
        );

        // 能力协商面（`E7`/`EL3`）：token 是**集合**，判「会不会做某件事」问集合，不比版本号。
        let caps = ccm_const_list("CAPABILITIES");
        assert_eq!(
            caps.len(),
            18,
            "`--ccm-probe` 的能力 token 从 18 个变成 {}：{caps:?}\n\
             ⇒ 这是插件协商的**样板**（`E7`：一条 probe 子命令 → `key=value` 行 → \
             消费者声明它要哪些 token）。加能力是好事，但今天已有两个真实消费者\
             （`src/shared/cc-bus/scripts/cc-spawn` 检 4 个 token · `src/launch-render-cli.ts` 的 \
             `CLI_REQUIRED_CAPS` 检 7 个），这个数变了要顺手看一眼它们。\n\
             ⚠ 两个消费者**都是子集检查** ⇒ **加 token 安全，删/改名才危险**。\
             ⇒ 下一个人加 token 时不必重读这两处；**改名或删 token 时必须重读**。\n\
             〔`K-R61` 09-11：17 → 18，加的是 `base-url-across-tmux`。\
             『谁在数它』那张表住 `src/backend/control/ccm/mod.rs` 的 \
             `CAPABILITIES` 头注，**本条不复述第二份** —— 只提醒：`src-tauri/build.rs` \
             那个 `extract_capabilities` 抠的是 daemon 流模式那个同名常量，盖不到这里。〕",
            caps.len()
        );
        assert_eq!(
            ccm_const_list("AGENTS"),
            vec!["claude".to_string(), "codex".to_string()],
            "`--ccm-probe` 报的 agent 集合变了 —— `E4b` 的 per-agent 表要跟着加行，\
             而 `E4c` 记着那张表今天有**三份副本**（`golden.tsv` 4 key · `control/ccm` 5 函数 · \
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
            &root.join("src/backend"),
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

    /// 抽取器的**行为**自检：喂人造语料，取法必须只取该取的那一段。
    ///
    /// 没有这条，上面那几个数只是「今天碰巧数出来的数」—— 取法坏掉时它照样可能落在同一个数上。
    ///
    /// 🔴 〔`K-R48` 第二拍 09-11〕**语料从 bash `case` 换成 Rust `const`**：
    /// 被测对象从那份已删的 bash `ccm` 换成了 `src/backend/control/ccm/mod.rs`，自检跟着换语言。
    /// 从前那三格（一臂不许吃到下一臂 / 缺臂落通配 / 段界）随 `case_arm` 一起没了 ——
    /// **不是丢了，是那个形状不存在了**（Rust 的 `match` 没有 `;;` 这个坑）。
    #[test]
    fn the_const_list_extractor_takes_one_list_not_the_whole_file() {
        // 正向：真去抠一次，成员必须是**这一个**常量的（不是把下一个常量也吃进来）。
        let agents = ccm_const_list("AGENTS");
        assert_eq!(agents, vec!["claude".to_string(), "codex".to_string()]);
        // 段界自检：`AGENTS` 抠出来的里面不许出现 `CAPABILITIES` 的成员
        //（吃过头的话两张表会合并，而「17 个 token」那一格会静默地变成另一个数）。
        let caps = ccm_const_list("CAPABILITIES");
        assert!(
            !agents
                .iter()
                .any(|a| caps.contains(a) && a != "claude" && a != "codex"),
            "`AGENTS` 抠过头了 —— 吃到了下一个常量：{agents:?}"
        );
        assert!(
            caps.len() > agents.len(),
            "两张表抠成了同一份 —— 锚点没起作用"
        );
        // per-agent 函数那一格：抠出来的每一项都要带住址前缀（免得两份同名函数被数成一个）。
        let fns = ccm_per_agent_fns();
        for f in &fns {
            assert!(f.starts_with("mod.rs::"), "per-agent 函数名没带住址：{f}");
        }
        // 段界自检：人群刻意**只到 `control/ccm/mod.rs`** —— `agents/mod.rs::account_env_of`
        // 是 daemon 早有的东西，混进来这个数就变成另一个量（见 `ccm_per_agent_fns` 头注）。
        assert!(
            !fns.iter().any(|f| f.contains("account_env_of")),
            "人群扩到 `agents/mod.rs` 了：{fns:?} —— 那个数不再是「`ccm` 的 per-agent 表有多大」"
        );
    }
}
