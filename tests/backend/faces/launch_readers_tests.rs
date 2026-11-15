//! 起会话面新形的读者那一侧：模型清单真跑一个回显的小程序读回 · 起会话框的模型与权限档照新形出 · 换档投命令那一形送的恰是那一句。
//! 一律拿替身家（`fake::LAUNCH`）喂；回显的小程序是这里写出来的 sh 脚本（只在 unix 跑），不起真的 agent。

use super::*;
use crate::agents::{fake, ListedModel};
use crate::control::terminals::{input_reply, Refused};
use crate::control::terminals_wire::{Sent, SentResult};
use crate::faces::session_mode_face::{command_with, SayIo, SwitchResult};
use crate::faces::session_model_face::switch_line;

fn brains() -> Vec<ListedModel> {
    vec![
        ListedModel {
            value: "f-small".into(),
            text: "Small".into(),
            efforts: vec!["low".into(), "high".into()],
            default_effort: Some("low".into()),
        },
        ListedModel {
            value: "f-big".into(),
            text: "Big Brain".into(),
            efforts: vec![],
            default_effort: None,
        },
    ]
}

/// 一个回显的小程序：只认替身家声明的那几个词（别的词 ⇒ 退 3），打 `out` 那几行、退 `code`。
#[cfg(unix)]
fn echo_program(tag: &str, out: &[&str], code: i32) -> (std::path::PathBuf, String) {
    let d = std::env::temp_dir().join(format!("ccm-model-list-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join("fakeagent");
    let lines: Vec<String> = out.iter().map(|l| format!("'{l}'")).collect();
    // 马上要 exec 它：经 test-home 写（本进程从没有它的写 fd，旁边线程正 fork 时不撞 ETXTBSY）。
    test_home::write_exe(
        &p,
        &format!(
            "#!/bin/sh\n[ \"$*\" = 'brains --offline' ] || exit 3\nprintf '%s\\n' {}\nexit {code}\n",
            lines.join(" ")
        ),
    );
    let prog = p.display().to_string();
    (d, prog)
}

/// ★ 模型清单：照替身家声明的那条子命令跑回显的小程序，读回它列的那几个（先后照它）、各带推理档与缺省档。
#[cfg(unix)]
#[test]
fn the_model_list_runs_the_declared_subcommand_and_reads_back_what_it_lists() {
    let (d, prog) = echo_program(
        "ok",
        &["f-small|Small|low,high|low", "f-big|Big Brain||"],
        0,
    );
    assert_eq!(listed_models_via(&fake::LAUNCH, &prog), Some(brains()));
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 退出非零 · 输出读不成 · 词不对（小程序退 3）⇒ `None`。
#[cfg(unix)]
#[test]
fn a_failing_or_unreadable_model_list_reads_as_none() {
    let (d, prog) = echo_program("fail", &["f-small|Small|low|low"], 1);
    assert_eq!(listed_models_via(&fake::LAUNCH, &prog), None, "退出非零");
    let _ = std::fs::remove_dir_all(&d);
    let (d, prog) = echo_program("junk", &["not a list"], 0);
    assert_eq!(listed_models_via(&fake::LAUNCH, &prog), None, "读不成");
    let _ = std::fs::remove_dir_all(&d);
    let other = crate::agents::LaunchFace {
        model_list: Some(crate::agents::ModelList {
            args: &["models"],
            ..fake::LAUNCH.model_list.unwrap()
        }),
        ..fake::LAUNCH
    };
    let (d, prog) = echo_program("words", &["f-small|Small|low|low"], 0);
    assert_eq!(listed_models_via(&other, &prog), None, "词不是声明的那几个");
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 程序不在 ⇒ `None`；没声明清单那一格 ⇒ `None`（一个进程都不起）。
#[test]
fn a_missing_program_or_no_declared_list_reads_as_none() {
    let gone = std::env::temp_dir()
        .join(format!("ccm-model-list-gone-{}", std::process::id()))
        .join("fakeagent");
    assert_eq!(
        listed_models_via(&fake::LAUNCH, &gone.display().to_string()),
        None
    );
    let bare = crate::agents::LaunchFace {
        model_list: None,
        ..fake::LAUNCH
    };
    assert_eq!(listed_models_via(&bare, &gone.display().to_string()), None);
}

/// ★ 起会话框的模型：清单读得成照清单（值 ＋ 那一家写的名字），读不成照静态的那一张；权限档只列给选的（只认不选的不列）。
#[test]
fn the_new_session_facts_take_the_listed_models_or_else_the_static_ones() {
    let picks = |f: &AgentFacts| -> Vec<(String, String)> {
        f.models
            .iter()
            .map(|p| (p.value.clone(), p.text.clone()))
            .collect()
    };
    let listed = facts_of(fake::two_families(), "fake", &|_| Some(brains()));
    assert_eq!(
        picks(&listed),
        [
            ("f-small".to_string(), "Small".to_string()),
            ("f-big".to_string(), "Big Brain".to_string())
        ]
    );
    // 每个模型带它能选的推理档（原值 ＋ 后端写好的字，那一家排的先后）与缺省档；不分档 ⇒ 空（这个模型不给选）。
    let effort_text =
        |e: &str| copy_core::copy_text("beSessionModel.effort.item", &[("effort", e)]);
    let efforts: Vec<(Vec<(String, String)>, Option<String>)> = listed
        .models
        .iter()
        .map(|p| {
            (
                p.efforts
                    .iter()
                    .map(|e| (e.value.clone(), e.text.clone()))
                    .collect(),
                p.default_effort.clone(),
            )
        })
        .collect();
    assert_eq!(
        efforts,
        [
            (
                vec![
                    ("low".to_string(), effort_text("low")),
                    ("high".to_string(), effort_text("high"))
                ],
                Some("low".to_string())
            ),
            (vec![], None)
        ],
        "每个模型带它的推理档（带字）与缺省档"
    );
    assert!(
        listed.models[0]
            .efforts
            .iter()
            .all(|e| e.efforts.is_empty()),
        "推理档那一项自己不再带推理档"
    );
    // 那一家说的缺省档不在那个模型列着的档里（或那个模型不分档）⇒ 不出缺省档。
    let odd = facts_of(fake::two_families(), "fake", &|_| {
        Some(vec![
            ListedModel {
                value: "f-odd".into(),
                text: "Odd".into(),
                efforts: vec!["low".into()],
                default_effort: Some("max".into()),
            },
            ListedModel {
                value: "f-flat".into(),
                text: "Flat".into(),
                efforts: vec![],
                default_effort: Some("low".into()),
            },
        ])
    });
    assert!(
        odd.models.iter().all(|m| m.default_effort.is_none()),
        "缺省档只出列着的那几档之一"
    );
    let wire = serde_json::to_value(&listed.models[1]).unwrap();
    assert_eq!(
        wire.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["text", "value"],
        "不分档的那一项线上不出推理档那两格"
    );
    assert_eq!(
        serde_json::to_value(&listed.models[0]).unwrap()["defaultEffort"],
        "low"
    );
    let fallback = facts_of(fake::two_families(), "fake", &|_| None);
    assert_eq!(
        picks(&fallback),
        [("f-static".to_string(), "Static".to_string())]
    );
    let modes: Vec<(&str, &str)> = listed
        .permission_modes
        .iter()
        .map(|p| (p.value.as_str(), p.text.as_str()))
        .collect();
    assert_eq!(modes, [("ask", "Ask"), ("roam", "Roam")]);
    let unknown = facts_of(fake::two_families(), "gone", &|_| Some(brains()));
    assert!(unknown.models.is_empty() && unknown.permission_modes.is_empty());
}

/// 换模型收哪几个：可选的（清单或静态）·「默认」那一项；别的不收（`bad_args`）。
#[test]
fn a_switch_takes_an_offered_model_or_the_default_only() {
    let offered = brains();
    let with_default = crate::agents::LaunchFace {
        model_default: Some(crate::agents::Choice {
            value: "default",
            text: || "Default".to_string(),
        }),
        ..fake::LAUNCH
    };
    let line = |f: &crate::agents::LaunchFace, m: &str| {
        switch_line(f, &f.model_switch.unwrap(), &offered, m, None)
            .map_err(|s| s.code.unwrap_or_default())
    };
    assert_eq!(
        line(&with_default, "f-big").unwrap(),
        ("Big Brain".to_string(), ":brain f-big".to_string())
    );
    assert_eq!(line(&with_default, "default").unwrap().0, "Default");
    assert_eq!(line(&with_default, "f-static").unwrap_err(), "bad_args");
    assert_eq!(line(&fake::LAUNCH, "default").unwrap_err(), "bad_args");
}

/// ★ 换模型带上推理档：那个模型列着的档 ⇒ 送的那一句恰带它；不给 ⇒ 不带（那一家照它的缺省）；
/// 那个模型没列这一档 · 那个模型不分档 ⇒ `bad_args`，一句不送。
#[test]
fn a_switch_carries_the_effort_into_the_line_only_when_the_model_lists_it() {
    let offered = brains();
    let line = |m: &str, e: Option<&str>| {
        switch_line(
            &fake::LAUNCH,
            &fake::LAUNCH.model_switch.unwrap(),
            &offered,
            m,
            e,
        )
        .map_err(|s| s.code.unwrap_or_default())
    };
    assert_eq!(
        line("f-small", Some("high")).unwrap(),
        ("Small".to_string(), ":brain f-small @high".to_string())
    );
    assert_eq!(line("f-small", None).unwrap().1, ":brain f-small");
    assert_eq!(line("f-small", Some("max")).unwrap_err(), "bad_args");
    assert_eq!(line("f-big", Some("low")).unwrap_err(), "bad_args");
}

/// 投命令那一形的合成终端：记下送进去的每一句，交回给定的结局。
struct Said1 {
    lines: Vec<String>,
    result: SentResult,
    why: Option<Refused>,
}

impl SayIo for Said1 {
    fn say(&mut self, line: &str) -> Result<Sent, crate::common::said::Said> {
        self.lines.push(line.to_string());
        Ok(input_reply(self.result, self.why, None))
    }
}

fn said1(result: SentResult, why: Option<Refused>) -> Said1 {
    Said1 {
        lines: Vec::new(),
        result,
        why,
    }
}

/// ★ 投命令那一形：送的恰是那一家给这一档的那一句、只送一次；送到了也读不回此刻的档 ⇒ 结果未知（不说切到了）。
#[test]
fn the_command_form_sends_exactly_the_line_and_says_it_cannot_read_back() {
    let perm = fake::LAUNCH.permission.unwrap();
    let line = perm.switch_line("roam").expect("给选的档有那一句");
    let mut io = said1(SentResult::Delivered, None);
    let got = command_with("roam", &line, false, &mut io, &|| panic!("记下了")).unwrap();
    assert_eq!(io.lines, [":gate roam"]);
    assert_eq!(got.result, SwitchResult::Unsure);
    assert_eq!(got.mode, None);
    assert_eq!(
        got.said.as_deref(),
        Some(copy_core::copy_text("beSessionMode.unsure.unread", &[]).as_str())
    );
}

/// ★ 在等人 ⇒ 一句不送；终端没了 ⇒ 记下、等恢复照它起，回切到了；送不进去 ⇒ 照那一道门的结局。
#[test]
fn the_command_form_waits_keeps_and_refuses_like_say() {
    let mut io = said1(SentResult::Delivered, None);
    let got = command_with("ask", ":gate ask", true, &mut io, &|| panic!("记下了")).unwrap();
    assert!(io.lines.is_empty(), "在等人还送了");
    assert_eq!(
        (got.result, got.why),
        (SwitchResult::Refused, Some(Refused::Waiting))
    );
    let kept = std::cell::Cell::new(0);
    let keep = || {
        kept.set(kept.get() + 1);
        Ok(())
    };
    let mut io = said1(SentResult::Refused, Some(Refused::Ended));
    let got = command_with("ask", ":gate ask", false, &mut io, &keep).unwrap();
    assert_eq!(kept.get(), 1);
    assert_eq!(
        (got.result, got.mode.as_deref()),
        (SwitchResult::Done, Some("ask"))
    );
    let mut io = said1(SentResult::Refused, Some(Refused::NotYours));
    let got = command_with("ask", ":gate ask", false, &mut io, &|| panic!("记下了")).unwrap();
    assert_eq!(
        (got.result, got.why),
        (SwitchResult::Refused, Some(Refused::NotYours))
    );
}
