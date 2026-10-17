//! `ccm-user-path`：成品（状态 · 那一句 · 详情 · 给人看的那段命令）怎么从「问 / 改」两样结果里出。问 / 改两样换成替身。

use super::*;
use std::cell::RefCell;

const DIR: &str = r"C:\Users\u\.cc-monitor\bin";

/// 替身：记下被叫了哪几下（`set:true` / `set:false` / `holds`），按给定结果答。
struct Fake {
    calls: RefCell<Vec<&'static str>>,
    holds: Result<bool, Said>,
    set: Result<(), Said>,
}

impl Fake {
    fn new(holds: Result<bool, Said>, set: Result<(), Said>) -> Fake {
        Fake {
            calls: RefCell::new(Vec::new()),
            holds,
            set,
        }
    }
    fn run(&self, op: UserPathOp, windows: bool, dir: Option<&str>) -> UserPath {
        answer(
            op,
            windows,
            dir,
            &|d| {
                assert_eq!(d, DIR);
                self.calls.borrow_mut().push("holds");
                self.holds.clone()
            },
            &|d, on| {
                assert_eq!(d, DIR);
                self.calls
                    .borrow_mut()
                    .push(if on { "set:true" } else { "set:false" });
                self.set.clone()
            },
        )
    }
    fn calls(&self) -> Vec<&'static str> {
        self.calls.borrow().clone()
    }
}

fn boom(raw: &str) -> Said {
    Said::with_raw("原因词".to_string(), raw)
}

#[test]
fn elsewhere_it_says_this_is_not_windows_and_touches_nothing() {
    for op in [UserPathOp::Read, UserPathOp::Add, UserPathOp::Remove] {
        let f = Fake::new(Ok(true), Ok(()));
        let r = f.run(op, false, Some(DIR));
        assert_eq!(r.state, UserPathState::NotWindows);
        assert_eq!(r.said, copy_text("beUserPath.state.notWindows", &[]));
        assert_eq!((r.detail, r.command), (None, None));
        assert!(f.calls().is_empty(), "{:?}", f.calls());
    }
}

#[test]
fn reading_on_offers_the_removal_and_reading_off_offers_the_addition() {
    let f = Fake::new(Ok(true), Ok(()));
    let on = f.run(UserPathOp::Read, true, Some(DIR));
    assert_eq!(f.calls(), ["holds"]);
    assert_eq!(on.state, UserPathState::On);
    assert_eq!(on.said, copy_text("beUserPath.state.on", &[]));
    assert_eq!(on.detail, None);
    assert_eq!(on.command.as_deref(), Some(user_path_remove(DIR).as_str()));

    let f = Fake::new(Ok(false), Ok(()));
    let off = f.run(UserPathOp::Read, true, Some(DIR));
    assert_eq!(off.state, UserPathState::Off);
    assert_eq!(off.said, copy_text("beUserPath.state.off", &[("dir", DIR)]));
    assert_eq!(off.command.as_deref(), Some(user_path_add(DIR).as_str()));
}

#[test]
fn an_unreadable_user_path_is_unknown_not_off() {
    let f = Fake::new(Err(boom("Access is denied.")), Ok(()));
    let r = f.run(UserPathOp::Read, true, Some(DIR));
    assert_eq!(r.state, UserPathState::Unknown);
    assert_eq!(
        r.said,
        copy_text("beUserPath.state.unknown", &[("why", "原因词")])
    );
    assert!(
        r.detail
            .as_deref()
            .unwrap_or("")
            .contains("Access is denied."),
        "{:?}",
        r.detail
    );
    assert_eq!(r.command, None);
}

#[test]
fn add_and_remove_write_first_then_answer_with_what_is_read_back() {
    let f = Fake::new(Ok(true), Ok(()));
    let r = f.run(UserPathOp::Add, true, Some(DIR));
    assert_eq!(f.calls(), ["set:true", "holds"]);
    assert_eq!(
        (r.state, r.said),
        (UserPathState::On, copy_text("beUserPath.state.on", &[]))
    );

    let f = Fake::new(Ok(false), Ok(()));
    let r = f.run(UserPathOp::Remove, true, Some(DIR));
    assert_eq!(f.calls(), ["set:false", "holds"]);
    assert_eq!(r.state, UserPathState::Off);
    assert_eq!(r.detail, None);
}

#[test]
fn a_failed_write_says_so_while_the_state_is_what_is_read_back() {
    let f = Fake::new(Ok(false), Err(boom("exit 1\nnope")));
    let r = f.run(UserPathOp::Add, true, Some(DIR));
    assert_eq!(r.state, UserPathState::Off);
    assert_eq!(
        r.said,
        copy_text("beUserPath.write.addFailed", &[("why", "原因词")])
    );
    assert!(r.detail.as_deref().unwrap_or("").contains("nope"));
    assert_eq!(r.command.as_deref(), Some(user_path_add(DIR).as_str()));

    let f = Fake::new(Ok(true), Err(boom("nope")));
    let r = f.run(UserPathOp::Remove, true, Some(DIR));
    assert_eq!(r.state, UserPathState::On);
    assert_eq!(
        r.said,
        copy_text("beUserPath.write.removeFailed", &[("why", "原因词")])
    );
}

#[test]
fn without_a_home_there_is_no_directory_to_ask_about() {
    let f = Fake::new(Ok(true), Ok(()));
    let r = f.run(UserPathOp::Add, true, None);
    assert_eq!(r.state, UserPathState::Unknown);
    assert_eq!(r.said, copy_text("beUserPath.state.noHome", &[]));
    assert!(f.calls().is_empty());
}
