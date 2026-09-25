//! 〔AS2 · 第四波 4B〕monitor 侧 `skill_install.rs` 的判据：只编排 —— 读哪台、请哪台判、写了什么、CAS 期望是哪一份、stale 就停。
//!
//! 守的要求（住址）：用户裁决 **V113**「装要你点」「远端后端也能在远端装skill」· **V112**「内容，原样拷过去」·
//! `设计/01 §1.1`（判定在后端）· `§3.6`（写只经后端文件管理那一面）· `AS1.md §1.3`（stale 不重读重算）。
//!
//! 买到：替身后端（记下问了哪台、问了什么）＋ 替身门（临时目录上「读 · CAS · 写」）上真跑一遍 ——
//! 读在来源那台、判在被写那台 · 写的恰是判定放行的那几个、原文原样、期望是看差异时那一份、parents 开 ·
//! 有执行位的真 chmod · 写到一半 stale 就停且说清前面写了哪几个 · 应答缺格就报错不猜 ·
//! 发 / 读的字段与后端登记两向相等（读后端源码，异源）· 本模块零判定（闭集线上名零命中，人群从后端源码现抠）。
//! 买不到：🔴 真远端 · 真 Windows（替身门的 chmod 在非 unix 上恒失败，那一支只在 Linux 上量了成功面）。

use super::*;
use crate::user_files::tests::{temp_home, DiskDoor};
use std::cell::RefCell;

struct FakeAsk {
    asked: RefCell<Vec<(String, String, Value)>>,
    replies: RefCell<Vec<Result<Value, String>>>,
}

impl FakeAsk {
    fn new(replies: Vec<Result<Value, String>>) -> Self {
        FakeAsk {
            asked: RefCell::new(vec![]),
            replies: RefCell::new(replies),
        }
    }
}

impl Ask for FakeAsk {
    async fn ask(&self, origin: &Origin, cmd: &str, args: Value) -> Result<Value, String> {
        self.asked
            .borrow_mut()
            .push((origin.as_wire_str().to_string(), cmd.to_string(), args));
        self.replies.borrow_mut().remove(0)
    }
}

fn origin(s: &str) -> Origin {
    serde_json::from_value(json!(s)).expect("origin")
}

fn read_reply() -> Value {
    json!({
        "root": "/src/skills", "dir": "/src/skills/demo", "skipped": [],
        "files": [
            {"path": "SKILL.md", "text": "doc\n", "bytes": 4, "exec": false, "why": null},
            {"path": "bin/blob", "text": null, "bytes": 3, "exec": true, "why": "不是文本文件"},
            {"path": "run.sh", "text": "#!/bin/sh\n", "bytes": 10, "exec": true, "why": null},
        ],
    })
}

/// 〔SU1〕那台判的「每个要写的记什么」：恰是 `write` 那几个（摘要取假的、按位置编号；monitor 只原样交回，不看内容）。
fn ledger_of(write: &Value) -> Value {
    match write.as_array() {
        None => Value::Null,
        Some(w) => Value::Object(
            w.iter()
                .enumerate()
                .map(|(i, p)| {
                    (
                        p.as_str().unwrap().to_string(),
                        json!({"digest": format!("{i:016x}"), "created": i % 2 == 0}),
                    )
                })
                .collect(),
        ),
    }
}

fn plan_reply(write: Value) -> Value {
    let ledger = ledger_of(&write);
    json!({
        "root": "/dst/skills", "dir": "/dst/skills/demo", "base": "/dst", "prefix": "skills/demo",
        "rows": [
            {"path": "SKILL.md", "state": "differs", "suspects": [], "blocked": null},
            {"path": "bin/blob", "state": "new", "suspects": [{"kind": "binary", "value": "", "there": null}], "blocked": null},
            {"path": "run.sh", "state": "new", "suspects": [{"kind": "abs-path", "value": "/bin/sh", "there": "present"}], "blocked": null},
        ],
        "target": [{"path": "SKILL.md", "text": "old\n"}],
        "write": write,
        "ledger": ledger,
    })
}

#[tokio::test]
async fn preview_reads_on_the_source_and_asks_the_target_to_judge() {
    let ask = FakeAsk::new(vec![Ok(read_reply()), Ok(plan_reply(Value::Null))]);
    let p = preview_with(&ask, &origin("<local>"), &origin("dev"), "demo")
        .await
        .expect("看差异");
    let asked = ask.asked.borrow().clone();
    assert_eq!(asked.len(), 2);
    assert_eq!(
        (asked[0].0.as_str(), asked[0].1.as_str()),
        ("<local>", READ)
    );
    assert_eq!(asked[0].2, json!({"name": "demo"}));
    assert_eq!((asked[1].0.as_str(), asked[1].1.as_str()), ("dev", PLAN));
    assert_eq!(
        asked[1].2,
        json!({"name": "demo", "source": [
            {"path": "SKILL.md", "text": "doc\n", "exec": false},
            {"path": "bin/blob", "text": null, "exec": true},
            {"path": "run.sh", "text": "#!/bin/sh\n", "exec": true},
        ]}),
        "交给判定的恰是来源读到的那一份（原文原样、不带 why）"
    );
    assert_eq!(p.dir, "/dst/skills/demo");
    assert_eq!(p.rows.len(), 3);
    assert_eq!(p.rows[2].suspects[0].there.as_deref(), Some("present"));
    assert_eq!(
        p.target,
        vec![SkillTargetText {
            path: "SKILL.md".into(),
            text: "old\n".into()
        }]
    );
    assert_eq!(p.source[1].why.as_deref(), Some("不是文本文件"));
}

#[tokio::test]
async fn a_reply_that_breaks_the_contract_is_an_error_not_a_guess() {
    for (which, bad) in [
        (
            "read",
            json!({"files": [{"path": "a", "text": "x", "why": null}]}),
        ), // 缺 exec
        (
            "plan",
            json!({"dir": "/d", "rows": [{"path": "a", "suspects": [], "blocked": null}], "target": []}),
        ), // 缺 state
        (
            "plan",
            json!({"dir": "/d", "rows": [], "target": [{"path": "a"}]}),
        ), // 缺 text
    ] {
        let replies = if which == "read" {
            vec![Ok(bad.clone())]
        } else {
            vec![Ok(read_reply()), Ok(bad.clone())]
        };
        let ask = FakeAsk::new(replies);
        let e = preview_with(&ask, &origin("a"), &origin("b"), "demo")
            .await
            .expect_err("坏应答被收下");
        assert_eq!(e, *UNREADABLE_REPLY, "{bad}");
    }
}

fn source_files() -> Vec<SkillFile> {
    vec![
        SkillFile {
            path: "SKILL.md".into(),
            text: Some("doc\n".into()),
            exec: false,
            why: None,
        },
        SkillFile {
            path: "run.sh".into(),
            text: Some("#!/bin/sh\n".into()),
            exec: true,
            why: None,
        },
        SkillFile {
            path: "later.md".into(),
            text: Some("later\n".into()),
            exec: false,
            why: None,
        },
    ]
}

#[tokio::test]
async fn apply_writes_exactly_the_judged_paths_with_the_seen_expectations() {
    let home = temp_home("skill-apply");
    std::fs::create_dir_all(home.join("skills/demo")).unwrap();
    std::fs::write(home.join("skills/demo/SKILL.md"), "old\n").unwrap();
    let base = home.display().to_string();
    let mut reply = plan_reply(json!(["SKILL.md", "run.sh"]));
    reply["base"] = json!(base);
    let ledger = reply["ledger"].clone();
    let ask = FakeAsk::new(vec![Ok(reply), Ok(json!({"remaining": 2}))]);
    let door = DiskDoor::new(&home);
    let target = vec![SkillTargetText {
        path: "SKILL.md".into(),
        text: "old\n".into(),
    }];
    let take = vec!["SKILL.md".to_string(), "run.sh".to_string()];
    let out = apply_with(
        &ask,
        &door,
        &origin("dev"),
        "demo",
        &source_files(),
        &target,
        &take,
        &["SKILL.md".into()],
    )
    .await
    .expect("写");
    assert_eq!(
        out.written,
        vec!["SKILL.md".to_string(), "run.sh".to_string()]
    );
    let puts = door.puts.borrow().clone();
    let got: Vec<(String, String, Option<String>, bool)> = puts
        .iter()
        .map(|p| {
            (
                p.rel.clone(),
                p.content.clone(),
                p.expect.clone(),
                p.parents,
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            (
                "skills/demo/SKILL.md".into(),
                "doc\n".into(),
                Some("old\n".into()),
                true
            ),
            (
                "skills/demo/run.sh".into(),
                "#!/bin/sh\n".into(),
                None,
                true
            ),
        ],
        "只写判定放行的那几个；原文原样；期望是看差异时那一份（新的 = 必须不存在）"
    );
    assert!(
        !home.join("skills/demo/later.md").exists(),
        "没被放行的不许写"
    );
    // 交给判定的 take / overwrite 原样
    let asked = ask.asked.borrow().clone();
    assert_eq!(asked[0].2["take"], json!(["SKILL.md", "run.sh"]));
    assert_eq!(asked[0].2["overwrite"], json!(["SKILL.md"]));
    // 〔SU1〕写完把真写成了的那几个原样交那台记下（那台判的那几格，一个字不改）
    assert_eq!(asked.len(), 2);
    assert_eq!((asked[1].0.as_str(), asked[1].1.as_str()), ("dev", RECORD));
    assert_eq!(
        asked[1].2,
        json!({"op": "add", "name": "demo", "files": {"SKILL.md": ledger["SKILL.md"], "run.sh": ledger["run.sh"]}})
    );
    assert_eq!(out.record_failed, None);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(home.join("skills/demo/run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "有执行位的那一个要 chmod");
        assert!(out.chmod_failed.is_empty());
    }
    let _ = std::fs::remove_dir_all(&home);
}

#[tokio::test]
async fn a_target_changed_after_the_preview_stops_the_write_and_names_what_was_written() {
    let home = temp_home("skill-stale");
    std::fs::create_dir_all(home.join("skills/demo")).unwrap();
    std::fs::write(home.join("skills/demo/SKILL.md"), "old\n").unwrap();
    let mut reply = plan_reply(json!(["run.sh", "SKILL.md"]));
    reply["base"] = json!(home.display().to_string());
    let ask = FakeAsk::new(vec![Ok(reply)]);
    let door = DiskDoor::new(&home);
    // 第一次 put 之前有人在那个落点上放了一份（替身门先把它写进去）⇒ 新文件「必须不存在」的期望不成立 ⇒ stale
    door.interfere.borrow_mut().push("someone else\n".into());
    let target = vec![SkillTargetText {
        path: "SKILL.md".into(),
        text: "old\n".into(),
    }];
    let e = apply_with(
        &ask,
        &door,
        &origin("dev"),
        "demo",
        &source_files(),
        &target,
        &["run.sh".into(), "SKILL.md".into()],
        &["SKILL.md".into()],
    )
    .await;
    let e = e.expect_err("stale 仍被当成写成");
    assert!(
        e.contains("run.sh") && e.contains("一个都还没写") && e.contains("重新看一次差异"),
        "stale 要说清是「看差异之后又被改过」，并指出停在哪：{e}"
    );
    assert_eq!(ask.asked.borrow().len(), 1, "stale 不许重读重算");
    let _ = std::fs::remove_dir_all(&home);
}

/// 跨半边：两条命令名与字段 == 后端 `REGISTRY` 声明的（读后端源码；`skill-install-plan` 的 `root` 本侧不读，逐字登记为唯一例外）。
#[test]
fn what_monitor_sends_and_reads_is_what_the_backend_registers() {
    let inbound =
        std::fs::read_to_string(crate::guard_support::repo_root().join("src/backend/inbound.rs"))
            .expect("读后端 inbound.rs");
    let declared = |cmd: &str| -> std::collections::BTreeSet<String> {
        let at = inbound
            .find(&format!("name: \"{cmd}\""))
            .unwrap_or_else(|| panic!("后端没有 {cmd}"));
        let block = &inbound[at..at + inbound[at..].find("takes_input").unwrap()];
        let fields = &block[block.find("fields:").unwrap()..];
        fields
            .split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect()
    };
    let set = |xs: &[&str]| -> std::collections::BTreeSet<String> {
        xs.iter().map(|s| s.to_string()).collect()
    };
    assert_eq!(
        declared(READ),
        set(&["dir", "files", "name", "root", "skipped"])
    );
    let used_plan = set(&[
        "base",
        "dir",
        "name",
        "overwrite",
        "prefix",
        "rows",
        "source",
        "take",
        "target",
        "write",
    ]);
    let mut want = used_plan.clone();
    want.insert("root".into()); // 本侧不读 `root`（只给人看的那一格用 `dir`）
    want.insert("ledger".into()); // 〔SU1〕本侧读、原样交回 `skill-install-record`（在上面那张 used 里就是它；这里单列是为了看得见）
    assert_eq!(declared(PLAN), want);
    // 〔SU1〕记：发 `op` / `name` / `files`（add）· `dir` / `paths`（drop）；应答本侧不读 ⇒ `changed` / `remaining` 登记为不读的那两格。
    let mut rec = set(&["dir", "files", "name", "op", "paths"]);
    rec.extend(set(&["changed", "remaining"]));
    assert_eq!(declared(RECORD), rec);
    // 〔SU1〕卸：发 `dir` / `take` / `confirm`，读 `delete` / `forget`；`name` / `rows` / `seen` 是界面经通道看的那一半读的（本侧不读）。
    let mut un = set(&["confirm", "delete", "dir", "forget", "take"]);
    un.extend(set(&["name", "rows", "seen"]));
    assert_eq!(declared(UNINSTALL_PLAN), un);
}

/// 判定没长第二个家：本模块生产段零闭集线上名（人群从后端源码现抠：AS1 的四态 ＋ skill 的可疑项种类）。
#[test]
fn this_module_holds_no_install_rule() {
    let root = crate::guard_support::repo_root();
    let mine = guard_core::production_code(
        &std::fs::read_to_string(root.join("src/bridge/src/skill_install.rs")).unwrap(),
    );
    let mut names: Vec<String> = Vec::new();
    for (file, konst) in [
        ("src/backend/mcp_sync.rs", "pub(crate) const STATES"),
        ("src/backend/skill_install.rs", "pub const SUSPECT_KINDS"),
        // 〔SU1〕卸的四态也只许住后端（界面的给字表按它逐键给字，monitor 一个都不许写）。
        ("src/backend/skill_install.rs", "pub const UNINSTALL_STATES"),
    ] {
        let src = std::fs::read_to_string(root.join(file)).unwrap();
        let at = src
            .find(konst)
            .unwrap_or_else(|| panic!("{file} 里没有 {konst}"));
        let line = &src[at..at + src[at..].find(';').unwrap()];
        names.extend(line.split('"').skip(1).step_by(2).map(str::to_string));
    }
    assert!(names.len() >= 8, "人群没抠到：{names:?}");
    let hits: Vec<&String> = names
        .iter()
        .filter(|n| mine.contains(&format!("\"{n}\"")))
        .collect();
    assert!(
        hits.is_empty(),
        "monitor 这一侧出现了判定的线上名：{hits:?}"
    );
}

// ═══════════════════════ 〔SU1 · 第四波 4C · V116〕装完记 · 卸 ═══════════════════════
//
// 守的要求（住址）：用户裁决 **V116**「要，只删装时写进去的文件」—— 原文「装的时候记下写了哪些文件，卸只删这些（装完用户自己改过的先问）」；
// `调研/第四波记录/SU1.md §1.2`（写到一半停下也记 · 记不下来装照样算成、说清）· `§1.3`（卸：删经那台 `files-delete` 带 `expect`、stale 就停、删掉的摘掉）。
// 买到：替身后端 ＋ 替身门（临时目录上 CAS 删）—— 记的恰是真写成了的那几个、那几格原样 · 删的恰是判定放行的、期望是看的时候那一份 ·
// 摘的恰是删掉的 ＋ 已经不在的 · 停在半路也先摘已删的 · 记 / 摘失败说清不吞。
// 买不到：🔴 真远端 · 真后端（`BackendAsk` / `BackendDoor` 那一跳）· 空目录留着（`Door` 没有删空目录的口）。

#[tokio::test]
async fn a_partial_install_still_records_what_was_written() {
    let home = temp_home("skill-partial");
    std::fs::create_dir_all(home.join("skills/demo")).unwrap();
    // 看差异时 SKILL.md 是 "old\n"，之后被人改了 ⇒ 写它那一下 stale；run.sh 在它前面、已经写进去了
    std::fs::write(home.join("skills/demo/SKILL.md"), "someone else\n").unwrap();
    let mut reply = plan_reply(json!(["run.sh", "SKILL.md"]));
    reply["base"] = json!(home.display().to_string());
    let ledger = reply["ledger"].clone();
    let ask = FakeAsk::new(vec![Ok(reply), Ok(json!({"remaining": 1}))]);
    let door = DiskDoor::new(&home);
    let target = vec![SkillTargetText {
        path: "SKILL.md".into(),
        text: "old\n".into(),
    }];
    let e = apply_with(
        &ask,
        &door,
        &origin("dev"),
        "demo",
        &source_files(),
        &target,
        &["run.sh".into(), "SKILL.md".into()],
        &["SKILL.md".into()],
    )
    .await
    .expect_err("stale 仍被当成写成");
    assert!(e.contains("前面已经写了 run.sh"), "{e}");
    let asked = ask.asked.borrow().clone();
    assert_eq!(asked.len(), 2, "停在半路也要把已写的记下");
    assert_eq!(
        asked[1].2,
        json!({"op": "add", "name": "demo", "files": {"run.sh": ledger["run.sh"]}}),
        "记的恰是真写成了的那一个"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[tokio::test]
async fn a_record_that_cannot_be_kept_is_said_and_the_install_still_stands() {
    let home = temp_home("skill-norecord");
    let mut reply = plan_reply(json!(["run.sh"]));
    reply["base"] = json!(home.display().to_string());
    let ask = FakeAsk::new(vec![Ok(reply), Err("记录读不懂".into())]);
    let door = DiskDoor::new(&home);
    let out = apply_with(
        &ask,
        &door,
        &origin("dev"),
        "demo",
        &source_files(),
        &[],
        &["run.sh".into()],
        &[],
    )
    .await
    .expect("装本身成了");
    assert_eq!(out.written, vec!["run.sh".to_string()]);
    let why = out.record_failed.expect("记不下来要说");
    assert!(
        why.contains("记录读不懂") && why.contains("卸不掉"),
        "{why}"
    );
    // 那台没给「怎么记」⇒ 同样说清，不拿猜的摘要去记
    let mut reply = plan_reply(json!(["later.md"]));
    reply["base"] = json!(home.display().to_string());
    reply["ledger"] = json!({});
    let ask = FakeAsk::new(vec![Ok(reply)]);
    let out = apply_with(
        &ask,
        &door,
        &origin("dev"),
        "demo",
        &source_files(),
        &[],
        &["later.md".into()],
        &[],
    )
    .await
    .expect("装本身成了");
    assert!(out.record_failed.is_some_and(|w| w.contains("卸不掉")));
    assert_eq!(ask.asked.borrow().len(), 1, "没有可记的就不去记");
    let _ = std::fs::remove_dir_all(&home);
}

fn uninstall_reply(delete: Value, forget: Value) -> Value {
    json!({"dir": "/ignored", "name": "demo", "rows": [], "seen": [], "delete": delete, "forget": forget})
}

#[tokio::test]
async fn uninstall_deletes_exactly_the_judged_paths_with_the_seen_texts_and_drops_them() {
    let home = temp_home("skill-uninstall");
    let dir = home.join("skills/demo");
    std::fs::create_dir_all(&dir).unwrap();
    for (p, t) in [("a.md", "a\n"), ("b.md", "b\n"), ("keep.md", "k\n")] {
        std::fs::write(dir.join(p), t).unwrap();
    }
    let ask = FakeAsk::new(vec![
        Ok(uninstall_reply(json!(["a.md", "b.md"]), json!(["gone.md"]))),
        Ok(json!({"remaining": 1})),
    ]);
    let door = DiskDoor::new(&home);
    let d = dir.display().to_string();
    let seen = vec![
        SkillTargetText {
            path: "a.md".into(),
            text: "a\n".into(),
        },
        SkillTargetText {
            path: "b.md".into(),
            text: "b\n".into(),
        },
        SkillTargetText {
            path: "keep.md".into(),
            text: "k\n".into(),
        },
    ];
    // take 里多给一个 keep.md（界面勾了），判定只放行 a / b ⇒ 删的必须是判定那一份，不是 take
    let out = uninstall_with(
        &ask,
        &door,
        &origin("dev"),
        &d,
        &seen,
        &["a.md".into(), "b.md".into(), "keep.md".into()],
        &["b.md".into()],
    )
    .await
    .expect("卸");
    assert_eq!(out.deleted, vec!["a.md".to_string(), "b.md".to_string()]);
    assert_eq!(out.record_failed, None);
    assert_eq!(
        door.deleted.borrow().clone(),
        vec![
            ("a.md".to_string(), "a\n".to_string()),
            ("b.md".to_string(), "b\n".to_string())
        ],
        "删的恰是判定放行的那几个，期望是看的时候那一份"
    );
    assert!(dir.join("keep.md").exists(), "没被放行的不许删");
    assert!(dir.exists(), "目录留着（只删文件）");
    let asked = ask.asked.borrow().clone();
    assert_eq!(
        (asked[0].0.as_str(), asked[0].1.as_str(), asked[0].2.clone()),
        (
            "dev",
            UNINSTALL_PLAN,
            json!({"dir": d, "take": ["a.md", "b.md", "keep.md"], "confirm": ["b.md"]})
        ),
        "交给判定的 take / confirm 原样"
    );
    assert_eq!(
        (asked[1].1.as_str(), asked[1].2.clone()),
        (
            RECORD,
            json!({"op": "drop", "dir": d, "paths": ["a.md", "b.md", "gone.md"]})
        ),
        "摘的恰是删掉的 ＋ 已经不在的"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[tokio::test]
async fn an_uninstall_that_hits_a_changed_file_stops_says_so_and_still_drops_what_was_deleted() {
    let home = temp_home("skill-uninstall-stale");
    let dir = home.join("skills/demo");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.md"), "a\n").unwrap();
    std::fs::write(dir.join("b.md"), "b, edited after the look\n").unwrap();
    let ask = FakeAsk::new(vec![
        Ok(uninstall_reply(json!(["a.md", "b.md"]), json!([]))),
        Ok(json!({"remaining": 1})),
    ]);
    let door = DiskDoor::new(&home);
    let d = dir.display().to_string();
    let seen = vec![
        SkillTargetText {
            path: "a.md".into(),
            text: "a\n".into(),
        },
        SkillTargetText {
            path: "b.md".into(),
            text: "b\n".into(),
        },
    ];
    let e = uninstall_with(
        &ask,
        &door,
        &origin("dev"),
        &d,
        &seen,
        &["a.md".into(), "b.md".into()],
        &[],
    )
    .await
    .expect_err("stale 仍被当成删成");
    assert!(
        e.contains("b.md") && e.contains("前面已经删了 a.md") && e.contains("重新看一次"),
        "{e}"
    );
    assert!(dir.join("b.md").exists(), "看过之后被改的那一份不许删");
    let asked = ask.asked.borrow().clone();
    assert_eq!(asked.len(), 2, "stale 不重读重算；已删的照样摘");
    assert_eq!(
        asked[1].2,
        json!({"op": "drop", "dir": d, "paths": ["a.md"]})
    );
    // 摘也失败 ⇒ 两件事都说
    let ask = FakeAsk::new(vec![
        Ok(uninstall_reply(json!(["zzz.md"]), json!(["gone.md"]))),
        Err("写不进去".into()),
    ]);
    let e = uninstall_with(
        &ask,
        &door,
        &origin("dev"),
        &d,
        &seen,
        &["zzz.md".into()],
        &[],
    )
    .await
    .expect_err("没有原文的那一个不许删");
    assert!(e.contains("两端对不上") && e.contains("写不进去"), "{e}");
    let _ = std::fs::remove_dir_all(&home);
}

#[tokio::test]
async fn an_uninstall_reply_that_breaks_the_contract_deletes_nothing() {
    let home = temp_home("skill-uninstall-broken");
    let door = DiskDoor::new(&home);
    for bad in [
        json!({"delete": ["a"]}),
        json!({"forget": []}),
        json!({"delete": [1], "forget": []}),
    ] {
        let ask = FakeAsk::new(vec![Ok(bad.clone())]);
        let e = uninstall_with(&ask, &door, &origin("dev"), "/d", &[], &["a".into()], &[])
            .await
            .expect_err("坏应答被收下");
        assert_eq!(e, *UNREADABLE_REPLY, "{bad}");
    }
    assert!(door.deleted.borrow().is_empty());
    let _ = std::fs::remove_dir_all(&home);
}
