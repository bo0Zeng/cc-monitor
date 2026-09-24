//! Batch14-F47：SFTP 文件面板后端——per-host 常驻连接池 + 文件操作命令。
//!
//! ## 与既有 SFTP 写的关系（INVARIANT §1）
//! §1「monitor 零侵入 Claude 数据源」管的是 **monitor 作为监视器**不改坏 Claude 的
//! jsonl/pidfile。本模块是**用户亲自驱动**的通用文件传输面板,与数据源只读契约**正交**
//! （2026-07-10 用户拍板:SFTP 属独立文件传输功能,不算 monitor 写)。防误伤守卫**已经
//! 不住这儿了** —— 它是一族，住 [`crate::claude_data_fence`]
//! （〔`设计/99 §2 Q2`〕用户 2026-09-21 裁「拆」；理由与它买到的东西写在那个模块的头注里）。
//! 本模块**只是它的第一个消费者**：SFTP 写命令拒碰 Claude 数据源文件(往正被 Claude
//! 打开的 jsonl 写会损坏会话)——这是防手滑,不是合规。
//! ★〔devbench F10c〕这条承诺**现在有牙了**：**七个**写入口都过 `guard_write` 这件事由
//! `remote_write_registry::a_user_chosen_remote_write_passes_the_claude_data_fence` 钉着
//! 〔`设计/60 §5.4c` 09-20：`sftp_chmod` 是第七个〕。
//! 在那之前它零判据 —— 删掉任一处 `guard_write?`，全仓一条不红。
//! ★〔步 H2 09-21〕另加两条：`remote_write_registry` 的
//! `every_pool_command_is_either_a_registered_write_or_a_registered_read` 把「今天有几条写命令」
//! 从写死的人群换成**从本文件现打 ＋ 默认拒绝**（新增一条命令不归档当场红），
//! `a_fenced_write_refuses_before_it_touches_the_wire` 钉住 `guard_write` 出现在拿连接**之前**。
//!
//! ## 连接分离 + 已知取舍
//! SFTP 面板连接走**独立 utility 池**,与后端数据源流连接(`ssh_source` 长连接)
//! 分离,不共用——面板操作永不影响会话流。池按 origin 键、取用时校活性、死则重建。
//! - **一条连接 ＋ 一池通道**〔🔴 步 24 改，原文是「per-origin 锁串行化」〕:
//!   每 host 仍只拨**一条** SSH 连接，但在它上面按需开多条 SFTP 子系统通道
//!   （[`SESSION_CHANNEL_CAP`] 封顶、空闲复用、借不到就排队）⇒ **边传边浏览成立**、
//!   并发传输成立。传输另走一道更窄的车道闸（[`TRANSFER_LANE_CAP`]），
//!   好让浏览永远留得出格子。原来那句「同 host 不能边传边浏览、不能并发两个传输
//!   (刻意 v1 取舍)」**今天不成立了**，`设计/60 §2 档③` 第一行点的就是它。
//! - **非 UTF-8 文件名**:后端**不拦**对 lossy 名(含 U+FFFD)的写(russh-sftp 已有损解码,
//!   无法寻址真字节)——靠 F48 UI 灰置这些项;`lossy_name` 字段供前端判定。
//! - **空闲回收**:池连接空闲不主动回收(一台机一条 SFTP,YAGNI);死连按需重建
//!   (`drop_pooled` 现仅由 `evict_if_dead` 死连驱逐调用)。**配置改动未主动丢弃旧池连接**——
//!   旧连接滞留到自然死亡才用新配置(F47 取舍/backlog,未接线,别声称已接)。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::Mutex;

use crate::claude_data_fence::guard_write;
use crate::sftp::{connect_sftp, SftpConn};
use crate::ssh_source::RemoteConfig;

/// 🔴 **转出住址，不是第二个家。**
///
/// 判定的家是 [`crate::claude_data_fence`]（`pub fn` 全仓恰一处，由
/// `claude_data_fence_tests::the_protected_path_judgement_has_exactly_one_home` 钉着）。
/// 这一行只为**一个**还没改过来的消费者存在：`filewin::writeops::fenced_path`
/// 逐字写着 `sftp_pool::is_protected_claude_data_path`，而 `filewin/` 本轮在别人的写区里
/// （第五刀刚落地）⇒ 本轮不碰它，改用这一行把它接住。
///
/// ⚠ 它是一条**递减棘轮**：还在用旧住址的文件**恰好一份**，
/// 由 `claude_data_fence_tests::the_old_address_is_down_to_its_last_consumer` 钉成相等断言。
/// 那一份改过来的那天，**连这一行一起删** —— 别让它留成一个用不上的豁免。
pub use crate::claude_data_fence::is_protected_claude_data_path;

// === 连接池 ===
//
// ═══ 步 24 · 档③ 第一条：**多连接 ⇒ 边传边浏览** ═════════════════════════════
//
// `设计/60 §2 档③` 那张表第一行逐字：
//
// > | 边传边浏览 | ❌ | **我们**单连接 ＋ 锁串行。SFTP 本身允许多个未完成请求 |
//
// 本节就是把那个「我们」去掉的那一拍。改的是**形状**，不是参数：
//
//   此前：`origin → Arc<Mutex<Option<SftpConn>>>`，**一把锁 ＋ 一条通道**。
//         每个命令（列目录 / stat / 下载 / 上传 / 复制）整程持那把锁 ⇒
//         同 host 上任何两件事都排成一队，一条 8 GB 的下载期间面板整个不动。
//   此刻：`origin → Arc<OriginPool>`，**一条 SSH 连接 ＋ 一池 SFTP 通道**。
//         借通道靠信号量（[`SESSION_CHANNEL_CAP`]），归还即进空闲栈复用。
//         传输另走一道更窄的闸（[`TRANSFER_LANE_CAP`]），**浏览永远有格子**。
//
// ⚠ **硬前置照 `设计/15 §3.2` 那张表第 3 条办**（那一条点名「方案里必须有
//   per-session channel 上限」）—— 上限、它的来历与它买不到的东西，
//   逐条写在 [`SESSION_CHANNEL_CAP`] 的头注里，别只读这个数。

/// **per-session（＝ 一条 SSH 连接）SFTP 通道上限。**
///
/// # 6 是怎么来的 —— 设计里那条硬前置 ＋ 现打，不是手感
///
/// `设计/15 §3.2`「池化的硬冲突」表第 3 条逐字：
///
/// > 🔴 **OpenSSH `MaxSessions`（默认 10）这一格全仓零处提及。** 今天 13 个 exec
/// > 各拨各的，撞的是 `MaxStartups`；池化成一条 session 后撞 `MaxSessions`
/// > ⇒ **方案里必须有 per-session channel 上限（建议 ≤6）**
///
/// 那个「10」与「它管的到底是什么」现打核过（本机 `OpenSSH_10.2p1 Ubuntu-2ubuntu3.6`）：
///
/// ```text
/// $ sudo sshd -T | grep -i maxsessions
/// maxsessions 10
/// $ man 5 sshd_config   （MaxSessions 那一段）
///   Specifies the maximum number of open shell, login or subsystem (e.g. sftp)
///   sessions permitted per network connection. … The default is 10.
/// ```
///
/// 「subsystem (e.g. **sftp**)」正是本池开的东西 ⇒ 每借一条通道就占远端一格。
///
/// ⇒ **取那条建议的上界 6，留 4 格余量。** 余量给的是这三件，逐条：
/// ① `MaxSessions` 可配 —— 被调低到 8 的机器不少，6 在那种机器上仍有富余；
/// ② 死连重建时旧通道在远端还没被 sshd 收干净，两代短暂重叠；
/// ③ 同一条连接上将来若要再放东西（端口转发 / 第二个面板），不至于一加就撞墙。
///
/// # ⚠ 它买不到什么（别把这个数读宽）
///
/// - **远端真正的 `MaxSessions` 本池量不到** —— SFTP/SSH 协议里没有这个数
///   （`sshd -T` 是在**那台机器上**跑的，我们只有一条客户端连接）。
///   ⇒ 6 是一条**本地自律**，不是协商结果。远端配成 4 的话第 5 条会被 sshd 拒开，
///   那一档退化成「这次操作报错」，不会把整条连接弄坏。
/// - **它不防连接泄漏**（`设计/15 §3.2` 同一张表第 1 条那件事）：
///   `Handle::drop` 是 no-op，丢掉 `SftpConn` 并不等于把连接关掉。本池
///   [`OriginPool::invalidate`] 丢的是**我们这侧的引用**，远端那格靠 sshd 自己回收。
/// - **它不给卡住的传输装期限**（同表第 4 条点名的那一格）：一条传输占着车道不放，
///   本上限只保证它最多占 1 格、浏览那 2 格动不了它。⚠ 唯一现成的兜底是
///   `russh-sftp` 自带的**每请求 10 秒**超时（现打 3.0.0 `client::Config::default()`
///   的 `request_timeout_secs: 10`）—— 它管单个请求，不管整趟传输。
pub const SESSION_CHANNEL_CAP: usize = 6;

/// **传输车道上限**：同时最多几条**传输**（download / upload / 复制退路）占着通道。
///
/// # 为什么不等于 [`SESSION_CHANNEL_CAP`]
///
/// 档③ 第一条要的是「**边传边浏览**」。一条传输能跑几十分钟，列一次目录几十毫秒 ——
/// 让传输把 6 格占满，「边传边浏览」只是从「1 条就卡」变成「6 条才卡」，
/// 那句话仍然是假的。⇒ 传输另有一道更窄的闸，**浏览永远剩 `6 - 4 = 2` 格**。
///
/// ⚠ 两道闸**叠加**：一条传输同时占 1 格车道 ＋ 1 格通道。
/// ⚠ 4 不是「最优并发度」—— 那要真机吞吐读数，本仓没有。它只是「留得出 2 格给浏览」
///   这个约束下的一个值；真要调，先量。
pub const TRANSFER_LANE_CAP: usize = 4;

/// 池里的**一条 SFTP 通道**。两个变体的差别只有一个：通道是**哪来的**。
///
/// - `Primary` —— `connect_sftp` 建连接时顺手建的那一条。**刻意不让它闲着**：
///   池把它当第 1 条通道借出去。否则每个 origin 白占远端一格 `MaxSessions`
///   却一次也用不上，而那一格是本节全部预算里的 1/6。
/// - `Extra` —— 在**同一条已鉴权的 SSH 连接**上另起的（`SftpConn::open_sftp_channel`）。
enum PooledChannel {
    Primary(Arc<SftpConn>),
    Extra {
        /// 保活：底层连接的 Arc。**不许省掉它** —— `sftp.rs` 头注逐字
        /// 「Handle 一 drop 整条 SSH 连接就断」，而这条通道跑在那条连接上。
        _conn: Arc<SftpConn>,
        sftp: russh_sftp::client::SftpSession,
    },
}

impl PooledChannel {
    fn sftp(&self) -> &russh_sftp::client::SftpSession {
        match self {
            PooledChannel::Primary(c) => &c.sftp,
            PooledChannel::Extra { sftp, .. } => sftp,
        }
    }
}

/// 一格通道池的读数快照。
///
/// ★ **生产侧也读它**（[`ChannelSet::lease`] 末尾那句 `debug!`）—— 刻意不做成
/// 「只给判据看的仪表」：那样它在非 test 构建里就是死字段，而门禁 `deadcode`
/// 那一格是**恒等**钉死的条数。仪表要么真有人读，要么别装。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelStats {
    /// 这一格**开过**几条通道（含 `connect_sftp` 顺手建的那条 `Primary`）。
    /// ⚠ 复用生效时它远小于借用次数 —— 判据拿它钉「复用没坏」那个恒等。
    pub opened: usize,
    /// 此刻在借的条数。
    pub in_use: usize,
    /// 同时在借的**历史最大值**（高水位）。判据拿它钉「上限真的生效」。
    pub peak: usize,
    /// 通道闸（= 建池时给的那个数）。
    pub cap: usize,
    /// 传输车道闸。`cap - lane_cap` 就是**永远留给浏览的格子数**。
    pub lane_cap: usize,
}

struct ChannelSetInner<C> {
    /// 空闲通道，**后进先出** —— 刚用过的那条最可能还活着。
    idle: std::sync::Mutex<Vec<C>>,
    opened: AtomicUsize,
    in_use: AtomicUsize,
    peak: AtomicUsize,
    /// 世代号。[`ChannelSet::invalidate`] 一加，**上一代借出去的通道回来时就不再进池**。
    ///
    /// 它修的是这条竞态：连接死了 ⇒ `invalidate()` 把空闲栈清空 ⇒
    /// 而一条**正在飞**的通道稍后才归还 ⇒ 它会把一条已死的通道塞回池里给下一个人，
    /// 下一个人拿到它必然失败一次。带世代号之后那条通道直接被丢掉。
    generation: AtomicU64,
}

/// 一条 SSH 连接上的 SFTP 通道集合：**两道闸 · 空闲复用 · 读数可取**。
///
/// # 两道闸，叠加
///
/// - **通道闸**（`cap`，生产值 [`SESSION_CHANNEL_CAP`]）：谁要通道都得过它 ——
///   它对着的是远端的 `MaxSessions`。
/// - **车道闸**（`lane_cap`，生产值 [`TRANSFER_LANE_CAP`]）：**只有传输**再过一道 ——
///   它对着的是「边传边浏览」，把 `cap - lane_cap` 格永久留给浏览。
///
/// ⚠ **车道闸刻意长在这里、不长在 `OriginPool` 上。** 理由是判据的形状，不是风格：
/// `OriginPool` 攥着一条真 SSH 连接，要一台真 sshd 才构造得出来 ⇒ 车道闸挂在它身上，
/// 「传输占满时浏览还进不进得来」这件事就**一条判据都写不出来**。
/// 挂在本类型上之后，那句话是一个**相等读数**：
/// 10 条传输 ＋ 1 次浏览、`(cap=6, lane_cap=4)` ⇒ 在借的恰好是 **5**（4 传输 ＋ 1 浏览）。
/// 少了车道闸那个数是 6（6 条传输把格子占满、浏览永远排队）——**两个数差得开**。
///
/// # 泛型参数 `C` 同理
///
/// 生产实例是 `ChannelSet<PooledChannel>`；判据拿 `ChannelSet<…>` 配一条
/// **讲真 SFTP 字节的合成会话**（`tests/bridge/sftp_pool_f4_tests.rs` 那台台架，
/// 照秤 F3 的形状办）。被判的是**同一个类型的同一个方法**，不是它的抄件。
pub struct ChannelSet<C> {
    inner: Arc<ChannelSetInner<C>>,
    /// 通道闸。**借不到就 await**（不是报错）—— 那个 await 就是「队列」。
    permits: Arc<tokio::sync::Semaphore>,
    /// 车道闸，只有传输过。
    lanes: Arc<tokio::sync::Semaphore>,
    cap: usize,
    lane_cap: usize,
}

/// 一条借出去的通道。**drop 即归还**（凭据回信号量、通道回空闲栈）。
pub struct Leased<C> {
    chan: Option<C>,
    /// 借出那一刻的世代号。归还时与当前世代不符 ⇒ 这条通道整个丢掉。
    generation: u64,
    inner: Arc<ChannelSetInner<C>>,
    /// 通道闸凭据。drop 即归还 —— 排队等通道的下一个人由此被唤醒。
    _permit: tokio::sync::OwnedSemaphorePermit,
    /// 车道闸凭据（只有传输那一路有）。同样 drop 即归还。
    _lane: Option<tokio::sync::OwnedSemaphorePermit>,
}

impl<C> Leased<C> {
    pub fn get(&self) -> &C {
        self.chan
            .as_ref()
            .expect("借据在 drop 之前恒持有通道（`discard` 消耗 self）")
    }

    /// 这条通道**不许再回池**（它跟着已死的连接一起废了）。
    pub fn discard(mut self) {
        self.chan = None;
    }
}

impl<C> Drop for Leased<C> {
    fn drop(&mut self) {
        self.inner.in_use.fetch_sub(1, Ordering::SeqCst);
        let Some(c) = self.chan.take() else { return };
        if self.generation != self.inner.generation.load(Ordering::SeqCst) {
            return; // 上一代的通道，不回池（见 `generation` 头注那条竞态）
        }
        if let Ok(mut v) = self.inner.idle.lock() {
            v.push(c);
        }
    }
}

impl<C> ChannelSet<C> {
    /// `cap` = 通道闸，`lane_cap` = 传输车道闸。
    ///
    /// # 断言 `lane_cap < cap`，不是在挑剔风格
    ///
    /// 两者相等 ⇒ 传输占得满全部通道 ⇒ 「浏览永远留得出格子」这句话当场作废，
    /// 而**代码照样跑、判据照样绿**（`in_use` 那个恒等会跟着一起变）。
    /// ⇒ 这个前提只能由构造处守，守不住就炸在建池那一刻，而不是某天用户点不动面板。
    pub fn new(cap: usize, lane_cap: usize) -> Self {
        assert!(
            lane_cap < cap && lane_cap > 0,
            "车道闸 {lane_cap} 必须真窄于通道闸 {cap}（且非 0）—— \
             等于它就等于没有车道闸：传输能把通道占满，「边传边浏览」那句话就是假的"
        );
        Self {
            inner: Arc::new(ChannelSetInner {
                idle: std::sync::Mutex::new(Vec::new()),
                opened: AtomicUsize::new(0),
                in_use: AtomicUsize::new(0),
                peak: AtomicUsize::new(0),
                generation: AtomicU64::new(0),
            }),
            permits: Arc::new(tokio::sync::Semaphore::new(cap)),
            lanes: Arc::new(tokio::sync::Semaphore::new(lane_cap)),
            cap,
            lane_cap,
        }
    }

    pub fn stats(&self) -> ChannelStats {
        ChannelStats {
            opened: self.inner.opened.load(Ordering::SeqCst),
            in_use: self.inner.in_use.load(Ordering::SeqCst),
            peak: self.inner.peak.load(Ordering::SeqCst),
            cap: self.cap,
            lane_cap: self.lane_cap,
        }
    }

    /// 把一条**已经存在**的通道放进空闲栈。
    ///
    /// 用在建连接那一刻：`connect_sftp` 顺手建的那条通道由此成为池里的第 1 条。
    /// ⚠ **不占预算** —— 预算管的是「在借」，不是「在池」。
    pub fn seed(&self, chan: C) {
        self.inner.opened.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut v) = self.inner.idle.lock() {
            v.push(chan);
        }
    }

    /// 占一格预算但**不要通道**（裸通道那一路：`copy-data` 要的是 `RawSftpSession`，
    /// 它带着那一趟握手协商到的 `copy_data` 读数，复用等于把一次协商当永久事实）。
    ///
    /// ⚠ **不计入 `opened`**：那个数专门用来判「空闲复用有没有生效」，
    /// 把不进池的通道混进去，那个恒等就读不懂了。
    pub async fn reserve(&self) -> Result<tokio::sync::OwnedSemaphorePermit, String> {
        self.permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| "SFTP 通道预算已关闭".to_string())
    }

    /// 占一格**车道**但不要通道（裸通道那一路：它自己去开 `RawSftpSession`）。
    /// ⚠ 调用方必须**先**要它、**再** [`ChannelSet::reserve`]，次序同
    /// [`ChannelSet::lease_transfer`]。
    pub async fn reserve_lane(&self) -> Result<tokio::sync::OwnedSemaphorePermit, String> {
        self.lanes
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| "SFTP 传输车道已关闭".to_string())
    }

    /// 借一条**浏览用**的通道：只过通道闸。
    /// **借不到就排队等**（`acquire_owned().await`）—— 那个等待就是「队列」。
    pub async fn lease<F, Fut>(&self, open: F) -> Result<Leased<C>, String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<C, String>>,
    {
        self.lease_inner(None, open).await
    }

    /// 借一条**传输用**的通道：**先过车道闸，再过通道闸**。
    ///
    /// ⚠ 次序是承重的。反过来（先通道后车道）⇒ `cap` 条传输先把通道全占住、
    /// 再一起去等车道 —— 浏览那几格当场蒸发，而车道闸的全部意义就是留住它们。
    /// 这一条由 `tests/bridge/sftp_pool_f4_tests.rs` 那个「在借恰好 5 不是 6」的
    /// 相等读数钉着：把下面这行 `lanes` 挪到 `reserve()` 之后，那个数就变成 6。
    pub async fn lease_transfer<F, Fut>(&self, open: F) -> Result<Leased<C>, String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<C, String>>,
    {
        let lane = self
            .lanes
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| "SFTP 传输车道已关闭".to_string())?;
        self.lease_inner(Some(lane), open).await
    }

    async fn lease_inner<F, Fut>(
        &self,
        lane: Option<tokio::sync::OwnedSemaphorePermit>,
        open: F,
    ) -> Result<Leased<C>, String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<C, String>>,
    {
        let permit = self.reserve().await?;
        let generation = self.inner.generation.load(Ordering::SeqCst);
        let reused = self.inner.idle.lock().ok().and_then(|mut v| v.pop());
        let chan = match reused {
            Some(c) => c,
            None => {
                // ⚠ 凭据已在手 ⇒ 开通道失败时 `permit` 随函数返回一起 drop、自动归还。
                let c = open().await?;
                self.inner.opened.fetch_add(1, Ordering::SeqCst);
                c
            }
        };
        let now = self.inner.in_use.fetch_add(1, Ordering::SeqCst) + 1;
        self.inner.peak.fetch_max(now, Ordering::SeqCst);
        let s = self.stats();
        tracing::debug!(
            "SFTP 通道借出：已开 {} 条、在借 {}、高水位 {}、通道闸 {}、车道闸 {}",
            s.opened,
            s.in_use,
            s.peak,
            s.cap,
            s.lane_cap
        );
        Ok(Leased {
            chan: Some(chan),
            generation,
            inner: self.inner.clone(),
            _permit: permit,
            _lane: lane,
        })
    }

    /// 这一池通道整个作废（连接死了）。空闲的当场丢，在飞的归还时丢。
    pub fn invalidate(&self) {
        self.inner.generation.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut v) = self.inner.idle.lock() {
            v.clear();
        }
    }
}

/// 一个 origin 的池格：**一条 SSH 连接 ＋ 它上面的一池 SFTP 通道（自带两道闸）**。
struct OriginPool {
    /// 懒建的底层连接。**整份 `sftp_pool.rs` 里 `connect_sftp` 只出现在
    /// [`OriginPool::conn`] 一处**（此前 5 处；`sftp_move_ledger::DIAL_CENSUS` 数的就是它）。
    conn: Mutex<Option<Arc<SftpConn>>>,
    /// 两道闸都住在这里面（见 [`ChannelSet`] 头注里「车道闸为什么不长在 `OriginPool` 上」）。
    channels: ChannelSet<PooledChannel>,
}

/// 一条**裸**通道的借据（`copy-data` 那一路）。三个 `_` 字段的用途就是「活着」。
struct RawLease {
    /// 车道凭据：退路那一支是真的在搬字节，它就是一条传输。
    _lane: tokio::sync::OwnedSemaphorePermit,
    /// 通道凭据：裸通道在远端同样占一格 `MaxSessions`，不分高层裸层。
    _slot: tokio::sync::OwnedSemaphorePermit,
    /// 保活：裸通道跑在这条连接上，它一 drop 连接就断。
    _conn: Arc<SftpConn>,
    rs: crate::sftp::RawSftp,
}

impl OriginPool {
    fn new() -> Self {
        Self {
            conn: Mutex::new(None),
            channels: ChannelSet::new(SESSION_CHANNEL_CAP, TRANSFER_LANE_CAP),
        }
    }

    /// 懒建这一格的 SSH 连接。
    ///
    /// ⚠ 锁**只在拨号期间**持有（此前那把锁是整程持有的，那才是串行的病根）。
    /// 并发首连时只有一个人真拨号，其余等它拨完直接拿 `Arc` 克隆。
    async fn conn(&self, cfg: &RemoteConfig) -> Result<Arc<SftpConn>, String> {
        let mut g = self.conn.lock().await;
        if g.is_none() {
            let c = Arc::new(connect_sftp(cfg).await?);
            // 顺手建的那条通道进池当第 1 条 —— 别让它白占远端一格 `MaxSessions`。
            self.channels.seed(PooledChannel::Primary(c.clone()));
            *g = Some(c);
        }
        Ok(g.as_ref().expect("上面刚填过").clone())
    }

    /// 怎么在这条连接上现开一条通道（`lease_transfer` 用它）。〔F7c 收尾 09-24〕浏览用的 `lease` 随那几条浏览命令一起走了。
    fn opener(
        conn: Arc<SftpConn>,
    ) -> impl std::future::Future<Output = Result<PooledChannel, String>> {
        async move {
            let sftp = conn.open_sftp_channel().await?;
            Ok(PooledChannel::Extra { _conn: conn, sftp })
        }
    }

    /// 借一条**传输用**通道：两道闸都过（次序与理由见 [`ChannelSet::lease_transfer`]）。
    async fn lease_transfer(&self, cfg: &RemoteConfig) -> Result<Leased<PooledChannel>, String> {
        let conn = self.conn(cfg).await?;
        self.channels.lease_transfer(|| Self::opener(conn)).await
    }

    /// 借一条裸通道发 `copy-data`。两道闸同样都过。
    async fn lease_raw(&self, cfg: &RemoteConfig) -> Result<RawLease, String> {
        let lane = self.channels.reserve_lane().await?;
        let slot = self.channels.reserve().await?;
        let conn = self.conn(cfg).await?;
        let rs = conn.open_raw_sftp().await?;
        Ok(RawLease {
            _lane: lane,
            _slot: slot,
            _conn: conn,
            rs,
        })
    }

    /// 这一格整个作废：连接 ＋ 它上面全部通道。下次借用干净重连。
    async fn invalidate(&self) {
        *self.conn.lock().await = None;
        self.channels.invalidate();
    }
}

fn pool() -> &'static Mutex<HashMap<String, Arc<OriginPool>>> {
    static POOL: std::sync::OnceLock<Mutex<HashMap<String, Arc<OriginPool>>>> =
        std::sync::OnceLock::new();
    POOL.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn pool_for(origin: &str) -> Arc<OriginPool> {
    let mut m = pool().lock().await;
    m.entry(origin.to_string())
        .or_insert_with(|| Arc::new(OriginPool::new()))
        .clone()
}

/// 连接死亡特征（op 失败时据此决定是否重建连接并重试一次）。
fn looks_like_dead_conn(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    [
        "eof",
        "closed",
        "broken",
        "reset",
        "disconnect",
        "not connected",
        "pipe",
    ]
    .iter()
    .any(|k| e.contains(k))
}

/// 丢弃某 origin 的池连接与它上面全部通道（现仅由 `evict_if_dead` 死连驱逐调用；下次操作重建）。
/// **`origin` 须传 `cfg.origin_label()`**（池按此键建格,传 host 会静默 no-op）。
/// D 审计 R3:先克隆出 Arc 释放 `pool()` 全局锁,再动那一格——否则会握着全局锁死等,
/// 卡住所有 origin 的新操作(每个命令都要 pool().lock())。
pub async fn drop_pooled(origin: &str) {
    let entry = pool().lock().await.get(origin).cloned();
    if let Some(entry) = entry {
        entry.invalidate().await;
    }
}

/// D 审计 R1:传输失败若像连接死亡,把该 origin 那一格作废,下次传输干净重连
/// （**不**重启当前这次——部分传输不静默从头来）。修「死连留在槽里→后续传输持续失败」。
async fn evict_if_dead(origin: &str, err: &str) {
    if looks_like_dead_conn(err) {
        drop_pooled(origin).await;
    }
}

// === 传输(download/upload):chunked + 进度 + 取消 ===

use std::sync::atomic::{AtomicBool, Ordering};
// `AsyncSeekExt`〔步 24〕：断点续传要 `READ`/`WRITE` 带 offset ⇒ 两侧都要 seek。
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

/// SFTP 写分块 ≤32KB（SFTP draft 建议;严格 server 拒超大包）。
const CHUNK: usize = 32 * 1024;
/// 进度上报节流:每 ≥256KB 报一次（避免刷爆 Channel),外加起止各一次。
const PROGRESS_EVERY: u64 = 256 * 1024;

#[derive(Serialize, Clone, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    /// C03：同 `SftpEntry.size` —— 传输字节数，2^53-1 ≈ 8 PB 够用；显式收窄不回落 `bigint`。
    #[cfg_attr(test, ts(type = "number"))]
    pub transferred: u64,
    /// 总字节;下载时=远端 size,上传时=本地 size。未知(极少)为 0。
    #[cfg_attr(test, ts(type = "number"))]
    pub total: u64,
}

/// 取消令牌注册表:transfer_id → flag。**transfer_id 必须全局唯一(前端用 uuid)**——
/// 并发复用同 id 会互相覆盖 flag(D 审计 R2)。用 std::sync::Mutex(map ops 无 await,
/// 可在 [`CancelGuard::drop`] 里同步清理,修 future 被 abort 时的泄漏 = D 审计 S4)。
fn cancels() -> &'static std::sync::Mutex<HashMap<String, Arc<AtomicBool>>> {
    static C: std::sync::OnceLock<std::sync::Mutex<HashMap<String, Arc<AtomicBool>>>> =
        std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// 注册取消令牌,返回 (flag, guard)。guard 一 drop(命令 future 正常完成 **或被 abort**)
/// 就按 `Arc::ptr_eq` 从表里摘除本次的 flag——ptr_eq 保证不误删并发同 id 的他人 flag。
struct CancelGuard {
    id: String,
    flag: Arc<AtomicBool>,
}
impl Drop for CancelGuard {
    fn drop(&mut self) {
        if let Ok(mut m) = cancels().lock() {
            if m.get(&self.id).is_some_and(|f| Arc::ptr_eq(f, &self.flag)) {
                m.remove(&self.id);
            }
        }
    }
}

fn register_cancel(id: &str) -> (Arc<AtomicBool>, CancelGuard) {
    let flag = Arc::new(AtomicBool::new(false));
    if let Ok(mut m) = cancels().lock() {
        m.insert(id.to_string(), flag.clone());
    }
    (
        flag.clone(),
        CancelGuard {
            id: id.to_string(),
            flag,
        },
    )
}

fn report(ch: &tauri::ipc::Channel<TransferProgress>, transferred: u64, total: u64) {
    let _ = ch.send(TransferProgress { transferred, total });
}

/// 下载核心。**语料是一个 SFTP 会话**（不是 `cfg`）——借通道那一段留在
/// [`sftp_download`] 里，好让判据能拿一台合成 SFTP 服务端直接喂它
/// （`tests/bridge/sftp_resume_tests.rs`，照秤 F3 的形状办）。
///
/// # 🔴 步 24 · 档③ 第三条：**断点续传**
///
/// `设计/60 §2 档③` 第三行逐字「断点续传 | ❌ | 没做（`READ`/`WRITE` 带 offset，协议支持）」。
/// 现在做了，形状是：`<local>.part` 还在 ⇒ [`resume_offset`] 先把**尾块**在两侧对一遍，
/// 对得上就从那个偏移接着读（一条带 offset 的 `SSH_FXP_READ`），对不上就从 0 重来。
///
/// ⚠ **`.part` 的存亡规矩变了，逐条写死**：
/// - **取消** ⇒ **留着**（那正是续传的本钱：暂停/继续是同一件事的两半）；
/// - **报错** ⇒ 照旧删（与此前逐字相同 —— 崩溃/断网不该在用户目录里堆垃圾）。
///
/// 区分靠**读取消标志**，不是靠比对错误文案：文案是给人看的，改一个字判据就瞎了。
async fn download_inner(
    sftp: &russh_sftp::client::SftpSession,
    remote_path: &str,
    local_path: &str,
    cancel: &Arc<AtomicBool>,
    on_progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<(), String> {
    let total = sftp
        .metadata(remote_path.to_string())
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    let mut rf = sftp
        .open_with_flags(
            remote_path.to_string(),
            russh_sftp::protocol::OpenFlags::READ,
        )
        .await
        .map_err(|e| format!("打开远端 {remote_path} 失败: {e}"))?;

    let tmp = format!("{local_path}.part");
    // ── 断点续传：`.part` 还在、尾块两侧逐字节对得上 ⇒ 从那里接着写 ──────────
    let resume_from = resume_offset(&mut rf, &tmp, total).await;
    let mut lf = if resume_from > 0 {
        let mut f = tokio::fs::OpenOptions::new()
            .write(true)
            .open(&tmp)
            .await
            .map_err(|e| format!("打开本地 {tmp} 续传失败: {e}"))?;
        f.seek(std::io::SeekFrom::Start(resume_from))
            .await
            .map_err(|e| format!("本地 {tmp} 定位到 {resume_from} 失败: {e}"))?;
        f
    } else {
        tokio::fs::File::create(&tmp)
            .await
            .map_err(|e| format!("创建本地 {tmp} 失败: {e}"))?
    };
    // `READ` 带 offset —— 协议本来就支持，此前我们恒从 0 读。
    rf.seek(std::io::SeekFrom::Start(resume_from))
        .await
        .map_err(|e| format!("远端 {remote_path} 定位到 {resume_from} 失败: {e}"))?;

    // 传输核心包进一层:失败(非取消)统一清理 `.part`(D 审计 S1;取消那一支见上方头注)。
    let core = async {
        let mut buf = vec![0u8; CHUNK];
        let mut done: u64 = resume_from;
        let mut last_report: u64 = resume_from;
        on_progress(done, total);
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err("已取消".to_string());
            }
            let n = rf
                .read(&mut buf)
                .await
                .map_err(|e| format!("读远端失败: {e}"))?;
            if n == 0 {
                break;
            }
            lf.write_all(&buf[..n])
                .await
                .map_err(|e| format!("写本地失败: {e}"))?;
            done += n as u64;
            if done - last_report >= PROGRESS_EVERY {
                last_report = done;
                on_progress(done, total);
            }
        }
        lf.flush()
            .await
            .map_err(|e| format!("flush 本地失败: {e}"))?;
        Ok(done)
    }
    .await;
    drop(lf);
    let done = match core {
        Ok(d) => d,
        Err(e) => {
            // 🔴 取消 ⇒ **留着** `.part`（下次续传的本钱）；报错 ⇒ 照旧清掉。
            // 判的是**取消标志**，不是错误文案 —— 文案改一个字，按文案判的那一版就瞎了。
            if !cancel.load(Ordering::SeqCst) {
                let _ = tokio::fs::remove_file(&tmp).await; // 清半成品 .part
            }
            return Err(e);
        }
    };
    tokio::fs::rename(&tmp, local_path).await.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("落地 {local_path} 失败: {e}")
    })?;
    on_progress(done, total);
    Ok(())
}

// ── 断点续传的公共件：**尾块对拍** ────────────────────────────────────────────
//
// `设计/60 §2 档③` 第三行只说了「`READ`/`WRITE` 带 offset，协议支持」。带 offset 是容易的；
// **难的是「凭什么敢接上去」**：半成品文件只记了「上次写到哪」，**没记那是谁的字节**。
//
// 光按长度接着写，这三种情形会缝出一个**看起来下完了**的坏文件：
//   ① 远端那份在两次传输之间被改过（同名不同内容）；
//   ② 上次下的是另一台机器上的同名文件（面板换了 origin）；
//   ③ `.part` 是别的程序留下的同名垃圾。
// 三种的产物都会被 rename 成正名 ⇒ **静默损坏**，而进度条一路绿到底。
//
// ⇒ 接之前先把**尾上一块**（≤`CHUNK`）在两侧逐字节对一遍，对不上就从 0 重来。

/// 从一条可寻址的流里，从 `at` 起**精确**读 `len` 字节；读不满/出错都回 `None`。
///
/// 两侧共用同一个实现（本地 `tokio::fs::File` 与远端 `russh-sftp` 的 `File`
/// 都实现了 `AsyncRead + AsyncSeek`）—— 抄成两份的话，
/// 「读不满算不算数」这种边角迟早在一侧写成另一个样子。
async fn read_exact_at<S>(s: &mut S, at: u64, len: usize) -> Option<Vec<u8>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
{
    s.seek(std::io::SeekFrom::Start(at)).await.ok()?;
    let mut buf = vec![0u8; len];
    s.read_exact(&mut buf).await.ok()?;
    Some(buf)
}

/// 下载的续传起点：`<local>.part` 已有的字节数 —— **但只有尾块两侧对得上才算**。
///
/// 回 `0` = 从头来（没有 `.part` / 它空 / 它比远端还长 / 尾块对不上 / 读失败）。
///
/// # 代价与射程，如实记
///
/// - 代价：**多一趟 ≤32 KiB 的远端读**（一条带 offset 的 `SSH_FXP_READ`）。
/// - 它**不是**整份校验：中间某一块被改过而尾块没变，本函数看不见。
///   真要那一维得比 hash，而标准 SFTP 里没有 hash（OpenSSH 的 `check-file` 扩展
///   本仓一台机器都没量过 —— `真相源/98 §1.4` 那三条边界一条没解）。
/// - ⚠ 尾块取在**末尾**而不是开头，是因为「接着写」接的就是那一处：
///   开头相同而中段不同的两份文件（同一个模板生成的），比对开头会**误接**。
async fn resume_offset<R>(remote: &mut R, part_path: &str, total: u64) -> u64
where
    R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
{
    let Ok(meta) = tokio::fs::metadata(part_path).await else {
        return 0; // 没有半成品 —— 正常的第一次下载
    };
    let have = meta.len();
    // `have > total` ⇒ 远端那份变小了（或压根不是同一个文件）⇒ 不敢接。
    if have == 0 || total == 0 || have > total {
        return 0;
    }
    let probe = have.min(CHUNK as u64);
    let at = have - probe;
    let Ok(mut lf) = tokio::fs::File::open(part_path).await else {
        return 0;
    };
    let (Some(local_tail), Some(remote_tail)) = (
        read_exact_at(&mut lf, at, probe as usize).await,
        read_exact_at(remote, at, probe as usize).await,
    ) else {
        return 0;
    };
    if local_tail == remote_tail {
        have
    } else {
        0
    }
}

/// 上传的续传起点：远端 `<remote>.tmp` 已有的字节数 —— 规矩与 [`resume_offset`] 逐条同形，
/// 只是两侧调了个个儿（半成品在远端、完整件在本地）。
///
/// ⚠ 它**动了 `local` 的游标**（对拍要 seek）。调用方必须在开跑前把游标显式拨到位 ——
/// `upload_inner` 里那两行无条件 `seek` 就是为这一条写的。
async fn remote_resume_offset(
    sftp: &russh_sftp::client::SftpSession,
    tmp_path: &str,
    local: &mut tokio::fs::File,
    total: u64,
) -> u64 {
    let Ok(m) = sftp.metadata(tmp_path.to_string()).await else {
        return 0; // 没有半成品 —— 正常的第一次上传
    };
    let have = m.len();
    if have == 0 || total == 0 || have > total {
        return 0;
    }
    let probe = have.min(CHUNK as u64);
    let at = have - probe;
    let Ok(mut rf) = sftp
        .open_with_flags(tmp_path.to_string(), russh_sftp::protocol::OpenFlags::READ)
        .await
    else {
        return 0;
    };
    let (Some(remote_tail), Some(local_tail)) = (
        read_exact_at(&mut rf, at, probe as usize).await,
        read_exact_at(local, at, probe as usize).await,
    ) else {
        return 0;
    };
    if local_tail == remote_tail {
        have
    } else {
        0
    }
}

// ═══ F7c：上传**只写暂存区**（`设计/60 §13`）═════════════════════════════════
//
// 用户逐字「保留SFTP. 思考怎么干净」＋「现在只允许后端的文件管理部分写文件」。
// ⇒ SFTP 缩成只做传输；上传**不直接写到目标**，只写进我们自己的暂存区
//    `~/.cc-monitor/staging/<key>.part`（相对 SFTP 的起始目录），传完由后端文件管理
//    **提交**（`files-commit-upload`，住后端 `control/files_commit.rs`）。
//    ⇒ 真正落进用户目录的那一下只有后端那一处。
//
// 🔴 本节的函数签名里**没有目标路径**：传输层连「最后要落到哪」都不知道 ——
//    「暂存区之外零写」因此不是一条自律，是一个**写不出来**的形状（判据另有一刀行为读数）。
//
// ⚠ 与 [`upload_inner`]（老面板那条 `sftp_upload` 用的、直写目标）的存亡规矩**正好相反**：
//    那边「撤留、报错删」—— 理由是半成品 `<目标>.tmp` 就摆在用户目录里；
//    这边「**失败留、撤删**」—— 暂存区是我们自己的目录，那条理由没了，
//    而续传最值钱的正是「断网失败」那一档（`设计/60 §13.3` 第 3 条）。

/// 暂存区相对 SFTP 起始目录（＝ 远端 home）的那一段。
///
/// 🔴 **后端那一侧有一份逐字相同的**（`control/files_commit.rs` 里同名常量，提交时它按
/// `$HOME` 拼同一个路径）。两个 crate 没有共享落点（`设计/60 §11.4` 同一条理由）⇒
/// 「两份逐字副本 ＋ 相等断言」：`sftp_staging_tests` 现读后端那一份源码逐字比。
pub const STAGING_DIR: &str = ".cc-monitor/staging";

/// 暂存区的上一级 —— **后端的家**。它不在 ⇒ 那台机器上没有部署后端 ⇒ 照实报，**不顺手建**
/// （`D11`：后端是给定的；而提交本来就要后端在）。
pub const STAGING_PARENT: &str = ".cc-monitor";

/// 暂存件的键长（十六进制位数）。后端 `files_commit::KEY_LEN` 同一个数（判据逐字比）。
pub const STAGING_KEY_LEN: usize = 32;

/// 一个键的暂存件路径（相对 SFTP 起始目录）。
pub fn staging_part(key: &str) -> String {
    format!("{STAGING_DIR}/{key}.part")
}

/// 由（本机路径 · 大小 · 修改时间）派生暂存件的键：**同一份文件重拖一次落到同一个暂存件上**，
/// 断点续传的尾块对拍（[`remote_resume_offset`]，一个字节没改）因此照旧生效。
///
/// ⚠ 用的是标准库的默认散列（两个前缀各散一次拼成 128 位）：它**跨 Rust 版本不保证稳定**
/// —— 那只意味着「升级之后第一次重拖不续传、从 0 来」，尾块对拍另有一道兜底，不会接错。
pub fn staging_key(local_path: &str, size: u64, mtime_ns: u128) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let half = |salt: &str| {
        let mut h = DefaultHasher::new();
        (salt, local_path, size, mtime_ns).hash(&mut h);
        h.finish()
    };
    // 两个 64 位散列各占键长的一半（`STAGING_KEY_LEN` 是后端认的那个长度，判据逐字比）。
    let w = STAGING_KEY_LEN / 2;
    format!(
        "{:0w$x}{:0w$x}",
        half("ccm-staging-a"),
        half("ccm-staging-b")
    )
}

/// 暂存区在不在；不在就建**最后那一段**（上一级 [`STAGING_PARENT`] 必须已经在）。
async fn ensure_staging_dir(sftp: &russh_sftp::client::SftpSession) -> Result<(), String> {
    if sftp.try_exists(STAGING_DIR).await.unwrap_or(false) {
        return Ok(());
    }
    if !sftp.try_exists(STAGING_PARENT).await.unwrap_or(false) {
        return Err(format!(
            "那台机器上没有 ~/{STAGING_PARENT}（后端还没部署）—— 上传要先落进它底下的暂存区，\
             而提交要那台机器上的后端来做"
        ));
    }
    if let Err(e) = sftp.create_dir(STAGING_DIR).await {
        // 并发的另一趟上传刚建好它 —— 那不算错。
        if !sftp.try_exists(STAGING_DIR).await.unwrap_or(false) {
            return Err(format!("建暂存区 ~/{STAGING_DIR} 失败: {e}"));
        }
    }
    Ok(())
}

/// 🔴 **上传的唯一形状**：本机文件 → 暂存件 `staging/<key>.part`。回传完的字节数。
///
/// - 暂存区不在就建最后那一段（上一级不在 ⇒ 报错，不顺手建）；
/// - 孤儿**不在这里扫**：那一扫住后端（`files_commit::sweep_stale`，每次提交成功时顺手扫）——
///   「多久算孤儿」是一个期限的**值**，归后端（`设计/05 §3.3.2`；本文件是面 A 的传输面候选，
///   `comm_boundary` 的 `X2` 当场咬过它一次）；而且修改时间与「此刻」要取**同一台机器的钟**，
///   在这里比就是拿 monitor 的钟去比远端的时间戳；
/// - 暂存件还在 ⇒ 尾块两侧对得上就从那里接着写（[`remote_resume_offset`]，与老路同一个函数）；
/// - **撤 ⇒ 删暂存件**（用户说了不要）；**失败 ⇒ 留着**（下一趟重拖从尾块接上）。
///
/// ⚠ 语料是**一个 SFTP 会话**（借通道那一段在传输台里），好让判据拿合成服务端直接喂它。
pub async fn upload_to_staging(
    sftp: &russh_sftp::client::SftpSession,
    local_path: &str,
    key: &str,
    cancel: &AtomicBool,
    on_progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<u64, String> {
    let total = tokio::fs::metadata(local_path)
        .await
        .map(|m| m.len())
        .map_err(|e| format!("读本地 {local_path} 失败: {e}"))?;
    let mut lf = tokio::fs::File::open(local_path)
        .await
        .map_err(|e| format!("打开本地 {local_path} 失败: {e}"))?;
    ensure_staging_dir(sftp).await?;

    let part = staging_part(key);
    let resume_from = remote_resume_offset(sftp, &part, &mut lf, total).await;
    let mut flags = russh_sftp::protocol::OpenFlags::CREATE
        | russh_sftp::protocol::OpenFlags::WRITE
        | russh_sftp::protocol::OpenFlags::READ;
    if resume_from == 0 {
        flags |= russh_sftp::protocol::OpenFlags::TRUNCATE;
    }
    let mut rf = sftp
        .open_with_flags(part.clone(), flags)
        .await
        .map_err(|e| format!("开暂存件 ~/{part} 失败: {e}"))?;
    // 两侧无条件 seek（同 [`upload_inner`] 那条理由：尾块对拍动过 `lf` 的游标）。
    rf.seek(std::io::SeekFrom::Start(resume_from))
        .await
        .map_err(|e| format!("暂存件定位到 {resume_from} 失败: {e}"))?;
    lf.seek(std::io::SeekFrom::Start(resume_from))
        .await
        .map_err(|e| format!("本地 {local_path} 定位到 {resume_from} 失败: {e}"))?;

    let core = async {
        let mut buf = vec![0u8; CHUNK];
        let mut done: u64 = resume_from;
        let mut last_report: u64 = resume_from;
        on_progress(done, total);
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err("已取消".to_string());
            }
            let n = lf
                .read(&mut buf)
                .await
                .map_err(|e| format!("读本地失败: {e}"))?;
            if n == 0 {
                break;
            }
            rf.write_all(&buf[..n])
                .await
                .map_err(|e| format!("写暂存件失败: {e}"))?;
            done += n as u64;
            if done - last_report >= PROGRESS_EVERY {
                last_report = done;
                on_progress(done, total);
            }
        }
        rf.flush()
            .await
            .map_err(|e| format!("flush 暂存件失败（写未确认）: {e}"))?;
        Ok(done)
    }
    .await;
    let _ = rf.shutdown().await;
    drop(rf);
    match core {
        Ok(done) => {
            on_progress(done, total);
            Ok(done)
        }
        Err(e) => {
            // 🔴 撤 ⇒ 删（用户说了不要）；失败 ⇒ 留（续传的本钱，在我们自己的目录里）。
            //    判的是**取消标志**，不是错误文案（同 [`download_inner`] 那条理由）。
            if cancel.load(Ordering::SeqCst) {
                let _ = sftp.remove_file(part.clone()).await;
            }
            Err(e)
        }
    }
}

// ═══ F7c：**传输台** —— 窗口经通道够到 SFTP 的唯一一处（`设计/60 §13.2 ①`）═══════
//
// 窗口进程一行 SFTP 都不碰：它经通道说
//   `call(origin, "transfer-upload" | "transfer-download", …)` ⇒ **开单**（登记一张票，不起跑，回 id）
//   `subscribe(origin, "transfer/<id>", None)`                 ⇒ **起跑并看进度**
//   停订（`Sub::stop` / 丢掉订阅 / 连接断了）                    ⇒ **撤这一趟**
// SFTP 连接留在 monitor，借的就是上面那一池（`lease_transfer`：车道 ＋ 通道两道闸，`6 − 4 = 2` 同一份）。
//
// 🔴 **起跑挂在订阅上、不挂在开单上**（`设计/60 §13.3` 第 2 条）：`chan::client` 发订阅那一帧是另起
//    一个任务 `post` 的，而 `call` 由调用方自己的任务发 ⇒ 两帧在线上的先后不保证。开单即起跑的话，
//    一趟小传输会在订阅到达之前跑完、终局没人收。订阅起跑 ⇒ 这个竞态不存在。
// 🔴 **撤就是停订**（`chan::wire::Sub::stop` 头注逐字「与 `call` 的撤单同义」）⇒ 没有第三条命令。
//
// ⚠ 本节**只说领域话**（「此刻传了多少 · 怎么收场的」）；把它翻成通道那几种格子的，是宿主
//    `chan/host.rs`（它不是通信层成员，「把载荷当 JSON 读」本来就是它的活）。

/// 开单：上传。载荷 `{"local_path"}`，回 `{"id","key"}`（`key` 是暂存件的键，提交时交给后端）。
pub const TRANSFER_UPLOAD: &str = "transfer-upload";
/// 开单：下载。载荷 `{"remote_path","local_path"}`，回 `{"id"}`。
pub const TRANSFER_DOWNLOAD: &str = "transfer-download";
/// 🔴 **窗口经通道说得出的传输命令：恰好这两条。** 取消不是命令（停订即撤）。
pub const TRANSFER_OPS: [&str; 2] = [TRANSFER_UPLOAD, TRANSFER_DOWNLOAD];
/// 进度流的 `kind`：`transfer/<id>`。
pub const TRANSFER_KIND_PREFIX: &str = "transfer/";

/// 这个操作名是不是传输台的。
pub fn is_transfer_op(op: &str) -> bool {
    TRANSFER_OPS.contains(&op)
}

/// 一趟传输**此刻**的样子（流里的每一格都是一整份快照，不是增量 ⇒ 合并掉中间几格不丢信息）。
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Snap {
    pub got: u64,
    pub total: u64,
    /// `Some` ⇒ 收场了（流的最后一格）。
    pub end: Option<End>,
}

/// 怎么收场的。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum End {
    /// 传完了。
    Done { bytes: u64 },
    /// 失败（带下层原话）。上传那一路的暂存件**留着**给续传。
    Failed(String),
    /// 撤了（停订 / 连接断了）。上传那一路的暂存件已删。
    Cancelled,
}

/// 一趟传输要跑的那件事：拿到「撤了没有」与「报进度」两样，回传完的字节数。
pub type TransferJob = Box<
    dyn FnOnce(
            Arc<AtomicBool>,
            Arc<dyn Fn(u64, u64) + Send + Sync>,
        ) -> futures::future::BoxFuture<'static, Result<u64, String>>
        + Send,
>;

/// 一张票。
struct Ticket {
    origin: crate::origin::Origin,
    /// 上传那一路的暂存件键（同一台机器上同一个键同时只许一张票在跑）。
    key: Option<String>,
    /// 还没起跑的那件事（第一次订阅时取走）。
    job: std::sync::Mutex<Option<TransferJob>>,
    cancel: Arc<AtomicBool>,
    state: Arc<tokio::sync::watch::Sender<Snap>>,
}

impl Ticket {
    fn is_over(&self) -> bool {
        self.state.borrow().end.is_some()
    }
}

/// 进程里全部在册的票：`id → 票`。
fn desk() -> &'static std::sync::Mutex<HashMap<String, Arc<Ticket>>> {
    static D: std::sync::OnceLock<std::sync::Mutex<HashMap<String, Arc<Ticket>>>> =
        std::sync::OnceLock::new();
    D.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

fn desk_lock() -> std::sync::MutexGuard<'static, HashMap<String, Arc<Ticket>>> {
    desk().lock().unwrap_or_else(|p| p.into_inner())
}

/// **开单**：登记一张票（不起跑）。同一台机器上同一个暂存件键已有一张没收场的票 ⇒ 拒。
///
/// 判据也从这里进（喂一件合成的事，不起真连接）。
pub fn open_ticket(
    origin: &crate::origin::Origin,
    key: Option<String>,
    job: TransferJob,
) -> Result<String, String> {
    let mut d = desk_lock();
    if let Some(k) = &key {
        if d.values()
            .any(|t| t.origin == *origin && t.key.as_ref() == Some(k) && !t.is_over())
        {
            return Err("同一份文件正在往那台机器上传（同一个暂存件），等它收场再来".to_string());
        }
    }
    let id = format!("xfer-{}", uuid::Uuid::new_v4().simple());
    let (tx, _rx) = tokio::sync::watch::channel(Snap::default());
    d.insert(
        id.clone(),
        Arc::new(Ticket {
            origin: origin.clone(),
            key,
            job: std::sync::Mutex::new(Some(job)),
            cancel: Arc::new(AtomicBool::new(false)),
            state: Arc::new(tx),
        }),
    );
    Ok(id)
}

/// 订阅那一格的守卫：**流被丢掉 = 停订**。没收场就撤；无论如何把票摘掉。
struct WatchGuard {
    id: String,
    ticket: Arc<Ticket>,
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        if !self.ticket.is_over() {
            self.ticket.cancel.store(true, Ordering::SeqCst);
        }
        desk_lock().remove(&self.id);
    }
}

/// **起跑并看**：`id` 那张票的快照流（第一格是此刻，之后每变一次出一格，收场那一格是最后一格）。
///
/// - 没有这张票 / 票不是这台机器的 ⇒ `Err(("no-such-transfer", …))`（宿主原位说出来）；
/// - 已经有人在看（第二次订阅）⇒ `Err(("already-watched", …))` —— 一趟传输一个看的人，撤的语义才不含糊。
///
/// ⚠ 必须在 tokio 运行时里调（起跑是 `tokio::spawn`）；宿主的订阅口恒在路由器的任务里。
pub fn watch_ticket(
    origin: &crate::origin::Origin,
    id: &str,
) -> Result<futures::stream::BoxStream<'static, Snap>, (&'static str, String)> {
    let ticket = desk_lock()
        .get(id)
        .cloned()
        .filter(|t| t.origin == *origin)
        .ok_or_else(|| ("no-such-transfer", format!("没有这一趟传输（{id}）")))?;
    let job = ticket
        .job
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
        .ok_or_else(|| {
            (
                "already-watched",
                format!("这一趟传输已经有人在看了（{id}）"),
            )
        })?;
    let rx = ticket.state.subscribe();
    // ── 起跑 ──
    let state = Arc::clone(&ticket.state);
    let cancel = Arc::clone(&ticket.cancel);
    let sink_state = Arc::clone(&state);
    let sink: Arc<dyn Fn(u64, u64) + Send + Sync> = Arc::new(move |got, total| {
        sink_state.send_modify(|s| {
            s.got = got;
            s.total = total;
        });
    });
    tokio::spawn(async move {
        let r = job(Arc::clone(&cancel), sink).await;
        let end = match r {
            Ok(bytes) => End::Done { bytes },
            // 判的是**撤的旗**，不是错误文案（同 `download_inner` 那条理由）。
            Err(_) if cancel.load(Ordering::SeqCst) => End::Cancelled,
            Err(e) => End::Failed(e),
        };
        state.send_modify(|s| s.end = Some(end));
    });
    // ── 看 ──
    let guard = WatchGuard {
        id: id.to_string(),
        ticket,
    };
    Ok(Box::pin(futures::stream::unfold(
        (rx, guard, true, false),
        |(mut rx, guard, first, over)| async move {
            if over {
                return None;
            }
            if !first && rx.changed().await.is_err() {
                // 发送端在票里、票在守卫里 ⇒ 这一支到不了；到了就当它收场了。
                let snap = Snap {
                    end: Some(End::Failed("这一趟传输的状态丢了".to_string())),
                    ..Snap::default()
                };
                return Some((snap, (rx, guard, false, true)));
            }
            let snap = rx.borrow_and_update().clone();
            let over = snap.end.is_some();
            Some((snap, (rx, guard, false, over)))
        },
    )))
}

/// **开单**那一面（宿主 `call` 调）：读载荷、造那件事、登记。回应答的 JSON。
///
/// 失败回 `(码, 话)`，宿主原样翻成「对端答不行」（与后端命令的拒绝同一个信封）。
pub async fn transfer_call(
    cfg: RemoteConfig,
    op: &str,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, (&'static str, String)> {
    let text = |k: &str| {
        payload
            .get(k)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or((
                "bad_args",
                format!("`{op}` 少了 `{k}`，或者它不是一个字符串"),
            ))
    };
    let origin = crate::origin::Origin(cfg.origin_label());
    match op {
        TRANSFER_UPLOAD => {
            let local = text("local_path")?;
            let meta = tokio::fs::metadata(&local)
                .await
                .map_err(|e| ("io_failed", format!("读本地 {local} 失败: {e}")))?;
            if !meta.is_file() {
                return Err(("bad_args", format!("{local} 不是一份普通文件")));
            }
            let mtime_ns = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let key = staging_key(&local, meta.len(), mtime_ns);
            let job_key = key.clone();
            let job: TransferJob = Box::new(move |cancel, sink| {
                Box::pin(async move {
                    let pool = pool_for(&cfg.origin_label()).await;
                    let lease = pool.lease_transfer(&cfg).await?;
                    let r =
                        upload_to_staging(lease.get().sftp(), &local, &job_key, &cancel, &*sink)
                            .await;
                    if let Err(e) = &r {
                        evict_if_dead(&cfg.origin_label(), e).await;
                    }
                    r
                })
            });
            let id = open_ticket(&origin, Some(key.clone()), job).map_err(|e| ("busy", e))?;
            Ok(serde_json::json!({ "id": id, "key": key }))
        }
        TRANSFER_DOWNLOAD => {
            let remote = text("remote_path")?;
            let local = text("local_path")?;
            // 本机落点那道围栏：开单时就判（出声早），起跑时 `download_inner` 之前再判一次。
            guard_write(&local).map_err(|e| ("refused", e))?;
            let job: TransferJob = Box::new(move |cancel, sink| {
                Box::pin(async move {
                    guard_write(&local)?;
                    let pool = pool_for(&cfg.origin_label()).await;
                    let lease = pool.lease_transfer(&cfg).await?;
                    let r =
                        download_inner(lease.get().sftp(), &remote, &local, &cancel, &*sink).await;
                    if let Err(e) = &r {
                        evict_if_dead(&cfg.origin_label(), e).await;
                    }
                    r?;
                    tokio::fs::metadata(&local)
                        .await
                        .map(|m| m.len())
                        .map_err(|e| format!("落地之后读不到 {local}: {e}"))
                })
            });
            let id = open_ticket(&origin, None, job).map_err(|e| ("busy", e))?;
            Ok(serde_json::json!({ "id": id }))
        }
        other => Err(("bad_args", format!("`{other}` 不是传输台的命令"))),
    }
}

// === 步 23b：远端内部复制 —— 零流量走 `copy-data`，协商不到就退路 **并出声** ===

/// 一次远端内部复制的裁决。
///
/// - `None` ⇒ 走了 `copy-data`，**一个文件字节都没经过这台机器**；
/// - `Some(说明)` ⇒ **退了路**，那一句就是要摆到用户眼前的话（含实际过网字节数）。
///
/// ★ 刻意**不是 `bool`**：`设计/60 §5` 第二段逐字「不许静默退化成 2× 流量 ——
/// 用户看得见『这一趟走的是慢路』」。一个 `bool` 到了界面上还得由界面去编一句话，
/// 而**慢路为什么慢**（没协商到扩展 / 句柄不是 UTF-8 / 服务端判 `OP_UNSUPPORTED`）
/// 只有这一层答得出。把那句话做成返回值的一部分 ⇒ 静默退化在类型上就做不到。
pub type CopyVerdict = Option<String>;

/// 远端内部复制的核心。**语料由调用方给**（一个裸会话），所以它在判据里
/// 既走得了「服务端支持」这一路、也走得了「服务端不支持」那一路 —— 秤 F3 两个方向。
///
/// # 两条路
///
/// **快路**（`rs.copy_data == true` 且两个句柄都能逐字节回送）：
/// `open(src,READ)` · `open(tmp,CREATE|EXCLUDE|WRITE)` · 一条 `copy-data` · `close` ×2
/// · 换名上位。**客户端侧 `SSH_FXP_READ` / `SSH_FXP_WRITE` / `SSH_FXP_DATA` 恒 0 条。**
///
/// **退路**（协商不到 / 句柄非 UTF-8 / 服务端回 `OP_UNSUPPORTED`）：
/// 逐块 `read` → `write` 中转。字节走「远端 → 你的机器 → 远端」，**就是 2× 流量**，
/// 与 `设计/60 §5` 说的「下载再上传」同一件事、同一个代价。
///
/// ⚠ **与「下载再上传」的实现差异如实记**：这里**不落本机磁盘**（块在内存里转手），
/// 省掉一趟本机读写；过网字节数与落盘版**一模一样**，所以「不许静默 2×」那条
/// 承诺的对象没变。选它的理由：不必再造一处本机临时文件的落点与清理
/// （那会多一处 `write_site_registry` 管辖的本机写面）。
///
/// # 落地纪律照 [`upload_inner`]
///
/// 先写 `<to>.part`（**EXCLUDE** 创建，防 symlink 预置 clobber，同 `upload_atomic`）
/// → 成功后删旧 → rename 上位。**半途失败绝不在正名上留半截文件**。
///
/// # 它不守什么（逐条，别读大）
///
/// - **只复制一个普通文件**。目录递归不在这一层（`copy-data` 自己也只吃文件句柄；
///   本机 `sftp` 客户端现打同样拒：`Cannot copy non-regular file: %s`）。
/// - **退路没有取消点之外的断点续传**：取消/失败即清 `.part`，下次从头。
/// - 服务端报了 `copy-data` 却回**别的**错误状态（权限 / 磁盘满）⇒ **原样报错，不退路**。
///   退路只接「协商不到」那一族 —— 拿退路去盖真实故障，会把「远端满了」伪装成「慢了点」。
pub async fn copy_remote_path(
    rs: &crate::sftp::RawSftp,
    from: &str,
    to: &str,
    cancel: &AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<CopyVerdict, String> {
    use russh_sftp::protocol::{Packet, StatusCode};

    let total = rs
        .raw
        .stat(from.to_string())
        .await
        .ok()
        .and_then(|a| a.attrs.size)
        .unwrap_or(0);
    progress(0, total);

    let tmp = format!("{to}.part");
    let _ = rs.raw.remove(tmp.clone()).await; // best-effort 清残留/预置

    let h_src = rs
        .raw
        .open(
            from.to_string(),
            russh_sftp::protocol::OpenFlags::READ,
            attrs_empty(),
        )
        .await
        .map_err(|e| format!("打开远端源 {from} 失败: {e}"))?
        .handle;
    let h_dst = match rs
        .raw
        .open(
            tmp.clone(),
            russh_sftp::protocol::OpenFlags::CREATE
                | russh_sftp::protocol::OpenFlags::EXCLUDE
                | russh_sftp::protocol::OpenFlags::WRITE,
            attrs_empty(),
        )
        .await
    {
        Ok(h) => h.handle,
        Err(e) => {
            let _ = rs.raw.close(h_src).await;
            return Err(format!("创建远端 {tmp} 失败: {e}"));
        }
    };

    // ── 选路。**三个岔口，每个都留一句给用户的话** ──────────────────────────
    let mut verdict: CopyVerdict = if !rs.copy_data {
        Some("远端的 sftp-server 握手时没报 `copy-data` 扩展（或修订号不是 1）".to_string())
    } else if handle_is_lossy(&h_src) || handle_is_lossy(&h_dst) {
        // russh-sftp 解 SFTP 的 `string` 字段时对非 UTF-8 走 `from_utf8_lossy`
        // （现打核过 3.0.0 那份取 string 的辅助函数）⇒ 句柄里的字节被换成了 U+FFFD，
        // **再发回去就不是同一个句柄**。OpenSSH 的句柄是 4 字节大端的句柄序号，
        // 序号 ≥ 0x80 时末字节就不是合法 UTF-8 ⇒ 同一条会话开到 128 个以上句柄才碰得到。
        // ⚠ 这一条是**库的既有缺陷**（`read`/`write`/`close` 同样受影响），不是本路新增；
        //   但 `copy-data` 是唯一一处**我们自己把句柄再序列化一遍**的地方 ⇒ 这里必须判。
        Some("远端给的 SFTP 句柄含非 UTF-8 字节，库已有损解码 ⇒ 不敢照原样发回去".to_string())
    } else {
        None
    };

    let core = async {
        if verdict.is_none() {
            let body: Vec<u8> = crate::sftp::CopyDataExtension {
                read_from_handle: h_src.clone(),
                read_from_offset: 0,
                read_data_length: 0, // 0 = 一直读到 EOF（现打验过，见 sftp.rs 那张突变表）
                write_to_handle: h_dst.clone(),
                write_to_offset: 0,
            }
            .try_into()?;
            match rs.raw.extended(crate::sftp::COPY_DATA, body).await {
                Ok(Packet::Status(s)) if s.status_code == StatusCode::Ok => return Ok(0u64),
                Ok(Packet::Status(s)) if s.status_code == StatusCode::OpUnsupported => {
                    verdict = Some(
                        "远端报了 `copy-data`，可真发过去它回 `SSH_FX_OP_UNSUPPORTED`".to_string(),
                    );
                }
                Ok(Packet::Status(s)) => {
                    // 真实故障（权限 / 空间 / 路径）—— **不拿退路去盖它**。
                    return Err(format!(
                        "远端 copy-data 失败（{:?}）: {}",
                        s.status_code, s.error_message
                    ));
                }
                Ok(_) => return Err("远端对 copy-data 回了个非 STATUS 包".to_string()),
                Err(e) => return Err(format!("发 copy-data 失败: {e}")),
            }
        }
        // ── 退路：逐块中转。**这里每一块都是真的 2× 流量** ──────────────────
        let mut off: u64 = 0;
        let mut last_report: u64 = 0;
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err("已取消".to_string());
            }
            let chunk = match rs.raw.read(h_src.clone(), off, CHUNK as u32).await {
                Ok(d) => d.data,
                Err(russh_sftp::client::error::Error::Status(s))
                    if s.status_code == StatusCode::Eof =>
                {
                    break
                }
                Err(e) => return Err(format!("读远端源失败: {e}")),
            };
            if chunk.is_empty() {
                break;
            }
            let n = chunk.len() as u64;
            rs.raw
                .write(h_dst.clone(), off, chunk)
                .await
                .map_err(|e| format!("写远端目标失败: {e}"))?;
            off += n;
            if off - last_report >= PROGRESS_EVERY {
                last_report = off;
                progress(off, total);
            }
        }
        Ok(off)
    }
    .await;

    let _ = rs.raw.close(h_src).await;
    let _ = rs.raw.close(h_dst).await;

    let relayed = match core {
        Ok(n) => n,
        Err(e) => {
            let _ = rs.raw.remove(tmp.clone()).await; // 清半成品 .part
            return Err(e);
        }
    };

    // 目标原文件在此之前完好无损；此后才删旧 + 换名（russh-sftp 的 rename 不覆盖）。
    if rs.raw.stat(to.to_string()).await.is_ok() {
        rs.raw
            .remove(to.to_string())
            .await
            .map_err(|e| format!("删旧 {to} 失败: {e}"))?;
    }
    rs.raw
        .rename(tmp.clone(), to.to_string())
        .await
        .map_err(|e| format!("rename {tmp} → {to} 失败: {e}"))?;
    progress(total.max(relayed), total);

    // 退路那句话在这里才**装上读数** —— 「慢」不是形容词，是一个字节数。
    Ok(verdict
        .map(|why| format!("{why} ⇒ 退回中转：{relayed} 字节经过了你这台机器（零流量复制没走上）")))
}

/// 一个空属性块（`SSH_FXP_OPEN` 的 attrs 位图全 0）。
///
/// 抽成函数**不是**为了省字：`russh_sftp::protocol::FileAttributes::empty()` 在
/// `copy_remote_path` 里要写两遍，而那两处必须一模一样（一处带了 permissions
/// 就会在 `EXCLUDE` 创建时改变落地权限）。
fn attrs_empty() -> russh_sftp::protocol::FileAttributes {
    russh_sftp::protocol::FileAttributes::empty()
}

/// 这个 SFTP 句柄是不是已经被库有损解码过了（含 U+FFFD ⇒ 原字节回不去）。
///
/// 与从前那个判文件名的（`is_lossy_name`〔散文墓碑〕，随列目录命令删了）同一条性质、**刻意不复用那一个**：那个判的是**文件名**
/// （有损只影响显示），这个判的是**句柄**（有损意味着「发回去就是另一个句柄」）。
/// 两个判据的后果完全不同，共用一个名字会让下一个人以为改一处就够。
fn handle_is_lossy(handle: &str) -> bool {
    handle.contains('\u{FFFD}')
}

/// 远端内部复制。**零流量优先，退路必出声。**
///
/// 返回 `null` = 服务端内部复制（`copy-data`），一个文件字节都没过网；
/// 返回一串话 = **退了路**，那串话就是要给用户看的（已含实际过网字节数）。
///
/// `from`（源）与 `to`（目标）**各过一次** `guard_write` —— 照 [`sftp_rename`] 的先例：
/// 既不许把 Claude 的会话文件复制走，也不许复制成一个 Claude 数据源名
/// （往正被 Claude 打开的 jsonl 上盖一份复制品，和覆写它一样会损坏会话）。
/// ★ **池化那一段刻意留在本函数体内、不抽 `copy_inner`**（`download_inner` /
/// `upload_inner` 那两条是抽出去的）。理由是判据的形状，不是风格：
/// `remote_write_registry::the_ipc_entry_points_route_through_a_registered_write_site`
/// 那张路由表是 `(入口所在文件, 入口名, 它该转发到的已登记写点)` **一跳**的 ——
/// 中间多垫一层 `copy_inner`，「按钮 ↔ 真实写点」那条边就表达不出来，
/// 而那条边正是那一条判据存在的全部理由（「两个不同的层，一条边」）。
/// ⇒ 让 `sftp_copy` 的函数体里**直接点名** `copy_remote_path`。
///
/// **不走 `with_sftp` 的重试**：同 `download_inner` / `upload_inner`，
/// 半途失败不静默从头重来（`.part` 已清，重来由用户决定）。
#[tauri::command]
pub async fn sftp_copy(
    cfg: RemoteConfig,
    from: String,
    to: String,
    transfer_id: String,
    on_progress: tauri::ipc::Channel<TransferProgress>,
) -> Result<CopyVerdict, String> {
    guard_write(&from)?;
    guard_write(&to)?;
    let (cancel, _guard) = register_cancel(&transfer_id); // _guard 摘除注册项(含 abort)
    let r = async {
        let pool = pool_for(&cfg.origin_label()).await;
        // 🔴 **快路一个字节都没多付**：这里换掉的只是「怎么拿到那条裸通道」——
        // 从「抢 per-origin 独占锁」换成「拿一格车道 ＋ 一格通道预算」。
        // `copy_remote_path` 的语料仍然是一个裸会话，选路、包面、退路出声**逐字未动**
        // （秤 F3 两个方向照跑）。变的是：复制期间别的操作不再被这台 host 上的锁挡住。
        let lease = pool.lease_raw(&cfg).await?;
        let report_to = |done: u64, total: u64| report(&on_progress, done, total);
        copy_remote_path(&lease.rs, &from, &to, &cancel, &report_to).await
    }
    .await;
    if let Err(e) = &r {
        evict_if_dead(&cfg.origin_label(), e).await; // R1:死连不留在槽里毒化后续
    }
    r
}

#[cfg(test)]
#[path = "../../../tests/bridge/sftp_pool_tests.rs"]
mod tests;

/// **秤 F3**（`设计/17 §6.9`）：零流量复制的对拍，正反两个方向。
/// 刻意**另立一个模块**而不是塞进上面那份 —— 它自带一个合成 SFTP 服务端与一层
/// 按字节数包的计数流，是一台**台架**，与 `sftp_pool_tests` 那些单点判据不同族。
#[cfg(test)]
#[path = "../../../tests/bridge/sftp_copy_f3_tests.rs"]
mod copy_f3_tests;

/// **秤 F4**（`设计/60 §2 档③`）：多通道池那三件事的对拍 ——
/// 边传边浏览 · 并发/队列 · 断点续传。同 F3，它也是一台**台架**
/// （进程内合成 SFTP 服务端 ＋ 逐条记 `READ`/`WRITE` 偏移），与上面那些单点判据不同族。
#[cfg(test)]
#[path = "../../../tests/bridge/sftp_pool_f4_tests.rs"]
mod pool_f4_tests;

/// 〔F7c · 第三波 09-24〕**暂存区那一族**（`设计/60 §13`）：上传只写暂存区 · 撤删失败留 · 孤儿扫 ·
/// 键 · 与后端那一份常量逐字比。自带一台**逐条记改动路径**的合成 SFTP 服务端（F4 那台不记路径、
/// 不认建目录与列目录），所以另立一个模块。
#[cfg(test)]
#[path = "../../../tests/bridge/sftp_staging_tests.rs"]
pub(crate) mod staging_tests;

/// 〔F7c · 第三波 09-24〕**SFTP 那一族收到只剩传输**的恒等登记（`设计/60 §13.4`）：Tauri 命令 == 待收 ·
/// 远端写函数 == 暂存区那两个 ∪ 待收 · 通道上的传输操作恰好两条。待收每一格要求消费者此刻在盘上。
#[cfg(test)]
#[path = "../../../tests/bridge/sftp_family_registry_tests.rs"]
mod family_registry;
