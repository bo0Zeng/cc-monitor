//! U2（2026-08-01）：**路径的平台语义**。
//!
//! 与 [`super::proc`] 分开成两个文件，不是为了整齐：`path_key` 处理的是
//! **NTFS 的大小写不敏感**，既不是 `/proc` 也不是进程身份，塞进那个自称
//! 「`/proc` 与进程身份这一族」的模块名不副实。
//!
//! 更实际的理由是 **U4**：给 Windows 补第二套实现时，路径语义与进程语义会各自长出一批分支，
//! 现在分开是零成本，到时候再搬就是第二次搬同一段代码 —— 那正是账本要防的「补丁叠补丁」。
//! （功能计划步骤 1 本来就写了要建本文件，实现时漏了，Phase D 审计逮出来补上。）

use std::path::{Path, PathBuf};

/// Case-fold the path on Windows so notify's NTFS case variance does not double
/// emit; on other platforms keep the path verbatim.
///
/// 从前这里写「与 monitor 侧那份 jsonl 读者的同名两分支同规则」—— 那份读者 CF1 删了，
/// 今天这两个分支只有这一份（调用方是本 crate 的 `observe/watcher.rs`）。
#[cfg(windows)]
pub(crate) fn path_key(p: &Path) -> PathBuf {
    PathBuf::from(p.to_string_lossy().to_ascii_lowercase())
}

#[cfg(not(windows))]
pub(crate) fn path_key(p: &Path) -> PathBuf {
    p.to_path_buf()
}

/// `K-R55`（2026-09-11）：**本平台放「每用户临时文件」的那个根目录**。
///
/// # 它从哪来
///
/// `K-R52` 的 A2 堆里挂着 `observe/tmux_observe.rs::tmux_socket_dir` 那句
/// `PathBuf::from("/tmp")`，签字栏逐字：「同一行上方的 `uid` 那半**已经**有两条
/// `#[cfg]` 臂了，而这一半没有 —— 典型的『只修一半』。该和 `platform/paths.rs` 住一起。」
/// 这就是它搬过来之后的样子，连同它那另一半（[`current_uid`]）。
///
/// ⚠ **搬的是平台原语，不是那条组合**：`tmux_socket_dir` 自己（读 `TMUX_TMPDIR`、
/// 拼 `tmux-<uid>`）是 **tmux 的约定**，不是平台语义 ⇒ 它留在 `observe/`。
/// 分界线就是 `K33` 裁定二那一句：住进适配层的是「这台机器上这件事怎么做」，
/// 不是「这件事是什么」。
///
/// ⚠ 非 unix 那一臂给的是 `std::env::temp_dir()`，**不是**一个编出来的答案：
/// 那是标准库对同一个概念（本平台的临时目录）在那个平台上的取值口。
/// 而 tmux 在那里本来就不存在 ⇒ 这条路走不到，给它一个真实的根目录只为**别撒谎**。
#[cfg(unix)]
pub(crate) fn temp_root() -> PathBuf {
    PathBuf::from("/tmp")
}

#[cfg(not(unix))]
pub(crate) fn temp_root() -> PathBuf {
    std::env::temp_dir()
}

/// 跑着的这个进程的 uid。**非 unix 上没有这个概念 ⇒ 给 `0`**。
///
/// ⚠⚠ 这一半先前就住在 `observe/watcher.rs` 里、**并且已经有两条 cfg 臂**
/// （那份注释还记着「首版直接 `unsafe { libc::getuid() }`，本机 cargo test 全绿、
/// Windows 侧编不过」）。搬过来不是因为它坏，是因为**它和它的另一半被拆在两处**：
/// 一半有门、一半没门，而两半合起来才是一条路径。
#[cfg(unix)]
pub(crate) fn current_uid() -> u32 {
    // SAFETY: `getuid` 无副作用、不会失败。
    unsafe { libc::getuid() }
}

#[cfg(not(unix))]
pub(crate) fn current_uid() -> u32 {
    0
}

/// 〔「不跨文件系统边界那一档没做（设备号要走 `platform/`）」〕
/// 一个路径（**跟链接**）所在文件系统的设备号 —— 走一棵树时「这一层是不是挂着另一个文件系统」那一判用它
/// （`files::index::build` · `files::size`）。调用方只把它用在「不跟链接地看过、确是目录」的条目上，跟不跟链接在那里没有差别。
///
/// 非 unix ⇒ `None`：那一判在那个平台上**不开口**（全当同一个文件系统），如实登记，不编一个数。
#[cfg(unix)]
pub(crate) fn device_of(p: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt as _;
    std::fs::metadata(p).ok().map(|m| m.dev())
}

#[cfg(not(unix))]
pub(crate) fn device_of(_p: &Path) -> Option<u64> {
    None
}

/// 一个路径（**跟链接**，与 `files-stat` 其余几格同源）的属主：用户名；查不到名字 ⇒ uid 的数字串。
/// 非 unix ⇒ `None`（那边的属主是另一套东西，不编一个）。
#[cfg(unix)]
pub(crate) fn owner_of(p: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt as _;
    let uid = std::fs::metadata(p).ok()?.uid();
    Some(user_name(uid).unwrap_or_else(|| uid.to_string()))
}

#[cfg(not(unix))]
pub(crate) fn owner_of(_p: &Path) -> Option<String> {
    None
}

/// uid → 用户名（`getpwuid_r`，可重入；查不到 / 名字不是 UTF-8 ⇒ `None`）。
#[cfg(unix)]
fn user_name(uid: u32) -> Option<String> {
    let mut buf = vec![0 as libc::c_char; 4096];
    // SAFETY: `pwd` 由 libc 填；`buf` 活过本函数、长度如实交出；`out` 只在回 0 且非空时读。
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut out: *mut libc::passwd = std::ptr::null_mut();
    let rc = unsafe { libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), buf.len(), &mut out) };
    if rc != 0 || out.is_null() || pwd.pw_name.is_null() {
        return None;
    }
    // SAFETY: 上面判过非空；`pw_name` 指进 `buf`，以 NUL 收尾。
    let name = unsafe { std::ffi::CStr::from_ptr(pwd.pw_name) };
    name.to_str().ok().map(str::to_string)
}

/// 路径**本身**是符号链接 ⇒ 它的目标原文（`readlink`，不解、不跟）；不是链接 / 读不到 ⇒ `None`。
pub(crate) fn link_target_of(p: &Path) -> Option<std::path::PathBuf> {
    std::fs::read_link(p).ok()
}

/// 这台机器的**「文档」目录**（Windows 上 OneDrive 会把它挪走 ⇒ 问系统 `SHGetKnownFolderPath`）。
/// 从前是 monitor 进程问（`dirs::document_dir`）；别名方言进了后端之后，`$PROFILE` 在哪由**那台后端**问它自己的系统。
/// 非 Windows ⇒ `None`（那里没有 `$PROFILE` 要找，调用方退回 `home/Documents`，不编一个答案）。
#[cfg(windows)]
pub(crate) fn documents_dir() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt as _;
    #[repr(C)]
    struct Guid {
        d1: u32,
        d2: u16,
        d3: u16,
        d4: [u8; 8],
    }
    /// `FOLDERID_Documents` = `{FDD39AD0-238F-46AF-ADB4-6C85480369C7}`。
    const FOLDERID_DOCUMENTS: Guid = Guid {
        d1: 0xFDD3_9AD0,
        d2: 0x238F,
        d3: 0x46AF,
        d4: [0xAD, 0xB4, 0x6C, 0x85, 0x48, 0x03, 0x69, 0xC7],
    };
    #[link(name = "shell32")]
    extern "system" {
        fn SHGetKnownFolderPath(
            rfid: *const Guid,
            flags: u32,
            token: *mut core::ffi::c_void,
            path: *mut *mut u16,
        ) -> i32;
    }
    #[link(name = "ole32")]
    extern "system" {
        fn CoTaskMemFree(pv: *mut core::ffi::c_void);
    }
    let mut p: *mut u16 = std::ptr::null_mut();
    // SAFETY: 出参是一个指针槽；成功时系统分配、我们按约定用 `CoTaskMemFree` 还回去（失败时也可能分配，同样还）。
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_DOCUMENTS, 0, std::ptr::null_mut(), &mut p) };
    let out = if hr >= 0 && !p.is_null() {
        // SAFETY: 成功时 `p` 指向一个以 0 结尾的 UTF-16 串。
        let len = (0..).take_while(|&i| unsafe { *p.add(i) } != 0).count();
        let wide = unsafe { std::slice::from_raw_parts(p, len) };
        Some(PathBuf::from(std::ffi::OsString::from_wide(wide)))
    } else {
        None
    };
    if !p.is_null() {
        // SAFETY: `p` 由 `SHGetKnownFolderPath` 分配。
        unsafe { CoTaskMemFree(p.cast()) };
    }
    out
}

#[cfg(not(windows))]
pub(crate) fn documents_dir() -> Option<PathBuf> {
    None
}

/// 这台的文件系统**有没有可执行位**（unix 有；Windows 没有 —— 那边写口的改权限如实回失败，调用方据此不发）。
pub(crate) fn has_exec_bits() -> bool {
    cfg!(unix)
}

/// 〔原 `assets/door.rs`〕`rel`（`/` 或 `\` 分隔）接在 `home` 底下：分隔符跟 `home` 自己的写法走
/// （`home` 里只有 `\` ⇒ `\`，否则 `/`）—— 那台机器的 home 是什么写法由它自己的后端答，这里只照着拼。
pub(crate) fn join_under(home: &str, rel: &str) -> String {
    let sep = if home.contains('\\') && !home.contains('/') {
        '\\'
    } else {
        '/'
    };
    let mut out = home.trim_end_matches(['/', '\\']).to_string();
    for seg in rel.split(['/', '\\']).filter(|s| !s.is_empty()) {
        out.push(sep);
        out.push_str(seg);
    }
    out
}

/// **这台后端的家目录只在这里答** ——规矩（哪个环境变量算家、按平台的先后）是两侧的契约，
/// 住 `creds_core::store::home_dir_from`（monitor 同调它）；都没有 ⇒ `None`（调用方明说）。判据 `shell_home_guard.rs::the_home_directory_is_read_in_one_place`。
pub(crate) fn home_dir() -> Option<PathBuf> {
    creds_core::store::home_dir()
}

/// 这台后端自己在盘上的入口 `<家>/.cc-monitor/bin/ccm[.exe]`（后端落点，本机远端同一处）。起会话那一行直接叫它：
/// 终端里交互 shell 的 `PATH` 上未必有 `ccm`（没装接入块的远端就没有）。家目录说不出 ⇒ `None`。
pub(crate) fn installed_ccm_entry() -> Option<String> {
    let rel = format!(
        "{}{}",
        relay_route_core::BACKEND_LANDING_REL,
        std::env::consts::EXE_SUFFIX
    );
    let mut p = home_dir()?;
    for seg in rel.split('/') {
        p.push(seg);
    }
    Some(p.to_string_lossy().into_owned())
}

/// 这台后端的数据目录（`CCM_DATA_DIR` 优先，否则 `<家>/.cc-monitor`；规矩住 `creds_core::store::monitor_data_dir`）。
pub(crate) fn data_home() -> Option<PathBuf> {
    creds_core::store::monitor_data_dir(
        std::env::var(creds_core::store::DATA_DIR_ENV)
            .ok()
            .as_deref(),
        home_dir(),
    )
}

/// 同上，环境由 `get` 答（判据喂夹具、不改进程环境的那几个调用方用）。
pub(crate) fn home_dir_from(get: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    creds_core::store::home_dir_from(get)
}
