//! 第二版 `rotation.json` → 第三版。纯函数：读第二版的原文（JSON），出第三版 ＋ 要写的策略文件 ＋ 要挪开的旧脚本。
//! 写盘 · 挪文件 · 留 `.v2` 由调用方在锁里做。第二版的每份配置、每个活会话的副本都写成用户自己的策略文件（`轮换-v5-分层.md` §6.7）：
//! 代码照第二版文件里的编号取下面三份升级材料之一，设置照搬。材料只在这里，不进策略目录、不进任何默认。

use super::book::{self, Book};
use serde_json::Value;

/// 换形要的外部事实。
pub(crate) struct Env<'a> {
    /// 「其余的订阅号」那一行展开成的号（照账号库的先后）。
    pub(crate) rest: &'a [String],
    /// 会话此刻还活着（问 pidfile）。
    pub(crate) live: &'a dyn Fn(&str) -> bool,
    /// 会话的标题（没有 ⇒ 用 id 前 8 位）。
    pub(crate) title: &'a dyn Fn(&str) -> Option<String>,
    /// 做这一步那台的本地时区（说不出 ⇒ 空）。
    pub(crate) local_tz: Option<&'a str>,
}

/// 换形的结果。
#[derive(Debug)]
pub(crate) struct Upgraded {
    pub(crate) book: Book,
    /// 要写进策略目录的新文件：（id, 原文）。
    pub(crate) files: Vec<(String, String)>,
    /// 用户自己加的第三版脚本（id），挪到 `strategies/v3/`、不加载。
    pub(crate) old_scripts: Vec<String>,
}

/// 一天里的分钟数（[`super::form::DAY_MIN`]）。
const DAY_MIN: usize = super::form::DAY_MIN as usize;

/// 升级材料：第二版文件里的编号 `"1"` · `"2"` · `"3"` 依次对的代码（头里的设置是空号列表，名字由配置名填）。
const MATERIALS: [&str; 3] = [
    include_str!("upgrade/v2-1.rhai"),
    include_str!("upgrade/v2-2.rhai"),
    include_str!("upgrade/v2-3.rhai"),
];

/// 第二版一行参数里某窗的线：数 ⇒ 两窗都是它；`{5h, 7d}` ⇒ 各自（`null` ＝ 用满为止）；`null` ⇒ 用满为止。
fn line_of(line: Option<&Value>, w: &str) -> u64 {
    match line {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(100),
        Some(Value::Object(m)) => m.get(w).and_then(Value::as_u64).unwrap_or(100),
        _ => 100,
    }
}

/// `"HH:MM-HH:MM"` ⇒ 一天里的分钟（起, 止）；止 ≤ 起 ⇒ 跨午夜。
fn span(at: &str) -> Option<(usize, usize)> {
    let m = |x: &str| -> Option<usize> {
        let (h, mi) = x.trim().split_once(':')?;
        Some(h.parse::<usize>().ok()? * 60 + mi.parse::<usize>().ok()?)
    };
    let (a, b) = at.split_once('-')?;
    Some((m(a)?.min(DAY_MIN), m(b)?.min(DAY_MIN)))
}

/// 一行参数（`line` ＋ `slots`）⇒ 两窗各自的分段（`[{from, to, v}]`，连着一样的并成一段）。第三版的时段线两窗共用一个数。
fn caps_full(p: &Value) -> [Vec<Value>; 2] {
    ["5h", "7d"].map(|w| {
        let mut day = vec![line_of(p.get("line"), w); DAY_MIN];
        for s in p
            .get("slots")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (Some((a, b)), Some(v)) = (
                s.get("at").and_then(Value::as_str).and_then(span),
                s.get("line").map(|l| l.as_u64().unwrap_or(100)),
            ) else {
                continue;
            };
            let mut paint = |from: usize, to: usize| day[from..to].iter_mut().for_each(|x| *x = v);
            if a < b {
                paint(a, b);
            } else {
                paint(a, DAY_MIN);
                paint(0, b);
            }
        }
        let mut segs: Vec<Value> = Vec::new();
        let mut from = 0;
        for i in 1..=DAY_MIN {
            if i == DAY_MIN || day[i] != day[from] {
                segs.push(serde_json::json!({"from": from, "to": i, "v": day[from]}));
                from = i;
            }
        }
        segs
    })
}

/// 整份默认上限：一整天都是 100 的那一窗不写。
fn caps_default(p: &Value) -> Value {
    let full = caps_full(p);
    let mut m = serde_json::Map::new();
    for (w, segs) in ["5h", "7d"].iter().zip(full) {
        let all = segs.len() == 1 && segs[0]["v"] == 100;
        if !all {
            m.insert((*w).to_string(), Value::Array(segs));
        }
    }
    Value::Object(m)
}

/// 一份配置（或会话的副本）换成升级材料认的设置形状（§6.7 第 4 条），照搬每一格：`line` · `slots` ⇒ `caps`；`all_out` ⇒ `capKind`；
/// `mode` 的 `first` · `normal` · `standby` ⇒ 上 · 中 · 下三排（只留用到的、从 0 编号，只有一排不写）；编号 1 的 `standby_wait` ⇒ 备用号的 `delay`；
/// 整份 `delay` ⇒ 不在最上一排、又没另设的号的 `delay`。`back` · `stint` 不搬（§6.7 第 6 条）。
fn settings_of(c: &Value, env: &Env<'_>, base: &Value) -> Value {
    let params = c.get("params").cloned().unwrap_or(Value::Null);
    let init = c.get("acctInit").cloned().unwrap_or(Value::Null);
    let first_strategy = c.get("strategy").and_then(Value::as_str) == Some("1");
    let default_full = caps_full(&init);
    let all_delay = params
        .get("delay")
        .and_then(Value::as_u64)
        .filter(|d| *d > 0);
    let named: Vec<&str> = c
        .get("accounts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| r.get("id").and_then(Value::as_str))
        .collect();
    // （位次：上 ⇒ 0 · 中 ⇒ 1 · 下 ⇒ 2, 那一行）
    let mut rows: Vec<(u8, serde_json::Map<String, Value>)> = Vec::new();
    for r in c
        .get("accounts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let p = r.get("params").cloned().unwrap_or(Value::Null);
        let mode = p.get("mode").and_then(Value::as_str).unwrap_or("normal");
        let rank = match mode {
            "first" => 0,
            "standby" => 2,
            _ => 1,
        };
        let full = caps_full(&p);
        let mut extra = serde_json::Map::new();
        if full != default_full {
            extra.insert(
                "caps".into(),
                serde_json::json!({"5h": full[0].clone(), "7d": full[1].clone()}),
            );
        }
        if first_strategy && mode == "standby" {
            if let Some(d) = params.get("standby_wait").and_then(Value::as_u64) {
                extra.insert("delay".into(), Value::from(d));
            }
        }
        let with = |k: &str, v: Value| {
            let mut m = serde_json::Map::new();
            m.insert(k.into(), v);
            m.extend(extra.clone());
            m
        };
        if r.get("start") == Some(&Value::Bool(true)) {
            rows.push((rank, with("start", Value::Bool(true))));
        } else if r.get("rest") == Some(&Value::Bool(true)) {
            for a in env.rest.iter().filter(|a| !named.contains(&a.as_str())) {
                rows.push((rank, with("id", Value::from(a.as_str()))));
            }
        } else if let Some(id) = r.get("id").and_then(Value::as_str) {
            rows.push((rank, with("id", Value::from(id))));
        }
    }
    rows.sort_by_key(|r| r.0);
    let mut used: Vec<u8> = rows.iter().map(|r| r.0).collect();
    used.dedup();
    let top = used.first().copied();
    let accounts: Vec<Value> = rows
        .into_iter()
        .map(|(rank, mut m)| {
            let k = used.iter().position(|x| *x == rank).unwrap_or(0);
            if used.len() > 1 && k > 0 {
                m.insert("row".into(), Value::from(k));
            }
            if let Some(d) = all_delay.filter(|_| Some(rank) != top) {
                m.entry("delay").or_insert(Value::from(d));
            }
            Value::Object(m)
        })
        .collect();
    let mut s = base.clone();
    if let Some(m) = s.as_object_mut() {
        m.insert("caps".into(), caps_default(&init));
        m.insert(
            "capKind".into(),
            Value::from(
                if params.get("all_out").and_then(Value::as_str) == Some("stop") {
                    "hard"
                } else {
                    "soft"
                },
            ),
        );
        m.insert("accounts".into(), Value::Array(accounts));
    }
    s
}

/// 一份配置（或副本）写成 `id` 的策略文件（名字 `name`）：代码照它的编号取升级材料（用户自己的第三版脚本 ⇒ 记下要挪开，代码用编号 1 那份）。
fn place(c: &Value, id: &str, name: &str, env: &Env<'_>, out: &mut Upgraded) -> Result<(), String> {
    let n = c.get("strategy").and_then(Value::as_str).unwrap_or("");
    let idx = match n {
        "2" => 1,
        "3" => 2,
        _ => 0,
    };
    if !matches!(n, "1" | "2" | "3" | "") && !out.old_scripts.iter().any(|x| x == n) {
        out.old_scripts.push(n.to_string());
    }
    let src = MATERIALS[idx];
    let head = super::script::parse(src)
        .map_err(|e| format!("upgrade material {}: {}", idx + 1, e.msg))?
        .head;
    let settings = settings_of(c, env, &head.settings);
    let tz = c
        .get("tz")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| env.local_tz.map(str::to_string));
    let unknown = c.get("tzUnknown") == Some(&Value::Bool(true)) || tz.is_none();
    let h = super::script::Head {
        name: super::script::Text::One(name.to_string()),
        tz: if unknown { None } else { tz },
        tz_unknown: unknown,
        settings,
        ..head
    };
    let text = super::script::with_head(src, &h).map_err(|e| e.msg)?;
    out.files.push((id.to_string(), text));
    Ok(())
}

/// 第二版的换号记录 ⇒ 第三版（后端看得见的那几种；`refresh` · `overage` · 重启换丢）。
fn log_of(h: &Value) -> Option<book::Log> {
    let cause = match h.pointer("/leave/k").and_then(Value::as_str)? {
        "top" => book::Cause::Order,
        "unusable" => book::Cause::Unusable,
        "manual" => book::Cause::UseNow,
        "held" => book::Cause::Hold {
            until: h.get("fromResetsAt").and_then(Value::as_u64),
        },
        _ => return None,
    };
    Some(book::Log {
        at: h.get("at")?.as_u64()?,
        from: h.get("from")?.as_str()?.to_string(),
        to: h.get("to")?.as_str()?.to_string(),
        cause,
        why: None,
    })
}

/// ★ 第二版 → 第三版。
pub(crate) fn from_v2(v: &Value, env: &Env<'_>) -> Result<Upgraded, String> {
    if v.get("version").and_then(Value::as_u64) != Some(2) {
        return Err("not a version 2 rotation.json".into());
    }
    let mut out = Upgraded {
        book: Book {
            version: book::BOOK_VERSION,
            ..Book::default()
        },
        files: Vec::new(),
        old_scripts: Vec::new(),
    };
    let mut ids: std::collections::BTreeMap<String, String> = Default::default();
    for (cid, c) in v
        .get("configs")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let name = c.get("name").and_then(Value::as_str).unwrap_or(cid);
        place(c, cid, name, env, &mut out)?;
        ids.insert(cid.clone(), cid.clone());
    }
    // 新会话照旧不轮换（§2.4：升级出来的不进任何默认）；已有会话照旧用它原来那份。
    for (sid, s) in v
        .get("sessions")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let str_of = |k: &str| s.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let cfg = s
            .pointer("/config/id")
            .and_then(Value::as_str)
            .unwrap_or("");
        let mut strategy = ids.get(cfg).cloned().unwrap_or_default();
        if let Some(copy) = s.pointer("/config/copy").filter(|_| (env.live)(sid)) {
            let title = (env.title)(sid).unwrap_or_else(|| sid.chars().take(8).collect());
            let name = format!(
                "{} · {title}",
                copy.get("name").and_then(Value::as_str).unwrap_or(cfg)
            );
            let fid: String = format!("s_{}", sid.chars().take(8).collect::<String>());
            place(copy, &fid, &name, env, &mut out)?;
            strategy = fid;
        }
        let temp = s
            .pointer("/manual/hot/account")
            .and_then(Value::as_str)
            .map(|a| vec![a.to_string()])
            .unwrap_or_default();
        let log = s
            .get("history")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(log_of)
            .collect();
        out.book.sessions.insert(
            sid.clone(),
            book::Session {
                agent: str_of("agent"),
                start: str_of("start"),
                current: str_of("current"),
                since: s.get("since").and_then(Value::as_u64).unwrap_or(0),
                seen: s.get("seen").and_then(Value::as_u64).unwrap_or(0),
                strategy: book::Uses {
                    id: strategy,
                    rev: String::new(),
                    draft: None,
                },
                temp,
                memo: None,
                wake: book::Wake::default(),
                log,
            },
        );
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/rules/upgrade_tests.rs"]
mod tests;
