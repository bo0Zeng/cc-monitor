//! 后端 `deploy` 问那台的两条命令与它们回话的解读：`uname -s -m` → 表 A 的键（[`key_from_uname`]）· 在那台扫身份戳（[`stamp_scan_cmd`] ＋ [`interpret_stamp_scan`]）。
//!
//! 要求：「**OS 问不出来 ＝ 拒绝，不是回落**」；「读它字节里那段身份戳，不跑它」；「读不出来时的显式失败 —— 四态，不许合并」。
//! 界标由参数交（[`Marks`]）⇒ 用一对**占位界标**（规矩只看「两个界标之间 `[[:alnum:]_.-]+`」）；期望一律手写。

use super::*;

fn k(os: Os, arch: Arch) -> Key {
    Key { os, arch }
}

/// 占位界标（真界标住契约 crate 的 `STAMP_OPEN` / `STAMP_CLOSE`）。带正则元字符，量得出 `stamp_scan_cmd` 的转义。
const M: Marks<'static> = Marks {
    open: "<<id:",
    close: ":id>>",
};

/// 落点在远端 shell 里的一种写法（占位；真落点常量住 `relay_route_core` —— 判定只把它原样接在 `--` 后面）。
const LANDING_SHELL: &str = "\"$HOME\"/.cc-monitor/bin/ccm";

fn interpret_stamp_scan(exit: Option<u32>, stdout: &str, stderr: &str) -> RemoteIdentity {
    super::interpret_stamp_scan(exit, stdout, stderr, M)
}

fn stamp_scan_cmd(word: &str) -> String {
    super::stamp_scan_cmd(word, M)
}

// ═══ 问那台的 (OS, arch) ═══════════════════════════════════════════════════════

/// B5b：`uname -s -m` 收全的三样 → 键。退出码不是 0（Windows 默认 shell 没有 `uname`）/ 空 ⇒ 问不出 OS；只一段 ⇒ 问不出 arch。
#[test]
fn uname_answers_map_to_a_key_or_a_named_refusal() {
    assert_eq!(
        key_from_uname(Some(0), "Linux x86_64\n", ""),
        Ok(k(Os::Linux, Arch::X86_64))
    );
    match key_from_uname(
        Some(1),
        "",
        "'uname' is not recognized as an internal command",
    ) {
        Err(r @ Refusal::OsUnknown(_)) => {
            assert_eq!(
                r.raw(),
                Some("'uname' is not recognized as an internal command"),
                "那台的原话交给复制详情"
            );
            let said = r.say("devbox");
            assert!(!said.contains("is not recognized"), "原话不上句子：{said}");
            assert!(
                said.contains(copy_core::copy_static!("rsByteTable.key.said")),
                "{said}"
            );
        }
        other => panic!("退出码 1 该是问不出 OS：{other:?}"),
    }
    // 答了认不出的几段 ⇒ 句子只说「应答无法解析」，那几段进复制详情。
    match key_from_uname(Some(0), "Linux x86_64 extra", "") {
        Err(r @ Refusal::OsUnknown(_)) => {
            assert_eq!(r.raw(), Some("Linux x86_64 extra"));
            let said = r.say("devbox");
            assert!(!said.contains("extra"), "{said}");
            assert!(
                said.contains(copy_core::copy_static!("rsByteTable.key.unreadable")),
                "{said}"
            );
        }
        other => panic!("{other:?}"),
    }
    // 只答一段 ⇒ 问不出 arch，答的那一段进详情；什么都没答 ⇒ 没有原话。
    match key_from_uname(Some(0), "Linux\n", "") {
        Err(r @ Refusal::ArchUnknown(_)) => assert_eq!(r.raw(), Some("Linux")),
        other => panic!("{other:?}"),
    }
    assert_eq!(key_from_uname(Some(0), "", "").unwrap_err().raw(), None);
    // 没送退出码（连接被掐）≠ 0：不许读成「跑成了」。
    assert!(matches!(
        key_from_uname(None, "Linux x86_64", ""),
        Err(Refusal::OsUnknown(_))
    ));
    assert!(matches!(
        key_from_uname(Some(0), "", ""),
        Err(Refusal::OsUnknown(_))
    ));
    assert!(matches!(
        key_from_uname(Some(0), "Linux\n", ""),
        Err(Refusal::ArchUnknown(_))
    ));
    assert!(matches!(
        key_from_uname(Some(0), "Linux x86_64 extra", ""),
        Err(Refusal::OsUnknown(_))
    ));
}

/// Windows 那台用本地代码页（GBK）回话 ⇒ 界面上**不照抄乱码**，拒绝照旧。
///
/// 要求：「**拒绝是一个会到达用户的结论，不是一行 `debug` 日志**；文案照」
/// 与第 4 条「**OS 问不出 ＝ 拒绝，不是回落**」；读数
/// 「Windows 远端 `uname` 的回话在 toast / 日志里是**乱码**：PowerShell 按控制台代码页（GBK）吐错误，我们按 UTF-8 解」。
/// 异源：语料是**手写的 GBK 字节**（「无法将“uname”」），走与后端 `ssh-dial/uses.rs` 同一种有损解（`from_utf8_lossy`）；
/// 期望是「整句里零个 U+FFFD」＋「说得出不是 UTF-8」，不从被测常量里抠。正控：UTF-8 的回话照旧原样带出。
#[test]
fn an_answer_that_is_not_utf8_is_not_parroted_as_mojibake() {
    // 「uname : 无法将“uname”项识别为…」的 GBK 字节（前半段）。
    let gbk: &[u8] = b"uname : \xce\xde\xb7\xa8\xbd\xab\xa1\xb0uname\xa1\xb1";
    let lossy = String::from_utf8_lossy(gbk).into_owned();
    assert!(
        lossy.contains('\u{FFFD}'),
        "语料自检：GBK 字节按 UTF-8 解应当留下 U+FFFD"
    );
    for (exit, out, err) in [(Some(1), "", lossy.as_str()), (Some(0), lossy.as_str(), "")] {
        let r = key_from_uname(exit, out, err);
        let Err(refusal @ Refusal::OsUnknown(_)) = &r else {
            panic!("不是 UTF-8 的回话照样该是「问不出 OS」：{r:?}");
        };
        let said = refusal.say("vmself");
        assert!(!said.contains('\u{FFFD}'), "界面那句里照抄了乱码：{said}");
        assert!(
            said.contains(copy_core::copy_static!("rsByteTable.key.unreadable")),
            "那句话没说出「应答无法解析」：{said}"
        );
        assert!(said.contains("vmself"), "{said}");
        assert_eq!(refusal.raw(), Some(lossy.trim()), "回话原样进复制详情");
    }
    // 正控：UTF-8 的报错照旧说「应答报错」（不说成无法解析），原话进复制详情。
    match key_from_uname(Some(1), "", "'uname' is not recognized") {
        Err(r @ Refusal::OsUnknown(_)) => {
            let said = r.say("vmself");
            assert!(
                said.contains(copy_core::copy_static!("rsByteTable.key.said")),
                "{said}"
            );
            assert_eq!(r.raw(), Some("'uname' is not recognized"));
        }
        other => panic!("{other:?}"),
    }
}

// ═══ 远端判身份认字节═══════════════════════════════
//
// 要求：「**读它字节里那段身份戳，不跑它**」；「读不出来时的显式失败 —— 四态，不许合并」。

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

/// I2b：那条命令只读、界标不写字面量、路径过引号、身份至少一个字符（与 `identity_of_bytes` 同一条纪律）。
#[test]
fn the_stamp_scan_command_is_read_only_and_quoted() {
    // 落点是固定常量、以 shell 写法交进来（`"$HOME"` 在那台上展开），不再是要 quote 的外来路径。
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
