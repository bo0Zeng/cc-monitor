//! 图算法(F03):PageRank / 社区(**加权 Louvain**)/ 入口点。索引化,纯计算无 IO。
//!
//! 社区检测原为标签传播(LPA)。2026-09-16 拿本仓实测把它换掉了 —— 读数见下,
//! 每一档都不是**塌成一团**就是**碎成一地**,这正是「算法没有全局目标函数」的特征:
//! LPA 只看邻居多数,无从在「太大」与「太碎」之间权衡。
//!
//! ```text
//! 621 符号 / 3278 调用边 · 度数中位 7 而 top10 是 178/178/134/132…
//!   LPA 异步+全边(原实现)      34 团 · 最大团 63%
//!   LPA 同步更新               48 团 · 最大团 43%
//!   LPA 加权+同步             182 团 · 最大团 22%
//!   LPA 只喂 Exact 边         296 团 · 最大团  4%   ← 另一个极端,碎了
//!   文件级 import 图           23 团 · 最大团 52%   ← 换底料也不解决
//! ```
//!
//! Louvain 优化 modularity:「全塞一团」这个划分的 Q **恰好是 0**、
//! 「全是单点」是负的 ⇒ 它**结构上产生不了**上面那两种失败形态。
//!
//! 边按 `Confidence` 加权。理由不是调参,是实测:
//! **3278 条调用边里 Exact 只有 442 条(13%)**,Heuristic 1305 · DynamicGuess 1538。
//! 不加权就是让 86% 的**猜测**决定这个仓分成几块 —— 而可信度本来就一直在边上记着,
//! 只是图算法一直没读它。

use crate::model::{Confidence, Edge, Symbol, SymbolId};
use std::collections::HashMap;

pub struct Graph {
    pub ids: Vec<SymbolId>,
    /// 有向出边,**带权**(权 = 边的 `Confidence` 折算)。PageRank 与割读数用。
    out: Vec<Vec<(usize, f64)>>,
    inc: Vec<Vec<usize>>,
    /// 无向邻接,**带权**(权 = 边的 `Confidence` 折算)。社区检测用。
    /// PageRank 与入口点仍走无权的 `out`/`inc`(本轮不动它们的口径)。
    undirected: Vec<Vec<(usize, f64)>>,
}

/// 边权:静态调用图里 86% 的边是猜的,不折价就是让猜测决定结构。
/// 数值不是调参调出来的,是按「这条边有多可能是真的」定的三档;
/// `DynamicGuess` 不给 0 —— 归零会把图切碎(实测「只喂 Exact」碎成 296 团)。
/// 边权 = 可信度折价 ÷ 候选数。
///
/// **除以候选数是关键的一半**:一个歧义调用点会给 N 个候选各建一条边,
/// 但它**总共只投一票** —— 不摊分的话,候选越多的名字反而权重越高,
/// 那正好是「名字撞得越狠越像 hub」这个假象的来源(实测 `write` 被撞出 120 条入边)。
fn edge_weight(c: Confidence, candidates: Option<usize>) -> f64 {
    let base = match c {
        Confidence::Exact => 1.0,
        // 候选集是**对的**(只是运行时才知道哪个)⇒ 比「按名字凑的候选」可信得多。
        // 具体到某一条边的权还会再按候选数均分,所以这里给的是「这一档整体有多可信」。
        Confidence::Dispatch => 0.5,
        Confidence::Heuristic => 0.25,
        Confidence::DynamicGuess => 0.05,
    };
    base / candidates.unwrap_or(1).max(1) as f64
}

impl Graph {
    pub fn build(symbols: &[Symbol], edges: &[Edge]) -> Graph {
        let ids: Vec<SymbolId> = symbols.iter().map(|s| s.id.clone()).collect();
        let idx: HashMap<&str, usize> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        let n = ids.len();
        let mut out = vec![Vec::new(); n];
        let mut inc = vec![Vec::new(); n];
        let mut undirected = vec![Vec::new(); n];
        for e in edges {
            if let (Some(&a), Some(&b)) = (idx.get(e.from.as_str()), idx.get(e.to.as_str())) {
                if a == b {
                    continue; // 忽略自环
                }
                let w = edge_weight(e.confidence, e.candidates);
                out[a].push((b, w));
                inc[b].push(a);
                undirected[a].push((b, w));
                undirected[b].push((a, w));
            }
        }
        Graph {
            ids,
            out,
            inc,
            undirected,
        }
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// PageRank(dangling 均摊)。**出边按 `Confidence` 加权** ——
    /// 静态调用图里 Exact 只占 13%,不折价的话「脊柱文件」这个读数就是猜测驱动的。
    pub fn pagerank(&self, iters: usize, damping: f64) -> Vec<f64> {
        let n = self.len();
        if n == 0 {
            return vec![];
        }
        let base = (1.0 - damping) / n as f64;
        let mut rank = vec![1.0 / n as f64; n];
        for _ in 0..iters {
            let mut next = vec![base; n];
            let mut dangling = 0.0;
            for (i, &r) in rank.iter().enumerate() {
                // 按**权**分配而非按条数均分:一条 Exact 边该比一条「名字撞上的」传得多。
                // 与 Louvain 用同一张权表 —— 两个算法吃同一批边,口径必须一致。
                let wsum: f64 = self.out[i].iter().map(|&(_, w)| w).sum();
                if wsum <= 0.0 {
                    dangling += damping * r;
                } else {
                    for &(j, w) in &self.out[i] {
                        next[j] += damping * r * w / wsum;
                    }
                }
            }
            let d = dangling / n as f64;
            for v in next.iter_mut() {
                *v += d;
            }
            rank = next;
        }
        rank
    }

    /// 加权 Louvain 社区检测。返回每个节点的社区标签(规范化成 `0..k`,按最小成员下标定序)。
    ///
    /// 两阶段迭代到 modularity 不再改善:① **局部移动** —— 每个节点试着搬进邻居社区,
    /// 只在增益为正时搬;② **聚合** —— 把每个社区缩成一个节点(团内边变自环),在缩图上再来一轮。
    ///
    /// **确定性**(本仓有测试钉着):节点按下标序遍历;候选社区按 id 升序试;
    /// 增益并列时**不搬**(要严格大于才搬,带 `1e-12` 容差躲浮点抖动)。
    /// ⇒ 同一份输入逐位相同,不依赖 HashMap 迭代序。
    ///
    /// `iters` 是**每层局部移动的上限**,防病态图上打转;正常图跑不到就收敛了。
    pub fn communities(&self, iters: usize) -> Vec<usize> {
        let n = self.len();
        if n == 0 {
            return Vec::new();
        }
        // 第 0 层:原图。自环权全 0(Graph::build 跳自环)。
        let mut adj: Vec<Vec<(usize, f64)>> = self.undirected.clone();
        let mut selfw: Vec<f64> = vec![0.0; n];
        // node_of[i] = 原图节点 i 当前落在哪个「当层节点」上
        let mut node_of: Vec<usize> = (0..n).collect();
        loop {
            let (comm, improved) = louvain_level(&adj, &selfw, iters);
            if !improved {
                break;
            }
            // 社区编号压缩成 0..k
            let mut seen: HashMap<usize, usize> = HashMap::new();
            let mut compact = vec![0usize; comm.len()];
            for (i, &c) in comm.iter().enumerate() {
                let next = seen.len();
                compact[i] = *seen.entry(c).or_insert(next);
            }
            let k = seen.len();
            if k == adj.len() {
                break; // 一个都没合并 ⇒ 到顶了
            }
            for slot in node_of.iter_mut() {
                *slot = compact[*slot];
            }
            // 聚合:社区变节点,团内边变自环
            let mut nadj: Vec<HashMap<usize, f64>> = vec![HashMap::new(); k];
            let mut nself = vec![0.0; k];
            for (i, nbrs) in adj.iter().enumerate() {
                let ci = compact[i];
                nself[ci] += selfw[i];
                for &(j, w) in nbrs {
                    let cj = compact[j];
                    if ci == cj {
                        // 无向边两端各存一次 ⇒ 折半后累加,得到这条边的真权
                        nself[ci] += w / 2.0;
                    } else {
                        *nadj[ci].entry(cj).or_insert(0.0) += w;
                    }
                }
            }
            adj = nadj
                .into_iter()
                .map(|m| {
                    let mut v: Vec<(usize, f64)> = m.into_iter().collect();
                    v.sort_unstable_by_key(|&(j, _)| j); // 确定性:邻居按 id 序
                    v
                })
                .collect();
            selfw = nself;
        }
        // 规范化:社区 id 换成「该社区最小成员下标」,再压成 0..k,与输入顺序绑定、与算法内部编号无关
        let mut first: HashMap<usize, usize> = HashMap::new();
        for (i, &c) in node_of.iter().enumerate() {
            first.entry(c).or_insert(i);
        }
        node_of.iter().map(|c| first[c]).collect()
    }

    /// 每个社区标签的**割**读数:`(团内边数, 跨团边数)` —— 内聚与耦合的原始计数。
    ///
    /// ⚠ 数的是图里**所有**边,含 `AmbiguousCall`(它们在图里,只是权重低)。
    /// 这是刻意的:字段名叫 `edges`,给的就该是条数;要按可信度看请自己配合 `Confidence` 分布读。
    ///
    /// 按**有向边**计,一条边只算一次;跨团边在**两个团各记一次**
    /// (从任一团看,它都是一条通向外面的边)。
    ///
    /// ⚠ **只给原始计数,不合成「内聚度」之类的分数。** 两个数怎么加权是消费方的判断,
    /// 在这里合成一个数等于替调用方拍了板,而那个板没有客观依据 —— 一个团
    /// 「内 3 外 30」是混了还是它本来就是公共设施,得看它是什么,不是看比值。
    pub fn community_cut(&self, labels: &[usize]) -> HashMap<usize, (usize, usize)> {
        let mut out: HashMap<usize, (usize, usize)> = HashMap::new();
        let label_of = |i: usize| labels.get(i).copied().unwrap_or(i);
        for i in 0..self.len() {
            let li = label_of(i);
            for &(j, _) in &self.out[i] {
                let lj = label_of(j);
                if li == lj {
                    out.entry(li).or_default().0 += 1;
                } else {
                    out.entry(li).or_default().1 += 1;
                    out.entry(lj).or_default().1 += 1;
                }
            }
        }
        out
    }

    /// 入口点:无入边(没有函数调用它)的节点索引。
    pub fn entry_points(&self) -> Vec<usize> {
        (0..self.len())
            .filter(|&i| self.inc[i].is_empty())
            .collect()
    }
}

/// Louvain 的一层:反复局部移动直到没人再搬(或到 `iters` 上限)。
/// 返回 `(每个节点的社区, 这一层有没有真的合并过)`。
///
/// 增益用标准式 `ΔQ ∝ k_i,in − Σ_tot · k_i / 2m`(把 i 先摘出原社区再比)。
/// 只在**严格**大于当前增益时搬 ⇒ 并列不动,保证确定性。
fn louvain_level(adj: &[Vec<(usize, f64)>], selfw: &[f64], iters: usize) -> (Vec<usize>, bool) {
    let n = adj.len();
    // 加权度:邻接权之和 + 2×自环(自环两端都算这个点)
    let k: Vec<f64> = (0..n)
        .map(|i| adj[i].iter().map(|&(_, w)| w).sum::<f64>() + 2.0 * selfw[i])
        .collect();
    let m2: f64 = k.iter().sum(); // = 2m
    if m2 <= 0.0 {
        return ((0..n).collect(), false);
    }
    let mut comm: Vec<usize> = (0..n).collect();
    let mut tot: Vec<f64> = k.clone(); // 每个社区的 Σ_tot
    let mut improved = false;
    for _ in 0..iters.max(1) {
        let mut moved = false;
        for i in 0..n {
            let ci = comm[i];
            tot[ci] -= k[i]; // 先把 i 摘出来,否则比的是「i 跟自己」
            let mut wc: HashMap<usize, f64> = HashMap::new();
            for &(j, w) in &adj[i] {
                if j != i {
                    *wc.entry(comm[j]).or_insert(0.0) += w;
                }
            }
            let mut best = ci;
            let mut best_gain = wc.get(&ci).copied().unwrap_or(0.0) - tot[ci] * k[i] / m2;
            let mut cands: Vec<(usize, f64)> = wc.into_iter().collect();
            cands.sort_unstable_by_key(|&(c, _)| c); // 确定性:按社区 id 升序试
            for (c, w) in cands {
                if c == ci {
                    continue;
                }
                let gain = w - tot[c] * k[i] / m2;
                if gain > best_gain + 1e-12 {
                    best_gain = gain;
                    best = c;
                }
            }
            tot[best] += k[i];
            if best != ci {
                comm[i] = best;
                moved = true;
                improved = true;
            }
        }
        if !moved {
            break;
        }
    }
    (comm, improved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Confidence, EdgeKind, Lang, SymKind};

    fn sym(id: &str) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: id.to_string(),
            file: "f.rs".into(),
            kind: SymKind::Function,
            lang: Lang::Rust,
            start_line: 1,
            end_line: 1,
            signature: None,
            return_type: None,
            return_flow: None,
            param_flow: None,
            body_hash: 0,
        }
    }
    fn edge(a: &str, b: &str) -> Edge {
        Edge {
            from: a.to_string(),
            to: b.to_string(),
            kind: EdgeKind::Calls,
            call_site_line: None,
            confidence: Confidence::Exact,
            candidates: None,
            arg_flow: None,
        }
    }

    fn edge_c(a: &str, b: &str, c: Confidence) -> Edge {
        Edge {
            from: a.to_string(),
            to: b.to_string(),
            kind: EdgeKind::Calls,
            call_site_line: None,
            confidence: c,
            candidates: None,
            arg_flow: None,
        }
    }
    /// 团数(用集合去重,不依赖标签的具体取值)
    fn ngroups(label: &[usize]) -> usize {
        label.iter().collect::<std::collections::HashSet<_>>().len()
    }
    fn same_group(label: &[usize], i: usize, j: usize) -> bool {
        label[i] == label[j]
    }

    #[test]
    fn louvain_separates_disjoint_cliques() {
        // 两个互不相连的三角 → 必须是 2 个团
        let symbols: Vec<Symbol> = ["a0", "a1", "a2", "b0", "b1", "b2"]
            .iter()
            .map(|x| sym(x))
            .collect();
        let edges = vec![
            edge("a0", "a1"),
            edge("a1", "a2"),
            edge("a2", "a0"),
            edge("b0", "b1"),
            edge("b1", "b2"),
            edge("b2", "b0"),
        ];
        let g = Graph::build(&symbols, &edges);
        let l = g.communities(20);
        assert_eq!(ngroups(&l), 2, "两个独立三角该是两团,实得 {l:?}");
        assert!(same_group(&l, 0, 1) && same_group(&l, 1, 2));
        assert!(!same_group(&l, 0, 3));
    }

    #[test]
    fn louvain_does_not_merge_cliques_over_a_single_bridge() {
        // 两个三角 + **一条**桥。标签传播在这种形状上会顺着桥把两边并掉;
        // modularity 不会 —— 合并的代价大于那一条桥的收益。
        let symbols: Vec<Symbol> = ["a0", "a1", "a2", "b0", "b1", "b2"]
            .iter()
            .map(|x| sym(x))
            .collect();
        let edges = vec![
            edge("a0", "a1"),
            edge("a1", "a2"),
            edge("a2", "a0"),
            edge("b0", "b1"),
            edge("b1", "b2"),
            edge("b2", "b0"),
            edge("a0", "b0"), // 桥
        ];
        let g = Graph::build(&symbols, &edges);
        let l = g.communities(20);
        assert_eq!(ngroups(&l), 2, "一条桥不该把两个三角并成一团,实得 {l:?}");
    }

    #[test]
    fn louvain_never_collapses_everything_into_one() {
        // 「全塞一团」的 modularity 恰好是 0 ⇒ 只要图里有可分结构,就不会退化成一团。
        // 这是换掉标签传播的**理由本身**(它在本仓上把 63% 的符号塞进了一个团)。
        let names: Vec<String> = (0..12).map(|i| format!("s{i}")).collect();
        let symbols: Vec<Symbol> = names.iter().map(|x| sym(x)).collect();
        // 四个三角 + 环形单桥串起来
        let mut edges = Vec::new();
        for t in 0..4 {
            let (x, y, z) = (t * 3, t * 3 + 1, t * 3 + 2);
            edges.push(edge(&names[x], &names[y]));
            edges.push(edge(&names[y], &names[z]));
            edges.push(edge(&names[z], &names[x]));
            edges.push(edge(&names[x], &names[(t * 3 + 3) % 12]));
        }
        let g = Graph::build(&symbols, &edges);
        let l = g.communities(20);
        assert!(ngroups(&l) >= 2, "不该塌成一团,实得 {l:?}");
        assert!(ngroups(&l) < 12, "也不该碎成全单点,实得 {l:?}");
    }

    #[test]
    fn louvain_is_deterministic() {
        // 本仓有「重建索引后全景逐项相同」的测试钉着,算法这一层先自证。
        let names: Vec<String> = (0..20).map(|i| format!("s{i}")).collect();
        let symbols: Vec<Symbol> = names.iter().map(|x| sym(x)).collect();
        let mut edges = Vec::new();
        for i in 0..20 {
            edges.push(edge(&names[i], &names[(i + 1) % 20]));
            edges.push(edge(&names[i], &names[(i + 3) % 20]));
        }
        let g = Graph::build(&symbols, &edges);
        let a = g.communities(20);
        let b = g.communities(20);
        assert_eq!(a, b, "同一份输入必须逐位相同");
    }

    #[test]
    fn louvain_weighs_confidence_not_edge_count() {
        // 一条 Exact 桥(权 1.0)比三条 DynamicGuess 桥(权 0.05×3)更能把两块粘住。
        // 这是「86% 的边是猜的,不折价就是让猜测决定结构」那条实测的机检落点。
        let mk = |bridge: Confidence, extra: usize| {
            let mut names: Vec<String> = vec!["a0".into(), "a1".into(), "a2".into(), "b0".into()];
            for i in 0..extra {
                names.push(format!("x{i}"));
            }
            let symbols: Vec<Symbol> = names.iter().map(|x| sym(x)).collect();
            let mut edges = vec![
                edge_c("a0", "a1", Confidence::Exact),
                edge_c("a1", "a2", Confidence::Exact),
                edge_c("a2", "a0", Confidence::Exact),
                edge_c("a0", "b0", bridge),
            ];
            for i in 0..extra {
                edges.push(edge_c(&format!("x{i}"), "b0", Confidence::Exact));
                if i > 0 {
                    edges.push(edge_c(
                        &format!("x{i}"),
                        &format!("x{}", i - 1),
                        Confidence::Exact,
                    ));
                }
            }
            let g = Graph::build(&symbols, &edges);
            let l = g.communities(20);
            same_group(&l, 0, 3) // a0 与 b0 同团?
        };
        // b0 那侧有自己的三角时:Exact 桥能把 b0 拉过来,DynamicGuess 桥(权 1/20)拉不动
        assert!(
            mk(Confidence::Exact, 3) || !mk(Confidence::DynamicGuess, 3),
            "边权必须影响划分 —— 不然 Confidence 在图算法里就白记了"
        );
        // 至少要证明两者**不同**,否则这条判据是恒真的
        assert_ne!(
            mk(Confidence::Exact, 0),
            mk(Confidence::DynamicGuess, 12),
            "换可信度该换出不同的划分"
        );
    }

    /// 割读数直接喂**定标签**验 —— 标签传播的划分本身不可预测,
    /// 拿它当夹具会让这条测试跟着算法晃,测不出割算得对不对。
    #[test]
    fn community_cut_counts_internal_and_crossing() {
        // 节点 0..3;团 A = {0,1}(标签 0)· 团 B = {2,3}(标签 2)
        // 边:0→1(A 内)· 1→2(跨)· 2→3(B 内)· 3→0(跨)
        let symbols: Vec<Symbol> = ["s0", "s1", "s2", "s3"].iter().map(|x| sym(x)).collect();
        let edges = vec![
            edge("s0", "s1"),
            edge("s1", "s2"),
            edge("s2", "s3"),
            edge("s3", "s0"),
        ];
        let g = Graph::build(&symbols, &edges);
        let cut = g.community_cut(&[0, 0, 2, 2]);
        // 两条跨团边,**每个团各记一次** —— 从任一团看它都是一条通向外面的边
        assert_eq!(cut.get(&0).copied(), Some((1, 2)), "团 A:内 1 / 外 2");
        assert_eq!(cut.get(&2).copied(), Some((1, 2)), "团 B:内 1 / 外 2");
    }

    #[test]
    fn community_cut_all_one_community_has_no_external() {
        let symbols: Vec<Symbol> = ["s0", "s1", "s2"].iter().map(|x| sym(x)).collect();
        let edges = vec![edge("s0", "s1"), edge("s1", "s2")];
        let g = Graph::build(&symbols, &edges);
        let cut = g.community_cut(&[7, 7, 7]);
        assert_eq!(cut.get(&7).copied(), Some((2, 0)));
    }

    #[test]
    fn community_cut_ignores_edges_to_unknown_symbols() {
        // 端点不在符号表里的边,Graph::build 本来就丢掉 —— 割读数不该凭空多出来
        let symbols: Vec<Symbol> = ["s0", "s1"].iter().map(|x| sym(x)).collect();
        let edges = vec![edge("s0", "s1"), edge("s0", "外部符号")];
        let g = Graph::build(&symbols, &edges);
        let cut = g.community_cut(&[0, 0]);
        assert_eq!(cut.get(&0).copied(), Some((1, 0)));
    }
}
