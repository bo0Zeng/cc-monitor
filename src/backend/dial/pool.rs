//! 〔SR1a · 2026-09-24〕**按拨号身份复用 SSH 连接** —— 「本机只常驻一个后端、所有 SSH 连接由它复用」的那个「复用」。
//!
//! # 身份是什么
//!
//! 拨号请求里**决定「连到哪、以谁的身份」**的那几项：`host · port · user · key_path · host_key_fingerprint ·
//! 竞速地址 · 跳板`（[`identity`]）。用法（`use`）· 命令 · 阶段行 · `agent_sock` 都**不在**身份里 ——
//! 同一台远端的长流与一次性查询正是要共用一条连接。配置一改（换了钥匙 / 指纹固化了 / 换了跳板）就是
//! 另一个身份、另一条连接；旧的那条随用它的最后一条链路一起收掉。
//!
//! 〔NT1 · 2026-09-24〕一个身份从「一格」变成「一族」（至多 [`MAX_CONNECTIONS_PER_HOST`] 条，按需多开），见下面 NT1 那一段。
//!
//! # 活多久：**没有定时器**
//!
//! 表里存 `Weak`，链路手里拿 `Arc` ⇒ **最后一条链路走了，连接就断**（`Arc` 落零 ⇒ russh 句柄 drop）。
//! 「空闲多久再关」要一个会自己醒的构件，本 crate 不许有（`no_timer_guard`）。代价如实写：
//! 远端长流断了之后，下一次一次性查询要重新握手（长流在的时候它们全都复用那一条）。
//!
//! # 同一身份并发来拨
//!
//! 按身份串行：每个身份（〔NT1〕每一族）一把拨号锁，第二个来的等第一个拨完、再看一遍 —— 放得下就复用它拨出来的那条。
//! 黑洞地址的握手可以很久 —— 等着的那一个由界面侧的握手期限兜（到点它会 `link-close`，
//! 这条链路的任务被 abort，锁随之放开）。
//!
//! # 复用前问一句还活着没有
//!
//! `is_closed()` 为真 ⇒ 当它不在，重拨。「看着活着、开 channel 却失败」那一形由调用方处理
//! （`uses::Lease::session_channel`：摘掉 → 重拨一次）—— 那是**同一机制换一条新连接**，不是换一条路。
//! 〔NT1〕例外一形：远端**说满了**（`MaxSessions`）⇒ 不摘，那条连接学到上限（[`Budget::refused`]），换一格放。

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};

use super::connect::Linked;
use super::DialRequest;

/// 池里的一条连接要回答的几问。**抽成 trait 是为了判据**：池的记账（复用 / 串行 / 多开 / 收掉）
/// 不需要真 SSH 就验得动（`dial_pool_tests` 拿一个假连接 ＋ 真 [`Budget`] 喂它）。
pub(crate) trait Conn: Send + Sync + 'static {
    fn is_closed(&self) -> bool;
    /// 这条连接的通道预算。
    fn budget(&self) -> &Budget;
    /// 〔NT1〕托住另一条（批量连接随主连接活着 —— §「空闲时怎么收」）。
    fn adopt(&self, other: Arc<Self>)
    where
        Self: Sized;
}

impl Conn for Linked {
    fn is_closed(&self) -> bool {
        self.session.is_closed()
    }
    fn budget(&self) -> &Budget {
        &self.budget
    }
    fn adopt(&self, other: Arc<Self>) {
        self.held
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(other);
    }
}

// ═══ 〔NT1 · 2026-09-24〕**一个身份一族连接：按需智能多开** ═════════════════════════════════════
//
// 用户 V23（`设计/99 §1`，逐字）：「今天每台机器只有一条连接可以看情况多开. 智能一点. 这是属于 ssh 优化的部分.
// 智能多开链接\压缩等等」。设计与读数住 `调研/第四波记录/NT1.md §2`。
//
// - 一个身份一**族**：`members`（`Weak`，老的在前；最老的一条活着的叫「主连接」）· 一把拨号锁（同一族同一时刻只拨一条）·
//   一个 `Notify`（任何一格还回来 / 任何一条成员没了 ⇒ 叫醒等着的人 —— 等 = 排队，零定时器）。
// - 什么时候多开（[`Family::dial_reason`]）：交互（长流 / 查询）在所有成员上都借不到格 ⇒ [`Why::Full`]；
//   传输而主连接上有长流、族里还没有一条没长流的成员 ⇒ [`Why::Bulk`]（批量字节不许排在交互字节前面：
//   读数 ⑤b 同一条 TCP 上下载期间长流回声 82 → 314 ms）。远端回拒（`MaxSessions`）⇒ 那条连接学到上限、空格作废
//   （[`Budget::refused`]），重新放置时自然落到 `Full` 那一格。
// - 封顶 [`MAX_CONNECTIONS_PER_HOST`]；到顶了就等。
// - 收：成员只被用它的人 `Arc` 着 ⇒ 最后一个用户走了当场断（溢出那条就是这么收的）；**批量连接被主连接托着**
//   （[`Conn::adopt`]）⇒ 长流在它就在（下一趟传输不再握手）、主连接没了它随之断。**没有「空闲多久再关」**。

/// 一个身份最多几条连接：主连接 · 一条批量 · 一条溢出。
///
/// 为什么是 3：批量至多一条（传输车道每条连接 4 格，多一条批量只是多抢瓶颈的带宽份额，不是更快）；
/// 溢出那一条只在「主连接的 8 格真满了」或「远端 `MaxSessions` 被调低」时才出现，查询都短命，它随最后一条查询收掉。
/// 每多一条 = 远端多一个 `sshd-session` 进程 ＋ 一次握手鉴权（读数 ⑤a：≈ 5 个往返）。
pub(crate) const MAX_CONNECTIONS_PER_HOST: usize = 3;

/// 一条 session 通道（或一条隧道）走哪一道。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lane {
    /// 长流（交互，活得长）：借一格通道，并记在这条连接的「长流数」上。
    Stream,
    /// capture · files 链路（交互，短命）：借一格通道。
    Query,
    /// 传输：先过传输车道、再过通道闸。
    Transfer,
    /// 端口转发（direct-tcpip）：`MaxSessions` 不数它，不借格。
    Tunnel,
}

/// 这一次为什么多开了一条。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Why {
    /// 交互在所有成员上都借不到格（闸满 / 远端回拒之后学到的上限满）。
    Full,
    /// 传输要一条没有长流的连接，而族里还没有。
    Bulk,
}

/// 这一次放到了哪。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum How {
    /// 放在族里已有的一条上。
    Reused,
    /// 族里一条都没有，拨了第一条。
    Fresh,
    /// 多开了一条。
    Extra(Why),
}

/// 放置的结果：哪条连接 · 借到的格（隧道没有）· 怎么来的。
pub(crate) struct Placed<C> {
    pub(crate) conn: Arc<C>,
    pub(crate) permit: Option<Permit>,
    pub(crate) how: How,
}

/// 一个身份的一族连接。
struct Family<C> {
    members: Mutex<Vec<Weak<C>>>,
    dialing: tokio::sync::Mutex<()>,
    freed: Arc<Notify>,
}

impl<C: Conn> Family<C> {
    fn new() -> Self {
        Family {
            members: Mutex::new(Vec::new()),
            dialing: tokio::sync::Mutex::new(()),
            freed: Arc::new(Notify::new()),
        }
    }

    /// 此刻活着、没关的成员（老的在前）。顺手把死了的 `Weak` 摘掉（**只在有人来放置的时候**扫 —— 不要定时器）。
    fn live(&self) -> Vec<Arc<C>> {
        let mut g = self.members.lock().unwrap_or_else(|e| e.into_inner());
        g.retain(|w| w.strong_count() > 0);
        g.iter()
            .filter_map(Weak::upgrade)
            .filter(|c| !c.is_closed())
            .collect()
    }

    /// 传输能放上去的成员：主连接上没有长流 ⇒ 全体（主连接在先）；有 ⇒ 只有**没有长流**的非主成员。
    fn transfer_candidates(live: &[Arc<C>]) -> Vec<Arc<C>> {
        let Some(primary) = live.first() else {
            return Vec::new();
        };
        if primary.budget().streams() == 0 {
            live.to_vec()
        } else {
            live[1..]
                .iter()
                .filter(|c| c.budget().streams() == 0)
                .cloned()
                .collect()
        }
    }

    /// 在已有成员上借一格（不等）。
    fn fit(&self, lane: Lane) -> Option<Placed<C>> {
        let live = self.live();
        let placed = |conn: &Arc<C>, permit| Placed {
            conn: Arc::clone(conn),
            permit,
            how: How::Reused,
        };
        match lane {
            Lane::Tunnel => live.first().map(|c| placed(c, None)),
            Lane::Stream | Lane::Query => live
                .iter()
                .find_map(|c| c.budget().try_take(lane).map(|p| placed(c, Some(p)))),
            Lane::Transfer => Self::transfer_candidates(&live)
                .iter()
                .find_map(|c| c.budget().try_take(lane).map(|p| placed(c, Some(p)))),
        }
    }

    /// 借不到格的时候：该不该多开一条、为什么（`None` = 等）。**多开的全部判准在这一处。**
    fn dial_reason(live: &[Arc<C>], lane: Lane) -> Option<Why> {
        if live.len() >= MAX_CONNECTIONS_PER_HOST {
            return None;
        }
        let Some(primary) = live.first() else {
            // 族里一条活的都没有：拨第一条（调用方按 `How::Fresh` 记，这里的 `Why` 不用）。
            return Some(Why::Full);
        };
        match lane {
            // 有活的时隧道总放得下（`fit` 那一步就回了），走不到这里。
            Lane::Tunnel => None,
            Lane::Stream | Lane::Query => Some(Why::Full),
            Lane::Transfer => {
                // 主连接上有长流、而族里还没有一条没长流的成员 ⇒ 分道；其余（批量那条 / 主连接的车道满了）⇒ 等。
                let bulk_exists = live[1..].iter().any(|c| c.budget().streams() == 0);
                (primary.budget().streams() > 0 && !bulk_exists).then_some(Why::Bulk)
            }
        }
    }
}

/// 连接池。键 = [`identity`]。
pub(crate) struct Pool<C> {
    families: Mutex<HashMap<String, Arc<Family<C>>>>,
}

impl<C: Conn> Pool<C> {
    pub(crate) fn new() -> Self {
        Pool {
            families: Mutex::new(HashMap::new()),
        }
    }

    fn family(&self, key: &str) -> Arc<Family<C>> {
        let mut g = self.families.lock().unwrap_or_else(|e| e.into_inner());
        // 顺手清掉已经没人用的族（**只在有人来取的时候**扫一次）：没有活成员、也没人正拿着它（放置中 / 拨号中）。
        g.retain(|_, f| {
            Arc::strong_count(f) > 1
                || f.members
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .iter()
                    .any(|w| w.strong_count() > 0)
        });
        Arc::clone(
            g.entry(key.to_string())
                .or_insert_with(|| Arc::new(Family::new())),
        )
    }

    /// **放置**：在这个身份的一族里给 `lane` 找一格。已有成员借得到 ⇒ 复用；借不到而该多开（[`Family::dial_reason`]）⇒
    /// 拨一条新的（同一族同一时刻只拨一条：并发来的第二个等第一个拨完、再看一遍）；到顶了 ⇒ 等一格还回来。
    pub(crate) async fn place<F, Fut, E>(
        &self,
        key: &str,
        lane: Lane,
        mut dial: F,
    ) -> Result<Placed<C>, E>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<C, E>>,
    {
        let fam = self.family(key);
        loop {
            // 先登记「叫醒我」再看 —— 看完之后才还回来的那一格不会漏掉。
            let notified = fam.freed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(p) = fam.fit(lane) {
                return Ok(p);
            }
            let guard = fam.dialing.lock().await;
            if let Some(p) = fam.fit(lane) {
                return Ok(p);
            }
            let live = fam.live();
            if let Some(why) = Family::dial_reason(&live, lane) {
                let conn = Arc::new(dial().await?);
                conn.budget().attach(Arc::clone(&fam.freed));
                let permit = match lane {
                    Lane::Tunnel => None,
                    _ => conn.budget().try_take(lane),
                };
                fam.members
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(Arc::downgrade(&conn));
                let how = match live.first() {
                    None => How::Fresh,
                    Some(primary) => {
                        if why == Why::Bulk {
                            primary.adopt(Arc::clone(&conn));
                        }
                        How::Extra(why)
                    }
                };
                drop(guard);
                return Ok(Placed { conn, permit, how });
            }
            drop(guard);
            notified.await;
        }
    }

    /// 这一条不能用了（死了）⇒ 从族里摘掉（**摘掉 ≠ 关掉**：手里还拿着它的用户照用到走；只是新的放置不再落到它上面）。
    /// 只摘指名的那一条。
    pub(crate) fn evict(&self, key: &str, c: &Arc<C>) {
        let fam = {
            let g = self.families.lock().unwrap_or_else(|e| e.into_inner());
            g.get(key).cloned()
        };
        if let Some(fam) = fam {
            fam.members
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .retain(|w| w.upgrade().is_some_and(|cur| !Arc::ptr_eq(&cur, c)));
            fam.freed.notify_waiters();
        }
    }

    /// 此刻还活着（有人在用 / 被托着）的连接数（全部身份）。生产日志读它。
    pub(crate) fn live(&self) -> usize {
        let g = self.families.lock().unwrap_or_else(|e| e.into_inner());
        g.values()
            .map(|f| {
                f.members
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .iter()
                    .filter(|w| w.strong_count() > 0)
                    .count()
            })
            .sum()
    }
}

// ═══ 〔SR1b · 2026-09-24〕**一条连接上的通道预算** ═══════════════════════════════════════
//
// SFTP 进本机常驻后端之后，一台远端的长流 · 一次性查询 · SFTP **同一条 SSH 连接**（本池的复用）⇒
// 远端 `MaxSessions`（`man 5 sshd_config`：「open shell, login or subsystem (e.g. sftp) sessions permitted
// per network connection … The default is 10」；本机 `OpenSSH_10.2p1` 现打 `sshd -T` 印 `maxsessions 10`）
// 数的是**这一条连接上**的 session 通道 ⇒ 预算必须**按连接**记，不能再按「SFTP 那条连接」记。
// 从前 monitor 那一份（`sftp_pool` 里的通道闸，6 格）管的是 SFTP 自己那条独立连接。

/// **一条 SSH 连接上所有 session 通道**（exec 长流 · capture · sftp 子系统）的上限。
///
/// 8 = 默认 `MaxSessions` 10 留 2 格：① `sshd` 回收旧通道与新通道短暂重叠；② 被调低到 8 的机器
/// 仍不撞墙（撞了也只是这一次开通道报错，不坏整条连接）。端口转发的 direct-tcpip 通道**不计** ——
/// `MaxSessions` 不数它（它不是 session）。
/// ⚠ 远端真正的 `MaxSessions` **量不到**（协议里没有这个数）⇒ 这是一条**本地自律**，不是协商结果。
/// 〔NT1〕被远端回拒过的连接，上限改成被拒那一刻的在用格数（[`Budget::refused`]）。
pub(crate) const SESSION_CHANNEL_CAP: usize = 8;

/// 其中**传输**（sftp 上传 / 下载）最多占几格。
///
/// 4 ⇒ 满载传输时会话长流 ＋ 一次性查询仍有 ≥ 4 格 —— 一个大上传不许把「列会话」「读大纲」饿死。
/// 〔墓碑 —— 同名常量从前住 monitor 的 `sftp_pool.rs`，理由是「给浏览留格子」（`设计/60 §7` 第 3 条）；
///  浏览离开 SFTP 之后 S4 把它退役了。今天它回来，**理由换了**：给同一条连接上的会话与查询留格子。〕
/// 〔NT1〕主连接上有长流时传输另开一条批量连接（`Why::Bulk`）⇒ 这 4 格今天多半落在批量连接上。
pub(crate) const TRANSFER_LANE_CAP: usize = 4;

/// 一条连接的两道闸 ＋ 两个计数。**借不到不等**（[`Budget::try_take`]）—— 等由族那一层做（`Notify`，零定时器）。
pub(crate) struct Budget {
    sessions: Arc<Semaphore>,
    transfers: Arc<Semaphore>,
    /// 此刻借着 [`Lane::Stream`] 那一格的数（长流数）。
    streams: Arc<AtomicUsize>,
    /// 通道闸此刻的总格数（被远端回拒过 ⇒ 缩成那一刻的在用格数）。
    cap: AtomicUsize,
    /// 族的「有格还回来了」铃。放置时挂上（[`Budget::attach`]）。
    freed: OnceLock<Arc<Notify>>,
}

/// 借到的一格（或两格）。drop 即归还，并摇一下族的铃。
pub(crate) struct Permit {
    session: Option<OwnedSemaphorePermit>,
    lane: Option<OwnedSemaphorePermit>,
    stream: Option<Arc<AtomicUsize>>,
    freed: Option<Arc<Notify>>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        drop(self.lane.take());
        drop(self.session.take());
        if let Some(n) = self.stream.take() {
            n.fetch_sub(1, Ordering::SeqCst);
        }
        if let Some(f) = self.freed.take() {
            f.notify_waiters();
        }
    }
}

impl Budget {
    pub(crate) fn new() -> Self {
        Budget {
            sessions: Arc::new(Semaphore::new(SESSION_CHANNEL_CAP)),
            transfers: Arc::new(Semaphore::new(TRANSFER_LANE_CAP)),
            streams: Arc::new(AtomicUsize::new(0)),
            cap: AtomicUsize::new(SESSION_CHANNEL_CAP),
            freed: OnceLock::new(),
        }
    }

    /// 挂上族的铃（只挂一次）。
    pub(crate) fn attach(&self, freed: Arc<Notify>) {
        let _ = self.freed.set(freed);
    }

    /// 借一格，**不等**：借不到 ⇒ `None`。传输先过车道、再过通道闸（通道闸借不到就把车道那一格还掉 —— 排队等车道的不占通道格）。
    pub(crate) fn try_take(&self, lane: Lane) -> Option<Permit> {
        let lane_permit = match lane {
            Lane::Tunnel => return None,
            Lane::Transfer => Some(Arc::clone(&self.transfers).try_acquire_owned().ok()?),
            Lane::Stream | Lane::Query => None,
        };
        let session = Arc::clone(&self.sessions).try_acquire_owned().ok()?;
        let stream = (lane == Lane::Stream).then(|| {
            self.streams.fetch_add(1, Ordering::SeqCst);
            Arc::clone(&self.streams)
        });
        Some(Permit {
            session: Some(session),
            lane: lane_permit,
            stream,
            freed: self.freed.get().cloned(),
        })
    }

    /// 〔NT1〕**远端回拒了一条 session 通道**（`MaxSessions` 那一形）⇒ 这条连接的上限 = 此刻在用的格数：
    /// 闸里空着的格当场作废（`forget_permits`）。调用方先把被拒那一格还掉再调（它不算在用）。回学到的上限。
    /// 在用的格以后还回来照常可借 —— 上限就是「被拒那一刻远端肯给的数」。
    pub(crate) fn refused(&self) -> usize {
        let spare = self.sessions.available_permits();
        let gone = self.sessions.forget_permits(spare);
        self.cap.fetch_sub(gone, Ordering::SeqCst) - gone
    }

    /// 此刻借着长流那一格的数。
    pub(crate) fn streams(&self) -> usize {
        self.streams.load(Ordering::SeqCst)
    }

    /// 此刻 `(还剩几格通道, 还剩几格车道, 通道总格数)`。生产日志读它（不是只给判据看的仪表）。
    pub(crate) fn free(&self) -> (usize, usize, usize) {
        (
            self.sessions.available_permits(),
            self.transfers.available_permits(),
            self.cap.load(Ordering::SeqCst),
        )
    }
}

impl Drop for Budget {
    /// 一条成员没了 ⇒ 摇一下族的铃（到顶等着的人此刻可以多开了）。
    fn drop(&mut self) {
        if let Some(f) = self.freed.get() {
            f.notify_waiters();
        }
    }
}

/// 本进程那一个 SSH 连接池。
pub(crate) fn ssh() -> &'static Pool<Linked> {
    static P: OnceLock<Pool<Linked>> = OnceLock::new();
    P.get_or_init(Pool::new)
}

/// 一份拨号请求的**连接身份**（规范化 JSON 串，键序固定）。
pub(crate) fn identity(req: &DialRequest) -> String {
    let endpoints: Vec<serde_json::Value> = req
        .race_order()
        .iter()
        .map(|e| serde_json::json!([e.host, e.port]))
        .collect();
    let jump = req
        .jump
        .as_ref()
        .map(|j| serde_json::json!([j.host, j.port, j.user, j.key_path, j.host_key_fingerprint]));
    serde_json::json!([
        req.host,
        req.port,
        req.user,
        req.key_path,
        req.host_key_fingerprint,
        endpoints,
        jump
    ])
    .to_string()
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_pool_tests.rs"]
mod tests;
