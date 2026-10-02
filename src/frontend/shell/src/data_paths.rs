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
//! - monitor_data_dir 下所有持久化文件（config / sid-hwnd-cache / auto-launch / history-metadata）
//! - cc 集成短期 IPC 目录（ps-await / ps-registry）
//! - 滚动 log 目录 + 当前 log 文件
//! - WebView2 UserDataFolder 推断路径（基于 Tauri 默认约定）

use crate::copy_table::copy_text;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "camelCase")]
// **那个 `../..` 里有一级是「幻影目录」**（Phase D 审计 S1 查清 `ts-rs` 源码）：
// 有效路径 = `cwd` / `export_dir` / `export_to`，而 `export_dir` 默认是 `./bindings`
// ——**那个目录永不被创建**，它只是被 `../` 抵消掉的一级。所以从 `src/frontend/shell/` 跑测试时：
//   `src/frontend/shell/` + `bindings/` + `../../src/frontend/ui/generated` = `<repo>/src/frontend/ui/generated/` ✓
// 基准是**测试二进制的 cwd**（`std::env::current_dir()`，不是 `CARGO_MANIFEST_DIR`），
// 而 cargo 会把它设成 package root ⇒ `cargo test` 与
// `cargo test --manifest-path src/frontend/shell/Cargo.toml` 都落对（审计双向实测过）。
// **直接跑测试二进制则会落到仓库外**（审计实测落在 cwd 上两级）——CI 安全，
// 因为 `ci.yml` 用 `working-directory: src/frontend/shell`。
// 记这一段是因为：**我第一次写成 `../src/frontend/ui/generated/`，落到了 `src/frontend/shell/src/generated/`**，
// 而不翻 ts-rs 源码是推不出为什么要两级的。
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
    /// 文件大小（bytes）；目录返 None（不递归算大小避免大目录卡 IPC）
    ///
    /// **两个 `ts` 属性都不是装饰，各修掉一次「类型撒谎」**（C01 实测 + Phase D 审计取证）：
    ///
    /// 1. **`optional`**：本字段带 `skip_serializing_if`，`None` 时**字段在 JSON 里整个缺席**，
    ///    TS 侧收到 `undefined` 而不是 `null`。审计逐字节验过序列化产物：
    ///    `None` → `{"label":…,"exists":true}`（key 整个不在）；
    ///    `Some(u64::MAX)` → `…,"sizeBytes":18446744073709551615`。
    ///    机制也查过：`ts-rs` 对 `skip_serializing_if` 只置 `maybe_omitted`，
    ///    而它的兜底分支要求 `maybe_omitted && has_default`——本字段没有 `#[serde(default)]`，
    ///    所以显式 `ts(optional)` 确实必需。
    ///
    ///    **一处措辞订正**：本注释初版写「不加 `optional` 会得到必需且可为 null」——**不准**。
    ///    在 `type = "number"` 同时存在时，实测是 `sizeBytes: number`（必需、**不**可 null）；
    ///    `bigint | null` 只在两个属性都缺席时出现。两个都是谎，但是不同的谎。
    ///    （另：`ts(optional = nullable, type = "number")` 实测也产出 `sizeBytes?: number`
    ///    ——type override 吃掉 nullable，所以那条逃生路不存在。）
    ///
    /// 2. **`type = "number"`**：`ts-rs` 默认把 `u64` 映射成 `bigint`，
    ///    **而 Tauri 的命令 IPC 走 JSON，`u64` 到 TS 侧是 JSON number，不是 BigInt**。
    ///
    ///    **证据分强弱两层，用强的那层**（审计订正）：
    ///    - **强（原理级）**：`tauri-2.11.2/src/ipc/mod.rs:181-183` 的
    ///      `impl<T: Serialize> IpcResponse for T` 走 `serde_json::to_string(&self)`
    ///      ⇒ 线上是 **JSON 文本**，而 `JSON.parse` 永不产出 BigInt
    ///      ⇒ 命令返回值**在原理上不可能**以 BigInt 到达 TS 侧。
    ///    - **弱（现象级，本注释初版用的）**：改动前 `data-section.ts` 直接
    ///      `formatBytes(info.sizeBytes)` 而 `formatBytes` 内有 `.toFixed()`，
    ///      `bigint` 没有该方法 ⇒ 真是 BigInt 的话生产里早就 `TypeError`。
    ///      **它只证明「今天不是 bigint」，不证明「不可能是」。**
    ///    - **仓内同向先例**：用量那一轴的四个 `u64` 字段曾跨边界、TS 侧声明 `number`
    ///      并直接做算术。〔：那一轴整轴退役，**先例的样本没了、结论没变** ——
    ///      全仓无 BigInt 这一条今天照样现打得出来。〕
    ///
    ///    **收窄成 `number` 在这里是安全的**：本字段只用于展示文件大小，
    ///    而 f64 的安全整数上限 2^53-1 ≈ **8 PB**。
    ///    **全局的大整数策略由 C03 定**（哪些字段该走 string 过线）；
    ///    但无论策略如何，**类型不许与运行时不一致**，所以这一处在 C01 修掉。
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
    /// 〔「一台机器一个家」〕本机后端的家（`~/.cc-monitor`，按家目录算，不随 `CCM_DATA_DIR` 漂）
    pub backend_home: String,
    /// 本机后端住在这个家里的那几样（[`backend_entries`]）
    pub backend_entries: Vec<DataPathInfo>,
    /// WebView2 用户数据目录推断路径（cache / localStorage / IndexedDB / cookies）
    pub webview_user_data_dir: Option<DataPathInfo>,
    // 这里原来有 `$PROFILE` 备份目录那一格 —— 「`$PROFILE` 在哪」只由后端方言答，
    //   界面经通道直接问本机后端（`src/frontend/ui/settings/profile-backups.ts`），本命令不再带它。
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
    // 后端的家按家目录算（它自己也是这么落盘的）；取不到家目录 ⇒ 这一张卡空着，不猜。
    // 常驻监听口：与宿主同一个算法（按 Claude 家目录，`local_backend_host` 起常驻时就是这么算的）。
    let listen_port = crate::config::resolve_claude_dir()
        .map(|d| relay_route_core::listen_port_for(&d.to_string_lossy()));
    let (backend_home, backend_entries) = match creds_core::store::home_dir() {
        Some(home) => (
            home.join(backend_home_rel()).display().to_string(),
            backend_entries(&home, &monitor_data_dir, listen_port),
        ),
        None => ("(unknown)".to_string(), Vec::new()),
    };

    let webview_user_data_dir = detect_webview_data_dir(handle);

    DataPathsResponse {
        monitor_data_dir: monitor_data_dir.display().to_string(),
        entries,
        backend_home,
        backend_entries,
        webview_user_data_dir,
    }
}

/// 文件管理窗口书签那份文件的名字 —— **唯一住址**（本文件是数据目录的唯一权威枚举点）。
/// 窗口进程那一侧（`filewin/bookmarks.rs`）引它，app 这一侧不去够窗口模块（「文件管理器可以单独搞」那道门只有一扇）。
pub const FILEWIN_BOOKMARKS_FILE: &str = "filewin-bookmarks.json";

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
        // 🔴 原文「cc 集成的 sid → 终端 HWND 持久绑定」—— `sid` / `HWND`
        //   都是我们这侧的词。换成用户看得懂的说法。
        probe_file(
            monitor_data_dir.join("sid-hwnd-cache.json"),
            "sid-hwnd-cache.json",
            &copy_text("rsDataPaths.monitor.bindings", &[]),
            DataClass::Cache,
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
    ]
}

/// 后端的家（相对家目录的那一段）：取自契约常量里后端落点的第一段（`relay_route_core::BACKEND_LANDING_REL`），不另写一份。
fn backend_home_rel() -> &'static str {
    relay_route_core::BACKEND_LANDING_REL
        .split('/')
        .next()
        .unwrap_or(relay_route_core::BACKEND_LANDING_REL)
}

/// 契约里相对家目录的一段 ⇒ 这一行的名字（去掉后端的家那一段；目录带尾 `/`）。
fn home_rel_label(rel: &str, dir: bool) -> String {
    let tail = rel
        .strip_prefix(backend_home_rel())
        .unwrap_or(rel)
        .trim_start_matches('/');
    if dir {
        format!("{tail}/")
    } else {
        tail.to_string()
    }
}

/// 〔`INVARIANTS §2.1`〕一台机器一个家，家里的都进这一份枚举。
/// **本机后端住在同一个家里的那几样**（只给路径，不给删）。
///
/// 名字各取唯一住址，这里不写字面量：`~/.cc-monitor/` 下相对家目录的那一族取契约常量（`relay_route_core`，后端按同一份落盘；
/// 程序目录 = 后端落点的上一层）· 监听口的进程记录按同一个口（`listen_port` = 宿主按 Claude 家目录算的那个，`None` ⇒ 这一行不列）·
/// API key 那份按数据目录（`creds_core::store::credentials_path`）· 后端错误输出 = 宿主交给它的那份文件所在的目录
/// （`logging::backend_stderr_log_path`）。后端跑着时要用的（两把钥匙 · 进程记录）按真相记：删了要重起后端。
fn backend_entries(
    home: &Path,
    monitor_data_dir: &Path,
    listen_port: Option<u16>,
) -> Vec<DataPathInfo> {
    use relay_route_core as rr;
    let bin = rr::BACKEND_LANDING_REL
        .rsplit_once('/')
        .map_or(rr::BACKEND_LANDING_REL, |(dir, _)| dir);
    let stderr_dir = crate::logging::backend_stderr_log_path(monitor_data_dir)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| monitor_data_dir.to_path_buf());
    let stderr_label = stderr_dir
        .strip_prefix(monitor_data_dir)
        .map(|p| format!("{}/", p.to_string_lossy().replace('\\', "/")))
        .unwrap_or_else(|_| stderr_dir.display().to_string());
    let creds = creds_core::store::credentials_path(monitor_data_dir);
    let dir = |rel: &str, key_text: String, class: DataClass| {
        probe_dir(home.join(rel), &home_rel_label(rel, true), &key_text, class)
    };
    let file = |rel: &str, key_text: String, class: DataClass| {
        probe_file(
            home.join(rel),
            &home_rel_label(rel, false),
            &key_text,
            class,
        )
    };
    let mut out = vec![
        dir(
            bin,
            copy_text("rsDataPaths.backend.bin", &[]),
            DataClass::Cache,
        ),
        dir(
            rr::STAGING_DIR_REL,
            copy_text("rsDataPaths.backend.staging", &[]),
            DataClass::Cache,
        ),
        file(
            rr::KEY_FILE_REL,
            copy_text("rsDataPaths.backend.relayKey", &[]),
            DataClass::Truth,
        ),
        file(
            rr::LISTEN_TOKEN_FILE_REL,
            copy_text("rsDataPaths.backend.listenToken", &[]),
            DataClass::Truth,
        ),
    ];
    if let Some(port) = listen_port {
        let name = rr::listen_pid_file_name(port);
        out.push(probe_file(
            home.join(backend_home_rel()).join(&name),
            &name,
            &copy_text("rsDataPaths.backend.listenPid", &[]),
            DataClass::Truth,
        ));
    }
    out.extend([
        file(
            rr::BACKEND_POLICY_REL,
            copy_text("rsDataPaths.backend.policy", &[]),
            DataClass::Truth,
        ),
        file(
            rr::POSIX_ALIASES_REL,
            copy_text("rsDataPaths.backend.aliasesPosix", &[]),
            DataClass::Truth,
        ),
        file(
            rr::PS_ALIASES_REL,
            copy_text("rsDataPaths.backend.aliasesPs", &[]),
            DataClass::Truth,
        ),
        file(
            rr::SKILL_LEDGER_REL,
            copy_text("rsDataPaths.backend.skillLedger", &[]),
            DataClass::Truth,
        ),
        file(
            rr::ASSET_CATALOG_REL,
            copy_text("rsDataPaths.backend.assetCatalog", &[]),
            DataClass::Cache,
        ),
        dir(
            rr::ACCOUNTS_DIR_REL,
            copy_text("rsDataPaths.backend.accounts", &[]),
            DataClass::Truth,
        ),
        file(
            rr::ACCOUNTS_MCP_REL,
            copy_text("rsDataPaths.backend.accountsMcp", &[]),
            DataClass::Truth,
        ),
        probe_file(
            creds.clone(),
            &creds
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            &copy_text("rsDataPaths.backend.apikey", &[]),
            DataClass::Truth,
        ),
        probe_dir(
            stderr_dir,
            &stderr_label,
            &copy_text("rsDataPaths.backend.logs", &[]),
            DataClass::Cache,
        ),
        dir(
            rr::EXT_BACKUPS_DIR_REL,
            copy_text("rsDataPaths.backend.extBackups", &[]),
            DataClass::Truth,
        ),
    ]);
    out
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

// 这里原来有 `$PROFILE` 备份目录那一族（探 `$PROFILE` 两个目录名 · 目录里有没有 `.ccm-backup-`）——
//   `$PROFILE` 位置的第二个读者；搬到界面经通道问本机后端（`src/frontend/ui/settings/profile-backups.ts`）。

/// IPC：前端设置面板「数据」区打开时调一次。
///
/// async + spawn_blocking：probe 涉及若干次 stat / read_dir，量小但仍是阻塞 IO。
#[tauri::command]
pub async fn get_data_paths(handle: AppHandle) -> Result<DataPathsResponse, String> {
    tokio::task::spawn_blocking(move || Ok(collect(&handle)))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/data_paths_tests.rs"]
mod tests;
