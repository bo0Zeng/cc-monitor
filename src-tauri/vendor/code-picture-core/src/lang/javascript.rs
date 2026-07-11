//! JavaScript 的 `LangSupport`(F12)。分类逻辑抽成 `pub(super)` 函数,TypeScript 复用
//! (两者 grammar 节点种类同源)。
//! `class_declaration` 设限定;函数定义有三种形态:`function_declaration`、类内
//! `method_definition`、以及**赋给变量的箭头/函数表达式**(`const f = () => {}`)——
//! 后者符号挂在 `variable_declarator` 上抓名(**不**再匹配内层 arrow,避免双符号)。
//! 调用 `call_expression`:callee 为 `identifier`(自由)或 `member_expression`(`obj.m()`)。

use super::{field_text, LangSupport, SymbolDef};
use tree_sitter::Node;

/// 该节点的 `value` 子节点是否为"函数"(可赋给变量/类字段的可调用体)。
fn value_is_function(node: Node) -> bool {
    node.child_by_field_name("value")
        .map(|v| {
            matches!(
                v.kind(),
                "arrow_function" | "function_expression" | "generator_function"
            )
        })
        .unwrap_or(false)
}

pub(super) fn qualifier_of(node: Node, src: &[u8]) -> Option<String> {
    // 普通类 + TS 抽象类都为其方法设类型限定(JS grammar 不产 abstract_class_declaration,加了无副作用)
    if matches!(
        node.kind(),
        "class_declaration" | "abstract_class_declaration"
    ) {
        field_text(node, "name", src)
    } else {
        None
    }
}

pub(super) fn symbol_at(node: Node, src: &[u8]) -> Option<SymbolDef> {
    let name = match node.kind() {
        "function_declaration" | "generator_function_declaration" => field_text(node, "name", src),
        // 类方法:仅 class_body 内算(对象字面量简写方法 `{ m(){} }` 也是 method_definition,
        // 但不是自由函数——若当自由函数会污染全局名表、造伪边,故排除)
        "method_definition" if node.parent().map(|p| p.kind()) == Some("class_body") => {
            field_text(node, "name", src)
        }
        // 赋给变量的箭头/函数表达式:const f = () => {} / const f = function(){}
        "variable_declarator" if value_is_function(node) => field_text(node, "name", src),
        // 类字段箭头 `handler = () => {}`(现代 JS/TS 常见,如 React 绑定方法)。
        // JS 用 `field_definition`(property 字段)、TS 用 `public_field_definition`(name 字段)。
        "field_definition" | "public_field_definition" if value_is_function(node) => {
            field_text(node, "name", src).or_else(|| field_text(node, "property", src))
        }
        _ => None,
    }?;
    Some(SymbolDef {
        name,
        qualifier: None,
        kind: None,
    })
}

pub(super) fn call_of(node: Node, src: &[u8]) -> Option<(String, Option<String>, bool)> {
    if node.kind() != "call_expression" {
        return None;
    }
    let func = node.child_by_field_name("function")?;
    match func.kind() {
        "identifier" => func
            .utf8_text(src)
            .ok()
            .map(|s| (s.to_string(), None, false)),
        // obj.method() → 方法调用,接收者类型未知(DynamicGuess)
        "member_expression" => field_text(func, "property", src).map(|s| (s, None, true)),
        _ => None,
    }
}

pub(crate) struct Javascript;
pub(crate) static JAVASCRIPT: Javascript = Javascript;

impl LangSupport for Javascript {
    fn qualifier_of(&self, node: Node, src: &[u8]) -> Option<String> {
        qualifier_of(node, src)
    }
    fn symbol_at(&self, node: Node, src: &[u8]) -> Option<SymbolDef> {
        symbol_at(node, src)
    }
    fn call_of(&self, node: Node, src: &[u8]) -> Option<(String, Option<String>, bool)> {
        call_of(node, src)
    }
}
