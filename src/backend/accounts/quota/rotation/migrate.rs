//! 升级换形（一次）：第一版 `rotation.json`（没有 `version`：规则表 `rules` ＋ `defaultRule`，会话带 `source`）⇒ 第二版。
//! 读到第一版的那一下由 [`super::write_locked`] 在锁里写回第二版、原文留一份 `rotation.json.v1`。
//!
//! | 第一版 | 第二版 |
//! |---|---|
//! | 每条 `rules[id]` | `configs[id]`，`strategy: "1"`，名字照留；种子那条名「默认」⇒「默认配置 1」；`tz` 缺 ⇒ 做换形那台的本地时区（4.1.7 以前的后端按本机钟判时段）；那台说不出 ⇒ 不写、标 `tzUnknown`（上屏需手动，不猜 UTC） |
//! | `order` ＋ `enabled` | `accounts`：按 `order`，具名的只留勾上的 |
//! | `cap["*"]` | `acctInit.line`；并给每一行没有自己那一位的线补上 |
//! | `cap[号]` 数 · 分窗口 · `7d:<模型>` | 那一行的 `params.line`（`7d:<模型>` 并进 `7d` 取小的）；`0` ⇒ 整天不用（`slots: [{at: "00:00-24:00", line: 0}]`） |
//! | `cap[号]` 时段 | 那一行的 `params.slots`（`n` ⇒ `line`） |
//! | `stint` | 每行 `params.stint`（号自己的先，没有取整份的；分窗口取 5h，没有取 7d） |
//! | `preempt` · `wait` · `fallback` · `atLimit` | `params.back` · `params.standby_wait` · 那一行 `mode: "standby"` · `params.all_out`（`continue` ⇒ `go`） |
//! | — | `params.delay: 0`；补建「默认配置 2」「默认配置 3」（同名已有 ⇒ 跳过） |
//! | 会话 `follow` · `rule` · `custom` · `parent` | `config: {id, rev, copy?}`（`parent` 追到的那个会话此刻的那份；追不到按 `follow`） |
//! | `blockedAbove` | 丢掉 |
//! | `history[].why` | `leave` ＋ `pick` |

use super::{
    AcctRow, Book, Config, ConfigBody, Leave, OpenConfig, Params, Pick, SessionEntry, SwitchRecord,
    Who, SAVED_BY_SETTINGS, SEEDS,
};
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
    #[serde(default)]
    rev: u64,
    #[serde(default)]
    updated_at: u64,
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
    baseline: super::Baseline,
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
        if !out
            .iter()
            .any(|(x, _)| crate::accounts::quota::strategy::form::overlaps(x, at))
        {
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

/// 一份第一版轮换换成内置策略 1 的配置内容（名字 · 时区由调用方给）。
fn body_of(r: &Rotation, name: &str, local_tz: Option<&str>) -> ConfigBody {
    let all = r.cap.get("*");
    let mut all_line = line_positions(all);
    // `cap["*"]` 没写的那一位 ⇒ 落回更早的 `when.threshold.n`（10-11 那一次升级漏了这一格：除 cap 点了名的号以外全成了用满才换）。
    if let Some(n) = when_threshold(r.when.as_ref()) {
        for p in &mut all_line {
            p.get_or_insert(n);
        }
    }
    let all_stint = stint_of(r.stint.get("*"));
    let row_params = |id: Option<&str>| -> Params {
        let own = id.and_then(|a| r.cap.get(a));
        let mut pos = line_positions(own);
        for i in 0..2 {
            if pos[i].is_none() {
                pos[i] = all_line[i];
            }
        }
        let stint = id.and_then(|a| stint_of(r.stint.get(a))).or(all_stint);
        let standby = id.is_some_and(|a| r.fallback.iter().any(|f| f == a));
        let mut p = Params::new();
        p.insert("line".into(), line_value(pos));
        p.insert("slots".into(), Value::Array(slots_of(own)));
        p.insert(
            "mode".into(),
            Value::from(if standby { "standby" } else { "normal" }),
        );
        p.insert("stint".into(), stint.map_or(Value::Null, Value::from));
        p
    };
    let mut accounts: Vec<AcctRow> = Vec::new();
    for s in &r.order {
        let row = match s {
            Slot::Start { .. } => AcctRow {
                who: Who::Start,
                params: row_params(None),
            },
            Slot::Named(a) if r.enabled.iter().any(|e| e == a) => AcctRow {
                who: Who::Id(a.clone()),
                params: row_params(Some(a)),
            },
            Slot::Named(_) => continue,
        };
        if !accounts.iter().any(|x| x.who == row.who) {
            accounts.push(row);
        }
    }
    let mut acct_init = Params::new();
    acct_init.insert("line".into(), line_value(all_line));
    acct_init.insert("slots".into(), json!([]));
    acct_init.insert("mode".into(), json!("normal"));
    acct_init.insert("stint".into(), all_stint.map_or(Value::Null, Value::from));
    let mut params = Params::new();
    params.insert("back".into(), json!(r.preempt));
    params.insert("delay".into(), json!(0));
    params.insert("standby_wait".into(), json!(r.wait));
    params.insert(
        "all_out".into(),
        json!(if r.at_limit.as_deref() == Some("stop") {
            "stop"
        } else {
            "go"
        }),
    );
    ConfigBody {
        name: name.to_string(),
        strategy: "1".into(),
        accounts,
        params,
        acct_init,
        parked: BTreeMap::new(),
        tz: r.tz.clone().or_else(|| local_tz.map(str::to_string)),
        tz_unknown: r.tz.is_none() && local_tz.is_none(),
    }
}

/// 第一版换号原因 ⇒ `leave` ＋ `pick`（`n` ＝ 换到的号在那份配置里写在第几）。
fn record_of(r: &OldRecord, order: &dyn Fn(&str) -> Option<usize>) -> SwitchRecord {
    let k = match &r.why {
        Value::String(s) => s.clone(),
        Value::Object(m) => m.keys().next().cloned().unwrap_or_default(),
        _ => String::new(),
    };
    let arg = |f: &str| r.why.get(&k).and_then(|o| o.get(f)).cloned();
    let unusable = |why: Value| Leave::Unusable { why };
    let order_pick = || {
        order(&r.to).map(|n| Pick {
            strategy: "1".into(),
            why: json!({"k": "w_order", "n": n}),
        })
    };
    let (leave, pick) = match k.as_str() {
        "full" => (unusable(json!({"k": "@refused"})), None),
        "threshold" => (
            unusable(match (arg("w"), arg("n")) {
                (Some(w), Some(n)) => json!({"k": "@line", "w": w, "n": n}),
                (None, Some(n)) => json!({"k": "@line", "w": "5h", "n": n}),
                _ => json!({"k": "@full"}),
            }),
            None,
        ),
        "off" => (unusable(json!({"k": "@off"})), None),
        "manualHot" => (Leave::Manual, None),
        "manualRestart" => (Leave::ManualRestart, None),
        "held" | "toOverage" => (Leave::Held, None),
        "overage" => (Leave::Overage, None),
        "skipped" => (unusable(json!({"k": "@login"})), None),
        "stint" => (
            Leave::Outranked,
            Some(Pick {
                strategy: "1".into(),
                why: {
                    let n = arg("n").unwrap_or(json!(0));
                    json!({"k": "w_stint", "n": n, "m": n})
                },
            }),
        ),
        "wait" => (
            Leave::Outranked,
            Some(Pick {
                strategy: "1".into(),
                why: json!({"k": "w_wait", "acct": arg("account").unwrap_or(Value::Null), "at": r.from_resets_at}),
            }),
        ),
        _ => (Leave::Outranked, order_pick()),
    };
    SwitchRecord {
        at: r.at,
        at_text: None,
        from: r.from.clone(),
        to: r.to.clone(),
        leave,
        pick,
        from_resets_at: r.from_resets_at,
        from_resets_at_text: None,
        text: None,
    }
}

/// 跟随父会话最多追几层。
const PARENT_DEPTH: usize = 8;

/// ★ 第一版 ⇒ 第二版（整份，只在内存里；写回由调用方在锁里做）。读不懂 ⇒ `Err`（照读不懂办，不覆盖）。
pub(crate) fn from_v1(v: &Value, local_tz: Option<&str>) -> Result<Book, String> {
    let old: OldBook = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let mut rules = old.rules;
    let mut default_rule = old.default_rule;
    if rules.is_empty() {
        rules.insert(
            OLD_SEED_ID.into(),
            Rule {
                name: old_seed_name(),
                rotation: old.default.clone().unwrap_or_default(),
                rev: 1,
                updated_at: 0,
            },
        );
        default_rule = OLD_SEED_ID.into();
    }
    if !rules.contains_key(&default_rule) {
        if let Some((id, _)) = rules.iter().min_by(|a, b| a.1.name.cmp(&b.1.name)) {
            default_rule = id.clone();
        }
    }
    let seed_name = copy_core::copy_text("beRotation.config.seedName", &[("n", "1")]);
    let mut configs: BTreeMap<String, Config> = rules
        .iter()
        .map(|(id, r)| {
            let name = if id == OLD_SEED_ID && r.name == old_seed_name() {
                seed_name.clone()
            } else {
                r.name.clone()
            };
            (
                id.clone(),
                Config {
                    body: body_of(&r.rotation, &name, local_tz),
                    rev: r.rev.max(1),
                    updated_at: r.updated_at,
                    saved_by: SAVED_BY_SETTINGS.into(),
                },
            )
        })
        .collect();
    // 补建「默认配置 2」「默认配置 3」（同名已有 ⇒ 跳过）。
    for n in 1..SEEDS.len() {
        if let Some((id, c)) = super::seed_config(n, 0) {
            let taken = configs
                .values()
                .any(|x| super::name_key(&x.body.name) == super::name_key(&c.body.name));
            if !taken && !configs.contains_key(&id) {
                configs.insert(id, c);
            }
        }
    }
    // 每份照内置策略 1 的声明补齐每一格。
    if let Some(one) = crate::accounts::quota::strategy::fallback() {
        for c in configs.values_mut() {
            if c.body.strategy == "1" {
                c.body = super::doc::filled(&c.body, &one);
            }
        }
    }
    let open_of = |id: &str| OpenConfig {
        id: id.to_string(),
        rev: configs.get(id).map_or(1, |c| c.rev),
        copy: None,
    };
    let default_open = open_of(&default_rule);
    // 每个会话：先定它自己那一层（parent 先记下要追的那一个）。
    let own: BTreeMap<String, Result<OpenConfig, String>> = old
        .sessions
        .iter()
        .map(|(sid, s)| {
            let source = s.source.clone().unwrap_or(match s.follow {
                Some(false) => Source::Custom,
                _ => Source::Follow,
            });
            let o = match source {
                Source::Follow => Ok(default_open.clone()),
                Source::Rule(id) if configs.contains_key(&id) => Ok(open_of(&id)),
                Source::Rule(_) => Ok(default_open.clone()),
                Source::Custom => match &s.custom {
                    Some(c) => {
                        let base = configs.get(&default_rule);
                        let mut body =
                            body_of(c, base.map_or("", |b| b.body.name.as_str()), local_tz);
                        body.tz = base.and_then(|b| b.body.tz.clone()).or(body.tz);
                        body.tz_unknown = body.tz.is_none();
                        if let Some(one) = crate::accounts::quota::strategy::fallback() {
                            body = super::doc::filled(&body, &one);
                        }
                        let same = base.is_some_and(|b| b.body == body);
                        Ok(OpenConfig {
                            copy: (!same).then_some(body),
                            ..default_open.clone()
                        })
                    }
                    None => Ok(default_open.clone()),
                },
                Source::Parent(p) => Err(p),
            };
            (sid.clone(), o)
        })
        .collect();
    let chase = |sid: &str| -> OpenConfig {
        let mut cur = sid.to_string();
        for _ in 0..=PARENT_DEPTH {
            match own.get(&cur) {
                Some(Ok(o)) => {
                    return OpenConfig {
                        copy: None,
                        ..o.clone()
                    }
                }
                Some(Err(p)) => cur = p.clone(),
                None => break,
            }
        }
        default_open.clone()
    };
    let sessions = old
        .sessions
        .iter()
        .map(|(sid, s)| {
            let config = match &own[sid] {
                Ok(o) => o.clone(),
                Err(p) => chase(p),
            };
            let body = configs.get(&config.id).map(|c| &c.body);
            let order = |a: &str| {
                body.and_then(|b| {
                    b.accounts
                        .iter()
                        .position(|r| {
                            r.who == Who::Id(a.to_string()) || (a == s.start && r.who == Who::Start)
                        })
                        .map(|i| i + 1)
                })
            };
            let entry = SessionEntry {
                agent: s.agent.clone(),
                start: s.start.clone(),
                current: s.current.clone(),
                since: s.since,
                seen: s.seen,
                config,
                manual: super::Manual::default(),
                memo: None,
                baseline: s.baseline.clone(),
                history: s.history.iter().map(|r| record_of(r, &order)).collect(),
            };
            (sid.clone(), entry)
        })
        .collect();
    Ok(Book {
        default_config: default_rule,
        configs,
        sessions,
        from_v1: None,
    })
}
