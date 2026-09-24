//! 全景计算:PageRank 脊柱 · 加权 Louvain 子系统 · 入口点,按 token 预算裁剪。
//!
//! 贵,故整份缓存(`overview_cache`)。
//! 🔴 喂给图算法的边**按 `Confidence` 加权**,PageRank 与 Louvain 用**同一张权表** ——
//! 静态调用图里 Exact 只占一成多,不折价就是让猜测决定「这仓分成几块」。
//! `Imports` 边不进图算法(文件级端点,与符号图不同构)。

use super::{fnv1a, Engine};
use crate::model::{Edge, EdgeKind, Overview, RankedFile, Subsystem, SymbolId, TokenBudget};
use crate::rank;
use std::collections::{HashMap, HashSet};

impl Engine {
    /// 项目全景概览:PageRank 脊柱文件 + 社区子系统 + 入口点,按 token 预算裁剪。
    /// 项目全景(F03);F17:缓存**未裁剪**的全景(budget 无关),裁剪很廉价、每次对 clone 施加。
    pub fn overview(&self, budget: TokenBudget) -> Overview {
        let cached = self.overview_cache.borrow().clone();
        let mut ov = match cached {
            Some(raw) => raw,
            None => {
                let raw = self.compute_overview_raw();
                *self.overview_cache.borrow_mut() = Some(raw.clone());
                raw
            }
        };
        trim_to_budget(&mut ov, budget);
        ov
    }

    /// 全量计算未裁剪的全景(PageRank + 社区 + 脊柱/入口);贵,故缓存。
    fn compute_overview_raw(&self) -> Overview {
        let symbols = self.or_empty("all_symbols", self.idx.all_symbols());
        // 图算法吃 `Calls` + `AmbiguousCall`,**不吃 `Imports`**(文件级端点,与符号图不同构)。
        //
        // 为什么歧义边要进来:全砍掉只剩 442 条 Exact 边,图会碎(实测 296 团,全是单点对子)——
        // 「分不清是哪个」不等于「没有调用」,那条调用真实存在,只是落点不确定。
        // 但它**必须折价**:`Graph::build` 按 `Confidence` 给权(Exact 1.0 / Heuristic 0.25 /
        // DynamicGuess 0.05),且 PageRank 与 Louvain **用同一套权**——
        // 先前 Louvain 折价而 PageRank 不折,同一批边两套口径,脊柱排名仍是 86% 猜测驱动的。
        let edges: Vec<Edge> = self
            .or_empty("all_edges", self.idx.all_edges())
            .into_iter()
            .filter(|e| e.kind != EdgeKind::Imports)
            .collect();
        let g = rank::Graph::build(&symbols, &edges);

        let total_symbols = symbols.len();
        let total_files: usize = {
            let mut set = HashSet::new();
            for s in &symbols {
                set.insert(s.file.as_str());
            }
            set.len()
        };
        // F18 覆盖信号(上次全量 index 时统计;未索引 → 0)
        let meta_usize = |k: &str| -> usize {
            self.idx
                .get_meta(k)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
        };
        let unresolved_calls = meta_usize("unresolved_calls");
        let ambiguous_calls = meta_usize("ambiguous_calls");
        let unresolved_imports = meta_usize("unresolved_imports");
        let parse_errors = meta_usize("parse_errors");
        if g.is_empty() {
            return Overview {
                total_symbols,
                total_files,
                unresolved_calls,
                ambiguous_calls,
                unresolved_imports,
                parse_errors,
                ..Default::default()
            };
        }

        let pr = g.pagerank(30, 0.85);
        let comm = g.communities(10);
        let entries = g.entry_points();
        let id_file: HashMap<&str, &str> = symbols
            .iter()
            .map(|s| (s.id.as_str(), s.file.as_str()))
            .collect();

        let spine_files = compute_spine(&g.ids, &pr, &id_file);
        let cut = g.community_cut(&comm);
        let (subsystems, _) = compute_subsystems(&g.ids, &comm, &pr, &cut, &id_file);
        let entry_points = compute_entries(&g.ids, &pr, &entries);

        Overview {
            spine_files,
            subsystems,
            entry_points,
            total_symbols,
            total_files,
            unresolved_calls,
            ambiguous_calls,
            unresolved_imports,
            parse_errors,
            // 建这份全景的过程中读库出过错 ⇒ 结果不完整,必须说出来
            db_errors: self.db_errors(),
        }
    }
}

impl Engine {
    /// **架构图**:子系统当节点,跨子系统的调用当边。
    ///
    /// 它只是把 `overview` 已经算出来的事实换个形状 —— 不多算任何东西。
    /// 诚实信号(看不见多少 / 分不清多少 / 读库有没有出错)一并带出去,
    /// 否则一张图看起来永远是完整的。
    pub fn arch_graph(
        &self,
        budget: crate::model::TokenBudget,
        opts: crate::diagram::DrawOpts,
    ) -> crate::diagram::ArchGraph {
        let ov = self.overview(budget);
        let symbols = self.or_empty("all_symbols", self.idx.all_symbols());
        let edges: Vec<Edge> = self
            .or_empty("all_edges", self.idx.all_edges())
            .into_iter()
            .filter(|e| e.kind != EdgeKind::Imports)
            .collect();
        // ⚠ 重算一遍社区(而不是从 `Overview` 反推):`Subsystem` 只带文件不带成员,
        // 而按文件反推是**错的**(见 `compute_subsystems` 里那段注释)。
        // 这是显式、偶发的调用,重算这一次的代价可以接受。
        let g = rank::Graph::build(&symbols, &edges);
        let membership = if g.is_empty() {
            HashMap::new()
        } else {
            let pr = g.pagerank(30, 0.85);
            let comm = g.communities(10);
            let cut = g.community_cut(&comm);
            let id_file: HashMap<&str, &str> = symbols
                .iter()
                .map(|s| (s.id.as_str(), s.file.as_str()))
                .collect();
            compute_subsystems(&g.ids, &comm, &pr, &cut, &id_file).1
        };
        crate::diagram::arch_graph(&ov, &edges, &membership, opts)
    }

    /// **模块依赖图**:按目录聚合。与 `arch_graph` 画的不是同一件事 ——
    /// 模块图是「你声明的结构」,社区图是「实际存在的耦合」,不一致处最值得看。
    pub fn module_graph(&self, opts: crate::diagram::DrawOpts) -> crate::diagram::ArchGraph {
        let symbols = self.or_empty("all_symbols", self.idx.all_symbols());
        let edges = self.or_empty("all_edges", self.idx.all_edges());
        let mut g = crate::diagram::module_graph(&symbols, &edges, opts);
        // 诚实信号与 `arch_graph` 同源
        let meta = |k: &str| -> usize {
            self.idx
                .get_meta(k)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
        };
        g.unresolved_calls = meta("unresolved_calls");
        g.ambiguous_calls = meta("ambiguous_calls");
        g.db_errors = self.db_errors();
        g
    }

    /// **UML 类图**:类型 · 实现/继承 · 组合。关系来自**声明**,不是推断。
    pub fn uml(&self, max_types: usize) -> String {
        let symbols = self.or_empty("all_symbols", self.idx.all_symbols());
        let impls = self.or_empty("all_impls", self.idx.all_impls());
        let fields = self.or_empty("all_field_types", self.idx.all_field_types());
        crate::diagram::uml_mermaid(&symbols, &impls, &fields, max_types.max(1))
    }
}

/// 脊柱文件:按文件聚合 PageRank,分数降序(并列文件名升序)。
fn compute_spine(ids: &[SymbolId], pr: &[f64], id_file: &HashMap<&str, &str>) -> Vec<RankedFile> {
    let mut file_score: HashMap<&str, (f64, usize)> = HashMap::new();
    for (i, id) in ids.iter().enumerate() {
        if let Some(&file) = id_file.get(id.as_str()) {
            let e = file_score.entry(file).or_insert((0.0, 0));
            e.0 += pr[i];
            e.1 += 1;
        }
    }
    let mut spine: Vec<RankedFile> = file_score
        .into_iter()
        .map(|(file, (score, symbols))| RankedFile {
            file: file.to_string(),
            score,
            symbols,
        })
        .collect();
    spine.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.file.cmp(&b.file))
    });
    spine
}

/// 子系统:按社区标签聚合。
/// `label` = 成员最多的文件(给人读);`anchors` = PageRank 前 3 的成员(配对锚 + 好命名);
/// `member_hash` = 成员集合指纹(判「变没变」,**不是**身份);`internal/external_edges` = 割读数。
/// 排序:size 降序 → label 升序 → member_hash 升序(最后一项保证全序,不留并列靠 HashMap 序)。
fn compute_subsystems(
    ids: &[SymbolId],
    comm: &[usize],
    pr: &[f64],
    cut: &HashMap<usize, (usize, usize)>,
    id_file: &HashMap<&str, &str>,
) -> (Vec<Subsystem>, HashMap<SymbolId, usize>) {
    let mut by_label: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, &l) in comm.iter().enumerate() {
        by_label.entry(l).or_default().push(i);
    }
    let mut subsystems: Vec<(usize, Subsystem)> = by_label
        .into_iter()
        .map(|(label_key, members)| {
            let mut fcount: HashMap<&str, usize> = HashMap::new();
            for &i in &members {
                if let Some(&file) = id_file.get(ids[i].as_str()) {
                    *fcount.entry(file).or_insert(0) += 1;
                }
            }
            // 频次最高的文件作 label;`b.0.cmp(a.0)` 反转次键 → 并列取最小文件名
            let label = fcount
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                .map(|(f, _)| f.to_string())
                .unwrap_or_default();
            let mut files: Vec<String> = fcount.keys().map(|s| s.to_string()).collect();
            files.sort();

            // 指纹:成员 id **升序**后拼接 → FNV-1a。
            // ⚠ 这个 sort 当下是**防御性的不是承重的** —— 成员按节点下标递增入队,
            // 而节点顺序来自 `all_symbols()` 的 `ORDER BY id`,所以现在本就有序。
            // 但那个有序性住在两跳以外(索引层的 SQL + 建图的 enumerate),
            // 任一处一改这里就静默出随机哈希 ⇒ 在这里自己排一次,不赌远处的不变量。
            // 用 FNV-1a 不用 DefaultHasher:后者**跨 Rust 版本不保证稳定**,
            // 而这个数存在的意义就是跨次、跨机器比对(见 `fnv1a` 自己的注释)。
            let mut sorted_ids: Vec<&str> = members.iter().map(|&i| ids[i].as_str()).collect();
            sorted_ids.sort_unstable();
            let member_hash = format!("{:016x}", fnv1a(sorted_ids.join("\n").as_bytes()));

            // 代表:PageRank 降序,并列 id 升序(不依赖 members 的入队顺序)
            let mut ranked: Vec<(&SymbolId, f64)> =
                members.iter().map(|&i| (&ids[i], pr[i])).collect();
            ranked.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.0.cmp(b.0))
            });
            let anchors: Vec<SymbolId> =
                ranked.iter().take(3).map(|(id, _)| (*id).clone()).collect();

            let (internal_edges, external_edges) = cut.get(&label_key).copied().unwrap_or((0, 0));
            (
                label_key,
                Subsystem {
                    label,
                    files,
                    size: members.len(),
                    member_hash,
                    anchors,
                    internal_edges,
                    external_edges,
                },
            )
        })
        .collect();
    subsystems.sort_by(|(_, a), (_, b)| {
        b.size
            .cmp(&a.size)
            .then(a.label.cmp(&b.label))
            .then(a.member_hash.cmp(&b.member_hash))
    });
    // 🔴 成员映射必须**按符号**建,而且要在排序**之后** ——
    // 下标是排序后的位置,与 `Overview.subsystems` 对齐。
    // 按文件映射是错的:一个文件里的符号完全可能分属不同子系统(社区检测是符号级的),
    // 单文件仓里所有团会塌成一个 ⇒ 跨团调用一条都画不出来。
    let pos: HashMap<usize, usize> = subsystems
        .iter()
        .enumerate()
        .map(|(i, (key, _))| (*key, i))
        .collect();
    let membership: HashMap<SymbolId, usize> = comm
        .iter()
        .enumerate()
        .filter_map(|(i, l)| Some((ids.get(i)?.clone(), *pos.get(l)?)))
        .collect();
    (subsystems.into_iter().map(|(_, s)| s).collect(), membership)
}

/// 入口点(无入边),按 PageRank 降序(并列 id 升序)。
fn compute_entries(ids: &[SymbolId], pr: &[f64], entries: &[usize]) -> Vec<SymbolId> {
    let mut scored: Vec<(SymbolId, f64)> =
        entries.iter().map(|&i| (ids[i].clone(), pr[i])).collect();
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    scored.into_iter().map(|(id, _)| id).collect()
}

// 各类条目的每项序列化开销估算(分隔符/字段名等),供 budget 裁剪用
const SPINE_ITEM_TOKENS: usize = 4;

const SUBSYS_ITEM_TOKENS: usize = 6;

const ENTRY_ITEM_TOKENS: usize = 2;

/// 按 token 预算裁剪 overview:脊柱 50% / 子系统 30% / 入口 20%,各自贪心保留高排名项。
fn trim_to_budget(ov: &mut Overview, budget: TokenBudget) {
    let b = budget.0;
    let spine_b = b / 2;
    let sub_b = b / 10 * 3; // 先除后乘,避免 b*3 溢出
    let entry_b = b.saturating_sub(spine_b + sub_b);
    truncate_by_tokens(&mut ov.spine_files, spine_b, |rf| {
        TokenBudget::est_tokens(&rf.file) + SPINE_ITEM_TOKENS
    });
    truncate_by_tokens(&mut ov.subsystems, sub_b, |s| {
        // 代表符号也会打印出来 —— 不算进估算,裁剪就会低估实际输出
        TokenBudget::est_tokens(&s.label)
            + s.anchors
                .iter()
                .map(|a| TokenBudget::est_tokens(a))
                .sum::<usize>()
            + TokenBudget::est_tokens(&s.member_hash)
            + SUBSYS_ITEM_TOKENS
    });
    truncate_by_tokens(&mut ov.entry_points, entry_b, |id| {
        TokenBudget::est_tokens(id) + ENTRY_ITEM_TOKENS
    });
}

fn truncate_by_tokens<T>(v: &mut Vec<T>, budget: usize, cost: impl Fn(&T) -> usize) {
    let mut used = 0usize;
    let mut keep = 0usize;
    for item in v.iter() {
        used += cost(item);
        if used > budget {
            break;
        }
        keep += 1;
    }
    v.truncate(keep);
}
