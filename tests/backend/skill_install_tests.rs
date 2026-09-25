//! 〔AS2 · 第四波 4B〕`skill_install.rs` 的判据：读得全、判得对、闸没被绕开、可疑项按写下的规则标。
//!
//! 守的要求（住址）：用户裁决 **V113**「目录自动同步，装要你点」「远端后端也能在远端装skill」· **V112**「内容，原样拷过去并标出可疑项」
//! · 题面「可疑项规则要覆盖『可执行文件 / 绝对路径 / 对面未必有的命令』」· `AS1.md §2.1`（差异四态 ＋ 不同的要显式说盖）。
//!
//! 买到：临时目录上真读真判 —— 四态逐行相等（原样走 AS1 的 `diff`）· 「不同没说盖 ⇒ 整趟拒」（原样走 AS1 的 `plan`）·
//! 装不过去的（来源读不出原文 / 这台那一份盖不了）勾了就拒 · 可疑项四种逐条相等、闭集两向相等 · 回传的 CAS 期望只含这一趟拷的路径。
//! 买不到：🔴 真 Windows（执行位借 `plugin::discover::is_executable`，非 unix 恒 false）· 真远端（两台都是本机临时目录）。

use super::*;
use crate::mcp_sync::There;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-skillinst-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("建临时目录");
    p
}

/// 替身事实：路径 → 事实，名字 → 找不找得到。
struct FakeFacts {
    paths: BTreeMap<String, There>,
    cmds: BTreeMap<String, Option<bool>>,
}

impl Facts for FakeFacts {
    fn path(&self, p: &str) -> There {
        *self.paths.get(p).unwrap_or(&There::Absent)
    }
    fn command(&self, name: &str) -> Option<bool> {
        *self.cmds.get(name).unwrap_or(&Some(true))
    }
}

fn no_facts() -> FakeFacts {
    FakeFacts {
        paths: BTreeMap::new(),
        cmds: BTreeMap::new(),
    }
}

fn write(root: &Path, rel: &str, body: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

#[test]
fn read_hands_over_every_file_with_its_text_or_a_reason() {
    let d = temp_dir("read");
    write(&d, "demo/SKILL.md", b"---\ndescription: x\n---\n");
    write(&d, "demo/scripts/run.sh", b"#!/bin/sh\necho hi\n");
    write(&d, "demo/bin/blob", &[0u8, 1, 2]);
    let v = answer_read_at(Some(&d), &json!({"name": "demo"})).expect("读");
    let got: Vec<(String, bool, Option<String>)> = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap().to_string(),
                f["text"].is_string(),
                f["why"].as_str().map(str::to_string),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("SKILL.md".into(), true, None),
            ("bin/blob".into(), false, Some("不是文本文件".into())),
            ("scripts/run.sh".into(), true, None),
        ]
    );
    assert_eq!(v["dir"], json!(d.join("demo").display().to_string()));
    let e = answer_read_at(Some(&d), &json!({"name": "nope"})).expect_err("不存在的 skill");
    assert_eq!(e.0, "not_found");
    for bad in ["", "../x", ".hidden", "a/b", "c:\\x"] {
        let e = answer_read_at(Some(&d), &json!({ "name": bad })).expect_err("坏名字被收下");
        assert_eq!(e.0, "bad_args", "{bad:?}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

fn src(files: &[(&str, Option<&str>, bool)]) -> Value {
    json!(files
        .iter()
        .map(|(p, t, x)| json!({"path": p, "text": t, "exec": x}))
        .collect::<Vec<_>>())
}

#[test]
fn the_four_states_and_the_consent_gate_are_as1s_own() {
    let d = temp_dir("plan");
    write(&d, "demo/same.md", b"same\n");
    write(&d, "demo/diff.md", b"theirs\n");
    write(&d, "demo/only-here.md", b"keep\n");
    let source = src(&[
        ("same.md", Some("same\n"), false),
        ("diff.md", Some("mine\n"), false),
        ("new.md", Some("new\n"), false),
    ]);
    let v = answer_plan_with(
        &no_facts(),
        Some(&d),
        &json!({"name": "demo", "source": source}),
    )
    .expect("看差异");
    let states: Vec<(String, String)> = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["path"].as_str().unwrap().into(),
                r["state"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        states,
        vec![
            ("diff.md".to_string(), "differs".to_string()),
            ("new.md".into(), "new".into()),
            ("only-here.md".into(), "only-there".into()),
            ("same.md".into(), "same".into()),
        ]
    );
    assert_eq!(v["write"], Value::Null, "没给 take 不答写哪几个");
    // CAS 期望：只回这一趟拷的那几个路径在这台上的原文
    assert_eq!(
        v["target"],
        json!([{"path": "diff.md", "text": "theirs\n"}, {"path": "same.md", "text": "same\n"}])
    );
    let ask = |take: Value, overwrite: Value| {
        answer_plan_with(
            &no_facts(),
            Some(&d),
            &json!({"name": "demo", "source": source, "take": take, "overwrite": overwrite}),
        )
    };
    let e = ask(json!(["diff.md", "new.md"]), Value::Null).expect_err("不同的没说盖");
    assert_eq!(e.0, "needs_consent");
    let ok = ask(json!(["diff.md", "new.md", "same.md"]), json!(["diff.md"])).unwrap();
    assert_eq!(
        ok["write"],
        json!(["diff.md", "new.md"]),
        "same 不写、new 写、说了盖的 differs 写"
    );
    let e = ask(json!(["only-here.md"]), Value::Null).expect_err("只在这台的不是这一趟拷的");
    assert_eq!(e.0, "bad_args");
    let e = ask(json!(["new.md"]), json!(["diff.md"])).expect_err("说了盖却没勾");
    assert_eq!(e.0, "bad_args");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn what_cannot_travel_as_text_cannot_be_taken() {
    let d = temp_dir("blocked");
    write(&d, "demo/blob", &[0u8, 9]);
    let source = src(&[("blob", Some("text now\n"), false), ("bin", None, false)]);
    let base = json!({"name": "demo", "source": source});
    let v = answer_plan_with(&no_facts(), Some(&d), &base).unwrap();
    let row = |p: &str| {
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["path"] == p)
            .unwrap()
            .clone()
    };
    assert_eq!(row("blob")["blocked"], json!("不是文本文件"));
    assert_eq!(
        row("bin")["suspects"],
        json!([{"kind": "binary", "value": "", "there": null}])
    );
    let mut take_bin = base.clone();
    take_bin["take"] = json!(["bin"]);
    assert_eq!(
        answer_plan_with(&no_facts(), Some(&d), &take_bin)
            .unwrap_err()
            .0,
        "bad_args"
    );
    let mut take_blob = base.clone();
    take_blob["take"] = json!(["blob"]);
    take_blob["overwrite"] = json!(["blob"]);
    assert_eq!(
        answer_plan_with(&no_facts(), Some(&d), &take_blob)
            .unwrap_err()
            .0,
        "bad_file"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn suspects_follow_the_written_rules_and_the_closed_set_both_ways() {
    let facts = FakeFacts {
        paths: [
            ("/opt/tool".to_string(), There::Present),
            ("/home/me/data".to_string(), There::Absent),
            ("C:\\Tools\\x.exe".to_string(), There::Foreign),
        ]
        .into(),
        cmds: [
            ("python3".to_string(), Some(false)),
            ("node".to_string(), Some(true)),
        ]
        .into(),
    };
    let text = "#!/usr/bin/env python3\nrun('/opt/tool', \"/home/me/data\") # C:\\Tools\\x.exe.\n/opt/tool again\n";
    let got = suspects_of(Some(text), false, &facts);
    assert_eq!(
        got,
        vec![
            json!({"kind": "executable", "value": "", "there": null}),
            json!({"kind": "abs-path", "value": "/opt/tool", "there": "present"}),
            json!({"kind": "abs-path", "value": "/home/me/data", "there": "absent"}),
            json!({"kind": "abs-path", "value": "C:\\Tools\\x.exe", "there": "foreign"}),
            json!({"kind": "command-missing", "value": "python3", "there": "absent"}),
        ]
    );
    // 找得到的命令不标；`env` 启动器自己不标；`~/…` 不是绝对路径；解释器是绝对路径的那一形归 abs-path
    assert_eq!(
        suspects_of(Some("#!/usr/bin/env node\ncat ~/x\n"), false, &facts),
        vec![json!({"kind": "executable", "value": "", "there": null})]
    );
    assert_eq!(
        suspects_of(Some("#!/bin/bash\necho\n"), false, &facts),
        vec![
            json!({"kind": "executable", "value": "", "there": null}),
            json!({"kind": "abs-path", "value": "/bin/bash", "there": "absent"}),
        ]
    );
    assert_eq!(
        suspects_of(Some("plain\n"), true, &facts),
        vec![json!({"kind": "executable", "value": "", "there": null})]
    );
    assert_eq!(
        suspects_of(Some("plain\n"), false, &facts),
        Vec::<Value>::new()
    );
    // 闭集两向相等：上面几组夹具打出来的种类 == SUSPECT_KINDS
    let mut kinds: BTreeSet<String> = got
        .iter()
        .map(|s| s["kind"].as_str().unwrap().into())
        .collect();
    kinds.extend(
        suspects_of(None, false, &facts)
            .iter()
            .map(|s| s["kind"].as_str().unwrap().to_string()),
    );
    let closed: BTreeSet<String> = SUSPECT_KINDS.iter().map(|s| s.to_string()).collect();
    assert_eq!(kinds, closed);
    assert_eq!(
        shebang_command("#!/usr/bin/env -S deno run\n"),
        Some("deno".into())
    );
    assert_eq!(
        shebang_command("#!/bin/bash\n"),
        None,
        "解释器是绝对路径的那一形归 abs-path"
    );
}

#[test]
fn paths_in_the_source_are_fenced() {
    let d = temp_dir("fence");
    for bad in ["/etc/passwd", "../up", "a/../b", "a//b", "a\\b", "C:x", ""] {
        let e = answer_plan_with(
            &no_facts(),
            Some(&d),
            &json!({"name": "demo", "source": [{"path": bad, "text": "x", "exec": false}]}),
        )
        .expect_err("坏路径被收下");
        assert_eq!(e.0, "bad_args", "{bad:?}");
    }
    assert!(valid_rel("scripts/run.sh") && valid_rel("SKILL.md"));
    let _ = std::fs::remove_dir_all(&d);
}
