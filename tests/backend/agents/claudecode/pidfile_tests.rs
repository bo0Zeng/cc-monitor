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
            .any(|f| f.face == face.word() && f.entries.iter().any(|e| e.key == key))
    };
    assert!(has(DriftFace::UnknownSessionKind, "af2-kind-probe"));
    assert!(has(DriftFace::UnknownSessionStatus, "af2-status-probe"));
    assert!(!has(DriftFace::UnknownSessionKind, "bg"), "认得的不记");
}

/// 「是不是后台会话」「此刻在干什么」只在适配层判一次：后端别处 ＋ monitor 的壳，生产段零处拼 pidfile 的
/// `"interactive"` 字面量（零命中 ＋ 正控：同一个扫描器在本适配层那份里找得到它）。
#[test]
fn only_the_adapter_spells_the_pidfile_kind_word() {
    let backend = guard_core::addr::backend_src_root();
    let shell = backend.join("../frontend/shell/src");
    let needle = "\"interactive\"";
    let mut hits = Vec::new();
    let mut control = false;
    let adapter = backend.join("agents/claudecode/pidfile.rs");
    for root in [&backend, &shell] {
        for (at, raw) in guard_core::scan_tree!(root, &["rs"]) {
            if !guard_core::production_code(&raw).contains(needle) {
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

/// ★ 一份进程状态文件只在这里翻成一行：通用层判活 · 宣告 · 账号归属 · 「在等什么」要的每一格都在这一行里（造出来的最小一份）。
/// 串格不是串 ⇒ 没有；启动时刻收十进制串也收裸数字；`attachable` 只认真布尔。
#[test]
fn one_pidfile_becomes_one_fact_row() {
    let f = proc_of(&json!({
        "pid": 4242, "sessionId": "s-1", "cwd": "/w/p", "name": "job",
        "procStart": "12285972", "kind": "bg", "status": "waiting", "waitingFor": "input needed",
        "attachable": false, "statusUpdatedAt": 1_700_000_000_123u64, "startedAt": 1_700_000_000_000u64
    }));
    assert_eq!(
        f,
        ProcFact {
            kind: crate::agents::claudecode::AGENT_KIND,
            sid: Some("s-1".into()),
            cwd: Some("/w/p".into()),
            name: Some("job".into()),
            proc_start: Some(12_285_972),
            background: true,
            activity: Some(SessionActivity::NeedsYou),
            status_word: Some("waiting".into()),
            wait: Some(WaitOn::Input),
            waiting_word: Some("input needed".into()),
            attachable: Some(false),
            status_since_ms: Some(1_700_000_000_123),
            started_ms: Some(1_700_000_000_000),
        }
    );
    assert_eq!(
        proc_of(&json!({"procStart": 12285972})).proc_start,
        Some(12_285_972)
    );
    let odd = proc_of(&json!({"sessionId": 5, "attachable": "false", "procStart": "x"}));
    assert_eq!(
        (odd.sid, odd.attachable, odd.proc_start),
        (None, None, None)
    );
    assert!(!odd.background, "不写 kind ⇒ 交互");
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
    assert!(snap
        .iter()
        .any(|f| f.face == DriftFace::UnknownWaitingFor.word()
            && f.entries.iter().any(|e| e.key == "c2-new-wait")));
}

/// 「在等什么」那几个词只在适配层认：后端别处 ＋ monitor 的壳，生产段零处拼它们（零命中 ＋ 正控）。
#[test]
fn only_the_adapter_spells_the_waiting_words() {
    let backend = guard_core::addr::backend_src_root();
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
            let prod = guard_core::production_code(&raw);
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
