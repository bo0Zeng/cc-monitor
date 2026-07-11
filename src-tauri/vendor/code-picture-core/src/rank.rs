//! 图算法(F03):PageRank / 社区(标签传播)/ 入口点。索引化,纯计算无 IO。

use crate::model::{Edge, Symbol, SymbolId};
use std::collections::HashMap;

pub struct Graph {
    pub ids: Vec<SymbolId>,
    out: Vec<Vec<usize>>,
    inc: Vec<Vec<usize>>,
    undirected: Vec<Vec<usize>>,
}

impl Graph {
    pub fn build(symbols: &[Symbol], edges: &[Edge]) -> Graph {
        let ids: Vec<SymbolId> = symbols.iter().map(|s| s.id.clone()).collect();
        let idx: HashMap<&str, usize> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        let n = ids.len();
        let mut out = vec![Vec::new(); n];
        let mut inc = vec![Vec::new(); n];
        let mut undirected = vec![Vec::new(); n];
        for e in edges {
            if let (Some(&a), Some(&b)) = (idx.get(e.from.as_str()), idx.get(e.to.as_str())) {
                if a == b {
                    continue; // 忽略自环
                }
                out[a].push(b);
                inc[b].push(a);
                undirected[a].push(b);
                undirected[b].push(a);
            }
        }
        Graph {
            ids,
            out,
            inc,
            undirected,
        }
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// PageRank(dangling 均摊)。
    pub fn pagerank(&self, iters: usize, damping: f64) -> Vec<f64> {
        let n = self.len();
        if n == 0 {
            return vec![];
        }
        let base = (1.0 - damping) / n as f64;
        let mut rank = vec![1.0 / n as f64; n];
        for _ in 0..iters {
            let mut next = vec![base; n];
            let mut dangling = 0.0;
            for (i, &r) in rank.iter().enumerate() {
                if self.out[i].is_empty() {
                    dangling += damping * r;
                } else {
                    let share = damping * r / self.out[i].len() as f64;
                    for &j in &self.out[i] {
                        next[j] += share;
                    }
                }
            }
            let d = dangling / n as f64;
            for v in next.iter_mut() {
                *v += d;
            }
            rank = next;
        }
        rank
    }

    /// 标签传播社区检测(无向)。确定性:固定顺序 + 并列取最小标签。
    /// 返回每个节点的社区标签(某个节点索引作代表)。
    pub fn communities(&self, iters: usize) -> Vec<usize> {
        let n = self.len();
        let mut label: Vec<usize> = (0..n).collect();
        for _ in 0..iters {
            let mut changed = false;
            for i in 0..n {
                if self.undirected[i].is_empty() {
                    continue;
                }
                let mut counts: HashMap<usize, usize> = HashMap::new();
                for &nb in &self.undirected[i] {
                    *counts.entry(label[nb]).or_insert(0) += 1;
                }
                // 频次最高;并列取最小标签
                let best = counts
                    .iter()
                    .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                    .map(|(&l, _)| l);
                if let Some(bl) = best {
                    if label[i] != bl {
                        label[i] = bl;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        label
    }

    /// 入口点:无入边(没有函数调用它)的节点索引。
    pub fn entry_points(&self) -> Vec<usize> {
        (0..self.len())
            .filter(|&i| self.inc[i].is_empty())
            .collect()
    }
}
