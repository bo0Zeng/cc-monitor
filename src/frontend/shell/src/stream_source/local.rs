//! 本机那条流：本机后端帧 → 会话成品与内容收口。

use super::*;
use crate::event_replay::EventReplay;
use crate::session_book::In as BookIn;
use std::sync::Arc;

/// 活会话的记录文件怎么了。线上（后端 `session_file_gone` / `session_file_reread.why`）与交前端的
/// 那一格（`SessionFileNoticePayload.change`）同一组字面量。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChange {
    /// 不见了（删了 / 改名走了）。
    Gone,
    /// 变短了，已从头重读。
    Truncated,
    /// 游标之前被原地改写过，已从头重读。
    Rewritten,
}

impl FileChange {
    /// 交前端那一格的字面量。
    pub fn as_wire(self) -> &'static str {
        match self {
            FileChange::Gone => "gone",
            FileChange::Truncated => "truncated",
            FileChange::Rewritten => "rewritten",
        }
    }
}

/// 本机那条流交进来的东西（`local_lines` 通道上的一件）。
#[derive(Debug)]
pub(crate) enum LocalItem {
    /// 读循环分派第一层交回的有序那一件（[`route`]）。
    Frame(Ordered),
    /// 这条流结束了（两条读循环的收尾各送一次）。
    StreamEnded,
    /// 这条流上一行超长、整行丢了（说不出是哪个会话的哪一行）。
    LineLost,
}

/// **本机会话内容的消费者**：吃 `local_lines` 通道，交给与远端同一个 [`LineIntake`]。
/// `label` 是这条流说的那台：产品恒是 `<local>`（`local_lines::install`）；截图台架的无头壳每台机器起一个（`shots_shell`）。
///
/// 每条流一个 `LineIntake`：收到 [`LocalItem::StreamEnded`] ⇒ 冲掉残批、丢掉它（快照队列随之关）、
/// 下一条流换新的 —— 与远端「每条连接一套」同形。本机后端重连之后会重新宣告每个活会话，
/// 旁路快照按续点接着拉（`snapshot_resume`）。
///
/// **本机会话的起停也从这条流来**：有序的那几种经 [`apply`] 交 `session_book` 与收口（与远端同一处）；
/// 流断 ⇒ 本机的成品作废、当时活的 / 可重连的说不清（`session_book::In::LinkLost`），不归档。
/// ⚠ 本任务**绝不**等一个经本机通道的应答（快照那几问在分发器的任务里）：读循环可能正停在往本通道送东西上，
///   这里要是也等它 ⇒ 互等。
pub(crate) async fn consume_local(
    label: String,
    mut rx: tokio::sync::mpsc::Receiver<LocalItem>,
    replay: Arc<EventReplay>,
    health: HealthOut,
) {
    let origin = crate::origin::Origin(label.clone());
    loop {
        let mut intake = LineIntake::open(label.clone(), &replay, &health);
        // 这条流交来第一件东西 ⇒ 本机那台「看得见」（订阅原位收 `Seen`）；流结束 ⇒ `Unseen`。
        let mut seen = false;
        loop {
            let Some(item) = intake.recv_or_flush(&mut rx).await else {
                // 发送端全没了（进程收摊）：残批照发，然后退出。
                intake.flush().await;
                return;
            };
            if !seen && !matches!(item, LocalItem::StreamEnded) {
                seen = true;
                replay.origin_seen(&origin, true);
            }
            match item {
                LocalItem::Frame(f) => apply(&origin, f, &mut intake, &replay, &health).await,
                LocalItem::LineLost => intake.lost().await,
                LocalItem::StreamEnded => {
                    // 残批先冲完、再说「看不见了」（与远端同序：行先落）。
                    intake.flush().await;
                    crate::session_book::feed(BookIn::LinkLost {
                        origin: label.clone(),
                    });
                    replay.origin_seen(&origin, false);
                    tracing::info!(
                        "本机那条流结束：内容收口换新（下一条流重新宣告、快照按续点接着拉）"
                    );
                    break;
                }
            }
        }
    }
}
