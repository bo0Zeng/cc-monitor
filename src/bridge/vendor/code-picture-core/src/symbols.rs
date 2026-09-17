//! tree-sitter 符号提取(F11:语言无关驱动 + 按 `Lang` 分发到 `LangSupport`)。
//! id:方法 = `file#Type::name`(kind=Method),自由函数 = `file#name`(kind=Function);
//! 同 id 冲突时追加 `@行号` 消歧。`name` 保持裸名(供调用解析按名匹配)。

use crate::lang::{self, LangSupport, SymbolDef};
use crate::model::{Lang, SymKind, Symbol};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use tree_sitter::{Node, Parser};

pub fn symbols_in_source(src: &str, file: &str) -> Vec<Symbol> {
    index_source(src, file).0
}

/// 解析一次,返回 (符号, 是否解析不全)。`has_error` = tree-sitter 对不支持/畸形构造
/// 产了 ERROR 节点(F18 覆盖信号)。engine 用它避免"取符号 + 判 has_error"两次解析。
pub fn index_source(src: &str, file: &str) -> (Vec<Symbol>, bool) {
    let lang = match Lang::from_path(file) {
        Some(l) => l,
        None => return (Vec::new(), false),
    };
    let spec = match lang::spec_for(lang) {
        Some(s) => s,
        None => return (Vec::new(), false), // 该语言分类未实现(F12–F14 接入)
    };
    let tree = match parse_with(lang, src) {
        Some(t) => t,
        None => return (Vec::new(), false),
    };
    let has_error = tree.root_node().has_error();
    let mut out = Vec::new();
    let mut seen_ids = HashSet::new();
    collect(
        spec,
        tree.root_node(),
        src.as_bytes(),
        file,
        lang,
        None,
        &mut out,
        &mut seen_ids,
    );
    (out, has_error)
}

/// 按语言初始化解析器并解析(graph.rs 复用)。语言 grammar 见 `Lang::ts_language`。
pub(crate) fn parse_with(lang: Lang, src: &str) -> Option<tree_sitter::Tree> {
    let mut parser = Parser::new();
    parser.set_language(&lang.ts_language()).ok()?;
    parser.parse(src, None)
}

#[allow(clippy::too_many_arguments)]
fn collect(
    spec: &dyn LangSupport,
    node: Node,
    src: &[u8],
    file: &str,
    lang: Lang,
    qualifier: Option<&str>,
    out: &mut Vec<Symbol>,
    seen_ids: &mut HashSet<String>,
) {
    // 容器节点(impl/class/…)为其子节点设定类型限定
    let mut child_qual: Option<String> = qualifier.map(|s| s.to_string());
    if let Some(q) = spec.qualifier_of(node, src) {
        child_qual = Some(q);
    }

    if let Some(SymbolDef {
        name,
        qualifier: def_qual,
        kind: def_kind,
    }) = spec.symbol_at(node, src)
    {
        let start_line = node.start_position().row + 1;
        // 限定:定义**自带**的(如 C++ `A::method`)优先,否则回落祖先容器限定
        let qual = def_qual.as_deref().or(qualifier);
        // 种类:定义显式指定优先(如命名空间自由函数=Function、类容器=Class);
        // 否则按"最终限定有无"定 Method/Function(Rust 及多数语言够用)
        let kind = def_kind.unwrap_or(if qual.is_some() {
            SymKind::Method
        } else {
            SymKind::Function
        });
        // 可调用定义 → 体内嵌套定义视作自由;容器(class)→ 保留其为子设定的限定
        let is_callable = matches!(kind, SymKind::Function | SymKind::Method);
        let base_id = match qual {
            Some(q) => format!("{}#{}::{}", file, q, name),
            None => format!("{}#{}", file, name),
        };
        // 同 id 冲突 → 起始行消歧;若仍冲突(同行 3+ 同名,如单行多重载)再追加序号,
        // **保证唯一**,避免落库 PRIMARY KEY 冲突静默丢符号。
        let mut id = base_id;
        if seen_ids.contains(&id) {
            let base = id.clone();
            id = format!("{}@{}", base, start_line);
            let mut n = 2;
            while seen_ids.contains(&id) {
                id = format!("{}@{}#{}", base, start_line, n);
                n += 1;
            }
        }
        seen_ids.insert(id.clone());
        let body = &src[node.byte_range()];
        out.push(Symbol {
            id,
            name,
            file: file.to_string(),
            kind,
            lang,
            start_line,
            end_line: node.end_position().row + 1,
            // F68：签名文本（拿不到=None，如非标准 body 字段或非可调用符号）
            signature: spec.signature_of(node, src),
            body_hash: hash_bytes(body),
        });
        if is_callable {
            child_qual = None;
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect(
            spec,
            child,
            src,
            file,
            lang,
            child_qual.as_deref(),
            out,
            seen_ids,
        );
    }
}

fn hash_bytes(b: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.hash(&mut h);
    h.finish()
}
