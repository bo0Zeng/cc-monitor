//! **聊天输入框上方那一排附件的字**：交此刻每张卡走到哪 ＋ 断着时那台离线那一句 ＋ 本机这边的错，写好卡上第二行 · ✕ 的读屏名 ·
//! 卡下那一条（那一句 ＋ 按钮 ＋ 是不是虚线），出口照摆。收不收 · 上限 · 那台那一句仍是那台的。
//! 入 [`AttachWant`]（`chunk_core::attach::Tray::want` 照此刻写）、出 [`AttachFace`]；桌面本机后端与手机核心调同一份。
//! 那一条的先后：本机这边的错 › 传 · 落没成的（几份 ⇒ 数 ＋ 第一份的因）› 断着。开传前那台就拒的因写在卡上，不进那一条。

use cmd_wire::attachment::{
    AttachAct, AttachCardPhase, AttachFace, AttachFaceCard, AttachFaceNote, AttachLocal,
    AttachWant, AttachWantCard,
};
use copy_core::copy_text;

/// 一张卡上第二行：排队 · 失败 · 那台不收的那一句；传输中（只转环，不写进度数字）· 落好了 ⇒ 不写。
fn line(phase: &AttachCardPhase) -> Option<String> {
    match phase {
        AttachCardPhase::Queued => Some(copy_text("mobAttach.card.queued", &[])),
        AttachCardPhase::Failed { .. } | AttachCardPhase::Lost => {
            Some(copy_text("runs.state.failed", &[]))
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

#[cfg(test)]
#[path = "../../../../tests/common/face-core/attach_tests.rs"]
mod tests;
