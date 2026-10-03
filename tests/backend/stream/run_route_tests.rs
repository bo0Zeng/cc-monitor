//! 流归位那一跳的丢弃：每一种都数、都出声（同一种第 1、2、4、8… 次说一行）；正常说完不算。
//! 要求：流式没上屏时两侧都该说得出丢在哪一跳、为什么（原先这一跳一声不出，只能读码推）。
//! 帧怎么折、归哪个运行的判据住 `runs_guard.rs`（两套形状各跑一遍）；本文件只管丢弃的账。

use super::*;
use crate::agents::StreamFamily;
use std::sync::{Arc, Mutex};

const SID: &str = "s-1";

fn router() -> RunRouter {
    RunRouter::new(
        Arc::new(RunBook::default()),
        vec![StreamFamily {
            face: crate::agents::fake::runs::STREAM,
            owns: true,
        }],
    )
}

fn data(stream: &str, resp: u64, n: u64, d: &str) -> TapEvent {
    TapEvent {
        stream: stream.into(),
        owner: String::new(),
        resp,
        n,
        body: TapBody::Data(d.into()),
    }
}

fn end(resp: u64, n: u64) -> TapEvent {
    TapEvent {
        stream: SID.into(),
        owner: String::new(),
        resp,
        n,
        body: TapBody::End { broken: false },
    }
}

const OPEN: &str = r#"{"ev":"open","rid":"r"}"#;
const PART: &str = r#"{"ev":"part","at":0,"is":"words"}"#;
const SHUT: &str = r#"{"ev":"shut"}"#;

/// 本线程上 `f` 期间 `tracing` 说了什么（每行一条）。
pub(crate) fn heard(f: impl FnOnce()) -> Vec<String> {
    #[derive(Clone)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Sink {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let buf = Arc::new(Mutex::new(Vec::new()));
    let sink = Sink(buf.clone());
    let sub = tracing_subscriber::fmt()
        .with_writer(move || sink.clone())
        .with_ansi(false)
        .finish();
    tracing::subscriber::with_default(sub, f);
    let text = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    text.lines().map(str::to_string).collect()
}

/// 头件丢 ×3 · 没会话标签 · 折不出开始 · 断号 · 满了挤掉最老的：各数各的；正常说完的那一段不算；
/// 头件丢第 1、2 次各说一行、第 3 次不说。
#[test]
fn every_kind_of_drop_is_counted_and_spoken_at_powers_of_two() {
    let mut r = router();
    let said = heard(|| {
        for resp in 0..3 {
            r.on_tap(data(SID, resp, 1, PART)); // 开头丢了
        }
        r.on_tap(data("", 10, 0, OPEN)); // 没有会话标签
        r.on_tap(data(SID, 11, 0, r#"{"ev":"what"}"#)); // 折不出开始
        r.on_tap(data(SID, 12, 0, OPEN));
        r.on_tap(data(SID, 12, 2, PART)); // 1 号丢了

        // 正常说完：不算丢。
        for (n, d) in [OPEN, PART, SHUT].into_iter().enumerate() {
            r.on_tap(data(SID, 13, n as u64, d));
        }
        r.on_tap(end(13, 3));
        // 攒满之后再来一段 ⇒ 挤最老的。
        for resp in 100..100 + ROUTE_RESPS_KEEP as u64 + 1 {
            r.on_tap(data(SID, resp, 0, OPEN));
        }
    });
    let want: BTreeMap<Lost, u64> = [
        (Lost::Head, 3),
        (Lost::NoStream, 1),
        (Lost::NoFace, 1),
        (Lost::Gap, 1),
        (Lost::Evicted, 1),
    ]
    .into_iter()
    .collect();
    assert_eq!(r.lost, want);
    let spoke: Vec<&str> = said
        .iter()
        .filter_map(|l| l.split("没放出去 / 半路收了：").nth(1))
        .map(|t| t.split('（').next().unwrap_or(""))
        .collect();
    assert_eq!(
        spoke,
        ["Head", "Head", "NoStream", "NoFace", "Gap", "Evicted"],
        "出声的节奏不对：{said:#?}"
    );
}
