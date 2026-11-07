//! 账号的三段配置：名字怎么起 · 建号加三段（基于 `cc` / `cct` / `cch`、只写自己的号；同一形已有就不加、名字被占就跳过）·
//! 删号删掉合下来用它的全部（基于被删那一段的改指向）· 账号那一形怎么认。

use super::super::profile::parse_book;
use super::*;

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

const BASE: &str = "[cc]\ncwd-if = [\"~\", \"/w\"]\n[cct]\nfrom = \"cc\"\nccm-tmux = true\n[cch]\nfrom = \"cc\"\nhosted = true\n";

#[test]
fn adding_an_account_bases_its_three_on_cc_cct_cch_and_writes_only_the_account() {
    let b = parse_book(BASE);
    let r = plan_add(&b, "team", true, &exact);
    assert_eq!(
        r.changes,
        vec![
            set("teamcc", Some("cc"), &["--account", "team"]),
            set("teamcct", Some("cct"), &["--account", "team"]),
            set("teamcch", Some("cch"), &["--account", "team"]),
        ]
    );
    assert_eq!(r.added, ["teamcc", "teamcct", "teamcch"]);
    assert!(r.skipped.is_empty());
    // 没有 tmux 的目标不加 `<号>cct`。
    assert_eq!(
        plan_add(&b, "team", false, &exact).added,
        ["teamcc", "teamcch"]
    );
    // 没有 cct / cch 那一段 ⇒ 基于 cc，自己写上那一形的词；连 cc 都没有 ⇒ 不基于谁。
    let r = plan_add(&parse_book("[cc]\n"), "team", true, &exact);
    assert_eq!(
        r.changes,
        vec![
            set("teamcc", Some("cc"), &["--account", "team"]),
            set("teamcct", Some("cc"), &["--account", "team", "--ccm-tmux"]),
            set("teamcch", Some("cc"), &["--account", "team", "--hosted"]),
        ]
    );
    let r = plan_add(&parse_book(""), "team", true, &exact);
    assert_eq!(
        r.changes,
        vec![
            set("teamcc", None, &["--account", "team"]),
            set("teamcct", None, &["--account", "team", "--ccm-tmux"]),
            set("teamcch", None, &["--account", "team", "--hosted"]),
        ]
    );
}

#[test]
fn an_existing_profile_of_the_same_shape_counts_whatever_its_name() {
    let b = parse_book(&format!(
        "{BASE}[bee]\nfrom = \"cct\"\naccount = \"team\"\n"
    ));
    let r = plan_add(&b, "team", true, &exact);
    assert_eq!(
        r.added,
        ["teamcc", "teamcch"],
        "bee 已经是「team 号 ＋ tmux」"
    );
    assert!(r.skipped.is_empty());
}

#[test]
fn a_name_held_by_another_profile_is_skipped_not_overwritten() {
    let b = parse_book(&format!("{BASE}[workcct]\ncwd = \"/x\"\n"));
    let r = plan_add(&b, "work", true, &exact);
    assert_eq!(r.added, ["workcc", "workcch"]);
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
fn the_account_form_is_own_account_plus_maybe_tmux_or_hosted_judged_on_the_merged_result() {
    let b = parse_book(&format!(
        "{BASE}[workcc]\nfrom = \"cc\"\naccount = \"work\"\n[workcct]\nfrom = \"cct\"\naccount = \"work\"\n\
         [workcch]\nfrom = \"cch\"\naccount = \"work\"\n[own]\naccount = \"work\"\nhosted = true\n\
         [mixed]\naccount = \"work\"\ncwd = \"/x\"\n[talk]\naccount = \"work\"\nargs = [\"--model\", \"x\"]\n"
    ));
    let form = |n: &str| form_of(&b, b.find(n).unwrap());
    assert_eq!(form("workcc"), Some(("work".into(), Form::Plain)));
    assert_eq!(form("workcct"), Some(("work".into(), Form::Tmux)));
    assert_eq!(form("workcch"), Some(("work".into(), Form::Hosted)));
    assert_eq!(form("own"), Some(("work".into(), Form::Hosted)));
    assert_eq!(form("mixed"), None);
    assert_eq!(form("talk"), None);
    assert_eq!(form("cct"), None);
    assert_eq!(form("cch"), None);
}

#[test]
fn removing_an_account_drops_its_hosted_one_too() {
    let b = parse_book(&format!(
        "{BASE}[workcc]\nfrom = \"cc\"\naccount = \"work\"\n[workcch]\nfrom = \"cch\"\naccount = \"work\"\n"
    ));
    assert_eq!(plan_remove(&b, "work").removed, ["workcc", "workcch"]);
}
