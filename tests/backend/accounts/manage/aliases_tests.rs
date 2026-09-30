//! 账号别名：名字怎么起 · 并进用户清单时只增删账号那一形。
use super::*;

fn e(name: &str, args: &[&str]) -> Entry {
    (
        name.to_string(),
        args.iter().map(|s| s.to_string()).collect(),
    )
}

#[test]
fn alias_names_follow_the_shell_function_rules() {
    assert_eq!(alias_name("z").as_deref(), Some("alphacc"));
    assert_eq!(alias_name("side-2").as_deref(), Some("side2cc"));
    assert_eq!(alias_name("a_b").as_deref(), Some("a_bcc"));
    assert_eq!(alias_name("2x").as_deref(), Some("_2xcc"));
    assert_eq!(alias_name("---"), None);
    assert_eq!(alias_args("z"), ["--", "--account", "z"]);
}

#[test]
fn reconcile_adds_missing_drops_gone_and_leaves_user_aliases() {
    let cur = vec![
        e("mine", &["--", "--ccm-tmux"]),
        e("oldcc", &["--", "--account", "old"]),
        e("alphacc", &["--", "--account", "z"]),
    ];
    let r = reconcile(&cur, &["z".into(), "b".into()]);
    assert_eq!(
        r.list,
        vec![
            e("mine", &["--", "--ccm-tmux"]),
            e("alphacc", &["--", "--account", "z"]),
            e("betacc", &["--", "--account", "b"]),
        ]
    );
    assert_eq!(r.names, ["alphacc", "betacc"]);
    assert!(r.skipped.is_empty());
    assert_eq!(
        reconcile(&r.list, &["z".into(), "b".into()]).list,
        r.list,
        "不幂等"
    );
}

#[test]
fn a_name_held_by_a_user_alias_is_skipped_not_overwritten() {
    let cur = vec![e("alphacc", &["--", "--ccm-tmux"])];
    let r = reconcile(&cur, &["z".into()]);
    assert_eq!(r.list, cur);
    assert!(r.names.is_empty());
    assert_eq!(r.skipped, vec![("z".to_string(), "alphacc".to_string())]);
    // 两个号推出同一个名字：第二个跳过。
    let r2 = reconcile(&[], &["ab".into(), "a-b".into()]);
    assert_eq!(r2.names, ["abcc"]);
    assert_eq!(r2.skipped, vec![("a-b".to_string(), "abcc".to_string())]);
}
