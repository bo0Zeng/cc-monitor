use super::*;

/// 「这条判据不测上限那一格」的写法：给一个**永远触发不了**的上限。
/// ⚠ 刻意不写成 `TEE_DECODE_CAP` / `BODY_CAP` —— 拿被测的那个常量当期望值，
/// 判据就跟着它一起漂（本仓的「期望值必须手写」同一条纪律）。
const NO_CAP: usize = usize::MAX;

/// ★★ `阻-1(D3)`：**一个数就能把整个中转进程 abort 掉**这一格，今天有牙。
///
/// # 它钉的两件事，缺一不可
///
/// ㈠ **超上限要拒收**（`Ok(None)` ⇒ 调用方回 413）。死值验用的是 `BODY_CAP + 1`，
///    不是 `1e12` —— 后者在**没有上限**的版本上会让进程 **SIGABRT**，那是 **CRASH 不是红**
///    （判定行掉成 0），死值验拿不到「恰好这一格红」的读数。⇒ 用一个「超了但分配得动」的值。
///
/// ㈡ **一个字节都不许按 `n` 分配**。这一格用 `alloc_probe`（**线程级**分配高水位量具，
///    住 `crate::alloc_probe`，`VmHWM` 是进程级的、会把邻居测试算进来 —— 那份头注写着来历）。
///    没有 ㈡ 的话，「先 `vec![0u8; n]` 再判 `n > cap`」这种写法照样过 ㈠，
///    而它**仍然会 abort** —— 顺序错一行就前功尽弃，而 ㈠ 看不见顺序。
///
/// # 分母与非空对照
///
/// - `1e12` 那一形：**必须**一个字节不分配（阈值 1 MiB，比它小 6 个数量级）。
/// - 非空对照：同一把尺子量一条**正常**的读（8 MiB 的体）⇒ 高水位**必须**涨到 8 MiB 以上。
///   没有它，「峰值 = 0」可能只是量具坏了（那正是 `alloc_probe` 头注里逐字警告的
///   「源码落地不等于效果落地」）。
#[test]
fn an_oversized_content_length_is_refused_without_allocating_it() {
    const CAP: usize = 4 * 1024 * 1024; // 手写字面量，不引 BODY_CAP
    const HUGE: usize = 1_000_000_000_000;

    // ㈠ 超上限 ⇒ 拒收，且**一个字节都没从流里读走**。
    let src: &[u8] = b"hi";
    let mut r = std::io::Cursor::new(src);
    let base = crate::alloc_probe::reset_peak();
    let got = read_exact_body(&mut r, HUGE, CAP).expect("超上限不是 IO 错误，是一个答案");
    let peak = crate::alloc_probe::peak_since(base);
    assert!(got.is_none(), "超 cap 必须拒收（回 None ⇒ 调用方回 413）");
    assert_eq!(r.position(), 0, "拒收那一支不许从流里读走任何字节");
    assert!(
        peak < 1024 * 1024,
        "拒收那一支的本线程分配高水位是 {peak} 字节 —— 它不许随 `n` 走（n = {HUGE}）"
    );

    // ㈡ 刚好在上限上 ⇒ 收（边界是 `>`，不是 `>=`）。
    let body = vec![b'x'; CAP];
    let mut r = std::io::Cursor::new(&body[..]);
    let got = read_exact_body(&mut r, CAP, CAP).expect("io");
    assert_eq!(
        got.map(|v| v.len()),
        Some(CAP),
        "`n == cap` 必须收，边界是 `>`"
    );

    // ㈢ 非空对照：**正常**的读真的会把高水位顶上去 —— 否则上面那条「峰值不涨」是空真。
    let body = vec![b'y'; 8 * 1024 * 1024];
    let mut r = std::io::Cursor::new(&body[..]);
    let base = crate::alloc_probe::reset_peak();
    let got = read_exact_body(&mut r, body.len(), CAP * 4).expect("io");
    let peak = crate::alloc_probe::peak_since(base);
    assert_eq!(got.map(|v| v.len()), Some(8 * 1024 * 1024));
    assert!(
        peak >= 8 * 1024 * 1024,
        "非空对照：真读 8 MiB 时高水位只有 {peak} 字节 —— 量具没在量这条路"
    );

    // ㈣ 流比声明的短 ⇒ `Err(UnexpectedEof)`，**不是** `Ok(None)`（那是超上限**独占**的答案）。
    let mut r = std::io::Cursor::new(&b"abc"[..]);
    let e = read_exact_body(&mut r, 10, CAP).expect_err("短流必须是错误");
    assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof);
}

/// ★ `重要-2(D3)`：`Content-Length` 的三张脸必须分得开。
///
/// 分母 = 我列出的这 **7** 形。先前实现是 `…parse().ok()`，
/// 于是下表 `Unparsable` 那 4 形与 `Absent` 那 1 形**挤在同一个 `None` 里**
/// ⇒ 调用点把它们一律当成「没有请求体」，请求体被静默丢掉。
#[test]
fn content_length_tells_absent_apart_from_unparsable() {
    let head = |h: &str| {
        parse_request(format!("POST /x HTTP/1.1\r\n{h}\r\n\r\n").as_bytes()).expect("parse")
    };
    // 期望值全是手写字面量。
    assert_eq!(
        head("Host: x").content_length(),
        BodyLen::Absent,
        "没有这个头"
    );
    assert_eq!(
        head("Content-Length: 7").content_length(),
        BodyLen::Exact(7)
    );
    assert_eq!(
        head("Content-Length:   7  ").content_length(),
        BodyLen::Exact(7),
        "两端空白要吃掉"
    );
    for bad in ["7abc", "abc", "-1", "7, 7"] {
        assert_eq!(
            head(&format!("Content-Length: {bad}")).content_length(),
            BodyLen::Unparsable,
            "`{bad}` 必须是**读不懂**，不许退化成「没有请求体」"
        );
    }
}

/// ★ `重要-1(D3)` 的**判别器**那一格。分母 = 我列出的这 **9** 形。
#[test]
fn only_a_three_digit_1xx_status_counts_as_interim() {
    for yes in [
        "HTTP/1.1 100 Continue",
        "HTTP/1.1 103 Early Hints",
        "HTTP/1.0 101",
    ] {
        assert!(is_interim_status(yes), "{yes} 该算 1xx 中间响应");
    }
    for no in [
        "HTTP/1.1 200 OK",
        "HTTP/1.1 500 Internal Server Error",
        "HTTP/1.1 1",    // 不是三位
        "HTTP/1.1 1000", // 不是三位
        "HTTP/1.1 1xx",  // 不是三位数字
        "HTTP/1.1",      // 根本没有第二段
    ] {
        assert!(!is_interim_status(no), "{no} 不该算 1xx 中间响应");
    }
}

#[test]
fn parses_a_post_with_headers() {
    let raw = b"POST /s/agentA/sid/v1/messages?beta=true HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\nAuthorization: Bearer T\r\n\r\n";
    let h = parse_request(raw).expect("应当解析成功");
    assert_eq!(h.method, "POST");
    assert_eq!(h.target, "/s/agentA/sid/v1/messages?beta=true");
    assert_eq!(h.content_length(), BodyLen::Exact(3));
    assert_eq!(
        h.header("AUTHORIZATION"),
        Some("Bearer T"),
        "头名大小写不敏感"
    );
    assert!(!h.is_chunked_body());
}

#[test]
fn rejects_malformed_request_lines() {
    // 分母 = 我列出的这 5 形。
    for bad in [
        &b"GET\r\n\r\n"[..],
        &b"GET /x\r\n\r\n"[..],
        &b"GET /x FTP/1.0\r\n\r\n"[..],
        &b"GET x HTTP/1.1\r\n\r\n"[..],
        &b"GET /x HTTP/1.1\r\nbad header\r\n\r\n"[..],
    ] {
        assert!(parse_request(bad).is_none(), "这一形不该被接受");
    }
}

#[test]
fn read_head_stops_exactly_at_the_blank_line() {
    let mut src = std::io::Cursor::new(b"GET /x HTTP/1.1\r\nA: b\r\n\r\nBODYBYTES".to_vec());
    let head = read_head(&mut src, 4096).expect("io").expect("有头");
    assert_eq!(head.len(), "GET /x HTTP/1.1\r\nA: b\r\n\r\n".len());
    // ★ 这条是要害：读头**不许多吃一个字节**，否则请求体会缺头。
    let mut rest = Vec::new();
    std::io::Read::read_to_end(&mut src, &mut rest).expect("io");
    assert_eq!(rest, b"BODYBYTES");
}

/// 数「**真的**从源里读走了多少字节」的读源。
///
/// `read_head` 的两个 `Ok(None)` 出口**返回值本身分不开** ⇒ 想让「上限」那道门
/// 有自己的判据，只能量它**消耗了多少**。
struct CountingReader {
    inner: std::io::Cursor<Vec<u8>>,
    consumed: usize,
}

impl Read for CountingReader {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(b)?;
        self.consumed += n;
        Ok(n)
    }
}

/// ★ **上限那道门，单断**（回修轮之三，承接 D1 `重要-1`）。
///
/// `read_head` 有**两个** `Ok(None)` 出口 —— ①超上限 ②头没读完就 EOF ——
/// 而**返回值本身分不开它们**。先前那一条判据（`head_cap_is_enforced`）只断 `is_none()`，
/// 于是两个出口**互相兜底**：上限整个失效、读到 EOF 照样 `None` ⇒ **绿**（审计 `CE`）；
/// EOF 出口改成 `Some`、上限照样先拦住 ⇒ 也**绿**（审计 `CE2`）；**只有两刀同切才红**（`CE3`）。
/// ⇒ 它买到的是「目录级塌陷」，而名字说的是「enforced」〔`brief` 9：N 个独立源要 N 格单断〕。
///
/// 这一条改断**它到底读走了多少字节**：上限拦住 ⇒ 只该消耗 `CAP` 个，
/// **不是**把源里 9000 个全吃完。上限一旦失效，这个数当场对不上。
#[test]
fn the_cap_door_stops_the_read_at_exactly_cap_bytes() {
    const CAP: usize = 128;
    let mut src = CountingReader {
        inner: std::io::Cursor::new(vec![b'a'; 9000]),
        consumed: 0,
    };
    assert!(
        read_head(&mut src, CAP).expect("io").is_none(),
        "超上限的头不该被当成一个头返回"
    );
    assert_eq!(
        src.consumed, CAP,
        "上限拦住时只该消耗 {CAP} 字节；把源里 9000 字节全读完说明上限没生效"
    );
}

/// ★ **EOF 那道门，单断**。头**没有**空行就断流 ⇒ 不许当成一个头返回。
///
/// 源**短于**上限 ⇒ 上限那道门这一趟根本不会触发 ⇒ 能让它红的只有 EOF 这一道。
#[test]
fn the_eof_door_refuses_a_head_that_never_terminates() {
    const CAP: usize = 128;
    // 源：**短于** `CAP` 且没有空行 ⇒ 上限那道门这一趟根本不触发。
    const SRC: &[u8] = b"GET /x HTTP/1.1\r\nA: b\r\n";
    let mut src = CountingReader {
        inner: std::io::Cursor::new(SRC.to_vec()),
        consumed: 0,
    };
    assert!(
        read_head(&mut src, CAP).expect("io").is_none(),
        "没读到空行就断流的头不该被当成一个头返回"
    );
    // 非空对照：这一趟**真的**是一路读到源尽头才停的（撞 EOF），不是撞上限停的。
    //
    // ⚠ 订正〔回修轮之四 08-25，D2 `建议-1`〕：先前这里写的是 `assert!(src.consumed < CAP)`
    // —— 源是手写字面量 **23** 字节、`CAP` 是同一个函数里的手写常量 **128**
    // ⇒ **结构性恒真**，没有任何生产改动能让它红（`relay/` 里同族的第三处；
    //   前两处是 §8.15.8 自查逮到的 `自-1`/`自-2`）。**动机（做非空对照）是对的，
    //   写法永远不会失败** —— 一条永远不会红的断言不是对照，是装饰。
    // 今天改成断**恰好等于源长度**，两个方向都真会红：
    //   · 上限那道门要是提前拦住（例如 `while buf.len() < cap` 被改小）⇒ 消耗量 < 源长 ⇒ **红**；
    //   · 夹具哪天被写长过 `CAP` ⇒ 上限先触发、消耗量 = `CAP` ≠ 源长 ⇒ 也**红**
    //     （夹具漂移正是先前那一条想守的东西，今天它真守得住了）。
    assert_eq!(
        src.consumed,
        SRC.len(),
        "这一趟必须一路读到源尽头才停（源 {} 字节，上限 {CAP}）",
        SRC.len()
    );
}

/// **全断对照** —— 两道门**至少有一道**拦住了。
///
/// 它红 ⇒ 两道门**同时**坏了（审计 `CE3` 那一刀）。**单断哪一道它都不红**，
/// 这正是它替不了上面那两条的原因 ⇒ 留着只作对照，**名字也不再说「enforced」**。
#[test]
fn an_over_long_head_never_comes_back_as_a_head() {
    let mut src = std::io::Cursor::new(vec![b'a'; 9000]);
    assert!(read_head(&mut src, 128).expect("io").is_none());
    // 非空对照：`read_head` 在**正常**的头上真的会返回 `Some` —— 不然上面那句是空真。
    let mut ok = std::io::Cursor::new(b"GET /x HTTP/1.1\r\nA: b\r\n\r\n".to_vec());
    assert!(read_head(&mut ok, 128).expect("io").is_some());
}

#[test]
fn hop_by_hop_set_is_exactly_the_eight_we_named() {
    assert_eq!(HOP_BY_HOP.len(), 8, "改这张表要说明理由");
    assert!(is_hop_by_hop("Transfer-Encoding"));
    assert!(!is_hop_by_hop("Authorization"));
}

#[test]
fn chunked_view_decodes_across_arbitrary_split_points() {
    let wire = b"5\r\nhello\r\n5\r\nworld\r\n0\r\n\r\n";
    // 逐字节喂：拆帧必须对**任意切点**成立，否则「逐块透传」一进来就碎。
    let mut v = ChunkedView::default();
    let mut out = Vec::new();
    for b in wire.iter() {
        out.extend_from_slice(&v.feed(&[*b], NO_CAP));
    }
    assert_eq!(out, b"helloworld");
    // 一次性喂：同样的答案
    let mut v2 = ChunkedView::default();
    assert_eq!(v2.feed(wire, NO_CAP), b"helloworld");
}

/// ★ `TEE_DECODE_CAP` 在 `ChunkedView` 这一侧的那一格〔回修轮之五 08-25，`阻-1(D3)` 同职面〕。
///
/// 会无界涨的是**块长度行还没读到 `\r\n`** 那一支：上游发一条永不结束的长度行，
/// `buf` 就把收到的一切原样留着。分母 = 我列出的这 **2** 形（超上限丢+计数 · 没超一个字节不丢）。
#[test]
fn an_endless_chunk_size_line_is_dropped_and_counted_instead_of_growing_forever() {
    const CAP: usize = 64; // 手写字面量，不引生产常量

    // 非空对照：没超上限时，正常的 chunked 流一个字节都不丢。
    let mut ok = BodyView::Chunked(ChunkedView::default());
    assert_eq!(ok.feed(b"5\r\nhello\r\n0\r\n\r\n", CAP), b"hello");
    assert_eq!(ok.take_dropped(), 0, "正常的流不许丢");

    // 正题：一条永不结束的块长度行。
    let mut v = BodyView::Chunked(ChunkedView::default());
    let endless = vec![b'a'; CAP * 2];
    assert!(v.feed(&endless, CAP).is_empty(), "读不出块长度就不该吐东西");
    let dropped = v.take_dropped();
    assert!(
        dropped >= CAP as u64,
        "丢了 {dropped} 字节 —— 超上限的那截必须被丢掉**并计数**（不许静默）"
    );
    assert_eq!(v.take_dropped(), 0, "取走之后账要清零，不许重复报");
}

#[test]
fn body_view_picks_chunked_only_when_the_header_says_so() {
    let plain = vec![("Content-Type".to_string(), "text/event-stream".to_string())];
    assert!(matches!(BodyView::for_response(&plain), BodyView::Identity));
    let ch = vec![("Transfer-Encoding".to_string(), "chunked".to_string())];
    assert!(matches!(BodyView::for_response(&ch), BodyView::Chunked(_)));
}

#[test]
fn parses_a_response_head() {
    let raw = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n";
    let (status, headers) = parse_response(raw).expect("应当解析成功");
    assert_eq!(status, "HTTP/1.1 200 OK");
    assert_eq!(headers.len(), 1);
}
