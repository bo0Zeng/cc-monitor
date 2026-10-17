//! 帧门：对得上的照过 · 那台形状不同的丢 · 本侧不认的丢 · hello 照过（宿主照实说）· 读不出 kind 的丢。
use super::*;
use cmd_wire::frame_shapes::FRAMES;

#[test]
fn a_line_passes_only_when_its_kind_is_ours_and_shaped_alike() {
    let mut frames: std::collections::BTreeMap<String, String> = FRAMES
        .iter()
        .map(|(k, h)| (k.to_string(), h.to_string()))
        .collect();
    frames.insert("session_status".into(), "00000000".into());
    let offer = Offer::of(&cmd_wire::frame::Hello {
        frames,
        ..Default::default()
    });
    let pass = |k: &str| Admit::Pass(k.to_string());
    assert_eq!(
        admit(&offer, r#"{"kind":"line","session_id":"s"}"#),
        pass("line")
    );
    assert_eq!(
        admit(&offer, r#"{"kind":"session_status","sid":"s"}"#),
        Admit::Differs("session_status".into())
    );
    assert_eq!(
        admit(&offer, r#"{"kind":"made_up","x":1}"#),
        Admit::Unknown("made_up".into())
    );
    assert_eq!(admit(&offer, r#"{"kind":"hello"}"#), pass("hello"));
    assert_eq!(admit(&offer, "not json"), Admit::Unknown(String::new()));
    assert_eq!(
        admit(&offer, r#"{"sid":"s","kind":"session_added"}"#),
        pass("session_added"),
        "kind 不在行首也认得出"
    );
}
