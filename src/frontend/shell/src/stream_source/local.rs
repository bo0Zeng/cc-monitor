//! 本机那条流：本机后端的帧，连同它转来的别台那几条流上的事（`relayed`），按那台分到同一个收口 → 会话成品与内容收口。

use super::*;
use crate::event_replay::EventReplay;
use crate::session_book::In as BookIn;
use cmd_wire::frame::Relayed;
use std::collections::HashMap;
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

/// 告诉用户的那几句里那台叫什么：本机 ⇒「本机」，别台 ⇒ 它的名字。
pub(crate) fn host_label(origin: &crate::origin::Origin) -> String {
    if origin.is_local() {
        copy_core::copy_text("rsLocalBackend.place.thisMachine", &[])
    } else {
        origin.to_string()
    }
}

/// 「这台」那几句的读法：给人看的那一句 ＋ 复制详情。
pub(crate) fn said_of(p: &resident_client::feed::Problem) -> crate::detail::Said {
    if p.detail.trim().is_empty() {
        crate::detail::Said::written(p.said.clone())
    } else {
        crate::detail::Said::with_raw(p.said.clone(), &p.detail)
    }
}

/// 本机那条流交进来的东西（`local_lines` 通道上的一件）。
#[derive(Debug)]
pub(crate) enum LocalItem {
    /// 读循环分派第一层交回的有序那一件（[`route`]；别台转来的那一件是 [`Ordered::Relayed`]）。
    Frame(Ordered),
    /// 这条流结束了（两条读循环的收尾各送一次）。
    StreamEnded,
    /// 这条流上一行超长、整行丢了（说不出是哪个会话的哪一行）。
    LineLost,
}

/// 一台此刻这一条连接上收口这一头的那几样（接上时开、断了就丢）：攒批 ＋ 静默窗 ＋ 旁路快照（[`LineIntake`]）· 认不出的帧记账。
struct Conn {
    intake: LineIntake,
    /// 这条流上跳过了几帧认不出的。
    tally: crate::frame_tally::FrameTally,
    /// 解不出来的帧每种说一次（日志 ＋ 告诉用户）。
    unread: UnreadNotes,
}

impl Conn {
    fn open(origin: &crate::origin::Origin, replay: &Arc<EventReplay>) -> Conn {
        let label = origin.to_string();
        Conn {
            intake: LineIntake::open(label.clone(), replay),
            tally: crate::frame_tally::FrameTally::new(format!("stream_source {label}")),
            unread: UnreadNotes::new(label, host_label(origin)),
        }
    }
}

/// **会话内容的唯一收口**：吃 `local_lines` 通道；本机那条流 · 本机后端转来的各台那几条流（`relayed`），按那台各一份 [`LineIntake`]（本机 `<local>` 也是表里一格）。
///
/// - 本机：这条流交来第一件东西 ⇒ 本机那台「看得见」（订阅原位收 `Seen`）。
/// - 别台：里面那一行是 hello ⇒ 那台这一条连接开（[`attached`]）；`gap` ⇒ 原位一格；`ended` ⇒ 冲完、那台清单作废、订阅原位 `Unseen`（[`ended`]）；那台各条的说不清是本机后端在 `ended` 之前发的 `session_left`。
/// - 本机那条流结束（[`LocalItem::StreamEnded`]）⇒ 本机与经它转来的各台一并收（各台的连接随本机这条一起没了）；
///   下一条流换新的：本机后端先交此刻接着的各台的 hello、重新宣告每个活会话，旁路快照按续点接着拉（`snapshot_resume`）。
///
/// **会话的起停也从这条流来**：有序的那几种经 [`apply`] 交 `session_book` 与收口；流断 ⇒ 那台那一本的「清单报完了」作废（`session_book::In::Closed`），会话一条都不在这一侧改：说不清由本机后端逐条发。
/// ⚠ 本任务**绝不**等一个经本机通道的应答（快照那几问在分发器的任务里）：读循环可能正停在往本通道送东西上，
///   这里要是也等它 ⇒ 互等。
pub(crate) async fn consume(
    mut rx: tokio::sync::mpsc::Receiver<LocalItem>,
    replay: Arc<EventReplay>,
) {
    let local = crate::origin::Origin::local();
    let mut conns: HashMap<crate::origin::Origin, Conn> = HashMap::new();
    loop {
        let Some(item) = recv_or_flush(&mut conns, &mut rx).await else {
            // 发送端全没了（进程收摊）：残批照发，然后退出。
            for c in conns.values_mut() {
                c.intake.flush().await;
            }
            return;
        };
        if !matches!(item, LocalItem::StreamEnded) && !conns.contains_key(&local) {
            conns.insert(local.clone(), Conn::open(&local, &replay));
            replay.origin_seen(&local, true);
        }
        match item {
            LocalItem::Frame(Ordered::Relayed(r)) => relayed(r, &mut conns, &replay).await,
            LocalItem::Frame(f) => {
                if let Some(c) = conns.get_mut(&local) {
                    apply(&local, f, &mut c.intake, &replay).await
                }
            }
            LocalItem::LineLost => {
                if let Some(c) = conns.get_mut(&local) {
                    c.intake.lost().await
                }
            }
            LocalItem::StreamEnded => {
                let mut all: Vec<crate::origin::Origin> =
                    conns.keys().filter(|o| !o.is_local()).cloned().collect();
                all.sort();
                for o in all {
                    ended(&o, &mut conns, &replay).await;
                }
                if conns.contains_key(&local) {
                    ended(&local, &mut conns, &replay).await;
                }
                tracing::info!(
                    "本机那条流结束：本机与经它转来的各台的收口换新（下一条流重新宣告、快照按续点接着拉）"
                );
            }
        }
    }
}

/// 带静默窗地收下一件：哪一台手里攒着行，最多等 [`BATCH_QUIET`]，窗内没来新东西就把各台攒的冲掉再回去等。
/// ⚠ 超时打在 `recv` 上（cancel-safe），不打在读行上：tokio 的读行不是 cancellation-safe。
async fn recv_or_flush(
    conns: &mut HashMap<crate::origin::Origin, Conn>,
    rx: &mut tokio::sync::mpsc::Receiver<LocalItem>,
) -> Option<LocalItem> {
    loop {
        if !conns.values().any(|c| c.intake.holding()) {
            return rx.recv().await;
        }
        match tokio::time::timeout(BATCH_QUIET, rx.recv()).await {
            Ok(m) => return m,
            Err(_) => {
                for c in conns.values_mut() {
                    c.intake.flush().await;
                }
            }
        }
    }
}

/// 本机后端转来的别台那条流上的一件 ⇒ 那台那一份收口。
async fn relayed(
    r: Relayed,
    conns: &mut HashMap<crate::origin::Origin, Conn>,
    replay: &Arc<EventReplay>,
) {
    match r {
        Relayed::Line { machine, line } => {
            let origin = crate::origin::Origin::named(machine);
            if origin.is_local() {
                tracing::warn!("本机后端转来了一件说是本机的，丢掉");
                return;
            }
            // 里面那一行是 hello ⇒ 那台这一条连接开（同一台又来一行 hello ＝ 新的一条：旧的那份收口冲完换新，不说「断了」）。
            if cmd_wire::frame::kind_of(&line).ok().as_deref() == Some("hello") {
                if let Some(mut old) = conns.remove(&origin) {
                    old.intake.flush().await;
                }
                let mut c = Conn::open(&origin, replay);
                let hello = c.unread.take(&line, None, &mut c.tally);
                attached(&origin, hello, replay);
                conns.insert(origin.clone(), c);
                return;
            }
            let Some(c) = conns.get_mut(&origin) else {
                tracing::warn!(
                    "[{}] 还没接上就来了一帧，丢掉：{}",
                    origin,
                    line.chars().take(200).collect::<String>()
                );
                return;
            };
            let frame = c.unread.take(&line, None, &mut c.tally);
            if let Some(item) = frame.and_then(|f| route(&origin, None, f)) {
                apply(&origin, item, &mut c.intake, replay).await;
            }
        }
        // 那台流上一行超长、整行丢了 ⇒ 订阅原位一格 `Gap`（`to_seq` 缺）。
        Relayed::Gap { machine } => {
            if let Some(c) = conns.get_mut(&crate::origin::Origin::named(machine)) {
                c.intake.lost().await;
            }
        }
        Relayed::Ended { machine } => {
            let origin = crate::origin::Origin::named(machine);
            if conns.contains_key(&origin) {
                ended(&origin, conns, replay).await;
            }
        }
    }
}

/// 那台这一条连接接上了（里面那一行 hello）：记下它的能力事实 · 资产同步 · 书签迁移 · 报在看名单 · 订阅收 `Seen`。
fn attached(origin: &crate::origin::Origin, hello: Option<Frame>, replay: &Arc<EventReplay>) {
    let at = origin.to_string();
    let Some(Frame::Hello(h)) = hello else {
        tracing::warn!("[{at}] 那台的 hello 这一侧解不出");
        return;
    };
    tracing::info!(
        "stream_source [{at}] backend hello: build_id={} host_arch={} cmds={:?}",
        h.build_id,
        h.host_arch,
        h.commands
    );
    let offer = crate::inbound_client::reached(origin, &h);
    // 连上那一刻：让本机常驻后端沿池里那条 SSH 同步资产目录（后台跑，零判定）。
    crate::asset_sync::on_remote_ready(&at, offer.admits(crate::asset_sync::REMOTE_NEEDS));
    // 〔一次性迁移 · 下一版删〕桌面旧书签文件里这台那一格交给这台后端。
    crate::bookmarks_migration::after_ready(origin.clone());
    // 那台的在看名单经本机后端交那台的名单台（这条连接上整份报一次）。
    if let Some(client) = crate::inbound_client::local_client() {
        crate::stream_watch::desk(origin).up(&client, None);
    }
    // 订了这台会话流的那些订阅原位收一格 `Seen`。
    replay.origin_seen(origin, true);
}

/// 那台这一条连接断了：残批先冲完（行先落）、摘它的能力事实、那一本的清单作废、订阅原位收 `Unseen`（不是终点）。
async fn ended(
    origin: &crate::origin::Origin,
    conns: &mut HashMap<crate::origin::Origin, Conn>,
    replay: &Arc<EventReplay>,
) {
    if let Some(mut c) = conns.remove(origin) {
        c.intake.flush().await;
    }
    let at = origin.to_string();
    if !origin.is_local() {
        crate::inbound_client::gone(origin);
    }
    // 连接断了 ≠ 会话死了：说不清由本机后端逐条发（`session_left`，已经折进账里）；这里只作废「清单报完了」，下一条连接自己重报。
    crate::session_book::feed(BookIn::Closed { origin: at.clone() });
    replay.origin_seen(origin, false);
    tracing::info!("stream_source [{at}] 那一条连接结束 ⇒ 收口换新、这台的清单作废");
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/relayed_intake_tests.rs"]
mod relayed_intake_tests;
