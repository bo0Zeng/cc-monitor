//! 回退那一串（`control/rewind.rs`）：拿假 agent（测试档那一家，屏字 · 键与 Claude 都不同形）的终端跑，不起真 agent。

use super::*;
use crate::agents::fake::rewind::{FakeTerm, FACE};

fn edit(to: &str, how: RewindHow, when: RewindWhen) -> Rewind {
    Rewind {
        to: to.to_string(),
        how,
        when,
    }
}

fn run(term: &mut FakeTerm, r: &Rewind, text: &str) -> Walked {
    walk(&FACE, Plan { rewind: r, text }, term).expect("假终端不出命令级错")
}

fn texts(term: &FakeTerm) -> Vec<&str> {
    term.said.iter().map(|l| l.text.as_str()).collect()
}

/// 第一条：说了三句，在第二句上编辑成「改过的」、选只退对话 ⇒ 按出那一家申报的那一串、主线读回 ＝ 第一句 ＋「改过的」。
#[test]
fn editing_the_second_of_three_lines_rewinds_there_and_says_the_new_one() {
    let mut t = FakeTerm::with(&["第一句", "第二句是一句很长很长的话", "第三句"]);
    let got = run(
        &mut t,
        &edit("u2", RewindHow::Conversation, RewindWhen::AfterTurn),
        "改过的",
    );
    assert_eq!(got, Walked::Done);
    assert_eq!(texts(&t), ["第一句", "改过的"]);
    assert_eq!(t.said[0].uuid, "u1", "第一句还是原来那一条");
    // 调出（两下）→ 往上两行到第二句 → 进确认页 → 选中的就是「只退对话」直接确认。
    assert_eq!(t.keys, ["esc", "esc", "up", "up", "enter", "enter"]);
    assert_eq!(t.restored, ["talk only"]);
    assert_eq!(t.typed_in, ["改过的"]);
}

/// 回退列表换了样子（那一家升级了）⇒ 调出那两下之后认不出，不按 ↑ 不按 Enter、回没退成；改好的那句没送。
#[test]
fn a_menu_that_looks_different_gets_no_further_key_and_says_not_rewound() {
    let mut t = FakeTerm::with(&["一", "二", "三"]);
    t.new_look = true;
    let got = run(
        &mut t,
        &edit("u2", RewindHow::Conversation, RewindWhen::AfterTurn),
        "改过的",
    );
    assert_eq!(got, Walked::No(Why::MenuUnlike));
    assert_eq!(t.keys, ["esc", "esc"], "认不出的那一屏上一个键都不按");
    assert!(t.typed_in.is_empty());
    assert_eq!(texts(&t), ["一", "二", "三"]);
}

/// 一开始就认不出那一屏（弹着别的框）⇒ 一个键都不按。
#[test]
fn an_unknown_screen_at_the_start_gets_no_key_at_all() {
    let mut t = FakeTerm::with(&["一", "二"]);
    t.new_look = true;
    // 先把终端摆到列表上（换了样子的那一屏），再交给那一串。
    let _ = RewindIo::press(&mut t, "esc");
    let _ = RewindIo::press(&mut t, "esc");
    t.keys.clear();
    let got = run(
        &mut t,
        &edit("u1", RewindHow::Conversation, RewindWhen::AfterTurn),
        "x",
    );
    assert_eq!(got, Walked::No(Why::MenuUnlike));
    assert!(t.keys.is_empty());
}

/// 两下 Esc 被并成一下、列表没出来 ⇒ 不按 ↑（那会调出输入历史）、不按 Enter（那会把历史里那句重发），回没退成。
#[test]
fn when_the_list_does_not_open_no_up_and_no_enter_is_pressed() {
    let mut t = FakeTerm::with(&["一", "二", "三"]);
    t.merge_open = true;
    let got = run(
        &mut t,
        &edit("u2", RewindHow::Conversation, RewindWhen::AfterTurn),
        "改过的",
    );
    assert_eq!(got, Walked::No(Why::MenuClosed));
    assert_eq!(t.keys, ["esc", "esc"]);
    assert_eq!(texts(&t), ["一", "二", "三"], "没有一句被重发");
    assert!(t.typed.is_empty());
}

/// 选了对话和代码一起退、这一句的确认页上没有那一项 ⇒ 不按确认，退回输入行，回没退成（点名是哪一项）。
#[test]
fn a_choice_missing_from_the_confirm_page_backs_out_without_confirming() {
    let mut t = FakeTerm::with(&["一", "二", "三"]);
    let got = run(
        &mut t,
        &edit("u2", RewindHow::CodeAndConversation, RewindWhen::AfterTurn),
        "改过的",
    );
    assert_eq!(
        got,
        Walked::No(Why::NoChoice(RewindHow::CodeAndConversation))
    );
    assert!(t.restored.is_empty(), "一回都没退");
    assert_eq!(t.keys, ["esc", "esc", "up", "up", "enter", "left", "left"]);
    assert_eq!(texts(&t), ["一", "二", "三"]);
    assert!(
        matches!((FACE.read)(&t.drawn()), RewindScreen::Prompt { .. }),
        "退回了输入行"
    );
}

/// 那一句的确认页上有「连文件一起」⇒ 移到那一项（选中的正是它）才确认。
#[test]
fn the_chosen_item_is_selected_before_the_final_enter() {
    let mut t = FakeTerm::with(&["一", "二", "三"]);
    t.said[1].files = true;
    let got = run(
        &mut t,
        &edit("u2", RewindHow::CodeAndConversation, RewindWhen::AfterTurn),
        "改过的",
    );
    assert_eq!(got, Walked::Done);
    assert_eq!(t.restored, ["talk + files"]);
    assert_eq!(t.keys, ["esc", "esc", "up", "up", "enter", "down", "enter"]);
    assert_eq!(texts(&t), ["一", "改过的"]);
}

/// 主 agent 在出字、选了等这一轮完 ⇒ 一个键不按，回等（输入队照排着）。
#[test]
fn busy_and_after_turn_waits_without_a_key() {
    let mut t = FakeTerm::with(&["一", "二"]);
    t.busy = true;
    let got = run(
        &mut t,
        &edit("u1", RewindHow::Conversation, RewindWhen::AfterTurn),
        "改过的",
    );
    assert_eq!(got, Walked::Wait);
    assert!(t.keys.is_empty());
}

/// 主 agent 在出字、选了现在停 ⇒ 先按打断；被打断的那一句放回输入行 ⇒ 删掉、确认空了再退。
#[test]
fn busy_and_now_interrupts_then_clears_the_returned_line_then_rewinds() {
    let mut t = FakeTerm::with(&["一", "二"]);
    t.busy = true;
    t.interrupted = Some("半句".to_string());
    let got = run(
        &mut t,
        &edit("u1", RewindHow::Conversation, RewindWhen::Now),
        "改过的",
    );
    assert_eq!(got, Walked::Done);
    assert_eq!(
        t.keys,
        [
            "ctrl-c",
            "backspace",
            "backspace",
            "esc",
            "esc",
            "up",
            "up",
            "enter",
            "enter"
        ]
    );
    assert_eq!(texts(&t), ["改过的"]);
}

/// 这一家不支持现在停（没申报打断键）⇒ 在出字时一个键不按，回没退成。
#[test]
fn busy_and_now_without_an_interrupt_key_is_not_rewound() {
    let mut t = FakeTerm::with(&["一"]);
    t.busy = true;
    let face = RewindFace {
        interrupt: None,
        ..FACE
    };
    let got = walk(
        &face,
        Plan {
            rewind: &edit("u1", RewindHow::Conversation, RewindWhen::Now),
            text: "x",
        },
        &mut t,
    )
    .unwrap();
    assert_eq!(got, Walked::No(Why::CannotStop));
    assert!(t.keys.is_empty());
}

/// 输入行里有人没提交的字、这一家没申报删字键 ⇒ 不退（一个键不按）。
#[test]
fn typed_input_without_an_erase_key_is_left_alone() {
    let mut t = FakeTerm::with(&["一"]);
    t.typed = "没发的字".to_string();
    let face = RewindFace {
        erase: None,
        ..FACE
    };
    let got = walk(
        &face,
        Plan {
            rewind: &edit("u1", RewindHow::Conversation, RewindWhen::AfterTurn),
            text: "x",
        },
        &mut t,
    )
    .unwrap();
    assert_eq!(got, Walked::No(Why::InputBusy));
    assert!(t.keys.is_empty());
    assert_eq!(t.typed, "没发的字");
}

/// 退完那一家把原句填回输入行 ⇒ 删掉再说改好的那句（不接在原句后面）。
#[test]
fn a_refilled_input_line_is_cleared_before_the_new_line() {
    let mut t = FakeTerm::with(&["一", "二"]);
    t.refill = true;
    let got = run(
        &mut t,
        &edit("u2", RewindHow::Conversation, RewindWhen::AfterTurn),
        "改",
    );
    assert_eq!(got, Walked::Done);
    assert_eq!(texts(&t), ["一", "改"]);
    assert!(t
        .keys
        .ends_with(&["enter".to_string(), "backspace".to_string()]));
}

/// 只有后台在跑、主 agent 空着（屏是输入行）⇒ 照常退。
#[test]
fn background_work_alone_does_not_block() {
    let mut t = FakeTerm::with(&["一", "二"]);
    let got = run(
        &mut t,
        &edit("u2", RewindHow::Conversation, RewindWhen::AfterTurn),
        "改",
    );
    assert_eq!(got, Walked::Done);
}

/// 记录主线上没有被编辑的那一句（已经被别处退掉了）⇒ 一个键不按。
#[test]
fn a_line_not_on_the_mainline_is_not_rewound() {
    let mut t = FakeTerm::with(&["一", "二"]);
    let got = run(
        &mut t,
        &edit("u9", RewindHow::Conversation, RewindWhen::AfterTurn),
        "改",
    );
    assert_eq!(got, Walked::No(Why::NoSuchLine));
    assert!(t.keys.is_empty());
}

/// 列表上选中的那一行和记录对不上（屏上那一句不是记录里那一句）⇒ 不进确认页，退回输入行。
#[test]
fn a_list_that_disagrees_with_the_record_backs_out() {
    let mut t = FakeTerm::with(&["一", "二", "三"]);
    let got = walk(
        &FACE,
        Plan {
            rewind: &edit("u2", RewindHow::Conversation, RewindWhen::AfterTurn),
            text: "改",
        },
        &mut Lying(&mut t),
    )
    .unwrap();
    assert_eq!(got, Walked::No(Why::ListUnlike));
    assert!(t.restored.is_empty());
    assert!(!t.keys.contains(&"enter".to_string()));
}

/// 读记录那一口说的和屏上不是同一份（记录里第三句另有其字）。
struct Lying<'a>(&'a mut FakeTerm);

impl RewindIo for Lying<'_> {
    fn screen(&mut self) -> Result<Vec<String>, CmdErr> {
        self.0.screen()
    }
    fn press(&mut self, key: &str) -> Result<Sent, CmdErr> {
        self.0.press(key)
    }
    fn redrawn(&mut self) -> bool {
        false
    }
    fn pause(&mut self, _ms: u64) {}
    fn say(&mut self, text: &str) -> Result<Sent, CmdErr> {
        self.0.say(text)
    }
    fn lines(&mut self) -> Result<Vec<UserLine>, CmdErr> {
        let mut v = self.0.lines()?;
        if let Some(l) = v.last_mut() {
            l.text = "别的话".to_string();
        }
        Ok(v)
    }
}

/// 截短的那一行与摘要：一方是另一方的开头才算同一句。
#[test]
fn same_line_takes_a_truncated_row_but_not_another_line() {
    assert!(same_line(
        "第二句是一句很长很长…",
        "第二句是一句很长很长的话"
    ));
    assert!(same_line("a  b", "a b"));
    assert!(!same_line("第三句", "第二句"));
    assert!(!same_line("", "x"));
}

/// 发送那一下的选项：照那一家申报的几项；主 agent 在干活才给「什么时候退」，缺省等这一轮完；没申报 ⇒ 空、带不支持那一句。
#[test]
fn options_follow_the_declared_choices_and_the_busy_state() {
    let idle = options_of(Some(&FACE), false, CODE);
    assert_eq!(
        idle.how
            .iter()
            .map(|c| (c.how, c.default))
            .collect::<Vec<_>>(),
        [
            (RewindHow::Conversation, true),
            (RewindHow::CodeAndConversation, false)
        ]
    );
    assert!(
        idle.when.is_empty(),
        "空着（只有后台在跑也算）不给什么时候退"
    );
    assert_eq!(idle.said, None);

    let busy = options_of(Some(&FACE), true, CODE);
    assert_eq!(
        busy.when
            .iter()
            .map(|w| (w.when, w.default))
            .collect::<Vec<_>>(),
        [(RewindWhen::AfterTurn, true), (RewindWhen::Now, false)]
    );
    let no_stop = RewindFace {
        interrupt: None,
        ..FACE
    };
    assert_eq!(
        options_of(Some(&no_stop), true, CODE).when.len(),
        1,
        "不支持现在停 ⇒ 只给等这一轮完"
    );

    let none = options_of(None, true, CODE);
    assert!(none.how.is_empty() && none.when.is_empty());
    assert_eq!(none.said, Some(copy_text("beRewind.no.unsupported", &[])));
}

/// 那一句之后改过代码、没有子 agent 在跑：两项都能选、都不带原因。
const CODE: After = After {
    code: true,
    runs: 0,
};

fn rows(o: &RewindOptions) -> Vec<(RewindHow, bool, bool, Option<String>)> {
    o.how
        .iter()
        .map(|c| (c.how, c.enabled, c.default, c.why.clone()))
        .collect()
}

/// 第一条：那一句之后没改过代码 ⇒ 「对话和代码一起退」灰着、原因是后端写好的那一句；只退对话照常能选、缺省。
#[test]
fn without_code_changes_after_the_line_code_and_conversation_is_off_with_its_reason() {
    let got = options_of(
        Some(&FACE),
        false,
        After {
            code: false,
            runs: 0,
        },
    );
    assert_eq!(
        rows(&got),
        [
            (RewindHow::Conversation, true, true, None),
            (
                RewindHow::CodeAndConversation,
                false,
                false,
                Some(copy_text("beRewind.off.noCodeChange", &[]))
            ),
        ]
    );
    assert_eq!(got.said, None);
    assert!(rows(&options_of(Some(&FACE), false, CODE))
        .iter()
        .all(|(_, on, _, why)| *on && why.is_none()));
}

/// 那一家只申报了只退对话 ⇒ 「对话和代码一起退」照样列出、灰着写不支持（不是悄悄不给）。
#[test]
fn a_way_the_family_does_not_declare_is_listed_off_as_unsupported() {
    let talk_only = RewindFace {
        choices: &FACE.choices[..1],
        ..FACE
    };
    let got = options_of(Some(&talk_only), false, CODE);
    assert_eq!(
        rows(&got),
        [
            (RewindHow::Conversation, true, true, None),
            (
                RewindHow::CodeAndConversation,
                false,
                false,
                Some(copy_text("beRewind.off.unsupported", &[]))
            ),
        ]
    );
}

/// 缺省那一项不能选 ⇒ 缺省落到第一项能选的（缺省不落在灰着的那一项上）。
#[test]
fn the_default_never_sits_on_an_option_that_is_off() {
    const CHOICES: &[crate::agents::RewindChoice] = &[
        crate::agents::RewindChoice {
            how: RewindHow::Conversation,
            label: "a",
            text: || "a".into(),
            default: false,
        },
        crate::agents::RewindChoice {
            how: RewindHow::CodeAndConversation,
            label: "b",
            text: || "b".into(),
            default: true,
        },
    ];
    let face = RewindFace {
        choices: CHOICES,
        ..FACE
    };
    let got = options_of(
        Some(&face),
        false,
        After {
            code: false,
            runs: 0,
        },
    );
    assert_eq!(
        got.how
            .iter()
            .map(|c| (c.how, c.default))
            .collect::<Vec<_>>(),
        [
            (RewindHow::Conversation, true),
            (RewindHow::CodeAndConversation, false)
        ]
    );
}

/// 这句之后派出的子 agent 还有在跑的 ⇒ 选项下面照实写几个（退不碰它们）。
#[test]
fn running_agents_launched_after_the_line_are_said_with_their_count() {
    let got = options_of(
        Some(&FACE),
        false,
        After {
            code: true,
            runs: 2,
        },
    );
    assert_eq!(
        got.said,
        Some(copy_text("beRewind.note.runsAfter", &[("n", "2")]))
    );
}

/// 运行表里数「这句之后派出、还在跑」的：起于那一刻之前的 · 收了场的 · 起的时刻不知道的都不数；那一句的时刻不知道 ⇒ 一个都不说。
#[test]
fn runs_after_counts_only_running_ones_started_after_the_line() {
    let run = |state, started_ms| cmd_wire::frame::RunInfo {
        state,
        started_ms,
        ..Default::default()
    };
    use cmd_wire::frame::RunState::{Done, Running};
    let runs = [
        run(Running, Some(500)),
        run(Running, Some(1_000)),
        run(Running, Some(2_000)),
        run(Done, Some(2_000)),
        run(Running, None),
    ];
    assert_eq!(runs_after(&runs, Some(1_000)), 2);
    assert_eq!(runs_after(&runs, None), 0);
}

/// 那一句起的主线记录里有写文件的调用才算改过代码（那一家的一拍里认；子运行的记录不算）。
#[test]
fn code_after_sees_a_write_call_in_the_family_beats() {
    let said = serde_json::json!({"type": "user", "uuid": "u1", "message": {"role": "user", "content": "改一下"}});
    let read = serde_json::json!({"type": "assistant", "uuid": "a1", "message": {"role": "assistant", "content": [
        {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/w/a.rs"}}
    ]}});
    let edit = serde_json::json!({"type": "assistant", "uuid": "a2", "message": {"role": "assistant", "content": [
        {"type": "tool_use", "id": "t2", "name": "Edit", "input": {"file_path": "/w/a.rs", "old_string": "x", "new_string": "y"}}
    ]}});
    assert!(!code_after("claude", [&said, &read], &[]));
    assert!(code_after("claude", [&said, &read, &edit], &[]));
    assert!(
        !code_after("claude", [&said, &read, &edit], &["a2".to_string()]),
        "回退掉的那一条（主线外）改的不算"
    );
}

/// 两家今天的申报：Claude 有（缺省只退对话），Codex 没有。
#[test]
fn claude_declares_a_rewind_menu_and_codex_does_not() {
    let claude = crate::agents::rewind_face(Some("claude")).expect("Claude 申报了");
    assert_eq!(claude.choices.iter().filter(|c| c.default).count(), 1);
    assert!(crate::agents::rewind_face(Some("codex")).is_none());
}

/// 结局写成给人看的那一句：成了 · 在等不写；没退成点名缺的是哪一项（投递那一下照它写进那一条的样子）。
#[test]
fn the_outcome_writes_its_own_reason() {
    assert_eq!(Walked::Done.said(), None);
    assert_eq!(Walked::Wait.said(), None);
    assert_eq!(
        Walked::No(Why::NoChoice(RewindHow::CodeAndConversation)).said(),
        Some(copy_text(
            "beRewind.no.noChoice",
            &[(
                "choice",
                &copy_text("beRewind.how.codeAndConversation", &[])
            )]
        ))
    );
    assert_eq!(
        Walked::Unsure(Why::NotReadBack).said(),
        Some(copy_text("beRewind.unsure.notReadBack", &[]))
    );
}
