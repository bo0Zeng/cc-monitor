//! 计划是纯的：拿一份手搭的快照喂它，看出来的那几步。
use super::*;
use crate::accounts::manage::model::Manifest;
use crate::accounts::manage::scan::{DirState, Roots, Snapshot};
use std::collections::BTreeMap;

fn link(t: &str) -> Item {
    Item::Link {
        target: t.into(),
        dangling: false,
    }
}
fn file(mode: u32) -> Item {
    Item::File {
        mode: Some(mode),
        size: 2,
        mtime: None,
    }
}
fn dir(mode: u32, entries: &[(&str, Item)]) -> DirState {
    DirState {
        me: Some(Item::Dir { mode: Some(mode) }),
        entries: entries
            .iter()
            .map(|(n, i)| (n.to_string(), i.clone()))
            .collect::<BTreeMap<_, _>>(),
    }
}

/// `/h` 家；共享库有 skills · settings.json · agents · plans · accounts（排除）· x.bak（排除）；清单里一个号 z。
fn snap(z: DirState) -> Snapshot {
    let text = r#"{"version":1,"sharedStore":"/h/.claude","accounts":[{"name":"z","email":"","configDir":"/h/.cc-monitor/accounts/z","isDefault":true}]}"#;
    let mut dirs = BTreeMap::new();
    dirs.insert("/h/.cc-monitor/accounts/z".to_string(), z);
    Snapshot {
        roots: Some(Roots {
            home: "/h".into(),
            shared: "/h/.claude".into(),
            accts: "/h/.cc-monitor/accounts".into(),
        }),
        manifest_text: Some(text.into()),
        manifest: Some(Manifest::parse(text)),
        shared: dir(
            0o700,
            &[
                ("skills", Item::Dir { mode: Some(0o755) }),
                ("settings.json", file(0o644)),
                ("agents", Item::Dir { mode: Some(0o700) }),
                ("plans", Item::Dir { mode: Some(0o700) }),
                ("accounts", Item::Dir { mode: Some(0o755) }),
                ("x.bak", file(0o644)),
            ],
        ),
        accts: dir(0o700, &[]),
        dirs,
        ..Snapshot::default()
    }
}

fn healthy() -> DirState {
    dir(
        0o700,
        &[
            ("skills", link("/h/.claude/skills")),
            ("settings.json", link("/h/.claude/settings.json")),
            ("agents", link("/h/.claude/agents")),
            ("plans", link("/h/.claude/plans")),
            (".credentials.json", file(0o600)),
        ],
    )
}

#[test]
fn share_items_skip_identity_and_excluded_names() {
    assert_eq!(
        share_items(&snap(healthy()).shared),
        ["agents", "plans", "settings.json", "skills"]
    );
}

#[test]
fn repair_of_a_healthy_account_plans_nothing() {
    assert_eq!(plan_repair(&snap(healthy())).unwrap(), Plan::default());
}

#[test]
fn repair_plans_exactly_the_fixes() {
    let broken = dir(
        0o755,
        &[
            ("skills", link("/elsewhere")),
            (".credentials.json", file(0o644)),
            ("gone", link("/h/.claude/gone")),
            ("stats-cache.json", link("/h/.claude/stats-cache.json")),
            ("agents", link("/h/.claude/agents")),
            ("plans", link("/h/.claude/plans")),
        ],
    );
    let p = plan_repair(&snap(broken)).unwrap();
    let z = |n: &str| format!("/h/.cc-monitor/accounts/z/{n}");
    assert_eq!(
        p.ops,
        vec![
            Op::Chmod {
                mode: 0o700,
                at: "/h/.cc-monitor/accounts/z".into()
            },
            Op::Chmod {
                mode: 0o600,
                at: z(".credentials.json")
            },
            Op::Link {
                target: "/h/.claude/settings.json".into(),
                at: z("settings.json")
            },
            Op::Relink {
                target: "/h/.claude/skills".into(),
                at: z("skills")
            },
            Op::Remove(z("gone")),
            Op::Remove(z("stats-cache.json")),
        ]
    );
}

#[test]
fn add_plans_dir_links_and_manifest_and_refuses_duplicates() {
    let s = snap(healthy());
    let want = AddIntent {
        name: "b".into(),
        api_key: true,
        cred_file: None,
        make_default: false,
    };
    let p = plan_add(&s, &want).unwrap();
    assert_eq!(
        p.ops.first(),
        Some(&Op::MkDir("/h/.cc-monitor/accounts/b".into()))
    );
    assert_eq!(p.ops.last(), Some(&Op::WriteManifest));
    let m = p.manifest.unwrap();
    assert_eq!(m.find("b").unwrap().auth_kind.as_deref(), Some("api-key"));
    let dup = AddIntent {
        name: "z".into(),
        ..want
    };
    assert_eq!(plan_add(&s, &dup).unwrap_err().0, "refused");
}

/// Claude Code 按需才在配置根下建、各号该共用的那几个目录（自建的子代理 `agents` · 计划模式写的计划 `plans`：会话记录本来就共用，
/// 换个号接着那条会话也要找得到它的计划）：共享库里还没有也照样链 —— 不链的话 Claude Code 头一回用就在号里建一份实体，各号就分开了。
/// - 谁都没有 ⇒ 修复先在共享库里建出来，再给号链上；
/// - 只有一个号里有一份实体 ⇒ 把那一份挪进共享库当共用的那一份，再链回去；
/// - 两个号里各有一份 ⇒ 不替人合并：共享库里建一份空的，有实体的那两个只提示，其余照链。
/// 照 `agents` · `plans` 两个名字核（不是共享库里碰巧有的）；`telemetry` 是各号自己的（登在身份表里），不在这一族。
#[test]
fn the_shared_dirs_claude_makes_on_demand_are_linked_before_the_shared_store_has_them() {
    let acct = |n: &str| format!("/h/.cc-monitor/accounts/{n}");
    let two = |z: DirState, b: DirState| {
        let text = r#"{"version":1,"sharedStore":"/h/.claude","accounts":[{"name":"z","email":"","configDir":"/h/.cc-monitor/accounts/z","isDefault":true},{"name":"b","email":"","configDir":"/h/.cc-monitor/accounts/b","isDefault":false}]}"#;
        let mut s = snap(z);
        s.manifest_text = Some(text.into());
        s.manifest = Some(Manifest::parse(text));
        s.dirs.insert(acct("b"), b);
        s.shared.entries.remove("agents");
        s.shared.entries.remove("plans");
        s
    };
    let bare = || {
        let mut d = healthy();
        d.entries.remove("agents");
        d.entries.remove("plans");
        d
    };
    let with = |name: &str| {
        let mut d = bare();
        d.entries
            .insert(name.to_string(), Item::Dir { mode: Some(0o700) });
        d
    };
    let mk = |n: &str| Op::MkDir(format!("/h/.claude/{n}"));
    let ln = |who: &str, n: &str| Op::Link {
        target: format!("/h/.claude/{n}"),
        at: format!("{}/{n}", acct(who)),
    };
    // 谁都没有：建 · 链。
    let p = plan_repair(&two(bare(), bare())).unwrap();
    for n in ["agents", "plans"] {
        let made = p.ops.iter().position(|o| *o == mk(n));
        assert!(made.is_some(), "{n}：共享库里没建：{:?}", p.ops);
        for who in ["z", "b"] {
            let linked = p.ops.iter().position(|o| *o == ln(who, n));
            assert!(
                linked.is_some_and(|l| Some(l) > made),
                "{n}：{who} 没在建好之后链上：{:?}",
                p.ops
            );
        }
    }
    // 一个号里有实体：挪进共享库，那个号与别的号都链上；不另建。
    let p = plan_repair(&two(with("plans"), bare())).unwrap();
    let moved = Op::Move {
        from: format!("{}/plans", acct("z")),
        to: "/h/.claude/plans".into(),
    };
    let at = p.ops.iter().position(|o| *o == moved);
    assert!(at.is_some(), "z 的那份没挪进共享库：{:?}", p.ops);
    assert!(!p.ops.contains(&mk("plans")), "挪了还另建：{:?}", p.ops);
    for who in ["z", "b"] {
        let linked = p.ops.iter().position(|o| *o == ln(who, "plans"));
        assert!(linked > at, "{who} 没在挪好之后链上：{:?}", p.ops);
    }
    // 两个号里都有：建一份空的，只链没有的那一边；有的两个都只提示、不动。
    let p = plan_repair(&two(with("agents"), with("agents"))).unwrap();
    assert!(p.ops.contains(&mk("agents")), "{:?}", p.ops);
    assert!(
        !p.ops.iter().any(|o| matches!(o, Op::Move { .. })
            || *o == ln("z", "agents")
            || *o == ln("b", "agents")),
        "两份实体被动了：{:?}",
        p.ops
    );
    assert_eq!(
        p.notes.iter().filter(|n| n.contains("agents")).count(),
        2,
        "{:?}",
        p.notes
    );
    // 加号时也链（共享库里没有就先建）。
    let s = two(bare(), bare());
    let p = plan_add(
        &s,
        &AddIntent {
            name: "c".into(),
            api_key: true,
            cred_file: None,
            make_default: false,
        },
    )
    .unwrap();
    for n in ["agents", "plans"] {
        assert!(p.ops.contains(&mk(n)), "{n}：{:?}", p.ops);
        assert!(p.ops.contains(&ln("c", n)), "{n}：{:?}", p.ops);
    }
}
