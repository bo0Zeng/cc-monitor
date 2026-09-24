//! C# 的 `LangSupport`(F14)。方法恒在 class/struct/interface/record 内 → 全 Method
//! (`file#Type::name`);namespace **不**设限定(无自由方法,类已给限定)。构造器 name=类名
//! (`new D()`→`D::D`)。调用:`invocation_expression`(identifier / `member_access_expression`.name)
//! + `object_creation_expression`(`new`)。

use super::{field_text, LangSupport, SymbolDef};
use tree_sitter::Node;

pub(crate) struct CSharp;
pub(crate) static CSHARP: CSharp = CSharp;

impl LangSupport for CSharp {
    fn qualifier_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if matches!(
            node.kind(),
            "class_declaration"
                | "struct_declaration"
                | "interface_declaration"
                | "record_declaration"
        ) {
            field_text(node, "name", src)
        } else {
            None
        }
    }

    fn symbol_at(&self, node: Node, src: &[u8]) -> Option<SymbolDef> {
        if matches!(
            node.kind(),
            "method_declaration" | "constructor_declaration" | "local_function_statement"
        ) {
            let name = field_text(node, "name", src)?;
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
        match node.kind() {
            "invocation_expression" => {
                let func = node.child_by_field_name("function")?;
                match func.kind() {
                    "identifier" => func
                        .utf8_text(src)
                        .ok()
                        .map(|s| (s.to_string(), None, false)),
                    // M<int>() 泛型调用:剥 type-args 取裸名
                    "generic_name" => generic_bare(func, src).map(|n| (n, None, false)),
                    // obj.M() / this.n() / obj.M<int>() → 方法调用,接收者类型未知
                    "member_access_expression" => {
                        let name_node = func.child_by_field_name("name")?;
                        bare_name(name_node, src).map(|s| (s, None, true))
                    }
                    _ => None,
                }
            }
            "object_creation_expression" => field_text(node, "type", src).map(|t| (t, None, false)),
            _ => None,
        }
    }

    /// `using System;` · `using Foo.Bar;` · `using Alias = Foo.Baz;`
    fn import_of(&self, node: Node, src: &[u8]) -> Vec<String> {
        if node.kind() != "using_directive" {
            return Vec::new();
        }
        super::child_by_kind(node, "qualified_name")
            .or_else(|| super::child_by_kind(node, "identifier"))
            .and_then(|n| n.utf8_text(src).ok())
            .map(|s| vec![s.to_string()])
            .unwrap_or_default()
    }

    /// 🔴 **C# 的 `using` 解析不到文件,本门恒空 —— 这不是没做,是做不了。**
    /// `using` 引的是**命名空间**,而 C# 语言层面**没有**「一个命名空间一个文件 / 一个目录」
    /// 的规矩:一个文件可装多个命名空间,一个命名空间可散在任意多个文件、任意目录。
    /// Java/Kotlin 能按包名猜目录是因为那是那两门的约定,C# 没这个约定,猜就是编。
    ///
    /// ⇒ C# 仓的 import **全部**计入 `unresolved_imports`,那个数偏大是**如实反映**、不是 bug。
    /// 要 C# 的依赖方向得走 LSP 精确层(`lsp_*`),静态层给不了。
    fn resolve_import(&self, _from_file: &str, _specifier: &str) -> Vec<String> {
        Vec::new()
    }

    // ── 控制流分类 ──

    fn cfg_function_body<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        matches!(
            node.kind(),
            "method_declaration" | "constructor_declaration" | "local_function_statement"
        )
        .then(|| node.child_by_field_name("body"))?
    }

    fn is_block(&self, node: Node) -> bool {
        node.kind() == "block"
    }

    /// C# 的形参:`parameter_list` 里每个 `parameter(type:, name:)`。
    fn params_container<'a>(&self, node: Node<'a>, _src: &[u8]) -> Option<(Node<'a>, bool)> {
        if !matches!(
            node.kind(),
            "method_declaration" | "constructor_declaration" | "local_function_statement"
        ) {
            return None;
        }
        Some((node.child_by_field_name("parameters")?, false))
    }

    // ── 数据流(2026-09-17 接入)──

    fn call_args(&self, node: Node, src: &[u8]) -> Vec<Option<String>> {
        // C# 每个实参裹一层 `argument`,剥掉才看得见里面的标识符
        super::call_args_from(node.child_by_field_name("arguments"), src, &["argument"])
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
        if node.kind() != "local_declaration_statement" {
            return None;
        }
        let decl = (0..node.named_child_count())
            .filter_map(|i| node.named_child(i))
            .find(|c| c.kind() == "variable_declaration")?;
        let d = (0..decl.named_child_count())
            .filter_map(|i| decl.named_child(i))
            .find(|c| c.kind() == "variable_declarator")?;
        let name_node = d.child_by_field_name("name")?;
        let name = name_node.utf8_text(src).ok()?.to_string();
        // ⚠ 初始化表达式**没有字段名** —— 它是 `name` 之后的那个命名子
        let init = (0..d.named_child_count())
            .filter_map(|i| d.named_child(i))
            .find(|c| c.id() != name_node.id());
        // `var` 是推断,不是标注 —— 当作没写(由驱动看被调的返回类型)
        let ty = decl
            .child_by_field_name("type")
            .filter(|t| t.kind() != "implicit_type")
            .and_then(|t| t.utf8_text(src).ok())
            .map(super::outermost_type_name);
        Some((name, ty, init))
    }

    fn return_exprs<'a>(&self, node: Node<'a>, _src: &[u8]) -> Vec<Node<'a>> {
        match self.cfg_function_body(node) {
            Some(b) => super::return_exprs_in(self, b, "return_statement"),
            None => Vec::new(),
        }
    }

    fn return_type_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if node.kind() != "method_declaration" {
            return None;
        }
        Some(super::outermost_type_name(
            node.child_by_field_name("returns")?.utf8_text(src).ok()?,
        ))
    }

    fn receiver_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if node.kind() != "invocation_expression" {
            return None;
        }
        let f = node.child_by_field_name("function")?;
        if f.kind() != "member_access_expression" {
            return None;
        }
        let o = f.child_by_field_name("expression")?;
        super::access_path(o, src)
    }

    fn is_self_call(&self, node: Node, src: &[u8]) -> bool {
        if node.kind() != "invocation_expression" {
            return false;
        }
        node.child_by_field_name("function")
            .filter(|f| f.kind() == "member_access_expression")
            .and_then(|f| f.child_by_field_name("expression"))
            .and_then(|o| o.utf8_text(src).ok())
            .is_some_and(|t| t.trim() == "this")
    }

    /// 🔴 C# 的字段类型挂在**祖父**上:`field_declaration > variable_declaration(type:)`,
    /// 通用助手取不到 ⇒ 自己走一遍。属性(`int N { get; set; }`)则是直接的 `type:` + `name:`。
    fn type_fields(&self, node: Node, src: &[u8]) -> Option<(String, Vec<(String, String)>)> {
        if !matches!(node.kind(), "class_declaration" | "struct_declaration") {
            return None;
        }
        let name = node
            .child_by_field_name("name")?
            .utf8_text(src)
            .ok()?
            .to_string();
        let body = node.child_by_field_name("body")?;
        let mut out = Vec::new();
        let mut c = body.walk();
        for m in body.children(&mut c) {
            match m.kind() {
                "property_declaration" => {
                    let (Some(n), Some(t)) =
                        (m.child_by_field_name("name"), m.child_by_field_name("type"))
                    else {
                        continue;
                    };
                    if let (Ok(n), Ok(t)) = (n.utf8_text(src), t.utf8_text(src)) {
                        out.push((n.to_string(), super::outermost_type_name(t)));
                    }
                }
                "field_declaration" => {
                    let Some(vd) = (0..m.named_child_count())
                        .filter_map(|i| m.named_child(i))
                        .find(|c| c.kind() == "variable_declaration")
                    else {
                        continue;
                    };
                    let Some(t) = vd
                        .child_by_field_name("type")
                        .and_then(|t| t.utf8_text(src).ok())
                    else {
                        continue;
                    };
                    for i in 0..vd.named_child_count() {
                        let Some(d) = vd
                            .named_child(i)
                            .filter(|c| c.kind() == "variable_declarator")
                        else {
                            continue;
                        };
                        if let Some(Ok(n)) = d.child_by_field_name("name").map(|n| n.utf8_text(src))
                        {
                            out.push((n.to_string(), super::outermost_type_name(t)));
                        }
                    }
                }
                _ => {}
            }
        }
        Some((name, out))
    }

    fn impl_of(&self, node: Node, src: &[u8]) -> Option<(String, Vec<String>)> {
        if !matches!(node.kind(), "class_declaration" | "struct_declaration") {
            return None;
        }
        let name = node
            .child_by_field_name("name")?
            .utf8_text(src)
            .ok()?
            .to_string();
        // `: A, B` 是 `base_list`,**位置**子。C# 里基类与接口写在一处、语法分不开 ——
        // 都当成「实现关系」收下(宽的方向:多给候选,不会漏)。
        let mut traits = Vec::new();
        for i in 0..node.named_child_count() {
            if let Some(b) = node.named_child(i).filter(|c| c.kind() == "base_list") {
                super::collect_type_names(b, src, &mut traits);
            }
        }
        Some((name, traits))
    }

    fn is_opaque_scope(&self, node: Node) -> bool {
        matches!(
            node.kind(),
            "lambda_expression" | "anonymous_method_expression" | "local_function_statement"
        )
    }

    fn receiver_name(&self) -> Option<&'static str> {
        Some("this")
    }

    fn closure_params(&self, node: Node, src: &[u8]) -> Option<Vec<Option<String>>> {
        matches!(
            node.kind(),
            "lambda_expression" | "anonymous_method_expression"
        )
        .then(|| super::param_names_from(node.child_by_field_name("parameters"), src, false))
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
                    if a.is_named() && a.kind() != "block" {
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
            "for_statement" | "foreach_statement" | "while_statement" | "do_statement" => {
                match node.child_by_field_name("body") {
                    Some(body) => CtrlRole::Loop {
                        cond: node.child_by_field_name("condition"),
                        body,
                    },
                    None => CtrlRole::Unsupported("循环没有 body"),
                }
            }
            "try_statement" => {
                let Some(body) = node.child_by_field_name("body") else {
                    return CtrlRole::Unsupported("try 没有 body");
                };
                let mut c = node.walk();
                let handlers: Vec<Node> = node
                    .children(&mut c)
                    .filter(|x| x.kind().contains("catch") || x.kind().contains("finally"))
                    .filter_map(|x| x.child_by_field_name("body").or(Some(x)))
                    .collect();
                CtrlRole::Guarded { body, handlers }
            }
            "lock_statement" | "using_statement" | "checked_statement" => {
                match node.child_by_field_name("body") {
                    Some(body) => CtrlRole::Sequence { body },
                    None => CtrlRole::Unsupported("语句容器没有 body"),
                }
            }
            "block" => CtrlRole::Sequence { body: node },
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
            // `goto` 能跳到任意标签 ⇒ 建不出正确的图
            "goto_statement" | "labeled_statement" => CtrlRole::Unsupported("goto / 标签语句"),
            _ => CtrlRole::Plain,
        }
    }
}

/// `generic_name`(`M<int>`)的裸名 = 其首个 `identifier` 子节点。
fn generic_bare(node: Node, src: &[u8]) -> Option<String> {
    (0..node.child_count())
        .filter_map(|i| node.child(i))
        .find(|c| c.kind() == "identifier")
        .and_then(|c| c.utf8_text(src).ok())
        .map(String::from)
}

/// 成员名节点:`identifier` 直接取文本;`generic_name`(`M<int>`)剥 type-args。
fn bare_name(node: Node, src: &[u8]) -> Option<String> {
    if node.kind() == "generic_name" {
        generic_bare(node, src)
    } else {
        node.utf8_text(src).ok().map(String::from)
    }
}
