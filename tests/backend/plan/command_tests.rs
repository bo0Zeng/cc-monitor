//! 代敲 pb 的用户命令：退出码与 pb 那一句 ⇒ 回包或拒；真起一次（假 pb）看当前目录 · 不带 PB_ID · 只带那个动词。

use super::*;
use crate::plan::fixture::{dump, fake_pb, scratch, workspace};

#[test]
fn view_rc0_gives_the_page_path_out_of_pbs_sentence() {
    let got = classify(
        "view",
        Some(0),
        "写了 /tmp/pb-读图-0123456789.html\n用浏览器打开它（再敲一次 view 就换成此刻的图）\n"
            .as_bytes(),
        b"",
    )
    .unwrap();
    assert_eq!(got["rc"], 0);
    assert_eq!(got["path"], "/tmp/pb-读图-0123456789.html");
    assert_eq!(got["said"], "写了 /tmp/pb-读图-0123456789.html");
}

#[test]
fn view_rc0_without_a_page_is_a_failure_not_a_null_path() {
    let (code, _, raw) = classify("view", Some(0), "别的话\n".as_bytes(), b"").unwrap_err();
    assert_eq!(code, "failed");
    assert_eq!(raw.as_deref(), Some("别的话"));
}

#[test]
fn continue_and_pause_rc0_carry_pbs_sentence_and_no_path() {
    for v in ["continue", "pause"] {
        let got = classify(v, Some(0), "auto 开\n".as_bytes(), b"").unwrap();
        assert_eq!(
            got,
            serde_json::json!({"rc": 0, "said": "auto 开", "path": null})
        );
    }
}

#[test]
fn each_failing_code_lands_in_its_own_refusal() {
    let stderr = "这里不是一个 pb 工作区\n".as_bytes();
    let (c, said, _) = classify("pause", Some(2), b"", stderr).unwrap_err();
    assert_eq!((c, said.as_str()), ("refused", "这里不是一个 pb 工作区"));
    assert_eq!(
        classify("pause", Some(3), b"", stderr).unwrap_err().0,
        "not_workspace"
    );
    assert_eq!(
        classify("view", Some(4), b"", stderr).unwrap_err().0,
        "pb_unsupported"
    );
    assert_eq!(
        classify("view", Some(1), b"", stderr).unwrap_err().0,
        "failed"
    );
    assert_eq!(classify("view", None, b"", stderr).unwrap_err().0, "failed");
}

#[test]
fn a_real_run_goes_from_the_workspace_root_with_only_the_verb_and_no_pb_id() {
    let root = scratch("command-run");
    let entry = fake_pb(&root.join("pb"), "planned-build", Some(2));
    let ws = workspace(&root.join("ws"), &dump("/x"));
    let (code, said, _) = run(&entry, &ws, "pause").unwrap_err();
    assert_eq!((code, said.as_str()), ("refused", "forced"));
    let ran: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("pb/ran.json")).unwrap()).unwrap();
    assert_eq!(ran["argv"], serde_json::json!(["pause"]));
    assert_eq!(ran["pb_id"], false);
    assert_eq!(
        std::fs::canonicalize(ran["cwd"].as_str().unwrap()).unwrap(),
        std::fs::canonicalize(&ws).unwrap()
    );
    let _ = std::fs::remove_dir_all(&root);
}
