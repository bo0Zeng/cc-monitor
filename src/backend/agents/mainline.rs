//! **主线外清单**（ESC 回退掉的那几条）—— 通用层算，只吃适配层给的链事实（[`ChainFact`]），不认任何一家的字段名。
//!
//! 一家的记录若是一棵树（每条记着上一条是谁），用户在某条之前回退重发时，旧的那一支留在文件里、新的一支从同一个上一条长出来。
//! 主线 ＝ 每个分叉点选「子树里最新」的那一支；多个根时，只有「人说的、没等到回复（或最后一句是打断）」的那种死胡同根整棵算主线外，
//! 压缩边界那类非对话根与完整对话的根都留。排队消息（人在一轮跑着时插进去的话）在链上是一片裸叶，靠「它的原文在排队集合里」并回主线。
//!
//! 记录发出去时是主线、之后可能变成主线外（用户在它前面回退重发）⇒ 清单单独给（实时帧 `session_branch` 整份 · 冷读 `history-branch`），
//! 不做成每条记录上的标记。没有链的那一家不出事实 ⇒ 清单恒空。

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU64;

/// 人那一侧一句话的摘要（去首尾空白后的 64 位 FNV-1a）：排队豁免只比两句是不是同一句，原文不进链。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) struct Words(NonZeroU64);

impl Words {
    pub(crate) fn of(text: &str) -> Self {
        let h = crate::common::digest::Fnv::default()
            .part(text.trim().as_bytes())
            .value();
        Words(NonZeroU64::new(h).unwrap_or(NonZeroU64::MIN))
    }
}

/// 链上一个节点的事实（适配层从一条记录翻过来）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
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
    /// 人那一侧这条的原文摘要（排队豁免按它配）；不是人那一侧 ⇒ 缺。
    pub(crate) text: Option<Words>,
    /// 这条进不进界面（出记录成品）：只有进界面的才进清单，链上纯占位的那些不进。
    pub(crate) shown: bool,
}

/// 适配层从一行翻出的链事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChainFact {
    /// 链上一个节点。
    Node(Link),
    /// 一句排进队列的话（摘要）：它对应的那片裸叶不算回退。
    Queued(Words),
}

/// 链索引里一个节点：上一条按下标记（还没见到的那一条记在 `Chain.waiting`），时刻住 `Chain.times`，身份只住 `Chain.index`。
#[derive(Debug, Clone)]
struct Node {
    parent: Option<u32>,
    /// 这条的时刻在 `times` 里止于哪（起点 ＝ 上一个节点的止点）。
    at_end: u32,
    text: Option<Words>,
    said: bool,
    reply: bool,
    interrupt: bool,
    shown: bool,
}

/// 一份会话记录的链索引：每个节点只记下标 · 时刻 · 几个标志 · 原文摘要。可以抄一份接着记（扫描图续扫）；骨架落盘那一形见 [`skel`]。
#[derive(Debug, Default, Clone)]
pub(crate) struct Chain {
    /// 身份 ⇒ 下标；身份只存这一份（清单出身份时按下标反查它）。
    index: HashMap<Box<str>, u32>,
    nodes: Vec<Node>,
    /// 各节点的时刻首尾相接。
    times: String,
    /// 说到过、还没见到的上一条 ⇒ 等它的那几个下标（它一到就接上 ⇒ 整份重算）。
    waiting: HashMap<Box<str>, Vec<u32>>,
    /// 有没有见到接在它下面的（排队豁免只给裸叶）。
    has_child: Vec<bool>,
    queued: HashSet<Words>,
    /// 上一次算的结果：每个节点在不在主线上 · 是不是排队豁免回来的 · 属于哪个根 · 胜出的根。
    on_main: Vec<bool>,
    /// `on_main` 里有没有 `false`（重算时定；简单延长只追加 `true`，不改它）—— 排队那一条每来一行都问，不逐格数。
    any_off_main: bool,
    exempt: Vec<bool>,
    root_of: Vec<u32>,
    winner: Option<u32>,
    off: Vec<String>,
    /// 自上一次 [`Chain::take_kept`] 起清单变过 ⇒ 那时的清单与此刻的公共前缀至多这么长（`session_branch` 只发变了的那一截）；没变过 ⇒ `None`。
    kept: Option<usize>,
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
        let l = match fact {
            ChainFact::Queued(w) => {
                let fresh = self.queued.insert(w);
                return fresh && (self.dirty || self.any_off_main);
            }
            ChainFact::Node(l) => l,
        };
        if self.index.contains_key(l.id.as_str()) {
            return false; // 重复投递：保首见
        }
        let i = self.nodes.len() as u32;
        let adopted = self.waiting.remove(l.id.as_str()).unwrap_or_default();
        self.index.insert(l.id.into_boxed_str(), i);
        // 指向自己的那条照样接上（当根算，见 `main_branch`）。
        let p = l.parent.as_deref().and_then(|q| self.index.get(q).copied());
        let earlier = p.filter(|&p| p < i).map(|p| p as usize);
        let simple = adopted.is_empty()
            && !self.dirty
            && match (p, earlier) {
                (_, Some(p)) => {
                    !self.has_child[p]
                        && self.on_main[p]
                        && !self.exempt[p]
                        && Some(self.root_of[p]) == self.winner
                }
                (Some(_), None) => false,
                (None, _) => i == 0,
            };
        if let (None, Some(q)) = (p, l.parent) {
            self.waiting.entry(q.into_boxed_str()).or_default().push(i);
        }
        if let Some(p) = earlier {
            self.has_child[p] = true;
        }
        for &c in &adopted {
            self.nodes[c as usize].parent = Some(i);
        }
        self.times.push_str(&l.at);
        self.nodes.push(Node {
            parent: p,
            at_end: self.times.len() as u32,
            text: l.text,
            said: l.said,
            reply: l.reply,
            interrupt: l.interrupt,
            shown: l.shown,
        });
        self.has_child.push(!adopted.is_empty() || p == Some(i));
        self.exempt.push(false);
        self.on_main.push(true);
        self.root_of.push(earlier.map_or(i, |p| self.root_of[p]));
        if simple && i == 0 {
            self.winner = Some(0);
        }
        !simple
    }

    /// 主线外的那几条（只含进界面的；文件序）。是上一次算的结果：[`Chain::load`] 之后先 [`Chain::settle`]。
    pub(crate) fn off(&self) -> &[String] {
        debug_assert!(!self.dirty, "记了没算就来读清单");
        &self.off
    }

    /// 上一次问它到此刻，清单前几条没动（主线外清单是文件序，新的分叉一般接在尾上 ⇒ 只有后面那几条是新的）；问完重新记。
    /// 中间变了几次都算上（取最短的公共前缀）⇒ 拿上一次发出去的那一份接上这一截，就是此刻的清单。
    pub(crate) fn take_kept(&mut self) -> usize {
        self.kept.take().unwrap_or(self.off.len())
    }

    /// 记了几个节点。
    pub(crate) fn len(&self) -> usize {
        self.nodes.len()
    }

    /// 到此刻整份重算一共走过几格（见 `walked` 字段）。
    pub(crate) fn walked(&self) -> u64 {
        self.walked
    }

    /// 第 `i` 个节点的时刻。
    fn at(&self, i: usize) -> &str {
        let start = i
            .checked_sub(1)
            .map_or(0, |p| self.nodes[p].at_end as usize);
        &self.times[start..self.nodes[i].at_end as usize]
    }

    /// 按下标排好的身份（清单 · 骨架要）。
    fn ids(&self) -> Vec<&str> {
        let mut ids = vec![""; self.nodes.len()];
        for (id, &i) in &self.index {
            ids[i as usize] = id;
        }
        ids
    }

    fn recompute(&mut self) -> bool {
        self.walked += self.nodes.len() as u64;
        let at: Vec<&str> = (0..self.nodes.len()).map(|i| self.at(i)).collect();
        let m = main_branch(&self.nodes, &at);
        let exempt = exempt_queued(&self.nodes, &m.on, &self.has_child, &self.queued);
        let ids = self.ids();
        let off: Vec<&str> = (0..self.nodes.len())
            .filter(|&i| self.nodes[i].shown && !m.on[i] && !exempt[i])
            .map(|i| ids[i])
            .collect();
        let changed = (!off.iter().eq(self.off.iter())).then(|| {
            let k = off
                .iter()
                .zip(&self.off)
                .take_while(|(a, b)| **a == b.as_str())
                .count();
            (k, off.into_iter().map(str::to_string).collect())
        });
        self.any_off_main = m.on.iter().any(|on| !on);
        self.on_main = m.on;
        self.root_of = m.root_of;
        self.winner = m.winner;
        self.exempt = exempt;
        let Some((k, off)) = changed else {
            return false;
        };
        self.kept = Some(self.kept.map_or(k, |was| was.min(k)));
        self.off = off;
        true
    }
}

/// 骨架落盘那一形（`observe::skeleton`）：只存记进来的事实（节点 ＋ 排队摘要），读回来照原样一条条记、算一次 ——
/// 算出来的那几样（主线 · 豁免 · 清单）不存，跟整份记一遍的结果相同。
pub(crate) mod skel {
    use super::{Chain, ChainFact, Link, Words};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        parent: Option<&'a str>,
        at: &'a str,
        said: bool,
        reply: bool,
        interrupt: bool,
        text: Option<Words>,
        shown: bool,
    }

    struct Rows<'a>(&'a Chain);

    impl Serialize for Rows<'_> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let c = self.0;
            let ids = c.ids();
            let mut up: Vec<Option<&str>> = c
                .nodes
                .iter()
                .map(|n| n.parent.map(|p| ids[p as usize]))
                .collect();
            for (q, kids) in &c.waiting {
                for &k in kids {
                    up[k as usize] = Some(q);
                }
            }
            s.collect_seq(c.nodes.iter().enumerate().map(|(i, n)| Row {
                id: ids[i],
                parent: up[i],
                at: c.at(i),
                said: n.said,
                reply: n.reply,
                interrupt: n.interrupt,
                text: n.text,
                shown: n.shown,
            }))
        }
    }

    #[derive(Serialize)]
    struct FactsRef<'a> {
        nodes: Rows<'a>,
        queued: Vec<Words>,
    }

    #[derive(Deserialize)]
    struct Facts {
        nodes: Vec<Link>,
        queued: Vec<Words>,
    }

    pub(crate) fn serialize<S: Serializer>(c: &Chain, s: S) -> Result<S::Ok, S::Error> {
        let mut queued: Vec<Words> = c.queued.iter().copied().collect();
        queued.sort_unstable_by_key(|w| w.0);
        FactsRef {
            nodes: Rows(c),
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
    root_of: Vec<u32>,
    winner: Option<u32>,
}

/// 主线。身份已去重；没有上一条（或指向自己）＝ 根。全迭代：链几乎线性，递归会炸栈。
fn main_branch(nodes: &[Node], at: &[&str]) -> Main {
    let n_all = nodes.len();
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); n_all];
    let mut parent: Vec<Option<usize>> = vec![None; n_all];
    let mut roots = Vec::new();
    for (i, n) in nodes.iter().enumerate() {
        match n.parent.map(|p| p as usize) {
            Some(p) if p != i => {
                kids[p].push(i);
                parent[i] = Some(p);
            }
            _ => roots.push(i),
        }
    }
    // 自底向上（Kahn）：子树最新时刻 · 子树里最新那句对话及它是不是打断 · 子树里有没有回复。
    let mut latest: Vec<&str> = at.to_vec();
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
        let mut max = at[i];
        let mut cat = if conv { at[i] } else { "" };
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
        conv_at[i] = if conv { at[i] } else { "" };
        conv_int[i] = conv && n.interrupt;
        has_reply[i] = n.reply;
    }
    let mut root_of: Vec<u32> = (0..n_all as u32).collect();
    for i in 0..n_all {
        let mut r = i;
        let mut hops = 0;
        while let Some(p) = parent[r] {
            if p < i {
                r = root_of[p] as usize;
                break;
            }
            r = p;
            hops += 1;
            if hops > n_all {
                break; // 环
            }
        }
        root_of[i] = r as u32;
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
        winner: winner.map(|w| w as u32),
    }
}

/// 排队豁免：不在主线上、没有孩子的人那一侧裸叶，原文在排队集合里 ⇒ 并回主线。回豁免的那几条。
fn exempt_queued(
    nodes: &[Node],
    on: &[bool],
    has_child: &[bool],
    queued: &HashSet<Words>,
) -> Vec<bool> {
    let mut out = vec![false; nodes.len()];
    if queued.is_empty() {
        return out;
    }
    for (i, n) in nodes.iter().enumerate() {
        out[i] = n.said
            && !on[i]
            && !has_child[i]
            && n.text.as_ref().is_some_and(|t| queued.contains(t));
    }
    out
}

#[cfg(test)]
#[path = "../../../tests/backend/agents/mainline_tests.rs"]
pub(crate) mod tests;
