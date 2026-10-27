//! 每块的接手会话在哪几组：组模块那一份照 sid 对（子 agent 接手 ⇒ 父会话）· 口子回错 ⇒ 「读取失败」带那一句，不是空 ·
//! 没有接手会话的块不问组模块 · 那一格的字后端写好。

use super::*;
use crate::history::groups::{merge, unlisted, GroupsHere, Half, HereGroup, HereMember};
use crate::plan::fixture::{dump, made_with, who, MAIN};
use cmd_wire::listing::RowLife;
use cmd_wire::Machine;
use copy_core::copy_text;
use std::cell::Cell;

/// 这台那一半：g1「开发」· g3「评审」里有 MAIN；g2「杂项」里只有别人。
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
    let group = |id: &str, name: &str, made: i64| HereGroup {
        id: id.into(),
        name: name.into(),
        made,
        named: 1,
    };
    let here = GroupsHere {
        groups: vec![
            group("g1", "开发", 1),
            group("g2", "杂项", 2),
            group("g3", "评审", 3),
        ],
        members: vec![member(MAIN, &["g1", "g3"]), member("someone-else", &["g2"])],
    };
    merge(
        &[(Machine(cmd_wire::LOCAL.into()), Half::Now(here))],
        Vec::new(),
    )
}

fn block<'a>(doc: &'a PlanRead, slice: &str, id: &str) -> &'a PlanBlock {
    doc.slices
        .iter()
        .find(|s| s.name == slice)
        .and_then(|s| s.blocks.iter().find(|b| b.id == id))
        .unwrap_or_else(|| panic!("没有 {slice}/{id}"))
}

fn none() -> String {
    copy_text("plan.page.none", &[])
}

#[test]
fn 口子回错_有接手会话的块是读取失败_不是空() {
    let mut doc = made_with(&dump("/w"), &who);
    annotate(&mut doc, &|| Err("注解文件无法解析".into()));
    let g = &block(&doc, "alpha", "project").groups;
    assert_eq!(
        g.unread.as_deref(),
        Some(copy_text("bePlan.groups.unread", &[("why", "注解文件无法解析")]).as_str())
    );
    assert!(g.groups.is_empty());
    // 没有接手人的那一块不牵扯组模块：照「没有接手会话」，不说读取失败。
    assert_eq!(block(&doc, "beta", "project").groups, nobody());
    assert_eq!(nobody().unread, None);
    assert_eq!(nobody().text, none());
}

#[test]
fn 接手会话在哪几组_子agent照父会话_字照排() {
    let mut doc = made_with(&dump("/w"), &who);
    annotate(&mut doc, &|| Ok(ties()));
    let g = &block(&doc, "alpha", "project").groups;
    let ids: Vec<&str> = g.groups.iter().map(|x| x.id.as_str()).collect();
    assert_eq!(ids, ["g1", "g3"]);
    assert_eq!(
        g.text,
        format!("开发{}评审", copy_text("kit.text.sep", &[]))
    );
    assert_eq!(g.unread, None);
    // A2 的负责人是 MAIN 底下的子 agent ⇒ 照 MAIN 那一条。
    assert_eq!(&block(&doc, "alpha", "A2").groups, g);
}

#[test]
fn 读到了_不在组里是一杠() {
    let mut doc = made_with(&dump("/w"), &|_: &str| crate::plan::Whose::Session {
        sid: "lonely".into(),
        live: None,
    });
    annotate(&mut doc, &|| Ok(ties()));
    let g = &block(&doc, "alpha", "project").groups;
    assert!(g.groups.is_empty());
    assert_eq!(
        (g.text.as_str(), g.unread.as_deref()),
        (none().as_str(), None)
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
    let mut nobody_doc = made_with(&dump("/w"), &|_: &str| crate::plan::Whose::Unknown);
    annotate(&mut nobody_doc, &|| {
        asked.set(asked.get() + 1);
        Err("不该问".into())
    });
    assert_eq!(asked.get(), 1);
    assert_eq!(block(&nobody_doc, "alpha", "project").groups, nobody());
}
