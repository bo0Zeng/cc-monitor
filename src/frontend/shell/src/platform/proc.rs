//! 进程这一族的平台差异〔余下〕：壳里平台 cfg 的唯一住址（同 [`super::fs`]）。

/// 本机常驻后端能不能**脱离**起（`local_backend_host.rs::spawn_detached`）：Linux（套接字）与 Windows（命名管道，`own-chan`）上成立 ——
/// 别的平台走被监护那条。
pub const CAN_DETACH: bool = cfg!(any(target_os = "linux", windows));

/// 这个平台上可执行文件名的后缀（Windows `.exe`，别处空串）。`filewin/proc.rs::window_bin_name` 用它：平台原语只住这一层。
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

/// 「用系统程序打开 / 在文件夹中显示」按哪个系统挑程序。挑法是按它分的纯函数，判据在哪台上都量得到三个系统的挑法。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Windows,
    Mac,
    /// 别的 unix（Linux · BSD）：freedesktop 那一套。
    Linux,
}

impl Os {
    /// 这台编出来的系统。
    pub const HERE: Os = if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::Mac
    } else {
        Os::Linux
    };
}

/// 交给系统默认程序打开一条路径 / 一个网址：`(程序, argv)`。只选程序；起它在调用方（`lib.rs::os_open`）走唯一出口。
pub fn os_opener(target: &str) -> (&'static str, Vec<String>) {
    opener_on(Os::HERE, target)
}

/// [`os_opener`] 按系统那一挑。目标原样、单独一个实参，中间不经 shell：Windows 上 `cmd /C start` 会把 `&` 当命令分隔
/// （登录网址被截断，`…&calc` 还会多起一个程序），改由 `explorer` 交给关联程序（文件 · 目录 · 网址同一口）。
pub fn opener_on(os: Os, target: &str) -> (&'static str, Vec<String>) {
    let bin = match os {
        Os::Windows => "explorer",
        Os::Mac => "open",
        Os::Linux => "xdg-open",
    };
    (bin, vec![target.to_string()])
}

/// [`os_opener`] 收不收这个网址：只认 `http:` · `https:` · `mailto:`（不分大小写）。别的 scheme（`file:` · 自定义协议）
/// 交给系统就是起它登记的那个程序，界面里的外链（`md-view.ts` 只放这三种）· 登录页 · 说明链接都用不着。
pub fn openable_url(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|s| lower.starts_with(s))
}

/// 在系统文件管理器里显示一条路径（选中它）：`(程序, argv)`。Linux 这一挑是退路（[`reveal_on_bus`] 答不上时）：开它所在的目录。
pub fn os_revealer(path: &str) -> (&'static str, Vec<String>) {
    revealer_on(Os::HERE, path)
}

/// [`os_revealer`] 按系统那一挑。
pub fn revealer_on(os: Os, path: &str) -> (&'static str, Vec<String>) {
    match os {
        Os::Windows => ("explorer", vec![format!("/select,{path}")]),
        Os::Mac => ("open", vec!["-R".to_string(), path.to_string()]),
        Os::Linux => {
            let dir = std::path::Path::new(path)
                .parent()
                .map_or_else(|| path.to_string(), |d| d.to_string_lossy().into_owned());
            ("xdg-open", vec![dir])
        }
    }
}

/// Linux 上「在文件夹中显示」先问会话总线上的文件管理器（`org.freedesktop.FileManager1.ShowItems`，它会选中那一项）。
/// 答上了 ⇒ `true`；没有总线 · 没有文件管理器在听 · 别的系统 ⇒ `false`，调用方走 [`os_revealer`] 那一挑。
pub fn reveal_on_bus(path: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        let Ok(bus) = zbus::blocking::Connection::session() else {
            return false;
        };
        bus.call_method(
            Some("org.freedesktop.FileManager1"),
            "/org/freedesktop/FileManager1",
            Some("org.freedesktop.FileManager1"),
            "ShowItems",
            &(vec![file_uri(path)], ""),
        )
        .is_ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        false
    }
}

/// 一条本机绝对路径的 `file://` 网址：非 unreserved 的字节转义成 `%XX`，`/` 留着。
pub fn file_uri(path: &str) -> String {
    let mut out = String::from("file://");
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/platform/proc_tests.rs"]
mod tests;
