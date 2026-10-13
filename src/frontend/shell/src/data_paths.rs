//! Issue #3 (A 透明化)：枚举 monitor 写到磁盘的所有持久化数据 + WebView2 用户数据
//! 路径，给设置面板的"数据"区做展示。**只读，不动数据**。
//!
//! 一个 IPC 把所有信息一次性返回（≤ 20 个路径，文件 stat 极快）。前端按类别渲染 +
//! [打开] 按钮触发 tauri-plugin-opener。
//!
//! 不包含：
//! - localStorage keys：前端自己 `Object.entries(localStorage)` 拿
//! - Claude Code CLI 自己写的数据（projects / sessions / tasks）：那是用户数据源不归 monitor 管
//!
//! 包含：
//! - monitor_data_dir 下所有持久化文件（config / auto-launch / history-metadata）
//! - cc 集成短期 IPC 目录（ps-await / ps-registry）
//! - 滚动 log 目录 + 当前 log 文件（连同宿主交给本机后端的错误输出）· 填入的 API key
//! - 本机后端家里（`~/.cc-monitor/`）那几样**不在这里**：界面照本机后端 `data-report` 的 `own` 画
//! - WebView2 UserDataFolder 推断路径（基于 Tauri 默认约定）

use crate::copy_table::copy_text;
use crate::detail::Said;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "camelCase")]
// `../..` 里有一级是「幻影目录」：ts-rs 的有效路径 = `cwd` / `export_dir` / `export_to`，而 `export_dir` 默认是 `./bindings`、永不被创建，只是被 `../` 抵消掉的一级。
// 从 `src/frontend/shell/` 跑测试时：`src/frontend/shell/` + `bindings/` + `../../src/frontend/ui/generated` = `<repo>/src/frontend/ui/generated/`。
// 基准是测试二进制的 cwd（`std::env::current_dir()`，不是 `CARGO_MANIFEST_DIR`），cargo 会把它设成 package root；直接跑测试二进制会落到仓库外
// （`ci.yml` 用 `working-directory: src/frontend/shell`，CI 安全）。
pub struct DataPathInfo {
    /// 用户可见的简短名字（如 "config.json"）
    pub label: String,
    /// 〔`INVARIANTS §2.1` 末条〕**真相还是缓存**。
    ///
    /// **非可选**：[`probe_file`] / [`probe_dir`] 不收它就编不过 ⇒ 「新文件必须选类」由类型系统兜住，
    /// 不再只靠 `INVARIANTS §2.1` 那张散文表。设置面板「数据位置」每一行照它说「删了会丢 / 可随手删」。
    pub class: DataClass,
    /// 绝对路径
    pub path: String,
    /// "file" | "dir"
    pub kind: String,
    /// 存什么的简短描述
    pub description: String,
    /// 是否存在
    pub exists: bool,
    /// 文件大小（bytes）；目录返 None（不递归算大小，避免大目录卡 IPC）。两个 `ts` 属性都是承重的：
    /// 1. `optional`：本字段带 `skip_serializing_if`，`None` 时字段在 JSON 里整个缺席，TS 侧收到 `undefined`；ts-rs 只在 `maybe_omitted && has_default` 时兜底，
    ///    本字段没有 `#[serde(default)]` ⇒ 显式 `ts(optional)` 必需（`ts(optional = nullable, type = "number")` 产出的也是 `sizeBytes?: number`：type override 吃掉 nullable）。
    /// 2. `type = "number"`：ts-rs 默认把 `u64` 映射成 `bigint`，而 Tauri 的命令 IPC 走 JSON（`impl<T: Serialize> IpcResponse for T` 走 `serde_json::to_string`），
    ///    `JSON.parse` 永不产出 BigInt。收窄成 `number` 安全：只用于展示文件大小，f64 的安全整数上限 2^53-1 ≈ 8 PB。类型不许与运行时不一致。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub size_bytes: Option<u64>,
}

/// `INVARIANTS §2.1` 那两类：**真相**（用户手写 / 意图，删了丢东西）与**缓存 / 派生**（能从别处重建，随便删）。
///
/// ⚠ 只有两档，没有「混」：`auto-launch.json` 那种「一个文件里既有真相又有派生」按**真相**记 ——
/// 这一格回答的是「删了会不会丢东西」，混着真相的文件删了就会丢。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub enum DataClass {
    /// 用户手写 / 意图：删了丢东西，要备份。
    Truth,
    /// 能从别处重建：随手删，下次用到时重建。
    Cache,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DataPathsResponse {
    pub monitor_data_dir: String,
    /// monitor 自己的持久化数据（按类别有序）
    pub entries: Vec<DataPathInfo>,
    /// WebView2 用户数据目录推断路径（cache / localStorage / IndexedDB / cookies）
    pub webview_user_data_dir: Option<DataPathInfo>,
    // `$PROFILE` 在哪只由后端方言答，本命令不带它。
    // 本机后端家里（`~/.cc-monitor/`）那几样也不带：界面照本机后端 `data-report` 的 `own` 画（`footprint/data.rs::own_rows`）。
}

/// 日志目录那一行的名字。设置面板认它：那一行不自带 [打开]，改成指向「日志」那一块
/// （越界是界面层的重复，修在界面层 —— **不许**从这份枚举里删掉它，`INVARIANTS §2.1` 的唯一权威枚举点）。
/// ⚠ 跨语言常量：TS 那侧 `settings/data-section.ts::LOGS_DIR_LABEL` 同名同值，由那边的 vitest 读本文件对拍。
pub const LOGS_DIR_LABEL: &str = "logs/";

/// 收集所有 monitor 写到磁盘的数据路径。需要 AppHandle 才能拿 LocalAppData 推断 WebView2 路径。
pub fn collect(handle: &AppHandle) -> DataPathsResponse {
    let monitor_data_dir =
        crate::config::resolve_monitor_data_dir().unwrap_or_else(|| PathBuf::from("(unknown)"));

    let entries = monitor_entries(&monitor_data_dir);
    let webview_user_data_dir = detect_webview_data_dir(handle);

    DataPathsResponse {
        monitor_data_dir: monitor_data_dir.display().to_string(),
        entries,
        webview_user_data_dir,
    }
}

/// 文件管理窗口书签那份文件的名字 —— **唯一住址**（本文件是数据目录的唯一权威枚举点）。
/// 窗口进程那一侧（`filewin/bookmarks.rs`）引它，app 这一侧不去够窗口模块（「文件管理器可以单独搞」那道门只有一扇）。
pub const FILEWIN_BOOKMARKS_FILE: &str = "filewin-bookmarks.json";

/// 文件管理窗口记整窗缩放那份文件的名字 —— 同 [`FILEWIN_BOOKMARKS_FILE`]（开窗入口拼好全路径、随种子交给窗口）。
pub const FILEWIN_VIEW_FILE: &str = "filewin-view.json";

/// monitor data dir 下逐个文件 / 目录的枚举 —— **唯一权威枚举点**（`INVARIANTS §2.1`）。
/// 从 [`collect`] 里抽出来，只为让「每一项是哪一类」能不带 `AppHandle` 地被判据逐项对拍。
fn monitor_entries(monitor_data_dir: &Path) -> Vec<DataPathInfo> {
    vec![
        probe_file(
            monitor_data_dir.join("config.json"),
            "config.json",
            &copy_text("rsDataPaths.monitor.config", &[]),
            DataClass::Truth,
        ),
        probe_file(
            monitor_data_dir.join("auto-launch.json"),
            "auto-launch.json",
            &copy_text("rsDataPaths.monitor.autoLaunch", &[]),
            // 开关是你选的（真相）；exe 路径每次启动自愈重写（派生）⇒ 整份按真相记（见 `DataClass`）。
            DataClass::Truth,
        ),
        probe_file(
            monitor_data_dir.join("history-metadata.json"),
            "history-metadata.json",
            &copy_text("rsDataPaths.monitor.historyMeta", &[]),
            DataClass::Truth,
        ),
        // 文件管理窗口的书签（monitor 自己的状态文件，与 `config.json` 同一族）。
        //   名字只住本文件 [`FILEWIN_BOOKMARKS_FILE`] 一处（开窗入口 `filewin/entry.rs::open_with` 用它拼好全路径、随种子交给窗口）；你收藏的目录 ⇒ 删了会丢，按真相记。
        probe_file(
            monitor_data_dir.join(FILEWIN_BOOKMARKS_FILE),
            FILEWIN_BOOKMARKS_FILE,
            &copy_text("rsDataPaths.monitor.bookmarks", &[]),
            DataClass::Truth,
        ),
        // 文件管理窗口的整窗缩放（你调过的大小 ⇒ 按真相记）。
        probe_file(
            monitor_data_dir.join(FILEWIN_VIEW_FILE),
            FILEWIN_VIEW_FILE,
            &copy_text("rsDataPaths.monitor.filewinView", &[]),
            DataClass::Truth,
        ),
        probe_dir(
            monitor_data_dir.join("ps-await"),
            "ps-await/",
            &copy_text("rsDataPaths.monitor.integrationSignal", &[]),
            DataClass::Cache,
        ),
        probe_dir(
            monitor_data_dir.join("ps-registry"),
            "ps-registry/",
            &copy_text("rsDataPaths.monitor.psRegistry", &[]),
            DataClass::Cache,
        ),
        probe_dir(
            monitor_data_dir.join("logs"),
            LOGS_DIR_LABEL,
            &copy_text("rsDataPaths.monitor.logs", &[]),
            DataClass::Cache,
        ),
        // 宿主交给本机后端的错误输出（`logging::backend_stderr_log_path` 所在的目录）：住数据目录、不在后端的家里 ⇒ 由这里列。
        backend_stderr_dir(monitor_data_dir),
        // 填入的 API key（`creds_core::store::credentials_path`）：同样住数据目录。
        {
            let creds = creds_core::store::credentials_path(monitor_data_dir);
            let name = creds
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            probe_file(
                creds,
                &name,
                &copy_text("rsDataPaths.monitor.apikey", &[]),
                DataClass::Truth,
            )
        },
    ]
}

/// 本机后端错误输出所在的目录那一行：名字 ＝ 相对数据目录的那一段（目录带尾 `/`，落在 `logs/` 下 ⇒ 界面照样指去「日志」页）。
fn backend_stderr_dir(monitor_data_dir: &Path) -> DataPathInfo {
    let dir = crate::logging::backend_stderr_log_path(monitor_data_dir)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| monitor_data_dir.to_path_buf());
    let label = dir
        .strip_prefix(monitor_data_dir)
        .map(|p| format!("{}/", p.to_string_lossy().replace('\\', "/")))
        .unwrap_or_else(|_| dir.display().to_string());
    probe_dir(
        dir,
        &label,
        &copy_text("rsDataPaths.monitor.backendLogs", &[]),
        DataClass::Cache,
    )
}

fn probe_file(path: PathBuf, label: &str, description: &str, class: DataClass) -> DataPathInfo {
    let exists = path.is_file();
    let size_bytes = if exists {
        std::fs::metadata(&path).ok().map(|m| m.len())
    } else {
        None
    };
    DataPathInfo {
        label: label.to_string(),
        class,
        path: path.display().to_string(),
        kind: "file".to_string(),
        description: description.to_string(),
        exists,
        size_bytes,
    }
}

fn probe_dir(path: PathBuf, label: &str, description: &str, class: DataClass) -> DataPathInfo {
    let exists = path.is_dir();
    // 不递归算 dir 大小：避免大日志目录 / WebView2 cache 让 IPC 阻塞数秒。
    // 前端如果想看大小，自己通过 [打开] 进资源管理器查。
    DataPathInfo {
        label: label.to_string(),
        class,
        path: path.display().to_string(),
        kind: "dir".to_string(),
        description: description.to_string(),
        exists,
        size_bytes: None,
    }
}

/// 推断 WebView2 UserDataFolder。
///
/// Tauri 2 默认 WebView2 UserDataFolder 在 `app_local_data_dir()/EBWebView/`，
/// `app_local_data_dir()` = `%LOCALAPPDATA%\<identifier>`（Windows）/
/// `~/Library/Application Support/<identifier>` (macOS) / `~/.local/share/<identifier>` (Linux)。
///
/// 这是约定，不是保证——若未来 Tauri/wry 改默认路径，前端显示的 dir 可能不准。但反正这是
/// "透明化" 的展示，存在性会反映真实情况。
fn detect_webview_data_dir(handle: &AppHandle) -> Option<DataPathInfo> {
    let local_data = handle.path().app_local_data_dir().ok()?;
    let webview_dir = local_data.join("EBWebView");
    Some(probe_dir(
        webview_dir,
        "WebView2 / EBWebView/",
        &copy_text("rsDataPaths.webview.dataDir", &[]),
        // 里面有 localStorage（界面偏好）⇒ 删了会丢东西，按真相记。
        DataClass::Truth,
    ))
}

/// IPC：前端设置面板「数据」区打开时调一次。
///
/// async + spawn_blocking：probe 涉及若干次 stat / read_dir，量小但仍是阻塞 IO。
#[tauri::command]
pub async fn get_data_paths(handle: AppHandle) -> Result<DataPathsResponse, Said> {
    let r: Result<DataPathsResponse, Said> = async move {
        tokio::task::spawn_blocking(move || Ok(collect(&handle)))
            .await
            .map_err(Said::crashed)?
    }
    .await;
    r.map_err(|s| s.named("get_data_paths"))
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/data_paths_tests.rs"]
mod tests;
