//! 丢帧账（`src/frontend/shell/src/frame_tally.rs`）的判据。
//!
//! （逐字）「两个真实的静默口：`from_utf8_lossy` 在读路径上（非 UTF-8 字节静默变 U+FFFD）·
//! 坏帧/未知 kind → warn ＋ 跳过、永不中断流（设计如此，但「今天丢了多少帧」没有计数器；W5-VIS）」。
//!
//! 三件：① 账本本身（计数 · 2 的幂次才说 · 总账两向）② 三条读帧循环都接上了（按文件的调用处数 == 手写表，两向）
//! ③ 三条循环里「认不出就跳过」那一支不再有裸 `continue`（零命中，带正控）。

use super::*;

/// ① 计数与说话节奏：第 1、2、4、8 次说，其余不说；说的话带累计数与那一行的开头。
#[test]
fn w5vis_the_tally_counts_every_drop_and_speaks_on_powers_of_two() {
    let mut t = FrameTally::new("台架");
    let spoke: Vec<u64> = (1..=9u64)
        .filter(|_| t.note_unparsed("{\"kind\":\"future_thing\"}").is_some())
        .collect();
    assert_eq!(spoke, vec![1, 2, 4, 8], "说话的次序不是 2 的幂");
    let n = t.note_unparsed("x").map_or(String::new(), |s| s);
    assert!(n.is_empty(), "第 10 次不该说");
    let mut t2 = FrameTally::new("台架");
    let first = t2
        .note_unparsed("{\"kind\":\"future_thing\"}")
        .expect("第一次必须说");
    for must in ["台架", "累计 1 帧", "future_thing"] {
        assert!(first.contains(must), "第一次那句话里缺 `{must}`：{first}");
    }
    let bad = t2
        .note_bad_utf8(b"{\"k\":\"\xff\"}")
        .expect("第一行非 UTF-8 必须说");
    assert!(
        bad.contains("累计 1 行") && bad.contains('\u{FFFD}'),
        "{bad}"
    );
    // 开头截在字符边界上、不超过上限。
    let long = "错".repeat(400);
    let s = t2.note_unparsed(&long).expect("第 2 次要说");
    assert!(s.len() < long.len(), "那一行没有截：{} 字节", s.len());
    std::mem::forget(t); // 本条不看 Drop 那一行（下一条看总账本身）
    std::mem::forget(t2);
}

/// ① 总账两向：两样都是 0 ⇒ 不说；任一样非 0 ⇒ 说，且两个数 == 真记下的数。
#[test]
fn w5vis_the_summary_is_silent_only_when_nothing_was_dropped() {
    let quiet = FrameTally::new("台架");
    assert_eq!(quiet.summary(), None);
    let mut t = FrameTally::new("台架");
    for _ in 0..3 {
        let _ = t.note_unparsed("?");
    }
    let _ = t.note_bad_utf8(b"\xfe");
    let s = t.summary().expect("丢过帧却不出总账");
    assert!(
        s.contains("跳过了 3 帧") && s.contains("1 行含非 UTF-8"),
        "{s}"
    );
    let mut only_utf8 = FrameTally::new("台架");
    let _ = only_utf8.note_bad_utf8(b"\xfe");
    assert!(only_utf8.summary().is_some(), "只丢了非 UTF-8 行也要出总账");
    std::mem::forget(t);
    std::mem::forget(only_utf8);
}

/// 三条读帧循环各自接了几处：`(文件, FrameTally::new 处数, 认不出的那一支（`unread.take(`）处数, note_bad_utf8 处数)`。
///
/// - `stream_source/`：读任务一本（只数非 UTF-8 —— 它按替换字符读）· 主循环一本（只数认不出的）；
/// - `local_backend_host.rs`：脱离载体那条循环一本，两样都数（非 UTF-8 行在那里整行丢）；
/// - `local_backend.rs`：stdio 载体那条循环一本，两样都数。
/// 认不出的那一支一律经 `UnreadNotes::take`（`stream_source/frame.rs`），记账（`note_unparsed`）只住那一处。
const WIRED: &[(&str, usize, usize, usize)] = &[
    ("src/frontend/shell/src/stream_source/run.rs", 2, 1, 1),
    ("src/frontend/shell/src/stream_source/frame.rs", 0, 0, 0),
    ("src/frontend/shell/src/local_backend_host.rs", 1, 1, 1),
    ("src/frontend/shell/src/local_backend.rs", 1, 1, 1),
];

/// ② 接线：盘上真有接账本的文件集合 == [`WIRED`]，且每份的三个处数相等（两向：多接 / 少接 / 挪走都红）。
/// 人群从生产源码派生（`src/frontend/shell/src` 整棵，剥测试段与注释），`frame_tally.rs` 自己不算。
#[test]
fn w5vis_every_frame_reader_keeps_the_tally_and_nothing_else_pretends_to() {
    let root = crate::guard_support::repo_root();
    let mut seen: Vec<(String, usize, usize, usize)> = Vec::new();
    for (path, raw) in guard_core::scan_tree_excluding(
        &root.join("src/frontend/shell/src"),
        &["rs"],
        &["src/frontend/shell/src/frame_tally.rs"],
    ) {
        let prod = guard_core::production_code(&raw);
        let counts = (
            prod.matches("FrameTally::new(").count(),
            prod.matches("unread.take(").count(),
            prod.matches(".note_bad_utf8(").count(),
        );
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        // 记账那一句只许住 `UnreadNotes::take` 里（frame.rs 恰好一处，别处零处）。
        let is_frame_rs = rel == "src/frontend/shell/src/stream_source/frame.rs";
        assert_eq!(
            prod.matches(".note_unparsed(").count(),
            usize::from(is_frame_rs),
            "{rel}：`.note_unparsed(` 处数不对 —— 认不出的那一支要经 `UnreadNotes::take`"
        );
        if counts != (0, 0, 0) || is_frame_rs {
            seen.push((rel, counts.0, counts.1, counts.2));
        }
    }
    seen.sort();
    let mut want: Vec<(String, usize, usize, usize)> = WIRED
        .iter()
        .map(|(f, a, b, c)| (f.to_string(), *a, *b, *c))
        .collect();
    want.sort();
    assert_eq!(seen, want, "接丢帧账的地方与登记表对不上");
}

/// ③ 三条循环里「认不出 / 不是 UTF-8 就跳过」那几支都先记账再跳（块里没有记账调用 ⇒ 红）；
/// 远端主循环那句逐帧 `warn`（`skipping unparseable/unknown frame`）零命中、记账那一句恰好一处。
/// 正控：旧形（裸 `continue`）必须被认出。
#[test]
fn w5vis_no_frame_reader_skips_an_unparsed_frame_silently() {
    /// `(文件里的哪一支（头，恰好一处）, 块里必须有的记账调用)`。块 = 头之后到第一个 `continue;`。
    const SKIPS: &[(&str, &str, &str)] = &[(
        "local_backend_host.rs",
        "let Ok(line) = std::str::from_utf8(&buf) else {",
        "tally.note_bad_utf8(",
    )];
    /// 认不出的那一支：头本身就是记账（`UnreadNotes::take` 里记账、每种说一次），块里只剩 `continue;`。
    const TAKES: &[(&str, &str)] = &[
        (
            "local_backend_host.rs",
            "let Some(f) = unread.take(line, &mut tally, &crate::stream_source::local_health)",
        ),
        (
            "local_backend.rs",
            "let Some(frame) = unread.take(&line, &mut tally, &crate::stream_source::local_health)",
        ),
    ];
    fn block_after<'a>(prod: &'a str, head: &str) -> Result<&'a str, String> {
        let at = guard_core::find_pinned(prod, head)?;
        let tail = &prod[at + head.len()..];
        // 块 = 头之后到**第一个** `continue;`（那一支就是以它收尾的）。
        let (block, _) = tail
            .split_once("continue;")
            .ok_or("那一支后面没有 `continue;`")?;
        Ok(block)
    }
    let host = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    let stdio = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend.rs"
    ));
    let ssh = crate::guard_support::stream_source_production();
    for (f, head, must) in SKIPS {
        let prod = if *f == "local_backend.rs" {
            &stdio
        } else {
            &host
        };
        let block = block_after(prod, head).unwrap_or_else(|e| panic!("{f}：{e}"));
        assert!(
            guard_core::find_pinned(block, must).is_ok(),
            "{f}：`{head}` 那一支跳过之前没记账（找不到 `{must}`）：{block}"
        );
    }
    for (f, head) in TAKES {
        let prod = if *f == "local_backend.rs" {
            &stdio
        } else {
            &host
        };
        guard_core::find_pinned(prod, head)
            .unwrap_or_else(|e| panic!("{f}：认不出的那一支不是恰好一处经 `unread.take`：{e}"));
    }
    guard_core::find_pinned(&ssh, "tally.note_unparsed(line)")
        .unwrap_or_else(|e| panic!("stream_source 主循环记账那一句不是恰好一处：{e}"));
    assert!(
        guard_core::find_pinned(&ssh, "skipping unparseable/unknown frame").is_err()
            && !guard_core::contains_word(&ssh, "skipping"),
        "stream_source 里逐帧 warn 那一句还在 —— 又回到一帧一行、从不计数"
    );
    // 正控：旧形必须被认出（量具没瞎）。
    let old = "let Some(f) = crate::stream_source::parse_frame(line) else {\n    continue;\n};\n";
    assert!(
        guard_core::find_pinned(old, TAKES[0].1).is_err(),
        "旧形（裸 continue）没被认出 —— 量具瞎了"
    );
}
