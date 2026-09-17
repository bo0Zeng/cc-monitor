//! 调用图边构建(F02;F11 语言无关驱动 + 按 `Lang` 分发)。对每个函数体内的调用点,
//! 按名(+作用域)解析到仓内符号,建 `Calls` 边并标注置信度。只连已知符号;外部调用不建边。
//! 精度天花板:静态、无类型推断 —— 靠 `Confidence` 诚实标注,不追求 sound。
//! **不做磁盘 IO**:`src` 由 engine 传入(IO 边界只在 index/git/scan)。

use crate::lang::{self, LangSupport};
use crate::model::{Confidence, Edge, EdgeKind, Lang, SymKind, Symbol, SymbolId};
use crate::symbols;
use std::collections::HashMap;
use tree_sitter::Node;

/// 名字 → 候选符号(用于按名解析调用)。
pub struct SymbolTable {
    by_name: HashMap<String, Vec<Symbol>>,
}

impl SymbolTable {
    pub fn from_symbols(syms: Vec<Symbol>) -> SymbolTable {
        let mut by_name: HashMap<String, Vec<Symbol>> = HashMap::new();
        for s in syms {
            by_name.entry(s.name.clone()).or_default().push(s);
        }
        SymbolTable { by_name }
    }
    fn candidates(&self, name: &str) -> &[Symbol] {
        self.by_name.get(name).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// 构建某文件里所有函数的出边。`src` 与该文件的符号(`file_syms`)由调用方给出。
/// 语言由 `file` 扩展名判定;分类未实现的语言 → 无边(不报错)。
/// 返回 `(边, 未解析调用点数)`:识别为调用但被调不在符号表(外部/stdlib/漏抓)→ 未建边,计入
/// `unresolved`(F18 覆盖信号)。
pub fn build_edges(
    src: &str,
    file: &str,
    file_syms: &[Symbol],
    table: &SymbolTable,
) -> (Vec<Edge>, usize) {
    let lang = match Lang::from_path(file) {
        Some(l) => l,
        None => return (vec![], 0),
    };
    let spec = match lang::spec_for(lang) {
        Some(s) => s,
        None => return (vec![], 0),
    };
    let tree = match symbols::parse_with(lang, src) {
        Some(t) => t,
        None => return (vec![], 0),
    };
    let mut edges: Vec<Edge> = Vec::new();
    // (from,to) → edges 下标,用于按置信度就地升级去重
    let mut index_of: HashMap<(String, String), usize> = HashMap::new();
    let mut unresolved = 0usize;
    collect_calls(
        spec,
        tree.root_node(),
        src.as_bytes(),
        file_syms,
        None,
        table,
        &mut edges,
        &mut index_of,
        &mut unresolved,
    );
    (edges, unresolved)
}

#[allow(clippy::too_many_arguments)]
fn collect_calls(
    spec: &dyn LangSupport,
    node: Node,
    src: &[u8],
    file_syms: &[Symbol],
    current_fn: Option<&str>,
    table: &SymbolTable,
    edges: &mut Vec<Edge>,
    index_of: &mut HashMap<(String, String), usize>,
    unresolved: &mut usize,
) {
    // 沿 AST 祖先确定 enclosing 函数(比按行号归属更准:同行多函数/嵌套都对)
    let this_fn: Option<String> = if spec.symbol_at(node, src).is_some() {
        fn_id_of(spec, node, src, file_syms)
    } else {
        None
    };
    let enclosing: Option<&str> = this_fn.as_deref().or(current_fn);

    if let Some((name, qual, is_method)) = spec.call_of(node, src) {
        if let Some(from) = current_fn {
            let line = node.start_position().row + 1;
            let targets = resolve(&name, qual.as_deref(), is_method, table);
            // 识别为调用、在函数内,但连不上任何仓内符号 → 未解析(外部/stdlib/漏抓)
            if targets.is_empty() {
                *unresolved += 1;
            }
            for (to, conf) in targets {
                upsert(edges, index_of, from, &to, line, conf);
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_calls(
            spec, child, src, file_syms, enclosing, table, edges, index_of, unresolved,
        );
    }
}

/// 把定义节点对回它的**可调用**符号 id(按名 + 起始行匹配;只认 Function/Method,
/// 使类/模块容器节点不会被当作调用的 enclosing 作用域——F13/F14 产出容器符号时仍稳)。
fn fn_id_of(
    spec: &dyn LangSupport,
    node: Node,
    src: &[u8],
    file_syms: &[Symbol],
) -> Option<String> {
    let name = spec.symbol_at(node, src)?.name;
    let line = node.start_position().row + 1;
    file_syms
        .iter()
        .find(|s| {
            s.name == name
                && s.start_line == line
                && matches!(s.kind, SymKind::Function | SymKind::Method)
        })
        .map(|s| s.id.clone())
}

/// 同 (from,to) 去重,保留**最高置信度**(Exact>Heuristic>DynamicGuess)。
fn upsert(
    edges: &mut Vec<Edge>,
    index_of: &mut HashMap<(String, String), usize>,
    from: &str,
    to: &str,
    line: usize,
    conf: Confidence,
) {
    let key = (from.to_string(), to.to_string());
    match index_of.get(&key) {
        Some(&i) => {
            if rank(conf) > rank(edges[i].confidence) {
                edges[i].confidence = conf;
                edges[i].call_site_line = Some(line);
            }
        }
        None => {
            index_of.insert(key, edges.len());
            edges.push(Edge {
                from: from.to_string(),
                to: to.to_string(),
                kind: EdgeKind::Calls,
                call_site_line: Some(line),
                confidence: conf,
            });
        }
    }
}

fn rank(c: Confidence) -> u8 {
    match c {
        Confidence::Exact => 2,
        Confidence::Heuristic => 1,
        Confidence::DynamicGuess => 0,
    }
}

/// 解析调用 → (目标 id, 置信度) 列表。语言无关(输入已由 `LangSupport::call_of` 抽好)。
fn resolve(
    name: &str,
    qual: Option<&str>,
    is_method: bool,
    table: &SymbolTable,
) -> Vec<(SymbolId, Confidence)> {
    let cands = table.candidates(name);
    if cands.is_empty() {
        return vec![]; // 外部 / stdlib / 宏,不建边
    }
    // 作用域调用 Type::name:优先限定名唯一匹配
    if let Some(q) = qual {
        let matched: Vec<&Symbol> = cands
            .iter()
            .filter(|s| id_has_qualifier(&s.id, q, name))
            .collect();
        if matched.len() == 1 {
            return vec![(matched[0].id.clone(), Confidence::Exact)];
        }
        if !matched.is_empty() {
            return matched
                .iter()
                .map(|s| (s.id.clone(), Confidence::Heuristic))
                .collect();
        }
        // 限定名没匹配上(如自由函数经 module:: 调用)→ 退回按名
    }
    if is_method {
        // recv.method():接收者类型未知
        return cands
            .iter()
            .map(|s| (s.id.clone(), Confidence::DynamicGuess))
            .collect();
    }
    if cands.len() == 1 {
        return vec![(cands[0].id.clone(), Confidence::Exact)];
    }
    cands
        .iter()
        .map(|s| (s.id.clone(), Confidence::Heuristic))
        .collect()
}

/// id 的符号段(最后一个 `#` 之后、去掉 `@行号`)是否等于 `Type::name`。
/// 结构化匹配,避免裸 `contains` 的子串误配。
fn id_has_qualifier(id: &str, qual: &str, name: &str) -> bool {
    let sym_seg = match id.rsplit_once('#') {
        Some((_, seg)) => seg.split('@').next().unwrap_or(seg),
        None => return false,
    };
    sym_seg == format!("{}::{}", qual, name)
}
