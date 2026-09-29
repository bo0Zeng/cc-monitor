//! `relay/framer.rs::LineFramer` 的判据〔SC1 · `设计/17 §3.7`〕。
//!
//! 全部是**次数 / 长度的相等**，不看墙钟：墙钟随机器与负载变，而「每个字节被找几次、被搬几次」
//! 是算法的形状本身。期望值一律按**输入的形状**手算（`6k + 3` · `7` · `0` · 判据自己数的喂入量），
//! 不从被测代码里取 —— 账本只在被测那一侧，判据这一侧是另一份算术。
//!
//! 设计与读数住 `调研/第四波记录/SC1.md`。

use super::{Ledger, LineFramer};
use crate::relay::http1::{BodyView, ChunkedView};
use crate::relay::tee::SseSplitter;

/// 「这条判据不测上限那一格」：一个永远触发不了的上限（期望值不许拿被测常量算）。
const NO_CAP: usize = usize::MAX;

/// 一条 617 KiB 的 `data:` 行 —— `设计/17 §1.1` 里那条最大记录的量级。
const GIANT: usize = 617 * 1024;

/// 按 `step` 字节一块切（最后一块可短）。`step == 0` 表示整段一次。
fn pieces(wire: &[u8], step: usize) -> Vec<&[u8]> {
    if step == 0 {
        return vec![wire];
    }
    wire.chunks(step).collect()
}

fn giant_line() -> Vec<u8> {
    let mut v = b"data: ".to_vec();
    v.extend(std::iter::repeat_n(b'x', GIANT));
    v.push(b'\n');
    v
}

fn small_lines(n: usize) -> Vec<u8> {
    let mut v = Vec::new();
    for i in 0..n {
        v.extend_from_slice(format!("data: [{i}]\n").as_bytes());
    }
    v
}

// ════════════════════════════════════════════════════════════════════════════
//  F1 · SSE：每个字节恰好被找一次；每次喂入都收在行尾时一个字节都不搬
// ════════════════════════════════════════════════════════════════════════════

/// ★ F1 —— `17 §3.7` 点名的那一形：一条巨记录卡在缓冲里，按 16 KiB 一块陆续到，完了再跟 1 000 条小行。
///
/// 先前的 `SseSplitter`：每块都从 `partial` 开头重找 `\n`（617 KiB / 16 KiB ≈ 39 块 ⇒ 扫 ≈ 39²/2 × 16 KiB ≈ 12 MiB），
/// 巨行吐出之后每切一条小行又 `drain` 一次前缀。今天 `examined == pushed`、`moved == 0`。
#[test]
fn f1_a_giant_sse_line_arriving_in_pieces_is_scanned_exactly_once_and_never_moved() {
    let giant = giant_line();
    let tail = small_lines(1000);
    let mut s = SseSplitter::default();
    let mut fed = 0u64;
    let mut events = Vec::new();
    // 巨行按 16 KiB 一块喂（最后一块恰好收在它的 `\n` 上）…
    for p in pieces(&giant, 16 * 1024) {
        fed += p.len() as u64;
        events.extend(s.feed(p, NO_CAP));
    }
    // …然后 1 000 条小行一次喂进来。
    fed += tail.len() as u64;
    events.extend(s.feed(&tail, NO_CAP));

    assert_eq!(events.len(), 1001, "巨行 1 条 ＋ 小行 1 000 条");
    assert_eq!(events[0].len(), GIANT, "巨行的载荷一个字节都不许少");
    assert_eq!(events[1000], "[999]");
    assert_eq!(
        s.framer.ledger,
        Ledger {
            pushed: fed,
            examined: fed,
            moved: 0,
        },
        "喂了 {fed} 字节：每个字节必须**恰好**被找一次（`examined == pushed`），\
         且每次喂入前上一轮都收在行尾 ⇒ 一个字节都不该被搬（`moved == 0`）"
    );
}

/// F1 的切法不变量：逐字节 · 整段一次 · 质数 4 099 字节一块 —— `examined == pushed` 三种切法同一条等式。
///
/// ⚠ 这里的长行刻意只有 16 KiB，**不是** 617 KiB：死值验把「每次喂入都从头找」那一形放回去时，
/// 逐字节喂一条 617 KiB 的行是 ~2×10¹¹ 次比较 —— 判据不是红，是**挂住十几分钟**
/// （现打：那一刀在 debug 构建上跑了 683 秒被手动杀掉）。挂住的判据在 CI 上等于没有判据。
/// 617 KiB 那一形由上面 F1 那条按 16 KiB 一块喂（二次形状下也只有 ~2.5×10⁷，照样当场红）。
#[test]
fn f1_examined_equals_pushed_for_every_way_of_cutting_the_stream() {
    let mut wire = small_lines(50);
    wire.extend_from_slice(b"data: ");
    wire.extend(std::iter::repeat_n(b'y', 16 * 1024));
    wire.push(b'\n');
    wire.extend(small_lines(50));
    wire.extend_from_slice(b"data: [half"); // 末尾留半行：它也要被找过一次
    for step in [1usize, 0, 4099] {
        let mut s = SseSplitter::default();
        let mut n = 0usize;
        for p in pieces(&wire, step) {
            n += s.feed(p, NO_CAP).len();
        }
        assert_eq!(n, 101, "切法 {step}：事件数");
        let l = s.framer.ledger;
        assert_eq!(
            l.pushed,
            wire.len() as u64,
            "切法 {step}：账本自己数的喂入量与判据数的不一致"
        );
        assert_eq!(
            l.examined, l.pushed,
            "切法 {step}：找分隔符看过 {} 字节，喂了 {} 字节 —— 有字节被重找了（或漏找了）",
            l.examined, l.pushed
        );
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  F2 · 搬运的两向：有残余时恰好搬残余一次；没有残余时零
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn f2_the_leftover_after_the_last_line_is_moved_exactly_once() {
    // 有残余：第一口吐出 `data: a`，留下 `data: b`（7 字节）；第二口进来之前恰好把那 7 个字节挪到头上。
    let mut s = SseSplitter::default();
    assert_eq!(s.feed(b"data: a\ndata: b", NO_CAP), vec!["a".to_string()]);
    assert_eq!(s.feed(b"c\n", NO_CAP), vec!["bc".to_string()]);
    assert_eq!(
        s.framer.ledger.moved, 7,
        "残余 `data: b` 是 7 个字节，搬一次"
    );

    // 没有残余：逐行喂 ⇒ 每一口都收在行尾 ⇒ 零搬运（非空对照：上面那格的 7 不是「见谁都搬」）。
    let mut s = SseSplitter::default();
    for line in [&b"data: a\n"[..], b"data: b\n", b"data: c\n"] {
        s.feed(line, NO_CAP);
    }
    assert_eq!(s.framer.ledger.moved, 0, "每口都收在行尾，不该搬任何字节");
}

// ════════════════════════════════════════════════════════════════════════════
//  F3 · chunked：只有块长度行被找；数据段与块尾一个字节都不找
// ════════════════════════════════════════════════════════════════════════════

const CHUNK_DATA: usize = 0x4000; // 16 KiB —— 块长度行恰好是 "4000\r\n"，6 字节
const CHUNKS: usize = 40;

/// 造一段 chunked 正文，并记下每条块长度行在串里的 `[起, 止)`（判据自己记，异源于被测）。
fn chunked_body() -> (Vec<u8>, Vec<u8>, Vec<(usize, usize)>) {
    let mut wire = Vec::new();
    let mut data = Vec::new();
    let mut size_lines = Vec::new();
    for i in 0..CHUNKS {
        let s = wire.len();
        wire.extend_from_slice(b"4000\r\n");
        size_lines.push((s, wire.len()));
        let block: Vec<u8> = (0..CHUNK_DATA)
            .map(|j| b"abcdefgh\r\n"[(i + j) % 10])
            .collect();
        wire.extend_from_slice(&block);
        data.extend_from_slice(&block);
        wire.extend_from_slice(b"\r\n");
    }
    let s = wire.len();
    wire.extend_from_slice(b"0\r\n");
    size_lines.push((s, wire.len()));
    (wire, data, size_lines)
}

/// 走生产那条口（`BodyView::feed`，`server.rs::handle` 调的就是它）解一整段，带回分帧器的账。
fn decode_chunked(wire: &[u8], step: usize) -> (Vec<u8>, Ledger) {
    let mut view = BodyView::Chunked(ChunkedView::default());
    let mut out = Vec::new();
    for p in pieces(wire, step) {
        out.extend(view.feed(p, NO_CAP));
    }
    let BodyView::Chunked(v) = &view else {
        unreachable!()
    };
    (out, v.framer.ledger)
}

/// ★ F3 —— 数据里故意掺着 `\r\n`：它们在数据段里，**不许**被当成分隔符去找。
#[test]
fn f3_only_chunk_size_lines_are_scanned_whatever_the_read_size() {
    let (wire, data, _) = chunked_body();
    for step in [1usize, 0, 64 * 1024, 4099] {
        let (out, ledger) = decode_chunked(&wire, step);
        assert_eq!(out, data, "切法 {step}：解出来的字节必须 == 各块数据拼接");
        assert_eq!(
            ledger.examined,
            (6 * CHUNKS + 3) as u64,
            "切法 {step}：只有 {CHUNKS} 条 `4000\\r\\n` 与一条 `0\\r\\n` 该被找（6k + 3）"
        );
    }
}

/// ★ F3 的搬运那一格：每次喂入前被搬的只可能是**被 `read` 边界切开的那半条块长度行**，
/// 而且只在「这半条之前，上一口里有东西被消费了」时才搬（否则它本来就在头上）。
/// ⇒ `moved` == Σ（边界 − 那条块长度行的起点），条件是上一条边界落在这条块长度行起点之前 ——
/// 由判据按布局自己算（异源），四种 `read` 长度各一遍。
#[test]
fn f3_a_read_boundary_moves_only_the_half_size_line_it_cuts() {
    let (wire, data, size_lines) = chunked_body();
    let mut some_step_moves = false;
    for step in [64 * 1024usize, 4099, 3, 1] {
        let mut expected_moved = 0u64;
        let mut b = step;
        while b < wire.len() {
            if let Some((s, _)) = size_lines.iter().find(|(s, e)| *s < b && b < *e) {
                if b - step < *s {
                    expected_moved += (b - s) as u64;
                }
            }
            b += step;
        }
        some_step_moves |= expected_moved > 0;
        let (out, ledger) = decode_chunked(&wire, step);
        assert_eq!(out, data, "切法 {step}");
        assert_eq!(
            ledger.moved, expected_moved,
            "切法 {step}：先前每块三次前缀 drain、每次搬剩下的全部；\
             今天只该搬被 `read` 边界切开、且前面有东西被消费掉的那半条块长度行"
        );
    }
    assert!(
        some_step_moves,
        "四种切法没有一种让边界切进块长度行 —— 本格在空转"
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  F4 · CRLF：孤立的 `\n` 不算；`\r` | `\n` 跨两次喂入照样认；每个字节仍只找一次
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn f4_crlf_mode_skips_a_bare_lf_and_joins_a_split_crlf() {
    let mut f = LineFramer::new(b"\r\n");
    f.push(b"ab\ncd\r");
    assert!(
        f.next_line().is_none(),
        "只有孤立的 \\n 与一个悬着的 \\r，不成行"
    );
    f.push(b"\nef");
    assert_eq!(f.next_line(), Some(&b"ab\ncd"[..]));
    assert!(f.next_line().is_none());
    assert_eq!(f.pending(), 2, "剩 `ef`");
    assert_eq!(
        f.ledger,
        Ledger {
            pushed: 9,
            examined: 9,
            moved: 0,
        },
        "`ab\\ncd\\r` 找一遍（6），`\\nef` 找一遍（3）—— 没有一个字节被找两次"
    );
}

/// F4 在调用点上的样子：块长度行里一个孤立的 `\n` **不**结束这一行 —— 与收口之前
/// `windows(2)` 找 `\r\n` 的语义逐字节相同（那一行读成 `4\nabcd`，解不出十六进制 ⇒ 收工、零输出）。
#[test]
fn f4_a_bare_lf_never_ends_a_chunk_size_line() {
    let mut view = BodyView::Chunked(ChunkedView::default());
    assert!(view.feed(b"4\nabcd\r\n0\r\n", NO_CAP).is_empty());
    // 非空对照：同样的数据写成合法的 CRLF 块，照常解出来。
    let mut view = BodyView::Chunked(ChunkedView::default());
    assert_eq!(view.feed(b"4\r\nabcd\r\n0\r\n", NO_CAP), b"abcd".to_vec());
}

// ════════════════════════════════════════════════════════════════════════════
//  F5 / F6 · 分帧只有一个实现，两个调用点是它的客户
// ════════════════════════════════════════════════════════════════════════════

/// `relay/` 生产段（逐份过 `production_code`，再过共享的剥注释原语 —— 散文里提到 `drain` 不算手抄）。
fn relay_production() -> Vec<(String, String)> {
    let root = crate::guard_support::relay_root(); // 〔RE〕`relay` 模块的根
    guard_core::scan_tree_excluding(&root, &["rs"], &[])
        .into_iter()
        .map(|(p, raw)| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let prod = crate::guard_support::production_code(&raw);
            (name, guard_core::strip_comment_lines(&prod))
        })
        .collect()
}

/// 手抄分帧的三种形状。**运行时拼**：本文件不在被扫的树里，但别让这张表的字面量去任何地方当命中源。
fn hand_framing_shapes() -> [String; 3] {
    [
        format!(".{}(..", "drain"),
        format!(".{}(", "windows"),
        format!("b'\\{}'", "n"),
    ]
}

/// ★ F5 —— 零命中（`framer.rs` 之外）＋ 正控（`framer.rs` 里那一次 `drain` 恰好一处）。
#[test]
fn f5_relay_has_exactly_one_framer() {
    let files = relay_production();
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    for must in ["framer.rs", "tee.rs", "http1.rs"] {
        assert!(
            names.contains(&must),
            "扫描面里没有 {must} —— 取法坏了，下面的零命中在空转"
        );
    }
    let shapes = hand_framing_shapes();
    let mut hits = Vec::new();
    for (name, prod) in &files {
        if name == "framer.rs" {
            continue;
        }
        for (no, line) in prod.lines().enumerate() {
            for s in &shapes {
                if line.contains(s.as_str()) {
                    hits.push(format!("{name}（生产段第 {} 行）: {}", no + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "`relay/` 里又长出了手抄分帧（`设计/17 §3.7`：该用 `framer.rs::LineFramer`）：{hits:#?}"
    );
    // 正控：那一次（也是唯一一次）前缀搬运住在分帧器里 —— 本条的针认得出它。
    let framer = &files.iter().find(|(n, _)| n == "framer.rs").unwrap().1;
    guard_core::find_pinned(framer, &shapes[0])
        .unwrap_or_else(|e| panic!("正控：`framer.rs` 里的前缀搬运应恰好一处：{e}"));
    assert!(
        framer.contains(shapes[2].as_str()),
        "正控：`framer.rs` 找分隔符用的就是那个字节字面量"
    );
}

/// F6 —— 两个调用点各恰好一处 `LineFramer::new(<分隔符>)`，分隔符各是各的
/// （SSE 按 `\n` 切、chunked 块长度行按 `\r\n` 切；分隔符换错了，合法输入照样解得对 ——
/// 只在坏输入上露馅，见 F4 的 `f4_a_bare_lf_never_ends_a_chunk_size_line`）。
#[test]
fn f6_both_call_sites_are_clients_of_the_one_framer() {
    let files = relay_production();
    let ctor = format!("{}::new(", "LineFramer");
    for (site, delim) in [("tee.rs", r#"b"\n")"#), ("http1.rs", r#"b"\r\n")"#)] {
        let prod = &files.iter().find(|(n, _)| n == site).unwrap().1;
        guard_core::find_pinned(prod, &ctor)
            .unwrap_or_else(|e| panic!("`{site}` 应恰好一处用分帧器：{e}"));
        let needle = format!("{ctor}{delim}");
        guard_core::find_pinned(prod, &needle)
            .unwrap_or_else(|e| panic!("`{site}` 的分帧器分隔符应是 `{delim}`：{e}"));
    }
}
