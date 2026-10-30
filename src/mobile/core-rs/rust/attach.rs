//! 手机聊天输入框上方那一排附件的字：Kotlin 交此刻每张卡走到哪 ＋ 断着时那台离线那一句 ＋ 手机这边的错，
//! 这里写好卡上第二行 · ✕ 的读屏名 · 卡下那一条（那一句 ＋ 按钮 ＋ 是不是虚线），Kotlin 照摆。收不收 · 上限 · 那台那一句仍是那台的。
//!
//! 入 [`AttachWant`]、出 [`AttachFace`]（两端同一份类型，`cmd_wire::attach`）；读不动 ⇒ 空串。
//! 那一条的先后：手机这边的错 › 传 · 落没成的（几份 ⇒ 数 ＋ 第一份的因）› 断着。开传前那台就拒的因写在卡上，不进那一条。

use cmd_wire::attach::{
    AttachAct, AttachCardPhase, AttachFace, AttachFaceCard, AttachFaceNote, AttachLocal,
    AttachWant, AttachWantCard,
};
use copy_core::copy_text;

/// 一张卡上第二行：排队 · 失败 · 那台不收的那一句；传输中（只转环，不写进度数字）· 落好了 ⇒ 不写。
fn line(phase: &AttachCardPhase) -> Option<String> {
    match phase {
        AttachCardPhase::Queued => Some(copy_text("mobAttach.card.queued", &[])),
        AttachCardPhase::Failed { .. } | AttachCardPhase::Lost => {
            Some(copy_text("mobAttach.card.failed", &[]))
        }
        AttachCardPhase::Refused { said } => Some(said.clone()),
        AttachCardPhase::Moving | AttachCardPhase::Landed => None,
    }
}

fn bar(text: String, act: Option<AttachAct>, dashed: bool) -> AttachFaceNote {
    AttachFaceNote { text, act, dashed }
}

/// 卡下那一条；什么都没有 ⇒ `None`。
fn note(want: &AttachWant) -> Option<AttachFaceNote> {
    match want.local {
        Some(AttachLocal::NoRoom) => {
            return Some(bar(
                copy_text("mobAttach.note.local", &[]),
                Some(AttachAct::Repick),
                false,
            ))
        }
        Some(AttachLocal::NoCamera) => {
            return Some(bar(copy_text("mobAttach.note.noCamera", &[]), None, false))
        }
        None => {}
    }
    let failed: Vec<String> = want
        .cards
        .iter()
        .filter_map(|c| match &c.phase {
            AttachCardPhase::Failed { said } => Some(said.clone()),
            // 本机那一份副本开不了：照分块那一半读不动本机文件的那一句。
            AttachCardPhase::Lost => {
                Some(copy_text("mobFiles.local.readFailed", &[("name", &c.name)]))
            }
            _ => None,
        })
        .collect();
    if let Some(said) = failed.first().map(String::as_str) {
        let text = if failed.len() == 1 {
            copy_text("mobAttach.note.failed", &[("said", said)])
        } else {
            copy_text(
                "mobAttach.note.failedMany",
                &[("n", &failed.len().to_string()), ("said", said)],
            )
        };
        return Some(bar(text, Some(AttachAct::Retry), false));
    }
    let offline = want.offline.as_deref()?;
    Some(bar(
        copy_text("mobAttach.note.offline", &[("said", offline)]),
        None,
        true,
    ))
}

fn card(c: &AttachWantCard) -> AttachFaceCard {
    AttachFaceCard {
        line: line(&c.phase),
        remove: copy_text("mobAttach.card.remove", &[("name", &c.name)]),
    }
}

/// 那一排此刻的字（见模块头）。
pub fn face(want: &AttachWant) -> AttachFace {
    AttachFace {
        cards: want.cards.iter().map(card).collect(),
        note: note(want),
    }
}

/// [`face`] 的 JSON 进出；读不动 ⇒ 空串。
pub fn face_json(want: &str) -> String {
    serde_json::from_str::<AttachWant>(want)
        .ok()
        .and_then(|w| serde_json::to_string(&face(&w)).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(phase: AttachCardPhase) -> AttachWantCard {
        AttachWantCard {
            name: "a.png".into(),
            phase,
        }
    }

    fn want(cards: Vec<AttachWantCard>) -> AttachWant {
        AttachWant {
            cards,
            offline: None,
            local: None,
        }
    }

    fn failed(said: &str) -> AttachCardPhase {
        AttachCardPhase::Failed { said: said.into() }
    }

    fn refused(said: &str) -> AttachCardPhase {
        AttachCardPhase::Refused { said: said.into() }
    }

    #[test]
    fn each_card_says_where_it_is_and_moving_or_landed_ones_say_nothing() {
        let v = face(&want(vec![
            card(AttachCardPhase::Queued),
            card(AttachCardPhase::Moving),
            card(AttachCardPhase::Landed),
            card(failed("磁盘满")),
            card(refused("超上限")),
        ]));
        let lines: Vec<Option<String>> = v.cards.iter().map(|c| c.line.clone()).collect();
        assert_eq!(
            lines,
            vec![
                Some(copy_text("mobAttach.card.queued", &[])),
                // 传输中只转环，不写进度数字。
                None,
                None,
                Some(copy_text("mobAttach.card.failed", &[])),
                Some("超上限".to_string()),
            ]
        );
        assert_eq!(
            v.cards[0].remove,
            copy_text("mobAttach.card.remove", &[("name", "a.png")])
        );
    }

    #[test]
    fn the_bar_counts_alike_failures_and_says_the_first_reason() {
        let one = face(&want(vec![card(failed("磁盘满")), card(refused("超上限"))]));
        assert_eq!(
            one.note,
            Some(bar(
                copy_text("mobAttach.note.failed", &[("said", "磁盘满")]),
                Some(AttachAct::Retry),
                false
            ))
        );
        let two = face(&want(vec![card(failed("磁盘满")), card(failed("别的"))]));
        assert_eq!(
            two.note.map(|n| n.text),
            Some(copy_text(
                "mobAttach.note.failedMany",
                &[("n", "2"), ("said", "磁盘满")]
            ))
        );
    }

    #[test]
    fn a_lost_local_copy_reads_like_the_chunk_loop_s_own_read_failure() {
        let v = face(&want(vec![card(AttachCardPhase::Lost)]))
            .note
            .expect("有那一条");
        assert_eq!(
            v.text,
            copy_text(
                "mobAttach.note.failed",
                &[(
                    "said",
                    &copy_text("mobFiles.local.readFailed", &[("name", "a.png")])
                )]
            )
        );
        assert_eq!(v.act, Some(AttachAct::Retry));
    }

    #[test]
    fn local_beats_failures_and_failures_beat_offline() {
        let all = AttachWant {
            cards: vec![card(failed("磁盘满")), card(AttachCardPhase::Queued)],
            offline: Some("devbox 离线".into()),
            local: Some(AttachLocal::NoRoom),
        };
        assert_eq!(
            face(&all).note,
            Some(bar(
                copy_text("mobAttach.note.local", &[]),
                Some(AttachAct::Repick),
                false
            ))
        );
        let no_local = AttachWant { local: None, ..all };
        assert_eq!(
            face(&no_local).note.and_then(|n| n.act),
            Some(AttachAct::Retry)
        );
        let off = AttachWant {
            offline: Some("devbox 离线".into()),
            ..want(vec![card(AttachCardPhase::Queued)])
        };
        assert_eq!(
            face(&off).note,
            Some(bar(
                copy_text("mobAttach.note.offline", &[("said", "devbox 离线")]),
                None,
                true
            ))
        );
        assert_eq!(face(&want(vec![card(refused("超上限"))])).note, None);
        let camera = AttachWant {
            local: Some(AttachLocal::NoCamera),
            ..want(vec![])
        };
        assert_eq!(face(&camera).note.and_then(|n| n.act), None);
    }

    #[test]
    fn the_json_ends_are_the_contract_shapes() {
        let got = face_json(
            r#"{"cards":[{"name":"a.png","phase":{"kind":"refused","said":"超上限"}}],"offline":null,"local":"no_camera"}"#,
        );
        let v: serde_json::Value = serde_json::from_str(&got).expect("一行 JSON");
        assert_eq!(v["cards"][0]["line"], "超上限");
        assert_eq!(v["note"]["act"], serde_json::Value::Null);
        assert_eq!(v["note"]["dashed"], false);
    }

    #[test]
    fn unreadable_input_is_an_empty_line() {
        assert_eq!(face_json("nope"), "");
        assert_eq!(face_json("{}"), "");
        // 认不得的那一步不当成「没第二行」照常出卡：两端契约对不上。
        assert_eq!(
            face_json(
                r#"{"cards":[{"name":"a.png","phase":{"kind":"nope"}}],"offline":null,"local":null}"#
            ),
            ""
        );
        assert_eq!(
            face_json(r#"{"cards":[],"offline":null,"local":"no_disk"}"#),
            ""
        );
    }
}
