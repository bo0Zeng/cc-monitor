//! C 的 `LangSupport`(F14)。C 只有自由函数(无类型限定)。名藏在
//! `function_definition` → `function_declarator` → `identifier`(可能被 pointer/reference 包裹)。

use super::{LangSupport, SymbolDef};
use tree_sitter::Node;

pub(crate) struct CLang;
pub(crate) static CLANG: CLang = CLang;

/// 剥离 pointer/reference declarator,取到 `function_declarator` 里的名字节点。
pub(super) fn fn_name_node(decl: Node) -> Option<Node> {
    match decl.kind() {
        "function_declarator" => decl.child_by_field_name("declarator"),
        "pointer_declarator" | "reference_declarator" => {
            fn_name_node(decl.child_by_field_name("declarator")?)
        }
        _ => None,
    }
}

impl LangSupport for CLang {
    fn qualifier_of(&self, _node: Node, _src: &[u8]) -> Option<String> {
        None // C 无类型限定
    }

    fn symbol_at(&self, node: Node, src: &[u8]) -> Option<SymbolDef> {
        if node.kind() != "function_definition" {
            return None;
        }
        let name_node = fn_name_node(node.child_by_field_name("declarator")?)?;
        if name_node.kind() != "identifier" {
            return None;
        }
        Some(SymbolDef {
            name: name_node.utf8_text(src).ok()?.to_string(),
            qualifier: None,
            kind: None,
        })
    }

    fn call_of(&self, node: Node, src: &[u8]) -> Option<(String, Option<String>, bool)> {
        if node.kind() != "call_expression" {
            return None;
        }
        let func = node.child_by_field_name("function")?;
        if func.kind() == "identifier" {
            func.utf8_text(src)
                .ok()
                .map(|s| (s.to_string(), None, false))
        } else {
            None // 函数指针调用等,尽力略过
        }
    }
}
