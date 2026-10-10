//! 额度每号几行 · 开窗那一判：对金样 `tests/__fixtures__/quota-text.golden.json`（每号每行每格 ＋ `--text` 整段字 ＋ 每号的 `warm`）。
//! 回包照真出口走一遍：语义位那一格的字由显示态那一处写（`show.rs::slot_words`）· 时刻的字由出口那一遍写（`with_texts`）· 再添行与开窗。
use super::*;

const REGEN: &str = "CCM_REGEN_QUOTA_ROWS";

fn golden_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/__fixtures__/quota-text.golden.json")
}

/// 一份回包照真出口那几步走一遍（不写 `rows` 的那几格从回包里删掉再重写，金样里只留原数与显示态）。
fn through_the_exit(reply: &Value, tz: i64) -> Value {
    use crate::accounts::quota::show::{slot_words, QuotaState};
    let mut v = reply.clone();
    let now = v["now"].as_i64().unwrap_or_default();
    for a in v["accounts"].as_array_mut().into_iter().flatten() {
        let state: QuotaState = serde_json::from_value(a["state"].clone()).unwrap();
        let limiting = a["limiting"].as_str().map(str::to_string);
        for x in a["slots"].as_array_mut().into_iter().flatten() {
            let here = limiting.as_deref() == x["slot"].as_str();
            let pct = x["pct"].as_u64().map(|p| p as u32);
            let full = x["full"].as_bool().unwrap_or(false);
            let (t, tone) = slot_words(state, here, pct, full);
            x["text"] = serde_json::to_value(t).unwrap();
            x["tone"] = serde_json::to_value(tone).unwrap();
        }
    }
    crate::common::time::with_texts(&mut v, now, tz);
    with_rows(&mut v);
    let empty = ["accounts", "unseen"]
        .iter()
        .all(|l| v[*l].as_array().is_none_or(Vec::is_empty));
    v["text"] = serde_json::to_value(head_text(
        v["state"].as_str().unwrap_or(""),
        empty,
        v["reason"].as_str(),
    ))
    .unwrap();
    v["fiveHour"] =
        serde_json::to_value(head_five_hour(v["state"].as_str().unwrap_or(""))).unwrap();
    v
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

/// ★★ 每个号的几行、`--text` 的整段字、开窗那一判，逐字对金样（人读过「现打」再落盘：`CCM_REGEN_QUOTA_ROWS=1`）。
#[test]
fn rows_text_and_warm_match_the_golden() {
    let raw = std::fs::read_to_string(golden_path()).unwrap();
    let mut g: Value = serde_json::from_str(&raw).unwrap();
    let cases = g["cases"].as_array_mut().unwrap();
    assert!(cases.len() >= 10, "金样读空了");
    let mut wrong = Vec::new();
    for c in cases.iter_mut() {
        let tz = c["tzOffsetMin"].as_i64().unwrap_or(0);
        let mut v = through_the_exit(&c["reply"], tz);
        let now = v["now"].as_i64().unwrap_or_default();
        // 开窗：照真出口（with_warm 按这台的钟写 `atText`；金样按它自己的偏移比，这里按偏移重写一遍）。
        for (list, seen) in [("accounts", true), ("unseen", false)] {
            for x in v[list].as_array_mut().into_iter().flatten() {
                let mut w = serde_json::to_value(warm_of(x, seen, now)).unwrap();
                crate::common::time::with_texts(&mut w, now, tz);
                x["warm"] = w;
            }
        }
        let blocks: Vec<Value> = ["accounts", "unseen"]
            .iter()
            .flat_map(|l| v[*l].as_array().cloned().unwrap_or_default())
            .map(|x| serde_json::json!({"account": x["account"], "rows": texts(&x["rows"]), "warm": x["warm"]}))
            .collect();
        let got = (
            Value::Array(blocks),
            Value::String(crate::control::ship_text::ship_text(&v)),
        );
        if (c["blocks"].clone(), c["text"].clone()) != got {
            wrong.push(format!("{}\n{}\n{}", c["name"], got.0, got.1));
        }
        c["blocks"] = got.0;
        c["text"] = got.1;
    }
    if std::env::var_os(REGEN).is_some() {
        std::fs::write(
            golden_path(),
            format!("{}\n", serde_json::to_string_pretty(&g).unwrap()),
        )
        .unwrap();
    }
    assert!(
        wrong.is_empty(),
        "与金样不一致（{REGEN}=1 重写）。现打：\n{}",
        wrong.join("\n\n")
    );
}

/// 开窗那一判的几条边：没见过数 ⇒ 发；登录拿不到 ⇒ 过一小时再看；在计时 ⇒ 睡到重置之后一分钟；7d 用满 ⇒ 睡到 7d 重置。
#[test]
fn warm_sends_only_when_the_window_is_not_ticking() {
    let now = 1_000_000;
    let w = |a: Value, seen: bool| warm_of(&a, seen, now);
    assert_eq!(
        w(serde_json::json!({"login": "ok"}), false).act,
        WarmAct::Send
    );
    assert_eq!(
        w(serde_json::json!({"login": "needsLogin"}), false).at,
        Some(now + WARM_LOGIN_RETRY_S)
    );
    let ticking = serde_json::json!({"login": "ok", "state": "ok", "slots": [{"slot": "5h", "pct": 40, "resetsAt": now + 600, "resetsAtText": "x"}]});
    assert_eq!(w(ticking, true).at, Some(now + 600 + WARM_MARGIN_S));
    let week = serde_json::json!({"login": "ok", "state": "ok", "slots": [{"slot": "5h", "pct": 1, "resetsAt": now + 60}, {"slot": "7d", "pct": 100, "resetsAt": now + 9000}]});
    assert_eq!(w(week, true).at, Some(now + 9000 + WARM_MARGIN_S));
    let expired = serde_json::json!({"login": "ok", "state": "ok", "slots": [{"slot": "5h", "pct": 90, "resetsAt": now - 5}]});
    assert_eq!(w(expired, true).act, WarmAct::Send);
}

/// 读不出那一句照读答的原因说（读的哪份 · 原因词）；没给原因才退回「原因不明」那一句。
#[test]
fn the_unreadable_head_says_the_reason() {
    let why = "读取失败 · /h/.cc-monitor/quota.json · 无权限";
    assert_eq!(
        head_text("unreadable", true, Some(why)).map(|w| w.0),
        Some(why.to_string())
    );
    assert_eq!(
        head_text("unreadable", true, None).map(|w| w.0),
        Some(copy_text("acct.text.readFail", &[]))
    );
    assert!(head_text("present", false, Some(why)).is_none());
}

/// ★★ 「5h 那一格」（恢复菜单 · 新会话框每个号后面那一格）由核心写：订阅号 `5h 41%` · 卡着的照语义位的字；
/// 按量号 · 没出过数的号 ⇒ 没有这一格；账读不出 ⇒ 顶上一格 `5h 读不到`（每号那一格这时没有号可挂）。
#[test]
fn the_five_hour_cell_is_written_by_the_core() {
    let raw = std::fs::read_to_string(golden_path()).unwrap();
    let g: Value = serde_json::from_str(&raw).unwrap();
    let cell = |value: &str| {
        Value::String(copy_text(
            "resumeMenu.account.quota",
            &[
                ("slot", &copy_text("acct.slot.fiveHour", &[])),
                ("value", value),
            ],
        ))
    };
    let mut seen_unreadable = false;
    let mut seen_sub = false;
    for c in g["cases"].as_array().unwrap() {
        let v = through_the_exit(&c["reply"], 0);
        let name = c["name"].as_str().unwrap_or("");
        if v["state"] == "unreadable" {
            seen_unreadable = true;
            assert_eq!(
                v["fiveHour"],
                cell(&copy_text("acct.val.unreadable", &[])),
                "{name}"
            );
        } else {
            assert_eq!(
                v["fiveHour"],
                Value::Null,
                "{name}：读得出 ⇒ 顶上没有这一格"
            );
        }
        for a in v["accounts"].as_array().into_iter().flatten() {
            let want = if a["kind"] == "api" {
                Value::Null
            } else {
                seen_sub = true;
                let text = a["slots"]
                    .as_array()
                    .and_then(|xs| xs.iter().find(|x| x["slot"] == "5h"))
                    .and_then(|x| x["text"].as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| copy_text("acct.val.none", &[]));
                cell(&text)
            };
            assert_eq!(a["fiveHour"], want, "{name} · {}", a["account"]);
        }
        for a in v["unseen"].as_array().into_iter().flatten() {
            assert_eq!(a["fiveHour"], Value::Null, "{name}：没出过数 ⇒ 不出这一格");
        }
    }
    assert!(
        seen_unreadable && seen_sub,
        "金样里要有读不出的一份与订阅号"
    );
}

/// ★ `quota-read` 线上形状的金样（Rust · TS · Kotlin 共用这一份）：`quota-text` 金样里每个案例的回包照真出口走完
/// （每格的字与语气 · 时刻字 · 每号 `rows: [[{text, tone}]]` · `fiveHour` · `warm` · 号名 / 位名 `names`）原样落盘。
/// 重写：`CCM_REGEN_QUOTA_READ=1 cargo test --lib -- quota_rows`。
#[test]
fn the_wire_golden_is_what_the_exit_writes() {
    let raw = std::fs::read_to_string(golden_path()).unwrap();
    let g: Value = serde_json::from_str(&raw).unwrap();
    let mut cases = Vec::new();
    for c in g["cases"].as_array().unwrap() {
        let tz = c["tzOffsetMin"].as_i64().unwrap_or(0);
        let mut v = through_the_exit(&c["reply"], tz);
        let now = v["now"].as_i64().unwrap_or_default();
        for (list, seen) in [("accounts", true), ("unseen", false)] {
            for x in v[list].as_array_mut().into_iter().flatten() {
                let mut w = serde_json::to_value(warm_of(x, seen, now)).unwrap();
                crate::common::time::with_texts(&mut w, now, tz);
                x["warm"] = w;
            }
        }
        crate::accounts::quota::name_words::with_names(&mut v);
        cases.push(serde_json::json!({"name": c["name"], "tzOffsetMin": tz, "reply": v}));
    }
    let got = serde_json::json!({
        "说明": "quota-read 的线上形状（后端真出口走完那一份，由 quota_rows_tests::the_wire_golden_is_what_the_exit_writes 写）：每号 rows 是 [[{text, tone}]]、fiveHour、warm、names。三个语言的读者都照它比，不各自手抄。",
        "cases": cases,
    });
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/__fixtures__/quota-read.golden.json");
    let text = format!("{}\n", serde_json::to_string_pretty(&got).unwrap());
    if std::env::var_os("CCM_REGEN_QUOTA_READ").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        text == want,
        "quota-read 线上金样与真出口不一致（CCM_REGEN_QUOTA_READ=1 重写）"
    );
}
