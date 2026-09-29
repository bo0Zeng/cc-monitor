//! **到达定义**(reaching definitions):在控制流图上算「在某一行用到某个变量时,
//! 是哪几次赋值的值可能到这儿」。
//!
//! 它解的是流不敏感分析的那个过度近似:
//!
//! ```text
//! y = a;  g(y);      ← 流不敏感:y 的来源 = {a, b} ⇒ 说 a 和 b 都流到了两个 g
//! y = b;  g(y);      ← 到达定义:第一个 g 只看得见 a,第二个只看得见 b
//! ```
//!
//! 标准的正向 must/may 数据流:`OUT[B] = GEN[B] ∪ (IN[B] − KILL[B])`,
//! 按 may 语义(取并集)。**块内还要按行号分先后** —— 只到块粒度的话,
//! 上面那个例子两条赋值在同一个块里,照样分不开,等于白做。
//!
//! 🔴 **边界**:
//! * CFG 作废(`unsupported`)⇒ 本模块**一律不给答案**,调用方退回流不敏感那一档;
//! * 只认 `CfgDef` 记下的局部绑定 —— 再赋值(`x = ..` 而非 `let x = ..`)、
//!   经指针/引用改写、字段写入,**都不在射程内**;
//! * 因此它**收紧假阳,不消灭假阳** —— 结论仍是「可能」,只是可能的范围小了。

use crate::model::Cfg;
use std::collections::{HashMap, HashSet};

/// 每个块**入口处**的到达定义集合。定义用 (变量, 行号) 标识。
type DefId = (String, usize);

/// 算每个块入口的到达定义。CFG 作废时返回 `None`。
fn reaching_in(cfg: &Cfg) -> Option<Vec<HashSet<DefId>>> {
    if cfg.unsupported.is_some() {
        return None;
    }
    let n = cfg.blocks.len();
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (f, t) in &cfg.edges {
        if *t < n && *f < n {
            preds[*t].push(*f);
        }
    }
    let mut ins: Vec<HashSet<DefId>> = vec![HashSet::new(); n];
    // 定长迭代到不动点。上限 = 块数 + 2,足够(每轮至少确定一个块,环也收敛)
    for _ in 0..n + 2 {
        let mut changed = false;
        for b in 0..n {
            let mut merged: HashSet<DefId> = HashSet::new();
            for p in &preds[b] {
                merged.extend(out_of(cfg, *p, &ins[*p]));
            }
            if merged != ins[b] {
                ins[b] = merged;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Some(ins)
}

/// `OUT[B] = GEN[B] ∪ (IN[B] − KILL[B])`。
/// KILL 按**访问路径相关**:写 `s` 杀掉进来的 `s.f`(整个 s 被换了);
/// 写 `s.f` **不杀** `s.g` —— 这正是字段敏感买到的东西。
fn out_of(cfg: &Cfg, b: usize, in_b: &HashSet<DefId>) -> HashSet<DefId> {
    let defs = &cfg.blocks[b].defs;
    let mut out: HashSet<DefId> = in_b
        .iter()
        .filter(|(v, _)| !defs.iter().any(|d| crate::lang::paths_related(&d.var, v)))
        .cloned()
        .collect();
    // 同一块内同名多次赋值 ⇒ 只有**最后一次**能流出这个块
    let mut last: HashMap<&str, usize> = HashMap::new();
    for d in defs {
        last.insert(d.var.as_str(), d.at);
    }
    for (v, l) in last {
        out.insert((v.to_string(), l));
    }
    out
}

/// 在偏移 `at`(行 `line`)用到路径 `var` 时,能看见哪几次赋值。
///
/// 块内按**字节偏移**分先后(行号排不了同一行内的先后:`let s = f(); g(s)` 写一行时,
/// 「定义在使用之前」按行号判是假的);块归属仍按行范围。
/// 同块里 `at` 之前最近的那次赋值**遮蔽**入口处进来的;之前没有才用入口集合。
fn defs_visible_at(
    cfg: &Cfg,
    ins: &[HashSet<DefId>],
    var: &str,
    at: usize,
    line: usize,
) -> Vec<usize> {
    let mut out = Vec::new();
    for (b, blk) in cfg.blocks.iter().enumerate() {
        let local: Vec<usize> = blk
            .defs
            .iter()
            .filter(|d| crate::lang::paths_related(&d.var, var) && d.at < at)
            .map(|d| d.at)
            .collect();
        // 这一行不在本块里就跳过 —— 用调用点行号落块,块的行范围是判据
        if line < blk.start_line || line > blk.end_line {
            continue;
        }
        match local.last() {
            Some(&l) => out.push(l), // 同块内更近的那次赋值遮蔽外面的
            None => out.extend(
                ins[b]
                    .iter()
                    .filter(|(v, _)| crate::lang::paths_related(v, var))
                    .map(|(_, l)| *l),
            ),
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// 在 `line` 用到 `ident` 时,它的值**可能**来自哪几个形参位。
///
/// 顺到达定义做传递闭包:`ident` ← 某次赋值的 sources ← 那些标识符在**那一行**的到达定义……
/// 有界(8 层),防病态输入打转。
///
/// `None` = CFG 不可用 ⇒ 调用方退回流不敏感那一档(**不是**「没有来源」)。
pub fn param_origins_at(
    cfg: &Cfg,
    ident: &str,
    at: usize,
    line: usize,
    params: &[Option<String>],
) -> Option<Vec<usize>> {
    let ins = reaching_in(cfg)?;
    let mut out = Vec::new();
    let mut seen: HashSet<(String, usize)> = HashSet::new();
    let mut frontier = vec![(ident.to_string(), at)];
    for _ in 0..8 {
        let mut next: Vec<(String, usize)> = Vec::new();
        for (cur, at) in frontier {
            if !seen.insert((cur.clone(), at)) {
                continue;
            }
            if let Some(i) = params
                .iter()
                // 形参匹配看路径的**根**:`s.f` 的根是 `s`
                .position(|p| p.as_deref() == Some(crate::lang::path_root(&cur)))
            {
                out.push(i);
            }
            for dl in defs_visible_at(cfg, &ins, &cur, at, line) {
                for blk in &cfg.blocks {
                    for d in &blk.defs {
                        if crate::lang::paths_related(&d.var, &cur) && d.at == dl {
                            next.extend(d.sources.iter().map(|s| (s.clone(), dl)));
                        }
                    }
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    out.sort_unstable();
    out.dedup();
    Some(out)
}
