//! Python 的 `LangSupport`(F12)。`class_definition` 设类型限定;`function_definition`
//! 是可调用定义(在 class 内→Method,模块级→Function,由驱动按祖先限定定);
//! `call` 的 callee 为 `identifier`(自由)或 `attribute`(`obj.m()`,方法调用)。

use super::{LangSupport, SymbolDef};
use tree_sitter::Node;

pub(crate) struct Python;
pub(crate) static PYTHON: Python = Python;

impl LangSupport for Python {
    fn qualifier_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if node.kind() == "class_definition" {
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(src).ok())
                .map(String::from)
        } else {
            None
        }
    }

    fn symbol_at(&self, node: Node, src: &[u8]) -> Option<SymbolDef> {
        if node.kind() == "function_definition" {
            let name = node
                .child_by_field_name("name")?
                .utf8_text(src)
                .ok()?
                .to_string();
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
        if node.kind() != "call" {
            return None;
        }
        let func = node.child_by_field_name("function")?;
        match func.kind() {
            "identifier" => func
                .utf8_text(src)
                .ok()
                .map(|s| (s.to_string(), None, false)),
            // obj.method() → 方法调用,接收者类型未知(DynamicGuess)
            "attribute" => func
                .child_by_field_name("attribute")
                .and_then(|a| a.utf8_text(src).ok())
                .map(|s| (s.to_string(), None, true)),
            _ => None,
        }
    }
}
