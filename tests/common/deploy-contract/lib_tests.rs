//! 要求住址：`4d-lanes.md ## 发版后四路 ### P1` 第 1 件 —— 部署那一族的契约（键 · 戳格式 · 答话形状 · 路径）；`设计/96 §7.2.1`「读它字节里那段身份戳，不跑它」。
//!
//! 〔P1〕判定那几格（身份判定 · 取样解释 · 只升不降 · 旧入口 · 旧落点 · 表 A / 表 B 的判序）随判定搬进后端 `tests/backend/control/deploy_plan_tests.rs`；
//! 这里只剩戳的格式（扫描命令 · 扫描回话 · 字节自报 · 序键）。界标由参数交（[`Marks`]）⇒ 用一对**占位界标**（规矩只看「两个界标之间 `[[:alnum:]_.-]+`」）。

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

// ═══ 〔DP1 · 第四波〕远端判身份认字节（`设计/96 §7.2`）═══════════════════════════════
//
// 要求住址：`设计/96 §7.2.1`，逐字：「**读它字节里那段身份戳，不跑它**」；`§7.2.4`：「读不出来时的显式失败 —— 四态，不许合并」。

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

// ═══ 〔HX2 · 主会话 D-b〕戳的序键（「只升不降」那条判定住后端）══════════════════════════════

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
}

// 〔MIG-3b〕B1b（出过的每一个 `BUILD_ID` 都有序、历史表爬升）住后端 `deploy_plan_tests.rs`：它读的是后端源码里的历史表，
//   放在后端那一侧不跨两半（`cross_half_edge_registry`）。

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
