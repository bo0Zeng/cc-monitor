//! **账号之间共用的用户级 MCP** —— 纯：共享集合 ＋ 上次同步时各号的样子（底）＋ 各号此刻的样子 ⇒ 一份计划。
//! 一个字节都不写；读盘与落盘在 [`super::mcp_share_exec`]。
//!
//! 为什么要有底：Claude 运行时会拿内存里的旧内容整份重写配置文件，刚同步进去的条目可能被盖掉。
//! 只看「此刻」分不清「用户删了」与「被盖掉了」，也分不清「用户在这个号里改了」与「被旧内容盖回去了」。
//!
//! 逐条名字、逐个读得出来的号（`c` = 号此刻 · `b` = 底 · `s` = 共享集合）：
//! - `c` 在、`c ≠ b`、`c ≠ s` ⇒ 这个号提出了一版（用户在那个号里加的或改的）；
//! - 没有号提 ⇒ 目标就是 `s`：`c` 缺了 ⇒ 补回（删除只在 cc-monitor 里做）；`c == b ≠ s` ⇒ 盖回；`s` 里没有 ⇒ 从号里撤；
//! - 提的都是同一版、且每个提出者那里 `s == b`（共享那边自它的底以来没动过）⇒ 采纳进共享集合、同步到所有号；
//! - 否则两边都改了 ⇒ 不自动选：这一条整条冻住（不写、底不动），列出各版让用户挑。
//!
//! cc-monitor 里定的（删一条 · 挑一版 · 装到全局）走 [`decide`]：先改共享集合、再把每个号此刻那一条收进底
//! ⇒ 没有号算「提出」，照常对照之后所有号都跟上。

use copy_core::copy_text;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// 一份服务器表（名 → 那一条原样的定义）。
pub(crate) type Servers = Map<String, Value>;

/// 存的格式版本。
const VERSION: u64 = 1;
/// 底那一格的键。
const BASE_KEY: &str = "base";
/// 「停止同步」那一格的键（停着 ⇒ `false`；开着时这一格不写）。
const SYNC_KEY: &str = "sync";

/// 盘上那一份：共享集合 ＋ 每个号（按配置目录）上次同步时的样子。
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Store {
    pub servers: Servers,
    pub base: BTreeMap<String, Servers>,
    /// 用户停了同步：各号各管各的，已经同步过去的不删；开回来那一刻照常三方对照。
    pub paused: bool,
}

impl Store {
    /// 解盘上那份。`key` = 服务器表的键（与各号配置文件里的同名，读用户级 MCP 的几处因此一种读法）。
    /// 形状不对 ⇒ `Err`（这一份不拿来算，更不拿来盖）。报错里只有位置，不带任何值。
    pub(crate) fn parse(text: &str, key: &str) -> Result<Store, String> {
        let shape = || copy_text("beAcctMcpShare.store.badShape", &[]);
        let root: Value =
            serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| {
                copy_text(
                    "beAcctMcpShare.store.badJson",
                    &[("line", &e.line().to_string())],
                )
            })?;
        let Value::Object(mut o) = root else {
            return Err(shape());
        };
        if o.get("version").and_then(Value::as_u64) != Some(VERSION) {
            return Err(shape());
        }
        let servers = match o.remove(key) {
            None => Servers::new(),
            Some(Value::Object(m)) => m,
            Some(_) => return Err(shape()),
        };
        let mut base = BTreeMap::new();
        match o.remove(BASE_KEY) {
            None => {}
            Some(Value::Object(m)) => {
                for (dir, v) in m {
                    let Value::Object(s) = v else {
                        return Err(shape());
                    };
                    base.insert(dir, s);
                }
            }
            Some(_) => return Err(shape()),
        }
        let paused = match o.remove(SYNC_KEY) {
            None => false,
            Some(Value::Bool(on)) => !on,
            Some(_) => return Err(shape()),
        };
        Ok(Store {
            servers,
            base,
            paused,
        })
    }

    /// 写回去的那份原文（两格缩进、末尾换行）。
    pub(crate) fn render(&self, key: &str) -> String {
        let base: Map<String, Value> = self
            .base
            .iter()
            .map(|(d, s)| (d.clone(), Value::Object(s.clone())))
            .collect();
        let mut root = Map::new();
        root.insert("version".to_string(), json!(VERSION));
        root.insert(key.to_string(), Value::Object(self.servers.clone()));
        root.insert(BASE_KEY.to_string(), Value::Object(base));
        if self.paused {
            root.insert(SYNC_KEY.to_string(), json!(false));
        }
        let mut out = serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_default();
        out.push('\n');
        out
    }
}

/// 一个号此刻的样子（读盘那一半交来）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Seen {
    pub name: String,
    pub dir: String,
    /// 那份配置里的服务器表；文件不在 ⇒ 空表；读不出来 ⇒ `None`（这个号这一趟不参与、底不动）。
    pub now: Option<Servers>,
}

/// 冲突里的一版。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Choice {
    /// `None` = 共享集合里的那一版；`Some(号)` = 那个号里的那一版。
    pub from: Option<String>,
    /// 此刻是这一版的那几个号。
    pub holders: Vec<String>,
    /// 这一版是「没有这一条」（共享集合里已经删了）。
    pub gone: bool,
}

/// 两边都改了的一条。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Conflict {
    pub name: String,
    pub choices: Vec<Choice>,
}

/// 一趟对照的结果。
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Plan {
    /// 对照之后的共享集合 ＋ 底（假定 [`Plan::writes`] 全落下了；没落下的那个号由执行器把底换回原来的）。
    pub store: Store,
    /// 要改的号（配置目录 → 那个号的服务器表应当变成的样子）。
    pub writes: Vec<(String, Servers)>,
    /// 采纳进共享集合的条目（名, 从哪个号）。
    pub adopted: Vec<(String, String)>,
    pub conflicts: Vec<Conflict>,
}

fn readable(seen: &[Seen]) -> impl Iterator<Item = (&Seen, &Servers)> {
    seen.iter().filter_map(|s| s.now.as_ref().map(|n| (s, n)))
}

/// 三方对照（见模块头注）。
pub(crate) fn plan(store: &Store, seen: &[Seen]) -> Plan {
    let empty = Servers::new();
    let base_of = |dir: &str| store.base.get(dir).unwrap_or(&empty);
    let mut names: BTreeSet<String> = store.servers.keys().cloned().collect();
    for (a, c) in readable(seen) {
        names.extend(c.keys().cloned());
        names.extend(base_of(&a.dir).keys().cloned());
    }
    let mut servers = store.servers.clone();
    let mut frozen = BTreeSet::new();
    let mut adopted = Vec::new();
    let mut conflicts = Vec::new();
    for n in &names {
        let s = store.servers.get(n);
        // （提出的那一版, 哪个号, 共享那边自这个号的底以来动过没有）
        let mut proposals: Vec<(&Value, &str, bool)> = Vec::new();
        for (a, c) in readable(seen) {
            let b = base_of(&a.dir).get(n);
            if let Some(cv) = c.get(n) {
                if Some(cv) != b && Some(cv) != s {
                    proposals.push((cv, &a.name, s != b));
                }
            }
        }
        let Some(&(first, from, _)) = proposals.first() else {
            continue;
        };
        let agree = proposals.iter().all(|p| p.0 == first);
        let shared_still = proposals.iter().all(|p| !p.2);
        if agree && shared_still {
            servers.insert(n.clone(), first.clone());
            adopted.push((n.clone(), from.to_string()));
        } else {
            frozen.insert(n.clone());
            conflicts.push(conflict_of(n, s, &proposals, seen));
        }
    }
    let mut writes = Vec::new();
    let mut base = BTreeMap::new();
    for (a, c) in readable(seen) {
        let old = base_of(&a.dir);
        let mut want = c.clone();
        let mut next = Servers::new();
        for n in &names {
            if frozen.contains(n) {
                if let Some(v) = old.get(n) {
                    next.insert(n.clone(), v.clone());
                }
                continue;
            }
            match servers.get(n) {
                Some(v) => {
                    want.insert(n.clone(), v.clone());
                    next.insert(n.clone(), v.clone());
                }
                None => {
                    want.remove(n);
                }
            }
        }
        if want != *c {
            writes.push((a.dir.clone(), want));
        }
        base.insert(a.dir.clone(), next);
    }
    for s in seen.iter().filter(|s| s.now.is_none()) {
        if let Some(b) = store.base.get(&s.dir) {
            base.insert(s.dir.clone(), b.clone());
        }
    }
    Plan {
        store: Store {
            servers,
            base,
            paused: store.paused,
        },
        writes,
        adopted,
        conflicts,
    }
}

fn conflict_of(
    name: &str,
    s: Option<&Value>,
    proposals: &[(&Value, &str, bool)],
    seen: &[Seen],
) -> Conflict {
    let holders_of = |v: Option<&Value>| -> Vec<String> {
        readable(seen)
            .filter(|(_, c)| c.get(name) == v)
            .map(|(a, _)| a.name.clone())
            .collect()
    };
    let mut choices = Vec::new();
    if s.is_some() || proposals.iter().any(|p| p.2) {
        choices.push(Choice {
            from: None,
            holders: holders_of(s),
            gone: s.is_none(),
        });
    }
    let mut seen_values: Vec<&Value> = Vec::new();
    for (v, from, _) in proposals {
        if seen_values.contains(v) {
            continue;
        }
        seen_values.push(v);
        choices.push(Choice {
            from: Some((*from).to_string()),
            holders: holders_of(Some(v)),
            gone: false,
        });
    }
    Conflict {
        name: name.to_string(),
        choices,
    }
}

/// cc-monitor 里定了 `name` 该是什么（`None` = 删）：共享集合按它改，每个读得出来的号此刻那一条收进底。
/// 之后照常 [`plan`] ⇒ 没有号算「提出」，所有号都跟上这一版。
pub(crate) fn decide(store: &Store, seen: &[Seen], name: &str, to: Option<Value>) -> Store {
    let mut next = store.clone();
    match to {
        Some(v) => {
            next.servers.insert(name.to_string(), v);
        }
        None => {
            next.servers.remove(name);
        }
    }
    for (a, c) in readable(seen) {
        let b = next.base.entry(a.dir.clone()).or_default();
        match c.get(name) {
            Some(v) => {
                b.insert(name.to_string(), v.clone());
            }
            None => {
                b.remove(name);
            }
        }
    }
    next
}

/// 挑一版：`from` = 那个号里此刻的那一条；`None` = 共享集合里的那一条（共享集合里没有 ⇒ 删）。
/// 那个号不在 / 读不出来 / 里面没有这一条 ⇒ `Err`（那句话说清是哪个号）。
pub(crate) fn pick(
    store: &Store,
    seen: &[Seen],
    name: &str,
    from: Option<&str>,
) -> Result<Option<Value>, String> {
    let Some(acct) = from else {
        return Ok(store.servers.get(name).cloned());
    };
    seen.iter()
        .find(|s| s.name == acct)
        .and_then(|s| s.now.as_ref())
        .and_then(|c| c.get(name))
        .cloned()
        .map(Some)
        .ok_or_else(|| {
            copy_text(
                "beAcctMcpShare.pick.noSuch",
                &[("name", name), ("account", acct)],
            )
        })
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/mcp_share_tests.rs"]
mod tests;
