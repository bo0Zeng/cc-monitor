//! **主线外清单**（ESC 回退掉的那几条）—— 通用层算，只吃适配层给的链事实（[`ChainFact`]），不认任何一家的字段名。
//!
//! 一家的记录若是一棵树（每条记着上一条是谁），用户在某条之前回退重发时，旧的那一支留在文件里、新的一支从同一个上一条长出来。
//! 主线 ＝ 每个分叉点选「子树里最新」的那一支；多个根时，只有「人说的、没等到回复（或最后一句是打断）」的那种死胡同根整棵算主线外，
//! 压缩边界那类非对话根与完整对话的根都留。排队消息（人在一轮跑着时插进去的话）在链上是一片裸叶，靠「它的原文在排队集合里」并回主线。
//!
//! 记录发出去时是主线、之后可能变成主线外（用户在它前面回退重发）⇒ 清单单独给（实时帧 `session_branch` 整份 · 冷读 `history-branch`），
//! 不做成每条记录上的标记。没有链的那一家不出事实 ⇒ 清单恒空。

use std::collections::{HashMap, HashSet};

/// 链上一个节点（适配层从一条记录翻过来）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Link {
    /// 这条记录的身份（与记录成品的 `id` 同值）。
    pub(crate) id: String,
    /// 上一条是谁；缺 ＝ 根。
    pub(crate) parent: Option<String>,
    /// 时刻（同一种写法、可按字典序比先后）。
    pub(crate) at: String,
    /// 人那一侧说的（对话里的一句，不论是不是人本人打的字）。
    pub(crate) said: bool,
    /// 代理的回复。
    pub(crate) reply: bool,
    /// 整条是一个打断标记（回撤死胡同的信号）。
    pub(crate) interrupt: bool,
    /// 人那一侧这条的原文（排队豁免按它配）；不是人那一侧 ⇒ 缺。
    pub(crate) text: Option<String>,
    /// 这条进不进界面（出记录成品）：只有进界面的才进清单，链上纯占位的那些不进。
    pub(crate) shown: bool,
}

/// 适配层从一行翻出的链事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChainFact {
    /// 链上一个节点。
    Node(Link),
    /// 一句排进队列的话（原文，已 trim）：它对应的那片裸叶不算回退。
    Queued(String),
}

/// 一份会话记录的链索引：每个节点只记几个短串。
#[derive(Debug, Default)]
pub(crate) struct Chain {
    nodes: Vec<Link>,
    at: HashMap<String, usize>,
    has_child: Vec<bool>,
    /// 说到过、但还没见到的上一条（它一到，接在它下面的那几条就不再是根 ⇒ 整份重算）。
    unseen_parents: HashSet<String>,
    queued: HashSet<String>,
    /// 上一次算的结果：每个节点在不在主线上 · 是不是排队豁免回来的 · 属于哪个根 · 胜出的根。
    on_main: Vec<bool>,
    exempt: Vec<bool>,
    root_of: Vec<usize>,
    winner: Option<usize>,
    off: Vec<String>,
}

impl Chain {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 喂一条事实；回：主线外清单变没变。
    ///
    /// 大多数新节点接在主线末梢上（上一条在主线上、属于胜出的根、此前没有孩子）⇒ 主线照旧延长一格，不重算。
    /// 其余（新根 · 分叉 · 接在主线外或豁免叶上 · 补上了先前缺的上一条 · 主线外有东西时排队集合变了）整份重算 ——
    /// 结果与对全部事实重算一次逐条相同。
    pub(crate) fn push(&mut self, fact: ChainFact) -> bool {
        match fact {
            ChainFact::Queued(t) => {
                let fresh = self.queued.insert(t);
                fresh && self.on_main.iter().any(|m| !m) && self.recompute()
            }
            ChainFact::Node(l) => {
                if self.at.contains_key(&l.id) {
                    return false; // 重复投递：保首见
                }
                let i = self.nodes.len();
                let p = l.parent.as_deref().and_then(|p| self.at.get(p).copied());
                let adopts = self.unseen_parents.remove(&l.id);
                let simple = !adopts
                    && match p {
                        Some(p) => {
                            !self.has_child[p]
                                && self.on_main[p]
                                && !self.exempt[p]
                                && Some(self.root_of[p]) == self.winner
                        }
                        None => i == 0,
                    };
                match (p, &l.parent) {
                    (Some(p), _) => self.has_child[p] = true,
                    (None, Some(q)) => {
                        self.unseen_parents.insert(q.clone());
                    }
                    (None, None) => {}
                }
                self.at.insert(l.id.clone(), i);
                self.nodes.push(l);
                self.has_child.push(false);
                self.exempt.push(false);
                self.on_main.push(true);
                self.root_of.push(p.map_or(i, |p| self.root_of[p]));
                if simple {
                    if i == 0 {
                        self.winner = Some(0);
                    }
                    return false;
                }
                self.recompute()
            }
        }
    }

    /// 主线外的那几条（只含进界面的；文件序）。
    pub(crate) fn off(&self) -> &[String] {
        &self.off
    }

    fn recompute(&mut self) -> bool {
        let m = main_branch(&self.nodes);
        let exempt = exempt_queued(&self.nodes, &m.on, &self.queued);
        let off: Vec<String> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, n)| n.shown && !m.on[*i] && !exempt[*i])
            .map(|(_, n)| n.id.clone())
            .collect();
        self.on_main = m.on;
        self.root_of = m.root_of;
        self.winner = m.winner;
        self.exempt = exempt;
        if off == self.off {
            return false;
        }
        self.off = off;
        true
    }
}

/// 一批事实 ⇒ 主线外清单（冷读那一路；与 [`Chain`] 逐条喂的结果相同）。
pub(crate) fn off_of(facts: impl IntoIterator<Item = ChainFact>) -> Vec<String> {
    let mut c = Chain::new();
    for f in facts {
        match f {
            ChainFact::Queued(t) => {
                c.queued.insert(t);
            }
            ChainFact::Node(l) => {
                if !c.at.contains_key(&l.id) {
                    c.at.insert(l.id.clone(), c.nodes.len());
                    c.nodes.push(l);
                }
            }
        }
    }
    c.recompute();
    c.off
}

struct Main {
    on: Vec<bool>,
    root_of: Vec<usize>,
    winner: Option<usize>,
}

/// 主线。身份已去重；`parent` 指向集合外 ＝ 根。全迭代：链几乎线性，递归会炸栈。
fn main_branch(nodes: &[Link]) -> Main {
    let n_all = nodes.len();
    let idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); n_all];
    let mut parent: Vec<Option<usize>> = vec![None; n_all];
    let mut roots = Vec::new();
    for (i, n) in nodes.iter().enumerate() {
        match n.parent.as_deref().and_then(|p| idx.get(p)) {
            Some(&p) if p != i => {
                kids[p].push(i);
                parent[i] = Some(p);
            }
            _ => roots.push(i),
        }
    }
    // 自底向上（Kahn）：子树最新时刻 · 子树里最新那句对话及它是不是打断 · 子树里有没有回复。
    let mut latest: Vec<&str> = nodes.iter().map(|n| n.at.as_str()).collect();
    let mut conv_at: Vec<&str> = vec![""; n_all];
    let mut conv_int = vec![false; n_all];
    let mut has_reply = vec![false; n_all];
    let mut remaining: Vec<usize> = kids.iter().map(Vec::len).collect();
    let mut queue: Vec<usize> = (0..n_all).filter(|&i| remaining[i] == 0).collect();
    let mut done = vec![false; n_all];
    let mut qh = 0;
    while qh < queue.len() {
        let i = queue[qh];
        qh += 1;
        let n = &nodes[i];
        let conv = n.said || n.reply;
        let mut max = n.at.as_str();
        let mut cat = if conv { n.at.as_str() } else { "" };
        let mut cint = conv && n.interrupt;
        let mut hr = n.reply;
        for &k in &kids[i] {
            if latest[k] > max {
                max = latest[k];
            }
            if conv_at[k] > cat {
                cat = conv_at[k];
                cint = conv_int[k];
            }
            hr |= has_reply[k];
        }
        latest[i] = max;
        conv_at[i] = cat;
        conv_int[i] = cint;
        has_reply[i] = hr;
        done[i] = true;
        if let Some(p) = parent[i] {
            remaining[p] -= 1;
            if remaining[p] == 0 {
                queue.push(p);
            }
        }
    }
    // 剩下的 ＝ 环（追加写的记录不该有；防御）：信号退化成自身。
    for i in (0..n_all).filter(|&i| !done[i]) {
        let n = &nodes[i];
        let conv = n.said || n.reply;
        conv_at[i] = if conv { n.at.as_str() } else { "" };
        conv_int[i] = conv && n.interrupt;
        has_reply[i] = n.reply;
    }
    let mut root_of: Vec<usize> = (0..n_all).collect();
    for i in 0..n_all {
        let mut r = i;
        let mut hops = 0;
        while let Some(p) = parent[r] {
            if p < i {
                r = root_of[p];
                break;
            }
            r = p;
            hops += 1;
            if hops > n_all {
                break; // 环
            }
        }
        root_of[i] = r;
    }
    let winner = roots.iter().copied().fold(None::<usize>, |w, r| match w {
        Some(w) if latest[r] <= latest[w] => Some(w),
        _ => Some(r),
    });
    let mut on = vec![false; n_all];
    for &root in &roots {
        if Some(root) != winner && nodes[root].said && (!has_reply[root] || conv_int[root]) {
            continue; // 被回撤废弃的首条 / 重发：整棵在主线外
        }
        let mut cur = root;
        loop {
            if on[cur] {
                break; // 环防御
            }
            on[cur] = true;
            let ks = &kids[cur];
            let Some(&first) = ks.first() else { break };
            cur = ks[1..]
                .iter()
                .fold(first, |w, &k| if latest[k] > latest[w] { k } else { w });
        }
    }
    Main {
        on,
        root_of,
        winner,
    }
}

/// 排队豁免：不在主线上、没有孩子的人那一侧裸叶，原文在排队集合里 ⇒ 并回主线。回豁免的那几条。
fn exempt_queued(nodes: &[Link], on: &[bool], queued: &HashSet<String>) -> Vec<bool> {
    let mut out = vec![false; nodes.len()];
    if queued.is_empty() {
        return out;
    }
    let has_child: HashSet<&str> = nodes.iter().filter_map(|n| n.parent.as_deref()).collect();
    for (i, n) in nodes.iter().enumerate() {
        out[i] = n.said
            && !on[i]
            && !has_child.contains(n.id.as_str())
            && n.text.as_ref().is_some_and(|t| queued.contains(t));
    }
    out
}

#[cfg(test)]
#[path = "../../../tests/backend/agents/mainline_tests.rs"]
mod tests;
