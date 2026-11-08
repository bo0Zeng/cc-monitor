//! 内置那几段：三个入口（`cc` · `cct` · `cch`）＋ 每个号三条，照账号库现算；用户文件里同名那一段盖过内置；
//! 号不在了它那三条随之不在；账号那一形怎么认。

use super::super::profile::parse_book;
use super::*;

fn names(b: &Book) -> Vec<&str> {
    b.profiles.iter().map(|p| p.name.as_str()).collect()
}

fn merged(user: &str, accounts: &[&str], tmux: bool) -> Book {
    let accounts: Vec<String> = accounts.iter().map(|s| s.to_string()).collect();
    with_builtins(parse_book(user), &accounts, tmux, &|a, b| a == b)
}

#[test]
fn with_no_user_profiles_the_book_has_the_three_entries_and_three_per_account() {
    let b = merged("", &["lab"], true);
    assert_eq!(names(&b), ["cc", "cct", "cch", "labcc", "labcct", "labcch"]);
    assert!(b.profiles.iter().all(|p| p.builtin));
    let form = |n: &str| form_of(&b, b.find(n).unwrap());
    assert_eq!(form("labcc"), Some(("lab".into(), Form::Plain)));
    assert_eq!(form("labcct"), Some(("lab".into(), Form::Tmux)));
    assert_eq!(form("labcch"), Some(("lab".into(), Form::Hosted)));
    // 号的三条各基于同一形的入口。
    let from = |n: &str| b.find(n).unwrap().from.clone();
    assert_eq!(from("labcct").as_deref(), Some("cct"));
    assert_eq!(from("labcch").as_deref(), Some("cch"));
    // 没有 tmux 的那台不带 `cct` 与 `<号>cct`。
    assert_eq!(
        names(&merged("", &["lab"], false)),
        ["cc", "cch", "labcc", "labcch"]
    );
}

#[test]
fn a_user_profile_of_the_same_name_overrides_the_builtin_one() {
    let b = merged(
        "[labcc]\naccount = \"lab\"\ncwd = \"/w\"\n[mine]\nfrom = \"labcc\"\n",
        &["lab"],
        true,
    );
    let labcc = b.find("labcc").unwrap();
    assert!(!labcc.builtin, "labcc 应是用户那段");
    assert_eq!(labcc.from, None);
    assert_eq!(b.profiles.iter().filter(|p| p.name == "labcc").count(), 1);
    assert!(b.find("cc").unwrap().builtin);
    assert!(!b.find("mine").unwrap().builtin);
    // 用户改了 `cc`，基于它的号那几条跟着合下来。
    let b = merged("[cc]\ncwd = \"/w\"\n", &["lab"], true);
    assert!(!b.find("cc").unwrap().builtin);
    match profile::resolve(&b, "labcct", &[]).unwrap().parsed {
        Parsed::Opts(o) => {
            assert_eq!(o.account, "lab");
            assert!(o.use_tmux);
        }
        Parsed::Early(_) => panic!("labcct 合不下来"),
    }
    // 用户那段写错了也占着这个名字：不拿内置的顶替。
    let b = merged("[cch]\nbogus = 1\n", &[], true);
    assert!(b.find("cch").is_none_or(|p| !p.builtin));
    // 「同名」照那种 shell 的规矩：不分大小写的那种里 `[CC]` 就盖着内置的 `cc`，不另出一条。
    let b = with_builtins(parse_book("[CC]\ncwd = \"/w\"\n"), &[], true, &|a, b| {
        a.eq_ignore_ascii_case(b)
    });
    assert_eq!(names(&b), ["CC", "cct", "cch"]);
    // 基于 `cc` 的内置那几条照用户那段的名字基于它，合得下来。
    assert_eq!(b.find("cct").unwrap().from.as_deref(), Some("CC"));
    assert!(profile::resolve(&b, "cch", &[]).is_ok());
    // 整份写坏了 ⇒ 一段都不认，内置的也不出。
    assert!(merged("[[[ not toml", &["lab"], true).profiles.is_empty());
}

#[test]
fn an_account_gone_from_the_table_takes_its_three_with_it() {
    assert_eq!(
        names(&merged("", &["lab", "work"], true)),
        ["cc", "cct", "cch", "labcc", "labcct", "labcch", "workcc", "workcct", "workcch"]
    );
    assert_eq!(
        names(&merged("", &["work"], true)),
        ["cc", "cct", "cch", "workcc", "workcct", "workcch"]
    );
    // 号名里 shell 放不下的字符去掉；去完什么都不剩 ⇒ 不给它起。
    assert_eq!(names(&merged("", &["---"], true)), ["cc", "cct", "cch"]);
}

#[test]
fn the_account_form_is_own_account_plus_maybe_tmux_or_hosted_judged_on_the_merged_result() {
    let b = merged(
        "[workcc]\nfrom = \"cc\"\naccount = \"work\"\n[workcct]\nfrom = \"cct\"\naccount = \"work\"\n\
         [workcch]\nfrom = \"cch\"\naccount = \"work\"\n[own]\naccount = \"work\"\nhosted = true\n\
         [mixed]\naccount = \"work\"\ncwd = \"/x\"\n[talk]\naccount = \"work\"\nargs = [\"--model\", \"x\"]\n",
        &[],
        true,
    );
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
