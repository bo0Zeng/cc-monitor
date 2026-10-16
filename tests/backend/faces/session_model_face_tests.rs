//! 会话开着时换模型：在等人不送；送了那一行没送到 ⇒ 照那一下的结局回；送到了 ⇒ 等记录里读回，读到才算换了，没读到 ⇒ 结果未知。

use super::*;
use crate::agents::ModelSaid;
use crate::control::terminals_wire::{Sent, SentResult};

fn sent(result: SentResult) -> Sent {
    Sent {
        result,
        why: (result == SentResult::Refused)
            .then_some(crate::control::terminals::Refused::NotYours),
        said: (result == SentResult::Refused).then(|| "x".to_string()),
        screen: None,
        resumed: false,
    }
}

/// ★ 在等人 ⇒ 不送，回被拒 ＋ `waiting` ＋ 那一句。
#[test]
fn a_waiting_session_gets_nothing_sent() {
    let got = waiting_reply();
    assert_eq!(got.result, SwitchResult::Refused);
    assert_eq!(got.why, Some(crate::control::terminals::Refused::Waiting));
    assert!(got.said.is_some() && got.model.is_none());
}

/// 不该走到记下那一支。
fn no_keep() -> Result<String, crate::wire::spec::Fail> {
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
    let sonnet = claude.model_choice("sonnet").unwrap();
    for why in [Refused::Ended, Refused::NotKnown] {
        let keep = || {
            crate::faces::session_open_face::keep_for_resume(&own, sid, &rec, Some("sonnet"), None)
                .map(|()| (sonnet.text)())
        };
        let gone = Sent {
            why: Some(why),
            ..sent(SentResult::Refused)
        };
        let r = after_send(gone, &keep).unwrap().unwrap();
        assert_eq!(r.result, SwitchResult::Done);
        assert_eq!(r.model, Some((sonnet.text)()));
        assert_eq!((r.why, r.said), (None, None));
    }
    let got = crate::history::history_annotations::resume_with_at(
        &d.join(creds_core::store::HISTORY_METADATA_FILE),
        sid,
    )
    .unwrap();
    assert_eq!(got.model.as_deref(), Some("sonnet"));
    assert_eq!(got.end, std::fs::metadata(&rec).unwrap().len());
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 读回：换成了 ⇒ done ＋ 显示名 ＋ 思考档 ＋ 那一句（照实说也存成了这个号新会话的默认）；没变 ⇒ done ＋ 「未变」那一句；
/// 没读到 ⇒ 结果未知，不猜。
#[test]
fn the_readback_decides_and_silence_is_unsure() {
    let set = settled(Some(ModelSaid {
        name: "Opus 5.5 (1M context)".into(),
        kept: false,
        effort: Some("xhigh".into()),
    }));
    assert_eq!(set.result, SwitchResult::Done);
    assert_eq!(set.model.as_deref(), Some("Opus 5.5 (1M context)"));
    assert_eq!(set.effort.as_deref(), Some("xhigh"));
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
    let kept = settled(Some(ModelSaid {
        name: "Fable 5.1".into(),
        kept: true,
        effort: None,
    }));
    assert_eq!(kept.result, SwitchResult::Done);
    assert_eq!(
        kept.said.as_deref(),
        Some(copy_core::copy_text("beSessionModel.set.kept", &[("model", "Fable 5.1")]).as_str())
    );
    let none = settled(None);
    assert_eq!(none.result, SwitchResult::Unsure);
    assert!(none.model.is_none());
    assert_eq!(
        none.said.as_deref(),
        Some(copy_core::copy_text("beSessionModel.unsure.noReadback", &[]).as_str())
    );
}

/// ★ 线上那一形：没有的格不出（`why` · `said` · `effort`），`model` 恒在（读不到 ⇒ `null`）。
#[test]
fn the_reply_wire_names_its_cells() {
    let v = serde_json::to_value(settled(None)).unwrap();
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["model", "result", "said"]);
    assert_eq!(v["result"], "unsure");
}

/// ★ 会话开着时换模型收的值：那一家可选的模型 ＋ 「默认」那一项的值（回到它自己的缺省模型）；别的不收。
#[test]
fn the_default_item_is_a_model_the_session_takes() {
    let (_, claude) = crate::agents::pick_kind(Some("claude")).unwrap();
    assert!(claude.model_choice("default").is_some());
    assert!(claude.model_choice("opus").is_some());
    assert!(claude.model_choice("nope").is_none());
    let (_, codex) = crate::agents::pick_kind(Some("codex")).unwrap();
    assert!(codex.model_choice("default").is_none());
}
