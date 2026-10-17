//! `session-arrive`：报到了回 sid；没见到回那一句（起在 tmux 里的带那一屏末几行）＋ 那个终端还在才给句柄；坏入参不等。

use super::*;
use std::sync::Mutex;

/// 替身：报不报到 · 那一屏什么样，都是夹具给的；记下被问过的那几样。
struct Fake {
    arrives: Option<String>,
    screen: fn() -> Screen,
    asked: Mutex<Vec<String>>,
}

impl Host for Fake {
    fn arrival(
        &self,
        by: &ArriveBy,
        since: u64,
        ms: u64,
    ) -> impl Future<Output = Option<String>> + Send {
        self.asked
            .lock()
            .unwrap()
            .push(format!("arrival {by:?} {since} {ms}"));
        std::future::ready(self.arrives.clone())
    }
    fn screen(&self, session: &str) -> impl Future<Output = Screen> + Send {
        self.asked.lock().unwrap().push(format!("screen {session}"));
        std::future::ready((self.screen)())
    }
}

fn fake(arrives: Option<&str>, screen: fn() -> Screen) -> Fake {
    Fake {
        arrives: arrives.map(str::to_string),
        screen,
        asked: Mutex::new(Vec::new()),
    }
}

fn ask(by: ArriveBy, session: Option<&str>) -> ArriveAsk {
    ArriveAsk {
        by,
        since: 7,
        session: session.map(str::to_string),
    }
}

fn block<T>(f: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("起运行时")
        .block_on(f)
}

fn there() -> Screen {
    Screen::There {
        terminal: "t-1".into(),
        text: "a\n\nb\nc\nd\ne\nf\ng\nError: no such flag --x\n   \n".into(),
    }
}

fn said_of(e: &ArriveEnd) -> &str {
    match e {
        ArriveEnd::Missed { said, .. } => said,
        ArriveEnd::Arrived { .. } => panic!("应是没见到：{e:?}"),
    }
}

#[test]
fn arrived_answers_the_sid_and_does_not_read_the_screen() {
    let h = fake(Some("s-new"), there);
    let got = block(run(
        ask(ArriveBy::Cwd { cwd: "/w".into() }, Some("w-1")),
        45_000,
        &h,
    ))
    .unwrap();
    assert_eq!(
        got,
        ArriveEnd::Arrived {
            sid: "s-new".into()
        }
    );
    let asked = h.asked.lock().unwrap().clone();
    assert_eq!(
        asked,
        vec![r#"arrival Cwd { cwd: "/w" } 7 45000"#.to_string()]
    );
}

#[test]
fn missed_in_tmux_says_the_last_words_and_hands_the_terminal() {
    let h = fake(None, there);
    let got = block(run(
        ask(
            ArriveBy::Sid {
                sid: "0b1c2d3e-0000-4000-8000-000000000001".into(),
            },
            Some("w-1"),
        ),
        45_000,
        &h,
    ))
    .unwrap();
    let ArriveEnd::Missed { terminal, .. } = &got else {
        panic!("{got:?}")
    };
    assert_eq!(terminal.as_deref(), Some("t-1"));
    let said = said_of(&got);
    let (title, body) = said.split_once('\n').expect("标题 ＋ 正文");
    assert_eq!(
        title,
        copy_core::copy_text("beSessionArrive.missed.title", &[])
    );
    assert!(body.contains("w-1"), "{body}");
    assert!(body.contains(&copy_core::short_duration(45_000)), "{body}");
    assert!(
        body.ends_with("c\nd\ne\nf\ng\nError: no such flag --x"),
        "末 6 行非空行：{body}"
    );
    assert!(h.asked.lock().unwrap().iter().any(|a| a == "screen w-1"));
}

#[test]
fn missed_says_which_way_the_screen_was_not_there() {
    let gone = block(run(
        ask(ArriveBy::Cwd { cwd: "/w".into() }, Some("w-1")),
        1_000,
        &fake(None, || Screen::Gone),
    ))
    .unwrap();
    assert_eq!(
        gone,
        ArriveEnd::Missed {
            said: format!(
                "{}\n{}",
                copy_core::copy_text("beSessionArrive.missed.title", &[]),
                copy_core::copy_text(
                    "beSessionArrive.missed.gone",
                    &[("dur", &copy_core::short_duration(1_000)), ("name", "w-1")]
                )
            ),
            terminal: None
        }
    );
    let empty = block(run(
        ask(ArriveBy::Cwd { cwd: "/w".into() }, Some("w-1")),
        1_000,
        &fake(None, || Screen::There {
            terminal: "t".into(),
            text: " \n\n".into(),
        }),
    ))
    .unwrap();
    assert!(said_of(&empty).ends_with(&copy_core::copy_text(
        "beSessionArrive.missed.emptyScreen",
        &[("dur", &copy_core::short_duration(1_000)), ("name", "w-1")]
    )));
    let unread = block(run(
        ask(ArriveBy::Cwd { cwd: "/w".into() }, Some("w-1")),
        1_000,
        &fake(None, || Screen::Unread("tmux 坏了".into())),
    ))
    .unwrap();
    assert!(said_of(&unread).contains("tmux 坏了"));
}

#[test]
fn missed_in_a_window_points_at_the_window_and_reads_nothing() {
    let h = fake(None, there);
    let got = block(run(
        ask(ArriveBy::Cwd { cwd: "/w".into() }, None),
        2_000,
        &h,
    ))
    .unwrap();
    assert_eq!(
        got,
        ArriveEnd::Missed {
            said: format!(
                "{}\n{}",
                copy_core::copy_text("beSessionArrive.missed.title", &[]),
                copy_core::copy_text(
                    "beSessionArrive.missed.inWindow",
                    &[("dur", &copy_core::short_duration(2_000))]
                )
            ),
            terminal: None
        }
    );
    assert!(!h
        .asked
        .lock()
        .unwrap()
        .iter()
        .any(|a| a.starts_with("screen")));
}

#[test]
fn a_bad_ask_is_refused_before_waiting() {
    let h = fake(Some("x"), there);
    assert!(block(run(
        ask(
            ArriveBy::Sid {
                sid: "not a sid".into()
            },
            None
        ),
        1_000,
        &h
    ))
    .is_err());
    assert!(block(run(
        ask(ArriveBy::Cwd { cwd: String::new() }, None),
        1_000,
        &h
    ))
    .is_err());
    assert!(
        block(run(
            ask(ArriveBy::Cwd { cwd: "/w".into() }, None),
            MAX_WAIT_MS + 1,
            &h
        ))
        .is_ok(),
        "期限过长只封顶，不拒"
    );
    assert!(h
        .asked
        .lock()
        .unwrap()
        .iter()
        .all(|a| !a.contains("not a sid")));
    assert!(
        h.asked
            .lock()
            .unwrap()
            .iter()
            .any(|a| a.ends_with(&MAX_WAIT_MS.to_string())),
        "封到上界"
    );
}

#[test]
fn last_words_keeps_the_last_six_non_blank_lines() {
    assert_eq!(
        last_words("1\n2\n\n3\n4\n5\n6\n7  \n \n"),
        "2\n3\n4\n5\n6\n7"
    );
    assert_eq!(last_words(" \n"), "");
}
