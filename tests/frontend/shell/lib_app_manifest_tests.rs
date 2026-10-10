//! # （Windows 应用清单：本包链出来的每一个程序都带「通用控件 v6」那一条依赖，测试程序也一样）
//!
//! Tauri 的 Windows 运行时（tao · wry · 对话框）按名字引入 `comctl32.dll` 的 `TaskDialogIndirect` · `SetWindowSubclass` 一族，
//! 系统目录里那份 comctl32 是 v5，按名字只有 v6 导出它们 ⇒ 程序没带「依赖 Common-Controls 6.0.0.0」的清单，
//! 系统装不上它，进程在第一行代码之前就死（`0xc0000139` STATUS_ENTRYPOINT_NOT_FOUND）。
//! 清单由 `build.rs` 编成资源、交给本包的**每一个**链接产物（不只是 `[[bin]]`）；本判据在这份测试程序里读回自己的清单。
//! 只在 Windows 上有意义（别处没有资源段这回事）。
#![cfg(windows)]

const RT_MANIFEST: usize = 24;
const CREATEPROCESS_MANIFEST_RESOURCE_ID: usize = 1;

#[link(name = "kernel32")]
extern "system" {
    fn FindResourceW(module: isize, name: usize, kind: usize) -> isize;
    fn LoadResource(module: isize, res: isize) -> isize;
    fn LockResource(data: isize) -> *const u8;
    fn SizeofResource(module: isize, res: isize) -> u32;
}

/// 本程序自己内嵌的那份进程清单（没有 ⇒ `None`）。
fn own_manifest() -> Option<String> {
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

#[test]
fn the_test_binary_carries_the_common_controls_v6_manifest() {
    let m = own_manifest().expect(
        "这份测试程序没有内嵌进程清单 —— build.rs 只把清单交给了 [[bin]]，测试程序一碰 Tauri 运行时就起不来（0xc0000139）",
    );
    assert!(
        m.contains("Microsoft.Windows.Common-Controls") && m.contains("6.0.0.0"),
        "清单里没有「通用控件 v6」那一条依赖：{m}"
    );
}
