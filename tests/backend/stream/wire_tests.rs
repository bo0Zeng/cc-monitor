//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md §10`（backend → client 线上契约）
//!
//! 核原文：`IPC-PROTOCOL.md §10` 逐字「每行恰好一个 UTF-8 JSON 对象，`\n` 结尾，对象内无裸 `\n`/`\r`」；同节帧表逐格写了各字段的
//! additive 规则与今天的取值 —— 本族判出方向帧的字节与那张表一致。出方向单写者那两条服务同一节的「每行恰好一个对象」与「首帧」；
//! 「SSH 流单写者」不单独立条。

/// ★★ **出方向帧只许有一个写者**〔audit-0805 08-08，Phase G 第 60 件，D6〕。
///
/// `stream/inbound/` 头注逐字写着「`writer_task`（**出方向帧的唯一出口**）」。
/// 那句话撑着 NDJSON 在线上的完整性：两个写者并发写同一个 stdout，
/// 帧就会**互相撕开**（半行 + 半行），而这条流**仓外 aterm 正在消费**（D6 契约冻结）。
///
/// ⇒ 而它只是散文。08-08 实测：在 `stream/inbound/` 加一个自己 `tokio::io::stdout()`
/// 并 `write_all` 的函数，**backend 292 条判据一条不红**。
///
/// # 人群与豁免
///
/// 人群 = **流式那条路**（`main.rs` / `stream/inbound/` 整个目录 / `wire.rs` / `listen.rs`）生产段里所有
/// `write_all(`。今天恰好三处，各自登记：
/// · `wire.rs::write_and_flush_hello` —— 握手前那一帧，写完才产出 `HelloFlushed`；
///   它按定义发生在 writer_task 起来**之前**，不存在并发。
/// · `main.rs::write_frame` —— writer_task 的出口本体。
/// · `listen.rs::write_line` —— attach 握手的应答行（`K-P1` 补进人群的那一员，见下）。
///
/// ⚠ `observe/*_query.rs` 那些 `stdout()` **不在人群里**：它们是一次性 CLI 子命令
/// （打完就退，没有 writer_task），与流式路不共享那个 stdout 的生命周期。
///
/// # ★★ `K-P1` 08-26：把 `listen.rs` 补进人群，**因为常驻正好落进一条自陈的缝里**
///
/// `relay/mod.rs` 头注逐字记着：「**谁在同一个进程里既跑流式又跑中转，今天没有任何判据挡着**
/// （那条「只有一个写者」的判据人群只有三个文件，**不含 `relay/`**）」。
/// 常驻之后「同一个进程既听口又跑流」正好是那一形 —— 而这一次那个新文件是**流式路自己的**。
/// ⇒ 补人群是**收紧**：`listen.rs` 一旦多长出第二处 `write_all(`，本条当场红。
/// ⚠ 如实说清它**没有**顺手治的那一格：`relay/` 仍然不在人群里
/// （那是另立的一件，本件不碰 `relay/`）。补的是「常驻自己那条路」，不是那条自陈的全部。
#[test]
fn the_outbound_stream_has_exactly_one_writer() {
    const STREAMING: &[(&str, &str)] = &[
        (
            "wire.rs",
            "write_and_flush_hello：握手帧，发生在 writer_task 起来之前",
        ),
        ("main.rs", "write_frame：writer_task 的出口本体"),
        (
            "listen.rs",
            "write_line：attach 握手的应答行（ok / refused）。\
                 它写的**不是**出方向帧，而且发生在 writer_task 起来之前 —— \
                 与 write_and_flush_hello 同一个性质：那一刻这条连接上还没有第二个写者。",
        ),
    ];
    let root = crate::guard_support::src_root();
    let verb = format!("write_{}(", "all");
    let mut found: Vec<(String, usize)> = Vec::new();
    // ⚠ 本条的人群里**必须有 `wire.rs`** —— 握手帧就写在这里。
    //   第一版（判据还住在 `wire.rs` 自己的 `#[cfg(test)]` 段里那会儿）直接用
    //   `scan_tree!`，于是反向锚点当场报「登记的写者 wire.rs 找不到」：
    //   **摘除自己这件事，在「我自己也是被测对象」时会反过来咬人。**
    // ⚠ **那一刀今天不生效**：判据搬来 `tests/backend/` 之后由
    //   `#[path]` 挂载 ⇒ `file!()` 是带 `..` 的折返路径 ⇒ 后缀比不命中
    //   ⇒ `wire.rs` 走普通遍历**本来就在**人群里，下面那句 `files.push` 补的是**第二份**。
    //   同一份被数两遍在这里**无害**（下面按文件名分别断「恰好 1 处」，不做跨文件累加），
    //   而它**刻意不删**：它把「被测那一份一定在人群里」钉成一件不依赖扫描面的事。
    let mut files: Vec<(std::path::PathBuf, String)> = guard_core::scan_tree!(&root, &["rs"]);
    files.push((
        std::path::PathBuf::from("wire.rs"),
        include_str!("../../../src/backend/stream/wire.rs").to_string(),
    ));
    for (path, src) in files {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap()
            .to_string();
        // 入方向是一个目录（`stream/inbound/`）：整个目录都在人群里，按仓内相对路径记名。
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let in_inbound = rel.starts_with("stream/inbound/");
        // 只看流式那条路（`wire.rs` · `main.rs` · `listen.rs` ＋ 入方向那个目录）；`observe/*_query.rs` 是一次性子命令，理由见头注。
        if !in_inbound && !matches!(name.as_str(), "wire.rs" | "main.rs" | "listen.rs") {
            continue;
        }
        let name = if in_inbound { rel } else { name };
        let n = guard_core::production_code(&src)
            .lines()
            .filter(|l| l.contains(verb.as_str()))
            .count();
        if n > 0 {
            found.push((name, n));
        }
    }
    found.sort();
    // 抽取器自检：一处都没扫到 ⇒ 下面整条空转。
    assert!(
        !found.is_empty(),
        "流式路上一处 `write_all(` 都没扫到 —— 抽取器坏了，本条此刻无效"
    );
    for (name, n) in &found {
        let known = STREAMING.iter().find(|(f, _)| f == name);
        let (_, why) = known.unwrap_or_else(|| {
            panic!(
                "`{name}` 在流式路上写出方向帧，却不是登记的那两个写者之一。\n\
                     ⚠ 两个写者并发写同一个 stdout ⇒ 帧互相撕开（半行 + 半行），\n\
                     而这条流**仓外 aterm 正在消费**（D6：暴露给第三方 = 契约冻结成本）。\n\
                     真要发帧，走 `writer_task` 那个出口。"
            )
        });
        assert_eq!(
            *n, 1,
            "`{name}` 里有 {n} 处 `write_all(`（应恰好 1）。登记说法：{why}\n\
                 多出来的那处就是第二个出口 —— 同一个文件里也一样会撕帧。"
        );
    }
    // 反向锚点：登记的两个写者必须都还在（少一个 = 流式路被改形了，得重判）。
    for (f, why) in STREAMING {
        assert!(
            found.iter().any(|(n, _)| n == f),
            "登记的写者 `{f}` 在流式路上找不到 `write_all(` 了（说法：{why}）—— \
                 结构变了就回来重判，别让本条在半个人群上绿着。"
        );
    }
}

/// ★★ **`HelloFlushed` 的构造面必须只有一个出口**〔audit-0805 08-08，Phase G 第 58 件〕。
///
/// 本文件头注逐字写着：见证「**只能由真的写出并 flush 了 Hello 的那条路才能产出**
/// （构造函数私有，唯一出口是 `write_and_flush_hello`）」，并据此**把一条旧机检整条删掉**
/// （原来那条按「两个字符串的字节位置」判 reader 顺序，被 D 审计用一次搬函数绕过了）。
///
/// ⇒ 于是整条「Hello 之前不许起 inbound」的保证，**全压在这个类型的构造面上**。
/// 08-08 实测：给它加一个 `pub fn assume() -> Self`，**两侧 291 + 989 条判据一条不红**。
/// 与 F45（monitor 侧 `BackendHello` / `ParkedWriter`）**同一形态在另一半**。
///
/// 钉三件（第三件是 F45 的变异逼出来的：编译器替你挡住的，正是没人写下来的）：
/// ① 生产段里 `HelloFlushed(` 的构造恰好一处；② `impl` 里除 `for_tests` 外没有别的
/// 关联函数，且 `for_tests` 必须带 `#[cfg(test)]`；③ 元组字段必须私有。
#[test]
fn the_hello_witness_has_exactly_one_way_to_exist() {
    let src = include_str!("../../../src/backend/stream/wire.rs");
    let prod = guard_core::production_code(src);

    // ① 构造点恰好一处。
    // ⚠ 运行时拼：写成字面量的话，**本条自己的诊断文案**里那个串也会被数进去
    //   （第一版就栽在这，报「出现了 2 次」）。判据扫自己所在的文件时，
    //   它写下的每一个例子都会变成语料 —— 这一族本仓已记过多次。
    let ctor_form = format!("HelloFlushed{}", "(()");
    // ⚠ 再收窄一层：`pub struct HelloFlushed(());` 这行**声明**也含同一个串。
    //   要数的是「构造表达式」，不是「这串字符」——匹配单位比事实大的第二次（F24 族）。
    let ctors = prod
        .lines()
        .filter(|l| l.contains(ctor_form.as_str()) && !l.contains("struct "))
        .count();
    assert_eq!(
        ctors, 1,
        "生产段里那个元组构造出现了 {ctors} 次（应恰好 1，在 `write_and_flush_hello` 里）。\n\
             多一处 = 多一条不经「真的写出并 flush 了 Hello」的产出路 —— \n\
             而旧的顺序机检**已经因为这条类型保证被删掉了**，没有第二层。"
    );
    assert!(
        prod.contains("fn write_and_flush_hello"),
        "生产段里没有 `write_and_flush_hello` —— 抽取器坏了，本条此刻无效"
    );

    // ② `impl HelloFlushed` 里的关联函数：只许 `for_tests`，且必须 cfg(test)。
    let at = src
        .find("impl HelloFlushed {")
        .expect("找不到 `impl HelloFlushed` —— 抽取器坏了");
    let mut fns = Vec::new();
    let mut cfg_test_seen = false;
    for (i, line) in src[at..].lines().enumerate() {
        if i > 0 && !line.is_empty() && !line.starts_with(char::is_whitespace) {
            break;
        }
        if line.trim() == concat!("#[cfg(te", "st)]") {
            cfg_test_seen = true;
        }
        if let Some(rest) = line.trim().strip_prefix("pub fn ") {
            let n: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            fns.push((n, cfg_test_seen));
            cfg_test_seen = false;
        }
    }
    assert_eq!(
        fns.len(),
        1,
        "`impl HelloFlushed` 里有 {} 个 `pub fn`（应恰好 1 个 `for_tests`）：{fns:?}\n\
             多出来的那个就是第二条产出路 —— 见证一旦能凭空造，`inbound::spawn` 那道\n\
             「拿不到见证就调不了」的编译期门就形同虚设。",
        fns.len()
    );
    assert_eq!(
        fns[0].0, "for_tests",
        "唯一那个 `pub fn` 不叫 `for_tests`：{fns:?}"
    );
    assert!(
        fns[0].1,
        "`for_tests` 上没有 `#[cfg(test)]` —— 那它在 release 里也存在，\
             「生产路径拿不到见证」这句话当场不成立"
    );

    // ③ 字段必须私有（F45 那轮的教训：编译器替你挡住的，正是没人写下来的）。
    let decl = prod
        .lines()
        .find(|l| l.contains("struct HelloFlushed"))
        .expect("找不到结构体声明 —— 抽取器坏了");
    assert!(
        !decl.contains("pub ("),
        "`HelloFlushed` 的元组字段被改成了 `pub`（{decl:?}）—— \
             那样任何模块都能 `HelloFlushed(())` 凭空造见证，连函数都不用走。"
    );
}
use super::*;
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
/// `tap` 的 `ev` 与 `end` 恰有一个：最少格留 `end`（两格都缺不是一帧合法的 `tap`）。
fn golden_pairs() -> Vec<[Frame; 2]> {
    use crate::agents::{RunDid, StreamEv};
    use crate::stream::wire::{
        AgentHome, LostFrame, RereadWhy, RunEnded, RunInfo, RunState, RunWhy, SessionContainer,
        SessionFate, TapEnd, TerminalHost, TransferEnd, Unavailable,
    };
    let s = |v: &str| v.to_string();
    let both = |f: Frame| [f.clone(), f];
    vec![
        [
            Frame::Hello {
                v: 1,
                build_id: s("b1"),
                host_arch: s("x86_64"),
                claude_dir: s("/home/u/.claude"),
                homes: vec![AgentHome {
                    agent_kind: s("claude"),
                    path: s("/home/u/.claude"),
                }],
                capabilities: vec![s("bg"), s("tail-only")],
                emits: vec![s("line")],
                commands: vec![s("ping")],
                unavailable: vec![Unavailable {
                    command: s("tmux-list"),
                    code: s("no_tmux"),
                }],
                host_env: [(s("CCM_RELAY_PORT"), s("47001"))].into_iter().collect(),
                uncancellable: vec![s("ping")],
            },
            Frame::Hello {
                v: 1,
                build_id: s("b1"),
                host_arch: s("x86_64"),
                claude_dir: s("/home/u/.claude"),
                homes: vec![],
                capabilities: vec![],
                emits: vec![],
                commands: vec![],
                unavailable: vec![],
                host_env: Default::default(),
                uncancellable: vec![],
            },
        ],
        [
            Frame::Line {
                session_id: s("s1"),
                path: s("/p/s1.jsonl"),
                seq: 3,
                record: Some(crate::agents::record::Record {
                    at: Some(s("2026-10-09T01:30:00.000Z")),
                    time_text: Some(crate::common::cells::Words(s("09:30"))),
                    ..said_record("u1", "q")
                }),
                cwd: Some(s("/w")),
                byte_offset: 120,
                rid: Some(s("r1")),
                raw: Some(s("{}")),
            },
            Frame::Line {
                session_id: s("s1"),
                path: s("/p/s1.jsonl"),
                seq: 3,
                record: None,
                cwd: None,
                byte_offset: 120,
                rid: None,
                raw: None,
            },
        ],
        [
            Frame::SessionAdded {
                sid: s("s1"),
                agent_kind: Some(s("claude")),
                liveness_confidence: Some(s("pidfile")),
                background: true,
                attachable: Some(true),
                cwd: Some(s("/w")),
                project_dir: Some(s("/w")),
                name: Some(s("n")),
                path: Some(s("/p/s1.jsonl")),
                lines: Some(9),
                activity: Some(crate::agents::SessionActivity::NeedsYou),
                activity_text: crate::stream::wire::activity_cells(Some(
                    crate::agents::SessionActivity::NeedsYou,
                ))
                .0,
                activity_tone: crate::stream::wire::activity_cells(Some(
                    crate::agents::SessionActivity::NeedsYou,
                ))
                .1,
                waiting_for: Some(s("permission prompt")),
                container: Some(SessionContainer::Hosted {
                    host: TerminalHost::Tmux,
                    terminal: Some("tmux-3-7".into()),
                }),
                pid: Some(42),
            },
            Frame::SessionAdded {
                sid: s("s1"),
                agent_kind: None,
                liveness_confidence: None,
                background: false,
                attachable: None,
                cwd: None,
                project_dir: None,
                name: None,
                path: None,
                lines: None,
                activity: None,
                activity_text: crate::stream::wire::activity_cells(None).0,
                activity_tone: crate::stream::wire::activity_cells(None).1,
                waiting_for: None,
                container: None,
                pid: None,
            },
        ],
        [
            Frame::SessionStatus {
                sid: s("s1"),
                activity: Some(crate::agents::SessionActivity::NeedsYou),
                activity_text: crate::stream::wire::activity_cells(Some(
                    crate::agents::SessionActivity::NeedsYou,
                ))
                .0,
                activity_tone: crate::stream::wire::activity_cells(Some(
                    crate::agents::SessionActivity::NeedsYou,
                ))
                .1,
                waiting_for: Some(s("permission prompt")),
                liveness_confidence: Some(s("pidfile")),
            },
            Frame::SessionStatus {
                sid: s("s1"),
                activity: None,
                activity_text: crate::stream::wire::activity_cells(None).0,
                activity_tone: crate::stream::wire::activity_cells(None).1,
                waiting_for: None,
                liveness_confidence: None,
            },
        ],
        both(Frame::session_state(s("s1"), SessionFate::Reconnectable)),
        [
            Frame::SessionRemoved {
                sid: s("s1"),
                cause: RemovalCause::Superseded,
            },
            Frame::SessionRemoved {
                sid: s("s1"),
                cause: RemovalCause::Gone,
            },
        ],
        both(Frame::TurnEnd {
            session_id: s("s1"),
            uuid: s("u1"),
        }),
        [
            Frame::Overflow {
                dropped: 7,
                lost: vec![LostFrame {
                    kind: "session_added",
                    subject: Some(s("s1")),
                }],
                lost_truncated: true,
            },
            Frame::Overflow {
                dropped: 7,
                lost: vec![],
                lost_truncated: false,
            },
        ],
        [
            Frame::Reply {
                id: s("q1"),
                ok: false,
                code: Some(s("bad_args")),
                message: Some(s("m")),
                detail: Some(s("码：bad_args")),
                data: Some(serde_json::json!({"k": 1})),
            },
            Frame::Reply {
                id: s("q1"),
                ok: true,
                code: None,
                message: None,
                detail: None,
                data: None,
            },
        ],
        both(Frame::Cancelled { id: s("q1") }),
        both(Frame::AccountsChanged),
        both(Frame::ProfilesChanged),
        both(Frame::QuotaChanged),
        both(Frame::RotationChanged { sid: s("s1") }),
        both(Frame::RotationRulesChanged),
        both(Frame::PlanChanged {
            workspace: s("/w"),
            rev: s("r1"),
            needs: 2,
        }),
        both(Frame::TasksChanged { sid: s("s1") }),
        both(Frame::SessionsReplayed),
        both(Frame::SessionFileGone {
            session_id: s("s1"),
            path: s("/p/s1.jsonl"),
        }),
        both(Frame::SessionFileReread {
            session_id: s("s1"),
            path: s("/p/s1.jsonl"),
            why: RereadWhy::Truncated,
        }),
        both(Frame::LinkData {
            link: s("L1"),
            data: s("aGkK"),
        }),
        [
            Frame::LinkEnd {
                link: s("L1"),
                error: Some(s("e")),
            },
            Frame::LinkEnd {
                link: s("L1"),
                error: None,
            },
        ],
        [
            Frame::Transfer {
                id: s("x1"),
                got: 4,
                total: 4,
                end: Some(TransferEnd::Done {
                    bytes: 4,
                    sha256: Some(s("ab")),
                }),
            },
            Frame::Transfer {
                id: s("x1"),
                got: 0,
                total: 4,
                end: None,
            },
        ],
        both(Frame::Probe {
            ticket: s("t1"),
            cell: serde_json::json!({"reached": "ssh"}),
        }),
        [
            Frame::Tap {
                stream: s("s1"),
                run: Some(s("a1")),
                resp: 0,
                n: 1,
                ev: Some(StreamEv::Text { i: 0, s: s("hi") }),
                end: Some(TapEnd::Done),
            },
            Frame::Tap {
                stream: s("s1"),
                run: None,
                resp: 0,
                n: 1,
                ev: None,
                end: Some(TapEnd::Done),
            },
        ],
        [
            Frame::SessionRuns {
                sid: s("s1"),
                runs: vec![RunInfo {
                    run: s("a1"),
                    label: Some(s("scan")),
                    kind: Some(s("Explore")),
                    tool: Some(s("t1")),
                    parent: Some(s("a9")),
                    state: RunState::Failed,
                    last: Some(RunDid::Tool { name: s("Bash") }),
                    waiting: Some(s("Bash")),
                    started_ms: Some(1_000),
                    started_text: Some(s("00:00")),
                    active_ms: Some(2_000),
                    ended_ms: Some(3_000),
                    why: Some(RunWhy::Reported),
                    error: Some(s("boom")),
                    calls: 7,
                    background: true,
                }],
                ended: vec![RunEnded {
                    run: s("a0"),
                    tool: s("t0"),
                    state: RunState::Failed,
                }],
            },
            Frame::SessionRuns {
                sid: s("s1"),
                runs: vec![RunInfo {
                    run: s("a1"),
                    ..RunInfo::default()
                }],
                ended: vec![],
            },
        ],
        [
            Frame::SessionBranch {
                sid: s("s1"),
                path: s("/p/s1.jsonl"),
                off: vec![s("u2"), s("a2")],
            },
            Frame::SessionBranch {
                sid: s("s1"),
                path: s("/p/s1.jsonl"),
                off: vec![],
            },
        ],
        [
            Frame::TerminalScreen {
                ticket: s("t1"),
                seq: 2,
                view: serde_json::json!({"screen": "00000000000000a1", "cols": 80, "rows": 1, "cursor": {"x": 0, "y": 0, "visible": true}, "lines": [{"text": "ok", "spans": [{"from": 0, "to": 2, "fg": "green"}]}], "scrollback_lines": 0, "capped": false, "captured_at": 1, "captured_at_text": "00:00:01"}),
            },
            Frame::TerminalScreen {
                ticket: s("t1"),
                seq: 1,
                view: serde_json::json!({}),
            },
        ],
        [
            Frame::TerminalFollowEnd {
                ticket: s("t1"),
                why: crate::stream::wire::FollowEnd::TooBig,
                said: s("that-sentence-1"),
            },
            Frame::TerminalFollowEnd {
                ticket: s("t1"),
                why: crate::stream::wire::FollowEnd::Gone,
                said: s("that-sentence-2"),
            },
        ],
    ]
}

/// `Frame` 的全部变体名（从源码的 `pub enum Frame` 里数，异源于上面的样本）→ 线上 kind（snake_case）。
fn frame_variant_kinds() -> std::collections::BTreeSet<String> {
    let src = include_str!("../../../src/backend/stream/wire.rs");
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
/// 每一行都是单行 JSON、`\n` 收尾、kind 与 `loss_identity` 报的一致。
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
                assert_eq!(
                    kind,
                    f.loss_identity().kind,
                    "kind 标签与 loss_identity 不一致"
                );
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

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(SESSION_STREAM_GOLDEN);
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

/// ★ W1：链路两帧 ＋ `accounts_changed` 的**逐字节**金标准。`link_end` 的 `error` 缺席时不上线（正常收尾）。
#[test]
fn link_frames_have_exactly_these_bytes() {
    let cases = [
        (
            Frame::LinkData {
                link: "m1.0-3".into(),
                data: b64_encode(b"hi\n"),
            },
            "{\"kind\":\"link_data\",\"link\":\"m1.0-3\",\"data\":\"aGkK\"}\n",
        ),
        (
            Frame::LinkEnd {
                link: "m1.0-3".into(),
                error: None,
            },
            "{\"kind\":\"link_end\",\"link\":\"m1.0-3\"}\n",
        ),
        (
            Frame::LinkEnd {
                link: "m1.0-3".into(),
                error: Some("读链路下行失败".into()),
            },
            "{\"kind\":\"link_end\",\"link\":\"m1.0-3\",\"error\":\"读链路下行失败\"}\n",
        ),
        (Frame::AccountsChanged, "{\"kind\":\"accounts_changed\"}\n"),
        (Frame::ProfilesChanged, "{\"kind\":\"profiles_changed\"}\n"),
        (Frame::QuotaChanged, "{\"kind\":\"quota_changed\"}\n"),
        (
            Frame::RotationChanged { sid: "s1".into() },
            "{\"kind\":\"rotation_changed\",\"sid\":\"s1\"}\n",
        ),
        (
            Frame::RotationRulesChanged,
            "{\"kind\":\"rotation_rules_changed\"}\n",
        ),
        (
            Frame::TasksChanged { sid: "s1".into() },
            "{\"kind\":\"tasks_changed\",\"sid\":\"s1\"}\n",
        ),
        (
            Frame::PlanChanged {
                workspace: "/w".into(),
                rev: "0123abcd".into(),
                needs: 2,
            },
            "{\"kind\":\"plan_changed\",\"workspace\":\"/w\",\"rev\":\"0123abcd\",\"needs\":2}\n",
        ),
    ];
    for (f, want) in cases {
        assert_eq!(to_line(&f).unwrap(), want);
    }
}

/// ★ B8：`transfer` 帧的**逐字节**金标准（四形：进行中 · 传完 · 失败 · 撤）；`end` 缺席时不上线；
/// 它丢了不可恢复（终局丢了，看的人永远等下去）。
#[test]
fn transfer_frames_have_exactly_these_bytes() {
    let f = |end: Option<crate::stream::wire::TransferEnd>| Frame::Transfer {
        id: "xfer-7".into(),
        got: 262144,
        total: 1000000,
        end,
    };
    let cases = [
        (
            f(None),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000}\n",
        ),
        (
            f(Some(crate::stream::wire::TransferEnd::Done {
                bytes: 1000000,
                sha256: None,
            })),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"done\",\"bytes\":1000000}}\n",
        ),
        // 上传那一路的传完带整份摘要（提交时的对拍依据）；下载那一路没有 ⇒ 上一格原样不上线。
        (
            f(Some(crate::stream::wire::TransferEnd::Done {
                bytes: 1000000,
                sha256: Some("ab".repeat(32)),
            })),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"done\",\"bytes\":1000000,\"sha256\":\"abababababababababababababababababababababababababababababababab\"}}\n",
        ),
        (
            f(Some(crate::stream::wire::TransferEnd::Failed { why: "写暂存件失败".into(), code: None, detail: None })),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"failed\",\"why\":\"写暂存件失败\"}}\n",
        ),
        // 带码的那一形（今天只有 SFTP 起始目录不是后端 home 那一码）。
        (
            f(Some(crate::stream::wire::TransferEnd::Failed { why: "w".into(), code: Some("sftp_home_mismatch".into()), detail: None })),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"failed\",\"why\":\"w\",\"code\":\"sftp_home_mismatch\"}}\n",
        ),
        // 带复制详情的那一形（句子只带原因词，原话在 `detail` 里）。
        (
            f(Some(crate::stream::wire::TransferEnd::Failed { why: "w".into(), code: None, detail: Some("d".into()) })),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"failed\",\"why\":\"w\",\"detail\":\"d\"}}\n",
        ),
        (
            f(Some(crate::stream::wire::TransferEnd::Cancelled)),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"cancelled\"}}\n",
        ),
    ];
    for (frame, want) in cases {
        assert_eq!(to_line(&frame).unwrap(), want);
        assert!(!frame.loss_is_recoverable(), "传输帧被判成丢了可恢复");
    }
}

/// `tap` 的逐字节线上形状（期望是手写字面量；monitor 侧 `parse_frame` 拿同样的串核自己）。
/// `ev` 是归一事件（后端按上游协议折好的，界面不认任何一家的事件名）；`ev` 与 `end` 恰有一个；`run` 缺 ＝ 主运行。可丢（SSE 只保快）。
#[test]
fn tap_frames_have_exactly_these_bytes() {
    use crate::agents::{BlockKind, StreamEv};
    let tap = |run: Option<&str>,
               n: u64,
               ev: Option<StreamEv>,
               end: Option<crate::stream::wire::TapEnd>| Frame::Tap {
        stream: "0b6c1f7e-sid".into(),
        run: run.map(str::to_string),
        resp: 12,
        n,
        ev,
        end,
    };
    let cases = [
        (
            tap(None, 0, Some(StreamEv::Start { rid: "r-1".into() }), None),
            "{\"kind\":\"tap\",\"stream\":\"0b6c1f7e-sid\",\"resp\":12,\"n\":0,\"ev\":{\"t\":\"start\",\"rid\":\"r-1\"}}\n",
        ),
        (
            tap(
                Some("a1"),
                1,
                Some(StreamEv::Block { i: 0, kind: BlockKind::Tool, tool: Some("Bash".into()) }),
                None,
            ),
            "{\"kind\":\"tap\",\"stream\":\"0b6c1f7e-sid\",\"run\":\"a1\",\"resp\":12,\"n\":1,\"ev\":{\"t\":\"block\",\"i\":0,\"kind\":\"tool\",\"tool\":\"Bash\"}}\n",
        ),
        (
            tap(None, 2, Some(StreamEv::Text { i: 0, s: "hi".into() }), None),
            "{\"kind\":\"tap\",\"stream\":\"0b6c1f7e-sid\",\"resp\":12,\"n\":2,\"ev\":{\"t\":\"text\",\"i\":0,\"s\":\"hi\"}}\n",
        ),
        (
            tap(None, 3, Some(StreamEv::Stop { ok: false }), None),
            "{\"kind\":\"tap\",\"stream\":\"0b6c1f7e-sid\",\"resp\":12,\"n\":3,\"ev\":{\"t\":\"stop\",\"ok\":false}}\n",
        ),
        (
            tap(None, 9, None, Some(crate::stream::wire::TapEnd::Done)),
            "{\"kind\":\"tap\",\"stream\":\"0b6c1f7e-sid\",\"resp\":12,\"n\":9,\"end\":\"done\"}\n",
        ),
        (
            tap(None, 9, None, Some(crate::stream::wire::TapEnd::Broken)),
            "{\"kind\":\"tap\",\"stream\":\"0b6c1f7e-sid\",\"resp\":12,\"n\":9,\"end\":\"broken\"}\n",
        ),
    ];
    for (frame, want) in cases {
        assert_eq!(to_line(&frame).unwrap(), want);
        assert!(
            frame.loss_is_recoverable(),
            "tap 帧可丢（jsonl 保对），却被判成不可恢复"
        );
    }
}

/// 运行表那一帧的逐字节线上形状（monitor 侧 `parse_frame` 拿同样的串核自己）。
#[test]
fn session_runs_frames_have_exactly_these_bytes() {
    use crate::agents::RunDid;
    use crate::stream::wire::{RunEnded, RunInfo, RunState, RunWhy};
    let f = Frame::SessionRuns {
        sid: "s1".into(),
        runs: vec![
            RunInfo {
                run: "a1".into(),
                label: Some("scan".into()),
                kind: Some("Explore".into()),
                tool: Some("t1".into()),
                parent: Some("a0".into()),
                state: RunState::Running,
                last: Some(RunDid::Tool {
                    name: "Bash".into(),
                }),
                waiting: Some("Bash".into()),
                started_ms: Some(1_000),
                active_ms: Some(2_000),
                calls: 3,
                background: true,
                ..RunInfo::default()
            },
            RunInfo {
                run: "a2".into(),
                state: RunState::Failed,
                last: Some(RunDid::Say),
                started_ms: Some(1_000),
                active_ms: Some(2_000),
                ended_ms: Some(3_000),
                why: Some(RunWhy::Reported),
                error: Some("boom".into()),
                ..RunInfo::default()
            },
            RunInfo {
                run: "a3".into(),
                state: RunState::Unknown,
                why: Some(RunWhy::Quiet),
                ..RunInfo::default()
            },
        ],
        ended: vec![RunEnded {
            run: "a0".into(),
            tool: "t0".into(),
            state: RunState::Failed,
        }],
    };
    assert_eq!(
        to_line(&f).unwrap(),
        "{\"kind\":\"session_runs\",\"sid\":\"s1\",\"runs\":[{\"run\":\"a1\",\"label\":\"scan\",\"kind\":\"Explore\",\"tool\":\"t1\",\"parent\":\"a0\",\"state\":\"running\",\"last\":{\"t\":\"tool\",\"name\":\"Bash\"},\"waiting\":\"Bash\",\"started_ms\":1000,\"active_ms\":2000,\"calls\":3,\"background\":true},{\"run\":\"a2\",\"state\":\"failed\",\"last\":{\"t\":\"say\"},\"started_ms\":1000,\"active_ms\":2000,\"ended_ms\":3000,\"why\":\"reported\",\"error\":\"boom\"},{\"run\":\"a3\",\"state\":\"unknown\",\"why\":\"quiet\"}],\"ended\":[{\"run\":\"a0\",\"tool\":\"t0\",\"state\":\"failed\"}]}\n"
    );
    assert!(
        !f.loss_is_recoverable(),
        "运行表丢了别处补不回来（要等下一次变）"
    );
}

/// base64 编解码对 **RFC 4648 §10** 的七条标准向量（异源 = RFC；monitor 侧那一份拿同一组向量核自己）。
#[test]
fn b64_matches_the_rfc_4648_test_vectors() {
    let vectors = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ];
    for (plain, enc) in vectors {
        assert_eq!(b64_encode(plain.as_bytes()), enc, "编 {plain:?}");
        assert_eq!(b64_decode(enc).unwrap(), plain.as_bytes(), "解 {enc:?}");
    }
    // 字母表最后两位（`+` `/`，RFC 4648 §4 表 1 的 62 / 63）：§10 那七条向量一个都用不到它们 ——
    // 死值验 K20 首跑实测：把字母表尾巴换成 URL 安全版（`-_`）七条照样绿。0xfb 0xff ⇒ 62 · 63 · 60。
    assert_eq!(b64_encode(&[0xfb, 0xff]), "+/8=");
    assert_eq!(b64_decode("+/8=").unwrap(), [0xfb, 0xff]);
    // 全 256 个字节值来回一趟。
    let all: Vec<u8> = (0..=255u8).collect();
    assert_eq!(b64_decode(&b64_encode(&all)).unwrap(), all);
    // 坏形一律拒（不猜）。
    for bad in ["A", "AA=", "A===", "Zg==Zg==", "Zm9v!A==", "===="] {
        assert!(b64_decode(bad).is_err(), "{bad:?} 该被拒");
    }
    // 每个字节值单放在一组的末位：字母表里的 64 个认、`=` 当补位，别的一律拒。
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for c in 0..=255u8 {
        let quad = String::from_utf8_lossy(&[b'A', b'A', b'A', c]).into_owned();
        let ok = b64_decode(&quad).is_ok();
        assert_eq!(ok, alphabet.contains(&c) || c == b'=', "字节 {c:#04x}");
    }
}

/// backend-09：TurnEnd 上线形——`{"kind":"turn_end","session_id","uuid"}`，**无 byte_offset**
/// （只 Line 带）。uuid = 客户端 dedup 键。
#[test]
fn turn_end_frame_serializes_with_session_and_uuid_only() {
    let line = to_line(&Frame::TurnEnd {
        session_id: "sid-9".into(),
        uuid: "u-abc".into(),
    })
    .expect("serialize");
    let v: Value = serde_json::from_str(line.strip_suffix('\n').unwrap()).expect("json");
    assert_eq!(v["kind"], "turn_end");
    assert_eq!(v["session_id"], "sid-9");
    assert_eq!(v["uuid"], "u-abc");
    assert!(v.get("byte_offset").is_none(), "TurnEnd 不带 byte_offset");
}

#[test]
fn overflow_frame_serializes_with_dropped_count() {
    let line = to_line(&Frame::Overflow {
        dropped: 42,
        lost: Vec::new(),
        lost_truncated: false,
    })
    .expect("serialize");
    let body = line.strip_suffix('\n').unwrap();
    let v: Value = serde_json::from_str(body).expect("valid json");
    assert_eq!(v["kind"], "overflow");
    assert_eq!(v["dropped"], 42);
}

// `tmux_sessions` 帧单行转义那条随帧删了（tmux 原文只在进程内喂会话账本）。

/// F66（#58③）wire 契约：hello 的 `capabilities`。
/// ① 非空 → 序列化为数组（monitor 据此发 flag）。
/// ② 空集 → `skip_serializing_if` 省略字段（additive：等价旧 hello，旧 monitor
///    收到即空集缺省——向后兼容的关键）。
fn hello(caps: Vec<String>) -> Frame {
    hello_with(caps, vec![])
}

fn hello_with(caps: Vec<String>, emits: Vec<String>) -> Frame {
    Frame::Hello {
        v: 1,
        build_id: "b".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/c".into(),
        homes: vec![],
        capabilities: caps,
        emits,
        commands: vec![],
        unavailable: vec![],
        host_env: Default::default(),
        uncancellable: vec![],
    }
}

#[test]
fn hello_capabilities_serializes_when_present_and_omits_when_empty() {
    // ① 非空 → 数组在线上
    let line = to_line(&hello(vec!["bg".into(), "tail-only".into()])).expect("serialize");
    let v: Value = serde_json::from_str(line.strip_suffix('\n').unwrap()).expect("json");
    assert_eq!(v["kind"], "hello");
    assert_eq!(v["capabilities"], serde_json::json!(["bg", "tail-only"]));
    // ② 空集 → 字段省略（旧 hello 等价形态，旧 monitor 忽略缺失 = 空集缺省）
    let line = to_line(&hello(vec![])).expect("serialize");
    let v: Value = serde_json::from_str(line.strip_suffix('\n').unwrap()).expect("json");
    assert!(
        v.get("capabilities").is_none(),
        "空 capabilities 必须被 skip（additive 向后兼容）"
    );
}

/// phase②：Hello.emits 与 capabilities 同 additive 规律、且**独立正交**（一个非空一个空时
/// 各自序列化/省略互不牵连）。aterm 门控读 emits 判「backend 发不发某帧」。
#[test]
fn hello_emits_serializes_orthogonally_to_capabilities() {
    // emits 非空、capabilities 空 → 只 emits 在线上、capabilities 省略。
    let line = to_line(&hello_with(
        vec![],
        vec!["session_status".into(), "turn_end".into()],
    ))
    .expect("serialize");
    let v: Value = serde_json::from_str(line.strip_suffix('\n').unwrap()).expect("json");
    assert_eq!(
        v["emits"],
        serde_json::json!(["session_status", "turn_end"])
    );
    assert!(
        v.get("capabilities").is_none(),
        "capabilities 空 → 省略（不受 emits 非空牵连）"
    );
    // 空 emits → 字段省略（additive：旧 client 忽略缺失 = 不依赖任何 α 专属帧）。
    let line = to_line(&hello_with(vec!["bg".into()], vec![])).expect("serialize");
    let v: Value = serde_json::from_str(line.strip_suffix('\n').unwrap()).expect("json");
    assert!(v.get("emits").is_none(), "空 emits 必须被 skip（additive）");
}

#[test]
fn seq_counter_is_monotonic_per_path_and_independent_across_paths() {
    let mut c = SeqCounter::new();
    assert_eq!(c.next("/a"), 0);
    assert_eq!(c.next("/a"), 1);
    assert_eq!(c.next("/a"), 2);

    // A different path starts its own sequence at 0.
    assert_eq!(c.next("/b"), 0);
    assert_eq!(c.next("/b"), 1);

    // /a is unaffected and keeps climbing.
    assert_eq!(c.next("/a"), 3);

    // Default impl behaves the same.
    let mut d = SeqCounter::default();
    assert_eq!(d.next("/x"), 0);
    assert_eq!(d.next("/x"), 1);
}

/// 一条最少格的 `said` 通用记录（结构占位）。
fn said_record(id: &str, text: &str) -> crate::agents::record::Record {
    use crate::agents::record::{Block, Body, Record};
    Record {
        agent: "claude".into(),
        id: id.into(),
        at: None,
        time_text: None,
        body: Body::Said {
            who: crate::agents::UserText {
                speaker: crate::agents::Speaker::Human,
                text: text.into(),
                pasted: Vec::new(),
            },
            blocks: vec![Block::Text { text: text.into() }],
            results: Default::default(),
            cwd: None,
        },
    }
}

#[test]
fn line_with_quotes_backslashes_and_newline_roundtrips() {
    let raw = "before\"quote\\backslash\nafter-newline";
    let frame = Frame::Line {
        session_id: "sid".into(),
        path: "/some/path.jsonl".into(),
        seq: 42,
        record: Some(said_record("u1", raw)),
        cwd: None,
        byte_offset: 99,
        rid: None,
        raw: None,
    };

    let line = to_line(&frame).expect("serialize");
    assert!(line.ends_with('\n'));

    // Minus the single trailing terminator, there must be NO bare newline:
    // the embedded newline in the record is escaped by serde_json as "\n".
    let body = &line[..line.len() - 1];
    assert!(
        !body.contains('\n'),
        "embedded newline must be escaped, got bare newline in: {body:?}"
    );

    // Parses back and the raw field is recovered byte-for-byte.
    let v: Value = serde_json::from_str(body).expect("parse");
    assert_eq!(v["kind"], "line");
    assert_eq!(v["record"]["blocks"][0]["text"], raw);
    assert_eq!(v["seq"], 42);
}

/// ★ `S4` 的红线**落到生产路径上**：`main.rs` 今天必须给 `homes` 传空表。
///
/// 下面那条 `dg3_codex_fields_skipped_when_absent_claude_byte_equivalent` 钉的是
/// 「**给**空表就得到旧字节」—— 它管不了「生产那边到底给没给空表」。
/// 有人在 `main.rs` 里往 `homes` 填一项，那条照样全绿，而线上字节已经变了。
/// 本条补的正是那半：**谁在调它、生产路径上有没有第二条绕过去的路**。
///
/// 它会在 `S5`/DG1 接线那天**故意变红**，那是设计出来的：填 `homes` 是一次
/// 跨仓契约变更（仓外 aterm 的 hello fixture 按精确字节对），
/// 必须有人当场重新裁一次，而不是顺手改过去。
#[test]
fn production_hello_leaves_homes_empty_so_claude_bytes_stay_frozen() {
    let prod = crate::guard_support::production_code(include_str!("../../../src/backend/main.rs"));
    let sites: Vec<&str> = prod
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("homes:"))
        .collect();
    assert_eq!(
        sites.len(),
        1,
        "`main.rs` 生产段里给 `homes` 赋值的地方有 {} 处（应当恰好 1 处）——\n\
             0 处 ⇒ 抽取坏了（本断言此刻在空转）；≥2 处 ⇒ 有第二条路，红线只守住一条。\n\
             实得：{sites:?}",
        sites.len()
    );
    assert_eq!(
        sites[0], "homes: Vec::new(),",
        "`main.rs` 开始往 `homes` 里填东西了 ⇒ hello 帧对 Claude 的线上字节**变了**。\n\
             这不是 bug，是 `S5`/DG1 该做的事 —— 但它是一次**跨仓契约变更**：\n\
             仓外 aterm 的 hello fixture 按精确字节对（契约冻结 2026-07-18）。\n\
             正确动作：与 aterm 同轮改，并同轮更新\n\
             `dg3_codex_fields_skipped_when_absent_claude_byte_equivalent` 的期望串。"
    );
}

/// ★ `S5` 的另一半，**刻意贴着上面那条放**：上面钉的是「生产路径给空表」，
/// 本条钉的是「**给空表不是因为后端不会发现**」。两条合起来才是 `S5` 的口径 ——
/// **能填不真填**。
///
/// # 少了本条会怎样
///
/// 只有上面那条的话，「`homes` 恒空」与「backend 根本没有发现能力」在判据眼里**一模一样**。
/// 于是有人在清理死代码时把 `agents::visible_homes` 整个删掉 —— 上面那条照样绿，
/// 而 `S5` 交付的东西没了、`S6` 的地基也没了，**没有任何东西会说**。
///
/// # 它为什么要真的构造一个 `Hello` 帧
///
/// 「能填」不只是"有个函数能列目录"，是「**填进去就能发**」：类型要正好是
/// `Hello.homes` 的元素类型，序列化出来要是合法的一行。否则真填那天还得再改一次形状，
/// 而那时改的是**跨仓契约**那一侧的东西 —— 最不该临时改形状的地方。
/// ⇒ 本条按生产路径**唯一**要改的那一行（`homes: Vec::new()` → `homes: visible_homes()`）
/// 原样搭一遍，证明那一行换过去**编得过、发得出**。
#[test]
fn the_backend_can_already_discover_homes_it_just_does_not_send_them() {
    // ─── ① 夹具那一半：**不依赖这台机器上装了什么**，所以能断言精确字节 ───
    //
    // 合成一家 agent（一个 `mkdir` 出来的 home）走一遍真正的发现路，
    // 把结果按生产路径**唯一**要改的那一行（`homes: …`）填进 Hello。
    // ⇒ 「能填」在这里不是"有个函数能列目录"，是**填进去就发得出、且字节是这个样子**。
    let root = std::env::temp_dir().join(format!("ccm-s5-wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("synthetic-home");
    std::fs::create_dir_all(&home).expect("建夹具 home");
    fn synth_present() -> Option<std::path::PathBuf> {
        Some(
            std::env::temp_dir()
                .join(format!("ccm-s5-wire-{}", std::process::id()))
                .join("synthetic-home"),
        )
    }
    fn synth_absent() -> Option<std::path::PathBuf> {
        Some(
            std::env::temp_dir()
                .join(format!("ccm-s5-wire-{}", std::process::id()))
                .join("no-such-home"),
        )
    }
    #[rustfmt::skip]
    let synth: &[crate::agents::Adapter] = &[
        crate::agents::Adapter { kind: "synthetic", home: synth_present, account_env: None, assets: None, history: None, upstream: None, mcp: None, footprint: None, accounts: None, records: None, processes: None, launch: None, compact_request: None, local: None },
        crate::agents::Adapter { kind: "ghost",     home: synth_absent, account_env: None, assets: None, history: None, upstream: None, mcp: None, footprint: None, accounts: None, records: None, processes: None, launch: None, compact_request: None, local: None },
    ];
    let discovered = crate::agents::visible_among(synth);
    assert_eq!(
        discovered.len(),
        1,
        "发现能力被掏空了（或判准放行了缺席的那家）—— 实得 {discovered:?}"
    );

    let line = to_line(&Frame::Hello {
        v: 1,
        build_id: "b".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/c".into(),
        homes: discovered,
        capabilities: vec![],
        emits: vec![],
        commands: vec![],
        unavailable: vec![],
        host_env: Default::default(),
        uncancellable: vec![],
    })
    .expect("填了 homes 的 hello 必须序列化得出来");
    assert_eq!(
        line,
        format!(
            "{{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b\",\"host_arch\":\"x86_64\",\
                 \"claude_dir\":\"/c\",\"homes\":[{{\"agent_kind\":\"synthetic\",\"path\":{}}}]}}\n",
            serde_json::to_string(&home.to_string_lossy().into_owned()).unwrap()
        ),
        "发现出来的东西**塞不进** `Hello.homes`，或塞进去之后字节形状不对。\n\
             ⇒ 真填那天还得再改一次形状 —— 而那时改的是跨仓契约那一侧，最不该临时改形状的地方。"
    );
    let _ = std::fs::remove_dir_all(&root);

    // ─── ② 真机那一半：跑一遍**生产入口**，只断言与世界无关的性质 ───
    //
    // 不断言条数 —— 那会变成"跑测试这台机器上装了几个 agent"，是世界的事实不是代码的。
    for h in &crate::agents::visible_homes() {
        assert!(
            !h.agent_kind.trim().is_empty(),
            "发现出来的项没有 agent_kind：{h:?}"
        );
        assert!(
            std::path::Path::new(&h.path).is_dir(),
            "发现出来的 {} 报了一个不是目录的 path：{}\n\
                 ⇒ 判准（home 目录存在）与产出对不上，消费方会拿它去拼子路径",
            h.agent_kind,
            h.path
        );
    }
}

// ─── DG3（#2D）：Codex wire additive 面 · 序列化 parity（aterm 消费侧 fixture 交叉核点）───

/// **present 形**（多 agent 的后端）：新字段在线上、snake_case、值域正确。
///
/// ⚠ **测试名刻意不改**：`src/doc/IPC-PROTOCOL.md` 与**仓外 aterm** 都按这个名字
/// 引用它当 fixture 真值，改名等于在跨仓契约上制造一处找不着。
/// 它钉的东西没变（「present 形的精确字节」），变的只是承载 agent 维度的字段 ——
/// `S4` 把 `codex_dir` + `kinds` 换成了通用的 `homes`（`D3`：agent 名只许在值里）。
#[test]
fn dg3_codex_fields_serialize_when_present() {
    let hello = to_line(&Frame::Hello {
        v: 1,
        build_id: "b".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/c".into(),
        homes: vec![
            AgentHome {
                agent_kind: "claude".into(),
                path: "/c".into(),
            },
            AgentHome {
                agent_kind: "codex".into(),
                path: "/home/u/.codex".into(),
            },
        ],
        capabilities: vec![],
        emits: vec![],
        commands: vec![],
        unavailable: vec![],
        host_env: Default::default(),
        uncancellable: vec![],
    })
    .unwrap();
    // ★ 精确字节（aterm fixture 交叉核真值）：字段按声明序，`homes` 在 `claude_dir` 之后；
    //   表内每项按 `AgentHome` 声明序 `agent_kind` → `path`。
    assert_eq!(
        hello,
        "{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b\",\"host_arch\":\"x86_64\",\"claude_dir\":\"/c\",\"homes\":[{\"agent_kind\":\"claude\",\"path\":\"/c\"},{\"agent_kind\":\"codex\",\"path\":\"/home/u/.codex\"}]}\n"
    );

    let sa = to_line(&Frame::SessionAdded {
        sid: "s".into(),
        agent_kind: Some("codex".into()),
        liveness_confidence: Some("heuristic".into()),
        background: false,
        attachable: None,
        cwd: None,
        project_dir: None,
        name: None,
        path: None,
        lines: None,
        activity: None,
        activity_text: crate::stream::wire::activity_cells(None).0,
        activity_tone: crate::stream::wire::activity_cells(None).1,
        waiting_for: None,
        container: None,
        pid: None,
    })
    .unwrap();
    assert_eq!(
        sa,
        "{\"kind\":\"session_added\",\"sid\":\"s\",\"agent_kind\":\"codex\",\"liveness_confidence\":\"heuristic\",\"activity_text\":\"运行中\",\"activity_tone\":\"now\"}\n"
    );

    let ss = to_line(&Frame::SessionStatus {
        sid: "s".into(),
        activity: None,
        activity_text: crate::stream::wire::activity_cells(None).0,
        activity_tone: crate::stream::wire::activity_cells(None).1,
        waiting_for: None,
        liveness_confidence: Some("heuristic".into()),
    })
    .unwrap();
    assert_eq!(
        ss,
        "{\"kind\":\"session_status\",\"sid\":\"s\",\"activity_text\":\"运行中\",\"activity_tone\":\"now\",\"liveness_confidence\":\"heuristic\"}\n"
    );
}

/// **absent 形**（Claude 会话 / 旧后端）：skip_if_none/empty → 字段**完全省略**，帧对 Claude
/// **字节等价旧形**（向后兼容红线）。aterm 消费侧「省=null→缺省 claude/authoritative」据此对齐。
/// ★ 精确字节串 = aterm fixture 交叉核的真值。
#[test]
fn dg3_codex_fields_skipped_when_absent_claude_byte_equivalent() {
    let hello = to_line(&Frame::Hello {
        v: 1,
        build_id: "b".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/c".into(),
        homes: vec![],
        capabilities: vec![],
        emits: vec![],
        commands: vec![],
        unavailable: vec![],
        host_env: Default::default(),
        uncancellable: vec![],
    })
    .unwrap();
    assert_eq!(
        hello,
        "{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b\",\"host_arch\":\"x86_64\",\"claude_dir\":\"/c\"}\n",
        "`homes` 空 → 省略，Hello 字节等价旧形。\n\
             ★ 这条同时是 `S4` 的红线：换掉 `codex_dir`/`kinds` 之后，\n\
             **Claude 那条线上的字节必须一个都没动**（右边这串是 `S4` 之前的原样）。"
    );

    let sa = to_line(&Frame::SessionAdded {
        sid: "s".into(),
        agent_kind: None,
        liveness_confidence: None,
        background: false,
        attachable: None,
        cwd: None,
        project_dir: None,
        name: None,
        path: None,
        lines: None,
        activity: None,
        activity_text: crate::stream::wire::activity_cells(None).0,
        activity_tone: crate::stream::wire::activity_cells(None).1,
        waiting_for: None,
        container: None,
        pid: None,
    })
    .unwrap();
    assert_eq!(
        sa, "{\"kind\":\"session_added\",\"sid\":\"s\",\"activity_text\":\"运行中\",\"activity_tone\":\"now\"}\n",
        "agent_kind(缺=claude)/liveness_confidence(缺=authoritative) 省略，字节等价旧形"
    );

    let ss = to_line(&Frame::SessionStatus {
        sid: "s".into(),
        activity: None,
        activity_text: crate::stream::wire::activity_cells(None).0,
        activity_tone: crate::stream::wire::activity_cells(None).1,
        waiting_for: None,
        liveness_confidence: None,
    })
    .unwrap();
    assert_eq!(
        ss, "{\"kind\":\"session_status\",\"sid\":\"s\",\"activity_text\":\"运行中\",\"activity_tone\":\"now\"}\n",
        "liveness_confidence 省略，字节等价旧形"
    );
}

/// ★ `K-P4`（09-04）握手帧**第四条面**的线上形状：absent 形字节等价 + present 形精确字节。
///
/// # 两半为什么在同一条测试里
///
/// 分开写的话，「省略时字节不变」那半在**真填那天照样绿** —— 它构造的是自己那张空表，
/// 不是生产那张。⇒ 本条钉的是「**这个字段**的两种形状各自长什么样」；
/// 「**生产给的是哪一种**」由 `main.rs` 那条抽取式判据钉，两条合起来才是「今天不变」。
/// 同 `homes` 的两条（`production_hello_leaves_homes_empty…` + `dg3_…_byte_equivalent`）
/// 的分工，缺任一条那句话都不成立。
#[test]
fn hello_unavailable_is_additive_present_and_absent() {
    // ① absent：空表省略 ⇒ 与第四条面加进来**之前**逐字节相同。
    //    ★ 右边这串与 `dg3_codex_fields_skipped_when_absent_claude_byte_equivalent`
    //      里那串**刻意逐字重复**：那条守 `S4` 的红线，本条守 `K-P4` 的。
    //      同一串钉在两处，任何一处被改掉都还有另一处会红。
    let absent = to_line(&Frame::Hello {
        v: 1,
        build_id: "b".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/c".into(),
        homes: vec![],
        capabilities: vec![],
        emits: vec![],
        commands: vec![],
        unavailable: vec![],
        host_env: Default::default(),
        uncancellable: vec![],
    })
    .unwrap();
    assert_eq!(
        absent,
        "{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b\",\"host_arch\":\"x86_64\",\"claude_dir\":\"/c\"}\n",
        "空表没被省略 ⇒ hello 的线上字节变了 ⇒ 仓外 aterm 那份按精确字节对的 fixture 当场对不上\n\
             （契约冻结 2026-07-18）。additive 的全部意义就在这一格。"
    );

    // ② present：字段落在 `commands` **之后**（serde 按声明序），
    //    表内每项按 `Unavailable` 的声明序 `command` → `code`。
    //    ★ 这一串是**真填那天**给 aterm 的 fixture 真值（同 `dg3_…_serialize_when_present` 的地位）。
    let present = to_line(&Frame::Hello {
        v: 1,
        build_id: "b".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/c".into(),
        homes: vec![],
        capabilities: vec![],
        emits: vec![],
        commands: vec!["kill".into(), "ping".into()],
        unavailable: vec![Unavailable {
            command: "kill".into(),
            code: "no_tmux".into(),
        }],
        host_env: Default::default(),
        uncancellable: vec![],
    })
    .unwrap();
    assert_eq!(
        present,
        "{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b\",\"host_arch\":\"x86_64\",\"claude_dir\":\"/c\",\
             \"commands\":[\"kill\",\"ping\"],\"unavailable\":[{\"command\":\"kill\",\"code\":\"no_tmux\"}]}\n"
    );

    // ③ **`kill` 同时在两张表里**，这不是笔误 —— 本字段说的是「接得下但做不到」。
    //    从 `commands` 里摘掉才是错的：那样客户端 `accepts()` 会直接拒发，
    //    连「点了告诉你为什么」这条兜底路都没了。
    assert!(
        present.contains("\"commands\":[\"kill\""),
        "present 形里 `kill` 必须仍留在 `commands` 里：{present}"
    );
    // ④ 反向自检：两串不同（否则上面两条可能被同一个退化实现一起满足）。
    assert_ne!(absent, present);
}

/// ★ S0：`cause` 的线上表现 —— `Gone` **不写字段**（additive，旧 monitor 原样工作），
/// 只有 `Superseded` 才出现。这条同时是**跨语言双写点**的本侧锚：字面量
/// `"superseded"` 与 monitor `src/frontend/shell/src/stream_source/` 的解析处逐字一致。
#[test]
fn removal_cause_is_additive_on_the_wire() {
    let gone = to_line(&Frame::SessionRemoved {
        sid: "s".into(),
        cause: RemovalCause::Gone,
    })
    .unwrap();
    assert!(
        !gone.contains("cause"),
        "Gone 必须不写 cause 字段（否则旧 monitor 看到未知字段、帧也白白变大）：{gone}"
    );
    let sup = to_line(&Frame::SessionRemoved {
        sid: "s".into(),
        cause: RemovalCause::Superseded,
    })
    .unwrap();
    assert!(
        sup.contains(r#""cause":"superseded""#),
        "Superseded 必须逐字发 \"superseded\"（monitor 侧按这个字面量解析）：{sup}"
    );
    // 反向自检：两条不是同一个串（否则上面两个断言可能同时被一个退化实现满足）。
    assert_ne!(gone, sup);
}

/// `session_added.container` 的线上形：对象 ＋ 缺席，四格各钉一处（精确字节）。
///
/// - 缺席：「不知道」。
/// - `{"host":"tmux","terminal":…}` / `{"host":"tmux"}`（句柄算不出就不写那一格）/ `{"host":"none"}`。
#[test]
fn session_added_container_is_an_object_with_host_and_terminal() {
    use crate::stream::wire::{SessionContainer, TerminalHost};
    let frame = |c: Option<SessionContainer>| {
        to_line(&Frame::SessionAdded {
            sid: "s".into(),
            agent_kind: None,
            liveness_confidence: None,
            background: false,
            attachable: None,
            cwd: None,
            project_dir: None,
            name: None,
            path: None,
            lines: None,
            activity: None,
            activity_text: crate::stream::wire::activity_cells(None).0,
            activity_tone: crate::stream::wire::activity_cells(None).1,
            waiting_for: None,
            container: c,
            pid: None,
        })
        .unwrap()
    };
    let hosted = |t: Option<&str>| SessionContainer::Hosted {
        host: TerminalHost::Tmux,
        terminal: t.map(str::to_string),
    };
    assert_eq!(frame(None), "{\"kind\":\"session_added\",\"sid\":\"s\",\"activity_text\":\"运行中\",\"activity_tone\":\"now\"}\n");
    assert_eq!(
        frame(Some(hosted(Some("tmux-3-7")))),
        "{\"kind\":\"session_added\",\"sid\":\"s\",\"activity_text\":\"运行中\",\"activity_tone\":\"now\",\"container\":{\"host\":\"tmux\",\"terminal\":\"tmux-3-7\"}}\n"
    );
    assert_eq!(
        frame(Some(hosted(None))),
        "{\"kind\":\"session_added\",\"sid\":\"s\",\"activity_text\":\"运行中\",\"activity_tone\":\"now\",\"container\":{\"host\":\"tmux\"}}\n"
    );
    assert_eq!(
        frame(Some(SessionContainer::None)),
        "{\"kind\":\"session_added\",\"sid\":\"s\",\"activity_text\":\"运行中\",\"activity_tone\":\"now\",\"container\":{\"host\":\"none\"}}\n"
    );
}

/// `session_state` 的**逐字节**金标准：两个取值、字段顺序 `sid` 在前；写好的字与语气跟在 `state` 后面。
/// monitor `stream_source::parse_frame` 照这两个字面量认它。
#[test]
fn mig1_session_state_has_exactly_these_bytes() {
    use crate::stream::wire::SessionFate;
    for (state, word, name, hint) in [
        (
            SessionFate::Reconnectable,
            "reconnectable",
            "beSession.fate.reconnectable",
            "beSession.fate.reconnectableHint",
        ),
        (
            SessionFate::Ended,
            "ended",
            "beSession.fate.ended",
            "beSession.fate.endedHint",
        ),
    ] {
        let (name, hint) = (
            copy_core::copy_text(name, &[]),
            copy_core::copy_text(hint, &[]),
        );
        assert_eq!(
            to_line(&Frame::session_state("abc".into(), state)).unwrap(),
            format!("{{\"kind\":\"session_state\",\"sid\":\"abc\",\"state\":\"{word}\",\"state_text\":\"{name}\",\"state_hint\":\"{hint}\",\"state_tone\":\"plain\"}}\n")
        );
    }
}

/// `sessions_replayed` 的**逐字节**金标准：无载荷，只有 kind。
/// monitor `stream_source::parse_frame` 照这个字面量认它。
#[test]
fn sessions_replayed_has_exactly_these_bytes() {
    assert_eq!(
        to_line(&Frame::SessionsReplayed).unwrap(),
        "{\"kind\":\"sessions_replayed\"}\n"
    );
}

/// `session_added.pid` 的线上形：缺席 ⇒ 与本字段加进来之前逐字节相同；带上 ⇒ 排在最后、是个整数。
#[test]
fn loc1b_session_added_pid_is_additive() {
    let frame = |pid: Option<u32>| {
        to_line(&Frame::SessionAdded {
            sid: "s".into(),
            agent_kind: None,
            liveness_confidence: None,
            background: false,
            attachable: None,
            cwd: None,
            project_dir: None,
            name: None,
            path: None,
            lines: None,
            activity: None,
            activity_text: crate::stream::wire::activity_cells(None).0,
            activity_tone: crate::stream::wire::activity_cells(None).1,
            waiting_for: None,
            container: None,
            pid,
        })
        .unwrap()
    };
    assert_eq!(frame(None), "{\"kind\":\"session_added\",\"sid\":\"s\",\"activity_text\":\"运行中\",\"activity_tone\":\"now\"}\n");
    assert_eq!(
        frame(Some(4242)),
        "{\"kind\":\"session_added\",\"sid\":\"s\",\"activity_text\":\"运行中\",\"activity_tone\":\"now\",\"pid\":4242}\n"
    );
}

// ═══ hello 回显宿主交来的那几格（`host_env`）═══════════════════════════════
//
// 要求：「常驻后端身份带数据目录（接错了拒并出声）」；`INVARIANTS §42` → `IPC-PROTOCOL.md §10` hello 那一行
// （additive：空表省略、线上字节不变）。审计 `E-compat.md` §E10 · `GP1.md §7.6` 第 4 条。

/// 🔴 I1a：回显恰是名单内被交了的那几格、原样（手写期望）；没交 / 空串的那一格不回显；token 与无关变量即使在环境里也不回显。
#[test]
fn hx2_host_env_echoes_exactly_the_handed_names_and_never_the_token() {
    let env: std::collections::HashMap<&str, &str> = [
        ("CCM_RELAY_PORT", "8788"),
        ("CCM_DATA_DIR", "/iso/home"),
        ("CCM_APIKEY_CREDENTIALS", "/d/apikey-credentials.json"),
        (crate::stream::listen::ENV_TOKEN_FILE, "s3cret-token"),
        ("HOME", "/home/u"),
    ]
    .into_iter()
    .collect();
    let got = crate::stream::wire::host_env_from(|n| env.get(n).map(|v| v.to_string()));
    let want: std::collections::BTreeMap<String, String> = [
        ("CCM_RELAY_PORT".to_string(), "8788".to_string()),
        ("CCM_DATA_DIR".to_string(), "/iso/home".to_string()),
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want);
    assert!(crate::stream::wire::host_env_from(|_| None).is_empty());
}

/// 🔴 I1b：钥匙那个变量名不在回显名单里（名单两格 == 手写；钥匙名不在其中）—— hello 谁都读得到。
#[test]
fn hx2_the_listen_token_is_never_echoed() {
    let names: std::collections::BTreeSet<&str> =
        crate::stream::wire::HOST_ECHO_ENVS.into_iter().collect();
    let want: std::collections::BTreeSet<&str> =
        ["CCM_RELAY_PORT", "CCM_DATA_DIR"].into_iter().collect();
    assert_eq!(names, want);
    assert!(
        !names.contains(crate::stream::listen::ENV_TOKEN_FILE)
            && !names.contains(crate::stream::listen::ENV_PORT)
    );
}

/// 🔴 I1c：空表 ⇒ 省略（线上字节与既有冻结串逐字节相同）；有值 ⇒ 落在最后、键按名排序。生产那一行恰好一处、读的是真环境。
#[test]
fn hx2_production_hello_bytes_do_not_change_when_nothing_was_handed() {
    let hello = |host_env: std::collections::BTreeMap<String, String>| {
        to_line(&Frame::Hello {
            v: 1,
            build_id: "b".into(),
            host_arch: "x86_64".into(),
            claude_dir: "/c".into(),
            homes: vec![],
            capabilities: vec![],
            emits: vec![],
            commands: vec![],
            unavailable: vec![],
            host_env,
            uncancellable: vec![],
        })
        .unwrap()
    };
    assert_eq!(
        hello(Default::default()),
        "{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b\",\"host_arch\":\"x86_64\",\"claude_dir\":\"/c\"}\n"
    );
    let present = hello(
        [
            ("CCM_RELAY_PORT".to_string(), "8788".to_string()),
            ("CCM_DATA_DIR".to_string(), "/d".to_string()),
        ]
        .into_iter()
        .collect(),
    );
    assert_eq!(
        present,
        "{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b\",\"host_arch\":\"x86_64\",\"claude_dir\":\"/c\",\
         \"host_env\":{\"CCM_DATA_DIR\":\"/d\",\"CCM_RELAY_PORT\":\"8788\"}}\n"
    );
    let main = guard_core::production_code(include_str!("../../../src/backend/main.rs"));
    assert_eq!(
        main.matches("host_env: wire::host_env_from(|name| std::env::var(name).ok()),")
            .count(),
        1,
        "生产 hello 那一格不是「从真环境按名单回显」那一行"
    );
}

/// `uncancellable`：空表省略（字节与既有冻结串相同）；有值落在最末（`host_env` 之后）。
#[test]
fn net2_uncancellable_is_additive_and_last() {
    let hello = |uncancellable: Vec<String>| {
        to_line(&Frame::Hello {
            v: 1,
            build_id: "b".into(),
            host_arch: "x86_64".into(),
            claude_dir: "/c".into(),
            homes: vec![],
            capabilities: vec![],
            emits: vec![],
            commands: vec![],
            unavailable: vec![],
            host_env: Default::default(),
            uncancellable,
        })
        .unwrap()
    };
    assert_eq!(
        hello(vec![]),
        "{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b\",\"host_arch\":\"x86_64\",\"claude_dir\":\"/c\"}\n"
    );
    assert_eq!(
        hello(vec!["kill".into()]),
        "{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b\",\"host_arch\":\"x86_64\",\"claude_dir\":\"/c\",\"uncancellable\":[\"kill\"]}\n"
    );
}

// ═══ 第二个前端在读的形状：冻结（只许加字段）═══════════════════════════════
//
// 两个前端吃同一个后端：桌面端（monitor）与第二个前端（手机端）。后者按下表逐字段读帧，缺一格就把整帧当坏帧丢
// （症状是「会话列表有、点进去永远空白」，零报错）。⇒ 表里登记的每一格不许改名、删、换类型；加新字段照旧（additive）。

/// （帧 kind 或 `request`, 字段, JSON 类型）。`request` 是入方向请求信封。
const SECOND_FRONTEND_READS: &[(&str, &str, &str)] = &[
    ("hello", "v", "number"),
    ("hello", "build_id", "string"),
    ("hello", "host_arch", "string"),
    ("hello", "claude_dir", "string"),
    ("hello", "capabilities", "array"),
    ("hello", "emits", "array"),
    ("line", "session_id", "string"),
    ("line", "path", "string"),
    ("line", "seq", "number"),
    ("line", "byte_offset", "number"),
    ("line", "raw", "string"),
    ("session_added", "sid", "string"),
    ("session_added", "path", "string"),
    ("session_added", "cwd", "string"),
    ("session_added", "name", "string"),
    ("session_added", "lines", "number"),
    ("session_added", "waiting_for", "string"),
    ("session_added", "agent_kind", "string"),
    ("session_added", "liveness_confidence", "string"),
    ("session_added", "attachable", "bool"),
    ("session_status", "sid", "string"),
    ("session_status", "waiting_for", "string"),
    ("session_status", "liveness_confidence", "string"),
    ("session_removed", "sid", "string"),
    ("session_removed", "cause", "string"),
    ("overflow", "dropped", "number"),
    ("overflow", "lost", "array"),
    ("overflow", "lost_truncated", "bool"),
    ("turn_end", "session_id", "string"),
    ("turn_end", "uuid", "string"),
    // 〔第二个前端 2026-10-08〕**逐字流**（聊天屏的打字机）。`tap_frames_have_exactly_these_bytes` 钉的是字节，
    // 本表钉的是**有人在读它** —— 少了这几行，整条 `tap` 被清理掉时没有任何判据说得出「有个前端靠它」。
    // ⚠ `ev` 与 `end` 恰有一个 ⇒ 一份样本只带得动 `ev`（打字机读的那一支）；`end` 两个取值的字节由那条逐字节判据钉。
    ("tap", "stream", "string"),
    ("tap", "run", "string"),
    ("tap", "resp", "number"),
    ("tap", "n", "number"),
    ("tap", "ev", "object"),
    // 〔第二个前端 2026-10-08〕**一切帧命令的应答与撤销**。失败那三格（`code` / `message` / `detail`）是
    // 失败卡与「复制详情」的全部原料：`detail` 跟着帧回，而最常见的那条错正好是「联系不上」—— 那时再去取就取不到。
    ("reply", "id", "string"),
    ("reply", "ok", "bool"),
    ("reply", "code", "string"),
    ("reply", "message", "string"),
    ("reply", "detail", "string"),
    ("reply", "data", "object"),
    ("cancelled", "id", "string"),
    ("request", "id", "string"),
    ("request", "cmd", "string"),
    ("request", "args", "object"),
    ("request", "within_ms", "number"),
];

/// 每一种帧都填满（可选格全给值），这样「这一格还在不在、是什么类型」才看得见。
fn every_frame_the_second_frontend_reads() -> Vec<Value> {
    let s = Some("x".to_string());
    let frames = vec![
        hello_with(vec!["bg".into()], vec!["turn_end".into()]),
        Frame::Line {
            session_id: "s".into(),
            path: "/p".into(),
            seq: 1,
            record: None,
            cwd: None,
            byte_offset: 9,
            rid: None,
            raw: Some("{}".into()),
        },
        Frame::SessionAdded {
            sid: "s".into(),
            agent_kind: s.clone(),
            liveness_confidence: s.clone(),
            background: false,
            attachable: Some(false),
            cwd: s.clone(),
            project_dir: None,
            name: s.clone(),
            path: s.clone(),
            lines: Some(3),
            activity: None,
            activity_text: crate::stream::wire::activity_cells(None).0,
            activity_tone: crate::stream::wire::activity_cells(None).1,
            waiting_for: s.clone(),
            container: None,
            pid: None,
        },
        Frame::SessionStatus {
            sid: "s".into(),
            activity: None,
            activity_text: crate::stream::wire::activity_cells(None).0,
            activity_tone: crate::stream::wire::activity_cells(None).1,
            waiting_for: s.clone(),
            liveness_confidence: s.clone(),
        },
        Frame::SessionRemoved {
            sid: "s".into(),
            cause: RemovalCause::Superseded,
        },
        Frame::Overflow {
            dropped: 2,
            lost: vec![LostFrame {
                kind: "session_status",
                subject: s.clone(),
            }],
            lost_truncated: true,
        },
        Frame::TurnEnd {
            session_id: "s".into(),
            uuid: "u".into(),
        },
        Frame::Tap {
            stream: "s".into(),
            run: s.clone(),
            resp: 1,
            n: 0,
            ev: Some(crate::agents::StreamEv::Start { rid: "r-1".into() }),
            end: None,
        },
        Frame::Reply {
            id: "1".into(),
            ok: false,
            code: s.clone(),
            message: s.clone(),
            detail: s.clone(),
            data: Some(serde_json::json!({})),
        },
        Frame::Cancelled { id: "1".into() },
    ];
    frames
        .iter()
        .map(|f| serde_json::from_str(to_line(f).unwrap().trim_end()).unwrap())
        .collect()
}

fn json_type(v: &Value) -> &'static str {
    match v {
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Bool(_) => "bool",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "null",
    }
}

/// 冻结表里的每一格，在真序列化出来的帧里都在、类型对；请求信封四键由真解析器认得、缺 `id` / `cmd` 就不认（`within_ms` 可缺）。
#[test]
fn the_shapes_the_second_frontend_reads_stay_put() {
    let frames = every_frame_the_second_frontend_reads();
    let mut kinds_seen = std::collections::BTreeSet::new();
    for (kind, field, ty) in SECOND_FRONTEND_READS {
        if *kind == "request" {
            continue;
        }
        let f = frames
            .iter()
            .find(|f| f["kind"] == *kind)
            .unwrap_or_else(|| panic!("冻结表里的帧 {kind} 不在样本里"));
        kinds_seen.insert(*kind);
        let got = f
            .get(*field)
            .unwrap_or_else(|| panic!("{kind}.{field} 没了（第二个前端在读它）：{f}"));
        assert_eq!(json_type(got), *ty, "{kind}.{field} 换了类型：{f}");
    }
    assert_eq!(kinds_seen.len(), frames.len(), "样本里有冻结表没登记的帧");

    let req: Request =
        serde_json::from_str(r#"{"id":"1","cmd":"ping","args":{"a":1},"within_ms":10000}"#)
            .expect("四键信封不认了");
    let v = serde_json::json!({"id": req.id, "cmd": req.cmd, "args": req.args, "within_ms": req.within_ms});
    let keys: Vec<&str> = SECOND_FRONTEND_READS
        .iter()
        .filter(|(k, _, _)| *k == "request")
        .map(|(_, f, ty)| {
            assert_eq!(json_type(&v[*f]), *ty, "请求信封 {f} 换了类型");
            *f
        })
        .collect();
    assert_eq!(keys, ["id", "cmd", "args", "within_ms"]);
    for missing in [r#"{"cmd":"ping","args":{}}"#, r#"{"id":"1","args":{}}"#] {
        assert!(
            serde_json::from_str::<Request>(missing).is_err(),
            "缺键的信封也认了：{missing}"
        );
    }
}

/// `line.raw`：没索要的客户端字节一个不变（不带这一格）；索要了才带、是那一行原文。
#[test]
fn line_raw_is_only_there_when_asked() {
    let line = |raw: Option<String>| {
        to_line(&Frame::Line {
            session_id: "s".into(),
            path: "/p".into(),
            seq: 0,
            record: None,
            cwd: None,
            byte_offset: 5,
            rid: None,
            raw,
        })
        .unwrap()
    };
    assert_eq!(
        line(None),
        "{\"kind\":\"line\",\"session_id\":\"s\",\"path\":\"/p\",\"seq\":0,\"byte_offset\":5}\n"
    );
    assert_eq!(
        line(Some("{\"a\":1}".into())),
        "{\"kind\":\"line\",\"session_id\":\"s\",\"path\":\"/p\",\"seq\":0,\"byte_offset\":5,\"raw\":\"{\\\"a\\\":1}\"}\n"
    );
}

// ═══ 成品面：通用记录与 history-read 的行，两个前端共同的契约（冻结：只许加，加了也要登记进来）═══════
//
// 换形那一刀（10-09）之后，`line.record` 与 `history-read.rows` 是两个前端都吃的**成品面**：判定只在后端，前端只排版。
// ⇒ 下表登记的每一格不许改名、删、换类型；新加一格也得先登记进来（`every_product_field_is_in_the_frozen_table`），
// 这样「两边在读的到底是哪几格」永远有一张表说得清。

/// （哪一面, 字段, JSON 类型）。面：`record` 公共格 · `said` / `reply` / `retry` / `title` / `queued` 各类自己的格
/// · `who`（`UserText`）· `error`（报错回复）· `block:<type>` 各种内容块 · `rows`（`history-read` 每一行）。
const PRODUCT_FACE: &[(&str, &str, &str)] = &[
    ("record", "agent", "string"),
    ("record", "id", "string"),
    ("record", "at", "string"),
    ("record", "timeText", "string"),
    ("record", "t", "string"),
    ("said", "who", "object"),
    ("said", "blocks", "array"),
    ("said", "results", "object"),
    ("said", "cwd", "string"),
    ("reply", "blocks", "array"),
    ("reply", "model", "string"),
    ("reply", "autoReply", "bool"),
    ("reply", "endsTurn", "bool"),
    ("reply", "cards", "object"),
    ("reply", "steps", "object"),
    ("reply", "runs", "object"),
    ("reply", "error", "object"),
    ("retry", "reason", "string"),
    ("retry", "attempt", "number"),
    ("retry", "max", "number"),
    ("title", "text", "string"),
    ("title", "by", "string"),
    ("queued", "who", "object"),
    ("who", "speaker", "object"),
    ("who", "text", "string"),
    ("error", "reason", "string"),
    ("error", "status", "number"),
    ("block:text", "type", "string"),
    ("block:text", "text", "string"),
    ("block:thinking", "type", "string"),
    ("block:thinking", "text", "string"),
    ("block:tool_use", "type", "string"),
    ("block:tool_use", "id", "string"),
    ("block:tool_use", "name", "string"),
    ("block:tool_use", "input", "object"),
    ("block:tool_result", "type", "string"),
    ("block:tool_result", "for", "string"),
    ("block:tool_result", "content", "array"),
    ("block:tool_result", "isError", "bool"),
    ("block:image", "type", "string"),
    ("block:image", "source", "object"),
    ("rows", "end", "number"),
    ("rows", "hash", "number"),
    ("rows", "record", "object"),
    ("rows", "cwd", "string"),
];

/// 记录金样里**全格**那几条（适配层真打出来的，`record_of_tests` 钉着它与代码一致）。
fn full_records() -> Vec<Value> {
    let golden = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/__fixtures__/record.golden.jsonl"
    ))
    .expect("记录金样不在");
    golden
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|v| {
            [
                "said-full",
                "reply-full",
                "reply-error",
                "retry-full",
                "title-agent",
                "queued",
            ]
            .contains(&v["case"].as_str().unwrap_or(""))
        })
        .map(|v| v["record"].clone())
        .collect()
}

/// `history-read` 一页的行：真走读核（Claude 那一家），一条带 `cwd` 的人话 ＋ 一条不进界面的。
fn history_read_rows() -> Vec<Value> {
    let face = crate::agents::claudecode::RECORDS;
    let page = concat!(
        r#"{"type":"user","uuid":"u1","timestamp":"2026-10-09T01:30:00.000Z","cwd":"/w","message":{"role":"user","content":"q"}}"#,
        "\n",
        r#"{"type":"permission-mode","permissionMode":"default","sessionId":"s"}"#,
        "\n",
    );
    let mut reader = crate::observe::record_page::Reader::new(&face, 0, &[], false);
    crate::observe::record_page::rows_of(&mut reader, 0, page.as_bytes())
}

/// 一面的样本：`(面, 那一面的对象)`，每一面至少一份。
fn product_samples() -> Vec<(String, Value)> {
    let mut out = Vec::new();
    for r in full_records() {
        let t = r["t"].as_str().unwrap().to_string();
        out.push(("record".to_string(), r.clone()));
        out.push((t.clone(), r.clone()));
        if let Some(w) = r.get("who") {
            out.push(("who".to_string(), w.clone()));
        }
        if let Some(e) = r.get("error") {
            out.push(("error".to_string(), e.clone()));
        }
        for b in r
            .get("blocks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            out.push((format!("block:{}", b["type"].as_str().unwrap()), b.clone()));
        }
    }
    for row in history_read_rows() {
        out.push(("rows".to_string(), row));
    }
    out
}

/// 记录的五个类别。
const RECORD_CLASSES: [&str; 5] = ["said", "reply", "retry", "title", "queued"];

/// 这一面的这一格登记过没有：各类别的格 ＝ 它自己那几格 ＋ `record` 公共格。
fn registered(face: &str, field: &str) -> bool {
    PRODUCT_FACE.iter().any(|(f, k, _)| {
        *k == field && (*f == face || (*f == "record" && RECORD_CLASSES.contains(&face)))
    })
}

/// 冻结表里每一格，在真打出来的成品里都在、类型对。
#[test]
fn the_record_shape_both_frontends_read_stays_put() {
    let samples = product_samples();
    for (face, field, ty) in PRODUCT_FACE {
        let hits: Vec<&Value> = samples
            .iter()
            .filter(|(f, _)| f == face)
            .filter_map(|(_, v)| v.get(*field))
            .collect();
        assert!(!hits.is_empty(), "{face}.{field} 没了（两个前端在读它）");
        for got in hits {
            assert_eq!(json_type(got), *ty, "{face}.{field} 换了类型：{got}");
        }
    }
}

/// 成品面进冻结表：真打出来的成品里出现的每一格都登记过（加一格可以，但得先进表 —— 两边读哪几格才说得清）。
#[test]
fn every_product_field_is_in_the_frozen_table() {
    let mut loose = Vec::new();
    // `record` 那一面的样本就是各类别的样本（按类别查，那里连公共格一起认）。
    for (face, v) in product_samples().into_iter().filter(|(f, _)| f != "record") {
        for key in v.as_object().expect("成品面都是对象").keys() {
            if !registered(&face, key) {
                loose.push(format!("{face}.{key}"));
            }
        }
    }
    loose.sort();
    loose.dedup();
    assert!(loose.is_empty(), "成品里有没登记进冻结表的格：{loose:?}");
}

// ═══ 第二个前端调的一次性子命令：冻结（叫法 · 位置参数 · 输出里它读的那几格）══════════════
//
// `--resolve` 的入出形状另有冻结金样（`tests/__fixtures__/resolve-contract.golden.json`）；会话 id 的校验规则钉在
// `resolve_query_tests::the_session_id_rule_the_second_frontend_relies_on_stays_put`。

/// （子命令, 位置参数个数）。叫法与位置参数一改，第二个前端那一侧就叫不通了。
const SECOND_FRONTEND_SUBCOMMANDS: &[(&str, usize)] = &[
    ("--list-projects", 0),
    ("--list-sessions", 1),
    ("--read-session", 1),
    ("--read-session-tail", 2),
    ("--read-session-from-offset", 2),
    ("--resolve", 0),
    ("--search", 1),
    ("--fork-session", 2),
    // 〔第二个前端 2026-10-08〕它**今天在调 / 这一拍要接**的另外九条。上面那八条之外，这九条一条都没有判据保护
    //   —— 上一次清理命令名（`invalid_args` 并进 `bad_args` · `--history-projects` / `--history-sessions` 从
    //   `SUBCOMMANDS` 消失）在它那一侧**不会红，会在用户手里坏**。
    // 两类，各有各的真检验（见本族那条测试，membership 之外不许空转）：
    //   ① CLI 独有、有位置参数的三条 ⇒ 下面 `runs` 里照原来的叫法真跑一趟；
    //   ② 帧面派生的六条 ⇒ 入参走 stdin 一段 JSON，argv 上一个位置参数都没有（0 由 `cli_control::spec_for` 验）。
    ("--backend-probe", 0),
    ("--find-in-session", 1),
    ("--list-user-inputs", 1),
    ("--ping", 0),
    ("--terminals-list", 0),
    ("--terminal-preview", 0),
    ("--terminal-input", 0),
    ("--history-page", 0),
    ("--history-facts", 0),
];

/// 上面那张表里**帧面派生**的那几条（CLI 口由 `cli_control::cli_exposed` 自动给，没有分派臂）。
///
/// 🔴 **单列一张不是冗余，是上面那张表对它们恒绿** —— 现打出来的：把帧面 `history-page` 改个名，
/// CLI 口当场消失，而 `SUBCOMMANDS` 里 `"--history-page"` 那个串还在（两者都是源码字面量、互不相干）
/// ⇒ `the_subcommands_the_second_frontend_calls_stay_put` **照样报绿**。本表让那一形当场红。
const SECOND_FRONTEND_DERIVED_SUBCOMMANDS: &[&str] = &[
    "--ping",
    "--terminals-list",
    "--terminal-preview",
    "--terminal-input",
    "--history-page",
    "--history-facts",
];

/// （子命令, 输出一行里第二个前端读的字段, JSON 类型）。
const SECOND_FRONTEND_SUBCOMMAND_FIELDS: &[(&str, &str, &str)] = &[
    ("--list-projects", "dirName", "string"),
    ("--list-projects", "projectPath", "string"),
    ("--list-projects", "sessionCount", "number"),
    ("--list-projects", "lastActivityMs", "number"),
    ("--list-sessions", "sessionId", "string"),
    ("--list-sessions", "aiTitle", "string"),
    ("--list-sessions", "cwd", "string"),
    ("--list-sessions", "jsonlPath", "string"),
    ("--list-sessions", "messageCountApprox", "number"),
    ("--list-sessions", "startedAtMs", "number"),
    ("--list-sessions", "updatedAtMs", "number"),
    ("--list-sessions", "isBg", "bool"),
];

/// 结构夹具：一个项目、一个会话（三行中性记录），住临时家目录。
fn second_frontend_home(tag: &str) -> (std::path::PathBuf, String, std::path::PathBuf) {
    let home = std::env::temp_dir().join(format!("ccm-mif-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let sid = "11111111-2222-3333-4444-555555555555".to_string();
    let path = dir.join(format!("{sid}.jsonl"));
    let rows = [
        format!(
            r#"{{"type":"user","uuid":"aaaaaaaa-0000-0000-0000-000000000001","sessionId":"{sid}","timestamp":"2026-01-01T00:00:00Z","cwd":"/p","message":{{"role":"user","content":"x"}}}}"#
        ),
        r#"{"type":"ai-title","aiTitle":"t"}"#.to_string(),
        format!(
            r#"{{"type":"assistant","uuid":"aaaaaaaa-0000-0000-0000-000000000002","parentUuid":"aaaaaaaa-0000-0000-0000-000000000001","sessionId":"{sid}","timestamp":"2026-01-01T00:00:01Z","cwd":"/p","message":{{"id":"m1","role":"assistant","content":[{{"type":"text","text":"y"}}]}}}}"#
        ),
    ];
    std::fs::write(&path, rows.join("\n") + "\n").unwrap();
    (home, sid, path)
}

/// 每一条都还在子命令表里、照原来的叫法与位置参数真跑得通；列项目 / 列会话输出里它读的那几格还在、类型没变。
#[test]
fn the_subcommands_the_second_frontend_calls_stay_put() {
    for (flag, _) in SECOND_FRONTEND_SUBCOMMANDS {
        assert!(crate::SUBCOMMANDS.contains(flag), "{flag} 不在子命令表里了");
    }
    // ⚠ **上面那一圈对帧面派生的那几条是「串对串」** —— 本表与 `SUBCOMMANDS` 都是源码里的字面量，
    //   帧面那条命令改名 / 删掉 / 进了 `STREAM_ONLY`，CLI 口当场消失而那个串还在 ⇒ 它照样绿（实测过）。
    //   ⇒ 派生那几条逐条验**真能派发**（[`SECOND_FRONTEND_DERIVED_SUBCOMMANDS`]），顺便验登记的 0 不是凑的。
    for flag in SECOND_FRONTEND_DERIVED_SUBCOMMANDS {
        let spec = crate::control::cli_control::spec_for(flag).unwrap_or_else(|| {
            panic!(
                "{flag} 是帧面派生的 CLI 口，而 `spec_for` 今天查不到它 —— 帧面那条命令改名 / 删了 / 进了
                 `cli_control::STREAM_ONLY`，CLI 面当场消失，而 `SUBCOMMANDS` 里那个串还在
                 ⇒ 第二个前端调它只会拿到一堆 jsonl 行（`is_query_mode` 把它当未知 flag ⇒ 照常进流模式）。"
            )
        });
        let (_, pos) = SECOND_FRONTEND_SUBCOMMANDS
            .iter()
            .find(|(f, _)| f == flag)
            .unwrap_or_else(|| panic!("{flag} 列在派生表里，却不在冻结表里"));
        assert_eq!(
            *pos, 0,
            "{flag} 是帧面 `{}` 派生的 CLI 口（入参走 stdin），位置参数该登记成 0",
            spec.name
        );
    }
    // 探测口不在 `REGISTRY` 上（它是 `cli_control` 自己那一口）⇒ 单独验分派臂认得它。
    assert!(
        crate::control::cli_control::handles("--backend-probe"),
        "`--backend-probe` 的分派臂不认它了 —— 第二个前端靠它判这台支持哪几条"
    );
    let (home, sid, path) = second_frontend_home("sub");
    let p = path.to_string_lossy().to_string();
    let argv = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let runs: Vec<(&str, Vec<String>)> = vec![
        ("--list-projects", argv(&["--list-projects"])),
        ("--list-sessions", argv(&["--list-sessions", "-p"])),
        ("--read-session", argv(&["--read-session", &p])),
        (
            "--read-session-tail",
            argv(&["--read-session-tail", &p, "1"]),
        ),
        (
            "--read-session-from-offset",
            argv(&["--read-session-from-offset", &p, "0"]),
        ),
        ("--search", argv(&["--search", "x"])),
        (
            "--find-in-session",
            argv(&["--find-in-session", "--query", "x", &p]),
        ),
        ("--list-user-inputs", argv(&["--list-user-inputs", &p])),
        (
            "--fork-session",
            argv(&[
                "--fork-session",
                &sid,
                "aaaaaaaa-0000-0000-0000-000000000002",
            ]),
        ),
    ];
    for (flag, args) in &runs {
        let want = SECOND_FRONTEND_SUBCOMMANDS
            .iter()
            .find(|(f, _)| f == flag)
            .unwrap()
            .1;
        // 位置参数 ＝ 去掉子命令本身、去掉 `--opt` 与紧跟它的那个值之后剩下的。
        // （原来写的是 `args.len() - 1`，那只对「不带选项」那几条成立；`--find-in-session` 必带 `--query <q>`。）
        let mut positional = 0usize;
        let mut it = args[1..].iter();
        while let Some(a) = it.next() {
            if a.starts_with("--") {
                let _ = it.next();
            } else {
                positional += 1;
            }
        }
        assert_eq!(positional, want, "{flag} 的位置参数个数");
        assert!(crate::is_query_mode(args), "{flag} 不进一次性查询了");
        let code = match *flag {
            "--search" => crate::observe::search_query::run(&home, args),
            "--fork-session" => crate::control::fork_write::run(&home, args),
            _ => crate::observe::history_query::run(&home, args),
        };
        assert_eq!(code, 0, "{flag} 照原来的叫法跑不通了：{args:?}");
    }
    let mut buf = Vec::new();
    crate::observe::history_query::list_projects_to(&home, &mut buf).expect("列项目");
    let projects: Value =
        serde_json::from_str(String::from_utf8_lossy(&buf).lines().next().unwrap()).unwrap();
    let mut buf = Vec::new();
    crate::observe::history_query::list_sessions_into(&home, "-p", &mut buf).expect("列会话");
    let sessions: Value = String::from_utf8_lossy(&buf)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["sessionId"] == sid.as_str())
        .expect("列会话里没有夹具那条");
    for (flag, field, ty) in SECOND_FRONTEND_SUBCOMMAND_FIELDS {
        let row = if *flag == "--list-projects" {
            &projects
        } else {
            &sessions
        };
        let got = row
            .get(*field)
            .unwrap_or_else(|| panic!("{flag} 的输出里没了 {field}：{row}"));
        assert_eq!(json_type(got), *ty, "{flag}.{field} 换了类型：{row}");
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// 〔G2〕会话状态带写好的字与语气（对所有出口一次补齐）：活着的三态在 `session_added` · `session_status` 上
/// （`activity_text` · `activity_tone`，没有 `activity` ⇒ 两格都不上线）；离开「活」之后的两种在 `session_state` 上
/// （`state_text` 短名 · `state_hint` 悬停那一句 · `state_tone`）。字走文案表，出口照抄、不按码取字。
#[test]
fn session_frames_carry_the_state_written_and_toned() {
    use crate::agents::SessionActivity as A;
    use crate::stream::wire::{activity_cells, SessionFate};
    let status = |a: Option<A>| {
        let (activity_text, activity_tone) = activity_cells(a);
        serde_json::to_value(Frame::SessionStatus {
            sid: "s".into(),
            activity: a,
            activity_text,
            activity_tone,
            waiting_for: None,
            liveness_confidence: None,
        })
        .unwrap()
    };
    for (a, key, tone) in [
        (A::Working, "beSession.activity.working", "now"),
        (A::NeedsYou, "beSession.activity.needsYou", "need"),
        (A::Idle, "beSession.activity.idle", "plain"),
        (A::BackgroundWork, "beSession.activity.backgroundWork", "busy"),
    ] {
        let v = status(Some(a));
        assert_eq!(v["activity_text"], copy_core::copy_text(key, &[]), "{a:?}");
        assert_eq!(v["activity_tone"], tone, "{a:?}");
    }
    // 说不清在干什么（活着、那一家没说）⇒ 核心也给一格字与语气（出口不自己补「运行中」）。
    let none = status(None);
    assert_eq!(
        none["activity_text"],
        copy_core::copy_text("beSession.activity.unclear", &[])
    );
    assert_eq!(none["activity_tone"], "now");

    for (f, name, hint) in [
        (
            SessionFate::Reconnectable,
            "beSession.fate.reconnectable",
            "beSession.fate.reconnectableHint",
        ),
        (
            SessionFate::Ended,
            "beSession.fate.ended",
            "beSession.fate.endedHint",
        ),
    ] {
        let v = serde_json::to_value(Frame::session_state("s".into(), f)).unwrap();
        assert_eq!(v["state_text"], copy_core::copy_text(name, &[]), "{f:?}");
        assert_eq!(v["state_hint"], copy_core::copy_text(hint, &[]), "{f:?}");
        assert_eq!(v["state_tone"], "plain", "{f:?}");
    }
}
