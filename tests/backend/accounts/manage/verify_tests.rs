//! 核对是纯的：同一份手搭的快照，改一格就该多一条致命。
use super::*;
use crate::accounts::manage::scan::{DirState, Roots};
use std::collections::BTreeMap;

fn snap(z: BTreeMap<String, Item>) -> Snapshot {
    let text = r#"{"version":1,"sharedStore":"/h/.claude","accounts":[{"name":"z","email":"","configDir":"/h/.claude-accts/z","isDefault":true}]}"#;
    let mut dirs = BTreeMap::new();
    dirs.insert(
        "/h/.claude-accts/z".to_string(),
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
            accts: "/h/.claude-accts".into(),
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
    assert!(fails(&r)[0].contains("/h/.claude-accts/accounts.json"));
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
