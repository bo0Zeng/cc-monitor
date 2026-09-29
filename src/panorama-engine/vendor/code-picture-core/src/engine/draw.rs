//! **画图的统一出口**(2026-09-24):`Engine::draw(kind, &req) -> Result<Diagram, DiagramError>`。
//!
//! 注册表(`diagram::registry`)说「有哪些图」,这里说「每种怎么取事实」——
//! 只取、不算:团/模块图照旧走 `arch_graph` / `module_graph`,调用子图走
//! `subgraph` / `subgraph_with_ambiguous`,类图走 `uml_graph`。
//!
//! 🔴 每条失败都**明说**:缺符号、符号不存在、认不出的图种(后者在 `DiagramKind::from_id`)。
//! 没有一条会回落成「画一张别的图」。

use crate::diagram::{
    self, CallEdge, CallNode, Diagram, DiagramBody, DiagramError, DiagramKind, DiagramRequest,
    Honesty, Omitted,
};
use crate::engine::Engine;
use crate::model::{Confidence, EdgeKind, SubGraph, TokenBudget};

impl Engine {
    /// 画一张图。`req` 里这张图不认的字段被忽略(认哪些见 `kind.info().params`)。
    pub fn draw(&self, kind: DiagramKind, req: &DiagramRequest) -> Result<Diagram, DiagramError> {
        let opts = req.draw_opts();
        let mut d = match kind {
            // ⚠ 预算给满:`overview(budget)` 会按 token 预算**裁掉子系统**,而被裁掉的那些
            //   不会进 `omitted` —— 那是静默省略。画图的省略只许由 `max_nodes` 决定、并记账。
            DiagramKind::Arch => {
                Diagram::clusters(kind, self.arch_graph(TokenBudget(usize::MAX), opts))
            }
            DiagramKind::Module => Diagram::clusters(kind, self.module_graph(opts)),
            DiagramKind::Calls => self.draw_calls(kind, req)?,
            DiagramKind::Uml => {
                let symbols = self.or_empty("all_symbols", self.idx.all_symbols());
                let impls = self.or_empty("all_impls", self.idx.all_impls());
                let fields = self.or_empty("all_field_types", self.idx.all_field_types());
                let g = diagram::uml_graph(&symbols, &impls, &fields, opts.max_nodes);
                Diagram {
                    kind,
                    // 类图不画调用、不滤猜测边、不排测试 ⇒ 那几格是「不量」,不是 0
                    honesty: Honesty {
                        omitted: Some(g.omitted),
                        ..Honesty::default()
                    },
                    body: DiagramBody::TypeGraph {
                        types: g.types,
                        relations: g.relations,
                    },
                }
            }
        };
        // 读库出错记在引擎上,取完事实之后再读 —— 这一次取数出的错也算进去
        d.honesty.db_errors = self.db_errors();
        Ok(d)
    }

    /// 调用子图。`certain_only` 打开时滤掉「按名字凑」的边(`Heuristic`/`DynamicGuess`),
    /// **派发边留着**(候选集完整,只是运行时才知道是哪个)—— 与团/模块图同一条口径。
    fn draw_calls(&self, kind: DiagramKind, req: &DiagramRequest) -> Result<Diagram, DiagramError> {
        let sym = req
            .symbol
            .clone()
            .ok_or(DiagramError::NeedsSymbol { kind })?;
        if self.find_symbol(&sym).is_none() {
            return Err(DiagramError::SymbolNotFound { symbol: sym });
        }
        let depth = req.depth();
        let opts = req.draw_opts();
        let sg: SubGraph = self.subgraph_with_ambiguous(&sym, depth);
        let guess = |c: Confidence| matches!(c, Confidence::Heuristic | Confidence::DynamicGuess);
        let mut filtered = 0usize;
        let edges: Vec<CallEdge> = sg
            .edges
            .iter()
            .filter(|e| e.kind != EdgeKind::Imports)
            .filter(|e| {
                let drop = opts.certain_only && guess(e.confidence);
                if drop {
                    filtered += 1;
                }
                !drop
            })
            .map(|e| CallEdge {
                from: e.from.clone(),
                to: e.to.clone(),
                confidence: e.confidence,
                candidates: e.candidates,
                call_site_line: e.call_site_line,
            })
            .collect();
        // 只因被滤掉的边才进图的端点也不画 —— 否则图上会有孤零零、没人连的节点
        let touched: std::collections::HashSet<&str> = edges
            .iter()
            .flat_map(|e| [e.from.as_str(), e.to.as_str()])
            .chain(std::iter::once(sym.as_str()))
            .collect();
        let nodes: Vec<CallNode> = sg
            .symbols
            .iter()
            .filter(|s| touched.contains(s.id.as_str()))
            .map(|s| CallNode {
                id: s.id.clone(),
                name: s.name.clone(),
                file: s.file.clone(),
                kind: s.kind.clone(),
                start_line: s.start_line,
            })
            .collect();
        let dropped_nodes = sg.symbols.len() - nodes.len();
        let meta = |k: &str| -> usize {
            self.idx
                .get_meta(k)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
        };
        Ok(Diagram {
            kind,
            honesty: Honesty {
                unresolved_calls: Some(meta("unresolved_calls")),
                ambiguous_calls: Some(meta("ambiguous_calls")),
                filtered_guess_links: Some(filtered),
                // 调用子图不排测试:「我改这个,哪些测试会红」正是调用关系该答的
                excluded_test_symbols: None,
                // 调用子图没有节点上限;只有「因边被滤掉而一起没画的端点」
                omitted: Some(Omitted {
                    nodes: dropped_nodes,
                    symbols: dropped_nodes,
                    links: 0,
                }),
                db_errors: Vec::new(),
            },
            body: DiagramBody::CallGraph {
                center: sym,
                depth,
                nodes,
                edges,
            },
        })
    }
}
