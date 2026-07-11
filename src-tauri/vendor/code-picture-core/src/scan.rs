//! 扫仓库文件(IO 边界之一)。跳过 VCS / 构建产物 / 依赖目录。

use crate::model::Lang;
use std::fs;
use std::path::Path;

/// 不下钻的目录:VCS、构建产物、第三方依赖(否则 JS/TS 仓 node_modules 会符号爆炸)。
/// F12 起统一(source_files 与 markdown 遍历共用)。
fn is_ignored_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "target"
            | ".codepicture"
            | "node_modules"
            | "vendor"
            | "dist"
            | "build"
            | "__pycache__"
            | ".venv"
            | "venv"
    )
}

/// F11:扫全部**受支持语言**的源文件(按扩展名判定语言)。返回 (仓库相对路径, 语言)。
/// 非源码 / 未支持扩展名的文件被忽略。取代原 `rust_files`,让索引管线多语言就绪。
pub fn source_files(repo: &Path) -> Vec<(String, Lang)> {
    let mut out = Vec::new();
    walk_source(repo, repo, &mut out);
    out
}

/// F05:扫 `.md`(账本已定的 scan.rs 演进,供 doc-links 复用)。
pub fn markdown_files(repo: &Path) -> Vec<String> {
    files_with_ext(repo, "md")
}

fn walk_source(root: &Path, dir: &Path, out: &mut Vec<(String, Lang)>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if is_ignored_dir(&name) {
                continue;
            }
            walk_source(root, &path, out);
        } else if let Ok(rel) = path.strip_prefix(root) {
            let rel = rel.to_string_lossy().replace('\\', "/");
            if let Some(lang) = Lang::from_path(&rel) {
                out.push((rel, lang));
            }
        }
    }
}

fn files_with_ext(repo: &Path, ext: &str) -> Vec<String> {
    let mut out = Vec::new();
    walk(repo, repo, ext, &mut out);
    out
}

fn walk(root: &Path, dir: &Path, ext: &str, out: &mut Vec<String>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if is_ignored_dir(&name) {
                continue;
            }
            walk(root, &path, ext, out);
        } else if path.extension().map(|e| e == ext).unwrap_or(false) {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}
