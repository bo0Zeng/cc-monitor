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

/// 两种 PowerShell profile：5.1 (Windows PowerShell) 和 7.x (PowerShell Core)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProfileKind {
    /// Windows PowerShell 5.1: ~/Documents/WindowsPowerShell/profile.ps1
    Ps51,
    /// PowerShell 7.x:        ~/Documents/PowerShell/profile.ps1
    Ps7,
}

impl ProfileKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ps51 => "Windows PowerShell 5.1",
            Self::Ps7 => "PowerShell 7.x",
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

/// 解析两个 profile 路径。无 USERPROFILE 时返回空 Vec。
pub fn discover_profiles() -> Vec<(ProfileKind, PathBuf)> {
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

/// 生成将要写入的 cc function 代码（替换 placeholder）。
pub fn render_cc_code(command_name: &str) -> String {
    let safe_name = sanitize_command_name(command_name);
    CC_TEMPLATE.replace("{{COMMAND_NAME}}", &safe_name)
}

/// idempotent 安装：把 cc function 块写到 profile，已有 ccm 块则原地替换。
/// 用户在 BEGIN/END 块外的内容完全不动。
pub fn install_to_profile(path: &PathBuf, command_name: &str) -> Result<(), String> {
    let code = render_cc_code(command_name);
    let existing = if path.exists() {
        std::fs::read_to_string(path).map_err(|e| format!("read existing profile failed: {e}"))?
    } else {
        // 确保父目录存在
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create profile dir failed: {e}"))?;
        }
        String::new()
    };

    let updated = replace_or_append_block(&existing, &code);
    atomic_write_string(path, &updated).map_err(|e| format!("write profile failed: {e}"))?;
    Ok(())
}

/// 卸载：整块删除 BEGIN/END 之间的内容（含 marker 行）。块外内容不动。
pub fn uninstall_from_profile(path: &PathBuf) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let existing =
        std::fs::read_to_string(path).map_err(|e| format!("read existing profile failed: {e}"))?;
    let stripped = strip_block(&existing);
    if stripped == existing {
        return Ok(()); // 没有块，无需写
    }
    atomic_write_string(path, &stripped).map_err(|e| format!("write profile failed: {e}"))?;
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

fn atomic_write_string(path: &PathBuf, content: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("ps1.tmp");
    std::fs::write(&tmp, content)?;
    let _ = std::fs::remove_file(path);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_cc_code_substitutes_placeholder() {
        let out = render_cc_code("ccm");
        assert!(out.contains("function ccm"));
        assert!(!out.contains("{{COMMAND_NAME}}"));
        assert!(out.contains("BEGIN v1"));
        assert!(out.contains("cc-monitor END"));
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
}
