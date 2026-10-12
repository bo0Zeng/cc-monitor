//! **账号之间同步 MCP 登录** —— 纯：上次同步时各号有哪几条（底）＋ 各号此刻凭据文件里那一节 ⇒ 一份计划。
//! 一个字节都不写；读盘与落盘在 [`super::mcp_logins_exec`]。
//!
//! 为什么不能只抄一次：很多 MCP 服务每续一次就发一个新的刷新令牌、旧的当场作废 ⇒ 一份一续，别的副本就全废了。
//! 所以一个号一登录或一续期，就要马上把新的那一条写进所有号；各号的 claude 续期前先在锁里重读盘、用盘上的刷新令牌
//! （10-12 沙箱实测 2.1.296），所以写进去的新令牌下一次续期就用上。
//!
//! 逐条（键 = 那一家给一个服务器算的那一格）、逐个读得出来的号，那一条分三态：没有 · 失效（访问令牌与刷新令牌都空：
//! 那一家续期被拒后清掉的样子）· 能用。
//! - 有号里**没有**这一条、而底里那个号有 ⇒ 用户在那个号里退出了这个登录 ⇒ 所有号都撤（除非别的号此刻有一条
//!   自底以来新来的能用的：新登录压过退出）；
//! - 否则在能用的几条里挑最新的：过期时刻最晚的那条；一样晚 ⇒ 自底以来变过的那条；再一样 ⇒ 先列的号。
//!   别的号那一条与它不同（没有 · 失效 · 旧的）⇒ 写成它。失效的那一条永远不往外传。
//! - 一条能用的都没有 ⇒ 不动。
//!
//! 写的时候只换那一节里那一条，带上「写前比对」要的指纹（计划时看到的那一条）；落不下的那个号，底换回原来的。

use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// 一节登录（键 → 那一条原样）。
pub(crate) type Entries = Map<String, Value>;

/// 底：配置目录 → （键 → 上次同步之后那一条的指纹）。
pub(crate) type LoginBase = BTreeMap<String, BTreeMap<String, String>>;

/// 一条登录里通用层要读的那几格的名字（那一家的格式住适配层）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Cells {
    pub access: &'static str,
    pub refresh: &'static str,
    pub expires_ms: &'static str,
}

/// 一个号此刻的那一节。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SeenLogins {
    pub dir: String,
    /// `None` = 凭据文件不在或读不出来：这个号这一趟不参与，底不动。
    pub now: Option<Entries>,
}

/// 要落的一下：把 `dir` 那个号的 `key` 那一条换成 `to`（`None` = 删掉这一条）。
/// `expect` = 计划时看到的那一条的指纹（`None` = 那时没有这一条）：落盘前盘上已经不是它 ⇒ 不写。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LoginWrite {
    pub dir: String,
    pub key: String,
    pub to: Option<Value>,
    pub expect: Option<String>,
}

/// 一趟对照的结果。
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct LoginPlan {
    pub writes: Vec<LoginWrite>,
    /// 假定 [`LoginPlan::writes`] 全落下了之后的底（没落下的那个号由执行器把底换回原来的）。
    pub base: LoginBase,
}

fn text<'a>(e: &'a Value, cell: &str) -> Option<&'a str> {
    e.get(cell)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// 这一条能不能用：访问令牌或刷新令牌至少有一个。
pub(crate) fn live(e: &Value, c: &Cells) -> bool {
    text(e, c.access).is_some() || text(e, c.refresh).is_some()
}

fn expires(e: &Value, c: &Cells) -> u64 {
    e.get(c.expires_ms).and_then(Value::as_u64).unwrap_or(0)
}

/// 一条的指纹：只取令牌那三格（访问 · 刷新 · 过期时刻），算摘要的前 16 位十六进制。不可逆，存进底不露令牌。
pub(crate) fn fingerprint(e: &Value, c: &Cells) -> String {
    let joined = format!(
        "{}\0{}\0{}",
        text(e, c.access).unwrap_or(""),
        text(e, c.refresh).unwrap_or(""),
        expires(e, c)
    );
    let mut d = crate::files::content_sha256(joined.as_bytes());
    d.truncate(16);
    d
}

/// 三态对照（见模块头注）。
pub(crate) fn plan(base: &LoginBase, seen: &[SeenLogins], c: &Cells) -> LoginPlan {
    let empty = BTreeMap::new();
    let base_of = |dir: &str| base.get(dir).unwrap_or(&empty);
    let readable: Vec<(&SeenLogins, &Entries)> = seen
        .iter()
        .filter_map(|s| s.now.as_ref().map(|n| (s, n)))
        .collect();
    let mut keys: BTreeSet<&str> = BTreeSet::new();
    for (s, n) in &readable {
        keys.extend(n.keys().map(String::as_str));
        keys.extend(base_of(&s.dir).keys().map(String::as_str));
    }
    let mut writes = Vec::new();
    let mut next: LoginBase = BTreeMap::new();
    for (s, _) in &readable {
        next.insert(s.dir.clone(), BTreeMap::new());
    }
    for k in keys {
        // （那一条, 号, 自底以来变过没有）
        let mut cands: Vec<(&Value, &str, bool)> = Vec::new();
        let mut removed = false;
        for (s, n) in &readable {
            let b = base_of(&s.dir).get(k);
            match n.get(k) {
                None => removed |= b.is_some(),
                Some(e) if live(e, c) => {
                    cands.push((e, &s.dir, b != Some(&fingerprint(e, c))));
                }
                Some(_) => {}
            }
        }
        let new_since = cands.iter().any(|x| x.2);
        let target: Option<Option<&Value>> = if removed && !new_since {
            Some(None)
        } else {
            let mut best: Option<&(&Value, &str, bool)> = None;
            for x in &cands {
                let better = match best {
                    None => true,
                    Some(b) => (expires(x.0, c), x.2) > (expires(b.0, c), b.2),
                };
                if better {
                    best = Some(x);
                }
            }
            best.map(|b| Some(b.0))
        };
        for (s, n) in &readable {
            let cur = n.get(k);
            let cur_fp = cur.map(|e| fingerprint(e, c));
            let landed: Option<String> = match target {
                // 没有能用的：不动，底照此刻记。
                None => cur_fp.clone(),
                Some(None) => {
                    if cur.is_some() {
                        writes.push(LoginWrite {
                            dir: s.dir.clone(),
                            key: k.to_string(),
                            to: None,
                            expect: cur_fp.clone(),
                        });
                    }
                    None
                }
                Some(Some(w)) => {
                    let want = fingerprint(w, c);
                    if cur_fp.as_deref() != Some(want.as_str()) {
                        writes.push(LoginWrite {
                            dir: s.dir.clone(),
                            key: k.to_string(),
                            to: Some(w.clone()),
                            expect: cur_fp.clone(),
                        });
                    }
                    Some(want)
                }
            };
            if let Some(fp) = landed {
                next.entry(s.dir.clone())
                    .or_default()
                    .insert(k.to_string(), fp);
            }
        }
    }
    for s in seen.iter().filter(|s| s.now.is_none()) {
        if let Some(b) = base.get(&s.dir) {
            next.insert(s.dir.clone(), b.clone());
        }
    }
    next.retain(|_, m| !m.is_empty());
    LoginPlan { writes, base: next }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/mcp_logins_tests.rs"]
mod tests;
