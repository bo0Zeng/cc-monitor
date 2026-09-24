//! **AST 层**:把一个符号对应的函数定义节点找出来,交给语言层建控制流图。
//!
//! 它存在的理由是**分层**:`engine` 是查询层,不该直接碰 `tree_sitter`。
//! 定位节点 + 读盘 + 转交语言层这三件事都住这里,`engine::cfg` 只是转发。
//!
//! 🔴 建不出正确的 CFG 就整份作废(`Cfg::unsupported`)—— 那条规矩住语言层
//! (见 `lang::rust_cfg`),这里只负责把它原样带出来,**不许在这一层"补救"**。

mod build;
pub(crate) use build::Builder;

use crate::lang::{self, LangSupport};
use crate::model::{Cfg, Lang, Symbol, SymbolId};
use crate::symbols;
use std::path::Path;

/// 给一个**函数定义节点**建 CFG 轮廓。**建图算法与语言契约分开** ——
/// 语言层只答分类(`ctrl_role` 等),构造走这里的通用 `Builder`。
///
/// `None` = 本门语言没实现 `cfg_function_body`(= 明确没做,不是「这函数没有控制流」)。
pub fn outline<'a>(
    spec: &dyn LangSupport,
    node: tree_sitter::Node,
    src: &'a [u8],
    ctx: Option<&'a crate::lang::FlowCtx<'a>>,
) -> Option<crate::lang::CfgOutline> {
    let body = spec.cfg_function_body(node)?;
    Some(Builder::new(spec, src, ctx).finish(body, node.end_position().row + 1))
}

/// 给一个符号建控制流图。`None` = 本门语言未实现 / 定位不到定义节点(索引陈旧)。
pub fn build(repo: &Path, sym: &SymbolId, s: &Symbol) -> Option<Cfg> {
    let lang = Lang::from_path(&s.file)?;
    let spec = lang::spec_for(lang)?;
    let src = std::fs::read_to_string(repo.join(&s.file)).ok()?;
    let tree = symbols::parse_with(lang, &src)?;
    let node = find_fn_node(spec, tree.root_node(), src.as_bytes(), s)?;
    // 查询口没有符号表与别名表 ⇒ 不缩窄也不归一(拿到的是更宽的那一档)
    let o = outline(spec, node, src.as_bytes(), None)?;
    Some(Cfg {
        function: sym.clone(),
        blocks: o.blocks,
        edges: o.edges,
        entry: o.entry,
        exit: o.exit,
        unsupported: o.unsupported,
    })
}

/// 在语法树里找到某符号对应的定义节点。按**名字 + 起始行**定位 ——
/// 行号来自索引,索引陈旧时可能找不到(返回 `None`,**不猜**)。
fn find_fn_node<'t>(
    spec: &dyn LangSupport,
    node: tree_sitter::Node<'t>,
    src: &[u8],
    sym: &Symbol,
) -> Option<tree_sitter::Node<'t>> {
    if node.start_position().row + 1 == sym.start_line {
        if let Some(def) = spec.symbol_at(node, src) {
            if def.name == sym.name {
                return Some(node);
            }
        }
    }
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if let Some(found) = find_fn_node(spec, ch, src, sym) {
            return Some(found);
        }
    }
    None
}
