//! 会话开着时切权限档：只有「按一下换到下一档」的循环 ⇒ 读一次 · 按一下 · 等重画 · 再读，到了为止；
//! 在等人不按；认不出 · 不在循环里 · 按了没变都照实说「结果未知」，不猜。

use super::*;
use crate::control::terminals_wire::{Sent, SentResult};

/// 合成的那一屏：`order` 是那一家的循环（按一下走一格），`at` 此刻在第几格；`blind` 为真 ⇒ 认不出；`stuck` 为真 ⇒ 按了不重画。
struct Fake {
    order: Vec<&'static str>,
    at: usize,
    blind: bool,
    stuck: bool,
    presses: usize,
    opened: bool,
    sent: SentResult,
}

impl Fake {
    fn new(order: &[&'static str], at: usize) -> Fake {
        Fake {
            order: order.to_vec(),
            at,
            blind: false,
            stuck: false,
            presses: 0,
            opened: false,
            sent: SentResult::Delivered,
        }
    }
}

impl ModeIo for Fake {
    fn open(&mut self) -> Result<(), CmdErr> {
        self.opened = true;
        Ok(())
    }
    fn read(&mut self) -> Result<Option<&'static str>, CmdErr> {
        Ok((!self.blind).then(|| self.order[self.at]))
    }
    fn press(&mut self) -> Result<Sent, CmdErr> {
        self.presses += 1;
        if self.sent == SentResult::Delivered && !self.stuck {
            self.at = (self.at + 1) % self.order.len();
        }
        Ok(Sent {
            result: self.sent,
            why: (self.sent == SentResult::Refused)
                .then_some(crate::control::terminals::Refused::NotYours),
            said: None,
            screen: None,
        })
    }
    fn redrawn(&mut self) -> bool {
        !self.stuck
    }
}

const SW: crate::agents::ModeSwitch = crate::agents::ModeSwitch {
    key: "shift-tab",
    cycle_max: 5,
    read: |_| None,
};

fn run(target: &str, waiting: bool, f: &mut Fake) -> ModeSwitched {
    switch_with(target, &SW, waiting, f).unwrap()
}

/// ★ 循环走到那一档为止：从 default 到 plan 按两下，回 done ＋ 读到的那一档。
#[test]
fn it_presses_until_the_mode_reads_back() {
    let mut f = Fake::new(&["default", "acceptEdits", "plan"], 0);
    let got = run("plan", false, &mut f);
    assert_eq!(got.result, SwitchResult::Done);
    assert_eq!(got.mode, Some("plan"));
    assert_eq!(f.presses, 2);
    // 已经是那一档 ⇒ 一下都不按。
    let mut f = Fake::new(&["default", "acceptEdits", "plan"], 2);
    assert_eq!(run("plan", false, &mut f).result, SwitchResult::Done);
    assert_eq!(f.presses, 0);
}

/// ★ 在等人 ⇒ 不起看屏的客户端、一下都不按，回被拒 ＋ `waiting`。
#[test]
fn a_waiting_session_is_not_touched() {
    let mut f = Fake::new(&["default", "acceptEdits"], 0);
    let got = run("acceptEdits", true, &mut f);
    assert_eq!(got.result, SwitchResult::Refused);
    assert_eq!(got.why, Some(crate::control::terminals::Refused::Waiting));
    assert!(!f.opened && f.presses == 0);
}

/// ★ 绕了一圈没见到那一档（此会话的循环里没有它）⇒ 结果未知，停在绕回来的那一下，不接着按。
#[test]
fn a_mode_outside_the_cycle_stops_after_one_round() {
    let mut f = Fake::new(&["default", "acceptEdits"], 0);
    let got = run("plan", false, &mut f);
    assert_eq!(got.result, SwitchResult::Unsure);
    assert_eq!(got.mode, Some("default"));
    assert_eq!(f.presses, 2);
    assert_eq!(
        got.said.as_deref(),
        Some(copy_core::copy_text("beSessionMode.unsure.notInCycle", &[]).as_str())
    );
}

/// ★ 认不出此刻的档 ⇒ 不按、结果未知；按了屏幕没重画 ⇒ 不再按、结果未知。
#[test]
fn an_unreadable_or_frozen_screen_is_unsure_not_guessed() {
    let mut f = Fake::new(&["default", "plan"], 0);
    f.blind = true;
    let got = run("plan", false, &mut f);
    assert_eq!(got.result, SwitchResult::Unsure);
    assert_eq!(got.mode, None);
    assert_eq!(f.presses, 0);
    assert_eq!(
        got.said.as_deref(),
        Some(copy_core::copy_text("beSessionMode.unsure.unread", &[]).as_str())
    );
    let mut f = Fake::new(&["default", "plan"], 0);
    f.stuck = true;
    let got = run("plan", false, &mut f);
    assert_eq!(got.result, SwitchResult::Unsure);
    assert_eq!(f.presses, 1);
    assert_eq!(
        got.said.as_deref(),
        Some(copy_core::copy_text("beSessionMode.unsure.noRedraw", &[]).as_str())
    );
}

/// ★ 按键那一下没送到（不是这个前端的会话 · 不知道送没送到）⇒ 照那一下的结局回，不再按。
#[test]
fn a_press_that_did_not_land_ends_the_walk() {
    let mut f = Fake::new(&["default", "plan"], 0);
    f.sent = SentResult::Refused;
    let got = run("plan", false, &mut f);
    assert_eq!(got.result, SwitchResult::Refused);
    assert_eq!(got.why, Some(crate::control::terminals::Refused::NotYours));
    assert_eq!(f.presses, 1);
    let mut f = Fake::new(&["default", "plan"], 0);
    f.sent = SentResult::Unsure;
    assert_eq!(run("plan", false, &mut f).result, SwitchResult::Unsure);
    assert_eq!(f.presses, 1);
}
