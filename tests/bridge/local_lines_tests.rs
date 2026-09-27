//! 〔CF1 · 第四波 4B · 2026-09-24〕**本机会话内容走本机后端的 Line 帧** 的判据。
//!
//! 守的要求：`设计/00 §2.5 ②`「本机改走后端的 Line 帧，消掉 monitor 里第二套 watcher 与第二套 offsets/seq」·
//! `设计/01 §6.1`「一条流，一个来源；本机与远端走同一条帧路」· `设计/05 §3.3.4`（级 3 无界堆积是禁止态）。
//! 设计与读数住 `调研/第四波记录/CF1.md`。
//!
//! | # | 判什么 | 异源 / 两向 |
//! |---|---|---|
//! | F1 | 本机吸收点交回的帧种类 == {line, session_added, session_removed} ∪〔LOC1b〕{session_status, sessions_replayed} ∪〔FW1〕{session_file_gone, session_file_reread}；喂的种类 == `parse_frame` 认得的全部种类 | 帧是手写线上 JSON；种类全集从 `parse_frame` 源码里摘（两向） |
//! | F2 | 本机消费者的纯分派核真值表 | 期望手写 |
//! | F3 | 两条读循环各恰好一处把交回的帧送进本机内容通道（送法各按载体）、各恰好一处送「流结束」 | 源码锚，恰好一处 |
//! | F4 | 内容出口的调用方集合（`batch_to_payloads` / `on_line_batch_awaited` / `flush_lines` / `LineIntake::open` / `Batcher::new` / `SnapshotQueue::new`） | 全仓生产段扫描，两向集合相等 |
//! | F5 | 两条载体的起参是同一份常量，且其中每一个旗标都 ∈ 后端 `STREAM_FLAGS`；`--tail-only` 在 ⟺ 消费者认定 tail-only | 后端源码 —— 住 `ssh_source_stream_flag_gate_tests.rs`（同一条跨半边已登记，不另开一条） |
//! | F6 | 快照被撤时的补偿归档：本机不补、远端补 | — |
//! | F7 | monitor 生产段里那套 watcher 的名字零命中（带正控） | — |
//! | L1 | 〔LOC1b〕本机起停帧 ⇒ 本机活会话表的起停事实（藏起来的 bg 不进；流断带上可重连那一摞） | 期望手写 |
//! | F8 | 真后端 × 生产 stdio 读循环 ⇒ 宣告带 `path`/`lines`、新行的 `seq` == 行号（`#[ignore]`，由 `tests/evidence/CF1-local-lines.py` 带二进制跑） | 两侧各是真实现 |

use super::*;
use crate::ssh_source::{local_step, parse_frame, InboundFrame, LocalItem, LocalStep};
use std::collections::{BTreeSet, HashSet};

// ─── 小工具：函数范围（rustfmt 保证收尾 `}` 与签名同缩进）───────────────────────────

fn fn_end(lines: &[&str], start: usize) -> usize {
    let indent: String = lines[start].chars().take_while(|c| *c == ' ').collect();
    let close = format!("{indent}}}");
    (start..lines.len())
        .find(|&k| lines[k] == close)
        .unwrap_or(lines.len() - 1)
}

/// 某一行所在的函数名（往回找 `fn <名>`，且它的体要包住这一行）。
fn enclosing_fn(lines: &[&str], at: usize) -> Option<String> {
    for i in (0..=at).rev() {
        let l = lines[i];
        let rest = l
            .split(" fn ")
            .nth(1)
            .or_else(|| l.trim_start().strip_prefix("fn "));
        if let Some(rest) = rest {
            let n: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !n.is_empty() && fn_end(lines, i) >= at {
                return Some(n);
            }
        }
    }
    None
}

/// 调用形 `needle`（如 `"flush_lines("`）在 monitor 全仓生产段里的 `(文件, 外层 fn)` 集合。
/// 定义处（`fn flush_lines(`）与更长名字的尾巴（`xflush_lines(`）不算。
fn callers_of(needle: &str) -> BTreeSet<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = BTreeSet::new();
    for (path, raw) in guard_core::scan_tree!(&root, &["rs"]) {
        let prod = guard_core::production_code(&raw);
        let file = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let lines: Vec<&str> = prod.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let hit = l.match_indices(needle).any(|(k, _)| {
                let before = &l[..k];
                let prev = before.chars().next_back();
                !before.ends_with("fn ") && !prev.is_some_and(|c| c.is_alphanumeric() || c == '_')
            });
            if hit {
                let f = enclosing_fn(&lines, i)
                    .unwrap_or_else(|| panic!("{file} 里的 `{needle}` 不在任何 fn 里 —— 抽取坏了"));
                out.insert((file.clone(), f));
            }
        }
    }
    out
}

fn set(xs: &[(&str, &str)]) -> BTreeSet<(String, String)> {
    xs.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

/// 造通道但**不起消费者**，把收端交给测试（F8 要直接看送进来的东西）。
/// 与生产的 `install` 共用同一个 `OnceLock` ⇒ 一个测试进程里只许调一次（第二次回 `None`）。
/// ⚠ 住测试文件里（本模块是 `local_lines` 的子模块，看得见那个私有 `SENDER`）：生产树里的
/// `#[cfg(test)]` 支撑项有递减棘轮（`structural_scan_tests` 的 P9 那一条）。
fn tap() -> Option<tokio::sync::mpsc::Receiver<LocalItem>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<LocalItem>(LOCAL_LINES_CAPACITY);
    SENDER.set(tx).ok()?;
    Some(rx)
}

// ─── F1 ────────────────────────────────────────────────────────────────────

/// `parse_frame` 认得的全部 `kind`：从它的源码里摘（`"xxx" =>` 那几条臂）。
fn parse_frame_kinds() -> BTreeSet<String> {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    let lines: Vec<&str> = prod.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.starts_with("pub fn parse_frame("))
        .expect("ssh_source 生产段里找不到 `pub fn parse_frame(` —— 抽取面画错了");
    let end = fn_end(&lines, start);
    let mut kinds = BTreeSet::new();
    for l in &lines[start..=end] {
        let t = l.trim_start();
        if let Some(rest) = t.strip_prefix('"') {
            if let Some((k, tail)) = rest.split_once('"') {
                if tail.trim_start().starts_with("=>") {
                    kinds.insert(k.to_string());
                }
            }
        }
    }
    kinds
}

/// 每一种帧一行手写线上 JSON（**不从实现生成**）。
///
/// ⚠ 两种不在表里、理由各写清：
/// - `turn_end`：`parse_frame` 对它回 `None`（本就不进任何吸收点）；
/// - `tmux_sessions`：它的吸收是写**进程级**的 tmux 原文账本（`<local>` 那一格），这里喂它会跟
///   `local_backend_tests` 里钉那一跳的判据抢同一格全局状态；它的吸收由那条判据管。
///
/// ⇒ 表的种类 ＋ 这两种 == `parse_frame` 的全部臂（两向），新长一种帧就红：先答它是不是内容。
const FRAMES: &[(&str, &str)] = &[
    (
        "hello",
        r#"{"kind":"hello","v":1,"build_id":"x","host_arch":"x86_64","claude_dir":"/h/.claude"}"#,
    ),
    (
        "line",
        r#"{"kind":"line","session_id":"s1","path":"/h/.claude/projects/p/s1.jsonl","seq":7,"raw":"{}"}"#,
    ),
    (
        "session_added",
        r#"{"kind":"session_added","sid":"s1","path":"/h/.claude/projects/p/s1.jsonl","lines":7}"#,
    ),
    (
        "session_status",
        r#"{"kind":"session_status","sid":"s1","status":"idle"}"#,
    ),
    (
        "session_removed",
        r#"{"kind":"session_removed","sid":"s1"}"#,
    ),
    ("overflow", r#"{"kind":"overflow","dropped":3}"#),
    (
        "tmux_session_closed",
        r#"{"kind":"tmux_session_closed","name":"cf1-x"}"#,
    ),
    (
        "reply",
        r#"{"kind":"reply","id":"cf1-no-such-id","ok":true}"#,
    ),
    ("cancelled", r#"{"kind":"cancelled","id":"cf1-no-such-id"}"#),
    ("accounts_changed", r#"{"kind":"accounts_changed"}"#),
    (
        "link_data",
        r#"{"kind":"link_data","link":"cf1-no-such-link","data":"aGk="}"#,
    ),
    (
        "link_end",
        r#"{"kind":"link_end","link":"cf1-no-such-link"}"#,
    ),
    // 〔合并主线 8f9263c3〕U4b 的「A 的清单报完了」与 SR1b 的传输进度 —— 都不是会话内容。
    //   〔LOC1b · 4D〕「清单报完了」从此交回（本机活会话表要它，且要排在它前面那些宣告之后）；传输进度仍就地吸收。
    ("sessions_replayed", r#"{"kind":"sessions_replayed"}"#),
    // 〔FW1 · 第四波 4D · D-d〕记录文件不见了 / 被改过 —— **是**会话内容那一族（与行同序进内容通道）。
    (
        "session_file_gone",
        r#"{"kind":"session_file_gone","session_id":"s1","path":"/h/.claude/projects/p/s1.jsonl"}"#,
    ),
    (
        "session_file_reread",
        r#"{"kind":"session_file_reread","session_id":"s1","path":"/h/.claude/projects/p/s1.jsonl","why":"rewritten"}"#,
    ),
    (
        "transfer",
        r#"{"kind":"transfer","id":"cf1-no-such-ticket","got":1,"total":2}"#,
    ),
    // 〔TAP · V124〕中转抄出来的 SSE 事件 —— 不是会话内容（jsonl 才是），就地转给前端，不进内容通道。
    (
        "tap",
        r#"{"kind":"tap","stream":"s1","resp":0,"n":0,"data":"{}"}"#,
    ),
];

#[test]
fn the_absorb_point_hands_back_exactly_the_content_and_lifecycle_frames() {
    // 两向：表里的种类 ＋ 两种刻意不喂的 == parse_frame 的全部臂。
    let mut fed: BTreeSet<String> = FRAMES.iter().map(|(k, _)| k.to_string()).collect();
    fed.insert("turn_end".into());
    fed.insert("tmux_sessions".into());
    let all = parse_frame_kinds();
    assert!(
        all.len() >= 10,
        "只从 parse_frame 里摘到 {} 种 —— 抽取坏了（本条此刻是空转的）",
        all.len()
    );
    assert_eq!(
        fed, all,
        "喂进吸收点的帧种类与 `parse_frame` 认得的种类对不上 —— 新长了一种帧：\
         先答它是不是会话内容（是 ⇒ 吸收点要把它交回、本机消费者要处置它），再补进 `FRAMES`"
    );
    let mut handed_back = BTreeSet::new();
    for (kind, line) in FRAMES {
        let f = parse_frame(line)
            .unwrap_or_else(|| panic!("手写的 `{kind}` 帧 parse_frame 解不出来：{line}"));
        if let Some(back) = crate::backend::control::local_backend::absorb_local_frame(f, None) {
            // 交回的就是喂进去的那一种（不是别的东西）。
            let same = matches!(
                (kind, &back),
                (&"line", InboundFrame::Line { .. })
                    | (&"session_added", InboundFrame::SessionAdded { .. })
                    | (&"session_removed", InboundFrame::SessionRemoved { .. })
                    | (&"session_status", InboundFrame::SessionStatus { .. })
                    | (&"sessions_replayed", InboundFrame::SessionsReplayed)
                    | (
                        &("session_file_gone" | "session_file_reread"),
                        InboundFrame::SessionFileNotice { .. }
                    )
            );
            assert!(same, "喂的是 `{kind}`，交回来的是别的：{back:?}");
            handed_back.insert(kind.to_string());
        }
    }
    assert_eq!(
        handed_back,
        [
            "line",
            "session_added",
            "session_removed",
            "session_status",
            "sessions_replayed",
            // 〔FW1〕记录文件的出声（同一条内容通道，与行同序）。
            "session_file_gone",
            "session_file_reread",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect::<BTreeSet<_>>(),
        "本机吸收点交回的帧种类 ≠ 内容三种 ＋ 起停两种（〔LOC1b〕红绿灯 · 清单报完了：本机活会话表由这条流喂）＋ 记录文件出声两种（〔FW1〕）：\
         多交 ⇒ 别的帧混进来；少交 ⇒ 本机那一种又被就地丢了（`真相源/10 §7.1` 那一形）"
    );
}

// ─── F2 ────────────────────────────────────────────────────────────────────

fn frame(line: &str) -> LocalItem {
    LocalItem::Frame(parse_frame(line).expect("手写帧解不出来"))
}

#[test]
fn the_local_dispatch_core_matches_the_hand_written_table() {
    const LINE_A: &str =
        r#"{"kind":"line","session_id":"a","path":"/p/a.jsonl","seq":4,"raw":"{\"x\":1}"}"#;
    const LINE_B: &str =
        r#"{"kind":"line","session_id":"b","path":"/p/b.jsonl","seq":0,"raw":"{}"}"#;
    const ADD_A: &str = r#"{"kind":"session_added","sid":"a","session_kind":"interactive","path":"/p/a.jsonl","lines":4}"#;
    const ADD_A_OLD_CC: &str = r#"{"kind":"session_added","sid":"a"}"#;
    const ADD_B_BG: &str =
        r#"{"kind":"session_added","sid":"b","session_kind":"bg","path":"/p/b.jsonl","lines":9}"#;
    const REM_B: &str = r#"{"kind":"session_removed","sid":"b"}"#;
    const STATUS: &str = r#"{"kind":"session_status","sid":"a","status":"busy"}"#;

    let line_a = LocalStep::Line {
        session_id: "a".into(),
        path: "/p/a.jsonl".into(),
        seq: 4,
        raw: r#"{"x":1}"#.into(),
        end: None,
    };
    let line_b = LocalStep::Line {
        session_id: "b".into(),
        path: "/p/b.jsonl".into(),
        seq: 0,
        raw: "{}".into(),
        end: None,
    };

    // ① 显示 bg：一切照转。
    let mut h = HashSet::new();
    assert_eq!(local_step(frame(LINE_A), true, &mut h), line_a);
    assert_eq!(
        local_step(frame(ADD_A), true, &mut h),
        LocalStep::Announce {
            sid: "a".into(),
            path: Some("/p/a.jsonl".into()),
            lines: Some(4)
        }
    );
    assert_eq!(
        local_step(frame(ADD_B_BG), true, &mut h),
        LocalStep::Announce {
            sid: "b".into(),
            path: Some("/p/b.jsonl".into()),
            lines: Some(9)
        }
    );
    assert_eq!(local_step(frame(LINE_B), true, &mut h), line_b);
    assert_eq!(
        local_step(frame(REM_B), true, &mut h),
        LocalStep::Remove { sid: "b".into() }
    );
    assert_eq!(local_step(frame(STATUS), true, &mut h), LocalStep::Skip);
    assert!(h.is_empty());

    // ② 不显示 bg：bg 的宣告与它的行一律不进；交互 / 旧 CC（没写 kind）照转。
    let mut h = HashSet::new();
    assert_eq!(local_step(frame(ADD_B_BG), false, &mut h), LocalStep::Skip);
    assert_eq!(local_step(frame(LINE_B), false, &mut h), LocalStep::Skip);
    assert_eq!(
        local_step(frame(ADD_A_OLD_CC), false, &mut h),
        LocalStep::Announce {
            sid: "a".into(),
            path: None,
            lines: None
        }
    );
    assert_eq!(local_step(frame(LINE_A), false, &mut h), line_a);
    // 退场：撤它，藏起来的集合里也忘掉 —— 同 sid 以交互身份回来就照常显示。
    assert_eq!(
        local_step(frame(REM_B), false, &mut h),
        LocalStep::Remove { sid: "b".into() }
    );
    assert!(h.is_empty(), "退场之后 bg 集合里还留着它：{h:?}");
    assert_eq!(local_step(frame(LINE_B), false, &mut h), line_b);

    // ③ 流结束：集合清空（下一条流会重新宣告）。
    let mut h = HashSet::new();
    let _ = local_step(frame(ADD_B_BG), false, &mut h);
    assert_eq!(
        local_step(LocalItem::StreamEnded, false, &mut h),
        LocalStep::StreamEnded
    );
    assert!(h.is_empty());
}

// ─── F3 ────────────────────────────────────────────────────────────────────

#[test]
fn both_read_loops_hand_content_frames_to_the_local_channel_exactly_once() {
    let stdio = guard_core::production_code(include_str!(
        "../../src/bridge/src/backend/control/local_backend.rs"
    ));
    let host =
        guard_core::production_code(include_str!("../../src/bridge/src/local_backend_host.rs"));
    // stdio 载体是裸线程 ⇒ `_blocking` 那一形；常驻载体是 tokio 任务 ⇒ `.await` 那一形。
    for (name, src, deliver, ended, wrong) in [
        (
            "stdio 载体（local_backend·rs）",
            &stdio,
            "crate::local_lines::deliver_blocking(f);",
            "crate::local_lines::stream_ended_blocking();",
            "crate::local_lines::deliver(",
        ),
        (
            "常驻载体（local_backend_host·rs）",
            &host,
            "crate::local_lines::deliver(f).await;",
            "crate::local_lines::stream_ended().await;",
            "crate::local_lines::deliver_blocking(",
        ),
    ] {
        let d = guard_core::find_pinned(src, deliver)
            .unwrap_or_else(|e| panic!("{name}：`{deliver}` 不是恰好一处：{e}"));
        let e = guard_core::find_pinned(src, ended)
            .unwrap_or_else(|e| panic!("{name}：`{ended}` 不是恰好一处：{e}"));
        assert!(
            d < e,
            "{name}：「流结束」要送在读循环**之后**（它前面是送帧的那一处）"
        );
        // 送进通道的就是吸收点交回来的那个 `Some(f)`：它紧跟在 `if let Some(f) = …absorb_local_frame(…) {` 后面。
        let head = &src[..d];
        let a = head
            .rfind("absorb_local_frame(")
            .unwrap_or_else(|| panic!("{name}：送帧之前没有吸收点"));
        assert!(
            head[..a]
                .rfind("if let Some(f) =")
                .is_some_and(|k| a - k < 80),
            "{name}：送进通道的必须就是吸收点交回来的那一帧（`if let Some(f) = …absorb_local_frame(…)`）"
        );
        assert!(
            head[a..].trim_end().ends_with('{') && !head[a..].contains(';'),
            "{name}：吸收点与送帧之间不许夹别的语句"
        );
        assert!(
            !src.contains(wrong),
            "{name}：用了另一种载体的送法 `{wrong}`（tokio 任务里 blocking_send 会 panic；裸线程里没有 runtime 可 await）"
        );
    }
}

// ─── F4 ────────────────────────────────────────────────────────────────────

#[test]
fn every_line_leaves_through_the_one_intake_both_sources_share() {
    let cases: &[(&str, &[(&str, &str)], &str)] = &[
        (
            "batch_to_payloads(",
            &[("ssh_source.rs", "flush_lines")],
            "行 → 载荷只有一个出口；多一处 ⇒ 又长出一条不经 LineIntake 的内容路（本机 watcher 那一形）",
        ),
        (
            "on_line_batch_awaited(",
            &[("ssh_source.rs", "flush_lines")],
            "重放缓冲只有一个入口",
        ),
        (
            "flush_lines(",
            &[
                ("ssh_source.rs", "fetch_snapshot"),
                ("ssh_source.rs", "flush"),
                ("ssh_source.rs", "line"),
            ],
            "冲批只从 LineIntake 的两个口与旁路快照走",
        ),
        (
            "LineIntake::open(",
            &[
                ("ssh_source.rs", "consume_local"),
                ("ssh_source.rs", "stream_loop"),
            ],
            "两个帧源（远端 stream_loop · 本机 consume_local）各构造一次同一个收口",
        ),
        (
            "Batcher::new(",
            &[("ssh_source.rs", "open")],
            "攒批器只在收口里造 —— 远端若再自己造一个，就又是两份",
        ),
        (
            "SnapshotQueue::new(",
            &[("ssh_source.rs", "open")],
            "快照队列只在收口里造",
        ),
    ];
    for (needle, want, why) in cases {
        let got = callers_of(needle);
        assert!(!got.is_empty(), "`{needle}` 一个调用方都没扫到 —— 扫描坏了");
        assert_eq!(got, set(want), "`{needle}` 的生产调用方对不上：{why}");
    }
}

// ─── F6 ────────────────────────────────────────────────────────────────────

#[test]
fn only_remote_snapshots_compensate_with_a_session_ended() {
    use crate::origin::Origin;
    use crate::ssh_source::compensates_on_cancel;
    assert!(
        !compensates_on_cancel(&Origin::local()),
        "本机快照被撤时补发 session-ended ⇒ 会把本机那边判成「可重连」的会话压成「已结束」\
         （本机行不复活 tab，没有要治的病）"
    );
    assert!(
        compensates_on_cancel(&Origin("devbox".into())),
        "远端快照被撤时不补 ⇒ 已 flush 的那一块把刚归档的远端 tab 见行复活（D-B1 僵尸）"
    );
    // 消费者给本机收口的名字就是本机那个 origin（否则上面那一格判的不是它）。
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    guard_core::find_pinned(&prod, "let label = crate::origin::LOCAL.to_string();")
        .unwrap_or_else(|e| panic!("本机消费者的收口名字不是 `origin::LOCAL`：{e}"));
}

// ─── F7 ────────────────────────────────────────────────────────────────────

/// 那套 watcher 退场之后不许再出现的名字（生产段）。
const GONE: &[&str] = &[
    "crate::watcher",
    "mod watcher;",
    "initial_scan_done",
    "force_rescan",
];

fn hits_in(prod: &str) -> Vec<&'static str> {
    GONE.iter().copied().filter(|n| prod.contains(n)).collect()
}

#[test]
fn the_second_watcher_leaves_no_trace_in_monitor_production() {
    // 正控：同一个判定函数喂一段合成代码，四针全中。
    let synthetic = format!(
        "mod {w};\nuse crate::{w}::JsonlLine;\nlet {i} = h.{i};\nlet _ = {f}_tx.send(x);\n",
        w = "watcher",
        i = "initial_scan_done",
        f = "force_rescan"
    );
    assert_eq!(hits_in(&synthetic).len(), GONE.len(), "判定函数本身是瞎的");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut bad = Vec::new();
    let mut n = 0usize;
    for (path, raw) in guard_core::scan_tree!(&root, &["rs"]) {
        n += 1;
        let prod = guard_core::production_code(&raw);
        for h in hits_in(&prod) {
            bad.push(format!("{} · `{h}`", path.display()));
        }
    }
    assert!(n >= 90, "只扫到 {n} 份 .rs —— 遍历坏了");
    assert!(
        !root.join("watcher.rs").exists(),
        "`src/bridge/src/watcher.rs` 又出现了 —— 那是第二套 watcher（`真相源/10 §7.1`）"
    );
    assert!(
        bad.is_empty(),
        "monitor 生产段里那套 watcher 的名字又出现了：\n{}",
        bad.join("\n")
    );
}

// ─── F8（真后端，`#[ignore]`）────────────────────────────────────────────────

/// **真后端（stdio 载体、隔离 HOME）× 生产读循环（`local_stdio_consumer`）× 本机内容通道。**
///
/// 由 `tests/evidence/CF1-local-lines.py` 带 `CF1_BACKEND`（后端二进制）来跑；没有那个变量就不该跑（`#[ignore]`）。
/// 夹具：一个活的 `sleep` 进程冒充会话（pidfile 的 `procStart` 取它自己的 `/proc` 启动时刻 —— 后端的冒名检查认这个），
/// 会话文件先写 3 行（其中一行是空白，不计行号）。起参就是生产那一份 `LOCAL_STREAM_ARGS`。
/// 期望：宣告帧带 `path` 与 `lines == 2`（prime 到的完整行数）；再追加一行 ⇒ `line` 帧 `seq == 2`、原文不变，
/// 且历史那两行**不**作为 `line` 帧重放（tail-only）；后端被杀 ⇒ 读循环收尾送「流结束」。
#[test]
#[ignore = "要真后端二进制：由 tests/evidence/CF1-local-lines.py 带 CF1_BACKEND 来跑"]
fn a_real_backend_feeds_local_lines_through_the_production_read_loop() {
    use std::io::Write;
    let _local = crate::backend::control::inbound_client::local_origin_test_lock();
    let bin = std::env::var("CF1_BACKEND").expect("没有 CF1_BACKEND —— 这条只该由读数脚本来跑");
    let mut rx = tap().expect("本机内容通道已经被装过了 —— 这条要单独跑");
    let home = std::env::temp_dir().join(format!("cf1-local-lines-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let claude = home.join(".claude");
    let proj = claude.join("projects").join("-tmp-cf1");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(claude.join("sessions")).unwrap();
    // 冒充会话的活进程。
    // ⚠ C7i：它不许带着「我在哪个 tmux 窗格里」—— 后端宣告时会去给那个窗格打 `@ccm_sid`。
    //   后端自己的 `TMUX` 也摘了、`TMUX_TMPDIR` 指进临时家目录（与 SR1a 那条同一套隔离）。
    let mut sleeper = std::process::Command::new("sleep")
        .arg("600")
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .spawn()
        .unwrap();
    let pid = sleeper.id();
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let after_comm = &stat[stat.rfind(')').unwrap() + 2..];
    // `/proc/<pid>/stat` 第 22 列 starttime；`comm` 之后从第 3 列起数 ⇒ 下标 19。
    let ticks: u64 = after_comm.split(' ').nth(19).unwrap().parse().unwrap();
    let sid = "cf100000-0000-4000-8000-000000000001";
    let jsonl = proj.join(format!("{sid}.jsonl"));
    std::fs::write(
        &jsonl,
        "{\"type\":\"user\",\"n\":0}\n   \n{\"type\":\"assistant\",\"n\":1}\n",
    )
    .unwrap();
    std::fs::write(
        claude.join("sessions").join(format!("{pid}.json")),
        format!(
            "{{\"pid\":{pid},\"sessionId\":\"{sid}\",\"cwd\":\"/tmp/cf1\",\"kind\":\"interactive\",\"procStart\":\"{ticks}\"}}"
        ),
    )
    .unwrap();
    // 断言失败时也要把两个子进程收掉（否则后端握着继承来的 stderr，外层 cargo 要等到 `sleep 600` 自己退才收工）。
    struct Reap(Vec<u32>);
    impl Drop for Reap {
        fn drop(&mut self) {
            for pid in &self.0 {
                let _ = std::process::Command::new("kill")
                    .arg(pid.to_string())
                    .status();
            }
        }
    }
    let mut reap = Reap(vec![pid]);
    let mut child = std::process::Command::new(&bin)
        .args(crate::backend::control::local_backend::LOCAL_STREAM_ARGS)
        .env("HOME", &home)
        .env("CLAUDE_CONFIG_DIR", &claude)
        .env("TMUX_TMPDIR", &home)
        .env_remove("TMUX")
        .env_remove("CCM_LISTEN_PORT")
        .env_remove("CCM_LISTEN_TOKEN")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("起不了后端");
    reap.0.push(child.id());
    let (stdin, stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
    let reader = std::thread::spawn(move || {
        crate::backend::control::local_backend::local_stdio_consumer(stdin, stdout)
    });
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let next = |rx: &mut tokio::sync::mpsc::Receiver<LocalItem>| {
        rt.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(20), rx.recv())
                .await
                .expect("20 秒没等到下一件")
                .expect("通道断了")
        })
    };
    // ① 宣告：带 path 与 prime 到的完整行数；宣告之前不许有这个会话的行帧。
    let (path, lines) = loop {
        match next(&mut rx) {
            LocalItem::Frame(InboundFrame::SessionAdded {
                sid: s,
                path,
                lines,
                ..
            }) if s == sid => break (path, lines),
            LocalItem::Frame(InboundFrame::Line { session_id, .. }) if session_id == sid => {
                panic!("宣告之前就来了这个会话的行帧")
            }
            _ => continue,
        }
    };
    assert_eq!(path.as_deref(), Some(jsonl.to_string_lossy().as_ref()));
    assert_eq!(lines, Some(2), "prime 到的完整行数（空白行不计）");
    // ② 追加一行 ⇒ 下一条这个会话的 line 帧就是它：seq == 行号 2（历史两行不重放）。
    let appended = "{\"type\":\"user\",\"n\":2}";
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&jsonl)
        .unwrap();
    writeln!(f, "{appended}").unwrap();
    drop(f);
    // ⚠ 后端先宣告（Phase 1 同步扫、prime 游标）、**之后**才挂上 `projects/` 的 watch ⇒ 恰好落在这两步之间的
    //   一次写不会有事件，要等下一次写才被读出来（游标已 prime，不丢，只是晚到 —— 首跑实打到过一次 20 s 空等）。
    //   真会话会一直写，这里照做：每 2 s 没等到就再补一行**空白行**（不计行号、不改编号）催一次事件。
    let wait_line = |rx: &mut tokio::sync::mpsc::Receiver<LocalItem>| {
        rt.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
                .await
                .ok()
                .map(|x| x.expect("通道断了"))
        })
    };
    let mut nudges = 0;
    let (seq, raw) = loop {
        match wait_line(&mut rx) {
            Some(LocalItem::Frame(InboundFrame::Line {
                session_id,
                seq,
                raw,
                ..
            })) if session_id == sid => break (seq, raw),
            Some(_) => continue,
            None => {
                nudges += 1;
                assert!(nudges <= 10, "补了 10 次事件还没等到新行");
                let mut f = std::fs::OpenOptions::new()
                    .append(true)
                    .open(&jsonl)
                    .unwrap();
                writeln!(f, "  ").unwrap();
            }
        }
    };
    assert_eq!((seq, raw.as_str()), (2, appended));
    println!("CF1-LOCAL-LINES nudges={nudges}");
    // ③ 后端没了 ⇒ 读循环收尾送「流结束」。
    let _ = child.kill();
    let _ = child.wait();
    loop {
        if let LocalItem::StreamEnded = next(&mut rx) {
            break;
        }
    }
    let _ = reader.join();
    let _ = sleeper.kill();
    let _ = sleeper.wait();
    let _ = std::fs::remove_dir_all(&home);
    println!("CF1-LOCAL-LINES ok");
}

// ─── L1〔LOC1b · 第四波 4D〕本机起停帧 ⇒ 本机活会话表 ──────────────────────────────────
//
// 要求住址：`INVARIANTS §40` 逐字「我的目的就是把本地当成不走 ssh 的远端」· `设计/01 §1.1` 逐字「一切判定都在后端」。

#[test]
fn the_local_lifecycle_core_matches_the_hand_written_table() {
    use crate::session_map::{Lifecycle, LiveEntry, RemovalCause};
    use crate::ssh_source::local_lifecycle;
    const ADD_A: &str = r#"{"kind":"session_added","sid":"a","session_kind":"interactive","cwd":"/w","name":"n","status":"busy","pid":42}"#;
    const ADD_B_BG: &str = r#"{"kind":"session_added","sid":"b","session_kind":"bg"}"#;
    const STATUS_A: &str = r#"{"kind":"session_status","sid":"a","status":"idle"}"#;
    const STATUS_B: &str = r#"{"kind":"session_status","sid":"b","status":"idle"}"#;
    const REM_A: &str = r#"{"kind":"session_removed","sid":"a","cause":"superseded"}"#;
    const REM_B: &str = r#"{"kind":"session_removed","sid":"b"}"#;
    const LISTED: &str = r#"{"kind":"sessions_replayed"}"#;
    const LINE: &str = r#"{"kind":"line","session_id":"a","path":"/p/a.jsonl","seq":0,"raw":"{}"}"#;

    let none = HashSet::new();
    assert_eq!(
        local_lifecycle(&frame(ADD_A), true, &none, &[]),
        Some(Lifecycle::Added {
            sid: "a".into(),
            entry: LiveEntry {
                cwd: Some("/w".into()),
                kind: Some("interactive".into()),
                name: Some("n".into()),
                status: Some("busy".into()),
                waiting_for: None,
                pid: Some(42),
            }
        })
    );
    assert_eq!(
        local_lifecycle(&frame(STATUS_A), true, &none, &[]),
        Some(Lifecycle::Status {
            sid: "a".into(),
            status: Some("idle".into()),
            waiting_for: None
        })
    );
    assert_eq!(
        local_lifecycle(&frame(REM_A), true, &none, &[]),
        Some(Lifecycle::Removed {
            sid: "a".into(),
            cause: RemovalCause::Superseded
        })
    );
    assert_eq!(
        local_lifecycle(&frame(LISTED), true, &none, &[]),
        Some(Lifecycle::Listed)
    );
    assert_eq!(
        local_lifecycle(&frame(LINE), true, &none, &[]),
        None,
        "内容行不是起停事实"
    );
    assert_eq!(
        local_lifecycle(&LocalItem::StreamEnded, true, &none, &["c".to_string()]),
        Some(Lifecycle::StreamEnded {
            idle: vec!["c".into()]
        })
    );
    // 不显示 bg：bg 的宣告不进；已藏起来的 sid 的灯与摘除也不进（同 `local_step` 的藏法）。
    assert_eq!(local_lifecycle(&frame(ADD_B_BG), false, &none, &[]), None);
    assert!(
        local_lifecycle(&frame(ADD_B_BG), true, &none, &[]).is_some(),
        "正控：显示 bg 时它进"
    );
    let hidden: HashSet<String> = ["b".to_string()].into_iter().collect();
    assert_eq!(local_lifecycle(&frame(STATUS_B), false, &hidden, &[]), None);
    assert_eq!(local_lifecycle(&frame(REM_B), false, &hidden, &[]), None);
    assert!(
        local_lifecycle(&frame(REM_B), false, &none, &[]).is_some(),
        "正控：没藏的照进"
    );
}

/// 〔FW1 · 第四波 4D · D-d〕本机分派核：记录文件的出声交 `Notice`；藏起来的 bg 会话照旧不出声。
#[test]
fn a_session_file_notice_is_dispatched_unless_the_session_is_hidden() {
    const GONE: &str = r#"{"kind":"session_file_gone","session_id":"a","path":"/p/a.jsonl"}"#;
    const REREAD_B: &str =
        r#"{"kind":"session_file_reread","session_id":"b","path":"/p/b.jsonl","why":"truncated"}"#;
    const ADD_B_BG: &str = r#"{"kind":"session_added","sid":"b","session_kind":"bg"}"#;
    let mut h = HashSet::new();
    assert_eq!(
        local_step(frame(GONE), false, &mut h),
        LocalStep::Notice {
            sid: "a".into(),
            path: "/p/a.jsonl".into(),
            change: crate::ssh_source::FileChange::Gone,
        }
    );
    assert_eq!(local_step(frame(ADD_B_BG), false, &mut h), LocalStep::Skip);
    assert_eq!(local_step(frame(REREAD_B), false, &mut h), LocalStep::Skip);
    assert_eq!(
        local_step(frame(REREAD_B), true, &mut HashSet::new()),
        LocalStep::Notice {
            sid: "b".into(),
            path: "/p/b.jsonl".into(),
            change: crate::ssh_source::FileChange::Truncated,
        }
    );
}
