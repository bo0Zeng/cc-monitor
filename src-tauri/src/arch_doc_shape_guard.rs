//! F19：**顶层架构文档必须覆盖的结构性事实**，以及**它不许再长回逐文件模块表**。
//!
//! # 病：真架构住进了 1517 行的 INVARIANTS，顶层文档在讲文件清单
//!
//! F19 摸底实测（重写前的 `doc/ARCHITECTURE.md`，437 行）：
//!
//! - **`backend` 这个词全文只出现 1 次**，而且指的是**前端**的 `session-backend.ts`；
//! - **「轮询」0 次** —— 本工作区两条主线（backend 边界 · 零轮询）在顶层文档里**根本不存在**；
//! - 一张 **122 行的逐文件模块树**占掉全文四分之一，其中 **82 处**是文件名，
//!   而同一份清单在 `src-tauri/README.md` 里**另有一份**；
//! - 那份文档自己**两次**把「模块表」委派给子目录 README —— 它自己就说过不该放。
//!
//! ⇒ **分层倒挂**：顶层讲实现细节、约束住进了详细文档。新人读完顶层文档会得出
//! 「monitor 只读本机文件」这个结论（§1 的图只画了本机两个源）。
//!
//! # 为什么这条判据不是「又数一遍行数」
//!
//! 行数不是病，**覆盖面**才是。所以这里钉两件事：
//!
//! 1. **存在钉**：那几条结构性事实**必须在文档里**（`REQUIRED_FACTS`）；
//! 2. **形状钉**：文件名提及处数有天花板 —— 逐文件清单**长回来就红**。
//!
//! ⚠ 存在钉最容易做成**恒真断言**（「文档里有 `backend` 这个词」重写之后必然为真）。
//! 所以本模块**自带一条反向断言**：把承载这些事实的那一节剥掉，
//! 若判据仍然全绿，说明它命中的是别处的零碎词句、而不是那一节 ⇒ 那条反向断言会红。
//! 这是 `templates` 里那条「『什么都没发生』型断言必须配一条『让它发生』的反向断言」
//! 在**文档判据**上的形态。

/// 顶层架构文档**必须覆盖**的结构性事实。`(事实, 探针（全部命中才算覆盖）, 缺了会怎样)`。
///
/// # ⚠ 这张表是**规格**，不是「真相源的副本」
///
/// 它不存文档里的任何一句话，只存**几个必须能被找到的锚**。
/// 文档怎么讲随作者，但**讲没讲**由这里判。
///
/// ⚠ 探针刻意选**成对**的（概念词 + 那个概念的落点名），
/// 单个泛词（`backend`）在重写后恒真，钉不住任何东西。
#[cfg(test)]
const REQUIRED_FACTS: &[(&str, &[&str], &str)] = &[
    (
        "backend 边界 = 读（observe）+ 控制（control），一份代码两种承载",
        &["backend = 读", "observe/", "control/", "两种承载"],
        "缺了它，顶层文档里就没有本工作区的主线；重写前实测：`backend` 全文 1 次且指的是前端文件",
    ),
    (
        "零轮询不是口号 —— 它由四张登记表覆盖，且有登记在案的例外",
        &["零轮询", "事件源", "no_timer_guard", "polling_registry"],
        "缺了它，读者会以为「不轮询」只是句纪律；实测重写前「轮询」在全文 0 次",
    ),
    (
        "远端那条数据流链：daemon 的 JSONL 帧走一条 SSH 长连接回来",
        &["ssh_source.rs", "JSONL", "长连接"],
        "缺了它，读者会从 §1 的图得出「monitor 只读本机文件」——重写前的图只有本机两个源",
    ),
    (
        "那条搬不动的边界：最后那次 exec 必须在用户自己的终端里",
        &["exec", "搬不走", "平面 ③"],
        "缺了它，下一个人会试图把开窗也搬进 daemon（定框 C13 逐字写着这条被误读过一次）",
    ),
    (
        "共享 crate 与 daemon 为什么不进 workspace，以及代价",
        &["workspace 成员", "原生构建", "八处"],
        "缺了它，下一个人会「顺手」把 daemon 收进 workspace，或者以为 `cargo test` 覆盖了它",
    ),
];

#[cfg(test)]
mod tests {
    use super::REQUIRED_FACTS;
    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .to_path_buf()
    }

    fn arch_doc() -> String {
        let p = repo_root().join("doc/ARCHITECTURE.md");
        assert!(
            p.is_file(),
            "读不到 {} —— 读不到的文件只会静默返回空串，那会让本模块零命中地绿",
            p.display()
        );
        std::fs::read_to_string(&p).expect("读 ARCHITECTURE.md")
    }

    /// 按 `## N.` 标题切段；返回**剥掉指定那一节之后**的全文。
    ///
    /// 用于反向断言：承载事实的那一节被剥掉，存在钉必须失守。
    fn without_section(src: &str, head_prefix: &str) -> String {
        let lines: Vec<&str> = src.lines().collect();
        let a = lines
            .iter()
            .position(|l| l.starts_with(head_prefix))
            .unwrap_or_else(|| panic!("找不到以 {head_prefix:?} 开头的小节 —— 标题措辞变了"));
        let b = lines[a + 1..]
            .iter()
            .position(|l| l.starts_with("## "))
            .map(|k| a + 1 + k)
            .unwrap_or(lines.len());
        let mut kept: Vec<&str> = Vec::new();
        kept.extend_from_slice(&lines[..a]);
        kept.extend_from_slice(&lines[b..]);
        kept.join("\n")
    }

    /// 某份文本里**没被覆盖**的事实清单：`(事实, 缺哪个探针)`。
    fn uncovered(src: &str) -> Vec<(&'static str, &'static str)> {
        let mut out = Vec::new();
        for (fact, probes, _) in REQUIRED_FACTS {
            for probe in probes.iter() {
                if !src.contains(probe) {
                    out.push((*fact, *probe));
                    break;
                }
            }
        }
        out
    }

    /// ★★ 存在钉：顶层文档仍然覆盖每一条结构性事实。
    #[test]
    fn the_top_level_doc_still_carries_every_structural_fact() {
        let missing = uncovered(&arch_doc());
        assert!(
            missing.is_empty(),
            "`doc/ARCHITECTURE.md` 丢了结构性事实：{missing:?}\n\
             ⚠ **别改这张表来求绿** —— 表里每条都写着「缺了会怎样」，\n\
             那几句是从重写前那份文档的实测病灶来的（`backend` 1 次 / 「轮询」0 次）。\n\
             要么把事实写回文档，要么先证明那条事实已经不成立了。"
        );
    }

    /// ★★ **反向断言：把承载事实的那一节剥掉，上面那条必须失守。**
    ///
    /// ⚠ **订正（F19 实施中被自己的变异逮到）**：本条头注原写「没有这一条，
    /// 存在钉就可能是恒真的」—— **那句是误导**。M3 变异（把四条事实的探针全换成泛词
    /// `backend` / `事件` / `crate`）时**本条照绿**，因为那些泛词**也**主要长在那一节里。
    ///
    /// ⇒ 本条真正测的是「**事实长没长在该长的那一节**」（剥了节就失守），
    /// **不测「探针够不够有辨别力」** —— 后者由
    /// `no_fact_may_rest_on_a_probe_that_is_everywhere` 管。两件事，别混说。
    #[test]
    fn the_facts_really_live_in_the_sections_that_are_supposed_to_carry_them() {
        let src = arch_doc();
        assert!(uncovered(&src).is_empty(), "基线就没绿，先修上一条");

        // 层边界那一节承载「backend 边界 / 零轮询 / 搬不动的边界 / 不进 workspace」四条。
        let no_layers = without_section(&src, "## 2. ");
        let lost = uncovered(&no_layers);
        assert!(
            lost.len() >= 3,
            "剥掉「层边界」那一节之后，仍有 {} 条事实被判成「覆盖了」（只丢了 {lost:?}）——\n\
             说明探针命中的是别处的零碎词句，而不是那一节。这条判据是**恒真的**，重挑探针。",
            REQUIRED_FACTS.len() - lost.len()
        );

        // 数据流那一节承载「远端那条链」。
        let no_flow = without_section(&src, "## 1. ");
        assert!(
            uncovered(&no_flow)
                .iter()
                .any(|(f, _)| f.contains("远端那条数据流链")),
            "剥掉「数据流」那一节之后，「远端那条链」仍被判成覆盖了 —— \
             那条链的探针没长在它该长的地方"
        );
    }

    /// ★★ **探针必须有辨别力** —— M3 变异（探针换泛词）从这条判据的缺口里活着走了出去。
    ///
    /// # 三条量法，都是量出来的
    ///
    /// 实测今天全部探针在 `ARCHITECTURE.md` 里的频次：**每条事实都至少有一个「只出现 1 次」
    /// 的锚**（`backend = 读` / `零轮询` / `ssh_source.rs` / `exec` / `原生构建` 各 1 次），
    /// 而 M3 用的泛词是 `backend` **12** 次 · `事件` **8** 次 · `crate` **6** 次。
    /// 最高的现役探针是 `control/` **15** 次 —— 它靠同组的稀有锚兜着，本身不承重。
    ///
    /// ⇒ ① 每条事实**至少 3 个探针**（单探针的事实经不起一个措辞改动）；
    /// ② 每条至少有**一个频次 ≤ 2 的稀有锚**；③ 没有探针频次 > 20。
    #[test]
    fn no_fact_may_rest_on_a_probe_that_is_everywhere() {
        let src = arch_doc();
        let times = |probe: &str| src.matches(probe).count();
        for (fact, probes, why) in REQUIRED_FACTS {
            assert!(
                probes.len() >= 3,
                "事实「{fact}」只有 {} 个探针 —— 单探针撑不住一条结构性主张，\
                 一次措辞改动就能让它零命中地绿。它的份量见「缺了会怎样」：{why}",
                probes.len()
            );
            let rare = probes.iter().filter(|p| times(p) <= 2).count();
            assert!(
                rare >= 1,
                "事实「{fact}」的探针全是常见词（频次 {:?}）—— 没有稀有锚就是**恒真断言**：\
                 换掉整节内容它照样绿。实测每条现役事实都有一个频次 1 的锚。",
                probes.iter().map(|p| (*p, times(p))).collect::<Vec<_>>()
            );
            for p in probes.iter() {
                let n = times(p);
                assert!(
                    n <= 20,
                    "事实「{fact}」的探针 {p:?} 在文档里出现 {n} 次 —— 太常见，\
                     它承不了「这条事实讲过了」这个判断（今天最高的现役探针是 15 次）"
                );
            }
        }
    }

    /// ★ 形状钉之二：**State 注册表的摘要不许长回来。**
    ///
    /// 〔F19〕原 §3 有一张 7 行的 State 表，而 `doc/STATE-MATRIX.md` 里有**严格更全的同一张**
    /// （多三列 + 逐命令 consumer），且原文自己就写着「详细矩阵 → STATE-MATRIX.md」——
    /// **自己承认家在那边、又存了一份摘要**。⇒ 摘要删除、只留指针，本条钉住它不再长回来。
    ///
    /// ⚠ 钉的是**表的形状**（`Arc<X>` 行）而不是「不许提 State」：
    /// §3 现在仍然要讲「为什么漏一次 `app.manage()` 编译器抓不住」——那是架构性的。
    /// 实测：删表后 `Arc<` **0 行**，`manage` 只剩那句理由里的 **1 处**。
    #[test]
    fn the_state_matrix_summary_never_grows_back() {
        let src = arch_doc();
        let rows: Vec<&str> = src
            .lines()
            .filter(|l| l.starts_with('|') && l.contains("Arc<"))
            .collect();
        assert!(
            rows.is_empty(),
            "`doc/ARCHITECTURE.md` 里又出现了 {} 行 State 表（{rows:?}）——\
             那张表的唯一的家是 `doc/STATE-MATRIX.md`，这里只该有指针。",
            rows.len()
        );
        assert!(
            src.contains("STATE-MATRIX.md"),
            "指针也没了 —— 那比存一份副本更糟：撤 State 的人根本找不到那份强制 checklist"
        );
    }

    /// ★ 形状钉：**逐文件模块表不许长回来。**
    ///
    /// # 天花板是量出来的，不是猜的
    ///
    /// 实测：重写**前**全文 `.rs`/`.ts` 文件名提及 **117 处**（其中那张 122 行的
    /// 模块表独占 **82 处**、含 23 行 `├── ` 树枝）；重写**后 49 处**（§2 内只剩 13 处）。
    /// ⇒ 天花板 **70**：既给正常增长留了空档，又容不下一张重新长出来的清单。
    ///
    /// ⚠ 这里**不禁**文件名 —— §5「关键设计选择」必须指名道姓才讲得清「为什么不能用别的方案」。
    /// 禁的是**规模**：一份顶层文档不该靠列文件来解释自己。
    #[test]
    fn the_per_file_module_tree_never_grows_back() {
        let src = arch_doc();
        let mut mentions = 0usize;
        for tok in
            src.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '/' | '-' | '.')))
        {
            if tok.len() > 3 && (tok.ends_with(".rs") || tok.ends_with(".ts")) {
                mentions += 1;
            }
        }
        assert!(
            mentions <= 70,
            "`doc/ARCHITECTURE.md` 里的文件名提及涨到 {mentions} 处（天花板 70，重写后实测 49）——\n\
             逐文件清单正在长回来。它的家是 `src-tauri/README.md` / `src/README.md` / \n\
             `backend/mod.rs` 的 `BACKEND_FILES`，顶层文档只该指过去。"
        );
        assert!(
            mentions >= 20,
            "只数出 {mentions} 处文件名 —— 抽取器多半坏了（重写后实测 49 处）。\
             返回一个小得离谱的数时，本条会零命中地绿，所以它必须红。"
        );
        let branches = src.matches("\u{251c}\u{2500}\u{2500} ").count();
        assert_eq!(
            branches, 0,
            "出现了 {branches} 行目录树树枝 —— 重写前那张模块表有 23 行。\
             §1 的 ASCII 数据流图不用这种树枝，所以这条不会误伤它。"
        );
    }
}
