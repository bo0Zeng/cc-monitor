//! 关主窗那一下归壳一处（`main_close.rs`）：壳自己问本机后端（有期限）、自己弹框、自己销毁，网页 JS 卡住也关得掉。
//! 会断什么由后端写好（`rows`），壳只画。
use super::{closing_exits, dialog_text, request_of, settle};
use crate::chan::host::busy::FilewinTotals;
use cmd_wire::machine::{InterruptAct, InterruptRow, MachineInterruptsReply};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn reply(rows: Vec<InterruptRow>) -> MachineInterruptsReply {
    MachineInterruptsReply {
        relayed_sessions: Some(0),
        relayed_maybe: 0,
        live_streams: 0,
        forwards: 0,
        rows,
    }
}

fn two_rows() -> Vec<InterruptRow> {
    vec![
        InterruptRow {
            label: "cut".into(),
            items: vec!["a".into(), "b".into()],
        },
        InterruptRow {
            label: "keep".into(),
            items: vec!["c".into()],
        },
    ]
}

#[tokio::test(start_paused = true)]
async fn the_backend_hanging_still_ends_in_a_decision_within_the_budget() {
    // 本机后端那一问永远不回：到期就照「问不到」往下走（弹框只说那一句），不吊着那一下。
    let seen = Arc::new(Mutex::new(String::new()));
    let s = seen.clone();
    let go = settle(
        true,
        || std::future::pending::<Option<MachineInterruptsReply>>(),
        Duration::from_secs(10),
        move |body: String| async move {
            *s.lock().unwrap() = body;
            true
        },
    )
    .await;
    assert!(go);
    assert_eq!(
        *seen.lock().unwrap(),
        copy_core::copy_text("quitNow.dialog.unknown", &[])
    );
}

#[tokio::test]
async fn nothing_cut_closes_without_asking() {
    let asked = Arc::new(AtomicUsize::new(0));
    let a = asked.clone();
    let go = settle(
        true,
        || async { Some(reply(Vec::new())) },
        Duration::from_secs(10),
        move |_| {
            a.fetch_add(1, Ordering::Relaxed);
            async { false }
        },
    )
    .await;
    assert!(go);
    assert_eq!(asked.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn rows_from_the_backend_are_shown_as_written_and_cancel_keeps_the_window() {
    let seen = Arc::new(Mutex::new(String::new()));
    let s = seen.clone();
    let go = settle(
        true,
        || async { Some(reply(two_rows())) },
        Duration::from_secs(10),
        move |body: String| async move {
            *s.lock().unwrap() = body;
            false
        },
    )
    .await;
    assert!(!go);
    assert_eq!(*seen.lock().unwrap(), "cut\n· a\n· b\n\nkeep\n· c");
    assert_eq!(dialog_text(&two_rows()), "cut\n· a\n· b\n\nkeep\n· c");
}

#[tokio::test]
async fn another_window_holding_the_process_closes_without_asking() {
    let asked = Arc::new(AtomicUsize::new(0));
    let (a, b) = (asked.clone(), asked.clone());
    let go = settle(
        false,
        move || {
            a.fetch_add(1, Ordering::Relaxed);
            async { Some(reply(two_rows())) }
        },
        Duration::from_secs(10),
        move |_| {
            b.fetch_add(1, Ordering::Relaxed);
            async { false }
        },
    )
    .await;
    assert!(go);
    assert_eq!(asked.load(Ordering::Relaxed), 0);
}

#[test]
fn only_main_and_settings_open_means_the_process_exits() {
    assert!(closing_exits(&["main", "settings"]));
    assert!(closing_exits(&["main"]));
    assert!(!closing_exits(&["main", "viewer-abc"]));
}

#[test]
fn the_question_carries_every_machine_and_the_file_windows_counts() {
    let q = request_of(FilewinTotals {
        windows: 2,
        unsaved: 1,
        running: 3,
    });
    assert_eq!(
        serde_json::to_value(&q).unwrap(),
        serde_json::json!({
            "appExit": true,
            "fleet": true,
            "act": "stop",
            "files": {"unsaved": 1, "running": 3},
        })
    );
    assert_eq!(q.act, Some(InterruptAct::Stop), "关窗 ＝ 停到再启动");
}

/// 网页里不许再有人接管主窗的关：一挂 `onCloseRequested`，Tauri 就等网页 JS 定、JS 卡住窗就关不掉。
/// 允许挂的只有各自那扇窗自己的（设置窗 · agent 窗）。
#[test]
fn the_web_page_does_not_take_over_closing_the_main_window() {
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui");
    let own_windows = ["settings/panel.ts", "views/agent-window.ts"];
    let files = guard_core::scan_tree!(&ui, &["ts"]);
    let hits: Vec<String> = files
        .iter()
        .filter_map(|(p, text)| {
            let rel = p
                .strip_prefix(&ui)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            (!rel.starts_with("generated/")
                && !own_windows.contains(&rel.as_str())
                && text.contains("onCloseRequested"))
            .then_some(rel)
        })
        .collect();
    assert!(hits.is_empty(), "主窗的关又交给了网页 JS：{hits:?}");
    assert!(
        files
            .iter()
            .any(|(p, text)| p.ends_with("settings/panel.ts") && text.contains("onCloseRequested")),
        "自检：扫描认得出 onCloseRequested"
    );
}
