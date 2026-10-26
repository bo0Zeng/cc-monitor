//! **一帧落到哪**：本机那条流与本机后端转来的各台那几条同一处分派，分两层（两层是线程决定的，不是两份实现）：
//! - [`route`]：读的那一头就地交、从不阻塞的那几种（应答 · 链路 · 传输 · 测试连接 · 一组进度 · 活卡 · 终端画面）；
//!   有序的那几种回 [`Ordered`]。
//! - [`apply`]：有序的那几种交会话账（`session_book`）与内容收口（[`LineIntake`]），同一条流上的先后原样保留。
//!
//! 本机两条读循环调 [`route`]、经 `local_lines` 送 [`Ordered`]；收的那一头（`consume`）把别台转来的那一行解开、再调 [`route`]，两路都调 [`apply`]。

use super::*;
use crate::event_replay::EventReplay;
use crate::session_book::In as BookIn;
use cmd_wire::frame as wire;
use std::sync::Arc;

/// 有序的那几种：交会话账与内容收口，先后就是意义。
#[derive(Debug)]
pub(crate) enum Ordered {
    /// 本机后端转来的别台那条流上的一件（收的那一头按那台分到那一份收口）。
    Relayed(wire::Relayed),
    Line(wire::Line<crate::ui_contract::RecordBody>),
    SessionAdded(wire::SessionAdded),
    SessionPrimed(wire::SessionPrimed),
    SessionStatus(wire::SessionStatus),
    SessionState(wire::SessionState),
    SessionRemoved(wire::SessionRemoved),
    Overflow(wire::Overflow),
    Changed(wire::Changed),
    SessionsReplayed(wire::SessionsReplayed),
    SessionFileGone(wire::SessionFileGone),
    SessionFileReread(wire::SessionFileReread),
    SessionRuns(wire::SessionRuns),
    SessionBranch(wire::SessionBranch),
}

/// 第一层：就地交的那几种交掉（回 `None`），有序的那几种回 `Some`。`client` ＝ 这条连接的客户端（应答按 `id` 交回请求方）。
/// 只开在本机那条流上的那几种（传输 · 测试连接 · 转来的别台）从别台来 ⇒ 照实说、丢；hello 只在接上那一刻来，流中间再来 ⇒ 照实说、丢。
pub(crate) fn route(
    origin: &crate::origin::Origin,
    client: Option<&crate::inbound_client::InboundClient>,
    frame: Frame,
) -> Option<Ordered> {
    let at = origin.as_wire_str();
    let local = *origin == crate::origin::Origin::local();
    match frame {
        Frame::Hello(_) => tracing::warn!("[{at}] 流中间来了 hello，丢掉"),
        Frame::Relayed(r) if local => return Some(Ordered::Relayed(r)),
        Frame::Relayed(r) => {
            tracing::warn!(
                "[{at}] 别台那条流上又套着一件转来的（{}），丢掉",
                r.machine()
            )
        }
        f @ (Frame::Reply(_) | Frame::Cancelled(_)) => match client {
            Some(c) => {
                resident_client::client::deliver_answer(c, f);
            }
            None => tracing::warn!("[{at}] 还没握手就来了应答，丢掉"),
        },
        // 传输台住本机后端：进度 / 终局交中继（从不阻塞）。
        Frame::Transfer(t) if local => crate::sftp_pool::deliver(t),
        // 测试连接在本机常驻后端里跑：那一格原样交中继（界面严格收）。
        Frame::Probe(p) if local => crate::probe_relay::deliver(&p.ticket, p.cell.to_string()),
        Frame::Transfer(_) | Frame::Probe(_) => {
            tracing::warn!("[{at}] 远端后端发来了只开在本机后端上的帧（传输 / 测试连接），丢掉")
        }
        // 一组复制 · 删除 · 改权限的进度：按票交给订了它的那扇文件窗口（没人订就丢）。
        Frame::FilesBatch(b) => {
            let ticket = b.ticket.clone();
            crate::files_batch_relay::deliver(&ticket, body_without(&b, "ticket"))
        }
        // 活卡（后端拼好的成品）原样转前端（不进内容通道）。
        Frame::LiveCard(l) => crate::live_card::deliver(at, l),
        // 终端实时预览：一屏 / 收尾 ⇒ 交订了这台 `terminal-screen/<票>` 的那条订阅。
        Frame::TerminalScreen(s) => {
            let ticket = s.ticket.clone();
            crate::terminal_screen_relay::deliver(origin, &ticket, body_without(&s, "ticket"))
        }
        Frame::TerminalFollowEnd(e) => {
            let ticket = e.ticket.clone();
            crate::terminal_screen_relay::deliver(origin, &ticket, body_without(&e, "ticket"))
        }
        Frame::Line(l) => return Some(Ordered::Line(l)),
        Frame::SessionAdded(f) => return Some(Ordered::SessionAdded(f)),
        Frame::SessionPrimed(f) => return Some(Ordered::SessionPrimed(f)),
        Frame::SessionStatus(f) => return Some(Ordered::SessionStatus(f)),
        Frame::SessionState(f) => return Some(Ordered::SessionState(f)),
        Frame::SessionRemoved(f) => return Some(Ordered::SessionRemoved(f)),
        Frame::Overflow(f) => return Some(Ordered::Overflow(f)),
        Frame::Changed(f) => return Some(Ordered::Changed(f)),
        Frame::SessionsReplayed(f) => return Some(Ordered::SessionsReplayed(f)),
        Frame::SessionFileGone(f) => return Some(Ordered::SessionFileGone(f)),
        Frame::SessionFileReread(f) => return Some(Ordered::SessionFileReread(f)),
        Frame::SessionRuns(f) => return Some(Ordered::SessionRuns(f)),
        Frame::SessionBranch(f) => return Some(Ordered::SessionBranch(f)),
    }
    None
}

/// 帧体去掉一格（票）的文本：转交给订了那张票的订阅（格体由界面严格收）。
fn body_without<T: serde::Serialize>(body: &T, key: &str) -> String {
    let mut v = serde_json::to_value(body).unwrap_or_default();
    if let Some(o) = v.as_object_mut() {
        o.remove(key);
    }
    v.to_string()
}

/// 第二层：有序的那一件交会话账与内容收口。
/// 会话离开（`session_removed`）与去向（`session_state`）先冲残批：它们说的「离开了」必须排在这个会话的行之后。
pub(crate) async fn apply(
    origin: &crate::origin::Origin,
    item: Ordered,
    intake: &mut LineIntake,
    replay: &Arc<EventReplay>,
    health: &HealthOut,
) {
    let host_label = origin.as_wire_str();
    let at = host_label.to_string();
    if matches!(item, Ordered::SessionRemoved(_) | Ordered::SessionState(_)) {
        intake.flush().await;
    }
    match item {
        // 转来的那一件由收的那一头按那台分走，不进某一台的收口。
        Ordered::Relayed(r) => tracing::warn!(
            "[{at}] 转来的一件（{}）落到了某一台的收口里，丢掉",
            r.machine()
        ),
        Ordered::Line(l) => {
            intake
                .line(JsonlLine {
                    session_id: l.session_id,
                    path: std::path::PathBuf::from(l.path),
                    seq: l.seq,
                    record: l.record,
                    cwd: l.cwd,
                    end: Some(l.byte_offset),
                    rid: l.rid,
                    catch_up: false,
                })
                .await
        }
        Ordered::SessionAdded(meta) => {
            let sid = meta.sid.clone();
            tracing::info!("session-added: [{host_label}] sid={sid} → 成品交出口");
            crate::session_book::feed(BookIn::Live {
                origin: at,
                sid,
                meta,
            });
        }
        // 读完了：有记录文件就排旁路快照（队列按 sid 幂等）。
        Ordered::SessionPrimed(p) => intake.announced(&p.sid, p.path, p.lines),
        Ordered::SessionStatus(s) => {
            tracing::info!(
                "session-status: [{at}] sid={} live={:?} waiting_for={:?} → 成品交出口",
                s.sid,
                s.face.live,
                s.waiting_for
            );
            crate::session_book::feed(BookIn::Status {
                origin: at,
                status: s,
            });
        }
        Ordered::SessionRemoved(r) => {
            // 只剩内容流的边界：摘排队中的快照、给在途的打取消标记、续点作废；去向由下一帧 `session_state` 说。
            let sid = r.sid;
            tracing::info!(
                "session-removed: [{host_label}] sid={sid}（内容流收口；去向看 session_state）"
            );
            intake.removed(&sid);
        }
        Ordered::SessionState(s) => {
            let (sid, state) = (&s.sid, s.state);
            tracing::info!("session-state: [{host_label}] sid={sid} → {state:?}");
            crate::session_book::feed(BookIn::Left {
                origin: at,
                state: s,
            });
        }
        Ordered::Overflow(o) => on_overflow(origin, health, &o),
        Ordered::Changed(c) => {
            let topic = serde_json::to_value(c.topic())
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            // 本机的机器表变了 ⇒ 本壳重问、对齐远端流，对齐完再告诉界面（`machine_table::tell_followed`）：这一格不往界面转。
            if c.topic() == wire::Topic::Machines && *origin == crate::origin::Origin::local() {
                crate::machine_table::refresh();
                return;
            }
            replay.changed(origin, &topic, body_without(&c, "topic"));
        }
        Ordered::SessionsReplayed(_) => {
            tracing::info!("sessions-replayed: [{at}] 活会话清单报完了 → 成品交出口");
            crate::session_book::feed(BookIn::Listed { origin: at });
        }
        Ordered::SessionFileGone(g) => {
            intake
                .notice(&g.session_id, &g.path, FileChange::Gone, None)
                .await
        }
        Ordered::SessionFileReread(r) => {
            let change = match r.why {
                wire::RereadWhy::Truncated => FileChange::Truncated,
                wire::RereadWhy::Rewritten => FileChange::Rewritten,
            };
            intake
                .notice(&r.session_id, &r.path, change, Some(r.lines))
                .await
        }
        Ordered::SessionRuns(runs) => crate::session_book::feed(BookIn::Runs { origin: at, runs }),
        Ordered::SessionBranch(b) => crate::session_book::feed(BookIn::Branch {
            origin: origin.clone(),
            sid: b.sid,
            path: b.path,
            at: b.at,
            off: b.off,
            sum: b.sum,
        }),
    }
}

/// 那台管道拥塞丢过帧：日志 ＋ 经 `remote-health` 原样交那台写的那一句（标题位界面排机器名）。
/// 丢了会话状态那一类的，那台紧跟着在同一条连接上整份重报，这里不判、不重接。
fn on_overflow(origin: &crate::origin::Origin, health: &HealthOut, o: &wire::Overflow) {
    let at = origin.as_wire_str();
    tracing::warn!("stream_source [{at}] overflow: {}", o.said.0);
    let payload = crate::ui_contract::RemoteHealthPayload {
        origin: at.to_string(),
        kind: "overflow".to_string(),
        message: o.said.0.clone(),
        detail: String::new(),
    };
    if let Err(e) = health(payload) {
        tracing::warn!("stream_source remote-health emit failed: {e}");
    }
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/frame_dispatch_shape.rs"]
mod frame_dispatch_shape;
