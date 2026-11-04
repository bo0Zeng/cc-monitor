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
/// 人群 = **流式那条路**（`stream/entry.rs` / `stream/writer.rs` / `stream/inbound/` 整个目录 / `wire/mod.rs`（帧）/ `wire/listen.rs`）生产段里所有
/// `write_all(`。今天恰好三处，各自登记：
/// · `wire::write_and_flush_hello`（`wire/mod.rs`）—— 握手前那一帧，写完才产出 `HelloFlushed`；
///   它按定义发生在 writer_task 起来**之前**，不存在并发。
/// · `stream/writer.rs::write_frame` —— writer_task 的出口本体。
/// · `listen.rs::write_line` —— attach 握手的应答行（常驻那条路的那一员，见下）。
///
/// ⚠ `observe/*_query.rs` 那些 `stdout()` **不在人群里**：它们是一次性 CLI 子命令
/// （打完就退，没有 writer_task），与流式路不共享那个 stdout 的生命周期。
///
/// # `listen.rs` 在人群里：常驻让同一个进程既听口又跑流
///
/// 监听口那份文件是**流式路自己的** ⇒ 它在人群里：`listen.rs` 一旦多长出第二处 `write_all(`，本条当场红。
/// ⚠ `relay/` 不在人群里：同一个进程既跑流式又跑中转，本条不管（那是另立的一件）。
#[test]
fn the_outbound_stream_has_exactly_one_writer() {
    const STREAMING: &[(&str, &str)] = &[
        (
            "wire.rs",
            "write_and_flush_hello：握手帧，发生在 writer_task 起来之前",
        ),
        ("stream/writer.rs", "write_frame：writer_task 的出口本体"),
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
        let by_rel = rel.starts_with("stream/inbound/")
            || rel == "stream/writer.rs"
            || rel == "stream/entry.rs";
        // 只看流式那条路（`wire.rs` · `listen.rs` ＋ 进程入口 ＋ 写者 ＋ 入方向那个目录）；`observe/*_query.rs` 是一次性子命令，理由见头注。
        if !by_rel && !matches!(name.as_str(), "wire.rs" | "listen.rs") {
            continue;
        }
        let name = if by_rel { rel } else { name };
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

/// ★★ **`HelloFlushed` 的构造面必须只有一个出口**。
///
/// `wire/mod.rs` 那份类型的文档写着：见证「**只能由真的写完并 flush 了一帧才能产出**
/// （构造函数私有，唯一出口是 `write_and_flush_hello`）」；按字节位置判 reader 先后的机检对函数边界是瞎的，所以不用它。
///
/// ⇒ 于是整条「Hello 之前不许起 inbound」的保证，**全压在这个类型的构造面上**：
/// 给它加一个 `pub fn assume() -> Self`，别的判据一条都不会红。
///
/// 钉三件（第三件是编译器挡不住、要写下来的那一格）：
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
    //   要数的是「构造表达式」，不是「这串字符」——匹配单位比事实大。
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

    // ② `impl HelloFlushed` 里的关联函数：只许 `for_tests`，且必须只在测试档与 `test-support`（只由上层的 dev 依赖开）。
    let at = src
        .find("impl HelloFlushed {")
        .expect("找不到 `impl HelloFlushed` —— 抽取器坏了");
    let mut fns = Vec::new();
    let mut cfg_test_seen = false;
    for (i, line) in src[at..].lines().enumerate() {
        if i > 0 && !line.is_empty() && !line.starts_with(char::is_whitespace) {
            break;
        }
        if line.trim() == concat!("#[cfg(any(te", "st, feature = \"test-support\"))]") {
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
        "`for_tests` 上没有「只在测试档与 `test-support`」那一格 cfg —— 那它在 release 里也存在，\
             「生产路径拿不到见证」这句话当场不成立"
    );

    // ③ 字段必须私有（公开的元组字段让外面能直接构造见证）。
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
use super::test_support::{said_record, ALL, SECOND_FRONTEND_READS};
use super::*;
use serde_json::Value;

// 会话流跨语言金样那几条（夹具经历史 · 识别两层造）挂主 crate：`on_upper_tests.rs`。

/// ★ W1：`changed` 的**逐字节**金标准。
#[test]
fn changed_frames_have_exactly_these_bytes() {
    let cases = [
        (
            Frame::changed(crate::wire::Changed::Accounts {}),
            "{\"kind\":\"changed\",\"topic\":\"accounts\"}\n",
        ),
        (
            Frame::changed(crate::wire::Changed::RotationStrategies { body: None }),
            "{\"kind\":\"changed\",\"topic\":\"rotation_strategies\"}\n",
        ),
        (
            Frame::changed(crate::wire::Changed::Tasks {
                key: "s1".into(),
                body: None,
            }),
            "{\"kind\":\"changed\",\"topic\":\"tasks\",\"key\":\"s1\"}\n",
        ),
        (
            Frame::changed(crate::wire::Changed::Plan {
                key: "/w".into(),
                rev: Some("0123abcd".into()),
                body: Some(cmd_wire::frame::PlanNeeds { needs: 2 }),
            }),
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
            f(Some(crate::wire::TransferEnd::Failed(crate::wire::detail::Failed::local("写暂存件失败".into())))),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"failed\",\"code\":\"failed\",\"said\":\"写暂存件失败\",\"detail\":\"\"}}\n",
        ),
        // 带码 · 复制详情的那一形（今天按码换路的只有 SFTP 起始目录不是后端 home 那一码；句子只带原因词，原话在 `detail` 里）。
        (
            f(Some(crate::wire::TransferEnd::Failed(crate::wire::detail::Failed {
                code: "sftp_home_mismatch".into(),
                said: "w".into(),
                detail: "d".into(),
                data: None,
            }))),
            "{\"kind\":\"transfer\",\"id\":\"xfer-7\",\"got\":262144,\"total\":1000000,\"end\":{\"state\":\"failed\",\"code\":\"sftp_home_mismatch\",\"said\":\"w\",\"detail\":\"d\"}}\n",
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
                        LiveBlock { kind: BlockKind::Thinking, md: None, tool: None },
                        LiveBlock { kind: BlockKind::Text, md: Some(vec![cmd_wire::md::MdBlock::P { k: vec![cmd_wire::md::Inl::S("你好".into())] }]), tool: None },
                    ],
                },
            ),
            "{\"kind\":\"live_card\",\"sid\":\"0b6c1f7e-sid\",\"card\":12,\"n\":0,\"op\":\"open\",\"rid\":\"r-1\",\"phase\":\"streaming\",\"clipped\":false,\"blocks\":[{\"kind\":\"thinking\"},{\"kind\":\"text\",\"md\":[{\"t\":\"p\",\"k\":[\"你好\"]}]}]}\n",
        ),
        (
            live(Some("a1"), 1, LiveOp::Block { i: 2, block: BlockKind::Tool, tool: Some("Bash".into()) }),
            "{\"kind\":\"live_card\",\"sid\":\"0b6c1f7e-sid\",\"run\":\"a1\",\"card\":12,\"n\":1,\"op\":\"block\",\"i\":2,\"block\":\"tool\",\"tool\":\"Bash\"}\n",
        ),
        (
            live(None, 2, LiveOp::Text { i: 1, keep: 3, tail: vec![cmd_wire::md::MdBlock::Rule] }),
            "{\"kind\":\"live_card\",\"sid\":\"0b6c1f7e-sid\",\"card\":12,\"n\":2,\"op\":\"text\",\"i\":1,\"keep\":3,\"tail\":[{\"t\":\"rule\"}]}\n",
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
                status_text: crate::common::cells::Words("核心·在跑".into()),
                label_text: crate::common::cells::Words("scan".into()),
                last: Some(RunDid::Tool {
                    name: "Bash".into(),
                }),
                last_text: Some(crate::common::cells::Words("调用 Bash".into())),
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
            status_text: crate::common::cells::Words("核心·失败".into()),
        }],
        groups: cmd_wire::frame::RunGroups {
            running: vec!["a1".into()],
            recent: vec!["a2".into()],
            older: vec!["a3".into()],
            older_text: None,
            ..Default::default()
        },
        summary: Some(crate::common::cells::Words("核心·agent 3".into())),
    });
    assert_eq!(
        to_line(&f).unwrap(),
        "{\"kind\":\"session_runs\",\"sid\":\"s1\",\"runs\":[{\"run\":\"a1\",\"label\":\"scan\",\"kind\":\"Explore\",\"tool\":\"t1\",\"parent\":\"a0\",\"state\":\"running\",\"status_text\":\"核心·在跑\",\"label_text\":\"scan\",\"last\":{\"t\":\"tool\",\"name\":\"Bash\"},\"last_text\":\"调用 Bash\",\"waiting\":\"Bash\",\"started_ms\":1000,\"active_ms\":2000,\"calls\":3,\"background\":true},{\"run\":\"a2\",\"state\":\"failed\",\"status_text\":\"\",\"label_text\":\"\",\"last\":{\"t\":\"say\"},\"started_ms\":1000,\"active_ms\":2000,\"ended_ms\":3000,\"why\":\"reported\",\"error\":\"boom\"},{\"run\":\"a3\",\"state\":\"unknown\",\"status_text\":\"\",\"label_text\":\"\",\"why\":\"quiet\"}],\"ended\":[{\"run\":\"a0\",\"tool\":\"t0\",\"state\":\"failed\",\"status_text\":\"核心·失败\"}],\"groups\":{\"running\":[\"a1\"],\"recent\":[\"a2\"],\"older\":[\"a3\"]},\"summary\":\"核心·agent 3\"}\n"
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
    // 把字母表尾巴换成 URL 安全版（`-_`），七条照样绿 ⇒ 另钉这一条：0xfb 0xff ⇒ 62 · 63 · 60。
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

/// 离开「活」那一帧的**逐字节**金标准：`sid` 在前、整份脸在后；去向只在 `face.life`（`is` 那几个字面量客户端按 `cmd-wire` 认）。
/// 被顶替才带 `by`，别的不出这一格。
#[test]
fn the_session_left_frame_has_exactly_these_bytes() {
    for (life, by, word) in [
        (
            crate::wire::Life::Reconnectable,
            None,
            r#"{"is":"reconnectable"}"#,
        ),
        (crate::wire::Life::Ended, None, r#"{"is":"ended"}"#),
        (
            crate::wire::Life::Ended,
            Some("s2"),
            r#"{"is":"ended","by":"s2"}"#,
        ),
        (crate::wire::Life::Gone, None, r#"{"is":"gone"}"#),
    ] {
        let line = to_line(&Frame::session_left(
            "abc".into(),
            life,
            by.map(str::to_string),
            None,
        ))
        .unwrap();
        assert!(
            line.starts_with(r#"{"kind":"session_left","sid":"abc","face":{"dot":"#),
            "{line}"
        );
        assert!(line.contains(&format!(r#""life":{word},"#)), "{line}");
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
            version: None,
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
    let main = guard_core::production_code(include_str!("../../../src/backend/stream/entry.rs"));
    assert_eq!(
        main.matches("host_env: wire::host_env_from(|name| std::env::var(name).ok()),")
            .count(),
        1,
        "生产 hello 那一格不是「从真环境按名单回显」那一行"
    );
}

// ═══ 第二个前端在读的形状：冻结（只许加字段）═══════════════════════════════
// 那张表住 `test_support::SECOND_FRONTEND_READS`（`build_id_guard` 也读它）。

/// 每一种帧都填满（可选格全给值），这样「这一格还在不在、是什么类型」才看得见。
fn every_frame_the_second_frontend_reads() -> Vec<Value> {
    let s = Some("x".to_string());
    let frames = [
        Frame::Hello(crate::wire::Hello {
            build_id: "b".into(),
            host_arch: "x86_64".into(),
            version: None,
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
                    md: Some(vec![cmd_wire::md::MdBlock::P {
                        k: vec![cmd_wire::md::Inl::S("hi".into())],
                    }]),
                    tool: None,
                }],
            },
        }),
        Frame::Reply(crate::wire::Reply {
            id: "1".into(),
            ok: false,
            code: s.clone(),
            said: s.clone(),
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
    let sh = crate::common::time::Tz::named("Asia/Shanghai").unwrap();
    let tz_of = |line: &str| serde_json::from_str::<Request>(line).expect("信封认得").tz;
    assert_eq!(tz_of(r#"{"id":"1","cmd":"ping","tz":"Asia/Shanghai"}"#), sh);
    for line in [
        r#"{"id":"1","cmd":"ping"}"#,
        r#"{"id":"1","cmd":"ping","tz":"Nowhere/Atlantis"}"#,
        r#"{"id":"1","cmd":"ping","tz":480}"#,
        r#"{"id":"1","cmd":"ping","tz":null}"#,
    ] {
        assert_eq!(tz_of(line), crate::common::time::Tz::default(), "{line}");
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

/// `changed` 的小成品超了主题表那一格的上限 ⇒ 不带（客户端重问）；没超 ⇒ 照带。
#[test]
fn a_changed_body_over_its_cap_is_left_off() {
    let cap = Topic::SessionHidden.spec().body_cap;
    let hidden = |said: String| crate::wire::Changed::SessionHidden {
        key: "s1".into(),
        body: Some(cmd_wire::frame::HiddenNow {
            hidden: Some(cmd_wire::session::Hidden {
                by: cmd_wire::session::HiddenBy::Own,
                said,
            }),
        }),
    };
    match Frame::changed(hidden("x".repeat(cap))) {
        Frame::Changed(crate::wire::Changed::SessionHidden { body, .. }) => {
            assert_eq!(body, None, "超了上限还带着")
        }
        other => panic!("{other:?}"),
    }
    match Frame::changed(hidden("x".into())) {
        Frame::Changed(c) => assert_eq!(c, hidden("x".into()), "没超上限却去掉了"),
        other => panic!("{other:?}"),
    }
}

/// `changed` 丢了能不能恢复照主题表答。
#[test]
fn changed_loss_follows_the_topic_table() {
    for t in ALL {
        let f = Frame::changed(
            crate::wire::Changed::bare(t, Some("k".into())).expect("每个主题都有那一臂"),
        );
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
    let runs: Vec<std::ops::Range<usize>> = (0..off.len())
        .step_by(7)
        .map(|s| s..(s + 7).min(off.len()))
        .collect();
    let frames = Frame::branch_pages("s", "/p/s.jsonl", &off, &runs, 0);
    assert!(frames.len() > 1, "要分好几帧：{}", frames.len());
    let mut all = Vec::new();
    let mut got_runs = Vec::new();
    for f in &frames {
        let line = to_line(f).unwrap();
        assert!(
            line.len() <= REPLY_PAGE_BYTES.as_len(),
            "一帧 {} 字节",
            line.len()
        );
        let Frame::SessionBranch(crate::wire::SessionBranch {
            at,
            off,
            runs: rs,
            sum,
            ..
        }) = f
        else {
            panic!()
        };
        assert_eq!(*at as usize, all.len());
        // 这一帧的段：段首都落在这一帧那几条里。
        for r in rs {
            assert!((*at..*at + off.len() as u64).contains(&r.at), "段首 {} 不在这一帧里", r.at);
            got_runs.push(r.at as usize..(r.at + r.n) as usize);
        }
        all.extend(off.iter().cloned());
        // read8c：每帧的摘要 ＝ 到它末尾为止那一截的摘要（客户端交回的就是它）。
        assert_eq!(
            *sum,
            crate::wire::branch_sum(&all),
            "第 {at} 条起那一帧的摘要不是到它末尾的那一截"
        );
    }
    assert_eq!(all, off);
    assert_eq!(got_runs, runs, "各帧的段拼起来不是整份的段");
    let tail = Frame::branch_pages("s", "/p/s.jsonl", &off[..10], &[], 8);
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
    let none = Frame::branch_pages("s", "/p/s.jsonl", &off[..3], &[], 3);
    assert!(
        matches!(&none[..], [Frame::SessionBranch(crate::wire::SessionBranch { at: 3, off, .. })] if off.is_empty())
    );
}

/// 发变了的那一截时 `at` 挪到段首：第 `at - 1` 条所在那一段可能接上了新来的（条数变了）⇒ 整段重发，起在它之前的段不动。
/// 每段带写好的框头那一句（`×n`）。
#[test]
fn a_branch_delta_starts_at_the_head_of_the_run_it_cuts_into() {
    let off: Vec<String> = (0..10).map(|i| format!("id{i}")).collect();
    let runs = [0..4, 4..10];
    let delta = Frame::branch_pages("s", "/p/s.jsonl", &off, &runs, crate::wire::run_head(&runs, 7));
    let [Frame::SessionBranch(b)] = &delta[..] else {
        panic!("一帧：{delta:?}")
    };
    assert_eq!(b.at, 4, "没挪到段首");
    assert_eq!(b.off, off[4..]);
    assert_eq!(
        b.runs,
        [crate::wire::BranchRun {
            at: 4,
            n: 6,
            said: crate::common::cells::Words(copy_core::copy_text(
                "branchFold.wrapRun.summary",
                &[("n", "6")]
            )),
        }]
    );
    assert_eq!(b.sum, crate::wire::branch_sum(&off));
    // 「前 4 条照旧」：第 3 条那一段可能接上了第 4 条起新来的（两段之间的边界在不在前缀里说不准）⇒ 也挪回它的段首。
    assert_eq!(crate::wire::run_head(&runs, 4), 0);
    assert_eq!(crate::wire::run_head(&runs, 0), 0);
    // 清单缩成前缀（尾上那几条回到主线）⇒ 最后那一段条数变了，也挪到它的段首整段重发。
    let short = [0..4, 4..6];
    let shrunk = Frame::branch_pages("s", "/p/s.jsonl", &off[..6], &short, crate::wire::run_head(&short, 6));
    assert!(
        matches!(&shrunk[..], [Frame::SessionBranch(b)] if b.at == 4 && b.off.len() == 2 && b.runs.len() == 1 && b.runs[0].n == 2)
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
        host: None,
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
        Life::Reading { host: None },
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

/// 整份脸上能不能结束（`can.end`，右键 · 多选 · 终端名单 · 占位都只读它）由核心判：
/// 活着且住在终端宿主里（或住哪判不了，交了由那台认）· 程序退了 tmux 会话还在 ⇒ 能；
/// 不在任何宿主里 ⇒ 不能、写非 cc-monitor 起的那一句；已结束 · 记录已不在 ⇒ 不能、写已结束那一句；说不清 · 看不见 ⇒ 不能、写那一态那一句。
#[test]
fn session_frames_say_whether_the_session_can_be_ended() {
    use crate::wire::{Housed, Life, SessionFace, TerminalHost};
    let end = |f: SessionFace| serde_json::to_value(f).unwrap()["can"]["end"].clone();
    let yes = serde_json::json!({"can": "yes"});
    let no = |why: String| serde_json::json!({"can": "no", "why": why});
    for host in [
        Some(Housed::In(TerminalHost::Tmux)),
        Some(Housed::In(TerminalHost::Managed)),
        None,
    ] {
        assert_eq!(
            end(SessionFace::of(Life::Live {
                activity: None,
                host
            })),
            yes,
            "{host:?}"
        );
        assert_eq!(
            end(SessionFace::of(Life::Reading { host })),
            yes,
            "{host:?}"
        );
    }
    assert_eq!(end(SessionFace::of(Life::Reconnectable)), yes);
    assert_eq!(
        end(SessionFace::of(Life::Live {
            activity: None,
            host: Some(Housed::Bare)
        })),
        no(copy_core::copy_text("terminal.empty.notOurs", &[]))
    );
    let ended = copy_core::copy_text("sessionState.ended.name", &[]);
    assert_eq!(
        end(SessionFace::of(Life::Ended)),
        no(copy_core::copy_text(
            "tabBatch.why.ended",
            &[("ended", &ended)]
        ))
    );
    let gone = copy_core::copy_text("sessionState.gone.name", &[]);
    assert_eq!(
        end(SessionFace::of(Life::Gone)),
        no(copy_core::copy_text(
            "tabBatch.why.ended",
            &[("ended", &gone)]
        ))
    );
    assert_eq!(
        end(SessionFace::of(Life::Unclear)),
        no(copy_core::copy_text("runs.state.unknown", &[]))
    );
    assert_eq!(
        end(face_core::book::unseen(crate::common::cells::Words(
            "gpu-01 离线".into()
        ))),
        no("gpu-01 离线".into())
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
                host: Some(crate::wire::Housed::In(crate::wire::TerminalHost::Tmux)),
            },
        ),
        (
            "live-working-plain",
            Life::Live {
                activity: Some(A::Working),
                host: Some(crate::wire::Housed::Bare),
            },
        ),
        (
            "live-background",
            Life::Live {
                activity: Some(A::BackgroundWork),
                host: None,
            },
        ),
        (
            "live-idle",
            Life::Live {
                activity: Some(A::Idle),
                host: None,
            },
        ),
        (
            "live-unclear",
            Life::Live {
                activity: None,
                host: None,
            },
        ),
        (
            "reading",
            Life::Reading {
                host: Some(crate::wire::Housed::In(crate::wire::TerminalHost::Tmux)),
            },
        ),
        ("reconnectable", Life::Reconnectable),
        ("ended", Life::Ended),
        ("gone", Life::Gone),
        ("unclear", Life::Unclear),
    ];
    let mut produced: serde_json::Map<String, Value> = lives
        .into_iter()
        .map(|(k, l)| {
            (
                k.to_string(),
                serde_json::to_value(SessionFace::of(l)).unwrap(),
            )
        })
        .collect();
    produced.insert(
        "unseen".into(),
        serde_json::to_value(face_core::book::unseen(crate::common::cells::Words(
            comms_inward::chan::said::offline("gpu-01"),
        )))
        .unwrap(),
    );
    let produced = serde_json::to_string_pretty(&produced).unwrap() + "\n";
    let path = guard_core::addr::repo_root().join(SESSION_FACE_GOLDEN);
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
        "src/backend/wire/mod.rs · of · SessionFace",
        "src/backend/wire/mod.rs · live_face · SessionFace",
        "src/backend/wire/mod.rs · of_parts · NameFace",
        // 那台看不见时那一张住会话簿那一层（桌面后端与手机核心同一处）。
        "src/common/face-core/src/book.rs · unseen · SessionFace",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(
        seen, want,
        "会话那几格写好的字只许经构造口造（`SessionFace::of` · `NameFace::of_parts` · `face_core::book::unseen`）"
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
        if path.components().any(|c| c.as_os_str() == "node_modules") {
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

/// 监控板组内一把尺：可重连 · 已结束的 `session_left` 那份脸的序，都排在任何活会话的序之后；
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
                host: None,
            })
            .order,
        )
    })
    .collect();
    let fate = |f| {
        serde_json::to_value(Frame::session_left("s".into(), f, None, None)).unwrap()["face"]
            ["order"]
            .as_u64()
    };
    assert_eq!(live, [0, 1, 2, 3].into());
    assert_eq!(
        (
            fate(crate::wire::Life::Reconnectable),
            fate(crate::wire::Life::Ended)
        ),
        (Some(4), Some(5))
    );
}

/// 那台看不见时画的那一张（断线那一刻本机后端给那台每条活的 · 可重连的发它）：点「说不清」、不说在跑、不是定局；
/// 只在右键菜单里关，不给恢复（也许还在跑，再起一份就是同一会话两个进程），别的都不给；
/// 悬停那一句 · 状态一句就是那台的离线那一句（与机器成品的 `offline_said` 同一处取字），短名照旧「状态不明」、不说已结束。
#[test]
fn the_unseen_face_is_unknown_closes_from_the_menu_and_says_that_machines_offline_line() {
    use cmd_wire::frame::{Allow, CanClose, CanTerm, SessionDot};
    let said = comms_inward::chan::said::offline("gpu-01");
    let f = face_core::book::unseen(crate::common::cells::Words(said.clone()));
    assert_eq!(f.dot, SessionDot::Unknown);
    assert_eq!(f.life, crate::wire::WireLife::Unclear);
    assert!(!f.settled, "看不见不是定局：重连后那台的帧把脸带回来");
    assert_eq!(f.can.close, CanClose::Menu);
    assert!(matches!(f.can.resume, Allow::No { .. }));
    assert!(matches!(f.can.term, CanTerm::No { .. }));
    for a in [
        &f.can.reattach,
        &f.can.model,
        &f.can.permission,
        &f.can.stop,
    ] {
        assert!(matches!(a, Allow::No { .. }), "{a:?}");
    }
    assert_eq!(f.tip.0, said, "悬停是那台的离线那一句");
    assert_eq!(f.line.0, said);
    assert_eq!(f.name.0, copy_core::copy_text("runs.state.unknown", &[]));
}

/// 界面那一侧不造脸：`src/frontend` 的 ts 生产源里没有一个函数交出一张 `SessionFace`（`…): SessionFace {` / `=>`），
/// 也没有一份写着 `settled:` 的字面量（整份脸才有那一格）。那台看不见时那一张也由后端造（`SessionFace::of(Life::Unseen)`，断线那一刻本机后端发）。
/// 正控：同一把尺量得到一段造脸的写法（原先界面自己拼的那一张就是这个形）。
#[test]
fn the_interface_never_builds_a_face_of_its_own() {
    // 运行时拼，免得命中本条自己（同上一条）。
    let face = ["Session", "Face"].concat();
    let ret = format!("): {face}");
    let settled = ["sett", "led:"].concat();
    let built = |code: &str| -> Vec<String> {
        guard_core::strip_comment_lines(code)
            .lines()
            .filter(|l| {
                l.find(&ret).is_some_and(|i| {
                    let rest = l[i + ret.len()..].trim_start();
                    rest.starts_with('{') || rest.starts_with("=>")
                }) || guard_core::contains_word(l, &settled)
            })
            .map(|l| l.trim().to_string())
            .collect()
    };
    let specimen = format!(
        "// {settled} 注释里不算\nexport function madeHere(machine: string): {face} {{\n  return {{ {settled} false }};\n}}"
    );
    assert_eq!(
        built(&specimen),
        vec![
            format!("export function madeHere(machine: string): {face} {{"),
            format!("return {{ {settled} false }};")
        ],
        "正控：这把尺量得到界面自己造脸的那一形（注释里的不算）"
    );
    let repo = guard_core::addr::repo_root();
    let mut found = Vec::new();
    for (path, raw) in guard_core::scan_tree!(&repo.join("src/frontend"), &["ts"]) {
        if path.components().any(|c| c.as_os_str() == "generated") {
            continue;
        }
        let rel = path
            .strip_prefix(&repo)
            .unwrap_or(&path)
            .display()
            .to_string();
        for l in built(&raw) {
            found.push(format!("{rel}: {l}"));
        }
    }
    assert!(
        found.is_empty(),
        "界面自己造了脸（脸只由后端造，看不见那一张也是）：\n  {}",
        found.join("\n  ")
    );
}
