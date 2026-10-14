//! 命令表 · 连接本身：`cancel` · `ping` · 链路四条 · 传输四条（帧面硬臂那几条的本体是 `dispatch` 的硬臂）。

use crate::wire::spec::{arg, out, CommandSpec, Run};

pub(super) const SPECS: &[CommandSpec] = &[
    crate::wire::typed::frame::<crate::wire::conn::Cancel>(
        "撤掉一条在跑的命令（`target` = 它的 `id`）；撤不存在的 id 也回 `ok`",
    ),
    // 流只发要看的会话：本连接那份 watcher 的名单 ⇒ **只在帧面**。
    crate::wire::typed::frame::<crate::wire::conn::StreamWatch>(
        "这条流此刻在看哪几个会话（整份换；从没报过 ＝ 全看）：不在名单里的会话，它的行 · 运行表 · 主线外清单 · 活卡不上这条流（刚进名单的会话此刻每张活卡补一张整卡，出名单的撤卡），宣告 · 状态 · 轮次边沿照发；主线外清单只给 `branch` 里要了的会话、照手上那份只补缺的；回刚进名单的那几个从第几行起上流",
    ),
    // **链路四条** —— 本机只常驻一个后端，
    // 到各远端的 SSH 连接由它持有、按拨号身份复用（`ssh-dial/pool.rs`）；monitor 经这条流开「链路」，
    // 链路上的字节与 C2 那个 `--dial` 子进程的 stdout 逐字节同形（`dial/mod.rs` 头注）。
    // 四条都是 `Run::Builtin`：要碰本连接的链路表 ⇒ **只在帧面**，CLI 面不派生（一次性进程没有「连接」可言）。
    crate::wire::typed::frame::<crate::dial::link::LinkOpen>("开一条链路"),
    crate::wire::typed::frame::<crate::dial::link::LinkData>("往链路里送一块上行字节"),
    crate::wire::typed::frame::<crate::dial::link::LinkCredit>("还下行信用"),
    crate::wire::typed::frame::<crate::dial::link::LinkClose>("关一条链路"),
    // **传输四条** —— 用户「SFTP 进本机常驻后端，只写暂存区」：传输台从 monitor 搬进
    // 本机常驻后端。四条都是帧面硬臂：要碰本连接的票表与应答通道 ⇒ **只在帧面**。
    // 开单两条是 typed（入参 · 回包 · 码从类型来），帧面硬臂那一档：分派照旧在 `dispatch` 那条硬臂。
    crate::wire::typed::frame::<crate::control::transfer::TransferUpload>(
        "开一张上传单",
    ),
    crate::wire::typed::frame::<crate::control::transfer::TransferDownload>(
        "开一张下载单",
    ),
    CommandSpec {
        name: "transfer-start",
        summary: "传输单起跑",
        codes: &["bad_args", "no_such_transfer", "already_started"],
        fields: &[arg("id", "传输单 id；之后进度与终局走 `transfer` 帧")],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "transfer-stop",
        summary: "撤一张传输单",
        codes: &["bad_args"],
        fields: &[arg("id", "传输单 id；撤不在册的也回 `ok`")],
        takes_input: true,
        run: Run::Builtin,
    },
    crate::wire::typed::asynchronous::<crate::wire::conn::Ping>("问活：零载荷，回 `ok`"),
];
