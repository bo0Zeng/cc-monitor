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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// 一份会话记录的链索引：每个节点只记几个短串。可以抄一份接着记（扫描图续扫）；骨架落盘那一形见 [`skel`]。
#[derive(Debug, Default, Clone)]
pub(crate) struct Chain {
    nodes: Vec<Link>,
    at: HashMap<String, usize>,
    has_child: Vec<bool>,
    /// 说到过、但还没见到的上一条（它一到，接在它下面的那几条就不再是根 ⇒ 整份重算）。
    unseen_parents: HashSet<String>,
    queued: HashSet<String>,
    /// 上一次算的结果：每个节点在不在主线上 · 是不是排队豁免回来的 · 属于哪个根 · 胜出的根。
    on_main: Vec<bool>,
    /// `on_main` 里有没有 `false`（重算时定；简单延长只追加 `true`，不改它）—— 排队那一条每来一行都问，不逐格数。
    any_off_main: bool,
    exempt: Vec<bool>,
    root_of: Vec<usize>,
    winner: Option<usize>,
    off: Vec<String>,
    /// 记进来了、还没重算（[`Chain::load`] 攒着，[`Chain::settle`] 一次算掉）。
    dirty: bool,
    /// 到此刻整条走过几格（每次整份重算记一遍节点数）：冷接那一趟的日志带它；判据判「一趟读多少行都只走一遍」也数它，不按墙钟。
    walked: u64,
}

impl Chain {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 记一条事实、不重算（要不要重算记下来，[`Chain::settle`] 一次算掉）。
    ///
    /// 大多数新节点接在主线末梢上（上一条在主线上、属于胜出的根、此前没有孩子）⇒ 主线照旧延长一格，不用重算。
    /// 其余（新根 · 分叉 · 接在主线外或豁免叶上 · 补上了先前缺的上一条 · 主线外有东西时排队集合变了）要整份重算 ——
    /// 结果与对全部事实重算一次逐条相同。一次重算是整份的量 ⇒ 一趟读进多少条都是全记完再 `settle` 一次，
    /// 不逐条算（12 万行、上万处分叉 / 排队的记录逐条重算就是平方，hot1）。
    pub(crate) fn load(&mut self, fact: ChainFact) {
        if self.stage(fact) {
            self.dirty = true;
        }
    }

    /// 攒着的事实一次算掉；回：主线外清单比上一次算的变没变。
    pub(crate) fn settle(&mut self) -> bool {
        if !std::mem::take(&mut self.dirty) {
            return false;
        }
        self.recompute()
    }

    /// 记账；回：这条要不要整份重算。已经攒着没算（`dirty`）时上一次的结果不作数 ⇒ 一律记成要重算。
    fn stage(&mut self, fact: ChainFact) -> bool {
        match fact {
            ChainFact::Queued(t) => {
                let fresh = self.queued.insert(t);
                fresh && (self.dirty || self.any_off_main)
            }
            ChainFact::Node(l) => {
                if self.at.contains_key(&l.id) {
                    return false; // 重复投递：保首见
                }
                let i = self.nodes.len();
                let p = l.parent.as_deref().and_then(|p| self.at.get(p).copied());
                let adopts = self.unseen_parents.remove(&l.id);
                let simple = !adopts
                    && !self.dirty
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
                if simple && i == 0 {
                    self.winner = Some(0);
                }
                !simple
            }
        }
    }

    /// 主线外的那几条（只含进界面的；文件序）。是上一次算的结果：[`Chain::load`] 之后先 [`Chain::settle`]。
    pub(crate) fn off(&self) -> &[String] {
        debug_assert!(!self.dirty, "记了没算就来读清单");
        &self.off
    }

    /// 记了几个节点。
    pub(crate) fn len(&self) -> usize {
        self.nodes.len()
    }

    /// 到此刻整份重算一共走过几格（见 `walked` 字段）。
    pub(crate) fn walked(&self) -> u64 {
        self.walked
    }

    fn recompute(&mut self) -> bool {
        self.walked += self.nodes.len() as u64;
        let m = main_branch(&self.nodes);
        let exempt = exempt_queued(&self.nodes, &m.on, &self.queued);
        let off: Vec<String> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, n)| n.shown && !m.on[*i] && !exempt[*i])
            .map(|(_, n)| n.id.clone())
            .collect();
        self.any_off_main = m.on.iter().any(|on| !on);
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

/// 骨架落盘那一形（`observe::skeleton`）：只存记进来的事实（节点 ＋ 排队原文），读回来照原样一条条记、算一次 ——
/// 算出来的那几样（主线 · 豁免 · 清单）不存，跟整份记一遍的结果相同。
pub(crate) mod skel {
    use super::{Chain, ChainFact, Link};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize)]
    struct FactsRef<'a> {
        nodes: &'a [Link],
        queued: Vec<&'a String>,
    }

    #[derive(Deserialize)]
    struct Facts {
        nodes: Vec<Link>,
        queued: Vec<String>,
    }

    pub(crate) fn serialize<S: Serializer>(c: &Chain, s: S) -> Result<S::Ok, S::Error> {
        let mut queued: Vec<&String> = c.queued.iter().collect();
        queued.sort();
        FactsRef {
            nodes: &c.nodes,
            queued,
        }
        .serialize(s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Chain, D::Error> {
        let f = Facts::deserialize(d)?;
        let mut c = Chain::new();
        for n in f.nodes {
            c.load(ChainFact::Node(n));
        }
        for q in f.queued {
            c.load(ChainFact::Queued(q));
        }
        c.settle();
        Ok(c)
    }
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
pub(crate) mod tests;
