//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md §10`（backend → client 线上契约）
//!
//! 核原文：`IPC-PROTOCOL.md §10` 逐字「每行恰好一个 UTF-8 JSON 对象，`\n` 结尾，对象内无裸 `\n`/`\r`」；同节帧表逐格写了各字段的
//! additive 规则与今天的取值 —— 本族判出方向帧的字节与那张表一致。出方向单写者那两条服务同一节的「每行恰好一个对象」与「首帧」；
//! 它不单独立条是 V97 的裁定（「SSH 流单写者」不升）。〔JA1 点址 2026-09-24〕

/// ★★ **出方向帧只许有一个写者**〔audit-0805 08-08，Phase G 第 60 件，D6〕。
///
/// `inbound.rs` 头注逐字写着「`writer_task`（**出方向帧的唯一出口**）」。
/// 那句话撑着 NDJSON 在线上的完整性：两个写者并发写同一个 stdout，
/// 帧就会**互相撕开**（半行 + 半行），而这条流**仓外 aterm 正在消费**（D6 契约冻结）。
///
/// ⇒ 而它只是散文。08-08 实测：在 `inbound.rs` 加一个自己 `tokio::io::stdout()`
/// 并 `write_all` 的函数，**backend 292 条判据一条不红**。
///
/// # 人群与豁免
///
/// 人群 = **流式那条路**（`main.rs` / `inbound.rs` / `wire.rs` / `listen.rs`）生产段里所有
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
    // ⚠ 〔`P4` 2026-09-21〕**那一刀今天不生效**：判据搬来 `tests/backend/` 之后由
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
        // 只看流式那条路的四个文件；`observe/*_query.rs` 是一次性子命令，理由见头注。
        if !matches!(
            name.as_str(),
            "wire.rs" | "main.rs" | "inbound.rs" | "listen.rs"
        ) {
            continue;
        }
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

#[test]
fn each_variant_serializes_to_single_line_with_expected_kind() {
    let cases: Vec<(Frame, &str)> = vec![
        (
            Frame::Hello {
                v: 1,
                build_id: "b".into(),
                host_arch: "x86_64".into(),
                claude_dir: "/home/u/.claude".into(),
                homes: vec![],
                capabilities: vec!["bg".into(), "tail-only".into()],
                emits: vec!["line".into(), "session_status".into()],
                commands: vec![],
                unavailable: vec![],
                host_env: Default::default(),
                uncancellable: vec![],
            },
            "hello",
        ),
        (
            Frame::Line {
                session_id: "s".into(),
                path: "/p".into(),
                seq: 0,
                message: None,
                cwd: None,
                byte_offset: 0,
            },
            "line",
        ),
        (
            Frame::SessionAdded {
                sid: "s".into(),
                agent_kind: None,
                liveness_confidence: None,
                session_kind: None,
                attachable: None,
                cwd: None,
                name: None,
                path: None,
                lines: None,
                status: None,
                waiting_for: None,
                rbind_token: None,
                container: None,
                pid: None,
            },
            "session_added",
        ),
        (
            Frame::SessionStatus {
                sid: "s".into(),
                status: Some("busy".into()),
                waiting_for: None,
                liveness_confidence: None,
            },
            "session_status",
        ),
        (
            Frame::SessionRemoved {
                sid: "s".into(),
                cause: RemovalCause::Gone,
            },
            "session_removed",
        ),
        (
            Frame::TurnEnd {
                session_id: "s".into(),
                uuid: "u".into(),
            },
            "turn_end",
        ),
        (
            Frame::Overflow {
                dropped: 7,
                lost: Vec::new(),
                lost_truncated: false,
            },
            "overflow",
        ),
        // 〔audit-0805 08-06〕补上此前**测试段零构造**的三个变体。
        // `Reply` 的上线形另有 `inbound.rs` 钉着；`TmuxSessionClosed` / `Cancelled`
        // 此前**只有 monitor 侧「解析成 None」的负向断言** —— 那是消费方的行为，
        // 不是后端序列化形态：改掉 kind 标签或字段名，两边都不会红。
        (
            Frame::Reply {
                id: "r1".into(),
                ok: true,
                code: None,
                message: None,
                data: None,
            },
            "reply",
        ),
        (Frame::Cancelled { id: "r1".into() }, "cancelled"),
        // 〔SR1a〕账号清单变了（无载荷；逐字节形状另由 `link_frames_have_exactly_these_bytes` 钉）。
        (Frame::AccountsChanged, "accounts_changed"),
        // 〔MIG-3b · ㉓②〕某个会话的任务清单变了（只带 sid；逐字节形状由 `link_frames_have_exactly_these_bytes` 钉）。
        (Frame::TasksChanged { sid: "s1".into() }, "tasks_changed"),
        // 〔SR1a〕链路两帧（逐字节形状另由 `link_frames_have_exactly_these_bytes` 钉）。
        (
            Frame::LinkData {
                link: "L".into(),
                data: "AA==".into(),
            },
            "link_data",
        ),
        (
            Frame::LinkEnd {
                link: "L".into(),
                error: None,
            },
            "link_end",
        ),
        // 〔SR1b〕传输进度 / 终局（逐字节形状另由 `transfer_frames_have_exactly_these_bytes` 钉）。
        (
            Frame::Transfer {
                id: "xfer-1".into(),
                got: 0,
                total: 0,
                end: None,
            },
            "transfer",
        ),
        (
            Frame::Probe {
                ticket: "t-1".into(),
                cell: serde_json::json!({"reached": "ssh"}),
            },
            "probe",
        ),
        // 〔P7〕长活的一格进度（格原样；全景是上游 `IndexProgress`）。
        (
            Frame::Progress {
                ticket: "t-1".into(),
                cell: serde_json::json!({"phase": "Parse", "done": 1, "total": 2}),
            },
            "progress",
        ),
        (Frame::SessionsReplayed, "sessions_replayed"),
        // 〔MIG-1〕会话账本的成品（逐字节形状另由 `mig1_session_state_has_exactly_these_bytes` 钉）。
        (
            Frame::SessionState {
                sid: "s".into(),
                state: crate::stream::wire::SessionFate::Ended,
            },
            "session_state",
        ),
        // 〔TAP〕中转抄出来的 SSE 事件（逐字节形状另由 `tap_frames_have_exactly_these_bytes` 钉）。
        (
            Frame::Tap {
                stream: "s".into(),
                resp: 0,
                n: 0,
                data: Some("{}".into()),
                end: None,
            },
            "tap",
        ),
        // 〔FW1 · 第四波 4D〕活会话的记录文件不见了 / 被改过已从头重读（逐字节形状另由
        //   `watcher_tests::the_two_session_file_frames_have_exactly_these_bytes` 钉）。
        (
            Frame::SessionFileGone {
                session_id: "s".into(),
                path: "/p".into(),
            },
            "session_file_gone",
        ),
        (
            Frame::SessionFileReread {
                session_id: "s".into(),
                path: "/p".into(),
                why: crate::stream::wire::RereadWhy::Rewritten,
            },
            "session_file_reread",
        ),
    ];

    // ★ 人群自检：**样本必须覆盖 `Frame` 的每一个变体**〔audit-0805 08-06〕。
    //
    // 原来这里是一张**手写清单**：11 个变体只列了 8 个，而加第 12 个变体时
    // **没有任何东西会红** —— 一条上线契约帧就那样进了协议。
    // ⇒ 把人群换成**枚举本身**：从源码数 `pub enum Frame` 的变体数，
    // 与样本产出的**去重 kind 数**对拍。加变体不补样本 ⇒ 当场红。
    let src = include_str!("../../../src/backend/stream/wire.rs");
    let beg = src.find("pub enum Frame").expect("找不到 Frame 枚举");
    let end = src[beg..].find("\n}\n").expect("找不到枚举结尾") + beg;
    let variant_count = src[beg..end]
        .lines()
        .filter(|l| {
            let t = l.trim_end();
            t.starts_with("    ")
                && !t.starts_with("     ")
                && t.trim_start().starts_with(|c: char| c.is_ascii_uppercase())
                && (t.contains('{') || t.ends_with(','))
        })
        .count();
    assert!(
        variant_count >= 10,
        "只数出 {variant_count} 个 Frame 变体（08-06 实测 11）—— 抽取坏了，下面的对拍会空转"
    );

    let mut kinds: Vec<&str> = Vec::new();
    for (frame, expected_kind) in &cases {
        let line = to_line(frame).expect("serialize");
        assert_eq!(&parse_kind(&line), expected_kind);
        if !kinds.contains(expected_kind) {
            kinds.push(expected_kind);
        }
    }
    assert_eq!(
        kinds.len(),
        variant_count,
        "样本覆盖 {} 个 kind，而 `Frame` 有 {variant_count} 个变体 —— \n\
             新增变体没补样本：它的上线形态（kind 标签 / 字段名 / 单行）此刻无人钉。\n\
             ⚠ 这是**契约帧**，`backend-api` 的 D6 写着「暴露给第三方 = 契约冻结成本」，\n\
             而 aterm 正在消费这条流。\n\
             已覆盖：{kinds:?}",
        kinds.len()
    );
}

/// 〔SR1a〕★ W1：链路两帧 ＋ `accounts_changed` 的**逐字节**金标准。`link_end` 的 `error` 缺席时不上线（正常收尾）。
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
        (
            Frame::TasksChanged { sid: "s1".into() },
            "{\"kind\":\"tasks_changed\",\"sid\":\"s1\"}\n",
        ),
    ];
    for (f, want) in cases {
        assert_eq!(to_line(&f).unwrap(), want);
    }
}

/// 〔SR1b〕★ B8：`transfer` 帧的**逐字节**金标准（四形：进行中 · 传完 · 失败 · 撤）；`end` 缺席时不上线；
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
        // 〔FW1 · 第四波 4D〕上传那一路的传完带整份摘要（提交时的对拍依据）；下载那一路没有 ⇒ 上一格原样不上线。
        (
            f(Some(crate::stream::wire::TransferEnd::Done {
                bytes: 1000000,
                sha256: Some("ab".repeat(32)),
            })),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"done\",\"bytes\":1000000,\"sha256\":\"abababababababababababababababababababababababababababababababab\"}}\n",
        ),
        (
            f(Some(crate::stream::wire::TransferEnd::Failed { why: "写暂存件失败".into(), code: None })),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"failed\",\"why\":\"写暂存件失败\"}}\n",
        ),
        // 〔FILES2 · Q5〕带码的那一形（今天只有 SFTP 起始目录不是后端 home 那一码）。
        (
            f(Some(crate::stream::wire::TransferEnd::Failed { why: "w".into(), code: Some("sftp_home_mismatch".into()) })),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"failed\",\"why\":\"w\",\"code\":\"sftp_home_mismatch\"}}\n",
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

/// 〔TAP · V124〕`tap` 两形的逐字节线上形状（期望是手写字面量；monitor 侧 `parse_frame` 拿同样的串核自己）。
/// `data` 是**一个 JSON 串**（上游字节敌手可控，不参与帧结构）；`data` 与 `end` 恰有一个。可丢（SSE 只保快，V24）。
#[test]
fn tap_frames_have_exactly_these_bytes() {
    let cases = [
        (
            Frame::Tap {
                stream: "0b6c1f7e-sid".into(),
                resp: 12,
                n: 3,
                data: Some("{\"type\":\"ping\"}".into()),
                end: None,
            },
            "{\"kind\":\"tap\",\"stream\":\"0b6c1f7e-sid\",\"resp\":12,\"n\":3,\"data\":\"{\\\"type\\\":\\\"ping\\\"}\"}\n",
        ),
        (
            Frame::Tap {
                stream: "0b6c1f7e-sid".into(),
                resp: 12,
                n: 9,
                data: None,
                end: Some(crate::stream::wire::TapEnd::Done),
            },
            "{\"kind\":\"tap\",\"stream\":\"0b6c1f7e-sid\",\"resp\":12,\"n\":9,\"end\":\"done\"}\n",
        ),
        (
            Frame::Tap {
                stream: "0b6c1f7e-sid".into(),
                resp: 12,
                n: 9,
                data: None,
                end: Some(crate::stream::wire::TapEnd::Broken),
            },
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

/// 〔SR1a〕base64 编解码对 **RFC 4648 §10** 的七条标准向量（异源 = RFC；monitor 侧那一份拿同一组向量核自己）。
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

// 〔MIG-1 续 · V41〕`tmux_sessions` 帧单行转义那条随帧删了（tmux 原文只在进程内喂会话账本）。

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

#[test]
fn line_with_quotes_backslashes_and_newline_roundtrips() {
    let raw = "before\"quote\\backslash\nafter-newline";
    let frame = Frame::Line {
        session_id: "sid".into(),
        path: "/some/path.jsonl".into(),
        seq: 42,
        message: Some(serde_json::json!({ "text": raw })),
        cwd: None,
        byte_offset: 99,
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
    assert_eq!(v["message"]["text"], raw);
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
        crate::agents::Adapter { kind: "synthetic", home: synth_present, account_env: None, assets: None, history: None, upstream: None, mcp: None, footprint: None, records: None, processes: None },
        crate::agents::Adapter { kind: "ghost",     home: synth_absent, account_env: None, assets: None, history: None, upstream: None, mcp: None, footprint: None, records: None, processes: None },
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
/// ⚠ **测试名刻意不改**〔`S4`〕：`src/doc/IPC-PROTOCOL.md` 与**仓外 aterm** 都按这个名字
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
        session_kind: None,
        attachable: None,
        cwd: None,
        name: None,
        path: None,
        lines: None,
        status: None,
        waiting_for: None,
        rbind_token: None,
        container: None,
        pid: None,
    })
    .unwrap();
    assert_eq!(
        sa,
        "{\"kind\":\"session_added\",\"sid\":\"s\",\"agent_kind\":\"codex\",\"liveness_confidence\":\"heuristic\"}\n"
    );

    let ss = to_line(&Frame::SessionStatus {
        sid: "s".into(),
        status: Some("busy".into()),
        waiting_for: None,
        liveness_confidence: Some("heuristic".into()),
    })
    .unwrap();
    assert_eq!(
        ss,
        "{\"kind\":\"session_status\",\"sid\":\"s\",\"status\":\"busy\",\"liveness_confidence\":\"heuristic\"}\n"
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
        session_kind: None,
        attachable: None,
        cwd: None,
        name: None,
        path: None,
        lines: None,
        status: None,
        waiting_for: None,
        rbind_token: None,
        container: None,
        pid: None,
    })
    .unwrap();
    assert_eq!(
        sa, "{\"kind\":\"session_added\",\"sid\":\"s\"}\n",
        "agent_kind(缺=claude)/liveness_confidence(缺=authoritative) 省略，字节等价旧形"
    );

    let ss = to_line(&Frame::SessionStatus {
        sid: "s".into(),
        status: Some("idle".into()),
        waiting_for: None,
        liveness_confidence: None,
    })
    .unwrap();
    assert_eq!(
        ss, "{\"kind\":\"session_status\",\"sid\":\"s\",\"status\":\"idle\"}\n",
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
/// `"superseded"` 与 monitor `src/frontend/shell/src/ssh_source.rs` 的解析处逐字一致。
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

/// ★★ `设计/80 §8.7` 步 2：`session_added.rbind_token` 的**线上形状** ——
/// absent 形字节等价 ＋ present 形精确字节。
///
/// # 两条合起来才是那句 additive 承诺（缺任一条它都不成立）
///
/// 同 `homes` / `unavailable` 两族的既有分工（那两处的头注逐字写过这一点）：
/// - **absent**：`None` 被省略 ⇒ 与本字段加进来**之前**逐字节相同。
///   这一格是给**没索要令牌的客户端**（含仓外 aterm，它按精确字节对 fixture、
///   契约冻结 2026-07-18）的红线 —— 而生产路上「没索要就不读」那道闸门
///   由 `watcher_tests::the_launch_token_rides_the_session_added_frame_only_when_the_client_asked`
///   的阴性一钉住，两处合起来才是「默认关」。
/// - **present**：字段按 `wire.rs` 声明序排在**最后**（`waiting_for` 之后）。
///   ⚠ 本行右边那串就是消费侧照着实现的东西 —— 挪字段位置会改它。
#[test]
fn session_added_rbind_token_is_additive_present_and_absent() {
    // ① absent：省略 ⇒ 与本字段加进来之前逐字节相同。
    //    ★ 右边这串与 `dg3_codex_fields_skipped_when_absent_claude_byte_equivalent`
    //      里那串**刻意逐字重复**（同 `hello_unavailable_is_additive_present_and_absent`
    //      的手法）：同一串钉在两处，任何一处被改掉都还有另一处会红。
    let absent = to_line(&Frame::SessionAdded {
        sid: "s".into(),
        agent_kind: None,
        liveness_confidence: None,
        session_kind: None,
        attachable: None,
        cwd: None,
        name: None,
        path: None,
        lines: None,
        status: None,
        waiting_for: None,
        rbind_token: None,
        container: None,
        pid: None,
    })
    .unwrap();
    assert_eq!(
        absent, "{\"kind\":\"session_added\",\"sid\":\"s\"}\n",
        "`rbind_token` 为 `None` 时没被省略 ⇒ `session_added` 的线上字节变了\n\
         ⇒ 仓外 aterm 那份按精确字节对的 fixture 当场对不上（契约冻结 2026-07-18）。\n\
         additive 的全部意义就在这一格。"
    );

    // ② present：真带上时的精确字节（消费侧照这个实现）。
    let present = to_line(&Frame::SessionAdded {
        sid: "s".into(),
        agent_kind: None,
        liveness_confidence: None,
        session_kind: None,
        attachable: None,
        cwd: None,
        name: None,
        path: None,
        lines: None,
        status: None,
        waiting_for: None,
        rbind_token: Some("0123456789abcdef0123456789abcdef".into()),
        container: None,
        pid: None,
    })
    .unwrap();
    assert_eq!(
        present,
        "{\"kind\":\"session_added\",\"sid\":\"s\",\"rbind_token\":\"0123456789abcdef0123456789abcdef\"}\n",
        "`rbind_token` 的线上名 / 位置变了 —— 那个名字是两路共用的契约（`设计/80 §8.7` 那张表钉死）"
    );
}

/// 〔U4b · 第四波〕`session_added.container` 的线上形：**两个字面量 ＋ 缺席**，三格各钉一处。
///
/// - 缺席：与本字段加进来之前逐字节相同（右边那串与上一条 absent 那串刻意逐字重复）。
/// - `tmux` / `none`：字段按声明序排在最后（`rbind_token` 之后）。这两个字面量是 monitor
///   `ssh_source::parse_frame` 照着认的东西 —— 两边各写一遍，对不上时 monitor 把它当「不知道」，
///   而「不知道」是合法值 ⇒ **不会有任何东西报错**，所以这里用精确字节钉。
#[test]
fn session_added_container_is_additive_with_two_literals() {
    let frame = |c: Option<crate::stream::wire::SessionContainer>| {
        to_line(&Frame::SessionAdded {
            sid: "s".into(),
            agent_kind: None,
            liveness_confidence: None,
            session_kind: None,
            attachable: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
            rbind_token: None,
            container: c,
            pid: None,
        })
        .unwrap()
    };
    assert_eq!(frame(None), "{\"kind\":\"session_added\",\"sid\":\"s\"}\n");
    assert_eq!(
        frame(Some(crate::stream::wire::SessionContainer::Tmux)),
        "{\"kind\":\"session_added\",\"sid\":\"s\",\"container\":\"tmux\"}\n"
    );
    assert_eq!(
        frame(Some(crate::stream::wire::SessionContainer::None)),
        "{\"kind\":\"session_added\",\"sid\":\"s\",\"container\":\"none\"}\n"
    );
}

/// 〔MIG-1 · `99 §2.1 ⑬`〕`session_state` 的**逐字节**金标准：两个取值、字段顺序 `sid` 在前。
/// monitor `ssh_source::parse_frame` 照这两个字面量认它。
#[test]
fn mig1_session_state_has_exactly_these_bytes() {
    use crate::stream::wire::SessionFate;
    for (state, word) in [
        (SessionFate::Reconnectable, "reconnectable"),
        (SessionFate::Ended, "ended"),
    ] {
        assert_eq!(
            to_line(&Frame::SessionState {
                sid: "abc".into(),
                state
            })
            .unwrap(),
            format!("{{\"kind\":\"session_state\",\"sid\":\"abc\",\"state\":\"{word}\"}}\n")
        );
    }
}

/// 〔U4b · 第四波〕`sessions_replayed` 的**逐字节**金标准：无载荷，只有 kind。
/// monitor `ssh_source::parse_frame` 照这个字面量认它。
#[test]
fn sessions_replayed_has_exactly_these_bytes() {
    assert_eq!(
        to_line(&Frame::SessionsReplayed).unwrap(),
        "{\"kind\":\"sessions_replayed\"}\n"
    );
}

/// 〔LOC1b · 第四波 4D〕`session_added.pid` 的线上形：缺席 ⇒ 与本字段加进来之前逐字节相同；带上 ⇒ 排在最后、是个整数。
#[test]
fn loc1b_session_added_pid_is_additive() {
    let frame = |pid: Option<u32>| {
        to_line(&Frame::SessionAdded {
            sid: "s".into(),
            agent_kind: None,
            liveness_confidence: None,
            session_kind: None,
            attachable: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
            rbind_token: None,
            container: None,
            pid,
        })
        .unwrap()
    };
    assert_eq!(frame(None), "{\"kind\":\"session_added\",\"sid\":\"s\"}\n");
    assert_eq!(
        frame(Some(4242)),
        "{\"kind\":\"session_added\",\"sid\":\"s\",\"pid\":4242}\n"
    );
}

// ═══ 〔HX2 · 第四波 4D〕hello 回显宿主交来的那几格（`host_env`）═══════════════════════════════
//
// 要求住址：题面 HX2 逐字「常驻后端身份带数据目录（接错了拒并出声）」；`INVARIANTS §42` → `IPC-PROTOCOL.md §10` hello 那一行
// （additive：空表省略、线上字节不变）。审计 `E-compat.md` §E10 · `GP1.md §7.6` 第 4 条。

/// 🔴 I1a：回显恰是名单内被交了的那几格、原样（手写期望）；没交 / 空串的那一格不回显；token 与无关变量即使在环境里也不回显。
#[test]
fn hx2_host_env_echoes_exactly_the_handed_names_and_never_the_token() {
    let env: std::collections::HashMap<&str, &str> = [
        ("CCM_RELAY_PORT", "8788"),
        ("CCM_APIKEY_CREDENTIALS", "/d/apikey-credentials.json"),
        ("CCM_HISTORY_METADATA", ""),
        (crate::stream::listen::ENV_TOKEN, "s3cret-token"),
        ("HOME", "/home/u"),
    ]
    .into_iter()
    .collect();
    let got = crate::stream::wire::host_env_from(|n| env.get(n).map(|v| v.to_string()));
    let want: std::collections::BTreeMap<String, String> = [
        ("CCM_RELAY_PORT".to_string(), "8788".to_string()),
        (
            "CCM_APIKEY_CREDENTIALS".to_string(),
            "/d/apikey-credentials.json".to_string(),
        ),
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want);
    assert!(crate::stream::wire::host_env_from(|_| None).is_empty());
}

/// 🔴 I1b：token 那个变量名不在回显名单里（名单三格 == 手写；token 名不在其中）—— hello 谁都读得到。
#[test]
fn hx2_the_listen_token_is_never_echoed() {
    let names: std::collections::BTreeSet<&str> =
        crate::stream::wire::HOST_ECHO_ENVS.into_iter().collect();
    let want: std::collections::BTreeSet<&str> = [
        "CCM_RELAY_PORT",
        "CCM_APIKEY_CREDENTIALS",
        "CCM_HISTORY_METADATA",
    ]
    .into_iter()
    .collect();
    assert_eq!(names, want);
    assert!(
        !names.contains(crate::stream::listen::ENV_TOKEN)
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
            ("CCM_APIKEY_CREDENTIALS".to_string(), "/d/k".to_string()),
        ]
        .into_iter()
        .collect(),
    );
    assert_eq!(
        present,
        "{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b\",\"host_arch\":\"x86_64\",\"claude_dir\":\"/c\",\
         \"host_env\":{\"CCM_APIKEY_CREDENTIALS\":\"/d/k\",\"CCM_RELAY_PORT\":\"8788\"}}\n"
    );
    let main = guard_core::production_code(include_str!("../../../src/backend/main.rs"));
    assert_eq!(
        main.matches("host_env: wire::host_env_from(|name| std::env::var(name).ok()),")
            .count(),
        1,
        "生产 hello 那一格不是「从真环境按名单回显」那一行"
    );
}

/// 〔NET2 · additive〕`uncancellable`：空表省略（字节与既有冻结串相同）；有值落在最末（`host_env` 之后）。
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
