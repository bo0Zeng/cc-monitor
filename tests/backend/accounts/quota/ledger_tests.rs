//! 额度账：回包读数 → 按号记账 → 落盘 → 读帧，逐字段对得上；只在显示变了时推。

use super::*;
use crate::agents::{QuotaReading, QuotaStatus, QuotaWindow};
use std::path::PathBuf;

fn reading(used_5h: f64, refused: bool) -> QuotaReading {
    QuotaReading {
        status: Some(if refused {
            QuotaStatus::Rejected
        } else {
            QuotaStatus::Allowed
        }),
        refused,
        limiting: Some("five_hour".into()),
        resets_at: Some(1_800_003_600),
        windows: vec![QuotaWindow {
            name: "five_hour".into(),
            used: Some(used_5h),
            resets_at: Some(1_800_003_600),
            warned_at: None,
        }],
        overage: None,
    }
}

/// 这一条额度账自己的节点通道（不经进程里那一条：别的判据也往那一条发）。
struct Rung(tokio::sync::broadcast::Receiver<crate::accounts::quota::nodes::Node>);

impl Rung {
    /// 自上次看以来来过「显示变了」的节点（看过就清）。
    fn has_changed(&mut self) -> Result<bool, ()> {
        let mut any = false;
        while let Ok(n) = self.0.try_recv() {
            assert_eq!(
                n.source,
                crate::accounts::quota::nodes::Source::Reading,
                "{n:?}"
            );
            any = true;
        }
        Ok(any)
    }

    fn mark_unchanged(&mut self) {
        let _ = self.has_changed();
    }
}

fn ringing(path: Option<PathBuf>) -> (Ledger, Rung) {
    let tx = tokio::sync::broadcast::channel(16).0;
    let rx = tx.subscribe();
    let mut l = Ledger::at(path);
    l.nodes = tx;
    (l, Rung(rx))
}

fn on_disk(path: &std::path::Path) -> Book {
    match read_at(path) {
        Read::Present(b) => b,
        other => panic!("盘上应当读得懂，实得 {other:?}"),
    }
}

#[test]
fn a_reading_lands_under_its_account_on_disk_and_on_the_wire_field_for_field() {
    let d = test_home::scratch("land");
    let path = d.join(FILE_NAME);
    // 写句柄的数据目录那一根就是这个临时目录（落点同 `path`）。
    let own = crate::platform::fs::Own::issue_data_at(&d);
    let (l, mut rx) = ringing(Some(path.clone()));
    record_seen(
        &l,
        &own,
        None,
        "claude-code",
        "q",
        reading(0.42, false),
        1_800_000_000,
    );
    let want = Observed {
        agent: "claude-code".into(),
        account: "q".into(),
        seen_at: 1_800_000_000,
        reading: reading(0.42, false),
        windows_seen: Default::default(),
        trend: [(
            "five_hour".to_string(),
            Trend {
                since: 1_800_000_000,
                steps: Vec::new(),
            },
        )]
        .into(),
        per_pct: Default::default(),
        history: Default::default(),
    };
    assert_eq!(on_disk(&path).accounts, vec![want.clone()]);
    assert_eq!(l.entry("claude-code", "q"), Some(want.clone()));
    assert!(
        rx.has_changed().expect("通道在"),
        "第一次看到这个号 ⇒ 推一下"
    );
    let w = answered(Some(&path), 1_800_000_100);
    assert_eq!(w["state"], "present");
    assert_eq!(w["now"], 1_800_000_100u64);
    assert_eq!(
        w["accounts"],
        serde_json::json!([{
            "agent": "claude-code",
            "account": "q",
            "seenAt": 1_800_000_000u64,
            "reading": {
                "status": "allowed",
                "refused": false,
                "limiting": "five_hour",
                "resetsAt": 1_800_003_600u64,
                "windows": [{"name": "five_hour", "used": 0.42, "resetsAt": 1_800_003_600u64}],
            },
        }])
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn only_a_change_you_could_see_pushes_and_quiet_repeats_reach_disk_at_most_once_a_minute() {
    let d = test_home::scratch("quiet");
    let path = d.join(FILE_NAME);
    // 写句柄的数据目录那一根就是这个临时目录（落点同 `path`）。
    let own = crate::platform::fs::Own::issue_data_at(&d);
    let (l, mut rx) = ringing(Some(path.clone()));
    record_seen(
        &l,
        &own,
        None,
        "claude-code",
        "q",
        reading(0.420, false),
        1_000,
    );
    rx.mark_unchanged();
    // 取整后的百分比没变（42%）⇒ 不推、盘上的时刻不动。
    record_seen(
        &l,
        &own,
        None,
        "claude-code",
        "q",
        reading(0.4204, false),
        1_010,
    );
    assert!(!rx.has_changed().expect("通道在"));
    assert_eq!(on_disk(&path).accounts[0].seen_at, 1_000);
    assert_eq!(l.entry("claude-code", "q").map(|e| e.seen_at), Some(1_010));
    // 够久了 ⇒ 落一次盘，仍不推。
    record_seen(
        &l,
        &own,
        None,
        "claude-code",
        "q",
        reading(0.4204, false),
        1_000 + PERSIST_EVERY,
    );
    assert!(!rx.has_changed().expect("通道在"));
    assert_eq!(on_disk(&path).accounts[0].seen_at, 1_000 + PERSIST_EVERY);
    // 43% ⇒ 推、立刻落。
    record_seen(
        &l,
        &own,
        None,
        "claude-code",
        "q",
        reading(0.43, false),
        1_000 + PERSIST_EVERY + 1,
    );
    assert!(rx.has_changed().expect("通道在"));
    assert_eq!(
        on_disk(&path).accounts[0].seen_at,
        1_000 + PERSIST_EVERY + 1
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn two_accounts_keep_two_rows_and_a_restart_remembers_both() {
    let d = test_home::scratch("two");
    let path = d.join(FILE_NAME);
    // 写句柄的数据目录那一根就是这个临时目录（落点同 `path`）。
    let own = crate::platform::fs::Own::issue_data_at(&d);
    let (l, _rx) = ringing(Some(path.clone()));
    record_seen(&l, &own, None, "claude-code", "q", reading(0.1, false), 10);
    record_seen(&l, &own, None, "claude-code", "a", reading(1.0, true), 11);
    let book = on_disk(&path);
    let names: Vec<&str> = book.accounts.iter().map(|e| e.account.as_str()).collect();
    assert_eq!(names, ["a", "q"]);
    let again = Ledger::at(Some(path.clone()));
    assert_eq!(
        again.entry("claude-code", "a").map(|e| e.reading.refused),
        Some(true)
    );
    assert_eq!(again.entry("claude-code", "q").map(|e| e.seen_at), Some(10));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_file_it_cannot_read_is_left_alone_and_said_so() {
    let d = test_home::scratch("bad");
    let path = d.join(FILE_NAME);
    // 写句柄的数据目录那一根就是这个临时目录（落点同 `path`）。
    let own = crate::platform::fs::Own::issue_data_at(&d);
    std::fs::write(&path, "not json").expect("write");
    let (l, _rx) = ringing(Some(path.clone()));
    record_seen(&l, &own, None, "claude-code", "q", reading(0.1, false), 10);
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "not json");
    assert_eq!(answered(Some(&path), 0)["state"], "unreadable");
    assert_eq!(answered(Some(&d.join("absent.json")), 0)["state"], "absent");
    assert_eq!(answered(None, 0)["state"], "unreadable");
    let _ = std::fs::remove_dir_all(&d);
}

/// 版本照声明表：落一笔之后盘上顶层是 `version: 1`；盘上那份比这一版新 ⇒ 读不出来、这一笔不落、一个字节不动。
#[test]
fn the_book_carries_its_version_and_a_newer_one_is_left_alone() {
    let d = test_home::scratch("version");
    let path = d.join(FILE_NAME);
    let own = crate::platform::fs::Own::issue_data_at(&d);
    let (l, _rx) = ringing(Some(path.clone()));
    record_seen(&l, &own, None, "claude-code", "q", reading(0.1, false), 10);
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(v["version"], serde_json::json!(1), "没带声明的版本：{v}");
    let newer = r#"{"version":2,"accounts":[]}"#;
    std::fs::write(&path, newer).unwrap();
    assert_eq!(answered(Some(&path), 0)["state"], "unreadable");
    let (l, _rx) = ringing(Some(path.clone()));
    record_seen(&l, &own, None, "claude-code", "q", reading(0.2, false), 20);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        newer,
        "新版那份被动了"
    );
    let _ = std::fs::remove_dir_all(&d);
}

// ── 第二个来源（`quota-probe`）：按窗口合并 · 两个写者谁也冲不掉谁 ─────────────────────

fn win(name: &str, used: f64, resets_at: Option<u64>) -> QuotaWindow {
    QuotaWindow {
        name: name.into(),
        used: Some(used),
        resets_at,
        warned_at: None,
    }
}

fn names(o: &Observed) -> Vec<&str> {
    o.reading.windows.iter().map(|w| w.name.as_str()).collect()
}

/// ★ 报用量记进同一条账：只并窗口（按模型那一档多出来、同名的盖掉），回包头那几格（状态 · 被拒 · 卡在哪）不动；
/// 每个窗口记自己几点、从哪看到的；下一份回包头只盖它带着的窗口，报用量那一档留着。
#[test]
fn a_probe_merges_windows_into_the_same_entry_and_headers_do_not_wash_them_out() {
    let d = test_home::scratch("probe-merge");
    let path = d.join(FILE_NAME);
    // 写句柄的数据目录那一根就是这个临时目录（落点同 `path`）。
    let own = crate::platform::fs::Own::issue_data_at(&d);
    let (l, mut rx) = ringing(Some(path.clone()));
    record_seen(&l, &own, None, "claude-code", "q", reading(0.42, true), 100);
    rx.mark_unchanged();
    let got = record_probe(
        &l,
        &own,
        "claude-code",
        "q",
        vec![
            win("five_hour", 0.45, Some(1_800_003_600)),
            win("seven_day_fable", 0.0, None),
        ],
        200,
    )
    .expect("落盘");
    assert_eq!(names(&got), ["five_hour", "seven_day_fable"]);
    assert!(got.reading.refused, "被拒那一格只随回包头变");
    assert_eq!(got.reading.windows[0].used, Some(0.45));
    assert_eq!(
        got.window_seen("seven_day_fable"),
        WindowSeen {
            at: 200,
            from: Source::Usage
        }
    );
    assert_eq!(got.seen_at, 200);
    assert!(rx.has_changed().expect("通道在"), "显示变了 ⇒ 推");
    assert_eq!(on_disk(&path).accounts, vec![got.clone()]);
    // 下一份回包头只带 five_hour：盖掉它、留着 seven_day_fable（与它几点、从哪看到的）。
    record_seen(&l, &own, None, "claude-code", "q", reading(0.5, false), 300);
    let after = on_disk(&path).accounts.remove(0);
    assert_eq!(names(&after), ["five_hour", "seven_day_fable"]);
    assert!(!after.reading.refused);
    assert_eq!(after.window_seen("five_hour").from, Source::Headers);
    assert_eq!(
        after.window_seen("seven_day_fable"),
        WindowSeen {
            at: 200,
            from: Source::Usage
        }
    );
    // 线上：只有来源 / 时刻与这一条不同的窗口才列出来。
    let wire = answered(Some(&path), 300);
    assert_eq!(
        wire["accounts"][0]["windowsSeen"],
        serde_json::json!({"seven_day_fable": {"at": 200, "from": "usage"}})
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 两个写者交错（常驻里的中转那一份内存账 · 一次性 CLI 的报用量）：谁也冲不掉谁的窗口；
/// 常驻那份在用的那一刻比盘上的戳，别人写过就并进来（不定时）。
#[test]
fn two_writers_interleaving_never_lose_a_window() {
    let d = test_home::scratch("two-writers");
    let path = d.join(FILE_NAME);
    // 写句柄的数据目录那一根就是这个临时目录（落点同 `path`）。
    let own = crate::platform::fs::Own::issue_data_at(&d);
    let resident = Ledger::at(Some(path.clone()));
    record_seen(
        &resident,
        &own,
        None,
        "claude-code",
        "q",
        reading(0.10, false),
        100,
    );
    // 另一个进程（CLI 一次性的那一形）：自己开一份账、只写报用量。
    let cli = Ledger::at(Some(path.clone()));
    record_probe(
        &cli,
        &own,
        "claude-code",
        "q",
        vec![win("seven_day_opus", 0.30, Some(1_800_500_000))],
        110,
    )
    .expect("落盘");
    // 常驻那份：下一次用的时候就看得见（比戳 ⇒ 并进来）。
    let seen = resident.entry("claude-code", "q").expect("有");
    assert_eq!(names(&seen), ["five_hour", "seven_day_opus"]);
    // 中转再记一份（显示变了 ⇒ 落盘）：报用量那一档留在盘上。
    record_seen(
        &resident,
        &own,
        None,
        "claude-code",
        "q",
        reading(0.20, false),
        120,
    );
    // CLI 那份还拿着旧的内存账：它再报一次，也冲不掉中转刚写的 five_hour。
    record_probe(
        &cli,
        &own,
        "claude-code",
        "q",
        vec![win("seven_day_opus", 0.31, Some(1_800_500_000))],
        130,
    )
    .expect("落盘");
    let disk = on_disk(&path).accounts.remove(0);
    assert_eq!(names(&disk), ["five_hour", "seven_day_opus"]);
    assert_eq!(disk.reading.windows[0].used, Some(0.20), "中转那一份留着");
    assert_eq!(disk.reading.windows[1].used, Some(0.31), "报用量那一份留着");
    // 中转的内存账：没变的观测不落盘，但下一次用的时候并进了 CLI 后来写的那份。
    record_seen(
        &resident,
        &own,
        None,
        "claude-code",
        "q",
        reading(0.2001, false),
        140,
    );
    let mem = resident.entry("claude-code", "q").expect("有");
    assert_eq!(mem.reading.windows[1].used, Some(0.31));
    assert_eq!(mem.reading.windows[0].used, Some(0.2001));
    let _ = std::fs::remove_dir_all(&d);
}

/// 只经报用量见过的号：回包头那几格是空的（没被拒 · 没说状态），窗口照报的。家推不出 ⇒ 报用量那一路照实失败（不只记在内存）。
#[test]
fn a_probe_on_a_fresh_account_and_without_a_home() {
    let d = test_home::scratch("probe-fresh");
    let path = d.join(FILE_NAME);
    // 写句柄的数据目录那一根就是这个临时目录（落点同 `path`）。
    let own = crate::platform::fs::Own::issue_data_at(&d);
    let l = Ledger::at(Some(path.clone()));
    let got = record_probe(
        &l,
        &own,
        "claude-code",
        "b",
        vec![win("five_hour", 0.0, None)],
        50,
    )
    .expect("落盘");
    assert!(!got.reading.refused);
    assert_eq!(got.reading.status, None);
    assert_eq!(names(&got), ["five_hour"]);
    assert!(record_probe(
        &Ledger::at(None),
        &crate::platform::fs::Own::for_tests(),
        "claude-code",
        "b",
        vec![],
        50
    )
    .is_err());
    let _ = std::fs::remove_dir_all(&d);
}

/// 读不懂的那份是成功应答里的一格：`reason` 只是那一句（路径 ＋ 原因词，不带解析器原话），原话进 `detail`（复制详情，命令 `quota-read` · 码 `unreadable`）。
#[test]
fn an_unreadable_book_answers_a_sentence_and_a_detail() {
    let dir = test_home::scratch("ledger-unreadable");
    let p = dir.join("quota.json");
    std::fs::write(&p, b"{not json").unwrap();
    let v = answered(Some(&p), 1);
    assert_eq!(v["state"], "unreadable");
    let reason = v["reason"].as_str().expect("reason");
    assert_eq!(
        reason,
        copy_text(
            "beOwnState.read.notJson",
            &[("path", &p.display().to_string())]
        )
    );
    let detail = v["detail"].as_str().expect("没有 detail");
    assert!(
        detail.contains("quota-read") && detail.contains("unreadable"),
        "{detail}"
    );
    assert!(
        detail.contains("key must be a string"),
        "解析器原话没进详情：{detail}"
    );
    let fine = answered(Some(&dir.join("absent.json")), 1);
    assert!(fine["detail"].is_null(), "不在那一形多出了详情：{fine}");
    std::fs::remove_dir_all(&dir).ok();
}

/// 5h 一个窗口的读数（重置时刻另给）。
fn reading_until(used_5h: f64, resets_at: u64) -> QuotaReading {
    QuotaReading {
        status: Some(QuotaStatus::Allowed),
        refused: false,
        limiting: Some("five_hour".into()),
        resets_at: Some(resets_at),
        windows: vec![QuotaWindow {
            name: "five_hour".into(),
            used: Some(used_5h),
            resets_at: Some(resets_at),
            warned_at: None,
        }],
        overage: None,
    }
}

/// ★ 每个窗口记此前变过的值：值没变只是又看到一次 ⇒ 不动；值变了 ⇒ 旧值与它头一次看到的时刻记进变过的那几个；
/// 窗口重置了（重置时刻换了）⇒ 清掉。落盘的那份同样带着；quota-read 不往外给这一格。
#[test]
fn a_window_keeps_its_last_different_sample_until_it_resets() {
    let d = test_home::scratch("trend");
    let path = d.join(FILE_NAME);
    // 写句柄的数据目录那一根就是这个临时目录（落点同 `path`）。
    let own = crate::platform::fs::Own::issue_data_at(&d);
    let l = Ledger::at(Some(path.clone()));
    let t = 1_800_000_000;
    let reset = t + 18_000;
    let trend = |l: &Ledger| l.entry("claude-code", "q").expect("entry").trend["five_hour"].clone();
    record_seen(
        &l,
        &own,
        None,
        "claude-code",
        "q",
        reading_until(0.40, reset),
        t,
    );
    assert_eq!(
        trend(&l),
        Trend {
            since: t,
            steps: Vec::new()
        }
    );
    record_seen(
        &l,
        &own,
        None,
        "claude-code",
        "q",
        reading_until(0.40, reset),
        t + 60,
    );
    assert_eq!(
        trend(&l),
        Trend {
            since: t,
            steps: Vec::new()
        },
        "又看到一次同一个值不算新采样"
    );
    record_seen(
        &l,
        &own,
        None,
        "claude-code",
        "q",
        reading_until(0.46, reset),
        t + 600,
    );
    let want = Trend {
        since: t + 600,
        steps: vec![Sample {
            used: 0.40,
            since: t,
        }],
    };
    assert_eq!(trend(&l), want);
    assert_eq!(
        on_disk(&path).accounts[0].trend["five_hour"],
        want,
        "盘上那份没带上"
    );
    record_seen(
        &l,
        &own,
        None,
        "claude-code",
        "q",
        reading_until(0.02, reset + 18_000),
        t + 700,
    );
    assert_eq!(
        trend(&l),
        Trend {
            since: t + 700,
            steps: Vec::new()
        },
        "窗口重置了还留着上一窗的采样"
    );
    std::fs::remove_dir_all(&d).ok();
}

/// ★ 每号「1% ≈ 多少用量」：中转那一路的读数 ＋ 流里数到的用量 ⇒ 量出来那一刻记进账、落盘、显示态带上；
/// 量到之前账上没有这一格（不猜）；量出来超过 7 天就不作数。
#[test]
fn the_size_of_one_percent_is_measured_kept_and_aged() {
    let d = test_home::scratch("perpct");
    let path = d.join(FILE_NAME);
    let own = crate::platform::fs::Own::issue_data_at(&d);
    let l = Ledger::open(
        Some(path.clone()),
        tokio::sync::broadcast::channel(16).0,
        Default::default(),
    );
    let t = 1_800_000_000;
    let reset = t + 18_000;
    let sid = Some("s-perpct");
    let seen = |used: f64, at: u64| {
        record_seen(
            &l,
            &own,
            sid,
            "claude-code",
            "q",
            reading_until(used, reset),
            at,
        );
    };
    let used = |input: u64| {
        l.spend.heard(
            "s-perpct",
            &format!(
                r#"{{"type":"message_start","message":{{"usage":{{"input_tokens":{input}}}}}}}"#
            ),
            t,
        );
    };
    seen(0.10, t);
    used(1_000);
    seen(0.11, t + 60);
    for i in 0..5 {
        seen(0.11, t + 120 + i);
        used(10_000);
    }
    assert!(
        on_disk(&path).accounts[0].per_pct.is_empty(),
        "量到之前账上有了这一格"
    );
    seen(0.15, t + 600);
    let o = &on_disk(&path).accounts[0];
    assert_eq!(o.tokens_per_pct("five_hour", t + 600), Some(12_500));
    let key = |_: &str| None;
    assert_eq!(
        crate::accounts::quota::show::windows_of(o, &key, t + 600)[0].tokens_per_pct,
        Some(12_500),
        "显示态没带上"
    );
    assert_eq!(
        o.tokens_per_pct("five_hour", t + 600 + 7 * 86_400),
        Some(12_500),
        "满 7 天那一刻还该作数"
    );
    assert_eq!(
        o.tokens_per_pct("five_hour", t + 600 + 7 * 86_400 + 1),
        None,
        "过了 7 天还作数"
    );
    std::fs::remove_dir_all(&d).ok();
}

/// 额度账读答过真序列化器（判它的各格）。
fn answered(path: Option<&Path>, now: u64) -> serde_json::Value {
    serde_json::to_value(answer_of(path, now)).expect("json")
}

/// ★ E2 用速：两次不同的采样 ⇒ 斜率（点 / 分）；只有一次 · 过了 30 分 · 不涨 ⇒ 没有。
#[test]
fn the_rate_is_the_slope_of_two_recent_samples_in_points_per_minute() {
    let l = Ledger::at(None);
    let own = crate::platform::fs::Own::for_tests();
    let t = 1_800_000_000;
    let reset = t + 18_000;
    record_seen(
        &l,
        &own,
        None,
        "claude-code",
        "q",
        reading_until(0.40, reset),
        t,
    );
    let o = l.entry("claude-code", "q").expect("entry");
    assert_eq!(o.rate("five_hour", t + 10), None, "只有一个采样");
    record_seen(
        &l,
        &own,
        None,
        "claude-code",
        "q",
        reading_until(0.46, reset),
        t + 600,
    );
    let o = l.entry("claude-code", "q").expect("entry");
    // 600 秒涨 6 点 ⇒ 0.6 点 / 分。
    let r = o.rate("five_hour", t + 700).expect("有用速");
    assert!((r - 0.6).abs() < 1e-9, "{r}");
    assert_eq!(
        o.rate("five_hour", t + 600 + 1_801),
        None,
        "涨在 30 分钟以前"
    );
    assert_eq!(o.rate("seven_day", t + 700), None, "没这个窗口");
    let down = Ledger::at(None);
    let own = crate::platform::fs::Own::for_tests();
    record_seen(
        &down,
        &own,
        None,
        "claude-code",
        "q",
        reading_until(0.46, reset),
        t,
    );
    record_seen(
        &down,
        &own,
        None,
        "claude-code",
        "q",
        reading_until(0.40, reset),
        t + 600,
    );
    let o = down.entry("claude-code", "q").expect("entry");
    assert_eq!(o.rate("five_hour", t + 700), None, "不涨");
}

/// ★ 用速照最近 30 分钟里最早一次变的值起算：读数一格一格涨了 25 分钟、最后 3 秒里跳了 3 点（10-30 夜里 y 那一串的样子）
/// ⇒ 0.4 点 / 分，不是最后两次之间的 60 点 / 分；比 30 分钟还早的那几次不算进来。
#[test]
fn the_rate_spans_the_last_half_hour_not_the_last_two_readings() {
    let l = Ledger::at(None);
    let own = crate::platform::fs::Own::for_tests();
    let t = 1_800_000_000;
    let reset = t + 18_000;
    let seen = |used: f64, at: u64| {
        record_seen(
            &l,
            &own,
            None,
            "claude-code",
            "q",
            reading_until(used, reset),
            at,
        );
    };
    seen(0.70, t - 3_600);
    seen(0.75, t - 2_400);
    for (i, u) in [0.80, 0.81, 0.82, 0.83, 0.84, 0.87].into_iter().enumerate() {
        seen(u, t + 300 * i as u64);
    }
    seen(0.90, t + 1_503);
    let o = l.entry("claude-code", "q").expect("entry");
    let r = o.rate("five_hour", t + 1_510).expect("有用速");
    // 0.80（t）→ 0.90（t + 1503）：10 点 / 25.05 分。
    assert!((r - 10.0 / (1_503.0 / 60.0)).abs() < 1e-6, "{r}");
}

fn wins(windows: Vec<QuotaWindow>) -> QuotaReading {
    QuotaReading {
        status: Some(QuotaStatus::Allowed),
        refused: false,
        limiting: None,
        resets_at: None,
        windows,
        overage: None,
    }
}

/// ★ 号的历史：周窗换窗那一下记下旧窗最后读到的用量 ⇒ 周重置时剩几点；5 小时新窗头一次看到的时刻对着重置时刻的
/// 10 分钟格 ⇒ 开窗往哪边取整。盘上那份带着、重开读得回来；同一次换窗内存与落盘两处各并一次也只记一笔；quota-read 不往外给。
#[test]
fn a_turnover_keeps_what_the_week_left_and_which_way_the_5h_grid_rounds() {
    let d = test_home::scratch("history");
    let path = d.join(FILE_NAME);
    let own = crate::platform::fs::Own::issue_data_at(&d);
    let l = Ledger::at(Some(path.clone()));
    let wk = 1_792_792_800; // 10-23 22:00 周窗重置
    let old5 = wk - 600;
    let open = 1_792_843_620; // 10-24 12:07 头一次看到新的 5 小时窗
    let r5 = 1_792_861_800; // 那一窗 17:10 重置
    let see = |w7: QuotaWindow, w5: QuotaWindow, at: u64| {
        record_seen(&l, &own, None, "claude-code", "q", wins(vec![w7, w5]), at);
    };
    see(
        win("seven_day", 0.62, Some(wk)),
        win("five_hour", 0.30, Some(old5)),
        wk - 3600,
    );
    see(
        win("seven_day", 0.01, Some(wk + 7 * 86_400)),
        win("five_hour", 0.30, Some(old5)),
        wk + 300,
    );
    see(
        win("seven_day", 0.02, Some(wk + 7 * 86_400)),
        win("five_hour", 0.01, Some(r5)),
        open,
    );
    let o = l.entry("claude-code", "q").expect("entry");
    assert_eq!(o.weekly_left_at_reset("seven_day"), vec![38.0]);
    assert_eq!(
        o.window_grid("five_hour", 18_000),
        Some(true),
        "12:07 看到 · 17:10 重置 ⇒ 往上取"
    );
    assert_eq!(o.history["seven_day"].ends.len(), 1, "{:?}", o.history);
    assert_eq!(o.history["five_hour"].opens.len(), 1, "{:?}", o.history);
    let again = Ledger::at(Some(path.clone()))
        .entry("claude-code", "q")
        .expect("restart");
    assert_eq!(again.history, o.history, "盘上那份没带上");
    assert!(
        answered(Some(&path), open)["accounts"][0]
            .get("history")
            .is_none(),
        "quota-read 不该带历史"
    );
    std::fs::remove_dir_all(&d).ok();
}

/// ★ 开窗取整往哪边：差（头一次看到 − 格）有一次比 −60 秒还早 ⇒ 往上取；作数的至少 3 次、都不早于格 ⇒ 往下取；
/// 不到 3 次说不出；差 10 分钟以上的那次（隔了好久才看到）不作数。周剩几点留最近 4 次、开窗留最近 8 次。
#[test]
fn the_grid_says_down_only_after_three_close_opens_and_up_on_one_early_one() {
    let l = Ledger::at(None);
    let own = crate::platform::fs::Own::issue_data_at(&test_home::scratch("grid"));
    let base = 1_792_861_800;
    let at = |k: u64| base + k * 36_000;
    let see = |k: u64, late: i64| {
        let r = at(k);
        let seen = (r as i64 - 18_000 + late) as u64;
        record_seen(
            &l,
            &own,
            None,
            "claude-code",
            "q",
            wins(vec![win("five_hour", 0.01, Some(r))]),
            seen,
        );
        l.entry("claude-code", "q").expect("entry")
    };
    see(0, 120);
    see(1, 120);
    assert_eq!(
        see(2, 120).window_grid("five_hour", 18_000),
        None,
        "两次说不出"
    );
    assert_eq!(
        see(3, 900).window_grid("five_hour", 18_000),
        None,
        "隔了 15 分钟才看到的不作数"
    );
    assert_eq!(see(4, 120).window_grid("five_hour", 18_000), Some(false));
    assert_eq!(see(5, -180).window_grid("five_hour", 18_000), Some(true));
    let mut o = see(6, 120);
    for k in 7..14 {
        o = see(k, 120);
    }
    assert_eq!(o.history["five_hour"].opens.len(), 8);
    assert_eq!(o.history["five_hour"].ends.len(), 4);
    assert_eq!(
        o.history["five_hour"].opens.last().map(|x| x.resets),
        Some(at(13)),
        "留的是最近的"
    );
}
