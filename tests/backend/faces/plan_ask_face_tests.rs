//! 问图（`plan-ask`）：开口那句由这台拼（别名合下来的 `say` ＋「看 编号「标题」：」）· 起在哪那一句（`runs`）也由这台照别名合下来的写 · 给了别名就记成这台问图用的 ·
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
        "[base]\naccount = \"lab\"\n\n[look]\nfrom = \"base\"\nccm-tmux = true\nsay = \"{SAY}\"\ntab-group = \"看图\"\n\n[bare]\nccm-tmux = true\n\n[plain]\nbase = true\n"
    ))
}

/// 「起在哪」那一句里的几段，照这台的文案表连起来（期望逐段手写）。
fn runs_of(parts: &[String]) -> String {
    parts.join(&copy_core::copy_text("bePlan.ask.runsSep", &[]))
}

fn tmux_part() -> String {
    copy_core::copy_text("bePlan.ask.runsTmux", &[])
}

fn cell_part() -> String {
    copy_core::copy_text("bePlan.ask.line", &[("id", "A1-1"), ("title", "甲的读入")])
}

/// 问一次（这台有 tmux）：交回回包与记下的别名。
fn ask(args: Value, kept: Option<&str>) -> (Result<Value, Said>, Vec<String>) {
    ask_on(args, kept, Some(true))
}

/// 问一次，这台有没有 tmux 由判据给。
fn ask_on(
    args: Value,
    kept: Option<&str>,
    tmux: Option<bool>,
) -> (Result<Value, Said>, Vec<String>) {
    let got = RefCell::new(Vec::new());
    let keep = |a: &str| {
        got.borrow_mut().push(a.to_string());
        Ok(())
    };
    let req: PlanAskReq = serde_json::from_value(args).expect("入参解得出");
    let out = ask_with(&req, &doc(), &book(), kept, tmux, &keep).map(|r| as_json(&r));
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
    assert_eq!(
        out["title"],
        copy_core::copy_text("bePlan.ask.title", &[("title", "甲的读入")]),
        "框顶那一句也由这台写：{out}"
    );
    assert_eq!(kept, vec!["look".to_string()], "选了的这条记成这台问图用的");
}

#[test]
fn a_tmuxless_machine_refuses_with_no_tmux() {
    let (out, kept) = ask_on(
        json!({"workspace": "/w", "slice": "alpha", "id": "A1-1", "alias": "look"}),
        None,
        Some(false),
    );
    let e = out.expect_err("没有 tmux 要拒（开口那句只跟 tmux）");
    assert_eq!(e.code.as_deref(), Some(crate::NO_TMUX), "{e:?}");
    assert!(kept.is_empty(), "拒了就不记：{kept:?}");
    let (out, _) = ask_on(
        json!({"workspace": "/w", "slice": "alpha", "id": "A1-1"}),
        None,
        None,
    );
    assert!(out.is_ok(), "探不出 tmux ⇒ 照做：{out:?}");
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
    let (out, _) = ask(
        json!({"workspace": "/w", "slice": "alpha", "id": "A1-1"}),
        None,
    );
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
    assert_eq!(e.code.as_deref(), Some("no_such_alias"), "{e:?}");
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
        assert_eq!(e.code.as_deref(), Some("no_such_cell"), "{slice} {id}: {e:?}");
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

#[test]
fn runs_says_where_it_starts_from_what_the_alias_resolves_to() {
    let (out, _) = ask(
        json!({"workspace": "/w", "slice": "alpha", "id": "A1-1", "alias": "look"}),
        None,
    );
    let out = out.expect("问得出");
    let want = runs_of(&[
        tmux_part(),
        copy_core::copy_text(
            "bePlan.ask.runsAccount",
            &[("account", "lab"), ("from", "base")],
        ),
        copy_core::copy_text("bePlan.ask.runsGroup", &[("group", "看图")]),
    ]);
    assert_eq!(
        out["runs"], want,
        "号来自基于的那一层 · 进分组照别名写的：{out}"
    );
    let runs = out["runs"].as_str().unwrap_or_default();
    for part in ["lab", "base", "看图"] {
        assert!(runs.contains(part), "那一句「{runs}」里没有「{part}」");
    }
}

#[test]
fn runs_leaves_out_what_the_alias_does_not_say_and_names_base() {
    let (bare, _) = ask(
        json!({"workspace": "/w", "slice": "alpha", "id": "A1-1", "alias": "bare"}),
        None,
    );
    assert_eq!(
        bare.expect("问得出")["runs"],
        tmux_part(),
        "没写号 · 没写分组 ⇒ 只剩 tmux 里"
    );
    let (plain, _) = ask(
        json!({"workspace": "/w", "slice": "alpha", "id": "A1-1", "alias": "plain"}),
        None,
    );
    assert_eq!(
        plain.expect("问得出")["runs"],
        runs_of(&[
            tmux_part(),
            copy_core::copy_text("bePlan.ask.runsBase", &[("from", "plain")]),
        ]),
    );
}

#[test]
fn no_alias_means_no_runs() {
    let (out, _) = ask(
        json!({"workspace": "/w", "slice": "alpha", "id": "A1-1"}),
        None,
    );
    assert_eq!(
        out.expect("问得出")["runs"],
        Value::Null,
        "没选别名 ⇒ runs 为 null"
    );
    let (kept, _) = ask(
        json!({"workspace": "/w", "slice": "alpha", "id": "A1-1"}),
        Some("look"),
    );
    assert!(
        kept.expect("问得出")["runs"].is_string(),
        "记着的那条预选上了 ⇒ 也写 runs"
    );
}
