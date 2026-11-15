//! 会话开着时换模型：在等人不送；送了那一行没送到 ⇒ 照那一下的结局回；送到了 ⇒ 等记录里读回，读到才算换了，没读到 ⇒ 结果未知。

use super::*;
use crate::agents::ModelSaid;
use crate::control::terminals_wire::{Sent, SentResult};

fn sent(result: SentResult) -> Sent {
    crate::control::terminals::input_reply(
        result,
        (result == SentResult::Refused).then_some(crate::control::terminals::Refused::NotYours),
        None,
    )
}

/// ★ 在等人 ⇒ 不送，回被拒 ＋ `waiting` ＋ 那一句。
#[test]
fn a_waiting_session_gets_nothing_sent() {
    let got = waiting_reply();
    assert_eq!(got.result, SwitchResult::Refused);
    assert_eq!(got.why, Some(crate::control::terminals::Refused::Waiting));
    assert!(got.said.is_some() && got.model.is_none());
}

/// ★ 送到了之后：那一家不声明读回（替身家）⇒ 当场回「已发出」＋ 后端写好的那一句，不等；声明了读回 ⇒ 照旧等读回。
#[test]
fn a_delivered_line_without_a_declared_readback_is_sent_at_once() {
    let sw = crate::agents::fake::LAUNCH
        .model_switch
        .expect("替身家换得了模型");
    assert!(sw.read.is_none(), "替身家换模型不写读回");
    match after_delivery(sw.read.map(|_| ()), "Big Brain", false) {
        Sent2::Done(r) => {
            assert_eq!(r.result, SwitchResult::Sent);
            assert_eq!((r.model.as_deref(), r.why), (None, None));
            assert_eq!(
                r.said.as_deref(),
                Some(
                    copy_core::copy_text("beSessionModel.set.sent", &[("model", "Big Brain")])
                        .as_str()
                )
            );
            let v = serde_json::to_value(&r).unwrap();
            assert_eq!(v["result"], "sent");
        }
        Sent2::Wait(..) => panic!("不声明读回还在等"),
    }
    let reads = crate::agents::ModelSwitch {
        read: Some(|_| None),
        ..sw
    };
    assert!(
        matches!(
            after_delivery(reads.read.map(|_| ()), "Big Brain", true),
            Sent2::Wait((), true)
        ),
        "声明了读回 ⇒ 照旧等"
    );
}

/// 不该走到记下那一支。
fn no_keep() -> Result<String, crate::common::said::Said> {
    panic!("记下了")
}

/// ★ 那一行送到了 ⇒ 接着等读回（`None`）；没送到 · 不知道送没送到 ⇒ 就是结局，不等。
#[test]
fn only_a_delivered_line_waits_for_the_readback() {
    assert!(after_send(sent(SentResult::Delivered), &no_keep)
        .unwrap()
        .is_none());
    let r = after_send(sent(SentResult::Refused), &no_keep)
        .unwrap()
        .unwrap();
    assert_eq!(r.result, SwitchResult::Refused);
    assert_eq!(r.why, Some(crate::control::terminals::Refused::NotYours));
    assert_eq!(
        after_send(sent(SentResult::Unsure), &no_keep)
            .unwrap()
            .unwrap()
            .result,
        SwitchResult::Unsure
    );
}

/// ★ 这台已经没有它的终端（名单里没有 · 只剩 shell）⇒ 记在这条会话上、回换成了 ＋ 清单里那一项的字，不等读回、不给那一句；
/// 注解里那一格就是选的原值、`end` ＝ 此刻记录多长。
#[test]
fn a_session_without_its_terminal_keeps_the_model_for_the_next_resume() {
    use crate::control::terminals::Refused;
    let d = std::env::temp_dir().join(format!("ccm-model-kept-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let rec = d.join("s.jsonl");
    std::fs::write(&rec, "{\"i\":0}\n").unwrap();
    let own = crate::platform::fs::Own::issue_data_at(&d);
    let sid = "11111111-2222-3333-4444-555555555555";
    let (_, claude) = crate::agents::pick_kind(Some("claude")).unwrap();
    let sonnet = claude.model_word("sonnet", None).unwrap();
    for why in [Refused::Ended, Refused::NotKnown] {
        let keep = || {
            crate::history::history_annotations::keep_for_resume(
                &own,
                sid,
                &rec,
                Some("sonnet"),
                None,
            )
            .map(|()| sonnet.clone())
        };
        let gone = crate::control::terminals::input_reply(SentResult::Refused, Some(why), None);
        let r = after_send(gone, &keep).unwrap().unwrap();
        assert_eq!(r.result, SwitchResult::Done);
        assert_eq!(r.model, Some(sonnet.clone()));
        assert_eq!((r.why, r.said), (None, None));
    }
    let got = crate::history::history_annotations::resume_with_at(
        &d.join(relay_route_core::file_name_of(
            relay_route_core::HISTORY_ANNOTATIONS_REL,
        )),
        sid,
    )
    .unwrap();
    assert_eq!(got.model.as_deref(), Some("sonnet"));
    assert_eq!(got.end, std::fs::metadata(&rec).unwrap().len());
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 读回：换成了 ⇒ done ＋ 显示名 ＋ 那一句（照实说也存成了这个号新会话的默认）；没变 ⇒ done ＋ 「未变」那一句；
/// 没读到 ⇒ 结果未知，不猜。
#[test]
fn the_readback_decides_and_silence_is_unsure() {
    let set = settled(
        Some(ModelSaid {
            name: "Opus 5.5 (1M context)".into(),
            kept: false,
        }),
        false,
    );
    assert_eq!(set.result, SwitchResult::Done);
    assert_eq!(set.model.as_deref(), Some("Opus 5.5 (1M context)"));
    assert_eq!(
        set.said.as_deref(),
        Some(
            copy_core::copy_text(
                "beSessionModel.set.saved",
                &[("model", "Opus 5.5 (1M context)")]
            )
            .as_str()
        )
    );
    let kept = settled(
        Some(ModelSaid {
            name: "Fable 5.1".into(),
            kept: true,
        }),
        true,
    );
    assert_eq!(kept.result, SwitchResult::Done);
    assert_eq!(
        kept.said.as_deref(),
        Some(copy_core::copy_text("beSessionModel.set.kept", &[("model", "Fable 5.1")]).as_str())
    );
    let none = settled(None, false);
    assert_eq!(none.result, SwitchResult::Unsure);
    assert!(none.model.is_none());
    assert_eq!(
        none.said.as_deref(),
        Some(copy_core::copy_text("beSessionModel.unsure.noReadback", &[]).as_str())
    );
}

/// ★ 线上那一形：没有的格不出（`why` · `said`），`model` 恒在（读不到 ⇒ `null`）。
#[test]
fn the_reply_wire_names_its_cells() {
    let v = serde_json::to_value(settled(None, false)).unwrap();
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["model", "result", "said"]);
    assert_eq!(v["result"], "unsure");
}

/// ★ 会话开着时换模型收的值：那一家可选的模型 ＋ 「默认」那一项的值（回到它自己的缺省模型）；别的不收。
#[test]
fn the_default_item_is_a_model_the_session_takes() {
    let (_, claude) = crate::agents::pick_kind(Some("claude")).unwrap();
    assert!(claude.model_word("default", None).is_some());
    assert!(claude.model_word("opus", None).is_some());
    assert!(claude.model_word("nope", None).is_none());
    let (_, codex) = crate::agents::pick_kind(Some("codex")).unwrap();
    assert!(codex.model_word("default", None).is_none());
}

/// ★ 换成了那一句照链接布局说：那份设置各号共用 ⇒「各号新会话的默认」，各自一份 ⇒「此号新会话的默认」。
#[test]
fn the_saved_sentence_follows_whether_the_settings_are_shared() {
    let said = |shared: bool| {
        settled(
            Some(ModelSaid {
                name: "Sonnet 5.5".into(),
                kept: false,
            }),
            shared,
        )
        .said
        .unwrap()
    };
    let m = [("model", "Sonnet 5.5")];
    assert_eq!(
        said(false),
        copy_core::copy_text("beSessionModel.set.saved", &m)
    );
    assert_eq!(
        said(true),
        copy_core::copy_text("beSessionModel.set.savedShared", &m)
    );
    assert_ne!(said(true), said(false));
}

/// 合成的终端：`screens` 是送出那一行之后依次重画出的几屏（重画完就不再变），`arrive_at` 为第几次问时读回已到（`None` ＝ 没到）。
struct Term {
    screens: Vec<Vec<String>>,
    at: usize,
    asked: usize,
    arrive_at: Option<usize>,
    pressed: Vec<&'static str>,
    sent: SentResult,
}

impl Term {
    fn new(screens: &[&str]) -> Term {
        Term {
            screens: screens
                .iter()
                .map(|s| s.lines().map(str::to_string).collect())
                .collect(),
            at: 0,
            asked: 0,
            arrive_at: None,
            pressed: Vec::new(),
            sent: SentResult::Delivered,
        }
    }
}

impl ConfirmIo for Term {
    fn arrived(&mut self) -> Option<ModelSaid> {
        self.asked += 1;
        (self.arrive_at.is_some_and(|n| self.asked >= n)).then(|| ModelSaid {
            name: "Sonnet 5.5".into(),
            kept: false,
        })
    }
    fn redrawn(&mut self) -> bool {
        if self.at + 1 < self.screens.len() {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn screen(&mut self) -> Result<Vec<String>, crate::common::said::Said> {
        Ok(self.screens[self.at].clone())
    }
    fn press(&mut self, key: &'static str) -> Result<Sent, crate::common::said::Said> {
        self.pressed.push(key);
        Ok(sent(self.sent))
    }
}

const BOX: crate::agents::ScreenBox = crate::agents::ScreenBox {
    seen: |lines| lines.iter().any(|l| l.contains("Switch model?")),
    key: "enter",
};

/// ★ 送出那一行之后弹了认得出的确认框 ⇒ 按一下确认键（只一下）、接着等读回；读回先到 ⇒ 不按；认不出的框 ⇒ 不按、照旧等读回（读不回就是结果未知）。
#[test]
fn a_known_box_is_confirmed_once_and_anything_else_is_left_alone() {
    let mut b = Term::new(&["> /model sonnet", "   Switch model?\n   ❯ 1. Yes"]);
    assert!(matches!(
        confirm_with(&BOX, &mut b).unwrap(),
        Confirmed::Pressed
    ));
    assert_eq!(b.pressed, vec!["enter"]);

    let mut b = Term::new(&["> /model sonnet", "   Switch model?"]);
    b.arrive_at = Some(1);
    match confirm_with(&BOX, &mut b).unwrap() {
        Confirmed::Arrived(s) => assert_eq!(s.name, "Sonnet 5.5"),
        _ => panic!("读回先到却没交出来"),
    }
    assert!(b.pressed.is_empty(), "读回先到还按了");

    let mut b = Term::new(&["> /model sonnet", "   Do you want to proceed?"]);
    assert!(matches!(
        confirm_with(&BOX, &mut b).unwrap(),
        Confirmed::Nothing
    ));
    assert!(b.pressed.is_empty(), "认不出的框也按了");
}

/// ★ 确认那一下没送到 · 不知道送没送到 ⇒ 就是结局（照那一下回），不再等读回。
#[test]
fn a_confirm_press_that_did_not_land_ends_it() {
    for (r, want) in [
        (SentResult::Refused, SwitchResult::Refused),
        (SentResult::Unsure, SwitchResult::Unsure),
    ] {
        let mut b = Term::new(&["   Switch model?"]);
        b.sent = r;
        match confirm_with(&BOX, &mut b).unwrap() {
            Confirmed::Ended(m) => assert_eq!(m.result, want),
            _ => panic!("没送到那一下没成结局"),
        }
    }
}

/// ★ 会话开着时替人送字一律经读屏驱动（送前忘掉此刻之前的变化）：faces 生产段里直调 `terminals_wire::say` 的只有送一句那一道
/// （`session_send.rs`，它就是 `session-say` 本身）。
#[test]
fn faces_send_through_the_screen_drive_except_session_say() {
    let dir = guard_core::lexically_normalized(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
    let mut seen = 0;
    let mut bad = Vec::new();
    for (p, src) in guard_core::scan_tree!(&dir, &["rs"]) {
        let p = guard_core::lexically_normalized(&p);
        if p.parent() != Some(dir.as_path()) {
            continue;
        }
        seen += 1;
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let prod = guard_core::production_code(&src);
        if guard_core::contains_word(&prod, "terminals_wire::say") && name != "session_send.rs" {
            bad.push(name);
        }
    }
    assert!(
        seen > 30,
        "faces 目录只读到 {seen} 份 —— 住址不对，零命中不作数"
    );
    assert!(bad.is_empty(), "这几份绕过读屏驱动直接送字：{bad:?}");
}
/// ★ 换模型照按 sid 找到它的那一家认：那一家没声明换模型 ⇒ 不支持（不落到默认那一家去送）；声明了 ⇒ 就是它那一格。
#[test]
fn the_switch_follows_the_family_the_session_was_found_in() {
    let (reg, at) = found_in_the_other("model-none", None);
    let got = switch_face_among(&reg, &at).map(|(f, _)| f.adapter_id);
    assert_eq!(
        got.map_err(|s| s.code),
        Err(Some("unsupported".to_string())),
        "那一家没声明换模型 ⇒ 不支持，不照默认那一家送"
    );
    let (reg, at) = found_in_the_other("model-own", crate::agents::fake::LAUNCH.model_switch);
    let got = switch_face_among(&reg, &at).map(|(f, _)| f.adapter_id);
    assert_eq!(got.ok(), Some("other"), "声明了 ⇒ 照那一家的起会话事实");
}

/// 会话落在 `other` 那一家（照路径认不出），`other` 换模型那一格照 `switch`；默认那一家换得了模型（[`crate::agents::fake::found_in_the_other`]）。
fn found_in_the_other(
    tag: &str,
    switch: Option<crate::agents::ModelSwitch>,
) -> (Vec<crate::agents::Adapter>, crate::agents::SessionAt) {
    assert!(
        crate::agents::fake::LAUNCH.model_switch.is_some(),
        "默认那一家换得了模型"
    );
    crate::agents::fake::found_in_the_other(tag, |f| crate::agents::LaunchFace {
        model_switch: switch,
        ..f
    })
}

/// 替身终端：记下送了哪几句 · 按了哪几个键；送一律送到，屏是空的、不重画。
#[derive(Default)]
struct Keys {
    said: Vec<String>,
    pressed: Vec<&'static str>,
}

impl ModelDrive for Keys {
    fn open(&mut self) -> Result<Option<crate::control::terminals::Refused>, Said> {
        Ok(None)
    }
    fn say(&mut self, line: &str) -> Result<Sent, Said> {
        self.said.push(line.to_string());
        Ok(sent(SentResult::Delivered))
    }
    fn redrawn(&mut self) -> bool {
        false
    }
    fn screen(&mut self) -> Result<Vec<String>, Said> {
        Ok(Vec::new())
    }
    fn press(&mut self, key: &'static str) -> Result<Sent, Said> {
        self.pressed.push(key);
        Ok(sent(SentResult::Delivered))
    }
}

/// 照 `at` 那一份送一次（模型取替身家静态那一张的头一项，不列清单），回 结局 ＋ 替身终端收到的。
fn send_once(
    reg: &[crate::agents::Adapter],
    at: &crate::agents::SessionAt,
) -> (Result<ModelSwitched, Option<String>>, Keys) {
    let model = crate::agents::fake::LAUNCH
        .models
        .and_then(|m| m.first())
        .expect("替身家有静态模型")
        .value
        .to_string();
    let req = SessionModelReq {
        sid: "00000000-0000-4000-8000-0000000000c7".parse().expect("sid"),
        model,
        client: None,
        effort: None,
    };
    let homes = [crate::agents::AgentHome {
        kind: at.kind,
        home: at.home.clone(),
    }];
    let own = crate::platform::Own::issue_at(&at.home);
    let mut keys = Keys::default();
    let got = send_among(reg, at, &req, homes.as_slice(), &own, &|_| None, &mut keys)
        .map(|s| match s {
            Sent2::Done(r) => r,
            Sent2::Wait(..) => panic!("替身家不写读回，不该等"),
        })
        .map_err(|s| s.code);
    (got, keys)
}

/// ★ 送那一层：会话那一家不支持换模型 ⇒ 不支持，终端一个键都没收到（不照默认那一家送它那一句）；
/// 支持 ⇒ 送的就是那一家那一句。
#[test]
fn an_unsupported_family_gets_not_one_key() {
    let (reg, at) = found_in_the_other("send-none", None);
    let (got, keys) = send_once(&reg, &at);
    assert_eq!(got.err(), Some(Some("unsupported".to_string())));
    assert!(
        keys.said.is_empty() && keys.pressed.is_empty(),
        "那一家不支持还送了：{:?} {:?}",
        keys.said,
        keys.pressed
    );
    let (reg, at) = found_in_the_other("send-own", crate::agents::fake::LAUNCH.model_switch);
    let (got, keys) = send_once(&reg, &at);
    assert_eq!(got.map(|r| r.result).ok(), Some(SwitchResult::Sent));
    assert_eq!(keys.said.len(), 1, "支持 ⇒ 送那一句");
    assert!(
        keys.said[0].starts_with(":brain "),
        "送的是那一家的那一句：{:?}",
        keys.said
    );
}
