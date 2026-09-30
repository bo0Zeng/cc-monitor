//! 进程这一族的平台差异〔P4 · 阶段 H：`设计/90 §4` · `15 §5.3 C6` 余下〕：壳里平台 cfg 的唯一住址（同 [`super::fs`]）。

/// 本机常驻后端能不能**脱离**起（`local_backend_host.rs::spawn_detached`）：今天只在 Linux 上成立 ——
/// 别的平台走被监护那条（Windows 那一族没做，`99 §4.4`「本机常驻后端脱离不了」那一行）。
pub const CAN_DETACH: bool = cfg!(target_os = "linux");

/// 这个平台上可执行文件名的后缀（Windows `.exe`，别处空串）。〔P4 · P4b〕从 `filewin/proc.rs::window_bin_in` 与 `local_backend.rs::resolve_beside_this_exe` 收来：平台原语只住这一层。
pub const EXE_SUFFIX: &str = std::env::consts::EXE_SUFFIX;

/// 一次 `wait()` 的结局里**被哪个信号杀的**（unix 才有这一维；别处恒 `None`）。
pub fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    }
    #[cfg(not(unix))]
    {
        let _ = status;
        None
    }
}

/// 交给系统默认 opener 打开一个路径 / 目录：`(程序, argv)`。〔P4 · 阶段 H〕原住 `lib.rs::open_with_os`（选程序那一段，逐字）；起它照旧在调用方走唯一出口。
pub fn os_opener(path_or_dir: &str) -> (&'static str, Vec<String>) {
    #[cfg(windows)]
    // `cmd /C start ""` 兜 path 里的空格；第一个空串是 `start` 的窗口标题位。
    let pick = (
        "cmd",
        vec![
            "/C".to_string(),
            "start".to_string(),
            String::new(),
            path_or_dir.to_string(),
        ],
    );
    #[cfg(target_os = "macos")]
    let pick = ("open", vec![path_or_dir.to_string()]);
    #[cfg(all(unix, not(target_os = "macos")))]
    let pick = ("xdg-open", vec![path_or_dir.to_string()]);
    pick
}
