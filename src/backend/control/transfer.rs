//! 传输台住本机常驻后端：SFTP 只写暂存区，窗口那一侧的 `call` / `subscribe` 不变。
//!
//! # 线上四条命令（`Run::Builtin`：要碰本连接的票表与应答通道，同 `link-*`）
//!
//! | 命令 | `args` | 应答 | |
//! |---|---|---|---|
//! | `transfer-upload` | `{dial, local_path}` | `{id, key}` | 开单（不起跑）；`key` = 暂存件的键，提交时交给远端后端 `files-commit-upload` |
//! | `transfer-download` | `{dial, remote_path, local_path}` | `{id}` | 开单；本机落点当场过路径解析（出声早），起跑时再过一次 |
//! | `transfer-start` | `{id}` | — | 起跑；进度走出方向 `transfer` 帧（`wire.rs`） |
//! | `transfer-stop` | `{id}` | — | 撤（幂等）。上传删暂存件；下载留 `.part` |
//!
//! 起跑挂在 `transfer-start` 上、不挂在开单上：monitor 先登记好看的人、再起跑，终局就不会没人收。
//!
//! # 它是第三层（文件管理写面）的成员
//!
//! 下载要写本机用户选的落点（`.part` ＋ 改名上位）⇒ 按 `INVARIANTS §41.6` 只许住文件管理那一面：
//! 本模块登记在 `readonly_guard::MUTATING_FACE_MODULES`，改动动词只用闭集里的（`O_EXCL` 新建 · 接着写 · 改名 · 删文件），
//! 每一处改动之前先过 `files_write::resolve_in_root`；门只有 `stream/inbound/`。
//! 远端那一半（暂存区的写）全经 `dial/sftp.rs` 的写原语（只许两处、先过 `fenced_remote`）。
//!
//! # 存亡规矩
//!
//! | 事件 | 上传的暂存件 | 下载的 `.part` |
//! |---|---|---|
//! | 撤（`transfer-stop` / 本机流断了） | 删（用户说了不要） | 留（续传的本钱） |
//! | 失败 | 留（续传最值钱的正是这一档） | 留（同一条理由；一个字节都没落的空 `.part` 才清） |
//! | 成功 | 远端后端那次提交把它变成目标 | 改名上位 |
//!
//! 判的是撤的旗，不是错误文案。续传：两侧都先把尾块逐字节对一遍，对得上才接（半成品只记了「写到哪」，
//! 没记「那是谁的字节」，只看长度会缝出一个坏文件）。
//!
//! # 零定时器
//!
//! 撤 = 一面旗 ＋ 一个 `Notify`（等拨号那一段可以被当场打断）；进度 = `watch`（转发任务按变更合并，堵住时只合并不堆积）。

use copy_core::{copy_text, io_reason};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{mpsc, watch, Notify};

use crate::common::said::Said;
use crate::control::files_commit::{KEY_LEN, PART_SUFFIX};
use crate::control::files_write::{opener, resolve_in_root};
use crate::dial::sftp::{self, Dial, Session};
use crate::stream::wire::{Frame, TransferEnd};

/// 开单：上传。
pub const TRANSFER_UPLOAD: &str = "transfer-upload";
/// 开单：下载。
pub const TRANSFER_DOWNLOAD: &str = "transfer-download";
/// 起跑。
pub const TRANSFER_START: &str = "transfer-start";
/// 撤。
pub const TRANSFER_STOP: &str = "transfer-stop";
/// SFTP 读写的一块（≤32 KiB：SFTP 草案建议；严格的服务端拒超大包）。
const CHUNK: usize = 32 * 1024;

/// 进度至少隔这么多字节报一次（起止各另报一次）。
const PROGRESS_EVERY: u64 = 256 * 1024;

/// 一条流连接上同时在册的票数上限（有界资源：每张票起跑后两个任务）。
pub const MAX_TICKETS_PER_CONNECTION: usize = 64;

// ═══ 暂存件的键 ════════════════════════════════════════════════════════════════════════

/// 由（本机路径 · 大小 · 修改时间）派生暂存件的键：同一份文件重拖一次落到同一个暂存件上 ⇒ 续传的尾块对拍照旧生效。
/// 暂存区、键长、键派生与提交那一侧（`files_commit`）同在本 crate。标准库默认散列跨 Rust 版本不保证稳定 ——
/// 只意味着升级之后第一次重拖从 0 来；尾块对拍另有一道兜底，不会接错。
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
/// 名字按 `OsString` 拿（Linux 上落点可以是非 UTF-8 的原始字节：有损名下载「字节原样当文件名」）。
fn land_parts(local_path: &Path) -> Result<(PathBuf, OsString, OsString), Said> {
    let p = local_path;
    let shown = p.display().to_string();
    if !p.is_absolute() {
        return Err(copy_text("beTransfer.land.notAbsolute", &[("path", &shown)]).into());
    }
    let name = p
        .file_name()
        .ok_or_else(|| copy_text("beTransfer.land.noName", &[("path", &shown)]))?
        .to_os_string();
    let root = p
        .parent()
        .ok_or_else(|| copy_text("beTransfer.land.noParent", &[("path", &shown)]))?
        .to_path_buf();
    let mut part = name.clone();
    part.push(".part");
    Ok((root, name, part))
}

/// 开单时的那一判（不动盘）：落点与它的半成品都过得了路径解析。
pub fn land_check(local_path: impl AsRef<Path>) -> Result<(), String> {
    // 只拆路径、只过路径解析：没有下层原话。
    let (root, name, part) = land_parts(local_path.as_ref()).map_err(|s| s.said)?;
    resolve_in_root(&root, &name)?;
    resolve_in_root(&root, &part)?;
    Ok(())
}

/// 从 0 开一份半成品：旧的在就先删（它的尾块已经对不上了），再 `O_EXCL` 新建。
fn land_open_fresh(root: &Path, part: &OsStr) -> Result<std::fs::File, Said> {
    let at = resolve_in_root(root, part)?;
    if std::fs::symlink_metadata(&at).is_ok() {
        std::fs::remove_file(&at).map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beTransfer.land.dropPartFailed",
                    &[
                        ("path", &at.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })?;
    }
    opener()
        .write(true)
        .create_new(true)
        .open(&at)
        .map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beTransfer.land.createFailed",
                    &[
                        ("path", &at.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })
}

/// 续传：**不原地接着写**（第三层禁「续写」那种开法：每一个写句柄都得是 `O_EXCL` 新建）⇒
/// 旧半成品改名成 `<名>.old` → `O_EXCL` 新建 `<名>` → 把前 `keep` 字节从旧的抄过来 → 删旧的。
/// 回来的句柄游标停在 `keep`。代价如实记：前缀在本机盘上多抄一遍（本机盘速，不走网）。
fn land_carry_over(root: &Path, part: &OsStr, keep: u64) -> Result<std::fs::File, Said> {
    let at = resolve_in_root(root, part)?;
    let mut old_name = part.to_os_string();
    old_name.push(".old");
    let old = resolve_in_root(root, &old_name)?;
    if std::fs::symlink_metadata(&old).is_ok() {
        std::fs::remove_file(&old).map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beTransfer.land.dropLeftoverFailed",
                    &[
                        ("path", &old.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })?;
    }
    std::fs::rename(&at, &old).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beTransfer.land.moveAsideFailed",
                &[
                    ("path", &at.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    let mut fresh = opener()
        .write(true)
        .create_new(true)
        .open(&at)
        .map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beTransfer.land.createFailed",
                    &[
                        ("path", &at.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })?;
    let copied = {
        let src = opener().read(true).open(&old).map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beTransfer.land.readPartFailed",
                    &[
                        ("path", &old.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })?;
        std::io::copy(&mut std::io::Read::take(src, keep), &mut fresh).map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beTransfer.land.copyPrefixFailed",
                    &[("n", &keep.to_string()), ("why", &io_reason(e.kind()))],
                ),
                &e,
            )
        })?
    };
    std::fs::remove_file(&old).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beTransfer.land.dropPartFailed",
                &[
                    ("path", &old.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    if copied != keep {
        return Err(copy_text(
            "beTransfer.land.copyPrefixShort",
            &[("got", &copied.to_string()), ("n", &keep.to_string())],
        )
        .into());
    }
    Ok(fresh)
}

/// 传完：半成品改名上位。
fn land_commit(root: &Path, part: &OsStr, name: &OsStr) -> Result<(), Said> {
    let from = resolve_in_root(root, part)?;
    let to = resolve_in_root(root, name)?;
    std::fs::rename(&from, &to).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beTransfer.land.commitFailed",
                &[
                    ("path", &to.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })
}

/// 失败（不是撤）：删掉半成品。
fn land_discard(root: &Path, part: &OsStr) {
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

/// 上传的唯一形状：本机文件 → 暂存件 `~/.cc-monitor/staging/<key>.part`。回传完的字节数 ＋ 整份的摘要。
///
/// 摘要是提交那一下的对拍依据：一边传一边对本机那份逐字节算 SHA-256（续传时先把前缀在本机读一遍算进去，不过网）；
/// 远端后端改名上位之前对暂存件算一遍，不等 ⇒ 拒并删掉那份暂存件。尾块对拍看不见「前缀 ＋ 洞 ＋ 尾巴」，整份摘要看得见。
///
/// 暂存区不在就建最后那一段（上一级 `~/.cc-monitor` 不在 ⇒ 报错，不顺手建：后端是给定的，提交也要它在）。
/// 孤儿不在这里扫（远端后端 `files_commit::sweep_stale`，每次提交成功时顺手扫）。
pub(crate) async fn upload_to_staging(
    s: &Session,
    local_path: &str,
    key: &str,
    cancel: &Cancel,
    on_progress: &Sink<'_>,
) -> Result<(u64, String), Said> {
    let total = tokio::fs::metadata(local_path)
        .await
        .map(|m| m.len())
        .map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beTransfer.local.readFailed",
                    &[("path", local_path), ("why", &io_reason(e.kind()))],
                ),
                &e,
            )
        })?;
    let mut lf = tokio::fs::File::open(local_path).await.map_err(|e| {
        Said::with_raw(
            copy_text(
                "beTransfer.local.readFailed",
                &[("path", local_path), ("why", &io_reason(e.kind()))],
            ),
            &e,
        )
    })?;
    if sftp::exists(s, sftp::STAGING_ROOT).await != Some(true) {
        if sftp::exists(s, ".cc-monitor").await != Some(true) {
            return Err(copy_text("beTransfer.upload.notDeployed", &[]).into());
        }
        sftp::make_dir(s, sftp::STAGING_ROOT)
            .await
            .map_err(Said::from)?;
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
        .map_err(Said::from)?;
    // 两侧无条件 seek：尾块对拍动过 `lf` 的游标。
    rf.seek(std::io::SeekFrom::Start(resume_from))
        .await
        .map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beTransfer.upload.seekStagedFailed",
                    &[
                        ("n", &resume_from.to_string()),
                        ("why", &sftp::why_of_io(&e)),
                    ],
                ),
                &e,
            )
        })?;
    // 续传：接上的那一截前缀在本机读一遍算进摘要（与发出去的那一份逐字节同源：同一个本机文件）。
    let mut digest = crate::files::ContentDigest::new();
    if resume_from > 0 {
        lf.seek(std::io::SeekFrom::Start(0)).await.map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beTransfer.local.readFailed",
                    &[("path", local_path), ("why", &io_reason(e.kind()))],
                ),
                &e,
            )
        })?;
        let mut left = resume_from;
        let mut buf = vec![0u8; CHUNK];
        while left > 0 {
            let want = left.min(CHUNK as u64) as usize;
            lf.read_exact(&mut buf[..want]).await.map_err(|e| {
                Said::with_raw(
                    copy_text(
                        "beTransfer.local.readFailed",
                        &[("path", local_path), ("why", &io_reason(e.kind()))],
                    ),
                    &e,
                )
            })?;
            digest.update(&buf[..want]);
            left -= want as u64;
        }
    }
    lf.seek(std::io::SeekFrom::Start(resume_from))
        .await
        .map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beTransfer.local.seekFailed",
                    &[
                        ("path", local_path),
                        ("n", &resume_from.to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })?;
    let core = async {
        let mut buf = vec![0u8; CHUNK];
        let mut done: u64 = resume_from;
        let mut last_report: u64 = resume_from;
        on_progress(done, total);
        loop {
            if cancel.is_set() {
                return Err(copy_text("beTransfer.run.cancelled", &[]).into());
            }
            let n = lf.read(&mut buf).await.map_err(|e| {
                Said::with_raw(
                    copy_text(
                        "beTransfer.local.readFailed",
                        &[("path", local_path), ("why", &io_reason(e.kind()))],
                    ),
                    &e,
                )
            })?;
            if n == 0 {
                break;
            }
            rf.write_all(&buf[..n]).await.map_err(|e| {
                Said::with_raw(
                    copy_text(
                        "beTransfer.upload.writeRemoteFailed",
                        &[("why", &sftp::why_of_io(&e))],
                    ),
                    &e,
                )
            })?;
            digest.update(&buf[..n]);
            done += n as u64;
            if done - last_report >= PROGRESS_EVERY {
                last_report = done;
                on_progress(done, total);
            }
        }
        rf.flush().await.map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beTransfer.upload.writeRemoteFailed",
                    &[("why", &sftp::why_of_io(&e))],
                ),
                &e,
            )
        })?;
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
            Ok((done, digest.finish()))
        }
        Err(e) => {
            // 撤 ⇒ 删（用户说了不要）；失败 ⇒ 留（续传的本钱，在我们自己的目录里）。
            if cancel.is_set() {
                let _ = sftp::remove(s, &part).await;
            }
            Err(e)
        }
    }
}

/// 下载：远端只读；本机落点 `<落点>.part` ＋ 改名上位（第三层：每一处改动先过路径解析）。回落地的字节数。
pub(crate) async fn download_to_local(
    s: &Session,
    remote_path: &str,
    local_path: impl AsRef<Path>,
    cancel: &Cancel,
    on_progress: &Sink<'_>,
) -> Result<u64, Said> {
    let (root, name, part) = land_parts(local_path.as_ref())?;
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
    // `rf_at` = 远端句柄此刻停在哪（知道才填）。尾块对上了 ⇒ 探针那一读恰好读满到 `have`，
    //   句柄就停在 `have` == `resume_from`。
    let (resume_from, rf_at) = if have > 0 {
        match tokio::fs::OpenOptions::from(opener())
            .read(true)
            .open(&part_path)
            .await
        {
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
    // 句柄已经停在 `resume_from` 就不 seek：russh-sftp 的第一读按服务端肯给的最大包请求（约 255 KiB，远大于探针那 32 KiB），
    // 多出来的那一截留在读缓冲里，正好是续传要的下一段；seek 一下（哪怕原地）就把缓冲扔掉、从 `have` 再请求一遍。
    // 只有对不上（从 0 来）或探针读坏了（位置说不准）才 seek。
    if rf_at != Some(resume_from) {
        rf.seek(std::io::SeekFrom::Start(resume_from))
            .await
            .map_err(|e| {
                Said::with_raw(
                    copy_text(
                        "beTransfer.download.seekFailed",
                        &[
                            ("path", remote_path),
                            ("n", &resume_from.to_string()),
                            ("why", &sftp::why_of_io(&e)),
                        ],
                    ),
                    &e,
                )
            })?;
    }
    let core = async {
        let mut buf = vec![0u8; CHUNK];
        let mut done: u64 = resume_from;
        let mut last_report: u64 = resume_from;
        on_progress(done, total);
        loop {
            if cancel.is_set() {
                return Err(copy_text("beTransfer.run.cancelled", &[]).into());
            }
            let n = rf.read(&mut buf).await.map_err(|e| {
                Said::with_raw(
                    copy_text(
                        "beTransfer.download.readRemoteFailed",
                        &[("why", &sftp::why_of_io(&e))],
                    ),
                    &e,
                )
            })?;
            if n == 0 {
                break;
            }
            lf.write_all(&buf[..n]).await.map_err(|e| {
                Said::with_raw(
                    copy_text(
                        "beTransfer.download.writeLocalFailed",
                        &[("why", &io_reason(e.kind()))],
                    ),
                    &e,
                )
            })?;
            done += n as u64;
            if done - last_report >= PROGRESS_EVERY {
                last_report = done;
                on_progress(done, total);
            }
        }
        lf.flush().await.map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beTransfer.download.writeLocalFailed",
                    &[("why", &io_reason(e.kind()))],
                ),
                &e,
            )
        })?;
        Ok(done)
    }
    .await;
    drop(lf);
    let done = match core {
        Ok(d) => d,
        Err(e) => {
            // 撤 ⇒ 留着 `.part`（下次续传的本钱）。失败 ⇒ 也留着：弱网上「连接没了」就是失败，删了重下就从 0 起。
            // 不按错误文案分「断线 / 别的失败」。唯一还清的一形：一个字节都没落（空 `.part` 没有续传的本钱）。
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
    /// `home`：窗口问那台后端拿到的 `$HOME`（给了才比）—— SFTP 起始目录与它不一致 ⇒ 一个字节不写、以 `sftp_home_mismatch` 收场。
    Upload {
        local: String,
        key: String,
        home: Option<String>,
    },
    Download {
        remote: String,
        local: PathBuf,
    },
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
        detail: None,
        data,
    }
}

/// 这几条命令就地被拒的那一帧（详情里带命令名）。
fn err(id: &str, cmd: &str, code: &str, message: &str) -> Frame {
    Frame::refused(id, cmd, code, message)
}

/// 下载的本机落点：字符串或 `{"b16": …}`（与文件管理面同一个字节形）。
fn local_path_of(args: &serde_json::Value) -> Result<PathBuf, String> {
    args.get("local_path")
        .and_then(crate::files::raw::from_json)
        .filter(|b| !b.is_empty())
        .map(|b| crate::files::raw::to_path_buf(&b))
        .ok_or_else(|| {
            crate::common::contract::malformed("missing `local_path` (a string or {\"b16\": …})")
        })
}

fn text<'a>(args: &'a serde_json::Value, k: &str) -> Result<&'a str, String> {
    args.get(k)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            crate::common::contract::malformed(&format!("missing `{k}` or not a string"))
        })
}

fn dial_of(args: &serde_json::Value) -> Result<Dial, String> {
    let d = args
        .get("dial")
        .ok_or_else(|| crate::common::contract::malformed("missing `dial` (a dial request)"))?;
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

    /// **本面唯一的答口**（`stream/inbound/` 那四条硬臂都经它进来，`files/module_boundary_guard::DOORS` 登记）。
    pub fn answer_wire(&self, cmd: &str, id: &str, args: &serde_json::Value) -> Frame {
        match cmd {
            TRANSFER_UPLOAD => self.upload(id, args),
            TRANSFER_DOWNLOAD => self.download(id, args),
            TRANSFER_START => self.start(id, args),
            TRANSFER_STOP => self.stop(id, args),
            other => err(
                id,
                other,
                "unknown_command",
                &crate::common::contract::malformed(&format!("unknown transfer command `{other}`")),
            ),
        }
    }

    /// 此刻在册的票数（生产日志读它）。
    pub(crate) fn len(&self) -> usize {
        lock(&self.tickets).len()
    }

    fn register(&self, id: &str, cmd: &str, t: Ticket) -> Result<String, Frame> {
        let mut g = lock(&self.tickets);
        if g.len() >= MAX_TICKETS_PER_CONNECTION {
            return Err(err(
                id,
                cmd,
                "too_many_transfers",
                &copy_text(
                    "beTransfer.register.tooMany",
                    &[("max", &MAX_TICKETS_PER_CONNECTION.to_string())],
                ),
            ));
        }
        if let Some(k) = &t.key {
            if g.values().any(|o| o.key.as_ref() == Some(k)) {
                return Err(err(
                    id,
                    cmd,
                    "busy",
                    &copy_text("beTransfer.register.sameFile", &[]),
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
            Err(m) => return err(id, TRANSFER_UPLOAD, "bad_args", &m),
        };
        let meta = match std::fs::metadata(local) {
            Ok(m) => m,
            Err(e) => {
                // 开单那一下就读不到本机那份：应答带复制详情（系统原话不上句子）。
                let said = copy_text(
                    "beTransfer.local.readFailed",
                    &[("path", local), ("why", &io_reason(e.kind()))],
                );
                return Frame::err_raw(id, TRANSFER_UPLOAD, "io_failed", &said, &e.to_string());
            }
        };
        if !meta.is_file() {
            return err(
                id,
                TRANSFER_UPLOAD,
                "bad_args",
                &copy_text("beTransfer.local.notRegular", &[("path", local)]),
            );
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
                home: args
                    .get("home")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
            }),
            key: Some(key.clone()),
            cancel: Arc::new(Cancel::default()),
        };
        match self.register(id, TRANSFER_UPLOAD, t) {
            Ok(tid) => ok(id, Some(serde_json::json!({ "id": tid, "key": key }))),
            Err(f) => f,
        }
    }

    /// `transfer-download`：开单；本机落点当场过路径解析。
    fn download(&self, id: &str, args: &serde_json::Value) -> Frame {
        // 本机落点收字符串或 `{"b16": …}`（有损名下载在 Linux 上按原始字节落名）。
        let parsed = dial_of(args).and_then(|d| {
            Ok((
                d,
                text(args, "remote_path")?.to_string(),
                local_path_of(args)?,
            ))
        });
        let (dial, remote, local) = match parsed {
            Ok(v) => v,
            Err(m) => return err(id, TRANSFER_DOWNLOAD, "bad_args", &m),
        };
        if let Err(e) = land_check(&local) {
            return err(id, TRANSFER_DOWNLOAD, "refused", &e);
        }
        let t = Ticket {
            dial,
            job: Some(Job::Download { remote, local }),
            key: None,
            cancel: Arc::new(Cancel::default()),
        };
        match self.register(id, TRANSFER_DOWNLOAD, t) {
            Ok(tid) => ok(id, Some(serde_json::json!({ "id": tid }))),
            Err(f) => f,
        }
    }

    /// `transfer-stop`：撤。没起跑的票当场摘掉；起跑了的由它自己收场（撤删 / 撤留）后摘掉。
    fn stop(&self, id: &str, args: &serde_json::Value) -> Frame {
        let tid = match text(args, "id") {
            Ok(t) => t.to_string(),
            Err(m) => return err(id, TRANSFER_STOP, "bad_args", &m),
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
            Err(m) => return err(id, TRANSFER_START, "bad_args", &m),
        };
        let (dial, job, cancel) = {
            let mut g = lock(&self.tickets);
            let Some(t) = g.get_mut(&tid) else {
                return err(
                    id,
                    TRANSFER_START,
                    "no_such_transfer",
                    &copy_text("beTransfer.start.unknown", &[("tid", &tid.to_string())]),
                );
            };
            let Some(job) = t.job.take() else {
                return err(
                    id,
                    TRANSFER_START,
                    "already_started",
                    &copy_text("beTransfer.start.already", &[("tid", &tid.to_string())]),
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
                Ok((bytes, sha256)) => TransferEnd::Done { bytes, sha256 },
                Err(_) if cancel.is_set() => TransferEnd::Cancelled,
                Err((stop, code)) => TransferEnd::failed(
                    stop.said,
                    code.map(str::to_string),
                    TRANSFER_START,
                    stop.raw.as_deref(),
                ),
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
/// 回传完的字节数；上传那一路另带整份的摘要（提交时的对拍依据），下载那一路 `None`。
async fn run(
    dial: &Dial,
    job: Job,
    cancel: &Cancel,
    sink: &Sink<'_>,
) -> Result<(u64, Option<String>), (Said, Option<&'static str>)> {
    let session = tokio::select! {
        s = sftp::open_for_transfer(dial) => s.map_err(|e| (e, None))?,
        _ = cancel.wait() => return Err((Said::from(copy_text("beTransfer.run.cancelled", &[])), None)),
    };
    match job {
        Job::Upload { local, key, home } => {
            // 连上时比：SFTP 起始目录（`realpath(".")`）≠ 那台后端的 `$HOME` ⇒ 暂存件会落到后端看不见的地方
            //   （chroot / `internal-sftp -d`），一个字节不写，交窗口改走后端链路分块写。
            if let Some(why) = home
                .as_deref()
                .and_then(|h| start_dir_mismatch(session.home(), h))
            {
                return Err((Said::from(why), Some(SFTP_HOME_MISMATCH)));
            }
            upload_to_staging(&session, &local, &key, cancel, sink)
                .await
                .map(|(n, sha)| (n, Some(sha)))
                .map_err(|e| (e, None))
        }
        Job::Download { remote, local } => {
            download_to_local(&session, &remote, &local, cancel, sink)
                .await
                .map(|n| (n, None))
                .map_err(|e| (e, None))
        }
    }
}

/// 上传收场码：SFTP 起始目录不是那台后端的 home。
pub const SFTP_HOME_MISMATCH: &str = "sftp_home_mismatch";

/// SFTP 起始目录与后端 `$HOME` 不一致 ⇒ 那一句话；一致（去掉尾 `/` 逐字节相等）⇒ `None`。
/// ⚠ 认下的：home 经符号链接指过去（`/home` → `/usr/home`）时两串不等 ⇒ 判成不一致、走慢路（仍做得成，只是慢）。
pub fn start_dir_mismatch(sftp_start: &str, backend_home: &str) -> Option<String> {
    let norm = |s: &str| {
        let t = s.trim_end_matches('/');
        if t.is_empty() {
            "/".to_string()
        } else {
            t.to_string()
        }
    };
    (norm(sftp_start) != norm(backend_home)).then(|| {
        copy_core::copy_text(
            "beTransfer.home.mismatch",
            &[("sftp", sftp_start), ("home", backend_home)],
        )
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/control/transfer_tests.rs"]
mod tests;
