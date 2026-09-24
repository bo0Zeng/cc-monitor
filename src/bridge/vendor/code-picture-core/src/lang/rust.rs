//! Rust 的 `LangSupport`(F11:从 symbols.rs/graph.rs 原逻辑平移,行为不变)。
//! `impl` 设类型限定;`function_item` 是可调用定义;`call_expression` 是调用点。

use super::{dir_of, join_normalized, LangSupport, SymbolDef};
use tree_sitter::Node;

pub(crate) struct Rust;
pub(crate) static RUST: Rust = Rust;

impl LangSupport for Rust {
    fn qualifier_of(&self, node: Node, src: &[u8]) -> Option<String> {
        // `impl T` / `impl Tr for T` ⇒ 方法归**类型** T;
        // `trait Tr` ⇒ 默认方法归 **trait** Tr —— 动态派发时它也是候选之一,
        // 不加限定的话它会以裸名混进「同名自由函数」里,谁也认不出它是谁的。
        if matches!(node.kind(), "impl_item" | "trait_item") {
            node.child_by_field_name("type")
                .or_else(|| node.child_by_field_name("name"))
                .and_then(|t| t.utf8_text(src).ok())
                .map(super::outermost_type_name)
        } else {
            None
        }
    }

    fn symbol_at(&self, node: Node, src: &[u8]) -> Option<SymbolDef> {
        if node.kind() == "function_item" {
            let name = node
                .child_by_field_name("name")?
                .utf8_text(src)
                .ok()?
                .to_string();
            // Rust:限定与 kind 都由驱动按祖先 `impl` 决定(自由函数 / 方法),定义不自带
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
        if node.kind() != "call_expression" {
            return None;
        }
        let callee = node.child_by_field_name("function")?;
        let is_method = callee.kind() == "field_expression";
        let (name, qual) = callee_name(callee, src)?;
        Some((name, qual, is_method))
    }

    /// `use_declaration` → 展开成**每个被引路径**。
    ///
    /// 🔴 **必须展开分组导入**:`use crate::{scan, symbols}` 的花括号前只有 `crate`,
    /// 不展开就一个模块都拿不到 —— 而分组导入在 Rust 里极常见(本仓 `engine.rs` 就有
    /// 一条一次引六个模块的)。
    ///
    /// ⚠ **`mod foo;` 不算 import** —— 它说的是「本文件**包含**模块 foo」,是包含不是依赖,
    /// 混进来会让依赖方向图凭空多出一堆边。
    fn import_of(&self, node: Node, src: &[u8]) -> Vec<String> {
        if node.kind() != "use_declaration" {
            return Vec::new();
        }
        let Some(arg) = node.child_by_field_name("argument") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        collect_use_paths(arg, src, "", &mut out);
        out
    }

    /// 只解 `crate::` / `super::` / `self::` 三种**仓内**前缀 ——
    /// 裸路径(`std::fs` · `serde::de`)是外部 crate,解不了也不该解。
    ///
    /// ⚠ 诚实边界:`use` 路径把**模块**与**条目**混在一起(`crate::model::Lang` 里
    /// `model` 是模块、`Lang` 是类型),静态分不开 ⇒ 逐段缩短去试,第一个命中的当模块。
    fn resolve_import(&self, from_file: &str, specifier: &str) -> Vec<String> {
        let segs: Vec<&str> = specifier.split("::").filter(|s| !s.is_empty()).collect();
        let (base, rest): (String, &[&str]) = match segs.first() {
            Some(&"crate") => match crate_root(from_file) {
                Some(r) => (r.to_string(), &segs[1..]),
                None => return Vec::new(), // 认不出 crate 根就不猜
            },
            Some(&"self") => (module_dir(from_file), &segs[1..]),
            Some(&"super") => {
                // 连续多个 `super::` 逐级上爬
                let mut dir = module_dir(from_file);
                let mut i = 0;
                while segs.get(i) == Some(&"super") {
                    dir = dir_of(&dir).to_string();
                    i += 1;
                }
                (dir, &segs[i..])
            }
            // 同工作区兄弟 crate:`use code_picture_core::model::X`。
            // crate 名用下划线、目录名惯例用连字符 ⇒ 两形都给;`crates/` 这类源根前缀
            // 不在 use 路径里,只能靠调用方的**后缀匹配**去命中。
            // ⚠ 外部 crate(`serde::` `std::`)也会走到这里 —— 但仓里没有同名目录,
            // 自然解不到,不会产假边。
            _ => return sibling_crate_candidates(&segs),
        };
        // 逐段缩短:`a::b::c` → a/b/c → a/b → a
        let mut out = Vec::new();
        for take in (1..=rest.len()).rev() {
            let joined = rest[..take].join("/");
            if let Some(pre) = join_normalized(&base, &joined) {
                out.push(format!("{pre}.rs"));
                out.push(format!("{pre}/mod.rs"));
            }
        }
        out
    }

    /// `self.m()`(`field_expression` 的接收者节点 kind 就是 `self`)
    /// 与 `Self::m()`(`scoped_identifier` 的 path 是 `Self`)。
    fn is_self_call(&self, node: Node, src: &[u8]) -> bool {
        if node.kind() != "call_expression" {
            return false;
        }
        let Some(callee) = node.child_by_field_name("function") else {
            return false;
        };
        match callee.kind() {
            "field_expression" => callee
                .child_by_field_name("value")
                .is_some_and(|v| v.kind() == "self"),
            "scoped_identifier" => callee
                .child_by_field_name("path")
                .and_then(|p| p.utf8_text(src).ok())
                .is_some_and(|t| t == "Self"),
            _ => false,
        }
    }

    /// `fn f() -> Graph` 的 `return_type` 字段。
    fn return_type_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if node.kind() != "function_item" {
            return None;
        }
        let t = node.child_by_field_name("return_type")?;
        Some(super::outermost_type_name(t.utf8_text(src).ok()?))
    }

    /// `let x: T = init;` —— 字段 `pattern` / `type` / `value`。
    fn local_binding<'a>(
        &self,
        node: Node<'a>,
        src: &[u8],
    ) -> Option<(String, Option<String>, Option<Node<'a>>)> {
        // 🔴 再赋值(`x = ..`)也是定义点 —— 到达定义只认 `let` 的话,
        // 「分支里各赋一次」这种极常见的写法整条追不到。
        if node.kind() == "assignment_expression" {
            let lhs = node.child_by_field_name("left")?;
            // 左边可以是**访问路径**(`s.f = ..`)—— 把 `s.f` 当扩展变量名即可,
            // 不需要知道 `s` 的类型(字段敏感只要「同一条路径」这个判据)。
            // 下标写入(`a[i] = ..`)仍不认:那要知道 i 的值域。
            let name = super::access_path(lhs, src)?;
            return Some((name, None, node.child_by_field_name("right")));
        }
        if node.kind() != "let_declaration" {
            return None;
        }
        let pat = node.child_by_field_name("pattern")?;
        // 只认最简单的 `let x = ..`;解构(`let (a, b) = ..`)不认 —— 认了就是猜
        if pat.kind() != "identifier" {
            return None;
        }
        let name = pat.utf8_text(src).ok()?.to_string();
        let ty = node
            .child_by_field_name("type")
            .and_then(|t| t.utf8_text(src).ok())
            .map(super::outermost_type_name);
        Some((name, ty, node.child_by_field_name("value")))
    }

    /// `g.run()` → `g`。`a.b().c()` 这种复杂接收者返回 None。
    fn receiver_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if node.kind() != "call_expression" {
            return None;
        }
        let callee = node.child_by_field_name("function")?;
        if callee.kind() != "field_expression" {
            return None;
        }
        super::access_path(callee.child_by_field_name("value")?, src)
    }

    /// `fn f(&self, a, b)` → `["a", "b"]`。接收者的判定是**精确的** ——
    /// tree-sitter-rust 给 `&self` 一个专门的节点 kind `self_parameter`,不是靠名字猜。
    fn params_container<'a>(&self, node: Node<'a>, _src: &[u8]) -> Option<(Node<'a>, bool)> {
        if node.kind() != "function_item" {
            return None;
        }
        let params = node.child_by_field_name("parameters")?;
        let has_self = params
            .named_child(0)
            .is_some_and(|p| p.kind() == "self_parameter");
        Some((params, has_self))
    }

    fn call_args(&self, node: Node, src: &[u8]) -> Vec<Option<String>> {
        if node.kind() != "call_expression" {
            return Vec::new();
        }
        // `&x` / `&mut x` 是借用,传的还是同一个值 ⇒ 剥掉再看
        super::call_args_from(
            node.child_by_field_name("arguments"),
            src,
            &["reference_expression"],
        )
    }

    fn call_arg_mentions(&self, node: Node, src: &[u8]) -> Vec<Vec<String>> {
        if node.kind() != "call_expression" {
            return Vec::new();
        }
        super::arg_mentions_from(node.child_by_field_name("arguments"), src)
    }

    // ── 控制流分类(建图算法住 crate::cfg::build,这里只答「这是什么角色」)──

    fn cfg_function_body<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        (node.kind() == "function_item").then(|| node.child_by_field_name("body"))?
    }

    fn is_block(&self, node: Node) -> bool {
        node.kind() == "block"
    }

    fn type_fields(&self, node: Node, src: &[u8]) -> Option<(String, Vec<(String, String)>)> {
        if node.kind() != "struct_item" {
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
        if node.kind() != "impl_item" {
            return None;
        }
        let ty = super::outermost_type_name(node.child_by_field_name("type")?.utf8_text(src).ok()?);
        // `impl T { .. }`(无 trait)不是实现关系 —— 给空表,不给 None
        let traits = node
            .child_by_field_name("trait")
            .and_then(|t| t.utf8_text(src).ok())
            .map(|t| vec![super::outermost_type_name(t)])
            .unwrap_or_default();
        Some((ty, traits))
    }

    fn is_opaque_scope(&self, node: Node) -> bool {
        matches!(node.kind(), "closure_expression" | "function_item")
    }

    fn receiver_name(&self) -> Option<&'static str> {
        Some("self")
    }

    fn closure_params(&self, node: Node, src: &[u8]) -> Option<Vec<Option<String>>> {
        (node.kind() == "closure_expression")
            .then(|| super::param_names_from(node.child_by_field_name("parameters"), src, false))
    }

    /// `?` 是隐式早退 —— 不建模的话 CFG 会声称 `let v = mk()?;` 之后的代码**一定**执行。
    fn has_implicit_exit(&self, node: Node) -> bool {
        fn has_try(n: Node) -> bool {
            if matches!(n.kind(), "closure_expression" | "function_item") {
                return false;
            }
            if n.kind() == "try_expression" {
                return true;
            }
            let mut c = n.walk();
            let found = n.children(&mut c).any(has_try);
            found
        }
        has_try(node)
    }

    fn ctrl_role<'a>(&self, node: Node<'a>, src: &[u8]) -> super::CtrlRole<'a> {
        use super::{CtrlRole, JumpKind};
        // 语句裹着表达式:剥掉 expression_statement 看真身
        let n = if node.kind() == "expression_statement" {
            node.named_child(0).unwrap_or(node)
        } else {
            node
        };
        match n.kind() {
            "if_expression" => {
                let mut arms = Vec::new();
                if let Some(c) = n.child_by_field_name("consequence") {
                    arms.push(c);
                }
                // else 可能是 block,也可能是 `else if`(照旧是个 if_expression)
                let alt = n
                    .child_by_field_name("alternative")
                    .and_then(|a| a.named_child(0).or(Some(a)));
                let exhaustive = alt.is_some();
                if let Some(a) = alt {
                    arms.push(a);
                }
                CtrlRole::Branch {
                    cond: n.child_by_field_name("condition"),
                    arms,
                    exhaustive,
                }
            }
            "match_expression" => {
                let Some(mb) = n.child_by_field_name("body") else {
                    return CtrlRole::Unsupported("match 没有 body");
                };
                let mut c = mb.walk();
                let arms: Vec<Node> = mb
                    .children(&mut c)
                    .filter(|x| x.kind() == "match_arm")
                    .filter_map(|a| a.child_by_field_name("value"))
                    .collect();
                // Rust 的 match 必须穷尽 ⇒ 没有「全不中」那条路
                CtrlRole::Branch {
                    cond: n.child_by_field_name("value"),
                    arms,
                    exhaustive: true,
                }
            }
            "loop_expression" | "while_expression" | "for_expression" => {
                match n.child_by_field_name("body") {
                    Some(body) => CtrlRole::Loop {
                        cond: n
                            .child_by_field_name("condition")
                            .or_else(|| n.child_by_field_name("value")),
                        body,
                    },
                    None => CtrlRole::Unsupported("循环没有 body"),
                }
            }
            "return_expression" => CtrlRole::Return,
            "break_expression" | "continue_expression" => CtrlRole::Jump {
                kind: if n.kind() == "break_expression" {
                    JumpKind::Break
                } else {
                    JumpKind::Continue
                },
                labeled: n.named_child(0).map(|c| c.kind()) == Some("label"),
            },
            _ => {
                let _ = src;
                CtrlRole::Plain
            }
        }
    }

    /// Rust 有两种返回:显式 `return e`,以及**块的尾表达式**(没有分号的最后一项)。
    /// 只取尾表达式那一条就会漏掉所有 early return。
    fn return_exprs<'a>(&self, node: Node<'a>, _src: &[u8]) -> Vec<Node<'a>> {
        fn walk<'a>(n: Node<'a>, out: &mut Vec<Node<'a>>) {
            if matches!(n.kind(), "closure_expression" | "function_item") && !out.is_empty() {
                return; // 不下钻嵌套作用域(它的 return 是它自己的)
            }
            if n.kind() == "return_expression" {
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
        // 尾表达式:块的最后一个命名子,且不是语句
        let mut c = body.walk();
        if let Some(last) = body.children(&mut c).filter(|x| x.is_named()).last() {
            if !last.kind().ends_with("_statement") && last.kind() != "expression_statement" {
                out.push(last);
            }
        }
        out
    }
}

/// 递归展开 use 树:`use a::{b, c::d}` → `["a::b", "a::c::d"]`。
/// 前缀在下钻时累加,所以嵌套分组(`use a::{b::{c, d}}`)也展得开。
fn collect_use_paths(node: Node, src: &[u8], prefix: &str, out: &mut Vec<String>) {
    fn join(p: &str, s: &str) -> String {
        if p.is_empty() {
            s.to_string()
        } else {
            format!("{p}::{s}")
        }
    }
    match node.kind() {
        "scoped_use_list" => {
            let head = node
                .child_by_field_name("path")
                .and_then(|p| p.utf8_text(src).ok())
                .unwrap_or("");
            let inner = join(prefix, head);
            if let Some(list) = super::child_by_kind(node, "use_list") {
                let mut c = list.walk();
                for ch in list.children(&mut c) {
                    if ch.is_named() {
                        collect_use_paths(ch, src, &inner, out);
                    }
                }
            }
        }
        "use_list" => {
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.is_named() {
                    collect_use_paths(ch, src, prefix, out);
                }
            }
        }
        // `use a::b as c` → 取 as 前的路径
        "use_as_clause" => {
            if let Some(p) = node.child_by_field_name("path") {
                collect_use_paths(p, src, prefix, out);
            }
        }
        // `use a::b::*` → 目标是 a::b
        "use_wildcard" => {
            if let Some(h) = super::child_by_kind(node, "scoped_identifier")
                .or_else(|| super::child_by_kind(node, "identifier"))
                .and_then(|n| n.utf8_text(src).ok())
            {
                out.push(join(prefix, h));
            }
        }
        "scoped_identifier" | "identifier" | "crate" | "self" | "super" => {
            if let Ok(t) = node.utf8_text(src) {
                out.push(join(prefix, t));
            }
        }
        _ => {}
    }
}

/// 同工作区兄弟 crate 的候选路径(后缀形)。crate 名 `a_b` ↔ 目录名 `a-b` 两形都给。
fn sibling_crate_candidates(segs: &[&str]) -> Vec<String> {
    let Some(first) = segs.first() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let hyphen = first.replace('_', "-");
    let names: Vec<&str> = if hyphen == *first {
        vec![first]
    } else {
        vec![&hyphen, first]
    };
    for name in names {
        for take in (1..segs.len()).rev() {
            let joined = segs[1..=take].join("/");
            out.push(format!("{name}/src/{joined}.rs"));
            out.push(format!("{name}/src/{joined}/mod.rs"));
        }
        out.push(format!("{name}/src/lib.rs"));
    }
    out
}

/// crate 根目录(含末尾 `/` 的前缀)。按 Cargo 标准布局从路径里认 `src/` 那一段。
/// 认不出来 → None(不猜)。
fn crate_root(from_file: &str) -> Option<&str> {
    if let Some(i) = from_file.rfind("/src/") {
        return Some(&from_file[..i + 5]);
    }
    if from_file.starts_with("src/") {
        return Some("src/");
    }
    None
}

/// 当前文件所在**模块**的目录(`self::` 的基准)。
/// `lib.rs` / `main.rs` / `mod.rs` 的模块目录就是它自己的目录;
/// 其余文件 `x.rs` 的子模块住 `x/`(Rust 2018 布局)。
fn module_dir(from_file: &str) -> String {
    let dir = dir_of(from_file);
    let stem = from_file
        .rsplit('/')
        .next()
        .unwrap_or(from_file)
        .strip_suffix(".rs")
        .unwrap_or("");
    if matches!(stem, "lib" | "main" | "mod") {
        dir.to_string()
    } else if dir.is_empty() {
        stem.to_string()
    } else {
        format!("{dir}/{stem}")
    }
}

/// 从 callee 节点抽 (名字, 作用域限定?)。(平移自原 graph.rs)
fn callee_name(callee: Node, src: &[u8]) -> Option<(String, Option<String>)> {
    match callee.kind() {
        "identifier" => callee.utf8_text(src).ok().map(|s| (s.to_string(), None)),
        "scoped_identifier" => {
            let name = callee
                .child_by_field_name("name")?
                .utf8_text(src)
                .ok()?
                .to_string();
            let qual = callee
                .child_by_field_name("path")
                .and_then(|p| p.utf8_text(src).ok())
                // module::Type → 取最后一段作为类型限定
                .map(|s| s.rsplit("::").next().unwrap_or(s).to_string());
            Some((name, qual))
        }
        "field_expression" => {
            let f = callee.child_by_field_name("field")?;
            f.utf8_text(src).ok().map(|s| (s.to_string(), None))
        }
        _ => None,
    }
}
