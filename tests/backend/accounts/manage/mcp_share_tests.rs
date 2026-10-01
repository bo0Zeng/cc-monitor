//! 账号之间同步用户级 MCP 的三方对照（纯）：在一个号里加 ⇒ 进共享集合、写到别的号；被旧内容盖掉 ⇒ 补回；
//! cc-monitor 里删 ⇒ 所有号都撤；两边都改 ⇒ 不自动选、冻住那一条。

use super::*;
use serde_json::json;

fn servers(v: Value) -> Servers {
    match v {
        Value::Object(m) => m,
        _ => panic!("夹具是对象"),
    }
}

fn seen(name: &str, now: Value) -> Seen {
    Seen {
        name: name.to_string(),
        dir: format!("/h/.claude-accts/{name}"),
        now: Some(servers(now)),
    }
}

fn dir(name: &str) -> String {
    format!("/h/.claude-accts/{name}")
}

/// 两个号都同步过 `cclsp` 那一刻的样子。
fn synced() -> Store {
    let one = servers(json!({ "cclsp": { "command": "cclsp" } }));
    Store {
        servers: one.clone(),
        base: [(dir("z"), one.clone()), (dir("b"), one)].into(),
    }
}

fn write_of<'a>(p: &'a Plan, name: &str) -> Option<&'a Servers> {
    p.writes
        .iter()
        .find(|(d, _)| *d == dir(name))
        .map(|(_, s)| s)
}

#[test]
fn added_in_one_account_goes_into_the_set_and_to_the_others() {
    let st = synced();
    let any = json!({ "command": "anysearch" });
    let p = plan(
        &st,
        &[
            seen(
                "z",
                json!({ "cclsp": { "command": "cclsp" }, "anysearch": any }),
            ),
            seen("b", json!({ "cclsp": { "command": "cclsp" } })),
        ],
    );
    assert_eq!(p.store.servers.get("anysearch"), Some(&any));
    assert_eq!(p.adopted, vec![("anysearch".to_string(), "z".to_string())]);
    assert!(
        write_of(&p, "z").is_none(),
        "提出的那个号已经是这一版，不写"
    );
    assert_eq!(
        write_of(&p, "b").and_then(|s| s.get("anysearch")),
        Some(&any)
    );
    assert!(p.conflicts.is_empty());
}

#[test]
fn an_entry_overwritten_by_old_content_is_put_back() {
    let any = json!({ "command": "anysearch" });
    let mut st = synced();
    st.servers.insert("anysearch".into(), any.clone());
    for b in st.base.values_mut() {
        b.insert("anysearch".into(), any.clone());
    }
    // b 号被 Claude 拿内存里的旧内容整份重写：没有 anysearch 了。
    let p = plan(
        &st,
        &[
            seen(
                "z",
                json!({ "cclsp": { "command": "cclsp" }, "anysearch": any }),
            ),
            seen("b", json!({ "cclsp": { "command": "cclsp" } })),
        ],
    );
    assert_eq!(
        write_of(&p, "b").and_then(|s| s.get("anysearch")),
        Some(&any)
    );
    assert_eq!(
        p.store.servers.get("anysearch"),
        Some(&any),
        "共享集合里还在"
    );
}

#[test]
fn equal_to_the_base_but_not_the_set_means_written_back_over() {
    let new = json!({ "command": "cclsp", "args": ["--v2"] });
    let mut st = synced();
    st.servers.insert("cclsp".into(), new.clone());
    st.base
        .get_mut(&dir("z"))
        .unwrap()
        .insert("cclsp".into(), new.clone());
    let p = plan(
        &st,
        &[
            seen("z", json!({ "cclsp": new })),
            seen("b", json!({ "cclsp": { "command": "cclsp" } })),
        ],
    );
    assert_eq!(write_of(&p, "b").and_then(|s| s.get("cclsp")), Some(&new));
    assert!(p.adopted.is_empty() && p.conflicts.is_empty());
}

#[test]
fn removed_in_cc_monitor_leaves_every_account() {
    let st = synced();
    let now = [
        seen("z", json!({ "cclsp": { "command": "cclsp" } })),
        seen("b", json!({ "cclsp": { "command": "cclsp" } })),
    ];
    let p = plan(&decide(&st, &now, "cclsp", None), &now);
    assert!(p.store.servers.is_empty());
    for a in ["z", "b"] {
        assert_eq!(write_of(&p, a), Some(&Servers::new()), "{a} 里撤掉");
    }
}

#[test]
fn changed_on_both_sides_is_not_picked_automatically() {
    // 共享那边（经 cc-monitor）换成 v2，底收的是当时 b 号里的旧版；之后 b 号里又被人改成 v3。
    let now0 = [
        seen("z", json!({ "cclsp": { "command": "cclsp" } })),
        seen("b", json!({ "cclsp": { "command": "cclsp" } })),
    ];
    let v2 = json!({ "command": "cclsp", "args": ["2"] });
    let st = decide(&synced(), &now0, "cclsp", Some(v2.clone()));
    let v3 = json!({ "command": "cclsp", "args": ["3"] });
    let now = [
        seen("z", json!({ "cclsp": { "command": "cclsp" } })),
        seen("b", json!({ "cclsp": v3 })),
    ];
    let p = plan(&st, &now);
    assert!(p.writes.is_empty(), "冲突的那一条整条冻住：一个号都不写");
    assert_eq!(p.store.servers.get("cclsp"), Some(&v2), "共享集合不动");
    assert_eq!(p.store.base, st.base, "底不动（等用户挑）");
    let c = &p.conflicts[0];
    assert_eq!(c.name, "cclsp");
    assert_eq!(
        c.choices,
        vec![
            Choice {
                from: None,
                holders: vec![],
                gone: false
            },
            Choice {
                from: Some("b".into()),
                holders: vec!["b".into()],
                gone: false
            },
        ]
    );
    // 用户挑了 b 号那一版 ⇒ 两个号都是它。
    let to = pick(&p.store, &now, "cclsp", Some("b")).unwrap();
    let after = plan(&decide(&p.store, &now, "cclsp", to), &now);
    assert!(after.conflicts.is_empty());
    assert_eq!(
        write_of(&after, "z").and_then(|s| s.get("cclsp")),
        Some(&v3)
    );
    assert!(write_of(&after, "b").is_none());
}

#[test]
fn two_accounts_adding_different_versions_conflict_without_a_shared_choice() {
    let p = plan(
        &Store::default(),
        &[
            seen("z", json!({ "s": { "command": "one" } })),
            seen("b", json!({ "s": { "command": "two" } })),
        ],
    );
    assert!(p.writes.is_empty() && p.store.servers.is_empty());
    let froms: Vec<_> = p.conflicts[0]
        .choices
        .iter()
        .map(|c| c.from.clone())
        .collect();
    assert_eq!(froms, vec![Some("z".to_string()), Some("b".to_string())]);
}

#[test]
fn first_sync_adopts_what_the_accounts_already_agree_on() {
    let p = plan(
        &Store::default(),
        &[
            seen(
                "z",
                json!({ "cclsp": { "command": "c" }, "anysearch": { "command": "a" } }),
            ),
            seen("b", json!({ "cclsp": { "command": "c" } })),
            seen("q", json!({})),
        ],
    );
    assert_eq!(
        p.store.servers.keys().collect::<Vec<_>>(),
        vec!["anysearch", "cclsp"]
    );
    assert!(write_of(&p, "z").is_none());
    assert_eq!(write_of(&p, "b").map(|s| s.len()), Some(2));
    assert_eq!(write_of(&p, "q").map(|s| s.len()), Some(2));
}

#[test]
fn an_unreadable_account_is_left_alone_and_keeps_its_base() {
    let st = synced();
    let p = plan(
        &st,
        &[
            seen(
                "z",
                json!({ "cclsp": { "command": "cclsp" }, "x": { "command": "x" } }),
            ),
            Seen {
                name: "b".into(),
                dir: dir("b"),
                now: None,
            },
        ],
    );
    assert!(write_of(&p, "b").is_none());
    assert_eq!(p.store.base.get(&dir("b")), st.base.get(&dir("b")));
    assert!(p.store.servers.contains_key("x"));
}

#[test]
fn the_store_round_trips_and_a_bad_one_is_refused() {
    let st = synced();
    assert_eq!(Store::parse(&st.render(KEY), KEY), Ok(st));
    for bad in [
        "[]",
        "{\"version\":2}",
        "{\"version\":1,\"mcpServers\":[]}",
        "{\"version\":1,\"base\":{\"d\":1}}",
        "{",
    ] {
        assert!(Store::parse(bad, KEY).is_err(), "{bad:?} 该拒");
    }
}

const KEY: &str = "mcpServers";
