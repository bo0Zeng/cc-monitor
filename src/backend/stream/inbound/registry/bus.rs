//! 命令表 · cc-bus 总线：`bus-*`。

use crate::wire::spec::CommandSpec;

pub(super) const SPECS: &[CommandSpec] = &[
    // P4f：cc-bus 的读命令。**转调本机的 cc-bus 命令**，不在后端里重实现总线
    //（「细节先按原本的就行」「后面我可能要改ccbus」）。
    // ⚠ 刻意**没有** `bus-recv`：`cc-recv` 会推进已读位置，backend 代读等于把消息从人那里
    //   偷走。「有没有新的」由 `bus-list` 的待读数回答（只读、不消费）。理由全文在 `control/cc_bus.rs`。
    //   形状（入参 · 回包 · 码表）在 `control/cc_bus_wire.rs`。读的两条起子进程并等它退出 ⇒ 阻塞档。
    //   起会话只走 `session-open`，「谁起的谁」读血缘 ⇒ 总线这里不起、不杀会话。
    crate::wire::typed::blocking::<crate::control::cc_bus_wire::BusListCmd>(
        "谁在线 + 各自待读多少",
    ),
    // 信封的唯一一处校验：`cc-send` 发之前问它，回规范信封（盖好 id · 时间），不合格当场拒。纯函数、不起进程。
    crate::wire::typed::asynchronous::<crate::control::cc_bus_wire::BusEnvelopeCmd>(
        "核一封信、盖上 id 与时间",
    ),
    // 驾驶舱读收件箱：转调 `cc-log`（只读，不推已读位置 —— 不是 `bus-recv`）。
    crate::wire::typed::blocking::<crate::control::cc_bus_wire::BusInboxCmd>(
        "只读看一个 agent 收件箱的尾巴",
    ),
];
