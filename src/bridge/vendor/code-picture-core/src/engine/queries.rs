//! 图查询:调用关系 · 波及面 · 调用路径 · 参数流 · 文件级依赖 · 符号检索。
//!
//! 🔴 **这一层的规矩:只走 `Calls`(唯一解析)。** 歧义边另有专门的口
//! (`ambiguous_callers` / `ambiguous_callees`),混着走就是把「N 选 1」当调用事实 ——
//! 实测代价见 `model::EdgeKind::AmbiguousCall`。
//!
//! 有界的查询(`paths` / `flow`)**截断必明说**:「空 + 未截断」= 真的没有;
//! 「空 + 已截断」= 我没找完。不给这两个值,读者分不出来。

use super::{Direction, Engine};
use crate::graph;
use crate::model::{
    AffectedSymbol, CallPath, Edge, FlowSet, FlowStep, ImpactSet, NodeView, PathSet, SubGraph,
    SymKind, Symbol, SymbolId,
};
use std::collections::HashSet;

impl Engine {
    /// 追一个**形参**的值往下流到哪些函数的哪些形参位。
    ///
    /// 🔴 **只追「原样传下去」这一种流动** —— 实参必须是个裸标识符,且它正好是当前形参。
    /// 被变换过(`g(x + 1)` · `g(x.field)` · `g(h(x))`)、存进结构体再取出、经闭包捕获的,
    /// **一律追不到**。那些要函数体内的定义-使用链(第三档),不在本层射程内。
    ///
    /// 只走 `Calls`(唯一解析)。同一对 (调用方,被调) 的参数流是多个调用点的**并集**,
    /// 分不出是哪一处传的(见 `Edge.arg_flow`)。
    pub fn flow(&self, sym: &SymbolId, param: usize, max_depth: usize) -> FlowSet {
        let mut out = FlowSet {
            root: sym.clone(),
            param,
            steps: Vec::new(),
            truncated: false,
        };
        let mut seen: HashSet<(String, usize)> = HashSet::new();
        seen.insert((sym.clone(), param));
        let mut frontier = vec![(sym.clone(), param, false)];
        let mut depth = 0usize;
        while !frontier.is_empty() {
            if depth >= max_depth {
                // 还有前沿没展开 ⇒ 是「我没往下找」,不是「到头了」
                out.truncated = true;
                break;
            }
            depth += 1;
            let mut next = Vec::new();
            for (node, p, from_derived) in &frontier {
                for e in self.or_empty("edges_from", self.idx.edges_from(node)) {
                    let Some(af) = &e.arg_flow else { continue };
                    for pair in af.split(',') {
                        let Some((i, j, derived)) = graph::parse_flow_pair(pair) else {
                            continue;
                        };
                        if i != *p {
                            continue;
                        }
                        // 链上只要有一跳是「可能」,后面全段都只能当「可能」读
                        let derived = derived || *from_derived;
                        if seen.insert((e.to.clone(), j)) {
                            out.steps.push(FlowStep {
                                id: e.to.clone(),
                                param: j,
                                depth,
                                derived,
                            });
                            next.push((e.to.clone(), j, derived));
                        }
                    }
                }
            }
            frontier = next;
        }
        // 确定性:depth 升序 → id 升序 → 参数位升序
        out.steps.sort_by(|a, b| {
            a.depth
                .cmp(&b.depth)
                .then(a.id.cmp(&b.id))
                .then(a.param.cmp(&b.param))
        });
        out
    }

    /// 枚举从 `from` 到 `to` 的调用路径。
    ///
    /// **只走 `Calls` 边**(唯一解析)。走歧义边等于把「N 选 1」**乘进链里** ——
    /// 一条 4 跳的链若每跳都是 N 选 1,整条链成立的可能性低到没有意义,
    /// 而它读起来却像一条确凿的调用链,比不给答案坏得多。
    ///
    /// 界:`max_depth` 跳数上限 · `max_paths` 条数上限。任一触顶 ⇒ `truncated = true`。
    /// 环(递归 / 互递归)在当前路径上一遇到就切断并计数,不展开。
    ///
    /// 🔴 产出是**调用关系上的链,不是执行轨迹** —— 图里没有分支、循环、提前返回,
    /// 也没有先后。详见 `CallPath` 的注释。
    pub fn paths(
        &self,
        from: &SymbolId,
        to: &SymbolId,
        max_depth: u32,
        max_paths: usize,
    ) -> PathSet {
        let mut out = PathSet {
            from: from.clone(),
            to: to.clone(),
            paths: Vec::new(),
            truncated: false,
            cycles_cut: 0,
        };
        if max_depth == 0 || max_paths == 0 {
            out.truncated = true;
            return out;
        }
        let mut stack: Vec<Edge> = Vec::new();
        let mut on_path: HashSet<String> = HashSet::new();
        on_path.insert(from.clone());
        self.dfs_paths(
            from,
            to,
            max_depth,
            max_paths,
            &mut stack,
            &mut on_path,
            &mut out,
        );
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn dfs_paths(
        &self,
        node: &SymbolId,
        target: &SymbolId,
        depth_left: u32,
        max_paths: usize,
        stack: &mut Vec<Edge>,
        on_path: &mut HashSet<String>,
        out: &mut PathSet,
    ) {
        if depth_left == 0 {
            out.truncated = true; // 到深度顶了:后面可能还有路,没找而已
            return;
        }
        // 确定性:出边按目标 id 升序(edges_from 的 SQL 无 ORDER BY,不赌它的行序)
        let mut edges = self.or_empty("edges_from", self.idx.edges_from(node));
        edges.sort_by(|a, b| a.to.cmp(&b.to));
        for e in edges {
            if out.paths.len() >= max_paths {
                out.truncated = true;
                return;
            }
            if e.to == *target {
                stack.push(e);
                out.paths.push(CallPath {
                    steps: stack.clone(),
                });
                stack.pop();
                continue;
            }
            if !on_path.insert(e.to.clone()) {
                out.cycles_cut += 1; // 这一跳会绕回当前路径上的某点 ⇒ 切断
                continue;
            }
            let next = e.to.clone();
            stack.push(e);
            self.dfs_paths(
                &next,
                target,
                depth_left - 1,
                max_paths,
                stack,
                on_path,
                out,
            );
            stack.pop();
            on_path.remove(&next);
        }
    }

    /// 谁**可能**调它(歧义入边,一跳)。与 `callers` **刻意分开** ——
    /// 这些边的含义是「这个调用点在仓内有 N 个同名候选,我分不出是哪个」,
    /// 混进 `callers` 就等于又把「N 选 1」当成了调用事实(那正是 2026-09-16 修掉的病)。
    pub fn ambiguous_callers(&self, sym: &SymbolId) -> Vec<Edge> {
        self.or_empty("ambiguous_edges_to", self.idx.ambiguous_edges_to(sym))
    }

    /// 它**可能**调谁(歧义出边,一跳)。诚实边界同 `ambiguous_callers`。
    pub fn ambiguous_callees(&self, sym: &SymbolId) -> Vec<Edge> {
        self.or_empty("ambiguous_edges_from", self.idx.ambiguous_edges_from(sym))
    }

    /// 这个**文件** import 了仓内哪些文件(直接依赖,一跳)。
    ///
    /// 与 `callees` 不是一回事:端点是**文件级 id**(裸仓库相对路径),参数也是文件不是符号。
    /// ⚠ 只含**仓内**目标 —— 外部包 / stdlib / 本门语言解析不到文件的(如 C# 的 `using`)
    /// 一律不建边,条数记在索引统计的 `unresolved_imports` 里。
    pub fn imports(&self, file: &str) -> Vec<Edge> {
        self.or_empty("import_edges_from", self.idx.import_edges_from(file))
    }

    /// 仓内哪些文件 import 了这个**文件**(反向依赖,一跳)。
    /// 诚实边界同 `imports`。
    pub fn imported_by(&self, file: &str) -> Vec<Edge> {
        self.or_empty("import_edges_to", self.idx.import_edges_to(file))
    }

    /// 它调谁(出边),BFS 到 depth 层(depth=1 = 直接)。
    pub fn callees(&self, sym: &SymbolId, depth: u32) -> Vec<Edge> {
        self.traverse(sym, depth, Direction::Out)
    }

    /// 谁调它(入边)。
    pub fn callers(&self, sym: &SymbolId, depth: u32) -> Vec<Edge> {
        self.traverse(sym, depth, Direction::In)
    }

    fn traverse(&self, start: &SymbolId, depth: u32, dir: Direction) -> Vec<Edge> {
        let mut result = Vec::new();
        let mut seen_edges: HashSet<(String, String)> = HashSet::new();
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(start.clone());
        let mut frontier = vec![start.clone()];
        let mut d = 0;
        while d < depth && !frontier.is_empty() {
            let mut next = Vec::new();
            for node in &frontier {
                let edges = match dir {
                    Direction::Out => self.or_empty("edges_from", self.idx.edges_from(node)),
                    Direction::In => self.or_empty("edges_to", self.idx.edges_to(node)),
                };
                for e in edges {
                    let other = match dir {
                        Direction::Out => e.to.clone(),
                        Direction::In => e.from.clone(),
                    };
                    if seen_edges.insert((e.from.clone(), e.to.clone())) {
                        result.push(e);
                    }
                    if visited.insert(other.clone()) {
                        next.push(other);
                    }
                }
            }
            frontier = next;
            d += 1;
        }
        result
    }

    /// 改动 `sym` 的 blast-radius:反向可达的全部传递调用者,各带最短反向距离(depth)。
    pub fn impact(&self, sym: &SymbolId) -> ImpactSet {
        let mut affected: Vec<AffectedSymbol> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(sym.clone());
        let mut frontier = vec![sym.clone()];
        let mut depth = 0usize;
        while !frontier.is_empty() {
            depth += 1;
            let mut next = Vec::new();
            for node in &frontier {
                for e in self.or_empty("edges_to", self.idx.edges_to(node)) {
                    if visited.insert(e.from.clone()) {
                        affected.push(AffectedSymbol {
                            id: e.from.clone(),
                            depth,
                        });
                        next.push(e.from);
                    }
                }
            }
            frontier = next;
        }
        // 确定性排序:depth 升序、并列 id 升序(不依赖 edges_to 顺序)
        affected.sort_by(|a, b| a.depth.cmp(&b.depth).then(a.id.cmp(&b.id)));
        ImpactSet {
            root: sym.clone(),
            affected,
        }
    }

    /// 单符号完整视图(F15):符号 + 直接 callers/callees + 关联文档 + 批注。
    /// 一等方法,MCP 与 cc-monitor 共用(此前 node 在 MCP 层现拼)。符号不存在 → None。
    pub fn node(&self, sym: &SymbolId) -> Option<NodeView> {
        let symbol = self.find_symbol(sym)?;
        Some(NodeView {
            symbol,
            callers: self.callers(sym, 1),
            callees: self.callees(sym, 1),
            docs: self.docs_for(sym),
            annotations: self.annotations_for(sym),
        })
    }

    /// 以 `sym` 为心的双向邻域子图(F15):depth 层内的边(callers ∪ callees,去重)+
    /// 涉及到的全部符号(含中心;悬空 id 查不到则略过)。确定性排序,便于前端/测试稳定。
    pub fn subgraph(&self, sym: &SymbolId, depth: u32) -> SubGraph {
        let mut edges = self.callers(sym, depth);
        edges.extend(self.callees(sym, depth));
        let mut seen_edge = HashSet::new();
        edges.retain(|e| seen_edge.insert((e.from.clone(), e.to.clone())));
        edges.sort_by(|a, b| a.from.cmp(&b.from).then(a.to.cmp(&b.to)));

        let mut ids: HashSet<SymbolId> = HashSet::new();
        ids.insert(sym.clone());
        for e in &edges {
            ids.insert(e.from.clone());
            ids.insert(e.to.clone());
        }
        let mut symbols: Vec<Symbol> = ids.iter().filter_map(|id| self.find_symbol(id)).collect();
        symbols.sort_by(|a, b| a.id.cmp(&b.id));
        SubGraph { symbols, edges }
    }

    /// 同 `subgraph`,但**把歧义边也带上**(两端都在图里的那些)。
    ///
    /// 🔴 给**画图**用。`callers`/`callees`/`impact` 排除歧义边是对的 ——
    /// 那几个接口的答案会被当成事实用。但一张图上「这儿有东西但钉不死」
    /// 本身就是该看见的信息,只要它**看起来不一样**(见 `crate::diagram`)。
    /// ⚠ 只补两端都已在图里的:歧义边常常一个调用点扇出十几个候选,
    ///    让它们把图撑开会把图变成噪声。
    pub fn subgraph_with_ambiguous(&self, sym: &SymbolId, depth: u32) -> SubGraph {
        let mut sg = self.subgraph(sym, depth);
        let ids: HashSet<String> = sg.symbols.iter().map(|s| s.id.clone()).collect();
        let mut extra: Vec<Edge> = Vec::new();
        for s in &sg.symbols {
            for e in self.ambiguous_callees(&s.id) {
                if ids.contains(&e.to) {
                    extra.push(e);
                }
            }
        }
        // 根自己的歧义被调:即使对端不在图里也带上 —— 那正是「这一跳钉不死」的现场
        for e in self.ambiguous_callees(sym) {
            extra.push(e);
        }
        let mut seen: HashSet<(String, String)> = sg
            .edges
            .iter()
            .map(|e| (e.from.clone(), e.to.clone()))
            .collect();
        for e in extra {
            if seen.insert((e.from.clone(), e.to.clone())) {
                if !ids.contains(&e.to) {
                    if let Some(t) = self.find_symbol(&e.to) {
                        sg.symbols.push(t);
                    }
                }
                sg.edges.push(e);
            }
        }
        sg.symbols.sort_by(|a, b| a.id.cmp(&b.id));
        sg.symbols.dedup_by(|a, b| a.id == b.id);
        sg.edges
            .sort_by(|a, b| a.from.cmp(&b.from).then(a.to.cmp(&b.to)));
        sg
    }

    /// 库里符号总数(测试/诊断用)。
    pub fn symbol_count(&self) -> usize {
        self.idx.count_symbols().unwrap_or(0)
    }

    /// 按 id 取单个符号(F06)。
    /// 某文件某行落在哪个**可调用符号**里。多个嵌套时取**最内层**(行范围最窄的那个)。
    ///
    /// 给精确层反查用:LSP 答的是「定义在 file:line」,要换回符号 id。
    /// ⚠ 只认 Function / Method —— 落在类型/常量上时返回 `None`(那不是个可调用目标)。
    pub fn symbol_at_line(&self, file: &str, line: usize) -> Option<SymbolId> {
        self.idx
            .all_symbols()
            .ok()?
            .into_iter()
            .filter(|s| {
                s.file == file
                    && s.start_line <= line
                    && line <= s.end_line
                    && matches!(s.kind, SymKind::Function | SymKind::Method)
            })
            .min_by_key(|s| s.end_line - s.start_line)
            .map(|s| s.id)
    }

    pub fn find_symbol(&self, id: &SymbolId) -> Option<Symbol> {
        self.idx.symbol_by_id(id)
    }

    /// 按名子串搜索符号(F06)。
    pub fn search(&self, query: &str, limit: usize) -> Vec<Symbol> {
        self.or_empty("search_symbols", self.idx.search_symbols(query, limit))
    }
}
