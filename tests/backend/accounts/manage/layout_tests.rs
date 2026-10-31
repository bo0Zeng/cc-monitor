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

/// `/h` 家；共享库有 skills · settings.json · agents · plans · projects · sessions，另有几项不在已知共享表里的
/// （accounts · x.bak · 登录刷新那把锁与它的属主记录）；清单里一个号 z。
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
                ("projects", Item::Dir { mode: Some(0o700) }),
                ("sessions", Item::Dir { mode: Some(0o700) }),
                ("accounts", Item::Dir { mode: Some(0o755) }),
                ("x.bak", file(0o644)),
                (".oauth_refresh.lock", Item::Dir { mode: Some(0o700) }),
                (".oauth_refresh.lock.owner", file(0o600)),
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
            ("projects", link("/h/.claude/projects")),
            ("sessions", link("/h/.claude/sessions")),
            (".credentials.json", file(0o600)),
        ],
    )
}

#[test]
fn share_items_take_only_the_known_shared_names() {
    assert_eq!(
        share_items(&snap(healthy()).shared),
        [
            "agents",
            "plans",
            "projects",
            "sessions",
            "settings.json",
            "skills"
        ]
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
            ("projects", link("/h/.claude/projects")),
            ("sessions", link("/h/.claude/sessions")),
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
    assert_eq!(
        plan_add(&s, &dup).unwrap_err().code.as_deref(),
        Some("refused")
    );
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

/// 对齐：已知共享表里的一项共享库里新冒出来（`history.jsonl`）⇒ 每个号补一条链；按需才建的那几个谁都没有 ⇒ 建、都链；
/// 一个号有实体 ⇒ 共享库里建一份空的、别的号链上、那个号不动；两个号各有一份 ⇒ 建一份空的、两个号都不动；号里自己的实体（`skills`）不动；
/// 锁文件不链；只建目录 · 建链接；接好之后 ⇒ 空计划。
#[test]
fn aligning_links_every_shared_item_an_account_lacks_and_settles_to_nothing() {
    let acct = |n: &str| format!("/h/.cc-monitor/accounts/{n}");
    let text = r#"{"version":1,"sharedStore":"/h/.claude","accounts":[{"name":"z","email":"","configDir":"/h/.cc-monitor/accounts/z","isDefault":true},{"name":"b","email":"","configDir":"/h/.cc-monitor/accounts/b","isDefault":false}]}"#;
    let two = |z: DirState, b: DirState| {
        let mut s = snap(z);
        s.manifest_text = Some(text.into());
        s.manifest = Some(Manifest::parse(text));
        s.dirs.insert(acct("b"), b);
        s.shared.entries.remove("agents");
        s.shared.entries.remove("plans");
        s.shared.entries.insert("history.jsonl".into(), file(0o600));
        s
    };
    let bare = || {
        let mut d = healthy();
        d.entries.remove("agents");
        d.entries.remove("plans");
        d
    };
    let with = |n: &str| {
        let mut d = bare();
        d.entries.insert(n.into(), Item::Dir { mode: Some(0o700) });
        d
    };
    let mk = |n: &str| Op::MkDir(format!("/h/.claude/{n}"));
    let ln = |who: &str, n: &str| Op::Link {
        target: format!("/h/.claude/{n}"),
        at: format!("{}/{n}", acct(who)),
    };
    let p = plan_align(&two(bare(), bare())).unwrap();
    assert_eq!(
        p.ops,
        vec![
            mk("agents"),
            mk("plans"),
            ln("z", "agents"),
            ln("z", "history.jsonl"),
            ln("z", "plans"),
            ln("b", "agents"),
            ln("b", "history.jsonl"),
            ln("b", "plans"),
        ]
    );
    let mut own_skills = bare();
    own_skills.entries.remove("skills");
    own_skills
        .entries
        .insert("skills".into(), Item::Dir { mode: Some(0o700) });
    let p = plan_align(&two(own_skills, bare())).unwrap();
    assert!(
        !p.ops.iter().any(|o| match o {
            Op::Link { at, .. } | Op::Relink { at, .. } => at.ends_with("/z/skills"),
            Op::Remove(at) => at.ends_with("/skills"),
            _ => false,
        }),
        "号里自己的 skills 被动了：{:?}",
        p.ops
    );
    // 一个号里有实体：对齐不挪它 —— 共享库里建一份空的、别的号链上，有实体的那个号一步都不出（挪只在「修复」）。
    let p = plan_align(&two(with("plans"), bare())).unwrap();
    assert_eq!(
        p.ops,
        vec![
            mk("agents"),
            mk("plans"),
            ln("z", "agents"),
            ln("z", "history.jsonl"),
            ln("b", "agents"),
            ln("b", "history.jsonl"),
            ln("b", "plans"),
        ]
    );
    let p = plan_align(&two(with("agents"), with("agents"))).unwrap();
    assert!(p.ops.contains(&mk("agents")), "{:?}", p.ops);
    assert!(
        !p.ops.contains(&ln("z", "agents")) && !p.ops.contains(&ln("b", "agents")),
        "两份实体被动了：{:?}",
        p.ops
    );
    // 接好之后：空计划。
    assert_eq!(plan_align(&snap(healthy())).unwrap(), Plan::default());
}

/// 会话记录（`projects`）与在跑会话的那份登记（`sessions`）：后端只在共享库里找它们，所以各号必须链回共享库那一份。
/// 新机器上 `~/.claude` 里还没有这两个 ⇒ 建库 · 加号照样先在共享库里建出来再链 —— 不链的话那个号头一回起会话就在号里落一份实体，
/// 后端看不见那条会话。
#[test]
fn transcripts_and_live_session_records_are_linked_even_when_the_shared_store_has_none_yet() {
    let bare = || {
        let mut s = snap(healthy());
        s.shared.entries.remove("projects");
        s.shared.entries.remove("sessions");
        s
    };
    let mut s = bare();
    s.manifest = None;
    s.manifest_text = None;
    s.dirs.clear();
    let mk = |n: &str| Op::MkDir(format!("/h/.claude/{n}"));
    let ln = |who: &str, n: &str| Op::Link {
        target: format!("/h/.claude/{n}"),
        at: format!("/h/.cc-monitor/accounts/{who}/{n}"),
    };
    let p = plan_init(&s, "lab").unwrap();
    for n in ["projects", "sessions"] {
        let made = p.ops.iter().position(|o| *o == mk(n));
        let linked = p.ops.iter().position(|o| *o == ln("lab", n));
        assert!(made.is_some(), "{n}：共享库里没建：{:?}", p.ops);
        assert!(linked > made, "{n}：没在建好之后链上：{:?}", p.ops);
    }
    let s = bare();
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
    for n in ["projects", "sessions"] {
        assert!(p.ops.contains(&mk(n)), "{n}：{:?}", p.ops);
        assert!(p.ops.contains(&ln("c", n)), "{n}：{:?}", p.ops);
    }
}

/// 认不得的不链：共享库顶层不在已知共享表里的（`bridge-spawn` · `ssh` · 普通文件 · 锁的临时件，权限不论）一项都不进共享集；
/// 已知的那一项 `0600` 也照链。号里 cc-monitor 建过的 `bridge-spawn → 共享库/bridge-spawn` ⇒ 对齐与修复都摘掉这条链
/// （链的目标一个字节不动）；号里的 `bridge-spawn` 是实体 ⇒ 不动。
#[test]
fn only_known_shared_names_are_linked_and_old_links_to_unknown_ones_are_dropped() {
    let unknown = [
        ("bridge-spawn", Item::Dir { mode: Some(0o700) }),
        ("ssh", Item::Dir { mode: Some(0o700) }),
        ("x.json", file(0o644)),
        (".oauth_refresh.lock.owner.tmp.1a2b3c4d", file(0o664)),
    ];
    let z = "/h/.cc-monitor/accounts/z";
    let shaped = |own: Item| {
        let mut d = healthy();
        d.entries.insert("bridge-spawn".into(), own);
        let mut s = snap(d);
        for (n, it) in &unknown {
            s.shared.entries.insert((*n).into(), it.clone());
        }
        s.shared.entries.insert("settings.json".into(), file(0o600));
        s
    };
    let s = shaped(link("/h/.claude/bridge-spawn"));
    let items = share_items(&s.shared);
    assert_eq!(
        items,
        [
            "agents",
            "plans",
            "projects",
            "sessions",
            "settings.json",
            "skills"
        ]
    );
    for p in [plan_align(&s).unwrap(), plan_repair(&s).unwrap()] {
        assert!(
            !p.ops
                .iter()
                .any(|o| matches!(o, Op::Link { .. } | Op::Relink { .. })),
            "{:?}",
            p.ops
        );
        assert_eq!(
            p.ops,
            vec![Op::Remove(format!("{z}/bridge-spawn"))],
            "{:?}",
            p.ops
        );
    }
    let s = shaped(Item::Dir { mode: Some(0o700) });
    assert_eq!(plan_align(&s).unwrap(), Plan::default());
}

/// 原子写与编辑器的旁名不进共享集；号里一条 cc-monitor 建的链（`<号>/foo → 共享库/foo`）共享库那头已不在 ⇒ 对齐摘掉它；
/// 指向别处的悬空链 · 号里自己的实体不动。
#[test]
fn side_names_are_not_shared_and_align_drops_its_own_dangling_links() {
    let sides = [
        "settings.json.ccm-backup-1-0",
        ".credentials.json.1.ThreadId(2).0.tmp",
        "settings.json.ccm-swap-1-0.part",
        "x.part",
        "settings.json.ccm-tmp-1-2-3",
        ".settings.json.swp",
        "settings.json~",
    ];
    let mut z = healthy();
    z.entries.insert(
        "foo".into(),
        Item::Link {
            target: "/h/.claude/foo".into(),
            dangling: true,
        },
    );
    z.entries.insert(
        "bar".into(),
        Item::Link {
            target: "/elsewhere/bar".into(),
            dangling: true,
        },
    );
    z.entries.insert("mine".into(), file(0o644));
    let mut s = snap(z);
    for n in sides {
        s.shared.entries.insert(n.into(), file(0o600));
    }
    let items = share_items(&s.shared);
    for n in sides {
        assert!(!items.contains(&n.to_string()), "{n}");
    }
    let p = plan_align(&s).unwrap();
    assert_eq!(
        p.ops,
        vec![Op::Remove("/h/.cc-monitor/accounts/z/foo".into())]
    );
}

/// 号里指向共享库锁文件的旧链（旧版「修复」链过的）：对齐摘掉这条链；号里自己的锁文件 · 指向别处的同名链不动。
#[test]
fn align_drops_old_links_to_the_shared_lock_files() {
    let mut z = healthy();
    z.entries.insert(
        ".oauth_refresh.lock".into(),
        link("/h/.claude/.oauth_refresh.lock"),
    );
    z.entries.insert(
        ".oauth_refresh.lock.owner".into(),
        link("/h/.claude/.oauth_refresh.lock.owner"),
    );
    let p = plan_align(&snap(z)).unwrap();
    assert_eq!(
        p.ops,
        vec![
            Op::Remove("/h/.cc-monitor/accounts/z/.oauth_refresh.lock".into()),
            Op::Remove("/h/.cc-monitor/accounts/z/.oauth_refresh.lock.owner".into()),
        ]
    );
    let mut z = healthy();
    z.entries.insert(
        ".oauth_refresh.lock".into(),
        Item::Dir { mode: Some(0o700) },
    );
    z.entries
        .insert(".oauth_refresh.lock.owner".into(), link("/elsewhere/owner"));
    assert_eq!(plan_align(&snap(z)).unwrap(), Plan::default());
}

/// 对齐种下的空按需目录（共享库里有 `plans`、快照说它是空的）在修复里当作「共享库还没有」：恰好一个号里有实体 ⇒
/// 先摘掉那个空目录、再把那一份挪过去，各号链上；对齐同一快照不挪不摘；共享库那一份不空 ⇒ 不挪，有实体的那个号只提示。
#[test]
fn repair_lifts_the_one_entity_over_the_empty_dir_align_seeded() {
    let acct = |n: &str| format!("/h/.cc-monitor/accounts/{n}");
    let text = r#"{"version":1,"sharedStore":"/h/.claude","accounts":[{"name":"z","email":"","configDir":"/h/.cc-monitor/accounts/z","isDefault":true},{"name":"b","email":"","configDir":"/h/.cc-monitor/accounts/b","isDefault":false}]}"#;
    let mut z = healthy();
    z.entries
        .insert("plans".into(), Item::Dir { mode: Some(0o700) });
    let mut s = snap(z);
    s.manifest_text = Some(text.into());
    s.manifest = Some(Manifest::parse(text));
    s.dirs.insert(acct("b"), healthy());
    s.empty_shared.insert("plans".into());
    let p = plan_repair(&s).unwrap();
    let drop = Op::DropEmpty("/h/.claude/plans".into());
    let moved = Op::Move {
        from: format!("{}/plans", acct("z")),
        to: "/h/.claude/plans".into(),
    };
    let d = p.ops.iter().position(|o| *o == drop);
    let m = p.ops.iter().position(|o| *o == moved);
    assert!(d.is_some() && m > d, "没先摘空目录再挪：{:?}", p.ops);
    let relinked = Op::Link {
        target: "/h/.claude/plans".into(),
        at: format!("{}/plans", acct("z")),
    };
    assert!(p.ops.iter().position(|o| *o == relinked) > m, "{:?}", p.ops);
    let a = plan_align(&s).unwrap();
    assert!(
        !a.ops
            .iter()
            .any(|o| matches!(o, Op::Move { .. } | Op::DropEmpty(_))),
        "{:?}",
        a.ops
    );
    s.empty_shared.clear();
    let p = plan_repair(&s).unwrap();
    assert!(
        !p.ops
            .iter()
            .any(|o| matches!(o, Op::Move { .. } | Op::DropEmpty(_))),
        "{:?}",
        p.ops
    );
    assert!(p.notes.iter().any(|n| n.contains("plans")), "{:?}", p.notes);
}
