//! **按拨号身份复用 SSH 连接** —— 「本机只常驻一个后端、所有 SSH 连接由它复用」的那个「复用」。
//!
//! # 身份是什么
//!
//! 拨号请求里**决定「连到哪、以谁的身份」**的那几项：`host · port · user · key_path · host_key_fingerprint ·
//! 竞速地址 · 跳板`（[`identity`]）。用法（`use`）· 命令 · 阶段行 · `agent_sock` 都**不在**身份里 ——
//! 同一台远端的长流与一次性查询正是要共用一条连接。配置一改（换了钥匙 / 指纹固化了 / 换了跳板）就是
//! 另一个身份、另一条连接；旧的那条随用它的最后一条链路一起收掉。
//!
//! 一个身份是一族连接（至多 [`MAX_CONNECTIONS_PER_HOST`] 条，按需多开），见下面「一个身份一族连接」那一段。
//!
//! # 活多久：**没有定时器**
//!
//! 表里存 `Weak`，链路手里拿 `Arc` ⇒ **最后一条链路走了，连接就断**（`Arc` 落零 ⇒ russh 句柄 drop）。
//! 「空闲多久再关」要一个会自己醒的构件，本 crate 不许有（`no_timer_guard`）。代价如实写：
//! 远端长流断了之后，下一次一次性查询要重新握手（长流在的时候它们全都复用那一条）。
//!
//! # 同一身份并发来拨
//!
//! 按身份串行：每个身份（每一族）一把拨号锁，第二个来的等第一个拨完、再看一遍 —— 放得下就复用它拨出来的那条。
//! 黑洞地址的握手可以很久 —— 等着的那一个由界面侧的握手期限兜（到点它会 `link-close`，
//! 这条链路的任务被 abort，锁随之放开）。
//!
//! # 复用前问一句还活着没有
//!
//! `is_closed()` 为真 ⇒ 当它不在，重拨。「看着活着、开 channel 却失败」那一形由调用方处理
//! （`uses::Lease::session_channel`：摘掉 → 重拨一次）—— 那是**同一机制换一条新连接**，不是换一条路。
//! 例外一形：远端**说满了**（`MaxSessions`）⇒ 不摘，那条连接学到上限（[`Budget::refused`]），换一格放。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};

use crate::connect::Linked;
use crate::DialRequest;

/// 池里的一条连接要回答的几问。**抽成 trait 是为了判据**：池的记账（复用 / 串行 / 多开 / 收掉）
/// 不需要真 SSH 就验得动（`dial_pool_tests` 拿一个假连接 ＋ 真 [`Budget`] 喂它）。
pub trait Conn: Send + Sync + 'static {
    fn is_closed(&self) -> bool;
    /// 这条连接的通道预算。
    fn budget(&self) -> &Budget;
    /// 托住另一条（批量连接随主连接活着 —— §「空闲时怎么收」）。
    fn adopt(&self, other: Arc<Self>)
    where
        Self: Sized;
    /// 把停着的空闲 sftp 会话关掉、还出它那一格（有就 `true`）。借不到格时池先做这一步。
    fn reclaim_idle(&self) -> bool;
    /// 最后一次收到对端字节的时刻（[`Heard::at`]）。一条成员被判死时，族里那之后也没动静的跟着摘（[`Watch`]）。
    fn heard_at(&self) -> u64;
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
    fn reclaim_idle(&self) -> bool {
        let parked = self
            .idle_sftp
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        parked.is_some()
    }
    fn heard_at(&self) -> u64 {
        self.heard.at()
    }
}

/// 本进程单调钟的毫秒数（从第一次读起算，至少是 1 —— 0 留给「还没收到过」）。
/// 只用来比「这条连接多久没收到对端字节了」，不驱动任何等待、不让谁醒来。
pub fn mono_ms() -> u64 {
    static ORIGIN: OnceLock<tokio::time::Instant> = OnceLock::new();
    let now = tokio::time::Instant::now();
    let origin = *ORIGIN.get_or_init(|| now);
    u64::try_from(now.saturating_duration_since(origin).as_millis()).unwrap_or(u64::MAX - 1) + 1
}

/// 一条连接**最后一次收到对端字节**的时刻（[`mono_ms`]；0 = 还没收到过）。TCP 那一层每读到字节记一笔
/// （`connect::Hearing`）。健康的空闲连接也会每 10 秒收到一次保活回话（`connect::KEEPALIVE`）⇒
/// 「很久没收到」说明对端没在回话：路断成黑洞时 TCP 看着还活着，只有这一格看得出来。
pub struct Heard {
    at: AtomicU64,
    /// 这条连接没了（russh 报「断开」那一刻置真）。握着连接的那一种链路（`use: hold`）等它 —— 事件，不是计时。
    gone: tokio::sync::watch::Sender<bool>,
}

impl Default for Heard {
    fn default() -> Self {
        Heard {
            at: AtomicU64::new(0),
            gone: tokio::sync::watch::Sender::new(false),
        }
    }
}

impl Heard {
    /// 一条刚握完手的连接：此刻算收到过。
    pub fn new() -> Self {
        Heard {
            at: AtomicU64::new(mono_ms()),
            ..Heard::default()
        }
    }

    /// 收到了对端的字节。
    pub fn touch(&self) {
        self.at.fetch_max(mono_ms(), Ordering::Relaxed);
    }

    pub fn at(&self) -> u64 {
        self.at.load(Ordering::Relaxed)
    }

    /// 多久没收到对端字节了（毫秒）。
    pub fn silent_for(&self) -> u64 {
        mono_ms().saturating_sub(self.at())
    }

    /// 这条连接没了（`connect::Checker` 在 russh 报断开时调）。
    pub fn mark_gone(&self) {
        self.gone.send_replace(true);
    }

    /// 等这条连接没了（已经没了 ⇒ 当场回）。
    pub async fn gone(&self) {
        let mut rx = self.gone.subscribe();
        // 发端住本结构里、活得比这一问长 ⇒ `changed` 回错（发端没了）只会在进程收尾时出现，那时也算没了。
        while !*rx.borrow_and_update() {
            if rx.changed().await.is_err() {
                return;
            }
        }
    }
}

//
// ═══ 一个身份一族连接：按需多开 ═════════════════════════════════════
//
// - 一个身份一族：`members`（`Weak`，老的在前；最老的一条活着的叫「主连接」）· 一把拨号锁（同一族同一时刻只拨一条）·
//   一个 `Notify`（任何一格还回来 / 任何一条成员没了 ⇒ 叫醒等着的人 —— 等 = 排队，零定时器）。
// - 什么时候多开（[`Family::dial_reason`]）：交互（长流 / 查询）在所有成员上都借不到格 ⇒ [`Why::Full`]；
//   传输而主连接上有长流、族里还没有一条没长流的成员 ⇒ [`Why::Bulk`]（批量字节不许排在交互字节前面：
//   同一条 TCP 上下载期间长流回声会从 82 涨到 314 ms）。远端回拒（`MaxSessions`）⇒ 那条连接学到上限、空格作废
//   （[`Budget::refused`]），重新放置时落到 `Full` 那一格。
// - 封顶 [`MAX_CONNECTIONS_PER_HOST`]；到顶了就等。
// - 收：成员只被用它的人 `Arc` 着 ⇒ 最后一个用户走了当场断（溢出那条就是这么收的）；批量连接被主连接托着
//   （[`Conn::adopt`]）⇒ 长流在它就在（下一趟传输不再握手）、主连接没了它随之断。没有「空闲多久再关」。

/// 一个身份最多几条连接：主连接 · 一条批量 · 一条溢出。批量至多一条（传输车道每条连接 4 格，多一条只是多抢瓶颈带宽）；
/// 溢出那一条只在主连接的 8 格真满了、或远端 `MaxSessions` 被调低时才出现，随最后一条查询收掉。
/// 每多一条 = 远端多一个 `sshd-session` 进程 ＋ 一次握手鉴权（≈ 5 个往返）。
pub const MAX_CONNECTIONS_PER_HOST: usize = 3;

/// 一条 session 通道（或一条隧道）走哪一道。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
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
pub enum Why {
    /// 交互在所有成员上都借不到格（闸满 / 远端回拒之后学到的上限满）。
    Full,
    /// 传输要一条没有长流的连接，而族里还没有。
    Bulk,
}

/// 这一次放到了哪。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    /// 放在族里已有的一条上。
    Reused,
    /// 族里一条都没有，拨了第一条。
    Fresh,
    /// 多开了一条。
    Extra(Why),
}

/// 放置的结果：哪条连接 · 借到的格（隧道没有）· 怎么来的。
pub struct Placed<C> {
    pub conn: Arc<C>,
    pub permit: Option<Permit>,
    pub how: How,
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
pub struct Pool<C> {
    families: Mutex<HashMap<String, Arc<Family<C>>>>,
}

impl<C: Conn> Default for Pool<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Conn> Pool<C> {
    pub fn new() -> Self {
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
    pub async fn place<F, Fut, E>(&self, key: &str, lane: Lane, mut dial: F) -> Result<Placed<C>, E>
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
            // 借不到格 ⇒ 先挤掉停着的空闲 sftp 会话（还出它那一格），再看一遍 —— 空位不许逼出一次多开 / 一次等。
            if lane != Lane::Tunnel && fam.live().iter().any(|c| c.reclaim_idle()) {
                continue;
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

    /// 这个身份此刻能放传输的成员（与放置同一条分道规矩：主连接上有长流 ⇒ 只有没长流的非主成员）。
    /// `sftp.rs` 在这些成员上找停着的空闲会话。
    pub fn transfer_candidates(&self, key: &str) -> Vec<Arc<C>> {
        let fam = {
            let g = self.families.lock().unwrap_or_else(|e| e.into_inner());
            g.get(key).cloned()
        };
        fam.map(|f| Family::transfer_candidates(&f.live()))
            .unwrap_or_default()
    }

    /// 这一条不能用了（死了）⇒ 从族里摘掉（**摘掉 ≠ 关掉**：手里还拿着它的用户照用到走；只是新的放置不再落到它上面）。
    /// 只摘指名的那一条。
    pub fn evict(&self, key: &str, c: &Arc<C>) {
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

    /// 这一条被判成黑洞（从 `since` 起等它回话、没等到）⇒ 摘掉它，连同族里**从 `since` 起也没收到过字节**的成员：
    /// 它们多半走的是同一条断掉的路（换网 / NAT 表过期），留着的话下一回放置落到它们上面、再干等一回。
    /// 那之后收到过字节的成员留着。摘掉 ≠ 关掉（同 [`Pool::evict`]）。
    pub fn evict_silent(&self, key: &str, dead: &Arc<C>, since: u64) {
        let fam = {
            let g = self.families.lock().unwrap_or_else(|e| e.into_inner());
            g.get(key).cloned()
        };
        if let Some(fam) = fam {
            fam.members
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .retain(|w| {
                    w.upgrade()
                        .is_some_and(|cur| !Arc::ptr_eq(&cur, dead) && cur.heard_at() > since)
                });
            fam.freed.notify_waiters();
        }
    }

    /// 此刻还活着（有人在用 / 被托着）的连接数（全部身份）。生产日志读它。
    pub fn live(&self) -> usize {
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

/// 在一条池里的连接上等远端回话（开通道 / 验活），被打断（或判死，[`Watch::dead`]）就摘掉它，
/// 连同族里从开始等那一刻起也没收到过字节的成员（[`Pool::evict_silent`]）。
///
/// 路径断成黑洞（换网 / NAT 表过期 —— 没有 RST，TCP 看着还活着）之后，这条连接要等保活连着没回话（保活 `KEEPALIVE_MAX + 1` 轮，最后收包之后 20 秒）才判死；
/// 在那之前每一条新链路都会放到它上面、在开通道那一步干等（判死那一刻 russh 断开连接，开通道随之失败、换到新连接上）。
/// ⇒ 等回话那一段挂一个 [`Watch`]：没等到就被丢了（链路被关 ⇒ 任务被 abort ⇒ `Drop` 跑）⇒ 从族里摘掉这条连接
/// （摘掉 ≠ 关掉：还攥着它的用户照用到走；新的放置不再落到它上面）。等到了 ⇒ [`Watch::done`] 解除。
/// 事件驱动，零定时器。代价：用户点取消关掉链路也会摘一次 —— 多一次握手，不坏事。
/// 怎么判出死的（[`Watch::dead`]）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeadBy {
    /// 赛跑里新起的一条比它先站住。
    Outrun,
    /// 等回话时它断了（保活连着没回话，russh 断开；或对端断了）。
    Broke,
}

pub struct Watch<'a, C: Conn> {
    pool: &'a Pool<C>,
    key: &'a str,
    conn: &'a Arc<C>,
    /// 开始等回话的那一刻（[`mono_ms`]）。
    since: u64,
    armed: bool,
}

impl<'a, C: Conn> Watch<'a, C> {
    pub fn new(pool: &'a Pool<C>, key: &'a str, conn: &'a Arc<C>) -> Self {
        Watch {
            pool,
            key,
            conn,
            since: mono_ms(),
            armed: true,
        }
    }

    /// 等到了回话：解除。
    pub fn done(mut self) {
        self.armed = false;
    }

    /// 判死了（[`DeadBy`]）：当场摘。
    pub fn dead(mut self, by: DeadBy) {
        self.armed = false;
        match by {
            DeadBy::Outrun => tracing::warn!(
                "dial: 新起的一条比这条先站住 —— 判它是黑洞，从族里摘掉（连同那之后也没动静的成员）"
            ),
            DeadBy::Broke => tracing::warn!(
                "dial: 等回话时这条连接断了（保活连着没回话）—— 判它是黑洞，从族里摘掉（连同那之后也没动静的成员）"
            ),
        }
        self.pool.evict_silent(self.key, self.conn, self.since);
    }
}

impl<C: Conn> Drop for Watch<'_, C> {
    fn drop(&mut self) {
        if self.armed {
            tracing::warn!(
                "dial: 在这条连接上等远端回话时被打断了（链路被关）—— 当它可能是黑洞，从族里摘掉（连同那之后也没动静的成员）"
            );
            self.pool.evict_silent(self.key, self.conn, self.since);
        }
    }
}

//
// ═══ 一条连接上的通道预算 ═══════════════════════════════════════
//
// 一台远端的长流 · 一次性查询 · SFTP 共用同一条 SSH 连接 ⇒ 远端 `MaxSessions`（`man 5 sshd_config`：「open shell, login or
// subsystem (e.g. sftp) sessions permitted per network connection … The default is 10」）数的是这一条连接上的 session 通道 ⇒ 预算按连接记。

/// 一条 SSH 连接上所有 session 通道（exec 长流 · capture · sftp 子系统）的上限。
///
/// 8 = 默认 `MaxSessions` 10 留 2 格：① `sshd` 回收旧通道与新通道短暂重叠；② 被调低到 8 的机器仍不撞墙。
/// 端口转发的 direct-tcpip 通道不计（`MaxSessions` 不数它）。远端真正的 `MaxSessions` 量不到（协议里没有这个数）⇒
/// 这是本地自律，不是协商结果；被远端回拒过的连接，上限改成被拒那一刻的在用格数（[`Budget::refused`]）。
pub const SESSION_CHANNEL_CAP: usize = 8;

/// 其中传输（sftp 上传 / 下载）最多占几格。4 ⇒ 满载传输时会话长流 ＋ 一次性查询仍有 ≥ 4 格（大上传不许把「列会话」饿死）。
/// 主连接上有长流时传输另开一条批量连接（`Why::Bulk`）⇒ 这 4 格多半落在批量连接上。
pub const TRANSFER_LANE_CAP: usize = 4;

/// 一条连接的两道闸 ＋ 两个计数。**借不到不等**（[`Budget::try_take`]）—— 等由族那一层做（`Notify`，零定时器）。
pub struct Budget {
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
pub struct Permit {
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

impl Default for Budget {
    fn default() -> Self {
        Self::new()
    }
}

impl Budget {
    pub fn new() -> Self {
        Budget {
            sessions: Arc::new(Semaphore::new(SESSION_CHANNEL_CAP)),
            transfers: Arc::new(Semaphore::new(TRANSFER_LANE_CAP)),
            streams: Arc::new(AtomicUsize::new(0)),
            cap: AtomicUsize::new(SESSION_CHANNEL_CAP),
            freed: OnceLock::new(),
        }
    }

    /// 挂上族的铃（只挂一次）。
    pub fn attach(&self, freed: Arc<Notify>) {
        let _ = self.freed.set(freed);
    }

    /// 借一格，**不等**：借不到 ⇒ `None`。传输先过车道、再过通道闸（通道闸借不到就把车道那一格还掉 —— 排队等车道的不占通道格）。
    pub fn try_take(&self, lane: Lane) -> Option<Permit> {
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

    /// **远端回拒了一条 session 通道**（`MaxSessions` 那一形）⇒ 这条连接的上限 = 此刻在用的格数：
    /// 闸里空着的格当场作废（`forget_permits`）。调用方先把被拒那一格还掉再调（它不算在用）。回学到的上限。
    /// 在用的格以后还回来照常可借 —— 上限就是「被拒那一刻远端肯给的数」。
    pub fn refused(&self) -> usize {
        let spare = self.sessions.available_permits();
        let gone = self.sessions.forget_permits(spare);
        self.cap.fetch_sub(gone, Ordering::SeqCst) - gone
    }

    /// 此刻借着长流那一格的数。
    pub fn streams(&self) -> usize {
        self.streams.load(Ordering::SeqCst)
    }

    /// 此刻 `(还剩几格通道, 还剩几格车道, 通道总格数)`。生产日志读它（不是只给判据看的仪表）。
    pub fn free(&self) -> (usize, usize, usize) {
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
pub fn ssh() -> &'static Pool<Linked> {
    static P: OnceLock<Pool<Linked>> = OnceLock::new();
    P.get_or_init(Pool::new)
}

/// 一份拨号请求的**连接身份**（规范化 JSON 串，键序固定）。
pub fn identity(req: &DialRequest) -> String {
    let endpoints: Vec<serde_json::Value> = req
        .race_order()
        .iter()
        .map(|e| serde_json::json!([e.host, e.port]))
        .collect();
    let jump = req.jump.as_ref().map(hop_identity);
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

/// 一台跳板的身份：`[主机, 口, 用户, 钥匙, 指纹]`，再外一台在 ⇒ 它的身份接在第六格（多跳）。
fn hop_identity(j: &crate::JumpHop) -> serde_json::Value {
    let mut v = serde_json::json!([j.host, j.port, j.user, j.key_path, j.host_key_fingerprint]);
    if let (Some(outer), Some(a)) = (j.jump.as_deref(), v.as_array_mut()) {
        a.push(hop_identity(outer));
    }
    v
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_pool_tests.rs"]
mod tests;
