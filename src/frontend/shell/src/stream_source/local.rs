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
    /// 本机某个会话的任务清单变了 ⇒ 交重放缓冲那张订阅表（与远端同一个 `tasks_changed`）。
    Tasks { sid: String },
    /// 本机的额度账变了（`None`）/ 某个会话的轮换变了（`Some(sid)`）⇒ 交重放缓冲那张订阅表（与远端同一个口）。
    Quota { sid: Option<String> },
    /// 本机的轮换规则表变了 ⇒ 同上那张订阅表一格 `{"rules":true}`。
    Rules,
    /// 进 [`LineIntake::notice`]（冲掉残批、交一格出声）。
    Notice {
        sid: String,
        path: String,
        change: FileChange,
    },
    /// 这一件不进内容流（被藏起来的 bg 会话的行 / 宣告，或不是内容帧）。
    Skip,
    /// 冲掉残批、换一个新的 [`LineIntake`]（下一条流从头来）。
    StreamEnded,
    /// 进 [`LineIntake::lost`]（冲掉残批、原位给一格 `Gap`）。
    Lost,
}

/// 后台会话要不要藏：没开「显示后台会话」就藏（是不是后台由后端判好，`session_added.background`）。
fn local_hides(background: bool, show_bg: bool) -> bool {
    !show_bg && background
}

/// 本机那条流上的一件东西 ⇒ 交 `session_book` 的成品（**纯**；藏起来的 bg 会话不进）。
///
/// 与远端 [`stream_loop`] 那几条臂交同一种成品（本机 ＝ 不走 ssh 的远端，`INVARIANTS §40`）；与 [`local_step`] 读同一件东西、
/// 同一个「藏不藏」口径（[`local_hides`]），但**先于**它跑（它会改 `hidden`）。流断 ⇒ 这台的成品作废（说不清）。
pub(crate) fn local_product(
    item: &LocalItem,
    show_bg: bool,
    hidden: &std::collections::HashSet<String>,
) -> Option<BookIn> {
    let origin = || crate::origin::LOCAL.to_string();
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
            waiting_for,
            container,
            pid,
            ..
        }) => (!local_hides(*background, show_bg)).then(|| BookIn::Live {
            origin: origin(),
            sid: sid.clone(),
            meta: LiveMeta {
                background: *background,
                attachable: *attachable,
                cwd: cwd.clone(),
                project_dir: project_dir.clone(),
                name: name.clone(),
                activity: *activity,
                waiting_for: waiting_for.clone(),
                container: container.clone(),
                pid: *pid,
            },
        }),
        LocalItem::Frame(InboundFrame::SessionStatus {
            sid,
            activity,
            waiting_for,
        }) => (!hidden.contains(sid)).then(|| BookIn::Status {
            origin: origin(),
            sid: sid.clone(),
            activity: *activity,
            waiting_for: waiting_for.clone(),
        }),
        LocalItem::Frame(InboundFrame::SessionState { sid, state }) => (!hidden.contains(sid))
            .then(|| BookIn::Left {
                origin: origin(),
                sid: sid.clone(),
                fate: *state,
            }),
        LocalItem::Frame(InboundFrame::SessionsReplayed) => {
            Some(BookIn::Listed { origin: origin() })
        }
        LocalItem::Frame(InboundFrame::SessionRuns { sid, runs, ended }) => (!hidden.contains(sid))
            .then(|| BookIn::Runs {
                origin: origin(),
                sid: sid.clone(),
                runs: runs.clone(),
                ended: ended.clone(),
            }),
        LocalItem::Frame(InboundFrame::SessionBranch { sid, off, .. }) => (!hidden.contains(sid))
            .then(|| BookIn::Branch {
                origin: origin(),
                sid: sid.clone(),
                off: off.clone(),
            }),
        LocalItem::Frame(_) => None,
    }
}

/// 本机消费者的**纯分派核**：一件东西 × 「显示 bg 吗」× 「藏起来的 sid」⇒ 怎么处置。
///
/// 为什么 bg 在这里藏而不在后端那边按旗标分：本机常驻后端**跨 monitor 存活**，`adopt` 只比
/// `build_id` 与家目录、不比起参 ⇒ 起参里的 `--with-bg` 挡不住「用户关了 bg 显示、却接上了一个按开着起的后端」。
/// 于是两条载体一律带 `--with-bg`，显示与否在这一侧按 `session_added.background` 定（协议序保证宣告先于行）。
pub(crate) fn local_step(
    item: LocalItem,
    show_bg: bool,
    hidden: &mut std::collections::HashSet<String>,
) -> LocalStep {
    match item {
        LocalItem::StreamEnded => {
            hidden.clear();
            LocalStep::StreamEnded
        }
        LocalItem::LineLost => LocalStep::Lost,
        LocalItem::Frame(InboundFrame::Line {
            session_id,
            path,
            seq,
            record,
            cwd,
            end,
            rid,
        }) => {
            if hidden.contains(&session_id) {
                LocalStep::Skip
            } else {
                LocalStep::Line {
                    session_id,
                    path,
                    seq,
                    record,
                    cwd,
                    end: Some(end),
                    rid,
                }
            }
        }
        LocalItem::Frame(InboundFrame::SessionAdded {
            sid,
            background,
            path,
            lines,
            ..
        }) => {
            if local_hides(background, show_bg) {
                hidden.insert(sid);
                LocalStep::Skip
            } else {
                // 同一个 sid 原地翻回交互（极少见）⇒ 不再藏。
                hidden.remove(&sid);
                LocalStep::Announce { sid, path, lines }
            }
        }
        // 藏着的 sid 留到它的去向（`session_state`）那一帧才摘：去向也要照「藏」那一条滤掉。
        LocalItem::Frame(InboundFrame::SessionRemoved { sid }) => LocalStep::Remove { sid },
        LocalItem::Frame(InboundFrame::SessionState { sid, .. }) => {
            hidden.remove(&sid);
            LocalStep::Skip
        }
        // 藏起来的 bg 会话照旧不出声（它的行也不进内容流）。
        LocalItem::Frame(InboundFrame::SessionFileNotice { sid, path, change }) => {
            if hidden.contains(&sid) {
                LocalStep::Skip
            } else {
                LocalStep::Notice { sid, path, change }
            }
        }
        // 任务清单变了：与 bg 藏不藏无关（任务面板按 sid 取，藏起来的会话本来就没有 tab）。
        LocalItem::Frame(InboundFrame::TasksChanged { sid }) => LocalStep::Tasks { sid },
        // 额度 / 轮换变了：与 bg 藏不藏无关（界面按 sid 取）。
        LocalItem::Frame(InboundFrame::QuotaChanged) => LocalStep::Quota { sid: None },
        LocalItem::Frame(InboundFrame::RotationChanged { sid }) => {
            LocalStep::Quota { sid: Some(sid) }
        }
        LocalItem::Frame(InboundFrame::RotationRulesChanged) => LocalStep::Rules,
        LocalItem::Frame(_) => LocalStep::Skip,
    }
}

/// 本机常驻后端两种载体的起参里**恒有** `--tail-only` ⇒ 本机那条流的历史一律走旁路快照。
/// 两份起参与本常量的一致性由判据对拍（`local_lines_tests`），不靠这句注释。
pub(crate) const LOCAL_STREAM_TAIL_ONLY: bool = true;

/// **本机会话内容的消费者**：吃 `local_lines` 通道，交给与远端同一个 [`LineIntake`]。
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
    mut rx: tokio::sync::mpsc::Receiver<LocalItem>,
    replay: Arc<EventReplay>,
    health: HealthOut,
) {
    let show_bg = crate::load_show_bg_sessions();
    let label = crate::origin::LOCAL.to_string();
    let mut hidden: std::collections::HashSet<String> = std::collections::HashSet::new();
    loop {
        let mut intake = LineIntake::open(label.clone(), LOCAL_STREAM_TAIL_ONLY, &replay, &health);
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
            // 本机起停的成品：先交 `session_book`（它按 `hidden` 滤，而下面 `local_step` 会改 `hidden`）。
            //   流断 / 去向那两件先冲掉残批再交（与远端同序：行先落、再说「离开了 / 看不见了」）。
            if matches!(
                item,
                LocalItem::StreamEnded | LocalItem::Frame(InboundFrame::SessionState { .. })
            ) {
                intake.flush().await;
            }
            if let Some(ev) = local_product(&item, show_bg, &hidden) {
                crate::session_book::feed(ev);
            }
            match local_step(item, show_bg, &mut hidden) {
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
                LocalStep::Tasks { sid } => {
                    replay.tasks_changed(&crate::origin::Origin(label.clone()), &sid)
                }
                LocalStep::Quota { sid } => {
                    replay.quota_changed(&crate::origin::Origin(label.clone()), sid.as_deref())
                }
                LocalStep::Rules => {
                    replay.rotation_rules_changed(&crate::origin::Origin(label.clone()))
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
