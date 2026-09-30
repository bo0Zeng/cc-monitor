//! 〔AS2 · 第四波 4B〕`skill_install.rs` 的判据：读得全、判得对、闸没被绕开、可疑项按写下的规则标。
//!
//! 守的要求（住址）：用户裁决 **V113**「目录自动同步，装要你点」「远端后端也能在远端装skill」· **V112**「内容，原样拷过去并标出可疑项」
//! · 题面「可疑项规则要覆盖『可执行文件 / 绝对路径 / 对面未必有的命令』」· `AS1.md §2.1`（差异四态 ＋ 不同的要显式说盖）。
//!
//! 买到：临时目录上真读真判 —— 四态逐行相等（原样走 AS1 的 `diff`）· 「不同没说盖 ⇒ 整趟拒」（原样走 AS1 的 `plan`）·
//! 装不过去的（来源读不出原文 / 这台那一份盖不了）勾了就拒 · 可疑项四种逐条相等、闭集两向相等 · 回传的 CAS 期望只含这一趟拷的路径。
//! 买不到：🔴 真 Windows（执行位借 `plugin::discover::is_executable`，非 unix 恒 false）· 真远端（两台都是本机临时目录）。

use super::*;
use crate::assets::mcp_sync::There;

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
    assert_eq!(
        (v["base"].clone(), v["prefix"].clone()),
        (json!(d.display().to_string()), json!("demo")),
        "skill 根在 ⇒ 写落点就是它，前缀是名字"
    );
    // skill 根不在 ⇒ 落点退到上一层（配置根），前缀带上 skills 那一段
    let fresh = d.join("no-skills-yet");
    let w = answer_plan_with(
        &no_facts(),
        Some(&fresh),
        &json!({"name": "demo", "source": src(&[("a.md", Some("a"), false)])}),
    )
    .unwrap();
    assert_eq!(
        (w["base"].clone(), w["prefix"].clone()),
        (json!(d.display().to_string()), json!("no-skills-yet/demo"))
    );
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

// ═══════════════════════ 〔SU1 · 第四波 4C · V116〕记与卸 ═══════════════════════
//
// 守的要求（住址）：用户裁决 **V116**「要，只删装时写进去的文件」—— 原文「装的时候记下写了哪些文件，卸只删这些（装完用户自己改过的先问）」；
// `调研/第四波记录/SU1.md §1.2`（记的是这台判过的那一份的摘要 ＋ 新旧）· `§1.3`（卸：逐文件四态 · 要问的两种 · 闸整趟拒）。
// 买到：临时目录上真判 —— 装判定答的 `ledger` 恰是 `write` 那几个（两向）、摘要 == 来源原文的、`created` == 四态里的 `new` ·
// 卸的四态逐行相等、闭集两向、`seen` 恰是能删的那几份的现有原文 · 闸（没点名 / 删不了 / 不在记录里 / 两张单子对不上）整趟拒 ·
// 装判定 → 记 → 卸判定一趟串起来（跨模块：装时算的摘要卸时认得）。
// 买不到：真 Windows（`symlink_metadata` / 链接那一形）· 真远端。

fn ledger_with(d: &Path, name: &str, files: Value) -> (PathBuf, String) {
    let file = d.join("state/skill-installs.json");
    let root = d.join("skills");
    crate::assets::skill_ledger::record_at(
        &file,
        Some(&root),
        &json!({"op": "add", "name": name, "files": files}),
    )
    .expect("记");
    (file, root.join(name).display().to_string())
}

fn dg(t: &str) -> String {
    crate::assets::skill_ledger::digest_of(t)
}

#[test]
fn the_install_plan_hands_back_what_to_record_for_exactly_what_it_will_write() {
    let d = temp_dir("ledger-plan");
    write(&d, "demo/diff.md", b"theirs\n");
    write(&d, "demo/same.md", b"same\n");
    let source = src(&[
        ("diff.md", Some("mine\n"), false),
        ("new.md", Some("new\n"), false),
        ("same.md", Some("same\n"), false),
    ]);
    let look = answer_plan_with(
        &no_facts(),
        Some(&d),
        &json!({"name": "demo", "source": source}),
    )
    .unwrap();
    assert_eq!(look["ledger"], Value::Null, "没给 take 不答要记什么");
    let v = answer_plan_with(
        &no_facts(),
        Some(&d),
        &json!({"name": "demo", "source": source, "take": ["diff.md", "new.md", "same.md"], "overwrite": ["diff.md"]}),
    )
    .unwrap();
    let keys: BTreeSet<String> = v["ledger"].as_object().unwrap().keys().cloned().collect();
    let write: BTreeSet<String> = v["write"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_string())
        .collect();
    assert_eq!(keys, write, "要记的恰是真要写的那几个（same 不写也不记）");
    assert_eq!(
        v["ledger"],
        json!({
            "diff.md": {"digest": dg("mine\n"), "created": false},
            "new.md": {"digest": dg("new\n"), "created": true},
        }),
        "摘要是来源那一份原文的；盖掉原有的 ⇒ created=false，新建的 ⇒ true"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 盘上摆出四态各一（外加「装之前就在」与「装完用户自己加的」各一），逐行相等。
fn uninstall_fixture(tag: &str) -> (PathBuf, PathBuf, String) {
    let d = temp_dir(tag);
    let (ledger, dir) = ledger_with(
        &d,
        "demo",
        json!({
            "intact.md": {"digest": dg("ours\n"), "created": true},
            "overwrote.md": {"digest": dg("ours too\n"), "created": false},
            "edited.md": {"digest": dg("ours\n"), "created": true},
            "gone.md": {"digest": dg("x"), "created": true},
            "blob": {"digest": dg("x"), "created": true},
            "sub/linked.md": {"digest": dg("ours\n"), "created": true},
        }),
    );
    write(&d, "skills/demo/intact.md", b"ours\n");
    write(&d, "skills/demo/overwrote.md", b"ours too\n");
    write(
        &d,
        "skills/demo/edited.md",
        b"ours, then the user typed here\n",
    );
    write(&d, "skills/demo/blob", &[0u8, 1, 2]);
    write(&d, "skills/demo/users-own.md", b"not from any install\n");
    // 装写进去的是一份文件，装完那个路径被换成了一个目录 ⇒ 不按原文删它。
    std::fs::create_dir_all(d.join("skills/demo/sub/linked.md")).unwrap();
    (d, ledger, dir)
}

#[test]
fn uninstall_judges_every_recorded_file_as_it_is_on_disk_now() {
    let (d, ledger, dir) = uninstall_fixture("uninst-judge");
    let v = answer_uninstall_plan_at(&ledger, &json!({"dir": dir})).expect("看");
    let rows: Vec<(String, String, bool, bool, bool)> = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["path"].as_str().unwrap().into(),
                r["state"].as_str().unwrap().into(),
                r["created"].as_bool().unwrap(),
                r["deletable"].as_bool().unwrap(),
                r["ask"].as_bool().unwrap(),
            )
        })
        .collect();
    let row = |p: &str, s: &str, c: bool, del: bool, ask: bool| {
        (p.to_string(), s.to_string(), c, del, ask)
    };
    assert_eq!(
        rows,
        vec![
            row("blob", "unreadable", true, false, false),
            row("edited.md", "modified", true, true, true),
            row("gone.md", "gone", true, false, false),
            row("intact.md", "intact", true, true, false),
            row("overwrote.md", "intact", false, true, true),
            row("sub/linked.md", "unreadable", true, false, false),
        ],
        "只列记着的（用户自己加的 users-own.md 不在）；换了种类 / 非文本的不按原文删；改过的与装前就在的要问"
    );
    assert_eq!(
        v["seen"],
        json!([
            {"path": "edited.md", "text": "ours, then the user typed here\n"},
            {"path": "intact.md", "text": "ours\n"},
            {"path": "overwrote.md", "text": "ours too\n"},
        ]),
        "CAS 期望恰是能删的那几份的现有原文"
    );
    assert_eq!(
        (v["delete"].clone(), v["forget"].clone()),
        (Value::Null, Value::Null)
    );
    assert_eq!(v["name"], json!("demo"));
    // 闭集两向：夹具打出来的态 == UNINSTALL_STATES
    let got: BTreeSet<String> = rows.iter().map(|r| r.1.clone()).collect();
    let closed: BTreeSet<String> = UNINSTALL_STATES.iter().map(|s| s.to_string()).collect();
    assert_eq!(got, closed);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_uninstall_gate_refuses_the_whole_trip() {
    let (d, ledger, dir) = uninstall_fixture("uninst-gate");
    let ask = |take: Value, confirm: Value| {
        answer_uninstall_plan_at(
            &ledger,
            &json!({"dir": dir, "take": take, "confirm": confirm}),
        )
    };
    let code = |r: Answer| r.expect_err("该拒的被放行").0;
    assert_eq!(
        code(ask(json!(["intact.md", "edited.md"]), Value::Null)),
        "needs_consent",
        "改过的没点名"
    );
    assert_eq!(
        code(ask(json!(["overwrote.md"]), Value::Null)),
        "needs_consent",
        "装之前就在的没点名"
    );
    assert_eq!(
        code(ask(json!(["gone.md"]), Value::Null)),
        "bad_args",
        "已经不在的删不了"
    );
    assert_eq!(
        code(ask(json!(["blob"]), Value::Null)),
        "bad_args",
        "读不出原文的删不了"
    );
    assert_eq!(
        code(ask(json!(["users-own.md"]), Value::Null)),
        "bad_args",
        "不在记录里的不是装写进去的"
    );
    assert_eq!(
        code(ask(json!(["intact.md"]), json!(["edited.md"]))),
        "bad_args",
        "点名了却没勾"
    );
    assert_eq!(
        code(answer_uninstall_plan_at(
            &ledger,
            &json!({"dir": dir, "confirm": ["edited.md"]})
        )),
        "bad_args",
        "给了 confirm 没给 take"
    );
    assert_eq!(
        code(answer_uninstall_plan_at(
            &ledger,
            &json!({"dir": "/nowhere"})
        )),
        "not_found"
    );
    let ok = ask(
        json!(["intact.md", "edited.md", "overwrote.md"]),
        json!(["edited.md", "overwrote.md"]),
    )
    .expect("点名了就放行");
    assert_eq!(
        ok["delete"],
        json!(["edited.md", "intact.md", "overwrote.md"])
    );
    assert_eq!(ok["forget"], json!(["gone.md"]), "已经不在的交回去摘掉");
    let ok = ask(json!([]), Value::Null).expect("什么都不删也是一趟");
    assert_eq!(
        (ok["delete"].clone(), ok["forget"].clone()),
        (json!([]), json!(["gone.md"]))
    );
    // 记录读不懂 ⇒ 说读不懂，不说「没装过」
    std::fs::write(&ledger, b"{broken").unwrap();
    assert_eq!(
        code(answer_uninstall_plan_at(&ledger, &json!({"dir": dir}))),
        "ledger_unreadable"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 一趟串起来：装判定答的 `ledger` 原样记下 → 盘上就是装写进去的那一份 → 卸判定说「原样」、`created` 与四态一致。
#[test]
fn what_the_install_plan_says_to_record_is_what_the_uninstall_plan_recognizes() {
    let d = temp_dir("roundtrip");
    let root = d.join("skills");
    write(&d, "skills/demo/old.md", b"before the install\n");
    let source = src(&[
        ("old.md", Some("from the source\n"), false),
        ("new.md", Some("brand new\n"), false),
    ]);
    let plan = answer_plan_with(
        &no_facts(),
        Some(&root),
        &json!({"name": "demo", "source": source, "take": ["old.md", "new.md"], "overwrite": ["old.md"]}),
    )
    .unwrap();
    // 替身「写」：照 write 把来源原文落盘（monitor 那一侧经 files-put 做的事）
    write(&d, "skills/demo/old.md", b"from the source\n");
    write(&d, "skills/demo/new.md", b"brand new\n");
    let file = d.join("skill-installs.json");
    crate::assets::skill_ledger::record_at(
        &file,
        Some(&root),
        &json!({"op": "add", "name": "demo", "files": plan["ledger"]}),
    )
    .unwrap();
    let v = answer_uninstall_plan_at(&file, &json!({"dir": plan["dir"]})).unwrap();
    assert_eq!(
        v["rows"],
        json!([
            {"path": "new.md", "state": "intact", "created": true, "deletable": true, "ask": false},
            {"path": "old.md", "state": "intact", "created": false, "deletable": true, "ask": true},
        ])
    );
    let _ = std::fs::remove_dir_all(&d);
}
