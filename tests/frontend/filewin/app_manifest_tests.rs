//! # （Windows 进程清单：文件窗口包的测试程序也带「通用控件 v6」那一条依赖）
//!
//! rfd 的 `common-controls-v6` 按名字引入 comctl32 的 `TaskDialogIndirect`；没清单 ⇒ 这份测试程序在 Windows 上
//! 第一行代码之前就死（`0xc0000139`，真机复现过）。清单与编法住 `src/common/win-manifest`，与 monitor 同一份。
//! 只在 Windows 上有意义；Linux 上的同一件事由门禁 `winlink` 从资源表里看。
#![cfg(windows)]

#[test]
fn the_test_binary_carries_the_common_controls_v6_manifest() {
    win_manifest::assert_own_process_carries_it("文件窗口的 lib 测试程序");
}
