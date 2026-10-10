//! # 要求住址：`INVARIANTS §48.1`（常驻后端的门由内核给：只给本人的套接字 ＋ 对端 uid，没有钥匙）
//!
//! 本族钉帧面那一半：常驻开关怎么认（`mode_from`：空 / `1` / 别的值拒）· attach 行只判形状（`attach_verdict`）·
//! 拒绝理由闭集 · 握手行上界 · 退出码。门本身（目录独占锁 · 对端 uid）在共享 crate `own-chan` 的判据里，
//! 抢门牌与沙箱那一判在 `main_claim_tests.rs` / `resident_tests.rs`。

use super::*;

fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |k: &str| {
        pairs
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| (*v).to_string())
    }
}

/// 没有 / 空串 ⇒ stdio（shell 里 `export X=` 是常态）；`1` ⇒ 常驻；别的值 ⇒ 拒，且点名那个变量。
/// 从前那两个变量（口 · 钥匙文件）给了也不认：常驻只看这一个开关。
#[test]
fn the_resident_switch_is_empty_one_or_refused() {
    assert_eq!(mode_from(&env_of(&[])).unwrap(), Mode::Stdio);
    assert_eq!(
        mode_from(&env_of(&[(ENV_RESIDENT, "  ")])).unwrap(),
        Mode::Stdio
    );
    assert_eq!(
        mode_from(&env_of(&[(ENV_RESIDENT, "1")])).unwrap(),
        Mode::Listen
    );
    for bad in ["0", "yes", "true", "2"] {
        let (e, diag) = crate::common::contract::tests::diag(|| {
            mode_from(&env_of(&[(ENV_RESIDENT, bad)])).unwrap_err()
        });
        assert!(
            diag.contains(ENV_RESIDENT),
            "拒绝的诊断没点名开关：{e} / {diag}"
        );
    }
    assert_eq!(
        mode_from(&env_of(&[
            ("CCM_LISTEN_PORT", "51000"),
            ("CCM_LISTEN_TOKEN_FILE", "/h/k")
        ]))
        .unwrap(),
        Mode::Stdio,
        "旧的两个变量还在起作用"
    );
}

/// attach 行可带这条连接的流模式旗标：缺 ⇒ 用进程默认；只认 `STREAM_FLAGS` 里那几个，别的一律当 malformed。
#[test]
fn attach_flags_are_optional_closed_and_per_connection() {
    assert_eq!(attach_flags(r#"{"attach":true}"#), Ok(None));
    assert_eq!(
        attach_flags(r#"{"attach":true,"flags":["--tail-only","--with-pid","--with-raw"]}"#),
        Ok(Some(crate::StreamWants {
            with_bg: false,
            tail_only: true,
            with_pid: true,
            with_raw: true,
            tz: Default::default(),
        }))
    );
    // 看的那一台的时区跟在旁边一格（不进 `flags`）：认得 ⇒ 那个时区；认不得 ⇒ UTC。
    let sh = crate::Tz::named("Asia/Shanghai").unwrap();
    assert_eq!(
        attach_flags(r#"{"attach":true,"flags":[],"tz":"Asia/Shanghai"}"#).map(|w| w.map(|w| w.tz)),
        Ok(Some(sh))
    );
    assert_eq!(
        attach_flags(r#"{"attach":true,"flags":[],"tz":"Nowhere/Atlantis"}"#)
            .map(|w| w.map(|w| w.tz)),
        Ok(Some(crate::Tz::default()))
    );
    assert_eq!(
        attach_flags(r#"{"attach":true,"flags":[]}"#),
        Ok(Some(crate::StreamWants::default())),
        "空表 = 一个都不要（老后端那一形），不是「用默认」"
    );
    assert_eq!(
        attach_flags(r#"{"attach":true,"flags":["--search"]}"#),
        Err(())
    );
    assert_eq!(
        attach_flags(r#"{"attach":true,"flags":"--tail-only"}"#),
        Err(())
    );
}

/// attach 行只判形状：`{"attach":true}`（可带 `flags`）才要流；钥匙串、数字、缺字段、不是 JSON 都是 malformed。
#[test]
fn attach_verdicts_judge_the_shape_only() {
    assert_eq!(attach_verdict("{\"attach\":true}"), Verdict::Attach);
    assert_eq!(attach_verdict("{\"attach\":true}\n"), Verdict::Attach);
    assert_eq!(
        attach_verdict("{\"attach\":true,\"flags\":[\"--tail-only\"]}"),
        Verdict::Attach
    );
    for bad in [
        "{\"attach\":\"0123456789abcdef0123456789abcdef\"}",
        "{\"attach\":false}",
        "{\"attach\":1}",
        "{}",
        "not json",
    ] {
        assert_eq!(attach_verdict(bad), Verdict::Malformed, "{bad}");
    }
}

/// 分档表逐格钉死：形状对就交流（多客户，不看有没有人占着）；不对就拒、理由是 malformed。
#[test]
fn the_two_tier_split_is_pinned_cell_by_cell() {
    assert_eq!(admit(Verdict::Attach), Admit::Stream);
    assert_eq!(admit(Verdict::Malformed), Admit::Refuse(REFUSE_MALFORMED));
}

/// 拒绝理由是闭集，且拼出来的每一行都是**合法 JSON**。
///
/// ⚠ 这条防的是「顺手把一段外来字节拼进那行」：只要 `reason` 还是这三个常量，
/// 那行就不可能被撕开；哪天有人改成 `refusal_line(&err.to_string())`，本条当场红。
#[test]
fn refusal_reasons_are_a_closed_set() {
    for r in [REFUSE_MALFORMED, REFUSE_ABSENT, REFUSE_UNREACHABLE] {
        let line = refusal_line(r);
        assert!(line.ends_with('\n'), "NDJSON 每行必须以换行收尾：{line:?}");
        let v: serde_json::Value =
            serde_json::from_str(line.trim()).expect("拒绝行必须是合法 JSON");
        assert_eq!(v["attach"], "refused");
        assert_eq!(v["reason"], r);
    }
    // ★★ 反向锚点：**喂给 `refusal_line` 的实参只许来自那个闭集。**
    //
    // ⚠⚠ 〔08-26 收工前自查逮到的〕本条第一版扫的是 **`listen.rs` 自己**，
    //    而 `refusal_line` 的**唯一调用点在 `main.rs`** —— 本模块生产段里
    //    带 `refusal_line(` 的行只有那条 `pub fn` 声明（而它被 filter 掉了）
    //    ⇒ 那个循环**跑零圈**，是个**空转**的判据。
    //    它读起来完全正常，而它一个字节都没在守。⇒ 人群搬到真正的调用点。
    //    ★ 这是本轮「守卫范围 ≠ 性质范围」那一族的第四形：**人群画在了错的文件上**。
    // 小中继（`control/resident.rs`）那一处由下面另一段钉。
    let caller =
        crate::guard_support::production_code(include_str!("../../../src/backend/main.rs"));
    let calls: Vec<&str> = caller
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("refusal_line("))
        .collect();
    // 反空真：一个调用点都扫不到 ⇒ 下面整段空转（第一版就是死在这一格）。
    assert_eq!(
        calls.len(),
        1,
        "`main.rs` 生产段里 `refusal_line(` 有 {} 处（应恰好 1 处）：{calls:?}\n\
             0 处 = 抽取坏了或调用点搬家了，本条在空转；≥2 处 = 拒绝理由有第二个产出点。",
        calls.len()
    );
    assert!(
        calls[0].contains("refusal_line(reason)"),
        "`refusal_line` 的实参不是那个从 `Admit::Refuse` 里解出来的 `reason`：{}\n\
             ⇒ 有人往那行 JSON 里拼了一段**外来字节**（比如 `&e.to_string()`）—— 那会撕行。",
        calls[0]
    );
    assert!(
        caller.contains("listen::Admit::Refuse(reason)"),
        "`main.rs` 里那个 `reason` 不再是从 `Admit::Refuse` 解出来的 —— \
             那它是从哪来的？闭集这件事就断在这里。"
    );
    // 小中继那一处：实参是 `relay_refusal` 的答（只回两个常量）。
    let relay = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/resident.rs"
    ));
    let relay_calls: Vec<&str> = relay
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("refusal_line("))
        .collect();
    assert_eq!(relay_calls.len(), 1, "{relay_calls:?}");
    assert!(relay_calls[0].contains("refusal_line(reason)"));
    assert!(relay.contains("let reason = relay_refusal(e.kind());"));
    assert_eq!(
        crate::control::resident::relay_refusal(std::io::ErrorKind::NotFound),
        REFUSE_ABSENT
    );
    assert_eq!(
        crate::control::resident::relay_refusal(std::io::ErrorKind::ConnectionRefused),
        REFUSE_ABSENT
    );
    assert_eq!(
        crate::control::resident::relay_refusal(std::io::ErrorKind::PermissionDenied),
        REFUSE_UNREACHABLE
    );
}

/// ★★ `重-1`：**本轮新增那三个文件的头注里，每一个住址今天都真的在。**
///
/// 〔`K-P1-D1` `重-1`〕`listen.rs` 头注原先写「由 `frozen_single_client_guard.rs`
/// 那条触发器看着」—— 那个文件**全仓不存在**（命中 1 处，就是那一行自己），
/// 真名是 `single_stream_guard.rs`。
///
/// ⚠ **治的不是那一个词**，也不是「本模块头注」这一处（`brief` 第 15 条那一问：
/// 我治的是这一处，还是**所有同职的地方**？）⇒ 人群是**本轮新增的那三个后端文件**
/// 的头注，逐个文件、逐个住址。一条承重头注把读者指向一个不存在的住址，
/// 是「**指了住址，但住址是假的**」——`brief` 第 13 条那一族的反面。
///
/// # 分母（现打 08-27，量具 = 一段正则 + 逐条落盘核在不在）
///
/// - 本轮新增的三个后端文件（`listen.rs` / `single_stream_guard.rs` / `ratchet_guard.rs`）
///   **全文**的路径式引用共 **22 处**（去掉一个正则假阳性 `buf.shrink_to_fit()`），
///   **悬空 1 处**，就是这一处；另两份**零悬空**。
/// - ⚠ **射程之外，如实登记**：同一把尺子扫**全 crate 69 个 `.rs` 的 `//!` 头注**
///   ⇒ 105 处引用、**悬空 1 处**：`platform/pidwatch/linux.rs` 头注的
///   「从 `platform/pidwatch.rs` 逐字搬来」（今天是 `platform/pidwatch/mod.rs`）。
///   那是一句**历史出处**、不是「由谁看着」，而且那个文件**不在本轮写区** ⇒ 交回 PM。
///   ⇒ **本条不是全 crate 的**：它管的是本件自己新增的那三份。
#[test]
fn every_file_this_head_note_points_at_really_exists() {
    let heads = [
        (
            "listen.rs",
            include_str!("../../../src/backend/stream/listen.rs"),
        ),
        (
            "single_stream_guard.rs",
            include_str!("../single_stream_guard.rs"),
        ),
        ("ratchet_guard.rs", include_str!("../ratchet_guard.rs")),
    ];
    let head: String = heads
        .iter()
        .flat_map(|(_, src)| src.lines().take_while(|l| l.starts_with("//!")))
        .collect::<Vec<_>>()
        .join("\n");
    // 反空真①：三份头注切不出来 ⇒ 下面整段空转。
    assert!(
        head.len() > 4000,
        "三份头注只切到 {} 字节 —— 切错了，本条此刻在空转",
        head.len()
    );
    // 反空真①b：**三份都要有**（少一份 = 人群悄悄缩了一格，而它读起来完全正常）。
    for (name, src) in &heads {
        assert!(
            src.lines().take_while(|l| l.starts_with("//!")).count() >= 10,
            "`{name}` 的头注只有几行 —— 它被搬空了，本条对这一份在空转"
        );
    }
    // 头注里写住址有四种形态：后端树内的裸文件名 · `relay/xxx.rs` 这种树内相对路径 ·
    // `src/frontend/shell/src/xxx.rs` / `tests/e2e/xxx.sh` 这种从仓根写起的 ·
    // **monitor 侧的裸文件名**（`backend_policy.rs` —— 跨半个仓引用在本仓是常态）。四个根都试。
    let roots = [
        // 〔搬树 2026-09-17〕后端源码树从 `src/backend` 搬到 `<repo>/src/backend`，
        // manifest 留在原处 ⇒ 这一格不再是 `manifest/src`。走那个唯一住址。
        crate::guard_support::src_root(),
        // 〔搬测试 2026-09-17〕头注里点名的很多是判据文件，它们今天住第二棵树。
        crate::guard_support::tests_root(),
        crate::guard_support::repo_root(),
        crate::guard_support::repo_root().join("src/frontend/shell/src"),
        // 🔴 〔搬树 2026-09-18 ·  纪律 3〕**monitor 也有第二棵树。**
        //    上一格（`src/frontend/shell/src`）接的是「头注点名 monitor 侧的裸文件名」那一形，
        //    而剖分之后 monitor 的判据整批住 `<repo>/tests/frontend/shell/`
        //    ⇒ 头注里那些 `X_tests.rs` 四个根一个都够不着，被读成「假住址」。
        crate::guard_support::repo_root().join("tests/frontend/shell"),
    ];
    let mut checked = 0usize;
    for word in head.split(|c: char| !(c.is_ascii_alphanumeric() || "_./-".contains(c))) {
        let w = word.trim_matches(|c| c == '.' || c == '/');
        if !(w.ends_with(".rs") || w.ends_with(".sh")) {
            continue;
        }
        checked += 1;
        // `relay/xxx.rs` 是模块住址：`relay` 的根（`mod.rs` 所在）住 `src/comms/outward/`，非成员 door / listen 住 `src/backend/relay/`。
        let as_relay_module = w
            .strip_prefix("relay/")
            .is_some_and(|r| crate::guard_support::relay_root().join(r).exists());
        assert!(
            roots.iter().any(|r| r.join(w).exists()) || as_relay_module,
            "头注指着 `{w}`，而五个根下都找不到它（后端生产树 · 后端测试树 · 仓根 · \
                 monitor 生产树 `src/frontend/shell/src/` · monitor 测试树 `tests/frontend/shell/`）——\n\
                 ★ 指了住址而住址是假的：读者会以为那一格有人守着，去找的时候什么都没有。\n\
                 ⇒ 要么改成真名，要么把那句话删掉；**别留一个假住址**。"
        );
    }
    // 反空真②：一个住址都没扫到 ⇒ 上面的 `for` 跑零圈（本轮 `refusal_reasons_are_a_closed_set`
    // 第一版正是死在这一格：人群画在了错的文件上，循环跑零圈而读起来完全正常）。
    assert!(
        checked >= 9,
        "三份头注里只扫到 {checked} 个文件式住址（下限 9；08-27 实测 **11** = \
             `listen.rs` 4 + `single_stream_guard.rs` 4 + `ratchet_guard.rs` 3，留两格余量）\
             —— 抽取坏了或某一份头注被搬空了"
    );
    // ★ 那条触发器**按名字**指得住：`single_stream_guard.rs` 必须被头注点到。
    assert!(
        head.contains("single_stream_guard.rs"),
        "头注不再指名那条「多客户端的流」触发器 —— 明确不做的那一半就只剩一句散文"
    );
    assert!(
        include_str!("../single_stream_guard.rs")
            .contains("fn the_single_stream_shape_is_still_exactly_one_client"),
        "`single_stream_guard.rs` 里那条触发器不见了 —— 头注在替一个不存在的性质背书"
    );
}

/// ★ `ATTACH_OK_LINE` 也得是合法的一行 NDJSON（同上，形状钉死）。
#[test]
fn the_ok_line_is_one_valid_ndjson_line() {
    assert!(ATTACH_OK_LINE.ends_with('\n'));
    assert_eq!(ATTACH_OK_LINE.matches('\n').count(), 1);
    let v: serde_json::Value = serde_json::from_str(ATTACH_OK_LINE.trim()).expect("合法 JSON");
    assert_eq!(v["attach"], "ok");
}

/// ★★ **超限之后内存不涨** —— 这条性质只有把 `cap` 做成参数才测得动。
///
/// 拿真常量（8 KiB）来测的话，要喂进去的字节量大到只能靠量 RSS 去证，
/// 而那种证法进不了单测（backend 侧当年正是那么发现问题的）。
#[tokio::test]
async fn an_over_cap_attach_line_is_dropped_whole_and_says_so() {
    let long = format!("{}\n", "x".repeat(100));
    let mut rd = tokio::io::BufReader::new(long.as_bytes());
    assert_eq!(
        read_capped_line(&mut rd, 8).await.expect("读"),
        HandshakeLine::TooLong(100),
        "超限的行必须**整行丢弃并报出字节数**，不许截断成一行「看起来对」的 JSON"
    );
}

/// 三种结局各一格。⚠ `Eof` 那一格**不是错误** —— 它正是「只读 hello 就走」那一档。
#[tokio::test]
async fn the_three_handshake_line_outcomes_are_each_reachable() {
    let mut eof = tokio::io::BufReader::new(&b""[..]);
    assert_eq!(
        read_capped_line(&mut eof, 64).await.expect("读"),
        HandshakeLine::Eof
    );

    let mut one = tokio::io::BufReader::new(&b"{\"attach\":true}\n"[..]);
    assert_eq!(
        read_capped_line(&mut one, 64).await.expect("读"),
        HandshakeLine::Line("{\"attach\":true}".into())
    );

    // 没有换行就 EOF：仍当一行交出去（`read_line` 的旧行为逐字如此）。
    let mut half = tokio::io::BufReader::new(&b"abc"[..]);
    assert_eq!(
        read_capped_line(&mut half, 64).await.expect("读"),
        HandshakeLine::Line("abc".into())
    );
}

/// 两个退出码不许撞：宿主按它们分「已有人在听」与「配置写错了」两条不同的路。
#[test]
fn the_two_exit_codes_are_distinct_and_nonzero() {
    assert_ne!(EXIT_ADDR_IN_USE, EXIT_BAD_LISTEN_CONFIG);
    assert!(EXIT_ADDR_IN_USE > 0 && EXIT_BAD_LISTEN_CONFIG > 0);
}

/// 多客户：A 与 C 同时连着，A 走了不算「最后一个」；都走了才归零；同一个号走两次不重复扣。
#[test]
fn the_last_client_is_the_last_of_all_connections_not_the_first_to_leave() {
    let mut c = Clients::default();
    let a = c.join();
    let b = c.join();
    assert_ne!(a, b);
    assert_ne!(a, 0, "0 是空转那一份 watcher 的槽位号");
    assert_eq!(
        c.leave(a),
        1,
        "A 走了就算归零 —— C 还连着，后端却会按退出行为退"
    );
    assert_eq!(c.leave(a), 1, "同一条走两次扣了两次");
    assert_eq!(c.leave(b), 0);
}
