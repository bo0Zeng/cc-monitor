//! 会话流帧的跨语言金样（`tests/__fixtures__/session-stream.golden.jsonl`，后端测试用真序列化器写，每种帧
//! 「全格」「最少格」两行）对这一侧的判据：
//! ① 每一行过帧门、按类型解得开、解成那一种；
//! ② 金样的种类 == `cmd-wire` 帧表的种类 ＋ hello（两向）；
//! ③ 进不来的帧不被吞：每条连接每种说一次（健康信息一句、日志一条），之后只记账。

use super::*;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

const GOLDEN: &str = include_str!("../../../__fixtures__/session-stream.golden.jsonl");

/// 一帧的线上 kind（照它自己的序列化器）。
fn kind_of(f: &Frame) -> String {
    serde_json::to_value(f).unwrap()["kind"]
        .as_str()
        .unwrap()
        .to_string()
}

/// 握手那一刻帧表与本侧一致的那条连接（帧门全放行）。
fn our_gate() -> std::sync::Arc<crate::inbound_client::InboundClient> {
    let (mine, _theirs) = tokio::io::duplex(1024);
    let hello = crate::inbound_client::witness_of(&crate::frame_fixtures::our_hello(&[])).unwrap();
    crate::inbound_client::park(mine).into_client(hello)
}

/// 同一条连接，只是那台那一版的 `kind` 那一种形状不同。
fn gate_differing(kind: &str) -> std::sync::Arc<crate::inbound_client::InboundClient> {
    let (mine, _theirs) = tokio::io::duplex(1024);
    let mut h = crate::frame_fixtures::our_hello(&[]);
    if let cmd_wire::frame::Frame::Hello(h) = &mut h {
        h.frames.insert(kind.to_string(), "00000000".to_string());
    }
    let hello = crate::inbound_client::witness_of(&h).unwrap();
    crate::inbound_client::park(mine).into_client(hello)
}

fn without(o: &Map<String, Value>, key: &str) -> String {
    let mut o = o.clone();
    o.remove(key);
    Value::Object(o).to_string()
}

fn golden_row(kind: &str) -> Map<String, Value> {
    GOLDEN
        .lines()
        .filter_map(|l| match serde_json::from_str(l) {
            Ok(Value::Object(o)) if o["kind"] == kind => Some(o),
            _ => None,
        })
        .min_by_key(Map::len)
        .unwrap_or_else(|| panic!("金样里没有 `{kind}`"))
}

/// ① 每一行过门、解得开、解成那一种（记录那一格原文收下）。
#[tokio::test]
async fn every_golden_row_passes_the_gate_and_decodes_as_its_kind() {
    let gate = our_gate();
    let mut notes = UnreadNotes::new("台架-origin", "台架");
    let mut tally = crate::frame_tally::FrameTally::new("台架");
    let mut n = 0;
    for line in GOLDEN.lines() {
        let want = serde_json::from_str::<Value>(line).unwrap()["kind"]
            .as_str()
            .unwrap()
            .to_string();
        let f = notes
            .take(line, Some(&gate), &mut tally, &|p| {
                panic!("金样这一行进不来：{line}：{p:?}")
            })
            .unwrap();
        assert_eq!(kind_of(&f), want, "金样这一行解成了别的种类：{line}");
        n += 1;
    }
    std::mem::forget(tally);
    assert!(n >= 48, "金样只读到 {n} 行 —— 读空了");
}

/// ② 金样的种类 == 帧表的种类 ＋ hello（帧表由 `cmd-wire` 的类型现算，金样由后端的序列化器写）。
#[test]
fn the_golden_kinds_are_exactly_the_frame_table_and_hello() {
    let golden: BTreeSet<String> = GOLDEN
        .lines()
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    let table: BTreeSet<String> = cmd_wire::frame_shapes::FRAMES
        .iter()
        .map(|(k, _)| k.to_string())
        .chain(["hello".to_string()])
        .collect();
    assert_eq!(golden, table, "金样的种类与帧表对不上");
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

/// ③ 一条流里混进帧门拦下的（那台那一版形状不同 · 这一侧不认）与解不开的：好帧照收；每种第一次 ⇒ 健康信息一句、日志一条；
/// 同一条连接上再来只记账；换一条连接重新说。
#[tokio::test]
async fn an_unread_frame_is_said_once_per_kind_per_connection() {
    let hello = Value::Object(golden_row("hello")).to_string();
    let line_ok = Value::Object(golden_row("line")).to_string();
    let line_bad = without(&golden_row("line"), "seq");
    let status = Value::Object(golden_row("session_status")).to_string();
    let stream = [
        hello.as_str(),
        r#"{"kind":"future_thing"}"#,
        line_ok.as_str(),
        r#"{"kind":"future_thing","x":1}"#,
        line_bad.as_str(),
        status.as_str(),
        line_bad.as_str(),
        status.as_str(),
    ];
    let gate = gate_differing("session_status");

    let warns = Warns::default();
    let sink = warns.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || sink.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .finish();
    let said = std::cell::RefCell::new(Vec::<crate::ui_contract::RemoteHealthPayload>::new());
    let took: Vec<String> = tracing::subscriber::with_default(subscriber, || {
        tracing::callsite::rebuild_interest_cache();
        let mut notes = UnreadNotes::new("台架-origin", "台架");
        let mut tally = crate::frame_tally::FrameTally::new("台架");
        let took = stream
            .iter()
            .filter_map(|l| notes.take(l, Some(&gate), &mut tally, &|p| said.borrow_mut().push(p)))
            .map(|f| kind_of(&f))
            .collect();
        std::mem::forget(tally);
        took
    });
    assert_eq!(took, ["hello", "line"], "好帧没照收 / 拦下的漏进来了");

    let said = said.into_inner();
    let got: Vec<(&str, &str, &str)> = said
        .iter()
        .map(|p| (p.kind.as_str(), p.origin.as_str(), p.detail.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("frame-differs", "台架-origin", "frames: future_thing"),
            ("frame-shape", "台架-origin", ""),
            ("frame-differs", "台架-origin", "frames: session_status"),
        ],
        "健康信息不是每种恰好一句：{said:?}"
    );
    assert_eq!(said[0].message, copy_core::backend_frames_differ("台架"));
    assert!(
        said[1].message.contains("台架") && said[1].message.contains("line"),
        "解不开那一句没点名那台与那一种：{}",
        said[1].message
    );
    let log = String::from_utf8(warns.0.lock().unwrap().clone()).unwrap();
    for (needle, what) in [
        ("frame `future_thing` differs", "不认的那一种"),
        ("frame `session_status` differs", "形状不同的那一种"),
        ("frame `line` malformed", "解不开的那一种"),
    ] {
        assert_eq!(
            log.matches(needle).count(),
            1,
            "{what}日志不是恰好一条：\n{log}"
        );
    }

    // 换一条连接：重新说。
    let again = std::cell::RefCell::new(0usize);
    let mut notes = UnreadNotes::new("台架-origin", "台架");
    let mut tally = crate::frame_tally::FrameTally::new("台架");
    let _ = notes.take(stream[1], Some(&gate), &mut tally, &|_| {
        *again.borrow_mut() += 1
    });
    std::mem::forget(tally);
    assert_eq!(again.into_inner(), 1, "新连接上第一次见到也要说");
}
