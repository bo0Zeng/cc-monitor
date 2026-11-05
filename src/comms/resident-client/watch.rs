//! **每台一张名单台（[`Desk`]）**：宿主只交「现在在看哪几条会话」（整份换），报给那台、新连接重报、应答 `from[]` 交出去都在这里，全仓只这一处。
//!
//! - 新连接：attach 行带上此刻的名单（[`Desk::ask_for_attach`]），那台从第一帧起照它发；接上之后（[`Desk::up`]）带过的那份与此刻的不一样
//!   （握手中变了 · 这条载体没有 attach 行）⇒ 当场在它上面整份报一次（`stream-watch`）。
//! - 名单变了 ⇒ 在当前连接上整份报；一台只一个发送任务，手里只留最新一份，同一台按序报、不后发先到；报没成不重试，下一条连接照样带上。
//! - 应答 `from[]`（刚进名单的会话从第几行起上流）原样交宿主，由看着那一条的去补 `[已有, seq)`。
//!
//! 名单从空开始（一条都还没人看 ⇒ 那台一条都不发）。宿主经 `held` 交回手上各份主线外清单有什么，那台只补缺的（桌面照手上那份交；手机不留那一份、每条都交 `at: 0`，那台发整份）。
//! 手上留着记录的会话（`kept`，桌面的重放留存）随名单一起交、一起报：那台不在说的它答一句已结束；不留的（手机）交空的。

use crate::client::InboundClient;
use cmd_wire::watch::{BranchHeld, WatchAsk, WatchFrom, WatchReply};
use copy_core::discard::{Discard, Why};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::sync::Notify;

/// 那台后端的命令名。
pub const WATCH_CMD: &str = "stream-watch";

/// 一次报名单的期限：后端在连接内就地做完（只是换一张表、数几个游标），几秒足够；超了就等下一次变化 / 下一条连接。
#[expect(
    clippy::disallowed_methods,
    reason = "报一次在看名单的期限：客户端自己定的数，管 `stream-watch` 那一问"
)]
pub const WATCH_BUDGET: Duration = Duration::from_secs(10);

type HeldFn = Box<dyn Fn(&BTreeSet<String>) -> Vec<BranchHeld> + Send + Sync>;
type FromFn = Box<dyn Fn(Vec<WatchFrom>) + Send + Sync>;

/// 一台的名单台。
pub struct Desk {
    /// 报给谁：`None` ＝ 连接通往的那个后端自己；`Some(那台)` ＝ 经连接通往的后端转给它手上那一台（`call_at`：桌面问别台都经本机后端）。
    at: Option<String>,
    now: Mutex<Now>,
    /// 名单里各条手上那份主线外清单有什么（读在报的那一刻）。
    held: HeldFn,
    /// 应答里的 `from[]` 交给谁（空的不交）。
    from: FromFn,
    /// 叫醒发送任务（名单变了 · 换了一条连接 · 台没了）。
    poke: Arc<Notify>,
}

#[derive(Default)]
struct Now {
    /// 此刻在看的 ＋ 手上留着的。
    watched: BTreeSet<String>,
    kept: BTreeSet<String>,
    /// 当前那条连接（弱引用：连接走了就不报）。
    link: Weak<InboundClient>,
    /// 那条连接已经知道的那一份（attach 行带过的 · 报过的：在看的 ＋ 手上留着的）；`None` ＝ 它还什么都不知道（全发）。
    told: Option<(BTreeSet<String>, BTreeSet<String>)>,
    /// 等「这一份报上去了」的那几位（[`Desk::set_and_hear`]）：报完交那一趟的 `from[]`；没有连接 · 已经知道 ⇒ 交空的。
    waiting: Vec<tokio::sync::oneshot::Sender<Vec<WatchFrom>>>,
}

impl Desk {
    /// 起一张（在当前 tokio 运行时上起它的发送任务；台没了任务跟着收）。
    pub fn new(
        held: impl Fn(&BTreeSet<String>) -> Vec<BranchHeld> + Send + Sync + 'static,
        from: impl Fn(Vec<WatchFrom>) + Send + Sync + 'static,
    ) -> Arc<Desk> {
        Self::start(None, held, from)
    }

    /// 起一张报给 `machine` 的（经连接通往的后端转给它手上那一台）。
    pub fn at(
        machine: &str,
        held: impl Fn(&BTreeSet<String>) -> Vec<BranchHeld> + Send + Sync + 'static,
        from: impl Fn(Vec<WatchFrom>) + Send + Sync + 'static,
    ) -> Arc<Desk> {
        Self::start(Some(machine.to_string()), held, from)
    }

    fn start(
        at: Option<String>,
        held: impl Fn(&BTreeSet<String>) -> Vec<BranchHeld> + Send + Sync + 'static,
        from: impl Fn(Vec<WatchFrom>) + Send + Sync + 'static,
    ) -> Arc<Desk> {
        let desk = Arc::new(Desk {
            at,
            now: Mutex::new(Now::default()),
            held: Box::new(held),
            from: Box::new(from),
            poke: Arc::new(Notify::new()),
        });
        tokio::spawn(send_loop(Arc::downgrade(&desk), Arc::clone(&desk.poke)));
        desk
    }

    /// 宿主：此刻在看这几条 ＋ 手上留着这几条（都整份换）。没变不惊动。
    pub fn set(&self, watched: BTreeSet<String>, kept: BTreeSet<String>) {
        let moved = {
            let mut n = lock(&self.now);
            let moved = n.watched != watched || n.kept != kept;
            n.watched = watched;
            n.kept = kept;
            moved
        };
        if moved {
            self.poke.notify_one();
        }
    }

    /// 同 [`Desk::set`]，回一个收端：这一份报上去那一趟的 `from[]`（刚进名单的那几条从第几行起上流）；
    /// 当前连接已经知道这一份 · 此刻没有连接 ⇒ 交空的（下一条连接的 attach 行照样带上这一份）。
    pub fn set_and_hear(
        &self,
        watched: BTreeSet<String>,
        kept: BTreeSet<String>,
    ) -> tokio::sync::oneshot::Receiver<Vec<WatchFrom>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut n = lock(&self.now);
            n.watched = watched;
            n.kept = kept;
            n.waiting.push(tx);
        }
        self.poke.notify_one();
        rx
    }

    /// 此刻的名单。
    pub fn watched(&self) -> BTreeSet<String> {
        lock(&self.now).watched.clone()
    }

    /// 连接那一侧：要写 attach 行了 ⇒ 此刻的名单 ＋ 各条手上的清单 ＋ 手上留着的。
    pub fn ask_for_attach(&self) -> WatchAsk {
        let (watched, kept) = {
            let n = lock(&self.now);
            (n.watched.clone(), n.kept.clone())
        };
        self.ask(watched, kept)
    }

    /// 连接那一侧：接上了；`carried` ＝ 它的 attach 行带过的那一份（这条载体没有 attach 行 ⇒ `None`）。
    /// 与此刻的名单不一样 ⇒ 当场在它上面报一次。
    pub fn up(&self, client: &Arc<InboundClient>, carried: Option<&WatchAsk>) {
        {
            let mut n = lock(&self.now);
            n.link = Arc::downgrade(client);
            n.told = carried.map(|a| {
                (
                    a.sids.iter().cloned().collect(),
                    a.kept.iter().cloned().collect(),
                )
            });
        }
        self.poke.notify_one();
    }

    fn ask(&self, watched: BTreeSet<String>, kept: BTreeSet<String>) -> WatchAsk {
        let branch = (self.held)(&watched);
        WatchAsk {
            sids: watched.into_iter().collect(),
            branch,
            kept: kept.into_iter().collect(),
        }
    }

    /// 当前连接还不知道此刻的名单 ⇒ 报到它知道为止（报的时候又变了 ⇒ 接着报最新那份）。
    async fn flush(&self) {
        loop {
            let (client, now, waiting) = {
                let mut n = lock(&self.now);
                let Some(client) = n.link.upgrade() else {
                    for w in n.waiting.drain(..) {
                        w.send(Vec::new()).discard(Why::PeerGone);
                    }
                    return;
                };
                let now = (n.watched.clone(), n.kept.clone());
                if n.told.as_ref() == Some(&now) {
                    for w in n.waiting.drain(..) {
                        w.send(Vec::new()).discard(Why::PeerGone);
                    }
                    return;
                }
                n.told = Some(now.clone());
                (client, now, std::mem::take(&mut n.waiting))
            };
            let args = serde_json::to_value(self.ask(now.0, now.1)).unwrap_or_default();
            let answer = match &self.at {
                None => client.call(WATCH_CMD, args, WATCH_BUDGET).await,
                Some(m) => {
                    let deadline = tokio::time::Instant::now() + WATCH_BUDGET;
                    client.call_at(m, WATCH_CMD, args, None, deadline).await
                }
            };
            let from = match answer {
                Ok(data) => match data.map(serde_json::from_value::<WatchReply>) {
                    Some(Ok(r)) => r.from,
                    Some(Err(e)) => {
                        tracing::warn!("stream_watch 应答形状不对，不收：{e}");
                        Vec::new()
                    }
                    None => Vec::new(),
                },
                Err(e) => {
                    tracing::warn!("stream_watch 报名单没报上：{e:?}");
                    Vec::new()
                }
            };
            for w in waiting {
                w.send(from.clone()).discard(Why::PeerGone);
            }
            if !from.is_empty() {
                (self.from)(from);
            }
        }
    }
}

impl Drop for Desk {
    fn drop(&mut self) {
        self.poke.notify_one();
    }
}

/// 一台的发送任务：被叫醒就把当前连接报到最新那份；台没了就收。
async fn send_loop(desk: Weak<Desk>, poke: Arc<Notify>) {
    loop {
        poke.notified().await;
        let Some(d) = desk.upgrade() else {
            return;
        };
        d.flush().await;
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
#[path = "../../../tests/comms/resident-client/watch_tests.rs"]
mod tests;
