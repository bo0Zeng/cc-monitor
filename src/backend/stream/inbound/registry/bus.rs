//! 命令表 · cc-bus 总线：`bus-*`。收件箱住这台后端（`control/cc_bus/store.rs`），这几条是读写它的唯一入口；
//! `cc-bus <动词>` 每一条只是把一句交给这里（`ccm -- --bus-<动词>`）。形状（入参 · 回包 · 码表）在 `control/cc_bus_wire.rs`。

use crate::wire::spec::CommandSpec;

pub(super) const SPECS: &[CommandSpec] = &[
    // 名单：各收件箱 ＋ 叫它什么 · 在不在 ＋ 待读多少。
    crate::wire::typed::blocking::<crate::control::cc_bus_wire::BusListCmd>(
        "谁在线 + 各自待读多少",
    ),
    // 发信：核信封 · 判范围 · 去重 · 限流，收进收件人的收件箱，回那一封与敲不敲。
    crate::wire::typed::asynchronous::<crate::control::cc_bus_wire::BusSendCmd>(
        "发一封信：核信封、照规矩收进收件箱",
    ),
    // 驾驶舱读收件箱：只读看尾巴（不推已读位置）。
    crate::wire::typed::blocking::<crate::control::cc_bus_wire::BusInboxCmd>(
        "只读看一个 agent 收件箱的尾巴",
    ),
    // 两阶段读：peek 纯读出那一段与令牌，commit 拿令牌推已读位置（`cc-bus recv` ＝ peek 接 commit；Stop 钩子喂回成功之后才 commit）。
    crate::wire::typed::blocking::<crate::control::cc_bus_wire::BusPeekCmd>(
        "读新信（不推已读位置）",
    ),
    crate::wire::typed::blocking::<crate::control::cc_bus_wire::BusCommitCmd>(
        "推已读位置（比令牌）",
    ),
    crate::wire::typed::blocking::<crate::control::cc_bus_wire::BusUnreadCmd>(
        "未读几封",
    ),
];
