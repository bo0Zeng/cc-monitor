//! C++ 的 `LangSupport`(F14)。`class/struct` 设类型限定;**namespace 不设**(作用域≠类型,
//! 否则命名空间自由函数会被误判成 Method)。
//! **类外定义 `void A::m(){}`** 的 declarator 是 `qualified_identifier`(无 class 祖先)——用
//! `SymbolDef.qualifier`=scope 自带限定 A(F11 加宽 trait 正为此,F14 关键验证)。
//! 调用:`identifier`(自由)/ `field_expression`(`obj.m()`/`ptr->m()`)/ `qualified_identifier`
//! (`A::sm()` 作用域调用)/ `new_expression`(构造)。

use super::c::fn_name_node;
use super::{field_text, LangSupport, SymbolDef};
use tree_sitter::Node;

pub(crate) struct Cpp;
pub(crate) static CPP: Cpp = Cpp;

impl LangSupport for Cpp {
    fn qualifier_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if matches!(node.kind(), "class_specifier" | "struct_specifier") {
            field_text(node, "name", src)
        } else {
            None
        }
    }

    fn symbol_at(&self, node: Node, src: &[u8]) -> Option<SymbolDef> {
        if node.kind() != "function_definition" {
            return None;
        }
        let name_node = fn_name_node(node.child_by_field_name("declarator")?)?;
        match name_node.kind() {
            // 类内 inline 定义(field_identifier)/ 自由函数(identifier)/ 运算符/析构
            "identifier" | "field_identifier" | "operator_name" | "destructor_name" => {
                Some(SymbolDef {
                    name: name_node.utf8_text(src).ok()?.to_string(),
                    qualifier: None,
                    kind: None,
                })
            }
            // 类外定义 void A::m():自带限定 scope=A(或 ns::A)
            "qualified_identifier" => Some(SymbolDef {
                name: field_text(name_node, "name", src)?,
                qualifier: field_text(name_node, "scope", src),
                kind: None,
            }),
            _ => None,
        }
    }

    fn call_of(&self, node: Node, src: &[u8]) -> Option<(String, Option<String>, bool)> {
        match node.kind() {
            "call_expression" => {
                let func = node.child_by_field_name("function")?;
                match func.kind() {
                    "identifier" => func
                        .utf8_text(src)
                        .ok()
                        .map(|s| (s.to_string(), None, false)),
                    // f<int>() 泛型调用:剥 type-args 取裸名
                    "template_function" => field_text(func, "name", src).map(|n| (n, None, false)),
                    // obj.m() / ptr->m() / obj.m<int>() → 方法调用,接收者类型未知
                    "field_expression" => member_name(func.child_by_field_name("field")?, src)
                        .map(|s| (s, None, true)),
                    // A::sm() 作用域调用 → 带限定,可 Exact
                    "qualified_identifier" => {
                        let name = field_text(func, "name", src)?;
                        Some((name, field_text(func, "scope", src), false))
                    }
                    _ => None,
                }
            }
            // new B() → 连到构造器 B::B(泛型 new B<T>() 的 type 是 template_type,不剥离,记债)
            "new_expression" => field_text(node, "type", src).map(|t| (t, None, false)),
            _ => None,
        }
    }
}

/// 成员名:`obj.m()` 的 field 是 `field_identifier`;`obj.m<int>()` 是 `template_method`(带 name 字段)。
fn member_name(field: Node, src: &[u8]) -> Option<String> {
    if field.kind() == "template_method" {
        field_text(field, "name", src)
    } else {
        field.utf8_text(src).ok().map(String::from)
    }
}
