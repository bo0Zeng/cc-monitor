//! 读族声明的错误码与命令表登记的那一行对得上（幽灵码检查）：码表住命令表（`stream::inbound::REGISTRY` 那一行的错误枚举），
//! 读族在下层的包里够不到它 ⇒ 这几条挂在命令那一层（`stream/inbound/files_face.rs`）。

use crate::files::index::test_support::resident_lock;
use crate::files::test_support;

/// 那条能力在命令表里登记的码（码表从那一行的错误枚举来）。
fn registered_codes(cap: &str) -> &'static [&'static str] {
    let wire = cap.replace('.', "-");
    crate::stream::inbound::REGISTRY
        .iter()
        .find(|s| s.name == wire)
        .unwrap_or_else(|| panic!("`{wire}` 不在命令表里"))
        .codes
}

fn f7a_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-f7a-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).expect("建夹具目录");
    d
}

fn path_json(p: &std::path::Path) -> serde_json::Value {
    serde_json::Value::String(p.to_str().expect("夹具路径是 ASCII").to_string())
}

/// 新那两条自己声明的错误码，**都得真的出得来**（幽灵码检查）。
///
/// ⚠ `unreadable` 那一档由上面那条
/// [`a_rebuild_on_an_unreadable_root_is_refused_without_touching_the_resident_index`]
/// 行使（那条同时钉着「不许清索引」），这里只补形状类的那几个。
#[test]
fn the_new_two_capabilities_declared_codes_are_not_ghosts() {
    let _lock = resident_lock();
    // 🔴 这个位是**进程级**的，而 `cargo test` 默认并行 ⇒ 先串行（理由住 `index_testing.rs`）
    let _serial = crate::files::index::test_support::serial();
    for (cap, args, want) in [
        // `path` 那一族与同族其余两条逐字同一条路（`path_arg`）。
        (
            "files.index.rebuild",
            serde_json::json!({"path": ""}),
            "bad_path",
        ),
        // 形状不对（不是字符串也不是 `{"b16"}`）在解入参那一步就拒 ⇒ `bad_args`；解得出、却是空的 ⇒ `bad_path`。
        (
            "files.index.rebuild",
            serde_json::json!({"path": 7}),
            "bad_args",
        ),
        // `dirs` **自己**的形状不对 ⇒ `bad_args`（少了它 / 不是数组）。
        ("files.browse", serde_json::json!({}), "bad_args"),
        (
            "files.browse",
            serde_json::json!({"dirs": "/tmp"}),
            "bad_args",
        ),
        // 数组里某一项形状不对 ⇒ 同样在解入参那一步拒（`bad_args`）；某一项是空的 ⇒ `bad_path`。
        ("files.browse", serde_json::json!({"dirs": [7]}), "bad_args"),
        (
            "files.browse",
            serde_json::json!({"dirs": [{"b16": "zz"}]}),
            "bad_args",
        ),
        (
            "files.browse",
            serde_json::json!({"dirs": [""]}),
            "bad_path",
        ),
    ] {
        let got = test_support::answer(cap, &args, &Default::default());
        assert!(
            matches!(&got, Err(s) if s.code.as_deref() == Some(want)),
            "`{cap}` 对 {args:?} 该回 `{want}`，回的是 {got:?}"
        );
        let declared = registered_codes(cap);
        assert!(
            declared.contains(&want),
            "`{cap}` 的错误码表里没有 `{want}` —— 它真的会回这个码"
        );
    }
}

/// 每条能力声明的错误码，都得**真的出得来**（幽灵码检查）。
///
/// ⚠ 它只覆盖今天造得出触发条件的那几个；`unreadable` 那一档要一个读不了的路径，
/// 用一个不存在的路径造（`io::ErrorKind` 不同，但走的是同一条出口）。
#[test]
fn the_declared_error_codes_are_not_ghosts() {
    let _lock = resident_lock();
    let nowhere = std::env::temp_dir().join(format!("ccm-24f-ghost-{}", std::process::id()));
    std::fs::remove_dir_all(&nowhere).ok();
    let arg = serde_json::json!({"path": nowhere.to_str().expect("ASCII")});
    for (cap, want) in [("files.ls", "not_found"), ("files.stat", "unreadable")] {
        let got = test_support::answer(cap, &arg, &Default::default());
        assert!(
            matches!(&got, Err(s) if s.code.as_deref() == Some(want)),
            "`{cap}` 对一个不存在的路径没有回 `{want}`，回的是 {got:?}"
        );
        let declared = registered_codes(cap);
        assert!(
            declared.contains(&"unreadable") && declared.contains(&"bad_path"),
            "`{cap}` 的错误码表漏了它真的会回的那几个"
        );
    }
    // `files.ls` 打不开的另两种：给的是一份文件 ⇒ `not_dir`；没有读权限 ⇒ `denied`（unix）。
    let base = std::env::temp_dir().join(format!("ccm-24f-ghost2-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(base.join("locked")).unwrap();
    std::fs::write(base.join("f.txt"), b"x").unwrap();
    let ls = |p: &std::path::Path| {
        test_support::answer(
            "files.ls",
            &serde_json::json!({"path": p.to_str().unwrap()}),
            &Default::default(),
        )
    };
    assert!(
        matches!(
            ls(&base.join("f.txt")),
            Err(s) if s.code.as_deref() == Some("not_dir")
        ),
        "给一份文件没回 `not_dir`"
    );
    // 句子里只有原因词（全仓那一张表；「不是目录」这一种它说「原因不明」），系统原话进 `raw`、不进句子。
    let e = ls(&base.join("f.txt")).unwrap_err();
    assert_eq!(
        e.said,
        copy_core::copy_text(
            "beFilesRead.ls.unreadable",
            &[(
                "kind",
                &copy_core::io_reason(std::io::ErrorKind::NotADirectory)
            )]
        )
    );
    let raw = e.raw.clone().unwrap_or_default();
    assert!(!raw.trim().is_empty(), "系统原话丢了：{e:?}");
    assert!(!e.said.contains(raw.trim()), "原话进了句子：{e:?}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(base.join("locked"), std::fs::Permissions::from_mode(0o000))
            .unwrap();
        // root 读得动一切 ⇒ 这一格在 root 下判不了（那时列得出来，读数照实）。
        match ls(&base.join("locked")) {
            Ok(_) => {}
            Err(s) if s.code.as_deref() == Some("denied") => {}
            other => panic!("没权限没回 `denied`：{other:?}"),
        }
        std::fs::set_permissions(base.join("locked"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }
    std::fs::remove_dir_all(&base).ok();
}

/// 三形「不是文本」各自落 `not_text`；读不到落 `unreadable`；参数形状落各自的码。
/// ⚠ 最后一条是**两向集合相等**：造得出来的码 == 声明的码（多一个是幽灵、少一个是没声明）。
#[test]
fn every_refusal_of_read_text_lands_on_its_own_declared_code() {
    let d = f7a_dir("rt-codes");
    let nul = d.join("nul.bin");
    std::fs::write(&nul, b"ab\0cd").expect("铺");
    let bad = d.join("latin1.txt");
    std::fs::write(&bad, [0x63u8, 0x61, 0x66, 0xE9]).expect("铺"); // 「café」的 Latin-1 字节
    let big = d.join("big.txt");
    std::fs::write(&big, "y".repeat(10)).expect("铺");
    let gone = d.join("gone.txt");
    let ceiling = crate::files::READ_TEXT_MAX_BYTES as u64;
    let cases: Vec<(serde_json::Value, &str)> = vec![
        (
            serde_json::json!({ "path": path_json(&nul), "max_bytes": 100 }),
            "not_text",
        ),
        (
            serde_json::json!({ "path": path_json(&bad), "max_bytes": 100 }),
            "not_text",
        ),
        (
            serde_json::json!({ "path": path_json(&d), "max_bytes": 100 }),
            "not_text",
        ),
        (
            serde_json::json!({ "path": path_json(&big), "max_bytes": 9 }),
            "too_large",
        ),
        (
            serde_json::json!({ "path": path_json(&gone), "max_bytes": 100 }),
            "unreadable",
        ),
        (serde_json::json!({ "max_bytes": 100 }), "bad_args"),
        (
            serde_json::json!({ "path": "", "max_bytes": 100 }),
            "bad_path",
        ),
        (serde_json::json!({ "path": path_json(&big) }), "bad_args"),
        (
            serde_json::json!({ "path": path_json(&big), "max_bytes": 0 }),
            "bad_args",
        ),
        (
            serde_json::json!({ "path": path_json(&big), "max_bytes": ceiling + 1 }),
            "bad_args",
        ),
    ];
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for (args, want) in &cases {
        let got = test_support::answer("files.read.text", args, &Default::default());
        assert!(
            matches!(&got, Err(s) if s.code.as_deref() == Some(*want)),
            "`files.read.text` 对 {args} 该回 `{want}`，回的是 {got:?}"
        );
        seen.insert(want);
    }
    // 正控：天花板本身是收的（对一份小文件）—— 不然上面那一条「超天花板」证不了边界在哪。
    assert!(
        test_support::answer(
            "files.read.text",
            &serde_json::json!({ "path": path_json(&big), "max_bytes": ceiling }),
            &Default::default()
        )
        .is_ok(),
        "`max_bytes` 恰好等于天花板被拒了"
    );
    let declared: std::collections::BTreeSet<&str> = registered_codes("files.read.text")
        .iter()
        .copied()
        .collect();
    assert_eq!(seen, declared, "造得出来的码与登记的码不是同一个集合");
    std::fs::remove_dir_all(&d).ok();
}
