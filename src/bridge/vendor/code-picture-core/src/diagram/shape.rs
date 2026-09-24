//! **一张图是什么**(2026-09-24):统一的 [`Diagram`] + 公共诚实信号 [`Honesty`]。
//!
//! 此前四种图出口形状各异(`ArchGraph` / `SubGraph` / 裸字符串),诚实信号散在
//! `ArchGraph` 的字段里,调用子图与 UML **干脆没有**。收成一个类型之后:
//! * 消费方按 [`DiagramBody`] 的**形状**渲染(`#[serde(tag = "shape")]`,标签就在数据上);
//! * **每种图都带** [`Honesty`] —— 一张图看起来永远是完整的,除非它自己说出漏了多少;
//! * Mermaid 降为渲染器之一([`to_mermaid`]),MCP 用它。
//!
//! 全部 `Serialize + Deserialize`,往返相等(有测试钉)。

use super::registry::{DiagramKind, DiagramShape};
use super::uml::{type_graph_mermaid, TypeNode, TypeRelation};
use super::{clusters_mermaid, edge_style, escape, ArchGraph, ArchLink, ArchNode};
use crate::model::{Confidence, SymKind, SymbolId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 一张画好的图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagram {
    pub kind: DiagramKind,
    pub honesty: Honesty,
    pub body: DiagramBody,
}

/// 按**形状**分的图体。变体集合与 [`DiagramShape::ALL`] 一一对应(有测试钉)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum DiagramBody {
    /// 节点 + 带成分的聚合连接。
    Clusters {
        nodes: Vec<ArchNode>,
        links: Vec<ArchLink>,
    },
    /// 符号级调用子图。
    CallGraph {
        center: SymbolId,
        depth: u32,
        nodes: Vec<CallNode>,
        edges: Vec<CallEdge>,
    },
    /// 类型 + 实现/组合关系。
    TypeGraph {
        types: Vec<TypeNode>,
        relations: Vec<TypeRelation>,
    },
}

impl DiagramBody {
    pub fn shape(&self) -> DiagramShape {
        match self {
            DiagramBody::Clusters { .. } => DiagramShape::Clusters,
            DiagramBody::CallGraph { .. } => DiagramShape::CallGraph,
            DiagramBody::TypeGraph { .. } => DiagramShape::TypeGraph,
        }
    }
}

/// 调用子图的一个节点(符号的**画图所需**那几样,不是整个 `Symbol`)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallNode {
    pub id: SymbolId,
    pub name: String,
    pub file: String,
    pub kind: SymKind,
    pub start_line: usize,
}

/// 调用子图的一条边。一条就是一条,不聚合 —— 所以只有一个可信度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallEdge {
    pub from: SymbolId,
    pub to: SymbolId,
    pub confidence: Confidence,
    /// 歧义边:这个调用点在仓内有几个同名候选(「N 选 1」要印出来)。
    pub candidates: Option<usize>,
    pub call_site_line: Option<usize>,
}

/// **公共诚实信号** —— 每种图都带。
///
/// 🔴 `None` 与 `Some(0)` 是两回事:`Some(0)` = 量了、没有;`None` = **这张图不量这个**
/// (类图不画调用 ⇒ 调用那两格是 `None`)。把 `None` 写成 0 会让读者以为「一处都没漏」。
///
/// 格子怎么读:
/// * `unresolved_calls` —— **看不见**:识别为调用但连不上仓内符号(仓外 / 漏抓)。全仓读数。
/// * `ambiguous_calls` —— **分不清**:仓内有同名候选但钉不死是哪个。全仓读数。
/// * `filtered_guess_links` —— **滤掉**:全靠名字凑、一条确定的都没有而没画的连接(本图)。
/// * `excluded_test_symbols` —— **排除**:因为是测试而没画的符号(本图)。
/// * `omitted` —— **省略**:因为节点上限而没画的节点 / 符号 / 连接(本图)。
/// * `db_errors` —— **读库出错**:非空 = 这张图不完整。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Honesty {
    pub unresolved_calls: Option<usize>,
    pub ambiguous_calls: Option<usize>,
    pub filtered_guess_links: Option<usize>,
    pub excluded_test_symbols: Option<usize>,
    pub omitted: Option<Omitted>,
    pub db_errors: Vec<String>,
}

/// 因为节点上限而没画的量。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Omitted {
    pub nodes: usize,
    pub symbols: usize,
    pub links: usize,
}

impl Honesty {
    /// 团/模块图的诚实信号 —— 六格全量。
    pub fn of_arch(g: &ArchGraph) -> Honesty {
        let (nodes, symbols, links) = g.omitted;
        Honesty {
            unresolved_calls: Some(g.unresolved_calls),
            ambiguous_calls: Some(g.ambiguous_calls),
            filtered_guess_links: Some(g.filtered_guess_links),
            excluded_test_symbols: Some(g.excluded_test_symbols),
            omitted: Some(Omitted {
                nodes,
                symbols,
                links,
            }),
            db_errors: g.db_errors.clone(),
        }
    }
}

impl Diagram {
    /// 团/模块形状的图(`arch_graph` / `module_graph` 的产出)。
    pub fn clusters(kind: DiagramKind, g: ArchGraph) -> Diagram {
        let honesty = Honesty::of_arch(&g);
        Diagram {
            kind,
            honesty,
            body: DiagramBody::Clusters {
                nodes: g.nodes,
                links: g.links,
            },
        }
    }
}

/// 任何一张图 → Mermaid(图体 + 图注)。**渲染器之一**,不是图本身。
pub fn to_mermaid(d: &Diagram) -> String {
    let mut s = match &d.body {
        DiagramBody::Clusters { nodes, links } => clusters_mermaid(nodes, links),
        DiagramBody::CallGraph { nodes, edges, .. } => call_graph_mermaid(nodes, edges),
        DiagramBody::TypeGraph { types, relations } => type_graph_mermaid(types, relations),
    };
    let drawn = match &d.body {
        DiagramBody::Clusters { nodes, links } => Some((nodes.len(), links_count(links))),
        _ => None,
    };
    // 类图没有「可信度不同的边」—— 关系全来自声明 ⇒ 不印线型那一行
    let arrows = d.body.shape() != DiagramShape::TypeGraph;
    s.push_str(&legend_lines(&d.honesty, drawn, arrows));
    s
}

fn links_count(links: &[ArchLink]) -> usize {
    links.iter().map(|l| l.count()).sum()
}

/// 调用子图的图体。与 `subgraph_mermaid` 同一套线型。
fn call_graph_mermaid(nodes: &[CallNode], edges: &[CallEdge]) -> String {
    let mut s = String::from("flowchart LR\n");
    let mut id_of: HashMap<&str, String> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        let nid = format!("n{i}");
        s.push_str(&format!(
            "  {}[\"{}<br/><small>{}</small>\"]\n",
            nid,
            escape(&n.name),
            escape(&n.file)
        ));
        id_of.insert(n.id.as_str(), nid);
    }
    for e in edges {
        let (Some(a), Some(b)) = (id_of.get(e.from.as_str()), id_of.get(e.to.as_str())) else {
            continue;
        };
        let (arrow, tag) = edge_style(e.confidence);
        let tag = match (tag, e.candidates) {
            ("", _) => String::new(),
            (t, Some(n)) => format!("{t} {n} 选 1"),
            (t, None) => t.to_string(),
        };
        if tag.is_empty() {
            s.push_str(&format!("  {a} {arrow} {b}\n"));
        } else {
            s.push_str(&format!("  {a} {arrow}|\"{tag}\"| {b}\n"));
        }
    }
    s
}

/// `arch_mermaid` 的图注(兼容入口)。
pub(crate) fn honesty_legend(h: &Honesty, g: Option<&ArchGraph>) -> String {
    legend_lines(h, g.map(|g| (g.nodes.len(), links_count(&g.links))), true)
}

/// 图注:线型 + 诚实信号。`drawn` = 团/模块图画出的(节点数, 跨团调用条数),其它形状 `None`。
fn legend_lines(h: &Honesty, drawn: Option<(usize, usize)>, arrows: bool) -> String {
    let mut s = String::from("\n");
    if arrows {
        s.push_str("%% 线型:粗实线 ==> 确定 · 虚线 -.-> 动态派发 · 虚线带 ? 按名字凑的候选\n");
    }
    // 两格都是 None = 这张图不画调用 ⇒ 这一行不印(印成「不适用」只是噪声)
    if h.unresolved_calls.is_some() || h.ambiguous_calls.is_some() {
        let cell = |v: Option<usize>| v.map_or("不适用".to_string(), |n| n.to_string());
        s.push_str(&format!(
            "%% 诚实读数:{} 处调用**看不见**(仓外/漏抓)· {} 处**看得见但分不清**\n",
            cell(h.unresolved_calls),
            cell(h.ambiguous_calls)
        ));
    }
    if let Some(n) = h.excluded_test_symbols.filter(|n| *n > 0) {
        s.push_str(&format!(
            "%% ⚠ 已把**测试**排除在外({n} 个符号)—— 架构图讲的是产品的结构。\n\
             %%    要看测试请关掉 exclude_tests\n"
        ));
    }
    if let Some(n) = h.filtered_guess_links.filter(|n| *n > 0) {
        s.push_str(&format!(
            "%% ⚠ 另有 {n} 条连接**全靠名字凑**、一条确定的都没有,已滤掉 ——\n\
             %%    它不是依赖的证据。要看全部请关掉 certain_only\n"
        ));
    }
    if let Some(o) = h.omitted.filter(|o| o.nodes > 0) {
        match drawn {
            Some((drawn_nodes, drawn_links)) => {
                s.push_str(&format!(
                    "%% ⚠ 只画了最大的 {drawn_nodes} 个团;另有 {} 个更小的团没画(共 {} 个符号)\n",
                    o.nodes, o.symbols
                ));
                s.push_str(&format!(
                    "%%    画出的跨团调用 {drawn_links} 条,省掉的 {} 条\n",
                    o.links
                ));
                if o.links > drawn_links {
                    s.push_str(
                        "%%    🔴 省掉的比画出的还多 —— 小团里有被到处调用的公共件,这张图看不出它们\n",
                    );
                }
            }
            None => s.push_str(&format!(
                "%% ⚠ 另有 {} 个节点没画(节点上限),连带 {} 条关系\n",
                o.nodes, o.links
            )),
        }
    }
    if !h.db_errors.is_empty() {
        s.push_str(&format!(
            "%% 🔴 读索引出错 {} 处 —— 这张图**不完整**\n",
            h.db_errors.len()
        ));
    }
    s
}
