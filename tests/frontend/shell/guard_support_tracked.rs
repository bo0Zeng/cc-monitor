use std::path::Path;

/// git 跟踪着的、住在 `dir` 下（含子目录，按路径段比）且后缀是 `ext` 的文件：仓根相对、正斜杠、排好序。
/// 工作树里已删的不算。
///
/// 扫描型判据「采集面没塌」那一格的另一侧：与 `scan_tree!` 不同源（一个问 git 索引，一个走文件系统），
/// 所以「扫到的 ⊇ 这里列的」是一条两侧不同源的对拍，不靠写死的份数。
pub(crate) fn tracked_under(dir: &str, ext: &str) -> Vec<String> {
    let root = super::repo_root();
    let out = std::process::Command::new("git")
        .args(["ls-files", "-z", "--", dir])
        .current_dir(&root)
        .output()
        .unwrap_or_else(|e| panic!("起不来 `git ls-files`（{e}）—— 采集面对拍的另一侧拿不到"));
    assert!(
        out.status.success(),
        "`git ls-files -- {dir}` 非零退出：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut v: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|rel| {
            !rel.is_empty()
                && Path::new(rel).extension().is_some_and(|e| e == ext)
                && root.join(rel).is_file()
        })
        .map(str::to_string)
        .collect();
    v.sort();
    v
}

/// 正控：入口 `lib.rs` 在 git 跟踪着的人群里；扩展名过滤真的在过滤。
#[test]
fn tracked_under_lists_the_tracked_entry_and_filters_by_extension() {
    let rs = tracked_under("src/frontend/shell/src", "rs");
    assert!(
        rs.iter().any(|r| r == "src/frontend/shell/src/lib.rs"),
        "git 跟踪着的人群里没有 `lib.rs`：{rs:?}"
    );
    assert!(rs.iter().all(|r| r.ends_with(".rs")), "扩展名过滤失效");
}
