//! 会话流帧的跨语言金样（`tests/__fixtures__/session-stream.golden.jsonl`，后端测试用真序列化器写，每种帧
//! 「全格」「最少格」两行）对这一侧解析器的三条判据：
//! ② 每一行解得出、解成那一种；「最少格」逐格删 ⇒ 形状不对，「全格」比「最少格」多出的格逐格删 ⇒ 照样解得出
//!   （必填集合取自金样，判定来自真解析器）；
//! ③ 认得的种类 == 金样的种类（两向）；
//! ④ 不认识的种类不被吞：每条连接每种说一次（健康信息一句、日志一条），之后只记账。

use super::*;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const GOLDEN: &str = include_str!("../../../__fixtures__/session-stream.golden.jsonl");

/// 解成的那一种 → 线上 kind（穷尽，无兜底臂）。
fn kind_of(f: &InboundFrame) -> &'static str {
    match f {
        InboundFrame::Hello { .. } => "hello",
        InboundFrame::Line { .. } => "line",
        InboundFrame::SessionAdded { .. } => "session_added",
        InboundFrame::SessionsReplayed => "sessions_replayed",
        InboundFrame::SessionFileNotice {
            change: FileChange::Gone,
            ..
        } => "session_file_gone",
        InboundFrame::SessionFileNotice { .. } => "session_file_reread",
        InboundFrame::SessionStatus { .. } => "session_status",
        InboundFrame::SessionRemoved { .. } => "session_removed",
        InboundFrame::SessionState { .. } => "session_state",
        InboundFrame::SessionRuns { .. } => "session_runs",
        InboundFrame::SessionBranch { .. } => "session_branch",
        InboundFrame::Overflow { .. } => "overflow",
        InboundFrame::Reply { .. } => "reply",
        InboundFrame::Cancelled { .. } => "cancelled",
        InboundFrame::AccountsChanged => "accounts_changed",
        InboundFrame::ProfilesChanged => "profiles_changed",
        InboundFrame::TasksChanged { .. } => "tasks_changed",
        InboundFrame::LinkData { .. } => "link_data",
        InboundFrame::LinkEnd { .. } => "link_end",
        InboundFrame::Transfer { .. } => "transfer",
        InboundFrame::Tap(_) => "tap",
        InboundFrame::Probe { .. } => "probe",
        InboundFrame::TerminalScreen { .. } => "terminal_screen",
        InboundFrame::TerminalFollowEnd { .. } => "terminal_follow_end",
        InboundFrame::TurnEnd => "turn_end",
        InboundFrame::QuotaChanged => "quota_changed",
        InboundFrame::RotationChanged { .. } => "rotation_changed",
        InboundFrame::RotationRulesChanged => "rotation_rules_changed",
        InboundFrame::PlanChanged { .. } => "plan_changed",
    }
}

/// 金样按种类分组：`kind → (全格, 最少格)`（键少的那一行是最少格；没有可选格的两行相同）。
fn golden_pairs() -> BTreeMap<String, (Map<String, Value>, Map<String, Value>)> {
    let mut rows: BTreeMap<String, Vec<Map<String, Value>>> = BTreeMap::new();
    for line in GOLDEN.lines() {
        let Value::Object(o) = serde_json::from_str(line).expect("金样每行是 JSON") else {
            panic!("金样这一行不是对象：{line}");
        };
        let kind = o["kind"].as_str().expect("金样每行带 kind").to_string();
        rows.entry(kind).or_default().push(o);
    }
    rows.into_iter()
        .map(|(kind, mut two)| {
            assert_eq!(two.len(), 2, "金样里 `{kind}` 不是恰好两行");
            two.sort_by_key(Map::len);
            let min = two.remove(0);
            let full = two.remove(0);
            assert!(
                min.keys().all(|k| full.contains_key(k)),
                "`{kind}` 的最少格不是全格的子集"
            );
            (kind, (full, min))
        })
        .collect()
}

fn without(o: &Map<String, Value>, key: &str) -> String {
    let mut o = o.clone();
    o.remove(key);
    Value::Object(o).to_string()
}

/// ② 每一行解得出、解成那一种；必填 == 最少格里的格（删哪一格都判形状不对），全格多出来的格删了照样解得出。
#[test]
fn every_golden_row_parses_and_exactly_the_minimal_fields_are_required() {
    for line in GOLDEN.lines() {
        let want = serde_json::from_str::<Value>(line).unwrap()["kind"]
            .as_str()
            .unwrap()
            .to_string();
        let f = parse_frame(line).unwrap_or_else(|e| panic!("金样这一行解不出（{e}）：{line}"));
        assert_eq!(kind_of(&f), want, "金样这一行解成了别的种类：{line}");
    }
    let pairs = golden_pairs();
    let mut required = 0usize;
    for (kind, (full, min)) in &pairs {
        for key in min.keys().filter(|k| *k != "kind") {
            required += 1;
            match parse_frame(&without(min, key)) {
                Err(Unread::BadShape { kind: k, .. }) if k == *kind => {}
                other => panic!("`{kind}` 删掉必填格 `{key}` 却没判形状不对：{other:?}"),
            }
        }
        for key in full.keys().filter(|k| !min.contains_key(*k)) {
            let f = parse_frame(&without(full, key))
                .unwrap_or_else(|e| panic!("`{kind}` 删掉可选格 `{key}` 就解不出了：{e}"));
            assert_eq!(kind_of(&f), kind);
        }
    }
    assert!(
        required > pairs.len(),
        "金样里一共只有 {required} 个必填格 —— 金样读空了，本条在空转"
    );
}

/// ③ 认得的种类（`parse_frame` 的臂）== 金样的种类（== 后端 `Frame` 的全部变体，后端那一侧钉）。
#[test]
fn the_kinds_this_side_knows_are_exactly_the_golden_kinds() {
    let golden: BTreeSet<String> = golden_pairs().into_keys().collect();
    let known = crate::guard_support::parse_frame_kinds();
    assert_eq!(
        known, golden,
        "认得的种类与金样对不上 —— 后端加了一种帧要在 `parse_frame` 补一条臂（消费或认识但不消费）"
    );
}

/// 抓本线程上 warn 级以上的日志。
#[derive(Clone, Default)]
struct Warns(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
impl std::io::Write for Warns {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// ④ 一条流里混进不认识的种类与缺必填格的帧：好帧照收；每种第一次 ⇒ 健康信息一句、日志一条；
/// 同一条连接上再来只记账；换一条连接重新说。
#[test]
fn an_unread_frame_is_said_once_per_kind_per_connection() {
    let pairs = golden_pairs();
    let hello = Value::Object(pairs["hello"].1.clone()).to_string();
    let line_ok = Value::Object(pairs["line"].1.clone()).to_string();
    let line_bad = without(&pairs["line"].1, "byte_offset");
    let stream = [
        hello.as_str(),
        r#"{"kind":"future_thing"}"#,
        line_ok.as_str(),
        r#"{"kind":"future_thing","x":1}"#,
        line_bad.as_str(),
        r#"{"kind":"other_new"}"#,
        line_bad.as_str(),
    ];

    let warns = Warns::default();
    let sink = warns.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || sink.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .finish();
    let said = std::cell::RefCell::new(Vec::<crate::ui_contract::RemoteHealthPayload>::new());
    let took: Vec<&'static str> = tracing::subscriber::with_default(subscriber, || {
        tracing::callsite::rebuild_interest_cache();
        let mut notes = UnreadNotes::new("台架-origin", "台架");
        let mut tally = crate::frame_tally::FrameTally::new("台架");
        let took = stream
            .iter()
            .filter_map(|l| notes.take(l, &mut tally, &|p| said.borrow_mut().push(p)))
            .map(|f| kind_of(&f))
            .collect();
        std::mem::forget(tally);
        took
    });
    assert_eq!(took, ["hello", "line"], "好帧没照收");

    let said = said.into_inner();
    let got: Vec<(&str, &str)> = said
        .iter()
        .map(|p| (p.kind.as_str(), p.origin.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("frame-unknown", "台架-origin"),
            ("frame-shape", "台架-origin"),
            ("frame-unknown", "台架-origin"),
        ],
        "健康信息不是每种恰好一句：{said:?}"
    );
    for (p, kind) in said.iter().zip(["future_thing", "line", "other_new"]) {
        assert!(
            p.message.contains("台架") && p.message.contains(kind),
            "那句话没点名那台与那一种：{}",
            p.message
        );
    }
    let log = String::from_utf8(warns.0.lock().unwrap().clone()).unwrap();
    assert_eq!(
        log.matches("unknown frame kind `future_thing`").count(),
        1,
        "同一种不认识的帧日志不是恰好一条：\n{log}"
    );
    assert_eq!(
        log.matches("frame `line` malformed").count(),
        1,
        "同一种形状不对的帧日志不是恰好一条：\n{log}"
    );

    // 换一条连接：重新说。
    let again = std::cell::RefCell::new(0usize);
    let mut notes = UnreadNotes::new("台架-origin", "台架");
    let mut tally = crate::frame_tally::FrameTally::new("台架");
    let _ = notes.take(stream[1], &mut tally, &|_| *again.borrow_mut() += 1);
    std::mem::forget(tally);
    assert_eq!(again.into_inner(), 1, "新连接上第一次见到也要说");
}
