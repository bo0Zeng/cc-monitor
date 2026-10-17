//! 要求：「monitor 侧的 `backend/` 目录有一道宿主无关判据（禁 `AppHandle`/`State<`/`.emit(`）与一道平台无关判据」·
//! 「壳里 `src/frontend/shell/src/backend/` 九份逐份回真住址、`backend` 目录名从壳里消失」。
//!
//! 平台无关那一道不在这里：这一组全在壳的源码人群里（壳 `src/` ＋ manifest 声明的兄弟包），
//! 由 `platform/platform_home_guard.rs::platform_forms_live_only_at_home` 一并判（平台形态只许住 `platform/`，没有例外表）。
//!
use std::fs;
use std::path::{Path, PathBuf};

/// 从前 `backend/` 目录那一组（今天都住壳根，通信层成员 `backend_route` 住 `src/comms/inward/`），相对仓根。
/// 删了的两份（`tmux.rs` · `gate2_parity.rs`，子步 1）不在。**加一份调后端的客户端 / 宿主进来就在这里加一行。**
const GUARDED: &[&str] = &[
    "src/frontend/shell/src/backend_control.rs",
    "src/frontend/shell/src/frame_query.rs",
    "src/frontend/shell/src/inbound_client.rs",
    "src/frontend/shell/src/local_backend.rs",
    // 原先被 SFTP 拨号挡路石那张底账挡回的那份：那张账随 SR1b 退役，它今天只问本机后端 `deploy-install` · `deploy-uninstall`，两道都过。
    "src/frontend/shell/src/deploy.rs",
    // 原先被宿主无关判据挡回的那份：`remote-health` 改经注入的 `HealthOut`（`lib.rs::remote_health_out` 造），生产段零窗口把手。
    // 会话流来源是一个目录：整个目录都在人群里（[`guarded_files`] 展开以 `/` 结尾的那一格）。
    "src/frontend/shell/src/stream_source/",
    "src/comms/inward/backend_route.rs",
];

fn repo_root() -> PathBuf {
    guard_core::addr::repo_root()
}

/// 两道守卫共同的扫描面：`(展示名, 绝对路径)`。以 `/` 结尾的那一格是目录，展开成其下每一份 `.rs`。
fn guarded_files() -> Vec<(String, PathBuf)> {
    GUARDED
        .iter()
        .flat_map(|rel| {
            if rel.ends_with('/') {
                let files = guard_core::addr::stream_source_files();
                assert!(
                    files.iter().all(|(f, _)| f.starts_with(rel)),
                    "目录那一格 `{rel}` 与会话流来源的住址对不上"
                );
                files
                    .into_iter()
                    .map(|(f, _)| (f.clone(), repo_root().join(f)))
                    .collect::<Vec<_>>()
            } else {
                vec![(rel.to_string(), repo_root().join(rel))]
            }
        })
        .collect()
}

/// 那一组的 `(绝对路径, 全文)`（别的判据借这一份人群，别各读各的）。读不到 ⇒ 空串（调用方的地板认得出）。
pub(crate) fn guarded_sources() -> Vec<(PathBuf, String)> {
    guarded_files()
        .into_iter()
        .map(|(_, p)| {
            let s = fs::read_to_string(&p).unwrap_or_default();
            (p, s)
        })
        .collect()
}

/// ★ 点名的都在（读不到的文件只会静默返回空串，下面两条会零命中地绿）。
#[test]
fn every_guarded_file_exists() {
    for (f, p) in guarded_files() {
        assert!(
            p.is_file(),
            "点名的 {f} 不在了 —— 搬走 / 改名就改 `GUARDED`，删了就摘掉那一行"
        );
    }
    assert!(Path::new(&repo_root()).is_dir());
}

/// ★ **宿主无关**：那一组的生产段里不许出现 GUI 宿主的把手。
///
/// 这条是「一份代码两种宿主」在今天**唯一可机检的形态**：一旦这里的代码抓了窗口把手
/// 或自己 emit 事件，它就只能跑在 GUI 进程里 —— 而 U8a-2c / U9b 的前提正是它能被
/// 换个宿主跑起来。
///
/// ⚠ **`#[tauri::command]` 是允许的**（登记在案的例外）：它是 IPC 入口的**标注**，
/// 标注之下的函数体仍须宿主无关 —— 那正是本条查的东西。
/// ⚠ 它是**约定型守卫**（同 `readonly_guard` 一族）：查的是符号名的源码形态，
/// 挡得住「顺手 `app.emit` 一下」，挡不住「换个名字继续错」。**比没有强，别读成证明。**
#[test]
fn the_backend_layer_stays_host_agnostic() {
    const FORBIDDEN: &[&str] = &[
        "AppHandle",
        "tauri::Window",
        "WebviewWindow",
        "State<",
        ".emit(",
        "Emitter",
        "Manager",
    ];
    let mut offenders = Vec::new();
    let mut scanned = 0usize;
    for (f, path) in guarded_files() {
        let src = guard_core::production_code(&fs::read_to_string(&path).unwrap_or_default());
        scanned += src.len();
        for needle in FORBIDDEN {
            if src.contains(needle) {
                offenders.push(format!("  {f}: `{needle}`"));
            }
        }
    }
    // 剥完还得有东西可扫 —— 否则这条零命中变绿。
    assert!(
        scanned > 4000,
        "剥掉测试段后只剩 {scanned} 字节可扫，这条会零命中变绿"
    );
    assert!(
        offenders.is_empty(),
        "`backend/` 的生产段抓了 GUI 宿主的把手 —— 那它就只能跑在 GUI 进程里，\n\
             而「一份代码两种宿主」的前提是它能被换个宿主跑起来：\n{}",
        offenders.join("\n")
    );
}
