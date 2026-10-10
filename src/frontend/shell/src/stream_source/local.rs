//! 本机那条流：本机后端帧 → 会话成品与内容收口。

use super::*;
use crate::event_replay::EventReplay;
use crate::session_book::{In as BookIn, LiveMeta};
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
    /// `session_file_reread.why` → 那一形；认不出 ⇒ `None`（整帧跳过，不猜）。
    pub fn reread_from_wire(why: &str) -> Option<Self> {
        match why {
            "truncated" => Some(FileChange::Truncated),
            "rewritten" => Some(FileChange::Rewritten),
            _ => None,
        }
    }

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
    /// 读循环从 `absorb_local_frame` 手里接回的内容帧（`line` / `session_added` / `session_removed`）。
    Frame(InboundFrame),
    /// 这条流结束了（两条读循环的收尾各送一次）。
    StreamEnded,
    /// 这条流上一行超长、整行丢了（说不出是哪个会话的哪一行）。
    LineLost,
}

/// 本机消费者对一件东西的处置 —— **纯函数**的输出，异步那半只照做。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LocalStep {
    /// 进 [`LineIntake::line`]。
    Line {
        session_id: String,
        path: String,
        seq: u64,
        record: Option<crate::ui_contract::RecordBody>,
        cwd: Option<String>,
        end: Option<u64>,
        rid: Option<String>,
    },
    /// 进 [`LineIntake::announced`]。
    Announce {
        sid: String,
        path: Option<String>,
        lines: Option<u64>,
    },
    /// 冲掉残批，再进 [`LineIntake::removed`]。
    Remove { sid: String },
    /// 本机的某样东西变了 ⇒ 交重放缓冲那张订阅表（与远端同一个 `changed`）。
    Changed { topic: String, cell: String },
    /// 一轮结束（交那台的会话流一格）。
    TurnEnd { sid: String },
    /// 进 [`LineIntake::notice`]（冲掉残批、交一格出声）。
    Notice {
        sid: String,
        path: String,
        change: FileChange,
    },
    /// 这一件不进内容流（不是内容帧）。
    Skip,
    /// 冲掉残批、换一个新的 [`LineIntake`]（下一条流从头来）。
    StreamEnded,
    /// 进 [`LineIntake::lost`]（冲掉残批、原位给一格 `Gap`）。
    Lost,
}

/// 本机那条流上的一件东西 ⇒ 交 `session_book` 的成品（**纯**）。藏起来的 bg 会话到不了这里（[`BgHide`] 那道门在前头）。
///
/// 与远端 [`stream_loop`] 那几条臂交同一种成品（本机 ＝ 不走 ssh 的远端，`INVARIANTS §40`）。流断 ⇒ 这台的成品作废（说不清）。
pub(crate) fn local_product(item: &LocalItem, label: &str) -> Option<BookIn> {
    let origin = || label.to_string();
    match item {
        LocalItem::StreamEnded => Some(BookIn::LinkLost { origin: origin() }),
        LocalItem::LineLost => None,
        LocalItem::Frame(InboundFrame::SessionAdded {
            sid,
            background,
            attachable,
            cwd,
            project_dir,
            name,
            activity,
            activity_text,
            activity_tone,
            waiting_for,
            container,
            pid,
            ..
        }) => Some(BookIn::Live {
            origin: origin(),
            sid: sid.clone(),
            meta: LiveMeta {
                background: *background,
                attachable: *attachable,
                cwd: cwd.clone(),
                project_dir: project_dir.clone(),
                name: name.clone(),
                activity: *activity,
                activity_text: activity_text.clone(),
                activity_tone: activity_tone.clone(),
                waiting_for: waiting_for.clone(),
                container: container.clone(),
                pid: *pid,
            },
        }),
        LocalItem::Frame(InboundFrame::SessionStatus {
            sid,
            activity,
            activity_text,
            activity_tone,
            waiting_for,
        }) => Some(BookIn::Status {
            origin: origin(),
            sid: sid.clone(),
            activity: *activity,
            activity_text: activity_text.clone(),
            activity_tone: activity_tone.clone(),
            waiting_for: waiting_for.clone(),
        }),
        LocalItem::Frame(InboundFrame::SessionState { sid, state, words }) => Some(BookIn::Left {
            origin: origin(),
            sid: sid.clone(),
            fate: *state,
            words: Some(words.clone()),
        }),
        LocalItem::Frame(InboundFrame::SessionsReplayed) => {
            Some(BookIn::Listed { origin: origin() })
        }
        LocalItem::Frame(InboundFrame::SessionRuns { sid, runs, ended }) => Some(BookIn::Runs {
            origin: origin(),
            sid: sid.clone(),
            runs: runs.clone(),
            ended: ended.clone(),
        }),
        LocalItem::Frame(InboundFrame::SessionBranch { sid, off, .. }) => Some(BookIn::Branch {
            origin: crate::origin::Origin(label.to_string()),
            sid: sid.clone(),
            off: off.clone(),
        }),
        LocalItem::Frame(_) => None,
    }
}

/// 本机消费者的**纯分派核**：一件东西 ⇒ 怎么处置。藏起来的 bg 会话到不了这里（[`BgHide`] 那道门在前头）。
pub(crate) fn local_step(item: LocalItem) -> LocalStep {
    match item {
        LocalItem::StreamEnded => LocalStep::StreamEnded,
        LocalItem::LineLost => LocalStep::Lost,
        LocalItem::Frame(InboundFrame::Line {
            session_id,
            path,
            seq,
            record,
            cwd,
            end,
            rid,
        }) => LocalStep::Line {
            session_id,
            path,
            seq,
            record,
            cwd,
            end: Some(end),
            rid,
        },
        LocalItem::Frame(InboundFrame::SessionAdded {
            sid, path, lines, ..
        }) => LocalStep::Announce { sid, path, lines },
        LocalItem::Frame(InboundFrame::SessionRemoved { sid }) => LocalStep::Remove { sid },
        LocalItem::Frame(InboundFrame::SessionFileNotice { sid, path, change }) => {
            LocalStep::Notice { sid, path, change }
        }
        LocalItem::Frame(InboundFrame::TurnEnd { sid }) => LocalStep::TurnEnd { sid },
        // 「X 变了」：与 bg 藏不藏无关（界面按主题 · key 取，藏起来的会话本来就没有 tab）。
        LocalItem::Frame(InboundFrame::Changed { topic, cell }) => {
            LocalStep::Changed { topic, cell }
        }
        LocalItem::Frame(_) => LocalStep::Skip,
    }
}

/// **「bg 会话藏不藏」那一道门**，本机与远端两条流各过一次、同一份（本机 [`consume_local`]、远端 `run.rs` 的读循环）：
/// 没开「显示后台会话」⇒ 宣告时记下它（是不是后台由后端判好，`session_added.background`；协议序保证宣告先于行），
/// 它的行 · 状态 · 运行表 · 主线外 · 文件提示 · 一轮结束 · 去向一律不进；去向那一帧之后摘掉（摘除那一帧照过：它只是内容流的边界）。
/// 每条流一个（流断 ⇒ 下一条流重新宣告，换新的）。
pub(crate) struct BgHide {
    show_bg: bool,
    hidden: std::collections::HashSet<String>,
}

impl BgHide {
    pub(crate) fn new(show_bg: bool) -> Self {
        BgHide {
            show_bg,
            hidden: Default::default(),
        }
    }

    /// 这一帧藏不藏（宣告那一帧顺手记账）。
    pub(crate) fn hides(&mut self, f: &InboundFrame) -> bool {
        match f {
            InboundFrame::SessionAdded {
                sid, background, ..
            } => {
                if *background && !self.show_bg {
                    self.hidden.insert(sid.clone());
                    true
                } else {
                    // 同一个 sid 原地翻回交互（极少见）⇒ 不再藏。
                    self.hidden.remove(sid);
                    false
                }
            }
            InboundFrame::SessionState { sid, .. } => self.hidden.remove(sid),
            InboundFrame::Line {
                session_id: sid, ..
            }
            | InboundFrame::SessionStatus { sid, .. }
            | InboundFrame::SessionRuns { sid, .. }
            | InboundFrame::SessionBranch { sid, .. }
            | InboundFrame::SessionFileNotice { sid, .. }
            | InboundFrame::TurnEnd { sid } => self.hidden.contains(sid),
            _ => false,
        }
    }

    /// 本机那条流上的一件东西过这道门：藏 ⇒ `None`；流结束 · 丢行照过。
    pub(crate) fn admit(&mut self, item: LocalItem) -> Option<LocalItem> {
        match &item {
            LocalItem::Frame(f) if self.hides(f) => None,
            _ => Some(item),
        }
    }
}

/// **本机会话内容的消费者**：吃 `local_lines` 通道，交给与远端同一个 [`LineIntake`]。
/// `label` 是这条流说的那台：产品恒是 `<local>`（`local_lines::install`）；截图台架的无头壳每台机器起一个（`shots_shell`）。
///
/// 每条流一个 `LineIntake`：收到 [`LocalItem::StreamEnded`] ⇒ 冲掉残批、丢掉它（快照队列随之关）、
/// 下一条流换新的 —— 与远端「每条连接一套」同形。本机后端重连之后会重新宣告每个活会话，
/// 旁路快照按续点接着拉（`snapshot_resume`）。
///
/// **本机会话的起停也从这条流来**：后端出的成品经 [`local_product`] 交 `session_book`（与远端同一个口）；
/// 流断 ⇒ 本机的成品作废、当时活的 / 可重连的说不清（`session_book::In::LinkLost`），不归档。
/// ⚠ 本任务**绝不**等一个经本机通道的应答（快照那几问在分发器的任务里）：读循环可能正停在往本通道送东西上，
///   这里要是也等它 ⇒ 互等。
pub(crate) async fn consume_local(
    label: String,
    mut rx: tokio::sync::mpsc::Receiver<LocalItem>,
    replay: Arc<EventReplay>,
    health: HealthOut,
) {
    let show_bg = crate::load_show_bg_sessions();
    loop {
        let mut intake = LineIntake::open(label.clone(), &replay, &health);
        // 没开「显示后台会话」⇒ bg 会话连同它的行 · 状态 · 去向一起不进（远端那条流过同一道门）。
        let mut bg = BgHide::new(show_bg);
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
                replay.origin_seen(&crate::origin::Origin(label.clone()), true);
            }
            let Some(item) = bg.admit(item) else {
                continue;
            };
            // 本机起停的成品：先交 `session_book`。
            //   流断 / 去向那两件先冲掉残批再交（与远端同序：行先落、再说「离开了 / 看不见了」）。
            if matches!(
                item,
                LocalItem::StreamEnded | LocalItem::Frame(InboundFrame::SessionState { .. })
            ) {
                intake.flush().await;
            }
            if let Some(ev) = local_product(&item, &label) {
                crate::session_book::feed(ev);
            }
            match local_step(item) {
                LocalStep::Line {
                    session_id,
                    path,
                    seq,
                    record,
                    cwd,
                    end,
                    rid,
                } => {
                    intake
                        .line(JsonlLine {
                            session_id,
                            path: std::path::PathBuf::from(path),
                            seq,
                            record,
                            cwd,
                            end,
                            rid,
                        })
                        .await
                }
                LocalStep::Announce { sid, path, lines } => intake.announced(&sid, path, lines),
                LocalStep::Remove { sid } => {
                    intake.flush().await;
                    intake.removed(&sid);
                }
                LocalStep::Notice { sid, path, change } => intake.notice(&sid, &path, change).await,
                LocalStep::Changed { topic, cell } => {
                    replay.changed(&crate::origin::Origin(label.clone()), &topic, cell)
                }
                LocalStep::TurnEnd { sid } => {
                    replay.on_turn_end(&crate::origin::Origin(label.clone()), sid)
                }
                LocalStep::Skip => {}
                LocalStep::Lost => intake.lost().await,
                LocalStep::StreamEnded => {
                    intake.flush().await;
                    replay.origin_seen(&crate::origin::Origin(label.clone()), false);
                    tracing::info!(
                        "本机那条流结束：内容收口换新（下一条流重新宣告、快照按续点接着拉）"
                    );
                    break;
                }
            }
        }
    }
}
