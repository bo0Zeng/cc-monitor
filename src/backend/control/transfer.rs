//! 传输台住本机常驻后端：SFTP 只写暂存区，窗口那一侧的 `call` / `subscribe` 不变。
//!
//! # 线上四条命令（帧面硬臂：要碰本连接的票表与应答通道，同 `link-*`）
//!
//! 开单两条是 typed（入参 · 回包 · 码从 `cmd_wire::transfer` 与下面的错误枚举来，登记 `typed::frame`，硬臂经 `typed::answer` 调本体）；
//! 起跑 · 撤两条仍是 `Run::Builtin`。
//!
//! | 命令 | `args` | 应答 | |
//! |---|---|---|---|
//! | `transfer-upload` | `TransferUploadReq {dial, local_path, home?}` | `TransferUploaded {id, key}` | 开单（不起跑）；`key` = 暂存件的键，提交时交给远端后端 `files-commit-upload` |
//! | `transfer-download` | `TransferDownloadReq {dial, remote_path, local_path}` | `TransferOpened {id}` | 开单；本机落点当场过路径解析（出声早），起跑时再过一次 |
//! | `transfer-start` | `{id}` | — | 起跑；进度走出方向 `transfer` 帧（`wire.rs`） |
//! | `transfer-stop` | `{id}` | — | 撤（幂等）。上传删暂存件；下载留 `.part` |
//!
//! 起跑挂在 `transfer-start` 上、不挂在开单上：monitor 先登记好看的人、再起跑，终局就不会没人收。
//!
//! # 归原生那一块（两台之间搬字节是拨号那一半的活）
//!
//! 它引 `dial::sftp` 拿会话、搬字节；文件管理那一块（`files/` · `control/files_*.rs`）只管本机的读和写，它一处都不引
//! （`files/module_boundary_guard.rs` 两个方向都钉）。两块共用的路径解析 · 暂存件命名 · 流式摘要 · 路径字节还原都住 `common/`。
//! 下载要写本机用户选的落点（`.part` ＋ 改名上位）：写经票表手里那一份 `Face`（帧面分派那一层造台时交来），
//! 用到的只有 `O_EXCL` 新建 · 接着写 · 改名 · 删文件；每一处改动的路径都是 `common::root_path::walk_in_root` 交回的 `Resolved`。
//! 远端那一半（暂存区的写）全经 `ssh-dial/sftp.rs` 的写原语（只许两处、先过 `fenced_remote`）。
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

use crate::wire::{FrameMake as _, TransferEndExt as _};
use copy_core::{copy_text, io_reason};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{mpsc, watch, Notify};

use crate::common::root_path::{walk_in_root, Last};
use crate::common::said::Said;
use crate::common::staging::{KEY_LEN, PART_SUFFIX};
use crate::dial::sftp::{self, Dial, Session};
use crate::platform::fs::{Face, FaceFile, Ro};
use crate::wire::typed::{self, codes};
use crate::wire::{Frame, TransferEnd};
use cmd_wire::batch::{Conflict, Conflicts, Decide};
use cmd_wire::files as cwf;
use cmd_wire::transfer::{
    OpenState, OpenedItem, TransferDownloadReq, TransferOpened, TransferUploadReq,
};

/// 开单：上传。
pub const TRANSFER_UPLOAD: &str = "transfer-upload";
/// 开单：下载。
pub const TRANSFER_DOWNLOAD: &str = "transfer-download";
/// 起跑。
pub const TRANSFER_START: &str = <TransferStart as typed::Command>::NAME;
/// 撤。
pub const TRANSFER_STOP: &str = <TransferStop as typed::Command>::NAME;
/// 开单（上传）那一条（typed，帧面硬臂那一档：要碰本连接的票表）。
pub(crate) struct TransferUpload;

codes! {
    /// `transfer-upload` 会回的码（一项没开成不是整组没开成：读不到本机那一份 `io_failed` · 同一份已有一张票 `busy` ·
    /// 票满了 `too_many_transfers` 都在那一项上）。
    pub(crate) enum UploadErr {
        /// 远端落点有同名的、还没答怎么办（一单都没开）：`data` 是同名清单。
        Exists = "exists" => Conflicts,
    }
}

impl typed::Command for TransferUpload {
    const NAME: &'static str = "transfer-upload";
    type Req = TransferUploadReq;
    type Reply = TransferOpened;
    type Err = UploadErr;
}

/// 起跑 · 撤那两条的入参：那张票。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct TransferIdReq {
    /// 传输单 id（开单那一条回的）。
    pub(crate) id: String,
}

codes! {
    /// `transfer-start` 会回的码。
    pub(crate) enum StartErr {
        /// 这条连接上没有这张票。
        NoSuchTransfer = "no_such_transfer",
        /// 这张票已经起跑过。
        AlreadyStarted = "already_started",
    }
}

codes! {
    /// `transfer-stop` 没有别的码（撤不在册的也回 `ok`）。
    pub(crate) enum StopErr {}
}

/// 起跑那一条（帧面硬臂：之后进度与终局走 `transfer` 帧）。
pub(crate) struct TransferStart;

impl typed::Command for TransferStart {
    const NAME: &'static str = "transfer-start";
    type Req = TransferIdReq;
    type Reply = ();
    type Err = StartErr;
}

/// 撤那一条（帧面硬臂，幂等）。
pub(crate) struct TransferStop;

impl typed::Command for TransferStop {
    const NAME: &'static str = "transfer-stop";
    type Req = TransferIdReq;
    type Reply = ();
    type Err = StopErr;
}

/// 开单（下载）那一条（同 [`TransferUpload`]）。
pub(crate) struct TransferDownload;

codes! {
    /// `transfer-download` 会回的码。
    pub(crate) enum DownloadErr {
        /// 本机落点目录过不了路径解析。
        Refused = "refused",
        /// 本机落点有同名的、还没答怎么办（一单都没开）：`data` 是同名清单。
        Exists = "exists" => Conflicts,
    }
}

impl typed::Command for TransferDownload {
    const NAME: &'static str = "transfer-download";
    type Req = TransferDownloadReq;
    type Reply = TransferOpened;
    type Err = DownloadErr;
}

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

// ═══ 本机落点（每一处改动先过路径解析）══════════════════════════════════════════════

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
    walk_in_root(Last::Link, &root, &name)?;
    walk_in_root(Last::Link, &root, &part)?;
    Ok(())
}

/// 从 0 开一份半成品：旧的在就先删（它的尾块已经对不上了），再 `O_EXCL` 新建。
fn land_open_fresh(face: &Face, root: &Path, part: &OsStr) -> Result<FaceFile, Said> {
    let at = walk_in_root(Last::Link, root, part)?;
    if Ro::symlink_metadata(&at).is_ok() {
        face.delete_file(&at).map_err(|e| {
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
    face.create_new(&at).map_err(|e| {
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

/// 续传：**不原地接着写**（`Face` 没有「续写」那种开法：每一个写句柄都是 `O_EXCL` 新建）⇒
/// 旧半成品改名成 `<名>.old` → `O_EXCL` 新建 `<名>` → 把前 `keep` 字节从旧的抄过来 → 删旧的。
/// 回来的句柄游标停在 `keep`。代价如实记：前缀在本机盘上多抄一遍（本机盘速，不走网）。
fn land_carry_over(face: &Face, root: &Path, part: &OsStr, keep: u64) -> Result<FaceFile, Said> {
    let at = walk_in_root(Last::Link, root, part)?;
    let mut old_name = part.to_os_string();
    old_name.push(".old");
    let old = walk_in_root(Last::Link, root, &old_name)?;
    if Ro::symlink_metadata(&old).is_ok() {
        face.delete_file(&old).map_err(|e| {
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
    face.move_file(&at, &old).map_err(|e| {
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
    let mut fresh = face.create_new(&at).map_err(|e| {
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
        let src = Ro::open_nofollow(&old).map_err(|e| {
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
    face.delete_file(&old).map_err(|e| {
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
fn land_commit(face: &Face, root: &Path, part: &OsStr, name: &OsStr) -> Result<(), Said> {
    let from = walk_in_root(Last::Link, root, part)?;
    let to = walk_in_root(Last::Link, root, name)?;
    face.move_file(&from, &to).map_err(|e| {
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
fn land_discard(face: &Face, root: &Path, part: &OsStr) {
    if let Ok(at) = walk_in_root(Last::Link, root, part) {
        let _ = face.delete_file(&at);
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
    let total = Ro::metadata_async(local_path)
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
    let mut lf = Ro::open_async(local_path).await.map_err(|e| {
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
    let mut digest = crate::common::digest::ContentDigest::new();
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

/// 下载：远端只读；本机落点 `<落点>.part` ＋ 改名上位（每一处改动先过路径解析）。回落地的字节数。
pub(crate) async fn download_to_local(
    face: &Face,
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
    let have = Ro::metadata_async(&part_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    // `rf_at` = 远端句柄此刻停在哪（知道才填）。尾块对上了 ⇒ 探针那一读恰好读满到 `have`，
    //   句柄就停在 `have` == `resume_from`。
    let (resume_from, rf_at) = if have > 0 {
        match Ro::open_nofollow_async(&part_path).await {
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
        land_carry_over(face, &root, &part, resume_from)?
    } else {
        land_open_fresh(face, &root, &part)?
    };
    // 两种开法回来的游标都停在 `resume_from`（新建 ⇒ 0；抄完前缀 ⇒ 前缀末尾）。
    let mut lf = std_file.into_async();
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
            let landed = Ro::metadata_async(root.join(&part))
                .await
                .map_or(0, |m| m.len());
            if !cancel.is_set() && landed == 0 {
                land_discard(face, &root, &part);
            }
            return Err(e);
        }
    };
    land_commit(face, &root, &part, &name)?;
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
    /// 下载落点那一下写用的文件管理写句柄（帧面分派那一层造好交来的；起跑时带一份进那趟任务）。
    face: Face,
}

impl Drop for Desk {
    /// 连接没了（monitor 走了）⇒ 它开的传输一律撤（= 旧形「连接断了 ⇒ 撤」）。
    fn drop(&mut self) {
        for (_, t) in lock(&self.tickets).drain() {
            t.cancel.fire();
        }
    }
}

/// 这几条命令就地被拒的那一帧（详情里带命令名）。
fn err(id: &str, cmd: &str, code: &str, message: &str) -> Frame {
    Frame::refused(id, cmd, code, message)
}

/// 票号：本进程里单调（票只在一条连接里有意义；`xfer-` 前缀与旧传输台同形）。
fn next_id() -> String {
    static N: AtomicU64 = AtomicU64::new(1);
    format!("xfer-{}", N.fetch_add(1, Ordering::SeqCst))
}

impl Desk {
    pub fn new(replies: mpsc::Sender<Frame>, face: Face) -> Self {
        Desk {
            tickets: Arc::new(Mutex::new(HashMap::new())),
            replies,
            face,
        }
    }

    /// **本面的答口**（`stream/inbound/` 那几条硬臂都经它进来）。上传开单要先去远端落点探同名（要等那台）⇒ 走 [`Desk::upload_wire`]。
    pub fn answer_wire(
        &self,
        cmd: &str,
        id: &str,
        args: &serde_json::Value,
        tz: &crate::Tz,
    ) -> Frame {
        match cmd {
            TRANSFER_DOWNLOAD => {
                typed::answer::<TransferDownload>(id, args, |r| self.download(r, tz))
            }
            TRANSFER_START => typed::answer::<TransferStart>(id, args, |r| self.start(r)),
            TRANSFER_STOP => typed::answer::<TransferStop>(id, args, |r| {
                self.stop(r);
                Ok(())
            }),
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

    /// `transfer-download`：一组落到本机一个目录。同名在本机落点那一侧先探：没答的 ⇒ 一单不开、回 `exists` 带同名清单；
    /// 保留两份 ⇒ 另起 `名 (2).ext`（撞上往后试）；落点当场过路径解析，起跑时再过一次。同名清单里本机那一格的时刻按 `tz` 写好。
    fn download(
        &self,
        req: TransferDownloadReq,
        tz: &crate::Tz,
    ) -> Result<TransferOpened, DownloadErr> {
        let dial = Dial::parse(&req.dial).map_err(DownloadErr::BadArgs)?;
        if req.local_dir.bytes().is_empty() {
            return Err(DownloadErr::BadArgs(crate::common::contract::malformed(
                "missing `local_dir` (a string or {\"b16\": …})",
            )));
        }
        let dir = crate::common::path_wire::to_path_buf(req.local_dir.bytes());
        let on_exists = req.on_exists.unwrap_or_default();
        // 每一项的落名：给了照给的，没给照远端那一段名字。
        let names: Vec<OsString> = req
            .items
            .iter()
            .map(|it| match &it.name {
                Some(n) => crate::common::path_wire::to_path_buf(n.bytes()).into_os_string(),
                None => OsString::from(it.remote_path.rsplit('/').next().unwrap_or_default()),
            })
            .collect();
        let mut ask = Vec::new();
        let mut choices: Vec<Option<Option<Decide>>> = Vec::with_capacity(names.len());
        for (i, name) in names.iter().enumerate() {
            let key = i.to_string();
            match Ro::symlink_metadata(dir.join(name)) {
                Err(_) => choices.push(None),
                Ok(m) => {
                    let choice =
                        crate::common::batch_book::choice_of(&key, on_exists, req.decide.as_ref());
                    if choice.is_none() {
                        let there = if m.is_dir() {
                            cwf::FileKind::Dir
                        } else {
                            cwf::FileKind::File
                        };
                        ask.push(Conflict {
                            key,
                            name: cwf::WirePath(
                                Path::new(name).as_os_str().as_encoded_bytes().to_vec(),
                            ),
                            kind: cwf::FileKind::File,
                            size: None,
                            mtime_text: None,
                            there,
                            there_size: m.is_file().then(|| m.len()),
                            there_mtime_text: crate::common::batch_book::moment_text(
                                crate::common::batch_book::mtime_secs(m.modified()),
                                tz,
                            ),
                            answers: crate::common::batch_book::answers_of(m.is_dir(), true),
                        });
                    }
                    choices.push(Some(choice));
                }
            }
        }
        if !ask.is_empty() {
            let n = ask.len().to_string();
            return Err(DownloadErr::Exists(
                copy_text("beFiles.batch.clash", &[("n", &n)]),
                Conflicts { conflicts: ask },
            ));
        }
        let items = req
            .items
            .into_iter()
            .zip(names)
            .zip(choices)
            .enumerate()
            .map(|(i, ((it, name), choice))| {
                let key = i.to_string();
                let landed = match choice {
                    None | Some(Some(Decide::Overwrite)) => Some((dir.join(&name), None)),
                    Some(None) | Some(Some(Decide::Skip)) => None,
                    Some(Some(Decide::KeepBoth)) => mint_local(&dir, &name)
                        .map(|p| (p.clone(), p.file_name().map(OsStr::to_os_string))),
                };
                let Some((local, new_name)) = landed else {
                    return opened(key, OpenState::Skipped);
                };
                if let Err(m) = land_check(&local) {
                    return not_opened(key, "refused", m, None);
                }
                let t = Ticket {
                    dial: dial.clone(),
                    job: Some(Job::Download {
                        remote: it.remote_path,
                        local: local.clone(),
                    }),
                    key: None,
                    cancel: Arc::new(Cancel::default()),
                };
                match register(&self.tickets, t) {
                    Ok(id) => OpenedItem {
                        id: Some(id),
                        path: Some(cwf::WirePath(local.as_os_str().as_encoded_bytes().to_vec())),
                        new_name: new_name.map(|n| {
                            cwf::WirePath(Path::new(&n).as_os_str().as_encoded_bytes().to_vec())
                        }),
                        ..opened(key, OpenState::Opened)
                    },
                    Err((c, m)) => not_opened(key, c, m, None),
                }
            })
            .collect();
        Ok(TransferOpened { items })
    }

    /// `transfer-stop`：撤。没起跑的票当场摘掉；起跑了的由它自己收场（撤删 / 撤留）后摘掉。
    fn stop(&self, r: TransferIdReq) {
        let tid = r.id;
        let mut g = lock(&self.tickets);
        if let Some(t) = g.get(&tid) {
            t.cancel.fire();
            if t.job.is_some() {
                g.remove(&tid);
            }
        }
    }

    /// `transfer-start`：起跑。两个任务：传输本体 · 进度转发（`watch` 合并）。
    fn start(&self, r: TransferIdReq) -> Result<(), StartErr> {
        let tid = r.id;
        let (dial, job, cancel) = {
            let mut g = lock(&self.tickets);
            let Some(t) = g.get_mut(&tid) else {
                return Err(StartErr::NoSuchTransfer(copy_text(
                    "beTransfer.start.unknown",
                    &[("tid", &tid.to_string())],
                )));
            };
            let Some(job) = t.job.take() else {
                return Err(StartErr::AlreadyStarted(copy_text(
                    "beTransfer.start.already",
                    &[("tid", &tid.to_string())],
                )));
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
        let face = self.face.clone();
        tokio::spawn(async move {
            let tx = Arc::new(tx);
            let sink_tx = Arc::clone(&tx);
            let sink = move |got: u64, total: u64| {
                sink_tx.send_modify(|p| {
                    p.got = got;
                    p.total = total;
                });
            };
            let r = run(&face, &dial, job, &cancel, &sink).await;
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
        Ok(())
    }
}

/// 登记一张票。满册 ⇒ `too_many_transfers`；同一个暂存件键已有一张 ⇒ `busy`（下层那一形：码 ＋ 那一句，各条照自己的码表收）。
fn register(tickets: &Tickets, t: Ticket) -> Result<String, (&'static str, String)> {
    let mut g = lock(tickets);
    if g.len() >= MAX_TICKETS_PER_CONNECTION {
        return Err((
            "too_many_transfers",
            copy_text(
                "beTransfer.register.tooMany",
                &[("max", &MAX_TICKETS_PER_CONNECTION.to_string())],
            ),
        ));
    }
    if let Some(k) = &t.key {
        if g.values().any(|o| o.key.as_ref() == Some(k)) {
            return Err(("busy", copy_text("beTransfer.register.sameFile", &[])));
        }
    }
    let tid = next_id();
    g.insert(tid.clone(), t);
    Ok(tid)
}

// ═══ 上传开单：先在远端落点探整组 ═══════════════════════════════════════════════════════

/// 远端落点那台上一格的样子：是什么 · 多大（文件）· 修改时刻（unix 秒）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Seen {
    pub(crate) kind: cwf::FileKind,
    pub(crate) size: Option<u64>,
    pub(crate) mtime: Option<i64>,
}

/// 上传的落点那台：一格在不在（生产那一份问那台后端的 `files-stat`，判据用本机目录上的替身）。
/// 问不出来（那台没答 · 不在）一律当不在：提交时不盖（`overwrite: false`），探完之后才冒出来的同名照旧被拒，不静默盖掉。
pub(crate) trait Landing: Send + Sync {
    fn look<'a>(&'a self, path: &'a [u8]) -> super::transfer_copy::Fut<'a, Option<Seen>>;
}

/// 生产那一份：经那台的拨号请求问它后端的 `files-stat`（路径按字节，目录名不是 UTF-8 也问得到）。
struct Dialed {
    dial: serde_json::Value,
    tz: crate::Tz,
}

impl Landing for Dialed {
    fn look<'a>(&'a self, path: &'a [u8]) -> super::transfer_copy::Fut<'a, Option<Seen>> {
        Box::pin(async move {
            let args = serde_json::json!({ "path": cwf::WirePath(path.to_vec()) });
            let v = crate::dial::remote_ask::ask_dialed(
                &self.dial,
                "files-stat",
                &args,
                &self.tz,
                &crate::dial::remote_ask::DialRemote,
            )
            .await
            .ok()?;
            let r: cwf::StatReply = serde_json::from_value(v).ok()?;
            Some(Seen {
                size: (r.kind == cwf::FileKind::File).then_some(r.size),
                mtime: r.mtime_secs.and_then(|s| i64::try_from(s).ok()),
                kind: r.kind,
            })
        })
    }
}

/// 上传一项探过、答过之后：照这个名字开单（`overwrite` ＝ 提交时盖不盖）· 跳过 · 没开成。
enum Fate {
    Open {
        name: Vec<u8>,
        minted: bool,
        overwrite: bool,
    },
    Skip,
    Miss(&'static str, String),
}

fn under(dir: &[u8], name: &[u8]) -> Vec<u8> {
    let mut p = dir.to_vec();
    if !p.ends_with(b"/") {
        p.push(b'/');
    }
    p.extend_from_slice(name);
    p
}

/// 上传一组的探与答：每一项落在远端目录里同名那一格；那儿已有、而没答 ⇒ 一单不开，回 `exists` 带同名清单（两头的时刻按 `tz` 写好）；
/// 答了 ⇒ 跳过 · 覆盖（只对文件：目录不合并 ⇒ 跳过）· 保留两份（远端那一侧另起 `名 (2).ext`，撞上往后试）。
async fn upload_fates(
    land: &dyn Landing,
    req: &TransferUploadReq,
    tz: &crate::Tz,
) -> Result<Vec<Fate>, UploadErr> {
    use crate::common::batch_book::{choice_of, key_of, moment_text, mtime_secs};
    let on_exists = req.on_exists.unwrap_or_default();
    let dir = req.remote_dir.bytes();
    let mut ask = Vec::new();
    let mut probed = Vec::with_capacity(req.items.len());
    for (i, it) in req.items.iter().enumerate() {
        let key = key_of(i);
        let Some(name) = Path::new(&it.local_path)
            .file_name()
            .map(|n| n.as_encoded_bytes().to_vec())
        else {
            probed.push(Err(Fate::Miss(
                "refused",
                copy_text("beTransfer.local.notRegular", &[("path", &it.local_path)]),
            )));
            continue;
        };
        let Some(there) = land.look(&under(dir, &name)).await else {
            probed.push(Ok((name, None)));
            continue;
        };
        let choice = choice_of(&key, on_exists, req.decide.as_ref());
        let dir_into = there.kind == cwf::FileKind::Dir;
        if choice.is_none() {
            let local = Ro::metadata(&it.local_path).ok();
            ask.push(Conflict {
                key,
                name: cwf::WirePath(name.clone()),
                kind: cwf::FileKind::File,
                size: local.as_ref().map(|m| m.len()),
                mtime_text: moment_text(local.as_ref().and_then(|m| mtime_secs(m.modified())), tz),
                there: there.kind,
                there_size: there.size,
                there_mtime_text: moment_text(there.mtime, tz),
                answers: crate::common::batch_book::answers_of(dir_into, true),
            });
        }
        probed.push(Ok((name, Some((choice, dir_into)))));
    }
    if !ask.is_empty() {
        let n = ask.len().to_string();
        return Err(UploadErr::Exists(
            copy_text("beFiles.batch.clash", &[("n", &n)]),
            Conflicts { conflicts: ask },
        ));
    }
    let mut out = Vec::with_capacity(probed.len());
    for p in probed {
        out.push(match p {
            Err(f) => f,
            Ok((name, None)) => Fate::Open {
                name,
                minted: false,
                overwrite: false,
            },
            Ok((_, Some((Some(Decide::Skip) | None, _))))
            | Ok((_, Some((Some(Decide::Overwrite), true)))) => Fate::Skip,
            Ok((name, Some((Some(Decide::Overwrite), false)))) => Fate::Open {
                name,
                minted: false,
                overwrite: true,
            },
            Ok((name, Some((Some(Decide::KeepBoth), _)))) => {
                let shown = std::ffi::OsString::from(String::from_utf8_lossy(&name).into_owned());
                let mut free = None;
                for k in 2..=999u32 {
                    let n = crate::common::batch_book::minted(Path::new(&shown), k)
                        .into_os_string()
                        .into_encoded_bytes();
                    if land.look(&under(dir, &n)).await.is_none() {
                        free = Some(n);
                        break;
                    }
                }
                match free {
                    Some(name) => Fate::Open {
                        name,
                        minted: true,
                        overwrite: false,
                    },
                    None => Fate::Miss("exists", copy_text("beFiles.batch.mintSpent", &[])),
                }
            }
        });
    }
    Ok(out)
}

/// 上传那一项开单：读不到本机那一份 ⇒ `io_failed`（原话另带）；不是普通文件 ⇒ `refused`。
fn open_upload(
    tickets: &Tickets,
    dial: &Dial,
    local: &str,
    home: Option<String>,
) -> Result<(String, String), (&'static str, String, Option<String>)> {
    let meta = Ro::metadata(local).map_err(|e| {
        let said = copy_text(
            "beTransfer.local.readFailed",
            &[("path", local), ("why", &io_reason(e.kind()))],
        );
        ("io_failed", said, Some(e.to_string()))
    })?;
    if !meta.is_file() {
        return Err((
            "refused",
            copy_text("beTransfer.local.notRegular", &[("path", local)]),
            None,
        ));
    }
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let key = staging_key(local, meta.len(), mtime_ns);
    let t = Ticket {
        dial: dial.clone(),
        job: Some(Job::Upload {
            local: local.to_string(),
            key: key.clone(),
            home,
        }),
        key: Some(key.clone()),
        cancel: Arc::new(Cancel::default()),
    };
    let id = register(tickets, t).map_err(|(c, m)| (c, m, None))?;
    Ok((id, key))
}

/// `transfer-upload` 的本体：远端落点探整组（[`upload_fates`]）→ 照答逐项开单（不起跑）。一项没开成那一项回 `failed`（码 ＋ 原因词 ＋ 原话），别的照开。
pub(crate) async fn upload_with(
    tickets: &Tickets,
    land: &dyn Landing,
    req: TransferUploadReq,
    tz: &crate::Tz,
) -> Result<TransferOpened, UploadErr> {
    let dial = Dial::parse(&req.dial).map_err(UploadErr::BadArgs)?;
    if req.remote_dir.bytes().is_empty() {
        return Err(UploadErr::BadArgs(crate::common::contract::malformed(
            "missing `remote_dir` (a string or {\"b16\": …})",
        )));
    }
    let fates = upload_fates(land, &req, tz).await?;
    let items = req
        .items
        .iter()
        .zip(fates)
        .enumerate()
        .map(|(i, (it, fate))| {
            let key = i.to_string();
            match fate {
                Fate::Skip => opened(key, OpenState::Skipped),
                Fate::Miss(code, said) => not_opened(key, code, said, None),
                Fate::Open {
                    name,
                    minted,
                    overwrite,
                } => match open_upload(tickets, &dial, &it.local_path, req.home.clone()) {
                    Ok((id, staged)) => OpenedItem {
                        id: Some(id),
                        staged: Some(staged),
                        new_name: minted.then(|| cwf::WirePath(name)),
                        overwrite,
                        ..opened(key, OpenState::Opened)
                    },
                    Err((code, said, raw)) => not_opened(key, code, said, raw),
                },
            }
        })
        .collect();
    Ok(TransferOpened { items })
}

impl Desk {
    /// `transfer-upload` 那一发（帧面硬臂；探同名要等远端那台 ⇒ 交出一个异步处理器）：入参按类型收 → 探 → 照答开单 → 回包。
    pub(crate) fn upload_wire_for(&self) -> crate::wire::spec::DataHandler {
        let tickets = Arc::clone(&self.tickets);
        typed::answer_async::<TransferUpload, _, _>(move |req, cx| async move {
            let land = Dialed {
                dial: req.dial.clone(),
                tz: cx.tz.clone(),
            };
            upload_with(&tickets, &land, req, &cx.tz).await
        })
    }
}

/// 开单的一项：只有键与那一样（别的格缺席）。
fn opened(key: String, state: OpenState) -> OpenedItem {
    OpenedItem {
        key,
        state,
        id: None,
        staged: None,
        path: None,
        new_name: None,
        overwrite: false,
        code: None,
        reason: None,
        detail: None,
    }
}

/// 没开成的那一项：码 ＋ 原因词 ＋ 那一句与原话进复制详情。
fn not_opened(key: String, code: &str, said: String, raw: Option<String>) -> OpenedItem {
    OpenedItem {
        code: Some(code.to_string()),
        reason: Some(copy_core::code_reason(code, raw.as_deref())),
        detail: Some(match raw {
            Some(r) => format!("{said}\n{r}"),
            None => said,
        }),
        ..opened(key, OpenState::Failed)
    }
}

/// 保留两份：本机落点目录里第一个没被占的 `名 (k).ext`（k 从 2 起）。
fn mint_local(dir: &Path, name: &OsStr) -> Option<PathBuf> {
    (2..=crate::common::batch_book::MINT_TRIES)
        .map(|k| dir.join(crate::common::batch_book::minted(Path::new(name), k)))
        .find(|p| Ro::symlink_metadata(p).is_err())
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
        let frame = Frame::Transfer(crate::wire::Transfer {
            id: id.clone(),
            got: p.got,
            total: p.total,
            end: p.end,
        });
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
    face: &Face,
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
            download_to_local(face, &session, &remote, &local, cancel, sink)
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
