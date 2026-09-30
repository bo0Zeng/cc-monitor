//! 文件窗口包的平台层〔P4 · 阶段 H：`设计/90 §4`；主会话 09-30 认：窗口包链不到壳的 `platform/`，自开这一份〕：本包平台 cfg 的唯一住址。
//! 都原住别处、逐字搬来：[`any_thread_hook`]（`shell.rs`）· [`local_dest_kept`] · [`path_from_bytes`]（`lossy_pull.rs`）·
//! [`SYSTEM_CJK_FONTS`]（`fonts.rs`）· [`rss_kib`]（`scale.rs`）· [`BACKSLASH_IS_SEP`]（`download.rs` · `lossy_pull.rs` 各一处 `cfg!(windows)`）。

/// 🔴 让 winit 的事件循环能建在**非主线程**上。
///
/// 三个平台各一句；macOS 上**没有**对应的扩展 trait（NSApplication 铁定要主线程），
/// 而本仓承诺的平台是 Windows / Linux ⇒ 今天碰不到那一格。
pub fn any_thread_hook(builder: &mut eframe::EventLoopBuilder<eframe::UserEvent>) {
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
    {
        use winit::platform::wayland::EventLoopBuilderExtWayland;
        use winit::platform::x11::EventLoopBuilderExtX11;
        EventLoopBuilderExtX11::with_any_thread(builder, true);
        EventLoopBuilderExtWayland::with_any_thread(builder, true);
    }
    #[cfg(windows)]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        EventLoopBuilderExtWindows::with_any_thread(builder, true);
        // 🔴 〔WN1 · 09-24〕**进程 DPI 归这个窗口进程自己管** —— 这个进程里没有 Tauri。
        // 从前是 `false`（「进程 DPI 归 Tauri 管」，同进程时代）；窗口进程独立之后那一格
        // 让它全程 `UNAWARE`。四格读数与改回来的理由住本模块头注「WN1」那一节。
        EventLoopBuilderExtWindows::with_dpi_aware(builder, true);
    }
    #[cfg(target_os = "macos")]
    {
        // 够不着：macOS 没有 with_any_thread。留个明确的编译期落点，
        // 免得哪天上了 macOS 还以为这条路是通的。
        let _ = builder;
    }
}

#[cfg(unix)]
pub(crate) fn local_dest_kept(
    p: &std::path::Path,
    _shown: &str,
    raw: &[u8],
) -> (serde_json::Value, Option<String>) {
    use std::os::unix::ffi::OsStrExt as _;
    let mut b = p
        .parent()
        .map(|d| d.as_os_str().as_bytes().to_vec())
        .unwrap_or_default();
    if b.last() != Some(&b'/') {
        b.push(b'/');
    }
    b.extend_from_slice(raw);
    (crate::source::wire_bytes(&b), None)
}

#[cfg(not(unix))]
pub(crate) fn local_dest_kept(
    p: &std::path::Path,
    shown: &str,
    _raw: &[u8],
) -> (serde_json::Value, Option<String>) {
    (
        serde_json::Value::String(p.to_string_lossy().to_string()),
        Some(copy_core::copy_text(
            "rsFilewinLossyPull.note.renamed",
            &[("name", shown)],
        )),
    )
}

/// 本机路径的分隔符里有没有 `\`（Windows 有，别处只有 `/`）。〔P4 · 阶段 H〕原是 `download.rs::plan_dest` · `lossy_pull.rs::split_local` 里各一处 `cfg!(windows)`。
pub const BACKSLASH_IS_SEP: bool = cfg!(windows);

/// 线上那一形解出来的字节 → 本机路径：unix 按字节原样；别处按 UTF-8 有损转。〔P4 · 阶段 H〕原住 `lossy_pull.rs::local_path_of`。
pub fn path_from_bytes(b: &[u8]) -> std::path::PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        std::path::PathBuf::from(std::ffi::OsStr::from_bytes(b))
    }
    #[cfg(not(unix))]
    {
        std::path::PathBuf::from(String::from_utf8_lossy(b).to_string())
    }
}

/// 系统自带的 CJK 字体候选，按优先级（`fonts::candidates` 把它接在 `CCM_CJK_FONT` 之后）。〔P4 · 阶段 H〕原住 `fonts.rs::candidates`，逐字。
#[cfg(target_os = "windows")]
pub const SYSTEM_CJK_FONTS: &[&str] = &[
    // 微软雅黑 —— Vista 起随系统装，**不分区域设置**
    r"C:\Windows\Fonts\msyh.ttc",
    r"C:\Windows\Fonts\msyh.ttf",   // Win7 及更早是 .ttf
    r"C:\Windows\Fonts\msjh.ttc",   // 微软正黑（繁体系统）
    r"C:\Windows\Fonts\simsun.ttc", // 宋体，兜底
];
/// 同上（非 Windows）。
#[cfg(not(target_os = "windows"))]
pub const SYSTEM_CJK_FONTS: &[&str] = &[
    // ↓ 本机（Debian/Ubuntu 系，fonts-noto-cjk）现打就是这一份
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", // Arch
    "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", // Fedora
    "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", // 文泉驿，小
    "/usr/share/fonts/truetype/arphic/uming.ttc",
];

/// 读进程 RSS。Linux 走 `/proc/self/statm`；别的平台返回 0（**不是假装 0 字节**，
/// 是「这台机器上量不到」—— 判据那边按 0 跳过，并且把「跳过了」印出来）。
pub fn rss_kib() -> u64 {
    #[cfg(target_os = "linux")]
    {
        let s = match std::fs::read_to_string("/proc/self/statm") {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let pages: u64 = s
            .split_whitespace()
            .nth(1)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        pages.saturating_mul(4) // 4 KiB/page
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}
