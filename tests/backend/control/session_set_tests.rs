//! 补足一组（[`plan_rows`]）：每行数「这台上 · 目录就是这一行的 · 用这一行的别名起的 · 还活着」的，只起差的几个。

use super::*;
use crate::assets::aliases::profile::{parse_book, RowEdit};

const BOOK: &str = "[dev]\nccm-tmux = true\n\n[look]\naccount = \"b\"\n\n[bad]\nfrom = \"nope\"\n";

fn row(cwd: &str, alias: &str, n: u8) -> RowEdit {
    RowEdit {
        cwd: cwd.into(),
        alias: alias.into(),
        n,
        tab_group: None,
    }
}

fn live(sid: &str, cwd: &str, alias: Option<&str>) -> Live {
    Live {
        sid: sid.into(),
        cwd: cwd.into(),
        alias: alias.map(str::to_string),
    }
}

fn home() -> Option<&'static std::path::Path> {
    Some(std::path::Path::new("/home/pi"))
}

fn every_dir(_: &str) -> bool {
    true
}

#[test]
fn a_row_with_two_alive_asking_three_starts_one_more() {
    let book = parse_book(BOOK);
    let p = plan_rows(
        &[row("~/work/ledger", "dev", 3)],
        &book,
        &[
            live("s1", "/home/pi/work/ledger", Some("dev")),
            live("s2", "/home/pi/work/ledger/", Some("dev")),
            // 别的别名 · 别的目录 · 不经别名起的：都不算。
            live("s3", "/home/pi/work/ledger", Some("look")),
            live("s4", "/home/pi/work/other", Some("dev")),
            live("s5", "/home/pi/work/ledger", None),
        ],
        home(),
        &every_dir,
    );
    assert_eq!((p.rows[0].have, p.rows[0].more), (2, 1));
    assert_eq!(p.more, 1);
    assert!(p.rows[0].blocked.is_none());
}

#[test]
fn an_already_full_row_starts_nothing_and_extra_ones_are_not_stopped() {
    let book = parse_book(BOOK);
    let p = plan_rows(
        &[row("/srv/a", "dev", 1)],
        &book,
        &[
            live("s1", "/srv/a", Some("dev")),
            live("s2", "/srv/a", Some("dev")),
        ],
        home(),
        &every_dir,
    );
    assert_eq!((p.rows[0].have, p.rows[0].more), (2, 0));
    assert_eq!(p.more, 0);
}

#[test]
fn a_row_that_cannot_start_says_why_and_does_not_count_toward_more() {
    let book = parse_book(BOOK);
    let p = plan_rows(
        &[
            row("/srv/a", "gone", 2),
            row("/srv/a", "bad", 1),
            row("/srv/missing", "dev", 1),
            row("/srv/a", "look", 1),
        ],
        &book,
        &[],
        home(),
        &|d: &str| d != "/srv/missing",
    );
    let why: Vec<bool> = p.rows.iter().map(|r| r.blocked.is_some()).collect();
    assert_eq!(why, vec![true, true, true, false]);
    assert_eq!(
        p.rows[0].blocked.as_deref(),
        Some(copy_core::copy_text("beSessionSet.alias.missing", &[("alias", "gone")]).as_str())
    );
    assert!(p.rows.iter().take(3).all(|r| r.more == 0));
    assert_eq!(p.more, 1);
}

#[test]
fn the_group_name_defaults_to_the_alias_and_takes_the_dir_when_the_alias_repeats() {
    let book = parse_book(BOOK);
    let mut named = row("~/work/cp", "dev", 1);
    named.tab_group = Some("cp 组".into());
    let p = plan_rows(
        &[
            row("~/work/cc-monitor", "dev", 2),
            row("~/work/pb", "dev", 1),
            named,
            row("~/work/cc-bus", "look", 1),
        ],
        &book,
        &[],
        home(),
        &every_dir,
    );
    let groups: Vec<&str> = p.rows.iter().map(|r| r.group.as_str()).collect();
    let with_dir = |g: &str, d: &str| {
        copy_core::copy_text("beSessionSet.group.withDir", &[("group", g), ("dir", d)])
    };
    assert_eq!(
        groups,
        vec![
            with_dir("dev", "cc-monitor").as_str(),
            with_dir("dev", "pb").as_str(),
            "cp 组",
            "look",
        ]
    );
}

#[test]
fn set_plan_reads_a_saved_set_or_unsaved_rows_and_refuses_both_or_neither() {
    let text = format!("{BOOK}\n[set.\"g\"]\nrow = [{{ cwd = \"/srv/a\", alias = \"dev\", n = 2 }}]\n\n[set.\"坏\"]\nrow = [{{ cwd = \"/srv/a\", alias = \"dev\", n = 0 }}]\n");
    let book = parse_book(&text);
    let ask = |name: Option<&str>, rows: Option<Vec<RowEdit>>| {
        answer(
            &SetPlanReq {
                name: name.map(str::to_string),
                rows,
            },
            &book,
            &[live("s1", "/srv/a", Some("dev"))],
            home(),
            &every_dir,
        )
    };
    let saved = ask(Some("g"), None).expect("存好的那一组");
    assert_eq!((saved.rows[0].have, saved.more), (1, 1));
    let unsaved = ask(None, Some(vec![row("/srv/a", "dev", 3)])).expect("框里的几行");
    assert_eq!(unsaved.more, 2);
    assert_eq!(ask(Some("没有"), None).unwrap_err().0, "no_such_set");
    assert_eq!(ask(Some("坏"), None).unwrap_err().0, "refused");
    assert_eq!(ask(None, None).unwrap_err().0, "bad_args");
    assert_eq!(ask(Some("g"), Some(vec![])).unwrap_err().0, "bad_args");
}
