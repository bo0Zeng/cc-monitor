use super::*;
use crate::agents::{SessionActivity, WaitOn};
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
    // `shell` ＝ 一轮停了、它在后台起的命令还在跑（那一家：空闲 ＋ 有没跑完的后台 shell 任务）—— 不是「claude 退了只剩 shell」。
    assert_eq!(
        activity_of(&json!({"status": "shell"})),
        Some(SessionActivity::BackgroundWork)
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

/// ★ 「在等什么」：那一家 pidfile 的 `waitingFor` 全部六个词逐个翻成与哪一家无关的那一维；没写 ⇒ 说不清；认不出 ⇒ 说不清并记漂移账。
/// 六个词是那一家自己的全集（它把前四个归「权限」类、`input needed` 归「提问」类、`dialog open` 不归类）。
#[test]
fn every_waiting_word_maps_to_what_is_awaited() {
    for (w, want) in [
        ("permission prompt", WaitOn::Permission),
        ("sandbox request", WaitOn::Network),
        ("worker request", WaitOn::Worker),
        ("goal proposal", WaitOn::Goal),
        ("input needed", WaitOn::Input),
        ("dialog open", WaitOn::Dialog),
    ] {
        assert_eq!(
            wait_of(&json!({"status": "waiting", "waitingFor": w})),
            Some(want),
            "{w}"
        );
    }
    assert_eq!(
        wait_of(&json!({"status": "waiting"})),
        None,
        "没写 ⇒ 说不清"
    );
    assert_eq!(
        wait_of(&json!({"status": "waiting", "waitingFor": 3})),
        None
    );
    assert_eq!(
        wait_of(&json!({"status": "waiting", "waitingFor": "c2-new-wait"})),
        None,
        "认不出 ⇒ 说不清，不猜"
    );
    let snap = crate::agents::claudecode::drift::snapshot();
    assert!(snap.iter().any(|f| f.face == DriftFace::UnknownWaitingFor
        && f.entries.iter().any(|e| e.key == "c2-new-wait")));
}

/// 「在等什么」那几个词只在适配层认：后端别处 ＋ monitor 的壳，生产段零处拼它们（零命中 ＋ 正控）。
#[test]
fn only_the_adapter_spells_the_waiting_words() {
    let backend = crate::guard_support::src_root();
    let shell = backend.join("../frontend/shell/src");
    let adapter = backend.join("agents/claudecode/pidfile.rs");
    let needles = [
        "\"permission prompt\"",
        "\"sandbox request\"",
        "\"worker request\"",
        "\"goal proposal\"",
        "\"input needed\"",
        "\"dialog open\"",
    ];
    let mut hits = Vec::new();
    let mut control = 0;
    for root in [&backend, &shell] {
        for (at, raw) in guard_core::scan_tree!(root, &["rs"]) {
            let prod = crate::guard_support::production_code(&raw);
            for n in needles {
                if !prod.contains(n) {
                    continue;
                }
                if at == adapter {
                    control += 1;
                } else {
                    hits.push(format!("{} :: {n}", at.display()));
                }
            }
        }
    }
    assert_eq!(control, needles.len(), "正控：适配层那份里该恰好认那六个词");
    assert!(
        hits.is_empty(),
        "适配层之外又认了「在等什么」的词：{hits:?}"
    );
}
