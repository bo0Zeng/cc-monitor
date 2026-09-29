//! **函数摘要**的格式与跨函数传播。
//!
//! 摘要一条 = `i~j:path`,读作「第 `i` 参的值**可能**流进第 `j` 参所指对象的 `path` 字段」。
//! 过程内那一档在 `crate::symbols::param_flow_of` 算(只看函数自己的函数体);
//! 这里补的是**跨函数**那一档 —— 顺调用边把被调的摘要抬到调用方头上。
//!
//! ```text
//! fn store(dst, v)  { dst.f = v; }          ⇒ 摘要 1~0:f
//! fn outer(x, s)    { store(&mut s, x); }   ⇒ 摘要 0~1:f   ← 这一条得靠传播才有
//! ```
//!
//! 传播规则:边 `C → D` 的 `arg_flow` 给出「调用方第 i 参 → 被调第 a 参」的映射;
//! D 的摘要 `a~b:p` 说「D 把第 a 参写进第 b 参的 p」。于是只要调用方的第 i 参映到 a、
//! 第 k 参映到 b,调用方自己就有 `i~k:p`。
//!
//! 🔴 **边界**(都是「追不到」= 漏,不是「多说了」= 宽):
//! * 只顺 `Calls` 边传 —— `AmbiguousCall` 的被调不唯一,抬哪个的摘要都是编;
//! * 只在实参是**裸标识符**时能在调用点落地(见 `graph::calls` 的 `effects`);
//! * 不动点有轮数上限,到顶就停(此时摘要**偏少**,追不到而已)。

use crate::model::{Edge, EdgeKind, Slot, SymbolId};
use std::collections::{BTreeSet, HashMap};

/// 摘要一条:槽位 `i` 的值写进槽位 `j` 所指对象的 `field`。槽位含**接收者**(见 [`Slot`])。
type Entry = (Slot, Slot, String);
/// 一条可传播的调用边:(调用方, 被调, 「调用方槽位 → 被调槽位」映射)。
type Link<'a> = (&'a str, &'a str, Vec<(Slot, Slot)>);

/// 传播的轮数上限。每轮至少让一条调用链多长一跳,现实调用链远短于此;
/// 到顶就停,结果是摘要**偏少**(追不到),不会变成假事实。
const MAX_ROUNDS: usize = 8;

/// 解析摘要串 → `(i, j, field)` 三元组。认不出的条目**跳过**,不猜。
pub(super) fn parse_param_flow(flow: &str) -> Vec<Entry> {
    flow.split(',')
        .filter_map(|part| {
            let (pair, field) = part.trim().split_once(':')?;
            let (i, j) = pair.split_once('~')?;
            Some((Slot::parse(i)?, Slot::parse(j)?, field.trim().to_string()))
        })
        .collect()
}

/// 解析 `arg_flow` → 「调用方第 i 参 → 被调第 j 参」的映射(`>` 与 `~` 一视同仁 ——
/// 摘要本就只到「可能」这一档,再分确定没有意义)。
fn parse_arg_flow(flow: &str) -> Vec<(Slot, Slot)> {
    flow.split(',')
        .filter_map(|part| {
            let p = part.trim();
            let (i, j) = p.split_once('>').or_else(|| p.split_once('~'))?;
            Some((Slot::parse(i)?, Slot::parse(j)?))
        })
        .collect()
}

/// 跨函数不动点:顺调用边把被调的摘要抬到调用方头上,直到不再变。
///
/// 返回加厚后的「符号 id → 摘要串」;没变的不返回。
pub(super) fn propagate(
    seed: &HashMap<SymbolId, String>,
    edges: &[Edge],
) -> HashMap<SymbolId, String> {
    // 只顺 `Calls`(唯一解析)的边,且这条边得有参数流才谈得上映射
    let links: Vec<Link> = edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .filter_map(|e| {
            let m = parse_arg_flow(e.arg_flow.as_deref()?);
            (!m.is_empty()).then_some((e.from.as_str(), e.to.as_str(), m))
        })
        .collect();

    // BTreeSet 保证输出顺序确定 —— 摘要要落库,不确定的顺序会让增量索引反复变脏
    let mut cur: HashMap<&str, BTreeSet<Entry>> = seed
        .iter()
        .map(|(id, f)| (id.as_str(), parse_param_flow(f).into_iter().collect()))
        .collect();

    for _ in 0..MAX_ROUNDS {
        let mut changed = false;
        for (from, to, map) in &links {
            let Some(callee) = cur.get(*to) else { continue };
            let lifted: Vec<Entry> = callee
                .iter()
                .flat_map(|(a, b, p)| {
                    // 调用方的第 i 参映到被调第 a 参、第 k 参映到第 b 参 ⇒ 调用方有 i~k:p
                    map.iter()
                        .filter(move |(_, ja)| ja == a)
                        .flat_map(move |(i, _)| {
                            map.iter()
                                .filter(move |(_, jb)| jb == b)
                                .filter(move |(k, _)| k != i)
                                .map(move |(k, _)| (*i, *k, p.clone()))
                        })
                })
                .collect();
            if lifted.is_empty() {
                continue;
            }
            let slot = cur.entry(from).or_default();
            for e in lifted {
                changed |= slot.insert(e);
            }
        }
        if !changed {
            break;
        }
    }

    cur.into_iter()
        .filter(|(_, set)| !set.is_empty())
        .map(|(id, set)| {
            let joined = set
                .iter()
                .map(|(i, j, p)| format!("{}~{}:{p}", i.token(), j.token()))
                .collect::<Vec<_>>()
                .join(",");
            (id.to_string(), joined)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Confidence;

    fn call(from: &str, to: &str, arg_flow: &str) -> Edge {
        Edge {
            from: from.into(),
            to: to.into(),
            kind: EdgeKind::Calls,
            confidence: Confidence::Exact,
            call_site_line: Some(1),
            candidates: None,
            arg_flow: Some(arg_flow.into()),
        }
    }

    #[test]
    fn a_callee_summary_is_lifted_onto_its_caller() {
        // outer(x, s) { store(&mut s, x); } —— 实参 0 位是 s(outer 第 1 参),
        // 1 位是 x(outer 第 0 参) ⇒ arg_flow "1>0,0>1";store 摘要 1~0:f。
        // ⇒ outer 应得 0~1:f(把第 0 参写进了第 1 参的 f)。
        let seed = HashMap::from([("store".to_string(), "1~0:f".to_string())]);
        let got = propagate(&seed, &[call("outer", "store", "1>0,0>1")]);
        assert_eq!(got.get("outer").map(String::as_str), Some("0~1:f"));
    }

    #[test]
    fn lifting_chains_through_two_hops() {
        // a → b → store,三层都该把摘要抬上来。
        let seed = HashMap::from([("store".to_string(), "1~0:f".to_string())]);
        let edges = [
            call("b", "store", "1>0,0>1"),
            call("a", "b", "0>0,1>1"), // a 的参数原样传给 b
        ];
        let got = propagate(&seed, &edges);
        assert_eq!(got.get("b").map(String::as_str), Some("0~1:f"));
        assert_eq!(got.get("a").map(String::as_str), Some("0~1:f"));
    }

    #[test]
    fn a_cycle_terminates() {
        // 互相递归不该转不停 —— 轮数上限兜底。
        let seed = HashMap::from([("store".to_string(), "1~0:f".to_string())]);
        let edges = [
            call("a", "b", "0>0,1>1"),
            call("b", "a", "0>0,1>1"),
            call("a", "store", "1>0,0>1"),
        ];
        let got = propagate(&seed, &edges);
        assert_eq!(got.get("a").map(String::as_str), Some("0~1:f"));
        assert_eq!(got.get("b").map(String::as_str), Some("0~1:f"));
    }

    #[test]
    fn an_ambiguous_edge_carries_nothing() {
        // 🔴 被调不唯一 ⇒ 抬哪个的摘要都是编。
        let seed = HashMap::from([("store".to_string(), "1~0:f".to_string())]);
        let mut e = call("outer", "store", "1>0,0>1");
        e.kind = EdgeKind::AmbiguousCall;
        let got = propagate(&seed, &[e]);
        assert_eq!(got.get("outer"), None);
    }

    #[test]
    fn parsing_skips_what_it_cannot_read() {
        assert_eq!(
            parse_param_flow("1~0:f,garbage,s~1:a.b"),
            vec![
                (Slot::Param(1), Slot::Param(0), "f".to_string()),
                (Slot::Recv, Slot::Param(1), "a.b".to_string())
            ],
            "认不出的条目跳过;`s` 是接收者槽位"
        );
    }
}
