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
//! - PowerShell profile 最近备份目录（如果装过 cc 集成）

use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
// **那个 `../../` 里有一级是「幻影目录」**（Phase D 审计 S1 查清 `ts-rs` 源码）：
// 有效路径 = `cwd` / `export_dir` / `export_to`，而 `export_dir` 默认是 `./bindings`
// ——**那个目录永不被创建**，它只是被 `../` 抵消掉的一级。所以从 `src-tauri/` 跑测试时：
//   `src-tauri/` + `bindings/` + `../../src/generated/` = `<repo>/src/generated/` ✓
// 基准是**测试二进制的 cwd**（`std::env::current_dir()`，不是 `CARGO_MANIFEST_DIR`），
// 而 cargo 会把它设成 package root ⇒ `cargo test` 与
// `cargo test --manifest-path src-tauri/Cargo.toml` 都落对（审计双向实测过）。
// **直接跑测试二进制则会落到仓库外**（审计实测落在 cwd 上两级）——CI 安全，
// 因为 `ci.yml` 用 `working-directory: src-tauri`。
// 记这一段是因为：**我第一次写成 `../src/generated/`，落到了 `src-tauri/src/generated/`**，
// 而不翻 ts-rs 源码是推不出为什么要两级的。
pub struct DataPathInfo {
    /// 用户可见的简短名字（如 "config.json"）
    pub label: String,
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
    ///    - **仓内同向先例**：`usage.rs:24-27` 的 `TokenUsage` 四个 `u64` 字段跨边界，
    ///      TS 侧 `views/usage-pivot.ts` 声明 `number` 并直接做算术。全仓无 BigInt。
    ///
    ///    **收窄成 `number` 在这里是安全的**：本字段只用于展示文件大小，
    ///    而 f64 的安全整数上限 2^53-1 ≈ **8 PB**。
    ///    **全局的大整数策略由 C03 定**（哪些字段该走 string 过线）；
    ///    但无论策略如何，**类型不许与运行时不一致**，所以这一处在 C01 修掉。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DataPathsResponse {
    pub monitor_data_dir: String,
    /// monitor 自己的持久化数据（按类别有序）
    pub entries: Vec<DataPathInfo>,
    /// WebView2 用户数据目录推断路径（cache / localStorage / IndexedDB / cookies）
    pub webview_user_data_dir: Option<DataPathInfo>,
    /// PowerShell profile 备份目录（最多列前 3 个，去重）
    pub profile_backup_dirs: Vec<DataPathInfo>,
}

/// 收集所有 monitor 写到磁盘的数据路径。需要 AppHandle 才能拿 LocalAppData 推断 WebView2 路径。
pub fn collect(handle: &AppHandle) -> DataPathsResponse {
    let monitor_data_dir =
        crate::paths::resolve_monitor_data_dir().unwrap_or_else(|| PathBuf::from("(unknown)"));

    let entries = vec![
        probe_file(
            monitor_data_dir.join("config.json"),
            "config.json",
            "主题 / 字体 / claudeDir override / 诊断开关",
        ),
        probe_file(
            monitor_data_dir.join("sid-hwnd-cache.json"),
            "sid-hwnd-cache.json",
            "cc 集成的 sid → 终端 HWND 持久绑定",
        ),
        probe_file(
            monitor_data_dir.join("auto-launch.json"),
            "auto-launch.json",
            "auto-launch monitor 开关 + 当前 monitor exe 路径",
        ),
        probe_file(
            monitor_data_dir.join("history-metadata.json"),
            "history-metadata.json",
            "历史浏览器：star / 重命名 / 隐藏",
        ),
        probe_dir(
            monitor_data_dir.join("ps-await"),
            "ps-await/",
            "cc 集成短期 IPC：PS 通知 monitor 找窗口（写时存在，握手后被删）",
        ),
        probe_dir(
            monitor_data_dir.join("ps-registry"),
            "ps-registry/",
            "cc 集成短期 IPC：monitor 把绑定结果告诉 PS（PS 进程同寿）",
        ),
        probe_dir(
            monitor_data_dir.join("logs"),
            "logs/",
            "诊断日志（按天滚动，保留 3 天）",
        ),
    ];

    let webview_user_data_dir = detect_webview_data_dir(handle);
    let profile_backup_dirs = detect_profile_backup_dirs();

    DataPathsResponse {
        monitor_data_dir: monitor_data_dir.display().to_string(),
        entries,
        webview_user_data_dir,
        profile_backup_dirs,
    }
}

fn probe_file(path: PathBuf, label: &str, description: &str) -> DataPathInfo {
    let exists = path.is_file();
    let size_bytes = if exists {
        std::fs::metadata(&path).ok().map(|m| m.len())
    } else {
        None
    };
    DataPathInfo {
        label: label.to_string(),
        path: path.display().to_string(),
        kind: "file".to_string(),
        description: description.to_string(),
        exists,
        size_bytes,
    }
}

fn probe_dir(path: PathBuf, label: &str, description: &str) -> DataPathInfo {
    let exists = path.is_dir();
    // 不递归算 dir 大小：避免大日志目录 / WebView2 cache 让 IPC 阻塞数秒。
    // 前端如果想看大小，自己通过 [打开] 进资源管理器查。
    DataPathInfo {
        label: label.to_string(),
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
        "WebView2 cache / localStorage / IndexedDB / cookies。由 WebView2 Runtime 管理。",
    ))
}

/// 扫 PowerShell profile 的备份目录（profile_installer 写入 `<profile>.ccm-backup-<ms>`）。
///
/// monitor 不持久化备份位置——这里只在 5 个标准 profile 位置探一遍。
fn detect_profile_backup_dirs() -> Vec<DataPathInfo> {
    let candidates = candidate_profile_dirs();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for dir in candidates {
        let key = dir.to_string_lossy().to_lowercase();
        if !seen.insert(key) {
            continue;
        }
        if has_backup_in_dir(&dir) {
            out.push(probe_dir(
                dir,
                "PowerShell profile 备份目录",
                "v1.7.10+ 装 cc 集成时自动备份到 <profile>.ccm-backup-<时间戳>",
            ));
        }
        if out.len() >= 3 {
            break;
        }
    }
    out
}

fn candidate_profile_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = dirs::home_dir() {
        // PS 5.1 默认位置
        out.push(home.join("Documents").join("WindowsPowerShell"));
        // PS 7.x 默认位置
        out.push(home.join("Documents").join("PowerShell"));
        // OneDrive 重定向场景
        if let Ok(onedrive) = std::env::var("OneDrive") {
            out.push(
                PathBuf::from(&onedrive)
                    .join("Documents")
                    .join("WindowsPowerShell"),
            );
            out.push(
                PathBuf::from(&onedrive)
                    .join("Documents")
                    .join("PowerShell"),
            );
        }
    }
    out
}

fn has_backup_in_dir(dir: &Path) -> bool {
    let Ok(it) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in it.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.contains(".ccm-backup-") {
            return true;
        }
    }
    false
}

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
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestDir(PathBuf);
    impl TestDir {
        fn new(tag: &str) -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let p = std::env::temp_dir().join(format!(
                "ccm-data-paths-test-{}-{tag}-{n}",
                std::process::id(),
            ));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            TestDir(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn probe_file_returns_exists_and_size_when_present() {
        let d = TestDir::new("probe-file");
        let p = d.path().join("x.json");
        fs::write(&p, "hello").unwrap();
        let info = probe_file(p.clone(), "x.json", "test");
        assert!(info.exists);
        assert_eq!(info.size_bytes, Some(5));
        assert_eq!(info.kind, "file");
    }

    #[test]
    fn probe_file_returns_not_exists_when_absent() {
        let d = TestDir::new("probe-file-absent");
        let info = probe_file(d.path().join("absent.json"), "absent.json", "test");
        assert!(!info.exists);
        assert!(info.size_bytes.is_none());
    }

    #[test]
    fn probe_dir_never_returns_size() {
        let d = TestDir::new("probe-dir");
        fs::write(d.path().join("a"), "abc").unwrap();
        let info = probe_dir(d.path().to_path_buf(), "x/", "test");
        assert!(info.exists);
        assert!(info.size_bytes.is_none());
        assert_eq!(info.kind, "dir");
    }

    #[test]
    fn has_backup_in_dir_detects_ccm_backup_files() {
        let d = TestDir::new("backup-detect");
        // 没 backup
        assert!(!has_backup_in_dir(d.path()));
        // 加一个普通文件
        fs::write(d.path().join("profile.ps1"), "").unwrap();
        assert!(!has_backup_in_dir(d.path()));
        // 加 backup
        fs::write(d.path().join("profile.ps1.ccm-backup-12345"), "").unwrap();
        assert!(has_backup_in_dir(d.path()));
    }
}
