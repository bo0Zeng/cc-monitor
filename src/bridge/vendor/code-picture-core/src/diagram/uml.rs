//! **UML 类图**:类型当类,`impls` 当实现/继承,`field_types` 当组合。
//!
//! 这三样都是**声明里写着的**,不是推断出来的:
//! `impl Tr for A` · `struct E { idx: Index }` · `class A implements B`。
//!
//! ⚠ 只画**两端都在仓内**的关系 —— 指向外部类型(`Vec`/`String`)的字段不画成关系,
//! 那会让图被 stdlib 淹掉,而且它们也不是这个仓的结构(字段本身仍列在类里)。
//!
//! 2026-09-24:先出**结构**([`uml_graph`]),Mermaid 只是它的一个渲染 ——
//! 此前只有字符串,第三方想下钻(点一个类型 → 它的方法)只能去解析 Mermaid。

use super::shape::{Honesty, Omitted};
use crate::model::{FieldType, ImplRel, SymKind, Symbol, SymbolId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// 一个类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeNode {
    /// 图里的 id(Mermaid 能吃的字符;由名字派生,人能读)。
    pub id: String,
    /// 声明里的类型名。
    pub name: String,
    /// 若仓内有同名的**类型符号**(`SymKind::Class`),这是它的 id —— 给下钻用。
    /// `None` = 这个类型只从方法限定名 / 字段 / 实现关系里认出来,没有独立符号。
    pub symbol: Option<SymbolId>,
    /// 全部字段(按名升序;类型在仓外的也列 —— 它是这个类的组成)。
    pub fields: Vec<TypeField>,
    /// 全部方法(按名、id 升序),连符号 id。
    pub methods: Vec<TypeMethod>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeField {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeMethod {
    pub name: String,
    pub symbol: SymbolId,
}

/// 类型之间的一条关系。`from`/`to` 是 [`TypeNode::id`]。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeRelation {
    pub from: String,
    pub to: String,
    pub kind: TypeRelKind,
    /// 组合关系的字段名;实现关系为 `None`。
    pub label: Option<String>,
}

/// 关系种类 —— 都来自声明。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeRelKind {
    /// `from` 实现 / 继承 `to`(`impl Tr for A` ⇒ A → Tr)。
    Implements,
    /// `from` 有一个类型为 `to` 的字段。
    Composes,
}

/// [`uml_graph`] 的产出:画出的类型与关系,加上没画多少。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UmlGraph {
    pub types: Vec<TypeNode>,
    pub relations: Vec<TypeRelation>,
    /// 没画的类型数(`nodes`)、它们的方法数(`symbols`)、因一端没画而省掉的关系数(`links`)。
    pub omitted: Omitted,
}

/// 构图:按「方法数 + 字段数」取最大的 `max_types` 个类型,确定性排序。
pub fn uml_graph(
    symbols: &[Symbol],
    impls: &[ImplRel],
    fields: &[FieldType],
    max_types: usize,
) -> UmlGraph {
    // 仓内有哪些类型:能当限定名的(有方法的)+ 有字段的 + 参与实现关系的
    let mut methods: HashMap<&str, Vec<TypeMethod>> = HashMap::new();
    for s in symbols {
        let seg = s.id.rsplit('#').next().unwrap_or(&s.id);
        let seg = seg.split('@').next().unwrap_or(seg);
        if let Some((ty, m)) = seg.rsplit_once("::") {
            methods.entry(ty).or_default().push(TypeMethod {
                name: m.to_string(),
                symbol: s.id.clone(),
            });
        }
    }
    let mut known: HashSet<&str> = methods.keys().copied().collect();
    for f in fields {
        known.insert(f.owner.as_str());
    }
    for i in impls {
        known.insert(i.type_name.as_str());
        known.insert(i.trait_name.as_str());
    }
    let mut score: HashMap<&str, usize> = HashMap::new();
    for (t, ms) in &methods {
        *score.entry(t).or_insert(0) += ms.len();
    }
    for f in fields {
        *score.entry(f.owner.as_str()).or_insert(0) += 1;
    }
    let mut ranked: Vec<&str> = known.into_iter().collect();
    ranked.sort_by_key(|t| (std::cmp::Reverse(*score.get(t).unwrap_or(&0)), *t));
    let max_types = max_types.max(1);
    let shown: HashSet<&str> = ranked.iter().take(max_types).copied().collect();

    // 类型名 → 同名的类型符号(取 id 最小,确定性)
    let mut class_sym: HashMap<&str, &str> = HashMap::new();
    for s in symbols.iter().filter(|s| s.kind == SymKind::Class) {
        let e = class_sym.entry(s.name.as_str()).or_insert(s.id.as_str());
        if s.id.as_str() < *e {
            *e = s.id.as_str();
        }
    }

    let mut names: Vec<&str> = shown.iter().copied().collect();
    names.sort();
    let types: Vec<TypeNode> = names
        .iter()
        .map(|t| {
            let mut fs: Vec<TypeField> = fields
                .iter()
                .filter(|f| f.owner == *t)
                .map(|f| TypeField {
                    name: f.field.clone(),
                    ty: f.ty.clone(),
                })
                .collect();
            fs.sort_by(|a, b| (&a.name, &a.ty).cmp(&(&b.name, &b.ty)));
            fs.dedup();
            let mut ms = methods.get(t).cloned().unwrap_or_default();
            ms.sort_by(|a, b| (&a.name, &a.symbol).cmp(&(&b.name, &b.symbol)));
            TypeNode {
                id: safe_type(t),
                name: t.to_string(),
                symbol: class_sym.get(t).map(|s| s.to_string()),
                fields: fs,
                methods: ms,
            }
        })
        .collect();

    // 关系:两端都在仓内才算;一端没画 ⇒ 记进 omitted.links
    let mut relations: Vec<TypeRelation> = Vec::new();
    let mut omitted_links = 0usize;
    let mut seen_omitted: HashSet<(String, String, TypeRelKind, Option<String>)> = HashSet::new();
    let known_all: HashSet<&str> = ranked.iter().copied().collect();
    let mut push = |a: &str, b: &str, kind: TypeRelKind, label: Option<String>| {
        if !known_all.contains(a) || !known_all.contains(b) {
            return; // 指向仓外类型:不是这个仓的结构,不算省略
        }
        if shown.contains(a) && shown.contains(b) {
            relations.push(TypeRelation {
                from: safe_type(a),
                to: safe_type(b),
                kind,
                label,
            });
        } else if seen_omitted.insert((a.to_string(), b.to_string(), kind, label)) {
            omitted_links += 1;
        }
    };
    for i in impls {
        push(&i.type_name, &i.trait_name, TypeRelKind::Implements, None);
    }
    for f in fields {
        push(
            &f.owner,
            &f.ty,
            TypeRelKind::Composes,
            Some(f.field.clone()),
        );
    }
    relations.sort_by(|a, b| {
        (&a.from, &a.to, a.kind as u8, &a.label).cmp(&(&b.from, &b.to, b.kind as u8, &b.label))
    });
    relations.dedup();

    let omitted_methods: usize = ranked
        .iter()
        .skip(max_types)
        .map(|t| methods.get(t).map_or(0, |m| m.len()))
        .sum();
    UmlGraph {
        types,
        relations,
        omitted: Omitted {
            nodes: ranked.len().saturating_sub(shown.len()),
            symbols: omitted_methods,
            links: omitted_links,
        },
    }
}

/// 类型关系图的**图体**(不含通用图注)。
pub(crate) fn type_graph_mermaid(types: &[TypeNode], relations: &[TypeRelation]) -> String {
    let mut out = String::from("classDiagram\n");
    for t in types {
        out.push_str(&format!("  class {} {{\n", t.id));
        for f in t.fields.iter().take(6) {
            out.push_str(&format!("    +{} : {}\n", f.name, f.ty));
        }
        let mut ms: Vec<&str> = t.methods.iter().map(|m| m.name.as_str()).collect();
        ms.dedup();
        for m in ms.iter().take(6) {
            out.push_str(&format!("    +{m}()\n"));
        }
        if ms.len() > 6 {
            out.push_str(&format!("    +... 还有 {} 个方法\n", ms.len() - 6));
        }
        out.push_str("  }\n");
    }
    for r in relations {
        match r.kind {
            TypeRelKind::Implements => {
                out.push_str(&format!("  {} <|.. {} : 实现\n", r.to, r.from));
            }
            TypeRelKind::Composes => out.push_str(&format!(
                "  {} --> {} : {}\n",
                r.from,
                r.to,
                r.label.as_deref().unwrap_or("")
            )),
        }
    }
    out.push_str(
        "\n%% 关系来自**声明**不是推断:`impl Tr for A` · `struct E { idx: Index }`\n\
         %% 只画两端都在仓内的关系 —— 指向 Vec/String 这类外部类型的不画\n",
    );
    out
}

/// 类图 → Mermaid(兼容入口,等价于 `to_mermaid` 画一张 `uml` 图)。
pub fn uml_mermaid(
    symbols: &[Symbol],
    impls: &[ImplRel],
    fields: &[FieldType],
    max_types: usize,
) -> String {
    let g = uml_graph(symbols, impls, fields, max_types);
    super::to_mermaid(&super::Diagram {
        kind: super::DiagramKind::Uml,
        honesty: Honesty {
            omitted: Some(g.omitted),
            ..Honesty::default()
        },
        body: super::DiagramBody::TypeGraph {
            types: g.types,
            relations: g.relations,
        },
    })
}

/// UML 里的类名:Mermaid 的 classDiagram 不吃 `<`、`'`(泛型/生命周期)。
pub(crate) fn safe_type(t: &str) -> String {
    t.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}
