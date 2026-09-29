//! Java 的 `LangSupport`(F13)。Java 无自由函数——方法/构造器恒在类型内,由祖先限定
//! 成 Method(`file#Type::name`)。`class/interface/enum/record` 设限定;
//! 构造器 name=类名(`new B()` 可连到 `B::B`);`method_invocation`/`object_creation_expression` 为调用。

use super::{field_text, LangSupport, SymbolDef};
use tree_sitter::Node;

pub(crate) struct Java;
pub(crate) static JAVA: Java = Java;

/// 该节点是否直接含 `class_body` 子节点(用于识别匿名类 `new T(){…}`)。
fn has_class_body(node: Node) -> bool {
    (0..node.child_count()).any(|i| node.child(i).map(|c| c.kind()) == Some("class_body"))
}

impl LangSupport for Java {
    fn qualifier_of(&self, node: Node, src: &[u8]) -> Option<String> {
        match node.kind() {
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration" => field_text(node, "name", src),
            // 匿名类 `new Runnable(){ … }`(object_creation_expression 带 class_body):
            // 其方法归到被实现的类型,否则会泄漏成假的顶层 Function 污染符号表
            "object_creation_expression" if has_class_body(node) => field_text(node, "type", src),
            _ => None,
        }
    }

    fn symbol_at(&self, node: Node, src: &[u8]) -> Option<SymbolDef> {
        // 方法 + 构造器都是可调用定义;恒在类型内 → 驱动按祖先限定成 Method
        if matches!(
            node.kind(),
            "method_declaration" | "constructor_declaration"
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
            "method_invocation" => {
                let name = field_text(node, "name", src)?;
                // 有接收者(obj.m() / this.n())→ 类型未知走 DynamicGuess;
                // 无接收者(隐式 this 的 m())→ 按名解析(单候选可 Exact)
                let is_method = node.child_by_field_name("object").is_some();
                Some((name, None, is_method))
            }
            // new B() → 连到构造器 B::B(type 字段取类名;泛型 new B<T>() 的 T 不剥离,记债)
            "object_creation_expression" => field_text(node, "type", src).map(|t| (t, None, false)),
            _ => None,
        }
    }

    /// `import com.x.Foo;` · `import static com.x.Bar.baz;` · `import com.y.*;`
    fn import_of(&self, node: Node, src: &[u8]) -> Vec<String> {
        if node.kind() != "import_declaration" {
            return Vec::new();
        }
        super::child_by_kind(node, "scoped_identifier")
            .or_else(|| super::child_by_kind(node, "identifier"))
            .and_then(|n| n.utf8_text(src).ok())
            .map(|s| vec![s.to_string()])
            .unwrap_or_default()
    }

    /// 包名映射目录是 Java 的硬约定,但源根(`src/main/java/`)不在包名里 ⇒ 给路径**后缀**。
    /// ⚠ 绝大多数 `import` 是 JDK / 第三方,解不到仓内文件是常态,如实计入 `unresolved_imports`。
    fn resolve_import(&self, _from_file: &str, specifier: &str) -> Vec<String> {
        super::dotted_candidates(specifier, "java")
    }

    // ── 控制流分类 ──

    fn cfg_function_body<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        matches!(
            node.kind(),
            "method_declaration" | "constructor_declaration"
        )
        .then(|| node.child_by_field_name("body"))?
    }

    fn is_block(&self, node: Node) -> bool {
        node.kind() == "block"
    }

    /// Java 的形参:`formal_parameters` 里每个 `formal_parameter(type:, name:)`。
    /// 接收者是隐式 `this`,不占形参位 ⇒ 不跳第 0 个。
    fn params_container<'a>(&self, node: Node<'a>, _src: &[u8]) -> Option<(Node<'a>, bool)> {
        if !matches!(
            node.kind(),
            "method_declaration" | "constructor_declaration"
        ) {
            return None;
        }
        Some((node.child_by_field_name("parameters")?, false))
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
        // 再赋值也是定义点;左边可以是访问路径(`s.f = ..`),下标写入不认
        if node.kind() == "assignment_expression" {
            let name = super::access_path(node.child_by_field_name("left")?, src)?;
            return Some((name, None, node.child_by_field_name("right")));
        }
        if node.kind() != "local_variable_declaration" {
            return None;
        }
        let d = node.child_by_field_name("declarator")?;
        let name = d
            .child_by_field_name("name")
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
        if node.kind() != "method_declaration" {
            return None;
        }
        Some(super::outermost_type_name(
            node.child_by_field_name("type")?.utf8_text(src).ok()?,
        ))
    }

    fn receiver_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if node.kind() != "method_invocation" {
            return None;
        }
        let o = node.child_by_field_name("object")?;
        super::access_path(o, src)
    }

    /// ⚠ 只认**显式** `this.m()`。无限定的 `m()` 在 Java 里可能是本类的、继承来的、
    /// 也可能是静态导入的 —— 认成本类就是猜。
    fn is_self_call(&self, node: Node, _src: &[u8]) -> bool {
        node.kind() == "method_invocation"
            && node
                .child_by_field_name("object")
                .is_some_and(|o| o.kind() == "this")
    }

    fn type_fields(&self, node: Node, src: &[u8]) -> Option<(String, Vec<(String, String)>)> {
        if !matches!(node.kind(), "class_declaration" | "interface_declaration") {
            return None;
        }
        super::type_fields_from(
            node,
            src,
            node.child_by_field_name("body"),
            &["field_declaration"],
        )
    }

    fn impl_of(&self, node: Node, src: &[u8]) -> Option<(String, Vec<String>)> {
        if node.kind() != "class_declaration" {
            return None;
        }
        let name = node
            .child_by_field_name("name")?
            .utf8_text(src)
            .ok()?
            .to_string();
        let mut traits = Vec::new();
        // `implements A, B` 在 `interfaces` 字段下裹一层 `type_list`
        if let Some(list) = node.child_by_field_name("interfaces") {
            super::collect_type_names(list, src, &mut traits);
        }
        if let Some(sup) = node.child_by_field_name("superclass") {
            super::collect_type_names(sup, src, &mut traits);
        }
        Some((name, traits))
    }

    fn is_opaque_scope(&self, node: Node) -> bool {
        matches!(
            node.kind(),
            "lambda_expression" | "method_declaration" | "class_declaration"
        )
    }

    fn receiver_name(&self) -> Option<&'static str> {
        Some("this")
    }

    fn closure_params(&self, node: Node, src: &[u8]) -> Option<Vec<Option<String>>> {
        (node.kind() == "lambda_expression")
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
            "for_statement" | "enhanced_for_statement" | "while_statement" | "do_statement" => {
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
            "synchronized_statement" | "labeled_statement" => {
                CtrlRole::Unsupported("同步块 / 标签语句")
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
            // Java 的标签语句配合带标签 break,一并作废(上面已处理)
            _ => CtrlRole::Plain,
        }
    }
}
