//! 起新会话框要的这台事实：最近用过的目录怎么排 · 这台能起哪几家。

use super::*;

#[test]
fn recent_dirs_are_newest_first_once_each_and_skip_the_hidden_one() {
    let items: Vec<(String, i64)> = vec![
        ("/a".into(), 10),
        ("/b".into(), 30),
        ("/a".into(), 40),
        ("".into(), 99),
        ("/hidden".into(), 100),
    ];
    let got = recent_dirs(&items, &|c| c == "/hidden");
    assert_eq!(
        got,
        vec![
            json!({ "cwd": "/a", "lastMs": 40 }),
            json!({ "cwd": "/b", "lastMs": 30 })
        ]
    );
    let many: Vec<(String, i64)> = (0..20).map(|i| (format!("/d{i}"), i)).collect();
    let got = recent_dirs(&many, &|_| false);
    assert_eq!(got.len(), RECENT_MAX);
    assert_eq!(got[0]["cwd"], "/d19");
}

#[test]
fn only_the_families_whose_launcher_this_machine_finds_are_offered() {
    let all = crate::agents::launchable_kinds();
    assert!(all.len() > 1, "注册表里由我们起的不止一家");
    assert_eq!(launchable_here(&|_| true), all);
    assert!(launchable_here(&|_| false).is_empty());
    let first = all[0];
    let launcher = crate::agents::pick_kind(Some(first))
        .unwrap()
        .1
        .default_launcher;
    assert_eq!(launchable_here(&|l| l == launcher), vec![first]);
}
