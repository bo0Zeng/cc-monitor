//! git 重命名跟随:旧路径 → 当前路径(顺 git 历史里的 rename 记录)。

use std::path::Path;
use std::process::Command;

/// 工作树里是否存在该文件(注:查的是文件系统,不是 git HEAD)。
pub fn file_exists(repo: &Path, rel: &str) -> bool {
    repo.join(rel).is_file()
}

/// 给定一个已不存在的旧路径,顺着 git 的 rename 链找它现在叫什么。找不到返回 None。
pub fn follow_rename(repo: &Path, orig: &str) -> Option<String> {
    let pairs = rename_pairs(repo);
    let mut cur = orig.to_string();
    let mut steps = 0;
    while let Some((_, new)) = pairs.iter().find(|(old, _)| *old == cur) {
        cur = new.clone();
        steps += 1;
        if steps > 64 {
            break; // 环保护
        }
    }
    if cur != orig && file_exists(repo, &cur) {
        Some(cur)
    } else {
        None
    }
}

fn rename_pairs(repo: &Path) -> Vec<(String, String)> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "log",
            "--all",
            "-M",
            "--diff-filter=R",
            "--name-status",
            "--format=",
        ])
        .output();
    let mut pairs = Vec::new();
    if let Ok(out) = out {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.starts_with('R') {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 3 {
                    pairs.push((parts[1].to_string(), parts[2].to_string()));
                }
            }
        }
    }
    pairs
}
