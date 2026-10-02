//! 核对是纯的：同一份手搭的快照，改一格就该多一条致命。
use super::*;
use crate::accounts::manage::scan::{DirState, Roots};
use std::collections::BTreeMap;

fn snap(z: BTreeMap<String, Item>) -> Snapshot {
    let text = r#"{"version":1,"sharedStore":"/h/.claude","accounts":[{"name":"z","email":"","configDir":"/h/.cc-monitor/accounts/z","isDefault":true}]}"#;
    let mut dirs = BTreeMap::new();
    dirs.insert(
        "/h/.cc-monitor/accounts/z".to_string(),
        DirState {
            me: Some(Item::Dir { mode: Some(0o700) }),
            entries: z,
        },
    );
    let mut shared = BTreeMap::new();
    shared.insert("skills".to_string(), Item::Dir { mode: Some(0o755) });
    Snapshot {
        roots: Some(Roots {
            home: "/h".into(),
            shared: "/h/.claude".into(),
            accts: "/h/.cc-monitor/accounts".into(),
        }),
        manifest: Some(super::super::model::Manifest::parse(text)),
        shared: DirState {
            me: Some(Item::Dir { mode: Some(0o700) }),
            entries: shared,
        },
        dirs,
        ..Snapshot::default()
    }
}

fn fails(r: &VerifyReport) -> Vec<&str> {
    r.checks
        .iter()
        .filter(|c| c.level == CheckLevel::Fail)
        .map(|c| c.text.as_str())
        .collect()
}

#[test]
fn a_correct_link_passes_and_a_wrong_one_fails() {
    let mut ok = BTreeMap::new();
    ok.insert(
        "skills".to_string(),
        Item::Link {
            target: "/h/.claude/skills".into(),
            dangling: false,
        },
    );
    let good = verify(&snap(ok), &[]);
    assert!(good.pass, "{:?}", good.checks);
    let mut bad = BTreeMap::new();
    bad.insert(
        "skills".to_string(),
        Item::Link {
            target: "/tmp/x".into(),
            dangling: false,
        },
    );
    let r = verify(&snap(bad), &[]);
    assert!(!r.pass);
    assert_eq!(r.fails, 1);
    assert!(fails(&r)[0].contains("/tmp/x"), "{:?}", fails(&r));
}

#[test]
fn no_manifest_is_one_fail_that_says_where() {
    let mut s = snap(BTreeMap::new());
    s.manifest = None;
    let r = verify(&s, &[]);
    assert_eq!(r.fails, 1);
    assert!(fails(&r)[0].contains("/h/.cc-monitor/accounts/accounts.json"));
}

#[test]
fn an_unknown_name_to_verify_is_a_fail_not_a_pass() {
    let mut ok = BTreeMap::new();
    ok.insert(
        "skills".to_string(),
        Item::Link {
            target: "/h/.claude/skills".into(),
            dangling: false,
        },
    );
    assert!(!verify(&snap(ok), &["nope".into()]).pass);
}

/// 一个号目录的顶层结构（只有名字与类型，几个号的并集）：`l` 链回共享库 · `f` 文件 · `d` 目录。
const ACCOUNT_DIR_SHAPE: &[(&str, char)] = &[
    (".claude.json", 'f'),
    (".credentials.json", 'f'),
    (".last-cleanup", 'f'),
    (".last-update-result.json", 'f'),
    ("CLAUDE.md", 'l'),
    ("backups", 'd'),
    ("cache", 'l'),
    ("downloads", 'l'),
    ("feedback", 'd'),
    ("file-history", 'l'),
    ("history.jsonl", 'l'),
    ("jobs", 'l'),
    ("mcp-needs-auth-cache.json", 'f'),
    ("paste-cache", 'l'),
    ("plugins", 'l'),
    ("policy-limits.json", 'f'),
    ("policy-limits.json.stamp.json", 'f'),
    ("projects", 'l'),
    ("remote-settings.json", 'f'),
    ("session-env", 'l'),
    ("sessions", 'l'),
    ("settings.json", 'l'),
    ("shell-snapshots", 'l'),
    ("skills", 'l'),
    ("stats-cache.json", 'f'),
    ("state", 'd'),
];

fn own_file(mode: u32) -> Item {
    Item::File {
        mode: Some(mode),
        size: 2,
        mtime: None,
    }
}

/// 共享库里有每一项链接的源头，**也有**每一份非身份本体的每号文件（共享库自己那份，最容易被当成「该共享」的那一形）；
/// 号 z 的目录照 [`ACCOUNT_DIR_SHAPE`] 摆（`extra` 再多放几份自己的文件，共享库里也放一份）。
fn shaped(extra: &[&str]) -> Snapshot {
    let mut s = snap(BTreeMap::new());
    let (mut shared, mut z) = (BTreeMap::new(), BTreeMap::new());
    let secret = |n: &str| identity().iter().any(|(i, _, sec)| *i == n && *sec);
    for (name, kind) in ACCOUNT_DIR_SHAPE {
        let it = match kind {
            'l' => Item::Link {
                target: format!("/h/.claude/{name}"),
                dangling: false,
            },
            'd' => Item::Dir { mode: Some(0o700) },
            _ => own_file(0o600),
        };
        if *kind != 'l' && secret(name) {
            z.insert(name.to_string(), it);
            continue;
        }
        let src = if *kind == 'd' || *kind == 'l' {
            Item::Dir { mode: Some(0o755) }
        } else {
            own_file(0o644)
        };
        shared.insert(name.to_string(), src);
        z.insert(name.to_string(), it);
    }
    for e in extra {
        shared.insert(e.to_string(), own_file(0o644));
        z.insert(e.to_string(), own_file(0o600));
    }
    s.shared.entries = shared;
    s.dirs.get_mut("/h/.cc-monitor/accounts/z").unwrap().entries = z;
    s
}

#[test]
fn every_per_account_file_in_an_account_dir_is_on_the_list_and_never_called_a_missing_link() {
    // ① 每号清单 == 号目录里不是链接的那几项（两向）。
    let mut want: Vec<&str> = ACCOUNT_DIR_SHAPE
        .iter()
        .filter(|(_, k)| *k != 'l')
        .map(|(n, _)| *n)
        .collect();
    want.sort_unstable();
    let mut got: Vec<&str> = identity().iter().map(|(n, _, _)| *n).collect();
    got.sort_unstable();
    assert_eq!(got, want);

    // ② 共享库里也有一份时，核对不说「应当是共享链接」，也不提示别的；正控：多一份不在清单上的自己的文件 ⇒ 恰好那一条致命。
    let r = verify(&shaped(&[]), &[]);
    let loud: Vec<&VerifyCheck> = r
        .checks
        .iter()
        .filter(|c| matches!(c.level, CheckLevel::Fail | CheckLevel::Warn))
        .collect();
    assert!(loud.is_empty(), "{loud:?}");
    let r = verify(&shaped(&["notes.txt"]), &[]);
    assert_eq!(r.fails, 1, "{:?}", r.checks);
    assert!(fails(&r)[0].contains("notes.txt"), "{:?}", fails(&r));

    // ③ 修复只补链接那几项：号目录空着时，计划里新建的链接 == 形状里的链接（每号那几份一份都不链）。
    let mut empty = shaped(&[]);
    empty
        .dirs
        .get_mut("/h/.cc-monitor/accounts/z")
        .unwrap()
        .entries
        .clear();
    let plan = crate::accounts::manage::layout::plan_repair(&empty).unwrap();
    let mut linked: Vec<String> = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            crate::accounts::manage::layout::Op::Link { at, .. } => {
                at.rsplit('/').next().map(str::to_string)
            }
            _ => None,
        })
        .collect();
    linked.sort_unstable();
    let mut links: Vec<String> = ACCOUNT_DIR_SHAPE
        .iter()
        .filter(|(_, k)| *k == 'l')
        .map(|(n, _)| n.to_string())
        .collect();
    links.sort_unstable();
    assert_eq!(linked, links);
}
