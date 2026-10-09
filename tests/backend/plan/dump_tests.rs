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
    let inner = ws.join("alpha/src");
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
