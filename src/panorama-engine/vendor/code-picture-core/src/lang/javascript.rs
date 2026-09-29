//! JavaScript 的 `LangSupport`(F12)。分类逻辑抽成 `pub(super)` 函数,TypeScript 复用
//! (两者 grammar 节点种类同源)。
//! `class_declaration` 设限定;函数定义有三种形态:`function_declaration`、类内
//! `method_definition`、以及**赋给变量的箭头/函数表达式**(`const f = () => {}`)——
//! 后者符号挂在 `variable_declarator` 上抓名(**不**再匹配内层 arrow,避免双符号)。
//! 调用 `call_expression`:callee 为 `identifier`(自由)或 `member_expression`(`obj.m()`)。

use super::{dir_of, ext_candidates, field_text, join_normalized, LangSupport, SymbolDef};
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
    /// F68：箭头/函数表达式赋值（`const f = () => {}` / 类字段 `h = () => {}`）的 body 藏在
    /// `value` 字段下的 arrow/function_expression 里，默认 helper 在顶层节点找不到 body 字段
    /// → 恒 None。这里剥一层 value → 取内层函数 body 前的文本（含名+参数+`=>`）。TS 委托到此。
    fn signature_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if matches!(
            node.kind(),
            "variable_declarator" | "field_definition" | "public_field_definition"
        ) {
            if let Some(inner) = node.child_by_field_name("value") {
                if let Some(body) = inner.child_by_field_name("body") {
                    return super::signature_before(node, body, src);
                }
            }
        }
        super::signature_before_body(node, src)
    }

    /// `import ... from 's'` · `export ... from 's'`(再导出也是真依赖)· `require('s')`。
    fn import_of(&self, node: Node, src: &[u8]) -> Vec<String> {
        let source = match node.kind() {
            // `export function f(){}` 没有 source 字段 → None → 照常下钻
            "import_statement" | "export_statement" => node.child_by_field_name("source"),
            "call_expression" => {
                let is_require = node
                    .child_by_field_name("function")
                    .and_then(|f| f.utf8_text(src).ok())
                    .is_some_and(|t| t == "require");
                if !is_require {
                    return Vec::new();
                }
                node.child_by_field_name("arguments")
                    .and_then(|a| super::child_by_kind(a, "string"))
            }
            _ => return Vec::new(),
        };
        source
            .and_then(|s| super::child_by_kind(s, "string_fragment"))
            .and_then(|f| f.utf8_text(src).ok())
            .map(|t| vec![t.to_string()])
            .unwrap_or_default()
    }

    /// 只解**相对**说明符(`./` `../`)—— 裸说明符(`react`)是 node_modules 依赖,不在仓内。
    /// ⚠ ESM 规矩要求写出扩展名,而 TS 源惯例写成 `.js` ⇒ 带扩展名时**回试同名其它扩展**,
    /// 否则 `import './a.js'` 指向 `a.ts` 会解不出来。
    fn resolve_import(&self, from_file: &str, specifier: &str) -> Vec<String> {
        const EXTS: [&str; 6] = ["ts", "tsx", "js", "jsx", "mjs", "cjs"];
        if !specifier.starts_with('.') {
            return Vec::new();
        }
        let Some(joined) = join_normalized(dir_of(from_file), specifier) else {
            return Vec::new();
        };
        if let Some((stem, ext)) = joined.rsplit_once('.') {
            if EXTS.contains(&ext) {
                let mut out = vec![joined.clone()];
                out.extend(ext_candidates(stem, &EXTS, &[]));
                return out;
            }
        }
        ext_candidates(&joined, &EXTS, &["index"])
    }

    /// `this.m()` —— `member_expression` 的 `object` 是 `this`。
    fn is_self_call(&self, node: Node, _src: &[u8]) -> bool {
        if node.kind() != "call_expression" {
            return false;
        }
        node.child_by_field_name("function")
            .filter(|f| f.kind() == "member_expression")
            .and_then(|f| f.child_by_field_name("object"))
            .is_some_and(|o| o.kind() == "this")
    }

    /// JS 没有返回类型标注 —— 恒 None(TS 覆盖它)。
    /// `const x = init` —— `lexical_declaration` / `variable_declaration` 里的
    /// `variable_declarator`,字段 `name` / `type` / `value`。
    fn local_binding<'a>(
        &self,
        node: Node<'a>,
        src: &[u8],
    ) -> Option<(String, Option<String>, Option<Node<'a>>)> {
        // 再赋值也是定义点(理由同 Rust:只认声明会漏掉分支里各赋一次的写法)
        if node.kind() == "assignment_expression" {
            let lhs = node.child_by_field_name("left")?;
            // 左边可以是访问路径(`s.f = ..`);下标写入仍不认
            let name = super::access_path(lhs, src)?;
            return Some((name, None, node.child_by_field_name("right")));
        }
        if node.kind() != "variable_declarator" {
            return None;
        }
        let name_node = node.child_by_field_name("name")?;
        if name_node.kind() != "identifier" {
            return None; // 解构不认
        }
        let name = name_node.utf8_text(src).ok()?.to_string();
        let ty = node
            .child_by_field_name("type")
            .and_then(|t| t.utf8_text(src).ok())
            .map(super::outermost_type_name);
        Some((name, ty, node.child_by_field_name("value")))
    }

    fn receiver_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if node.kind() != "call_expression" {
            return None;
        }
        let callee = node.child_by_field_name("function")?;
        if callee.kind() != "member_expression" {
            return None;
        }
        super::access_path(callee.child_by_field_name("object")?, src)
    }

    /// JS/TS 的方法没有显式接收者形参 ⇒ 不跳。
    fn params_container<'a>(&self, node: Node<'a>, _src: &[u8]) -> Option<(Node<'a>, bool)> {
        if !matches!(
            node.kind(),
            "function_declaration"
                | "function_expression"
                | "method_definition"
                | "arrow_function"
                | "generator_function_declaration"
        ) {
            return None;
        }
        // `x => ..` 走 `parameter`(直接是标识符),其余走 `parameters`
        let params = node
            .child_by_field_name("parameters")
            .or_else(|| node.child_by_field_name("parameter"))?;
        Some((params, false))
    }

    fn call_args(&self, node: Node, src: &[u8]) -> Vec<Option<String>> {
        if node.kind() != "call_expression" {
            return Vec::new();
        }
        // JS/TS 同样没有借用语法
        super::call_args_from(node.child_by_field_name("arguments"), src, &[])
    }

    fn call_arg_mentions(&self, node: Node, src: &[u8]) -> Vec<Vec<String>> {
        if node.kind() != "call_expression" {
            return Vec::new();
        }
        super::arg_mentions_from(node.child_by_field_name("arguments"), src)
    }

    // ── 控制流分类(TS 复用)──

    fn cfg_function_body<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        matches!(
            node.kind(),
            "function_declaration" | "function_expression" | "method_definition"
        )
        .then(|| node.child_by_field_name("body"))?
    }

    fn is_block(&self, node: Node) -> bool {
        node.kind() == "statement_block"
    }

    fn is_opaque_scope(&self, node: Node) -> bool {
        matches!(
            node.kind(),
            "arrow_function" | "function_expression" | "function_declaration"
        )
    }

    fn receiver_name(&self) -> Option<&'static str> {
        Some("this")
    }

    fn closure_params(&self, node: Node, src: &[u8]) -> Option<Vec<Option<String>>> {
        self.is_opaque_scope(node).then(|| {
            // `x => ..` 走 `parameter`(直接是标识符),`(a, b) => ..` 走 `parameters`
            super::param_names_from(
                node.child_by_field_name("parameters")
                    .or_else(|| node.child_by_field_name("parameter")),
                src,
                false,
            )
        })
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
                    // `else` 子句裹着真身(block 或 else-if)
                    a.named_child(0).unwrap_or(a)
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
            "for_statement" | "for_in_statement" | "while_statement" | "do_statement" => {
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
                let mut cur = node.walk();
                let handlers: Vec<Node> = node
                    .children(&mut cur)
                    .filter(|c| matches!(c.kind(), "catch_clause" | "finally_clause"))
                    .filter_map(|c| c.child_by_field_name("body").or(Some(c)))
                    .collect();
                CtrlRole::Guarded { body, handlers }
            }
            "statement_block" => CtrlRole::Sequence { body: node },
            "return_statement" => CtrlRole::Return,
            "break_statement" | "continue_statement" => CtrlRole::Jump {
                kind: if node.kind() == "break_statement" {
                    JumpKind::Break
                } else {
                    JumpKind::Continue
                },
                // JS 的 `break label;` —— 带标签就作废,同 Rust
                labeled: node.named_child(0).map(|c| c.kind()) == Some("statement_identifier"),
            },
            // ⚠ switch 的 fall-through 语义(没 break 就往下掉)使分支**不互斥**,
            //   而通用构造器的 Branch 假定互斥 ⇒ 不建模,整份作废。
            "switch_statement" => CtrlRole::Unsupported("switch(fall-through 语义)"),
            _ => CtrlRole::Plain,
        }
    }

    /// 只有显式 `return`(无尾表达式语义)。
    fn return_exprs<'a>(&self, node: Node<'a>, _src: &[u8]) -> Vec<Node<'a>> {
        fn walk<'a>(n: Node<'a>, out: &mut Vec<Node<'a>>) {
            if n.kind() == "return_statement" {
                if let Some(v) = n.named_child(0) {
                    out.push(v);
                }
            }
            let mut c = n.walk();
            for ch in n.children(&mut c) {
                walk(ch, out);
            }
        }
        let Some(body) = node.child_by_field_name("body") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        walk(body, &mut out);
        out
    }
}
