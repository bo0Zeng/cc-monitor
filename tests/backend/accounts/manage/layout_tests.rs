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

/// `/h` 家；共享库有 skills · settings.json · accounts（排除）· x.bak（排除）；清单里一个号 z。
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
            (".credentials.json", file(0o600)),
        ],
    )
}

#[test]
fn share_items_skip_identity_and_excluded_names() {
    assert_eq!(
        share_items(&snap(healthy()).shared),
        ["settings.json", "skills"]
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

/// 会话起停与会话记录那两项不许隔离：常驻后端只看共享库里的那一份，隔离了那个号起的会话主窗口里就看不见。别的共享项照旧能隔离。
#[test]
fn the_items_the_session_watcher_reads_cannot_be_isolated() {
    let mut s = snap(healthy());
    for n in ["sessions", "projects"] {
        s.shared
            .entries
            .insert(n.to_string(), Item::Dir { mode: Some(0o700) });
    }
    for n in ["sessions", "projects"] {
        let got = plan_isolate(&s, n);
        assert!(
            matches!(&got, Err((_, said)) if said.contains(n)),
            "{n} 被放行隔离了：{got:?}"
        );
    }
    assert!(plan_isolate(&s, "skills").is_ok(), "别的共享项照旧能隔离");
}
