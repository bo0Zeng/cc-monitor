//! 升级链的第一节（一次）：第一版 `rotation.json`（没有 `version`：规则表 `rules` ＋ `defaultRule`，会话带 `source`）⇒ 第二版的形（JSON），
//! 接着由 [`super::upgrade::from_v2`] 换成第三版；写回在 [`super::uses::migrate_at`] 一次做（原文留 `rotation.json.v1`）。
//! 第二版的类型已经删了：这里直接写出第二版的 JSON，只给下一节读。下一个大版本连这一节一起删。
//!
//! | 第一版 | 第二版 |
//! |---|---|
//! | 每条 `rules[id]` | `configs[id]`，`strategy: "1"`，名字照留（没有规则 ⇒ 补一条种子 `r_00000000`，名「默认」、内容照更早一版的 `default`；`defaultRule` 指空 ⇒ 名字最小的那条）；`tz` 缺 ⇒ 做换形那台的本地时区（4.1.7 以前的后端按本机钟判时段）；那台说不出 ⇒ 不写、标 `tzUnknown`（上屏需手动，不猜 UTC） |
//! | `order` ＋ `enabled` | `accounts`：按 `order`，具名的只留勾上的 |
//! | `cap["*"]` | `acctInit.line`；并给每一行没有自己那一位的线补上 |
//! | `cap[号]` 数 · 分窗口 · `7d:<模型>` | 那一行的 `params.line`（`7d:<模型>` 并进 `7d` 取小的）；`0` ⇒ 整天不用（`slots: [{at: "00:00-24:00", line: 0}]`） |
//! | `cap[号]` 时段 | 那一行的 `params.slots`（`n` ⇒ `line`） |
//! | `stint` | 每行 `params.stint`（号自己的先，没有取整份的；分窗口取 5h，没有取 7d） |
//! | `preempt` · `wait` · `fallback` · `atLimit` | `params.back` · `params.standby_wait` · 那一行 `mode: "standby"` · `params.all_out`（`continue` ⇒ `go`） |
//! | — | `params.delay: 0`（「默认配置 2 · 3」不再补：第三版有随包三份） |
//! | 会话 `follow` · `rule` · `custom` · `parent` | `config: {id, rev, copy?}`（`parent` 追到的那个会话此刻的那份；追不到按 `follow`） |
//! | `blockedAbove` | 丢掉 |
//! | `history[].why` | `leave`（只留下一节认的那几种，认不出的 ⇒ `top`）；`at` · `from` · `to` · `fromResetsAt` 照搬 |

use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// 第一版里的每号上限：一个数，或按时段的几段。
#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum CapValue {
    N(u8),
    Slots(Vec<CapSlot>),
}

#[derive(Deserialize, Clone)]
struct CapSlot {
    at: String,
    n: u8,
}

#[derive(Deserialize, Clone, PartialEq)]
#[serde(untagged)]
enum Slot {
    Start { start: bool },
    Named(String),
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Rotation {
    #[serde(default)]
    order: Vec<Slot>,
    #[serde(default)]
    enabled: Vec<String>,
    #[serde(default)]
    at_limit: Option<String>,
    #[serde(default)]
    cap: BTreeMap<String, BTreeMap<String, CapValue>>,
    #[serde(default)]
    stint: BTreeMap<String, BTreeMap<String, u8>>,
    #[serde(default)]
    preempt: bool,
    #[serde(default)]
    fallback: Vec<String>,
    #[serde(default = "wait_default")]
    wait: u8,
    #[serde(default)]
    tz: Option<String>,
    /// 更早的写法（`cap["*"]` 出现之前）：`"full"` 或 `{"threshold": {"n": 1..=99}}`（到 n% 就换），管所有号、两个窗口。
    #[serde(default)]
    when: Option<Value>,
}

/// `when.threshold.n`（没有 / 是 `"full"` / 形状不对 ⇒ 空）。
fn when_threshold(w: Option<&Value>) -> Option<u8> {
    w?.get("threshold")?
        .get("n")?
        .as_u64()
        .and_then(|n| u8::try_from(n).ok())
        .filter(|n| (1..=99).contains(n))
}

fn wait_default() -> u8 {
    40
}

impl Default for Rotation {
    fn default() -> Self {
        Self {
            order: vec![Slot::Start { start: true }],
            enabled: Vec::new(),
            at_limit: None,
            cap: BTreeMap::new(),
            stint: BTreeMap::new(),
            preempt: false,
            fallback: Vec::new(),
            wait: wait_default(),
            tz: None,
            when: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rule {
    name: String,
    rotation: Rotation,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
enum Source {
    Follow,
    Parent(String),
    Custom,
    Rule(String),
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OldRecord {
    at: u64,
    from: String,
    to: String,
    why: Value,
    #[serde(default)]
    from_resets_at: Option<u64>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OldSession {
    agent: String,
    start: String,
    current: String,
    since: u64,
    #[serde(default)]
    source: Option<Source>,
    #[serde(default)]
    follow: Option<bool>,
    #[serde(default)]
    custom: Option<Rotation>,
    #[serde(default)]
    history: Vec<OldRecord>,
    #[serde(default)]
    seen: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldBook {
    #[serde(default)]
    rules: BTreeMap<String, Rule>,
    #[serde(default)]
    default_rule: String,
    /// 更早一版：这台的默认轮换。
    #[serde(default)]
    default: Option<Rotation>,
    #[serde(default)]
    sessions: BTreeMap<String, OldSession>,
}

/// 第一版种子规则的 id（名字是第一版写进去的那个字，照文案表那一格认）。
const OLD_SEED_ID: &str = "r_00000000";

fn old_seed_name() -> String {
    copy_core::copy_text("beRotation.migrate.oldSeedName", &[])
}

/// 一位（`5h` / `7d`）的线：号自己那一行有就取（`7d:<模型>` 并进 `7d` 取小的），`*` 管两位。
fn line_positions(row: Option<&BTreeMap<String, CapValue>>) -> [Option<u8>; 2] {
    let mut out = [None, None];
    let Some(row) = row else { return out };
    let mut put = |i: usize, n: u8| {
        out[i] = Some(out[i].map_or(n, |x: u8| x.min(n)));
    };
    for (k, v) in row {
        let CapValue::N(n) = v else { continue };
        if *n == 0 {
            continue;
        }
        match k.as_str() {
            "*" => {
                put(0, *n);
                put(1, *n);
            }
            "5h" => put(0, *n),
            k if k == "7d" || k.starts_with("7d:") => put(1, *n),
            _ => {}
        }
    }
    out
}

fn line_value(p: [Option<u8>; 2]) -> Value {
    match p {
        [None, None] => Value::Null,
        [Some(a), Some(b)] if a == b => json!(a),
        [a, b] => {
            let mut m = serde_json::Map::new();
            if let Some(a) = a {
                m.insert("5h".into(), json!(a));
            }
            if let Some(b) = b {
                m.insert("7d".into(), json!(b));
            }
            Value::Object(m)
        }
    }
}

/// 一行的时段：号自己那一行的按时段写的几段（`n` ⇒ `line`，重叠的后来者丢掉）；写死的 `0` ⇒ 整天不用。
fn slots_of(row: Option<&BTreeMap<String, CapValue>>) -> Vec<Value> {
    let mut out: Vec<(String, u8)> = Vec::new();
    let Some(row) = row else { return Vec::new() };
    let mut add = |at: &str, n: u8| {
        if !out.iter().any(|(x, _)| overlaps(x, at)) {
            out.push((at.to_string(), n));
        }
    };
    for v in row.values() {
        match v {
            CapValue::N(0) => add("00:00-24:00", 0),
            CapValue::N(_) => {}
            CapValue::Slots(s) => {
                for x in s {
                    add(&x.at, x.n);
                }
            }
        }
    }
    out.into_iter()
        .map(|(at, n)| json!({"at": at, "line": n}))
        .collect()
}

/// 单段预算：一行的 `{窗口键|*: n}` 取 5h，没有取 7d，再没有取 `*`。
fn stint_of(row: Option<&BTreeMap<String, u8>>) -> Option<u8> {
    let row = row?;
    row.get("5h")
        .or_else(|| {
            row.iter()
                .find(|(k, _)| k.starts_with("7d"))
                .map(|(_, v)| v)
        })
        .or_else(|| row.get("*"))
        .copied()
}

/// `"HH:MM"` ⇒ 一天里的分钟（`24:00` 收）。
fn clock_of(s: &str) -> Option<u16> {
    let (h, m) = s.split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
    if m >= 60 || h > 24 || (h == 24 && m != 0) {
        return None;
    }
    Some(h * 60 + m)
}

/// 一段占一天里的哪几截（跨午夜的拆成两截）。
fn pieces(at: &str) -> Vec<(u16, u16)> {
    let Some((a, b)) = at.split_once('-') else {
        return Vec::new();
    };
    match (clock_of(a), clock_of(b)) {
        (Some(f), Some(t)) if f < t => vec![(f, t)],
        (Some(f), Some(t)) if f != t && f < 24 * 60 => vec![(f, 24 * 60), (0, t)],
        _ => Vec::new(),
    }
}

/// 两段有没有重叠的分钟（含跨午夜的）。
fn overlaps(a: &str, b: &str) -> bool {
    pieces(a)
        .iter()
        .any(|(f1, t1)| pieces(b).iter().any(|(f2, t2)| f1 < t2 && f2 < t1))
}

/// 一份第一版轮换换成第二版里内置策略 1 的配置内容（名字 · 时区由调用方给）。
fn body_of(r: &Rotation, name: &str, local_tz: Option<&str>) -> Value {
    let all = r.cap.get("*");
    let mut all_line = line_positions(all);
    // `cap["*"]` 没写的那一位 ⇒ 落回更早的 `when.threshold.n`（10-11 那一次升级漏了这一格：除 cap 点了名的号以外全成了用满才换）。
    if let Some(n) = when_threshold(r.when.as_ref()) {
        for p in &mut all_line {
            p.get_or_insert(n);
        }
    }
    let all_stint = stint_of(r.stint.get("*"));
    let row_params = |id: Option<&str>| -> Value {
        let own = id.and_then(|a| r.cap.get(a));
        let mut pos = line_positions(own);
        for i in 0..2 {
            if pos[i].is_none() {
                pos[i] = all_line[i];
            }
        }
        let stint = id.and_then(|a| stint_of(r.stint.get(a))).or(all_stint);
        let standby = id.is_some_and(|a| r.fallback.iter().any(|f| f == a));
        json!({
            "line": line_value(pos),
            "slots": slots_of(own),
            "mode": if standby { "standby" } else { "normal" },
            "stint": stint,
        })
    };
    let mut accounts: Vec<Value> = Vec::new();
    let mut seen: Vec<Option<String>> = Vec::new();
    for s in &r.order {
        let (who, row) = match s {
            Slot::Start { .. } => (None, json!({"start": true, "params": row_params(None)})),
            Slot::Named(a) if r.enabled.iter().any(|e| e == a) => (
                Some(a.clone()),
                json!({"id": a, "params": row_params(Some(a))}),
            ),
            Slot::Named(_) => continue,
        };
        if !seen.contains(&who) {
            seen.push(who);
            accounts.push(row);
        }
    }
    let tz = r.tz.clone().or_else(|| local_tz.map(str::to_string));
    json!({
        "name": name,
        "strategy": "1",
        "accounts": accounts,
        "params": {
            "back": r.preempt,
            "delay": 0,
            "standby_wait": r.wait,
            "all_out": if r.at_limit.as_deref() == Some("stop") { "stop" } else { "go" },
        },
        "acctInit": {"line": line_value(all_line), "slots": [], "mode": "normal", "stint": all_stint},
        "tz": tz,
        "tzUnknown": r.tz.is_none() && local_tz.is_none(),
    })
}

/// 第一版换号原因 ⇒ 第二版的 `leave`（只留下一节认的那几种）。
fn record_of(r: &OldRecord) -> Value {
    let k = match &r.why {
        Value::String(s) => s.clone(),
        Value::Object(m) => m.keys().next().cloned().unwrap_or_default(),
        _ => String::new(),
    };
    let leave = match k.as_str() {
        "full" | "threshold" | "off" | "skipped" => "unusable",
        "manualHot" => "manual",
        "manualRestart" => "manualRestart",
        "held" | "toOverage" => "held",
        "overage" => "overage",
        _ => "top",
    };
    json!({
        "at": r.at,
        "from": r.from,
        "to": r.to,
        "leave": {"k": leave},
        "fromResetsAt": r.from_resets_at,
    })
}

/// 跟随父会话最多追几层。
const PARENT_DEPTH: usize = 8;

/// ★ 第一版 ⇒ 第二版的 JSON（整份，只在内存里）。读不懂 ⇒ `Err`（照读不懂办，不覆盖）。
pub(crate) fn from_v1(v: &Value, local_tz: Option<&str>) -> Result<Value, String> {
    let old: OldBook = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let mut rules = old.rules;
    let mut default_rule = old.default_rule;
    if rules.is_empty() {
        rules.insert(
            OLD_SEED_ID.into(),
            Rule {
                name: old_seed_name(),
                rotation: old.default.clone().unwrap_or_default(),
            },
        );
        default_rule = OLD_SEED_ID.into();
    }
    if !rules.contains_key(&default_rule) {
        if let Some((id, _)) = rules.iter().min_by(|a, b| a.1.name.cmp(&b.1.name)) {
            default_rule = id.clone();
        }
    }
    let configs: BTreeMap<String, Value> = rules
        .iter()
        .map(|(id, r)| (id.clone(), body_of(&r.rotation, &r.name, local_tz)))
        .collect();
    // 每个会话：先定它自己那一层（parent 先记下要追的那一个）：（打开哪份, 副本）。
    type Open = (String, Option<Value>);
    let own: BTreeMap<String, Result<Open, String>> = old
        .sessions
        .iter()
        .map(|(sid, s)| {
            let source = s.source.clone().unwrap_or(match s.follow {
                Some(false) => Source::Custom,
                _ => Source::Follow,
            });
            let o = match source {
                Source::Follow => Ok((default_rule.clone(), None)),
                Source::Rule(id) if configs.contains_key(&id) => Ok((id, None)),
                Source::Rule(_) => Ok((default_rule.clone(), None)),
                Source::Custom => match &s.custom {
                    Some(c) => {
                        let base = configs.get(&default_rule);
                        let name = base
                            .and_then(|b| b.get("name"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        let mut body = body_of(c, name, local_tz);
                        if let Some(tz) = base.and_then(|b| b.get("tz")).filter(|t| !t.is_null()) {
                            body["tz"] = tz.clone();
                            body["tzUnknown"] = Value::Bool(false);
                        }
                        let same = base == Some(&body);
                        Ok((default_rule.clone(), (!same).then_some(body)))
                    }
                    None => Ok((default_rule.clone(), None)),
                },
                Source::Parent(p) => Err(p),
            };
            (sid.clone(), o)
        })
        .collect();
    let chase = |sid: &str| -> Open {
        let mut cur = sid.to_string();
        for _ in 0..=PARENT_DEPTH {
            match own.get(&cur) {
                Some(Ok(o)) => return (o.0.clone(), None),
                Some(Err(p)) => cur = p.clone(),
                None => break,
            }
        }
        (default_rule.clone(), None)
    };
    let sessions: serde_json::Map<String, Value> = old
        .sessions
        .iter()
        .map(|(sid, s)| {
            let (id, copy) = match &own[sid] {
                Ok(o) => o.clone(),
                Err(p) => chase(p),
            };
            let mut config = json!({"id": id, "rev": 1});
            if let Some(c) = copy {
                config["copy"] = c;
            }
            (
                sid.clone(),
                json!({
                    "agent": s.agent,
                    "start": s.start,
                    "current": s.current,
                    "since": s.since,
                    "seen": s.seen,
                    "config": config,
                    "history": s.history.iter().map(record_of).collect::<Vec<_>>(),
                }),
            )
        })
        .collect();
    Ok(json!({
        "version": 2,
        "defaultConfig": default_rule,
        "configs": configs,
        "sessions": sessions,
    }))
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/setup/migrate_tests.rs"]
mod tests;
