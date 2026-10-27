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

/// 别名写了 `tab-group`（自己写的或基于的那一段写的）⇒ 组名缺省取它；同一条别名在几个目录 ⇒ 「分组名 · 目录末一段」。
#[test]
fn the_group_name_defaults_to_the_aliases_tab_group_when_it_has_one() {
    let book = parse_book(
        "[dev]\nccm-tmux = true\ntab-group = \"开发\"\n\n[dev2]\nfrom = \"dev\"\n\n[look]\naccount = \"b\"\n",
    );
    let p = plan_rows(
        &[
            row("~/work/cc-monitor", "dev", 2),
            row("~/work/pb", "dev", 1),
            row("~/work/cp", "dev2", 1),
            row("~/work/cc-bus", "look", 1),
        ],
        &book,
        &[],
        home(),
        &every_dir,
    );
    assert!(p.rows.iter().all(|r| r.blocked.is_none()), "{p:?}");
    let groups: Vec<&str> = p.rows.iter().map(|r| r.group.as_str()).collect();
    let with_dir = |g: &str, d: &str| {
        copy_core::copy_text("beSessionSet.group.withDir", &[("group", g), ("dir", d)])
    };
    assert_eq!(
        groups,
        vec![
            with_dir("开发", "cc-monitor").as_str(),
            with_dir("开发", "pb").as_str(),
            "开发",
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

fn opened(sid: Option<&str>) -> cmd_wire::session::Opened {
    cmd_wire::session::Opened {
        landed: cmd_wire::session::Landed::Started,
        session: Some("t".into()),
        line: None,
        sid: sid.map(str::to_string),
        agent: None,
        cwd: None,
        account: None,
        note: None,
        prompt: None,
        arrival: None,
    }
}

/// 一点就起：每行只起差的几个；起之前那一笔先写好组注解（先定好的 sid 带着组 id）；同一个组名的几行进同一个组、
/// 组只找 / 建一次；起不成的那一条摘掉注解、照实记下，别的照起；起不了的那一行一条都不起。
#[test]
fn launching_marks_each_new_session_with_its_group_before_starting_it() {
    let book = parse_book(BOOK);
    let mut cc = row("/srv/cc", "dev", 3);
    cc.tab_group = Some("开发".into());
    let mut pb = row("/srv/pb", "dev", 1);
    pb.tab_group = Some("开发".into());
    let rows = vec![cc, pb, row("/srv/a", "gone", 1)];
    let plan = plan_rows(
        &rows,
        &book,
        &[
            live("s0", "/srv/cc", Some("dev")),
            live("s00", "/srv/cc", Some("dev")),
        ],
        home(),
        &every_dir,
    );
    let log = std::cell::RefCell::new(Vec::<String>::new());
    let n = std::cell::Cell::new(0);
    let out = launch_rows(
        &rows,
        &plan,
        &mut Hands {
            group_for: &mut |name: &str| {
                log.borrow_mut().push(format!("group {name}"));
                Ok(GroupRef {
                    id: "g1".into(),
                    name: name.into(),
                })
            },
            mark: &mut |g: &GroupRef| {
                n.set(n.get() + 1);
                let sid = format!("sid{}", n.get());
                log.borrow_mut().push(format!("mark {sid} {}", g.id));
                Some(sid)
            },
            start: &mut |r: &RowEdit, sid: Option<&str>| {
                log.borrow_mut()
                    .push(format!("start {} {}", r.cwd, sid.unwrap_or("-")));
                if r.cwd == "/srv/pb" {
                    Err(LaunchFail {
                        code: "start_failed".into(),
                        message: "坏了".into(),
                    })
                } else {
                    Ok(opened(sid))
                }
            },
            unmark: &mut |sid: &str, g: &GroupRef| {
                log.borrow_mut().push(format!("unmark {sid} {}", g.id));
            },
        },
    );
    assert_eq!(
        *log.borrow(),
        vec![
            "group 开发",
            "mark sid1 g1",
            "start /srv/cc sid1",
            "mark sid2 g1",
            "start /srv/pb sid2",
            "unmark sid2 g1",
        ]
    );
    assert_eq!((out.started, out.failed), (1, 1));
    assert_eq!(out.rows[0].opened[0].sid.as_deref(), Some("sid1"));
    assert_eq!(out.rows[1].failed[0].code, "start_failed");
    assert!(out.rows[2].blocked.is_some());
    assert!(out.rows[2].opened.is_empty() && out.rows[2].failed.is_empty());
    assert_eq!(
        out.rows[0].group.as_ref().map(|g| g.id.as_str()),
        Some("g1")
    );
}

/// 界面照摆的几句由这里写好：每行（菜单第二行那一段 · 已有几个 · 右侧再起 / 够了 / 起不了的原因）· 整组（几行连起来 · 右侧再起 / 已齐 · 合计 · 主按钮的字）。
#[test]
fn the_plan_carries_the_written_lines_the_ui_shows_as_is() {
    let book = parse_book(BOOK);
    let t = |k: &str, a: &[(&str, &str)]| copy_core::copy_text(k, a);
    let p = plan_rows(
        &[row("/srv/a", "dev", 2), row("/srv/b", "gone", 1)],
        &book,
        &[live("s1", "/srv/a", Some("dev"))],
        home(),
        &every_dir,
    );
    let why = t("beSessionSet.alias.missing", &[("alias", "gone")]);
    let a = t(
        "beSessionSet.row.counted",
        &[("cwd", "/srv/a"), ("group", "dev"), ("have", "1"), ("want", "2")],
    );
    let b = t("beSessionSet.row.blocked", &[("cwd", "/srv/b"), ("why", &why)]);
    assert_eq!(p.rows[0].said, a);
    assert_eq!(p.rows[1].said, b);
    assert_eq!(p.rows[0].have_said, t("beSessionSet.row.have", &[("n", "1")]));
    assert_eq!(p.rows[0].more_said, t("beSessionSet.more.some", &[("n", "1")]));
    assert_eq!(p.rows[1].more_said, why);
    assert_eq!(
        p.said,
        format!("{a}{}{b}", t("beSessionSet.row.sep", &[]))
    );
    assert_eq!(p.more_said, t("beSessionSet.more.some", &[("n", "1")]));
    assert_eq!(p.total_said, t("beSessionSet.more.total", &[("n", "1")]));
    assert_eq!(p.start_said, t("beSessionSet.start.some", &[("n", "1")]));
    // 都够了：右侧「已齐」、行右侧「—」、主按钮「已齐」。
    let full = plan_rows(
        &[row("/srv/a", "dev", 1)],
        &book,
        &[live("s1", "/srv/a", Some("dev"))],
        home(),
        &every_dir,
    );
    assert_eq!(full.more_said, t("beSessionSet.more.full", &[]));
    assert_eq!(full.start_said, t("beSessionSet.more.full", &[]));
    assert_eq!(full.rows[0].more_said, t("beSessionSet.more.none", &[]));
}

/// 起完那一句也由这里写：全成「已启动 n」；有没成的「启动 成/全」＋ 逐条「别名 启动失败 · 原因」。
#[test]
fn the_launch_result_carries_its_sentence_and_one_line_per_failure() {
    let book = parse_book(BOOK);
    let rows = vec![row("/srv/a", "dev", 1), row("/srv/b", "look", 1)];
    let plan = plan_rows(&rows, &book, &[], home(), &every_dir);
    let run = |fail_b: bool| {
        launch_rows(
            &rows,
            &plan,
            &mut Hands {
                group_for: &mut |name: &str| {
                    Ok(GroupRef {
                        id: "g".into(),
                        name: name.into(),
                    })
                },
                mark: &mut |_| None,
                start: &mut |r: &RowEdit, _| {
                    if fail_b && r.cwd == "/srv/b" {
                        Err(LaunchFail {
                            code: "start_failed".into(),
                            message: "b 需登录".into(),
                        })
                    } else {
                        Ok(opened(None))
                    }
                },
                unmark: &mut |_, _| {},
            },
        )
    };
    let t = |k: &str, a: &[(&str, &str)]| copy_core::copy_text(k, a);
    let ok = run(false);
    assert_eq!(ok.said, t("beSessionSet.launched.done", &[("n", "2")]));
    assert_eq!(ok.detail, "");
    let part = run(true);
    assert_eq!(
        part.said,
        t("beSessionSet.launched.partial", &[("started", "1"), ("all", "2")])
    );
    assert_eq!(
        part.detail,
        t("beSessionSet.launched.fail", &[("alias", "look"), ("why", "b 需登录")])
    );
}
