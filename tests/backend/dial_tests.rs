//! `D2` 判据的**乙半**（代理这一侧）＋ 它的反向自检。
//!
//! # 它断的是哪一个性质
//!
//! **「与远端跑 SSH 传输层握手」这件事，在本 crate 里只许发生在 `dial/` 底下。**
//!
//! 🔴 **它断的不是「`russh` 这个词还在不在」** —— `K-P6` 那一拍已经证过后者会量错集合
//! （**14 份在往外拨的文件里，11 份的 `russh` 代码态是 0**）。
//! 锚点取的是**真正把字节交给 SSH 状态机的那几个入口**，不是 crate 名、不是函数名。
//!
//! # 甲半在哪
//!
//! 甲半（界面这一侧：backend 那条长连接流不再自己拨号）住
//! `../../frontend/shell/src/stream_source/` 的测试模块 —— **两侧各扫各的 crate**，
//! 刻意不从这里 `include_str!` 伸到对面去（那会新增一条跨轨编译期边，
//! 而那张登记表不在本轮写区里）。

use super::*;

/// 从一行 JSON 文本读一份**组好的**请求（`DialRequest` 自己的反序列化 —— 生产上线交来的是一台机器的原样配置，
/// 经 `machine::resolve` 组成这一形之后走的也是这一份）。
fn parse_request(raw: &str) -> Result<DialRequest, serde_json::Error> {
    serde_json::from_str(raw.trim())
}

/// 真正把字节交给 SSH 状态机的入口。**这三条就是判据的锚点。**
///
/// 为什么是这三条：`connect` = 自己建 TCP 再跑传输层握手；`connect_stream` = 在别人给的
/// 流上跑传输层握手（跳板那一形）；`channel_open_direct_tcpip` = 把一条 TCP 隧道开到
/// 第三方去（隧道本身就是「往外拨」的另一种形态）。
///
/// ⚠ **诚实边界**：锚点按**源码文本**取 ⇒ 换个 `use` 别名、或把调用藏进宏，
/// 本判据看不见。它与 `readonly_guard` / `protocol_doc_guard` 头注里那条同族的
/// 诚实边界是一样的 —— **别读成证明**。
fn anchors() -> Vec<String> {
    // 运行时拼：直接写字面量的话，本模块自己就会被下面的扫描命中
    //（同类自指陷阱本仓在 P4 连踩过七次）。
    [
        ("client::conn", "ect("),
        ("client::conn", "ect_stream("),
        ("channel_open_direct", "_tcpip("),
    ]
    .iter()
    .map(|(a, b)| format!("{a}{b}"))
    .collect()
}

/// 允许出现拨号锚点的**文件维前缀**。闭集，只此一条。
const DIAL_HOME: &str = "dial/";

/// 语料地板：低于这个字节数就判「语料没喂进来」而不是「一处都没有」。
///
/// 🔴 这条就是 `P6bM4` 的被测对象 —— **喂空输入，它必须自己先红**。
/// 50_000 → 100_000：真语料长过了地板的 40 倍（约 2 MB），照下面那条判据的说法抬地板。
const CORPUS_FLOOR_BYTES: usize = 100_000;

/// 乙半判据本体。**纯函数**：语料由调用方给 ⇒ 阳性/阴性两个方向都切得动。
///
/// 返回 `Err(说法)` = 判据红。三种红各有各的话：
/// ① 语料太小（空转）· ② 一处锚点都没有（拨号根本没搬进来）· ③ 锚点长在 `dial/` 外面。
fn dial_locality(corpus: &[(String, String)]) -> Result<usize, String> {
    let bytes: usize = corpus.iter().map(|(_, c)| c.len()).sum();
    if bytes < CORPUS_FLOOR_BYTES {
        return Err(format!(
            "语料只有 {bytes} 字节（地板 {CORPUS_FLOOR_BYTES}）—— 本判据此刻在空转。\n\
                 「一处都没扫到」与「根本没扫」在终端上一模一样，这条地板就是把它们分开的那一刀。"
        ));
    }
    let pats = anchors();
    let mut total = 0usize;
    let mut strays: Vec<String> = Vec::new();
    for (name, code) in corpus {
        for p in &pats {
            let n = code.matches(p.as_str()).count();
            if n == 0 {
                continue;
            }
            total += n;
            if !name.starts_with(DIAL_HOME) {
                strays.push(format!("{name} 里有 {n} 处 `{p}`"));
            }
        }
    }
    if !strays.is_empty() {
        return Err(format!(
            "拨号锚点长到 `{DIAL_HOME}` 外面去了：{strays:?}\n\
                 本 crate 里「与远端跑 SSH 握手」只许住 `{DIAL_HOME}` —— \
                 别处要拨号，先回答「为什么这一处非得自己拨」。"
        ));
    }
    if total == 0 {
        return Err(format!(
            "本 crate 生产段里一处拨号锚点都没有（锚点：{pats:?}）——\n\
                 那意味着代理这一侧根本没在拨号，`K-P6b` 的乙半是空的。"
        ));
    }
    Ok(total)
}

/// 本 crate `src/` 递归全部 `.rs` 的**生产段**，相对 `src/` 的路径 + 正文。
///
/// 🔴 **必须走 `guard_core::scan_tree!`**（`scanning_guard_registry` 那条判据钉着）：
/// 裸 `read_dir` 的扫描型判据会**在自己的登记表 / 注释 / 常量里找到自己** ⇒ 恒绿，
/// `audit-0805` 实测过五次，五次都不是被判据自己逮到的。
///
/// ⚠⚠ **那个「代价」今天不存在，而这一段先前把它当现状写着**。
/// 先前逐字：「`scan_tree!` 按构造摘除调用者自己那一份 —— 也就是 `dial/mod.rs`
/// 不在它返回的人群里」。两处都假：
/// ① 自摘那一刀**在这一处不生效**（判据由 `#[path]` 挂载 ⇒ `file!()` 是带 `..`
///    的折返路径 ⇒ 后缀比不命中，
///    `the_scan_tree_macro_no_longer_excludes_its_caller_after_the_split` 守着这件事）；
/// ② 今天的调用者是 `tests/backend/dial_tests.rs`，**根本不是** `dial/mod.rs` ——
///    后者走普通遍历本来就在人群里。
/// ⇒ 下面那句 `include_str!` 的补回**今天是冗余的**（同一份被数两遍，靠尾部那句
/// `dedup_by` 收掉），而**刻意不删**：它把「被测那一份一定在人群里」钉成一件
/// **不依赖扫描面**的事 —— 扫描面哪天被改窄（只扫某个子目录），少扫不会红，而它还在。
fn crate_sources() -> Vec<(String, String)> {
    let root = crate::guard_support::src_root();
    let mut out: Vec<(String, String)> = Vec::new();
    for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        out.push((rel, crate::guard_support::production_code(&src)));
    }
    // 被测那一份。它走普通遍历**本来就在**人群里（自摘不生效，理由见本函数头注）；
    // 这一份是**冗余但刻意保留**的第二个来源，尾部 `dedup_by` 收掉重复。
    out.push((
        "dial/mod.rs".to_string(),
        crate::guard_support::production_code(include_str!("../../src/backend/dial/mod.rs")),
    ));
    out.sort();
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// ★ 乙半：**拨号只许住 `dial/`，而且必须真的有一处。**
#[test]
fn the_dial_only_happens_under_dial_home() {
    let corpus = crate_sources();
    assert!(
        corpus.len() >= 37,
        "只扫到 {} 份源文件 —— 抽取坏了（08-06 实测 37 份）",
        corpus.len()
    );
    match dial_locality(&corpus) {
        Ok(n) => assert!(
            n >= 1,
            "锚点数 {n} —— 不该走到这里，`dial_locality` 自己会拦"
        ),
        Err(e) => panic!("{e}"),
    }
}

/// ★ **拨号真的接得到 —— 不只是「代码住在这儿」**（`7u` 探针逮出来的那一格，换了入口）。
///
/// C2 那一版的入口是 `main.rs` 里那条 `--dial` 分派臂（实打：摘掉那条臂，`dial/` 一个字不动，
/// backend 侧判据一条都不红 —— 「住在哪」断不了「接没接上」）。SR1a 之后入口换成**流上的链路四条**：
/// `inbound::dispatch` 里四条硬臂各调 `dial::link::Table` 的一个方法。本条钉两件：
/// ① 四条硬臂各恰好调一次它那个方法（摘掉任一条 ⇒ 那条命令落进 `lookup` 的 `Run::Builtin` 兜底臂、回 `unknown_command`）；
/// ② `--dial` 那条老入口**零处**（它删了；留着一半 = `is_query_mode` 认它却没有臂接，v3.4.0 那一形）。
#[test]
fn the_link_arms_are_actually_wired_into_the_dispatch() {
    let inbound_prod = crate::guard_support::production_code(include_str!(
        "../../src/backend/stream/inbound/mod.rs"
    ));
    assert!(
        inbound_prod.len() > 3_000,
        "剥完 stream/inbound/ 生产段只剩 {} 字节 —— 剥法坏了，本条此刻在空转",
        inbound_prod.len()
    );
    // 运行时拼，免得本模块自己的文本被别的扫描器命中。
    let arms = [
        ("link-open", "open"),
        ("link-data", "data"),
        ("link-credit", "credit"),
        ("link-close", "close"),
    ];
    for (cmd, method) in arms {
        let arm = format!("\"{cmd}\" => ");
        let call = format!("links.{method}(&req.id, &req.args)");
        assert_eq!(
            inbound_prod.matches(arm.as_str()).count(),
            1,
            "`stream/inbound/` 生产段里 `{arm}` 不是恰好 1 条硬臂"
        );
        assert_eq!(
            inbound_prod.matches(call.as_str()).count(),
            1,
            "`stream/inbound/` 生产段里 `{call}` 不是恰好 1 处 —— `{cmd}` 那条臂没接到链路表上"
        );
        // 反向自检：把这一处调用剔掉，本条必须看得见（否则它在测「文本里有这个词」）。
        let without = inbound_prod.replace(call.as_str(), "nothing_at_all(");
        assert_eq!(without.matches(call.as_str()).count(), 0);
    }
    let root_prod =
        crate::guard_support::production_code(&crate::guard_support::backend_root_source());
    let flag = format!("{}{}", "\"--", "dial\"");
    assert_eq!(
        root_prod.matches(flag.as_str()).count(),
        0,
        "`main.rs` / `lib.rs` 生产段里还有 `{flag}` —— 那条老入口删了（SR1a），\
         留着一半就是 `is_query_mode` 认它、却没有臂接它（v3.4.0 `--account-trust-zero` 那一形）"
    );
    // 正控：同一把尺子在别的子命令上量得到（不是「这把尺子什么都量不到」）。
    let other = format!("{}{}", "\"--", "resident-ensure\"");
    assert!(
        root_prod.matches(other.as_str()).count() >= 1,
        "正控 `{other}` 一处都没量到 —— 本条在空转"
    );
}

/// ★ 反向自检 · **阳性方向**：把一处锚点放到 `dial/` 外面去，判据必须红**并点名**。
///
/// 语料是**活体**（真语料 + 一处注入），不是手搓的小串 —— 手搓的小串过不了字节地板，
/// 于是会以「空转」而不是「越界」红掉，那就测不到该测的那一条。
#[test]
fn a_dial_anchor_outside_dial_home_is_caught_and_named() {
    let mut corpus = crate_sources();
    let victim = "observe/watcher.rs".to_string();
    let injected = format!(
        "fn f() {{ let _ = {}(a, b, c); }}\n",
        anchors()[2].trim_end_matches('(')
    );
    let slot = corpus
        .iter_mut()
        .find(|(n, _)| *n == victim)
        .unwrap_or_else(|| panic!("语料里没有 {victim} —— 反例的宿主没了，换一个"));
    slot.1.push_str(&injected);

    let e = dial_locality(&corpus).expect_err("锚点长到 dial/ 外面了，判据居然是绿的");
    assert!(
        e.contains(&victim),
        "判据红了，但**没点名是哪一处** —— 只说「有问题」的诊断等于没有诊断。实得：{e}"
    );
}

/// ★ 反向自检 · **阴性方向**（`P6bM4`）：喂空语料，判据必须**自己先红**。
///
/// 少了这一条，「一处锚点都没有」与「语料压根没喂进来」在终端上同形，
/// 而后者会让这条判据在**任何**改动下都保持绿。
#[test]
fn an_empty_corpus_makes_the_judge_red_by_itself() {
    let e = dial_locality(&[]).expect_err("空语料居然判绿 —— 地板断言没接上");
    assert!(
        e.contains("空转"),
        "空语料红了，但红的理由不是「空转」——那说明它是被别的分支拦下的，地板没生效。实得：{e}"
    );
    // 再补一刀：语料非空但**远小于**地板，同样要以「空转」红。
    let tiny = vec![("dial/mod.rs".to_string(), "x".repeat(10))];
    let e2 = dial_locality(&tiny).expect_err("小语料居然判绿");
    assert!(e2.contains("空转"), "小语料红的理由不对：{e2}");
}

/// 地板不是摆设：把真语料**减到**地板以下，判据也要红。
///
/// 这一条与上一条不同 —— 上一条喂的是构造语料，这一条证明地板**对真语料同样有效**
/// （防「地板设得比真语料还小很多，等于没有」那一形）。
#[test]
fn the_corpus_floor_is_not_far_below_the_real_corpus() {
    let real: usize = crate_sources().iter().map(|(_, c)| c.len()).sum();
    assert!(
        real > CORPUS_FLOOR_BYTES,
        "真语料 {real} 字节，还没到地板 {CORPUS_FLOOR_BYTES} —— 地板设高了，判据恒红"
    );
    assert!(
        real < CORPUS_FLOOR_BYTES * 40,
        "真语料 {real} 字节，是地板 {CORPUS_FLOOR_BYTES} 的 40 倍以上 —— \
             地板设得太低，掉掉大半个 crate 它也不会响。**这是一条会随 crate 长大而变松的判据**，\
             长到这一条红的那天，把地板抬上去。"
    );
}

/// `--dial` 的请求行按**蛇形键**读。这条钉的是两端的字段名对得上。
///
/// ⚠ 它只钉本侧的读法；界面那侧写的是什么，由 `stream_source` 的判据钉。
/// **两侧各钉一半** —— 与 `build_id_guard` / `protocol_doc_guard` 的分工同形。
#[test]
fn the_request_line_is_read_with_snake_case_keys() {
    let raw = r#"{"host":"h","port":22,"user":"u","key_path":"/k","host_key_fingerprint":"SHA256:x","command":"c"}"#;
    let req: DialRequest = serde_json::from_str(raw).expect("蛇形键的请求行读不动");
    assert_eq!(req.host, "h");
    assert_eq!(req.port, 22);
    assert_eq!(req.user, "u");
    assert_eq!(req.key_path.as_deref(), Some("/k"));
    assert_eq!(req.host_key_fingerprint.as_deref(), Some("SHA256:x"));
    assert_eq!(req.command, "c");
    // 反向：camelCase 读不动 —— 免得哪天有人「顺手」改成 camelCase 而两端悄悄漂开。
    let camel = r#"{"host":"h","port":22,"user":"u","keyPath":"/k","command":"c"}"#;
    let r2: DialRequest = serde_json::from_str(camel).expect("多余字段应当被忽略");
    assert!(
        r2.key_path.is_none(),
        "camelCase 的 `keyPath` 居然被读进来了 —— 两端的键名契约不是蛇形独占"
    );
}

/// ack 一定是**一行**，而且以 `\n` 收尾 —— 界面在 `read_line` 上等着它。
#[tokio::test]
async fn the_ack_is_exactly_one_newline_terminated_line() {
    let mut buf: Vec<u8> = Vec::new();
    write_ack(
        &mut buf,
        &DialAck {
            ok: true,
            error: None,
            fingerprint: Some("SHA256:x".into()),
            fingerprints: [("a:22", "SHA256:x"), ("b:22", "SHA256:y")]
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .into(),
            jump_fingerprints: [("j:22", "SHA256:j")]
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .into(),
            endpoint: Some("h:22".into()),
            winner: Some(Endpoint {
                host: "h".into(),
                port: 22,
            }),
            strict: true,
            jump_strict: false,
            open_refused: Some("administratively_prohibited"),
            v: ACK_V,
            uses: USES,
        },
    )
    .await
    .expect("写 ack 失败");
    let s = String::from_utf8(buf).expect("ack 不是 utf8");
    assert_eq!(s.matches('\n').count(), 1, "ack 不是一行：{s:?}");
    assert!(s.ends_with('\n'), "ack 没有以换行收尾：{s:?}");
    let v: serde_json::Value = serde_json::from_str(s.trim()).expect("ack 不是合法 JSON");
    assert_eq!(v["ok"], serde_json::Value::Bool(true));
    // 逐地址指纹那一格（additive）。
    assert_eq!(
        v["fingerprints"],
        serde_json::json!({"a:22": "SHA256:x", "b:22": "SHA256:y"})
    );
    // 结构化的胜者与「这一趟是否严格校验」（界面据它记 last-good、判要不要自动固化，不再自己解析地址 / 重推指纹规则）。
    assert_eq!(v["winner"], serde_json::json!({"host": "h", "port": 22}));
    assert_eq!(
        (v["strict"].as_bool(), v["jump_strict"].as_bool()),
        (Some(true), Some(false))
    );
    // 跳板那一台自己那一格（additive）。
    assert_eq!(
        v["jump_fingerprints"],
        serde_json::json!({"j:22": "SHA256:j"})
    );
    // 开通道被回拒的原因码（additive；界面据它分「不许端口转发」与「口上还没人」）。
    assert_eq!(v["open_refused"], "administratively_prohibited");
}

/// （`AllowTcpForwarding no` ⇒ 控制隧道被拒、界面每分钟新拨 33 条）。
/// 开通道失败的原因码 == RFC 4254 §5.1 那张表（期望逐格取自 RFC 原文的码名，异源于实现）；不是开通道失败 ⇒ 不给码。
#[test]
fn channel_open_failures_map_to_the_rfc_reason_words() {
    use russh::ChannelOpenFailure as F;
    let rows = [
        (F::AdministrativelyProhibited, "administratively_prohibited"),
        (F::ConnectFailed, "connect_failed"),
        (F::UnknownChannelType, "unknown_channel_type"),
        (F::ResourceShortage, "resource_shortage"),
        (F::Unknown, "unknown"),
    ];
    for (f, want) in rows {
        assert_eq!(
            open_failure_word(&russh::Error::ChannelOpenFailure(f)),
            Some(want)
        );
    }
    assert_eq!(open_failure_word(&russh::Error::Disconnect), None);
    // 在执行链上：`tunnel` 那一臂开 direct-tcpip 失败时恰好一处把原因码装进 ack（其余几臂不装）。
    let prod =
        crate::guard_support::production_code(include_str!("../../src/backend/dial/uses.rs"));
    let at = guard_core::find_pinned(&prod, "Use::Tunnel => {").expect("tunnel 那一臂不是恰好一处");
    let arm = &prod[at..prod[at..]
        .find("Use::Files =>")
        .map_or(prod.len(), |e| at + e)];
    guard_core::find_pinned(arm, "super::open_failure_word(&e)")
        .expect("tunnel 那一臂没把原因码交出去");
    guard_core::find_pinned(&prod, "DialAck::open_refused(").expect("装原因码的 ack 不是恰好一处");
}

/// v2 的字段全是可选的：老界面（v1 六个字段）发来的请求照样读得动，而且用法缺省是长流。
/// 新字段按蛇形键读：`use` / `endpoints` / `jump` / `capture` / `forward` / `stages` / `probe`。
#[test]
fn a_v2_request_reads_and_a_v1_request_still_reads() {
    let v1 = r#"{"host":"h","port":22,"user":"u","key_path":"/k","host_key_fingerprint":null,"command":"c"}"#;
    let r = parse_request(v1).expect("v1 请求读不动了");
    assert_eq!(r.use_, Use::Stream);
    assert_eq!(
        r.race_order(),
        vec![Endpoint {
            host: "h".into(),
            port: 22
        }]
    );
    assert!(!r.stages && !r.probe && r.jump.is_none());
    let v2 = r#"{"host":"h","port":22,"user":"u","key_path":null,"host_key_fingerprint":null,
        "use":"capture","capture":{"max_bytes":7,"abort_marker":"M"},
        "endpoints":[{"host":"a","port":1},{"host":"b","port":2}],
        "jump":{"host":"j","port":2222,"user":"ju","key_path":"/jk","host_key_fingerprint":"SHA256:j","label":"跳"},
        "stages":true,"probe":true}"#;
    let r = parse_request(v2).expect("v2 请求读不动");
    assert_eq!(r.use_, Use::Capture);
    assert_eq!(
        r.command, "",
        "command 缺省为空（subsystem / forward 不需要它）"
    );
    assert_eq!(
        r.capture
            .as_ref()
            .map(|c| (c.max_bytes, c.abort_marker.clone())),
        Some((7, Some("M".into())))
    );
    assert_eq!(
        r.race_order(),
        vec![
            Endpoint {
                host: "a".into(),
                port: 1
            },
            Endpoint {
                host: "b".into(),
                port: 2
            }
        ],
        "给了竞速顺序就原样用（界面已按 last-good 排好）"
    );
    let j = r.jump.expect("跳板没读进来");
    assert_eq!(
        (j.host.as_str(), j.port, j.user.as_str(), j.label.as_str()),
        ("j", 2222, "ju", "跳")
    );
    assert!(r.stages && r.probe);
    for (word, want) in [
        ("stream", Use::Stream),
        ("capture", Use::Capture),
        ("forward", Use::Forward),
        // 受限的远端文件一问一答（部署）。
        ("files", Use::Files),
        // 到远端常驻后端监听口的隧道。
        ("tunnel", Use::Tunnel),
    ] {
        let raw = format!(
            r#"{{"host":"h","port":1,"user":"u","key_path":null,"host_key_fingerprint":null,"use":"{word}"}}"#
        );
        assert_eq!(parse_request(&raw).unwrap().use_, want);
        assert!(
            USES.contains(&word),
            "`{word}` 读得动却不在 ack 的 `uses` 里 —— 界面会把它判成老代理"
        );
    }
    assert_eq!(
        USES.len(),
        5,
        "`uses` 与 `Use` 的变体必须一一对应（两向：上面逐个读过，这里数一遍）"
    );
    // 隧道：目标口读得进来；它走不占 `MaxSessions` 的那一道（与 `forward` 同）。
    let tun = r#"{"host":"h","port":1,"user":"u","key_path":null,"host_key_fingerprint":null,"use":"tunnel","tunnel_port":49999}"#;
    assert_eq!(parse_request(tun).unwrap().tunnel_port, Some(49999));
    assert_eq!(
        super::uses::lane_of(Use::Tunnel),
        super::uses::Lane::Tunnel,
        "隧道占了 session 通道的格 —— 远端 MaxSessions 会被常驻那条流白白吃掉一格"
    );
    // 原始子系统字节流刻意不认（SFTP 住本机后端，界面只拿 `files` 的一问一答与 `transfer-*`，
    // 见 `dial/mod.rs` 头注）：读到它就是请求坏了。
    let sub = r#"{"host":"h","port":1,"user":"u","key_path":null,"host_key_fingerprint":null,"use":"subsystem"}"#;
    assert!(
        parse_request(sub).is_err(),
        "代理认了 subsystem —— SFTP 的协议字节会回到界面进程"
    );
}

/// ack 与阶段行的线上形状：ack 带 `v`/`uses`/`endpoint`；阶段行是 `{"stage":{"kind":…}}`，
/// `kind` 是 camelCase 的六个字面量（界面 `ConnectStage` 那一侧对拍）。
#[tokio::test]
async fn the_ack_and_the_stage_lines_have_the_shape_the_monitor_reads() {
    let mut buf: Vec<u8> = Vec::new();
    let sink = StageSink::new(true);
    sink.emit(Stage::Dialing {
        endpoint: "h:22".into(),
    });
    sink.emit(Stage::HostKey {
        endpoint: "h:22".into(),
        fingerprint: "SHA256:x".into(),
    });
    sink.emit(Stage::Failed {
        endpoint: "b:22".into(),
        reason: "[tcp] refused".into(),
    });
    sink.emit(Stage::Won {
        endpoint: "h:22".into(),
    });
    sink.emit(Stage::Auth {
        ok: true,
        detail: None,
    });
    sink.emit(Stage::Established);
    write_stages_then_ack(
        &mut buf,
        &sink,
        &DialAck::failed("x".into(), Some("SHA256:x".into())),
    )
    .await
    .unwrap();
    let text = String::from_utf8(buf).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 7, "六行阶段 ＋ 恰好一行 ack：{text}");
    let kinds: Vec<&str> = lines[..6]
        .iter()
        .map(|v| v["stage"]["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["dialing", "hostKey", "failed", "won", "auth", "established"]
    );
    assert_eq!(lines[1]["stage"]["fingerprint"], "SHA256:x");
    let ack = &lines[6];
    assert_eq!(ack["ok"], false);
    assert_eq!(ack["v"], ACK_V);
    assert_eq!(ack["uses"], serde_json::json!(USES));
    assert_eq!(
        ack["fingerprint"], "SHA256:x",
        "失败的 ack 也带上看到过的指纹（TOFU 展示要它）"
    );
    assert_eq!(
        ack["fingerprints"],
        serde_json::json!({}),
        "失败的 ack 逐地址那一格恒空"
    );
    // 不要阶段 ⇒ 一个字节的阶段都不出
    let mut quiet: Vec<u8> = Vec::new();
    let off = StageSink::new(false);
    off.emit(Stage::Established);
    write_stages_then_ack(&mut quiet, &off, &DialAck::failed("x".into(), None))
        .await
        .unwrap();
    assert_eq!(String::from_utf8(quiet).unwrap().lines().count(), 1);
}

/// 〔C2 → COPY〕拨号失败按错误**类型**分阶段标签（不看错误串：串改一个字不该改分类）。
#[test]
fn a_dial_error_is_bucketed_into_a_stage_label() {
    use super::connect::{stage_of_io, stage_of_russh};
    use std::io::{Error, ErrorKind as K};
    assert_eq!(stage_of_io(&Error::from(K::ConnectionRefused)), "tcp");
    assert_eq!(stage_of_io(&Error::from(K::HostUnreachable)), "tcp");
    assert_eq!(stage_of_io(&Error::from(K::TimedOut)), "timeout");
    assert_eq!(stage_of_io(&Error::other("超时 指纹 key timeout")), "other");
    assert_eq!(
        stage_of_russh(&russh::Error::IO(Error::from(K::NetworkUnreachable))),
        "tcp"
    );
    assert_eq!(stage_of_russh(&russh::Error::InactivityTimeout), "timeout");
    assert_eq!(stage_of_russh(&russh::Error::UnknownKey), "hostkey");
    assert_eq!(stage_of_russh(&russh::Error::Kex), "other");
}

/// ★ 多开的判准只住一处（`dial/pool.rs`；，见 `dial_pool_tests.rs` 头注 NT1 那一段 —— 行为判据在那边，这一条是源码面的）：`Family::dial_reason` 是生产段里唯一给出 `Why::` 的地方（`place` 只转述它），
/// 且 `Extra(` 只在 `place` 里造。针运行时拼。
#[test]
fn the_multi_open_judge_lives_in_one_function() {
    let src = crate::guard_support::production_code(include_str!("../../src/backend/dial/pool.rs"));
    let body = |name: &str| -> String {
        let at = src
            .find(&format!("fn {name}("))
            .or_else(|| src.find(&format!("fn {name}<")))
            .unwrap_or_else(|| panic!("找不到 fn {name}"));
        let rest = &src[at..];
        let open = rest.find('{').unwrap();
        let mut depth = 0usize;
        for (i, ch) in rest[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return rest[..open + i + 1].to_string();
                    }
                }
                _ => {}
            }
        }
        panic!("fn {name} 括号不配平");
    };
    let why = [
        format!("{}::{}", "Why", "Full"),
        format!("{}::{}", "Why", "Bulk"),
    ];
    let reason = body("dial_reason");
    let place_fn = body("place");
    for w in &why {
        assert!(reason.contains(w.as_str()), "dial_reason 里没有 {w}");
        assert!(
            !place_fn.contains(w.as_str()) || w.ends_with("Bulk"),
            "place 自己造了 {w}"
        );
    }
    // `Why::Bulk` 在 place 里只许出现在「托住批量连接」那一处比较里。
    assert_eq!(place_fn.matches(why[1].as_str()).count(), 1);
    let extra = format!("{}::{}(", "How", "Extra");
    assert_eq!(
        src.matches(extra.as_str()).count(),
        1,
        "`Extra(` 该只在 place 里造一次"
    );
    assert!(place_fn.contains(extra.as_str()));
}

// ═══ capture 那一臂：被丢 ⇒ 远端那条通道被关 ═══════════════════════════════════════
//
// 守的要求（住址，纪律 19）：红线（逐字）「复用后它占掉共享连接一个槽永不释放，局部卡死升级成全局卡死」·
// 「**本地撤单**与**对端撤活**是两件事」。

/// 一个记账的「写半边」：被关一次记一次。
struct CountingHalf(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl super::uses::Closable for CountingHalf {
    fn close_it(self) -> impl std::future::Future<Output = ()> + Send {
        async move {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

/// A5 ★ 守着写半边的那一层被丢 ⇒ **恰好**发一次关（行为：替身记账；异源：不看源码）；没被丢之前一次都不发（另一向）。
#[tokio::test]
async fn dropping_the_guard_closes_the_channel_exactly_once() {
    let n = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let g = super::uses::CloseOnDrop::new(CountingHalf(n.clone()));
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        n.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "还攥着就关了"
    );
    let _ = g.get();
    drop(g);
    for _ in 0..64 {
        if n.load(std::sync::atomic::Ordering::SeqCst) > 0 {
            break;
        }
        tokio::task::yield_now().await;
    }
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        n.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "被丢之后没有恰好发一次关 —— 远端那条通道（与它上面的进程）会留着，本地预算却把格还了"
    );
}

/// A5 接线（文本，如实登记：按行为量要真 sshd）：capture 那一臂拿到通道之后**先 `split`、写半边交给守卫**，
/// exec 走守卫里那一半、收全走读半边；裸通道（`&channel` / `&mut channel`）在那一臂里零处。
#[test]
fn the_capture_arm_holds_its_channel_under_the_close_guard() {
    let src = crate::guard_support::production_code(include_str!("../../src/backend/dial/uses.rs"));
    let at = guard_core::find_pinned(&src, "Use::Capture => {")
        .unwrap_or_else(|e| panic!("capture 那一臂不是恰好一处：{e}"));
    let end = guard_core::find_pinned(&src, "Use::Forward => {")
        .unwrap_or_else(|e| panic!("forward 那一臂不是恰好一处：{e}"));
    let arm = &src[at..end];
    for need in [
        "channel.split()",
        "CloseOnDrop::new(wr)",
        "exec_half(wr.get(),",
        "collect(&mut rd,",
    ] {
        assert_eq!(
            arm.matches(need).count(),
            1,
            "capture 臂里 `{need}` 不是恰好一处"
        );
    }
    for bare in ["exec(&channel", "&mut channel"] {
        assert_eq!(
            arm.matches(bare).count(),
            0,
            "capture 臂里又拿裸通道干活了：`{bare}`"
        );
    }
    // 正控：同一切法在 stream 臂里看得见裸 exec（识别器没瞎）。
    let st = guard_core::find_pinned(&src, "Use::Stream => {")
        .unwrap_or_else(|e| panic!("stream 那一臂不是恰好一处：{e}"));
    let stream = &src[st..at];
    assert_eq!(
        stream.matches("exec(&channel").count(),
        1,
        "正控失败：stream 臂里认不出 `exec(&channel`"
    );
}
