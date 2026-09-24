//! ST1「关窗改隐藏」的生命周期缝（`lib.rs::windows_to_destroy_after`）。
//!
//! 设置窗关窗 = 隐藏、永不自己销毁 ⇒ 主窗销毁时它必须跟着走，否则进程被一个看不见的窗口吊住。
//! 这里钉决策本身（纯函数，Linux 上就跑得动）；`on_window_event` 只负责照它执行。
use super::{windows_to_destroy_after, MAIN_WINDOW_LABEL, SETTINGS_WINDOW_LABEL};

#[test]
fn main_destroyed_takes_the_hidden_settings_window_with_it() {
    let alive = [SETTINGS_WINDOW_LABEL, "viewer-abc"];
    assert_eq!(
        windows_to_destroy_after(MAIN_WINDOW_LABEL, &alive),
        vec![SETTINGS_WINDOW_LABEL],
        "主窗没了而设置窗还藏着 —— 进程会被它吊住"
    );
}

#[test]
fn viewers_are_left_alone_they_are_visible_and_closable() {
    // 非空对照：同一个输入里 viewer 在，但不在输出里（它看得见、用户自己关得掉；
    // 「主窗关了 viewer 还开着」时进程该不该活，是 `01 §3.3b` 的事，这里不改）。
    let alive = [SETTINGS_WINDOW_LABEL, "viewer-abc"];
    assert!(!windows_to_destroy_after(MAIN_WINDOW_LABEL, &alive).contains(&"viewer-abc"));
}

#[test]
fn nothing_follows_when_settings_is_not_open() {
    assert!(windows_to_destroy_after(MAIN_WINDOW_LABEL, &["viewer-abc"]).is_empty());
}

#[test]
fn only_the_main_window_drags_others_down() {
    let alive = [MAIN_WINDOW_LABEL, SETTINGS_WINDOW_LABEL];
    for destroyed in [SETTINGS_WINDOW_LABEL, "viewer-abc"] {
        assert!(
            windows_to_destroy_after(destroyed, &alive).is_empty(),
            "{destroyed} 销毁不该牵连别的窗口"
        );
    }
}

#[test]
fn the_main_label_is_the_one_tauri_conf_declares() {
    // 主窗标签住 `tauri.conf.json`（设置窗那一侧由 `open_settings_window` 直接取
    // `SETTINGS_WINDOW_LABEL` 建窗 —— 同一个常量，漂不开）。主窗标签漂了，上面几条全绿、
    // 生产里一个都对不上。
    let conf = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json"),
    )
    .expect("读不到 tauri.conf.json");
    let v: serde_json::Value = serde_json::from_str(&conf).expect("tauri.conf.json 不是合法 JSON");
    // 不写 `label` 的那一项取 Tauri 的缺省标签 `main`（`WindowConfig::label` 的 serde 缺省值）。
    let labels: Vec<&str> = v["app"]["windows"]
        .as_array()
        .expect("找不到 app.windows")
        .iter()
        .map(|w| w["label"].as_str().unwrap_or("main"))
        .collect();
    assert!(
        labels.contains(&MAIN_WINDOW_LABEL),
        "tauri.conf.json 的窗口标签里没有 {MAIN_WINDOW_LABEL}：{labels:?}"
    );
}
