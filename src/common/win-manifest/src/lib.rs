//! 两个前端包（monitor · 文件窗口）在 Windows 上带的进程清单：只有这一份清单（`app.manifest`）、只有这一处编法。
//!
//! 为什么每个包都要、连测试程序也要：系统目录里那份 `comctl32.dll` 是 v5，`TaskDialogIndirect` · `SetWindowSubclass`
//! 一族按名字只有通用控件 v6 导出。monitor 经 Tauri 的 Windows 运行时（tao · wry · 对话框）、文件窗口经 rfd 的
//! `common-controls-v6` 按名字引入它们 ⇒ 程序没带「依赖 Common-Controls 6.0.0.0」的清单，系统装不上 v6，
//! 进程在第一行代码之前就死（`0xc0000139` STATUS_ENTRYPOINT_NOT_FOUND）。先死过的是 monitor 的 lib 测试程序
//! （清单当时只进 `[[bin]]`），后来文件窗口的 lib 测试程序照样（它的包当时没有构建脚本，一份清单都不带；真机复现过）。
//!
//! 两半：[`embed_for_every_artifact`] 给构建脚本调（`[build-dependencies]`）；[`assert_own_process_carries_it`] 给测试程序在
//! Windows 上读回自己那一份（`[dev-dependencies]`）。门禁 `winlink` 在 Linux 上从两份测试程序的资源表里看同一件事。

use std::path::{Path, PathBuf};

/// 清单的住址（本 crate 目录里那一份）。
fn manifest_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("app.manifest")
}

/// 在构建脚本里调：把清单编成资源、交给**调用方那个包的每一个**链接产物（程序 · 测试程序 · 例子）。
///
/// 不是 Windows target 就只登记 `rerun-if-changed`、什么都不编。编不成就说一声、退出（构建脚本的错误模型）。
/// 只往 `OUT_DIR` 写一行 `.rc`，不碰别处。
pub fn embed_for_every_artifact() {
    let manifest = manifest_path();
    println!("cargo:rerun-if-changed={}", manifest.display());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let rc = out.join("app-manifest.rc");
    std::fs::write(&rc, rc_line(&manifest.display().to_string())).expect("写不出清单的 .rc");
    if let Err(e) =
        embed_resource::compile_for_everything(&rc, embed_resource::NONE).manifest_required()
    {
        println!("cargo:warning=清单没编成资源：{e}");
        std::process::exit(1);
    }
}

/// 在构建脚本里调：把程序图标编成资源、交给**调用方那个包的程序**（`[[bin]]`；测试程序不要）。
///
/// 给自己不经 `tauri-build` 的那个前端程序用（文件窗口：egui 窗口没设图标时，Windows 的任务栏与 Alt-Tab 取可执行文件的第一个图标资源）。
/// 与清单那份 `.rc` 分开编：清单要进每一个链接产物，图标只进程序。不是 Windows target 就只登记 `rerun-if-changed`。
pub fn embed_icon_for_bins(icon: &Path) {
    println!("cargo:rerun-if-changed={}", icon.display());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let rc = out.join("app-icon.rc");
    std::fs::write(&rc, icon_rc_line(&icon.display().to_string())).expect("写不出图标的 .rc");
    if let Err(e) = embed_resource::compile(&rc, embed_resource::NONE).manifest_optional() {
        println!("cargo:warning=图标没编成资源：{e}");
        std::process::exit(1);
    }
}

/// 图标那一行：`1 ICON` ＝ 第一个图标组。反斜杠成对，同 [`rc_line`]。
fn icon_rc_line(icon: &str) -> String {
    format!("1 ICON \"{}\"\n", icon.replace('\\', "\\\\"))
}

/// `.rc` 里那一行：`1` ＝ CREATEPROCESS_MANIFEST_RESOURCE_ID，`24` ＝ RT_MANIFEST。rc 的字符串里反斜杠要成对。
fn rc_line(manifest: &str) -> String {
    format!("1 24 \"{}\"\n", manifest.replace('\\', "\\\\"))
}

/// 一份进程清单点没点名「通用控件 v6」那一条依赖。
#[cfg(any(windows, test))]
fn names_common_controls_v6(manifest: &str) -> bool {
    manifest.contains("Microsoft.Windows.Common-Controls") && manifest.contains("6.0.0.0")
}

/// 测试程序在 Windows 上调：本进程自己内嵌的那份清单在、且点名通用控件 v6；不然就地 panic（判据的错误模型），`who` 说是哪份测试程序。
#[cfg(windows)]
pub fn assert_own_process_carries_it(who: &str) {
    let m = own_manifest().unwrap_or_else(|| {
        panic!(
            "{who}：没有内嵌进程清单 —— 它的包的 build.rs 没调 `win_manifest::embed_for_every_artifact`，\
             一碰按名字引入的 comctl32 v6 那一族就起不来（0xc0000139）"
        )
    });
    assert!(
        names_common_controls_v6(&m),
        "{who}：清单里没有「通用控件 v6」那一条依赖：{m}"
    );
}

/// 本进程映像里的那份进程清单（没有 ⇒ `None`）。
#[cfg(windows)]
fn own_manifest() -> Option<String> {
    const RT_MANIFEST: usize = 24;
    const CREATEPROCESS_MANIFEST_RESOURCE_ID: usize = 1;
    #[link(name = "kernel32")]
    extern "system" {
        fn FindResourceW(module: isize, name: usize, kind: usize) -> isize;
        fn LoadResource(module: isize, res: isize) -> isize;
        fn LockResource(data: isize) -> *const u8;
        fn SizeofResource(module: isize, res: isize) -> u32;
    }
    // SAFETY：只读本进程映像里的资源；模块句柄 0 ＝ 本 exe，资源随映像常驻、不用释放。
    unsafe {
        let res = FindResourceW(0, CREATEPROCESS_MANIFEST_RESOURCE_ID, RT_MANIFEST);
        if res == 0 {
            return None;
        }
        let data = LockResource(LoadResource(0, res));
        let len = SizeofResource(0, res) as usize;
        if data.is_null() || len == 0 {
            return None;
        }
        Some(String::from_utf8_lossy(std::slice::from_raw_parts(data, len)).into_owned())
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/win-manifest/lib_tests.rs"]
mod tests;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本 crate 的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
