//! 扫仓库文件(IO 边界之一)。跳过 VCS / 构建产物 / 依赖目录。

use crate::model::Lang;
use std::fs;
use std::path::Path;

/// 不下钻的目录:VCS、Claude Code 工具目录、构建产物、第三方依赖(否则 JS/TS 仓 node_modules 会符号爆炸)。
/// F12 起统一(source_files 与 markdown 遍历共用)。
fn is_ignored_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            // Claude Code 工具目录:worktrees/(git 链接 worktree,索引=重复代码)、
            // planned-build/、commands/、agents/… 都非项目源码。
            | ".claude"
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

/// 该子目录是否为 git **链接 worktree / submodule**(`.git` 是**文件**、内容 `gitdir:` 指向别处)。
/// worktree 的代码是主工作树的**重复检出**、submodule 是外部依赖 → 索引它=重复/噪声符号,故遍历
/// 时不下钻。**主工作树 / 嵌套独立仓(`.git` 是目录)不受影响**,仍照旧索引。
/// 只对**下钻中遇到的子目录**生效;若用户显式把某 worktree 当 repo 根传入,仍会索引(那是显式意图)。
fn is_linked_worktree(dir: &Path) -> bool {
    dir.join(".git").is_file()
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
            if is_ignored_dir(&name) || is_linked_worktree(&path) {
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
            if is_ignored_dir(&name) || is_linked_worktree(&path) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cp-scan-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    /// scan 跳过 `.claude/`(含其下的 git worktree)+ 顶层链接 worktree(`.git` 文件),
    /// 但仍索引普通源码与嵌套独立仓(`.git` 目录)。
    #[test]
    fn skips_claude_and_worktrees_keeps_nested_repo() {
        let repo = tmp("wt");
        // 本仓源码 —— 应索引
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(repo.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
        // .claude/worktrees/audit-fixes/ —— 应跳过(整个 .claude 不下钻)
        fs::create_dir_all(repo.join(".claude/worktrees/audit-fixes/src")).unwrap();
        fs::write(
            repo.join(".claude/worktrees/audit-fixes/.git"),
            "gitdir: /x/.git/worktrees/audit-fixes\n",
        )
        .unwrap();
        fs::write(
            repo.join(".claude/worktrees/audit-fixes/src/dup.rs"),
            "pub fn dup() {}\n",
        )
        .unwrap();
        // 顶层链接 worktree(.git 是文件)—— 应跳过
        fs::create_dir_all(repo.join("wt/src")).unwrap();
        fs::write(repo.join("wt/.git"), "gitdir: /y/.git/worktrees/wt\n").unwrap();
        fs::write(repo.join("wt/src/wt.rs"), "pub fn wt() {}\n").unwrap();
        // 嵌套独立仓(.git 是目录)—— 仍索引(不改此行为)
        fs::create_dir_all(repo.join("nested/.git")).unwrap();
        fs::create_dir_all(repo.join("nested/src")).unwrap();
        fs::write(repo.join("nested/src/n.rs"), "pub fn n() {}\n").unwrap();

        let files: Vec<String> = source_files(&repo).into_iter().map(|(f, _)| f).collect();
        assert!(
            files.contains(&"src/lib.rs".to_string()),
            "本仓源码应索引: {files:?}"
        );
        assert!(
            files.contains(&"nested/src/n.rs".to_string()),
            "嵌套独立仓(.git 目录)应仍索引: {files:?}"
        );
        assert!(
            !files.iter().any(|f| f.contains(".claude")),
            ".claude 下应跳过: {files:?}"
        );
        assert!(
            !files.contains(&"wt/src/wt.rs".to_string()),
            "链接 worktree(.git 文件)应跳过: {files:?}"
        );
        let _ = fs::remove_dir_all(&repo);
    }
}
