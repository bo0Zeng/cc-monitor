//! 插件装删（`plugin_exec`）：照那一家的插件那一格跑、读退出码、照它的解法读列表。只拿替身家（`fake` 声明了 · `fake2` 没声明），
//! 跑法换成记账的替身：一条真命令都不起。替身家的写法是 `shelf/<名>`（与光名字不同）：装 · 删交的、列出来比的都是它。

use super::*;
use std::cell::RefCell;

/// 记下每一跳的 argv，按次序回给定的结局（不够了 ⇒ 回最后那一个）。
struct Script {
    seen: RefCell<Vec<Vec<String>>>,
    replies: Vec<Ran>,
}

impl Script {
    fn new(replies: Vec<Ran>) -> Script {
        Script {
            seen: RefCell::new(Vec::new()),
            replies,
        }
    }
    fn run(&self, argv: &[String]) -> Result<Ran, Said> {
        let i = self.seen.borrow().len();
        self.seen.borrow_mut().push(argv.to_vec());
        Ok(self.replies[i.min(self.replies.len() - 1)].clone())
    }
    fn seen(&self) -> Vec<Vec<String>> {
        self.seen.borrow().clone()
    }
}

fn ran(code: i32, stdout: &str, stderr: &str) -> Ran {
    Ran {
        code: Some(code),
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    }
}

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|s| s.to_string()).collect()
}

fn reg() -> &'static [crate::agents::Adapter] {
    crate::agents::fake::two_families()
}

#[test]
fn installing_runs_that_familys_line_with_its_spelling_and_confirms_it_on_the_list() {
    let s = Script::new(vec![ran(0, "", ""), ran(0, "shelf/widget 1.0 on\n", "")]);
    let done = install(reg(), None, "shelf/widget", &|a| s.run(a)).expect("退出码 0 ⇒ 装上");
    assert_eq!(
        s.seen(),
        [
            argv(&["fakeagent", "gadget", "fit", "shelf/widget"]),
            argv(&["fakeagent", "gadget", "roster"]),
        ]
    );
    assert_eq!(done.changed, ["shelf/widget"]);
    assert_eq!(done.note, None, "列里见到了 ⇒ 不另说");
    // 列里只有光名字对得上的别家写法 ⇒ 不算见到。
    let other = Script::new(vec![ran(0, "", ""), ran(0, "other/widget 1.0 on\n", "")]);
    let done = install(reg(), None, "shelf/widget", &|a| other.run(a)).expect("退出码 0");
    assert!(done.note.is_some(), "照光名字认成见到了");
}

#[test]
fn a_nonzero_exit_is_a_failure_and_carries_what_the_command_said() {
    let s = Script::new(vec![ran(3, "", "no such plugin")]);
    let e = install(reg(), None, "shelf/widget", &|a| s.run(a)).expect_err("退出码非 0 ⇒ 没成");
    assert_eq!(e.code.as_deref(), Some("io_failed"));
    assert!(
        e.raw
            .as_deref()
            .is_some_and(|r| r.contains("no such plugin")),
        "那条命令自己的话进原话：{e:?}"
    );
    assert_eq!(s.seen().len(), 1, "没成就不再列");
}

#[test]
fn removing_runs_the_remove_line_and_says_so_when_the_list_still_has_it() {
    let s = Script::new(vec![ran(0, "", ""), ran(0, "shelf/widget 2 off\n", "")]);
    let done = remove(reg(), None, "shelf/widget", &|a| s.run(a)).expect("退出码 0");
    assert_eq!(
        s.seen()[0],
        argv(&["fakeagent", "gadget", "drop", "shelf/widget"])
    );
    assert!(done.note.is_some(), "删完列里还在 ⇒ 说一句");
}

#[test]
fn a_list_it_cannot_read_is_said_to_be_unreadable_not_guessed() {
    let s = Script::new(vec![ran(0, "{\"installed\": []}", "")]);
    let e = listed(reg(), None, &|a| s.run(a)).expect_err("读不懂");
    assert_eq!(e.code.as_deref(), Some("bad_file"));
    let ok = Script::new(vec![ran(0, "shelf/lamp - on\n", "")]);
    assert_eq!(
        listed(reg(), None, &|a| ok.run(a)).expect("读得懂"),
        [crate::agents::PluginListed {
            key: "shelf/lamp".into(),
            name: "lamp".into(),
            version: None,
            on: true
        }]
    );
}

#[test]
fn a_family_without_plugin_cells_runs_nothing() {
    let s = Script::new(vec![ran(0, "", "")]);
    let fake2 = cmd_wire::AgentKind::from("fake2");
    assert_eq!(
        install(reg(), Some(&fake2), "shelf/widget", &|a| s.run(a))
            .expect_err("没声明 ⇒ 拒")
            .code
            .as_deref(),
        Some("refused")
    );
    for e in [
        remove(reg(), Some(&fake2), "shelf/widget", &|a| s.run(a)).map(|_| ()),
        listed(reg(), Some(&fake2), &|a| s.run(a)).map(|_| ()),
    ] {
        assert_eq!(e.expect_err("没声明 ⇒ 拒").code.as_deref(), Some("refused"));
    }
    assert!(s.seen().is_empty(), "一条都不跑");
    assert!(declared(reg(), None));
    assert!(!declared(reg(), Some(&fake2)));
}

/// 一条插件条目：带那一家的写法 `shelf/<名>`。
fn plugin(
    name: &str,
    project: Option<&str>,
    agent: Option<&str>,
) -> crate::assets::asset_catalog::Asset {
    crate::assets::asset_catalog::Asset {
        summary: serde_json::json!({ "market": "m", KEY: format!("shelf/{name}") }),
        ..keyless(name, project, agent)
    }
}

/// 同 [`plugin`]，条目上没有那一家的写法（旧对端扫出的那一形）。
fn keyless(
    name: &str,
    project: Option<&str>,
    agent: Option<&str>,
) -> crate::assets::asset_catalog::Asset {
    crate::assets::asset_catalog::Asset {
        agent: agent.map(cmd_wire::AgentKind::from),
        kind: crate::assets::asset_catalog::ExtKind::Plugin,
        name: name.into(),
        project: project.map(str::to_string),
        dir: None,
        digest: "d".into(),
        summary: serde_json::json!({ "market": "m" }),
        off: false,
    }
}

fn by_command(a: &crate::assets::asset_catalog::Asset) -> Option<bool> {
    a.summary
        .get(BY_COMMAND)
        .and_then(serde_json::Value::as_bool)
}

/// ★ 扫完照那一家自己列出来的结果标「经不经插件命令管」：照条目上那一家的写法比 ——
/// 列表里有那个写法的（用户级）⇒ 真；光名字对得上、写法对不上的 · 不在列表里的 · 项目里的 ⇒ 假；
/// 条目上没有写法的 ⇒ 留缺。每家只跑一次列表，没声明那一格的家不跑、不标；skill 不动。
#[test]
fn plugins_are_marked_by_command_from_that_familys_own_list() {
    let s = Script::new(vec![ran(0, "shelf/widget 1 on\nother/lamp 1 on\n", "")]);
    let mut assets = vec![
        plugin("widget", None, None),
        plugin("lamp", None, None),
        plugin("widget", Some("/w"), None),
        plugin("x", None, Some("fake2")),
        crate::assets::asset_catalog::Asset {
            kind: crate::assets::asset_catalog::ExtKind::Skill,
            ..plugin("s", None, None)
        },
        keyless("widget", None, None),
    ];
    let problems = mark_by_command(reg(), &mut assets, &|a| s.run(a));
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(s.seen(), [argv(&["fakeagent", "gadget", "roster"])]);
    let got: Vec<Option<bool>> = assets.iter().map(by_command).collect();
    assert_eq!(
        got,
        [Some(true), Some(false), Some(false), None, None, None]
    );
}

/// ★ 那一家扫出的写法原样进目录条目（`summary.key`），通用层不拼。
#[test]
fn the_familys_own_spelling_goes_into_the_catalog_entry_as_given() {
    let seen = crate::agents::PluginSeen {
        project: None,
        name: "widget".into(),
        key: "shelf/widget".into(),
        market: "shelf".into(),
        root: std::path::PathBuf::from("/p"),
        version: None,
        description: None,
        parts: Default::default(),
        on: true,
        from: crate::agents::PluginFrom::User,
    };
    let a = crate::assets::asset_catalog::plugin_asset(&seen);
    assert_eq!(a.name, "widget");
    assert_eq!(a.summary[KEY], "shelf/widget");
}

/// ★ 那一家列不出（退出码非 0 · 读不懂）⇒ 它的插件那一格留缺（不说假），并照实说一句；没扫到插件的家一条命令都不跑。
#[test]
fn a_family_whose_list_fails_has_all_its_plugins_left_unmarked_and_says_so() {
    for reply in [ran(2, "", "boom"), ran(0, "{\"installed\":[]}", "")] {
        let s = Script::new(vec![reply]);
        let mut assets = vec![plugin("widget", None, None)];
        let problems = mark_by_command(reg(), &mut assets, &|a| s.run(a));
        assert_eq!(problems.len(), 1, "列不出没说：{problems:?}");
        assert_eq!(by_command(&assets[0]), None);
    }
    let idle = Script::new(vec![ran(0, "", "")]);
    let mut none = vec![crate::assets::asset_catalog::Asset {
        kind: crate::assets::asset_catalog::ExtKind::Skill,
        ..plugin("s", None, None)
    }];
    assert!(mark_by_command(reg(), &mut none, &|a| idle.run(a)).is_empty());
    assert!(idle.seen().is_empty(), "没有插件也跑了列表");
}
