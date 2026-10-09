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
    // 另握一个空的 `Dispatch`：只剩一个登记的 dispatcher 时，tracing 按「碰到 callsite 的那条线程」的默认算 interest 并缓存 ——
    // 并行的别的测试先碰到就缓存成「没人要」，这里就听不见。进来先重算一遍缓存。
    let _spare = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    tracing::subscriber::with_default(sub, || {
        tracing::callsite::rebuild_interest_cache();
        f()
    });
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

/// ★ 自报的运行等于会话标签 ⇒ 主运行（有的家主运行也带那个头）；不等 ⇒ 那个子运行。
#[test]
fn an_owner_equal_to_the_stream_is_the_main_run() {
    let mut r = router();
    let run_of = |frames: Vec<Frame>| -> Vec<Option<String>> {
        frames
            .into_iter()
            .map(|f| match f {
                Frame::Tap { run, .. } => run,
                _ => panic!("不是 tap 帧"),
            })
            .collect()
    };
    let owned = |owner: &str, resp: u64| TapEvent {
        owner: owner.into(),
        ..data(SID, resp, 0, OPEN)
    };
    assert_eq!(
        run_of(r.on_tap(owned(SID, 1))),
        vec![None],
        "等于会话标签的那一发没归主运行"
    );
    assert_eq!(
        run_of(r.on_tap(owned("child-7", 2))),
        vec![Some("child-7".to_string())],
        "不等的那一发没归它自报的子运行"
    );
}
