//! PowerShell profile 路径解析 + 块插入/卸载 + 命令名冲突检测。
//!
//! ## 块标记
//!
//! cc-monitor 写入 profile 用明显的块边界，可被工具识别、原子替换、整块卸载：
//!
//! ```text
//! # === cc-monitor BEGIN v1 ===
//! ... cc function 内容 ...
//! # === cc-monitor END ===
//! ```
//!
//! 重装时找到 BEGIN/END 范围整块替换；卸载时整块删除。用户在块外的任何内容不动。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const BEGIN_MARKER: &str = "# === cc-monitor BEGIN";
const END_MARKER: &str = "# === cc-monitor END";

/// cc function 模板源码（含 `{{COMMAND_NAME}}` placeholder）
const CC_TEMPLATE: &str = include_str!("../scripts/cc.ps1.tpl");

/// PowerShell profile 类型标签。v1.7.2 起 UI 只用作显示提示，实际安装传 path。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProfileKind {
    /// Windows PowerShell 5.1（Windows 自带）→ Microsoft.PowerShell_profile.ps1
    Ps51,
    /// PowerShell 7.x（独立安装）→ Microsoft.PowerShell_profile.ps1
    Ps7,
    /// 用户自定义路径
    Custom,
}

impl ProfileKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ps51 => "Windows PowerShell 5.1",
            Self::Ps7 => "PowerShell 7.x",
            Self::Custom => "自定义路径",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProfileScan {
    pub kind: ProfileKind,
    pub path: String,
    pub exists: bool,
    /// 是否含 cc-monitor BEGIN/END 块
    pub has_ccm_block: bool,
    /// 块中的版本字符串（"v1" 等）
    pub ccm_block_version: Option<String>,
    /// 已有同名 function（非 ccm 块内的）
    pub conflicting_functions: Vec<String>,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct IntegrationStatus {
    pub profiles: Vec<ProfileScan>,
    pub active_registrations: u32,
    pub default_command_name: String,
}

/// 解析当前用户实际安装的 PS profile 路径。
///
/// **关键**：用 `Microsoft.PowerShell_profile.ps1`（`$PROFILE` 默认指向，即
/// CurrentUserCurrentHost）而非 `profile.ps1`（CurrentUserAllHosts，对所有
/// host 包括 ISE / VSCode 集成 terminal 生效，但绝大多数用户不用这个）。
///
/// v1.7.0-1.7.1 错用 `profile.ps1` → PowerShell 启动时根本不读那个文件 →
/// cc 集成形同虚设。v1.7.2 修正到默认 `$PROFILE`。
///
/// **自动识别**：
///   - PS 5.1 永远显示（Windows 自带）
///   - PS 7.x 只在 `Documents/PowerShell/` 目录存在时显示（说明用户装过且至少跑过一次）
pub fn discover_profiles() -> Vec<(ProfileKind, PathBuf)> {
    let Some(home) = dirs::document_dir() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    out.push((
        ProfileKind::Ps51,
        home.join("WindowsPowerShell")
            .join("Microsoft.PowerShell_profile.ps1"),
    ));
    let ps7_dir = home.join("PowerShell");
    if ps7_dir.exists() {
        out.push((
            ProfileKind::Ps7,
            ps7_dir.join("Microsoft.PowerShell_profile.ps1"),
        ));
    }
    out
}

/// v1.7.0-1.7.1 错位 profile 路径（已废弃，仅用于检测是否有遗留块需要清理）。
fn legacy_profile_paths() -> Vec<(ProfileKind, PathBuf)> {
    let Some(home) = dirs::document_dir() else {
        return Vec::new();
    };
    vec![
        (
            ProfileKind::Ps51,
            home.join("WindowsPowerShell").join("profile.ps1"),
        ),
        (
            ProfileKind::Ps7,
            home.join("PowerShell").join("profile.ps1"),
        ),
    ]
}

/// 扫所有 v1.7.0-1.7.1 错位 profile 文件，看哪些含 cc-monitor 块（需要用户手动清理）。
pub fn scan_legacy_profiles() -> Vec<(ProfileKind, String)> {
    let mut out = Vec::new();
    for (kind, path) in legacy_profile_paths() {
        if !path.exists() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let (has_block, _) = find_block_version(&content);
        if has_block {
            out.push((kind, path.to_string_lossy().into_owned()));
        }
    }
    out
}

/// 扫描任意路径的 profile 文件（v1.7.2 用户自定义路径用）。kind 字段标 Custom。
pub fn scan_path(path: &PathBuf, command_name: &str) -> ProfileScan {
    scan_profile(ProfileKind::Custom, path, command_name)
}

/// 扫描一个 profile 文件：是否存在 / 是否含 ccm 块 / 检测命令名冲突。
pub fn scan_profile(kind: ProfileKind, path: &PathBuf, command_name: &str) -> ProfileScan {
    let path_str = path.to_string_lossy().into_owned();
    if !path.exists() {
        return ProfileScan {
            kind,
            path: path_str,
            exists: false,
            has_ccm_block: false,
            ccm_block_version: None,
            conflicting_functions: Vec::new(),
            size_bytes: 0,
        };
    }
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let size_bytes = content.len() as u64;
    let (block_present, block_version) = find_block_version(&content);
    let conflicts = find_conflicting_functions(&content, command_name);
    ProfileScan {
        kind,
        path: path_str,
        exists: true,
        has_ccm_block: block_present,
        ccm_block_version: block_version,
        conflicting_functions: conflicts,
        size_bytes,
    }
}

/// 生成将要写入的代码（替换 placeholder）。
///
/// - `include_cc_function = true`：装 `__ccm_bind` helper **加上** `function {name}`
///   （适合 profile 里没有自定义 cc 的新用户，一键 work）。
/// - `include_cc_function = false`：只装 `__ccm_bind` helper（适合用户已有自定义
///   `function cc`——避免覆盖用户原有 cd/代理/etc 逻辑，用户自己在 cc 开头加
///   `__ccm_bind` 一行调用即可）。
pub fn render_cc_code(command_name: &str, include_cc_function: bool) -> String {
    let safe_name = sanitize_command_name(command_name);
    let cc_block = if include_cc_function {
        format!(
            "\nfunction {0} {{\n    [CmdletBinding()] param(\n        [Parameter(ValueFromRemainingArguments = $true)] $RemainingArgs\n    )\n    __ccm_bind\n    & claude $RemainingArgs\n}}\n",
            safe_name
        )
    } else {
        String::new()
    };
    CC_TEMPLATE.replace("{{CC_FUNCTION_BLOCK}}", &cc_block)
}

/// idempotent 安装：把 cc function 块写到 profile，已有 ccm 块则原地替换。
/// 用户在 BEGIN/END 块外的内容完全不动。
///
/// `include_cc_function = false` 时只装 `__ccm_bind` helper，不抢 cc function 名。
///
/// v1.7.10 安全加固（修 v1.7.9 留下的"profile 写坏"事故）：
///  1. 文件存在时**必先备份**到 `<path>.ccm-backup-<ms>` 再动笔
///  2. 写入走 `MoveFileExW(REPLACE_EXISTING)` 真原子（之前是 remove + rename
///     非原子，rename 失败会留下空文件 + 原内容丢失）
///  3. 写完立即 read 回来校验长度 == 期望长度；不匹配从 backup 回滚
///  4. 若 `path.exists()` 但 read 出空字符串（OneDrive placeholder / 文件锁等
///     罕见情况），**直接 abort 不写**——避免 existing="" + 块追加 = 用户内容被冲掉
pub fn install_to_profile(
    path: &PathBuf,
    command_name: &str,
    include_cc_function: bool,
) -> Result<(), String> {
    let code = render_cc_code(command_name, include_cc_function);

    let (existing, did_exist) = if path.exists() {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("read existing profile failed: {e}"))?;
        let on_disk_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        // 防御：文件在磁盘上有内容但读到 "" —— 罕见但要拦下来。OneDrive / 杀软介入
        // 可能让 read_to_string 在某些情况下返回 Ok("")。继续走会用 "" + new code
        // 覆盖原内容（v1.7.9 事故的可能子原因之一）。
        if on_disk_size > 0 && raw.is_empty() {
            return Err(format!(
                "profile 文件 {} 在磁盘上有 {} 字节但读到空内容（可能被 OneDrive/杀软锁定）。\
                 取消安装。请先在文件资源管理器里确认文件可读后重试。",
                path.display(),
                on_disk_size
            ));
        }
        (raw, true)
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create profile dir failed: {e}"))?;
        }
        (String::new(), false)
    };

    let updated = replace_or_append_block(&existing, &code);

    // 写之前先备份原文件（即使没动 BEGIN/END 块外的内容，也防 atomic_write 异常）
    let backup_path = if did_exist && !existing.is_empty() {
        let backup = backup_path_for(path);
        std::fs::copy(path, &backup)
            .map_err(|e| format!("backup profile to {} failed: {e}", backup.display()))?;
        Some(backup)
    } else {
        None
    };

    if let Err(e) = atomic_write_string(path, &updated) {
        // 写入失败：尝试从 backup 恢复
        if let Some(b) = &backup_path {
            let _ = std::fs::copy(b, path); // best-effort
        }
        return Err(format!(
            "write profile failed: {e}\n备份保留在: {}",
            backup_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(无备份，原文件不存在)".into())
        ));
    }

    // 写完读回来校验
    let written = std::fs::read_to_string(path)
        .map_err(|e| format!("写后回读校验失败: {e}（请检查 {} 内容）", path.display()))?;
    if written.len() != updated.len() {
        if let Some(b) = &backup_path {
            let _ = std::fs::copy(b, path);
        }
        return Err(format!(
            "写入后回读长度不匹配：期望 {} 字节，实际 {} 字节。已尝试从备份恢复。",
            updated.len(),
            written.len()
        ));
    }

    Ok(())
}

fn backup_path_for(path: &PathBuf) -> PathBuf {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut p = path.clone();
    let fname = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "profile".to_string());
    p.set_file_name(format!("{fname}.ccm-backup-{ms}"));
    p
}

/// 卸载：整块删除 BEGIN/END 之间的内容（含 marker 行）。块外内容不动。
///
/// v1.7.10：同 install 加 backup + 写后校验，避免卸载半途坏文件。
pub fn uninstall_from_profile(path: &PathBuf) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let existing =
        std::fs::read_to_string(path).map_err(|e| format!("read existing profile failed: {e}"))?;
    let on_disk_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if on_disk_size > 0 && existing.is_empty() {
        return Err(format!(
            "profile 文件 {} 在磁盘上有 {} 字节但读到空内容，取消卸载。",
            path.display(),
            on_disk_size
        ));
    }
    let stripped = strip_block(&existing);
    if stripped == existing {
        return Ok(()); // 没有块，无需写
    }

    let backup = backup_path_for(path);
    std::fs::copy(path, &backup)
        .map_err(|e| format!("backup profile to {} failed: {e}", backup.display()))?;

    if let Err(e) = atomic_write_string(path, &stripped) {
        let _ = std::fs::copy(&backup, path);
        return Err(format!(
            "write profile failed: {e}\n备份保留在: {}",
            backup.display()
        ));
    }
    let written = std::fs::read_to_string(path)
        .map_err(|e| format!("写后回读校验失败: {e}（请检查 {} 内容）", path.display()))?;
    if written.len() != stripped.len() {
        let _ = std::fs::copy(&backup, path);
        return Err(format!(
            "卸载后回读长度不匹配：期望 {} 字节，实际 {} 字节。已从备份恢复。",
            stripped.len(),
            written.len()
        ));
    }
    Ok(())
}

// === 内部 helpers ===

/// 找文件中第一个 cc-monitor 块的版本字符串（"v1" 等）。
fn find_block_version(content: &str) -> (bool, Option<String>) {
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix(BEGIN_MARKER) {
            // rest 可能是 " v1 ===" 之类
            let trimmed = rest.trim().trim_end_matches('=').trim();
            // trimmed = "v1"
            return (
                true,
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                },
            );
        }
    }
    (false, None)
}

/// 扫描 profile 找跟 command_name 同名的 function 定义（在 BEGIN/END 块外的）。
fn find_conflicting_functions(content: &str, command_name: &str) -> Vec<String> {
    let safe = sanitize_command_name(command_name);
    let mut inside_ccm_block = false;
    let mut hits = Vec::new();
    // 简单 line-based regex 替代：检查 "function <name>" 模式
    for line in content.lines() {
        let l = line.trim_start();
        if l.starts_with(BEGIN_MARKER) {
            inside_ccm_block = true;
            continue;
        }
        if l.starts_with(END_MARKER) {
            inside_ccm_block = false;
            continue;
        }
        if inside_ccm_block {
            continue;
        }
        // 简化匹配：以 "function" 开头 + 空白 + 同名（后跟空白/{/(）
        if let Some(rest) = l
            .strip_prefix("function ")
            .or_else(|| l.strip_prefix("function\t"))
        {
            let rest = rest.trim_start();
            // 取 function 后面的标识符
            let end = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
                .unwrap_or(rest.len());
            let name = &rest[..end];
            if name.eq_ignore_ascii_case(&safe) {
                hits.push(safe.clone());
                break;
            }
        }
    }
    hits
}

/// 找已有 cc-monitor 块的范围（line index, inclusive）。返回 None 表示没有。
fn find_block_range(content: &str) -> Option<(usize, usize)> {
    let mut begin: Option<usize> = None;
    for (idx, line) in content.lines().enumerate() {
        let l = line.trim_start();
        if l.starts_with(BEGIN_MARKER) && begin.is_none() {
            begin = Some(idx);
        } else if l.starts_with(END_MARKER) {
            if let Some(b) = begin {
                return Some((b, idx));
            }
        }
    }
    None
}

/// 在 existing 中替换 ccm 块；若不存在则追加。
fn replace_or_append_block(existing: &str, new_block: &str) -> String {
    if let Some((begin, end)) = find_block_range(existing) {
        // 替换：[0, begin) + new_block + (end, end_of_file]
        let lines: Vec<&str> = existing.lines().collect();
        let before = lines[..begin].join("\n");
        let after = if end + 1 < lines.len() {
            lines[(end + 1)..].join("\n")
        } else {
            String::new()
        };
        let mut out = String::new();
        if !before.is_empty() {
            out.push_str(&before);
            if !before.ends_with('\n') {
                out.push('\n');
            }
        }
        out.push_str(new_block.trim_end());
        out.push('\n');
        if !after.is_empty() {
            out.push_str(&after);
            if !after.ends_with('\n') {
                out.push('\n');
            }
        }
        out
    } else {
        // 追加
        let mut out = existing.to_string();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(new_block.trim_end());
        out.push('\n');
        out
    }
}

/// 删除 ccm 块（如果有）。
fn strip_block(existing: &str) -> String {
    let Some((begin, end)) = find_block_range(existing) else {
        return existing.to_string();
    };
    let lines: Vec<&str> = existing.lines().collect();
    let before = lines[..begin].join("\n");
    let after = if end + 1 < lines.len() {
        lines[(end + 1)..].join("\n")
    } else {
        String::new()
    };
    let mut out = String::new();
    if !before.is_empty() {
        out.push_str(&before);
        if !before.ends_with('\n') {
            out.push('\n');
        }
    }
    if !after.is_empty() {
        out.push_str(&after);
        if !after.ends_with('\n') {
            out.push('\n');
        }
    }
    // 防止文件结尾多空行
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

/// 命令名只允许字母数字下划线（防注入）。
fn sanitize_command_name(name: &str) -> String {
    let trimmed = name.trim();
    let cleaned: String = trimmed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if cleaned.is_empty() {
        "cc".to_string()
    } else {
        cleaned
    }
}

/// 原子写文件：写 .tmp 后用 `ReplaceFileW` 一步替换，**保留 dst 原有 ACL**。
///
/// v1.7.9 及之前用三步 `write(tmp) -> remove(path) -> rename(tmp, path)` 非原子，
/// 中途失败原文件丢失。v1.7.10 一开始改 MoveFileExW，但 MoveFileExW 仍把 tmp
/// 文件 ACL 写到 dst —— 如果 dst 父目录没给当前用户 explicit ACE（如用户把
/// Documents 重定向到非默认盘），用户自己都会读不了。
///
/// 正解：`ReplaceFileW(dst, src, ...)` —— 这个 API 专门做"原子替换内容但保留
/// dst 的 ACL / ADS / 创建时间"。Windows 文档明确推荐用它替换配置文件。
///
/// dst 不存在时 ReplaceFileW 会失败，fallback 到普通 rename（首次安装场景）。
/// tmp 文件名加 PID + 时间戳避免并行写碰撞。
fn atomic_write_string(path: &PathBuf, content: &str) -> std::io::Result<()> {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut tmp = path.clone();
    let fname = tmp
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "profile.ps1".to_string());
    tmp.set_file_name(format!("{fname}.ccm-tmp-{ms}-{}", std::process::id()));
    std::fs::write(&tmp, content)?;
    let r = atomic_replace_path(&tmp, path);
    if r.is_err() {
        // 替换失败：清掉 tmp 不留垃圾
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

#[cfg(windows)]
fn atomic_replace_path(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{ReplaceFileW, REPLACEFILE_WRITE_THROUGH};

    let to_wide = |p: &std::path::Path| -> Vec<u16> {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let src_w = to_wide(src);
    let dst_w = to_wide(dst);

    if !dst.exists() {
        // dst 不存在 ReplaceFileW 会失败；首次安装直接 rename（新文件 ACL 继承
        // 父目录，这是 Windows 创建文件的正常行为，没东西可保留）
        return std::fs::rename(src, dst);
    }

    unsafe {
        ReplaceFileW(
            PCWSTR(dst_w.as_ptr()),
            PCWSTR(src_w.as_ptr()),
            PCWSTR::null(),
            REPLACEFILE_WRITE_THROUGH,
            None,
            None,
        )
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.message().to_string()))
    }
}

#[cfg(not(windows))]
fn atomic_replace_path(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::rename(src, dst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_cc_code_with_function() {
        let out = render_cc_code("ccm", true);
        assert!(out.contains("function ccm"));
        assert!(out.contains("__ccm_bind"));
        assert!(!out.contains("{{CC_FUNCTION_BLOCK}}"));
        assert!(out.contains("BEGIN v1"));
        assert!(out.contains("cc-monitor END"));
    }

    #[test]
    fn render_cc_code_helper_only() {
        // 用户已有自定义 function cc 时只装 __ccm_bind helper，不生成 function cc
        let out = render_cc_code("cc", false);
        assert!(out.contains("__ccm_bind"));
        assert!(!out.contains("function cc"));
        assert!(!out.contains("{{CC_FUNCTION_BLOCK}}"));
        assert!(out.contains("BEGIN v1"));
    }

    #[test]
    fn sanitize_command_name_strips_specials() {
        assert_eq!(sanitize_command_name("cc"), "cc");
        assert_eq!(sanitize_command_name("my-cc"), "mycc");
        assert_eq!(sanitize_command_name("cc; rm -rf /"), "ccrmrf");
        assert_eq!(sanitize_command_name(""), "cc");
        assert_eq!(sanitize_command_name("   "), "cc");
    }

    #[test]
    fn find_conflict_in_user_function() {
        let content = r#"
function Get-Stuff { Write-Host x }
function cc { Write-Host my-cc }
"#;
        let conflicts = find_conflicting_functions(content, "cc");
        assert_eq!(conflicts, vec!["cc".to_string()]);
    }

    #[test]
    fn find_conflict_ignores_inside_ccm_block() {
        // 同名 function 在 ccm 块里 → 不算冲突（是我们自己写的）
        let content = r#"
function Get-Stuff { Write-Host x }
# === cc-monitor BEGIN v1 ===
function cc { __ccm_bind; & claude $args }
# === cc-monitor END ===
"#;
        let conflicts = find_conflicting_functions(content, "cc");
        assert!(conflicts.is_empty());
    }

    #[test]
    fn find_block_version_v1() {
        let content = "# === cc-monitor BEGIN v1 ===\nfunction cc {}\n# === cc-monitor END ===\n";
        let (present, ver) = find_block_version(content);
        assert!(present);
        assert_eq!(ver, Some("v1".to_string()));
    }

    #[test]
    fn replace_existing_block_keeps_other_content() {
        let existing = r#"# user stuff before
Set-Alias g git

# === cc-monitor BEGIN v1 ===
function cc { Write-Host old-version }
# === cc-monitor END ===

# user stuff after
$PSDefaultParameterValues = @{}
"#;
        let new_block =
            "# === cc-monitor BEGIN v1 ===\nfunction cc { Write-Host new-version }\n# === cc-monitor END ===";
        let out = replace_or_append_block(existing, new_block);
        assert!(out.contains("Set-Alias g git"));
        assert!(out.contains("$PSDefaultParameterValues = @{}"));
        assert!(out.contains("new-version"));
        assert!(!out.contains("old-version"));
    }

    #[test]
    fn append_to_empty_profile() {
        let new_block =
            "# === cc-monitor BEGIN v1 ===\nfunction cc { Write-Host hi }\n# === cc-monitor END ===";
        let out = replace_or_append_block("", new_block);
        assert!(out.starts_with("# === cc-monitor BEGIN"));
        assert!(out.ends_with("END ===\n"));
    }

    #[test]
    fn append_to_existing_profile_with_no_block() {
        let existing = "Set-Alias g git\n";
        let new_block = "# === cc-monitor BEGIN v1 ===\nfunction cc {}\n# === cc-monitor END ===";
        let out = replace_or_append_block(existing, new_block);
        assert!(out.starts_with("Set-Alias g git"));
        assert!(out.contains("BEGIN v1"));
    }

    #[test]
    fn strip_block_removes_only_block() {
        let existing = r#"Set-Alias g git
# === cc-monitor BEGIN v1 ===
function cc { Write-Host hi }
# === cc-monitor END ===
$PSDefaultParameterValues = @{}
"#;
        let out = strip_block(existing);
        assert!(out.contains("Set-Alias g git"));
        assert!(out.contains("$PSDefaultParameterValues"));
        assert!(!out.contains("BEGIN"));
        assert!(!out.contains("function cc"));
    }

    #[test]
    fn strip_block_no_op_when_no_block() {
        let content = "Set-Alias g git\n";
        assert_eq!(strip_block(content), content);
    }

    // === v1.7.10：install / uninstall end-to-end 落地保护测试 ===

    fn tmp_profile() -> PathBuf {
        let mut p = std::env::temp_dir();
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("ccm-test-{ms}-{}.ps1", std::process::id()));
        p
    }

    #[test]
    fn install_preserves_existing_user_content() {
        let p = tmp_profile();
        let user_content = "# my profile\nSet-Alias g git\nfunction prompt { 'PS> ' }\n";
        std::fs::write(&p, user_content).unwrap();

        install_to_profile(&p, "cc", false).unwrap();

        let after = std::fs::read_to_string(&p).unwrap();
        assert!(
            after.contains("Set-Alias g git"),
            "用户原有 alias 被冲掉了！after={after}"
        );
        assert!(after.contains("function prompt"));
        assert!(after.contains("# === cc-monitor BEGIN"));
        assert!(!after.is_empty());

        // 备份文件应该存在
        let parent = p.parent().unwrap();
        let stem = p.file_name().unwrap().to_string_lossy().to_string();
        let has_backup = std::fs::read_dir(parent)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{stem}.ccm-backup-"))
            });
        assert!(has_backup, "应该生成 .ccm-backup-<ts> 备份文件");

        // 清理
        let _ = std::fs::remove_file(&p);
        for entry in std::fs::read_dir(parent).unwrap().filter_map(|e| e.ok()) {
            let n = entry.file_name().to_string_lossy().to_string();
            if n.starts_with(&format!("{stem}.ccm-backup-")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    #[test]
    fn install_to_nonexistent_path_creates_file() {
        let p = tmp_profile();
        // 不预先创建
        assert!(!p.exists());
        install_to_profile(&p, "cc", true).unwrap();
        let content = std::fs::read_to_string(&p).unwrap();
        assert!(content.contains("function cc"));
        assert!(content.contains("__ccm_bind"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn reinstall_replaces_block_keeps_user_content() {
        let p = tmp_profile();
        std::fs::write(&p, "Set-Alias g git\n").unwrap();

        install_to_profile(&p, "cc", false).unwrap();
        // 第二次装：之前的块应该被原地替换，用户内容仍在
        install_to_profile(&p, "cc", true).unwrap();

        let after = std::fs::read_to_string(&p).unwrap();
        assert!(after.contains("Set-Alias g git"));
        assert!(after.contains("function cc"));
        // 只应该有一个 BEGIN 块
        assert_eq!(after.matches("# === cc-monitor BEGIN").count(), 1);

        let _ = std::fs::remove_file(&p);
        let parent = p.parent().unwrap();
        let stem = p.file_name().unwrap().to_string_lossy().to_string();
        for entry in std::fs::read_dir(parent).unwrap().filter_map(|e| e.ok()) {
            let n = entry.file_name().to_string_lossy().to_string();
            if n.starts_with(&format!("{stem}.ccm-backup-")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    /// v1.7.10：验证 install_to_profile 保留 dst 上的 explicit ACE。
    ///
    /// 复现 v1.7.9 zbl 事故场景：原 profile 上有 explicit ACE（用户自己加的或
    /// 系统给的），如果 atomic_replace 用 MoveFileExW / rename，explicit ACE
    /// 会被 tmp 文件的"继承父目录" ACL 覆盖丢失。用 ReplaceFileW 应该保留。
    ///
    /// 用 icacls 给文件加 `Everyone:(R)` explicit ACE，install 后跑 icacls
    /// 看这条 ACE 是否还在。
    #[cfg(windows)]
    #[test]
    fn install_preserves_explicit_acl_entries() {
        let p = tmp_profile();
        std::fs::write(&p, "Set-Alias g git\n").unwrap();

        // 给文件加 explicit Everyone:(R) ACE
        let add = std::process::Command::new("icacls")
            .arg(&p)
            .arg("/grant")
            .arg("Everyone:(R)")
            .output();
        let add = match add {
            Ok(o) if o.status.success() => o,
            _ => {
                // icacls 不可用（测试环境少见）—— 跳过
                let _ = std::fs::remove_file(&p);
                return;
            }
        };
        assert!(add.status.success(), "icacls /grant failed: {:?}", add);

        // 跑 install
        install_to_profile(&p, "cc", false).unwrap();

        // 看 explicit ACE 还在不在（icacls 输出里不带 (I) 标记的那条）
        let out = std::process::Command::new("icacls")
            .arg(&p)
            .output()
            .expect("icacls run");
        let stdout = String::from_utf8_lossy(&out.stdout);
        // 期望看到 Everyone:(R) 不带 (I) 前缀 —— 是 explicit ACE
        let has_explicit_everyone = stdout
            .lines()
            .any(|line| line.contains("Everyone:(R)") && !line.contains("(I)(R)"));
        assert!(
            has_explicit_everyone,
            "explicit Everyone:(R) ACE 应该被 ReplaceFileW 保留！icacls 输出:\n{stdout}"
        );

        let _ = std::fs::remove_file(&p);
        let parent = p.parent().unwrap();
        let stem = p.file_name().unwrap().to_string_lossy().to_string();
        for entry in std::fs::read_dir(parent).unwrap().filter_map(|e| e.ok()) {
            let n = entry.file_name().to_string_lossy().to_string();
            if n.starts_with(&format!("{stem}.ccm-backup-")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    #[test]
    fn uninstall_strips_block_keeps_user_content() {
        let p = tmp_profile();
        let user = "# mine\nSet-Alias g git\n";
        std::fs::write(&p, user).unwrap();
        install_to_profile(&p, "cc", true).unwrap();
        uninstall_from_profile(&p).unwrap();

        let after = std::fs::read_to_string(&p).unwrap();
        assert!(after.contains("Set-Alias g git"));
        assert!(!after.contains("# === cc-monitor"));
        assert!(!after.contains("__ccm_bind"));

        let _ = std::fs::remove_file(&p);
        let parent = p.parent().unwrap();
        let stem = p.file_name().unwrap().to_string_lossy().to_string();
        for entry in std::fs::read_dir(parent).unwrap().filter_map(|e| e.ok()) {
            let n = entry.file_name().to_string_lossy().to_string();
            if n.starts_with(&format!("{stem}.ccm-backup-")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}
