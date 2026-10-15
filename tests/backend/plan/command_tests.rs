//! 代敲 pb 的用户命令：退出码与 pb 那一句 ⇒ 回包或拒；真起一次（假 pb）看当前目录 · 不带 PB_ID · 只带那个动词；
//! 带着 agent 身份的那一侧调不到用户命令；接过仓库那几步。

use super::*;
use crate::plan::fixture::{dump, fake_pb, runs, scratch, workspace};
use std::cell::RefCell;

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
            serde_json::json!({"rc": 0, "said": "auto 开", "path": null, "took": null})
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
    let (code, said, _) = run(&entry, &ws, &["pause"]).unwrap_err();
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

/// pb 的用户命令只许人发起：这一侧自己带着 agent 的身份（`PB_ID`）⇒ 拒、一次 pb 都不起；`view` 不是用户命令，照起。
#[test]
fn an_agent_caller_gets_no_user_command_and_pb_is_never_started() {
    let root = scratch("command-agent");
    let entry = fake_pb(&root.join("pb"), "planned-build", None);
    let ws = workspace(&root.join("ws"), &dump("/x"));
    for args in [&["continue"][..], &["pause"], &["take"], &["repo", "alpha"]] {
        let (code, said, _) = run_as(&entry, &ws, args, true).unwrap_err();
        assert_eq!(code, "refused", "{args:?}");
        assert!(said.contains(args[0]), "{said}");
    }
    assert!(runs(&root.join("pb")).is_empty());
    let _ = run_as(&entry, &ws, &["view"], true);
    assert_eq!(runs(&root.join("pb")), vec![vec!["view".to_string()]]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_human_caller_passes_the_verb_and_its_args_through() {
    let root = scratch("command-repo");
    let entry = fake_pb(&root.join("pb"), "planned-build", Some(0));
    let ws = workspace(&root.join("ws"), &dump("/x"));
    let got = run_as(&entry, &ws, &["repo", "beta"], false).unwrap();
    assert_eq!(got["rc"], 0);
    assert_eq!(runs(&root.join("pb")), vec![vec!["repo".to_string(), "beta".to_string()]]);
    let _ = std::fs::remove_dir_all(&root);
}

fn ok(said: &str) -> Result<Value, Refused> {
    Ok(json!({"rc": 0, "said": said, "path": null, "took": null}))
}

/// 已经接过来的那一片：不起 pb，照成回、说接到的是哪一片。
#[test]
fn taking_a_slice_already_taken_runs_nothing() {
    let run = |a: &[&str]| -> Result<Value, Refused> { panic!("ran {a:?}") };
    let got = take_with(&Taking { slice: "alpha", path: "repos/alpha", docs_only: false, current: true }, &run).unwrap();
    assert_eq!(got["took"], json!({"slice": "alpha", "path": "repos/alpha", "switched": false}));
    assert_eq!(got["rc"], 0);
    assert!(got["said"].as_str().unwrap().contains("alpha"));
}

/// 此刻的那一片：只敲 `take`，pb 那一句原样带回。
#[test]
fn taking_the_current_slice_runs_take_alone() {
    let ran = RefCell::new(Vec::<Vec<String>>::new());
    let run = |a: &[&str]| {
        ran.borrow_mut().push(a.iter().map(|x| x.to_string()).collect());
        ok("「alpha」的仓库接过来了（alpha）：从现在起照规矩守")
    };
    let got = take_with(&Taking { slice: "alpha", path: "alpha", docs_only: true, current: true }, &run).unwrap();
    assert_eq!(*ran.borrow(), vec![vec!["take".to_string()]]);
    assert_eq!(got["took"], json!({"slice": "alpha", "path": "alpha", "switched": false}));
    assert_eq!(got["said"], "「alpha」的仓库接过来了（alpha）：从现在起照规矩守");
}

/// 不是此刻的那一片：pb 只接此刻那一片 ⇒ 先 `repo <片名>` 切过去、再 `take`；回包说切了。
#[test]
fn taking_another_slice_switches_to_it_first() {
    let ran = RefCell::new(Vec::<Vec<String>>::new());
    let run = |a: &[&str]| {
        ran.borrow_mut().push(a.iter().map(|x| x.to_string()).collect());
        ok("好")
    };
    let got = take_with(&Taking { slice: "beta", path: "repos/beta", docs_only: true, current: false }, &run).unwrap();
    assert_eq!(
        *ran.borrow(),
        vec![vec!["repo".to_string(), "beta".to_string()], vec!["take".to_string()]]
    );
    assert_eq!(got["took"]["switched"], true);
}

/// 切不过去 ⇒ 停在那一步、原样回拒，`take` 不敲。
#[test]
fn a_refused_switch_stops_before_take() {
    let ran = RefCell::new(0);
    let run = |_: &[&str]| -> Result<Value, Refused> {
        *ran.borrow_mut() += 1;
        Err(("refused", "拒：x".into(), None))
    };
    let e = take_with(&Taking { slice: "beta", path: "beta", docs_only: true, current: false }, &run).unwrap_err();
    assert_eq!(e.0, "refused");
    assert_eq!(*ran.borrow(), 1);
}
