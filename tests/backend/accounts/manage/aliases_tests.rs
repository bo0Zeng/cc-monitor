//! 账号的两条别名：名字怎么起 · 建号加两条（同一形已有就不加、名字被占就跳过）· 删号删掉指向它的全部 · 账号那一形怎么认。
use super::*;

fn e(name: &str, args: &[&str]) -> Entry {
    (
        name.to_string(),
        args.iter().map(|s| s.to_string()).collect(),
        RestTo::Agent,
    )
}

fn exact(a: &str, b: &str) -> bool {
    a == b
}

#[test]
fn alias_names_follow_the_shell_function_rules() {
    assert_eq!(alias_name("z", false).as_deref(), Some("zcc"));
    assert_eq!(alias_name("z", true).as_deref(), Some("zcct"));
    assert_eq!(alias_name("side-2", false).as_deref(), Some("side2cc"));
    assert_eq!(alias_name("a_b", true).as_deref(), Some("a_bcct"));
    assert_eq!(alias_name("2x", false).as_deref(), Some("_2xcc"));
    assert_eq!(alias_name("---", true), None);
    assert_eq!(alias_args("z", false), ["--", "--account", "z"]);
    assert_eq!(
        alias_args("z", true),
        ["--", "--account", "z", "--ccm-tmux"]
    );
}

#[test]
fn adding_an_account_appends_its_two_and_leaves_everything_else() {
    let cur = vec![
        e("mine", &["--", "--ccm-tmux"]),
        e("zcc", &["--", "--account", "z"]),
    ];
    let r = on_add(&cur, "b", true, &exact);
    assert_eq!(
        r.list,
        vec![
            e("mine", &["--", "--ccm-tmux"]),
            e("zcc", &["--", "--account", "z"]),
            e("bcc", &["--", "--account", "b"]),
            e("bcct", &["--", "--account", "b", "--ccm-tmux"]),
        ]
    );
    assert_eq!(r.added, ["bcc", "bcct"]);
    assert!(r.skipped.is_empty());
    // 没有 tmux 的目标只加 `<号>cc`。
    let r = on_add(&[], "b", false, &exact);
    assert_eq!(r.added, ["bcc"]);
    // 同一形已在（用户改了名）⇒ 不再加，也不算跳过。
    let renamed = vec![e("bee", &["--", "--ccm-tmux", "--account", "b"])];
    let r = on_add(&renamed, "b", true, &exact);
    assert_eq!(r.added, ["bcc"]);
    assert!(r.skipped.is_empty());
}

#[test]
fn a_name_held_by_another_alias_is_skipped_not_overwritten() {
    let cur = vec![e("zcct", &["--", "--cwd", "/x"])];
    let r = on_add(&cur, "z", true, &exact);
    assert_eq!(r.added, ["zcc"]);
    assert_eq!(r.skipped, ["zcct"]);
    assert_eq!(r.list[0], cur[0]);
    // PowerShell 认同名不分大小写：`ZCC` 占着 ⇒ `zcc` 跳过。
    let ps = vec![e("ZCC", &[])];
    let r = on_add(&ps, "z", false, &|a: &str, b: &str| {
        a.eq_ignore_ascii_case(b)
    });
    assert_eq!(r.skipped, ["zcc"]);
}

#[test]
fn removing_an_account_drops_every_alias_that_points_at_it_whatever_its_name() {
    let cur = vec![
        e("cc", &[]),
        e("zcc", &["--", "--account", "z"]),
        e("work", &["-p", "--", "--cwd", "/w", "--account", "z"]),
        e("zz", &["--", "--account", "zz"]),
        e("bcc", &["--", "--account", "b"]),
        // 交给 claude 的那一半里出现 `--account z` 不算指向。
        e("odd", &["--account", "z"]),
    ];
    let (kept, gone) = on_remove(&cur, "z");
    assert_eq!(gone, ["zcc", "work"]);
    let names: Vec<&str> = kept.iter().map(|x| x.0.as_str()).collect();
    assert_eq!(names, ["cc", "zz", "bcc", "odd"]);
}

#[test]
fn the_account_shape_is_exactly_one_account_with_or_without_tmux() {
    let sv = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let c = RestTo::Agent;
    assert_eq!(
        shape_of(&sv(&["--", "--account", "z"]), c),
        Some(("z".into(), false))
    );
    assert_eq!(
        shape_of(&sv(&["--", "--account", "z", "--ccm-tmux"]), c),
        Some(("z".into(), true))
    );
    assert_eq!(
        shape_of(&sv(&["--", "--ccm-tmux", "--account", "z"]), c),
        Some(("z".into(), true))
    );
    for not in [
        &["--", "--account", "z", "--cwd", "/x"][..],
        &["-p", "--", "--account", "z"],
        &["--", "--ccm-tmux"],
        &["--account", "z"],
        &["--", "--account"],
        &[],
    ] {
        assert_eq!(shape_of(&sv(not), c), None, "{not:?}");
    }
    assert_eq!(shape_of(&sv(&["--", "--account", "z"]), RestTo::Ccm), None);
}
