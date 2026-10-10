//! **本机会话内容走本机后端的 Line 帧** 的判据。
//!
//! 守的要求：「本机改走后端的 Line 帧，消掉 monitor 里第二套 watcher 与第二套 offsets/seq」·
//! 「一条流，一个来源；本机与远端走同一条帧路」· 级 3 无界堆积是禁止态。
//! 设计与读数住。
//!
//! | # | 判什么 | 异源 / 两向 |
//! |---|---|---|
//! | F1 | 本机吸收点交回的帧种类 == {line, session_added, session_removed} ∪{session_status, sessions_replayed} ∪{session_file_gone, session_file_reread} ∪{changed}（全部主题，本机账号清单 / 配置文件变了从前在这里被丢）；喂的种类 == `parse_frame` 认得的全部种类 | 帧是手写线上 JSON；种类全集从 `parse_frame` 源码里摘（两向） |
//! | F2 | 本机消费者的纯分派核真值表 | 期望手写 |
//! | F3 | 两条读循环各恰好一处把交回的帧送进本机内容通道（送法各按载体）、各恰好一处送「流结束」 | 源码锚，恰好一处 |
//! | F4 | 内容出口的调用方集合（`batch_to_payloads` / `on_line_batch_awaited` / `flush_lines` / `LineIntake::open` / `Batcher::new` / `SnapshotQueue::new`） | 全仓生产段扫描，两向集合相等 |
//! | F5 | 两条载体的起参是同一份常量，且其中每一个旗标都 ∈ 后端 `STREAM_FLAGS`；`--tail-only` 在 ⟺ 消费者认定 tail-only | 后端源码 —— 住 `stream_source/stream_flag_gate_tests.rs`（同一条跨半边已登记，不另开一条） |
//! | F6 | 快照被撤时的补偿归档：本机不补、远端补 | — |
//! | F7 | monitor 生产段里那套 watcher 的名字零命中（带正控） | — |
//! | L1 | 本机起停帧 ⇒ 本机活会话表的起停事实（藏起来的 bg 不进；流断带上可重连那一摞） | 期望手写 |
//! | Q1 | 本机后端推来的 `changed`（账号清单 · 额度 · 轮换 · 规则）走生产那一串（吸收点 → 本机通道 → `consume_local`）⇒ `<local>` 上各主题的 `changed/<主题>` 订阅逐格收到；交这一格的生产调用方 == {远端 `stream_loop`, 本机 `consume_local`} | 期望手写；全仓生产段扫描，两向集合相等 |
//! | F8 | 真后端 × 生产 stdio 读循环 ⇒ 宣告带 `path`/`lines`、新行的 `seq` == 行号（`#[ignore]`，由 `tests/evidence/CF1-local-lines.py` 带二进制跑） | 两侧各是真实现 |

use super::*;
use crate::stream_source::{local_step, parse_frame, BgHide, InboundFrame, LocalItem, LocalStep};
use std::collections::BTreeSet;

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
        // 按模块住址认：人群声明带进来的兄弟包（通信层 `comms-inward` 等）认作 `<包名>/…`。
        let file = guard_core::module_address(&root, &path);
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

/// 每一种帧一行手写线上 JSON（**不从实现生成**）。
///
/// ⚠ 一种不在表里、理由写清：`turn_end` 认识但不消费（本就不进任何吸收点）。
/// tmux 观测那两种帧删了（后端不再发，monitor 不再认）。
///
/// ⇒ 表的种类 ＋ 这两种 == `parse_frame` 的全部臂（两向），新长一种帧就红：先答它是不是内容。
const FRAMES: &[(&str, &str)] = &[
    (
        "hello",
        r#"{"kind":"hello","v":1,"build_id":"x","host_arch":"x86_64","claude_dir":"/h/.claude"}"#,
    ),
    (
        "line",
        r#"{"kind":"line","session_id":"s1","path":"/h/.claude/projects/p/s1.jsonl","seq":7,"byte_offset":70}"#,
    ),
    (
        "session_added",
        r#"{"kind":"session_added","sid":"s1","activity_text":"T","activity_tone":"now","path":"/h/.claude/projects/p/s1.jsonl","lines":7}"#,
    ),
    (
        "session_status",
        r#"{"kind":"session_status","sid":"s1","activity_text":"T","activity_tone":"now","status":"idle","activity":"idle"}"#,
    ),
    (
        "session_removed",
        r#"{"kind":"session_removed","sid":"s1"}"#,
    ),
    // 后端会话账本的成品（去向）—— 进内容通道（去向必须排在那个会话的行之后）。
    (
        "session_state",
        r#"{"kind":"session_state","sid":"s1","state":"ended","state_text":"t","state_hint":"h","state_tone":"plain"}"#,
    ),
    ("overflow", r#"{"kind":"overflow","dropped":3}"#),
    (
        "reply",
        r#"{"kind":"reply","id":"cf1-no-such-id","ok":true}"#,
    ),
    ("cancelled", r#"{"kind":"cancelled","id":"cf1-no-such-id"}"#),
    // 「X 变了」（全部主题）—— 交回读循环（本机消费者交 `changed/<主题>` 订阅，与远端同一个口）。
    (
        "changed",
        r#"{"kind":"changed","topic":"plan","key":"/w","rev":"r1","body":{"needs":2}}"#,
    ),
    (
        "link_data",
        r#"{"kind":"link_data","link":"cf1-no-such-link","data":"aGk="}"#,
    ),
    (
        "link_end",
        r#"{"kind":"link_end","link":"cf1-no-such-link"}"#,
    ),
    // U4b 的「A 的清单报完了」与 SR1b 的传输进度 —— 都不是会话内容。
    // 「清单报完了」从此交回（本机活会话表要它，且要排在它前面那些宣告之后）；传输进度仍就地吸收。
    ("sessions_replayed", r#"{"kind":"sessions_replayed"}"#),
    // 记录文件不见了 / 被改过 —— **是**会话内容那一族（与行同序进内容通道）。
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
    // 测试连接的进度格 —— 不是会话内容，就地交中继（`probe_relay`），不进内容通道。
    (
        "probe",
        r#"{"kind":"probe","ticket":"no-such-ticket","cell":{"reached":"ssh"}}"#,
    ),
    // 终端实时预览的一屏 / 收尾 —— 不是会话内容，就地交中继（`terminal_screen_relay`），不进内容通道。
    (
        "terminal_screen",
        r#"{"kind":"terminal_screen","ticket":"no-such-ticket","seq":1,"view":{}}"#,
    ),
    (
        "terminal_follow_end",
        r#"{"kind":"terminal_follow_end","ticket":"no-such-ticket","why":"gone","said":"x"}"#,
    ),
    // 中转抄出来的 SSE 事件 —— 不是会话内容（jsonl 才是），就地转给前端，不进内容通道。
    (
        "tap",
        r#"{"kind":"tap","stream":"s1","resp":0,"n":0,"ev":{"t":"stop","ok":true}}"#,
    ),
    // 一个会话的运行表 —— 会话成品，与起停同一条有序通道。
    (
        "session_runs",
        r#"{"kind":"session_runs","sid":"s1","runs":[],"ended":[]}"#,
    ),
    // 一个会话的主线外清单 —— 会话成品，同上。
    (
        "session_branch",
        r#"{"kind":"session_branch","sid":"s1","path":"/p/s1.jsonl","off":["u2"]}"#,
    ),
];

#[test]
fn the_absorb_point_hands_back_exactly_the_content_and_lifecycle_frames() {
    // 两向：表里的种类 ＋ 刻意不喂的那一种（认识但不消费）== parse_frame 的全部臂。
    let mut fed: BTreeSet<String> = FRAMES.iter().map(|(k, _)| k.to_string()).collect();
    fed.insert("turn_end".into());
    let all = crate::guard_support::parse_frame_kinds();
    assert_eq!(
        fed, all,
        "喂进吸收点的帧种类与 `parse_frame` 认得的种类对不上 —— 新长了一种帧：\
         先答它是不是会话内容（是 ⇒ 吸收点要把它交回、本机消费者要处置它），再补进 `FRAMES`"
    );
    let mut handed_back = BTreeSet::new();
    for (kind, line) in FRAMES {
        let f = parse_frame(line)
            .unwrap_or_else(|e| panic!("手写的 `{kind}` 帧 parse_frame 解不出来（{e}）：{line}"));
        if let Some(back) =
            crate::local_backend::absorb_local_frame(f, None, &crate::origin::Origin::local())
        {
            // 交回的就是喂进去的那一种（不是别的东西）。
            let same = matches!(
                (kind, &back),
                (&"line", InboundFrame::Line { .. })
                    | (&"changed", InboundFrame::Changed { .. })
                    | (&"session_added", InboundFrame::SessionAdded { .. })
                    | (&"session_removed", InboundFrame::SessionRemoved { .. })
                    | (&"session_state", InboundFrame::SessionState { .. })
                    | (&"session_status", InboundFrame::SessionStatus { .. })
                    | (&"sessions_replayed", InboundFrame::SessionsReplayed)
                    | (&"session_runs", InboundFrame::SessionRuns { .. })
                    | (&"session_branch", InboundFrame::SessionBranch { .. })
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
            // 「X 变了」（本机消费者交 `changed/<主题>` 订阅）。
            "changed",
            "session_state",
            "session_status",
            "sessions_replayed",
            // 记录文件的出声（同一条内容通道，与行同序）。
            "session_file_gone",
            "session_file_reread",
            // 运行表 · 主线外清单（会话成品，交会话账）。
            "session_runs",
            "session_branch",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect::<BTreeSet<_>>(),
        "本机吸收点交回的帧种类 ≠ 内容三种 ＋ 起停两种（红绿灯 · 清单报完了：本机活会话表由这条流喂）＋ 记录文件出声两种 ＋ `changed`：\
         多交 ⇒ 别的帧混进来；少交 ⇒ 本机那一种又被就地丢了"
    );
}

// ─── F2 ────────────────────────────────────────────────────────────────────

fn frame(line: &str) -> LocalItem {
    LocalItem::Frame(parse_frame(line).expect("手写帧解不出来"))
}

/// `consume_local` 那一拍：先过 bg 那道门（[`BgHide::admit`]，远端那条流同一份），过得去的才分派；藏 ⇒ 记成 `Skip`。
fn step(bg: &mut BgHide, item: LocalItem) -> LocalStep {
    bg.admit(item).map(local_step).unwrap_or(LocalStep::Skip)
}

#[test]
fn the_local_dispatch_core_matches_the_hand_written_table() {
    const LINE_A: &str = r#"{"kind":"line","session_id":"a","path":"/p/a.jsonl","seq":4,"record":{"x":1},"byte_offset":50}"#;
    const LINE_B: &str =
        r#"{"kind":"line","session_id":"b","path":"/p/b.jsonl","seq":0,"byte_offset":10}"#;
    const ADD_A: &str = r#"{"kind":"session_added","sid":"a","activity_text":"T","activity_tone":"now","session_kind":"interactive","path":"/p/a.jsonl","lines":4}"#;
    const ADD_A_OLD_CC: &str =
        r#"{"kind":"session_added","sid":"a","activity_text":"T","activity_tone":"now"}"#;
    const ADD_B_BG: &str = r#"{"kind":"session_added","sid":"b","activity_text":"T","activity_tone":"now","session_kind":"bg","background":true,"path":"/p/b.jsonl","lines":9}"#;
    const REM_B: &str = r#"{"kind":"session_removed","sid":"b"}"#;
    const STATUS: &str = r#"{"kind":"session_status","sid":"a","activity_text":"T","activity_tone":"now","status":"busy","activity":"working"}"#;

    let line_a = LocalStep::Line {
        session_id: "a".into(),
        path: "/p/a.jsonl".into(),
        seq: 4,
        record: crate::ui_contract::RecordBody::from_json(r#"{"x":1}"#.into()),
        cwd: None,
        end: Some(50),
        rid: None,
    };
    let line_b = LocalStep::Line {
        session_id: "b".into(),
        path: "/p/b.jsonl".into(),
        seq: 0,
        record: None,
        cwd: None,
        end: Some(10),
        rid: None,
    };

    // ① 显示 bg：一切照转。
    let mut h = BgHide::new(true);
    assert_eq!(step(&mut h, frame(LINE_A)), line_a);
    assert_eq!(
        step(&mut h, frame(ADD_A)),
        LocalStep::Announce {
            sid: "a".into(),
            path: Some("/p/a.jsonl".into()),
            lines: Some(4)
        }
    );
    assert_eq!(
        step(&mut h, frame(ADD_B_BG)),
        LocalStep::Announce {
            sid: "b".into(),
            path: Some("/p/b.jsonl".into()),
            lines: Some(9)
        }
    );
    assert_eq!(step(&mut h, frame(LINE_B)), line_b);
    assert_eq!(
        step(&mut h, frame(REM_B)),
        LocalStep::Remove { sid: "b".into() }
    );
    assert_eq!(step(&mut h, frame(STATUS)), LocalStep::Skip);

    // ② 不显示 bg：bg 的宣告与它的行一律不进；交互 / 旧 CC（没写 kind）照转。
    let mut h = BgHide::new(false);
    assert_eq!(step(&mut h, frame(ADD_B_BG)), LocalStep::Skip);
    assert_eq!(step(&mut h, frame(LINE_B)), LocalStep::Skip);
    assert_eq!(
        step(&mut h, frame(ADD_A_OLD_CC)),
        LocalStep::Announce {
            sid: "a".into(),
            path: None,
            lines: None
        }
    );
    assert_eq!(step(&mut h, frame(LINE_A)), line_a);
    // 退场：撤它；藏起来的要留到它的去向（`session_state`）那一帧才忘 —— 摘除那一帧就忘了 ⇒ 紧跟的去向会漏出去。
    assert_eq!(
        step(&mut h, frame(REM_B)),
        LocalStep::Remove { sid: "b".into() }
    );
    assert_eq!(
        step(
            &mut h,
            frame(
                r#"{"kind":"session_state","sid":"b","state":"ended","state_text":"t","state_hint":"h","state_tone":"plain"}"#
            )
        ),
        LocalStep::Skip
    );
    assert_eq!(
        step(&mut h, frame(LINE_B)),
        line_b,
        "去向之后还藏着它（同一个 sid 再来是新的宣告）"
    );

    // ③ 流结束照过那道门（`consume_local` 每条流换一道新门：下一条流会重新宣告）。
    let mut h = BgHide::new(false);
    let _ = step(&mut h, frame(ADD_B_BG));
    assert_eq!(step(&mut h, LocalItem::StreamEnded), LocalStep::StreamEnded);
    assert_eq!(step(&mut h, LocalItem::LineLost), LocalStep::Lost);
}

// ─── F3 ────────────────────────────────────────────────────────────────────

#[test]
fn both_read_loops_hand_content_frames_to_the_local_channel_exactly_once() {
    let stdio = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend.rs"
    ));
    let host = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    // stdio 载体是裸线程 ⇒ `_blocking` 那一形；常驻载体是 tokio 任务 ⇒ `.await` 那一形。
    for (name, src, deliver, ended, wrong) in [
        (
            "stdio 载体（local_backend·rs）",
            &stdio,
            "route.out.deliver(f);",
            "route.out.stream_ended();",
            "crate::local_lines::deliver(",
        ),
        (
            "常驻载体（local_backend_host·rs）",
            &host,
            "crate::local_lines::deliver(f).await;",
            "crate::local_lines::stream_ended().await;",
            "route.out.deliver(",
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
            &[("stream_source/batch.rs", "flush_lines")],
            "行 → 载荷只有一个出口；多一处 ⇒ 又长出一条不经 LineIntake 的内容路（本机 watcher 那一形）",
        ),
        (
            "on_line_batch_awaited(",
            &[("stream_source/batch.rs", "flush_lines")],
            "重放缓冲只有一个入口",
        ),
        (
            "flush_lines(",
            &[
                ("stream_source/snapshot.rs", "fetch_snapshot"),
                ("stream_source/batch.rs", "flush"),
                ("stream_source/batch.rs", "line"),
            ],
            "冲批只从 LineIntake 的两个口与旁路快照走",
        ),
        (
            "LineIntake::open(",
            &[
                ("stream_source/local.rs", "consume_local"),
                ("stream_source/run.rs", "stream_loop"),
            ],
            "两个帧源（远端 stream_loop · 本机 consume_local）各构造一次同一个收口",
        ),
        (
            "Batcher::new(",
            &[("stream_source/batch.rs", "open")],
            "攒批器只在收口里造 —— 远端若再自己造一个，就又是两份",
        ),
        (
            "SnapshotQueue::new(",
            &[("stream_source/batch.rs", "open")],
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
    use crate::stream_source::compensates_on_cancel;
    assert!(
        !compensates_on_cancel(&Origin::local()),
        "本机快照被撤时补发 session-ended ⇒ 会把本机那边判成「可重连」的会话压成「已结束」\
         （本机行不复活 tab，没有要治的病）"
    );
    assert!(
        compensates_on_cancel(&Origin("devbox".into())),
        "远端快照被撤时不补 ⇒ 已 flush 的那一块把刚归档的远端 tab 见行复活（D-B1 僵尸）"
    );
    // 产品起本机消费者时给的收口名字就是本机那个 origin（否则上面那一格判的不是它）。
    let prod: String = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_lines.rs"
    ))
    .chars()
    .filter(|c| !c.is_whitespace())
    .collect();
    assert_eq!(
        prod.matches("crate::stream_source::consume_local(crate::origin::LOCAL.to_string(),")
            .count(),
        1,
        "本机消费者的收口名字不是 `origin::LOCAL`"
    );
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
        "`src/frontend/shell/src/watcher.rs` 又出现了 —— 那是第二套 watcher"
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
    let _local = crate::inbound_client::local_origin_test_lock();
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
        .args(crate::local_backend::LOCAL_STREAM_ARGS)
        .env("HOME", &home)
        .env("CLAUDE_CONFIG_DIR", &claude)
        .env("TMUX_TMPDIR", &home)
        .env_remove("TMUX")
        .env_remove("CCM_LISTEN_PORT")
        .env_remove("CCM_LISTEN_TOKEN_FILE")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("起不了后端");
    reap.0.push(child.id());
    let (stdin, stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
    let reader = std::thread::spawn(move || {
        crate::local_backend::local_stdio_consumer(
            &crate::local_backend::StdioRoute::local(),
            stdin,
            stdout,
        )
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
    // 后端出成品：这一行要进得了界面（带链身份的 user 记录），帧上才有 `message` 可比。
    let appended = "{\"type\":\"user\",\"uuid\":\"u2\",\"timestamp\":\"t\",\"message\":{\"role\":\"user\",\"content\":\"x\"}}";
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
    let (seq, message) = loop {
        match wait_line(&mut rx) {
            Some(LocalItem::Frame(InboundFrame::Line {
                session_id,
                seq,
                record,
                ..
            })) if session_id == sid => break (seq, record),
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
    assert_eq!(seq, 2);
    let m = message.expect("后端没给那一行的成品");
    assert!(
        m.0.get().contains("\"id\":\"u2\""),
        "成品不是那一行：{}",
        m.0.get()
    );
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

// ─── L1本机起停帧 ⇒ 交 `session_book` 的成品 ──────────────────────────────────
//
// 要求住址：`INVARIANTS §40` 逐字「我的目的就是把本地当成不走 ssh 的远端」· 「一切判定都在后端」。

#[test]
fn the_local_product_core_matches_the_hand_written_table() {
    use crate::session_book::{Fate, In, LiveMeta};
    use crate::stream_source::local_product;
    // 带启动期令牌：成品要把它原样交给前端（`launch-arrival.ts` 认「我刚起的那条」）。
    const ADD_A: &str = r#"{"kind":"session_added","sid":"a","activity_text":"T","activity_tone":"now","session_kind":"interactive","cwd":"/w","project_dir":"/w/p","name":"n","status":"busy","activity":"working","pid":42,"container":{"host":"tmux","terminal":"tmux-3-7"}}"#;
    const ADD_B_BG: &str = r#"{"kind":"session_added","sid":"b","activity_text":"T","activity_tone":"now","session_kind":"bg","background":true}"#;
    const STATUS_A: &str = r#"{"kind":"session_status","sid":"a","activity_text":"T","activity_tone":"now","status":"idle","activity":"idle"}"#;
    const STATUS_B: &str = r#"{"kind":"session_status","sid":"b","activity_text":"T","activity_tone":"now","status":"idle","activity":"idle"}"#;
    const LEFT_A: &str = r#"{"kind":"session_state","sid":"a","state":"reconnectable","state_text":"t","state_hint":"h","state_tone":"plain"}"#;
    const LEFT_B: &str = r#"{"kind":"session_state","sid":"b","state":"ended","state_text":"t","state_hint":"h","state_tone":"plain"}"#;
    const REM_A: &str = r#"{"kind":"session_removed","sid":"a"}"#;
    const LISTED: &str = r#"{"kind":"sessions_replayed"}"#;
    const LINE: &str =
        r#"{"kind":"line","session_id":"a","path":"/p/a.jsonl","seq":0,"byte_offset":10}"#;
    let local = || "<local>".to_string();

    assert_eq!(
        local_product(&frame(ADD_A), crate::origin::LOCAL),
        Some(In::Live {
            origin: local(),
            sid: "a".into(),
            meta: LiveMeta {
                cwd: Some("/w".into()),
                project_dir: Some("/w/p".into()),
                name: Some("n".into()),
                activity: Some(crate::session_book::SessionActivity::Working),
                activity_text: "T".into(),
                activity_tone: "now".into(),
                container: Some(crate::session_book::SessionContainer::Hosted {
                    host: crate::session_book::TerminalHost::Tmux,
                    terminal: Some("tmux-3-7".into()),
                }),
                pid: Some(42),
                ..Default::default()
            }
        })
    );
    assert_eq!(
        local_product(&frame(STATUS_A), crate::origin::LOCAL),
        Some(In::Status {
            origin: local(),
            sid: "a".into(),
            activity: Some(crate::session_book::SessionActivity::Idle),
            activity_text: "T".into(),
            activity_tone: "now".into(),
            waiting_for: None
        })
    );
    assert_eq!(
        local_product(&frame(LEFT_A), crate::origin::LOCAL),
        Some(In::Left {
            origin: local(),
            sid: "a".into(),
            fate: Fate::Reconnectable,
            words: Some(crate::session_book::FateWords {
                text: "t".into(),
                hint: "h".into(),
                tone: "plain".into(),
            }),
        }),
        "去向原样交（后端裁的，连写好的字一起）"
    );
    assert_eq!(
        local_product(&frame(REM_A), crate::origin::LOCAL),
        None,
        "摘除本身只是内容流的边界"
    );
    assert_eq!(
        local_product(&frame(LISTED), crate::origin::LOCAL),
        Some(In::Listed { origin: local() })
    );
    assert_eq!(
        local_product(&frame(LINE), crate::origin::LOCAL),
        None,
        "内容行不是起停成品"
    );
    assert_eq!(
        local_product(&LocalItem::StreamEnded, crate::origin::LOCAL),
        Some(In::LinkLost { origin: local() })
    );
    // 不显示 bg：过那道门（[`BgHide::admit`]，同 `consume_local`）⇒ bg 的宣告不进；藏起来的 sid 的灯与去向也不进。
    let product = |bg: &mut BgHide, item: LocalItem| {
        bg.admit(item)
            .and_then(|i| local_product(&i, crate::origin::LOCAL))
    };
    let mut off = BgHide::new(false);
    assert_eq!(product(&mut off, frame(ADD_B_BG)), None);
    assert_eq!(product(&mut off, frame(STATUS_B)), None);
    assert_eq!(product(&mut off, frame(LEFT_B)), None);
    assert!(
        product(&mut off, frame(LEFT_B)).is_some(),
        "正控：去向之后不再藏"
    );
    assert!(
        product(&mut BgHide::new(true), frame(ADD_B_BG)).is_some(),
        "正控：显示 bg 时它进"
    );
}

/// 本机分派核：记录文件的出声交 `Notice`；藏起来的 bg 会话照旧不出声。
#[test]
fn a_session_file_notice_is_dispatched_unless_the_session_is_hidden() {
    const GONE: &str = r#"{"kind":"session_file_gone","session_id":"a","path":"/p/a.jsonl"}"#;
    const REREAD_B: &str =
        r#"{"kind":"session_file_reread","session_id":"b","path":"/p/b.jsonl","why":"truncated"}"#;
    const ADD_B_BG: &str = r#"{"kind":"session_added","sid":"b","activity_text":"T","activity_tone":"now","session_kind":"bg","background":true}"#;
    let mut h = BgHide::new(false);
    assert_eq!(
        step(&mut h, frame(GONE)),
        LocalStep::Notice {
            sid: "a".into(),
            path: "/p/a.jsonl".into(),
            change: crate::stream_source::FileChange::Gone,
        }
    );
    assert_eq!(step(&mut h, frame(ADD_B_BG)), LocalStep::Skip);
    assert_eq!(step(&mut h, frame(REREAD_B)), LocalStep::Skip);
    assert_eq!(
        step(&mut BgHide::new(true), frame(REREAD_B)),
        LocalStep::Notice {
            sid: "b".into(),
            path: "/p/b.jsonl".into(),
            change: crate::stream_source::FileChange::Truncated,
        }
    );
}

// ─── Q1 本机那条路上的 `changed` ⇒ 界面那条订阅收得到 ───────────────────────
//
// 要求住址：`INVARIANTS §40` 逐字「我的目的就是把本地当成不走 ssh 的远端」—— 远端那条读循环（`stream_loop`）
// 收到 `changed` 就交 `changed/<主题>` 订阅；本机这条从前在吸收点丢过额度 · 账号清单 · 配置文件，界面只能等别的事件顺带重读。

/// 手写线上帧走生产那一串：`parse_frame` → 吸收点 → 本机内容通道 → `consume_local` → 重放缓冲；
/// 订了 `<local>` 上 `changed/quota` · `changed/rotation` · `changed/accounts` 的订阅各逐格收到自己那一格（期望手写）。
#[tokio::test]
async fn local_changed_pushes_reach_the_ui_subscription() {
    use crate::chan::wire::Item as WItem;
    use crate::event_replay::{EventReplay, ItemSink};
    #[derive(Default)]
    struct Rec(std::sync::Mutex<Vec<(u64, serde_json::Value)>>);
    impl ItemSink for Rec {
        fn deliver(&self, _: &str, sub: u64, items: Vec<WItem>) {
            for i in items {
                if let WItem::Frame { body, .. } = i {
                    self.0
                        .lock()
                        .unwrap()
                        .push((sub, serde_json::from_slice(&body.0).unwrap()));
                }
            }
        }
    }
    let replay = std::sync::Arc::new(EventReplay::new());
    let rec = std::sync::Arc::new(Rec::default());
    replay.attach_sink(rec.clone());
    for (id, topic) in [(1, "quota"), (2, "rotation"), (3, "accounts")] {
        replay.subscribe(
            "w",
            id,
            &crate::origin::Origin::local(),
            &format!("{}/{topic}", crate::event_replay::CHANGED_KIND),
            None,
            16,
        );
    }
    let (tx, rx) = tokio::sync::mpsc::channel::<LocalItem>(16);
    let health: crate::stream_source::HealthOut = std::sync::Arc::new(|_| Ok(()));
    let consumer = tokio::spawn(crate::stream_source::consume_local(
        crate::origin::LOCAL.to_string(),
        rx,
        replay.clone(),
        health,
    ));
    for line in [
        r#"{"kind":"changed","topic":"quota"}"#,
        r#"{"kind":"changed","topic":"rotation","key":"s1"}"#,
        r#"{"kind":"changed","topic":"accounts"}"#,
    ] {
        let f = parse_frame(line).expect("手写帧解不出来");
        if let Some(f) =
            crate::local_backend::absorb_local_frame(f, None, &crate::origin::Origin::local())
        {
            tx.send(LocalItem::Frame(f)).await.unwrap();
        }
    }
    drop(tx);
    tokio::time::timeout(std::time::Duration::from_secs(5), consumer)
        .await
        .expect("本机消费者 5 秒没收摊")
        .unwrap();
    assert_eq!(
        *rec.0.lock().unwrap(),
        vec![
            (1, serde_json::json!({})),
            (2, serde_json::json!({"key": "s1"})),
            (3, serde_json::json!({})),
        ],
        "本机后端推来的 `changed`，界面那几条 `changed/<主题>` 订阅没收到（远端那条路收得到）"
    );
}

/// 两条路交界面的是同一个口：`changed` 那一格在 monitor 生产段里恰好由
/// 远端读循环与本机消费者各调一处（多一处 ⇒ 又长出第三条路；少一处 ⇒ 那条路又把它丢了）。
#[test]
fn both_paths_hand_changed_pushes_to_the_same_replay_entry() {
    let want = set(&[
        ("stream_source/local.rs", "consume_local"),
        ("stream_source/run.rs", "stream_loop"),
    ]);
    for needle in ["replay.changed("] {
        assert_eq!(callers_of(needle), want, "`{needle}` 的生产调用方对不上");
    }
}
