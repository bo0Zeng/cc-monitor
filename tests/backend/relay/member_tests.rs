//! 中转成员的单测里要借后端的那几格：内存探针（`alloc_probe`）量 `http1` 拒收不分配 ·
//! 上游选择拼给起会话那一发的 `/t/` 地址经 `route` 解析、再交给生产段那张决策表。
//! 纯中转的那一半住中转 crate 自己的 `http1_tests.rs` / `route_tests.rs`。

use comms_outward::test_support::http1::read_exact_body;
use comms_outward::test_support::route::parse;
use comms_outward::{Destination, Mode, RouteKey};

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

/// ★★★ **上游选择拼给起会话那一发的 `/t/` 地址**（`accounts::upstream_select::endpoint::relay_with`，
/// 路由语法住共享 crate `relay_route_core`）本解析器读成**直通模式**、各段各落各位；再交给**生产段那张决策表**
/// （`accounts::upstream_select::decide`）：那一家（登记过）⇒ 发到它自己的默认上游；同一条路由把第 1 段换成 `codex`（未登记，手写）⇒ 拒（404 ＋ 原因头，FIX3 之前是 502）。
///
/// ⇒ 「注入的那一形，中转真的会照直通处理」这一截从成品到决策表一路是真的。先前这里是三条跨半边对拍
/// （monitor `payload.rs` 的两份样例 · `APIKEY_TABLE_AGENT` · `AGENTS_WITH_DEFAULT_UPSTREAM`〔散文墓碑〕 现抠字面量），
/// 拼的那一侧搬进后端、语法进共享 crate 之后，两半之间没有第二份可对拍了（`cross_half_edge_registry` 那条边随之出列）。
/// 买不到的那一截（claude 拿到这个变量之后怎么走）同今天（`C7`）。
#[test]
fn the_passthrough_url_the_launch_answer_builds_parses_as_passthrough() {
    let answer = crate::accounts::upstream_select::endpoint::relay_with(
        "claude-code",
        &crate::accounts::upstream_select::endpoint::LaunchAccount::Named {
            config_dir: "/h/.claude-alt/acct-a".to_string(),
        },
        true,
        relay_route_core::PORT,
        &[],
        &|_, _| true,
    )
    .expect("成品");
    let url = answer.as_deref().expect("开关开、没行 ⇒ 该注入 `/t/`");
    let sample = url
        .strip_prefix(&format!("http://127.0.0.1:{}", relay_route_core::PORT))
        .expect("注入的不是回环那个口");
    assert!(sample.starts_with("/t/"), "不像直通路由键：{sample:?}");
    let r = parse(&format!("{sample}/v1/messages")).expect("monitor 拼的 `/t/` 那一形解析不了");
    // 期望值全是手写字面量。
    assert_eq!(r.mode, Mode::Passthrough, "`/t/` 没被读成直通");
    assert_eq!(r.key.seg1, "claude-code");
    assert_eq!(r.key.seg2, "acct-a");
    assert_eq!(r.rest, "/v1/messages");

    // 交给生产段那张决策表（空表 = 这个号在 apikey 表里没有行，即订阅号）。
    let table = crate::accounts::upstream_select::table::RoutingTable::build(std::iter::empty());
    let ups = crate::accounts::upstream_select::Upstreams::from_env(&|_| None).expect("内置默认");
    let said = |k: &RouteKey| {
        let mut out = String::new();
        crate::accounts::upstream_select::decide(&table, &ups, r.mode, k, &mut |d| {
            out = match d {
                Destination::Passthrough { upstream, .. } => {
                    format!("pass {}", upstream.host)
                }
                Destination::Refuse { status, reason, .. } => {
                    format!("refuse {status} {reason}")
                }
                Destination::Substitute { .. } => "substitute".to_string(),
            }
        });
        out
    };
    assert_eq!(
        said(&r.key),
        "pass api.anthropic.com",
        "登记过的那家没被直通到它自己的默认上游"
    );
    let codex = RouteKey {
        seg1: "codex".to_string(),
        seg2: r.key.seg2.clone(),
    };
    assert_eq!(
        said(&codex),
        "refuse 404 Not Found agent-not-registered",
        "🔴 codex 走 `/t/` 没被拒 ⇒ 它的请求会被发到别家的上游"
    );
}
