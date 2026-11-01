//! 一批会话合好的那一句：汇总句（一项点名 · 几项同文件批量那一句）· 没成 · 跳过的逐条 · 复制详情。

use super::*;
use cmd_wire::batch::SessionOutcome;

fn verb() -> Verb {
    Verb {
        action: "stop".to_string(),
        one_done: |t| copy_text("sessionState.kill.done", &[("title", t)]),
        one_failed: |t| copy_text("tabSessionActions.kill.failed", &[("title", t)]),
    }
}

fn row<'a>(title: &'a str, state: SessionState, said: Option<&'a str>, detail: &'a str) -> Row<'a> {
    Row {
        title,
        state,
        said,
        detail,
    }
}

fn head_line(n: &str, segs: &[(&str, &str)]) -> String {
    let mut parts = vec![copy_text(
        "beFiles.batch.head",
        &[("action", "stop"), ("n", n)],
    )];
    for (k, v) in segs {
        parts.push(match *k {
            "ok" => copy_text("beFiles.batch.segOk", &[("ok", v)]),
            "skip" => copy_text("beFiles.batch.segSkip", &[("skip", v)]),
            _ => copy_text("beFiles.batch.segBad", &[("bad", v)]),
        });
    }
    parts.join(&copy_text("kit.text.sep", &[]))
}

/// ★ 三项（成 · 成 · 没成）⇒ 同文件批量那一句、没成的那一行、只那一段复制详情。
#[test]
fn three_rows_with_one_failed_say_the_batch_line() {
    use SessionState::*;
    let rows = [
        row("a", Done, None, ""),
        row("b", Done, None, ""),
        row("c", Failed, Some("tmux silent"), "exit 1"),
    ];
    let s = summary_of(&verb(), &rows);
    assert_eq!(s.said, head_line("3", &[("ok", "2"), ("bad", "1")]));
    assert!(s.failed);
    assert_eq!(
        s.lines,
        vec![copy_text(
            "tabBatch.result.failedLine",
            &[("title", "c"), ("why", "tmux silent")]
        )]
    );
    assert_eq!(s.copy_detail, "tmux silent\nexit 1");
}

/// 一项成了 ⇒ 点名那一句，没有逐条行、没有复制详情。
#[test]
fn one_done_row_names_it() {
    let s = summary_of(&verb(), &[row("a", SessionState::Done, None, "")]);
    assert_eq!(
        s.said,
        copy_text("sessionState.kill.done", &[("title", "a")])
    );
    assert!(!s.failed);
    assert!(s.lines.is_empty());
    assert_eq!(s.copy_detail, "");
}

/// 一项没成 ⇒ 点名那一句没成，那一行是那台那一句。
#[test]
fn one_failed_row_names_it_and_keeps_the_line() {
    let s = summary_of(&verb(), &[row("a", SessionState::Failed, Some("gone"), "")]);
    assert_eq!(
        s.said,
        copy_text("tabSessionActions.kill.failed", &[("title", "a")])
    );
    assert!(s.failed);
    assert_eq!(
        s.lines,
        vec![copy_text(
            "tabBatch.result.failedLine",
            &[("title", "a"), ("why", "gone")]
        )]
    );
    assert_eq!(s.copy_detail, "", "没带详情的那一项不进复制详情");
}

/// 一项跳过 ⇒ 不算没成，那一行是跳过那一行。
#[test]
fn one_skipped_row_is_not_failed() {
    let s = summary_of(
        &verb(),
        &[row("a", SessionState::Skipped, Some("ended"), "")],
    );
    assert!(!s.failed);
    assert_eq!(s.said, head_line("1", &[("skip", "1")]));
    assert_eq!(
        s.lines,
        vec![copy_text(
            "tabBatch.result.skippedLine",
            &[("title", "a"), ("why", "ended")]
        )]
    );
}

/// 没成的在前、跳过的在后；复制详情各一段、段间空一行。
#[test]
fn failed_lines_come_before_skipped_and_details_join_with_a_blank_line() {
    use SessionState::*;
    let rows = [
        row("a", Skipped, Some("ended"), ""),
        row("b", Failed, Some("x"), "d1"),
        row("c", Failed, Some("y"), "d2"),
    ];
    let s = summary_of(&verb(), &rows);
    assert_eq!(
        s.lines,
        vec![
            copy_text(
                "tabBatch.result.failedLine",
                &[("title", "b"), ("why", "x")]
            ),
            copy_text(
                "tabBatch.result.failedLine",
                &[("title", "c"), ("why", "y")]
            ),
            copy_text(
                "tabBatch.result.skippedLine",
                &[("title", "a"), ("why", "ended")]
            ),
        ]
    );
    assert_eq!(s.copy_detail, "x\nd1\n\ny\nd2");
}

/// 回包：逐条结局原样照序带着，汇总那几格照逐条算好。
#[test]
fn the_reply_keeps_results_in_order_and_fills_the_summary() {
    let out = |sid: &str, state, said: Option<&str>, detail: &str| SessionOutcome {
        machine: cmd_wire::Machine::local(),
        sid: sid.parse().expect("sid"),
        outcome: state,
        why: None,
        said: said.map(str::to_string),
        copy_detail: detail.to_string(),
        title: sid.to_string(),
        session: None,
    };
    let results = vec![
        out("s-1", SessionState::Done, None, ""),
        out("s-2", SessionState::Done, None, ""),
        out("s-3", SessionState::Failed, Some("tmux silent"), "exit 1"),
    ];
    let r = sessions_reply(&verb(), results.clone());
    assert_eq!(r.results, results);
    assert_eq!(r.said, head_line("3", &[("ok", "2"), ("bad", "1")]));
    assert!(r.failed);
    assert_eq!(
        r.lines,
        vec![copy_text(
            "tabBatch.result.failedLine",
            &[("title", "s-3"), ("why", "tmux silent")]
        )]
    );
    assert_eq!(r.copy_detail, "tmux silent\nexit 1");
}
