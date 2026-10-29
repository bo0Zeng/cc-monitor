//! 代敲 pb 0.5 的用户命令：退出码与 pb 那一句 ⇒ 回包或拒；真起一次（假 pb）看当前目录 · 不带 PB_ID · 只带那个动词；
//! 带着 agent 身份的那一侧调不到用户命令；接过仓库（`adopt`）那几步；0.5 没有的动词不在界面能点的那几条里。

use super::*;
use crate::plan::fixture::{dump, fake_pb, runs, scratch, workspace};
use crate::plan::wire::{PlanAdopted, PlanCmdReply};
use std::cell::RefCell;

/// 界面能点的只剩 0.5 有的那几条（`repo` 是新计划框的［新建］）；记原话 · 接过仓库的旧名 · 旧的画图命令都不在。
#[test]
fn only_the_verbs_pb_05_has_are_offered() {
    assert_eq!(VERBS, ["continue", "pause", "adopt", "render", "repo"]);
    for gone in ["take", "record", "view"] {
        assert!(!VERBS.contains(&gone), "{gone}");
    }
}

#[test]
fn render_rc0_gives_the_page_path_out_of_pbs_sentence() {
    let got = classify(
        "render",
        Some(0),
        "写了 /tmp/pb-读图-0123456789.html\n用浏览器打开它（再敲一次 render 就换成此刻的图）\n"
            .as_bytes(),
        b"",
    )
    .unwrap();
    assert_eq!(
        got,
        PlanCmdReply {
            rc: 0,
            said: Some("写了 /tmp/pb-读图-0123456789.html".into()),
            path: Some("/tmp/pb-读图-0123456789.html".into()),
            adopted: None,
        }
    );
}

#[test]
fn render_rc0_without_a_page_is_a_failure_not_a_null_path() {
    let (code, _, raw) = classify("render", Some(0), "别的话\n".as_bytes(), b"").unwrap_err();
    assert_eq!(code, "failed");
    assert_eq!(raw.as_deref(), Some("别的话"));
}

#[test]
fn continue_and_pause_rc0_carry_pbs_sentence_and_no_path() {
    for v in ["continue", "pause"] {
        let got = classify(v, Some(0), "auto 开\n".as_bytes(), b"").unwrap();
        assert_eq!(got, reply("auto 开"));
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
        classify("render", Some(4), b"", stderr).unwrap_err().0,
        "pb_unsupported"
    );
    assert_eq!(
        classify("render", Some(1), b"", stderr).unwrap_err().0,
        "failed"
    );
    assert_eq!(
        classify("render", None, b"", stderr).unwrap_err().0,
        "failed"
    );
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

/// pb 的用户命令只许人发起：这一侧自己带着 agent 的身份（`PB_ID`）⇒ 拒、一次 pb 都不起；`render` 不是用户命令，照起。
#[test]
fn an_agent_caller_gets_no_user_command_and_pb_is_never_started() {
    let root = scratch("command-agent");
    let entry = fake_pb(&root.join("pb"), "planned-build", None);
    let ws = workspace(&root.join("ws"), &dump("/x"));
    for args in [
        &["continue"][..],
        &["pause"],
        &["adopt"],
        &["repo", "alpha"],
    ] {
        let (code, said, _) = run_as(&entry, &ws, args, true).unwrap_err();
        assert_eq!(code, "refused", "{args:?}");
        assert!(said.contains(args[0]), "{said}");
    }
    assert!(runs(&root.join("pb")).is_empty());
    let _ = run_as(&entry, &ws, &["render"], true);
    assert_eq!(runs(&root.join("pb")), vec![vec!["render".to_string()]]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_human_caller_passes_the_verb_and_its_args_through() {
    let root = scratch("command-repo");
    let entry = fake_pb(&root.join("pb"), "planned-build", Some(0));
    let ws = workspace(&root.join("ws"), &dump("/x"));
    let got = run_as(&entry, &ws, &["repo", "beta"], false).unwrap();
    assert_eq!(got.rc, 0);
    assert_eq!(
        runs(&root.join("pb")),
        vec![vec!["repo".to_string(), "beta".to_string()]]
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// pb 跑成了、只说了一句的回包。
fn reply(said: &str) -> PlanCmdReply {
    PlanCmdReply {
        rc: 0,
        said: Some(said.into()),
        path: None,
        adopted: None,
    }
}

fn ok(said: &str) -> Result<PlanCmdReply, Refused> {
    Ok(reply(said))
}

fn adopted(slice: &str, path: &str, switched: bool) -> Option<PlanAdopted> {
    Some(PlanAdopted {
        slice: slice.into(),
        path: path.into(),
        switched,
    })
}

/// 已经接过来的那个仓库：不起 pb，照成回、说是哪一个。
#[test]
fn adopting_a_repo_already_adopted_runs_nothing() {
    let run = |a: &[&str]| -> Result<PlanCmdReply, Refused> { panic!("ran {a:?}") };
    let got = adopt_with(
        &Adopting {
            slice: "alpha",
            path: "repos/alpha",
            docs_only: false,
            current: true,
        },
        &run,
    )
    .unwrap();
    assert_eq!(got.adopted, adopted("alpha", "repos/alpha", false));
    assert_eq!((got.rc, got.path), (0, None));
    assert!(got.said.unwrap().contains("alpha"));
}

/// 当前仓库：只敲 `adopt`，pb 那一句原样带回。
#[test]
fn adopting_the_current_repo_runs_adopt_alone() {
    let ran = RefCell::new(Vec::<Vec<String>>::new());
    let run = |a: &[&str]| {
        ran.borrow_mut()
            .push(a.iter().map(|x| x.to_string()).collect());
        ok("「alpha」的仓库接过来了（alpha）：从现在起照规则守")
    };
    let got = adopt_with(
        &Adopting {
            slice: "alpha",
            path: "alpha",
            docs_only: true,
            current: true,
        },
        &run,
    )
    .unwrap();
    assert_eq!(*ran.borrow(), vec![vec!["adopt".to_string()]]);
    assert_eq!(got.adopted, adopted("alpha", "alpha", false));
    assert_eq!(
        got.said.as_deref(),
        Some("「alpha」的仓库接过来了（alpha）：从现在起照规则守")
    );
}

/// 不是当前仓库：pb 只接当前仓库 ⇒ 先 `repo <仓库名>` 切过去、再 `adopt`；回包说切了。
#[test]
fn adopting_another_repo_switches_to_it_first() {
    let ran = RefCell::new(Vec::<Vec<String>>::new());
    let run = |a: &[&str]| {
        ran.borrow_mut()
            .push(a.iter().map(|x| x.to_string()).collect());
        ok("好")
    };
    let got = adopt_with(
        &Adopting {
            slice: "beta",
            path: "repos/beta",
            docs_only: true,
            current: false,
        },
        &run,
    )
    .unwrap();
    assert_eq!(
        *ran.borrow(),
        vec![
            vec!["repo".to_string(), "beta".to_string()],
            vec!["adopt".to_string()]
        ]
    );
    assert_eq!(got.adopted, adopted("beta", "repos/beta", true));
}

/// 切不过去 ⇒ 停在那一步、原样回拒，`adopt` 不敲。
#[test]
fn a_refused_switch_stops_before_adopt() {
    let ran = RefCell::new(0);
    let run = |_: &[&str]| -> Result<PlanCmdReply, Refused> {
        *ran.borrow_mut() += 1;
        Err(("refused", "拒：x".into(), None))
    };
    let e = adopt_with(
        &Adopting {
            slice: "beta",
            path: "beta",
            docs_only: true,
            current: false,
        },
        &run,
    )
    .unwrap_err();
    assert_eq!(e.0, "refused");
    assert_eq!(*ran.borrow(), 1);
}

/// 过了期限被收掉 ⇒ 「pb <动词> 超时」，等了多久照原语说的；起不来 ⇒ `failed`、带原话。
#[test]
fn a_timed_out_or_unstarted_pb_is_failed_with_the_verb() {
    let late = crate::plugin::invoke::Done {
        code: Some(crate::plugin::invoke::TIMED_OUT_CODE),
        stdout: Vec::new(),
        stderr: Vec::new(),
        waited_secs: Some(7),
    };
    let (code, said, raw) = answer("render", Ok(late)).unwrap_err();
    assert_eq!(code, "failed");
    assert_eq!(
        said,
        copy_text(
            "bePlan.command.timedOut",
            &[
                ("verb", "render"),
                ("dur", &copy_core::format_duration(7000))
            ]
        )
    );
    assert_eq!(raw, None);
    let gone = copy_core::said::Said::with_raw("x".into(), "no such file");
    let (code, said, raw) = answer("render", Err(gone)).unwrap_err();
    assert_eq!(
        (code, said.as_str(), raw.as_deref()),
        ("failed", "x", Some("no such file"))
    );
}
