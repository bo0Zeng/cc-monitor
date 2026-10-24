//! 跑 `pb dump`：退出码与形状版本怎么认；真起一次假 pb，看当前目录、argv、不带 `PB_ID`。

use super::*;
use crate::plan::fixture::{dump as fixture_dump, fake_pb, scratch, workspace};

#[test]
fn rc0_with_the_known_shape_is_a_dump() {
    let out = fixture_dump("/w").to_string();
    match classify(Some(0), out.as_bytes(), b"") {
        Ran::Dump { doc, raw } => {
            assert_eq!(doc["workspace"], "/w");
            assert_eq!(raw, out.as_bytes());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn rc3_is_not_a_workspace_and_carries_pbs_sentence() {
    match classify(Some(3), b"", "读不成：这里不是一个 pb 工作区\n".as_bytes()) {
        Ran::NotWorkspace(s) => assert!(s.contains("不是一个 pb 工作区")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn rc4_or_an_unknown_shape_means_this_pb_cannot_dump_for_us() {
    assert!(matches!(
        classify(Some(4), b"", b"usage"),
        Ran::Unsupported(_)
    ));
    let mut doc = fixture_dump("/w");
    doc["shape"] = serde_json::json!(2);
    assert!(matches!(
        classify(Some(0), doc.to_string().as_bytes(), b""),
        Ran::Unsupported(_)
    ));
    assert!(matches!(classify(Some(0), b"{}", b""), Ran::Unsupported(_)));
}

/// 只认 0.3.8 那一形（shape 4）；更早的（1 · 2 · 3）不硬读、照实说对不上，话里带它给的形状版本。
#[test]
fn only_shape_four_is_read_and_older_ones_say_the_pb_version_does_not_match() {
    assert_eq!(SHAPE, 4);
    for old in [1, 2, 3, 5] {
        let mut doc = fixture_dump("/w");
        doc["shape"] = serde_json::json!(old);
        match classify(Some(0), doc.to_string().as_bytes(), b"") {
            Ran::Unsupported(said) => assert!(said.contains(&old.to_string()), "{said}"),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn other_codes_and_garbage_are_failures_with_the_raw_sentence() {
    match classify(Some(2), b"", b"refused here\n") {
        Ran::Failed { raw, .. } => assert_eq!(raw.as_deref(), Some("refused here")),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        classify(Some(0), b"not json", b""),
        Ran::Failed { .. }
    ));
    assert!(matches!(classify(None, b"", b""), Ran::Failed { .. }));
}

/// 真起一次：当前目录是交进来的那个目录（pb 自己往上找工作区），argv 恰好是 `dump`，
/// 父进程环境里有 `PB_ID` 也不传下去（不带身份 ⇒ pb 认作人）。
#[test]
fn a_real_run_goes_from_the_given_dir_with_only_dump_and_no_pb_id() {
    let d = scratch("dump-run");
    let entry = fake_pb(&d.join("pb"), crate::plan::locate::PLUGIN_NAME, None);
    let ws = workspace(&d.join("ws"), &fixture_dump("/synthetic"));
    let inner = ws.join("repos/alpha/src");
    // 只这一条判据碰这个键；别的判据不读它。
    unsafe { std::env::set_var("PB_ID", "should-not-leak") };
    let ran = run(&entry, &inner);
    unsafe { std::env::remove_var("PB_ID") };
    assert!(matches!(ran, Ran::Dump { .. }), "{ran:?}");
    let log: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(d.join("pb/ran.json")).unwrap()).unwrap();
    assert_eq!(log["argv"], serde_json::json!(["dump"]));
    assert_eq!(
        std::fs::canonicalize(log["cwd"].as_str().unwrap()).unwrap(),
        std::fs::canonicalize(&inner).unwrap()
    );
    assert_eq!(log["pb_id"], false);
}

#[test]
fn a_real_run_outside_any_workspace_is_not_a_workspace() {
    let d = scratch("dump-outside");
    let entry = fake_pb(&d.join("pb"), crate::plan::locate::PLUGIN_NAME, None);
    std::fs::create_dir_all(d.join("plain")).unwrap();
    assert!(matches!(
        run(&entry, &d.join("plain")),
        Ran::NotWorkspace(_)
    ));
}

/// 一格的 agent 视角：`dump --cell` 回的那一份里取 `agent_view` 原文；带 `error`（图上没有那一格）⇒ 失败、话是 pb 那一句。
#[test]
fn a_cell_dump_gives_its_agent_view_or_pbs_error() {
    use crate::plan::fixture::{cell_view, cell_view_missing};
    let ok = cell_view("/w").to_string();
    let v = cell_of(classify(Some(0), ok.as_bytes(), b"")).unwrap();
    assert!(v.starts_with("── A1-2 甲的写出"), "{v}");
    let bad = cell_view_missing("/w").to_string();
    match cell_of(classify(Some(0), bad.as_bytes(), b"")) {
        Err(CellMiss::Gone(said)) => assert!(said.contains("A9"), "{said}"),
        other => panic!("{other:?}"),
    }
    let mut old = cell_view("/w");
    old["shape"] = serde_json::json!(3);
    assert!(matches!(
        cell_of(classify(Some(0), old.to_string().as_bytes(), b"")),
        Err(CellMiss::Ran(Ran::Unsupported(_)))
    ));
}

/// 真起一次 `--cell`：当前目录是工作区根，argv 是 `dump --cell <片> <格> <工作区>`，不带 `PB_ID`。
#[test]
fn a_real_cell_run_names_slice_cell_and_workspace() {
    let d = scratch("dump-cell");
    let entry = fake_pb(&d.join("pb"), crate::plan::locate::PLUGIN_NAME, None);
    let ws = workspace(&d.join("ws"), &fixture_dump("/synthetic"));
    let v = run_cell(&entry, &ws, "alpha", "A1-2").unwrap();
    assert!(v.starts_with("── A1-2"));
    let log: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(d.join("pb/ran.json")).unwrap()).unwrap();
    assert_eq!(
        log["argv"],
        serde_json::json!(["dump", "--cell", "alpha", "A1-2", ws.to_string_lossy()])
    );
    assert_eq!(log["pb_id"], false);
}

/// 起 pb 只有一处（`plan::pb`）：找 python · 说「没装 python」那一句 · 定期限 · 认超时，整个 `plan/` 里各至多一处，
/// dump · cell · command 各自只判结局。数的是 `plan/` 底下全部源文件，新加一份照样算进来。
#[test]
fn starting_pb_lives_in_one_function() {
    // 扫的是 `src/backend/plan/`（生产源码），本文件不在里面 ⇒ 不用摘自己。
    let dir = guard_core::addr::backend_src_root().join("plan");
    let texts: String = guard_core::scan_tree_excluding(&dir, &["rs"], &[])
        .into_iter()
        .map(|(_, src)| src)
        .collect();
    assert!(
        texts.contains("pub(crate) fn pb("),
        "plan/ 里没有起 pb 的那个函数"
    );
    for needle in [
        "noPythonHint",
        "is_timed_out",
        "Deadline::secs",
        "discover::find",
        "child_for",
    ] {
        let n = texts.matches(needle).count();
        assert!(n <= 1, "`{needle}` 在 plan/ 里出现 {n} 处");
    }
}

/// 过了期限被收掉 ⇒ 读计划超时，等了多久照原语说的；起不来 ⇒ 失败、带原话。
#[test]
fn a_timed_out_or_unstarted_pb_is_a_failed_read() {
    let late = crate::plugin::invoke::Done {
        code: Some(crate::plugin::invoke::TIMED_OUT_CODE),
        stdout: Vec::new(),
        stderr: Vec::new(),
        waited_secs: Some(7),
    };
    match ran_of(Ok(late)) {
        Ran::Failed { said, raw: None } => assert_eq!(
            said,
            copy_text(
                "bePlan.dump.timedOut",
                &[("dur", &copy_core::format_duration(7000))]
            )
        ),
        other => panic!("{other:?}"),
    }
    let gone = copy_core::said::Said {
        said: "x".into(),
        raw: Some("no such file".into()),
    };
    match ran_of(Err(gone)) {
        Ran::Failed { said, raw } => {
            assert_eq!(said, "x");
            assert_eq!(raw.as_deref(), Some("no such file"));
        }
        other => panic!("{other:?}"),
    }
}
