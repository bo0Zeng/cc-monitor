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

    /// `import com.x.Foo` · `import com.y.*`(节点 kind 就叫 `import`)。
    fn import_of(&self, node: Node, src: &[u8]) -> Vec<String> {
        if node.kind() != "import" {
            return Vec::new();
        }
        super::child_by_kind(node, "qualified_identifier")
            .or_else(|| super::child_by_kind(node, "identifier"))
            .and_then(|n| n.utf8_text(src).ok())
            .map(|s| vec![s.to_string()])
            .unwrap_or_default()
    }

    /// 同 Java 走包名→目录后缀。
    /// 🔴 **Kotlin 比 Java 弱一档**:Kotlin 不要求文件名与类名一致,一个文件可装任意多个
    /// 顶层声明 ⇒ `com.x.Foo` 未必住 `com/x/Foo.kt`。命中算赚,不中是常态,不是 bug。
    fn resolve_import(&self, _from_file: &str, specifier: &str) -> Vec<String> {
        super::dotted_candidates(specifier, "kt")
    }

    // ── 控制流分类 ──

    /// ⚠ Kotlin 的函数体是**节点 kind 而非字段名**(`function_body`,里面才是 `block`)——
    /// F68 在签名抽取上踩过同一颗雷(当时整门签名恒 None)。这里一并按 kind 找。
    fn cfg_function_body<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        if node.kind() != "function_declaration" {
            return None;
        }
        let fb = node
            .child_by_field_name("body")
            .or_else(|| super::child_by_kind(node, "function_body"))?;
        // `function_body` 外壳里才是真正的 block;表达式体(`fun f() = expr`)没有 block
        Some(super::child_by_kind(fb, "block").unwrap_or(fb))
    }

    fn is_block(&self, node: Node) -> bool {
        node.kind() == "block"
    }

    /// 🔴 Kotlin 的 `function_value_parameters` **没有字段名**,只能按 kind 找 ——
    /// 与分支体那个坑同源(见 `cfg_function_body` 的注释)。
    fn params_container<'a>(&self, node: Node<'a>, _src: &[u8]) -> Option<(Node<'a>, bool)> {
        if node.kind() != "function_declaration" {
            return None;
        }
        let params = (0..node.named_child_count())
            .filter_map(|i| node.named_child(i))
            .find(|c| c.kind() == "function_value_parameters")?;
        Some((params, false))
    }

    /// 🔴 Kotlin 的形参是 `(parameter (identifier) (user_type ..))` —— **也没有 `type` 字段**,
    /// 通用的 `param_types_from` 取不到。类型在第二个命名子上。
    fn param_types(&self, node: Node, src: &[u8]) -> Vec<Option<String>> {
        let Some((c, _)) = self.params_container(node, src) else {
            return Vec::new();
        };
        (0..c.named_child_count())
            .filter_map(|i| c.named_child(i))
            .map(|p| {
                p.named_child(1)
                    .and_then(|t| t.utf8_text(src).ok())
                    .map(super::outermost_type_name)
                    .filter(|t| !t.is_empty())
            })
            .collect()
    }

    // ── 数据流(2026-09-17 接入)──
    // 🔴 Kotlin 通篇**没有字段名**:实参容器、初始化表达式、返回类型都只能按 kind /
    //    位置找。这与分支体那个坑同源 —— 用 `child_by_field_name` 会一路静默返回 None。

    fn call_args(&self, node: Node, src: &[u8]) -> Vec<Option<String>> {
        super::call_args_from(Self::value_args(node), src, &["value_argument"])
    }

    fn call_arg_mentions(&self, node: Node, src: &[u8]) -> Vec<Vec<String>> {
        super::arg_mentions_from(Self::value_args(node), src)
    }

    fn local_binding<'a>(
        &self,
        node: Node<'a>,
        src: &[u8],
    ) -> Option<(String, Option<String>, Option<Node<'a>>)> {
        if node.kind() == "assignment" {
            let name = super::access_path(node.child_by_field_name("left")?, src)?;
            return Some((name, None, node.child_by_field_name("right")));
        }
        if node.kind() != "property_declaration" {
            return None;
        }
        let vd = node
            .named_child(0)
            .filter(|c| c.kind() == "variable_declaration")?;
        let name_node = vd.named_child(0).filter(|c| c.kind() == "identifier")?;
        let name = name_node.utf8_text(src).ok()?.to_string();
        // `val x: T = ..` 的类型挂在 variable_declaration 里(identifier 之后)
        let ty = vd
            .named_child(1)
            .and_then(|t| t.utf8_text(src).ok())
            .map(super::outermost_type_name);
        // 初始化表达式:property_declaration 里 variable_declaration 之后的那个命名子
        let init = (1..node.named_child_count()).find_map(|i| node.named_child(i));
        Some((name, ty, init))
    }

    fn return_exprs<'a>(&self, node: Node<'a>, _src: &[u8]) -> Vec<Node<'a>> {
        match self.cfg_function_body(node) {
            Some(b) => super::return_exprs_in(self, b, "return_expression"),
            None => Vec::new(),
        }
    }

    /// `fun m(..): T` 的 `T` —— 在形参容器**之后**、函数体**之前**的那个命名子。
    fn return_type_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if node.kind() != "function_declaration" {
            return None;
        }
        let mut seen_params = false;
        for i in 0..node.named_child_count() {
            let c = node.named_child(i)?;
            match c.kind() {
                "function_value_parameters" => seen_params = true,
                "function_body" => break,
                _ if seen_params => {
                    return c
                        .utf8_text(src)
                        .ok()
                        .map(super::outermost_type_name)
                        .filter(|t| !t.is_empty())
                }
                _ => {}
            }
        }
        None
    }

    fn receiver_of(&self, node: Node, src: &[u8]) -> Option<String> {
        let nav = Self::navigation(node)?;
        let o = nav.named_child(0)?;
        super::access_path(o, src)
    }

    fn is_self_call(&self, node: Node, _src: &[u8]) -> bool {
        Self::navigation(node)
            .and_then(|n| n.named_child(0))
            .is_some_and(|o| o.kind() == "this_expression")
    }

    /// 🔴 Kotlin 的属性**全靠位置**:`property_declaration > variable_declaration(标识符, 类型)`。
    /// `class_body` 也是位置子,不是 `body` 字段。
    fn type_fields(&self, node: Node, src: &[u8]) -> Option<(String, Vec<(String, String)>)> {
        if node.kind() != "class_declaration" {
            return None;
        }
        let name = node
            .child_by_field_name("name")?
            .utf8_text(src)
            .ok()?
            .to_string();
        let body = (0..node.named_child_count())
            .filter_map(|i| node.named_child(i))
            .find(|c| c.kind() == "class_body")?;
        let mut out = Vec::new();
        let mut c = body.walk();
        for m in body.children(&mut c) {
            if m.kind() != "property_declaration" {
                continue;
            }
            let Some(vd) = (0..m.named_child_count())
                .filter_map(|i| m.named_child(i))
                .find(|c| c.kind() == "variable_declaration")
            else {
                continue;
            };
            let (Some(n), Some(t)) = (vd.named_child(0), vd.named_child(1)) else {
                continue; // 没写类型的属性跳过 —— 推断不是我们的活
            };
            if let (Ok(n), Ok(t)) = (n.utf8_text(src), t.utf8_text(src)) {
                out.push((n.to_string(), super::outermost_type_name(t)));
            }
        }
        // 🔴 **主构造器属性**:`class C(val idx: Index)` —— Kotlin 里这是声明字段的
        // **主要方式**,不认它这门的字段类型基本全丢(类体里根本没有对应的声明)。
        // ⚠ 判据只能看文本里有没有 `val` / `var`:当前 grammar 不把它们建成命名节点,
        //   而没有 `val`/`var` 的形参**只是形参、不是字段**,认错就是编。
        if let Some(pc) = (0..node.named_child_count())
            .filter_map(|i| node.named_child(i))
            .find(|c| c.kind() == "primary_constructor")
        {
            for p in descend_class_params(pc) {
                let Ok(text) = p.utf8_text(src) else { continue };
                if !text.split_whitespace().any(|w| w == "val" || w == "var") {
                    continue;
                }
                let names: Vec<Node> = (0..p.named_child_count())
                    .filter_map(|i| p.named_child(i))
                    .collect();
                let Some(id) = names.iter().find(|c| c.kind() == "identifier") else {
                    continue;
                };
                let Some(ty) = names.iter().find(|c| c.kind().ends_with("type")) else {
                    continue;
                };
                if let (Ok(n), Ok(t)) = (id.utf8_text(src), ty.utf8_text(src)) {
                    out.push((n.to_string(), super::outermost_type_name(t)));
                }
            }
        }
        Some((name, out))
    }

    fn is_opaque_scope(&self, node: Node) -> bool {
        matches!(
            node.kind(),
            "lambda_literal" | "function_declaration" | "anonymous_function"
        )
    }

    /// ⚠ Kotlin 的 `lambda_literal` **没有字段名** —— 形参容器只能按 kind 找。
    /// 另:`{ g(it) }` 这种隐式 `it` 不在这里认(它没有形参节点),
    /// 而 `it` 恰好与外层同名的情形极罕见,不为它编一个。
    fn receiver_name(&self) -> Option<&'static str> {
        Some("this")
    }

    fn closure_params(&self, node: Node, src: &[u8]) -> Option<Vec<Option<String>>> {
        matches!(node.kind(), "lambda_literal" | "anonymous_function").then(|| {
            let container = (0..node.named_child_count())
                .filter_map(|i| node.named_child(i))
                .find(|c| matches!(c.kind(), "lambda_parameters" | "function_value_parameters"));
            super::param_names_from(container, src, false)
        })
    }

    fn ctrl_role<'a>(&self, node: Node<'a>, _src: &[u8]) -> super::CtrlRole<'a> {
        use super::{CtrlRole, JumpKind};
        match node.kind() {
            // ⚠ Kotlin 的 if/循环**没有 body/consequence 字段名** —— 只有 `condition` 有名字,
            //    分支体是**无名的位置子节点**。按字段名取会拿到 None ⇒ 整个分支体被丢掉,
            //    而图看起来还很正常(3 个空块)。⇒ 一律按位置取。
            "if_expression" => {
                let cond = node.child_by_field_name("condition");
                let arms: Vec<Node> = positional_bodies(node, cond);
                let exhaustive = arms.len() >= 2; // 有 else 支
                CtrlRole::Branch {
                    cond,
                    arms,
                    exhaustive,
                }
            }
            "for_statement" | "while_statement" | "do_while_statement" => {
                let cond = node.child_by_field_name("condition");
                match positional_bodies(node, cond).pop() {
                    Some(body) => CtrlRole::Loop { cond, body },
                    None => CtrlRole::Unsupported("循环没有 body"),
                }
            }
            "try_expression" => {
                let Some(body) = node
                    .child_by_field_name("body")
                    .or_else(|| node.named_child(0))
                else {
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
            "return_expression" => CtrlRole::Return,
            "break_expression" | "continue_expression" => CtrlRole::Jump {
                kind: if node.kind() == "break_expression" {
                    JumpKind::Break
                } else {
                    JumpKind::Continue
                },
                labeled: node.named_child(0).is_some(),
            },
            // ⚠ `when` 的分支互斥、但穷尽性要看有没有 `else` 分支,而它的 AST 形态
            //    在 tree-sitter-kotlin-ng 里没核过 ⇒ 不建模(作废好过猜)
            "when_expression" => CtrlRole::Unsupported("Kotlin 的 when 表达式"),
            _ => CtrlRole::Plain,
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

/// 取一个节点里**除条件外**的位置子节点(Kotlin 的分支体/循环体都没有字段名)。
/// 只收 `block` 与表达式类节点,跳过 `for` 的迭代变量与区间。
fn positional_bodies<'a>(node: Node<'a>, cond: Option<Node<'a>>) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    for i in 0..node.named_child_count() {
        let c = node.named_child(i).expect("named_child_count 内");
        if Some(c.id()) == cond.map(|x| x.id()) {
            continue;
        }
        // `for (i in 1..3)` 的 `i` 与 `1..3` 不是循环体
        if matches!(c.kind(), "variable_declaration" | "range_expression") {
            continue;
        }
        out.push(c);
    }
    out
}

/// `primary_constructor` 底下的每个 `class_parameter`(中间还裹一层 `class_parameters`)。
fn descend_class_params(pc: Node) -> Vec<Node> {
    (0..pc.named_child_count())
        .filter_map(|i| pc.named_child(i))
        .filter(|c| c.kind() == "class_parameters")
        .flat_map(|cps| {
            (0..cps.named_child_count())
                .filter_map(move |i| cps.named_child(i))
                .filter(|c| c.kind() == "class_parameter")
        })
        .collect()
}

impl Kotlin {
    /// `call_expression` 的实参容器(**位置**子,没有字段名)。
    fn value_args(node: Node) -> Option<Node> {
        if node.kind() != "call_expression" {
            return None;
        }
        (0..node.named_child_count())
            .filter_map(|i| node.named_child(i))
            .find(|c| c.kind() == "value_arguments")
    }

    /// 调用的被调侧若是 `a.b` 这种导航表达式,给出它。
    fn navigation(node: Node) -> Option<Node> {
        if node.kind() != "call_expression" {
            return None;
        }
        node.named_child(0)
            .filter(|c| c.kind() == "navigation_expression")
    }
}
