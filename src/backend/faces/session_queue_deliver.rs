//! 会话输入队的**投递**：主人这台一条线程，一条会话一次送一条，到「能送」才送（队本体与写口在 `history/session_queue.rs`）。
//!
//! - **起**：占着常驻的那一个进程起一份（[`start`]，经 `stream::inbound::deliver_queues`）；一台只有它一个投递的，不会两边各送一次。
//! - **醒**：活会话表发布一份（[`crate::history::live_table::on_publish`] 那一声）· 队动了（本进程投进一条 · watcher 看见别的进程写了队目录，
//!   都经 [`crate::history::session_queue::moved`]）·
//!   有等着的期限（送出后等那一轮起来 [`CONFIRM_MS`] · 收着的等够了）到点。别的时候一直睡。
//! - **判**（[`step`]，纯函数）：
//!   - 不活：这一段队里有东西时看见它活过 ⇒ 会话结束，没送的标未送达；没看见过 ⇒ 送（送字本体恢复它，那一句当第一句）。
//!   - 闲着 · 一轮停了、后台在跑 ⇒ 送；但上一条送出之后那一态没换过（那一态起始不晚于送出那一刻）、又没过 [`CONFIRM_MS`] ⇒ 等，
//!     那一轮还没起来（不往 agent 自己的排队里塞：那一队看不见也撤不了）。
//!   - 在跑 · 在等人（批准 · 回答框开着，打的字会落进框里）· 说不清 ⇒ 等。
//! - **送**：照送字本体（`faces/session_send.rs`：同一道身份门 · 只剩 shell 不送 · 终端没了按记下的号恢复）。结局：送到 · 不知道送没送到 ⇒ 摘掉
//!   （不重送）；在等人 ⇒ 照排着；别的被拒 ⇒ 那一条标未送达（留给发件人看、撤）；命令级错 ⇒ 照排着，下一次醒再试。
//! - 终端输入行里有没提交的字，tmux 判不出：这一道等托管那一形给事实；此刻不等。

use crate::agents::SessionActivity;
use crate::control::terminals_wire::{Sent, SentResult};
use crate::history::session_queue as q;
use cmd_wire::queue::QueueItem;
use crate::platform::fs::Own;
use crate::wire::spec::Fail;
use crate::wire::typed::Files;
use cmd_wire::session::Refused;
use std::collections::BTreeMap;

/// 送出一条之后，那一态一直没换（那一句没起一轮：`/clear` 一类）最多等多久就送下一条（毫秒，这台的钟）。
pub(crate) const CONFIRM_MS: i64 = 15_000;

/// 一条会话此刻的样子（活会话表那一份）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Look {
    /// 活着 ⇒ `Some(那一态)`（说不清 ⇒ `Some(None)`）；不活 ⇒ `None`。
    pub(crate) doing: Option<Option<SessionActivity>>,
    /// 那一态从何时起（epoch ms，这台的钟）。
    pub(crate) since_ms: Option<u64>,
}

/// 投递线程替一条会话记着的（不落盘：重启之后从头看）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Memo {
    /// 上一条送出的时刻（等那一轮起来）；确认过 · 过了期限 ⇒ 清掉。
    sent_ms: Option<i64>,
    /// 这一段队里有东西时看见它活过（队空了就清掉）。
    live: bool,
}

/// 每条会话一份 [`Memo`]。
pub(crate) type Memos = BTreeMap<String, Memo>;

/// 这一步做什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Send,
    Wait,
    End,
}

/// 上一条送出之后那一轮起来过没有：那一态换过（起始晚于送出那一刻）· 过了期限 ⇒ 算过了。
fn settled(look: &Look, sent_ms: i64, now_ms: i64) -> bool {
    let moved = look
        .since_ms
        .and_then(|s| i64::try_from(s).ok())
        .is_some_and(|s| s > sent_ms);
    moved || now_ms >= sent_ms.saturating_add(CONFIRM_MS)
}

/// 排在最前那一条此刻送不送（见头注）。
pub(crate) fn step(look: &Look, memo: &Memo, now_ms: i64) -> Step {
    let waiting_turn = memo.sent_ms.is_some_and(|t| !settled(look, t, now_ms));
    match look.doing {
        None if memo.live => Step::End,
        None if waiting_turn => Step::Wait,
        None => Step::Send,
        Some(Some(SessionActivity::Idle | SessionActivity::BackgroundWork)) if !waiting_turn => {
            Step::Send
        }
        Some(_) => Step::Wait,
    }
}

/// 投递那一遍要的：队在哪个家 · 一条会话此刻的样子 · 送一条。
pub(crate) struct Deps<'a> {
    pub(crate) own: &'a Own,
    pub(crate) look: &'a dyn Fn(&str) -> Look,
    pub(crate) send: &'a dyn Fn(&str, &QueueItem) -> Result<Sent, Fail>,
}

/// 送的结局怎么落：摘掉 · 照排着 · 标未送达。
fn landed(sent: &Sent) -> Option<bool> {
    match sent.result {
        SentResult::Delivered | SentResult::Unsure => Some(true),
        SentResult::Refused if sent.why == Some(Refused::Waiting) => None,
        SentResult::Refused => Some(false),
    }
}

/// 走一遍这台有队的每条会话，各判一步、照做；回下一次该醒的时刻（没有要等到点的 ⇒ `None`，一直睡到有事）。
pub(crate) fn pass(deps: &Deps<'_>, memos: &mut Memos, now_ms: i64) -> Option<i64> {
    let mut wake: Option<i64> = None;
    let mut soon = |t: i64| wake = Some(wake.map_or(t, |w| w.min(t)));
    let sids = q::sids(deps.own);
    memos.retain(|sid, m| sids.contains(sid) || m.sent_ms.is_some());
    for sid in &sids {
        let due = match q::due(deps.own, sid, now_ms) {
            Ok(d) => d,
            Err(f) => {
                tracing::warn!("[session-queue] {sid} 读不出队：{} {}", f.code, f.message);
                continue;
            }
        };
        if let Some(t) = due.held_until {
            soon(t);
        }
        let memo = memos.entry(sid.clone()).or_default();
        let look = (deps.look)(sid);
        if memo.sent_ms.is_some_and(|t| settled(&look, t, now_ms)) {
            memo.sent_ms = None;
        }
        let Some(item) = due.item else {
            memo.live = false;
            continue;
        };
        match step(&look, memo, now_ms) {
            Step::Wait => {}
            Step::End => {
                if let Err(f) = q::end(deps.own, sid, now_ms) {
                    tracing::warn!(
                        "[session-queue] {sid} 标未送达没成：{} {}",
                        f.code,
                        f.message
                    );
                }
                memo.live = false;
                continue;
            }
            Step::Send => match (deps.send)(sid, &item) {
                Ok(sent) => match landed(&sent) {
                    Some(true) => {
                        noted(q::delivered(deps.own, sid, &item.id), sid, After::Delivered);
                        memo.sent_ms = Some(now_ms);
                    }
                    Some(false) => {
                        tracing::warn!(
                            "[session-queue] {sid} 那一条送不进去（{:?}），标未送达",
                            sent.why
                        );
                        noted(q::give_up(deps.own, sid, &item.id), sid, After::GaveUp);
                    }
                    None => {}
                },
                Err(f) => tracing::warn!(
                    "[session-queue] {sid} 送没成，下次再试：{} {}",
                    f.code,
                    f.message
                ),
            },
        }
        if look.doing.is_some() {
            memo.live = true;
        }
        if let Some(t) = memo.sent_ms {
            soon(t.saturating_add(CONFIRM_MS));
        }
    }
    wake
}

/// 送完之后改队那一下没成（那一条已不在排着 · 写不进）：只留一行日志，下一遍照盘上此刻的样子再判。
fn noted(r: Result<bool, Fail>, sid: &str, what: After) {
    match r {
        Ok(true) => {}
        Ok(false) => tracing::info!("[session-queue] {sid} {what:?}：那一条已不在排着（撤了）"),
        Err(f) => tracing::warn!(
            "[session-queue] {sid} {what:?} 没成：{} {}",
            f.code,
            f.message
        ),
    }
}

/// 送完之后改队的那一下（只进日志）。
#[derive(Debug, Clone, Copy)]
enum After {
    /// 摘掉送到的那一条。
    Delivered,
    /// 标未送达。
    GaveUp,
}

/// 叫醒投递线程的那一头（没起 ⇒ 没有）。
static BELL: std::sync::OnceLock<std::sync::mpsc::Sender<()>> = std::sync::OnceLock::new();

/// 叫一声投递线程（活会话表发布一份 · 队动了；没起投递线程的进程里什么都不做）。
fn kick() {
    if let Some(tx) = BELL.get() {
        if tx.send(()).is_err() {
            tracing::warn!("[session-queue] 投递线程已经收了，这一声没人听");
        }
    }
}

/// 起这台的投递线程（一个进程一份；再调不再起）。`files`：恢复那一支起之前预标信任经的那扇门。
pub(crate) fn start(own: Own, files: Files) {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    if BELL.set(tx).is_err() {
        return;
    }
    crate::history::live_table::on_publish(kick);
    q::on_moved(kick);
    let spawned = std::thread::Builder::new()
        .name("session-queue-deliver".into())
        .spawn(move || {
            let home = crate::history::listing::agent_home();
            let look = |sid: &str| {
                let t = crate::history::live_table::now(&home);
                Look {
                    doing: t.doing_one(sid),
                    since_ms: t.since_of(sid),
                }
            };
            let send = |sid: &str, item: &QueueItem| send_here(sid, item, files);
            let deps = Deps {
                own: &own,
                look: &look,
                send: &send,
            };
            let mut memos = Memos::default();
            loop {
                let now = crate::common::time::now_ms();
                let wake = pass(&deps, &mut memos, now);
                if !sleep_until(&rx, wake) {
                    return;
                }
            }
        });
    if let Err(e) = spawned {
        tracing::error!("[session-queue] 投递线程起不来：{e}");
    }
}

/// 睡到被叫醒，或到 `wake` 那一刻（有的话）。叫醒的那一头全没了 ⇒ `false`（线程收）。
#[allow(
    clippy::disallowed_methods,
    reason = "会话输入队投递那两样到点就要再看：送出一句没起一轮（`/clear` 一类）时没有事件来，不到点再看整条队就卡死；收着的那一条等够了要排进去"
)]
fn sleep_until(rx: &std::sync::mpsc::Receiver<()>, wake: Option<i64>) -> bool {
    use std::sync::mpsc::RecvTimeoutError;
    let Some(at) = wake else {
        return rx.recv().is_ok();
    };
    let ms = at.saturating_sub(crate::common::time::now_ms()).max(0);
    let wait = std::time::Duration::from_millis(u64::try_from(ms).unwrap_or(0));
    !matches!(rx.recv_timeout(wait), Err(RecvTimeoutError::Disconnected))
}

/// 生产那一份送一条：照送字本体（[`super::session_send`]：在等人不送 · 只剩 shell 不送 · 终端没了按记下的号恢复、那一句当第一句 ·
/// 附件照那一家的写法拼）。发自哪一端是合形状的前端名 ⇒ 当这次请求自报的前端过身份门；不是 ⇒ 不自报（照名字规则）。
fn send_here(sid: &str, item: &QueueItem, files: Files) -> Result<Sent, Fail> {
    let sid: cmd_wire::Sid = sid.parse().map_err(|e: cmd_wire::sid::NotASid| {
        Fail::new(
            "bad_args",
            crate::common::contract::malformed(&e.to_string()),
        )
    })?;
    let client = crate::control::gate_rules::client_name_ok(&item.end).then(|| item.end.clone());
    super::session_send::here(&say_of(sid, item, client), files)
}

/// 队里那一条 ⇒ 送进去的那一句（字 · 附件原样带上）。
pub(crate) fn say_of(
    sid: cmd_wire::Sid,
    item: &QueueItem,
    client: Option<String>,
) -> super::session_send::Say {
    super::session_send::Say {
        sid,
        text: item.text.clone(),
        client,
        attachments: item.attachments.clone(),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/history/session_queue_deliver_tests.rs"]
mod tests;
