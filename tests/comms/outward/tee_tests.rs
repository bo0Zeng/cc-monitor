use super::*;

/// 「这条判据不测上限那一格」的写法：给一个**永远触发不了**的上限
/// （理由同 `http1.rs` 那份 `NO_CAP`：期望值不许拿被测常量算）。
const NO_CAP: usize = usize::MAX;

#[test]
fn splits_sse_data_lines_across_arbitrary_split_points() {
    let wire = b"event: message_start\ndata: {\"a\":1}\n\ndata: {\"b\":2}\n\ndata: [DONE]\n\n";
    let mut s = SseSplitter::default();
    let mut got = Vec::new();
    for b in wire.iter() {
        got.extend(s.feed(&[*b], NO_CAP));
    }
    // 期望值手写：两个事件，`[DONE]` 与非 data 行都不算。
    assert_eq!(got, vec!["{\"a\":1}".to_string(), "{\"b\":2}".to_string()]);
}

#[test]
fn a_half_delivered_line_is_not_emitted_until_it_completes() {
    let mut s = SseSplitter::default();
    // ⚠ 这里刻意用中括号载荷：只读护栏的剥法按**大括号配平**，
    // 测试串里出现不配对的大括号会把剥除边界带偏（该护栏头注逐字警告过这个形状）。
    assert!(s.feed(b"data: [1,", NO_CAP).is_empty(), "半行不许吐");
    assert_eq!(s.feed(b"2]\n", NO_CAP), vec!["[1,2]".to_string()]);
}

/// ★ `TEE_DECODE_CAP` 在 `SseSplitter` 这一侧的那一格。
///
/// 分母 = 我列出的这 **3** 形：①超长半行被丢掉且**计数**；②它**不连坐**后面的完整行；
/// ③没超上限时**一个字节都不丢**（非空对照 —— 没有它，「丢了 N 字节」可能只是它见谁丢谁）。
#[test]
fn an_endless_sse_line_is_dropped_and_counted_instead_of_growing_forever() {
    const CAP: usize = 64; // 手写字面量，不引生产常量
    let mut s = SseSplitter::default();

    // ③ 非空对照先做：正常的行一个字节都不许丢。
    assert_eq!(
        s.feed(b"data: {\"a\":1}\n", CAP),
        vec!["{\"a\":1}".to_string()]
    );
    assert_eq!(s.take_dropped(), 0, "正常的行不许丢");

    // ① 一条永不换行的超长行：吐不出东西，且**丢的字节数被记下来**。
    let long = vec![b'x'; CAP * 2];
    assert!(s.feed(&long, CAP).is_empty(), "超长的半行不许吐出来");
    let dropped = s.take_dropped();
    assert!(
        dropped >= CAP as u64,
        "丢了 {dropped} 字节 —— 超上限的半行必须被丢掉**并计数**（不许静默）"
    );
    assert_eq!(s.take_dropped(), 0, "取走之后账要清零，不许重复报");

    // ② 不连坐：紧跟其后的完整行照样拆得出来。
    assert_eq!(
        s.feed(b"tail\ndata: {\"b\":2}\n", CAP),
        vec!["{\"b\":2}".to_string()],
        "丢掉超长那一行之后，后面的完整行必须照常吐"
    );
}

// 这里原是 NDJSON 行落点那几条（meta 行 · 序号单调 · 上游载荷逃不出 `event` 那一格 · 队列满丢行且报账 · JSON 转义）：
//   那个落点随独立 `--relay` 删了。tap 那一形的逐件语义（先占号再投递 · 超界 / 投不进号照占 · 收尾带总号数 · 非 SSE 不交）
//   住 `host_tests`（真起中转、走生产接线）。

/// 名单上那几个头（[`super::super::Destinations::relay_headers`]）的 (名, 值) 只随 0 号那一件交一次：
/// 后面的事件与收尾那一件不再抄（接收侧在 0 号那一件上认归属与分类）。中转不解释它们。
#[test]
fn the_listed_headers_ride_only_the_first_event() {
    #[derive(Default)]
    struct Got(std::sync::Mutex<Vec<TapEvent>>);
    impl TapPort for Got {
        fn offer(&self, ev: TapEvent) -> bool {
            self.0.lock().unwrap().push(ev);
            true
        }
    }
    let got = std::sync::Arc::new(Got::default());
    let tee = TeeSink::to_port(got.clone());
    let heads = [("h-class", "auxiliary"), ("h-beta", "a-1, b-2")];
    let id = super::super::StreamId {
        stream: "s-1",
        heads: &heads,
    };
    let mut at = tee.open();
    tee.event(id, &mut at, r#"{"type":"message_start"}"#);
    tee.event(id, &mut at, r#"{"type":"ping"}"#);
    tee.close(id, at, false);
    let got = got.0.lock().unwrap();
    assert_eq!(got.len(), 3);
    let want: Vec<(&str, String)> = heads.iter().map(|&(k, v)| (k, v.to_string())).collect();
    assert_eq!(got[0].heads, want, "0 号那一件带名单上的头");
    assert!(
        got[1].heads.is_empty() && got[2].heads.is_empty(),
        "后面的不再抄：{got:?}"
    );
}
