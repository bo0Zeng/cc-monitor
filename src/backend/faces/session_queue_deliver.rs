//! 会话输入队的**投递**：主人这台一条线程，一条会话一次送一条，到「能送」才送（队本体与写口在 `history/session_queue.rs`）。
//!
//! - **起**：占着常驻的那一个进程起一份（[`start`]，经 `stream::inbound::deliver_queues`）；一台只有它一个投递的，不会两边各送一次。
//! - **醒**：活会话表发布一份（[`crate::history::live_table::on_publish`] 那一声）· 队动了（本进程投进一条 · watcher 看见别的进程写了队目录，
//!   都经 [`crate::history::session_queue::moved`]）·
//!   有等着的期限（送出后等那一轮起来 [`CONFIRM_MS`] · 收着的等够了 · 送那一下回了「等」之后再试 [`WAITING_RETRY_MS`]）到点。别的时候一直睡。
//! - **判**：排在最前那一条此刻送不送是纯函数 [`q::step`]（住队旁 `history/session_queue.rs`，判法写在它头上），agent 自己处理插进来的话。
//! - **送**：先在队那一把锁里把那一条标成在送（[`q::take`]：名单不列、撤不了；那一刻已被撤 ⇒ 不送；进程在送时没了 ⇒ 下次起来当已送），再照送字本体（`faces/session_send.rs`：同一道身份门 · 只剩 shell 不送 · 终端没了按记下的号恢复）。结局：送到 · 不知道送没送到 ⇒ 摘掉
//!   （不重送）；送那一下回「等」（框开着 · 回退那一串要等）⇒ 照排着，最晚 [`WAITING_RETRY_MS`] 再试；别的被拒 ⇒ 那一条标未送达（留给发件人看、撤；旧版托管进程不认「说一句」⇒ 那一条写明要重开那个终端）；命令级错 ⇒ 照排着，下一次醒再试。
//!   编辑已发的那一句（带 `rewind`）⇒ 先在会话里退到那一句之前再说它（[`super::session_rewind_face::walk_here`]）：成了 ⇒ 摘掉；
//!   等（屏上在出字 · 选的是等这一轮完）⇒ 照排着，名单那一条记下在等这一轮（[`q::mark_turn`]）；没退成 · 退了没送进 ⇒ 标未送达、那一条写上为什么；送进了、记录里还没读到 ⇒ 摘掉（不重送）。
//! - 终端输入行里有没有没提交的字：投递不看。
//!
//! # 为什么住 faces
//!
//! 送那一下要两边的东西：送字本体（`session_send`）走 control 的送键 · 恢复，恢复那一支经 `session_open_face::resume_of` 要 observe 的定位
//! （`observe::query::locate`）；编辑已发的那一句走 [`super::session_rewind_face::walk_here`]（observe 的定位 · 记录里读回那几行）。control 与 observe 互不引用（ARCHITECTURE §2.3），
//! 挪进 control 只能成环或另开注入口；faces 是唯一同时看得见两边的接线点。不碰盘、不送字的纯判定（[`q::step`] · [`q::Look`] · [`q::Memo`]）住队旁。

use super::session_send::Delivery;
use crate::agents::SessionActivity;
use crate::common::said::Said;
use crate::history::session_queue as q;
use crate::history::session_queue::{
    settled, step, Look, Memos, Step, CONFIRM_MS, WAITING_RETRY_MS,
};
use crate::platform::fs::Own;
use crate::wire::typed::Files;
use cmd_wire::queue::QueueItem;

/// 投递那一遍要的：队在哪个家 · 一条会话此刻的样子 · 送一条。
pub(crate) struct Deps<'a> {
    pub(crate) own: &'a Own,
    pub(crate) look: &'a dyn Fn(&str) -> Look,
    pub(crate) send: &'a dyn Fn(&str, &QueueItem) -> Result<Delivery, Said>,
}

/// 送那一下回的命令级错里「请求本身不合」那一个码：同一句再送也一样被拒，不重试。
const MALFORMED: &str = "bad_args";

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
                    &cmd_wire::failed::Failed::code_or_fallback(f.code.clone()),
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
                &cmd_wire::failed::Failed::code_or_fallback(f.code.clone()),
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
        match step(&look, memo, now_ms) {
            Step::Wait => {}
            Step::End => {
                if let Err(f) = q::end(deps.own, sid, now_ms) {
                    tracing::warn!(
                        "[session-queue] {sid} 标未送达没成：{} {}",
                        &cmd_wire::failed::Failed::code_or_fallback(f.code.clone()),
                        f.said
                    );
                }
                memo.live = false;
                continue;
            }
            // 认定要送：先在队那一把锁里把它标成在送（名单不再列它、撤不了）；那一刻已被撤 ⇒ 不送。
            Step::Send => match q::take(deps.own, sid, &item.id) {
                Ok(true) => send_one(deps, memo, sid, &item, now_ms),
                Ok(false) => {
                    tracing::info!("[session-queue] {sid} 要送的那一条已不在排着（撤了），不送")
                }
                Err(f) => tracing::warn!(
                    "[session-queue] {sid} 标在送没成，下次再试：{} {}",
                    &cmd_wire::failed::Failed::code_or_fallback(f.code.clone()),
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

/// 送已标成在送的那一条、照结局落：送到 · 不知道 ⇒ 摘掉；等 · 命令级错 ⇒ 收回成排着；送不进 ⇒ 照它那一句标未送达。
fn send_one(deps: &Deps<'_>, memo: &mut q::Memo, sid: &str, item: &QueueItem, now_ms: i64) {
    match (deps.send)(sid, item) {
        Ok(Delivery::Waiting) => {
            memo.retry_ms = Some(now_ms.saturating_add(WAITING_RETRY_MS));
            untaken(deps, sid, item);
            // 编辑已发的那一句：回退那一串读屏见在出字 ⇒ 名单照实说在等这一轮（久了改说久未结束）。走到这里 step 已判过没开框；
            // 两次读表之间恰好开了框 ⇒ 名单那一句照框说（`boxed` 先于在等这一轮），框合上下一遍照屏再判。
            if item.rewind.is_some() {
                if let Err(f) = q::mark_turn(deps.own, sid, &item.id, now_ms) {
                    tracing::warn!(
                        "[session-queue] {sid} 记在等这一轮没成：{} {}",
                        &cmd_wire::failed::Failed::code_or_fallback(f.code.clone()),
                        f.said
                    );
                }
            }
        }
        Ok(Delivery::In { .. }) => {
            memo.retry_ms = None;
            noted(q::delivered(deps.own, sid, &item.id), sid, After::Delivered);
            memo.sent_ms = Some(now_ms);
        }
        Ok(Delivery::No(no)) => {
            memo.retry_ms = None;
            tracing::warn!("[session-queue] {sid} 那一条送不进去（{no:?}），标未送达");
            noted(
                q::give_up(deps.own, sid, &item.id, no.said()),
                sid,
                After::GaveUp,
            );
        }
        // 请求本身不合（`payload` 超长这一类）：同一句每次必错 ⇒ 照那一句标未送达，不再重试。
        Err(f) if f.code.as_deref() == Some(MALFORMED) => {
            memo.retry_ms = None;
            tracing::warn!(
                "[session-queue] {sid} 那一条每次都会被拒（{MALFORMED} {}），标未送达",
                f.said
            );
            let why = copy_core::copy_text("beQueue.no.failed", &[("why", &f.said)]);
            noted(q::give_up(deps.own, sid, &item.id, why), sid, After::GaveUp);
        }
        Err(f) => {
            tracing::warn!(
                "[session-queue] {sid} 送没成，下次再试：{} {}",
                &cmd_wire::failed::Failed::code_or_fallback(f.code.clone()),
                f.said
            );
            untaken(deps, sid, item);
        }
    }
}

/// 交出去的那一条收回成排着（送那一下回「等」· 命令级错）。
fn untaken(deps: &Deps<'_>, sid: &str, item: &QueueItem) {
    if let Err(f) = q::untake(deps.own, sid, &item.id) {
        tracing::warn!(
            "[session-queue] {sid} 收回成排着没成：{} {}",
            &cmd_wire::failed::Failed::code_or_fallback(f.code.clone()),
            f.said
        );
    }
}

/// 送完之后改队那一下没成（那一条已不在排着 · 写不进）：只留一行日志，下一遍照盘上此刻的样子再判。
fn noted(r: Result<bool, Said>, sid: &str, what: After) {
    match r {
        Ok(true) => {}
        Ok(false) => tracing::info!("[session-queue] {sid} {what:?}：那一条已不在排着（撤了）"),
        Err(f) => tracing::warn!(
            "[session-queue] {sid} {what:?} 没成：{} {}",
            &cmd_wire::failed::Failed::code_or_fallback(f.code.clone()),
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
            let home = crate::agents::local_homes();
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
            // 上一个投递进程在送的那一刻没了：盘上留着在送的当已送（不知道送没送到 ⇒ 不重送）。
            for sid in q::sids(&own) {
                if let Err(f) = q::settle_sending(&own, &sid) {
                    tracing::warn!(
                        "[session-queue] {sid} 收拾上次在送的没成：{} {}",
                        &cmd_wire::failed::Failed::code_or_fallback(f.code.clone()),
                        f.said
                    );
                }
            }
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
            Delivery::No(Undeliverable::NotRewound(w.said().unwrap_or_else(|| {
                copy_core::copy_text("beQueue.no.notRewound", &[])
            })))
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
