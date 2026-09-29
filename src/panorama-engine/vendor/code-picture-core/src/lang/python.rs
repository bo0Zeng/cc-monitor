//! Python 的 `LangSupport`(F12)。`class_definition` 设类型限定;`function_definition`
//! 是可调用定义(在 class 内→Method,模块级→Function,由驱动按祖先限定定);
//! `call` 的 callee 为 `identifier`(自由)或 `attribute`(`obj.m()`,方法调用)。

use super::{dir_of, ext_candidates, join_normalized, LangSupport, SymbolDef};
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

    /// `import a.b` · `import a, b`(一条多模块)· `import a.b as c` · `from X import y`。
    fn import_of(&self, node: Node, src: &[u8]) -> Vec<String> {
        match node.kind() {
            "import_statement" => {
                let mut out = Vec::new();
                let mut cur = node.walk();
                for ch in node.children(&mut cur) {
                    let target = match ch.kind() {
                        "dotted_name" => Some(ch),
                        "aliased_import" => ch.child_by_field_name("name"), // `as` 前那截
                        _ => None,
                    };
                    if let Some(t) = target.and_then(|t| t.utf8_text(src).ok()) {
                        out.push(t.to_string());
                    }
                }
                out
            }
            // `from X import ...`:模块住 `module_name` 字段;相对导入原文带前导点。
            // ⚠ 不能按 kind 找 `dotted_name` —— 被导入的**名字**也是 `dotted_name`。
            "import_from_statement" => node
                .child_by_field_name("module_name")
                .and_then(|m| m.utf8_text(src).ok())
                .map(|s| vec![s.to_string()])
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// 前导点 = 相对导入:1 个点 = 本包目录,每多一个点上爬一级(PEP 328)。
    /// 无点 = 绝对导入,按仓库根试;`src/` 这类源根前缀由调用方的后缀匹配兜。
    fn resolve_import(&self, from_file: &str, specifier: &str) -> Vec<String> {
        let dots = specifier.chars().take_while(|c| *c == '.').count();
        let rest = specifier[dots..].replace('.', "/");
        let base = if dots == 0 {
            String::new()
        } else {
            let mut b = dir_of(from_file).to_string();
            for _ in 1..dots {
                b = dir_of(&b).to_string();
            }
            b
        };
        match join_normalized(&base, &rest) {
            Some(j) if !j.is_empty() => ext_candidates(&j, &["py", "pyi"], &["__init__"]),
            _ => Vec::new(),
        }
    }

    /// Python 没有 `let` —— **赋值就是唯一的绑定形式**。
    /// 先前没实现它 ⇒ Python 的局部类型表与来源表一直是空的,`flow` 在 Python 上
    /// 只追得到「原样传形参」那一种。
    ///
    /// `x: T = expr` 的标注住 `type` 字段;解构 / 属性写入(`s.f = ..`)一律不认。
    fn local_binding<'a>(
        &self,
        node: Node<'a>,
        src: &[u8],
    ) -> Option<(String, Option<String>, Option<Node<'a>>)> {
        if node.kind() != "assignment" {
            return None;
        }
        let lhs = node.child_by_field_name("left")?;
        // 访问路径(`s.f = ..`)也认;元组解构 / 下标写入不认
        let name = super::access_path(lhs, src)?;
        let ty = node
            .child_by_field_name("type")
            .and_then(|t| t.utf8_text(src).ok())
            .map(super::outermost_type_name);
        Some((name, ty, node.child_by_field_name("right")))
    }

    /// `self.m()` —— `attribute` 的 `object` 是名为 `self` 的 identifier。
    /// ⚠ `self` 只是**约定**(第一个参数叫什么都行),但违约的代码极少,
    /// 且认错的后果只是把一个本该精确的调用**算成歧义**,不会错报。
    fn is_self_call(&self, node: Node, src: &[u8]) -> bool {
        if node.kind() != "call" {
            return false;
        }
        node.child_by_field_name("function")
            .filter(|f| f.kind() == "attribute")
            .and_then(|f| f.child_by_field_name("object"))
            .and_then(|o| o.utf8_text(src).ok())
            .is_some_and(|t| t == "self")
    }

    /// ⚠ 接收者的判定在 Python 里**只能靠约定**(首参叫 `self` / `cls`)——
    /// 语言没给它专门的语法。判错会让参数位置**整体错位一格**,
    /// 那是会产出**假事实**的 ⇒ 首参名不是这两个之一时,一律**不跳**(宁可少对齐)。
    fn params_container<'a>(&self, node: Node<'a>, src: &[u8]) -> Option<(Node<'a>, bool)> {
        if node.kind() != "function_definition" {
            return None;
        }
        let params = node.child_by_field_name("parameters")?;
        let first_is_recv = params
            .named_child(0)
            .and_then(|p| p.utf8_text(src).ok())
            .is_some_and(|t| t == "self" || t == "cls");
        Some((params, first_is_recv))
    }

    fn call_args(&self, node: Node, src: &[u8]) -> Vec<Option<String>> {
        if node.kind() != "call" {
            return Vec::new();
        }
        // Python 没有借用语法 ⇒ 没有要剥的
        super::call_args_from(node.child_by_field_name("arguments"), src, &[])
    }

    fn call_arg_mentions(&self, node: Node, src: &[u8]) -> Vec<Vec<String>> {
        if node.kind() != "call" {
            return Vec::new();
        }
        super::arg_mentions_from(node.child_by_field_name("arguments"), src)
    }

    // ── 控制流分类 ──

    fn cfg_function_body<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        (node.kind() == "function_definition").then(|| node.child_by_field_name("body"))?
    }

    fn is_block(&self, node: Node) -> bool {
        node.kind() == "block"
    }

    fn is_opaque_scope(&self, node: Node) -> bool {
        matches!(node.kind(), "lambda" | "function_definition")
    }

    /// ⚠ Python 的**嵌套 `def`** 也遮蔽,但它常被抽成符号、走 `this_fn` 那条路;
    /// 这里只认 `lambda`(抽不成符号的那一形)。
    fn receiver_name(&self) -> Option<&'static str> {
        Some("self")
    }

    fn closure_params(&self, node: Node, src: &[u8]) -> Option<Vec<Option<String>>> {
        (node.kind() == "lambda")
            .then(|| super::param_names_from(node.child_by_field_name("parameters"), src, false))
    }

    fn ctrl_role<'a>(&self, node: Node<'a>, _src: &[u8]) -> super::CtrlRole<'a> {
        use super::{CtrlRole, JumpKind};
        match node.kind() {
            "if_statement" => {
                let mut arms = Vec::new();
                let mut exhaustive = false;
                if let Some(c) = node.child_by_field_name("consequence") {
                    arms.push(c);
                }
                let mut cur = node.walk();
                for ch in node.children(&mut cur) {
                    match ch.kind() {
                        // elif 自带条件;当成一条互斥分支即可 —— 分支间互斥这点是对的
                        "elif_clause" => {
                            if let Some(c) = ch.child_by_field_name("consequence") {
                                arms.push(c);
                            }
                        }
                        "else_clause" => {
                            exhaustive = true;
                            if let Some(b) = ch.child_by_field_name("body") {
                                arms.push(b);
                            }
                        }
                        _ => {}
                    }
                }
                CtrlRole::Branch {
                    cond: node.child_by_field_name("condition"),
                    arms,
                    exhaustive,
                }
            }
            "for_statement" | "while_statement" => match node.child_by_field_name("body") {
                Some(body) => CtrlRole::Loop {
                    cond: node.child_by_field_name("condition"),
                    body,
                },
                None => CtrlRole::Unsupported("循环没有 body"),
            },
            "try_statement" => {
                let Some(body) = node.child_by_field_name("body") else {
                    return CtrlRole::Unsupported("try 没有 body");
                };
                let mut cur = node.walk();
                let handlers: Vec<Node> = node
                    .children(&mut cur)
                    .filter(|c| matches!(c.kind(), "except_clause" | "finally_clause"))
                    .collect();
                CtrlRole::Guarded { body, handlers }
            }
            // `with` 只是个语句容器 —— 必须下钻,否则里面的 if/loop 全被吞掉
            "with_statement" => match node.child_by_field_name("body") {
                Some(body) => CtrlRole::Sequence { body },
                None => CtrlRole::Unsupported("with 没有 body"),
            },
            "return_statement" => CtrlRole::Return,
            "break_statement" => CtrlRole::Jump {
                kind: JumpKind::Break,
                labeled: false, // Python 没有标签跳转
            },
            "continue_statement" => CtrlRole::Jump {
                kind: JumpKind::Continue,
                labeled: false,
            },
            // ⚠ `match`(3.10+)的穷尽性要看有没有 `case _`,判起来不牢靠 ⇒ 不建模,整份作废
            "match_statement" => CtrlRole::Unsupported("Python 的 match 语句"),
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
