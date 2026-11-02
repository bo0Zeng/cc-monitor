//! 历史清单的行与几台合好的搜索行上那两格（「打开目录」出不出 · 删除确认的正文）的判据。
//!
//! 1. 目录在问的这一端、目录非空才打得开；那一台的、目录空的都不出。
//! 2. 正文：这台的不点名机器，那一台的点名；说不清在不在跑 ⇒ 换行再接一句；在跑 · 能删两形只说头一句（删的那一刻界面再核在跑）。

use super::*;
use cmd_wire::listing::CanDelete;
use cmd_wire::Machine;
use copy_core::copy_text;

#[test]
fn the_folder_opens_only_for_this_machines_rows_with_a_folder() {
    assert!(reveal_of(&Machine::local(), "/w/p"));
    assert!(!reveal_of(&Machine::local(), ""), "目录空 ⇒ 没有可打开的");
    assert!(
        !reveal_of(&Machine::named("pi"), "/w/p"),
        "那一台的目录不在这台"
    );
}

#[test]
fn deleting_names_the_machine_only_when_it_is_another_and_says_when_life_is_unclear() {
    let body = copy_text("history.delete.body", &[]);
    let remote = copy_text("history.delete.bodyRemote", &[("machine", "pi")]);
    let unsure = copy_text("history.delete.unsure", &[]);
    let here = Machine::local();
    let pi = Machine::named("pi");
    assert_eq!(delete_ask(&here, CanDelete::Yes).0, body);
    assert_eq!(delete_ask(&here, CanDelete::Live).0, body);
    assert_eq!(
        delete_ask(&here, CanDelete::Unsure).0,
        format!("{body}\n{unsure}")
    );
    assert_eq!(delete_ask(&pi, CanDelete::Yes).0, remote);
    assert_eq!(
        delete_ask(&pi, CanDelete::Unsure).0,
        format!("{remote}\n{unsure}")
    );
}
