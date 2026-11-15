//! 额度每号几行 · 开窗那一判：对金样 `tests/__fixtures__/quota-text.golden.json`（每号每行每格 ＋ `--text` 整段字 ＋ 每号的 `warm`）。
//! 回包照真出口走一遍：显示态那一判在 `show.rs`，字与语气由出口写（`written` · 时刻的字按 `TextClock`）· 再添行与开窗。
use super::*;
use serde_json::Value;

const GOLDEN: &str = "tests/__fixtures__/quota-text.golden.json";

fn golden_path() -> std::path::PathBuf {
    guard_core::addr::repo_root().join(GOLDEN)
}

/// 金样里一个出过数的号：原数 ＋ 显示态（格的字与语气、时刻字都不在，由出口写）。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeenIn {
    agent: String,
    account: String,
    seen_at: u64,
    reading: crate::agents::QuotaReading,
    /// 显示态里只给后端、不上线的那一格（金样的输入照旧写着它）。
    #[serde(default)]
    limiting: Option<String>,
    #[serde(flatten)]
    show: cmd_wire::quota::QuotaShow,
}

/// [`SeenIn`] 的显示态换成后端的行之后。
struct SeenRow {
    agent: String,
    account: String,
    seen_at: u64,
    reading: crate::agents::QuotaReading,
    limiting: Option<String>,
    show: crate::accounts::quota::show::QuotaShow,
}

/// 金样里线上那一份显示态 ⇒ 后端的行（只给后端看的几格先空着，由调用方照金样的输入补）。
fn row_of(w: cmd_wire::quota::QuotaShow) -> crate::accounts::quota::show::QuotaShow {
    use crate::accounts::quota::show::{QuotaShow, SlotShow, WindowShow};
    QuotaShow {
        kind: w.kind,
        state: w.state,
        stale: w.stale,
        limiting: None,
        slots: w
            .slots
            .into_iter()
            .map(|s| SlotShow {
                slot: s.slot,
                pct: s.pct,
                resets_at: s.resets_at,
                full: s.full,
                text: s.text,
                tone: s.tone,
                per_pct_text: s.per_pct_text,
                window: String::new(),
            })
            .collect(),
        login: w.login,
        windows: w
            .windows
            .into_iter()
            .map(|x| WindowShow {
                name: x.name,
                key: x.key,
                pct: x.pct,
                resets_at: x.resets_at,
                resets_at_text: None,
                resets_at_rel_text: None,
                seen_at: 0,
                from: x.from,
                tokens_per_pct: x.tokens_per_pct,
                per_pct_text: x.per_pct_text,
                lapsed: None,
                full: x.tone == Tone::Fail,
                near: x.tone != Tone::Plain,
                tone: x.tone,
                row: None,
            })
            .collect(),
        usage: w.usage,
    }
}

/// 一个号的开窗那一判照生产同一条路得出：账上那一条 ⇒ 事实（`facts::fact_of`）⇒ `rules::decide::warm_verdict`。
fn warm_from(
    agent: &str,
    account: &str,
    r: Option<&crate::agents::QuotaReading>,
    kind: cmd_wire::quota::Kind,
    login: cmd_wire::quota::LoginState,
    now: u64,
) -> crate::accounts::quota::rules::decide::WarmVerdict {
    use crate::accounts::quota::facts::{fact_of, Measured};
    let slot = crate::agents::route_named(agent).and_then(crate::agents::window_slot_of);
    let slot = |w: &str| slot.and_then(|f| f(w));
    let m = Measured {
        rate: &|_| None,
        per_pct: &|_| None,
    };
    let f = fact_of(account, r, kind, login, None, now, &slot, &m, &|t| {
        t.to_string()
    });
    crate::accounts::quota::rules::decide::warm_verdict(
        &crate::accounts::quota::rules::Acct::of_fact(agent, &f),
        now,
    )
}

/// 金样的输入只记显示态的各语义位、不记原窗口：照那几格还原原窗口（5h ⇒ five_hour · 7d ⇒ seven_day）。
fn windows_from_slots(
    slots: &[crate::accounts::quota::show::SlotShow],
) -> Vec<crate::agents::QuotaWindow> {
    slots
        .iter()
        .map(|s| crate::agents::QuotaWindow {
            name: if s.slot == "5h" {
                "five_hour"
            } else {
                "seven_day"
            }
            .into(),
            used: s.pct.map(|p| f64::from(p) / 100.0),
            resets_at: s.resets_at,
            warned_at: None,
        })
        .collect()
}

/// 一份回包照真出口走一遍（[`crate::faces::quota_read::reply_of`]，钟按金样那一份的时区）：金样里只留原数与显示态，
/// 字与语气都由出口写（`written` · `Stamp`）。
fn through_the_exit(reply: &Value, tz: &crate::common::time::Tz) -> Value {
    use crate::faces::quota_read::{reply_of, Base, Seen, Unseen, UnseenHead};
    let now = reply["now"].as_u64().unwrap_or_default();
    let seen: Vec<Seen> = reply["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| {
            // 用量那一格由出口写：金样里没有，先占个位。
            let mut a = a.clone();
            a["usage"] = serde_json::json!({"value": "", "text": ""});
            let x: SeenIn = serde_json::from_value(a.clone()).unwrap();
            let mut x = SeenRow {
                show: row_of(x.show),
                agent: x.agent,
                account: x.account,
                seen_at: x.seen_at,
                reading: x.reading,
                limiting: x.limiting,
            };
            // 不上线的那几格从金样的输入里补回去（显示态那一处算好的样子）。
            x.show.limiting = x.limiting.take();
            for (i, w) in x.show.windows.iter_mut().enumerate() {
                let raw = &a["windows"][i];
                w.lapsed = match raw["lapsed"].as_str() {
                    Some("reading") => Some(crate::accounts::quota::show::Lapsed::Reading),
                    Some("reset") => Some(crate::accounts::quota::show::Lapsed::Reset),
                    _ => None,
                };
                w.seen_at = raw["seenAt"].as_u64().unwrap_or_default();
            }
            // 金样的输入只记各语义位、没记窗口的：各窗口照生产那一处（`windows_of`）从还原的原窗口算（5h ⇒ five_hour · 7d ⇒ seven_day）。
            if x.show.windows.is_empty() {
                let mut o = crate::accounts::quota::ledger::Observed {
                    agent: x.agent.clone(),
                    account: x.account.clone(),
                    seen_at: x.seen_at,
                    reading: x.reading.clone(),
                    windows_seen: Default::default(),
                    trend: Default::default(),
                    per_pct: Default::default(),
                    history: Default::default(),
                    reread_failed: Default::default(),
                };
                if o.reading.windows.is_empty() {
                    o.reading.windows = windows_from_slots(&x.show.slots);
                }
                let key = |w: &str| crate::agents::claudecode::quota::key_of(w);
                x.show.windows = crate::accounts::quota::show::windows_of_with(&o, &key, now, true);
            }
            // 语义位那一格取的是哪个窗口（不上线）：照窗口键与语义位同名的那个补回去。
            for s in &mut x.show.slots {
                if let Some(w) = x
                    .show
                    .windows
                    .iter()
                    .find(|w| w.key.as_deref() == Some(s.slot.as_str()))
                {
                    s.window = w.name.clone();
                }
            }
            // 金样的输入只记显示态的各语义位、不记原窗口：开窗那一判要的事实照那几格还原（5h ⇒ five_hour · 7d ⇒ seven_day）。
            let mut facts_in = x.reading.clone();
            if facts_in.windows.is_empty() {
                facts_in.windows = windows_from_slots(&x.show.slots);
            }
            let warm = warm_from(
                &x.agent,
                &x.account,
                Some(&facts_in),
                x.show.kind,
                x.show.login,
                now,
            );
            Seen {
                agent: x.agent,
                account: x.account,
                seen_at: x.seen_at,
                reading: x.reading,
                show: x.show,
                warm,
            }
        })
        .collect();
    let unseen: Vec<Unseen> = reply["unseen"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|u| {
            let mut u = u.clone();
            u["usage"] = serde_json::json!({"value": "", "text": "", "tone": "plain"});
            let route = u["agent"].as_str().unwrap_or_default().to_string();
            let mut h: UnseenHead = serde_json::from_value(u).unwrap();
            // 金样里记的是账上的路由名；造没出过数的号那一处（`rotation_face`）换成线上的 kind。
            h.agent = crate::agents::kind_of_route(h.agent.as_str());
            let warm = warm_from(&route, &h.account, None, h.kind, h.login, now);
            Unseen { head: h, warm }
        })
        .collect();
    use crate::accounts::quota::ledger::ReadState;
    let state = match reply["state"].as_str() {
        Some("present") => ReadState::Present,
        Some("absent") => ReadState::Absent,
        _ => ReadState::Unreadable,
    };
    let base = Base {
        state,
        reason: reply["reason"].as_str().map(str::to_string),
        detail: reply["detail"].as_str().map(str::to_string),
        path: reply["path"].as_str().map(str::to_string),
        now,
        // 金样只记号名：显示只看号名，家空着。
        usable: serde_json::from_value::<Vec<String>>(reply["usable"].clone())
            .unwrap_or_default()
            .into_iter()
            .map(|a| (String::new(), a))
            .collect(),
        earliest: reply["earliest"]["account"].as_str().map(|a| {
            (
                (String::new(), a.to_string()),
                reply["earliest"]["at"].as_u64().unwrap_or_default(),
            )
        }),
    };
    let clock = crate::common::time::TextClock::new(now, tz);
    serde_json::to_value(reply_of(base, seen, unseen, &clock)).unwrap()
}

fn texts(rows: &Value) -> Value {
    Value::Array(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|r| {
                Value::Array(
                    r.as_array()
                        .unwrap()
                        .iter()
                        .map(|c| c["text"].clone())
                        .collect(),
                )
            })
            .collect(),
    )
}

/// ★★ 每个号的几行、`--text` 的整段字、开窗那一判，逐字对金样（输入与期望同住一份，现打的那份是把期望格换成现算的）。
#[test]
fn rows_text_and_warm_match_the_golden() {
    let raw = std::fs::read_to_string(golden_path()).unwrap();
    let mut g: Value = serde_json::from_str(&raw).unwrap();
    let cases = g["cases"].as_array_mut().unwrap();
    assert!(cases.len() >= 10, "金样读空了");
    for c in cases.iter_mut() {
        let tz = crate::common::time::Tz::of(c["tz"].as_str());
        let v = through_the_exit(&c["reply"], &tz);
        let blocks: Vec<Value> = ["accounts", "unseen"]
            .iter()
            .flat_map(|l| v[*l].as_array().cloned().unwrap_or_default())
            .map(|x| serde_json::json!({"account": x["account"], "rows": texts(&x["rows"]), "warm": x["warm"]}))
            .collect();
        let got = (
            Value::Array(blocks),
            Value::String(crate::wire::ship_text::ship_text(&v)),
        );
        c["blocks"] = got.0;
        c["text"] = got.1;
    }
    test_meters::golden::assert_golden(
        GOLDEN,
        &format!("{}\n", serde_json::to_string_pretty(&g).unwrap()),
        "每号的几行 · `--text` · 开窗那一判与金样不一致",
    );
}

/// 开窗那一判的几条边（照生产同一条路：账上那一条 ⇒ 事实 ⇒ accounts 判 ⇒ 这里写）：
/// 没见过数 ⇒ 发；登录拿不到 ⇒ 过一小时再看；在计时 ⇒ 睡到重置之后一分钟；7d 用满 ⇒ 睡到 7d 重置；
/// 按量号 ⇒ 不发（没有 5 小时窗可开）；被拒着 ⇒ 睡到被拒到期之后一分钟。
#[test]
fn warm_sends_only_when_the_window_is_not_ticking() {
    use crate::agents::{QuotaReading, QuotaWindow};
    use cmd_wire::quota::{Kind, LoginState};
    const ROUTE: &str = "claude-code";
    let now: u64 = 1_000_000;
    let clock = crate::common::time::TextClock::new(now, &crate::common::time::Tz::default());
    let win = |name: &str, used: f64, resets_at: u64| QuotaWindow {
        name: name.into(),
        used: Some(used),
        resets_at: Some(resets_at),
        warned_at: None,
    };
    let reading = |windows: Vec<QuotaWindow>, refused: Option<u64>| QuotaReading {
        status: None,
        refused: refused.is_some(),
        limiting: None,
        resets_at: refused,
        windows,
        overage: None,
    };
    let warm = |r: Option<&QuotaReading>, kind: Kind, login: LoginState| {
        warm_of(warm_from(ROUTE, "a", r, kind, login, now), None, &clock)
    };
    let at = |secs: u64| Some(i64::try_from(secs).unwrap());
    assert_eq!(warm(None, Kind::Sub, LoginState::Ok).act, WarmAct::Send);
    assert_eq!(
        warm(None, Kind::Sub, LoginState::NeedsLogin).at,
        at(now + 3600),
        "登录拿不到 ⇒ 过一小时再看"
    );
    let ticking = reading(vec![win("five_hour", 0.4, now + 600)], None);
    assert_eq!(
        warm(Some(&ticking), Kind::Sub, LoginState::Ok).at,
        at(now + 600 + 60)
    );
    let week = reading(
        vec![
            win("five_hour", 0.01, now + 60),
            win("seven_day", 1.0, now + 9000),
        ],
        None,
    );
    assert_eq!(
        warm(Some(&week), Kind::Sub, LoginState::Ok).at,
        at(now + 9000 + 60)
    );
    let expired = reading(vec![win("five_hour", 0.9, now - 5)], None);
    assert_eq!(
        warm(Some(&expired), Kind::Sub, LoginState::Ok).act,
        WarmAct::Send
    );
    let api = warm(None, Kind::Api, LoginState::Ok);
    assert_eq!(api.act, WarmAct::Wait, "按量号没有 5 小时窗可开：{api:?}");
    let api_reading = reading(Vec::new(), Some(now + 300));
    let api_show = crate::accounts::quota::show::QuotaShow {
        kind: Kind::Api,
        state: cmd_wire::quota::QuotaState::Refused,
        stale: false,
        limiting: None,
        slots: Vec::new(),
        login: LoginState::Ok,
        windows: Vec::new(),
        usage: Default::default(),
    };
    let api_refused = warm_of(
        warm_from(
            ROUTE,
            "k",
            Some(&api_reading),
            Kind::Api,
            LoginState::Ok,
            now,
        ),
        Some(&api_show),
        &clock,
    );
    assert_eq!(
        (api_refused.act, api_refused.text.0),
        (WarmAct::Wait, copy_text("reason.code.refused", &[])),
        "按量号被拒：没有窗可指，只说被拒"
    );
    let refused = reading(vec![win("five_hour", 0.2, now + 7200)], Some(now + 300));
    assert_eq!(
        warm(Some(&refused), Kind::Sub, LoginState::Ok).at,
        at(now + 300 + 60),
        "被拒着 ⇒ 等到被拒到期"
    );
}

/// 读不出那一句照读答的原因说（读的哪份 · 原因词）；没给原因才退回「原因不明」那一句。
#[test]
fn the_unreadable_head_says_the_reason() {
    let why = "读取失败 · /h/.cc-monitor/quota.json · 无权限";
    assert_eq!(
        head_text(
            crate::accounts::quota::ledger::ReadState::Unreadable,
            true,
            Some(why)
        )
        .map(|w| w.0),
        Some(why.to_string())
    );
    assert_eq!(
        head_text(
            crate::accounts::quota::ledger::ReadState::Unreadable,
            true,
            None
        )
        .map(|w| w.0),
        Some(copy_text("acct.text.readFail", &[]))
    );
    assert!(head_text(
        crate::accounts::quota::ledger::ReadState::Present,
        false,
        Some(why)
    )
    .is_none());
}

/// ★ `quota-read` 线上形状的金样（Rust · TS · Kotlin 共用这一份）：`quota-text` 金样里每个案例的回包照真出口走完
/// （每格的字与语气 · 时刻字 · 每号 `rows: [[{text, tone}]]` · `warm` · 号名 / 位名 `names`）原样落盘。
#[test]
fn the_wire_golden_is_what_the_exit_writes() {
    let raw = std::fs::read_to_string(golden_path()).unwrap();
    let g: Value = serde_json::from_str(&raw).unwrap();
    let mut cases = Vec::new();
    for c in g["cases"].as_array().unwrap() {
        let tz = crate::common::time::Tz::of(c["tz"].as_str());
        let v = through_the_exit(&c["reply"], &tz);
        cases.push(serde_json::json!({"name": c["name"], "tz": c["tz"], "reply": v}));
    }
    let got = serde_json::json!({
        "说明": "quota-read 的线上形状（后端真出口走完那一份，由 quota_rows_tests::the_wire_golden_is_what_the_exit_writes 写）：每号 rows 是 [[{text, tone}]]、sampled、warm、names。三个语言的读者都照它比，不各自手抄。",
        "cases": cases,
    });
    test_meters::golden::assert_golden(
        "tests/__fixtures__/quota-read.golden.json",
        &format!("{}\n", serde_json::to_string_pretty(&got).unwrap()),
        "quota-read 线上金样与真出口不一致",
    );
}

/// ★ 额度账读不懂（一份坏的夹具：不是 JSON）⇒ 一个号都不发：账号库里的每个号 `warm` 都是 `wait`（过一阵再看），
/// 那一句照实说哪一份读不懂、为什么（就是 `reason`：路径 ＋ 原因词），不把每号照「无采样 · 发一句才知道」判成 `send`。
#[test]
fn an_unreadable_ledger_sends_nothing_and_says_which_file_and_why() {
    let d = std::env::temp_dir().join(format!("ccm-quota-bad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join("quota.json");
    std::fs::write(&p, "{\"accounts\": [ 这不是 JSON").unwrap();
    let now = 1_791_189_600u64;
    let read = crate::accounts::quota::ledger::answer_of(Some(&p), now);
    let _ = std::fs::remove_dir_all(&d);
    assert_eq!(
        read.state,
        crate::accounts::quota::ledger::ReadState::Unreadable
    );
    let why = read.reason.clone().expect("读不懂要有原因");
    assert!(
        why.contains(&p.display().to_string()),
        "原因里说是哪一份：{why}"
    );
    let mut reply = serde_json::to_value(&read).unwrap();
    reply["unseen"] = serde_json::json!([
        {"account": "work", "agent": "claude-code", "kind": "sub", "login": "ok"},
        {"account": "team", "agent": "claude-code", "kind": "sub", "login": "ok"},
    ]);
    reply["usable"] = serde_json::json!([]);
    let v = through_the_exit(&reply, &crate::common::time::Tz::default());
    let unseen = v["unseen"].as_array().unwrap();
    assert_eq!(unseen.len(), 2);
    let said = copy_text("acct.warm.unreadable", &[("why", &why)]);
    for a in unseen {
        assert_eq!(a["warm"]["act"], "wait", "{}", a["account"]);
        assert_eq!(a["warm"]["text"], Value::String(said.clone()));
        assert_eq!(
            a["warm"]["at"],
            serde_json::json!(now as i64 + WARM_UNREADABLE_RETRY_S)
        );
    }
}

/// ★ 「现在还能不能发」那两行：有号发得出去 ⇒「可发 · n 个号」＋ 号名（`_` 写成它的叫法）、plain；一个都发不出去 ⇒「均不可发」＋ 最早哪个几点回来、fail；
/// 最早那个时刻已过 ⇒ 不写距今；读不出 / 一个号都没有 ⇒ 不出（看顶上那一句）。
#[test]
fn the_verdict_says_whether_anything_can_send() {
    use crate::accounts::quota::ledger::ReadState;
    use crate::faces::quota_read::{reply_of, Base};
    let now = 1_800_000_000u64;
    let clock = crate::common::time::TextClock::new(now, &crate::common::time::Tz::default());
    let unseen = |id: &str| crate::faces::quota_read::Unseen {
        head: crate::faces::quota_read::UnseenHead {
            agent: crate::agents::AgentKind::new("claude"),
            account: id.into(),
            kind: crate::accounts::quota::facts::Kind::Sub,
            login: crate::accounts::quota::show::LoginState::Ok,
            usage: unseen_usage(crate::accounts::quota::facts::Kind::Sub),
        },
        warm: crate::accounts::quota::rules::decide::WarmVerdict::Send(
            crate::accounts::quota::rules::decide::WarmWhy::NoWindow,
        ),
    };
    let base = |state, usable: &[&str], earliest: Option<(&str, u64)>| Base {
        state,
        reason: None,
        detail: None,
        path: None,
        now,
        usable: usable
            .iter()
            .map(|s| (String::new(), s.to_string()))
            .collect(),
        earliest: earliest.map(|(a, t)| ((String::new(), a.to_string()), t)),
    };
    let verdict = |b: Base, u: Vec<_>| reply_of(b, Vec::new(), u, &clock).verdict;
    let sep = copy_core::copy_text("kit.text.sep", &[]);
    let v = verdict(
        base(ReadState::Present, &["a", "_"], None),
        vec![unseen("a")],
    )
    .expect("有号");
    assert_eq!(
        (v.text.0, v.sub.map(|w| w.0), v.tone),
        (
            copy_core::copy_text("acct.verdict.can", &[("n", "2")]),
            Some(format!(
                "a{sep}{}",
                copy_core::copy_text("acct.tag.home", &[])
            )),
            crate::common::cells::Tone::Plain
        )
    );
    let back = now + 5_400;
    let v = verdict(
        base(ReadState::Present, &[], Some(("b", back))),
        vec![unseen("a")],
    )
    .expect("有号");
    assert_eq!(
        (v.text.0, v.sub.map(|w| w.0), v.tone),
        (
            copy_core::copy_text("acct.verdict.none", &[]),
            Some(copy_core::copy_text(
                "acct.verdict.earliest",
                &[
                    ("account", "b"),
                    ("at", &clock.text(back).0),
                    ("rel", &clock.rel(back).unwrap().0)
                ]
            )),
            crate::common::cells::Tone::Fail
        )
    );
    let v = verdict(
        base(ReadState::Present, &[], Some(("b", now - 60))),
        vec![unseen("a")],
    )
    .expect("有号");
    assert_eq!(
        v.sub.map(|w| w.0),
        Some(copy_core::copy_text(
            "acct.verdict.earliestAt",
            &[("account", "b"), ("at", &clock.text(now - 60).0)]
        ))
    );
    let v = verdict(base(ReadState::Present, &[], None), vec![unseen("a")]).expect("有号");
    assert_eq!(v.sub, None, "说不出几点回来 ⇒ 缺");
    assert!(
        verdict(base(ReadState::Present, &[], None), Vec::new()).is_none(),
        "一个号都没有"
    );
    assert!(
        verdict(base(ReadState::Unreadable, &["a"], None), vec![unseen("a")]).is_none(),
        "读不出"
    );
    assert!(
        verdict(base(ReadState::Absent, &[], None), Vec::new()).is_none(),
        "没记过账"
    );
}

/// ★ 两个语义位名（`5h` · `7d`）只住 `cmd_wire::quota::{SLOT_5H, SLOT_7D}`：后端生产段不再写字面量。
/// 例外只有一处：Claude 回包头的段名（`anthropic-ratelimit-unified-5h-…`）是上游协议的字，碰巧同形，不是语义位。
#[test]
fn slot_names_live_only_in_cmd_wire() {
    const ALLOWED: &[(&str, usize)] = &[("agents/claudecode/quota.rs", 2)];
    let root = guard_core::addr::backend_src_root();
    let mut hits: Vec<String> = Vec::new();
    for (p, src) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let rel = p
            .strip_prefix(&root)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        let code = guard_core::production_code(&src);
        let n = code.matches("\"5h\"").count() + code.matches("\"7d\"").count();
        let allowed = ALLOWED
            .iter()
            .find(|(f, _)| *f == rel)
            .map_or(0, |(_, n)| *n);
        if n != allowed {
            hits.push(format!("{rel}: {n} 处（许 {allowed}）"));
        }
    }
    assert!(
        hits.is_empty(),
        "语义位名写成了字面量：\n{}",
        hits.join("\n")
    );
}

/// ★ 钟面 `HH:MM` 只由 `common::time::hm` 写：后端生产段 `"{:02}:{:02}"` 只许它那一处。
/// （这根针归 `common/time.rs`，暂住这里：`time_tests.rs` 此刻归另一块。）
#[test]
fn the_clock_face_is_written_once() {
    let root = guard_core::addr::backend_src_root();
    let mut hits: Vec<String> = Vec::new();
    for (p, src) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let rel = p
            .strip_prefix(&root)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        let n = guard_core::production_code(&src)
            .matches("\"{:02}:{:02}\"")
            .count();
        if n > 0 {
            hits.push(format!("{rel}: {n}"));
        }
    }
    assert_eq!(
        hits,
        vec!["common/time.rs: 1".to_string()],
        "钟面另写了一份"
    );
}

/// 显示态写成字（判在 `accounts/quota/show.rs`，判好的由出口 `written` · `Stamp` 写）。
mod written_from_show {
    use super::*;
    use crate::accounts::quota::facts::Kind;
    use crate::accounts::quota::show::{show, windows_of, Facts, LoginState, QuotaState};
    use crate::agents::{QuotaOverage, QuotaReading, QuotaStatus, QuotaWindow};

    const NOW: u64 = 1_800_000_000;
    const LATER: u64 = NOW + 3_600;
    const EARLIER: u64 = NOW - 60;

    fn slot(w: &str) -> Option<&'static str> {
        crate::agents::claudecode::quota::slot_of(w)
    }

    fn win(name: &str, used: f64, resets_at: u64) -> QuotaWindow {
        QuotaWindow {
            name: name.into(),
            used: Some(used),
            resets_at: Some(resets_at),
            warned_at: None,
        }
    }

    /// 一份订阅号的快照：5h 用 `used_5h`、7d 两个窗口（取用得多的那个）。
    fn sub(used_5h: f64, resets_5h: u64) -> QuotaReading {
        QuotaReading {
            status: Some(QuotaStatus::Allowed),
            refused: false,
            limiting: Some("five_hour".into()),
            resets_at: Some(resets_5h),
            windows: vec![
                win("five_hour", used_5h, resets_5h),
                win("seven_day", 0.41, NOW + 5 * 86_400),
                win("seven_day_overage_included", 0.12, NOW + 5 * 86_400),
            ],
            overage: None,
        }
    }

    fn facts(kind: Kind) -> Facts {
        Facts {
            kind,
            login: LoginState::Ok,
            retry_at: None,
        }
    }

    /// ★ 一格的字与语气只住 `slot_words`：卡人的那一格按显示态换字；用满不管是不是卡人的那一格都 `✕`。
    #[test]
    fn a_slot_cell_is_written_once_by_state() {
        use crate::common::cells::Tone;
        let w = |state, here, pct, full| {
            let (t, tone) = slot_words(state, here, pct, full);
            (t.0, tone)
        };
        let c = |k: &str, a: &[(&str, &str)]| copy_core::copy_text(k, a);
        assert_eq!(
            w(QuotaState::OverageInUse, true, Some(40), false),
            (c("acct.row.over", &[]), Tone::Warn)
        );
        assert_eq!(
            w(QuotaState::Unseen, true, None, false),
            (c("acct.val.none", &[]), Tone::Plain)
        );
        assert_eq!(
            w(QuotaState::Ok, false, Some(100), true),
            (c("acct.val.full", &[]), Tone::Fail)
        );
        assert_eq!(
            w(QuotaState::Refused, true, Some(58), false),
            (c("acct.val.refusedPct", &[("pct", "58")]), Tone::Fail)
        );
        assert_eq!(
            w(QuotaState::Refused, true, None, false),
            (c("reason.code.refused", &[]), Tone::Fail)
        );
        assert_eq!(
            w(QuotaState::Refused, false, Some(10), false),
            (c("acct.val.pct", &[("pct", "10")]), Tone::Plain)
        );
        assert_eq!(
            w(QuotaState::Near, true, Some(85), false),
            (c("acct.val.pct", &[("pct", "85")]), Tone::Warn)
        );
        assert_eq!(
            w(QuotaState::Ok, true, None, false),
            (c("acct.val.none", &[]), Tone::Plain)
        );
    }

    /// ★★ 用量那一格（状态栏按钮 · 切号下拉 · 「下一个」）由核心写：窗口 · 值 · 重置 · 连成的一句 · 语气，出口照抄。
    /// 回包没说卡在哪一窗 ⇒ 画 `5h`（这一判也只在这里）。
    #[test]
    fn the_usage_cell_is_written_by_the_core() {
        let t = |k: &str| copy_core::copy_text(k, &[]);
        let clock = crate::common::time::TextClock::new(NOW, &crate::common::time::Tz::default());
        let shown = |r: Option<&QuotaReading>, kind: Kind| {
            let mut q = show(r.map(|r| (r, NOW)), facts(kind), NOW, &slot);
            written(&mut q, r.and_then(|r| r.resets_at), &clock);
            q.stamp(&clock);
            q.usage
        };
        let five = crate::accounts::quota::name_words::slot_text("5h");
        // 订阅 · 正常：`5h 50%`，常规色。
        let u = shown(Some(&sub(0.5, LATER)), Kind::Sub);
        assert_eq!(
            (
                u.slot.as_deref(),
                u.window.as_ref().map(|w| w.0.as_str()),
                u.reset.is_none(),
                u.tone
            ),
            (Some("5h"), Some(five.as_str()), true, Tone::Plain)
        );
        assert_eq!(
            u.text.0,
            copy_core::copy_text("acct.usage.wv", &[("w", &five), ("v", &u.value.0)])
        );
        // 快满 ⇒ 警示色。
        assert_eq!(shown(Some(&sub(0.9, LATER)), Kind::Sub).tone, Tone::Warn);
        // 用满 ⇒ 失败色。
        assert_eq!(shown(Some(&sub(1.0, LATER)), Kind::Sub).tone, Tone::Fail);
        // 被拒 ⇒ 失败色 ＋ 几点重置（照卡着那一窗的时刻），连成 `5h ✕ ↻hh:mm`。
        let mut refused = sub(1.0, LATER);
        refused.refused = true;
        let u = shown(Some(&refused), Kind::Sub);
        let reset = copy_core::copy_text("acct.reset.at", &[("at", &clock.text(LATER).0)]);
        assert_eq!(
            (u.tone, u.reset.as_ref().map(|r| r.0.clone())),
            (Tone::Fail, Some(reset.clone()))
        );
        assert_eq!(
            u.text.0,
            copy_core::copy_text(
                "acct.usage.wvr",
                &[("w", &five), ("v", &u.value.0), ("r", &reset)]
            )
        );
        // 超额在兜 ⇒ 警示色、不写重置。
        let mut over = sub(1.0, LATER);
        over.overage = Some(QuotaOverage {
            status: Some(QuotaStatus::Allowed),
            resets_at: None,
            disabled: None,
            in_use: true,
        });
        let u = shown(Some(&over), Kind::Sub);
        assert_eq!((u.tone, u.reset.is_none()), (Tone::Warn, true), "{u:?}");
        // 回包没说卡在哪一窗 ⇒ 画 5h。
        let mut free = sub(0.3, LATER);
        free.limiting = None;
        free.windows.retain(|w| w.name == "five_hour");
        assert_eq!(shown(Some(&free), Kind::Sub).slot.as_deref(), Some("5h"));
        // 没出过数 ⇒ `5h —`，常规色。
        let u = shown(None, Kind::Sub);
        assert_eq!(
            (u.value.0.as_str(), u.tone),
            (t("acct.val.none").as_str(), Tone::Plain)
        );
        // 按量 · 能发 ⇒ `按量`、没有窗口；按量 · 被拒 ⇒ `被拒 ↻hh:mm`、失败色。
        let u = shown(Some(&sub(0.0, LATER)), Kind::Api);
        assert_eq!(
            (u.window.is_none(), u.text.0.as_str(), u.tone),
            (true, t("acct.kind.api").as_str(), Tone::Plain)
        );
        let mut api = sub(0.0, LATER);
        api.refused = true;
        let u = shown(Some(&api), Kind::Api);
        assert_eq!(
            (u.value.0.as_str(), u.tone),
            (t("reason.code.refused").as_str(), Tone::Fail)
        );
        assert_eq!(
            u.text.0,
            copy_core::copy_text("acct.usage.wv", &[("w", &u.value.0), ("v", &reset)])
        );
    }

    /// 一份账：5h 用 30%、7d 两个窗口（`seven_day` 40% · `seven_day_opus` 85%），5h 与 opus 量过「1% ≈ 多少」。
    fn observed() -> crate::accounts::quota::ledger::Observed {
        use crate::accounts::quota::ledger::{Observed, PerPct};
        let mut r = sub(0.3, LATER);
        r.windows = vec![
            win("five_hour", 0.3, LATER),
            win("seven_day", 0.4, LATER + 86_400),
            win("seven_day_opus", 0.85, LATER + 86_400),
        ];
        Observed {
            agent: "claude".into(),
            account: "a".into(),
            seen_at: NOW,
            reading: r,
            windows_seen: Default::default(),
            trend: Default::default(),
            per_pct: [
                (
                    "five_hour".to_string(),
                    PerPct {
                        tokens: 2_100_000,
                        at: NOW,
                    },
                ),
                (
                    "seven_day_opus".to_string(),
                    PerPct {
                        tokens: 25_000,
                        at: NOW,
                    },
                ),
            ]
            .into(),
            history: Default::default(),
            reread_failed: Default::default(),
        }
    }

    /// ★ 窗口那一格的语气只在出口写（`written`）：显示态只判用满 · 快满（`full` · `near`），出来时语气还是常规；
    /// 写过之后用满 ⇒ fail · 快满 ⇒ warn · 其余常规。
    #[test]
    fn window_tone_is_written_only_by_the_exit() {
        use crate::common::cells::Tone;
        let mut o = observed();
        o.reading.windows.push(win("seven_day_sonnet", 1.0, LATER));
        let key = |w: &str| crate::agents::claudecode::quota::key_of(w);
        let mut sh = show(Some((&o.reading, o.seen_at)), facts(Kind::Sub), NOW, &slot);
        sh.windows = windows_of(&o, &key, NOW);
        let judged: Vec<_> = sh
            .windows
            .iter()
            .map(|w| (w.name.as_str(), w.full, w.near, w.tone))
            .collect();
        assert_eq!(
            judged,
            vec![
                ("five_hour", false, false, Tone::Plain),
                ("seven_day", false, false, Tone::Plain),
                ("seven_day_opus", false, true, Tone::Plain),
                ("seven_day_sonnet", true, true, Tone::Plain),
            ],
            "显示态只判、不写语气"
        );
        written(
            &mut sh,
            None,
            &crate::common::time::TextClock::new(NOW, &crate::common::time::Tz::default()),
        );
        let tones: Vec<_> = sh
            .windows
            .iter()
            .map(|w| (w.name.as_str(), w.tone))
            .collect();
        assert_eq!(
            tones,
            vec![
                ("five_hour", Tone::Plain),
                ("seven_day", Tone::Plain),
                ("seven_day_opus", Tone::Warn),
                ("seven_day_sonnet", Tone::Fail),
            ]
        );
    }

    /// ★ 「1% ≈ 多少」写好的字：每个量过的窗口一格（短写的数）；语义位那一格照它取的那个窗口（7d 取用得多的 opus），没量过 ⇒ 缺。
    #[test]
    fn per_pct_is_written_for_windows_and_the_slot_that_took_them() {
        let o = observed();
        let key = |w: &str| crate::agents::claudecode::quota::key_of(w);
        let mut sh = show(Some((&o.reading, o.seen_at)), facts(Kind::Sub), NOW, &slot);
        sh.windows = windows_of(&o, &key, NOW);
        written(
            &mut sh,
            None,
            &crate::common::time::TextClock::new(NOW, &crate::common::time::Tz::default()),
        );
        let pp = |n: &str| Some(Words(copy_core::copy_text("acct.val.perPct", &[("n", n)])));
        let win_pp: Vec<_> = sh
            .windows
            .iter()
            .map(|w| (w.name.as_str(), w.per_pct_text.clone()))
            .collect();
        assert_eq!(
            win_pp,
            vec![
                ("five_hour", pp("2.1M")),
                ("seven_day", None),
                ("seven_day_opus", pp("25K"))
            ]
        );
        let slot_pp: Vec<_> = sh
            .slots
            .iter()
            .map(|s| (s.slot.as_str(), s.per_pct_text.clone()))
            .collect();
        assert_eq!(slot_pp, vec![("5h", pp("2.1M")), ("7d", pp("25K"))]);
    }

    /// ★ 紧凑那一格的重置字（[`slot_reset`]）与 `rows` 里那一格同一个写法：还没到 ⇒ ↻时刻；已过 ⇒ 不写（不说「已过」）。
    /// 7d 那两个窗口都已重置 ⇒ 那一格没有重置字。
    #[test]
    fn slot_reset_text_is_the_rows_reset_cell() {
        let o = observed();
        let clock = crate::common::time::TextClock::new(NOW, &crate::common::time::Tz::default());
        let mut r = o.reading.clone();
        r.windows[1].resets_at = Some(EARLIER);
        r.windows[2].resets_at = Some(EARLIER);
        let sh = show(Some((&r, o.seen_at)), facts(Kind::Sub), NOW, &slot);
        let got: Vec<_> = sh
            .slots
            .iter()
            .map(|s| {
                (
                    s.slot.as_str(),
                    slot_reset(s.resets_at, &clock).map(|w| w.0),
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    "5h",
                    Some(copy_core::copy_text(
                        "acct.reset.at",
                        &[("at", &reset_at_text(LATER, &clock))]
                    ))
                ),
                ("7d", None),
            ]
        );
    }
}

/// ★ 那一家不归我方续的账号 0 令牌过期了（[`LoginState::BaseUntouched`]）：账号行打后端写好的那一句（警示语气，不说「需登录」）；
/// 开窗那一判等着、说的也是那一句。正常的号不打标签。
#[test]
fn an_account_zero_left_to_its_family_says_so_on_its_row_and_warm() {
    use cmd_wire::quota::{Kind, LoginState};
    let said = copy_text("acct.tag.baseUntouched", &[]);
    let row = head(
        "claude-code",
        "0",
        Kind::Sub,
        LoginState::BaseUntouched,
        None,
    );
    let tag = row.last().expect("有格");
    assert_eq!((tag.text.0.as_str(), tag.tone), (said.as_str(), Tone::Warn));
    assert_ne!(said, copy_text("acct.tag.login", &[]));
    assert_eq!(
        head("claude-code", "0", Kind::Sub, LoginState::Ok, None).len(),
        2,
        "正常的号不打标签"
    );
    let now: u64 = 1_000_000;
    let clock = crate::common::time::TextClock::new(now, &crate::common::time::Tz::default());
    let w = warm_of(
        warm_from(
            "claude-code",
            "0",
            None,
            Kind::Sub,
            LoginState::BaseUntouched,
            now,
        ),
        None,
        &clock,
    );
    assert_eq!((w.act, w.text.0), (WarmAct::Wait, said));
}

/// ★ 每号几行只说两样：收到几个窗口列几行 · 每行用量与几点重置 · 过了重置不出旧数 · 超额关着不出 · 采样不成行 · 状态只在抬头。
mod two_things {
    use super::*;
    use crate::accounts::quota::facts::Kind;
    use crate::accounts::quota::ledger::Observed;
    use crate::accounts::quota::show::{show, windows_of_with, Facts, LoginState};
    use crate::agents::{QuotaOverage, QuotaReading, QuotaStatus, QuotaWindow};
    use crate::common::time::{TextClock, Tz};

    /// 2027-01-15 08:00 UTC：当天 · 明天 · 别天三种时刻都落得开。
    const NOW: u64 = 1_800_000_000 - 1_800_000_000 % 86_400 + 8 * 3_600;

    fn win(name: &str, used: f64, resets_at: u64) -> QuotaWindow {
        QuotaWindow {
            name: name.into(),
            used: Some(used),
            resets_at: Some(resets_at),
            warned_at: None,
        }
    }

    fn reading(windows: Vec<QuotaWindow>) -> QuotaReading {
        QuotaReading {
            status: Some(QuotaStatus::Allowed),
            refused: false,
            limiting: None,
            resets_at: windows.first().and_then(|w| w.resets_at),
            windows,
            overage: None,
        }
    }

    fn observed(r: QuotaReading) -> Observed {
        Observed {
            agent: "claude".into(),
            account: "a".into(),
            seen_at: NOW - 600,
            reading: r,
            windows_seen: Default::default(),
            trend: Default::default(),
            per_pct: Default::default(),
            history: Default::default(),
            reread_failed: Default::default(),
        }
    }

    fn facts(login: LoginState) -> Facts {
        Facts {
            kind: Kind::Sub,
            login,
            retry_at: None,
        }
    }

    /// 照真出口走一遍：显示态 ⇒ 各窗口（`can` ＝ 那一家查得了用量）⇒ 出口那一条（[`crate::faces::quota_read::account_of`]）。
    fn account_with(o: &Observed, login: LoginState, can: bool) -> cmd_wire::quota::QuotaAccount {
        let slot = |w: &str| crate::agents::claudecode::quota::slot_of(w);
        let key = |w: &str| crate::agents::claudecode::quota::key_of(w);
        let mut sh = show(Some((&o.reading, o.seen_at)), facts(login), NOW, &slot);
        sh.windows = windows_of_with(o, &key, NOW, can);
        crate::faces::quota_read::account_of(
            crate::faces::quota_read::Seen {
                agent: "claude".into(),
                account: o.account.clone(),
                seen_at: o.seen_at,
                reading: o.reading.clone(),
                show: sh,
                warm: warm_from(
                    "claude",
                    &o.account,
                    Some(&o.reading),
                    Kind::Sub,
                    login,
                    NOW,
                ),
            },
            &TextClock::new(NOW, &Tz::default()),
        )
    }

    fn rows_with(o: &Observed, login: LoginState, can: bool) -> Rows {
        account_with(o, login, can).rows
    }

    fn rows_of(o: &Observed) -> Rows {
        rows_with(o, LoginState::Ok, true)
    }

    fn texts(rows: &Rows) -> Vec<Vec<String>> {
        rows.iter()
            .map(|r| r.iter().map(|c| c.text.0.clone()).collect())
            .collect()
    }

    fn t(k: &str, a: &[(&str, &str)]) -> String {
        copy_text(k, a)
    }

    fn hm(at: u64) -> String {
        TextClock::new(NOW, &Tz::default()).text(at).0
    }

    /// 只收到 5h 的号：抬头 ＋ 一行（不再补「7d —」）。
    #[test]
    fn only_the_five_hour_window_gives_one_row() {
        let o = observed(reading(vec![win("five_hour", 0.3, NOW + 3_600)]));
        let got = texts(&rows_of(&o));
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(
            got[1],
            vec![
                "5h".to_string(),
                t("acct.val.pct", &[("pct", "30")]),
                t("acct.reset.at", &[("at", &hm(NOW + 3_600))]),
                "+1h".to_string(),
            ]
        );
    }

    /// 收到第三窗（`seven_day_fable`）多一行，名字照字段名写。
    #[test]
    fn a_third_window_adds_a_row_named_by_its_field() {
        let o = observed(reading(vec![
            win("five_hour", 0.3, NOW + 3_600),
            win("seven_day", 0.4, NOW + 3 * 86_400),
            win("seven_day_fable", 0.5, NOW + 3 * 86_400),
        ]));
        let names: Vec<String> = texts(&rows_of(&o))
            .into_iter()
            .skip(1)
            .map(|r| r[0].clone())
            .collect();
        assert_eq!(
            names,
            vec![
                "5h".to_string(),
                "7d".to_string(),
                t("acct.window.tier", &[("slot", "7d"), ("tier", "fable")]),
            ]
        );
    }

    /// 重置时刻已过：查得了 ⇒「读取中」；查不了 ⇒「已重置」。都不出旧数、不出「已过」、不出时刻。
    #[test]
    fn a_lapsed_window_shows_no_old_number() {
        let o = observed(reading(vec![
            win("five_hour", 0.9, NOW - 60),
            win("seven_day", 0.4, NOW + 3 * 86_400),
        ]));
        let reading_now = texts(&rows_of(&o));
        assert_eq!(
            reading_now[1],
            vec!["5h".to_string(), t("kit.state.reading", &[])]
        );
        let cannot = texts(&rows_with(&o, LoginState::Ok, false));
        assert_eq!(
            cannot[1],
            vec!["5h".to_string(), t("acct.window.reset", &[])]
        );
        let all = format!("{reading_now:?}{cannot:?}");
        assert!(!all.contains("90%"), "旧数还在：{all}");
        assert!(!all.contains(&hm(NOW - 60)), "过了的时刻还在：{all}");
    }

    /// 重读没成（账上记着这一窗这一刻没读回来）⇒「已重置」，不再写「读取中」。
    #[test]
    fn a_failed_reread_says_reset() {
        let mut o = observed(reading(vec![win("five_hour", 0.9, NOW - 60)]));
        o.reread_failed.insert("five_hour".into(), NOW - 60);
        assert_eq!(
            texts(&rows_of(&o))[1],
            vec!["5h".to_string(), t("acct.window.reset", &[])]
        );
    }

    /// 重置时刻：当天写时刻 · 明天写「明天 HH:MM」· 别天写日期；后面跟还剩多久。
    #[test]
    fn the_reset_says_today_tomorrow_or_the_date() {
        let tomorrow = NOW + 20 * 3_600;
        let later = NOW + 3 * 86_400;
        let o = observed(reading(vec![
            win("five_hour", 0.3, tomorrow),
            win("seven_day", 0.4, later),
        ]));
        let got = texts(&rows_of(&o));
        let clock_hm = |at: u64| crate::common::time::hm(at as i64);
        assert_eq!(
            got[1][2],
            t(
                "acct.reset.at",
                &[(
                    "at",
                    &t("acct.reset.tomorrow", &[("at", &clock_hm(tomorrow))])
                )]
            )
        );
        assert_eq!(got[2][2], t("acct.reset.at", &[("at", &hm(later))]));
        assert!(hm(later).contains('-'), "别天带日期");
        assert_eq!(got[1].len(), 4, "带还剩多久");
    }

    /// 超额关着（没开 · 没在用）不出行；开着 ⇒ 一行；在用 ⇒ 一行、警示。
    #[test]
    fn overage_shows_only_when_on_or_in_use() {
        let over = |status, disabled: Option<&str>, in_use| {
            let mut r = reading(vec![win("five_hour", 0.3, NOW + 3_600)]);
            r.overage = Some(QuotaOverage {
                status: Some(status),
                resets_at: None,
                disabled: disabled.map(str::to_string),
                in_use,
            });
            rows_of(&observed(r))
        };
        let label = t("acct.row.over", &[]);
        let has = |rows: &Rows| texts(rows).iter().any(|r| r[0] == label);
        assert!(!has(&rows_of(&observed(reading(vec![win(
            "five_hour",
            0.3,
            NOW + 3_600
        )])))));
        assert!(!has(&over(
            QuotaStatus::Rejected,
            Some("org_level_disabled"),
            false
        )));
        let on = over(QuotaStatus::Allowed, None, false);
        assert!(has(&on));
        let using = over(QuotaStatus::Allowed, None, true);
        let row = using
            .iter()
            .find(|r| r[0].text.0 == label)
            .expect("在用有一行");
        assert_eq!(
            (row[1].text.0.as_str(), row[1].tone),
            (t("acct.val.overOn", &[]).as_str(), Tone::Warn)
        );
    }

    /// 采样不成行：几点采的是每号那一条的那一句（出口放进悬停提示）。
    #[test]
    fn sampling_is_not_a_row() {
        let o = observed(reading(vec![win("five_hour", 0.3, NOW + 3_600)]));
        let rows = rows_of(&o);
        assert_eq!(rows.len(), 2, "抬头 ＋ 5h 一行：{:?}", texts(&rows));
        assert!(
            rows.iter()
                .flatten()
                .all(|c| !c.text.0.contains(&hm(NOW - 600))),
            "几点采的不成格：{:?}",
            texts(&rows)
        );
        assert_eq!(
            account_with(&o, LoginState::Ok, true).sampled.0,
            t("beLastSeen.said.sampled", &[("time", &hm(NOW - 600))])
        );
    }

    /// ★ 号级「已重置」（`resetSinceSeen`）与窗口级「读取中 / 已重置」是一个判法：按钮那一格（用量）· 起会话菜单那一格（语义位）
    /// 都照那一窗那一行写；卡着的那一窗说不出几点重置、是回包那一刻过了 ⇒ 没有哪一窗在重读，说「已重置」，不写「—」。
    #[test]
    fn the_account_reset_and_the_window_reset_are_one_judgment() {
        let mut lapsed = reading(vec![win("five_hour", 0.9, NOW - 60)]);
        lapsed.limiting = Some("five_hour".into());
        let mut no_time = lapsed.clone();
        no_time.windows[0].resets_at = None;
        for (r, can, want) in [
            (lapsed.clone(), true, t("kit.state.reading", &[])),
            (lapsed, false, t("acct.window.reset", &[])),
            (no_time, true, t("acct.window.reset", &[])),
        ] {
            let a = account_with(&observed(r), LoginState::Ok, can);
            let show = &a.head.show;
            assert_eq!(show.state, cmd_wire::quota::QuotaState::ResetSinceSeen);
            let five = show.slots.iter().find(|s| s.slot == "5h").expect("5h");
            assert_eq!(
                (five.text.0.as_str(), show.usage.value.0.as_str()),
                (want.as_str(), want.as_str()),
                "语义位那一格与按钮那一格同一句"
            );
            assert_eq!(
                texts(&a.rows)[1],
                vec!["5h".to_string(), want.clone()],
                "那一窗那一行同一句（不写旧数）"
            );
            assert_eq!(show.windows[0].pct, None, "计量条不画旧数");
        }
    }

    /// ★ 设置页那几行：去掉账号行自己有的那几格（名 · 类型 · 需登录），被拒到几点留成一行，其余各行照抄；
    /// 成了行的每一窗记下它是第几行（手机计量条照那一行画）。
    #[test]
    fn the_account_line_keeps_the_state_and_every_window_row() {
        let mut r = reading(vec![
            win("five_hour", 0.6, NOW + 3_600),
            win("seven_day", 0.4, NOW + 3 * 86_400),
            win("seven_day_opus", 0.1, NOW + 3 * 86_400),
        ]);
        r.refused = true;
        r.status = Some(QuotaStatus::Rejected);
        r.limiting = Some("five_hour".into());
        let a = account_with(&observed(r), LoginState::Ok, true);
        let line = texts(&account_line_rows(&a.rows, LoginState::Ok, false));
        assert_eq!(
            line[0],
            vec![t("acct.head.refused", &[("at", &hm(NOW + 3_600))])]
        );
        assert_eq!(line[1..], texts(&a.rows)[1..]);
        let at: Vec<Option<u32>> = a.head.show.windows.iter().map(|w| w.row).collect();
        assert_eq!(at, vec![Some(1), Some(2), Some(3)]);
        let ok = account_with(
            &observed(reading(vec![win("five_hour", 0.3, NOW + 3_600)])),
            LoginState::Ok,
            true,
        );
        assert_eq!(
            texts(&account_line_rows(&ok.rows, LoginState::Ok, false)),
            texts(&ok.rows)[1..].to_vec(),
            "没有状态那几格 ⇒ 不成那一行"
        );
    }

    /// ★ 按钮那一格的重置时刻与行同一写法（明天写「明天 HH:MM」）。
    #[test]
    fn the_button_reset_is_written_like_the_rows() {
        let tomorrow = NOW + 20 * 3_600;
        let mut r = reading(vec![win("five_hour", 1.0, tomorrow)]);
        r.refused = true;
        r.status = Some(QuotaStatus::Rejected);
        r.limiting = Some("five_hour".into());
        let a = account_with(&observed(r), LoginState::Ok, true);
        let at = t(
            "acct.reset.tomorrow",
            &[("at", &crate::common::time::hm(tomorrow as i64))],
        );
        assert_eq!(
            a.head.show.usage.reset.map(|w| w.0),
            Some(t("acct.reset.at", &[("at", &at)]))
        );
        assert!(
            a.warm.text.0.contains(&at),
            "开窗那句的时刻同一写法：{:?}",
            a.warm
        );
    }

    /// ★ 开窗那句：5h 那一窗过了重置 ⇒ 照那一行的说法（会到点重读 ⇒ `5h 读取中`；查不了 ⇒ `5h 已重置`），不另说「窗口已过期」。
    #[test]
    fn warm_says_what_the_row_says_when_five_hours_lapsed() {
        for (can, v) in [(true, "kit.state.reading"), (false, "acct.window.reset")] {
            let a = account_with(
                &observed(reading(vec![win("five_hour", 0.9, NOW - 60)])),
                LoginState::Ok,
                can,
            );
            assert_eq!(a.warm.act, WarmAct::Send);
            assert_eq!(
                a.warm.text.0,
                t("acct.usage.wv", &[("w", "5h"), ("v", &t(v, &[]))])
            );
            assert_eq!(texts(&a.rows)[1][1], t(v, &[]));
        }
    }

    /// ★ 设置页那几行数旧 ⇒ 常规那几格画警示（同紧凑那一格的判法），名字那一格不动。
    #[test]
    fn stale_account_lines_turn_plain_values_into_warn() {
        let a = account_with(
            &observed(reading(vec![win("five_hour", 0.3, NOW + 3_600)])),
            LoginState::Ok,
            true,
        );
        let tones = |stale| -> Vec<Vec<Tone>> {
            account_line_rows(&a.rows, LoginState::Ok, stale)
                .iter()
                .map(|r| r.iter().map(|c| c.tone).collect())
                .collect()
        };
        assert_eq!(tones(false), vec![vec![Tone::Plain; 4]]);
        assert_eq!(
            tones(true),
            vec![vec![Tone::Plain, Tone::Warn, Tone::Warn, Tone::Warn]]
        );
    }

    /// ★ 照时长写的窗口键给人看时经文案表写（整天 · 整点 · 分钟各一条）。
    #[test]
    fn a_window_keyed_by_its_length_is_named_through_the_copy_table() {
        assert_eq!(
            window_label("6h"),
            t("durationFormat.short.hour", &[("n", "6")])
        );
        assert_eq!(
            window_label("30d"),
            t("durationFormat.short.day", &[("n", "30")])
        );
        assert_eq!(
            window_label("90m"),
            t("durationFormat.short.min", &[("n", "90")])
        );
        assert_eq!(window_label("5h"), t("acct.slot.fiveHour", &[]));
    }

    /// 号的状态（需登录 · 被拒到几点）只写在抬头那一行；窗口那几行不另写状态行。
    #[test]
    fn the_account_state_lives_only_in_the_head() {
        let mut r = reading(vec![win("five_hour", 1.0, NOW + 3_600)]);
        r.refused = true;
        r.status = Some(QuotaStatus::Rejected);
        r.limiting = Some("five_hour".into());
        let refused = t("acct.head.refused", &[("at", &hm(NOW + 3_600))]);
        let ok = texts(&rows_with(&observed(r.clone()), LoginState::Ok, true));
        assert_eq!(ok[0][2..], [refused.clone()], "{ok:?}");
        let rows = texts(&rows_with(&observed(r), LoginState::NeedsLogin, true));
        assert_eq!(
            rows[0][2..],
            [t("acct.tag.login", &[])],
            "号的状态只写一个，需登录在被拒之前：{rows:?}"
        );
        assert_eq!(rows.len(), 2, "只有抬头与 5h 一行：{rows:?}");
        let status = t("backend.column.status", &[]);
        assert!(rows.iter().skip(1).all(|r| r[0] != status));
    }

    /// ★ 没出过数的号：状态那一格也只写一个，先后「需登录 > 需 key > 无采样」（需登录自然无采样，两样都写是重复）。
    #[test]
    fn an_unseen_head_says_one_state_login_first() {
        let head = |login| {
            unseen_block(&crate::faces::quota_read::UnseenHead {
                agent: crate::agents::AgentKind::new("claude"),
                account: "team".into(),
                kind: Kind::Sub,
                login,
                usage: unseen_usage(Kind::Sub),
            })[0]
                .iter()
                .skip(2)
                .map(|c| c.text.0.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(head(LoginState::NeedsLogin), [t("acct.tag.login", &[])]);
        assert_eq!(head(LoginState::NeedsKey), [t("acct.tag.key", &[])]);
        assert_eq!(head(LoginState::Ok), [t("acct.seen.none", &[])]);
    }
}

/// ★ 语义位那一格（账号行 · 起会话菜单 · 按钮照它）取的那一窗过了重置时刻 ⇒ 同那一窗那一行写「读取中」/「已重置」，不写数。
#[test]
fn a_slot_on_a_lapsed_window_says_what_its_row_says() {
    use crate::accounts::quota::facts::Kind;
    use crate::accounts::quota::show::{show, windows_of_with, Facts, LoginState};
    const NOW: u64 = 1_800_000_000;
    let o = crate::accounts::quota::ledger::Observed {
        agent: "claude".into(),
        account: "a".into(),
        seen_at: NOW - 600,
        reading: crate::agents::QuotaReading {
            status: Some(crate::agents::QuotaStatus::Allowed),
            refused: false,
            limiting: Some("five_hour".into()),
            resets_at: Some(NOW - 60),
            windows: vec![crate::agents::QuotaWindow {
                name: "five_hour".into(),
                used: Some(0.9),
                resets_at: Some(NOW - 60),
                warned_at: None,
            }],
            overage: None,
        },
        windows_seen: Default::default(),
        trend: Default::default(),
        per_pct: Default::default(),
        history: Default::default(),
        reread_failed: Default::default(),
    };
    let slot = |w: &str| crate::agents::claudecode::quota::slot_of(w);
    let key = |w: &str| crate::agents::claudecode::quota::key_of(w);
    let facts = Facts {
        kind: Kind::Sub,
        login: LoginState::Ok,
        retry_at: None,
    };
    for (can, want) in [(true, "kit.state.reading"), (false, "acct.window.reset")] {
        let mut sh = show(Some((&o.reading, o.seen_at)), facts.clone(), NOW, &slot);
        sh.windows = windows_of_with(&o, &key, NOW, can);
        written(
            &mut sh,
            o.reading.resets_at,
            &crate::common::time::TextClock::new(NOW, &crate::common::time::Tz::default()),
        );
        let five = sh.slots.iter().find(|s| s.slot == "5h").expect("5h 那一格");
        assert_eq!(five.text.0, copy_text(want, &[]));
    }
}

/// ★ 审实现那一轮补的三条：过了重置的那一窗线上不带数 · 被拒到几点照回包那一刻（与「被拒」那一判同一处）· 超额在用过了重置不再说「使用中」。
mod lapsed_and_refused {
    use super::*;
    use crate::accounts::quota::facts::Kind;
    use crate::accounts::quota::ledger::Observed;
    use crate::accounts::quota::show::{overage_of, show, windows_of_with, Facts, LoginState};
    use crate::agents::{QuotaOverage, QuotaReading, QuotaStatus, QuotaWindow};
    use crate::common::time::{TextClock, Tz};

    const NOW: u64 = 1_800_000_000;

    fn win(name: &str, used: f64, resets_at: u64) -> QuotaWindow {
        QuotaWindow {
            name: name.into(),
            used: Some(used),
            resets_at: Some(resets_at),
            warned_at: None,
        }
    }

    fn observed(r: QuotaReading) -> Observed {
        Observed {
            agent: "claude".into(),
            account: "a".into(),
            seen_at: NOW - 600,
            reading: r,
            windows_seen: Default::default(),
            trend: Default::default(),
            per_pct: Default::default(),
            history: Default::default(),
            reread_failed: Default::default(),
        }
    }

    fn shown(o: &Observed) -> crate::accounts::quota::show::QuotaShow {
        let clock = TextClock::new(NOW, &Tz::default());
        let slot = |w: &str| crate::agents::claudecode::quota::slot_of(w);
        let key = |w: &str| crate::agents::claudecode::quota::key_of(w);
        let facts = Facts {
            kind: Kind::Sub,
            login: LoginState::Ok,
            retry_at: None,
        };
        let mut sh = show(Some((&o.reading, o.seen_at)), facts, NOW, &slot);
        sh.windows = windows_of_with(o, &key, NOW, true);
        written(&mut sh, o.reading.resets_at, &clock);
        sh.stamp(&clock);
        sh
    }

    fn reading(windows: Vec<QuotaWindow>, resets_at: Option<u64>) -> QuotaReading {
        QuotaReading {
            status: Some(QuotaStatus::Allowed),
            refused: false,
            limiting: Some("five_hour".into()),
            resets_at,
            windows,
            overage: None,
        }
    }

    #[test]
    fn a_lapsed_window_carries_no_number_on_the_wire() {
        let o = observed(reading(
            vec![win("five_hour", 0.97, NOW - 60)],
            Some(NOW - 60),
        ));
        let sh = shown(&o);
        assert_eq!(sh.windows[0].pct, None, "窗口那一格不带旧数");
        let five = sh.slots.iter().find(|s| s.slot == "5h").expect("5h");
        assert_eq!(
            (five.pct, five.full),
            (None, false),
            "语义位那一格不带数（不编 0）"
        );
    }

    #[test]
    fn the_refused_head_says_the_time_the_reply_gave() {
        let mut r = reading(vec![win("five_hour", 0.58, NOW + 3_600)], Some(NOW + 600));
        r.refused = true;
        r.status = Some(QuotaStatus::Rejected);
        let o = observed(r);
        let clock = TextClock::new(NOW, &Tz::default());
        let slot = |w: &str| crate::agents::claudecode::quota::slot_of(w);
        let key = |w: &str| crate::agents::claudecode::quota::key_of(w);
        let facts = Facts {
            kind: Kind::Sub,
            login: LoginState::Ok,
            retry_at: None,
        };
        let mut sh = show(Some((&o.reading, o.seen_at)), facts, NOW, &slot);
        sh.windows = windows_of_with(&o, &key, NOW, true);
        let rows = crate::faces::quota_read::account_of(
            crate::faces::quota_read::Seen {
                agent: "claude".into(),
                account: "a".into(),
                seen_at: o.seen_at,
                reading: o.reading.clone(),
                show: sh,
                warm: warm_from(
                    "claude",
                    "a",
                    Some(&o.reading),
                    Kind::Sub,
                    LoginState::Ok,
                    NOW,
                ),
            },
            &clock,
        )
        .rows;
        let want = copy_text(
            "acct.head.refused",
            &[("at", &reset_at_text(NOW + 600, &clock))],
        );
        assert!(
            rows[0].iter().any(|c| c.text.0 == want),
            "抬头照回包那一刻（与被拒那一判同一处）：{:?}",
            rows[0]
        );
    }

    #[test]
    fn overage_in_use_past_its_reset_is_not_in_use() {
        let mut r = reading(vec![win("five_hour", 1.0, NOW - 60)], Some(NOW - 60));
        r.overage = Some(QuotaOverage {
            status: Some(QuotaStatus::Allowed),
            resets_at: None,
            disabled: None,
            in_use: true,
        });
        assert_eq!(
            overage_of(&r, Kind::Sub, NOW),
            Some(false),
            "过了重置 ⇒ 只说开着"
        );
        r.resets_at = Some(NOW + 600);
        assert_eq!(overage_of(&r, Kind::Sub, NOW), Some(true));
    }
}
