//! 自动起算的醒点：假钟、假额度账、假「起那一趟」，不起真 claude、不起中转。
//! 开着且没窗 ⇒ 当场发一次；有窗 ⇒ 醒在重置那一刻、不早不晚；时段外不发、到起点发；全关 ⇒ 不醒；中转不在 ⇒ 不起进程、记失败不打转；
//! 卡住会话到最早恢复那一刻推一次；起那一趟有期限。

use super::*;
use crate::accounts::quota::autostart::{AutoConf, WindowSeen};
use std::cell::{Cell, RefCell};

const UTC: &dyn Fn(u64) -> i64 = &|_| 0;
/// 2026-10-05 00:00:00 UTC。
const DAY0: u64 = 1_791_158_400;

fn at(h: u64, m: u64) -> u64 {
    DAY0 + h * 3600 + m * 60
}

/// 一台假世界：一个号 q，开关 / 时段 / 额度账由判据摆；「起那一趟」只记次数、按需在额度账上出一个新窗。
struct Fake {
    clock: Cell<u64>,
    conf: RefCell<AutoConf>,
    seen: RefCell<Option<(WindowSeen, u64)>>,
    relay: Cell<bool>,
    sent: Cell<u32>,
    /// 起那一趟时额度账上出的新窗（`None` ⇒ 起了没出数）。
    opens: Cell<bool>,
    records: RefCell<Vec<(String, Result<(), AutostartFail>, u64)>>,
    stuck: RefCell<Vec<(String, u64)>>,
    pushed: RefCell<Vec<String>>,
}

impl Fake {
    fn new(conf: AutoConf) -> Self {
        Self {
            clock: Cell::new(at(9, 0)),
            conf: RefCell::new(conf),
            seen: RefCell::new(None),
            relay: Cell::new(true),
            sent: Cell::new(0),
            opens: Cell::new(true),
            records: RefCell::default(),
            stuck: RefCell::default(),
            pushed: RefCell::default(),
        }
    }

    fn reset_at(&self, r: u64) {
        *self.seen.borrow_mut() = Some((
            WindowSeen {
                five_hour_reset: Some(r),
                refused_until: None,
            },
            r - 5 * 3600,
        ));
    }

    /// 用这个世界走一步。
    fn step(&self, w: &mut Waker<'_>) -> Option<u64> {
        let now = || self.clock.get();
        let look = || Look {
            accounts: vec![Cand {
                id: "q".into(),
                dir: "/h/accounts/q".into(),
                conf: self.conf.borrow().clone(),
                seen: self.seen.borrow().map(|(s, _)| s),
                seen_at: self.seen.borrow().map(|(_, t)| t),
                needs_login: false,
            }],
            stuck: self.stuck.borrow().clone(),
        };
        let relay_up = || self.relay.get();
        let send = |_: &Path| {
            self.sent.set(self.sent.get() + 1);
            if self.opens.get() {
                let t = self.clock.get();
                *self.seen.borrow_mut() = Some((
                    WindowSeen {
                        five_hour_reset: Some(t + 5 * 3600),
                        refused_until: None,
                    },
                    t,
                ));
            }
            Ok(0)
        };
        let record = |a: &str, r: Result<(), AutostartFail>, t: u64| {
            self.records.borrow_mut().push((a.to_string(), r, t));
        };
        let push = |sid: &str| self.pushed.borrow_mut().push(sid.to_string());
        let world = World {
            now: &now,
            local: UTC,
            look: &look,
            relay_up: &relay_up,
            send: &send,
            record: &record,
            push_session: &push,
        };
        w.step(&world)
    }
}

fn on(window: Option<(&str, &str)>) -> AutoConf {
    AutoConf {
        enabled: true,
        window: window.map(|(f, t)| autostart::DayWindow {
            from: f.into(),
            to: t.into(),
        }),
        ..AutoConf::default()
    }
}

#[test]
fn on_and_no_window_sends_once_then_waits_for_the_new_reset() {
    let mem = Memory::default();
    let mut w = Waker::new(&mem);
    let f = Fake::new(on(None));
    assert_eq!(
        f.step(&mut w),
        Some(at(9, 0)),
        "发过 ⇒ 回此刻让调用方现读再算"
    );
    assert_eq!(f.sent.get(), 1);
    assert_eq!(f.records.borrow()[0].1, Ok(()));
    assert_eq!(
        f.step(&mut w),
        Some(at(14, 0)),
        "新窗在计时 ⇒ 醒在它的重置时刻"
    );
    assert_eq!(f.sent.get(), 1);
}

#[test]
fn a_running_window_wakes_at_its_reset_not_earlier_not_later() {
    let mem = Memory::default();
    let mut w = Waker::new(&mem);
    let f = Fake::new(on(None));
    let r = at(14, 10);
    f.reset_at(r);
    f.clock.set(r - 1);
    assert_eq!(f.step(&mut w), Some(r));
    assert_eq!(f.sent.get(), 0, "差一秒也不发");
    f.clock.set(r);
    f.step(&mut w);
    assert_eq!(f.sent.get(), 1, "到重置那一刻就发");
}

#[test]
fn outside_the_window_it_waits_for_the_start() {
    let mem = Memory::default();
    let mut w = Waker::new(&mem);
    let f = Fake::new(on(Some(("07:00", "24:00"))));
    f.clock.set(at(5, 0));
    assert_eq!(f.step(&mut w), Some(at(7, 0)));
    assert_eq!(f.sent.get(), 0);
    f.clock.set(at(7, 0));
    f.step(&mut w);
    assert_eq!(f.sent.get(), 1);
}

#[test]
fn all_off_registers_no_wake() {
    let mem = Memory::default();
    let mut w = Waker::new(&mem);
    let f = Fake::new(AutoConf::default());
    assert_eq!(f.step(&mut w), None, "关着且没有卡住的会话 ⇒ 无期限地等");
    f.reset_at(at(14, 10));
    assert_eq!(f.step(&mut w), None);
    assert_eq!(f.sent.get(), 0);
}

#[test]
fn relay_down_spawns_nothing_records_once_and_does_not_spin() {
    let mem = Memory::default();
    let mut w = Waker::new(&mem);
    let f = Fake::new(on(None));
    f.relay.set(false);
    assert_eq!(f.step(&mut w), Some(at(9, 0)));
    assert_eq!(
        f.sent.get(),
        0,
        "中转不在 ⇒ 不起进程（不经中转发出去的那一句不进额度账）"
    );
    assert_eq!(f.records.borrow()[0].1, Err(AutostartFail::RelayDown));
    f.clock.set(at(9, 1));
    assert_eq!(f.step(&mut w), None, "这一段空闲期试过了 ⇒ 不醒、不重试");
    assert_eq!(f.records.borrow().len(), 1);
    // 改了开关（帧面那一路忘掉它试过哪一段）⇒ 当场再试。
    mem.forget("q");
    f.relay.set(true);
    f.step(&mut w);
    assert_eq!(f.sent.get(), 1);
}

#[test]
fn a_send_that_brings_no_reading_is_recorded_as_such() {
    let mem = Memory::default();
    let mut w = Waker::new(&mem);
    let f = Fake::new(on(None));
    f.opens.set(false);
    f.step(&mut w);
    assert_eq!(f.records.borrow()[0].1, Err(AutostartFail::NoReading));
    assert_eq!(f.step(&mut w), None);
    assert_eq!(f.sent.get(), 1);
}

#[test]
fn a_stuck_session_is_pushed_once_at_its_earliest_return() {
    let mem = Memory::default();
    let mut w = Waker::new(&mem);
    let f = Fake::new(AutoConf::default());
    let t = at(11, 30);
    *f.stuck.borrow_mut() = vec![("s1".into(), t)];
    assert_eq!(f.step(&mut w), Some(t));
    assert!(f.pushed.borrow().is_empty());
    // 到点：那个号回来了 ⇒ 会话不再算卡住（清单里没它了），醒点照样推它一次。
    f.clock.set(t);
    f.stuck.borrow_mut().clear();
    assert_eq!(f.step(&mut w), None);
    assert_eq!(*f.pushed.borrow(), vec!["s1".to_string()]);
    f.clock.set(t + 60);
    f.step(&mut w);
    assert_eq!(f.pushed.borrow().len(), 1);
}

#[cfg(unix)]
#[test]
fn the_one_send_runs_ccm_with_the_line_and_a_deadline() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("ccm-autostart-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    let out = root.join("argv");
    let fake = root.join("fake-ccm");
    std::fs::write(
        &fake,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$PWD\" \"$@\" > '{}'\nexit 0\n",
            out.display()
        ),
    )
    .expect("write");
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let dir = root.join("data").join(autostart::DIR_NAME);
    let acct = root.join("accounts").join("q");
    assert_eq!(
        autostart_send::run_ccm(&fake, AGENT, &acct, &dir).expect("ran"),
        0
    );
    assert!(dir.is_dir(), "工作目录建好了");
    let got = std::fs::read_to_string(&out).expect("argv");
    let lines: Vec<&str> = got.lines().collect();
    assert_eq!(
        &lines[1..],
        autostart_send::ccm_argv(AGENT, &acct, &dir)
            .expect("argv")
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    // 期限：装一个短的总期限，那一趟卡住就被掐掉、回超时（不是一直等）。
    let _b = crate::platform::child::Budget::start(crate::platform::child::Deadline::millis(300));
    let slow = root.join("slow-ccm");
    std::fs::write(&slow, "#!/bin/sh\nsleep 5\n").expect("write");
    std::fs::set_permissions(&slow, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let r = autostart_send::run_ccm(&slow, AGENT, &acct, &dir);
    assert!(matches!(r, Err(ref e) if e.is_timed_out()), "{r:?}");
    let _ = std::fs::remove_dir_all(&root);
}
