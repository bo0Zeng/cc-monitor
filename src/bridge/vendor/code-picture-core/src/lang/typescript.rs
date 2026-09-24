//! TypeScript 的 `LangSupport`(F12)。tree-sitter-typescript 的函数/类/调用节点与
//! JavaScript 同源,故直接复用 `javascript` 的分类逻辑。TS 特有的 `interface` 内
//! `method_signature`(无函数体)不产可调用符号——`symbol_at` 不匹配它,自然跳过。
//! (TS 类字段箭头 `foo = () => {}` 用 `public_field_definition` 承载,留后细化,记债。)

use super::javascript;
use super::{LangSupport, SymbolDef};
use tree_sitter::Node;

pub(crate) struct Typescript;
pub(crate) static TYPESCRIPT: Typescript = Typescript;

impl LangSupport for Typescript {
    fn qualifier_of(&self, node: Node, src: &[u8]) -> Option<String> {
        javascript::qualifier_of(node, src)
    }
    fn symbol_at(&self, node: Node, src: &[u8]) -> Option<SymbolDef> {
        javascript::symbol_at(node, src)
    }
    fn call_of(&self, node: Node, src: &[u8]) -> Option<(String, Option<String>, bool)> {
        javascript::call_of(node, src)
    }
    /// F68：TS 沿用 JS 的箭头 override（TS grammar 的箭头赋值/类字段箭头同 JS 形态）。
    fn signature_of(&self, node: Node, src: &[u8]) -> Option<String> {
        javascript::JAVASCRIPT.signature_of(node, src)
    }

    /// TS 的 import 与 JS 同源(`import type` 只多一个 `type` 关键字,`source` 字段不变)。
    fn import_of(&self, node: Node, src: &[u8]) -> Vec<String> {
        javascript::JAVASCRIPT.import_of(node, src)
    }
    fn resolve_import(&self, from_file: &str, specifier: &str) -> Vec<String> {
        javascript::JAVASCRIPT.resolve_import(from_file, specifier)
    }

    /// TS 的 `this.m()` 与 JS 同形。
    fn is_self_call(&self, node: Node, src: &[u8]) -> bool {
        javascript::JAVASCRIPT.is_self_call(node, src)
    }

    /// TS 的 `function f(): Graph` —— `return_type` 字段是个 `type_annotation`(`: Graph`)。
    fn return_type_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if !matches!(
            node.kind(),
            "function_declaration" | "method_definition" | "function_signature"
        ) {
            return None;
        }
        let t = node.child_by_field_name("return_type")?;
        Some(super::outermost_type_name(t.utf8_text(src).ok()?))
    }
    fn local_binding<'a>(
        &self,
        node: Node<'a>,
        src: &[u8],
    ) -> Option<(String, Option<String>, Option<Node<'a>>)> {
        javascript::JAVASCRIPT.local_binding(node, src)
    }
    fn receiver_of(&self, node: Node, src: &[u8]) -> Option<String> {
        javascript::JAVASCRIPT.receiver_of(node, src)
    }

    fn params_container<'a>(&self, node: Node<'a>, src: &[u8]) -> Option<(Node<'a>, bool)> {
        javascript::JAVASCRIPT.params_container(node, src)
    }
    fn call_args(&self, node: Node, src: &[u8]) -> Vec<Option<String>> {
        javascript::JAVASCRIPT.call_args(node, src)
    }

    fn call_arg_mentions(&self, node: Node, src: &[u8]) -> Vec<Vec<String>> {
        javascript::JAVASCRIPT.call_arg_mentions(node, src)
    }

    // ── 控制流分类:TS 与 JS 同形,整族转发 ──

    fn cfg_function_body<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        javascript::JAVASCRIPT.cfg_function_body(node)
    }
    fn is_block(&self, node: Node) -> bool {
        javascript::JAVASCRIPT.is_block(node)
    }
    fn type_fields(&self, node: Node, src: &[u8]) -> Option<(String, Vec<(String, String)>)> {
        if !matches!(node.kind(), "class_declaration" | "interface_declaration") {
            return None;
        }
        let (name, mut fields) = super::type_fields_from(
            node,
            src,
            node.child_by_field_name("body"),
            &["public_field_definition", "property_signature"],
        )?;
        // **参数属性**:`constructor(private theme: Theme)` 同时就是字段声明。
        // 类体里**没有**对应的 `public_field_definition`,不在这儿收就整类丢掉。
        if let Some(ctor) = Self::constructor_of(node, src) {
            let names = self.param_names(ctor, src);
            let types = self.param_types(ctor, src);
            for (i, f) in Self::param_property_positions(ctor, src) {
                if let (Some(Some(_)), Some(Some(t))) = (names.get(i), types.get(i)) {
                    fields.push((f, t.clone()));
                }
            }
        }
        Some((name, fields))
    }

    fn param_properties(&self, node: Node, src: &[u8]) -> Vec<(usize, String)> {
        Self::param_property_positions(node, src)
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
        // `class_heritage` 是**位置**子(没有字段名),`implements` / `extends` 都挂在它下面
        let mut traits = Vec::new();
        for i in 0..node.named_child_count() {
            if let Some(h) = node.named_child(i).filter(|c| c.kind() == "class_heritage") {
                super::collect_type_names(h, src, &mut traits);
            }
        }
        Some((name, traits))
    }

    fn is_opaque_scope(&self, node: Node) -> bool {
        javascript::JAVASCRIPT.is_opaque_scope(node)
    }
    fn receiver_name(&self) -> Option<&'static str> {
        javascript::JAVASCRIPT.receiver_name()
    }

    fn closure_params(&self, node: Node, src: &[u8]) -> Option<Vec<Option<String>>> {
        javascript::JAVASCRIPT.closure_params(node, src)
    }
    fn ctrl_role<'a>(&self, node: Node<'a>, src: &[u8]) -> super::CtrlRole<'a> {
        javascript::JAVASCRIPT.ctrl_role(node, src)
    }

    fn return_exprs<'a>(&self, node: Node<'a>, src: &[u8]) -> Vec<Node<'a>> {
        javascript::JAVASCRIPT.return_exprs(node, src)
    }
}

impl Typescript {
    /// 类体里的 `constructor` 方法。
    fn constructor_of<'a>(class_node: Node<'a>, src: &[u8]) -> Option<Node<'a>> {
        let body = class_node.child_by_field_name("body")?;
        (0..body.named_child_count())
            .filter_map(|i| body.named_child(i))
            .find(|m| {
                m.kind() == "method_definition"
                    && m.child_by_field_name("name")
                        .and_then(|n| n.utf8_text(src).ok())
                        .is_some_and(|t| t == "constructor")
            })
    }

    /// 构造器形参里哪些是**参数属性**:`(形参位, 字段名)`。
    ///
    /// ⚠ 判据是带 `accessibility_modifier`(`private` / `public` / `protected`)。
    /// 单写 `readonly` 的那一形当前 grammar 不单独标记 ⇒ **认不出**,写在这儿免得以后当 bug 查。
    fn param_property_positions(ctor: Node, src: &[u8]) -> Vec<(usize, String)> {
        let Some(params) = ctor.child_by_field_name("parameters") else {
            return Vec::new();
        };
        (0..params.named_child_count())
            .filter_map(|i| {
                let p = params.named_child(i)?;
                let mut c = p.walk();
                let is_prop = p
                    .children(&mut c)
                    .any(|ch| ch.kind() == "accessibility_modifier");
                if !is_prop {
                    return None;
                }
                let name = p
                    .child_by_field_name("pattern")
                    .filter(|n| n.kind() == "identifier")?
                    .utf8_text(src)
                    .ok()?
                    .to_string();
                Some((i, name))
            })
            .collect()
    }
}
