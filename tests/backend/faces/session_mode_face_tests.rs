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
    gone: Option<crate::control::terminals::Refused>,
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
            gone: None,
        }
    }
}

impl ModeIo for Fake {
    fn open(&mut self) -> Result<Option<crate::control::terminals::Refused>, CmdErr> {
        self.opened = true;
        Ok(self.gone)
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
            resumed: false,
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
    switch_with(target, &SW, waiting, f, &|| panic!("记下了")).unwrap()
}

/// ★ 循环走到那一档为止：从 default 到 plan 按两下，回 done ＋ 读到的那一档。
#[test]
fn it_presses_until_the_mode_reads_back() {
    let mut f = Fake::new(&["default", "acceptEdits", "plan"], 0);
    let got = run("plan", false, &mut f);
    assert_eq!(got.result, SwitchResult::Done);
    assert_eq!(got.mode.as_deref(), Some("plan"));
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
    assert_eq!(got.mode.as_deref(), Some("default"));
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

/// ★ 回包逐字节（线上形挪进 `cmd-wire` 前后同一份字节）：键序 · `null` 的格照出 · `why` · `said` 缺席不出；
/// 手机按 `cmd_wire::replies::decode_reply` 解得开、按类型重写出来是同一份。
#[test]
fn the_reply_bytes_are_pinned_and_the_phone_reads_them_back() {
    let mut f = Fake::new(&["default", "acceptEdits", "plan"], 0);
    let done = run("plan", false, &mut f);
    let mut f = Fake::new(&["default", "acceptEdits"], 0);
    let refused = run("acceptEdits", true, &mut f);
    let said = crate::control::terminals::no_said(crate::control::terminals::Refused::Waiting);
    for (got, want) in [
        (
            done,
            r#"{"result":"done","mode":"plan","modeText":null}"#.to_string(),
        ),
        (
            refused,
            format!(
                r#"{{"result":"refused","mode":null,"modeText":null,"why":"waiting","said":"{said}"}}"#
            ),
        ),
    ] {
        let bytes = serde_json::to_string(&got).unwrap();
        assert_eq!(bytes, want);
        let back = cmd_wire::replies::decode_reply(
            "session-permission",
            serde_json::from_str(&bytes).unwrap(),
        )
        .expect("表里有 session-permission")
        .expect("解得开");
        assert_eq!(
            back,
            serde_json::from_str::<serde_json::Value>(&want).unwrap()
        );
    }
}

/// ★ 这台已经没有它的终端（名单里没有 · 只剩 shell）⇒ 一下都不按、读也不读，记下那一档、回切到了；
/// 别的不收（不归我们）照实回被拒、不记。
#[test]
fn a_session_without_its_terminal_keeps_the_mode_for_the_next_resume() {
    use crate::control::terminals::Refused;
    use std::cell::Cell;
    for why in [Refused::Ended, Refused::NotKnown] {
        let mut f = Fake::new(&["default", "acceptEdits", "plan"], 0);
        f.gone = Some(why);
        let kept = Cell::new(0);
        let got = switch_with("plan", &SW, false, &mut f, &|| {
            kept.set(kept.get() + 1);
            Ok(())
        })
        .unwrap();
        assert_eq!(got.result, SwitchResult::Done);
        assert_eq!(got.mode.as_deref(), Some("plan"));
        assert_eq!((got.why, got.said), (None, None));
        assert_eq!((kept.get(), f.presses), (1, 0));
    }
    let mut f = Fake::new(&["default", "plan"], 0);
    f.gone = Some(Refused::NotManaged);
    let got = run("plan", false, &mut f);
    assert_eq!(got.result, SwitchResult::Refused);
    assert_eq!(got.why, Some(Refused::NotManaged));
}

/// ★ 收哪几档：那一家循环里有的（含 `auto`）都收；认得出、却只能起会话时带参数定的（越过批准那几档）⇒ `launch_only` ＋ 说得清的那一句，
/// 不是「请求格式不对」；认不出的 ⇒ `bad_args`。
#[test]
fn auto_is_offered_and_a_launch_only_mode_says_so() {
    let (_, claude) = crate::agents::pick_kind(Some("claude")).unwrap();
    let perm = claude.permission.unwrap();
    for m in ["default", "acceptEdits", "plan", "auto"] {
        assert!(offered(&perm, m).is_ok(), "`{m}` 不收");
    }
    for m in ["bypassPermissions", "dontAsk"] {
        let e = offered(&perm, m).unwrap_err();
        assert_eq!(e.code, "launch_only", "`{m}`");
        assert_eq!(
            e.message,
            copy_core::copy_text("beSessionMode.switch.launchOnly", &[])
        );
    }
    assert_eq!(offered(&perm, "yolo").unwrap_err().code, "bad_args");
}
