//! 解析上下文:名字 → 候选符号 · 全仓文件表 · 返回类型表。
//!
//! 它是**只读的事实面** —— 建边的几条路(调用 / import / 参数流)都从这里取事实、
//! 都不改它。单独拎出来,是为了那几条路互不知道对方存在。

use crate::model::{Confidence, FieldType, ImplRel, Symbol, SymbolId};
use std::collections::{HashMap, HashSet};

pub struct SymbolTable {
    by_name: HashMap<String, Vec<Symbol>>,
    /// 全仓已索引的源码文件。**不从符号推** —— 没有符号的文件(纯类型/常量模块)
    /// 照样可以是 import 的目标,靠符号推会把它们漏掉。
    files: HashSet<String>,
    /// 文件名末段 → 完整路径(升序)。给「按后缀解析」用:Java/Kotlin 的包名→目录
    /// 约定不含 `src/main/java/` 这类前缀,拿不到完整路径,只能按后缀找。
    by_basename: HashMap<String, Vec<String>>,
    /// 符号 id → 返回类型名。给「`let g = f()` ⇒ g 的类型 = f 的返回类型」用。
    return_types: HashMap<SymbolId, String>,
    /// 符号 id → 哪几个形参流到返回值。给「缩窄 `let y = h(x, z)` 的来源」用。
    return_flows: HashMap<SymbolId, String>,
    /// (类型名, 字段名) → 字段类型名。定接收者类型用:`self.idx.m()` 里 `self.idx` 是什么。
    ///
    /// 🔴 值是 `Option`:同名类型在多个文件里各声明一次、且字段类型**不一致**时存 `None`,
    /// 查的时候答不出来 —— 分不清就不答,不挑一个。
    field_types: HashMap<(String, String), Option<String>>,
    /// trait / 接口名 → 仓内实现它的类型名。**也是「这名字是不是 trait」的唯一判据**。
    implementors: HashMap<String, Vec<String>>,
    /// 符号 id → **函数摘要**(`i~j:path`)。给「调用点 `store(&mut s, x)` 记下 `s.f ← x`」用。
    /// 🔴 可被 `extend_summaries` 就地加厚(跨函数不动点),所以它**不是**只读快照的一部分。
    param_flows: HashMap<SymbolId, String>,
}

impl SymbolTable {
    /// 文件表从符号推(**仅供测试与旧调用方**)。生产路径用 `with_files` 传真文件表 ——
    /// 否则没有符号的文件当不了 import 目标。
    pub fn from_symbols(syms: Vec<Symbol>) -> SymbolTable {
        let files: Vec<String> = {
            let mut v: Vec<String> = syms.iter().map(|s| s.file.clone()).collect();
            v.sort();
            v.dedup();
            v
        };
        SymbolTable::new(syms, files)
    }

    /// 全量构造:符号表 + **真实**全仓文件表。
    pub fn with_files(syms: Vec<Symbol>, files: Vec<String>) -> SymbolTable {
        SymbolTable::new(syms, files)
    }

    fn new(syms: Vec<Symbol>, mut files: Vec<String>) -> SymbolTable {
        let mut by_name: HashMap<String, Vec<Symbol>> = HashMap::new();
        let mut return_types: HashMap<SymbolId, String> = HashMap::new();
        let mut return_flows: HashMap<SymbolId, String> = HashMap::new();
        let mut param_flows: HashMap<SymbolId, String> = HashMap::new();
        for s in syms {
            if let Some(rt) = &s.return_type {
                return_types.insert(s.id.clone(), rt.clone());
            }
            if let Some(rf) = &s.return_flow {
                return_flows.insert(s.id.clone(), rf.clone());
            }
            if let Some(pf) = &s.param_flow {
                param_flows.insert(s.id.clone(), pf.clone());
            }
            by_name.entry(s.name.clone()).or_default().push(s);
        }
        files.sort();
        files.dedup();
        let mut by_basename: HashMap<String, Vec<String>> = HashMap::new();
        for f in &files {
            let base = f.rsplit('/').next().unwrap_or(f).to_string();
            by_basename.entry(base).or_default().push(f.clone());
        }
        SymbolTable {
            by_name,
            files: files.into_iter().collect(),
            by_basename,
            return_types,
            return_flows,
            param_flows,
            field_types: HashMap::new(),
            implementors: HashMap::new(),
        }
    }

    /// 装上全仓实现关系(建图前一次性给)。
    pub fn with_impls(mut self, impls: Vec<ImplRel>) -> SymbolTable {
        for i in impls {
            let v = self.implementors.entry(i.trait_name).or_default();
            if !v.contains(&i.type_name) {
                v.push(i.type_name);
            }
        }
        for v in self.implementors.values_mut() {
            v.sort(); // 确定性:候选顺序不赌入表顺序
        }
        self
    }

    /// 仓内实现 `trait_name` 的类型。`None` = 这名字不是(我们看得见的)trait。
    pub(super) fn implementors(&self, trait_name: &str) -> Option<&[String]> {
        self.implementors.get(trait_name).map(|v| v.as_slice())
    }

    /// 装上全仓字段类型表(建图前一次性给)。
    ///
    /// 同一 (类型, 字段) 出现多次:类型名一致就还是它,不一致就记成**答不出**。
    pub fn with_field_types(mut self, fields: Vec<FieldType>) -> SymbolTable {
        for f in fields {
            let key = (f.owner, f.field);
            match self.field_types.get(&key) {
                None => {
                    self.field_types.insert(key, Some(f.ty));
                }
                Some(Some(prev)) if prev == &f.ty => {}
                Some(_) => {
                    self.field_types.insert(key, None); // 冲突 ⇒ 分不清 ⇒ 不答
                }
            }
        }
        self
    }

    /// `owner` 类型的 `field` 字段是什么类型。分不清 / 没记 → `None`。
    pub(super) fn field_type_of(&self, owner: &str, field: &str) -> Option<&str> {
        self.field_types
            .get(&(owner.to_string(), field.to_string()))?
            .as_deref()
    }

    pub(super) fn return_type_of(&self, id: &str) -> Option<&str> {
        self.return_types.get(id).map(|s| s.as_str())
    }

    /// 哪几个形参位流到这个符号的返回值(`"0,2"`)。
    pub(super) fn return_flow_of(&self, id: &str) -> Option<&str> {
        self.return_flows.get(id).map(|s| s.as_str())
    }

    /// 这个符号的**函数摘要**(`i~j:path`)。
    pub(super) fn param_flow_of(&self, id: &str) -> Option<&str> {
        self.param_flows.get(id).map(|s| s.as_str())
    }

    /// 跨函数不动点算完后,把加厚的摘要写回。
    pub(super) fn set_param_flow(&mut self, id: &str, flow: String) {
        self.param_flows.insert(id.to_string(), flow);
    }

    /// 当前所有摘要(不动点的输入)。
    pub(super) fn param_flows(&self) -> &HashMap<SymbolId, String> {
        &self.param_flows
    }

    pub(super) fn candidates(&self, name: &str) -> &[Symbol] {
        self.by_name.get(name).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// 候选路径 → 仓内文件。**先试精确,再试后缀**(见 `by_basename` 注释)。
    /// 后缀命中多份(同名类散在多个源根)→ 取排序首个并标 `Heuristic`,不静默挑。
    pub(super) fn resolve_file(&self, candidates: &[String]) -> Option<(String, Confidence)> {
        for c in candidates {
            if self.files.contains(c) {
                return Some((c.clone(), Confidence::Exact));
            }
        }
        for c in candidates {
            let base = c.rsplit('/').next().unwrap_or(c);
            let Some(paths) = self.by_basename.get(base) else {
                continue;
            };
            // 后缀必须落在路径分隔处,免得 `x/Foo.java` 误命中 `y/MyFoo.java`
            let hits: Vec<&String> = paths
                .iter()
                .filter(|p| {
                    p.len() > c.len()
                        && p.ends_with(c.as_str())
                        && p.as_bytes()[p.len() - c.len() - 1] == b'/'
                })
                .collect();
            match hits.len() {
                0 => {}
                1 => return Some((hits[0].clone(), Confidence::Exact)),
                _ => return Some((hits[0].clone(), Confidence::Heuristic)),
            }
        }
        None
    }
}
