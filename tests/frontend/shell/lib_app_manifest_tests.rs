//! # （Windows 进程清单：本包链出来的每一个程序都带「通用控件 v6」那一条依赖，测试程序也一样）
//!
//! 为什么要、清单与编法住哪：`src/common/win-manifest` 的头注（文件窗口包同一份）。本判据在这份测试程序里读回自己的清单。
//! 只在 Windows 上有意义（别处没有资源段这回事）；Linux 上的同一件事由门禁 `winlink` 从资源表里看。
#![cfg(windows)]

#[test]
fn the_test_binary_carries_the_common_controls_v6_manifest() {
    win_manifest::assert_own_process_carries_it("monitor 的 lib 测试程序");
}
