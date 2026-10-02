//! **SFTP 住本机常驻后端**：在池里那条 SSH 连接上开 sftp 子系统 ·
//! **远端写围栏（只许两处）** · 远端写原语 · 部署那条链路（`use:"files"`）的一问一答。
//!
//! # 要求
//!
//! 「SFTP 怎么进单一常驻后端」一题选「**进本机常驻后端，只写暂存区**」：SFTP 连接由本机常驻后端管、
//! 与其它 SSH 复用；只往远端暂存区写，落进用户目录仍只经远端后端文件管理提交（`files-commit-upload`）；
//! 界面进程零 SSH。F08（自部署后端）**按构造搬不进远端后端**（那时它还不在），搬进**本机**后端没这个问题
//! ⇒ 第二个写根是部署目录 `~/.cc-monitor/bin/`。
//!
//! # 🔴 本文件是后端里**唯一**一份能改远端文件系统的代码（红线 `I7`，`INVARIANTS §41.6`）
//!
//! `readonly_guard::remote_write_layer` 钉三件：① 生产段里命中远端写能力网 / 动词网的文件 == `{dial/sftp.rs}`；
//! ② 本文件声明的写根（[`REMOTE_WRITE_ROOTS`]）== `{~/.cc-monitor/staging, ~/.cc-monitor/bin}`；
//! ③ 本文件里每个含远端改动动词的函数，第一个改动之前先有一次 [`fenced_remote`] 调用。
//! ⇒ **写原语只许住这里、只许从这里的函数进**；别的文件（`control/transfer.rs`、`uses.rs`）只调它们。
//!
//! # 围栏（[`fence_lexical`] 纯函数 ＋ [`fenced_remote`] 解链接）
//!
//! - 路径归一成 home 相对（home = SFTP `realpath(".")`，即起始目录）：绝对路径必须在 `<home>/` 底下；
//! - 词法：不许 `..` / `.` / 空段；前两段必须恰好是某一个根；写文件必须**在根底下**（不是根自己）；
//!   建目录另放行两个根自己与它们唯一的祖先 `.cc-monitor`（首次部署要建）；
//! - 解链接：目标的父目录 `realpath` 之后必须仍在那个根的 `realpath` 之下（挡「`bin/x` 是指向 `~/.ssh` 的链接」）；
//!   打开写的那一格另问一次 `lstat`，最后一段是链接就拒（开写会跟过去）。
//!
//! # 买不到什么（逐条）
//!
//! - **TOCTOU**：判定与动手之间有窗（同第三层 `files_write` 自陈的那一格）。
//! - **Windows 远端**：home 是 `C:\…` 形时归一化不认它 ⇒ 一律拒（出声，不猜）。一格读数都没有。
//! - **`ChrootDirectory` / `internal-sftp -d`**：起始目录不是 `$HOME` 的机器上，这里的「home」是 SFTP 的起始目录，
//!   与远端后端的 `$HOME` 不是同一个 ⇒ 暂存件提交会答「不在」（`files_commit` 头注那一格照旧）。

use copy_core::copy_text;
use std::sync::Arc;

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, OpenFlags};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
};

use super::connect::Linked;
use super::uses::{Lane, Lease};
use super::{pool, write_line, DialRequest, StageSink};

/// 🔴 **远端写根：恰好这两处**（home 相对）。`readonly_guard::remote_write_layer` 现读本行源码逐字比。
///
/// - `staging`：上传只写暂存区，落进用户目录由远端后端 `files-commit-upload` 做；
/// - `bin`：自部署（F08 后端二进制 ＋ `.build_id` · `ccm` 入口），后端还不在时只能靠它放上去。
pub(crate) const REMOTE_WRITE_ROOTS: [&str; 2] = [".cc-monitor/staging", ".cc-monitor/bin"];

/// 暂存区那一根（与 `control/files_commit.rs::STAGING_DIR` 相等 —— 同一个 crate，判据直接比）。
pub(crate) const STAGING_ROOT: &str = REMOTE_WRITE_ROOTS[0];

/// 两个根唯一的祖先：后端的家。只在「建目录」那一形里放行（首次部署要建它）。
const ROOTS_HOME: &str = ".cc-monitor";

/// files 链路上一行请求的上限（请求都是短 JSON；`put` 的字节不走行）。
const REQUEST_LINE_CAP: u64 = 64 * 1024;

/// `put` 一次最多收多少字节（后端二进制今天 MB 级；上限给宽，但不许无界进内存）。
pub(crate) const MAX_PUT_BYTES: u64 = 64 << 20;

/// 一条开在池里那条连接上的 SFTP 会话（占这条连接的一格通道，传输另占一格车道）。
pub(crate) struct Session {
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
/// 读数（`NT1.md §0.2` ⑤c）：1 KB 小文件每件 ≈ 9.7 个往返，其中开 sftp 通道（开 channel · 请求子系统 · `SSH_FXP_INIT` ·
/// `realpath .`）占 4 个。停一个 ⇒ 下一趟只花 1 个往返验活（`realpath .` 对一遍 home）。
/// - **它不托连接**：空位住在 `Linked` 里、不持 `Arc<Linked>` ⇒ 连接照旧随最后一个用户 / 托它的主连接一起走，空位随之没了。
/// - **它连同那一格传输许可一起停**（车道 ＋ 通道）：远端 `MaxSessions` 数的正是这条还开着的通道，账要对得上。
/// - **挤掉**：别的放置借不到格时，池先把空位里的关掉、还出那一格（`pool::Conn::reclaim_idle`）—— 事件驱动的回收，不是定时器。
pub(crate) struct Parked {
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
pub(crate) async fn open(
    lease: &mut Lease,
    req: &DialRequest,
    stages: &StageSink,
) -> Result<Session, String> {
    let (channel, permit) = lease.session_channel(req, stages).await?;
    subsystem(&channel)
        .await
        .map_err(|e| copy_text("beSftp.open.subsystem", &[("e", &e.to_string())]))?;
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
pub(crate) struct Dial(DialRequest);

impl Dial {
    pub(crate) fn parse(v: &serde_json::Value) -> Result<Dial, String> {
        super::parse_request_value(v).map(Dial).map_err(|(_, m)| m)
    }
}

/// 传输那一趟的会话：先找一条停着的空闲会话（1 个往返）；没有 ⇒ 在池里放置（同身份复用 / 分道到批量连接）
/// → 过**传输车道**开一条 sftp 通道（4 个往返）。
pub(crate) async fn open_for_transfer(d: &Dial) -> Result<Session, String> {
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
async fn init<S>(stream: S) -> Result<(SftpSession, String), String>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let sftp = SftpSession::new(stream)
        .await
        .map_err(|e| copy_text("beSftp.open.initFailed", &[("e", &e.to_string())]))?;
    let home = sftp
        .canonicalize(".")
        .await
        .map_err(|e| copy_text("beSftp.open.noHome", &[("e", &e.to_string())]))?;
    Ok((sftp, home))
}

impl Session {
    /// 在一条**已经是 sftp 子系统**的字节流上起会话：`SSH_FXP_INIT` ＋ 问一次起始目录的真路径。
    /// `keep` 是要与会话同生死的东西（预算那一格 ＋ 底层连接）。
    pub(crate) async fn over<S>(
        stream: S,
        keep: Box<dyn std::any::Any + Send + Sync>,
    ) -> Result<Session, String>
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
    pub(crate) fn home(&self) -> &str {
        &self.home
    }
}

// ═══ 围栏 ═══════════════════════════════════════════════════════════════════════════

/// 这一次改动是什么形。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Intent {
    /// 建 / 写 / 删 / 改名一份文件：必须**在某个根底下**（不是根自己）。
    File,
    /// 建一个目录：根底下、根自己、或两个根唯一的祖先 `.cc-monitor`。
    Dir,
}

/// 一次改动被拒的两档：围栏拦的（换条路径才有意义）· 盘上没成（重试才有意义）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    Fenced(String),
    Io(String),
}

impl Refusal {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Refusal::Fenced(_) => "fenced",
            Refusal::Io(_) => "io",
        }
    }
    pub(crate) fn message(&self) -> &str {
        match self {
            Refusal::Fenced(m) | Refusal::Io(m) => m,
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

fn fenced(msg: String) -> Refusal {
    Refusal::Fenced(msg)
}

fn io(msg: String) -> Refusal {
    Refusal::Io(msg)
}

/// **纯词法那一道**：把 `path` 归一成 home 相对，判它落不落在两个根里。
///
/// 回 `(home 相对路径, 它在哪个根底下)`；第二项 `None` = 它是 `.cc-monitor` 本身（只有建目录那一形放行）。
pub(crate) fn fence_lexical(
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
    for root in REMOTE_WRITE_ROOTS {
        let under = rel
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'));
        if under || (intent == Intent::Dir && rel == root) {
            return Ok((rel, Some(root)));
        }
    }
    Err(copy_text(
        "beSftp.fence.outsideRoots",
        &[
            ("a", REMOTE_WRITE_ROOTS[0]),
            ("b", REMOTE_WRITE_ROOTS[1]),
            ("path", p),
        ],
    ))
}

/// 父目录（home 相对，`/` 分隔）。
fn parent_of(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(p, _)| p).unwrap_or(".")
}

/// **围栏**：词法那一道 ＋ 父目录解链接之后仍在那个根之下。回 home 相对路径（SFTP 起始目录就是 home）。
pub(crate) async fn fenced_remote(
    s: &Session,
    path: &str,
    intent: Intent,
) -> Result<String, Refusal> {
    let (rel, root) = fence_lexical(&s.home, path, intent).map_err(fenced)?;
    let Some(root) = root else {
        return Ok(rel);
    };
    if rel == root {
        return Ok(rel);
    }
    let real_root = s.sftp().canonicalize(root).await.map_err(|e| {
        fenced(copy_text(
            "beSftp.fence.rootUnresolved",
            &[("root", root), ("e", &e.to_string())],
        ))
    })?;
    let parent = parent_of(&rel);
    let real_parent = s.sftp().canonicalize(parent).await.map_err(|e| {
        fenced(copy_text(
            "beSftp.fence.parentUnresolved",
            &[("parent", parent), ("e", &e.to_string())],
        ))
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
/// 标准 SFTP 的改名不覆盖（`russh-sftp` 没有 `posix-rename@openssh.com`）⇒ 只能这样近似原子。两个部署者交错时临时件 / 备份件各是各的（唯一名）；仍剩的一格如实记：`rel → .bak` 与 `tmp → rel` 之间 `rel` 有一瞬不在。
///
/// ⚠ **改名之后绝不 `set_metadata` 兜底 chmod** —— 真机 e2e 实证：OpenSSH sftp-server 上那一次 setstat 把刚上位的
/// 后端**截成 0 字节** ⇒ 不可 exec → 连接 EOF → 标记变空 → 无限重部署。权限只在 open-create 的属性里设一次。
/// ⚠ 〔2026-09-20 现打订正，随函数搬来〕「即便只设 permissions、`size=None` 也会截断」那句括号是推断、今天复现不出来
/// （真 `OpenSSH_10.2p1` 上 permissions-only 的 SETSTAT 前后都是 16 字节）；最可能当年那个属性块真的带了 size。
/// 🔴 **但这条禁令不放宽**：事故是真的、只量了一个服务端版本、部署路上多一次 setstat 收益为零风险是变砖。
/// ⚠ 与改权限命令的关系：那条命令（今天是后端 `files-chmod`）**要**改权限，是它的本职；它当年靠「属性块逐字节不带 size」
/// 那条判据挡住这一形（`the_chmod_attrs_never_put_a_size_on_the_wire`〔散文墓碑〕，随那条老命令一起删了）。
pub(crate) async fn put_atomic(
    s: &Session,
    path: &str,
    bytes: &[u8],
    mode: u32,
) -> Result<(), Refusal> {
    let rel = fenced_remote(s, path, Intent::File).await?;
    // 〔临时件名唯一〕临时件与备份件都带**这一趟独有**的后缀：两个部署者（两台 monitor 各自的常驻后端）
    //   同时往同一个落点放字节时，谁也不删谁的那一份（从前是固定的 `<rel>.tmp` / `<rel>.bak` ＋「先删残留」—— 那一删删的可能是
    //   别人正在写的那一份，审计 E3 子形 2）。仍 `EXCLUDE` 新建：真撞了名 ⇒ 当场失败，不会写进别人的那一份。
    //   〔墓碑 —— 「先删残留」那一步没了：固定名时它清上一趟崩掉留下的那一份；唯一名之后崩掉的那一趟留下的临时件
    //    没人认领（`put_atomic` 失败那几支会删自己的；进程被杀那一形留在 `~/.cc-monitor/bin/` 里，如实登记）。〕
    let tmp = format!("{rel}.{}.tmp", trip_tag());
    // 失败那几支删**自己这一趟**的临时件；连接已经断了（`sender dropped`）⇒ 删不掉：说一句，
    //   留下的那一份由下次连上的部署计划认领（`control/deploy_plan.rs::stale_leftovers`）。
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
            io(copy_text(
                "beSftp.put.createFailed",
                &[("path", &tmp), ("e", &e.to_string())],
            ))
        })?;
    let written = async {
        file.write_all(bytes).await.map_err(|e| {
            io(copy_text(
                "beSftp.put.writeFailed",
                &[("path", &tmp), ("e", &e.to_string())],
            ))
        })?;
        // `write_all` 只把 WRITE 包入队；ack 只在 flush / shutdown 里收（同 monitor 那一份的理由）。
        file.flush().await.map_err(|e| {
            io(copy_text(
                "beSftp.put.flushFailed",
                &[("path", &tmp), ("e", &e.to_string())],
            ))
        })?;
        file.shutdown().await.map_err(|e| {
            io(copy_text(
                "beSftp.put.closeFailed",
                &[("path", &tmp), ("e", &e.to_string())],
            ))
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
            return Err(io(copy_text(
                "beSftp.put.backupFailed",
                &[("path", &rel), ("backup", &b), ("e", &e.to_string())],
            )));
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
        return Err(io(copy_text(
            "beSftp.put.renameFailed",
            &[("tmp", &tmp), ("path", &rel), ("e", &e.to_string())],
        )));
    }
    if let Some(b) = bak {
        let _ = s.sftp().remove_file(b).await;
    }
    Ok(())
}

/// 一趟上传独有的后缀：pid ⊕ 纳秒 ⊕ 进程内计数（十六进制）。**不是**密码学随机 —— 它只要「两个部署者不撞」，
/// 撞了由 `EXCLUDE` 当场挡住（失败，不是写进别人的那一份）。
fn trip_tag() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
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

/// 这个名字是不是 [`put_atomic`] 留下的临时件 / 备份件：`<名>.<trip_tag>.tmp` 或 `.bak`
/// （`trip_tag` = 三段非空小写十六进制、`-` 相连）。形状只认这一种 —— 别的名字不是我们放的，一个不碰。
pub(crate) fn is_trip_leftover(name: &str) -> bool {
    let Some(stem) = name
        .strip_suffix(".tmp")
        .or_else(|| name.strip_suffix(".bak"))
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
pub(crate) async fn list_dir(s: &Session, path: &str) -> Option<Vec<(String, Option<u64>)>> {
    let rd = s.sftp().read_dir(path.to_string()).await.ok()?;
    Some(
        rd.map(|e| (e.file_name(), e.metadata().mtime.map(u64::from)))
            .collect(),
    )
}

/// 删一份文件。回「真删了」（不在 ⇒ `false`，不算错）。
pub(crate) async fn remove(s: &Session, path: &str) -> Result<bool, Refusal> {
    let rel = fenced_remote(s, path, Intent::File).await?;
    if !s.sftp().try_exists(rel.clone()).await.unwrap_or(false) {
        return Ok(false);
    }
    s.sftp().remove_file(rel.clone()).await.map_err(|e| {
        io(copy_text(
            "beSftp.remove.failed",
            &[("path", &rel), ("e", &e.to_string())],
        ))
    })?;
    Ok(true)
}

/// 建**一层**目录（已在 ⇒ 什么都不做）。
///
/// **这一趟建出来的**那一层当场收成只给本人（`own_dir::PRIVATE_DIR_MODE`，0700）——
/// 远端第一个建 `~/.cc-monitor`（以及 `bin` / `staging`）的就是这里（部署），此前按服务端 umask 建（常见 0755）。
/// 本机那一份是 `own_dir::ensure_private_dir`（它在本机文件系统上、这里调不到它）；两边共用同一个权限位常量。
/// ⚠ 用 `set_metadata`（SETSTAT）只对**目录**、只带 `permissions` 一格 —— `put_atomic` 头注那条「改名之后绝不 setstat」
/// 管的是刚上位的**文件**（那一次事故把后端截成 0 字节），目录没有长度，不在那条事故的射程里。
/// 收不窄（服务端不认 SETSTAT）⇒ 说一句、不挡部署：目录已经建出来了，权限宽一点不是「建不成」。
pub(crate) async fn make_dir(s: &Session, path: &str) -> Result<(), Refusal> {
    let rel = fenced_remote(s, path, Intent::Dir).await?;
    if s.sftp().try_exists(rel.clone()).await.unwrap_or(false) {
        return Ok(());
    }
    if let Err(e) = s.sftp().create_dir(rel.clone()).await {
        // 并发的另一趟刚建好它 —— 那不算错（也不是这一趟建的 ⇒ 不去动它的权限位）。
        if !s.sftp().try_exists(rel.clone()).await.unwrap_or(false) {
            return Err(io(copy_text(
                "beSftp.mkdir.failed",
                &[("path", &rel), ("e", &e.to_string())],
            )));
        }
        return Ok(());
    }
    let private = FileAttributes {
        permissions: Some(crate::common::own_dir::PRIVATE_DIR_MODE),
        ..Default::default()
    };
    if let Err(e) = s.sftp().set_metadata(rel.clone(), private).await {
        tracing::warn!("远端 ~/{rel} 建好了，但没能收成只给本人（{e}）—— 按服务端默认权限留着");
    }
    Ok(())
}

/// `mkdir -p`（只在围栏里）：先对**整条**路径过一次词法，再逐级 [`make_dir`]（每一级各自再过一次围栏）。
pub(crate) async fn make_dirs(s: &Session, path: &str) -> Result<(), Refusal> {
    let (rel, _) = fence_lexical(&s.home, path, Intent::Dir).map_err(fenced)?;
    let mut cur = String::new();
    for comp in rel.split('/') {
        if !cur.is_empty() {
            cur.push('/');
        }
        cur.push_str(comp);
        make_dir(s, &cur).await?;
    }
    Ok(())
}

/// 远端文件句柄（读写都是它；类型名只在本文件点一次）。
pub(crate) type RemoteFile = russh_sftp::client::fs::File;

/// 打开一份暂存件准备写（`truncate` ⇒ 从 0 写；否则接着写）。**最后一段是链接 ⇒ 拒**（开写会跟过去）。
pub(crate) async fn open_for_write(
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
            io(copy_text(
                "beSftp.open.writeFailed",
                &[("path", &rel), ("e", &e.to_string())],
            ))
        })
}

// ═══ 读（不改任何东西；不过围栏）══════════════════════════════════════════════════════

/// 打开一份远端文件只读。
pub(crate) async fn open_for_read(s: &Session, path: &str) -> Result<RemoteFile, String> {
    s.sftp()
        .open_with_flags(path.to_string(), OpenFlags::READ)
        .await
        .map_err(|e| {
            copy_text(
                "beSftp.open.readFailed",
                &[("path", path), ("e", &e.to_string())],
            )
        })
}

/// `metadata` 的大小：`None` = 那次调用失败；`Some(None)` = 服务器没给 size（**不是 0 字节**）。
pub(crate) async fn metadata_size(s: &Session, path: &str) -> Option<Option<u64>> {
    s.sftp()
        .metadata(path.to_string())
        .await
        .ok()
        .map(|m| m.size)
}

/// 在不在：`None` = 问不出来。
pub(crate) async fn exists(s: &Session, path: &str) -> Option<bool> {
    s.sftp().try_exists(path.to_string()).await.ok()
}

/// 整份读回来；读不出 ⇒ `None`。
pub(crate) async fn read_all(s: &Session, path: &str) -> Option<Vec<u8>> {
    s.sftp().read(path.to_string()).await.ok()
}

// ═══ files 链路（`use:"files"`）：受限远端文件的一问一答 ════════════════════════════════
//
// ack 之后，monitor 每写一行请求、这里回一行应答；`put` 那一行之后紧跟 `size` 个原始字节。
// monitor 那一侧是 `dial_host::RemoteFs`（部署的业务判定都在那边，这里只执行）。

/// 有上限地读一行（不含换行）。EOF 且没读到东西 ⇒ `None`。⚠ 必须写成 `.take(` 这个点调用。
async fn read_line_capped<R: AsyncBufRead + Unpin>(
    r: &mut R,
    cap: u64,
) -> Result<Option<String>, String> {
    let mut line = String::new();
    let n = (&mut *r)
        .take(cap + 1)
        .read_line(&mut line)
        .await
        .map_err(|e| copy_text("beSftp.request.readFailed", &[("e", &e.to_string())]))?;
    if n == 0 {
        return Ok(None);
    }
    if n as u64 > cap && !line.ends_with('\n') {
        return Err(crate::common::contract::malformed(&format!(
            "request line longer than {cap} bytes"
        )));
    }
    Ok(Some(line.trim_end_matches(['\n', '\r']).to_string()))
}

fn arg_str<'a>(v: &'a serde_json::Value, k: &str) -> Result<&'a str, String> {
    v.get(k)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| crate::common::contract::malformed(&format!("missing `{k}` (string)")))
}

fn arg_u64(v: &serde_json::Value, k: &str) -> Result<u64, String> {
    v.get(k).and_then(serde_json::Value::as_u64).ok_or_else(|| {
        crate::common::contract::malformed(&format!("missing `{k}` (non-negative integer)"))
    })
}

/// 失败那一形：与后端 CLI 错误信封同一对键（`code` ＋ `message`，`readonly_guard::error_envelope_registry` 签字）。
fn refused(code: &str, message: &str) -> serde_json::Value {
    serde_json::json!({ "code": code, "message": message })
}

fn refusal(r: &Refusal) -> serde_json::Value {
    refused(r.code(), r.message())
}

/// 读回来的那一份与期望的比对结论：`None` = 读不回来；`Some((读回长度, 首个差异))`，`差异 == None` ⇒ 逐字节相同。
fn readback_facts(expected: &[u8], actual: Option<&[u8]>) -> serde_json::Value {
    match actual {
        None => serde_json::Value::Null,
        Some(a) => {
            let first_diff = if a == expected {
                None
            } else {
                Some(
                    expected
                        .iter()
                        .zip(a)
                        .position(|(x, y)| x != y)
                        .unwrap_or(expected.len().min(a.len())),
                )
            };
            serde_json::json!({ "len": a.len(), "first_diff": first_diff })
        }
    }
}

/// 一条请求。`put` 从 `input` 里再收 `size` 个字节。
async fn answer<R: AsyncRead + Unpin>(
    s: &Session,
    req: &serde_json::Value,
    input: &mut R,
) -> serde_json::Value {
    let op = req
        .get("op")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let res: Result<serde_json::Value, serde_json::Value> = async {
        let bad = |m: String| refused("bad_request", &m);
        match op {
            "home" => Ok(serde_json::json!({ "home": s.home() })),
            // `stat` 那一问删了：它唯一的问者（monitor 部署时问落点在不在）随部署判定进了本机后端
            //   （`control/deploy_plan.rs` 直接用本文件的 `metadata_size` / `exists`），这条链路上零调用方。
            "read" => {
                let path = arg_str(req, "path").map_err(bad)?;
                let max = arg_u64(req, "max").map_err(bad)?;
                let data = read_all(s, path).await;
                if let Some(d) = &data {
                    if d.len() as u64 > max {
                        return Err(refused(
                            "too_big",
                            &copy_text(
                                "beSftp.read.tooBig",
                                &[
                                    ("path", path),
                                    ("size", &d.len().to_string()),
                                    ("max", &max.to_string()),
                                ],
                            ),
                        ));
                    }
                }
                // 只在需要时补问：读不出 ⇒ 在不在；读到空 ⇒ 大小。
                let (exists, size) = match &data {
                    None => (exists(s, path).await, None),
                    Some(d) if d.is_empty() => (None, metadata_size(s, path).await.flatten()),
                    Some(_) => (None, None),
                };
                Ok(serde_json::json!({
                    "data": data.as_deref().map(crate::stream::wire::b64_encode),
                    "exists": exists,
                    "size": size,
                }))
            }
            "put" => {
                let path = arg_str(req, "path").map_err(bad)?;
                let size = arg_u64(req, "size").map_err(bad)?;
                let mode = arg_u64(req, "mode").map_err(bad)? as u32;
                let verify = req
                    .get("verify")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if size > MAX_PUT_BYTES {
                    // 字节已经在路上 —— 收掉丢弃，别让下一行请求读到半截二进制。
                    let _ = tokio::io::copy(&mut (&mut *input).take(size), &mut tokio::io::sink())
                        .await;
                    return Err(refused(
                        "too_big",
                        &crate::common::contract::malformed(&format!(
                            "put of {size} bytes exceeds {MAX_PUT_BYTES}"
                        )),
                    ));
                }
                let mut bytes = Vec::with_capacity(size as usize);
                (&mut *input)
                    .take(size)
                    .read_to_end(&mut bytes)
                    .await
                    .map_err(|e| {
                        refused(
                            "io",
                            &copy_text("beSftp.put.receiveFailed", &[("e", &e.to_string())]),
                        )
                    })?;
                if bytes.len() as u64 != size {
                    return Err(refused(
                        "bad_request",
                        &copy_text(
                            "beSftp.put.short",
                            &[
                                ("size", &size.to_string()),
                                ("got", &bytes.len().to_string()),
                            ],
                        ),
                    ));
                }
                put_atomic(s, path, &bytes, mode)
                    .await
                    .map_err(|r| refusal(&r))?;
                let readback = if verify {
                    let back = read_all(s, path).await;
                    readback_facts(&bytes, back.as_deref())
                } else {
                    serde_json::Value::Null
                };
                Ok(serde_json::json!({ "readback": readback }))
            }
            "remove" => {
                let path = arg_str(req, "path").map_err(bad)?;
                let removed = remove(s, path).await.map_err(|r| refusal(&r))?;
                Ok(serde_json::json!({ "removed": removed }))
            }
            "mkdirs" => {
                let path = arg_str(req, "path").map_err(bad)?;
                make_dirs(s, path).await.map_err(|r| refusal(&r))?;
                Ok(serde_json::json!({}))
            }
            other => Err(refused(
                "unknown_op",
                &crate::common::contract::malformed(&format!(
                    "unknown op `{other}` (known: home / read / put / remove / mkdirs)"
                )),
            )),
        }
    }
    .await;
    res.unwrap_or_else(|e| e)
}

/// files 链路的服务循环：读一行请求、回一行应答，直到 monitor 那头关了（上行 EOF）。
pub(crate) async fn serve_files<R, W>(s: &Session, input: R, out: &mut W)
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    let mut rd = tokio::io::BufReader::new(input);
    loop {
        let line = match read_line_capped(&mut rd, REQUEST_LINE_CAP).await {
            Ok(Some(l)) => l,
            Ok(None) => return,
            Err(e) => {
                let _ = write_line(out, &refused("bad_request", &e)).await;
                return;
            }
        };
        let reply = match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(req) => answer(s, &req, &mut rd).await,
            Err(e) => refused("bad_request", &format!("请求行不是 JSON：{e}")),
        };
        if write_line(out, &reply).await.is_err() {
            return;
        }
    }
}

/// 台架：合成 SFTP 服务端（逐条记改动路径）。本文件的判据与 `control/transfer.rs` 的判据共用。
#[cfg(test)]
#[path = "../../../tests/backend/sftp_rig.rs"]
pub(crate) mod rig;

#[cfg(test)]
#[path = "../../../tests/backend/dial_sftp_tests.rs"]
mod tests;
