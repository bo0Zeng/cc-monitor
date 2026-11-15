// 开发调度各份测试共用的小件：列一层目录 · 写可执行夹具都只这一处。各份用 include! 引进来。

/// 列出 `dir` 这一层的条目（不往下走；目录不在 ⇒ 空）。只看自己沙箱里的一层，不是扫描型判据。
#[allow(dead_code)] // 调度端单测 · thread_cap · thread_cap_drop 不用
pub(crate) fn entries_of(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default()
}

/// 写一份可执行夹具（假 sccache · 假 srun · 假 ssh …），写完当场 exec 得了。
/// 本进程里直接 `fs::write` 的那份，写端开着时同一进程里别的测试正 fork 的子进程（exec 之前）也攥着它，
/// 紧接着 exec ⇒ ETXTBSY。改成先写到旁边一份、由一个子进程 `cp` 到目标：目标的写端只在那个已退出的子进程里开过。
#[allow(dead_code)] // e2e · thread_cap · thread_cap_drop 不用
pub(crate) fn write_exe(path: &std::path::Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    let mut side = path.as_os_str().to_owned();
    side.push(".src");
    let side = std::path::PathBuf::from(side);
    std::fs::write(&side, body).unwrap();
    let st = std::process::Command::new("cp")
        .arg(&side)
        .arg(path)
        .status()
        .unwrap();
    assert!(st.success(), "cp {} 没成", path.display());
    std::fs::remove_file(&side).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}
