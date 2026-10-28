//! 问图（`plan-ask`）：开口那句由这台拼（别名合下来的 `say` ＋「看 编号「标题」这一格：」）· 给了别名就记成这台问图用的 ·
//! 没给就用记着的那条（那条没了 ⇒ 不预选）· 别名不在 ⇒ 拒、不记 · 格不在 ⇒ 拒。夹具是合成的中性例子。

use super::*;
use crate::assets::aliases::profile::parse_book;
use crate::plan::fixture::{as_json, dump, who};
use serde_json::{json, Value};
use std::cell::RefCell;

const SAY: &str = "读计划图回答问题";

fn doc() -> PlanRead {
    let b = crate::plan::book::Book::default();
    let d = dump("/w");
    let raw = d.to_string().into_bytes();
    b.ingest(
        crate::plan::dump::Ran::Dump { doc: d, raw },
        std::path::Path::new("/w"),
        &who,
        1,
    )
    .unwrap()
}

fn book() -> crate::assets::aliases::profile::Book {
    parse_book(&format!(
        "[look]\nccm-tmux = true\nsay = \"{SAY}\"\n\n[bare]\nccm-tmux = true\n"
    ))
}

fn cell_part() -> String {
    copy_core::copy_text("bePlan.ask.line", &[("id", "A1-1"), ("title", "甲的读入")])
}

/// 问一次：交回回包与记下的别名。
fn ask(args: Value, kept: Option<&str>) -> (Result<Value, Fail>, Vec<String>) {
    let got = RefCell::new(Vec::new());
    let keep = |a: &str| {
        got.borrow_mut().push(a.to_string());
        Ok(())
    };
    let req: PlanAskReq = serde_json::from_value(args).expect("入参解得出");
    let out = ask_with(&req, &doc(), &book(), kept, &keep).map(|r| as_json(&r));
    (out, got.into_inner())
}

#[test]
fn a_chosen_alias_says_its_line_then_the_cell_and_is_kept() {
    let (out, kept) = ask(
        json!({"workspace": "/w", "slice": "alpha", "id": "A1-1", "alias": "look"}),
        None,
    );
    let out = out.expect("问得出");
    assert_eq!(out["alias"], "look", "{out}");
    assert_eq!(out["line"], format!("{SAY}\n{}", cell_part()), "{out}");
    assert_eq!(kept, vec!["look".to_string()], "选了的这条记成这台问图用的");
}

#[test]
fn no_alias_given_uses_the_kept_one_without_rewriting_it() {
    let (out, kept) = ask(
        json!({"workspace": "/w", "slice": "alpha", "id": "A1-1"}),
        Some("look"),
    );
    let out = out.expect("问得出");
    assert_eq!(out["alias"], "look", "{out}");
    assert_eq!(out["line"], format!("{SAY}\n{}", cell_part()), "{out}");
    assert!(kept.is_empty(), "没改选就不写：{kept:?}");
}

#[test]
fn an_alias_without_say_gives_only_the_cell_part() {
    let (out, _) = ask(
        json!({"workspace": "/w", "slice": "alpha", "id": "A1-1", "alias": "bare"}),
        None,
    );
    assert_eq!(out.expect("问得出")["line"], cell_part());
}

#[test]
fn a_kept_alias_that_is_gone_preselects_nothing() {
    let (out, kept) = ask(
        json!({"workspace": "/w", "slice": "alpha", "id": "A1-1"}),
        Some("removed"),
    );
    let out = out.expect("问得出");
    assert_eq!(out["alias"], Value::Null, "{out}");
    assert_eq!(out["line"], cell_part(), "{out}");
    assert!(kept.is_empty());
}

#[test]
fn nothing_kept_preselects_nothing() {
    let (out, _) = ask(json!({"workspace": "/w", "slice": "alpha", "id": "A1-1"}), None);
    let out = out.expect("问得出");
    assert_eq!(out["alias"], Value::Null, "{out}");
    assert_eq!(out["line"], cell_part(), "{out}");
}

#[test]
fn an_unknown_alias_is_refused_and_not_kept() {
    let (out, kept) = ask(
        json!({"workspace": "/w", "slice": "alpha", "id": "A1-1", "alias": "nope"}),
        None,
    );
    let e = out.expect_err("别名不在要拒");
    assert_eq!(e.code, "no_such_alias", "{e:?}");
    assert!(kept.is_empty(), "拒了就不记：{kept:?}");
}

#[test]
fn an_unknown_cell_is_refused() {
    for (slice, id) in [("alpha", "A99"), ("nosuch", "A1-1")] {
        let (out, kept) = ask(
            json!({"workspace": "/w", "slice": slice, "id": id, "alias": "look"}),
            None,
        );
        let e = out.expect_err("格不在要拒");
        assert_eq!(e.code, "no_such_cell", "{slice} {id}: {e:?}");
        assert!(kept.is_empty());
    }
}

#[test]
fn the_kept_alias_round_trips_through_the_review_file() {
    let r: crate::plan::review::Review =
        serde_json::from_value(json!({"ask": "look"})).expect("记着的那条读得回来");
    assert_eq!(r.ask(), Some("look"));
    assert_eq!(serde_json::to_value(&r).unwrap()["ask"], "look");
    assert_eq!(crate::plan::review::Review::default().ask(), None);
}
