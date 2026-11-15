//! 两台之间搬字节的 SFTP 那条腿（`transfer_copy.rs` 照它上传进那台暂存区 · 下载落到本机）：SFTP 只写暂存区。
//!
//! # 归原生那一块（两台之间搬字节是拨号那一半的活）
//!
//! 它引 `dial::sftp` 拿会话、搬字节；文件管理那一块（`files/`，独立的包）只管本机的读和写，它不引这里
//! （引不到上层由 Cargo 管）。两块共用的路径解析 · 暂存区与落进用户目录那一下（`common::staging`）· 流式摘要 · 路径字节还原都住 `common/`。
//! 下载先写进本机暂存区（`~/.cc-monitor/staging/<键>.part`），传完经 `common::staging::land` 落到用户选的落点（与上传提交同一处）：写经调用方交来的那一份 `Face`，
//! 暂存区里用到的只有 `O_EXCL` 新建 · 接着写 · 改名 · 删文件；每一处改动的路径都是 `common::root_path::walk_in_root` 交回的 `Resolved`。
//! 远端那一半（暂存区的写）全经 `ssh-dial/sftp.rs` 的写原语（只许两处、先过 `fenced_remote`）。
//!
//! # 存亡规矩
//!
//! | 事件 | 上传的暂存件 | 下载的半成品（本机暂存区） |
//! |---|---|---|
//! | 撤（撤的旗） | 删（用户说了不要） | 留（续传的本钱） |
//! | 失败 | 留（续传最值钱的正是这一档） | 留（同一条理由；一个字节都没落的空半成品才清） |
//! | 成功 | 那台后端那次提交把它变成目标 | `common::staging::land` 上位 |
//!
//! 判的是撤的旗，不是错误文案。续传：两侧都先把尾块逐字节对一遍，对得上才接（半成品只记了「写到哪」，
//! 没记「那是谁的字节」，只看长度会缝出一个坏文件）。
//!
//! # 零定时器
//!
//! 撤 = 一面旗，逐块看。

use copy_core::{copy_text, io_reason, Discard, Why};

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use crate::common::root_path::{walk_in_root, Last};
use crate::common::said::Said;
use crate::common::staging::{self, Content, Was, KEY_LEN, PART_SUFFIX};
use crate::dial::sftp::{self, Dial, Session};
use crate::platform::fs::{Face, FaceFile, Resolved, Ro};

/// SFTP 读写的一块（≤32 KiB：SFTP 草案建议；严格的服务端拒超大包）。
const CHUNK: usize = 32 * 1024;

/// 进度至少隔这么多字节报一次（起止各另报一次）。
const PROGRESS_EVERY: u64 = 256 * 1024;

// ═══ 暂存件的键 ════════════════════════════════════════════════════════════════════════

/// 由（那一份是谁 · 大小 · 修改时间）派生暂存件的键（上传：本机路径 ＋ 大小 ＋ 修改时间；下载：远端路径 ＋ 本机落点，后两格交 0）：同一份重拖一次落到同一个暂存件上 ⇒ 续传的尾块对拍照旧生效。
/// 暂存区、键长、键派生与提交那一侧（`files::commit`）一致，提交那一侧住文件管理那一块。标准库默认散列跨 Rust 版本不保证稳定 ——
/// 只意味着升级之后第一次重拖从 0 来；尾块对拍另有一道兜底，不会接错。
pub(crate) fn staging_key(what: impl std::hash::Hash, size: u64, mtime_ns: u128) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let half = |salt: &str| {
        let mut h = DefaultHasher::new();
        (salt, &what, size, mtime_ns).hash(&mut h);
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
pub(crate) fn staging_part(key: &str) -> String {
    format!("{}/{key}{PART_SUFFIX}", sftp::STAGING_ROOT)
}

// ═══ 本机落点（每一处改动先过路径解析）══════════════════════════════════════════════
//
// 下载的半成品写进本机暂存区（`~/.cc-monitor/staging/<键>.part`，键由「远端路径 ＋ 本机落点」派生 ⇒ 重拖同一份落到同一个半成品上），
// 传完经 `common::staging::land` 上位：用户目录里不出现半成品，落进去的那一下与上传提交同一处。

/// 本机落点拆成 `(父目录 = 路径解析的根, 文件名)`。必须是绝对路径、有文件名。
/// 名字按 `OsString` 拿（Linux 上落点可以是非 UTF-8 的原始字节：有损名下载「字节原样当文件名」）。
fn local_target(local_path: &Path) -> Result<(PathBuf, OsString), Said> {
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
    Ok((root, name))
}

/// 一趟下载的半成品在本机暂存区里的名字（`<键>.part`）。
pub(crate) fn download_part_name(remote_path: &str, local_path: &Path) -> String {
    let key = staging_key(
        (
            "download",
            remote_path,
            local_path.as_os_str().as_encoded_bytes(),
        ),
        0,
        0,
    );
    format!("{key}{PART_SUFFIX}")
}

/// 开一份半成品，回来的句柄游标停在 `keep`。
///
/// - `keep == 0`：旧的在就先删（它的尾块已经对不上了），再 `O_EXCL` 新建。
/// - 续传：**不原地接着写**（`Face` 没有「续写」那种开法：每一个写句柄都是 `O_EXCL` 新建）⇒
///   旧半成品改名成 `<名>.old` → `O_EXCL` 新建 `<名>` → 把前 `keep` 字节从旧的抄过来 → 删旧的。
///   代价如实记：前缀在本机盘上多抄一遍（本机盘速，不走网）。
fn open_part(face: &Face, dir: &Path, part: &str, keep: u64) -> Result<FaceFile, Said> {
    let at = walk_in_root(Last::Link, dir, part)?;
    let drop_it = |p: &Resolved, said: fn(&str, &str) -> String| {
        face.delete_file(p)
            .map_err(|e| Said::with_raw(said(&win_path_core::shown(p), &io_reason(e.kind())), &e))
    };
    let create = || {
        face.create_new(&at).map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beTransfer.land.createFailed",
                    &[
                        ("path", &win_path_core::shown(&at)),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })
    };
    if keep == 0 {
        if Ro::symlink_metadata(&at).is_ok() {
            drop_it(&at, |path, why| {
                copy_text(
                    "beTransfer.land.dropPartFailed",
                    &[("path", path), ("why", why)],
                )
            })?;
        }
        return create();
    }
    let old = walk_in_root(Last::Link, dir, format!("{part}.old"))?;
    if Ro::symlink_metadata(&old).is_ok() {
        drop_it(&old, |path, why| {
            copy_text(
                "beTransfer.land.dropLeftoverFailed",
                &[("path", path), ("why", why)],
            )
        })?;
    }
    face.move_file(&at, &old).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beTransfer.land.moveAsideFailed",
                &[
                    ("path", &win_path_core::shown(&at)),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    let mut fresh = create()?;
    let copied = {
        let src = Ro::open_nofollow(&old).map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beTransfer.land.readPartFailed",
                    &[
                        ("path", &win_path_core::shown(&old)),
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
    drop_it(&old, |path, why| {
        copy_text(
            "beTransfer.land.dropPartFailed",
            &[("path", path), ("why", why)],
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

// ═══ 撤 ════════════════════════════════════════════════════════════════════════════════

/// 一趟传输的「撤了没有」：一面旗（逐块看）。
#[derive(Default)]
pub(crate) struct Cancel {
    flag: AtomicBool,
}

impl Cancel {
    pub(crate) fn fire(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }
    pub(crate) fn is_set(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
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
/// 孤儿不在这里扫（远端后端 `files::commit::sweep_stale`，每次提交成功时顺手扫）。
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
                return Err(copy_text("reason.op.cancelled", &[]).into());
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
    rf.shutdown().await.discard(Why::DeadLink); // 传输的结局已经定了
    drop(rf);
    match core {
        Ok(done) => {
            on_progress(done, total);
            Ok((done, digest.finish()))
        }
        Err(e) => {
            // 撤 ⇒ 删（用户说了不要）；失败 ⇒ 留（续传的本钱，在我们自己的目录里）。
            if cancel.is_set() {
                sftp::remove(s, &part)
                    .await
                    .discard(Why::CleanupAfterFailure); // 删不掉只是暂存区里多一份，孤儿扫会收
            }
            Err(e)
        }
    }
}

/// 下载：远端只读；半成品写进本机暂存区（[`download_part_name`]），传完经 `common::staging::land` 上位（每一处改动先过路径解析）。
/// 回落地的字节数。
pub(crate) async fn download_to_local(
    face: &Face,
    s: &Session,
    remote_path: &str,
    local_path: impl AsRef<Path>,
    cancel: &Cancel,
    on_progress: &Sink<'_>,
) -> Result<u64, Said> {
    let (root, name) = local_target(local_path.as_ref())?;
    let home = staging::home()?;
    let dir = staging::ensure(face, &home)?;
    let part = download_part_name(remote_path, local_path.as_ref());
    let total = sftp::metadata_size(s, remote_path)
        .await
        .flatten()
        .unwrap_or(0);
    let mut rf = sftp::open_for_read(s, remote_path).await?;
    let part_path = dir.join(&part);
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
    let std_file = open_part(face, &dir, &part, resume_from)?;
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
                return Err(copy_text("reason.op.cancelled", &[]).into());
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
            // 撤 ⇒ 留着半成品（下次续传的本钱）。失败 ⇒ 也留着：弱网上「连接没了」就是失败，删了重下就从 0 起。
            // 不按错误文案分「断线 / 别的失败」。唯一还清的一形：一个字节都没落（空半成品没有续传的本钱）。
            let landed = Ro::metadata_async(&part_path).await.map_or(0, |m| m.len());
            if !cancel.is_set() && landed == 0 {
                if let Ok(at) = walk_in_root(Last::Link, &dir, &part) {
                    face.delete_file(&at).discard(Why::CleanupAfterFailure); // 只清空的半成品；清不掉只剩暂存区里一个 0 字节文件
                }
            }
            return Err(e);
        }
    };
    let part_at = walk_in_root(Last::Link, &dir, &part)?;
    // 落点的同名由调用方（`transfer_copy`）先问过、答过（覆盖 · 跳过 · 两份都留另起名）⇒ 这里照「在不在都行」上位，盖掉的沿用原权限位。
    staging::land(
        face,
        &root,
        Path::new(&name),
        Content::Staged(&part_at),
        None,
        Was::Either,
        &mut || {},
    )?;
    on_progress(done, total);
    Ok(done)
}

/// 那台的名字 ⇒ 照机器表组好的拨号请求（线上形，问那台后端用）＋ 开 sftp 用的那一份。表里没有 · 组不成 ⇒ 那一句。
pub(crate) fn dial_by_name(
    hosts: &[crate::dial::Host],
    machine: &cmd_wire::Machine,
) -> Result<(serde_json::Value, Dial), String> {
    crate::dial::sftp_named(hosts, &machine.to_string())
}

/// SFTP 起始目录与后端 `$HOME` 不一致 ⇒ 那一句话；一致（去掉尾 `/` 逐字节相等）⇒ `None`。
/// ⚠ 认下的：home 经符号链接指过去（`/home` → `/usr/home`）时两串不等 ⇒ 判成不一致、走慢路（仍做得成，只是慢）。
pub(crate) fn start_dir_mismatch(sftp_start: &str, backend_home: &str) -> Option<String> {
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
