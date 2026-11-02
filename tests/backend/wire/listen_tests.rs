//! # 要求住址：`INVARIANTS §48.1`（常驻后端的门由内核给：只给本人的套接字 ＋ 对端 uid，没有钥匙）
//!
//! 本族钉帧面那一半：常驻开关怎么认（`mode_from`：空 / `1` / 别的值拒）· attach 行的形状与回话里的新旧（`admit`）·
//! 拒绝理由闭集 · 握手行上界 · 退出码。门本身（目录独占锁 · 对端 uid）在共享 crate `own-chan` 的判据里，
//! 抢门牌与沙箱那一判在 `main_claim_tests.rs` / `resident_tests.rs`。

use super::*;

impl<W> Down<W> {
    /// 写下去的那一头（判据看压过的字节）。
    fn get_ref(&self) -> &W {
        &self.inner
    }
}

fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |k: &str| {
        pairs
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| (*v).to_string())
    }
}

/// 没有 / 空串 ⇒ stdio（shell 里 `export X=` 是常态）；`1` ⇒ 常驻；别的值 ⇒ 拒，且点名那个变量。
/// 常驻只看这一个开关。
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

/// attach 行只认 `cmd_wire::attach::AttachLine` 那一形（认不得的格 · `attach` 不是 `true` · 不是 JSON ⇒ malformed，同一句拒）；
/// 名单（`watch`）原样交出去；带 `mine` ⇒ 回话里是这台相对它的新旧，四态各一：那台旧 · 新 · 同 · 解不出序键；没带 ⇒ 回话与只说「可以」那一行逐字节相同。
#[test]
fn the_attach_reply_says_how_this_build_stands_against_the_clients() {
    let own = "p5c-own";
    let rel = |line: &str| match admit(line, own) {
        Admit::Stream { reply, .. } => reply.line(),
        Admit::Refuse(r) => format!("refused {r}"),
    };
    assert_eq!(rel(r#"{"attach":true}"#), "{\"attach\":\"ok\"}\n");
    for (mine, want) in [
        ("p5d-next", "older"),
        ("p5b-prev", "newer"),
        ("p5c-own", "same"),
        ("dev-build", "incomparable"),
    ] {
        assert_eq!(
            rel(&format!(r#"{{"attach":true,"mine":"{mine}"}}"#)),
            format!("{{\"attach\":\"ok\",\"relation\":\"{want}\"}}\n"),
            "{mine}"
        );
    }
    assert_eq!(
        admit(r#"{"attach":true,"tz":"Asia/Shanghai"}"#, own),
        Admit::Stream {
            tz: Some("Asia/Shanghai".into()),
            reply: cmd_wire::attach::AttachReply::Ok {
                relation: None,
                deflate: false
            },
            watch: None,
        }
    );
    // 要了 deflate ⇒ 这台总认（回话多一格）；没要 ⇒ 不回。
    assert_eq!(
        rel(r#"{"attach":true,"deflate":true}"#),
        "{\"attach\":\"ok\",\"deflate\":true}\n"
    );
    // 带名单 ⇒ 原样交给接流那一步（从第一帧起照它发）。
    assert!(matches!(
        admit(r#"{"attach":true,"watch":{"sids":["a"]}}"#, own),
        Admit::Stream { watch: Some(w), .. } if w.sids == ["a"]
    ));
    for bad in [
        r#"{"attach":"0123456789abcdef0123456789abcdef"}"#,
        r#"{"attach":false}"#,
        r#"{"attach":1}"#,
        r#"{"attach":true,"flags":[]}"#,
        r#"{"attach":true,"view":{}}"#,
        "{}",
        "not json",
    ] {
        assert_eq!(admit(bad, own), Admit::Refuse(REFUSE_MALFORMED), "{bad}");
    }
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
    //    而 `refusal_line` 的**唯一调用点在进程入口 `stream/entry.rs`** —— 本模块生产段里
    //    带 `refusal_line(` 的行只有那条 `pub fn` 声明（而它被 filter 掉了）
    //    ⇒ 那个循环**跑零圈**，是个**空转**的判据。
    //    它读起来完全正常，而它一个字节都没在守。⇒ 人群搬到真正的调用点。
    //    ★ 这是「守卫范围 ≠ 性质范围」那一族的第四形：**人群画在了错的文件上**。
    // 小中继（`control/resident.rs`）那一处由下面另一段钉。
    let caller = guard_core::production_code(include_str!("../../../src/backend/stream/entry.rs"));
    let calls: Vec<&str> = caller
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("refusal_line("))
        .collect();
    // 反空真：一个调用点都扫不到 ⇒ 下面整段空转（第一版就是死在这一格）。
    assert_eq!(
        calls.len(),
        1,
        "`stream/entry.rs` 生产段里 `refusal_line(` 有 {} 处（应恰好 1 处）：{calls:?}\n\
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
        "`stream/entry.rs` 里那个 `reason` 不再是从 `Admit::Refuse` 解出来的 —— \
             那它是从哪来的？闭集这件事就断在这里。"
    );
    // 小中继那一处：实参是 `relay_refusal` 的答（只回两个常量）。
    let relay =
        guard_core::production_code(include_str!("../../../src/backend/control/resident.rs"));
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

/// ★★ **`listen.rs` · `single_stream_guard.rs` · `ratchet_guard.rs` 三份头注里，每一个住址今天都真的在。**
///
/// 一条承重头注把读者指向一个不存在的住址，是「**指了住址，但住址是假的**」。
/// 人群是这三份文件的头注，逐个文件、逐个住址；全 crate 的 `//!` 头注不在本条射程里。
#[test]
fn every_file_this_head_note_points_at_really_exists() {
    let heads = [
        (
            "listen.rs",
            include_str!("../../../src/backend/wire/listen.rs"),
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
        // 后端源码树从 `src/backend` 搬到 `<repo>/src/backend`，
        // manifest 留在原处 ⇒ 这一格不再是 `manifest/src`。走那个唯一住址。
        guard_core::addr::backend_src_root(),
        // 头注里点名的很多是判据文件，它们今天住第二棵树。
        guard_core::addr::backend_tests_root(),
        guard_core::addr::repo_root(),
        guard_core::addr::repo_root().join("src/frontend/shell/src"),
        // 🔴 **monitor 也有第二棵树。**
        //    上一格（`src/frontend/shell/src`）接的是「头注点名 monitor 侧的裸文件名」那一形，
        //    而剖分之后 monitor 的判据整批住 `<repo>/tests/frontend/shell/`
        //    ⇒ 头注里那些 `X_tests.rs` 四个根一个都够不着，被读成「假住址」。
        guard_core::addr::repo_root().join("tests/frontend/shell"),
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
            .is_some_and(|r| guard_core::addr::relay_root().join(r).exists());
        assert!(
            roots.iter().any(|r| r.join(w).exists()) || as_relay_module,
            "头注指着 `{w}`，而五个根下都找不到它（后端生产树 · 后端测试树 · 仓根 · \
                 monitor 生产树 `src/frontend/shell/src/` · monitor 测试树 `tests/frontend/shell/`）——\n\
                 ★ 指了住址而住址是假的：读者会以为那一格有人守着，去找的时候什么都没有。\n\
                 ⇒ 要么改成真名，要么把那句话删掉；**别留一个假住址**。"
        );
    }
    // 反空真②：一个住址都没扫到 ⇒ 上面的 `for` 跑零圈（`refusal_reasons_are_a_closed_set`
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

// 读 attach 那一行的有界读（超限整行丢弃并报字节数 · 读到头 · 没换行的残行）对拍住 `tests/comms/inward/line_tests.rs`。

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

/// 回认之后那一侧的写半边：每帧写完 flush 一次 ⇒ 对端就地解得出这一帧（full flush 也对齐到字节），整串解出来与写进去的逐字节相同、
/// 少于原样（每帧各压各的，帧小就压得少）；不压那一形原样过。
#[tokio::test]
async fn a_deflating_writer_flushes_each_frame_so_it_inflates_on_arrival() {
    use tokio::io::AsyncWriteExt;
    let frames: Vec<String> = (0..200)
        .map(|i| {
            format!(
                "{{\"kind\":\"line\",\"session_id\":\"s-1\",\"path\":\"/home/u/.claude/projects/-p/s-1.jsonl\",\"seq\":{i},\"record\":{{\"type\":\"assistant\",\"text\":\"read the file then run the test and check the frame {}\"}}}}\n",
                i % 7
            )
        })
        .collect();
    let mut w = Down::new(Vec::new(), true);
    let mut z = flate2::Decompress::new(false);
    let mut fed = 0usize;
    let mut back = Vec::new();
    for f in &frames {
        w.write_all(f.as_bytes()).await.unwrap();
        w.flush().await.unwrap();
        let wire = w.get_ref();
        let mut out = vec![0u8; 64 * 1024];
        let before = z.total_out();
        z.decompress(&wire[fed..], &mut out, flate2::FlushDecompress::Sync)
            .unwrap();
        fed = wire.len();
        let made = (z.total_out() - before) as usize;
        assert_eq!(&out[..made], f.as_bytes(), "flush 之后这一帧当场解得出来");
        back.extend_from_slice(&out[..made]);
    }
    let raw: usize = frames.iter().map(String::len).sum();
    assert_eq!(back.len(), raw);
    assert!(fed < raw, "线上 {fed} 字节，原样 {raw}：没压下来");

    let mut p = Down::new(Vec::new(), false);
    p.write_all(frames[0].as_bytes()).await.unwrap();
    p.flush().await.unwrap();
    assert_eq!(p.get_ref(), frames[0].as_bytes(), "不压那一形原样过");
}

/// 一帧 `n` 个随机小写字母再接换行（xorshift，种子定死；夹具不放真正文）。
fn random_letters(seed: u64, n: usize) -> Vec<u8> {
    let mut s = seed | 1;
    let mut v: Vec<u8> = (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            b'a' + (s % 26) as u8
        })
        .collect();
    v.push(b'\n');
    v
}

/// 每帧一个压缩上下文：两帧一模一样，第二帧也回指不到第一帧 ⇒ 线上那一段与第一帧差不多长
/// （连着一个上下文时第二帧整段回指第一帧，只剩十几个字节：外来内容就能借长度猜别的帧里的秘密）。
#[tokio::test]
async fn frames_do_not_point_back_into_earlier_frames() {
    use tokio::io::AsyncWriteExt;
    let frame = random_letters(0x5eed, 4096);
    let mut w = Down::new(Vec::new(), true);
    w.write_all(&frame).await.unwrap();
    w.flush().await.unwrap();
    let first = w.get_ref().len();
    w.write_all(&frame).await.unwrap();
    w.flush().await.unwrap();
    let second = w.get_ref().len() - first;
    assert!(
        second * 10 >= first * 9,
        "第二帧线上 {second} 字节、第一帧 {first}：第二帧回指了第一帧"
    );
}

/// 客户端那一份读法（`resident_client::line::Inflate`，一个 raw inflate 流）接这台写出的流 ⇒ 逐字节相同：
/// 只换写的那一侧，解的那一侧不动。
#[tokio::test]
async fn the_client_inflater_reads_the_stream_back_byte_for_byte() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let frames: Vec<Vec<u8>> = (0..40u64)
        .map(|i| random_letters(0x9e37_79b9 + i % 3, 700 + (i % 5) as usize * 900))
        .collect();
    let mut w = Down::new(Vec::new(), true);
    for f in &frames {
        w.write_all(f).await.unwrap();
        w.flush().await.unwrap();
    }
    let wire = w.get_ref().clone();
    let mut r = resident_client::line::Inflate::new(tokio::io::BufReader::new(&wire[..]));
    let mut back = Vec::new();
    r.read_to_end(&mut back).await.unwrap();
    assert_eq!(back, frames.concat(), "客户端解出来与写进去的不一样");
}
