//! C 的 `LangSupport`(F14)。C 只有自由函数(无类型限定)。名藏在
//! `function_definition` → `function_declarator` → `identifier`(可能被 pointer/reference 包裹)。

use super::{dir_of, join_normalized, LangSupport, SymbolDef};
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

    /// `#include "local.h"` —— **只认引号形式**。`<stdio.h>`(节点是 `system_lib_string`)
    /// 是系统/第三方头,不是仓内依赖,连计数都不该计 ⇒ 直接当不是 import。
    fn import_of(&self, node: Node, src: &[u8]) -> Vec<String> {
        if node.kind() != "preproc_include" {
            return Vec::new();
        }
        super::child_by_kind(node, "string_literal")
            .and_then(|s| super::child_by_kind(s, "string_content"))
            .and_then(|c| c.utf8_text(src).ok())
            .map(|s| vec![s.to_string()])
            .unwrap_or_default()
    }

    /// 两试:① 相对导入方所在目录(`#include "../x.h"` 的常见写法);
    /// ② 原文当仓库相对路径后缀(项目配了 include 根、写成 `"proj/x.h"` 的情形)。
    /// ⚠ 真正的解析要读构建系统的 include 路径,静态层没有那个信息 —— 这两条是尽力。
    fn resolve_import(&self, from_file: &str, specifier: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(j) = join_normalized(dir_of(from_file), specifier) {
            out.push(j);
        }
        out.push(specifier.trim_start_matches("./").to_string());
        out
    }

    // ── 控制流分类 ──

    fn cfg_function_body<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        matches!(node.kind(), "function_definition").then(|| node.child_by_field_name("body"))?
    }

    fn is_block(&self, node: Node) -> bool {
        node.kind() == "compound_statement"
    }

    /// C 的形参藏在 `declarator`(`function_declarator`)底下,不是直接挂在函数上。
    /// ⚠ `int *p` 的 `declarator` 是 `pointer_declarator` 而非 identifier ⇒ 那一位给 `None`
    /// (位置占住,但不猜名字)。
    fn params_container<'a>(&self, node: Node<'a>, _src: &[u8]) -> Option<(Node<'a>, bool)> {
        if node.kind() != "function_definition" {
            return None;
        }
        let params = node
            .child_by_field_name("declarator")?
            .child_by_field_name("parameters")?;
        Some((params, false))
    }

    // ── 数据流(2026-09-17 接入)──

    fn call_args(&self, node: Node, src: &[u8]) -> Vec<Option<String>> {
        super::call_args_from(node.child_by_field_name("arguments"), src, &[])
    }

    fn call_arg_mentions(&self, node: Node, src: &[u8]) -> Vec<Vec<String>> {
        super::arg_mentions_from(node.child_by_field_name("arguments"), src)
    }

    fn local_binding<'a>(
        &self,
        node: Node<'a>,
        src: &[u8],
    ) -> Option<(String, Option<String>, Option<Node<'a>>)> {
        if node.kind() == "assignment_expression" {
            let name = super::access_path(node.child_by_field_name("left")?, src)?;
            return Some((name, None, node.child_by_field_name("right")));
        }
        if node.kind() != "declaration" {
            return None;
        }
        let d = node.child_by_field_name("declarator")?;
        if d.kind() != "init_declarator" {
            return None; // 无初始化的声明不是定义点(值未定)
        }
        // ⚠ `int *p = ..` 的 declarator 是 `pointer_declarator` —— **不认**,
        //   指针的别名语义不是语法定得了的。
        let name = d
            .child_by_field_name("declarator")
            .filter(|n| n.kind() == "identifier")?
            .utf8_text(src)
            .ok()?
            .to_string();
        let ty = node
            .child_by_field_name("type")
            .and_then(|t| t.utf8_text(src).ok())
            .map(super::outermost_type_name);
        Some((name, ty, d.child_by_field_name("value")))
    }

    fn return_exprs<'a>(&self, node: Node<'a>, _src: &[u8]) -> Vec<Node<'a>> {
        match self.cfg_function_body(node) {
            Some(b) => super::return_exprs_in(self, b, "return_statement"),
            None => Vec::new(),
        }
    }

    fn return_type_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if node.kind() != "function_definition" {
            return None;
        }
        Some(super::outermost_type_name(
            node.child_by_field_name("type")?.utf8_text(src).ok()?,
        ))
    }

    fn type_fields(&self, node: Node, src: &[u8]) -> Option<(String, Vec<(String, String)>)> {
        if !matches!(node.kind(), "struct_specifier" | "union_specifier") {
            return None;
        }
        super::type_fields_from(
            node,
            src,
            node.child_by_field_name("body"),
            &["field_declaration"],
        )
    }

    fn is_opaque_scope(&self, node: Node) -> bool {
        matches!(node.kind(), "function_definition")
    }

    fn ctrl_role<'a>(&self, node: Node<'a>, _src: &[u8]) -> super::CtrlRole<'a> {
        use super::{CtrlRole, JumpKind};
        match node.kind() {
            "if_statement" => {
                let mut arms = Vec::new();
                if let Some(c) = node.child_by_field_name("consequence") {
                    arms.push(c);
                }
                let alt = node.child_by_field_name("alternative").map(|a| {
                    // else 裹着真身(块 或 else-if)
                    if a.is_named() && a.kind() != "compound_statement" {
                        a.named_child(0).unwrap_or(a)
                    } else {
                        a
                    }
                });
                let exhaustive = alt.is_some();
                if let Some(a) = alt {
                    arms.push(a);
                }
                CtrlRole::Branch {
                    cond: node.child_by_field_name("condition"),
                    arms,
                    exhaustive,
                }
            }
            "for_statement" | "while_statement" | "do_statement" => {
                match node.child_by_field_name("body") {
                    Some(body) => CtrlRole::Loop {
                        cond: node.child_by_field_name("condition"),
                        body,
                    },
                    None => CtrlRole::Unsupported("循环没有 body"),
                }
            }
            // C 没有异常 ⇒ 没有受保护块
            "compound_statement" => CtrlRole::Sequence { body: node },
            "return_statement" => CtrlRole::Return,
            "break_statement" | "continue_statement" => CtrlRole::Jump {
                kind: if node.kind() == "break_statement" {
                    JumpKind::Break
                } else {
                    JumpKind::Continue
                },
                // 带标签 ⇒ 能跳出任意层,循环栈兜不住 ⇒ 作废
                labeled: node.named_child(0).is_some(),
            },
            // ⚠ switch 的 fall-through 让分支**不互斥**,而 Branch 假定互斥 ⇒ 不建模
            "switch_statement" | "switch_expression" => {
                CtrlRole::Unsupported("switch(fall-through 语义)")
            }
            // 🔴 `goto` 能跳到函数内任意标签 —— 循环栈与结构化建图彻底兜不住
            "goto_statement" | "labeled_statement" => CtrlRole::Unsupported("goto / 标签语句"),
            _ => CtrlRole::Plain,
        }
    }
}
