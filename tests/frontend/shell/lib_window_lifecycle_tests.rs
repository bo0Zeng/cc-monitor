//! # 要求住址：`设计/01 §1.3`（窗口生命周期：设置窗关了是隐藏，主窗走时带它一起走）
//!
//! 核原文：`设计/01 §1.3` 设置窗那一行逐字「隐藏的窗口会吊住进程 ⇒ 主窗销毁时连带销毁它（`lib.rs::windows_to_destroy_after`）」
//! —— 前四条判的正是这个决策（带走设置窗 · viewer 不动 · 别的窗销毁不牵连），第五条判主窗标签与 `tauri.conf.json` 对得上。〔JA1 点址 2026-09-24〕
//!
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

// ═══════════════════════════════════════════════════════════════════════════════════════
// 〔S5 · 第四波 · `INVARIANTS §22` 第 1、2 条〕开窗 IPC 的两格 —— 从前只写在散文里、没有一条会红的判据
// （`tests/frontend/ui/invariants-frontend-guard.vitest.ts` 头注从前登记着「22.1/22.2 没人守」，本拍改指这里）。
//
// 要求住址：`src/doc/INVARIANTS.md §22`，逐字：「**开窗 IPC 必须 `async`**」·「**禁止**用 `&str` 目标
// （→`EventTarget::AnyLabel`）配模块级 `listen`（→`Any`）—— Tauri 2 按 kind 匹配，`Any` 监听命不中 `AnyLabel` 发射，事件静默丢弃」。
//
// 两条都是「静默失败」：违反了不报错，只是白屏 / 卡死 / 事件丢了。所以只能在源码形状上钉：
// - **22.1**：建 `WebviewWindow` 的命令必须是 `async fn`（同步命令跑在主线程，`build()` 又要派发到主线程并等 ⇒ 死锁）；
// - **22.2**：给单个窗口定向投递用 `emit_to(EventTarget::webview_window(label), …)`，**不许**退回 `&str` 目标
//   （那是 `EventTarget::AnyLabel`，命不中前端 `getCurrentWebviewWindow().listen` 的窗口作用域监听，事件静默丢）。
//
// 人群从文件系统全集来（`src/frontend/shell/src/**.rs` 的生产段），调用点集合与登记两向相等 ——
// 新开一个建窗口 / 新写一处定向投递就红，逼着登记，同时逼着回答那一格。
// ═══════════════════════════════════════════════════════════════════════════════════════

/// 一处调用点：(文件名, 所在函数名, 所在函数那一行, 调用点之前那一段函数体)。
struct Site {
    file: String,
    func: String,
    fn_line: String,
    attr_above: String,
    body_before: String,
    line: String,
}

/// 把一行 `fn` 声明里的函数名抠出来（认 `pub` / `pub(crate)` / `async` 前缀）。不是声明 ⇒ `None`。
fn fn_name_of(line: &str) -> Option<String> {
    let t = guard_core::strip_visibility(line.trim_start());
    let t = t.strip_prefix("async ").unwrap_or(t);
    let rest = t.strip_prefix("fn ")?;
    let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
    Some(rest[..end].to_string())
}

/// 生产段里 `needle` 的每一处，连同它所在的函数。
fn sites_of(needle: &str) -> Vec<Site> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    for (path, raw) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let prod = guard_core::production_code(&raw);
        let lines: Vec<&str> = prod.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            if !l.contains(needle) {
                continue;
            }
            let (fi, func) = (0..i)
                .rev()
                .find_map(|k| fn_name_of(lines[k]).map(|n| (k, n)))
                .unwrap_or_else(|| panic!("{path:?} 里 `{needle}` 那一行往上找不到所在的函数"));
            let attr_above = (0..fi)
                .rev()
                .map(|k| lines[k].trim())
                .find(|t| !t.is_empty())
                .unwrap_or("")
                .to_string();
            out.push(Site {
                file: path.file_name().unwrap().to_string_lossy().to_string(),
                func,
                fn_line: lines[fi].trim().to_string(),
                attr_above,
                body_before: lines[fi..i].join("\n"),
                line: l.trim().to_string(),
            });
        }
    }
    out
}

/// 22.1：建窗口的地方恰好是登记的那两条命令，而且两条都是 `async fn`、都是 `#[tauri::command]`。
#[test]
fn every_window_building_command_is_async() {
    let sites = sites_of("WebviewWindowBuilder::new(");
    let mut got: Vec<String> = sites
        .iter()
        .map(|s| format!("{}::{}", s.file, s.func))
        .collect();
    got.sort();
    got.dedup();
    assert_eq!(
        got,
        vec![
            "lib.rs::open_session_in_new_window".to_string(),
            "lib.rs::open_settings_window".to_string(),
        ],
        "建 `WebviewWindow` 的地方与登记对不上（两向）。新开一个窗口 ⇒ 先回答 `INVARIANTS §22` 第 1 条，再登记进来"
    );
    for s in &sites {
        assert!(
            guard_core::strip_visibility(&s.fn_line).starts_with("async fn "),
            "`{}::{}` 建窗口却不是 `async fn`（`{}`）—— 同步命令跑在主线程，`build()` 要派发到主线程并等 ⇒ 死锁：\
             新窗口白屏、整个 app 卡死（`INVARIANTS §22` 第 1 条）",
            s.file,
            s.func,
            s.fn_line
        );
        assert_eq!(
            s.attr_above, "#[tauri::command]",
            "`{}::{}` 上面紧挨着的不是 `#[tauri::command]` —— 建窗口的得是那条 IPC 命令自己（中间隔一层同步 helper 就绕过了本条）",
            s.file, s.func
        );
    }
}

/// 22.2：定向投递的目标是 `EventTarget::webview_window(…)` —— 内联，或同一函数里由它绑定的局部名。
#[test]
fn every_emit_to_targets_a_webview_window_not_a_bare_label() {
    const CTOR: &str = "EventTarget::webview_window(";
    let sites = sites_of(".emit_to(");
    let mut got: Vec<String> = sites
        .iter()
        .map(|s| format!("{}::{}", s.file, s.func))
        .collect();
    got.sort();
    got.dedup();
    assert_eq!(
        got,
        // 〔CF2 · 第四波 4B〕定向投递换了住址：独立窗口的定向重放（原 `event_replay` 那一处）退役，
        //   今天唯一的一处是会话流的交格 —— 通道 webview 宿主的出口（主窗口与独立窗口都是定向）。
        vec!["webview.rs::deliver".to_string()],
        "`emit_to` 的调用点与登记对不上（两向）。新写一处定向投递 ⇒ 先回答 `INVARIANTS §22` 第 2 条，再登记进来"
    );
    for s in &sites {
        let arg = s.line.split_once(".emit_to(").unwrap().1;
        let arg = arg.split(',').next().unwrap().trim();
        let arg = arg.trim_start_matches('&');
        let arg = arg.strip_suffix(".clone()").unwrap_or(arg);
        let ok = if arg.contains(CTOR) {
            true
        } else {
            let is_ident =
                !arg.is_empty() && arg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            is_ident
                && s.body_before.lines().any(|l| {
                    let t = l.trim();
                    t.starts_with(&format!("let {arg} = ")) && t.contains(CTOR)
                })
        };
        assert!(
            ok,
            "`{}::{}` 的 `emit_to` 目标是 `{arg}` —— 它不是 `{CTOR}…)`，也不是同一函数里由它绑定的局部名。\n\
             `&str` / `String` 目标会变成 `EventTarget::AnyLabel`，命不中前端的窗口作用域监听，事件**静默丢弃**\
             （`INVARIANTS §22` 第 2 条）。那一行：{}",
            s.file,
            s.func,
            s.line
        );
    }
}
