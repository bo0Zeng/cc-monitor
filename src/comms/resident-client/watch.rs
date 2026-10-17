//! **每台一张名单台（[`Desk`]）**：宿主只交「现在在看哪几条会话」（整份换），报给那台、新连接重报、应答 `from[]` 交出去都在这里，全仓只这一处。
//!
//! - 新连接：attach 行带上此刻的名单（[`Desk::ask_for_attach`]），那台从第一帧起照它发；接上之后（[`Desk::up`]）带过的那份与此刻的不一样
//!   （握手中变了 · 这条载体没有 attach 行）⇒ 当场在它上面整份报一次（`stream-watch`）。
//! - 名单变了 ⇒ 在当前连接上整份报；一台只一个发送任务，手里只留最新一份，同一台按序报、不后发先到；报没成不重试，下一条连接照样带上。
//! - 应答 `from[]`（刚进名单的会话从第几行起上流）原样交宿主，由看着那一条的去补 `[已有, seq)`。
//!
//! 名单从空开始（一条都还没人看 ⇒ 那台一条都不发）。读主线外清单的宿主（桌面）经 `held` 交回手上各份有什么，那台只补缺的；不读的（手机）交空的。
//! 手上留着记录的会话（`kept`，桌面的重放留存）随名单一起交、一起报：那台不在说的它答一句已结束；不留的（手机）交空的。

use crate::client::InboundClient;
use cmd_wire::watch::{BranchHeld, WatchAsk, WatchFrom, WatchReply};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::sync::Notify;

/// 那台后端的命令名。
pub const WATCH_CMD: &str = "stream-watch";

/// 一次报名单的期限：后端在连接内就地做完（只是换一张表、数几个游标），几秒足够；超了就等下一次变化 / 下一条连接。
pub const WATCH_BUDGET: Duration = Duration::from_secs(10);

type HeldFn = Box<dyn Fn(&BTreeSet<String>) -> Vec<BranchHeld> + Send + Sync>;
type FromFn = Box<dyn Fn(Vec<WatchFrom>) + Send + Sync>;

/// 一台的名单台。
pub struct Desk {
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
    sids: BTreeSet<String>,
    kept: BTreeSet<String>,
    /// 当前那条连接（弱引用：连接走了就不报）。
    link: Weak<InboundClient>,
    /// 那条连接已经知道的那一份（attach 行带过的 · 报过的：在看的 ＋ 手上留着的）；`None` ＝ 它还什么都不知道（全发）。
    told: Option<(BTreeSet<String>, BTreeSet<String>)>,
}

impl Desk {
    /// 起一张（在当前 tokio 运行时上起它的发送任务；台没了任务跟着收）。
    pub fn new(
        held: impl Fn(&BTreeSet<String>) -> Vec<BranchHeld> + Send + Sync + 'static,
        from: impl Fn(Vec<WatchFrom>) + Send + Sync + 'static,
    ) -> Arc<Desk> {
        let desk = Arc::new(Desk {
            now: Mutex::new(Now::default()),
            held: Box::new(held),
            from: Box::new(from),
            poke: Arc::new(Notify::new()),
        });
        tokio::spawn(send_loop(Arc::downgrade(&desk), Arc::clone(&desk.poke)));
        desk
    }

    /// 宿主：此刻在看这几条 ＋ 手上留着这几条（都整份换）。没变不惊动。
    pub fn set(&self, sids: BTreeSet<String>, kept: BTreeSet<String>) {
        let moved = {
            let mut n = lock(&self.now);
            let moved = n.sids != sids || n.kept != kept;
            n.sids = sids;
            n.kept = kept;
            moved
        };
        if moved {
            self.poke.notify_one();
        }
    }

    /// 此刻的名单。
    pub fn sids(&self) -> BTreeSet<String> {
        lock(&self.now).sids.clone()
    }

    /// 连接那一侧：要写 attach 行了 ⇒ 此刻的名单 ＋ 各条手上的清单 ＋ 手上留着的。
    pub fn ask_for_attach(&self) -> WatchAsk {
        let (sids, kept) = {
            let n = lock(&self.now);
            (n.sids.clone(), n.kept.clone())
        };
        self.ask(sids, kept)
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

    fn ask(&self, sids: BTreeSet<String>, kept: BTreeSet<String>) -> WatchAsk {
        let branch = (self.held)(&sids);
        WatchAsk {
            sids: sids.into_iter().collect(),
            branch,
            kept: kept.into_iter().collect(),
        }
    }

    /// 当前连接还不知道此刻的名单 ⇒ 报到它知道为止（报的时候又变了 ⇒ 接着报最新那份）。
    async fn flush(&self) {
        loop {
            let (client, now) = {
                let mut n = lock(&self.now);
                let Some(client) = n.link.upgrade() else {
                    return;
                };
                let now = (n.sids.clone(), n.kept.clone());
                if n.told.as_ref() == Some(&now) {
                    return;
                }
                n.told = Some(now.clone());
                (client, now)
            };
            let args = serde_json::to_value(self.ask(now.0, now.1)).unwrap_or_default();
            match client.call(WATCH_CMD, args, WATCH_BUDGET).await {
                Ok(data) => {
                    let from = match data.map(serde_json::from_value::<WatchReply>) {
                        Some(Ok(r)) => r.from,
                        Some(Err(e)) => {
                            tracing::warn!("stream_watch 应答形状不对，不收：{e}");
                            Vec::new()
                        }
                        None => Vec::new(),
                    };
                    if !from.is_empty() {
                        (self.from)(from);
                    }
                }
                Err(e) => tracing::warn!("stream_watch 报名单没报上：{e:?}"),
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
