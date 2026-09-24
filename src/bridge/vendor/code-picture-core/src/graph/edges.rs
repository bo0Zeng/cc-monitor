//! 边的落地:去重 · 按可信度升级 · 参数流合并。
//!
//! 同一对 (from, to) 只留一条 —— **刻意的有损**:分不出是哪个调用点建的。
//! 要精确到调用点得按调用点建边,那是另一种图(见 `model::Edge::arg_flow`)。

use crate::model::{Confidence, Edge, EdgeKind};
use std::collections::HashMap;

/// 同 (from,to) 去重,保留**最高置信度**(Exact>Heuristic>DynamicGuess)。
/// 同一对 (from,to) 只留一条。并列时按置信度就地升级 ——
/// 同一个调用者对同一个被调,若既有唯一解析又有歧义解析,留**更可信**的那条。
#[allow(clippy::too_many_arguments)]
pub(super) fn upsert(
    edges: &mut Vec<Edge>,
    index_of: &mut HashMap<(String, String), usize>,
    from: &str,
    to: &str,
    line: usize,
    kind: EdgeKind,
    conf: Confidence,
    candidates: Option<usize>,
    arg_flow: Option<String>,
) {
    let key = (from.to_string(), to.to_string());
    match index_of.get(&key) {
        Some(&i) => {
            if rank(conf) > rank(edges[i].confidence) {
                edges[i].confidence = conf;
                edges[i].call_site_line = Some(line);
                edges[i].kind = kind;
                edges[i].candidates = candidates;
            }
            // 参数流取**并集**:同一对 (调用方,被调) 的多个调用点,边只有一条。
            // 这是有损的 —— 分不出是哪个调用点传的,`Edge.arg_flow` 的注释写着这条。
            edges[i].arg_flow = merge_arg_flow(edges[i].arg_flow.take(), arg_flow);
        }
        None => {
            index_of.insert(key, edges.len());
            edges.push(Edge {
                from: from.to_string(),
                to: to.to_string(),
                kind,
                call_site_line: Some(line),
                confidence: conf,
                candidates,
                arg_flow,
            });
        }
    }
}

/// 合并两份参数流串,去重后按 (i, j) 升序 —— 确定性。
pub(super) fn merge_arg_flow(a: Option<String>, b: Option<String>) -> Option<String> {
    let mut pairs: Vec<(usize, usize, bool)> = a
        .iter()
        .chain(b.iter())
        .flat_map(|s| s.split(','))
        .filter_map(parse_flow_pair)
        .collect();
    if pairs.is_empty() {
        return None;
    }
    // 同一对位置若既有确定又有可能 ⇒ 留**确定**那条(derived=false 排前,dedup_by 留首个)
    pairs.sort_unstable();
    pairs.dedup_by(|x, y| x.0 == y.0 && x.1 == y.1);
    Some(
        pairs
            .iter()
            .map(|(i, j, d)| {
                if *d {
                    format!("{i}~{j}")
                } else {
                    format!("{i}>{j}")
                }
            })
            .collect::<Vec<_>>()
            .join(","),
    )
}

/// `"0>1"` → `(0, 1, false)` 确定;`"0~1"` → `(0, 1, true)` 可能。
/// 形不对就丢掉(宁可少,不许错)。
pub fn parse_flow_pair(s: &str) -> Option<(usize, usize, bool)> {
    let t = s.trim();
    let (sep, derived) = if t.contains('>') {
        ('>', false)
    } else {
        ('~', true)
    };
    let (i, j) = t.split_once(sep)?;
    Some((i.trim().parse().ok()?, j.trim().parse().ok()?, derived))
}

pub(super) fn rank(c: Confidence) -> u8 {
    match c {
        Confidence::Exact => 3,
        Confidence::Dispatch => 2,
        Confidence::Heuristic => 1,
        Confidence::DynamicGuess => 0,
    }
}
