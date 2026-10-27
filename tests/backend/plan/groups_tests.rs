//! 每块的接手会话在哪几组：组模块那一份照 sid 对（子 agent 接手 ⇒ 父会话）· 口子回错 ⇒ 「读不到」带那一句，不是空 ·
//! 没有接手会话的块不问组模块。

use super::*;
use crate::history::groups::{merge, unlisted, GroupsHere, Half, HereGroup, HereMember};
use crate::plan::fixture::{dump, made_with, who, MAIN};
use cmd_wire::listing::RowLife;
use cmd_wire::Machine;
use std::cell::Cell;

/// 这台那一半：g1「开发」里有 MAIN；g2「杂项」里只有别人。
fn ties() -> GroupsRead {
    let member = |sid: &str, groups: &[&str]| {
        let (face, facts) = unlisted(None, RowLife::Ended);
        HereMember {
            sid: sid.into(),
            groups: groups.iter().map(|g| g.to_string()).collect(),
            pinned: false,
            face,
            facts,
        }
    };
    let group = |id: &str, name: &str| HereGroup {
        id: id.into(),
        name: name.into(),
        made: 1,
        named: 1,
    };
    let here = GroupsHere {
        groups: vec![group("g1", "开发"), group("g2", "杂项")],
        members: vec![member(MAIN, &["g1"]), member("someone-else", &["g2"])],
    };
    merge(&[(Machine(cmd_wire::LOCAL.into()), Half::Now(here))], Vec::new())
}

fn block<'a>(doc: &'a PlanRead, slice: &str, id: &str) -> &'a PlanBlock {
    doc.slices
        .iter()
        .find(|s| s.name == slice)
        .and_then(|s| s.blocks.iter().find(|b| b.id == id))
        .unwrap_or_else(|| panic!("没有 {slice}/{id}"))
}

#[test]
fn 口子回错_有接手会话的块是读不到_不是空() {
    let mut doc = made_with(&dump("/w"), &who);
    annotate(&mut doc, &|| Err("注解文件读不懂".into()));
    assert_eq!(
        block(&doc, "alpha", "project").groups,
        PlanBlockGroups::Unread {
            why: "注解文件读不懂".into()
        }
    );
    // 没有接手人的那一块不牵扯组模块：照「没有接手会话」，不说读不到。
    assert_eq!(block(&doc, "beta", "project").groups, PlanBlockGroups::None);
}

#[test]
fn 接手会话在哪几组_子agent照父会话() {
    let mut doc = made_with(&dump("/w"), &who);
    annotate(&mut doc, &|| Ok(ties()));
    let g1 = PlanBlockGroups::Read {
        groups: vec![PlanGroup {
            id: "g1".into(),
            title: "开发".into(),
        }],
    };
    assert_eq!(block(&doc, "alpha", "project").groups, g1);
    // A2 的负责人是 MAIN 底下的子 agent ⇒ 照 MAIN 那一条。
    assert_eq!(block(&doc, "alpha", "A2").groups, g1);
}

#[test]
fn 读到了_不在组里是空列() {
    let mut doc = made_with(&dump("/w"), &|_: &str| crate::plan::Whose::Session {
        sid: "lonely".into(),
        live: None,
    });
    annotate(&mut doc, &|| Ok(ties()));
    assert_eq!(
        block(&doc, "alpha", "project").groups,
        PlanBlockGroups::Read { groups: Vec::new() }
    );
}

#[test]
fn 一次只问一回_没有接手会话就不问() {
    let asked = Cell::new(0);
    let mut doc = made_with(&dump("/w"), &who);
    annotate(&mut doc, &|| {
        asked.set(asked.get() + 1);
        Ok(ties())
    });
    assert_eq!(asked.get(), 1);
    let mut nobody = made_with(&dump("/w"), &|_: &str| crate::plan::Whose::Unknown);
    annotate(&mut nobody, &|| {
        asked.set(asked.get() + 1);
        Ok(ties())
    });
    assert_eq!(asked.get(), 1);
    assert_eq!(block(&nobody, "alpha", "project").groups, PlanBlockGroups::None);
}
