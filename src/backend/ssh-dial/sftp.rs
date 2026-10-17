//! **SFTP 住本机常驻后端**：在池里那条 SSH 连接上开 sftp 子系统 ·
//! **远端写围栏（只许暂存区一处）** · 远端写原语（部署 `deploy::install` · 传输 `control/transfer.rs` 调它们）。
//!
//! # 要求
//!
//! 「SFTP 怎么进单一常驻后端」一题选「**进本机常驻后端，只写暂存区**」：SFTP 连接由本机常驻后端管、
//! 与其它 SSH 复用；只往远端暂存区写，落进用户目录仍只经远端后端文件管理提交（`files-commit-upload`）；
//! 界面进程零 SSH。F08（自部署后端）也只往暂存区放字节：换进 `~/.cc-monitor/bin/`、卸载都由传上去的那一份在那台上自己做
//! （`--place-self` · `--unplace-self`）⇒ 写根只有暂存区一处。
//!
//! # 🔴 本文件是后端里**唯一**一份能改远端文件系统的代码（红线 `I7`，`INVARIANTS §41.6`）
//!
//! `readonly_guard::remote_write_layer` 钉三件：① 生产段里命中远端写能力网 / 动词网的文件 == `{ssh-dial/sftp.rs}`；
//! ② 本文件声明的写根（[`STAGING_ROOT`]）== `~/.cc-monitor/staging`；
//! ③ 本文件里每个含远端改动动词的函数，第一个改动之前先有一次 [`fenced_remote`] 调用。
//! ⇒ **写原语只许住这里、只许从这里的函数进**；别的文件（`deploy/install.rs` · `control/transfer.rs`）只调它们。
//!
//! # 围栏（[`fence_lexical`] 纯函数 ＋ [`fenced_remote`] 解链接）
//!
//! - 路径归一成 home 相对（home = SFTP `realpath(".")`，即起始目录）：绝对路径必须在 `<home>/` 底下；
//! - 词法：不许 `..` / `.` / 空段；前两段必须恰好是写根；写文件必须**在根底下**（不是根自己）；
//!   建目录另放行根自己与它的祖先 `.cc-monitor`（首次部署要建）；
//! - 写根本身：`.cc-monitor` 与写根 `lstat` 不是链接、`realpath` 仍在 home 底下（挡「`staging` 本身是指向 `~/.ssh` 的链接」）；
//!   与本机后端自家目录同一条规矩（不是链接；本机那一份另核属主与权限位：`platform/fs/own.rs::own_dir`）；
//! - 解链接：目标的父目录 `realpath` 之后必须仍在写根的 `realpath` 之下（挡「`staging/x` 是指向 `~/.ssh` 的链接」）；
//!   打开写的那一格另问一次 `lstat`，最后一段是链接就拒（开写会跟过去）。
//!
//! # 买不到什么（逐条）
//!
//! - **TOCTOU**：判定与动手之间有窗（同 `files_write` 自陈的那一格）。
//! - **Windows 远端**：home 是 `C:\…` 形时归一化不认它 ⇒ 一律拒（出声，不猜）。一格读数都没有。
//! - **`ChrootDirectory` / `internal-sftp -d`**：起始目录不是 `$HOME` 的机器上，这里的「home」是 SFTP 的起始目录，
//!   与远端后端的 `$HOME` 不是同一个 ⇒ 暂存件提交会答「不在」（`files_commit` 头注那一格照旧）。

use crate::Said;
use copy_core::copy_text;
use std::sync::Arc;

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, OpenFlags};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use crate::connect::Linked;
use crate::uses::{Lane, Lease};
use crate::{pool, DialRequest, StageSink};

/// 🔴 **远端写根：恰好暂存区这一处**（home 相对）。`readonly_guard::remote_write_layer` 现读本行源码逐字比。
///
/// 上传只写这里，落进用户目录由远端后端 `files-commit-upload` 做；自部署也只把后端字节放进这里，换进 `~/.cc-monitor/bin/` · 卸载都由
/// 传上去的那一份自己在那台上做（`--place-self` · `--unplace-self`）。与 `control/files_commit.rs::STAGING_DIR` 相等（同一个 crate，判据直接比）。
pub const STAGING_ROOT: &str = ".cc-monitor/staging";

/// 写根的祖先：后端的家。只在「建目录」那一形里放行（首次部署要建它）。
const ROOTS_HOME: &str = ".cc-monitor";

/// **远端写的规矩**（宿主装进 [`crate::env::Env::files`]，一个进程一种；线上交来的请求改不动它）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteRules {
    /// 一个字节都不许写（没装宿主那一套时的缺省：宁可写不成，不许写错地方）。
    ReadOnly,
    /// 部署：只许 [`STAGING_ROOT`] 那一处（后端 · 手机）；建出来的目录当场收成 `dir_mode`（宿主交后端自家目录那一个权限位）。
    Deploy { dir_mode: u32 },
}

/// 一条开在池里那条连接上的 SFTP 会话（占这条连接的一格通道，传输另占一格车道）。
pub struct Session {
    /// 只在 `Drop` 那一刻被取走（停进空位）—— 之后没有人还拿得到 `&self`。
    sftp: Option<SftpSession>,
    /// SFTP 起始目录的真路径（`realpath(".")`）—— 围栏把绝对路径归一成相对它。
    home: String,
    /// 保活 ＋ 用完去哪。
    keep: Keep,
}

/// 会话要攥着的东西。**不许省**：连接句柄一 drop 整条连接就断，这条通道跑在它上面；预算那一格要攥到会话用完。
enum Keep {
    /// 台架 / files 链路：攥着就好，用完关掉（生产上 files 链路是 `(pool::Permit, Arc<Linked>)`）。
    Hold(#[allow(dead_code)] Box<dyn std::any::Any + Send + Sync>),
    /// 传输：用完**停进这条连接的空位**（[`Parked`]）—— 连同这一格传输许可；空位已有 / 连接关了 ⇒ 照常关掉。
    Park(Option<pool::Permit>, Arc<Linked>),
}

/// **一条停着的空闲 sftp 会话**，住在它那条连接里（`Linked::idle_sftp`，每条连接**一个空位**）。
///
/// 读数：1 KB 小文件每件 ≈ 9.7 个往返，其中开 sftp 通道（开 channel · 请求子系统 · `SSH_FXP_INIT` ·
/// `realpath .`）占 4 个。停一个 ⇒ 下一趟只花 1 个往返验活（`realpath .` 对一遍 home）。
/// - **它不托连接**：空位住在 `Linked` 里、不持 `Arc<Linked>` ⇒ 连接照旧随最后一个用户 / 托它的主连接一起走，空位随之没了。
/// - **它连同那一格传输许可一起停**（车道 ＋ 通道）：远端 `MaxSessions` 数的正是这条还开着的通道，账要对得上。
/// - **挤掉**：别的放置借不到格时，池先把空位里的关掉、还出那一格（`pool::Conn::reclaim_idle`）—— 事件驱动的回收，不是定时器。
pub struct Parked {
    sftp: SftpSession,
    home: String,
    permit: pool::Permit,
}

impl Drop for Session {
    fn drop(&mut self) {
        let Keep::Park(permit, linked) = &mut self.keep else {
            return;
        };
        let (Some(sftp), Some(permit)) = (self.sftp.take(), permit.take()) else {
            return;
        };
        if linked.session.is_closed() {
            return;
        }
        let mut slot = linked.idle_sftp.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            *slot = Some(Parked {
                sftp,
                home: std::mem::take(&mut self.home),
                permit,
            });
        }
    }
}

/// 在 channel 上请求 sftp 子系统，装进一个**显式 `Send`** 的盒子（理由同 `uses.rs::exec`：
/// 借 `&self` 的 `async fn` 放进要 `tokio::spawn` 的任务里会撞「`Send` is not general enough」）。
fn subsystem(
    channel: &russh::Channel<russh::client::Msg>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), russh::Error>> + Send + '_>> {
    Box::pin(channel.request_subsystem(true, "sftp"))
}

/// 开一条 SFTP 会话：过这条连接的预算（`lane`）→ 开 session channel → 请求 sftp 子系统 → `SSH_FXP_INIT` →
/// 问一次起始目录的真路径。**三到四个往返**。传输那一形用完停进这条连接的空位（[`Parked`]），
/// 下一趟传输先取它（[`open_for_transfer`]，1 个往返验活）—— 旧 monitor 池的空闲栈随它一起删了，今天回来的是「每条连接一个空位」。
pub async fn open(
    lease: &mut Lease,
    req: &DialRequest,
    stages: &StageSink,
) -> Result<Session, Said> {
    let (channel, permit) = lease.session_channel(req, stages).await?;
    subsystem(&channel)
        .await
        .map_err(|e| Said::with_raw(copy_text("beSftp.open.subsystem", &[]), &e))?;
    let stream = channel.into_stream();
    let linked = Arc::clone(lease.linked());
    if lease.lane() != Lane::Transfer {
        // files 链路（部署）：用完就关。
        return Session::over(stream, Box::new((permit, linked))).await;
    }
    // 传输：用完停进这条连接的空位（`Parked`）。
    let (sftp, home) = init(stream).await?;
    Ok(Session {
        sftp: Some(sftp),
        home,
        keep: Keep::Park(Some(permit), linked),
    })
}

/// 在这个身份的一族里找一条**停着的空闲会话**（只在传输能放的成员上找 —— 与放置同一条分道规矩），
/// 取出来、验活（`realpath .` 与停进去时的 home 相同）。验不过 ⇒ 丢掉它（关通道、还格），接着找下一条。
async fn take_parked(req: &DialRequest) -> Option<Session> {
    if req.probe || req.stages {
        return None;
    }
    let key = pool::identity(req);
    for linked in pool::ssh().transfer_candidates(&key) {
        let parked = linked
            .idle_sftp
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let Some(p) = parked else {
            continue;
        };
        // 验活这一问被打断（撤了 / 界面走了）⇒ 摘掉这条连接（`pool::Watch`：黑洞上它会一直等下去）。
        let watch = pool::Watch::new(pool::ssh(), &key, &linked);
        let alive = p.sftp.canonicalize(".").await;
        watch.done();
        match alive {
            Ok(h) if h == p.home => {
                tracing::info!("dial: {} 上复用了一条停着的 sftp 会话", linked.endpoint);
                return Some(Session {
                    sftp: Some(p.sftp),
                    home: p.home,
                    keep: Keep::Park(Some(p.permit), linked),
                });
            }
            other => tracing::info!(
                "dial: {} 上停着的 sftp 会话验不过（{other:?}），丢掉",
                linked.endpoint
            ),
        }
    }
    None
}

/// 一份拨号请求（传输台开单时读进来、起跑时拿它开会话）。包一层是为了让传输台**只经本文件**够到拨号：
/// 它手里不必有 `DialRequest` 这个类型（文件管理那一面的外向边钉在 `module_boundary_guard::OUTWARD`）。
#[derive(Clone)]
pub struct Dial(DialRequest);

impl Dial {
    pub fn parse(v: &serde_json::Value) -> Result<Dial, String> {
        crate::machine::resolve(v).map(Dial).map_err(|(_, m)| m)
    }

    /// 一份组好的拨号请求（手机交来的那一形 · 后端已经 `resolve` 过的）。
    pub fn of(req: DialRequest) -> Dial {
        Dial(req)
    }
}

/// 传输那一趟的会话：先找一条停着的空闲会话（1 个往返）；没有 ⇒ 在池里放置（同身份复用 / 分道到批量连接）
/// → 过**传输车道**开一条 sftp 通道（4 个往返）。
pub async fn open_for_transfer(d: &Dial) -> Result<Session, Said> {
    let req = &d.0;
    if let Some(s) = take_parked(req).await {
        return Ok(s);
    }
    let stages = StageSink::new(false);
    let mut lease = Lease::take(req, &stages, Lane::Transfer)
        .await
        .map_err(|(e, _)| e)?;
    open(&mut lease, req, &stages).await
}

/// `SSH_FXP_INIT` ＋ 问一次起始目录的真路径。
async fn init<S>(stream: S) -> Result<(SftpSession, String), Said>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let sftp = SftpSession::new(stream).await.map_err(|e| {
        Said::with_raw(
            copy_text("beSftp.open.initFailed", &[("why", &why_of(&e))]),
            &e,
        )
    })?;
    let home = sftp.canonicalize(".").await.map_err(|e| {
        Said::with_raw(copy_text("beSftp.open.noHome", &[("why", &why_of(&e))]), &e)
    })?;
    Ok((sftp, home))
}

impl Session {
    /// 在一条**已经是 sftp 子系统**的字节流上起会话：`SSH_FXP_INIT` ＋ 问一次起始目录的真路径。
    /// `keep` 是要与会话同生死的东西（预算那一格 ＋ 底层连接）。
    pub async fn over<S>(
        stream: S,
        keep: Box<dyn std::any::Any + Send + Sync>,
    ) -> Result<Session, Said>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (sftp, home) = init(stream).await?;
        Ok(Session {
            sftp: Some(sftp),
            home,
            keep: Keep::Hold(keep),
        })
    }

    fn sftp(&self) -> &SftpSession {
        match &self.sftp {
            Some(s) => s,
            None => unreachable!("sftp 会话只在 Drop 里被取走"),
        }
    }

    /// SFTP 起始目录的真路径。
    pub fn home(&self) -> &str {
        &self.home
    }
}

// ═══ 围栏 ═══════════════════════════════════════════════════════════════════════════

/// 这一次改动是什么形。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// 建 / 写 / 删 / 改名一份文件：必须**在某个根底下**（不是根自己）。
    File,
    /// 建一个目录：根底下、根自己、或它的祖先 `.cc-monitor`。
    Dir,
}

/// 一次改动被拒的两档：围栏拦的（换条路径才有意义）· 盘上没成（重试才有意义）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// `raw`：解链接那一问的下层原话（只有解不开那两形带）。
    Fenced { said: String, raw: Option<String> },
    /// `raw`：下层原话（进复制详情，不上句子；没有 ⇒ `None`）。
    Io { said: String, raw: Option<String> },
}

impl Refusal {
    pub fn code(&self) -> &'static str {
        match self {
            Refusal::Fenced { .. } => "fenced",
            Refusal::Io { .. } => "io",
        }
    }
    pub fn message(&self) -> &str {
        match self {
            Refusal::Fenced { said: m, .. } | Refusal::Io { said: m, .. } => m,
        }
    }
    /// 下层原话（盘上没成 · 围栏解链接解不开那几形带）。
    pub fn raw(&self) -> Option<&str> {
        match self {
            Refusal::Io { raw, .. } | Refusal::Fenced { raw, .. } => raw.as_deref(),
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

fn fenced(msg: String) -> Refusal {
    Refusal::Fenced {
        said: msg,
        raw: None,
    }
}

/// 围栏那一问（解链接）没答出来：句子（已带原因词）＋ 下层原话。
fn fenced_raw(said: String, raw: impl std::fmt::Display) -> Refusal {
    let r = Said::with_raw(said, raw);
    Refusal::Fenced {
        said: r.said,
        raw: r.raw,
    }
}

/// 盘上没成：句子（已带原因词）＋ 下层原话。
fn io_raw(said: String, raw: impl std::fmt::Display) -> Refusal {
    let r = Said::with_raw(said, raw);
    Refusal::Io {
        said: r.said,
        raw: r.raw,
    }
}

impl From<Refusal> for Said {
    fn from(r: Refusal) -> Said {
        Said {
            said: r.message().to_string(),
            raw: r.raw().map(str::to_string),
        }
    }
}

/// 下层错 ⇒ 句子里那一格原因词：SFTP 那一层的按状态（[`why_of_sftp`]），IO 错按种类（[`why_of_io`]）；其余「原因不明」。
/// 原话不在这里丢：调用方把它交给 [`Said`] / [`Refusal::Io`] 进复制详情。
fn why_of(e: &(dyn std::error::Error + 'static)) -> String {
    if let Some(io) = e.downcast_ref::<std::io::Error>() {
        return why_of_io(io);
    }
    match e.downcast_ref::<russh_sftp::client::error::Error>() {
        Some(se) => why_of_sftp(se),
        None => copy_core::io_reason(std::io::ErrorKind::Other),
    }
}

/// SFTP 文件句柄读写那一形的原因词。`russh_sftp` 把失败包进 `io::Error` 有两种包法，种类多是 Other：
/// ① 整个 SFTP 错包在里面（取出来按 [`why_of_sftp`]）；② 只剩服务端那句话（状态码丢了 —— 认 SFTP v3 标准码的那几句标准话）。
/// 都不是 ⇒ 按 IO 种类（`copy_core::io_reason`）。传输台写 / 读 / 挪远端那份时经这里。
pub fn why_of_io(e: &std::io::Error) -> String {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::TimedOut => return copy_text("reason.sftp.noAnswer", &[]),
        K::BrokenPipe | K::ConnectionReset => {
            // 传输中对端断了 ＝ SFTP 标准码 7（连接断）那一件。
            return copy_core::sftp_status_reason(7);
        }
        _ => {}
    }
    if e.kind() != K::Other {
        return copy_core::io_reason(e.kind());
    }
    let Some(inner) = e.get_ref() else {
        return copy_core::io_reason(K::Other);
    };
    if let Some(se) = inner.downcast_ref::<russh_sftp::client::error::Error>() {
        return why_of_sftp(se);
    }
    // 服务端那句标准话（OpenSSH sftp-server 按码给的那几句；大小写不论）。
    let code = match inner.to_string().trim().to_ascii_lowercase().as_str() {
        "no such file" => 2,
        "permission denied" => 3,
        "failure" => 4,
        "bad message" => 5,
        "no connection" => 6,
        "connection lost" => 7,
        "operation unsupported" => 8,
        _ => 0,
    };
    copy_core::sftp_status_reason(code)
}

/// `russh_sftp` 的错 ⇒ 原因词：状态按码（`copy_core::sftp_status_reason`）· 等不到回包 ⇒ 无应答 · 超出那台的限额 ⇒ 超出上限 ·
/// 回了不该回的包 ⇒ 程序出错（两端协议对不上）；其余（底层 IO 已成字串 · 通道收发断了）⇒ 原因不明。
fn why_of_sftp(e: &russh_sftp::client::error::Error) -> String {
    use russh_sftp::client::error::Error as E;
    match e {
        E::Status(st) => copy_core::sftp_status_reason(st.status_code as u32),
        E::Timeout => copy_text("reason.sftp.noAnswer", &[]),
        E::Limited(_) => copy_text("reason.sftp.overLimit", &[]),
        E::UnexpectedPacket => copy_text("reason.sftp.badMessage", &[]),
        E::IO(_) | E::UnexpectedBehavior(_) => copy_core::io_reason(std::io::ErrorKind::Other),
    }
}

/// **纯词法那一道**：把 `path` 归一成 home 相对，判它落不落在写根里。
///
/// 回 `(home 相对路径, 它在哪个根底下)`；第二项 `None` = 它是 `.cc-monitor` 本身（只有建目录那一形放行）。
pub fn fence_lexical(
    home: &str,
    path: &str,
    intent: Intent,
) -> Result<(String, Option<&'static str>), String> {
    let p = path.trim();
    if p.is_empty() {
        return Err(copy_text("beSftp.fence.empty", &[]));
    }
    let rel = if let Some(abs) = p.strip_prefix('/') {
        let h = home.trim_end_matches('/');
        if !home.starts_with('/') || h.is_empty() {
            return Err(copy_text("beSftp.fence.homeNotAbsolute", &[("home", home)]));
        }
        let Some(rest) = abs.strip_prefix(h.trim_start_matches('/')) else {
            return Err(copy_text(
                "beSftp.fence.outsideHome",
                &[("path", p), ("home", home)],
            ));
        };
        let Some(rest) = rest.strip_prefix('/') else {
            return Err(copy_text(
                "beSftp.fence.outsideHome",
                &[("path", p), ("home", home)],
            ));
        };
        rest
    } else {
        p
    };
    let rel = rel.trim_end_matches('/');
    let comps: Vec<&str> = rel.split('/').collect();
    if comps
        .iter()
        .any(|c| c.is_empty() || *c == "." || *c == ".." || c.contains('\0'))
    {
        return Err(copy_text("beSftp.fence.irregular", &[("path", p)]));
    }
    let rel = comps.join("/");
    if intent == Intent::Dir && rel == ROOTS_HOME {
        return Ok((rel, None));
    }
    let under = rel
        .strip_prefix(STAGING_ROOT)
        .is_some_and(|rest| rest.starts_with('/'));
    if under || (intent == Intent::Dir && rel == STAGING_ROOT) {
        return Ok((rel, Some(STAGING_ROOT)));
    }
    Err(copy_text(
        "beSftp.fence.outsideRoot",
        &[("path", p), ("root", STAGING_ROOT)],
    ))
}

/// 父目录（home 相对，`/` 分隔）。
fn parent_of(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(p, _)| p).unwrap_or(".")
}

/// 写根**本身**：`.cc-monitor` 与那个根都不是链接、解开后仍在家目录底下（根是链接 ⇒ 后面「在根底下」恒成立）。
/// 回解开的根；`.cc-monitor` 或根还不在 ⇒ `None`（底下没有东西可写错，建的那一下再问）。
async fn own_root(s: &Session, root: &str) -> Result<Option<String>, Refusal> {
    let unresolved = |dir: &str, e: &russh_sftp::client::error::Error| {
        fenced_raw(
            copy_text(
                "beSftp.fence.rootUnresolved",
                &[("root", dir), ("why", &why_of(e))],
            ),
            e,
        )
    };
    let not_own = |real: &str| {
        fenced(copy_text(
            "beSftp.fence.rootNotOwn",
            &[("root", root), ("real", real)],
        ))
    };
    for dir in [ROOTS_HOME, root] {
        match s.sftp().symlink_metadata(dir).await {
            Ok(m) if m.is_symlink() => {
                let real = s.sftp().canonicalize(dir).await.unwrap_or_default();
                return Err(not_own(&real));
            }
            Ok(_) => {}
            Err(russh_sftp::client::error::Error::Status(st))
                if st.status_code == russh_sftp::protocol::StatusCode::NoSuchFile =>
            {
                return Ok(None)
            }
            Err(e) => return Err(unresolved(dir, &e)),
        }
    }
    let real = s
        .sftp()
        .canonicalize(root)
        .await
        .map_err(|e| unresolved(root, &e))?;
    let under_home = real
        .strip_prefix(s.home.as_str())
        .is_some_and(|rest| rest.starts_with('/'));
    if !under_home {
        return Err(not_own(&real));
    }
    Ok(Some(real))
}

/// **围栏**：词法那一道 ＋ 写根本身是家目录里的真目录（[`own_root`]）＋ 父目录解链接之后仍在那个根之下。回 home 相对路径（SFTP 起始目录就是 home）。
pub async fn fenced_remote(s: &Session, path: &str, intent: Intent) -> Result<String, Refusal> {
    match crate::env::get().files {
        WriteRules::ReadOnly => {
            return Err(fenced(copy_text(
                "beSftp.fence.readOnly",
                &[("path", path.trim())],
            )))
        }
        WriteRules::Deploy { .. } => {}
    }
    let (rel, root) = fence_lexical(&s.home, path, intent).map_err(fenced)?;
    let Some(root) = root else {
        return Ok(rel);
    };
    let Some(real_root) = own_root(s, root).await? else {
        return Ok(rel);
    };
    if rel == root {
        return Ok(rel);
    }
    let parent = parent_of(&rel);
    let real_parent = s.sftp().canonicalize(parent).await.map_err(|e| {
        fenced_raw(
            copy_text(
                "beSftp.fence.parentUnresolved",
                &[("parent", parent), ("why", &why_of(&e))],
            ),
            &e,
        )
    })?;
    let inside = real_parent == real_root
        || real_parent
            .strip_prefix(real_root.as_str())
            .is_some_and(|rest| rest.starts_with('/'));
    if !inside {
        return Err(fenced(copy_text(
            "beSftp.fence.escaped",
            &[
                ("parent", parent),
                ("real", &real_parent),
                ("root", &real_root),
            ],
        )));
    }
    Ok(rel)
}

// ═══ 远端写原语（每个都先过围栏；只住这一份文件）═══════════════════════════════════════

/// 原子上传 `bytes` 到 `path`，权限 `mode`（只在 open-create 的属性里设一次）。
///
/// 搬自 monitor `sftp.rs` 的 F08 原子上传（那一份随界面进程零 SFTP 删了），序列逐步相同：
/// 写 `<path>.<这一趟独有的后缀>.tmp`（从前是固定 `<path>.tmp` ＋ 先删残留；**EXCLUDE** 创建：临时件是一条预置的链接也不会跟过去 —— F89a 审计）
/// → 旧目标**改名成 `.<后缀>.bak`**（不是删：「先删旧」一旦后续改名失败就丢原件，DN-7 订正过那句注释）
/// → 临时件上位 → 删 `.bak`。临时件与 `.bak` 都在目标**同一个父目录**里，那个父目录已经过了围栏。
/// 标准 SFTP 的改名不覆盖（`russh-sftp` 没有 `posix-rename@openssh.com`）⇒ 只能这样近似原子。两个部署者交错时临时件 / 备份件各是各的（唯一名）；`rel → .bak` 与 `tmp → rel` 之间 `rel` 有一瞬不在 ⇒ 部署不经它换落点（只放进暂存区，落点由那一份自己 `--place-self` 原子换进去）。
///
/// ⚠ **改名之后绝不 `set_metadata` 兜底 chmod** —— 真机 e2e 实证：OpenSSH sftp-server 上那一次 setstat 把刚上位的
/// 后端**截成 0 字节** ⇒ 不可 exec → 连接 EOF → 标记变空 → 无限重部署。权限只在 open-create 的属性里设一次。
/// ⚠ 〔2026-09-20 现打订正，随函数搬来〕「即便只设 permissions、`size=None` 也会截断」那句括号是推断、今天复现不出来
/// （真 `OpenSSH_10.2p1` 上 permissions-only 的 SETSTAT 前后都是 16 字节）；最可能当年那个属性块真的带了 size。
/// 🔴 **但这条禁令不放宽**：事故是真的、只量了一个服务端版本、部署路上多一次 setstat 收益为零风险是变砖。
/// ⚠ 与改权限命令的关系：那条命令（后端 `files-chmod`）**要**改权限，是它的本职，不归这条禁令管。
pub async fn put_atomic(s: &Session, path: &str, bytes: &[u8], mode: u32) -> Result<(), Refusal> {
    let rel = fenced_remote(s, path, Intent::File).await?;
    // 〔临时件名唯一〕临时件与备份件都带**这一趟独有**的后缀：两个部署者（两台 monitor 各自的常驻后端）
    //   同时往同一个落点放字节时，谁也不删谁的那一份（从前是固定的 `<rel>.tmp` / `<rel>.bak` ＋「先删残留」—— 那一删删的可能是
    //   别人正在写的那一份，审计 E3 子形 2）。仍 `EXCLUDE` 新建：真撞了名 ⇒ 当场失败，不会写进别人的那一份。
    let tmp = format!("{rel}.{}.tmp", trip_tag());
    // 失败那几支删**自己这一趟**的临时件；连接已经断了（`sender dropped`）⇒ 删不掉：说一句，
    //   留下的那一份由下次连上的部署计划认领（`deploy::stale_leftovers`）。
    let drop_own_tmp = || async {
        if let Err(e) = s.sftp().remove_file(tmp.clone()).await {
            tracing::warn!("dial: 上传没成，临时件 {tmp} 也没删掉（{e}）—— 下次连上由部署计划清");
        }
    };
    let attrs = FileAttributes {
        permissions: Some(mode),
        ..Default::default()
    };
    let mut file = s
        .sftp()
        .open_with_flags_and_attributes(
            tmp.clone(),
            OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
            attrs,
        )
        .await
        .map_err(|e| {
            io_raw(
                copy_text(
                    "beSftp.put.writeFailed",
                    &[("path", &tmp), ("why", &why_of(&e))],
                ),
                &e,
            )
        })?;
    let written = async {
        file.write_all(bytes).await.map_err(|e| {
            io_raw(
                copy_text(
                    "beSftp.put.writeFailed",
                    &[("path", &tmp), ("why", &why_of(&e))],
                ),
                &e,
            )
        })?;
        // `write_all` 只把 WRITE 包入队；ack 只在 flush / shutdown 里收（同 monitor 那一份的理由）。
        file.flush().await.map_err(|e| {
            io_raw(
                copy_text(
                    "beSftp.put.writeFailed",
                    &[("path", &tmp), ("why", &why_of(&e))],
                ),
                &e,
            )
        })?;
        file.shutdown().await.map_err(|e| {
            io_raw(
                copy_text(
                    "beSftp.put.writeFailed",
                    &[("path", &tmp), ("why", &why_of(&e))],
                ),
                &e,
            )
        })
    }
    .await;
    drop(file);
    if let Err(e) = written {
        // 删的是**自己这一趟**建的那一份（名字只有这一趟知道）。
        drop_own_tmp().await;
        return Err(e);
    }
    let bak = if s.sftp().try_exists(rel.clone()).await.unwrap_or(false) {
        let b = format!("{rel}.{}.bak", trip_tag());
        if let Err(e) = s.sftp().rename(rel.clone(), b.clone()).await {
            drop_own_tmp().await;
            return Err(io_raw(
                copy_text(
                    "beSftp.put.backupFailed",
                    &[("path", &rel), ("backup", &b), ("why", &why_of(&e))],
                ),
                &e,
            ));
        }
        Some(b)
    } else {
        None
    };
    if let Err(e) = s.sftp().rename(tmp.clone(), rel.clone()).await {
        drop_own_tmp().await;
        // 自己挪走的那份旧的：落点还空着 ⇒ 挪回去（不留一个没有后端的落点）；
        //   落点已经被另一个部署者放上了新的 ⇒ 那份旧的没人要了，删掉（不留一个没人认领的备份件）。
        if let Some(b) = bak {
            if s.sftp().try_exists(rel.clone()).await.unwrap_or(true) {
                let _ = s.sftp().remove_file(b).await;
            } else {
                let _ = s.sftp().rename(b, rel.clone()).await;
            }
        }
        return Err(io_raw(
            copy_text(
                "beSftp.put.renameFailed",
                &[("tmp", &tmp), ("path", &rel), ("why", &why_of(&e))],
            ),
            &e,
        ));
    }
    if let Some(b) = bak {
        let _ = s.sftp().remove_file(b).await;
    }
    Ok(())
}

/// **原子上传 ＋ 整份读回比对**（部署那一趟放进暂存区：`deploy` crate，桌面手机同一份；`files` 链路的 `put`（`verify`）也是它）：
/// [`put_atomic`] 放上去 → [`read_all`] 读回 → [`verify_readback`] 判；**读回不对 ⇒ 当场删掉传坏的那一份**（删不掉也照实说）。
///
/// 删的理由：一份传坏的字节留着没用，下一趟按残件清还得等期限；当场删掉，下一趟从头传。
/// 中途断了（读回之前 · 删之前）：留下的那一份是唯一名的暂存件，下次连上照计划当残件清。
pub async fn put_verified(s: &Session, path: &str, bytes: &[u8], mode: u32) -> Result<(), Refusal> {
    put_atomic(s, path, bytes, mode).await?;
    let back = read_all(s, path).await;
    let Err(bad) = verify_readback(path, bytes, back.as_deref()) else {
        return Ok(());
    };
    Err(match remove(s, path).await {
        Ok(_) => Refusal::Io {
            said: copy_text("rsSftp.upload.badRemoved", &[("bad", &bad)]),
            raw: None,
        },
        // 删也没删成：那一句说清两件事，删那一步的原话进复制详情。
        Err(e) => Refusal::Io {
            said: copy_text("rsSftp.upload.badKept", &[("bad", &bad)]),
            raw: Some(
                [Some(e.message()), e.raw()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        },
    })
}

/// 判一次上传的读回（纯函数）：`actual` = 读回来的那一份（读不回 ⇒ `None`）。逐字节相同才过；
/// 不过 ⇒ 那一句（读不回 · 长度不同 · 长度相同内容不同、首个差异在哪）。
///
/// 按字节而不是按字符串：上传的是**可执行二进制**。此前完全缺失的那一环（T04 审计①）：
/// 传输损坏的后端二进制照样被当成装好了 → 下次判「已是最新，跳过」→ 坏二进制永久驻留，而用户看到的是部署成功。
pub fn verify_readback(path: &str, expected: &[u8], actual: Option<&[u8]>) -> Result<(), String> {
    let Some(got) = actual else {
        return Err(copy_text("rsSftp.verify.unreadable", &[("path", path)]));
    };
    if got.len() != expected.len() {
        return Err(copy_text(
            "rsSftp.verify.lengthDiffers",
            &[
                ("path", path),
                ("expectedLen", &expected.len().to_string()),
                ("gotLen", &got.len().to_string()),
            ],
        ));
    }
    match expected.iter().zip(got).position(|(x, y)| x != y) {
        None => Ok(()),
        Some(at) => Err(copy_text(
            "rsSftp.verify.contentDiffers",
            &[
                ("path", path),
                ("expectedLen", &expected.len().to_string()),
                ("at", &at.to_string()),
            ],
        )),
    }
}

/// 一趟上传独有的后缀：pid ⊕ 纳秒 ⊕ 进程内计数（十六进制）。**不是**密码学随机 —— 它只要「两个部署者不撞」，
/// 撞了由 `EXCLUDE` 当场挡住（失败，不是写进别人的那一份）。
fn trip_tag() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = crate::env::get()
        .facts
        .now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    format!(
        "{:x}-{:x}-{:x}",
        std::process::id(),
        nanos,
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// 部署往暂存区放的那一份的名字：`<stem>.<trip_tag>.new`（每趟一个；没换进落点就断了 ⇒ 下一趟按 [`is_trip_leftover`] 认作残件）。
pub fn staged_name(stem: &str) -> String {
    format!("{stem}.{}.new", trip_tag())
}

/// 这个名字是不是一趟部署留下的件：[`put_atomic`] 的临时件 / 备份件（`<名>.<trip_tag>.tmp` · `.bak`）或部署放进暂存区、
/// 还没换进落点的那一份（`<名>.<trip_tag>.new`；`trip_tag` = 三段非空小写十六进制、`-` 相连）。形状只认这几种 —— 别的名字不是我们放的，一个不碰。
pub fn is_trip_leftover(name: &str) -> bool {
    let Some(stem) = [".tmp", ".bak", ".new"]
        .iter()
        .find_map(|sfx| name.strip_suffix(sfx))
    else {
        return false;
    };
    let Some((base, tag)) = stem.rsplit_once('.') else {
        return false;
    };
    let hex = |p: &str| !p.is_empty() && p.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    let parts: Vec<&str> = tag.split('-').collect();
    !base.is_empty() && parts.len() == 3 && parts.iter().all(|p| hex(p))
}

/// 列一个目录：`(名字, 修改时间秒)`；列不出 ⇒ `None`。只读，不过围栏。
pub async fn list_dir(s: &Session, path: &str) -> Option<Vec<(String, Option<u64>)>> {
    let rd = s.sftp().read_dir(path.to_string()).await.ok()?;
    Some(
        rd.map(|e| (e.file_name(), e.metadata().mtime.map(u64::from)))
            .collect(),
    )
}

/// 列一层目录：`(名, 是不是目录, 大小)`（`.` 与 `..` 不算）。读，不过围栏。
pub async fn list_entries(s: &Session, path: &str) -> Result<Vec<(String, bool, u64)>, Said> {
    let rd = s.sftp().read_dir(path.to_string()).await.map_err(|e| {
        Said::with_raw(
            copy_text(
                "beSftp.list.failed",
                &[("path", path), ("why", &why_of(&e))],
            ),
            &e,
        )
    })?;
    Ok(rd
        .filter(|e| e.file_name() != "." && e.file_name() != "..")
        .map(|e| {
            let m = e.metadata();
            (e.file_name(), m.is_dir(), m.size.unwrap_or(0))
        })
        .collect())
}

/// 删一份文件。回「真删了」（不在 ⇒ `false`，不算错）。
pub async fn remove(s: &Session, path: &str) -> Result<bool, Refusal> {
    let rel = fenced_remote(s, path, Intent::File).await?;
    if !s.sftp().try_exists(rel.clone()).await.unwrap_or(false) {
        return Ok(false);
    }
    s.sftp().remove_file(rel.clone()).await.map_err(|e| {
        io_raw(
            copy_text(
                "beSftp.remove.failed",
                &[("path", &rel), ("why", &why_of(&e))],
            ),
            &e,
        )
    })?;
    Ok(true)
}

/// 建**一层**目录（已在 ⇒ 什么都不做）。
///
/// 部署那一种规矩下，**这一趟建出来的**那一层当场收成只给本人（宿主交的 `dir_mode`；后端交 `own_dir::PRIVATE_DIR_MODE`，0700）——
/// 远端第一个建 `~/.cc-monitor`（以及 `bin` / `staging`）的就是这里（部署），此前按服务端 umask 建（常见 0755）。
/// 本机那一份是后端的 `Own::ensure_dir`（它在本机文件系统上、这里调不到它）；两边共用同一个权限位常量。
/// ⚠ 用 `set_metadata`（SETSTAT）只对**目录**、只带 `permissions` 一格 —— `put_atomic` 头注那条「改名之后绝不 setstat」
/// 管的是刚上位的**文件**（那一次事故把后端截成 0 字节），目录没有长度，不在那条事故的射程里。
/// 收不窄（服务端不认 SETSTAT）⇒ 说一句、不挡部署：目录已经建出来了，权限宽一点不是「建不成」。
pub async fn make_dir(s: &Session, path: &str) -> Result<(), Refusal> {
    let rel = fenced_remote(s, path, Intent::Dir).await?;
    if s.sftp().try_exists(rel.clone()).await.unwrap_or(false) {
        return Ok(());
    }
    if let Err(e) = s.sftp().create_dir(rel.clone()).await {
        // 并发的另一趟刚建好它 —— 那不算错（也不是这一趟建的 ⇒ 不去动它的权限位）。
        if !s.sftp().try_exists(rel.clone()).await.unwrap_or(false) {
            return Err(io_raw(
                copy_text(
                    "beSftp.put.writeFailed",
                    &[("path", &rel), ("why", &why_of(&e))],
                ),
                &e,
            ));
        }
        return Ok(());
    }
    let WriteRules::Deploy { dir_mode } = crate::env::get().files else {
        return Ok(());
    };
    let private = FileAttributes {
        permissions: Some(dir_mode),
        ..Default::default()
    };
    if let Err(e) = s.sftp().set_metadata(rel.clone(), private).await {
        tracing::warn!("远端 ~/{rel} 建好了，但没能收成只给本人（{e}）—— 按服务端默认权限留着");
    }
    Ok(())
}

/// `mkdir -p`（只在围栏里）：先对**整条**路径过一次词法，再逐级 [`make_dir`]（每一级各自再过一次围栏）。
pub async fn make_dirs(s: &Session, path: &str) -> Result<(), Refusal> {
    let rel = match crate::env::get().files {
        WriteRules::Deploy { .. } => fence_lexical(&s.home, path, Intent::Dir).map_err(fenced)?.0,
        WriteRules::ReadOnly => fenced_remote(s, path, Intent::Dir).await?,
    };
    let mut cur = String::new();
    for comp in rel.split('/').filter(|c| !c.is_empty()) {
        if !cur.is_empty() {
            cur.push('/');
        }
        cur.push_str(comp);
        make_dir(s, &cur).await?;
    }
    Ok(())
}

/// 远端文件句柄（读写都是它；类型名只在本文件点一次）。
pub type RemoteFile = russh_sftp::client::fs::File;

/// 打开一份暂存件准备写（`truncate` ⇒ 从 0 写；否则接着写）。**最后一段是链接 ⇒ 拒**（开写会跟过去）。
pub async fn open_for_write(
    s: &Session,
    path: &str,
    truncate: bool,
) -> Result<RemoteFile, Refusal> {
    let rel = fenced_remote(s, path, Intent::File).await?;
    if let Ok(m) = s.sftp().symlink_metadata(rel.clone()).await {
        if m.is_symlink() {
            return Err(fenced(copy_text("beSftp.open.isLink", &[("path", &rel)])));
        }
    }
    let mut flags = OpenFlags::CREATE | OpenFlags::WRITE | OpenFlags::READ;
    if truncate {
        flags |= OpenFlags::TRUNCATE;
    }
    s.sftp()
        .open_with_flags(rel.clone(), flags)
        .await
        .map_err(|e| {
            io_raw(
                copy_text(
                    "beSftp.put.writeFailed",
                    &[("path", &rel), ("why", &why_of(&e))],
                ),
                &e,
            )
        })
}

// ═══ 读（不改任何东西；不过围栏）══════════════════════════════════════════════════════

/// 打开一份远端文件只读。
pub async fn open_for_read(s: &Session, path: &str) -> Result<RemoteFile, Said> {
    s.sftp()
        .open_with_flags(path.to_string(), OpenFlags::READ)
        .await
        .map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beSftp.open.readFailed",
                    &[("path", path), ("why", &why_of(&e))],
                ),
                &e,
            )
        })
}

/// `metadata` 的大小：`None` = 那次调用失败；`Some(None)` = 服务器没给 size（**不是 0 字节**）。
pub async fn metadata_size(s: &Session, path: &str) -> Option<Option<u64>> {
    s.sftp()
        .metadata(path.to_string())
        .await
        .ok()
        .map(|m| m.size)
}

/// 在不在：`None` = 问不出来。
pub async fn exists(s: &Session, path: &str) -> Option<bool> {
    s.sftp().try_exists(path.to_string()).await.ok()
}

/// 整份读回来；读不出 ⇒ `None`。
pub async fn read_all(s: &Session, path: &str) -> Option<Vec<u8>> {
    s.sftp().read(path.to_string()).await.ok()
}

/// 台架：合成 SFTP 服务端（逐条记改动路径）。本文件的判据与后端 `control/transfer.rs` 的判据（经 `test-support`）共用。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../tests/backend/sftp_rig.rs"]
pub mod rig;

#[cfg(test)]
#[path = "../../../tests/backend/dial_sftp_tests.rs"]
mod tests;
