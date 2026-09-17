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
}
