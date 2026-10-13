//! 命令表 · cc-bus 总线：`bus-*`。

use crate::stream::inbound::spec::CommandSpec;

pub(super) const SPECS: &[CommandSpec] = &[
    // P4f：cc-bus 的两条基础命令。**转调本机的 cc-bus 命令**，不在后端里重实现总线
    //（「细节先按原本的就行」「后面我可能要改ccbus」）。
    // ⚠ 刻意**没有** `bus-recv`：`cc-recv` 会推进已读位置，backend 代读等于把消息从人那里
    //   偷走。「有没有新的」由 `bus-list` 的待读数回答（只读、不消费）。理由全文在 `control/cc_bus.rs`。
    //   形状（入参 · 回包 · 码表）在 `control/cc_bus_wire.rs`。七条都起子进程并等它退出 ⇒ 阻塞档。
    crate::stream::inbound::typed::blocking::<crate::control::cc_bus_wire::BusListCmd>(
        "谁在线 + 各自待读多少",
    ),
    // 广播：列名单（同 `bus-list` 那一个函数）→ 挑在线的 → 逐个投递（同 `bus-send` 那一处起进程）。
    //   部分投递失败**不整条回错**（成品里逐个列），只有「一条都还没发」的那一步（列名单）失败才回码。
    crate::stream::inbound::typed::blocking::<crate::control::cc_bus_wire::BusBroadcastCmd>(
        "给总线上在线的成员群发一条",
    ),
    crate::stream::inbound::typed::blocking::<crate::control::cc_bus_wire::BusKillCmd>(
        "收掉一个总线成员",
    ),
    crate::stream::inbound::typed::blocking::<crate::control::cc_bus_wire::BusSendCmd>(
        "发一条消息",
    ),
    // 派生一个协作 agent（转调 `cc-spawn`）。**会起一个真 agent 进程（烧额度）** —— 这一跳不重试由调用方负责，
    // 超时那一档的说法里明写「可能已经起来了」（`control::cc_bus::classify_spawn`）。
    crate::stream::inbound::typed::blocking::<crate::control::cc_bus_wire::BusSpawnCmd>(
        "派生一个协作 agent",
    ),
    // `K-R113`：**一次回全的具名读命令。** 名册与 spawn 台账互相引用，分两条命令取回来的两份是两个时刻的，
    // 拼出来的状态盘上从没存在过 ⇒ 两半在同一条命令里回。
    crate::stream::inbound::typed::blocking::<crate::control::cc_bus_wire::BusStateCmd>(
        "总线名单 ＋ spawn 台账一次回全",
    ),
    // 驾驶舱读收件箱：转调 `cc-log`（只读，不推已读位置 —— 不是 `bus-recv`）。
    crate::stream::inbound::typed::blocking::<crate::control::cc_bus_wire::BusInboxCmd>(
        "只读看一个 agent 收件箱的尾巴",
    ),
];
