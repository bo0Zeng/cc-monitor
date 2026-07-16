//! Kotlin 的 `LangSupport`(F13)。Kotlin 有**顶层自由函数**(→Function)与类内方法(→Method)。
//! **扩展函数 `fun String.ext()`**:receiver(name 之前的 `user_type`)经 `SymbolDef.qualifier`
//! 自带限定 → `file#String::ext`(F11 加宽 trait 正为此)。调用 `call_expression`:callee 为
//! `identifier`(自由)或 `navigation_expression`(`obj.m()`,取其末 `identifier` 作方法名)。

use super::{field_text, LangSupport, SymbolDef};
use tree_sitter::Node;

pub(crate) struct Kotlin;
pub(crate) static KOTLIN: Kotlin = Kotlin;

impl LangSupport for Kotlin {
    fn qualifier_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if matches!(node.kind(), "class_declaration" | "object_declaration") {
            field_text(node, "name", src)
        } else {
            None
        }
    }

    fn symbol_at(&self, node: Node, src: &[u8]) -> Option<SymbolDef> {
        if node.kind() != "function_declaration" {
            return None;
        }
        let name_node = node.child_by_field_name("name")?;
        let name = name_node.utf8_text(src).ok()?.to_string();
        Some(SymbolDef {
            name,
            // 扩展函数 receiver 自带限定;普通/成员函数无 → 回落祖先(类内→Method,顶层→Function)
            qualifier: receiver_type(node, name_node, src),
            kind: None,
        })
    }

    fn call_of(&self, node: Node, src: &[u8]) -> Option<(String, Option<String>, bool)> {
        if node.kind() != "call_expression" {
            return None;
        }
        let callee = node.child(0)?;
        match callee.kind() {
            "identifier" => callee
                .utf8_text(src)
                .ok()
                .map(|s| (s.to_string(), None, false)),
            // obj.method() / a.b.c():取导航表达式最后一个 identifier 作方法名
            "navigation_expression" => last_identifier(callee, src).map(|n| (n, None, true)),
            _ => None,
        }
    }
}

/// 扩展函数 receiver:`function_declaration` 里出现在 name 之前的 `user_type` 子节点。
/// (返回类型 `user_type` 在参数之后、name 之后,故用位置区分。)
fn receiver_type(node: Node, name_node: Node, src: &[u8]) -> Option<String> {
    let mut cur = node.walk();
    let recv = node
        .children(&mut cur)
        .find(|c| c.kind() == "user_type" && c.start_byte() < name_node.start_byte());
    recv.and_then(|c| c.utf8_text(src).ok()).map(String::from)
}

/// 导航表达式里最后一个 `identifier`(即被访问的成员名)。
fn last_identifier(node: Node, src: &[u8]) -> Option<String> {
    let mut cur = node.walk();
    let ids: Vec<Node> = node
        .children(&mut cur)
        .filter(|c| c.kind() == "identifier")
        .collect();
    ids.last()
        .and_then(|c| c.utf8_text(src).ok())
        .map(String::from)
}
