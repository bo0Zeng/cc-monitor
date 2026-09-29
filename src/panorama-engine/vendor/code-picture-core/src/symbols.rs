//! tree-sitter 符号提取(F11:语言无关驱动 + 按 `Lang` 分发到 `LangSupport`)。
//! id:方法 = `file#Type::name`(kind=Method),自由函数 = `file#name`(kind=Function);
//! 同 id 冲突时追加 `@行号` 消歧。`name` 保持裸名(供调用解析按名匹配)。

use crate::lang::{self, LangSupport, SymbolDef};
use crate::model::{FieldType, ImplRel, Lang, Slot, SymKind, Symbol};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use tree_sitter::{Node, Parser};

pub fn symbols_in_source(src: &str, file: &str) -> Vec<Symbol> {
    index_source(src, file).symbols
}

/// 一个文件解析一次的全部产出。
pub struct Indexed {
    pub symbols: Vec<Symbol>,
    /// 类型的字段类型标注(定接收者类型用)。见 [`FieldType`]。
    pub fields: Vec<FieldType>,
    /// 实现关系(动态派发用)。见 [`ImplRel`]。
    pub impls: Vec<ImplRel>,
    /// tree-sitter 对不支持/畸形构造产了 ERROR 节点(F18 覆盖信号)。
    pub has_error: bool,
}

/// 解析一次,把符号 · 字段类型 · 解析健康一并取回 ——
/// engine 因此不必为了三件事解析三遍。
pub fn index_source(src: &str, file: &str) -> Indexed {
    let empty = || Indexed {
        symbols: Vec::new(),
        fields: Vec::new(),
        impls: Vec::new(),
        has_error: false,
    };
    let Some(lang) = Lang::from_path(file) else {
        return empty();
    };
    // 该语言分类未实现(F12–F14 接入)
    let Some(spec) = lang::spec_for(lang) else {
        return empty();
    };
    let Some(tree) = parse_with(lang, src) else {
        return empty();
    };
    let has_error = tree.root_node().has_error();
    let mut symbols = Vec::new();
    let mut seen_ids = HashSet::new();
    collect(
        spec,
        tree.root_node(),
        src.as_bytes(),
        file,
        lang,
        None,
        &mut symbols,
        &mut seen_ids,
    );
    let mut fields = Vec::new();
    let mut impls = Vec::new();
    collect_types(
        spec,
        tree.root_node(),
        src.as_bytes(),
        &mut fields,
        &mut impls,
    );
    Indexed {
        symbols,
        fields,
        impls,
        has_error,
    }
}

/// 扫全文件,把类型声明的**字段类型标注**与**实现关系**一并收起来
/// (同一趟遍历,不为两件事各走一遍树)。
fn collect_types(
    spec: &dyn LangSupport,
    node: Node,
    src: &[u8],
    fields: &mut Vec<FieldType>,
    impls: &mut Vec<ImplRel>,
) {
    if let Some((owner, fs)) = spec.type_fields(node, src) {
        fields.extend(fs.into_iter().map(|(field, ty)| FieldType {
            owner: owner.clone(),
            field,
            ty,
        }));
    }
    if let Some((ty, traits)) = spec.impl_of(node, src) {
        impls.extend(traits.into_iter().map(|trait_name| ImplRel {
            type_name: ty.clone(),
            trait_name,
        }));
    }
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        collect_types(spec, ch, src, fields, impls);
    }
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
            // 返回类型名:局部变量定类型用(`let g = f()` ⇒ g 的类型 = f 的返回类型)
            return_type: spec.return_type_of(node, src),
            // 哪几个形参流到返回值:`let y = h(x, z)` 之后 y 的来源能缩到只有 x
            return_flow: return_flow_of(spec, node, src),
            // 函数摘要:写进形参所指对象的字段 —— 过程间传播靠它
            param_flow: param_flow_of(spec, node, src),
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

/// 算「哪几个形参的值会流到返回值」。
///
/// 顺 `return_exprs` 里提到的标识符,经函数体内的局部来源表追回形参位 ——
/// 与参数流同一套机制,只是终点从「被调的形参」换成「自己的返回值」。
///
/// ⚠ 与 `arg_flow` 同一种**宽**口径:被变换过的(`return a + 1`)照样算。
/// 这是刻意的 —— 它的用途是**缩窄**别处的来源集合,宽一点只会少缩,不会缩错。
fn return_flow_of(spec: &dyn lang::LangSupport, node: Node, src: &[u8]) -> Option<String> {
    let params = spec.param_names(node, src);
    if params.is_empty() {
        return None;
    }
    let rets = spec.return_exprs(node, src);
    if rets.is_empty() {
        return None;
    }
    let mut sources = std::collections::HashMap::new();
    // 建符号阶段还没有符号表 ⇒ 不缩窄(宽的那一档:只会多算,不会漏)
    lang::collect_local_sources(spec, node, src, None, &mut sources);
    let mut out: Vec<usize> = Vec::new();
    for r in rets {
        for id in lang::identifiers_in(r, src) {
            lang::param_origins(&id, &sources, &params, &mut out);
        }
    }
    out.sort_unstable();
    out.dedup();
    if out.is_empty() {
        return None;
    }
    Some(
        out.iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(","),
    )
}

/// 算**函数摘要**:「第 i 参的值写进了第 j 参所指对象的哪条字段」。
///
/// 纯语法、纯过程内,与 `return_flow_of` 同一套机制 —— 只是终点从「返回值」
/// 换成「另一个形参所指的对象」。
///
/// 判据:函数体里有一条**定义**,它的路径根是形参 `p_j` 且**带字段**(`dst.f`),
/// 而它的来源顺局部来源表追得回形参 `p_i` ⇒ 记 `i~j:f`。
///
/// 🔴 三条**不认**(认了就是猜):
/// * `dst = v` —— 重赋形参本身。值语义下传不出去;引用语义下要看是不是 `*dst = v`,
///   而那要懂每门的引用模型。带字段的写在九门里语义一致,只认那一种。
/// * `dst.push(v)` —— 方法可能改。要知道 `push` 干了什么才敢记,那是过程间的另一层。
/// * 歧义被调的摘要 —— 见 `graph::calls`,各候选摘要不同,挑哪个都是编。
///
/// ⚠ 建符号阶段**没有符号表** ⇒ 来源表不缩窄(宽的那一档:多算不漏算),
/// 且摘要此刻只到**一跳**。跨函数的传递闭包在 `graph` 里跑不动点补(见 `graph::summary`)。
fn param_flow_of(spec: &dyn lang::LangSupport, node: Node, src: &[u8]) -> Option<String> {
    // 槽位表 = `[接收者] ++ 形参`。接收者必须算一个 —— `fn set(&mut self, v){ self.f = v }`
    // 是最常见的那一形,不算它的话摘要在真实代码上几乎不响(实测本仓产出为 0)。
    let params = spec.param_names(node, src);
    let mut slots: Vec<Option<String>> = vec![spec.receiver_name().map(String::from)];
    slots.extend(params);
    // ⚠ 参数属性不受「至少两个槽位」这条限制:`constructor(private x: T)` 只有一个形参,
    //    但它确实把这个形参写进了接收者。
    if slots.iter().filter(|s| s.is_some()).count() < 2
        && spec.param_properties(node, src).is_empty()
    {
        return None; // 少于两个可辨的槽位,谈不上「这个写进那个」
    }
    let mut sources = std::collections::HashMap::new();
    lang::collect_local_sources(spec, node, src, None, &mut sources);
    let mut out: Vec<String> = Vec::new();
    // **参数属性**:`constructor(private theme: Theme)` 等价于 `this.theme = theme`,
    // 但函数体里没有这条赋值 —— 不认它,这一形的摘要整类抓不到。
    for (i, field) in spec.param_properties(node, src) {
        out.push(format!(
            "{}~{}:{field}",
            Slot::Param(i).token(),
            Slot::Recv.token()
        ));
    }
    for (target, srcs) in &sources {
        // 必须带字段:`dst.f` / `self.f` 认,裸 `dst` 不认
        let Some((root, field)) = target.split_once('.') else {
            continue;
        };
        let Some(j) = slots.iter().position(|p| p.as_deref() == Some(root)) else {
            continue;
        };
        let mut origins: Vec<usize> = Vec::new();
        for s in srcs {
            lang::param_origins(s, &sources, &slots, &mut origins);
        }
        origins.sort_unstable();
        origins.dedup();
        for i in origins {
            if i != j {
                out.push(format!(
                    "{}~{}:{field}",
                    Slot::from_index(i).token(),
                    Slot::from_index(j).token()
                ));
            }
        }
    }
    out.sort();
    out.dedup();
    (!out.is_empty()).then(|| out.join(","))
}
