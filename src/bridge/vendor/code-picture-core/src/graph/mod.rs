//! 图边构建的**编排层**。只做一件事:把一个文件的语法树过一遍,
//! 让几条互不相干的解析路各自建自己的边。
//!
//! | 子模块 | 管什么 |
//! |---|---|
//! | `table` | 解析上下文(只读事实面) |
//! | `resolve` | 调用点 → 符号的解析规则 |
//! | `calls` | `Calls` / `AmbiguousCall` 边 |
//! | `imports` | `Imports` 边(文件级端点) |
//! | `locals` | 函数体内的语法可及信息 |
//! | `edges` | 边的落地(去重 · 升级 · 合并) |
//!
//! 两类边都只连**仓内**目标;外部的不建边,各自计入 `unresolved_*`。
//! **不做磁盘 IO** —— `src` 由 engine 传入(IO 边界只在 index/git/scan)。

mod calls;
mod edges;
mod imports;
mod locals;
mod resolve;
mod summary;
mod table;

pub use edges::parse_flow_pair;
pub use table::SymbolTable;

/// 跨函数摘要传播:顺调用边把被调的**函数摘要**抬到调用方头上,直到不动点。
///
/// 就地加厚 `table`,并返回**变了的**那些(调用方据此决定要不要重建一趟边、要不要落库)。
/// 空 = 没什么可抬的 ⇒ 不必重建。
pub fn propagate_summaries(
    table: &mut SymbolTable,
    edges: &[Edge],
) -> HashMap<crate::model::SymbolId, String> {
    let before = table.param_flows().clone();
    let after = summary::propagate(&before, edges);
    let mut changed = HashMap::new();
    for (id, flow) in after {
        if before.get(&id) != Some(&flow) {
            table.set_param_flow(&id, flow.clone());
            changed.insert(id, flow);
        }
    }
    changed
}

use calls::collect_calls;
use imports::collect_imports;

use crate::lang;
use crate::model::{Edge, Lang, Symbol};
use crate::symbols;
use std::collections::{HashMap, HashSet};

/// 一次建边的产出。两类未解析**分开计数** —— 调用解析不到与 import 解析不到
/// 是两回事(前者多为 stdlib/外部函数,后者多为外部包),压成一个数会让覆盖读数说不清话。
#[derive(Debug, Clone, Default)]
pub struct EdgeBuild {
    pub edges: Vec<Edge>,
    /// 识别为调用但被调不在符号表(外部/stdlib/漏抓)→ 未建边(F18 覆盖信号)。
    pub unresolved_calls: usize,
    /// 仓内有同名候选但分不清是哪个 → 建了 `AmbiguousCall` 而非 `Calls`。
    pub ambiguous_calls: usize,
    /// 识别为 import 但目标不在仓内文件表(外部包/stdlib/本门解析不到文件)→ 未建边。
    pub unresolved_imports: usize,
}

/// 构建某文件的出边:函数体内的 `Calls` + 文件头的 `Imports`。
/// `src` 与该文件的符号(`file_syms`)由调用方给出。
/// 语言由 `file` 扩展名判定;分类未实现的语言 → 无边(不报错)。
pub fn build_edges(src: &str, file: &str, file_syms: &[Symbol], table: &SymbolTable) -> EdgeBuild {
    let mut out = EdgeBuild::default();
    let Some(lang) = Lang::from_path(file) else {
        return out;
    };
    let Some(spec) = lang::spec_for(lang) else {
        return out;
    };
    let Some(tree) = symbols::parse_with(lang, src) else {
        return out;
    };
    // (from,to) → edges 下标,用于按置信度就地升级去重
    let mut index_of: HashMap<(String, String), usize> = HashMap::new();
    collect_calls(
        spec,
        tree.root_node(),
        src.as_bytes(),
        file_syms,
        None,
        table,
        &mut out.edges,
        &mut index_of,
        &mut out.unresolved_calls,
        &mut out.ambiguous_calls,
        &HashMap::new(),
        &[],
        &HashMap::new(),
        None,
        &crate::lang::FlowCtx {
            narrow: None,
            aliases: &HashMap::new(),
            effects: None,
        },
        &HashSet::new(),
    );
    let mut seen_imports: HashSet<String> = HashSet::new();
    collect_imports(
        spec,
        tree.root_node(),
        src.as_bytes(),
        file,
        table,
        &mut out.edges,
        &mut seen_imports,
        &mut out.unresolved_imports,
    );
    out
}
