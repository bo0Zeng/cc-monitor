use super::*;
use crate::agents::SessionActivity;
use serde_json::json;

#[test]
fn background_is_any_kind_but_interactive() {
    assert!(!background_of(&json!({})), "不写 kind ⇒ 交互");
    assert!(!background_of(&json!({"kind": "interactive"})));
    assert!(background_of(&json!({"kind": "bg"})));
    assert!(
        background_of(&json!({"kind": "af2-new-kind"})),
        "认不出的 kind 当后台（kind 是授权型判据：只有 interactive 成 tab）"
    );
}

#[test]
fn activity_maps_the_status_words() {
    assert_eq!(
        activity_of(&json!({"status": "busy"})),
        Some(SessionActivity::Working)
    );
    assert_eq!(
        activity_of(&json!({"status": "waiting"})),
        Some(SessionActivity::NeedsYou)
    );
    assert_eq!(
        activity_of(&json!({"status": "idle"})),
        Some(SessionActivity::Idle)
    );
    assert_eq!(
        activity_of(&json!({"status": "shell"})),
        Some(SessionActivity::Idle)
    );
    assert_eq!(activity_of(&json!({})), None, "没写 ⇒ 说不清");
    assert_eq!(activity_of(&json!({"status": 3})), None);
    assert_eq!(
        activity_of(&json!({"status": "af2-new-status"})),
        None,
        "认不出 ⇒ 说不清，不猜"
    );
}

/// 认不出的词记进漂移账（全局账本 ⇒ 只断言「我那条在」）。
#[test]
fn unknown_words_are_booked_not_silent() {
    background_of(&json!({"kind": "af2-kind-probe"}));
    activity_of(&json!({"status": "af2-status-probe"}));
    let snap = crate::agents::claudecode::drift::snapshot();
    let has = |face: DriftFace, key: &str| {
        snap.iter()
            .any(|f| f.face == face && f.entries.iter().any(|e| e.key == key))
    };
    assert!(has(DriftFace::UnknownSessionKind, "af2-kind-probe"));
    assert!(has(DriftFace::UnknownSessionStatus, "af2-status-probe"));
    assert!(!has(DriftFace::UnknownSessionKind, "bg"), "认得的不记");
}

/// 「是不是后台会话」「此刻在干什么」只在适配层判一次：后端别处 ＋ monitor 的壳，生产段零处拼 pidfile 的
/// `"interactive"` 字面量（零命中 ＋ 正控：同一个扫描器在本适配层那份里找得到它）。
#[test]
fn only_the_adapter_spells_the_pidfile_kind_word() {
    let backend = crate::guard_support::src_root();
    let shell = backend.join("../frontend/shell/src");
    let needle = "\"interactive\"";
    let mut hits = Vec::new();
    let mut control = false;
    let adapter = backend.join("agents/claudecode/pidfile.rs");
    for root in [&backend, &shell] {
        for (at, raw) in guard_core::scan_tree!(root, &["rs"]) {
            if !crate::guard_support::production_code(&raw).contains(needle) {
                continue;
            }
            if at == adapter {
                control = true;
            } else {
                hits.push(at.display().to_string());
            }
        }
    }
    assert!(control, "正控：适配层那份里没扫到 —— 扫描器坏了或住址搬了");
    assert!(
        hits.is_empty(),
        "适配层之外又拼了 pidfile 的 kind 词：{hits:?}"
    );
}
