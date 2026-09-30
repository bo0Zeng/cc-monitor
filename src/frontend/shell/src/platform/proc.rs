//! 进程这一族的平台差异〔P4 · 阶段 H：`设计/90 §4` · `15 §5.3 C6` 余下〕：壳里平台 cfg 的唯一住址（同 [`super::fs`]）。

/// 本机常驻后端能不能**脱离**起（`local_backend_host.rs::spawn_detached`）：今天只在 Linux 上成立 ——
/// 别的平台走被监护那条（Windows 那一族没做，`99 §4.4`「本机常驻后端脱离不了」那一行）。
pub const CAN_DETACH: bool = cfg!(target_os = "linux");

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
