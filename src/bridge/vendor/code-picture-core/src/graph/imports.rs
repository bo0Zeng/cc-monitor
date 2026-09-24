//! `Imports` 边:import/use/include 的说明符 → **仓内文件**。
//!
//! 端点是**文件级 id**(裸仓库相对路径,无 `#`)—— 一条 import 的来源是整个文件,
//! 没有哪个符号当得起它。⚠ 这意味着 `index::delete_edges_from_file` 必须同时按
//! `from_id = <文件>` 删;漏删会让增量更新累积重复边。

use super::table::SymbolTable;
use crate::lang::LangSupport;
use crate::model::{Edge, EdgeKind};
use std::collections::HashSet;
use tree_sitter::Node;

/// 递归采 import。端点是**文件级 id**(裸路径);同一对 (文件,目标) 只留一条 ——
/// 一个文件从同一模块 import 五次是一条依赖,不是五条。
#[allow(clippy::too_many_arguments)]
pub(super) fn collect_imports(
    spec: &dyn LangSupport,
    node: Node,
    src: &[u8],
    file: &str,
    table: &SymbolTable,
    edges: &mut Vec<Edge>,
    seen: &mut HashSet<String>,
    unresolved: &mut usize,
) {
    let specifiers = spec.import_of(node, src);
    if !specifiers.is_empty() {
        for specifier in &specifiers {
            let candidates = spec.resolve_import(file, specifier);
            match table.resolve_file(&candidates) {
                // 自引用不建边(`a.ts` 里 `import './a'`,以及目录 index 自指)
                Some((target, _)) if target == file => {}
                Some((target, confidence)) => {
                    if seen.insert(target.clone()) {
                        edges.push(Edge {
                            from: file.to_string(),
                            to: target,
                            kind: EdgeKind::Imports,
                            call_site_line: Some(node.start_position().row + 1),
                            confidence,
                            candidates: None,
                            arg_flow: None,
                        });
                    }
                }
                None => *unresolved += 1,
            }
        }
        return; // import 语句内部不会再嵌 import,不必下钻
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_imports(spec, child, src, file, table, edges, seen, unresolved);
    }
}
