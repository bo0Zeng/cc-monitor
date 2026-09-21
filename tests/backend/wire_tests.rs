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
        include_str!("../../src/backend/wire.rs").to_string(),
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
    let src = include_str!("../../src/backend/wire.rs");
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
            },
            "hello",
        ),
        (
            Frame::Line {
                session_id: "s".into(),
                path: "/p".into(),
                seq: 0,
                raw: "{}".into(),
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
        (
            Frame::TmuxSessions {
                raw: "s1\t/p\tclaude\t1\t2\tsid-a".into(),
                observation: None,
            },
            "tmux_sessions",
        ),
        // 〔audit-0805 08-06〕补上此前**测试段零构造**的三个变体。
        // `Reply` 的上线形另有 `inbound.rs` 钉着；`TmuxSessionClosed` / `Cancelled`
        // 此前**只有 monitor 侧「解析成 None」的负向断言** —— 那是消费方的行为，
        // 不是后端序列化形态：改掉 kind 标签或字段名，两边都不会红。
        (
            Frame::TmuxSessionClosed {
                name: "cc-1".into(),
            },
            "tmux_session_closed",
        ),
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
    ];

    // ★ 人群自检：**样本必须覆盖 `Frame` 的每一个变体**〔audit-0805 08-06〕。
    //
    // 原来这里是一张**手写清单**：11 个变体只列了 8 个，而加第 12 个变体时
    // **没有任何东西会红** —— 一条上线契约帧就那样进了协议。
    // ⇒ 把人群换成**枚举本身**：从源码数 `pub enum Frame` 的变体数，
    // 与样本产出的**去重 kind 数**对拍。加变体不补样本 ⇒ 当场红。
    let src = include_str!("../../src/backend/wire.rs");
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

/// B2：TmuxSessions 帧带 tmux ls 原文——含**真 TAB**（列分隔）+ **换行**（多会话）→ 必须是**单行**
/// wire（TAB/换行经 serde 转义、无裸换行），roundtrip 字节还原（monitor `parse_tmux_ls` 靠真 TAB 分列）。
#[test]
fn tmux_sessions_frame_ships_raw_as_one_line() {
    let raw = "s1\t/p\tclaude\t1\t2\tsid-a\ns2\t/q\tnode\t0\t1\t";
    let line = to_line(&Frame::TmuxSessions {
        raw: raw.into(),
        observation: None,
    })
    .expect("serialize");
    assert!(line.ends_with('\n'));
    let body = line.strip_suffix('\n').unwrap();
    assert!(!body.contains('\n'), "内嵌换行须被转义、无裸换行: {body:?}");
    let v: Value = serde_json::from_str(body).expect("json");
    assert_eq!(v["kind"], "tmux_sessions");
    assert_eq!(v["raw"], raw); // TAB + 换行逐字还原
}

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
        raw: raw.to_string(),
        byte_offset: 99,
    };

    let line = to_line(&frame).expect("serialize");
    assert!(line.ends_with('\n'));

    // Minus the single trailing terminator, there must be NO bare newline:
    // the embedded newline in `raw` is escaped by serde_json as "\n".
    let body = &line[..line.len() - 1];
    assert!(
        !body.contains('\n'),
        "embedded newline must be escaped, got bare newline in: {body:?}"
    );

    // Parses back and the raw field is recovered byte-for-byte.
    let v: Value = serde_json::from_str(body).expect("parse");
    assert_eq!(v["kind"], "line");
    assert_eq!(v["raw"], raw);
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
    let prod = crate::guard_support::production_code(include_str!("../../src/backend/main.rs"));
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
        crate::agents::Adapter { kind: "synthetic", home: synth_present, account_env: None },
        crate::agents::Adapter { kind: "ghost",     home: synth_absent, account_env: None },
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
/// `"superseded"` 与 monitor `src/bridge/src/ssh_source.rs` 的解析处逐字一致。
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
