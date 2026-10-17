//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md §10`（backend → client 线上契约）
//!
//! 核原文：`IPC-PROTOCOL.md §10` 逐字「每行恰好一个 UTF-8 JSON 对象，`\n` 结尾，对象内无裸 `\n`/`\r`」；同节帧表逐格写了各字段的
//! additive 规则与今天的取值 —— 本族判出方向帧的字节与那张表一致。出方向单写者那两条服务同一节的「每行恰好一个对象」与「首帧」；
//! 「SSH 流单写者」不单独立条。

/// ★★ **出方向帧只许有一个写者**〔audit-0805 08-08，Phase G 第 60 件，D6〕。
///
/// `stream/inbound/` 头注逐字写着「`writer_task`（**出方向帧的唯一出口**）」。
/// 那句话撑着 NDJSON 在线上的完整性：两个写者并发写同一个 stdout，
/// 帧就会**互相撕开**（半行 + 半行），而这条流**手机端正在消费**（D6 契约冻结）。
///
/// ⇒ 而它只是散文。08-08 实测：在 `stream/inbound/` 加一个自己 `tokio::io::stdout()`
/// 并 `write_all` 的函数，**backend 292 条判据一条不红**。
///
/// # 人群与豁免
///
/// 人群 = **流式那条路**（`main.rs` / `stream/inbound/` 整个目录 / `wire/mod.rs`（帧）/ `wire/listen.rs`）生产段里所有
/// `write_all(`。今天恰好三处，各自登记：
/// · `wire::write_and_flush_hello`（`wire/mod.rs`）—— 握手前那一帧，写完才产出 `HelloFlushed`；
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
/// （那是另立的一件，这里不碰 `relay/`）。补的是「常驻自己那条路」，不是那条自陈的全部。
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
    let root = guard_core::addr::backend_src_root();
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
        include_str!("../../../src/backend/wire/mod.rs").to_string(),
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
                     而这条流**手机端正在消费**（D6：暴露给第三方 = 契约冻结成本）。\n\
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
    let src = include_str!("../../../src/backend/wire/mod.rs");
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
use crate::wire::SessionFaceExt as _;
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
    use crate::wire::{
        LiveBlock, LiveCard, LiveOp, LivePhase, RereadWhy, RunEnded, RunInfo, RunState, RunWhy,
        SessionContainer, SessionFate, TransferEnd, Unavailable,
    };
    let s = |v: &str| v.to_string();
    let both = |f: Frame| [f.clone(), f];
    vec![
        [
            Frame::Hello(crate::wire::Hello {
                build_id: s("b1"),
                host_arch: s("x86_64"),
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
                face: crate::wire::SessionFace::of(crate::wire::Life::Live { activity: Some(crate::agents::SessionActivity::NeedsYou), tmux: None })
                    .under(Some(s("b"))),
                name_face: crate::history::search_rules::name_face(
                    &crate::history::search_rules::LabelFacts {
                        title: Some(s("t")),
                        project_dir: Some(s("/w")),
                        ..Default::default()
                    },
                ),
                waiting_for: Some(s("permission prompt")),
                hidden: crate::control::session_hidden::judge(
                    &crate::control::session_hidden::Facts {
                        bg: true,
                        rules: crate::control::session_hidden::Rules {
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
                face: crate::wire::SessionFace::of(crate::wire::Life::Live { activity: None, tmux: None }),
                name_face: crate::history::search_rules::name_face(&Default::default()),
                waiting_for: None,
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
                face: crate::wire::SessionFace::of(crate::wire::Life::Live { activity: Some(crate::agents::SessionActivity::NeedsYou), tmux: None })
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
                face: crate::wire::SessionFace::of(crate::wire::Life::Live { activity: None, tmux: None }),
                name_face: crate::history::search_rules::name_face(&Default::default()),
                waiting_for: None,
                path: None,
            }),
        ],
        {
            // 全格：可重连；最少格：已结束（不能那几样带为什么）。
            [
                Frame::session_state(s("s1"), SessionFate::Reconnectable, Some(s("b"))),
                Frame::session_state(s("s1"), SessionFate::Ended, None),
            ]
        },
        [
            Frame::SessionRemoved(crate::wire::SessionRemoved {
                sid: s("s1"),
                cause: RemovalCause::Superseded,
                by: Some(s("s2")),
            }),
            Frame::SessionRemoved(crate::wire::SessionRemoved {
                sid: s("s1"),
                cause: RemovalCause::Gone,
                by: None,
            }),
        ],
        [Frame::overflow(0, true), Frame::overflow(7, false)],
        [
            Frame::Reply(crate::wire::Reply {
                id: s("q1"),
                ok: false,
                code: Some(s("bad_args")),
                message: Some(s("m")),
                detail: Some(s("码：bad_args")),
                data: Some(serde_json::json!({"k": 1})),
            }),
            Frame::Reply(crate::wire::Reply {
                id: s("q1"),
                ok: true,
                code: None,
                message: None,
                detail: None,
                data: None,
            }),
        ],
        both(Frame::Cancelled(crate::wire::Cancelled { id: s("q1") })),
        [
            Frame::changed(
                Topic::Plan,
                Some(s("/w")),
                Some(s("r1")),
                Some(serde_json::json!({"needs": 2})),
            ),
            Frame::changed(Topic::Accounts, None, None, None),
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
        both(Frame::LinkData(crate::wire::LinkData {
            link: s("L1"),
            data: s("aGkK"),
        })),
        [
            Frame::LinkEnd(crate::wire::LinkEnd {
                link: s("L1"),
                error: Some(s("e")),
            }),
            Frame::LinkEnd(crate::wire::LinkEnd {
                link: s("L1"),
                error: None,
            }),
        ],
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
                            text: Some(s("hi")),
                            tool: None,
                        },
                        LiveBlock {
                            kind: BlockKind::Tool,
                            text: None,
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
            }),
            Frame::SessionRuns(crate::wire::SessionRuns {
                sid: s("s1"),
                runs: vec![RunInfo {
                    run: s("a1"),
                    ..RunInfo::default()
                }],
                ended: vec![],
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
            }),
            Frame::TerminalScreen(crate::wire::TerminalScreen {
                ticket: s("t1"),
                seq: 1,
                view: serde_json::from_value(serde_json::json!({"screen": "0000000000000000", "cols": 1, "rows": 0, "cursor": {"x": 0, "y": 0, "visible": false}, "lines": [], "scrollback_lines": 0, "capped": false, "captured_at": 0, "captured_at_text": "00:00:00"})).unwrap(),
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

/// ★ W1：链路两帧 ＋ `changed` 的**逐字节**金标准。`link_end` 的 `error` 缺席时不上线（正常收尾）。
#[test]
fn link_frames_have_exactly_these_bytes() {
    let cases = [
        (
            Frame::LinkData(crate::wire::LinkData {
                link: "m1.0-3".into(),
                data: b64_encode(b"hi\n"),
            }),
            "{\"kind\":\"link_data\",\"link\":\"m1.0-3\",\"data\":\"aGkK\"}\n",
        ),
        (
            Frame::LinkEnd(crate::wire::LinkEnd {
                link: "m1.0-3".into(),
                error: None,
            }),
            "{\"kind\":\"link_end\",\"link\":\"m1.0-3\"}\n",
        ),
        (
            Frame::LinkEnd(crate::wire::LinkEnd {
                link: "m1.0-3".into(),
                error: Some("读链路下行失败".into()),
            }),
            "{\"kind\":\"link_end\",\"link\":\"m1.0-3\",\"error\":\"读链路下行失败\"}\n",
        ),
        (
            Frame::changed(Topic::Accounts, None, None, None),
            "{\"kind\":\"changed\",\"topic\":\"accounts\"}\n",
        ),
        (
            Frame::changed(Topic::RotationStrategies, None, None, None),
            "{\"kind\":\"changed\",\"topic\":\"rotation_strategies\"}\n",
        ),
        (
            Frame::changed(Topic::Tasks, Some("s1".into()), None, None),
            "{\"kind\":\"changed\",\"topic\":\"tasks\",\"key\":\"s1\"}\n",
        ),
        (
            Frame::changed(
                Topic::Plan,
                Some("/w".into()),
                Some("0123abcd".into()),
                Some(serde_json::json!({"needs": 2})),
            ),
            "{\"kind\":\"changed\",\"topic\":\"plan\",\"key\":\"/w\",\"rev\":\"0123abcd\",\"body\":{\"needs\":2}}\n",
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
    let f = |end: Option<crate::wire::TransferEnd>| {
        Frame::Transfer(crate::wire::Transfer {
            id: "xfer-7".into(),
            got: 262144,
            total: 1000000,
            end,
        })
    };
    let cases = [
        (
            f(None),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000}\n",
        ),
        (
            f(Some(crate::wire::TransferEnd::Done {
                bytes: 1000000,
                sha256: None,
            })),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"done\",\"bytes\":1000000}}\n",
        ),
        // 上传那一路的传完带整份摘要（提交时的对拍依据）；下载那一路没有 ⇒ 上一格原样不上线。
        (
            f(Some(crate::wire::TransferEnd::Done {
                bytes: 1000000,
                sha256: Some("ab".repeat(32)),
            })),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"done\",\"bytes\":1000000,\"sha256\":\"abababababababababababababababababababababababababababababababab\"}}\n",
        ),
        (
            f(Some(crate::wire::TransferEnd::Failed { why: "写暂存件失败".into(), code: None, detail: None })),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"failed\",\"why\":\"写暂存件失败\"}}\n",
        ),
        // 带码的那一形（今天只有 SFTP 起始目录不是后端 home 那一码）。
        (
            f(Some(crate::wire::TransferEnd::Failed { why: "w".into(), code: Some("sftp_home_mismatch".into()), detail: None })),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"failed\",\"why\":\"w\",\"code\":\"sftp_home_mismatch\"}}\n",
        ),
        // 带复制详情的那一形（句子只带原因词，原话在 `detail` 里）。
        (
            f(Some(crate::wire::TransferEnd::Failed { why: "w".into(), code: None, detail: Some("d".into()) })),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"failed\",\"why\":\"w\",\"detail\":\"d\"}}\n",
        ),
        (
            f(Some(crate::wire::TransferEnd::Cancelled)),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"cancelled\"}}\n",
        ),
    ];
    for (frame, want) in cases {
        assert_eq!(to_line(&frame).unwrap(), want);
        assert!(!frame.loss_is_recoverable(), "传输帧被判成丢了可恢复");
    }
}

/// `live` 的逐字节线上形状（期望是手写字面量；客户端按 `cmd-wire` 的类型解同样的串）。
/// 一张卡五种 `op`：整卡 · 加块 · 加字 · 说完了 · 撤卡；`run` 缺 ＝ 主运行。可丢（SSE 只保快，写者最低优先）。
#[test]
fn live_frames_have_exactly_these_bytes() {
    use crate::agents::BlockKind;
    use crate::wire::{LiveBlock, LiveCard, LiveOp, LivePhase, LiveWhy};
    let live = |run: Option<&str>, n: u64, op: LiveOp| {
        Frame::LiveCard(LiveCard {
            sid: "0b6c1f7e-sid".into(),
            run: run.map(str::to_string),
            card: 12,
            n,
            op,
        })
    };
    let cases = [
        (
            live(
                None,
                0,
                LiveOp::Open {
                    rid: "r-1".into(),
                    phase: LivePhase::Streaming,
                    clipped: false,
                    blocks: vec![
                        LiveBlock { kind: BlockKind::Thinking, text: None, tool: None },
                        LiveBlock { kind: BlockKind::Text, text: Some("你好".into()), tool: None },
                    ],
                },
            ),
            "{\"kind\":\"live_card\",\"sid\":\"0b6c1f7e-sid\",\"card\":12,\"n\":0,\"op\":\"open\",\"rid\":\"r-1\",\"phase\":\"streaming\",\"clipped\":false,\"blocks\":[{\"kind\":\"thinking\"},{\"kind\":\"text\",\"text\":\"你好\"}]}\n",
        ),
        (
            live(Some("a1"), 1, LiveOp::Block { i: 2, block: BlockKind::Tool, tool: Some("Bash".into()) }),
            "{\"kind\":\"live_card\",\"sid\":\"0b6c1f7e-sid\",\"run\":\"a1\",\"card\":12,\"n\":1,\"op\":\"block\",\"i\":2,\"block\":\"tool\",\"tool\":\"Bash\"}\n",
        ),
        (
            live(None, 2, LiveOp::Text { i: 1, s: "hi".into() }),
            "{\"kind\":\"live_card\",\"sid\":\"0b6c1f7e-sid\",\"card\":12,\"n\":2,\"op\":\"text\",\"i\":1,\"s\":\"hi\"}\n",
        ),
        (
            live(None, 3, LiveOp::Awaiting),
            "{\"kind\":\"live_card\",\"sid\":\"0b6c1f7e-sid\",\"card\":12,\"n\":3,\"op\":\"awaiting\"}\n",
        ),
        (
            live(None, 4, LiveOp::Close { why: LiveWhy::Record }),
            "{\"kind\":\"live_card\",\"sid\":\"0b6c1f7e-sid\",\"card\":12,\"n\":4,\"op\":\"close\",\"why\":\"record\"}\n",
        ),
        (
            live(None, 4, LiveOp::Close { why: LiveWhy::Unwatched }),
            "{\"kind\":\"live_card\",\"sid\":\"0b6c1f7e-sid\",\"card\":12,\"n\":4,\"op\":\"close\",\"why\":\"unwatched\"}\n",
        ),
    ];
    for (frame, want) in cases {
        assert_eq!(to_line(&frame).unwrap(), want);
        // 线上形住 `cmd-wire::live`：帧体（去掉 `kind`）按那份类型解得开、重写出来是同一份。
        let mut body: serde_json::Value = serde_json::from_str(want.trim_end()).unwrap();
        body.as_object_mut().unwrap().remove("kind");
        let card: cmd_wire::live::LiveCard =
            serde_json::from_value(body.clone()).expect("活卡帧体解得开");
        assert_eq!(serde_json::to_value(&card).unwrap(), body);
        assert!(
            frame.loss_is_recoverable(),
            "活卡帧可丢（jsonl 保对），却被判成不可恢复"
        );
        assert_eq!(
            frame.content_of(),
            Some("0b6c1f7e-sid"),
            "活卡帧不按名单走（`stream-watch`）"
        );
    }
}

/// 运行表那一帧的逐字节线上形状（客户端按 `cmd-wire` 的类型解同样的串）。
#[test]
fn session_runs_frames_have_exactly_these_bytes() {
    use crate::agents::RunDid;
    use crate::wire::{RunEnded, RunInfo, RunState, RunWhy};
    let f = Frame::SessionRuns(crate::wire::SessionRuns {
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
    });
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

/// `overflow` 只有那一句：只丢了行 · 已重新对齐 · 两样都有，三种各一句（数在句子里）。
#[test]
fn the_overflow_frame_is_the_one_sentence() {
    let said = |f: Frame| -> Value {
        let line = to_line(&f).expect("serialize");
        serde_json::from_str(line.strip_suffix('\n').unwrap()).expect("valid json")
    };
    assert_eq!(
        said(Frame::overflow(42, false)),
        serde_json::json!({ "kind": "overflow", "said": copy_core::copy_text("beStream.overflow.lines", &[("n", "42")]) })
    );
    assert_eq!(
        said(Frame::overflow(0, true))["said"],
        copy_core::copy_text("beStream.overflow.realigned", &[])
    );
    assert_eq!(
        said(Frame::overflow(3, true))["said"],
        copy_core::copy_text("beStream.overflow.realignedLines", &[("n", "3")])
    );
}

// `tmux_sessions` 帧单行转义那条随帧删了（tmux 原文只在进程内喂会话账本）。

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
        at_ms: None,
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
    let frame = Frame::Line(crate::wire::Line {
        session_id: "sid".into(),
        path: "/some/path.jsonl".into(),
        seq: 42,
        record: Some(said_record("u1", raw)),
        cwd: None,
        byte_offset: 99,
        rid: None,
    });

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

/// ★ S0：`cause` 的线上表现 —— `Gone` **不写字段**（additive，旧 monitor 原样工作），
/// 只有 `Superseded` 才出现。这条同时是**跨语言双写点**的本侧锚：字面量
/// `"superseded"` 与 monitor `src/frontend/shell/src/stream_source/` 的解析处逐字一致。
#[test]
fn removal_cause_is_additive_on_the_wire() {
    let gone = to_line(&Frame::SessionRemoved(crate::wire::SessionRemoved {
        sid: "s".into(),
        cause: RemovalCause::Gone,
        by: None,
    }))
    .unwrap();
    assert!(
        !gone.contains("cause"),
        "Gone 必须不写 cause 字段（否则旧 monitor 看到未知字段、帧也白白变大）：{gone}"
    );
    let sup = to_line(&Frame::SessionRemoved(crate::wire::SessionRemoved {
        sid: "s".into(),
        cause: RemovalCause::Superseded,
        by: Some("s2".into()),
    }))
    .unwrap();
    assert_eq!(
        sup,
        "{\"kind\":\"session_removed\",\"sid\":\"s\",\"cause\":\"superseded\",\"by\":\"s2\"}\n",
        "被顶替时带上顶替它的那条会话；不是顶替不出这一格"
    );
    assert!(!gone.contains("by"), "{gone}");
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
    use crate::wire::SessionContainer;
    let frame = |c: Option<SessionContainer>| {
        to_line(&Frame::SessionAdded(crate::wire::SessionAdded {
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
                tmux: None,
            }),
            name_face: crate::history::search_rules::name_face(&Default::default()),
            waiting_for: None,
            hidden: None,
            container: c,
            pid: 0,
        }))
        .unwrap()
    };
    let hosted = |t: Option<&str>| SessionContainer::Tmux {
        terminal: t.map(str::to_string),
    };
    assert_eq!(frame(None), "{\"kind\":\"session_added\",\"sid\":\"s\",\"agent_kind\":\"claude\",\"face\":{\"dot\":\"idle\",\"live\":{\"is\":\"yes\"},\"settled\":false,\"name\":\"状态不明\",\"tip\":\"状态不明\",\"line\":\"状态不明\",\"tone\":\"plain\",\"order\":3,\"can\":{\"close\":{\"can\":\"no\",\"why\":\"运行中\"},\"resume\":{\"can\":\"no\",\"why\":\"运行中\"},\"term\":{\"can\":\"yes\",\"tmux\":false},\"reattach\":{\"can\":\"no\",\"why\":\"运行中\"},\"model\":{\"can\":\"yes\"},\"permission\":{\"can\":\"yes\"}}},\"label\":\"没有说过话的会话\",\"pid\":0}\n");
    assert_eq!(
        frame(Some(hosted(Some("tmux-3-7")))),
        "{\"kind\":\"session_added\",\"sid\":\"s\",\"agent_kind\":\"claude\",\"face\":{\"dot\":\"idle\",\"live\":{\"is\":\"yes\"},\"settled\":false,\"name\":\"状态不明\",\"tip\":\"状态不明\",\"line\":\"状态不明\",\"tone\":\"plain\",\"order\":3,\"can\":{\"close\":{\"can\":\"no\",\"why\":\"运行中\"},\"resume\":{\"can\":\"no\",\"why\":\"运行中\"},\"term\":{\"can\":\"yes\",\"tmux\":false},\"reattach\":{\"can\":\"no\",\"why\":\"运行中\"},\"model\":{\"can\":\"yes\"},\"permission\":{\"can\":\"yes\"}}},\"label\":\"没有说过话的会话\",\"container\":{\"host\":\"tmux\",\"terminal\":\"tmux-3-7\"},\"pid\":0}\n"
    );
    assert_eq!(
        frame(Some(hosted(None))),
        "{\"kind\":\"session_added\",\"sid\":\"s\",\"agent_kind\":\"claude\",\"face\":{\"dot\":\"idle\",\"live\":{\"is\":\"yes\"},\"settled\":false,\"name\":\"状态不明\",\"tip\":\"状态不明\",\"line\":\"状态不明\",\"tone\":\"plain\",\"order\":3,\"can\":{\"close\":{\"can\":\"no\",\"why\":\"运行中\"},\"resume\":{\"can\":\"no\",\"why\":\"运行中\"},\"term\":{\"can\":\"yes\",\"tmux\":false},\"reattach\":{\"can\":\"no\",\"why\":\"运行中\"},\"model\":{\"can\":\"yes\"},\"permission\":{\"can\":\"yes\"}}},\"label\":\"没有说过话的会话\",\"container\":{\"host\":\"tmux\"},\"pid\":0}\n"
    );
    assert_eq!(
        frame(Some(SessionContainer::None)),
        "{\"kind\":\"session_added\",\"sid\":\"s\",\"agent_kind\":\"claude\",\"face\":{\"dot\":\"idle\",\"live\":{\"is\":\"yes\"},\"settled\":false,\"name\":\"状态不明\",\"tip\":\"状态不明\",\"line\":\"状态不明\",\"tone\":\"plain\",\"order\":3,\"can\":{\"close\":{\"can\":\"no\",\"why\":\"运行中\"},\"resume\":{\"can\":\"no\",\"why\":\"运行中\"},\"term\":{\"can\":\"yes\",\"tmux\":false},\"reattach\":{\"can\":\"no\",\"why\":\"运行中\"},\"model\":{\"can\":\"yes\"},\"permission\":{\"can\":\"yes\"}}},\"label\":\"没有说过话的会话\",\"container\":{\"host\":\"none\"},\"pid\":0}\n"
    );
}

/// `session_state` 的**逐字节**金标准：两个取值、字段顺序 `sid` 在前；整份脸跟在 `state` 后面。
/// 客户端按 `cmd-wire` 的类型认这两个字面量。
#[test]
fn mig1_session_state_has_exactly_these_bytes() {
    use crate::wire::SessionFate;
    for (state, word) in [
        (SessionFate::Reconnectable, "reconnectable"),
        (SessionFate::Ended, "ended"),
    ] {
        let face = serde_json::to_string(
            &<crate::wire::SessionFace as crate::wire::SessionFaceExt>::of(match state {
                SessionFate::Reconnectable => crate::wire::Life::Reconnectable,
                SessionFate::Ended => crate::wire::Life::Ended,
            }),
        )
        .unwrap();
        assert_eq!(
            to_line(&Frame::session_state("abc".into(), state, None)).unwrap(),
            format!("{{\"kind\":\"session_state\",\"sid\":\"abc\",\"state\":\"{word}\",\"face\":{face}}}\n")
        );
    }
}

/// `sessions_replayed` 的**逐字节**金标准：无载荷，只有 kind。
/// 客户端按 `cmd-wire` 的类型认这个字面量。
#[test]
fn sessions_replayed_has_exactly_these_bytes() {
    assert_eq!(
        to_line(&Frame::SessionsReplayed(crate::wire::SessionsReplayed {})).unwrap(),
        "{\"kind\":\"sessions_replayed\"}\n"
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
            tmux: None,
        }),
        name_face: crate::history::search_rules::name_face(&Default::default()),
        waiting_for: None,
        hidden: None,
        container: None,
        pid: 4242,
    }))
    .unwrap();
    assert_eq!(
        line,
        "{\"kind\":\"session_added\",\"sid\":\"s\",\"agent_kind\":\"claude\",\"face\":{\"dot\":\"idle\",\"live\":{\"is\":\"yes\"},\"settled\":false,\"name\":\"状态不明\",\"tip\":\"状态不明\",\"line\":\"状态不明\",\"tone\":\"plain\",\"order\":3,\"can\":{\"close\":{\"can\":\"no\",\"why\":\"运行中\"},\"resume\":{\"can\":\"no\",\"why\":\"运行中\"},\"term\":{\"can\":\"yes\",\"tmux\":false},\"reattach\":{\"can\":\"no\",\"why\":\"运行中\"},\"model\":{\"can\":\"yes\"},\"permission\":{\"can\":\"yes\"}}},\"label\":\"没有说过话的会话\",\"pid\":4242}\n"
    );
}

// ═══ hello 回显宿主交来的那几格（`host_env`）═══════════════════════════════
//
// 要求：「常驻后端身份带数据目录（接错了拒并出声）」；`INVARIANTS §42` → `IPC-PROTOCOL.md §10` hello 那一行
// （additive：空表省略、线上字节不变）。

/// 🔴 I1a：回显恰是名单内被交了的那几格、原样（手写期望）；没交 / 空串的那一格不回显；常驻开关与无关变量即使在环境里也不回显。
#[test]
fn hx2_host_env_echoes_exactly_the_handed_names_and_nothing_else() {
    let env: std::collections::HashMap<&str, &str> = [
        ("CCM_RELAY_PORT", "8788"),
        ("CCM_DATA_DIR", "/iso/home"),
        ("CCM_APIKEY_CREDENTIALS", "/d/apikey-credentials.json"),
        (crate::wire::listen::ENV_RESIDENT, "1"),
        ("HOME", "/home/u"),
    ]
    .into_iter()
    .collect();
    let got = crate::wire::host_env_from(|n| env.get(n).map(|v| v.to_string()));
    let want: std::collections::BTreeMap<String, String> = [
        ("CCM_RELAY_PORT".to_string(), "8788".to_string()),
        ("CCM_DATA_DIR".to_string(), "/iso/home".to_string()),
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want);
    assert!(crate::wire::host_env_from(|_| None).is_empty());
}

/// 🔴 I1b：回显名单恰两格（== 手写）；常驻开关不在其中。
#[test]
fn hx2_the_echo_list_is_exactly_the_two_handed_names() {
    let names: std::collections::BTreeSet<&str> = crate::wire::HOST_ECHO_ENVS.into_iter().collect();
    let want: std::collections::BTreeSet<&str> =
        ["CCM_RELAY_PORT", "CCM_DATA_DIR"].into_iter().collect();
    assert_eq!(names, want);
    assert!(!names.contains(crate::wire::listen::ENV_RESIDENT));
}

/// 🔴 I1c：空表 ⇒ 省略（线上字节与既有冻结串逐字节相同）；有值 ⇒ 落在最后、键按名排序。生产那一行恰好一处、读的是真环境。
#[test]
fn hx2_production_hello_bytes_do_not_change_when_nothing_was_handed() {
    let hello = |host_env: std::collections::BTreeMap<String, String>| {
        to_line(&Frame::Hello(crate::wire::Hello {
            build_id: "b".into(),
            host_arch: "x86_64".into(),
            commands: vec![],
            unavailable: vec![],
            host_env,
            shapes: Default::default(),
            frames: Default::default(),
        }))
        .unwrap()
    };
    assert_eq!(
        hello(Default::default()),
        "{\"kind\":\"hello\",\"build_id\":\"b\",\"host_arch\":\"x86_64\"}\n"
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
        "{\"kind\":\"hello\",\"build_id\":\"b\",\"host_arch\":\"x86_64\",\
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

// ═══ 第二个前端在读的形状：冻结（只许加字段）═══════════════════════════════
//
// 两个前端吃同一个后端：桌面端（monitor）与第二个前端（手机端）。后者按下表逐字段读帧，缺一格就把整帧当坏帧丢
// （症状是「会话列表有、点进去永远空白」，零报错）。⇒ 表里登记的每一格不许改名、删、换类型；加新字段照旧（additive）。
// 会话三帧（`session_added` · `session_status` · `session_removed`）在格目录里，冻结的格登在那边（`cells_catalog::tests::FROZEN`），不在这张表。

/// （帧 kind 或 `request`, 字段, JSON 类型）。`request` 是入方向请求信封。
pub(crate) const SECOND_FRONTEND_READS: &[(&str, &str, &str)] = &[
    ("hello", "build_id", "string"),
    ("hello", "host_arch", "string"),
    ("line", "session_id", "string"),
    ("line", "path", "string"),
    ("line", "seq", "number"),
    ("line", "byte_offset", "number"),
    // 〔第二个前端 2026-10-10〕通用记录那一团。里面的格（`said.results` 的 `patch` · `patchTruncated` · `file` …）整件冻结在格目录
    // （格目录冻结表 `cells_catalog::tests::FROZEN` 的 `record`，`Frozen::All`）；这里只钉「line 帧上有这一格」。
    ("line", "record", "object"),
    ("overflow", "said", "string"),
    // 〔第二个前端 live1 10-13〕**活卡**（聊天屏的逐字长出，核心拼好的成品）。`live_frames_have_exactly_these_bytes` 钉的是字节，
    // 本表钉的是**有人在读它**。⚠ 一份样本只带得动一种 `op`：取整卡（`open`）那一支；增量与撤卡那几种的字节由逐字节判据钉。
    ("live_card", "sid", "string"),
    ("live_card", "run", "string"),
    ("live_card", "card", "number"),
    ("live_card", "n", "number"),
    ("live_card", "op", "string"),
    ("live_card", "rid", "string"),
    ("live_card", "phase", "string"),
    ("live_card", "clipped", "bool"),
    ("live_card", "blocks", "array"),
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
        Frame::Hello(crate::wire::Hello {
            build_id: "b".into(),
            host_arch: "x86_64".into(),
            commands: vec![],
            unavailable: vec![],
            host_env: Default::default(),
            shapes: Default::default(),
            frames: Default::default(),
        }),
        Frame::Line(crate::wire::Line {
            session_id: "s".into(),
            path: "/p".into(),
            seq: 1,
            record: Some(said_record("u1", "hi")),
            cwd: None,
            byte_offset: 9,
            rid: None,
        }),
        Frame::overflow(2, true),
        Frame::LiveCard(crate::wire::LiveCard {
            sid: "s".into(),
            run: s.clone(),
            card: 1,
            n: 0,
            op: crate::wire::LiveOp::Open {
                rid: "r-1".into(),
                phase: crate::wire::LivePhase::Streaming,
                clipped: false,
                blocks: vec![crate::wire::LiveBlock {
                    kind: crate::agents::BlockKind::Text,
                    text: Some("hi".into()),
                    tool: None,
                }],
            },
        }),
        Frame::Reply(crate::wire::Reply {
            id: "1".into(),
            ok: false,
            code: s.clone(),
            message: s.clone(),
            detail: s.clone(),
            data: Some(serde_json::json!({})),
        }),
        Frame::Cancelled(crate::wire::Cancelled { id: "1".into() }),
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

/// 请求信封的 `tz`（看的那一台的时区）：认得的 IANA 名 ⇒ 那个时区；缺 · 认不得 · 不是串 ⇒ UTC（不拒，同 `within_ms` 的宽读）。
/// 推出去的帧在写出去那一下按这条流的时区写钟面（[`Frame::stamp`]）：`line.record.timeText`。
#[test]
fn the_envelope_carries_the_viewer_zone_and_pushed_lines_are_stamped_by_it() {
    let sh = crate::Tz::named("Asia/Shanghai").unwrap();
    let tz_of = |line: &str| serde_json::from_str::<Request>(line).expect("信封认得").tz;
    assert_eq!(tz_of(r#"{"id":"1","cmd":"ping","tz":"Asia/Shanghai"}"#), sh);
    for line in [
        r#"{"id":"1","cmd":"ping"}"#,
        r#"{"id":"1","cmd":"ping","tz":"Nowhere/Atlantis"}"#,
        r#"{"id":"1","cmd":"ping","tz":480}"#,
        r#"{"id":"1","cmd":"ping","tz":null}"#,
    ] {
        assert_eq!(tz_of(line), crate::Tz::default(), "{line}");
    }
    let rec = crate::agents::claudecode::parse::translated(&crate::agents::Line::new(r#"{"type":"user","uuid":"u","timestamp":"2026-10-07T20:30:00.000Z","message":{"role":"user","content":"hi"}}"#, 0))
    .unwrap()
    .unwrap()
    .record;
    let mut f = Frame::Line(crate::wire::Line {
        session_id: "s".into(),
        path: "/p".into(),
        seq: 0,
        record: rec,
        cwd: None,
        byte_offset: 0,
        rid: None,
    });
    assert!(
        !to_line(&f).unwrap().contains("timeText"),
        "产帧那一层不写钟面"
    );
    f.stamp(&sh);
    assert!(
        to_line(&f).unwrap().contains(r#""timeText":"04:30""#),
        "{}",
        to_line(&f).unwrap()
    );
}

// ═══ 第二个前端调的一次性子命令：冻结（叫法）══════════════════════════════════════
//
// 10-10 起手机只调这三条（其余都走常驻流上的帧命令）：探后端 · 起 / 找常驻后端 · 接常驻后端的小中继。

/// 第二个前端调的一次性子命令（都不带位置参数；`--resident-ensure` 认一个 `--replace`）。叫法一改它那一侧就叫不通了。
const SECOND_FRONTEND_SUBCOMMANDS: &[&str] =
    &["--backend-probe", "--resident-ensure", "--resident-attach"];

/// 每一条都还在子命令表里、还进一次性模式。
#[test]
fn the_subcommands_the_second_frontend_calls_stay_put() {
    for flag in SECOND_FRONTEND_SUBCOMMANDS {
        assert!(crate::SUBCOMMANDS.contains(flag), "{flag} 不在子命令表里了");
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
    use crate::wire::{Life, SessionFate};
    let status = |a: Option<A>| {
        serde_json::to_value(Frame::<crate::agents::record::Record>::SessionStatus(
            crate::wire::SessionStatus {
                sid: "s".into(),
                face: crate::wire::SessionFace::of(Life::Live {
                    activity: a,
                    tmux: None,
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
        (A::Working, "beSession.activity.working", "now"),
        (A::NeedsYou, "beSession.activity.needsYou", "need"),
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
        copy_core::copy_text("beSession.activity.unclear", &[])
    );
    assert_ne!(
        none["name"],
        copy_core::copy_text("beSession.activity.working", &[]),
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
        serde_json::to_value(crate::wire::SessionFace::of(Life::Reading { tmux: None })).unwrap();
    assert_eq!(reading["tone"], "plain");
    assert_eq!(reading["order"], none["order"]);
    assert!(
        reading["live"].get("activity").is_none(),
        "读记录中不说判出了哪一态"
    );

    for (f, name, hint) in [
        (
            SessionFate::Reconnectable,
            "sessionState.reconnectable.name",
            "sessionState.reconnectable.tooltip",
        ),
        (
            SessionFate::Ended,
            "sessionState.ended.name",
            "sessionState.ended.tooltip",
        ),
    ] {
        let v = &serde_json::to_value(Frame::session_state("s".into(), f, None)).unwrap()["face"];
        assert_eq!(v["name"], copy_core::copy_text(name, &[]), "{f:?}");
        assert_eq!(v["tip"], copy_core::copy_text(hint, &[]), "{f:?}");
        assert_eq!(v["tone"], "plain", "{f:?}");
    }
}

/// `changed` 的小成品超了主题表那一格的上限 ⇒ 不带（客户端重问）；主题不带小成品的给了也不带。
#[test]
fn a_changed_body_over_its_cap_is_left_off() {
    let cap = Topic::Plan.spec().body_cap;
    let big = serde_json::json!({ "needs": 1, "pad": "x".repeat(cap) });
    match Frame::changed(Topic::Plan, Some("/w".into()), None, Some(big)) {
        Frame::Changed(crate::wire::Changed { body, .. }) => {
            assert_eq!(body, None, "超了上限还带着")
        }
        other => panic!("{other:?}"),
    }
    match Frame::changed(Topic::Accounts, None, None, Some(serde_json::json!({}))) {
        Frame::Changed(crate::wire::Changed { body, .. }) => {
            assert_eq!(body, None, "不带小成品的主题带上了")
        }
        other => panic!("{other:?}"),
    }
}

/// `changed` 丢了能不能恢复照主题表答。
#[test]
fn changed_loss_follows_the_topic_table() {
    for t in Topic::ALL {
        let f = Frame::changed(t, None, None, None);
        assert_eq!(f.loss_is_recoverable(), t.spec().lossy, "{t:?}");
    }
}

/// 8a b：整份主线外清单切成几帧 `session_branch`：每帧一行 ≤ `REPLY_PAGE_BYTES`；`at` 依次接上；拼起来 ＝ 原清单；
/// 只发变了的那一截时从 `at` 起；后面什么都没有也发一帧空的（说「到此为止」）。
#[test]
fn a_long_branch_list_goes_out_in_page_sized_frames_that_add_up() {
    let off: Vec<String> = (0..40_000)
        .map(|i| format!("{i:08}-0000-4000-8000-000000000000"))
        .collect();
    let frames = Frame::branch_pages("s", "/p/s.jsonl", &off, 0);
    assert!(frames.len() > 1, "要分好几帧：{}", frames.len());
    let mut all = Vec::new();
    for f in &frames {
        let line = to_line(f).unwrap();
        assert!(
            line.len() <= crate::REPLY_PAGE_BYTES,
            "一帧 {} 字节",
            line.len()
        );
        let Frame::SessionBranch(crate::wire::SessionBranch { at, off, sum, .. }) = f else {
            panic!()
        };
        assert_eq!(*at as usize, all.len());
        all.extend(off.iter().cloned());
        // read8c：每帧的摘要 ＝ 到它末尾为止那一截的摘要（客户端交回的就是它）。
        assert_eq!(
            *sum,
            crate::wire::branch_sum(&all),
            "第 {at} 条起那一帧的摘要不是到它末尾的那一截"
        );
    }
    assert_eq!(all, off);
    let tail = Frame::branch_pages("s", "/p/s.jsonl", &off[..10], 8);
    assert!(
        matches!(&tail[..], [Frame::SessionBranch(crate::wire::SessionBranch { at: 8, off: o, sum, .. })] if o.len() == 2 && *sum == crate::wire::branch_sum(&off[..10]))
    );
    // 摘要跨进程稳定：钉一个数（FNV-1a 64），换了算法 ⇒ 客户端交回的旧摘要全对不上（只是多发整份，但要有意为之）。
    assert_eq!(crate::wire::branch_sum(&[]), "cbf29ce484222325");
    assert_eq!(
        crate::wire::branch_sum(&["a".to_string()]),
        format!(
            "{:016x}",
            (((0xcbf2_9ce4_8422_2325u64 ^ 0x61).wrapping_mul(0x0100_0000_01b3)) ^ 0x0a)
                .wrapping_mul(0x0100_0000_01b3)
        )
    );
    let none = Frame::branch_pages("s", "/p/s.jsonl", &off[..3], 3);
    assert!(
        matches!(&none[..], [Frame::SessionBranch(crate::wire::SessionBranch { at: 3, off, .. })] if off.is_empty())
    );
}

/// 整份脸上能不能换模型 · 权限档（`can.model` · `can.permission`）由核心判、随那一态带：
/// 在跑 ⇒ 模型灰、权限档可换；需手动 ⇒ 都灰；闲着 · 后台命令在跑 · 说不清 · 读记录中 ⇒ 都可换；
/// 离开「活」（可重连 · 已结束）⇒ 都可换（记在注解上、下次恢复用上）；记录已不在 ⇒ 都灰（恢复不了，那一句同恢复）。
/// 灰的那一格（`can: no`）带写好的那一句，可换的（`can: yes`）不带。
#[test]
fn session_frames_say_whether_model_and_permission_can_switch_now() {
    use crate::agents::SessionActivity as A;
    use crate::wire::Life;
    let yes = serde_json::json!({"can": "yes"});
    let no = |key: &str| serde_json::json!({"can": "no", "why": copy_core::copy_text(key, &[])});
    let switch = |life: Life| {
        let c = serde_json::to_value(crate::wire::SessionFace::of(life)).unwrap()["can"].clone();
        serde_json::json!({"model": c["model"], "permission": c["permission"]})
    };
    let live = |a: Option<A>| Life::Live {
        activity: a,
        tmux: None,
    };
    assert_eq!(
        switch(live(Some(A::Working))),
        serde_json::json!({"model": no("beSessionSwitch.why.running"), "permission": yes})
    );
    assert_eq!(
        switch(live(Some(A::NeedsYou))),
        serde_json::json!({"model": no("beSessionSwitch.why.waiting"), "permission": no("beSessionSwitch.why.waiting")})
    );
    for life in [
        live(Some(A::Idle)),
        live(Some(A::BackgroundWork)),
        live(None),
        Life::Reading { tmux: None },
    ] {
        assert_eq!(
            switch(life),
            serde_json::json!({"model": yes, "permission": yes})
        );
    }
    for life in [Life::Reconnectable, Life::Ended] {
        assert_eq!(
            switch(life),
            serde_json::json!({"model": yes, "permission": yes}),
            "{life:?}"
        );
    }
    assert_eq!(
        switch(Life::Gone),
        serde_json::json!({"model": no("sessionState.gone.tooltip"), "permission": no("sessionState.gone.tooltip")})
    );
}

/// 会话整份脸的金样住址（仓根相对）：桌面与手机读同一份，各自对它画点。
const SESSION_FACE_GOLDEN: &str = "tests/__fixtures__/session-face.golden.json";

/// 每一段一份整脸（点 · 字 · 序 · 能做什么）＝ 金样。形状或字是有意改的 ⇒ `REGOLD_SESSION_FACE=1` 重打，连两端一起对。
#[test]
fn every_life_has_one_whole_face_and_it_matches_the_golden() {
    use crate::agents::SessionActivity as A;
    use crate::wire::{Life, SessionFace};
    let lives: Vec<(&str, Life)> = vec![
        (
            "live-needs-you-tmux",
            Life::Live {
                activity: Some(A::NeedsYou),
                tmux: Some(true),
            },
        ),
        (
            "live-working-plain",
            Life::Live {
                activity: Some(A::Working),
                tmux: Some(false),
            },
        ),
        (
            "live-background",
            Life::Live {
                activity: Some(A::BackgroundWork),
                tmux: None,
            },
        ),
        (
            "live-idle",
            Life::Live {
                activity: Some(A::Idle),
                tmux: None,
            },
        ),
        (
            "live-unclear",
            Life::Live {
                activity: None,
                tmux: None,
            },
        ),
        ("reading", Life::Reading { tmux: Some(true) }),
        ("reconnectable", Life::Reconnectable),
        ("ended", Life::Ended),
        ("gone", Life::Gone),
        ("unclear", Life::Unclear),
    ];
    let produced: serde_json::Map<String, Value> = lives
        .into_iter()
        .map(|(k, l)| {
            (
                k.to_string(),
                serde_json::to_value(SessionFace::of(l)).unwrap(),
            )
        })
        .collect();
    let produced = serde_json::to_string_pretty(&produced).unwrap() + "\n";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(SESSION_FACE_GOLDEN);
    if std::env::var_os("REGOLD_SESSION_FACE").is_some() {
        std::fs::write(&path, &produced).expect("写金样");
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        on_disk == produced,
        "会话整份脸与金样不一致（有意改的 ⇒ `REGOLD_SESSION_FACE=1` 重打）：\n{produced}"
    );
}

/// 会话帧上那几格写好的字（此刻在干什么 · 能不能换 · 叫什么）的格是公开的（客户端要解），
/// 「只经那几个函数造」由这条守：全仓（源码 ＋ 判据）写 `SessionFace {` · `NameFace {` 结构字面量的地方
/// ＝ 后端 `wire` 那几个构造口（集合相等）。删除条件：哪天 Rust 能按 crate 限定构造。
#[test]
fn the_written_faces_are_built_in_one_place_only() {
    let repo = guard_core::addr::repo_root();
    let mut seen = std::collections::BTreeSet::new();
    for dir in ["src", "tests"] {
        for (path, raw) in guard_core::scan_tree!(&repo.join(dir), &["rs"]) {
            if path.starts_with(repo.join("src/vendor")) {
                continue;
            }
            let code = guard_core::production_code(&raw);
            let rel = path
                .strip_prefix(&repo)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            // 运行时拼，免得命中本条自己。
            for name in ["SessionFace", "NameFace"] {
                let needle = format!("{name} {{");
                for (at, _) in code.match_indices(&needle) {
                    let before = &code[..at];
                    if before
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                    {
                        continue;
                    }
                    let line = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
                    // 类型声明 · 实现头 · 返回类型那一处不是造。
                    if ["struct", "enum", "impl", "for", "trait", "->"]
                        .iter()
                        .any(|k| guard_core::contains_word(line, k))
                    {
                        continue;
                    }
                    let fun = before
                        .rfind("fn ")
                        .map(|i| {
                            before[i + 3..]
                                .chars()
                                .take_while(|c| c.is_alphanumeric() || *c == '_')
                                .collect::<String>()
                        })
                        .unwrap_or_default();
                    seen.insert(format!("{rel} · {fun} · {name}"));
                }
            }
        }
    }
    let want: std::collections::BTreeSet<String> = [
        "of · SessionFace",
        "live_face · SessionFace",
        "of_parts · NameFace",
    ]
    .iter()
    .map(|s| format!("src/backend/wire/mod.rs · {s}"))
    .collect();
    assert_eq!(
        seen, want,
        "会话那几格写好的字只许经后端 `wire` 的构造口造（`SessionFace::of` · `NameFace::of_parts`）"
    );
}

/// 会话帧 · 清单行上「此刻是什么样」只有整份脸一格：平铺的旧格（活动三格 · 去向三格 · 两个序 · 能不能换）
/// 在生产源（`src/` 的 rs · ts · kt，注释不算）零命中；正控：同一把尺在三种文件里都量得到 `SessionFace`。
#[test]
fn the_flat_state_cells_are_gone_from_the_sources() {
    let repo = guard_core::addr::repo_root();
    // 运行时拼，免得命中本条自己。
    let gone: Vec<String> = [
        ["Activity", "Face"],
        ["Can", "Switch"],
        ["activity", "_text"],
        ["activity", "_tone"],
        ["activity", "_order"],
        ["state", "_text"],
        ["state", "_hint"],
        ["state", "_tone"],
        ["state", "_order"],
    ]
    .iter()
    .map(|p| p.concat())
    .collect();
    let mut hits = Vec::new();
    let mut control = std::collections::BTreeSet::new();
    for (path, raw) in guard_core::scan_tree!(&repo.join("src"), &["rs", "ts", "kt"]) {
        if path.starts_with(repo.join("src/vendor"))
            || path.components().any(|c| c.as_os_str() == "node_modules")
        {
            continue;
        }
        let code = guard_core::production_code(&raw);
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        if guard_core::contains_word(&code, "SessionFace") {
            control.insert(ext);
        }
        let rel = path
            .strip_prefix(&repo)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        for g in &gone {
            if code.contains(g.as_str()) {
                hits.push(format!("{rel} · {g}"));
            }
        }
    }
    assert_eq!(
        control,
        ["kt", "rs", "ts"].iter().map(|s| s.to_string()).collect(),
        "正控：三种文件里都该量得到 `SessionFace`（没量到 ⇒ 本条在空转）"
    );
    assert!(hits.is_empty(), "平铺的旧状态格还在：{hits:#?}");
}

/// 监控板组内一把尺：可重连 · 已结束的 `session_state` 那份脸的序，都排在任何活会话的序之后；
/// 界面只按这一格排、不再自己按状态分段。
#[test]
fn the_fates_sort_after_every_live_session_on_one_ruler() {
    use crate::agents::SessionActivity as A;
    let live: std::collections::BTreeSet<u64> = [
        None,
        Some(A::NeedsYou),
        Some(A::Working),
        Some(A::BackgroundWork),
        Some(A::Idle),
    ]
    .into_iter()
    .map(|a| {
        u64::from(
            crate::wire::SessionFace::of(crate::wire::Life::Live {
                activity: a,
                tmux: None,
            })
            .order,
        )
    })
    .collect();
    let fate = |f| {
        serde_json::to_value(Frame::session_state("s".into(), f, None)).unwrap()["face"]["order"].as_u64()
    };
    assert_eq!(live, [0, 1, 2, 3].into());
    assert_eq!(
        (
            fate(crate::wire::SessionFate::Reconnectable),
            fate(crate::wire::SessionFate::Ended)
        ),
        (Some(4), Some(5))
    );
}
