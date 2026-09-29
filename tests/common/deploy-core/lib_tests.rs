//! 要求住址：`4d-lanes.md` MIG-3b 第 1 条 —— 部署决策的唯一一份（本机常驻后端出计划 · monitor 放字节共用）。
//!
//! 〔MIG-3b〕这几格原住 `tests/frontend/shell/sftp_tests.rs`（判定那时住 monitor 的 `sftp.rs`），随判定搬来，期望一字未改；
//! 界标改由参数交（[`Marks`]）⇒ 这里用一对**占位界标**（与真界标无关：规矩只看「两个界标之间 `[[:alnum:]_.-]+`」）。
//! 表 A / 表 B 那几格仍住 `tests/frontend/shell/byte_table_tests.rs`（经 `byte_table` 的再导出量同一份实现，另有几格读 monitor 的槽）。

use super::*;

/// 占位界标（真界标唯一住址在后端 `lib.rs`）。带正则元字符，量得出 `stamp_scan_cmd` 的转义。
const M: Marks<'static> = Marks {
    open: "<<id:",
    close: ":id>>",
};

/// 落点在远端 shell 里的一种写法（占位；真落点常量住 `relay_route_core`，本 crate 不依赖它 —— 判定只把它原样接在 `--` 后面）。
const LANDING_SHELL: &str = "\"$HOME\"/.cc-monitor/bin/ccm";

fn interpret_stamp_scan(exit: Option<u32>, stdout: &str, stderr: &str) -> RemoteIdentity {
    super::interpret_stamp_scan(exit, stdout, stderr, M)
}

fn stamp_scan_cmd(word: &str) -> String {
    super::stamp_scan_cmd(word, M)
}

// ═══ 身份判定（原 `sftp_tests.rs`）══════════════════════════════════════════════════

// ═══ 〔DP1 · 第四波〕远端判身份认字节（`设计/96 §7.2`）═══════════════════════════════
//
// 要求住址：`设计/96 §7.2.1`，逐字：「**读它字节里那段身份戳，不跑它**」；`§7.2.4`：「读不出来时的显式失败 —— 四态，不许合并」；
// `§7.2.3`：「部署决策的对照物只能是后者」（手上那份字节自报的，不是源码常量）。
// 〔墓碑 —— 这里原来是 K-W4 `§0c` 那几格：旁挂版本标记（目录级）与「落点那个文件在不在」两个事实合起来判的真值表。
//  旁挂标记在后端那条路上退役了（它是标签不是指纹），那几格随判定函数一起换成下面这几格。〕

/// I1：六形逐形（期望取自 `96 §7.2.4` 那张表 ＋ 0 字节那一格按「没装」）。
#[test]
fn identity_decision_answers_each_state_without_merging_them() {
    const EXPECT: &str = "p9b-sample";
    let d = |id: RemoteIdentity| identity_decision(&id, EXPECT, "aya", "/h/.cc-monitor/bin/ccm");
    assert!(
        matches!(d(RemoteIdentity::Missing), Ok(DeployAction::Deploy(_))),
        "没装 ⇒ 装"
    );
    assert!(
        matches!(d(RemoteIdentity::Empty), Ok(DeployAction::Deploy(_))),
        "0 字节 ⇒ 装"
    );
    assert_eq!(
        d(RemoteIdentity::Stamp(EXPECT.into())),
        Ok(DeployAction::Skip),
        "同一版 ⇒ 复用"
    );
    let Ok(DeployAction::Deploy(why)) = d(RemoteIdentity::Stamp("p8z-older".into())) else {
        panic!("更旧的一版 ⇒ 该换");
    };
    assert!(
        why.contains("p8z-older") && why.contains(EXPECT),
        "换的理由没说清两边各是哪一版：{why}"
    );
    // 三种「判不清它是谁」：显式失败，而且三句话互不相同（下一步不同：一个没身份、一个身份不唯一、一个判不了）。
    let no = d(RemoteIdentity::NoStamp).unwrap_err();
    let many = d(RemoteIdentity::Ambiguous(vec!["a1".into(), "b2".into()])).unwrap_err();
    let cant = d(RemoteIdentity::Unreadable("Permission denied".into())).unwrap_err();
    for e in [&no, &many, &cant] {
        assert!(
            e.contains("aya") && e.contains("/h/.cc-monitor/bin/ccm"),
            "没说哪台哪个文件：{e}"
        );
    }
    assert!(no.contains("不说自己是哪一版"), "{no}");
    assert!(many.contains("a1") && many.contains("b2"), "{many}");
    assert!(
        cant.contains("判不了") && cant.contains("Permission denied"),
        "{cant}"
    );
    assert!(no != many && many != cant && no != cant);
    // 出路是一个真存在的动作（机器页「卸载后端」），不是一句空话。
    assert!(no.contains("卸载后端") && many.contains("卸载后端"));
}

/// I2：扫描的回话 → 身份。退出码 0 / 1 / 其它 · 重复戳去重 · 两个不同戳 · 空身份不收。
#[test]
fn the_stamp_scan_answer_maps_to_exactly_one_identity_state() {
    let (o, c) = (M.open, M.close);
    let line = |id: &str| format!("{o}{id}{c}\n");
    assert_eq!(
        interpret_stamp_scan(Some(0), &line("p9-sample"), ""),
        RemoteIdentity::Stamp("p9-sample".into())
    );
    // 同一个戳在字节里出现两次（`grep -o` 逐处吐）⇒ 仍是一个身份。
    assert_eq!(
        interpret_stamp_scan(
            Some(0),
            &format!("{}{}", line("p9-sample"), line("p9-sample")),
            ""
        ),
        RemoteIdentity::Stamp("p9-sample".into())
    );
    assert_eq!(
        interpret_stamp_scan(Some(0), &format!("{}{}", line("b2"), line("a1")), ""),
        RemoteIdentity::Ambiguous(vec!["a1".into(), "b2".into()])
    );
    assert_eq!(
        interpret_stamp_scan(Some(0), &line(""), ""),
        RemoteIdentity::NoStamp
    );
    assert_eq!(
        interpret_stamp_scan(Some(1), "", ""),
        RemoteIdentity::NoStamp
    );
    assert!(matches!(
        interpret_stamp_scan(Some(2), "", "grep: /x: Permission denied"),
        RemoteIdentity::Unreadable(w) if w.contains("Permission denied")
    ));
    // 没送退出码（链路被掐）≠ 0：不许读成「扫到了」或「没有」。
    assert!(matches!(
        interpret_stamp_scan(None, &line("p9-sample"), ""),
        RemoteIdentity::Unreadable(_)
    ));
}

/// I2b：那条命令只读、界标不写字面量、路径过引号、身份至少一个字符（与 `build.rs::bytes_build_id` 同一条纪律）。
#[test]
fn the_stamp_scan_command_is_read_only_and_quoted() {
    // 〔E2〕落点是固定常量、以 shell 写法交进来（`"$HOME"` 在那台上展开），不再是要 quote 的外来路径。
    let cmd = stamp_scan_cmd(LANDING_SHELL);
    assert!(cmd.starts_with("LC_ALL=C grep -aoE "), "{cmd}");
    assert!(
        cmd.ends_with("-- \"$HOME\"/.cc-monitor/bin/ccm"),
        "落点那一格不对：{cmd}"
    );
    assert!(
        cmd.contains("[[:alnum:]_.-]+"),
        "身份那一段不是「至少一个字符」：{cmd}"
    );
    for m in [M.open, M.close] {
        assert!(cmd.contains(m), "界标没进命令：{m} / {cmd}");
    }
    // 只读：引号之外没有任何会写的东西（界标里的 `>>` 在引号里，是正则的一部分）。
    let unquoted: String = cmd.split('\'').step_by(2).collect();
    assert!(unquoted.contains("grep"), "拆引号拆歪了：{unquoted}");
    for w in [">", "rm ", "mv ", "tee", "chmod", "sed -i", ";", "|", "&"] {
        assert!(!unquoted.contains(w), "扫描命令里有写：{w} / {cmd}");
    }
}

// ── K-W4b：取样层那四个状态的**映射规则**逐格各一条 ─────────────────────
// 上面那几格买的是「判定那一半」与「两条路真的去问了」；取样这一半（`metadata` /
// `try_exists` 的答案怎么变成 `TargetBinary`）09-06 之前一条判据都没有：
// 把那个取样壳的体换成恒答 `Present`，全量 cargo **0 红**（沙箱实测）。
// 下面五格逐格钉一条规则，第六格是反向自检（证明它们不是恒真）。

/// 映射规则①：`metadata` 说它在、且**有字节** ⇒ `Present`。
#[test]
fn probe_metadata_with_bytes_maps_to_present() {
    assert_eq!(
        interpret_target_probe(Some(Some(2_300_000)), None),
        TargetBinary::Present
    );
    assert_eq!(
        interpret_target_probe(Some(Some(1)), None),
        TargetBinary::Present,
        "1 字节也是「有字节」—— 只有恰好 0 才是 Empty 那一格"
    );
}

/// 映射规则②：`metadata` 说它在、size **恰好 0** ⇒ `Empty`，不是 `Present`。
/// 0 字节不是假想形态：`upload_atomic` 那条「绝不 set_metadata」注释记的就是
/// 真机 e2e 把后端截成 0 字节、不可 exec 的那次事故，而 `try_exists` 会把它算成「在」。
#[test]
fn probe_metadata_saying_zero_bytes_maps_to_empty() {
    assert_eq!(
        interpret_target_probe(Some(Some(0)), None),
        TargetBinary::Empty
    );
    assert_ne!(
        interpret_target_probe(Some(Some(0)), None),
        interpret_target_probe(Some(Some(1)), None),
        "0 字节与有字节判成了同一格 ⇒ 身份那一步的 0 字节那一格（〔DP1〕按没装装）永远走不到"
    );
}

/// 映射规则③（本件的承重格）：`metadata` 成功而**服务器不给 size**（`Some(None)`）
/// ⇒ 仍是 `Present`。
/// `TargetBinary` 与取样壳的头注逐字：「服务器不给 size（size=None）≠ 0 字节」——
/// 把「没说」读成「空」，等于对着一台好机器每次连接都重传 2.3MB。
#[test]
fn probe_a_server_that_gives_no_size_is_not_the_empty_cell() {
    assert_eq!(
        interpret_target_probe(Some(None), None),
        TargetBinary::Present
    );
    assert_ne!(
        interpret_target_probe(Some(None), None),
        TargetBinary::Empty,
        "「服务器没给 size」被读成了「0 字节」"
    );
}

/// 映射规则④：`metadata` 失败、补问 `try_exists` **明确答不在** ⇒ `Missing`。
#[test]
fn probe_stat_failed_and_try_exists_says_no_maps_to_missing() {
    assert_eq!(
        interpret_target_probe(None, Some(false)),
        TargetBinary::Missing
    );
}

/// 映射规则⑤：`metadata` 失败、`try_exists` **也答不出来** ⇒ `Unknown`。
/// 不许滑成 `Missing`（一次 stat 失败换一次全量重传，版本门控就废了），
/// 也不许滑成 `Present`（那正是本枚举要治的那个静默）。
#[test]
fn probe_stat_failed_and_try_exists_cannot_answer_maps_to_unknown() {
    assert_eq!(interpret_target_probe(None, None), TargetBinary::Unknown);
    assert_ne!(
        interpret_target_probe(None, None),
        interpret_target_probe(None, Some(false)),
        "「问不出来」与「明确不在」判成了同一格 —— 这两者正是要分开的那两件事"
    );
}

/// **反向自检**：上面五格每一条都可能是恒真的（函数恒答那一张脸，断言照样绿）。
/// 这一格喂**全部六种输入**，钉的是「每一格只由它自己那条规则命中」——
/// 任何一臂被改到别的状态，下面必有一行不等。
#[test]
fn probe_no_cell_answers_in_place_of_another() {
    let table: [(Option<Option<u64>>, Option<bool>, TargetBinary, &str); 6] = [
        (Some(Some(9)), None, TargetBinary::Present, "有字节"),
        (Some(Some(0)), None, TargetBinary::Empty, "恰好 0 字节"),
        (Some(None), None, TargetBinary::Present, "服务器不给 size"),
        (
            None,
            Some(false),
            TargetBinary::Missing,
            "stat 失败 + try_exists 说不在",
        ),
        (
            None,
            Some(true),
            TargetBinary::Present,
            "stat 失败 + try_exists 说在",
        ),
        (
            None,
            None,
            TargetBinary::Unknown,
            "stat 失败 + try_exists 也答不出",
        ),
    ];
    for (size, exists, want, what) in table {
        assert_eq!(interpret_target_probe(size, exists), want, "{what}");
    }
    // 四个状态一个不少地被这张表喂到 —— 少一行就等于那一格没人看。
    for want in [
        TargetBinary::Present,
        TargetBinary::Missing,
        TargetBinary::Empty,
        TargetBinary::Unknown,
    ] {
        assert!(
            table.iter().any(|(_, _, w, _)| *w == want),
            "{want:?} 这一格没有输入喂给它"
        );
    }
    // 恒答任何一张脸都会被这三对逮住（不是「函数存在」那种空真）。
    assert_ne!(
        interpret_target_probe(Some(Some(0)), None),
        interpret_target_probe(Some(Some(9)), None)
    );
    assert_ne!(
        interpret_target_probe(Some(None), None),
        interpret_target_probe(Some(Some(0)), None)
    );
    assert_ne!(
        interpret_target_probe(None, Some(false)),
        interpret_target_probe(None, None)
    );
}

// ═══ 〔HX2 · 主会话 D-b〕只升不降（原 `sftp_tests.rs`）════════════════════════════════════

/// B1a：序键手写表 —— 合法形 · 多位代号 · 缺字母 · 大写 · 缺名 · 缺前缀。
#[test]
fn hx2_build_order_reads_generation_and_letter_and_refuses_other_shapes() {
    let cases: &[(&str, Option<(u32, u8)>)] = &[
        ("p1a-history", Some((1, b'a'))),
        ("p3m-ssh-zlib", Some((3, b'm'))),
        ("p2z-relay-in-resident", Some((2, b'z'))),
        ("p12c-x", Some((12, b'c'))),
        ("p3-x", None),
        ("p3M-x", None),
        ("p3m", None),
        ("p3m-", None),
        ("3m-x", None),
        ("sr1b-id", None),
        ("", None),
    ];
    for (id, want) in cases {
        assert_eq!(build_order(id), *want, "{id:?}");
    }
    assert!(is_newer("p3n-a", "p3m-b") && is_newer("p4a-a", "p3z-b"));
    assert!(!is_newer("p3m-a", "p3m-b"), "同序不同名 ⇒ 不算新");
    assert!(
        !is_newer("p3m-a", "p3n-b") && !is_newer("p3n-a", "junk") && !is_newer("junk", "p1a-x")
    );
}

// 〔MIG-3b〕B1b（出过的每一个 `BUILD_ID` 都有序、历史表爬升）住后端 `deploy_plan_tests.rs`：它读的是后端源码里的历史表，
//   放在后端那一侧不跨两半（`cross_half_edge_registry`）。

/// 🔴 B2：`identity_decision` 的「另一版」那一格按新旧拆开（期望手写）：旧 ⇒ 换；新 · 同序不同名 · 解不出 ⇒ 不动。
#[test]
fn hx2_a_different_build_is_replaced_only_when_it_is_older() {
    const MINE: &str = "p3n-mine";
    let d = |s: &str| {
        identity_decision(
            &RemoteIdentity::Stamp(s.into()),
            MINE,
            "aya",
            "/h/.cc-monitor/bin/ccm",
        )
    };
    assert!(
        matches!(d("p3m-older"), Ok(DeployAction::Deploy(_))),
        "旧 ⇒ 换"
    );
    assert!(
        matches!(d("p2z-older"), Ok(DeployAction::Deploy(_))),
        "旧一代 ⇒ 换"
    );
    assert_eq!(d(MINE), Ok(DeployAction::Skip), "同一版 ⇒ 复用");
    for theirs in ["p3o-newer", "p4a-newer", "p3n-sibling", "hand-built"] {
        match d(theirs) {
            Ok(DeployAction::Keep { theirs: t, why }) => {
                assert_eq!(t, theirs, "Keep 回的不是那台上的身份");
                assert!(
                    why.contains(theirs) && why.contains(MINE) && why.contains("aya"),
                    "{why}"
                );
            }
            other => panic!("{theirs:?} 不比 {MINE} 旧 ⇒ 该不动它，却是 {other:?}"),
        }
    }
}

/// 〔E2〕已部署的机器上落点是旧的三行入口（无身份戳）：认得出 ⇒ 换成后端本体；认不出的无戳文件照旧显式失败。
/// 〔MIG-3b〕原是 `sftp.rs` 那个落点判定函数体的源码切片判据，判定搬来之后改成行为判据。
#[test]
fn an_old_three_line_entry_at_the_landing_is_recognised_as_ours() {
    let old = "#!/bin/sh\n# cc-monitor: ccm = 后端本体的一次性模式（K33：所有命令只许有一处）\nexec '/home/u/.cc-monitor/bin/cc-monitor-backend' ccm \"$@\"\n";
    assert!(is_ours(old), "旧入口没认出来");
    assert!(
        !is_ours("#!/bin/sh\necho mine\n"),
        "用户自己的脚本被当成了我们的"
    );
    let v = |id: RemoteIdentity, bytes: Option<&[u8]>| {
        landing_verdict(&id, bytes, "p9b-mine", "aya", "~/.cc-monitor/bin/ccm")
    };
    assert!(matches!(
        v(RemoteIdentity::NoStamp, Some(old.as_bytes())),
        Ok(DeployAction::Deploy(_))
    ));
    assert!(v(RemoteIdentity::NoStamp, Some(b"#!/bin/sh\necho mine\n")).is_err());
    assert!(
        v(RemoteIdentity::NoStamp, None).is_err(),
        "读不到那份 ⇒ 照旧显式失败"
    );
    // 认旧入口只在「无戳」那一格：别的格子里带着同样的字节也照身份判。
    assert_eq!(
        v(
            RemoteIdentity::Stamp("p9b-mine".into()),
            Some(old.as_bytes())
        ),
        Ok(DeployAction::Skip)
    );
    assert!(v(
        RemoteIdentity::Ambiguous(vec!["a1".into(), "b2".into()]),
        Some(old.as_bytes())
    )
    .is_err());
}

/// 〔E2 · E-c〕旧落点那份后端字节：恰一个戳（我们编的）⇒ 删；不在 ⇒ 不说话；别的 ⇒ 不动；连问都没问成 ⇒ 带原话。四格互不合并。
#[test]
fn the_legacy_backend_is_removed_only_when_it_carries_exactly_one_stamp() {
    assert_eq!(
        legacy_verdict(Ok(RemoteIdentity::Missing)),
        LegacyVerdict::Absent
    );
    assert_eq!(
        legacy_verdict(Ok(RemoteIdentity::Stamp("p1a-x".into()))),
        LegacyVerdict::Remove
    );
    for kept in [
        RemoteIdentity::Empty,
        RemoteIdentity::NoStamp,
        RemoteIdentity::Ambiguous(vec!["a1".into(), "b2".into()]),
        RemoteIdentity::Unreadable("Permission denied".into()),
    ] {
        assert_eq!(
            legacy_verdict(Ok(kept.clone())),
            LegacyVerdict::Keep,
            "{kept:?}"
        );
    }
    assert_eq!(
        legacy_verdict(Err("链路断了".into())),
        LegacyVerdict::Unknown("链路断了".into())
    );
}

/// 手上一份字节自报的身份：与远端那条扫描同一条规矩（恰好一个才是身份 · 重复去重 · 空身份不收 · 0 字节是 Empty）。
#[test]
fn the_bytes_answer_their_own_identity_by_the_same_rule_as_the_remote_scan() {
    let stamp = |id: &str| format!("{}{id}{}", M.open, M.close);
    assert_eq!(identity_of_bytes(b"", M), RemoteIdentity::Empty);
    assert_eq!(identity_of_bytes(b"no stamp", M), RemoteIdentity::NoStamp);
    assert_eq!(
        identity_of_bytes(
            format!("xx{}yy{}", stamp("p9a-s"), stamp("p9a-s")).as_bytes(),
            M
        ),
        RemoteIdentity::Stamp("p9a-s".into())
    );
    assert_eq!(
        identity_of_bytes(format!("{}{}", stamp("b2"), stamp("a1")).as_bytes(), M),
        RemoteIdentity::Ambiguous(vec!["a1".into(), "b2".into()])
    );
    assert_eq!(
        identity_of_bytes(stamp("").as_bytes(), M),
        RemoteIdentity::NoStamp
    );
}

/// 拒绝点的前三步各在一步上：键拒 · 产线拒 · 承诺拒，第四步（带没带）不在这里。
#[test]
fn judge_refuses_at_the_key_the_line_and_the_promise_and_nowhere_else() {
    let linux = key_of("Linux", "x86_64");
    assert_eq!(judge(Product::Backend, Route::Remote, linux.clone()), linux);
    assert!(matches!(
        judge(Product::Backend, Route::Remote, key_of("", "")),
        Err(Refusal::OsUnknown { .. })
    ));
    assert!(matches!(
        judge(Product::Backend, Route::Remote, key_of("Darwin", "arm64")),
        Err(Refusal::UnsupportedMachine { .. })
    ));
    assert!(matches!(
        judge(Product::Backend, Route::Remote, key_of("Windows", "x86_64")),
        Err(Refusal::NotPromisedHere {
            route: Route::Remote,
            ..
        })
    ));
}
