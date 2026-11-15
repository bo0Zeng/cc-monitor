//! 命令表 · 连接本身：`cancel` · `stream-watch` · `ping` · 两台之间复制一组（帧面硬臂那几条的本体是 `dispatch` 的硬臂）·
//! 桌面附件草稿那一圈（`attach-draft-*`：本机后端替这条连接的界面编排，本体住 `control/attach_upload.rs`）。

use crate::wire::spec::CommandSpec;

pub(super) const SPECS: &[CommandSpec] = &[
    super::typed::frame::<crate::wire::conn::Cancel>(
        "撤掉一条在跑的命令（`target` = 它的 `id`）；撤不存在的 id 也回 `ok`",
    ),
    // 流只发要看的会话：本连接那份 watcher 的名单 ⇒ **只在帧面**。
    super::typed::frame::<crate::wire::conn::StreamWatch>(
        "这条流此刻在看哪几个会话（整份换；从没报过 ＝ 全看）：不在名单里的会话，它的行 · 运行表 · 主线外清单 · 活卡不上这条流（刚进名单的会话此刻每张活卡补一张整卡，出名单的撤卡），宣告 · 状态 · 轮次边沿照发；主线外清单只给 `branch` 里要了的会话、照手上那份只补缺的；回刚进名单的那几个从第几行起上流",
    ),
    // 两台之间复制一组：进度走本连接的应答通道（`files_batch` 帧）⇒ 帧面硬臂，在阻塞线程上做完再回。
    super::typed::frame_blocking::<crate::control::transfer_copy::TransferCopy>(
        "两台之间复制一组",
    ),
    super::typed::asynchronous::<crate::wire::conn::Ping>("问活：零载荷，回 `ok`"),
    // 桌面附件草稿那一排：本机后端问会话那台（问收不收 · 送块 · 提交 · 删）、推 `changed/attach`，界面只交名单。
    super::typed::asynchronous::<crate::control::attach_upload::AttachDraftPut>(
        "往一条草稿那一排加几份（壳放进本机暂存区的那一份 ＋ 名字 · 字节）：没有那一排 ⇒ 照会话那台开一排；回此刻那一排",
    ),
    super::typed::asynchronous::<crate::control::attach_upload::AttachDraftDrop>(
        "✕ 草稿那一排的一份（在传 ⇒ 撤那一趟；落好了 ⇒ 那台删掉那个编号）；回此刻那一排",
    ),
    super::typed::asynchronous::<crate::control::attach_upload::AttachDraftRetry>(
        "［重试］草稿那一排：失败 · 本机副本开不了的回排队，断着的再试一次；回此刻那一排",
    ),
    super::typed::asynchronous::<crate::control::attach_upload::AttachDraftTake>(
        "发出去那一下：全落好了 ⇒ 编号（按先后）、那一排清空；还有没落好的 ⇒ `null`",
    ),
    super::typed::asynchronous::<crate::control::attach_upload::AttachDraftView>(
        "草稿那一排此刻的样子（推的那一帧丢了 · 没带体时重问）",
    ),
];
