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

    /// C++ 的 `#include` 与 C 同形(同一个 `preproc_include` 节点),直接复用。
    /// ⚠ C++20 的 `import` 模块声明**不认** —— grammar 支持参差,现实中也极少见。
    fn import_of(&self, node: Node, src: &[u8]) -> Vec<String> {
        super::c::CLANG.import_of(node, src)
    }
    fn resolve_import(&self, from_file: &str, specifier: &str) -> Vec<String> {
        super::c::CLANG.resolve_import(from_file, specifier)
    }

    // ── 控制流分类:C++ 的结构化控制流与 C 同形,整族转发 ──
    // ⚠ C++ 的 try/catch **未建模**(C 的分类器里没有),踩到 `try_statement` 会当
    //    普通语句 ⇒ 但 `Plain` 承诺守卫会发现里面藏着控制流并整份作废。**方向安全**。

    fn cfg_function_body<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        super::c::CLANG.cfg_function_body(node)
    }
    fn is_block(&self, node: Node) -> bool {
        super::c::CLANG.is_block(node)
    }
    /// C++ 的形参与 C 同形(类内方法也是 `function_definition`)。
    fn params_container<'a>(&self, node: Node<'a>, src: &[u8]) -> Option<(Node<'a>, bool)> {
        super::c::CLANG.params_container(node, src)
    }

    // ── 数据流:与 C 同形,直接委托;方法调用的接收者是 C++ 独有的 ──

    fn call_args(&self, node: Node, src: &[u8]) -> Vec<Option<String>> {
        super::c::CLANG.call_args(node, src)
    }

    fn call_arg_mentions(&self, node: Node, src: &[u8]) -> Vec<Vec<String>> {
        super::c::CLANG.call_arg_mentions(node, src)
    }

    fn local_binding<'a>(
        &self,
        node: Node<'a>,
        src: &[u8],
    ) -> Option<(String, Option<String>, Option<Node<'a>>)> {
        super::c::CLANG.local_binding(node, src)
    }

    fn return_exprs<'a>(&self, node: Node<'a>, src: &[u8]) -> Vec<Node<'a>> {
        super::c::CLANG.return_exprs(node, src)
    }

    fn return_type_of(&self, node: Node, src: &[u8]) -> Option<String> {
        super::c::CLANG.return_type_of(node, src)
    }

    /// `obj.m()` / `obj->m()` 的接收者(`field_expression` 的 `argument` 侧)。
    fn receiver_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if node.kind() != "call_expression" {
            return None;
        }
        let f = node.child_by_field_name("function")?;
        if f.kind() != "field_expression" {
            return None;
        }
        let o = f.child_by_field_name("argument")?;
        super::access_path(o, src)
    }

    fn is_self_call(&self, node: Node, _src: &[u8]) -> bool {
        if node.kind() != "call_expression" {
            return false;
        }
        node.child_by_field_name("function")
            .filter(|f| f.kind() == "field_expression")
            .and_then(|f| f.child_by_field_name("argument"))
            .is_some_and(|o| o.kind() == "this")
    }

    /// C++ 的类体与 C 的结构体同形,多一个 `class_specifier`。
    fn type_fields(&self, node: Node, src: &[u8]) -> Option<(String, Vec<(String, String)>)> {
        if node.kind() == "class_specifier" {
            return super::type_fields_from(
                node,
                src,
                node.child_by_field_name("body"),
                &["field_declaration"],
            );
        }
        super::c::CLANG.type_fields(node, src)
    }

    fn impl_of(&self, node: Node, src: &[u8]) -> Option<(String, Vec<String>)> {
        if node.kind() != "class_specifier" && node.kind() != "struct_specifier" {
            return None;
        }
        let name = node
            .child_by_field_name("name")?
            .utf8_text(src)
            .ok()?
            .to_string();
        let mut traits = Vec::new();
        for i in 0..node.named_child_count() {
            if let Some(b) = node
                .named_child(i)
                .filter(|c| c.kind() == "base_class_clause")
            {
                super::collect_type_names(b, src, &mut traits);
            }
        }
        Some((name, traits))
    }

    fn is_opaque_scope(&self, node: Node) -> bool {
        super::c::CLANG.is_opaque_scope(node) || node.kind() == "lambda_expression"
    }

    /// C++ lambda 的形参藏在 `declarator`(`abstract_function_declarator`)底下,
    /// 不是直接挂在 lambda 上。
    fn receiver_name(&self) -> Option<&'static str> {
        Some("this")
    }

    fn closure_params(&self, node: Node, src: &[u8]) -> Option<Vec<Option<String>>> {
        (node.kind() == "lambda_expression").then(|| {
            let params = node
                .child_by_field_name("declarator")
                .and_then(|d| d.child_by_field_name("parameters"));
            super::param_names_from(params, src, false)
        })
    }
    fn ctrl_role<'a>(&self, node: Node<'a>, src: &[u8]) -> super::CtrlRole<'a> {
        super::c::CLANG.ctrl_role(node, src)
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
