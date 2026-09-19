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
//! 甲半（界面这一侧：daemon 那条长连接流不再自己拨号）住
//! `../../bridge/src/ssh_source.rs` 的测试模块 —— **两侧各扫各的 crate**，
//! 刻意不从这里 `include_str!` 伸到对面去（那会新增一条跨轨编译期边，
//! 而那张登记表不在本轮写区里）。

use super::*;

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
const CORPUS_FLOOR_BYTES: usize = 50_000;

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
/// ⚠⚠ **代价必须知道**：`scan_tree!` 按构造**摘除调用者自己那一份** ——
/// 也就是 `dial/mod.rs` 不在它返回的人群里。而拨号锚点恰恰全住这一份 ⇒
/// 光靠它，判据的「至少有一处」永远为 0。
/// ⇒ 本函数把自己那一份**显式**用 `include_str!` 补回来，两半各司其职：
/// **树扫描管「别处有没有」**（它摘掉自己正好），**`include_str!` 管「自己有没有」**。
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
    // 自己那一份 —— `scan_tree!` 摘掉了它，而它正是被测对象。
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

/// ★ **这条代理真的接得到 —— 不只是「代码住在这儿」。**
///
/// 🔴 **它是本轮 `7u` 探针逮出来的一格，不是设计时想到的。**
/// 实打：把 `main.rs` 里那条 `--dial` 分派臂**整条摘掉**（`dial/` 的代码一个字不动），
/// daemon 侧 **596 条判据一条都不红** —— 上面那条 `the_dial_only_happens_under_dial_home`
/// 断的是**住在哪**（locality），断不了**接没接上**（reachability），
/// 而 `argv_table_guard` 那条只看「存在的臂调不调实现」，摘掉的臂它看不见。
///
/// 那时的运行期形状：`--dial` 还在 `SUBCOMMANDS` 里 ⇒ `is_query_mode` 判真 ⇒
/// 落进 `_` 臂走历史查询 ⇒ `unknown argument` + exit 2。
/// **那正是 v3.4.0 `--account-trust-zero` 漏登记那次事故的形状**（只不过方向反过来：
/// 那次是表里漏、这次是臂里漏）。端到端不静默（界面会看到「代理一个字节都没回」），
/// 但**源码级的这次退化没有任何判据拦得住** —— 本条就是补上的那一刀。
#[test]
fn the_dial_arm_is_actually_wired_into_the_dispatch() {
    let main_prod =
        crate::guard_support::production_code(include_str!("../../src/backend/main.rs"));
    assert!(
        main_prod.len() > 3_000,
        "剥完 main.rs 生产段只剩 {} 字节 —— 剥法坏了，本条此刻在空转",
        main_prod.len()
    );
    // 运行时拼，免得本模块自己的文本被别的扫描器命中。
    let flag = format!("{}{}", "\"--", "dial\"");
    let call = format!("dial::{}(", "run");
    assert_eq!(
        main_prod.matches(flag.as_str()).count(),
        2,
        "`main.rs` 生产段里 `{flag}` 不是恰好 **2** 处。\
             那 2 处各有各的活，缺一个后果都不一样：\
             ① `SUBCOMMANDS` 那张表 —— `is_query_mode` 的闸门读它，不在表里就被当未知 flag \
             **静默进流模式**（v3.4.0 `--account-trust-zero` 那次事故的形状）；\
             ② `match` 那条分派臂 —— 不在就落进 `_` 臂走历史查询、`unknown argument` + exit 2。\
             ≥3 处 ⇒ 有第三个地方在认这个 token，先说清那是谁。"
    );
    assert_eq!(
        main_prod.matches(call.as_str()).count(),
        1,
        "`main.rs` 生产段里 `{call}` 不是恰好 1 处 —— \
             **分派臂被摘掉了**：代理的代码还在 `dial/`，但没有任何人调得到它。\
             那时 `--dial` 会落进 `_` 臂走历史查询，`unknown argument` + exit 2。"
    );
    // 反向自检：把臂那一行从语料里剔掉，本条必须红（否则它在测「文本里有这个词」）。
    let without_arm = main_prod.replace(call.as_str(), "nothing_at_all(");
    assert_eq!(
        without_arm.matches(call.as_str()).count(),
        0,
        "剔不掉那条臂 —— 本条的反向自检此刻是空转的"
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
/// ⚠ 它只钉本侧的读法；界面那侧写的是什么，由 `ssh_source` 的判据钉。
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
        },
    )
    .await
    .expect("写 ack 失败");
    let s = String::from_utf8(buf).expect("ack 不是 utf8");
    assert_eq!(s.matches('\n').count(), 1, "ack 不是一行：{s:?}");
    assert!(s.ends_with('\n'), "ack 没有以换行收尾：{s:?}");
    let v: serde_json::Value = serde_json::from_str(s.trim()).expect("ack 不是合法 JSON");
    assert_eq!(v["ok"], serde_json::Value::Bool(true));
}
