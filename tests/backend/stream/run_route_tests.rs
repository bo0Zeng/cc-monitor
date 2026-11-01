//! 流归位那一跳的丢弃：每一种都数、都出声（同一种第 1、2、4、8… 次说一行）；正常说完不算。
//! 要求：流式没上屏时两侧都该说得出丢在哪一跳、为什么（原先这一跳一声不出，只能读码推）。
//! 帧怎么折、归哪个运行的判据住 `runs_guard.rs`（两套形状各跑一遍）；本文件只管丢弃的账。

use super::*;
use crate::agents::StreamFamily;
use std::sync::Arc;

const SID: &str = "s-1";
/// 假的那一家自报运行的头（判据自己定的名字）。
const RUN: &str = "x-run";

fn router() -> RunRouter {
    RunRouter::new(
        Arc::new(RunBook::default()),
        vec![StreamFamily {
            face: crate::agents::fake::runs::STREAM,
            run: Some(RUN),
            side: None,
        }],
    )
}

fn data(stream: &str, resp: u64, n: u64, d: &str) -> TapEvent {
    TapEvent {
        stream: stream.into(),
        heads: Vec::new(),
        resp,
        n,
        body: TapBody::Data(d.into()),
    }
}

fn end(resp: u64, n: u64) -> TapEvent {
    TapEvent {
        stream: SID.into(),
        heads: Vec::new(),
        resp,
        n,
        body: TapBody::End { broken: false },
    }
}

const OPEN: &str = r#"{"ev":"open","rid":"r"}"#;
const PART: &str = r#"{"ev":"part","at":0,"is":"words"}"#;
const SHUT: &str = r#"{"ev":"shut"}"#;

/// 本线程上 `f` 期间 `tracing` 说了什么（量具住 `test-meters`；几份判据经这里引它）。
pub(crate) use test_meters::heard;

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
    let run_of = |frames: Vec<Routed>| -> Vec<Option<String>> {
        frames.into_iter().map(|f| f.run).collect()
    };
    let owned = |owner: &str, resp: u64| TapEvent {
        heads: vec![(RUN, owner.into())],
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

/// ★ 旁路请求（这一家声明的分类头报的是旁路的那几个值：预测下一句 · 标题 · 压缩……）不上活卡：一件都不放出去，照丢段那样数一次、说一行。
/// 分类头报的是对话（`main`）⇒ 照常放；没带分类头（旧版客户端 · 环境变量没写上）⇒ 照今天当主运行放，并照实说一行「没带分类头」，不猜。
#[test]
fn a_side_request_never_reaches_a_card() {
    const SIDE: crate::agents::SideClass = crate::agents::SideClass {
        header: "x-class",
        side: &["auxiliary", "compaction"],
    };
    let mut r = RunRouter::new(
        Arc::new(RunBook::default()),
        vec![StreamFamily {
            face: crate::agents::fake::runs::STREAM,
            run: Some(RUN),
            side: Some(SIDE),
        }],
    );
    let classed = |class: &str, resp: u64, n: u64, d: &str| TapEvent {
        heads: if class.is_empty() || n != 0 {
            Vec::new()
        } else {
            vec![(SIDE.header, class.into())]
        },
        ..data(SID, resp, n, d)
    };
    let mut out = Vec::new();
    let said = heard(|| {
        for (n, d) in [OPEN, PART, SHUT].into_iter().enumerate() {
            out.extend(r.on_tap(classed("auxiliary", 1, n as u64, d)));
        }
        out.extend(r.on_tap(end(1, 3)));
        assert!(out.is_empty(), "旁路的那一段放出去了：{out:?}");
        out.extend(r.on_tap(classed("main", 2, 0, OPEN)));
        assert_eq!(out.len(), 1, "对话那一发没放出去");
        out.extend(r.on_tap(classed("", 3, 0, OPEN)));
        assert_eq!(out.len(), 2, "没带分类头的那一发没照主运行放");
    });
    assert_eq!(r.lost.get(&Lost::Side), Some(&1));
    assert!(
        said.iter().any(|l| l.contains("Side")),
        "旁路那一段没出声：{said:#?}"
    );
    assert!(
        said.iter().any(|l| l.contains("没带分类头")),
        "没带分类头那一发没照实说：{said:#?}"
    );
}
