//! **画图**(2026-09-17):把已有的事实渲染成 Mermaid。
//!
//! 这一层**不算任何东西** —— 子系统是 `rank` 的加权 Louvain 算的,边与置信度是
//! `graph` 建的,它只负责画。所以它也不该有自己的"判断":
//!
//! 🔴 **不同可信度的边不许画成一样**。一张所有箭头长得一样的架构图,
//! 会让「确定的调用」和「按名字凑的候选」在读图的人眼里变成同一回事 ——
//! 那正是这一整轮在消灭的东西,不能在最后一步又造回来。
//!
//! | 线型 | 含义 |
//! |---|---|
//! | `A ==> B` 粗实线 | `Exact` —— 确定 |
//! | `A -.-> B` 虚线 | `Dispatch` —— 动态派发,候选集完整但运行时才知道是哪个 |
//! | `A -.-> B` 虚线 + `?` | 其余歧义 —— 候选是按名字凑的,可能整个都不对 |
//!
//! ## 分层(2026-09-24)
//!
//! | 子模块 | 管什么 |
//! |---|---|
//! | [`registry`] | **有哪些图**:`DiagramKind` 注册表 —— 唯一真相源,MCP 与第三方都从它派生 |
//! | [`shape`] | **一张图是什么**:统一的 `Diagram`(按形状分变体)+ 公共诚实信号 `Honesty` |
//! | 本文件 | 各形状的**构图**(`arch_graph` / `module_graph`)与 Mermaid 渲染 |
//! | [`uml`] | 类型关系图的构图与渲染 |
//!
//! 统一出口是 `Engine::draw(kind, &req)`(`engine/draw.rs`);Mermaid 只是渲染器之一
//! ([`to_mermaid`])。

pub mod registry;
pub mod shape;
pub mod uml;

pub use registry::{
    kinds, DiagramError, DiagramKind, DiagramKindInfo, DiagramParam, DiagramRequest, DiagramShape,
};
pub use shape::{to_mermaid, CallEdge, CallNode, Diagram, DiagramBody, Honesty, Omitted};
pub use uml::{uml_graph, uml_mermaid, TypeField, TypeMethod, TypeNode, TypeRelKind, TypeRelation};

use crate::model::{Confidence, Edge, EdgeKind, Overview, SubGraph, Subsystem};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 画图选项。
#[derive(Debug, Clone, Copy)]
pub struct DrawOpts {
    /// 最多画几个节点(按符号数降序留大的)。
    pub max_nodes: usize,
    /// **只画有确定成分的连接**。
    ///
    /// 🔴 一条**全靠名字凑出来**的连接不是依赖的证据 —— 它可能整条都是假的。
    /// 不滤的话图会变成毛线球:实测一个 TS 仓(未富化)14 个节点约 150 条连接、几乎全连通,
    /// 里面还有 `utils → pages`、`core → __tests__` 这种明显不成立的。
    /// ⚠ 滤掉多少**写进图注** —— 省略必须说出来。
    pub certain_only: bool,
    /// **把测试排除在外**(默认排除)。
    ///
    /// 🔴 架构图讲的是**产品的结构**,测试不是产品的结构。
    /// 不排的话测试会主导整张图 —— 实测 code-picture 自己的模块图里
    /// `code-picture-core/tests`(262 符号)是**最大的节点**;
    /// 更糟的是会造出「生产代码调测试」这种假边(测试辅助与生产函数重名撞出来的)。
    /// ⚠ 排掉多少**写进图注**。判据见 `crate::scan::is_test_file`。
    pub exclude_tests: bool,
}

impl Default for DrawOpts {
    fn default() -> Self {
        // 默认滤:一张能看的图比一张"什么都有"的图有用。滤掉多少会写在图注里。
        DrawOpts {
            max_nodes: 12,
            certain_only: true,
            exclude_tests: true,
        }
    }
}

/// 架构图:子系统当节点,跨子系统的调用当边。
#[derive(Debug, Clone, Default)]
pub struct ArchGraph {
    pub nodes: Vec<ArchNode>,
    pub links: Vec<ArchLink>,
    /// 诚实信号:这张图**建立在多少看不见的东西之上**。
    pub unresolved_calls: usize,
    pub ambiguous_calls: usize,
    /// 非空 = 读索引出过错,这张图不完整。
    pub db_errors: Vec<String>,
    /// 因为**全靠名字凑**而滤掉的连接数(`certain_only` 打开时)。
    pub filtered_guess_links: usize,
    /// 因为是**测试**而排除在外的符号数(`exclude_tests` 打开时)。
    pub excluded_test_symbols: usize,
    /// 因为太小而**没画**的团数 / 符号数 / 链接数。
    ///
    /// 🔴 必须报出来 —— 一张只画了大团的图,看起来和「这仓就这几块」长得一模一样。
    /// 省略是为了能看,不是为了好看,所以省了多少得写在图上。
    pub omitted: (usize, usize, usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ArchNode {
    /// Mermaid 里的 id。**从标签派生、人能读**(`lang_kotlin`)——
    /// `s0`/`s1` 那种不透明 id 逼着读图的人在边与节点定义之间来回翻。
    pub id: String,
    /// 代表文件(成员最多的那个)。
    pub label: String,
    pub size: usize,
    pub files: usize,
    /// 代表符号(给人一眼看懂这团在干嘛)。
    pub anchors: Vec<String>,
    /// **成员文件**(仓库相对,升序)。给消费方**下钻**用:点一个团/模块 → 列它有哪些文件。
    /// ⚠ 与 `files`(个数)同源,`member_files.len() == files`。
    pub member_files: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ArchLink {
    pub from: String,
    pub to: String,
    /// 确定的边条数。
    pub exact: usize,
    /// 动态派发(候选集完整,运行时才知道哪个)。
    pub dispatch: usize,
    /// 按名字凑的候选 —— `Heuristic` + `DynamicGuess`。
    pub guess: usize,
}

impl ArchLink {
    pub fn count(&self) -> usize {
        self.exact + self.dispatch + self.guess
    }
}

/// 把全景 + 全部边压成一张**子系统级**的架构图。
///
/// ⚠ 一个文件只归一个子系统(Louvain 的划分);跨子系统的调用聚合成一条链接。
/// ⚠ 子系统**内部**的调用不画 —— 那是下一层的事(`subgraph_mermaid`)。
/// `max_nodes` = 最多画几个团(按符号数降序留大的)。其余**不是丢掉,是记进 `omitted`**。
pub fn arch_graph(
    ov: &Overview,
    edges: &[Edge],
    membership: &HashMap<String, usize>,
    opts: DrawOpts,
) -> ArchGraph {
    let max_nodes = opts.max_nodes;
    // 按符号数降序留大的;并列时按下标,保确定性
    let mut order: Vec<usize> = (0..ov.subsystems.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(ov.subsystems[i].size), i));
    let kept: std::collections::HashSet<usize> = order.iter().take(max_nodes).copied().collect();
    let omitted_syms: usize = order
        .iter()
        .skip(max_nodes)
        .map(|&i| ov.subsystems[i].size)
        .sum();
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut ids: HashMap<usize, String> = HashMap::new();
    let nodes: Vec<ArchNode> = order
        .iter()
        .take(max_nodes)
        .map(|&i| {
            let s: &Subsystem = &ov.subsystems[i];
            let label = short_path(&s.label);
            let id = unique_id(&label, &mut used);
            ids.insert(i, id.clone());
            let mut member_files = s.files.clone();
            member_files.sort();
            member_files.dedup();
            ArchNode {
                id,
                label,
                size: s.size,
                files: member_files.len(),
                anchors: s.anchors.iter().map(|a| bare(a)).collect(),
                member_files,
            }
        })
        .collect();
    let node_id = |i: usize| ids.get(&i).cloned().unwrap_or_default();

    // 排掉的测试符号数(要印在图注里 —— 排除不许是静默的)
    let excluded_test_symbols = if opts.exclude_tests {
        membership
            .keys()
            .filter(|id| crate::scan::is_test_file(id))
            .count()
    } else {
        0
    };
    let mut omitted_links = 0usize;
    // 🔴 存**成分**而不是「最可信的那一档」。只存 best 的话,一捆 32 条边里混着
    // 猜出来的也会画成干净的粗实线 —— 聚合把不确定性藏起来,那正是这张图不该干的事。
    let mut agg: HashMap<(usize, usize), ArchLink> = HashMap::new();
    for e in edges {
        if e.kind == EdgeKind::Imports {
            continue; // 架构图画「谁调谁」,import 是另一张图
        }
        if opts.exclude_tests
            && (crate::scan::is_test_file(&e.from) || crate::scan::is_test_file(&e.to))
        {
            continue;
        }
        // 🔴 **按符号**查所属子系统,不能按文件:一个文件里的符号完全可能分属不同子系统
        // (社区检测是符号级的)。按文件查时单文件仓里所有团会塌成一个,一条跨团调用都画不出来。
        let (Some(&a), Some(&b)) = (membership.get(&e.from), membership.get(&e.to)) else {
            continue;
        };
        if a == b {
            continue; // 子系统内部的调用不画
        }
        if !kept.contains(&a) || !kept.contains(&b) {
            omitted_links += 1;
            continue;
        }
        let slot = agg.entry((a, b)).or_default();
        match e.confidence {
            Confidence::Exact => slot.exact += 1,
            Confidence::Dispatch => slot.dispatch += 1,
            Confidence::Heuristic | Confidence::DynamicGuess => slot.guess += 1,
        }
    }
    let mut filtered_guess_links = 0usize;
    let mut links: Vec<ArchLink> = agg
        .into_iter()
        .filter(|(_, l)| {
            // 全靠名字凑的连接不是依赖的证据 —— 滤掉,但记下滤了多少
            let keep = !opts.certain_only || l.exact > 0 || l.dispatch > 0;
            if !keep {
                filtered_guess_links += 1;
            }
            keep
        })
        .map(|((a, b), mut l)| {
            l.from = node_id(a);
            l.to = node_id(b);
            l
        })
        .collect();
    // 确定性:同一个仓两次画出同一张图
    links.sort_by(|x, y| (&x.from, &x.to).cmp(&(&y.from, &y.to)));

    let kept_n = nodes.len();
    ArchGraph {
        nodes,
        links,
        unresolved_calls: ov.unresolved_calls,
        ambiguous_calls: ov.ambiguous_calls,
        db_errors: ov.db_errors.clone(),
        filtered_guess_links,
        excluded_test_symbols,
        omitted: (
            ov.subsystems.len().saturating_sub(kept_n),
            omitted_syms,
            omitted_links,
        ),
    }
}

/// 标签 → **人能读的** Mermaid id。非字母数字换 `_`,重名加序号。
/// Mermaid 的 id 不能带 `/` `.` `-` 这些,但完全可以是有意义的词。
fn unique_id(label: &str, used: &mut std::collections::HashSet<String>) -> String {
    let base: String = label
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    let base = if base.is_empty() {
        "n".to_string()
    } else {
        base
    };
    let mut id = base.clone();
    let mut n = 2;
    while !used.insert(id.clone()) {
        id = format!("{base}_{n}");
        n += 1;
    }
    id
}

/// 路径缩到后两段:`crates/a/src/lang/kotlin.rs` → `lang/kotlin.rs`。
/// 全路径当标签会把图撑爆,而后两段在一个仓里基本够认。
fn short_path(p: &str) -> String {
    let segs: Vec<&str> = p.split('/').collect();
    segs[segs.len().saturating_sub(2)..].join("/")
}

/// 符号 id → 裸的限定名:`a/b.rs#Rust::ctrl_role@12` → `Rust::ctrl_role`。
fn bare(id: &str) -> String {
    let seg = id.rsplit('#').next().unwrap_or(id);
    seg.split('@').next().unwrap_or(seg).to_string()
}

/// 架构图 → Mermaid(连图注)。
pub fn arch_mermaid(g: &ArchGraph) -> String {
    let mut s = clusters_mermaid(&g.nodes, &g.links);
    s.push_str(&shape::honesty_legend(&Honesty::of_arch(g), Some(g)));
    s
}

/// 团/模块形状的**图体**(不含图注)。
pub(crate) fn clusters_mermaid(nodes: &[ArchNode], links: &[ArchLink]) -> String {
    let mut s = String::from("flowchart LR\n");
    for n in nodes {
        let anchors = if n.anchors.is_empty() {
            String::new()
        } else {
            format!("<br/><i>{}</i>", n.anchors.join(" · "))
        };
        s.push_str(&format!(
            "  {}[\"<b>{}</b><br/>{} 符号 / {} 文件{}\"]\n",
            n.id,
            escape(&n.label),
            n.size,
            n.files,
            escape(&anchors)
        ));
    }
    for l in links {
        let (arrow, tag) = link_style(l);
        s.push_str(&format!("  {} {}|\"{}\"| {}\n", l.from, arrow, tag, l.to));
    }
    s
}

/// 调用子图 → Mermaid(符号级)。
pub fn subgraph_mermaid(sg: &SubGraph) -> String {
    let mut s = String::from("flowchart LR\n");
    let mut id_of: HashMap<&str, String> = HashMap::new();
    for (i, sym) in sg.symbols.iter().enumerate() {
        let nid = format!("n{i}");
        s.push_str(&format!(
            "  {}[\"{}<br/><small>{}</small>\"]\n",
            nid,
            escape(&sym.name),
            escape(&sym.file)
        ));
        id_of.insert(sym.id.as_str(), nid);
    }
    for e in &sg.edges {
        let (Some(a), Some(b)) = (id_of.get(e.from.as_str()), id_of.get(e.to.as_str())) else {
            continue;
        };
        if e.kind == EdgeKind::Imports {
            continue;
        }
        let (arrow, tag) = edge_style(e.confidence);
        if tag.is_empty() {
            s.push_str(&format!("  {a} {arrow} {b}\n"));
        } else {
            s.push_str(&format!("  {a} {arrow}|\"{tag}\"| {b}\n"));
        }
    }
    s
}

/// **单条**边的线型 + 标签(符号级子图用 —— 那里没有聚合,一条就是一条)。
pub(crate) fn edge_style(c: Confidence) -> (&'static str, &'static str) {
    match c {
        Confidence::Exact => ("==>", ""),
        Confidence::Dispatch => ("-.->", "派发"),
        Confidence::Heuristic | Confidence::DynamicGuess => ("-.->", "分不清?"),
    }
}

/// 一捆**聚合**边的线型 + 标签。
///
/// 🔴 **混着的不许画成干净的粗实线**。一捆 32 条边里有 12 条是按名字凑的,
/// 画成 `==>|"32×"|` 就是把不确定性藏进了聚合 —— 读图的人会当成 32 条确定的调用。
/// 全确定才给 `==>`;一掺就降成细实线 `-->` 并在标签里**写出成分**。
fn link_style(l: &ArchLink) -> (&'static str, String) {
    match (l.exact, l.dispatch, l.guess) {
        (n, 0, 0) => ("==>", format!("{n}×")),
        (0, n, 0) => ("-.->", format!("{n}×派发")),
        (0, 0, n) => ("-.->", format!("{n}×分不清?")),
        (e, d, g) => {
            let mut parts = Vec::new();
            if e > 0 {
                parts.push(format!("{e} 确定"));
            }
            if d > 0 {
                parts.push(format!("{d} 派发"));
            }
            if g > 0 {
                parts.push(format!("{g} 分不清"));
            }
            // 细实线:比 `==>` 弱、比 `-.->` 强 —— 「有确定的,但不全是」
            ("-->", format!("{}×({})", l.count(), parts.join("+")))
        }
    }
}

/// Mermaid 标签里的危险字符。引号会把标签截断,方括号会被当成节点语法。
pub(crate) fn escape(t: &str) -> String {
    t.replace('"', "'").replace(['[', ']'], "")
}

// ── 模块依赖图:目录当节点 ──

/// **模块依赖图**:按目录聚合。
///
/// 🔴 这和架构图(`arch_graph`)画的**不是同一件事**,两张都要有:
/// * 模块图 = **你声明的结构**(目录怎么分的);
/// * 社区图 = **实际存在的耦合**(Louvain 按调用密度切出来的)。
///
/// 两者不一致的地方最值得看 —— 那是「目录这么分,但代码不这么走」。
/// 只画社区图的问题是:一个团叫 `tests/graph_test.rs` 却跨 23 个文件,人读不懂;
/// 只画模块图的问题是:它只能反映你已经知道的事。
pub fn module_graph(symbols: &[crate::model::Symbol], edges: &[Edge], opts: DrawOpts) -> ArchGraph {
    let max_nodes = opts.max_nodes;
    let mut dir_of: HashMap<&str, &str> = HashMap::new();
    let mut size: HashMap<&str, usize> = HashMap::new();
    let mut files: HashMap<&str, std::collections::HashSet<&str>> = HashMap::new();
    let mut excluded_test_symbols = 0usize;
    for s in symbols {
        if opts.exclude_tests && crate::scan::is_test_file(&s.file) {
            excluded_test_symbols += 1;
            continue;
        }
        let d = dir(&s.file);
        dir_of.insert(s.id.as_str(), d);
        *size.entry(d).or_insert(0) += 1;
        files.entry(d).or_default().insert(s.file.as_str());
    }
    let mut dirs: Vec<&str> = size.keys().copied().collect();
    dirs.sort_by_key(|d| (std::cmp::Reverse(size[d]), *d));
    let kept: Vec<&str> = dirs.iter().take(max_nodes).copied().collect();
    let keep: std::collections::HashSet<&str> = kept.iter().copied().collect();

    // 🔴 标签剥**公共前缀**,不能各取后两段:`crates/a/src/lang` 取后两段是 `src/lang`,
    // 而 `crates/a/tests` 取后两段是 `a/tests` —— 粒度不一致,读图的人分不清「谁的 src」。
    let prefix = common_prefix(&kept);
    let mut used = std::collections::HashSet::new();
    let mut ids: HashMap<&str, String> = HashMap::new();
    let nodes: Vec<ArchNode> = kept
        .iter()
        .map(|&d| {
            let label = d
                .strip_prefix(&prefix)
                .unwrap_or(d)
                .trim_start_matches('/')
                .to_string();
            let label = if label.is_empty() {
                d.to_string()
            } else {
                label
            };
            let id = unique_id(&label, &mut used);
            ids.insert(d, id.clone());
            let mut member_files: Vec<String> = files[d].iter().map(|f| f.to_string()).collect();
            member_files.sort();
            ArchNode {
                id,
                label,
                size: size[d],
                files: member_files.len(),
                anchors: Vec::new(), // 目录不需要代表符号,它的名字就是它自己
                member_files,
            }
        })
        .collect();

    let mut omitted_links = 0usize;
    let mut agg: HashMap<(&str, &str), ArchLink> = HashMap::new();
    for e in edges {
        if e.kind == EdgeKind::Imports {
            continue;
        }
        let (Some(&a), Some(&b)) = (dir_of.get(e.from.as_str()), dir_of.get(e.to.as_str())) else {
            continue;
        };
        if a == b {
            continue; // 模块内部的调用不画
        }
        if !keep.contains(a) || !keep.contains(b) {
            omitted_links += 1;
            continue;
        }
        let slot = agg.entry((a, b)).or_default();
        match e.confidence {
            Confidence::Exact => slot.exact += 1,
            Confidence::Dispatch => slot.dispatch += 1,
            Confidence::Heuristic | Confidence::DynamicGuess => slot.guess += 1,
        }
    }
    let mut filtered_guess_links = 0usize;
    let mut links: Vec<ArchLink> = agg
        .into_iter()
        .filter(|(_, l)| {
            let keep = !opts.certain_only || l.exact > 0 || l.dispatch > 0;
            if !keep {
                filtered_guess_links += 1;
            }
            keep
        })
        .map(|((a, b), mut l)| {
            l.from = ids[a].clone();
            l.to = ids[b].clone();
            l
        })
        .collect();
    links.sort_by(|x, y| (&x.from, &x.to).cmp(&(&y.from, &y.to)));
    let omitted_syms: usize = dirs.iter().skip(max_nodes).map(|d| size[d]).sum();
    ArchGraph {
        omitted: (
            dirs.len().saturating_sub(nodes.len()),
            omitted_syms,
            omitted_links,
        ),
        // ⚠ 别忘了这个:`..Default::default()` 会把没写的字段**静默清零**,
        //   而「滤掉了多少」是要印在图注里的诚实读数。
        filtered_guess_links,
        excluded_test_symbols,
        nodes,
        links,
        ..Default::default()
    }
}

/// 一组目录的**最长公共路径前缀**(按段,不按字符 —— 按字符会把 `src/la` 这种半截切出来)。
fn common_prefix(dirs: &[&str]) -> String {
    let Some(first) = dirs.first() else {
        return String::new();
    };
    let mut pre: Vec<&str> = first.split('/').collect();
    for d in dirs.iter().skip(1) {
        let segs: Vec<&str> = d.split('/').collect();
        let n = pre
            .iter()
            .zip(segs.iter())
            .take_while(|(a, b)| a == b)
            .count();
        pre.truncate(n);
        if pre.is_empty() {
            break;
        }
    }
    pre.join("/")
}

/// 文件的目录部分;顶层文件归 `.`。
fn dir(file: &str) -> &str {
    match file.rfind('/') {
        Some(i) => &file[..i],
        None => ".",
    }
}
