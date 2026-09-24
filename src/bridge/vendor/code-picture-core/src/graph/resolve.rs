//! 调用点 → 仓内符号的**解析规则**。整个静态层最要紧的一处。
//!
//! 🔴 **一个调用点的解析结果是一条,不是 N 条。** 把「我不知道是哪个」记成 N 条
//! 「就是这个」,其中至少 N-1 条是假的 —— 实测代价见 `model::EdgeKind::AmbiguousCall`。
//!
//! 限定名四条来源,按可靠性排(前三条都**语法可及、非推断**):
//! ① 类型限定 `Type::name` ② **模块限定**(模块名在多数语言里就是文件名)
//! ③ **接收者类型**(`self.m()` 是语言规则;`let g = f()` 后取 f 的返回类型)
//! ④ 裸名(唯一才算 Exact)。
//!
//! ⚠ 第 ③ 条来的限定**不许退回按名**:已知接收者是 `T` 而仓里没有 `T::name`,
//! 说明这调用落在仓外,该算「看不见」。

use super::table::SymbolTable;
use crate::model::{Confidence, Symbol, SymbolId};

/// 从符号 id 取类型限定:`file#Type::name` → `Type`;自由函数(`file#name`)→ None。
pub(super) fn qualifier_of_id(id: &str) -> Option<String> {
    let tail = id.split_once('#').map(|(_, t)| t).unwrap_or(id);
    tail.rsplit_once("::").map(|(q, _)| q.to_string())
}

/// 文件名去扩展名 —— 多数语言里模块名就是它(`src/a/annotations.rs` → `annotations`)。
pub(super) fn file_stem(file: &str) -> &str {
    let base = file.rsplit('/').next().unwrap_or(file);
    base.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(base)
}

/// 这个文件能被哪些**模块名**指到。
///
/// 通常就是文件名(`a/annotations.rs` → `annotations`);但**目录模块**的模块名是
/// **父目录名**:`graph/mod.rs` 的 stem 是 `mod`,而调用点写的是 `graph::build_edges`。
/// 不认这一形的话,目录模块里的每个自由函数调用都匹配不上限定名。
/// 同理 JS 的 `index.js` · Python 的 `__init__.py`。
fn module_names(file: &str) -> Vec<&str> {
    let stem = file_stem(file);
    let mut out = vec![stem];
    if matches!(stem, "mod" | "index" | "__init__") {
        if let Some((dir, _)) = file.rsplit_once('/') {
            out.push(dir.rsplit('/').next().unwrap_or(dir));
        }
    }
    out
}

/// 一个调用点的解析结果。**一个调用点最多产一条边** —— 这是本模块最要紧的一条:
/// 把「我不知道是哪个」记成 N 条「就是这个」,其中至少 N-1 条是假的。
pub(super) enum Resolved {
    /// 仓内一个同名的都没有(外部 / stdlib / 宏)→ 不建边,计入 unresolved。
    None,
    /// 唯一解析 → `Calls` / `Exact`。
    One(SymbolId),
    /// 多候选,或方法调用(接收者类型未知)→ **每个候选一条** `AmbiguousCall`。
    ///
    /// **为什么给每个候选都建边、而不是只挑一个**:只挑一个会让其余 N-1 个候选
    /// 在反向查询里**彻底消失** —— 问「谁可能调 q.rs#bar」得到空,而它明明是候选之一。
    /// 那是丢信息。病根是「假边混进 `Calls`」,靠 `kind` 隔离治;
    /// 边多本身不是病 —— 每条边的权按候选数均分(见 `rank::edge_weight`),
    /// 一个调用点总共只投一票,不会因为候选多就把谁顶成假 hub。
    Many {
        picks: Vec<SymbolId>,
        confidence: Confidence,
    },
}

/// 把一个调用点解析成 `Resolved`。
///
/// ⚠ **方法调用即使只有一个候选也算歧义** —— `x.foo()` 里 `x` 的类型未知,
/// 仓内只有一个 `foo` 不代表调的就是它(`x` 完全可能是个外部类型,`foo` 是它的方法)。
/// 候选数为 1 消不掉接收者未知这件事。
pub(super) fn resolve(
    name: &str,
    qual: Option<&str>,
    is_method: bool,
    table: &SymbolTable,
    qual_is_receiver_type: bool,
) -> Resolved {
    let cands = table.candidates(name);
    if cands.is_empty() {
        return Resolved::None;
    }
    // 候选 id 升序,确定性(不赌 candidates 的入表顺序)
    let sorted_ids = |v: &[&Symbol]| -> Vec<SymbolId> {
        let mut out: Vec<SymbolId> = v.iter().map(|s| s.id.clone()).collect();
        out.sort_unstable();
        out
    };
    // 作用域调用 Type::name:限定名唯一匹配才算解析成功
    if let Some(q) = qual {
        // 🔴 接收者类型是仓内 **trait / 接口** ⇒ 这是**动态派发**,先于一切判。
        // 另外两种答案都是错的:
        // * 解成「trait 的默认实现」是**假的 Exact** —— 实现者会覆盖它;
        // * 解成「看不见」也错 —— 那些实现明明在仓里。
        // 正确答案:候选 = 各实现者的同名方法(+ 默认实现,如果有),标 `AmbiguousCall`。
        // ⚠ 仓外也可能有实现者 ⇒ 即使仓内只有一个实现也**不升 Exact**。
        if qual_is_receiver_type {
            if let Some(impls) = table.implementors(q) {
                let picks: Vec<&Symbol> = cands
                    .iter()
                    .filter(|s| {
                        id_has_qualifier(&s.id, q, name)
                            || impls.iter().any(|i| id_has_qualifier(&s.id, i, name))
                    })
                    .collect();
                // 🔴 仓内**只有一个**可能的函数体 ⇒ 那就是它(没人覆盖默认实现的情形)。
                // 这不是「挑一个」,是候选集本来就只有一个元素。
                // ⚠ 仓外的实现者不在本仓的世界里 —— 与别处对「外部被调」的一贯态度一致。
                match picks.len() {
                    0 => {} // trait 认得出、但仓内没有同名方法 ⇒ 落在仓外,照常算看不见
                    1 => return Resolved::One(picks[0].id.clone()),
                    _ => {
                        return Resolved::Many {
                            picks: sorted_ids(&picks),
                            confidence: Confidence::Dispatch,
                        }
                    }
                }
            }
        }
        let matched: Vec<&Symbol> = cands
            .iter()
            .filter(|s| id_has_qualifier(&s.id, q, name))
            .collect();
        if matched.len() == 1 {
            return Resolved::One(matched[0].id.clone());
        }
        if !matched.is_empty() {
            return Resolved::Many {
                picks: sorted_ids(&matched),
                confidence: Confidence::Heuristic,
            };
        }
        // 类型限定没匹配上 → 再试**模块限定**:`annotations::write` 里 `annotations`
        // 指的是模块,而模块名在绝大多数语言里就是**文件名**。符号 id 是 `文件#名`,
        // `id_has_qualifier` 只认 `Type::method` 那种类型限定,认不出模块 ——
        // 不补这一条,`annotations::write(..)` 这种**写得明明白白的调用**也会被归进歧义。
        let by_module: Vec<&Symbol> = cands
            .iter()
            .filter(|sym| module_names(&sym.file).contains(&q))
            .collect();
        if by_module.len() == 1 {
            return Resolved::One(by_module[0].id.clone());
        }
        if !by_module.is_empty() {
            return Resolved::Many {
                picks: sorted_ids(&by_module),
                confidence: Confidence::Heuristic,
            };
        }
        // 🔴 限定来自**接收者类型**时不许退回按名:我们已经知道接收者是 `T`,
        // 而仓里没有 `T::name` ⇒ 这个调用落在仓外(外部类型的方法),就该算**看不见**。
        // 退回按名会把「`Result` 上的 `run`」错判成仓内唯一那个 `Graph::run`。
        // (限定来自**模块**时可以退 —— `module::free_fn()` 的 id 里本就不带模块名。)
        if qual_is_receiver_type {
            return Resolved::None;
        }
        // 🔴 限定名写在那儿、却**既不是仓内类型也不是仓内模块** ⇒ 目标在仓外。
        // 退回按名就是拿「名字碰巧一样」去顶掉「限定名明说了不是它」——
        // 实测代价:`Vec::new()` / `HashMap::new()` 这类被记成「可能调仓里那 8 个 `new`
        // 之一」,本仓光 `new` 一项就是 1395 条纯噪声边。
        //
        // ⚠ 唯一容许退回的情形:**名字本来就唯一**。那不叫「挑」,没什么可挑的;
        // 留这条是为了兜住我们还认不出的限定形(Rust 的 `super::` / `crate::a::b`)。
        // 名字不唯一时宁可算「看不见」—— 那是诚实的「不知道」,不是编一堆候选。
        if cands.len() > 1 {
            return Resolved::None;
        }
        // 名字唯一 → 落到下面的按名解析
    }
    let all: Vec<&Symbol> = cands.iter().collect();
    if is_method {
        return Resolved::Many {
            picks: sorted_ids(&all),
            confidence: Confidence::DynamicGuess,
        };
    }
    if all.len() == 1 {
        return Resolved::One(all[0].id.clone());
    }
    Resolved::Many {
        picks: sorted_ids(&all),
        confidence: Confidence::Heuristic,
    }
}

/// id 的符号段(最后一个 `#` 之后、去掉 `@行号`)是否等于 `Type::name`。
/// 结构化匹配,避免裸 `contains` 的子串误配。
pub(super) fn id_has_qualifier(id: &str, qual: &str, name: &str) -> bool {
    let sym_seg = match id.rsplit_once('#') {
        Some((_, seg)) => seg.split('@').next().unwrap_or(seg),
        None => return false,
    };
    sym_seg == format!("{}::{}", qual, name)
}
