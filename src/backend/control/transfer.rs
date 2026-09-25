//! 〔SR1b · 2026-09-24〕**传输台住本机常驻后端**（`设计/60 §4.6`：「那时 `§4.2` 的传输台从 monitor 搬进本机后端，
//! 窗口那一侧的 `call` / `subscribe` 不变」）。用户 V89「SFTP 进本机常驻后端，只写暂存区」。
//!
//! # 线上四条命令（`Run::Builtin`：要碰本连接的票表与应答通道，同 `link-*`）
//!
//! | 命令 | `args` | 应答 | |
//! |---|---|---|---|
//! | `transfer-upload` | `{dial, local_path}` | `{id, key}` | 开单（不起跑）；`key` = 暂存件的键，提交时交给远端后端 `files-commit-upload` |
//! | `transfer-download` | `{dial, remote_path, local_path}` | `{id}` | 开单；本机落点**当场**过路径解析（出声早），起跑时再过一次 |
//! | `transfer-start` | `{id}` | — | 起跑；进度走出方向 `transfer` 帧（`wire.rs`） |
//! | `transfer-stop` | `{id}` | — | 撤（幂等）。上传删暂存件；下载留 `.part` |
//!
//! **起跑挂在 `transfer-start` 上、不挂在开单上** —— 与旧传输台「起跑挂在订阅上」同一条理由（`设计/60 §4.2`）：
//! monitor 那一侧先登记好看的人、再起跑，终局就不会没人收。
//!
//! # 它是第三层（文件管理写面）的成员
//!
//! 下载要写**本机**用户选的落点（`.part` ＋ 改名上位）⇒ 那是一次用户文件的写，按 `INVARIANTS §41.6` 只许住文件管理那一面：
//! 本模块登记在 `readonly_guard::MUTATING_FACE_MODULES`，改动动词只用闭集里的（`O_EXCL` 新建 · 接着写 · 改名 · 删文件），
//! **每一处改动之前先过 `files_write::resolve_in_root`**（借用、不抄）；门仍只有 `inbound.rs`。
//! 远端那一半（暂存区的写）一行都不在这里 —— 全经 `dial/sftp.rs` 的写原语（只许两处、先过 `fenced_remote`）。
//!
//! # 存亡规矩（逐字沿用旧传输台，`设计/60 §4.3` 那张表）
//!
//! | 事件 | 上传的暂存件 | 下载的 `.part` |
//! |---|---|---|
//! | 撤（`transfer-stop` / 本机流断了） | **删**（用户说了不要） | **留**（续传的本钱） |
//! | 失败 | **留**（续传最值钱的正是这一档） | **留**（〔DP1〕同一条理由；一个字节都没落的空 `.part` 才清） |
//! | 成功 | 远端后端那次提交把它变成目标 | 改名上位 |
//!
//! 判的是**撤的旗**，不是错误文案。续传：两侧都先把**尾块**逐字节对一遍，对得上才接（半成品只记了「写到哪」、
//! 没记「那是谁的字节」，只看长度会缝出一个坏文件）。
//!
//! # 零定时器
//!
//! 撤 = 一面旗 ＋ 一个 `Notify`（等拨号那一段可以被当场打断）；进度 = `watch`（转发任务按变更合并，堵住时只合并不堆积）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{mpsc, watch, Notify};

use crate::control::files_commit::{KEY_LEN, PART_SUFFIX};
use crate::control::files_write::resolve_in_root;
use crate::dial::sftp::{self, Dial, Session};
use crate::wire::{Frame, TransferEnd};

/// 开单：上传。
pub const TRANSFER_UPLOAD: &str = "transfer-upload";
/// 开单：下载。
pub const TRANSFER_DOWNLOAD: &str = "transfer-download";
/// 起跑。
pub const TRANSFER_START: &str = "transfer-start";
/// 撤。
pub const TRANSFER_STOP: &str = "transfer-stop";

/// 本模块的线上命令（`readonly_guard` 第三层 ④ 那条「门里够得到写面的命令」的一侧，与 `inbound.rs` 源码异源）。
pub fn transfer_command_names() -> Vec<&'static str> {
    vec![
        TRANSFER_UPLOAD,
        TRANSFER_DOWNLOAD,
        TRANSFER_START,
        TRANSFER_STOP,
    ]
}

/// SFTP 读写的一块（≤32 KiB：SFTP 草案建议；严格的服务端拒超大包）。
const CHUNK: usize = 32 * 1024;

/// 进度至少隔这么多字节报一次（起止各另报一次）。
const PROGRESS_EVERY: u64 = 256 * 1024;

/// 一条流连接上同时在册的票数上限（有界资源：每张票起跑后两个任务）。
pub const MAX_TICKETS_PER_CONNECTION: usize = 64;

// ═══ 暂存件的键 ════════════════════════════════════════════════════════════════════════

/// 由（本机路径 · 大小 · 修改时间）派生暂存件的键：**同一份文件重拖一次落到同一个暂存件上** ⇒ 续传的尾块对拍照旧生效。
///
/// 〔SR1b〕搬自 monitor `sftp_pool::staging_key`：暂存区、键长、键派生从此与提交那一侧（`files_commit`）**同一个 crate 一份**
/// （原来是「两份逐字副本 ＋ 相等断言」）。⚠ 标准库默认散列**跨 Rust 版本不保证稳定** —— 那只意味着
/// 「升级之后第一次重拖不续传、从 0 来」，尾块对拍另有一道兜底，不会接错。
pub fn staging_key(local_path: &str, size: u64, mtime_ns: u128) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let half = |salt: &str| {
        let mut h = DefaultHasher::new();
        (salt, local_path, size, mtime_ns).hash(&mut h);
        h.finish()
    };
    let w = KEY_LEN / 2;
    format!(
        "{:0w$x}{:0w$x}",
        half("ccm-staging-a"),
        half("ccm-staging-b")
    )
}

/// 一个键的暂存件（home 相对）。
pub fn staging_part(key: &str) -> String {
    format!("{}/{key}{PART_SUFFIX}", sftp::STAGING_ROOT)
}

// ═══ 本机落点（第三层：每一处改动先过路径解析）══════════════════════════════════════════════

/// 本机落点拆成 `(父目录 = 路径解析的根, 文件名, 半成品名)`。必须是绝对路径、有文件名。
fn land_parts(local_path: &str) -> Result<(PathBuf, String, String), String> {
    let p = Path::new(local_path);
    if !p.is_absolute() {
        return Err(format!("本机落点必须是绝对路径：{local_path}"));
    }
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("本机落点没有文件名：{local_path}"))?
        .to_string();
    let root = p
        .parent()
        .ok_or_else(|| format!("本机落点没有父目录：{local_path}"))?
        .to_path_buf();
    let part = format!("{name}.part");
    Ok((root, name, part))
}

/// 开单时的那一判（不动盘）：落点与它的半成品都过得了路径解析。
pub fn land_check(local_path: &str) -> Result<(), String> {
    let (root, name, part) = land_parts(local_path)?;
    resolve_in_root(&root, &name)?;
    resolve_in_root(&root, &part)?;
    Ok(())
}

/// 从 0 开一份半成品：旧的在就先删（它的尾块已经对不上了），再 `O_EXCL` 新建。
fn land_open_fresh(root: &Path, part: &str) -> Result<std::fs::File, String> {
    let at = resolve_in_root(root, part)?;
    if std::fs::symlink_metadata(&at).is_ok() {
        std::fs::remove_file(&at).map_err(|e| format!("删旧半成品 {} 失败: {e}", at.display()))?;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&at)
        .map_err(|e| format!("创建本地 {} 失败: {e}", at.display()))
}

/// 续传：**不原地接着写**（第三层禁「续写」那种开法：每一个写句柄都得是 `O_EXCL` 新建）⇒
/// 旧半成品改名成 `<名>.old` → `O_EXCL` 新建 `<名>` → 把前 `keep` 字节从旧的抄过来 → 删旧的。
/// 回来的句柄游标停在 `keep`。代价如实记：前缀在本机盘上多抄一遍（本机盘速，不走网）。
fn land_carry_over(root: &Path, part: &str, keep: u64) -> Result<std::fs::File, String> {
    let at = resolve_in_root(root, part)?;
    let old_name = format!("{part}.old");
    let old = resolve_in_root(root, &old_name)?;
    if std::fs::symlink_metadata(&old).is_ok() {
        std::fs::remove_file(&old).map_err(|e| format!("删残留 {} 失败: {e}", old.display()))?;
    }
    std::fs::rename(&at, &old).map_err(|e| format!("挪开旧半成品 {} 失败: {e}", at.display()))?;
    let mut fresh = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&at)
        .map_err(|e| format!("创建本地 {} 失败: {e}", at.display()))?;
    let copied = {
        let src = std::fs::File::open(&old)
            .map_err(|e| format!("读旧半成品 {} 失败: {e}", old.display()))?;
        std::io::copy(&mut std::io::Read::take(src, keep), &mut fresh)
            .map_err(|e| format!("抄旧半成品的前 {keep} 字节失败: {e}"))?
    };
    std::fs::remove_file(&old).map_err(|e| format!("删旧半成品 {} 失败: {e}", old.display()))?;
    if copied != keep {
        return Err(format!("旧半成品只抄出 {copied} 字节（要 {keep}）"));
    }
    Ok(fresh)
}

/// 传完：半成品改名上位。
fn land_commit(root: &Path, part: &str, name: &str) -> Result<(), String> {
    let from = resolve_in_root(root, part)?;
    let to = resolve_in_root(root, name)?;
    std::fs::rename(&from, &to).map_err(|e| format!("落地 {} 失败: {e}", to.display()))
}

/// 失败（不是撤）：删掉半成品。
fn land_discard(root: &Path, part: &str) {
    if let Ok(at) = resolve_in_root(root, part) {
        let _ = std::fs::remove_file(&at);
    }
}

// ═══ 撤 ════════════════════════════════════════════════════════════════════════════════

/// 一趟传输的「撤了没有」：旗（逐块看）＋ 通知（等拨号那一段可以当场打断）。
#[derive(Default)]
pub(crate) struct Cancel {
    flag: AtomicBool,
    notify: Notify,
}

impl Cancel {
    pub(crate) fn fire(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }
    pub(crate) fn is_set(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
    async fn wait(&self) {
        loop {
            let n = self.notify.notified();
            if self.is_set() {
                return;
            }
            n.await;
        }
    }
}

// ═══ 两种传输的本体（语料是一条 SFTP 会话 —— 判据拿合成服务端直接喂它）═══════════════════════

/// 进度的出口：`(已到, 总共)`。
pub(crate) type Sink<'a> = dyn Fn(u64, u64) + Send + Sync + 'a;

/// 从一条可寻址的流里，从 `at` 起**精确**读 `len` 字节；读不满 / 出错都回 `None`（两侧共用这一份）。
async fn read_exact_at<S>(s: &mut S, at: u64, len: usize) -> Option<Vec<u8>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
{
    s.seek(std::io::SeekFrom::Start(at)).await.ok()?;
    let mut buf = vec![0u8; len];
    s.read_exact(&mut buf).await.ok()?;
    Some(buf)
}

/// 尾块对拍：`have` 字节的半成品与完整件在 `[have - 一块, have)` 上逐字节相同 ⇒ 回 `have`；否则 `0`。
async fn tails_agree<A, B>(half: &mut A, full: &mut B, have: u64, total: u64) -> u64
where
    A: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
    B: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
{
    if have == 0 || total == 0 || have > total {
        return 0;
    }
    let probe = have.min(CHUNK as u64);
    let at = have - probe;
    match (
        read_exact_at(half, at, probe as usize).await,
        read_exact_at(full, at, probe as usize).await,
    ) {
        (Some(a), Some(b)) if a == b => have,
        _ => 0,
    }
}

/// 失败之后把**已经发出去的写**一条条等到回话（坏的也要等完），最多这么多轮。
///
/// `russh-sftp` 的写是流水线的（一个文件最多 16 条在路上）：第一条坏回话就让 `flush` / `shutdown` 返回，
/// 后面已发出的写还在路上 —— 它们什么时候落、落不落，取决于对面。不等完就走，半成品的样子就不是「此刻」的样子：
/// 续传那一趟的尾块对拍只看尾巴，看不见中间被晚到的写留下的洞。每一轮 `flush` 至少收走一条回话，
/// 16 条在路上 ＋ 一条关文件，这个上限是它的几倍，只防「对面一直答坏」时原地打转（不是定时器，是次数）。
const DRAIN_ROUNDS: usize = 64;

/// 见 [`DRAIN_ROUNDS`]。
async fn drain_sent_writes(rf: &mut sftp::RemoteFile) {
    for _ in 0..DRAIN_ROUNDS {
        if rf.flush().await.is_ok() {
            return;
        }
    }
}

/// 🔴 **上传的唯一形状**：本机文件 → 暂存件 `~/.cc-monitor/staging/<key>.part`。回传完的字节数。
///
/// 暂存区不在就建最后那一段（上一级 `~/.cc-monitor` 不在 ⇒ 报错，**不顺手建** —— D11：后端是给定的，提交也要它在）。
/// 孤儿不在这里扫（远端后端 `files_commit::sweep_stale`，每次提交成功时顺手扫）。
pub(crate) async fn upload_to_staging(
    s: &Session,
    local_path: &str,
    key: &str,
    cancel: &Cancel,
    on_progress: &Sink<'_>,
) -> Result<u64, String> {
    let total = tokio::fs::metadata(local_path)
        .await
        .map(|m| m.len())
        .map_err(|e| format!("读本地 {local_path} 失败: {e}"))?;
    let mut lf = tokio::fs::File::open(local_path)
        .await
        .map_err(|e| format!("打开本地 {local_path} 失败: {e}"))?;
    if sftp::exists(s, sftp::STAGING_ROOT).await != Some(true) {
        if sftp::exists(s, ".cc-monitor").await != Some(true) {
            return Err(
                "那台机器上没有 ~/.cc-monitor（后端还没部署）—— 上传要先落进它底下的暂存区，\
                 而提交要那台机器上的后端来做"
                    .to_string(),
            );
        }
        sftp::make_dir(s, sftp::STAGING_ROOT)
            .await
            .map_err(|e| e.to_string())?;
    }
    let part = staging_part(key);
    let have = sftp::metadata_size(s, &part).await.flatten().unwrap_or(0);
    let resume_from = if have > 0 {
        match sftp::open_for_read(s, &part).await {
            Ok(mut rf) => tails_agree(&mut rf, &mut lf, have, total).await,
            Err(_) => 0,
        }
    } else {
        0
    };
    let mut rf = sftp::open_for_write(s, &part, resume_from == 0)
        .await
        .map_err(|e| e.to_string())?;
    // 两侧无条件 seek：尾块对拍动过 `lf` 的游标。
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
            if cancel.is_set() {
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
    if core.is_err() {
        drain_sent_writes(&mut rf).await;
    }
    let _ = rf.shutdown().await;
    drop(rf);
    match core {
        Ok(done) => {
            on_progress(done, total);
            Ok(done)
        }
        Err(e) => {
            // 🔴 撤 ⇒ 删（用户说了不要）；失败 ⇒ 留（续传的本钱，在我们自己的目录里）。
            if cancel.is_set() {
                let _ = sftp::remove(s, &part).await;
            }
            Err(e)
        }
    }
}

/// **下载**：远端只读；本机落点 `<落点>.part` ＋ 改名上位（第三层：每一处改动先过路径解析）。回落地的字节数。
pub(crate) async fn download_to_local(
    s: &Session,
    remote_path: &str,
    local_path: &str,
    cancel: &Cancel,
    on_progress: &Sink<'_>,
) -> Result<u64, String> {
    let (root, name, part) = land_parts(local_path)?;
    let total = sftp::metadata_size(s, remote_path)
        .await
        .flatten()
        .unwrap_or(0);
    let mut rf = sftp::open_for_read(s, remote_path).await?;
    let part_path = root.join(&part);
    let have = tokio::fs::metadata(&part_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    // 〔FW1 · 第四波〕`rf_at` = 远端句柄此刻停在哪（知道才填）。尾块对上了 ⇒ 探针那一读恰好读满到 `have`，
    //   句柄就停在 `have` == `resume_from`。
    let (resume_from, rf_at) = if have > 0 {
        match tokio::fs::File::open(&part_path).await {
            Ok(mut half) => {
                let r = tails_agree(&mut half, &mut rf, have, total).await;
                (r, (r == have).then_some(have))
            }
            Err(_) => (0, Some(0)),
        }
    } else {
        (0, Some(0))
    };
    let std_file = if resume_from > 0 {
        land_carry_over(&root, &part, resume_from)?
    } else {
        land_open_fresh(&root, &part)?
    };
    // 两种开法回来的游标都停在 `resume_from`（新建 ⇒ 0；抄完前缀 ⇒ 前缀末尾）。
    let mut lf = tokio::fs::File::from_std(std_file);
    // 🔴〔FW1 · 第四波〕**句柄已经停在 `resume_from` 就不 seek**。russh-sftp 的第一读按服务端肯给的最大包请求
    //   （约 255 KiB，远大于探针那 32 KiB），多出来的那一截留在它的读缓冲里 —— 正好是续传要的下一段；
    //   seek 一下（哪怕 seek 到原地）就把缓冲整个扔掉、从 `have` 再请求一遍。DP1 真 sshd 上量到的「一次续传多读
    //   228 352 字节」= 255 KiB − 32 KiB 就是这一截。病根是 seek 丢缓冲，不是探针与续传共用一个句柄（单开一个句柄，
    //   它的第一读照样请求那么大）。只有对不上（从 0 来）或探针读坏了（位置说不准）才 seek。
    if rf_at != Some(resume_from) {
        rf.seek(std::io::SeekFrom::Start(resume_from))
            .await
            .map_err(|e| format!("远端 {remote_path} 定位到 {resume_from} 失败: {e}"))?;
    }
    let core = async {
        let mut buf = vec![0u8; CHUNK];
        let mut done: u64 = resume_from;
        let mut last_report: u64 = resume_from;
        on_progress(done, total);
        loop {
            if cancel.is_set() {
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
            // 🔴 撤 ⇒ **留着** `.part`（下次续传的本钱）。〔DP1 · 第四波〕失败 ⇒ **也留着**：弱网上「连接没了」就是失败，
            //   从前这里失败即删，断一次线续传的本钱全没、重下从 0 起（NT1 报备 2 现打）。与上传「失败留」同一条理由
            //   （`设计/60 §4.3`：「续传最值钱的正是这一档；重拖同一份从尾块接上」）；不按错误文案分「断线 / 别的失败」——
            //   判的是撤的旗，不是错误文案。唯一还清的一形：一个字节都没落（空 `.part` 没有续传的本钱，只是垃圾）。
            let landed = tokio::fs::metadata(root.join(&part))
                .await
                .map_or(0, |m| m.len());
            if !cancel.is_set() && landed == 0 {
                land_discard(&root, &part);
            }
            return Err(e);
        }
    };
    land_commit(&root, &part, &name)?;
    on_progress(done, total);
    Ok(done)
}

// ═══ 票表（每条流连接一张）═══════════════════════════════════════════════════════════════

enum Job {
    Upload { local: String, key: String },
    Download { remote: String, local: String },
}

struct Ticket {
    dial: Dial,
    /// 还没起跑的那件事（`transfer-start` 取走）。
    job: Option<Job>,
    /// 上传那一路的暂存件键（同一个键同时只许一张票）。
    key: Option<String>,
    cancel: Arc<Cancel>,
}

type Tickets = Arc<Mutex<HashMap<String, Ticket>>>;

fn lock(t: &Tickets) -> std::sync::MutexGuard<'_, HashMap<String, Ticket>> {
    t.lock().unwrap_or_else(|e| e.into_inner())
}

/// 一条流连接的传输票表。
pub struct Desk {
    tickets: Tickets,
    replies: mpsc::Sender<Frame>,
}

impl Drop for Desk {
    /// 连接没了（monitor 走了）⇒ 它开的传输一律撤（= 旧形「连接断了 ⇒ 撤」）。
    fn drop(&mut self) {
        for (_, t) in lock(&self.tickets).drain() {
            t.cancel.fire();
        }
    }
}

fn ok(id: &str, data: Option<serde_json::Value>) -> Frame {
    Frame::Reply {
        id: id.to_string(),
        ok: true,
        code: None,
        message: None,
        data,
    }
}

fn err(id: &str, code: &str, message: &str) -> Frame {
    Frame::Reply {
        id: id.to_string(),
        ok: false,
        code: Some(code.to_string()),
        message: Some(message.to_string()),
        data: None,
    }
}

fn text<'a>(args: &'a serde_json::Value, k: &str) -> Result<&'a str, String> {
    args.get(k)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("少了 `{k}`，或者它不是一个字符串"))
}

fn dial_of(args: &serde_json::Value) -> Result<Dial, String> {
    let d = args
        .get("dial")
        .ok_or_else(|| "少了 `dial`（一份拨号请求）".to_string())?;
    Dial::parse(d)
}

/// 票号：本进程里单调（票只在一条连接里有意义；`xfer-` 前缀与旧传输台同形）。
fn next_id() -> String {
    static N: AtomicU64 = AtomicU64::new(1);
    format!("xfer-{}", N.fetch_add(1, Ordering::SeqCst))
}

impl Desk {
    pub fn new(replies: mpsc::Sender<Frame>) -> Self {
        Desk {
            tickets: Arc::new(Mutex::new(HashMap::new())),
            replies,
        }
    }

    /// **本面唯一的答口**（`inbound.rs` 那四条硬臂都经它进来，`files/module_boundary_guard::DOORS` 登记）。
    pub fn answer_wire(&self, cmd: &str, id: &str, args: &serde_json::Value) -> Frame {
        match cmd {
            TRANSFER_UPLOAD => self.upload(id, args),
            TRANSFER_DOWNLOAD => self.download(id, args),
            TRANSFER_START => self.start(id, args),
            TRANSFER_STOP => self.stop(id, args),
            other => err(
                id,
                "unknown_command",
                &format!("`{other}` 不是传输台的命令"),
            ),
        }
    }

    /// 此刻在册的票数（生产日志读它）。
    pub(crate) fn len(&self) -> usize {
        lock(&self.tickets).len()
    }

    fn register(&self, id: &str, t: Ticket) -> Result<String, Frame> {
        let mut g = lock(&self.tickets);
        if g.len() >= MAX_TICKETS_PER_CONNECTION {
            return Err(err(
                id,
                "too_many_transfers",
                &format!("这条连接上已经有 {MAX_TICKETS_PER_CONNECTION} 趟传输在册"),
            ));
        }
        if let Some(k) = &t.key {
            if g.values().any(|o| o.key.as_ref() == Some(k)) {
                return Err(err(
                    id,
                    "busy",
                    "同一份文件正在往那台机器上传（同一个暂存件），等它收场再来",
                ));
            }
        }
        let tid = next_id();
        g.insert(tid.clone(), t);
        Ok(tid)
    }

    /// `transfer-upload`：开单（不起跑）。
    fn upload(&self, id: &str, args: &serde_json::Value) -> Frame {
        let (dial, local) = match dial_of(args).and_then(|d| Ok((d, text(args, "local_path")?))) {
            Ok(v) => v,
            Err(m) => return err(id, "bad_args", &m),
        };
        let meta = match std::fs::metadata(local) {
            Ok(m) => m,
            Err(e) => return err(id, "io_failed", &format!("读本地 {local} 失败: {e}")),
        };
        if !meta.is_file() {
            return err(id, "bad_args", &format!("{local} 不是一份普通文件"));
        }
        let mtime_ns = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let key = staging_key(local, meta.len(), mtime_ns);
        let t = Ticket {
            dial,
            job: Some(Job::Upload {
                local: local.to_string(),
                key: key.clone(),
            }),
            key: Some(key.clone()),
            cancel: Arc::new(Cancel::default()),
        };
        match self.register(id, t) {
            Ok(tid) => ok(id, Some(serde_json::json!({ "id": tid, "key": key }))),
            Err(f) => f,
        }
    }

    /// `transfer-download`：开单；本机落点当场过路径解析。
    fn download(&self, id: &str, args: &serde_json::Value) -> Frame {
        let parsed = dial_of(args).and_then(|d| {
            Ok((
                d,
                text(args, "remote_path")?.to_string(),
                text(args, "local_path")?.to_string(),
            ))
        });
        let (dial, remote, local) = match parsed {
            Ok(v) => v,
            Err(m) => return err(id, "bad_args", &m),
        };
        if let Err(e) = land_check(&local) {
            return err(id, "refused", &e);
        }
        let t = Ticket {
            dial,
            job: Some(Job::Download { remote, local }),
            key: None,
            cancel: Arc::new(Cancel::default()),
        };
        match self.register(id, t) {
            Ok(tid) => ok(id, Some(serde_json::json!({ "id": tid }))),
            Err(f) => f,
        }
    }

    /// `transfer-stop`：撤。没起跑的票当场摘掉；起跑了的由它自己收场（撤删 / 撤留）后摘掉。
    fn stop(&self, id: &str, args: &serde_json::Value) -> Frame {
        let tid = match text(args, "id") {
            Ok(t) => t.to_string(),
            Err(m) => return err(id, "bad_args", &m),
        };
        let mut g = lock(&self.tickets);
        if let Some(t) = g.get(&tid) {
            t.cancel.fire();
            if t.job.is_some() {
                g.remove(&tid);
            }
        }
        ok(id, None)
    }

    /// `transfer-start`：起跑。两个任务：传输本体 · 进度转发（`watch` 合并）。
    fn start(&self, id: &str, args: &serde_json::Value) -> Frame {
        let tid = match text(args, "id") {
            Ok(t) => t.to_string(),
            Err(m) => return err(id, "bad_args", &m),
        };
        let (dial, job, cancel) = {
            let mut g = lock(&self.tickets);
            let Some(t) = g.get_mut(&tid) else {
                return err(id, "no_such_transfer", &format!("没有这一趟传输（{tid}）"));
            };
            let Some(job) = t.job.take() else {
                return err(
                    id,
                    "already_started",
                    &format!("这一趟传输已经起跑了（{tid}）"),
                );
            };
            (t.dial.clone(), job, Arc::clone(&t.cancel))
        };
        tracing::info!(
            "transfer: 起跑 {tid}（这条连接上此刻 {} 趟在册）",
            self.len()
        );
        let (tx, rx) = watch::channel::<Progress>(Progress::default());
        let forward = tokio::spawn(forward_progress(tid.clone(), rx, self.replies.clone()));
        let tickets = Arc::clone(&self.tickets);
        tokio::spawn(async move {
            let tx = Arc::new(tx);
            let sink_tx = Arc::clone(&tx);
            let sink = move |got: u64, total: u64| {
                sink_tx.send_modify(|p| {
                    p.got = got;
                    p.total = total;
                });
            };
            let r = run(&dial, job, &cancel, &sink).await;
            let end = match r {
                Ok(bytes) => TransferEnd::Done { bytes },
                Err(_) if cancel.is_set() => TransferEnd::Cancelled,
                Err(why) => TransferEnd::Failed { why },
            };
            tx.send_modify(|p| p.end = Some(end));
            // 等转发把终局那一帧送出去再摘票（摘早了，一条同键的新上传可能抢在撤删之前开单）。
            let _ = forward.await;
            lock(&tickets).remove(&tid);
        });
        ok(id, None)
    }
}

/// 一趟传输此刻的样子（整份快照，合并掉中间几格不丢信息）。
#[derive(Clone, Default)]
struct Progress {
    got: u64,
    total: u64,
    end: Option<TransferEnd>,
}

/// 进度转发：第一格是此刻，之后每变一次出一格，终局那一格是最后一格。走应答通道（不丢）。
async fn forward_progress(
    id: String,
    mut rx: watch::Receiver<Progress>,
    replies: mpsc::Sender<Frame>,
) {
    loop {
        let p = rx.borrow_and_update().clone();
        let last = p.end.is_some();
        let frame = Frame::Transfer {
            id: id.clone(),
            got: p.got,
            total: p.total,
            end: p.end,
        };
        if replies.send(frame).await.is_err() || last {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

/// 一趟：拨号 ＋ 开会话（过传输车道；这一段可以被撤当场打断）→ 按单子传。
async fn run(dial: &Dial, job: Job, cancel: &Cancel, sink: &Sink<'_>) -> Result<u64, String> {
    let session = tokio::select! {
        s = sftp::open_for_transfer(dial) => s?,
        _ = cancel.wait() => return Err("已取消".to_string()),
    };
    match job {
        Job::Upload { local, key } => upload_to_staging(&session, &local, &key, cancel, sink).await,
        Job::Download { remote, local } => {
            download_to_local(&session, &remote, &local, cancel, sink).await
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/transfer_tests.rs"]
mod tests;
