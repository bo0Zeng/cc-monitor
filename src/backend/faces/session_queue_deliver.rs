//! 会话输入队的**投递**：主人这台一条线程，一条会话一次送一条，到「能送」才送（队本体与写口在 `history/session_queue.rs`）。
//!
//! - **起**：占着常驻的那一个进程起一份（[`start`]，经 `stream::inbound::deliver_queues`）；一台只有它一个投递的，不会两边各送一次。
//! - **醒**：活会话表发布一份（[`crate::history::live_table::on_publish`] 那一声）· 队动了（本进程投进一条 · watcher 看见别的进程写了队目录，
//!   都经 [`crate::history::session_queue::moved`]）·
//!   有等着的期限（送出后等那一轮起来 [`CONFIRM_MS`] · 收着的等够了 · 送那一下回了「等」之后再试 [`WAITING_RETRY_MS`]）到点。别的时候一直睡。
//! - **判**（[`step`]，纯函数）：不看会话在不在跑、也不分来源（人发的 · cc-bus 投的 · 斜杠命令 · 计划审面退回一样），agent 自己处理插进来的话。
//!   - 不活：这一段队里有东西时看见它活过 ⇒ 会话结束，没送的标未送达；没看见过 ⇒ 送（送字本体恢复它，那一句当第一句）。
//!   - 屏上正有批准框 · 问答框（在等人）⇒ 压着：这时送字带回车会被当成对框的回答、可能误批；框答完就送。
//!   - 别的（闲着 · 在跑 · 后台在跑 · 说不清）⇒ 送。一次一条：上一条送出之后那一态没换过（那一态起始不晚于送出那一刻）、又没过
//!     [`CONFIRM_MS`] ⇒ 下一条先等，免得几句粘成一行。
//!   - 编辑已发的那一句、选了「等这一轮完」⇒ 在跑时不送（回退要走的那几步不插在一轮中间）；选「现在停」照送（停下这一轮是回退那一串的第一步，
//!     [`crate::control::rewind`]）。
//!   - 那台离线不归这里：发件端留着待发，接上之后照同一个编号补交。
//! - **送**：照送字本体（`faces/session_send.rs`：同一道身份门 · 只剩 shell 不送 · 终端没了按记下的号恢复）。结局：送到 · 不知道送没送到 ⇒ 摘掉
//!   （不重送）；送那一下回「等」（框开着 · 回退那一串要等）⇒ 照排着，最晚 [`WAITING_RETRY_MS`] 再试；别的被拒 ⇒ 那一条标未送达（留给发件人看、撤；旧版托管进程不认「说一句」⇒ 那一条写明要重开那个终端）；命令级错 ⇒ 照排着，下一次醒再试。
//!   编辑已发的那一句（带 `rewind`）⇒ 先在会话里退到那一句之前再说它（[`super::session_rewind_face::walk_here`]）：成了 ⇒ 摘掉；
//!   等 ⇒ 照排着；没退成 · 退了没送进 ⇒ 标未送达、那一条写上为什么；送进了、记录里还没读到 ⇒ 摘掉（不重送）。
//! - 终端输入行里有没有没提交的字：投递不看。

use super::session_send::Delivery;
use crate::agents::SessionActivity;
use crate::common::said::Said;
use crate::history::session_queue as q;
use crate::platform::fs::Own;
use crate::wire::typed::Files;
use cmd_wire::queue::QueueItem;
use std::collections::BTreeMap;

/// 送出一条之后，那一态一直没换（那一句没起一轮：`/clear` 一类）最多等多久就送下一条（毫秒，这台的钟）。
pub(crate) const CONFIRM_MS: i64 = 15_000;

/// 送那一下回「等」（框开着 · 回退那一串要等）之后，最晚多久再试一次（毫秒）：等的那一样了结时不一定有事件来，
/// 不到点再试那一条就一直排着。
pub(crate) const WAITING_RETRY_MS: i64 = 5_000;

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
    /// 上一回送那一下回了「等」：最晚这一刻再试（[`WAITING_RETRY_MS`]）；之后送成 · 被拒 · 队空了 ⇒ 清掉。
    retry_ms: Option<i64>,
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
pub(crate) fn step(look: &Look, memo: &Memo, item: &QueueItem, now_ms: i64) -> Step {
    let waiting_turn = memo.sent_ms.is_some_and(|t| !settled(look, t, now_ms));
    match look.doing {
        None if memo.live => Step::End,
        None => {
            if waiting_turn {
                Step::Wait
            } else {
                Step::Send
            }
        }
        Some(Some(SessionActivity::NeedsYou)) => Step::Wait,
        Some(Some(SessionActivity::Working)) if after_turn(item) => Step::Wait,
        Some(_) if waiting_turn => Step::Wait,
        Some(_) => Step::Send,
    }
}

/// 投递那一遍要的：队在哪个家 · 一条会话此刻的样子 · 送一条。
pub(crate) struct Deps<'a> {
    pub(crate) own: &'a Own,
    pub(crate) look: &'a dyn Fn(&str) -> Look,
    pub(crate) send: &'a dyn Fn(&str, &QueueItem) -> Result<Delivery, Said>,
}

/// 编辑已发的那一句、选了等这一轮完：在跑时不送（回退要走的那几步不能插在一轮中间；选「现在停」的照送）。
fn after_turn(item: &QueueItem) -> bool {
    item.rewind
        .as_ref()
        .is_some_and(|r| r.when == cmd_wire::rewind::RewindWhen::AfterTurn)
}

/// 送的结局怎么落：摘掉 · 照排着 · 标未送达。
fn landed(d: &Delivery) -> Option<bool> {
    match d {
        Delivery::In { .. } => Some(true),
        Delivery::Waiting => None,
        Delivery::No(_) => Some(false),
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
                tracing::warn!(
                    "[session-queue] {sid} 读不出队：{} {}",
                    f.code_or_fallback(),
                    f.said
                );
                continue;
            }
        };
        if let Some(t) = due.held_until {
            soon(t);
        }
        let memo = memos.entry(sid.clone()).or_default();
        let look = (deps.look)(sid);
        // 名单那一句跟着屏上开没开着框换（排着的说需手动 · 答完再送）：变了才写盘、推名单。投递判定不看它。
        if let Err(f) = q::mark_box(
            deps.own,
            sid,
            look.doing == Some(Some(SessionActivity::NeedsYou)),
        ) {
            tracing::warn!(
                "[session-queue] {sid} 记开框没成：{} {}",
                f.code_or_fallback(),
                f.said
            );
        }
        if memo.sent_ms.is_some_and(|t| settled(&look, t, now_ms)) {
            memo.sent_ms = None;
        }
        // 到了再试那一刻 ⇒ 这一遍就是那一次再试（照判送不送）；又回「等」会再记一个。不清 ⇒ 过了点的期限一遍遍立刻叫醒。
        if memo.retry_ms.is_some_and(|t| t <= now_ms) {
            memo.retry_ms = None;
        }
        let Some(item) = due.item else {
            memo.live = false;
            memo.retry_ms = None;
            continue;
        };
        match step(&look, memo, &item, now_ms) {
            Step::Wait => {}
            Step::End => {
                if let Err(f) = q::end(deps.own, sid, now_ms) {
                    tracing::warn!(
                        "[session-queue] {sid} 标未送达没成：{} {}",
                        f.code_or_fallback(),
                        f.said
                    );
                }
                memo.live = false;
                continue;
            }
            Step::Send => match (deps.send)(sid, &item) {
                Ok(sent) => match landed(&sent) {
                    None => {
                        memo.retry_ms = Some(now_ms.saturating_add(WAITING_RETRY_MS));
                    }
                    Some(true) => {
                        memo.retry_ms = None;
                        noted(q::delivered(deps.own, sid, &item.id), sid, After::Delivered);
                        memo.sent_ms = Some(now_ms);
                    }
                    Some(false) => {
                        memo.retry_ms = None;
                        tracing::warn!(
                            "[session-queue] {sid} 那一条送不进去（{:?}），标未送达",
                            sent
                        );
                        use super::session_send::Undeliverable as U;
                        use crate::control::terminals::Refused;
                        let why = match &sent {
                            Delivery::No(U::NotRewound(s)) => Some(s.clone()),
                            // 旧托管进程：笼统的「未送进会话」说不出该做什么，写明要重开那个终端。
                            Delivery::No(U::Refused(s)) if s.why == Some(Refused::HostOutdated) => {
                                s.said.clone()
                            }
                            _ => None,
                        };
                        noted(q::give_up(deps.own, sid, &item.id, why), sid, After::GaveUp);
                    }
                },
                Err(f) => tracing::warn!(
                    "[session-queue] {sid} 送没成，下次再试：{} {}",
                    f.code_or_fallback(),
                    f.said
                ),
            },
        }
        if look.doing.is_some() {
            memo.live = true;
        }
        if let Some(t) = memo.sent_ms {
            soon(t.saturating_add(CONFIRM_MS));
        }
        if let Some(t) = memo.retry_ms {
            soon(t);
        }
    }
    wake
}

/// 送完之后改队那一下没成（那一条已不在排着 · 写不进）：只留一行日志，下一遍照盘上此刻的样子再判。
fn noted(r: Result<bool, Said>, sid: &str, what: After) {
    match r {
        Ok(true) => {}
        Ok(false) => tracing::info!("[session-queue] {sid} {what:?}：那一条已不在排着（撤了）"),
        Err(f) => tracing::warn!(
            "[session-queue] {sid} {what:?} 没成：{} {}",
            f.code_or_fallback(),
            f.said
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
pub fn start(own: Own, files: Files) {
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
                let now = crate::common::time::now_ms() as i64;
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
#[expect(
    clippy::disallowed_methods,
    reason = "会话输入队投递那两样到点就要再看：送出一句没起一轮（`/clear` 一类）时没有事件来，不到点再看整条队就卡死；收着的那一条等够了要排进去"
)]
fn sleep_until(rx: &std::sync::mpsc::Receiver<()>, wake: Option<i64>) -> bool {
    use std::sync::mpsc::RecvTimeoutError;
    let Some(at) = wake else {
        return rx.recv().is_ok();
    };
    let ms = at
        .saturating_sub(crate::common::time::now_ms() as i64)
        .max(0);
    let wait = std::time::Duration::from_millis(u64::try_from(ms).unwrap_or(0));
    !matches!(rx.recv_timeout(wait), Err(RecvTimeoutError::Disconnected))
}

/// 生产那一份送一条：照送字本体（[`super::session_send`]：在等人不送 · 只剩 shell 不送 · 终端没了按记下的号恢复、那一句当第一句 ·
/// 附件照那一家的写法拼）。发自哪一端是合形状的前端名 ⇒ 当这次请求自报的前端过身份门；不是 ⇒ 不自报（照名字规则）。
fn send_here(sid: &str, item: &QueueItem, files: Files) -> Result<Delivery, Said> {
    let sid: cmd_wire::Sid = sid.parse().map_err(|e: cmd_wire::sid::NotASid| {
        crate::wire::spec::fail(
            "bad_args",
            crate::common::contract::malformed(&e.to_string()),
        )
    })?;
    let client = crate::control::gate_rules::client_name_ok(&item.end).then(|| item.end.clone());
    if let Some(r) = &item.rewind {
        return super::session_rewind_face::walk_here(sid.as_str(), r, &item.text, client)
            .map(rewound);
    }
    super::session_send::here(&say_of(sid, item, client), files)
}

/// 回退那一串的结局 ⇒ 投递的结局（见头注）。
pub(crate) fn rewound(w: crate::control::rewind::Walked) -> Delivery {
    use super::session_send::Undeliverable;
    use crate::control::rewind::{Walked, Why};
    match w {
        Walked::Done | Walked::Unsure(Why::NotReadBack) => Delivery::In { resumed: false },
        Walked::Wait => Delivery::Waiting,
        Walked::No(_) | Walked::Unsure(_) => {
            Delivery::No(Undeliverable::NotRewound(w.said().unwrap_or_default()))
        }
    }
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
