//! 文件窗口独立成包之后**跨两半**的判据 —— 一侧是窗口包（`cc_monitor_filewin`），异源那一侧是 monitor 的东西，
//! 只有 monitor 的测试档两边都够得着（monitor 链窗口包只为那个 `[[bin]]`，生产段零引用；测试档引它不进产物）。
//!
//! 要求：`§2.3`（窗口只说 `call`，开单与订阅的线上名字两侧对上）。

/// 🔴 窗口这一侧写死的那个**线上名字**（进度流的前缀）与对面（monitor 的传输中继）那一份逐字相等（原住 `transfer_tests` 的前半；
/// 后端提交命令表那一半留在窗口包里）。漂开的症状是具体的：订阅答「没有这条流」。
#[test]
fn the_transfer_names_the_window_says_are_the_ones_the_monitor_relays() {
    // 开单两条的名字不再两边各写一份：窗口发的、monitor 认的都是 `cmd_wire::transfer` 那两个入参类型挂的名字。
    assert_eq!(
        cc_monitor_filewin::transfer::KIND_PREFIX,
        crate::transfer_relay::TRANSFER_KIND_PREFIX
    );
}
