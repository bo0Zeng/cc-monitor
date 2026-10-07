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
    assert_eq!(alias_name("z", false).as_deref(), Some("alphacc"));
    assert_eq!(alias_name("z", true).as_deref(), Some("alphacct"));
    assert_eq!(alias_name("side-2", false).as_deref(), Some("side2cc"));
    assert_eq!(alias_name("a_b", true).as_deref(), Some("a_bcct"));
    assert_eq!(alias_name("2x", false).as_deref(), Some("_2xcc"));
    assert_eq!(alias_name("---", true), None);
}

#[test]
fn adding_an_account_bases_its_two_on_cc_and_cct_and_writes_only_the_account() {
    let b = parse_book(BASE);
    let r = plan_add(&b, "b", true, &exact);
    assert_eq!(
        r.changes,
        vec![
            set("betacc", Some("cc"), &["--account", "b"]),
            set("betacct", Some("cct"), &["--account", "b"]),
        ]
    );
    assert_eq!(r.added, ["betacc", "betacct"]);
    assert!(r.skipped.is_empty());
    // 没有 tmux 的目标只加 `<号>cc`。
    assert_eq!(plan_add(&b, "b", false, &exact).added, ["betacc"]);
    // 没有 cc / cct 那两段 ⇒ 不基于谁，tmux 那一段自己写上 tmux。
    let r = plan_add(&parse_book(""), "b", true, &exact);
    assert_eq!(
        r.changes,
        vec![
            set("betacc", None, &["--account", "b"]),
            set("betacct", None, &["--account", "b", "--ccm-tmux"]),
        ]
    );
}

#[test]
fn an_existing_profile_of_the_same_shape_counts_whatever_its_name() {
    let b = parse_book(&format!("{BASE}[bee]\nfrom = \"cct\"\naccount = \"b\"\n"));
    let r = plan_add(&b, "b", true, &exact);
    assert_eq!(r.added, ["betacc"], "bee 已经是「b 号 ＋ tmux」");
    assert!(r.skipped.is_empty());
}

#[test]
fn a_name_held_by_another_profile_is_skipped_not_overwritten() {
    let b = parse_book(&format!("{BASE}[alphacct]\ncwd = \"/x\"\n"));
    let r = plan_add(&b, "z", true, &exact);
    assert_eq!(r.added, ["alphacc"]);
    assert_eq!(r.skipped, ["alphacct"]);
}

#[test]
fn removing_an_account_drops_every_profile_that_ends_up_using_it_and_repoints_the_rest() {
    let b = parse_book(&format!(
        "{BASE}[alphacc]\nfrom = \"cc\"\naccount = \"z\"\n[zt]\nfrom = \"alphacc\"\nccm-tmux = true\n\
         [other]\nfrom = \"alphacc\"\naccount = \"y\"\n[yy]\naccount = \"y\"\n"
    ));
    let r = plan_remove(&b, "z");
    assert_eq!(r.removed, ["alphacc", "zt"], "zt 基于 alphacc，合下来也是 z 号");
    assert!(r.changes.contains(&Change::Remove("alphacc".into())));
    assert!(
        r.changes
            .contains(&set("other", Some("cc"), &["--account", "y"])),
        "other 换了号、留着，改成基于 alphacc 基于的 cc：{:?}",
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
        "{BASE}[alphacc]\nfrom = \"cc\"\naccount = \"z\"\n[alphacct]\nfrom = \"cct\"\naccount = \"z\"\n\
         [mixed]\naccount = \"z\"\ncwd = \"/x\"\n[talk]\naccount = \"z\"\nargs = [\"--model\", \"x\"]\n"
    ));
    let shape = |n: &str| shape_of(&b, b.find(n).unwrap());
    assert_eq!(shape("alphacc"), Some(("z".into(), false)));
    assert_eq!(shape("alphacct"), Some(("z".into(), true)));
    assert_eq!(shape("mixed"), None);
    assert_eq!(shape("talk"), None);
    assert_eq!(shape("cct"), None);
}
