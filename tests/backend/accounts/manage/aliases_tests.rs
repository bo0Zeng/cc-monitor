//! 账号的两段配置：名字怎么起 · 建号加两段（基于 `cc` / `cct`、只写自己的号；同一形已有就不加、名字被占就跳过）·
//! 删号删掉合下来用它的全部（基于被删那一段的改指向）· 账号那一形怎么认。

use super::*;
use crate::assets::aliases::profile::parse_book;

fn exact(a: &str, b: &str) -> bool {
    a == b
}

fn set(name: &str, from: Option<&str>, ccm: &[&str]) -> Change {
    Change::Set(ProfileEdit {
        name: name.to_string(),
        from: from.map(str::to_string),
        agent: Vec::new(),
        ccm: ccm.iter().map(|s| s.to_string()).collect(),
    })
}

const BASE: &str = "[cc]\ncwd-if = [\"~\", \"/w\"]\n[cct]\nfrom = \"cc\"\nccm-tmux = true\n";

#[test]
fn alias_names_follow_the_shell_function_rules() {
    assert_eq!(alias_name("work", false).as_deref(), Some("workcc"));
    assert_eq!(alias_name("work", true).as_deref(), Some("workcct"));
    assert_eq!(alias_name("side-2", false).as_deref(), Some("side2cc"));
    assert_eq!(alias_name("a_b", true).as_deref(), Some("a_bcct"));
    assert_eq!(alias_name("2x", false).as_deref(), Some("_2xcc"));
    assert_eq!(alias_name("---", true), None);
}

#[test]
fn adding_an_account_bases_its_two_on_cc_and_cct_and_writes_only_the_account() {
    let b = parse_book(BASE);
    let r = plan_add(&b, "team", true, &exact);
    assert_eq!(
        r.changes,
        vec![
            set("teamcc", Some("cc"), &["--account", "team"]),
            set("teamcct", Some("cct"), &["--account", "team"]),
        ]
    );
    assert_eq!(r.added, ["teamcc", "teamcct"]);
    assert!(r.skipped.is_empty());
    // 没有 tmux 的目标只加 `<号>cc`。
    assert_eq!(plan_add(&b, "team", false, &exact).added, ["teamcc"]);
    // 没有 cc / cct 那两段 ⇒ 不基于谁，tmux 那一段自己写上 tmux。
    let r = plan_add(&parse_book(""), "team", true, &exact);
    assert_eq!(
        r.changes,
        vec![
            set("teamcc", None, &["--account", "team"]),
            set("teamcct", None, &["--account", "team", "--ccm-tmux"]),
        ]
    );
}

#[test]
fn an_existing_profile_of_the_same_shape_counts_whatever_its_name() {
    let b = parse_book(&format!(
        "{BASE}[bee]\nfrom = \"cct\"\naccount = \"team\"\n"
    ));
    let r = plan_add(&b, "team", true, &exact);
    assert_eq!(r.added, ["teamcc"], "bee 已经是「team 号 ＋ tmux」");
    assert!(r.skipped.is_empty());
}

#[test]
fn a_name_held_by_another_profile_is_skipped_not_overwritten() {
    let b = parse_book(&format!("{BASE}[workcct]\ncwd = \"/x\"\n"));
    let r = plan_add(&b, "work", true, &exact);
    assert_eq!(r.added, ["workcc"]);
    assert_eq!(r.skipped, ["workcct"]);
}

#[test]
fn removing_an_account_drops_every_profile_that_ends_up_using_it_and_repoints_the_rest() {
    let b = parse_book(&format!(
        "{BASE}[workcc]\nfrom = \"cc\"\naccount = \"work\"\n[workt]\nfrom = \"workcc\"\nccm-tmux = true\n\
         [other]\nfrom = \"workcc\"\naccount = \"y\"\n[yy]\naccount = \"y\"\n"
    ));
    let r = plan_remove(&b, "work");
    assert_eq!(
        r.removed,
        ["workcc", "workt"],
        "workt 基于 workcc，合下来也是 work 号"
    );
    assert!(r.changes.contains(&Change::Remove("workcc".into())));
    assert!(
        r.changes
            .contains(&set("other", Some("cc"), &["--account", "y"])),
        "other 换了号、留着，改成基于 workcc 基于的 cc：{:?}",
        r.changes
    );
    assert!(!r
        .changes
        .iter()
        .any(|c| matches!(c, Change::Set(e) if e.name == "yy")));
}

#[test]
fn the_account_shape_is_own_account_plus_maybe_tmux_judged_on_the_merged_result() {
    let b = parse_book(&format!(
        "{BASE}[workcc]\nfrom = \"cc\"\naccount = \"work\"\n[workcct]\nfrom = \"cct\"\naccount = \"work\"\n\
         [mixed]\naccount = \"work\"\ncwd = \"/x\"\n[talk]\naccount = \"work\"\nargs = [\"--model\", \"x\"]\n"
    ));
    let shape = |n: &str| shape_of(&b, b.find(n).unwrap());
    assert_eq!(shape("workcc"), Some(("work".into(), false)));
    assert_eq!(shape("workcct"), Some(("work".into(), true)));
    assert_eq!(shape("mixed"), None);
    assert_eq!(shape("talk"), None);
    assert_eq!(shape("cct"), None);
}
