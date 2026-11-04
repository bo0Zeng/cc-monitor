//! 开一片之前问的那一份：空目录上问领域列出 pb 给的那几档 · pb 不在 ⇒ 说未装 · 工作区里撞上已有的一片 ⇒ 领域锁成它的、
//! 那一行只带名字 · pb 太旧没有 `--domains` ⇒ 要更新 · 那一行照路径与领域写。

use super::*;
use crate::plan::fixture::{dump, fake_pb, runs, workspace};
use crate::plan::locate::Missing;
use test_home::scratch;

fn names(f: &PlanNewForm) -> Vec<String> {
    f.domains.iter().filter_map(|d| d.name.clone()).collect()
}

fn no_slices(_: &str) -> Vec<(String, Option<String>)> {
    Vec::new()
}

#[test]
fn 空目录上问领域_列出那几档() {
    let root = scratch("new-empty");
    let entry = fake_pb(&root.join("pb"), "planned-build", None);
    let dir = root.join("work/ledger");
    let dir_s = dir.to_string_lossy().to_string();
    let f = form(
        &Ask {
            dir: &dir_s,
            path: "invoice",
            domain: Some("软件"),
        },
        Ok(entry),
        &no_slices,
    );
    assert_eq!(f.pb.state, "ok");
    assert_eq!(names(&f), ["研究", "软件"]);
    // 往上找不到工作区 ⇒ 这个目录就成工作区根（目录还不在也行）。
    assert_eq!(f.workspace, None);
    assert_eq!(f.root, dir_s);
    assert_eq!(
        f.repo_dir.as_deref(),
        Some(format!("{dir_s}/invoice").as_str())
    );
    assert_eq!(f.line.as_deref(), Some("pb repo invoice 软件"));
    assert_eq!(f.slice.as_deref(), Some("invoice"));
    let at = format!("{dir_s}/invoice");
    assert_eq!(
        f.note,
        Some(copy_core::copy_text("bePlan.new.repoAt", &[("dir", &at)]))
    );
    assert_eq!(f.existing, None);
    assert_eq!(
        runs(&root.join("pb")),
        [vec!["dump".to_string(), "--domains".into(), dir_s.clone()]]
    );
}

#[test]
fn 没装pb_说未装() {
    let f = form(
        &Ask {
            dir: "/nowhere",
            path: "x",
            domain: None,
        },
        Err(Missing::NotInstalled),
        &no_slices,
    );
    assert_eq!(f.pb.state, "missing");
    assert_eq!(
        f.pb.said,
        Some(copy_core::copy_text("bePlan.face.noPb", &[]))
    );
    assert!(f.domains.is_empty());
    assert_eq!(f.line, None);
}

#[test]
fn 工作区里撞上已有的一片_领域锁成它的_只带名字() {
    let root = scratch("new-hit");
    let entry = fake_pb(&root.join("pb"), "planned-build", None);
    let ws = workspace(&root.join("ws"), &dump(&root.join("ws").to_string_lossy()));
    let ws_s = ws.to_string_lossy().to_string();
    let deeper = format!("{ws_s}/repos");
    let slices = |w: &str| {
        assert_eq!(w, ws_s);
        vec![
            ("alpha".to_string(), Some("软件".to_string())),
            ("beta".into(), Some("研究".into())),
        ]
    };
    let f = form(
        &Ask {
            dir: &deeper,
            path: "repos/alpha",
            domain: Some("研究"),
        },
        Ok(entry),
        &slices,
    );
    assert_eq!(f.workspace.as_deref(), Some(ws_s.as_str()));
    assert_eq!(f.root, ws_s);
    assert_eq!(
        f.existing,
        Some(PlanNewExisting {
            slice: "alpha".into(),
            domain: Some("软件".into())
        })
    );
    assert_eq!(f.line.as_deref(), Some("pb repo alpha"));
    assert_eq!(
        f.note,
        Some(copy_core::copy_text(
            "bePlan.new.existing",
            &[("domain", "软件")]
        ))
    );
}

#[test]
fn 没有领域_路径空_那一行照实() {
    let root = scratch("new-line");
    let entry = fake_pb(&root.join("pb"), "planned-build", None);
    let dir = root.join("w").to_string_lossy().to_string();
    let ask = |path: &'static str, domain: Option<&'static str>| {
        form(
            &Ask {
                dir: &dir,
                path,
                domain,
            },
            Ok(entry.clone()),
            &no_slices,
        )
    };
    let q3 = ask("notes/q3", None);
    assert_eq!(q3.line.as_deref(), Some("pb repo notes/q3"));
    assert_eq!(q3.slice.as_deref(), Some("q3"));
    // 名字合不合 pb 的规矩不在这里判（提交时 pb 自己拒）：那一行照原样写。
    assert_eq!(
        ask("my notes", Some("研究")).line.as_deref(),
        Some("pb repo my notes 研究")
    );
    let empty = ask("", Some("研究"));
    assert_eq!((empty.line, empty.repo_dir), (None, None));
}

/// 代敲那几个参数与框里那一行同一判；在一个空目录里起 ⇒ 当前目录就是它（pb 在那里建工作区）。
#[test]
fn 代敲repo_参数与那一行同一判() {
    assert_eq!(
        repo_args("notes/q3/", Some("研究"), None),
        ["repo", "notes/q3", "研究"]
    );
    assert_eq!(
        repo_args("repos/alpha", Some("研究"), Some("alpha")),
        ["repo", "alpha"]
    );
    assert_eq!(repo_args("x", Some(""), None), ["repo", "x"]);
    let root = scratch("new-run");
    let entry = fake_pb(&root.join("pb"), "planned-build", None);
    let dir = root.join("w");
    std::fs::create_dir_all(&dir).unwrap();
    let _ = crate::plan::command::run_as(&entry, &dir, &["repo", "invoice", "软件"], false);
    let ran: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("pb/ran.json")).unwrap()).unwrap();
    assert_eq!(ran["argv"], serde_json::json!(["repo", "invoice", "软件"]));
    assert_eq!(ran["cwd"], serde_json::json!(dir.to_string_lossy()));
}

#[test]
fn pb太旧没有domains_要更新() {
    let root = scratch("new-old");
    let entry = fake_pb(&root.join("pb"), "planned-build", Some(4));
    let f = form(
        &Ask {
            dir: &root.to_string_lossy(),
            path: "x",
            domain: None,
        },
        Ok(entry),
        &no_slices,
    );
    assert_eq!(f.pb.state, "unsupported");
    assert!(f.domains.is_empty());
}
