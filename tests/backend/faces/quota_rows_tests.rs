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
    show: crate::accounts::quota::show::QuotaShow,
}

/// 一个号的开窗那一判照生产同一条路得出：账上那一条 ⇒ 事实（`facts::fact_of`）⇒ `rules::decide::warm_verdict`。
fn warm_from(
    agent: &str,
    account: &str,
    r: Option<&crate::agents::QuotaReading>,
    seen_at: Option<u64>,
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
    let f = fact_of(
        account,
        r,
        seen_at,
        kind,
        login,
        None,
        now,
        &slot,
        &m,
        &|t| t.to_string(),
    );
    crate::accounts::quota::rules::decide::warm_verdict(
        &crate::accounts::quota::rules::Acct::of_fact(&f),
        now,
    )
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
            let mut x: SeenIn = serde_json::from_value(a.clone()).unwrap();
            // 不上线的那几格从金样的输入里补回去（显示态那一处算好的样子）。
            x.show.limiting = x.limiting.take();
            for (i, w) in x.show.windows.iter_mut().enumerate() {
                let raw = &a["windows"][i];
                w.reset_since_seen = raw["resetSinceSeen"].as_bool().unwrap_or(false);
                w.seen_at = raw["seenAt"].as_u64().unwrap_or_default();
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
                facts_in.windows = x
                    .show
                    .slots
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
                    .collect();
            }
            let warm = warm_from(
                &x.agent,
                &x.account,
                Some(&facts_in),
                Some(x.seen_at),
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
            let warm = warm_from(&route, &h.account, None, None, h.kind, h.login, now);
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
        usable: serde_json::from_value(reply["usable"].clone()).unwrap_or_default(),
        earliest: reply["earliest"]["account"].as_str().map(|a| {
            (
                a.to_string(),
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
        warm_of(
            warm_from(ROUTE, "a", r, r.map(|_| now), kind, login, now),
            None,
            &clock,
        )
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
    let api_show = cmd_wire::quota::QuotaShow {
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
            Some(now),
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
        "说明": "quota-read 的线上形状（后端真出口走完那一份，由 quota_rows_tests::the_wire_golden_is_what_the_exit_writes 写）：每号 rows 是 [[{text, tone}]]、warm、names。三个语言的读者都照它比，不各自手抄。",
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
        usable: usable.iter().map(|s| s.to_string()).collect(),
        earliest: earliest.map(|(a, t)| (a.to_string(), t)),
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
            w(QuotaState::ResetSinceSeen, true, Some(40), false),
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
            written(&mut q, r.and_then(|r| r.resets_at));
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
        }
    }

    /// ★ 「1% ≈ 多少」写好的字：每个量过的窗口一格（短写的数）；语义位那一格照它取的那个窗口（7d 取用得多的 opus），没量过 ⇒ 缺。
    #[test]
    fn per_pct_is_written_for_windows_and_the_slot_that_took_them() {
        let o = observed();
        let key = |w: &str| crate::agents::claudecode::quota::key_of(w);
        let mut sh = show(Some((&o.reading, o.seen_at)), facts(Kind::Sub), NOW, &slot);
        sh.windows = windows_of(&o, &key, NOW);
        written(&mut sh, None);
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

    /// ★ 语义位那一格的重置字（`resetText`）与 `rows` 里那一格同一个写法：还没到 ⇒ ↻时刻；已过 ⇒ ↻时刻 已过。
    /// 7d 那两个窗口都已重置 ⇒ 那一格照旧取其中一个、写「已过」。
    #[test]
    fn slot_reset_text_is_the_rows_reset_cell() {
        let o = observed();
        let clock = crate::common::time::TextClock::new(NOW, &crate::common::time::Tz::default());
        let mut r = o.reading.clone();
        r.windows[1].resets_at = Some(EARLIER);
        r.windows[2].resets_at = Some(EARLIER);
        let mut sh = show(Some((&r, o.seen_at)), facts(Kind::Sub), NOW, &slot);
        sh.stamp(&clock);
        let got: Vec<_> = sh
            .slots
            .iter()
            .map(|s| (s.slot.as_str(), s.reset_text.clone().map(|w| w.0)))
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    "5h",
                    Some(copy_core::copy_text(
                        "acct.reset.at",
                        &[("at", &clock.text(LATER).0)]
                    ))
                ),
                (
                    "7d",
                    Some(copy_core::copy_text(
                        "acct.reset.past",
                        &[("at", &clock.text(EARLIER).0)]
                    ))
                ),
            ]
        );
    }
}
