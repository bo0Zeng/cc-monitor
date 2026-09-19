use super::{collect_reannounce, AnnouncedMeta};
use std::collections::HashMap;

fn meta(origin: &str, sid: &str, status: Option<&str>) -> AnnouncedMeta {
    AnnouncedMeta {
        payload: crate::bridge::RemoteSessionAddedPayload {
            session_id: sid.into(),
            origin: origin.into(),
            kind: None,
            attachable: None,
            cwd: Some("/p".into()),
            name: None,
        },
        status: status.map(str::to_string),
        waiting_for: None,
    }
}

/// F28 DoD：重发快照收集——拍平 + (origin, sid) 稳定排序 + status 保真。
#[test]
fn collect_flattens_sorts_and_preserves_status() {
    let mut reg: HashMap<String, HashMap<String, AnnouncedMeta>> = HashMap::new();
    reg.entry("pi".into())
        .or_default()
        .insert("sid-b".into(), meta("pi", "sid-b", Some("busy")));
    reg.entry("pi".into())
        .or_default()
        .insert("sid-a".into(), meta("pi", "sid-a", None));
    reg.entry("aya".into())
        .or_default()
        .insert("sid-z".into(), meta("aya", "sid-z", Some("waiting")));
    let out = collect_reannounce(&reg);
    let keys: Vec<(String, String)> = out
        .iter()
        .map(|m| (m.payload.origin.clone(), m.payload.session_id.clone()))
        .collect();
    assert_eq!(
        keys,
        vec![
            ("aya".into(), "sid-z".into()),
            ("pi".into(), "sid-a".into()),
            ("pi".into(), "sid-b".into()),
        ],
        "稳定 (origin, sid) 排序"
    );
    assert_eq!(out[0].status.as_deref(), Some("waiting"));
    assert_eq!(out[2].status.as_deref(), Some("busy"));
    assert!(collect_reannounce(&HashMap::new()).is_empty());
}
