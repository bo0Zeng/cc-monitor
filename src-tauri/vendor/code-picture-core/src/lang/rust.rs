//! Rust 的 `LangSupport`(F11:从 symbols.rs/graph.rs 原逻辑平移,行为不变)。
//! `impl` 设类型限定;`function_item` 是可调用定义;`call_expression` 是调用点。

use super::{LangSupport, SymbolDef};
use tree_sitter::Node;

pub(crate) struct Rust;
pub(crate) static RUST: Rust = Rust;

impl LangSupport for Rust {
    fn qualifier_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if node.kind() == "impl_item" {
            node.child_by_field_name("type")
                .and_then(|t| t.utf8_text(src).ok())
                .map(String::from)
        } else {
            None
        }
    }

    fn symbol_at(&self, node: Node, src: &[u8]) -> Option<SymbolDef> {
        if node.kind() == "function_item" {
            let name = node
                .child_by_field_name("name")?
                .utf8_text(src)
                .ok()?
                .to_string();
            // Rust:限定与 kind 都由驱动按祖先 `impl` 决定(自由函数 / 方法),定义不自带
            Some(SymbolDef {
                name,
                qualifier: None,
                kind: None,
            })
        } else {
            None
        }
    }

    fn call_of(&self, node: Node, src: &[u8]) -> Option<(String, Option<String>, bool)> {
        if node.kind() != "call_expression" {
            return None;
        }
        let callee = node.child_by_field_name("function")?;
        let is_method = callee.kind() == "field_expression";
        let (name, qual) = callee_name(callee, src)?;
        Some((name, qual, is_method))
    }
}

/// 从 callee 节点抽 (名字, 作用域限定?)。(平移自原 graph.rs)
fn callee_name(callee: Node, src: &[u8]) -> Option<(String, Option<String>)> {
    match callee.kind() {
        "identifier" => callee.utf8_text(src).ok().map(|s| (s.to_string(), None)),
        "scoped_identifier" => {
            let name = callee
                .child_by_field_name("name")?
                .utf8_text(src)
                .ok()?
                .to_string();
            let qual = callee
                .child_by_field_name("path")
                .and_then(|p| p.utf8_text(src).ok())
                // module::Type → 取最后一段作为类型限定
                .map(|s| s.rsplit("::").next().unwrap_or(s).to_string());
            Some((name, qual))
        }
        "field_expression" => {
            let f = callee.child_by_field_name("field")?;
            f.utf8_text(src).ok().map(|s| (s.to_string(), None))
        }
        _ => None,
    }
}
