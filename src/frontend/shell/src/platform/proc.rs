//! 进程这一族的平台差异〔余下〕：壳里平台 cfg 的唯一住址（同 [`super::fs`]）。

/// 本机常驻后端能不能**脱离**起（`local_backend_host.rs::spawn_detached`）：今天只在 Linux 上成立 ——
/// 别的平台走被监护那条（Windows 那一族没做，「本机常驻后端脱离不了」那一行）。
pub const CAN_DETACH: bool = cfg!(target_os = "linux");

/// 这个平台上可执行文件名的后缀（Windows `.exe`，别处空串）。从 `filewin/proc.rs::window_bin_in` 与 `local_backend.rs::resolve_beside_this_exe` 收来：平台原语只住这一层。
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

/// 交给系统默认 opener 打开一个路径 / 目录：`(程序, argv)`。原住 `lib.rs::open_with_os`（选程序那一段，逐字）；起它照旧在调用方走唯一出口。
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

/// 「monitor 起来了」的事件句柄（复位时用）。
#[cfg(windows)]
static UP_EVENT: std::sync::OnceLock<isize> = std::sync::OnceLock::new();

/// 挂上两样有名字的系统对象（同一个登录会话里可见），让别的进程不轮询就等得到「monitor 起来了 / 走了」：
/// 先在一个专门的线程上占住互斥量 `alive`（这个线程一直不退 ⇒ 本进程活多久就占多久；进程一没，系统替它放手），
/// 再把手动复位的事件 `up` 置位。顺序承重：先占住再置位，等的一方据此认得出「置位了但没人占着」是上一个 monitor 留下的。
/// 只有 Windows 有；别处没有谁来等，什么都不做。
#[cfg(windows)]
pub fn hold_monitor_marks(alive: &str, up: &str) {
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let (alive, up) = (wide(alive), wide(up));
    let spawned = std::thread::Builder::new()
        .name("monitor-up-mark".into())
        .spawn(move || {
            use windows::core::PCWSTR;
            use windows::Win32::System::Threading::{
                CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject, INFINITE,
            };
            unsafe {
                let Ok(m) = CreateMutexW(None, false, PCWSTR(alive.as_ptr())) else {
                    tracing::warn!("monitor-up-mark: create mutex failed");
                    return;
                };
                // 上一个 monitor 没正常退出时拿到的是「被遗弃」那一态，同样算占住；只有等失败才不算。
                if WaitForSingleObject(m, INFINITE).0 == u32::MAX {
                    tracing::warn!("monitor-up-mark: wait mutex failed");
                    return;
                }
                let Ok(ev) = CreateEventW(None, true, false, PCWSTR(up.as_ptr())) else {
                    tracing::warn!("monitor-up-mark: create event failed");
                    return;
                };
                UP_EVENT.get_or_init(|| ev.0);
                if let Err(e) = SetEvent(ev) {
                    tracing::warn!("monitor-up-mark: set event failed: {e}");
                }
            }
            loop {
                std::thread::park();
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("monitor-up-mark: spawn failed: {e}");
    }
}

#[cfg(not(windows))]
pub fn hold_monitor_marks(_alive: &str, _up: &str) {}

/// 正常退出时把「monitor 起来了」复位（崩了就复位不了 —— 那一态由等的一方看互斥量认出来）。
#[cfg(windows)]
pub fn lower_monitor_up_mark() {
    if let Some(h) = UP_EVENT.get() {
        let done = unsafe {
            windows::Win32::System::Threading::ResetEvent(windows::Win32::Foundation::HANDLE(*h))
        };
        if let Err(e) = done {
            tracing::warn!("monitor-up-mark: reset event failed: {e}");
        }
    }
}

#[cfg(not(windows))]
pub fn lower_monitor_up_mark() {}
