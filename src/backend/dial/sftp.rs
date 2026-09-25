//! 〔SR1b · 2026-09-24〕**SFTP 住本机常驻后端**：在池里那条 SSH 连接上开 sftp 子系统 ·
//! **远端写围栏（只许两处）** · 远端写原语 · 部署那条链路（`use:"files"`）的一问一答。
//!
//! # 用户裁决（`99 §1` V89，逐字）
//!
//! 「SFTP 怎么进单一常驻后端」一题选「**进本机常驻后端，只写暂存区**」：SFTP 连接由本机常驻后端管、
//! 与其它 SSH 复用；只往远端暂存区写，落进用户目录仍只经远端后端文件管理提交（`files-commit-upload`）；
//! 界面进程零 SSH。F08（自部署后端）**按构造搬不进远端后端**（那时它还不在），搬进**本机**后端没这个问题
//! ⇒ 第二个写根是部署目录 `~/.cc-monitor/bin/`。
//!
//! # 🔴 本文件是后端里**唯一**一份能改远端文件系统的代码（红线 `I7`，`INVARIANTS §41.6` 的 V89 订正）
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
/// - `staging`：上传只写暂存区（`设计/60 §4.3`），落进用户目录由远端后端 `files-commit-upload` 做；
/// - `bin`：自部署（F08 后端二进制 ＋ `.build_id` · `ccm` 入口 · cc-acct-iso），后端还不在时只能靠它放上去。
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
    sftp: SftpSession,
    /// SFTP 起始目录的真路径（`realpath(".")`）—— 围栏把绝对路径归一成相对它。
    home: String,
    /// 保活：借到的那一格预算 ＋ 底层那条 SSH 连接（生产上是 `(pool::Permit, Arc<Linked>)`）。
    /// **不许省**：连接句柄一 drop 整条连接就断，这条通道跑在它上面；预算那一格要攥到会话用完。
    _keep: Box<dyn std::any::Any + Send + Sync>,
}

/// 在 channel 上请求 sftp 子系统，装进一个**显式 `Send`** 的盒子（理由同 `uses.rs::exec`：
/// 借 `&self` 的 `async fn` 放进要 `tokio::spawn` 的任务里会撞「`Send` is not general enough」）。
fn subsystem(
    channel: &russh::Channel<russh::client::Msg>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), russh::Error>> + Send + '_>> {
    Box::pin(channel.request_subsystem(true, "sftp"))
}

/// 开一条 SFTP 会话：过这条连接的预算（`lane`）→ 开 session channel → 请求 sftp 子系统 → `SSH_FXP_INIT` →
/// 问一次起始目录的真路径。**三到四个往返** —— 不复用通道（旧 monitor 池的空闲栈随它一起删了），代价如实记。
pub(crate) async fn open(
    lease: &mut Lease,
    req: &DialRequest,
    stages: &StageSink,
    lane: Lane,
) -> Result<Session, String> {
    let (channel, permit) = lease.session_channel(req, stages, lane).await?;
    subsystem(&channel)
        .await
        .map_err(|e| format!("请求 sftp 子系统失败（远端 sshd 没开 sftp？）: {e}"))?;
    let keep: (pool::Permit, Arc<Linked>) = (permit, Arc::clone(lease.linked()));
    Session::over(channel.into_stream(), Box::new(keep)).await
}

/// 一份拨号请求（传输台开单时读进来、起跑时拿它开会话）。包一层是为了让传输台**只经本文件**够到拨号：
/// 它手里不必有 `DialRequest` 这个类型（文件管理那一面的外向边钉在 `module_boundary_guard::OUTWARD`）。
#[derive(Clone)]
pub(crate) struct Dial(DialRequest);

impl Dial {
    pub(crate) fn parse(v: &serde_json::Value) -> Result<Dial, String> {
        super::parse_request_value(v)
            .map(Dial)
            .map_err(|e| format!("拨号请求读不动：{e}"))
    }
}

/// 传输那一趟的会话：拿池里那条连接（同身份复用）→ 过**传输车道**开一条 sftp 通道。
pub(crate) async fn open_for_transfer(d: &Dial) -> Result<Session, String> {
    let req = &d.0;
    let stages = StageSink::new(false);
    let mut lease = Lease::take(req, &stages).await.map_err(|(e, _)| e)?;
    open(&mut lease, req, &stages, Lane::Transfer).await
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
        let sftp = SftpSession::new(stream)
            .await
            .map_err(|e| format!("初始化 sftp 会话失败: {e}"))?;
        let home = sftp
            .canonicalize(".")
            .await
            .map_err(|e| format!("问不出 SFTP 起始目录（realpath .）: {e}"))?;
        Ok(Session {
            sftp,
            home,
            _keep: keep,
        })
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
        return Err("refuse write: 远端路径是空的".to_string());
    }
    let rel = if let Some(abs) = p.strip_prefix('/') {
        let h = home.trim_end_matches('/');
        if !home.starts_with('/') || h.is_empty() {
            return Err(format!(
                "refuse write: SFTP 起始目录不是一个 POSIX 绝对路径（{home}）—— 归一化不认它，不猜"
            ));
        }
        let Some(rest) = abs.strip_prefix(h.trim_start_matches('/')) else {
            return Err(format!("refuse write: {p} 不在远端 home（{home}）底下"));
        };
        let Some(rest) = rest.strip_prefix('/') else {
            return Err(format!("refuse write: {p} 不在远端 home（{home}）底下"));
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
        return Err(format!(
            "refuse write: {p} 里有空段 / `.` / `..` —— 只收规整的路径"
        ));
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
    Err(format!(
        "refuse write: 远端只许往 ~/{} 与 ~/{} 底下写（用户 V89「只写暂存区」＋ 自部署），{p} 不在其中",
        REMOTE_WRITE_ROOTS[0], REMOTE_WRITE_ROOTS[1]
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
    let real_root = s.sftp.canonicalize(root).await.map_err(|e| {
        fenced(format!(
            "refuse write: 写根 ~/{root} 解析不了（它还不在？）：{e}"
        ))
    })?;
    let parent = parent_of(&rel);
    let real_parent = s.sftp.canonicalize(parent).await.map_err(|e| {
        fenced(format!(
            "refuse write: ~/{parent} 解析不了（父目录不在？）：{e}"
        ))
    })?;
    let inside = real_parent == real_root
        || real_parent
            .strip_prefix(real_root.as_str())
            .is_some_and(|rest| rest.starts_with('/'));
    if !inside {
        return Err(fenced(format!(
            "refuse write: ~/{parent} 解到底之后跑出了写根（{real_parent} 不在 {real_root} 里）"
        )));
    }
    Ok(rel)
}

// ═══ 远端写原语（每个都先过围栏；只住这一份文件）═══════════════════════════════════════

/// 原子上传 `bytes` 到 `path`，权限 `mode`（只在 open-create 的属性里设一次）。
///
/// 序列与 monitor 那一份 F08 的 `upload_atomic` 逐步相同（它的头注是这一序列的来历 ——
/// 「先备份不删旧」「**绝不** rename 之后 setstat 兜底 chmod」那两条事故教训都在那里）：
/// 写 `<path>.tmp`（先删残留，再 **EXCLUDE** 创建：临时件是一条预置的链接也不会跟过去）
/// → 旧目标**改名成 `.bak`**（不是删）→ 临时件上位 → 删 `.bak`。临时件与 `.bak` 都在目标**同一个父目录**里，
/// 那个父目录已经过了围栏。
pub(crate) async fn put_atomic(
    s: &Session,
    path: &str,
    bytes: &[u8],
    mode: u32,
) -> Result<(), Refusal> {
    let rel = fenced_remote(s, path, Intent::File).await?;
    let tmp = format!("{rel}.tmp");
    let attrs = FileAttributes {
        permissions: Some(mode),
        ..Default::default()
    };
    let _ = s.sftp.remove_file(tmp.clone()).await;
    let mut file = s
        .sftp
        .open_with_flags_and_attributes(
            tmp.clone(),
            OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
            attrs,
        )
        .await
        .map_err(|e| io(format!("创建 ~/{tmp} 失败: {e}")))?;
    file.write_all(bytes)
        .await
        .map_err(|e| io(format!("写 ~/{tmp} 失败: {e}")))?;
    // `write_all` 只把 WRITE 包入队；ack 只在 flush / shutdown 里收（同 monitor 那一份的理由）。
    file.flush()
        .await
        .map_err(|e| io(format!("flush ~/{tmp} 失败（写未确认）: {e}")))?;
    file.shutdown()
        .await
        .map_err(|e| io(format!("关闭 ~/{tmp} 失败: {e}")))?;
    drop(file);
    let bak = if s.sftp.try_exists(rel.clone()).await.unwrap_or(false) {
        let b = format!("{rel}.bak");
        let _ = s.sftp.remove_file(b.clone()).await;
        s.sftp
            .rename(rel.clone(), b.clone())
            .await
            .map_err(|e| io(format!("备份旧文件 ~/{rel} → ~/{b} 失败: {e}")))?;
        Some(b)
    } else {
        None
    };
    s.sftp
        .rename(tmp.clone(), rel.clone())
        .await
        .map_err(|e| io(format!("rename ~/{tmp} → ~/{rel} 失败: {e}")))?;
    if let Some(b) = bak {
        let _ = s.sftp.remove_file(b).await;
    }
    Ok(())
}

/// 删一份文件。回「真删了」（不在 ⇒ `false`，不算错）。
pub(crate) async fn remove(s: &Session, path: &str) -> Result<bool, Refusal> {
    let rel = fenced_remote(s, path, Intent::File).await?;
    if !s.sftp.try_exists(rel.clone()).await.unwrap_or(false) {
        return Ok(false);
    }
    s.sftp
        .remove_file(rel.clone())
        .await
        .map_err(|e| io(format!("删除 ~/{rel} 失败: {e}")))?;
    Ok(true)
}

/// 建**一层**目录（已在 ⇒ 什么都不做）。
pub(crate) async fn make_dir(s: &Session, path: &str) -> Result<(), Refusal> {
    let rel = fenced_remote(s, path, Intent::Dir).await?;
    if s.sftp.try_exists(rel.clone()).await.unwrap_or(false) {
        return Ok(());
    }
    if let Err(e) = s.sftp.create_dir(rel.clone()).await {
        // 并发的另一趟刚建好它 —— 那不算错。
        if !s.sftp.try_exists(rel.clone()).await.unwrap_or(false) {
            return Err(io(format!("建目录 ~/{rel} 失败: {e}")));
        }
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
    if let Ok(m) = s.sftp.symlink_metadata(rel.clone()).await {
        if m.is_symlink() {
            return Err(fenced(format!(
                "refuse write: ~/{rel} 是一条链接 —— 开写会跟过去，拒"
            )));
        }
    }
    let mut flags = OpenFlags::CREATE | OpenFlags::WRITE | OpenFlags::READ;
    if truncate {
        flags |= OpenFlags::TRUNCATE;
    }
    s.sftp
        .open_with_flags(rel.clone(), flags)
        .await
        .map_err(|e| io(format!("开 ~/{rel} 写失败: {e}")))
}

// ═══ 读（不改任何东西；不过围栏）══════════════════════════════════════════════════════

/// 打开一份远端文件只读。
pub(crate) async fn open_for_read(s: &Session, path: &str) -> Result<RemoteFile, String> {
    s.sftp
        .open_with_flags(path.to_string(), OpenFlags::READ)
        .await
        .map_err(|e| format!("打开远端 {path} 失败: {e}"))
}

/// `metadata` 的大小：`None` = 那次调用失败；`Some(None)` = 服务器没给 size（**不是 0 字节**）。
pub(crate) async fn metadata_size(s: &Session, path: &str) -> Option<Option<u64>> {
    s.sftp.metadata(path.to_string()).await.ok().map(|m| m.size)
}

/// 在不在：`None` = 问不出来。
pub(crate) async fn exists(s: &Session, path: &str) -> Option<bool> {
    s.sftp.try_exists(path.to_string()).await.ok()
}

/// 整份读回来；读不出 ⇒ `None`。
pub(crate) async fn read_all(s: &Session, path: &str) -> Option<Vec<u8>> {
    s.sftp.read(path.to_string()).await.ok()
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
        .map_err(|e| format!("读请求行失败：{e}"))?;
    if n == 0 {
        return Ok(None);
    }
    if n as u64 > cap && !line.ends_with('\n') {
        return Err(format!("请求行超过 {cap} 字节"));
    }
    Ok(Some(line.trim_end_matches(['\n', '\r']).to_string()))
}

fn arg_str<'a>(v: &'a serde_json::Value, k: &str) -> Result<&'a str, String> {
    v.get(k)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("缺 `{k}`（字符串）"))
}

fn arg_u64(v: &serde_json::Value, k: &str) -> Result<u64, String> {
    v.get(k)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("缺 `{k}`（非负整数）"))
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
            "stat" => {
                let path = arg_str(req, "path").map_err(bad)?;
                let meta = metadata_size(s, path).await;
                // `metadata` 成功就够判了，不多问一次（与 monitor `interpret_target_probe` 入参同形）。
                let exists = match meta {
                    Some(_) => None,
                    None => exists(s, path).await,
                };
                Ok(serde_json::json!({
                    "meta": meta.map(|size| serde_json::json!({ "size": size })),
                    "exists": exists,
                }))
            }
            "read" => {
                let path = arg_str(req, "path").map_err(bad)?;
                let max = arg_u64(req, "max").map_err(bad)?;
                let data = read_all(s, path).await;
                if let Some(d) = &data {
                    if d.len() as u64 > max {
                        return Err(refused(
                            "too_big",
                            &format!("{path} 有 {} 字节，超过这一问给的上限 {max}", d.len()),
                        ));
                    }
                }
                // 只在需要时补问（与 monitor `interpret_profile_read` 入参同形）：读不出 ⇒ 在不在；读到空 ⇒ 大小。
                let (exists, size) = match &data {
                    None => (exists(s, path).await, None),
                    Some(d) if d.is_empty() => (None, metadata_size(s, path).await.flatten()),
                    Some(_) => (None, None),
                };
                Ok(serde_json::json!({
                    "data": data.as_deref().map(crate::wire::b64_encode),
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
                        &format!("一次 put {size} 字节，超过上限 {MAX_PUT_BYTES}"),
                    ));
                }
                let mut bytes = Vec::with_capacity(size as usize);
                (&mut *input)
                    .take(size)
                    .read_to_end(&mut bytes)
                    .await
                    .map_err(|e| refused("io", &format!("收 put 的字节失败：{e}")))?;
                if bytes.len() as u64 != size {
                    return Err(refused(
                        "bad_request",
                        &format!("put 说 {size} 字节，只收到 {}", bytes.len()),
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
                &format!(
                    "files 链路不认 `{other}`（认 home / stat / read / put / remove / mkdirs）"
                ),
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
