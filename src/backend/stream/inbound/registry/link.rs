//! 命令表 · 连接本身：`cancel` · `stream-watch` · `ping` · 传输四条（文件名是当年链路四条的住处，链路那一族已删）（帧面硬臂那几条的本体是 `dispatch` 的硬臂）。

use crate::wire::spec::CommandSpec;

pub(super) const SPECS: &[CommandSpec] = &[
    super::typed::frame::<crate::wire::conn::Cancel>(
        "撤掉一条在跑的命令（`target` = 它的 `id`）；撤不存在的 id 也回 `ok`",
    ),
    // 流只发要看的会话：本连接那份 watcher 的名单 ⇒ **只在帧面**。
    super::typed::frame::<crate::wire::conn::StreamWatch>(
        "这条流此刻在看哪几个会话（整份换；从没报过 ＝ 全看）：不在名单里的会话，它的行 · 运行表 · 主线外清单 · 活卡不上这条流（刚进名单的会话此刻每张活卡补一张整卡，出名单的撤卡），宣告 · 状态 · 轮次边沿照发；主线外清单只给 `branch` 里要了的会话、照手上那份只补缺的；回刚进名单的那几个从第几行起上流",
    ),
    // **传输四条** —— 用户「SFTP 进本机常驻后端，只写暂存区」：传输台从 monitor 搬进
    // 本机常驻后端。四条都是帧面硬臂：要碰本连接的票表与应答通道 ⇒ **只在帧面**。
    // 开单两条是 typed（入参 · 回包 · 码从类型来），帧面硬臂那一档：分派照旧在 `dispatch` 那条硬臂。
    super::typed::frame::<crate::control::transfer::TransferUpload>(
        "开一张上传单",
    ),
    super::typed::frame::<crate::control::transfer::TransferDownload>(
        "开一张下载单",
    ),
    super::typed::frame::<crate::control::transfer::TransferStart>("传输单起跑"),
    super::typed::frame::<crate::control::transfer::TransferStop>("撤一张传输单"),
    // 两台之间复制一组：进度走本连接的应答通道（`files_batch` 帧）⇒ 帧面硬臂，在阻塞线程上做完再回。
    super::typed::frame_blocking::<crate::control::transfer_copy::TransferCopy>(
        "两台之间复制一组",
    ),
    super::typed::asynchronous::<crate::wire::conn::Ping>("问活：零载荷，回 `ok`"),
];
