//! 帧的判据里引到上层的那几条（夹具经历史那一层的 `name_face` · 识别那一层的 `dress_ask_with` 造，或问 CLI 面的子命令表）：
//! 帧面零件是独立的包（`backend-wire`）、够不到上层 ⇒ 挂在主 crate（`cross/wire.rs`）。其余帧的判据住 `wire_tests.rs`。

use crate::wire::test_support::said_record;
use crate::wire::*;
use serde_json::Value;

/// Helper: assert a line is a single JSON object ending in exactly one
/// `\n` with no other bare newline, then return its parsed `kind`.
fn parse_kind(line: &str) -> String {
    assert!(line.ends_with('\n'), "line must end in newline: {line:?}");
    let body = line.strip_suffix('\n').unwrap();
    assert!(
        !body.contains('\n'),
        "body must contain no bare newline: {body:?}"
    );
    assert!(
        !body.contains('\r'),
        "body must contain no bare carriage return: {body:?}"
    );
    let v: Value = serde_json::from_str(body).expect("body must be valid JSON");
    v.get("kind")
        .and_then(Value::as_str)
        .expect("kind must be a string")
        .to_string()
}

/// 会话流金样的住址（仓根相对）。monitor 那一侧读同一份逐行判自己的解析器。
const SESSION_STREAM_GOLDEN: &str = "tests/__fixtures__/session-stream.golden.jsonl";

/// 每种帧两行：`[全格, 最少格]` —— 全格 ＝ 可选格（`skip_serializing_if` 的）都在；最少格 ＝ 都缺。
/// 没有可选格的帧两行相同。
/// `live` 一种 `op` 一形：全格取整卡（带 `run`），最少格取「说完了」那一形（不带 `run`）。
fn golden_pairs() -> Vec<[Frame; 2]> {
    use crate::agents::{BlockKind, RunDid};
    use crate::wire::Life;
    use crate::wire::{
        LiveBlock, LiveCard, LiveOp, LivePhase, RereadWhy, RunEnded, RunInfo, RunState, RunWhy,
        SessionContainer, TransferEnd, Unavailable,
    };
    use cmd_wire::frame::{RunEndLine, RunExplain, RunLine};
    let s = |v: &str| v.to_string();
    let w = |v: &str| crate::common::cells::Words(v.to_string());
    let both = |f: Frame| [f.clone(), f];
    vec![
        [
            Frame::Hello(crate::wire::Hello {
                build_id: s("b1"),
                host_arch: s("x86_64"),
                version: Some(s("4.2.0")),
                commands: vec![s("ping")],
                unavailable: vec![Unavailable {
                    command: s("tmux-list"),
                    code: s("no_tmux"),
                }],
                host_env: [(s("CCM_RELAY_PORT"), s("47001"))].into_iter().collect(),
                shapes: [(s("ping"), s("0000abcd"))].into_iter().collect(),
                frames: [(s("line"), s("0000abcd"))].into_iter().collect(),
            }),
            Frame::Hello(crate::wire::Hello {
                build_id: s("b1"),
                host_arch: s("x86_64"),
                version: None,
                commands: vec![],
                unavailable: vec![],
                host_env: Default::default(),
                shapes: Default::default(),
                frames: Default::default(),
            }),
        ],
        [
            Frame::Line(crate::wire::Line {
                session_id: s("s1"),
                path: s("/p/s1.jsonl"),
                seq: 3,
                record: Some(crate::agents::record::Record {
                    at: Some(s("2026-10-09T01:30:00.000Z")),
                    time_text: Some(crate::common::cells::Words(s("09:30"))),
                    at_ms: Some(1_791_509_400_000),
                    ..said_record("u1", "q")
                }),
                cwd: Some(s("/w")),
                byte_offset: 120,
                rid: Some(s("r1")),
            }),
            Frame::Line(crate::wire::Line {
                session_id: s("s1"),
                path: s("/p/s1.jsonl"),
                seq: 3,
                record: None,
                cwd: None,
                byte_offset: 120,
                rid: None,
            }),
        ],
        [
            Frame::SessionAdded(crate::wire::SessionAdded {
                sid: s("s1"),
                agent_kind: s("claude"),
                background: true,
                attachable: Some(true),
                cwd: Some(s("/w")),
                project_dir: Some(s("/w")),
                name: Some(s("n")),
                path: Some(s("/p/s1.jsonl")),
                face: crate::wire::SessionFace::of(crate::wire::Life::Live { activity: Some(crate::agents::SessionActivity::NeedsYou), host: None })
                    .under(Some(s("b"))),
                name_face: crate::history::search_rules::name_face(
                    &crate::history::search_rules::LabelFacts {
                        title: Some(s("t")),
                        project_dir: Some(s("/w")),
                        ..Default::default()
                    },
                ),
                hidden: crate::history::session_hidden::judge(
                    &crate::history::session_hidden::Facts {
                        bg: true,
                        rules: crate::history::session_hidden::Rules {
                            child_tab: cmd_wire::session::Tab::Hide,
                            bg_tab: cmd_wire::session::Tab::Hide,
                        },
                        ..Default::default()
                    },
                ),
                container: Some(SessionContainer::Tmux {
                    terminal: Some("tmux-3-7".into()),
                }),
                pid: 42,
            }),
            Frame::SessionAdded(crate::wire::SessionAdded {
                sid: s("s1"),
                agent_kind: "claude".to_string(),
                background: false,
                attachable: None,
                cwd: None,
                project_dir: None,
                name: None,
                path: None,
                face: crate::wire::SessionFace::of(crate::wire::Life::Live { activity: None, host: None }),
                name_face: crate::history::search_rules::name_face(&Default::default()),
                hidden: None,
                container: None,
                pid: 0,
            }),
        ],
        [
            Frame::SessionPrimed(crate::wire::SessionPrimed {
                sid: s("s1"),
                path: Some(s("/p/s1.jsonl")),
                lines: Some(9),
            }),
            Frame::SessionPrimed(crate::wire::SessionPrimed {
                sid: s("s1"),
                path: None,
                lines: None,
            }),
        ],
        [
            Frame::SessionStatus(crate::wire::SessionStatus {
                sid: s("s1"),
                face: crate::wire::SessionFace::of(crate::wire::Life::Live { activity: Some(crate::agents::SessionActivity::NeedsYou), host: None })
                    .under(Some(s("b"))),
                name_face: crate::history::search_rules::name_face(
                    &crate::history::search_rules::LabelFacts {
                        title: Some(s("t")),
                        project_dir: Some(s("/w")),
                        ..Default::default()
                    },
                ),
                waiting_for: Some(s("permission prompt")),
                path: Some(s("/r/s1.jsonl")),
            }),
            Frame::SessionStatus(crate::wire::SessionStatus {
                sid: s("s1"),
                face: crate::wire::SessionFace::of(crate::wire::Life::Live { activity: None, host: None }),
                name_face: crate::history::search_rules::name_face(&Default::default()),
                waiting_for: None,
                path: None,
            }),
        ],
        // 全格：被顶替（带顶替它的那条）、带号；少的那一形：可重连（不带号，关不掉 · 终端那两格的形只它有）。
        [
            Frame::session_left(s("s1"), Life::Ended, Some(s("s2")), Some(s("b"))),
            Frame::session_left(s("s1"), Life::Reconnectable, None, None),
        ],
        [Frame::overflow(0, true), Frame::overflow(7, false)],
        [
            Frame::Reply(crate::wire::Reply {
                id: s("q1"),
                ok: false,
                code: Some(s("bad_args")),
                said: Some(s("m")),
                detail: Some(s("码：bad_args")),
                data: Some(serde_json::json!({"k": 1})),
            }),
            Frame::Reply(crate::wire::Reply {
                id: s("q1"),
                ok: true,
                code: None,
                said: None,
                detail: None,
                data: None,
            }),
        ],
        both(Frame::Cancelled(crate::wire::Cancelled { id: s("q1") })),
        [
            Frame::changed(crate::wire::Changed::Plan {
                key: s("/w"),
                rev: Some(s("r1")),
                body: Some(cmd_wire::frame::PlanNeeds { needs: 2 }),
            }),
            Frame::changed(crate::wire::Changed::Accounts {}),
        ],
        both(Frame::SessionsReplayed(crate::wire::SessionsReplayed {})),
        both(Frame::SessionFileGone(crate::wire::SessionFileGone {
            session_id: s("s1"),
            path: s("/p/s1.jsonl"),
        })),
        both(Frame::SessionFileReread(crate::wire::SessionFileReread {
            session_id: s("s1"),
            path: s("/p/s1.jsonl"),
            why: RereadWhy::Truncated,
            lines: 7,
        })),
        [
            Frame::Transfer(crate::wire::Transfer {
                id: s("x1"),
                got: 4,
                total: 4,
                end: Some(TransferEnd::Done {
                    bytes: 4,
                    sha256: Some(s("ab")),
                }),
            }),
            Frame::Transfer(crate::wire::Transfer {
                id: s("x1"),
                got: 0,
                total: 4,
                end: None,
            }),
        ],
        both(Frame::Probe(crate::wire::Probe {
            ticket: s("t1"),
            cell: serde_json::json!({"reached": "ssh"}),
        })),
        [
            Frame::FilesBatch(crate::wire::FilesBatch {
                ticket: s("b1"),
                done: 1,
                total: 3,
                current: Some(cmd_wire::files::WirePath::from("a.txt")),
                bytes_done: Some(512),
                bytes_total: Some(2048),
            }),
            Frame::FilesBatch(crate::wire::FilesBatch {
                ticket: s("b1"),
                done: 3,
                total: 3,
                current: None,
                bytes_done: None,
                bytes_total: None,
            }),
        ],
        [
            Frame::LiveCard(LiveCard {
                sid: s("s1"),
                run: Some(s("a1")),
                card: 0,
                n: 0,
                op: LiveOp::Open {
                    rid: s("r1"),
                    phase: LivePhase::Awaiting,
                    clipped: true,
                    blocks: vec![
                        LiveBlock {
                            kind: BlockKind::Text,
                            md: Some(vec![cmd_wire::md::MdBlock::P {
                                k: vec![cmd_wire::md::Inl::S(s("hi"))],
                            }]),
                            tool: None,
                        },
                        LiveBlock {
                            kind: BlockKind::Tool,
                            md: None,
                            tool: Some(s("Bash")),
                        },
                    ],
                },
            }),
            Frame::LiveCard(LiveCard {
                sid: s("s1"),
                run: None,
                card: 0,
                n: 1,
                op: LiveOp::Awaiting,
            }),
        ],
        [
            Frame::SessionRuns(crate::wire::SessionRuns {
                sid: s("s1"),
                runs: vec![RunInfo {
                    run: s("a1"),
                    label: Some(s("scan")),
                    kind: Some(s("Explore")),
                    tool: Some(s("t1")),
                    parent: Some(s("a9")),
                    state: RunState::Failed,
                    status_text: crate::common::cells::Words(s("核心·失败")),
                    took_text: Some(crate::common::cells::Words(s("核心·用时 2s"))),
                    label_text: crate::common::cells::Words(s("scan")),
                    last: Some(RunDid::Tool { name: s("Bash") }),
                    last_text: Some(crate::common::cells::Words(s("调用 Bash"))),
                    waiting: Some(s("Bash")),
                    started_ms: Some(1_000),
                    started_text: Some(s("00:00 起")),
                    active_ms: Some(2_000),
                    ended_ms: Some(3_000),
                    why: Some(RunWhy::Reported),
                    error: Some(s("boom")),
                    calls: 7,
                    background: true,
                    facts: vec![
                        RunLine {
                            text: w("核心·用时 {dur}"),
                            from: Some(1_000),
                        },
                        RunLine {
                            text: w("核心·工具 ×7"),
                            from: None,
                        },
                    ],
                    explain: Some(RunExplain {
                        lead: Some(w("核心·不明")),
                        line: RunLine {
                            text: w("核心·失败说明"),
                            from: None,
                        },
                        note: Some(w("核心·补一句")),
                        refresh: true,
                    }),
                    end: Some(RunEndLine {
                        head: w("核心·失败"),
                        sub: RunLine {
                            text: w("核心·记录 {dur}"),
                            from: Some(2_000),
                        },
                    }),
                    needs: crate::observe::recognize::facts::dress_ask_with(
                        None,
                        Some(crate::agents::asks::specimen()),
                        1,
                    ),
                }],
                ended: vec![RunEnded {
                    run: s("a0"),
                    tool: s("t0"),
                    state: RunState::Failed,
                    status_text: crate::common::cells::Words(s("核心·失败")),
                }],
                groups: cmd_wire::frame::RunGroups {
                    running: vec![],
                    recent: vec![s("a1")],
                    older: vec![s("a2")],
                    older_text: None,
                    ..Default::default()
                },
                summary: Some(w("核心·agent 1")),
            }),
            Frame::SessionRuns(crate::wire::SessionRuns {
                sid: s("s1"),
                runs: vec![RunInfo {
                    run: s("a1"),
                    ..RunInfo::default()
                }],
                ended: vec![],
                groups: cmd_wire::frame::RunGroups::default(),
                summary: None,
            }),
        ],
        [
            Frame::SessionBranch(crate::wire::SessionBranch {
                sid: s("s1"),
                path: s("/p/s1.jsonl"),
                at: 0,
                off: vec![s("u2"), s("a2")],
                sum: crate::wire::branch_sum(&[s("u2"), s("a2")]),
            }),
            Frame::SessionBranch(crate::wire::SessionBranch {
                sid: s("s1"),
                path: s("/p/s1.jsonl"),
                at: 2,
                off: vec![],
                sum: crate::wire::branch_sum(&[s("u2"), s("a2")]),
            }),
        ],
        [
            Frame::TerminalScreen(crate::wire::TerminalScreen {
                ticket: s("t1"),
                seq: 2,
                view: serde_json::from_value(serde_json::json!({"screen": "00000000000000a1", "cols": 80, "rows": 1, "cursor": {"x": 0, "y": 0, "visible": true}, "lines": [{"text": "ok", "spans": [{"from": 0, "to": 2, "fg": "green"}]}], "scrollback_lines": 0, "capped": false, "captured_at": 1, "captured_at_text": "00:00:01"})).unwrap(),
                input: cmd_wire::session::TermInput::Shared,
            }),
            Frame::TerminalScreen(crate::wire::TerminalScreen {
                ticket: s("t1"),
                seq: 1,
                view: serde_json::from_value(serde_json::json!({"screen": "0000000000000000", "cols": 1, "rows": 0, "cursor": {"x": 0, "y": 0, "visible": false}, "lines": [], "scrollback_lines": 0, "capped": false, "captured_at": 0, "captured_at_text": "00:00:00"})).unwrap(),
                input: cmd_wire::session::TermInput::Shared,
            }),
        ],
        [
            Frame::TerminalFollowEnd(crate::wire::TerminalFollowEnd {
                ticket: s("t1"),
                why: crate::wire::FollowEnd::TooBig,
                said: s("that-sentence-1"),
            }),
            Frame::TerminalFollowEnd(crate::wire::TerminalFollowEnd {
                ticket: s("t1"),
                why: crate::wire::FollowEnd::Gone,
                said: s("that-sentence-2"),
            }),
        ],
        [
            Frame::Relayed(cmd_wire::frame::Relayed::Line {
                machine: s("m1"),
                line: s(r#"{"kind":"sessions_replayed"}"#),
            }),
            Frame::Relayed(cmd_wire::frame::Relayed::Ended { machine: s("m1") }),
        ],
    ]
}

/// `Frame` 的全部变体名（从源码的 `pub enum Frame` 里数，异源于上面的样本）→ 线上 kind（snake_case）。
fn frame_variant_kinds() -> std::collections::BTreeSet<String> {
    let src = include_str!("../../../src/common/cmd-wire/src/frame.rs");
    let beg = src.find("pub enum Frame").expect("找不到 Frame 枚举");
    let end = src[beg..].find("\n}\n").expect("找不到枚举结尾") + beg;
    src[beg..end]
        .lines()
        .filter_map(|l| {
            let t = l.trim_end();
            let head = t.strip_prefix("    ")?;
            if head.starts_with(' ') || !head.starts_with(|c: char| c.is_ascii_uppercase()) {
                return None;
            }
            if !(t.contains('{') || t.ends_with(',')) {
                return None;
            }
            let name: String = head
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            let mut kind = String::new();
            for (i, c) in name.chars().enumerate() {
                if c.is_ascii_uppercase() && i > 0 {
                    kind.push('_');
                }
                kind.push(c.to_ascii_lowercase());
            }
            Some(kind)
        })
        .collect()
}

/// ★ 会话流跨语言金样：金样 == 真序列化器此刻的产出（逐行字节）；金样的 kind 集合 == `Frame` 的全部变体。
///
/// 每一行都是单行 JSON、`\n` 收尾。
/// 重打是显式动作：`REGOLD_SESSION_STREAM=1` 跑本条写盘；不设就只比、不一致就红并打印差异。
#[test]
fn the_session_stream_golden_is_what_the_serializer_writes() {
    let mut produced = String::new();
    let mut kinds = std::collections::BTreeSet::new();
    for pair in golden_pairs() {
        let pair_kinds: Vec<String> = pair
            .iter()
            .map(|f| {
                let line = to_line(f).expect("serialize");
                let kind = parse_kind(&line);
                produced.push_str(&line);
                kind
            })
            .collect();
        assert_eq!(
            pair_kinds[0], pair_kinds[1],
            "一对样本不是同一种帧：{pair_kinds:?}"
        );
        assert!(
            kinds.insert(pair_kinds[0].clone()),
            "{} 出现了两对样本",
            pair_kinds[0]
        );
    }
    let variants = frame_variant_kinds();
    assert!(
        variants.len() >= 20,
        "只数出 {} 个 Frame 变体 —— 抽取坏了，下面的对拍会空转",
        variants.len()
    );
    assert_eq!(
        kinds, variants,
        "金样的 kind 集合与 `Frame` 的变体对不上 —— 新增的帧要在 `golden_pairs` 补「全格」「最少格」两行"
    );

    let path = guard_core::addr::repo_root().join(SESSION_STREAM_GOLDEN);
    if std::env::var_os("REGOLD_SESSION_STREAM").is_some() {
        std::fs::write(&path, &produced).expect("写金样");
    }
    let on_disk = std::fs::read_to_string(&path).expect("读金样");
    if on_disk != produced {
        let diff: Vec<String> = on_disk
            .lines()
            .zip(produced.lines())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| format!("第 {} 行\n  金样：{a}\n  此刻：{b}", i + 1))
            .collect();
        panic!(
            "会话流金样与序列化器此刻的产出不一致（金样 {} 行 · 此刻 {} 行）：\n{}\n\
             形状是有意改的 ⇒ `REGOLD_SESSION_STREAM=1` 重打这一条，连同手机端一起对。",
            on_disk.lines().count(),
            produced.lines().count(),
            diff.join("\n")
        );
    }
}

/// 叫这条会话什么那两格：名字（`session_label`）＋ 项目目录最后一截（不知道项目目录 ⇒ 不带）。
#[test]
fn the_name_face_carries_the_name_and_the_project() {
    use crate::history::search_rules::LabelFacts;
    let face =
        |f: LabelFacts| serde_json::to_value(crate::history::search_rules::name_face(&f)).unwrap();
    let proj = || Some("/w/proj/".to_string());
    let untitled = copy_core::copy_text("history.row.untitled", &[]);
    assert_eq!(
        face(LabelFacts {
            first: "修一下".into(),
            project_dir: proj(),
            ..Default::default()
        }),
        serde_json::json!({"label": "修一下", "label_project": "proj"})
    );
    assert_eq!(
        face(LabelFacts {
            project_dir: Some("C:\\w\\q".into()),
            ..Default::default()
        }),
        serde_json::json!({"label": untitled, "label_project": "q"})
    );
    assert_eq!(
        face(LabelFacts::default()),
        serde_json::json!({ "label": untitled })
    );
}

/// `session_added.pid` 的线上形：恒在、排在最后、是个整数。
#[test]
fn loc1b_session_added_carries_the_pid() {
    let line = to_line(&Frame::SessionAdded(crate::wire::SessionAdded {
        sid: "s".into(),
        agent_kind: "claude".to_string(),
        background: false,
        attachable: None,
        cwd: None,
        project_dir: None,
        name: None,
        path: None,
        face: crate::wire::SessionFace::of(crate::wire::Life::Live {
            activity: None,
            host: None,
        }),
        name_face: crate::history::search_rules::name_face(&Default::default()),
        hidden: None,
        container: None,
        pid: 4242,
    }))
    .unwrap();
    assert_eq!(
        line,
        "{\"kind\":\"session_added\",\"sid\":\"s\",\"agent_kind\":\"claude\",\"face\":{\"dot\":\"idle\",\"life\":{\"is\":\"live\"},\"settled\":false,\"name\":\"状态不明\",\"tip\":\"状态不明\",\"line\":\"状态不明\",\"tone\":\"plain\",\"order\":3,\"can\":{\"close\":{\"can\":\"no\",\"why\":\"运行中\"},\"resume\":{\"can\":\"no\",\"why\":\"运行中\"},\"term\":{\"can\":\"yes\",\"host\":\"direct\"},\"reattach\":{\"can\":\"no\",\"why\":\"运行中\"},\"model\":{\"can\":\"yes\"},\"permission\":{\"can\":\"yes\"},\"stop\":{\"can\":\"no\",\"why\":\"状态不明\"},\"say\":{\"can\":\"yes\",\"hint\":\"发消息…\"},\"end\":{\"can\":\"yes\"}}},\"label\":\"没有说过话的会话\",\"pid\":4242}\n"
    );
}

// ═══ 第二个前端调的一次性子命令：冻结（叫法）══════════════════════════════════════
//
// 手机只调这两条（其余都走常驻流上的帧命令）：起 / 找常驻后端 · 接常驻后端的小中继（那台是哪一版在 attach 之后的 hello 里）。

/// 第二个前端调的一次性子命令（都不带位置参数；`--resident-ensure` 认一个 `--replace`）。叫法一改它那一侧就叫不通了。
const SECOND_FRONTEND_SUBCOMMANDS: &[&str] = &["--resident-ensure", "--resident-attach"];

/// 每一条都还在子命令表里、还进一次性模式。
#[test]
fn the_subcommands_the_second_frontend_calls_stay_put() {
    for flag in SECOND_FRONTEND_SUBCOMMANDS {
        assert!(
            crate::stream::cli::is_subcommand(flag),
            "{flag} 不是子命令了"
        );
        assert!(
            crate::stream::cli::is_query_mode(&[flag.to_string()]),
            "{flag} 不进一次性模式了"
        );
    }
}

/// 〔G2〕会话状态带写好的字与语气（对所有出口一次补齐）：整份脸的 `name` · `tone` · `order`（活着的几态 · 读记录中 ·
/// 离开「活」之后的两种，后两种另带悬停那一句 `tip`）。字走文案表，出口照抄、不按码取字。
#[test]
fn session_frames_carry_the_state_written_and_toned() {
    use crate::agents::SessionActivity as A;
    use crate::wire::Life;
    let status = |a: Option<A>| {
        serde_json::to_value(Frame::<crate::agents::record::Record>::SessionStatus(
            crate::wire::SessionStatus {
                sid: "s".into(),
                face: crate::wire::SessionFace::of(Life::Live {
                    activity: a,
                    host: None,
                }),
                name_face: crate::history::search_rules::name_face(&Default::default()),
                waiting_for: None,
                path: None,
            },
        ))
        .unwrap()["face"]
            .clone()
    };
    for (a, key, tone) in [
        (A::Working, "backend.status.connected", "now"),
        (A::NeedsYou, "beSession.needs.unknown", "need"),
        (A::Idle, "beSession.activity.idle", "plain"),
        (
            A::BackgroundWork,
            "beSession.activity.backgroundWork",
            "busy",
        ),
    ] {
        let v = status(Some(a));
        assert_eq!(v["name"], copy_core::copy_text(key, &[]), "{a:?}");
        assert_eq!(v["tone"], tone, "{a:?}");
    }
    // 说不清在干什么（活着、那一家没说）⇒ 核心照实写「说不清」、语气 plain（不说一句没判出来的「运行中」）。
    let none = status(None);
    assert_eq!(
        none["name"],
        copy_core::copy_text("runs.state.unknown", &[])
    );
    assert_ne!(
        none["name"],
        copy_core::copy_text("backend.status.connected", &[]),
        "判不出却写了运行中"
    );
    assert_eq!(none["tone"], "plain");

    // V10 监控板组内的序由核心给（`order`，小的在前）：等人 > 在干活 > 后台任务运行中 > 闲着 ＝ 说不清（序跟着语气走）。
    let order = |a: Option<A>| status(a)["order"].as_u64().expect("order 必有");
    let ranked = [
        Some(A::NeedsYou),
        Some(A::Working),
        Some(A::BackgroundWork),
        Some(A::Idle),
    ];
    for w in ranked.windows(2) {
        assert!(order(w[0]) < order(w[1]), "{:?} 应排在 {:?} 前", w[0], w[1]);
    }
    assert_eq!(
        order(None),
        order(Some(A::Idle)),
        "说不清与闲着同一档（语气都是 plain）"
    );
    // 读记录中：同「说不清」那一档，字照实说。
    let reading =
        serde_json::to_value(crate::wire::SessionFace::of(Life::Reading { host: None })).unwrap();
    assert_eq!(reading["tone"], "plain");
    assert_eq!(reading["order"], none["order"]);
    assert!(
        reading["life"].get("activity").is_none(),
        "读记录中不说判出了哪一态"
    );

    for (f, name, hint) in [
        (
            Life::Reconnectable,
            "sessionState.reconnectable.name",
            "sessionState.reconnectable.tooltip",
        ),
        (
            Life::Ended,
            "sessionState.ended.name",
            "sessionState.ended.tooltip",
        ),
    ] {
        let v =
            &serde_json::to_value(Frame::session_left("s".into(), f, None, None)).unwrap()["face"];
        assert_eq!(v["name"], copy_core::copy_text(name, &[]), "{f:?}");
        assert_eq!(v["tip"], copy_core::copy_text(hint, &[]), "{f:?}");
        assert_eq!(v["tone"], "plain", "{f:?}");
    }
}
