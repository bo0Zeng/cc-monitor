//! 上传的提交：把暂存区里一份传完的件挪进用户指定的目标。
//!
//! SFTP 只做传输、只写进我们自己的暂存区（`~/.cc-monitor/staging/<key>.part`，断点续传的本钱也住那儿），
//! 传完由这里提交 ⇒ 真正落进用户目录的那一下只有后端文件管理这一处。
//!
//! 住 `control/` 是层的归属（会改变世界）；它是文件管理后端的成员（`tests/backend/files/module_boundary_guard.rs`）。
//! 不住 `files/`：那一族整族纯读。写经文件管理面的句柄（`Face`，只收路径解析交回的 `Resolved`，
//! 借 `files_write` 那一份）；句柄只经命令表文件管理那一族递进来。
//!
//! # 提交的两支（`overwrite` 由调用方显式给，不给默认值）
//!
//! | `overwrite` | 做法 | 为什么 |
//! |---|---|---|
//! | `false` | 先在目标上用 `O_EXCL` 占一个位（已在 ⇒ 拒），再把暂存件改名上位、盖掉那个占位 | 「目标已经在了就拒」要原子地判；占位成功那一刻，目标确实此前不存在 |
//! | `true` | 暂存件直接改名上位 | 同一个盘上的改名是原子的：读的人要么看见旧的一整份，要么看见新的一整份 |
//!
//! 暂存件的路径由本模块自己拼（`$HOME` ＋ 固定的相对段 ＋ `key`），`key` 只收 [`KEY_LEN`] 位小写十六进制 ⇒ 调用方指不到暂存区之外的文件当「源」。
//! 跨盘（改名回 `EXDEV`）走「复制 ＋ 删」，住 [`land_staged`]。
//!
//! # 买不到的
//!
//! - 「SFTP 的起始目录 == 这里的 `$HOME`」是前提：`ChrootDirectory` / `internal-sftp -d` 的机器上两边拼出来的不是同一份 ⇒ 答「暂存件不在」。
//! - 占位与改名之间仍有一个窗：别的进程那一瞬往占位里写的东西会被盖掉（那是一份刚出现、0 字节起步的文件，不是用户既有数据）。
//! - Windows 远端：改名覆盖的语义不同（目标在就失败），`overwrite: true` 那一支在那边会拒。没量过。

//!
//! # 第二种件：存盘的块（`files-stage-chunk` ＋ `files-commit-text`）
//!
//! 文件窗口存一份装不进一条请求行的文本（入方向一行 1 MiB，`stream/inbound/mod.rs::MAX_LINE_BYTES`）时，
//! 切成几块逐块送进暂存区（`<key>.<seq>.chunk`，`O_EXCL` 新建：一块只写一次），再由 `files-commit-text` 按块号读回、
//! 拼起来、核总长，交给写面那一份覆盖写（`files_write::overwrite_text`）—— 同一份文件 1 MiB 上下存出同一种结果
//! （同目录暂存旁名 ＋ 换名上位：权限位沿用、链接解到底、不跨盘；怎么换住 `files_write::replace_contents`）。
//! 一块一份文件、不往一份上续写：`Face` 没有续写那几种开法（新建只有 `O_EXCL` 一种）。
//! 块数没有单独的上限：每块至少 1 字节 ⇒ 块数 ≤ 总长 ≤ `files::READ_TEXT_MAX_BYTES`。
//! 清理：提交不论成败都删掉这一次的块（窗口手上还有全文，重存换新键）；中途断掉留下的块由 [`sweep_stale`] 收。

use crate::control::files_write::{
    content_sha256, overwrite_text_expecting, rel_of, replace_contents, root_of, walk_in_root,
    wire_of, Act, Content, Last, ManageCommand, Was, WriteFail, WriteRefusal,
};
use crate::platform::fs::{Face, Ro};
use cmd_wire::files as wire;
use copy_core::{copy_text, io_reason};
use std::path::{Path, PathBuf};

/// 暂存区相对 `$HOME` 的那一段。值只住契约 crate（`relay_route_core::STAGING_DIR_REL`，monitor 的数据位置页按它列出）；
/// 往里写的一侧（`ssh-dial/sftp.rs::STAGING_ROOT` · `control/transfer.rs`）同在本 crate，由
/// `dial_sftp_tests::the_declared_write_roots_are_exactly_staging_and_bin` 直接比。
pub const STAGING_DIR: &str = relay_route_core::STAGING_DIR_REL;

// 暂存件的键长与后缀住 `common/staging.rs`：造键的一侧（传输台，原生那一块）与提交的一侧（这里）各引各的。
pub use crate::common::staging::{KEY_LEN, PART_SUFFIX};

/// 存盘块的后缀（`<key>.<seq>.chunk`）。与 [`PART_SUFFIX`] 刻意不同名：两种件的消耗方式不同
/// （上传件改名上位、块读回后删），混一个名字，提交与孤儿扫就得去猜它是哪一种。
pub const CHUNK_SUFFIX: &str = ".chunk";

/// 第 `seq` 块的文件名。
pub fn chunk_name(key: &str, seq: u64) -> String {
    format!("{key}.{seq}{CHUNK_SUFFIX}")
}

/// 这个名字是不是一块（回 `(键, 块号)`）。块号只认**规范**十进制（`0` 或不以 `0` 开头），
/// 否则 `a.01.chunk` 与 `a.1.chunk` 会是同一块的两个名字。
pub fn parse_chunk_name(name: &str) -> Option<(&str, u64)> {
    let (key, seq) = name.strip_suffix(CHUNK_SUFFIX)?.split_once('.')?;
    let canonical = !seq.is_empty()
        && seq.bytes().all(|b| b.is_ascii_digit())
        && (seq == "0" || !seq.starts_with('0'));
    if !is_key(key) || !canonical {
        return None;
    }
    Some((key, seq.parse().ok()?))
}

/// 本面的命令表（形状借写面那一个类型：线上契约的数据形态）。
pub const COMMIT_COMMANDS: &[ManageCommand] = &[
    ManageCommand {
        name: "files-commit-upload",
        purpose:
            "把暂存区里一份传完的上传件挪进用户指定的目标（先过路径解析；先核整份摘要 `expect: {sha256}`，\
             不等 ⇒ 删掉坏暂存件、`stale`；不覆盖时 `O_EXCL` 占位再改名上位）",
        // +`chunks`（入，可缺席；`bytes` 同时是入）：块形 ⇒ 先把块拼成暂存件（`files_upload_chunks`）再走这同一条提交。
    },
    // ── 存盘装不进一行时的两步（头注「第二种件」一节）──────────────
    ManageCommand {
        name: "files-stage-chunk",
        purpose: "把存盘的一块写进暂存区 `<key>.<seq>.chunk`（`O_EXCL` 新建：同一块写第二次就拒；暂存区不在就建）",
    },
    ManageCommand {
        name: "files-commit-text",
        purpose: "按块号读回 `0..chunks` 块、拼起来、总长必须等于 `bytes`，再覆盖写目标（原子换，与 \
               `files-write-text` 同一个原语：跟链接、只收已在的普通文件）；`expect: {sha256}` 必给，\
               盘上那份对不上 ⇒ `stale`、一个字节不写；不论成败都删掉这些块",
    },
];
/// `key` 合不合法：恰好 [`KEY_LEN`] 位、只有 `0-9a-f`。
pub fn is_key(key: &str) -> bool {
    key.len() == KEY_LEN
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// 暂存件的绝对路径。`key` 不合法 ⇒ 拒（**不**猜、不清洗）。
pub fn staged_path(home: &Path, key: &str) -> Result<PathBuf, WriteRefusal> {
    if !is_key(key) {
        return Err(WriteRefusal::Refused(crate::common::contract::malformed(
            &format!("staging key must be {KEY_LEN} lowercase hex chars (got {key:?})"),
        )));
    }
    Ok(home.join(STAGING_DIR).join(format!("{key}{PART_SUFFIX}")))
}

/// 提交：暂存件 → 目标。成功回 `(落点, 字节数)`。
///
/// 上位之前先对整份摘要：暂存件的 SHA-256 必须等于 `expect_sha256`（传输台一边传一边算、窗口原样交来）。
/// 不等 ⇒ 这份暂存件是坏的（失败后晚到的写在中间留了洞之类）⇒ 删掉它、回 `stale`、目标一个字节不动，调用方从 0 重传。
/// 怎么落交 [`replace_contents`]（盖掉已在的沿用它的权限位；`overwrite` 否 ⇒ 不覆盖上位，已在 ⇒ `exists`）。
/// 核与上位之间仍有窗（暂存区是我们自己的目录，窗里没人该碰它）。
pub fn commit_upload(
    face: &Face,
    home: &Path,
    key: &str,
    root: &Path,
    rel: impl AsRef<Path>,
    overwrite: bool,
    expect_sha256: &str,
) -> Result<(PathBuf, u64), WriteRefusal> {
    let staged = staged_path(home, key)?;
    // 不跟链接地看暂存件一眼：它必须是一份普通文件（一条链接当「源」＝ 挪走链接指向之外的东西，不许）。
    let meta = Ro::symlink_metadata(&staged).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "beFilesCommit.upload.stagedMissing",
                &[
                    ("path", &staged.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    if !meta.file_type().is_file() {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesCommit.upload.stagedNotRegular",
            &[("path", &staged.display().to_string())],
        )));
    }
    let staging = home.join(STAGING_DIR);
    let staged_at = walk_in_root(Last::Link, &staging, format!("{key}{PART_SUFFIX}"))
        .map_err(WriteRefusal::Refused)?;
    let got = crate::files::file_sha256(&staged).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "beFilesCommit.upload.stagedUnreadable",
                &[
                    ("path", &staged.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    if got != expect_sha256 {
        let removed = face.delete_file(&staged_at).is_ok();
        return Err(WriteRefusal::Stale(copy_text(
            "beFilesCommit.upload.corrupt",
            &[(
                "tail",
                &if removed {
                    copy_text("beFilesCommit.upload.corruptRemoved", &[])
                } else {
                    copy_text("beFilesCommit.upload.corruptKept", &[])
                },
            )],
        )));
    }
    let was = if overwrite { Was::Either } else { Was::New };
    replace_contents(
        face,
        root,
        rel.as_ref(),
        Content::Staged(&staged_at),
        None,
        was,
        &mut || {},
    )
}

// ═══════════════ 存盘的块：写一块 · 读回拼起来提交 · 删掉 ═══════════════

/// 暂存区在盘上的位置；不在就建（两层：`~/.cc-monitor` 与它底下的 `staging`，已在不算错）。
///
/// 每一层**先过路径解析**（以上一层为根）再建 —— `Face` 只收它交回的 `Resolved`。
/// ⚠ `~/.cc-monitor` 若是用户自己放的一条链接，跟过去（后端自己的家，与 `exit_policy` 同一个家）。
fn ensure_staging(face: &Face, home: &Path) -> Result<PathBuf, WriteRefusal> {
    let mut at = home.to_path_buf();
    for seg in STAGING_DIR.split('/') {
        let next = walk_in_root(Last::Link, &at, seg).map_err(WriteRefusal::Refused)?;
        // 这一趟建出来的那一层建的那一下就是 0700、已在的不动（`platform/fs` 带权限位建一层目录的那一个函数）。
        face.make_dir_private(&next).map_err(|e| {
            WriteRefusal::io(
                copy_text(
                    "beFilesWrite.write.failed",
                    &[
                        ("path", &next.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })?;
        if !Ro::metadata(&next).is_ok_and(|m| m.is_dir()) {
            return Err(WriteRefusal::Refused(copy_text(
                "beFilesCommit.staging.notDir",
                &[("path", &next.display().to_string())],
            )));
        }
        at = next.into_path_buf();
    }
    Ok(at)
}

/// **写一块**：`<key>.<seq>.chunk`，`O_EXCL` 新建。回写进去的字节数。
///
/// 同一块已经在（重发 / 两次存盘撞了键）⇒ 拒，一个字节不盖。写到一半失败 ⇒ 删掉自己刚建的那一份。
pub fn stage_chunk(
    face: &Face,
    home: &Path,
    key: &str,
    seq: u64,
    bytes: &[u8],
) -> Result<u64, WriteRefusal> {
    use std::io::Write as _;
    if !is_key(key) {
        return Err(WriteRefusal::Refused(crate::common::contract::malformed(
            &format!("staging key must be {KEY_LEN} lowercase hex chars (got {key:?})"),
        )));
    }
    let dir = ensure_staging(face, home)?;
    let at =
        walk_in_root(Last::Link, &dir, &chunk_name(key, seq)).map_err(WriteRefusal::Refused)?;
    let mut f = face.create_new(&at).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "beFilesCommit.chunk.createFailed",
                &[
                    ("seq", &seq.to_string()),
                    ("path", &at.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    if let Err(e) = f.write_all(bytes) {
        drop(f);
        let _ = face.delete_file(&at);
        return Err(WriteRefusal::io(
            copy_text(
                "beFilesCommit.chunk.writeBroke",
                &[
                    ("seq", &seq.to_string()),
                    ("path", &at.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        ));
    }
    Ok(bytes.len() as u64)
}

/// 按块号读回 `0..chunks`，拼起来。总长必须**恰好**等于 `bytes`，多出第 `chunks` 块也拒。
/// **纯读**（不跟链接地看每一块一眼：它必须是一份普通文件）。
fn gather_chunks(home: &Path, key: &str, chunks: u64, bytes: u64) -> Result<Vec<u8>, WriteRefusal> {
    if !is_key(key) {
        return Err(WriteRefusal::Refused(crate::common::contract::malformed(
            &format!("staging key must be {KEY_LEN} lowercase hex chars (got {key:?})"),
        )));
    }
    let dir = home.join(STAGING_DIR);
    let want = usize::try_from(bytes).unwrap_or(usize::MAX);
    let mut out: Vec<u8> = Vec::with_capacity(want.min(crate::files::READ_TEXT_MAX_BYTES));
    for seq in 0..chunks {
        let p = dir.join(chunk_name(key, seq));
        let meta = Ro::symlink_metadata(&p).map_err(|e| {
            WriteRefusal::io(
                copy_text(
                    "beFilesCommit.chunk.missing",
                    &[
                        ("seq", &seq.to_string()),
                        ("chunks", &chunks.to_string()),
                        ("path", &p.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })?;
        if !meta.file_type().is_file() {
            return Err(WriteRefusal::Refused(copy_text(
                "beFilesCommit.chunk.notRegular",
                &[
                    ("seq", &seq.to_string()),
                    ("path", &p.display().to_string()),
                ],
            )));
        }
        if out.len() as u64 + meta.len() > bytes {
            return Err(WriteRefusal::Io(copy_text(
                "beFilesCommit.chunk.overflow",
                &[("seq", &seq.to_string()), ("bytes", &bytes.to_string())],
            )));
        }
        let got = Ro::read_nofollow(&p).map_err(|e| {
            WriteRefusal::io(
                copy_text(
                    "beFilesCommit.chunk.unreadable",
                    &[
                        ("seq", &seq.to_string()),
                        ("path", &p.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })?;
        out.extend_from_slice(&got);
    }
    if out.len() as u64 != bytes {
        return Err(WriteRefusal::Io(copy_text(
            "beFilesCommit.chunk.lengthMismatch",
            &[
                ("chunks", &chunks.to_string()),
                ("got", &out.len().to_string()),
                ("bytes", &bytes.to_string()),
            ],
        )));
    }
    if Ro::symlink_metadata(dir.join(chunk_name(key, chunks))).is_ok() {
        return Err(WriteRefusal::Io(copy_text(
            "beFilesCommit.chunk.extra",
            &[("chunks", &chunks.to_string())],
        )));
    }
    Ok(out)
}

/// 删掉这个键的**全部**块（列暂存区、按名字认 —— 不按块号数：说错块数、中间缺一块时照样删干净）。
/// 尽力而为，删不掉不挡调用方（孤儿扫会收）。每一处删之前先过以暂存区为根的路径解析；链接不删
/// （那不是我们放的一块）。
pub fn drop_chunks(face: &Face, home: &Path, key: &str) {
    let dir = home.join(STAGING_DIR);
    let Ok(rd) = Ro::read_dir(&dir) else {
        return;
    };
    for entry in rd.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if parse_chunk_name(&name).is_none_or(|(k, _)| k != key) {
            continue;
        }
        let Ok(at) = walk_in_root(Last::Link, &dir, &name) else {
            continue;
        };
        if Ro::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_file()) {
            let _ = face.delete_file(&at);
        }
    }
}

/// **提交存盘**：读回 `0..chunks` 块 ⇒ 覆盖写 `root/rel`（写面那一份原子换的原语，路径解析在它里面）。
/// 回 `(落点, 字节数)`。**不论成败**都删掉这一次的块。
pub fn commit_text(
    face: &Face,
    home: &Path,
    key: &str,
    chunks: u64,
    bytes: u64,
    root: &Path,
    rel: &Path,
    expect_sha256: &str,
) -> Result<(PathBuf, u64, String), WriteRefusal> {
    // 与 `files-write-text` 同一道 CAS（`overwrite_text_expecting`）：两支存盘同一种结果，含「盘上被改过 ⇒ stale」。
    let r = gather_chunks(home, key, chunks, bytes).and_then(|body| {
        overwrite_text_expecting(face, root, rel, &body, expect_sha256)
            .map(|at| (at, body.len() as u64, content_sha256(&body)))
    });
    drop_chunks(face, home, key);
    r
}

/// 多久没动过的暂存件算孤儿（秒）。孤儿只有两种来路：上传失败之后再没重试的（失败刻意留着给续传）· 传完了窗口没来得及提交的；
/// 正在传的件每写一块就刷新修改时间。7 天是约定，不是量出来的。
/// 住后端：「修改时间」与「此刻」必须取同一台机器的钟。
pub const STAGING_STALE_SECS: u64 = 7 * 24 * 3600;

/// 孤儿扫：暂存区里修改时间早于 `now - STAGING_STALE_SECS` 的暂存件删掉。回删掉的名字。
///
/// - 只认我们自己的形状：`<32 位十六进制>.part` 与 `<32 位十六进制>.<块号>.chunk`；别的名字一个不碰。
/// - `keep` 那一个不碰（调用方此刻手上的那一份）。
/// - 每一处删之前先过以暂存区为根的路径解析（[`walk_in_root`]）：暂存区里一条指出去的链接不删。
/// - 尽力而为：列不出、删不掉都不挡调用方。只在事件上跑（提交成功那一刻，见 [`answer_commit`]），后端零定时器。
pub fn sweep_stale(face: &Face, home: &Path, now_secs: u64, keep: &str) -> Vec<String> {
    let dir = home.join(STAGING_DIR);
    let Ok(rd) = Ro::read_dir(&dir) else {
        return Vec::new();
    };
    let mut gone = Vec::new();
    for entry in rd.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        // 两种形状：上传件 `<key>.part` · 存盘块 `<key>.<seq>.chunk`。
        let Some(key) = name
            .strip_suffix(PART_SUFFIX)
            .or_else(|| parse_chunk_name(&name).map(|(k, _)| k))
        else {
            continue;
        };
        if !is_key(key) || key == keep {
            continue;
        }
        let Ok(at) = walk_in_root(Last::Link, &dir, &name) else {
            continue;
        };
        let Ok(meta) = Ro::symlink_metadata(&at) else {
            continue;
        };
        if !meta.file_type().is_file() {
            continue;
        }
        let Some(mtime) = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        else {
            continue;
        };
        if now_secs.saturating_sub(mtime.as_secs()) > STAGING_STALE_SECS
            && face.delete_file(&at).is_ok()
        {
            gone.push(name);
        }
    }
    gone
}

/// 这一面的失败：与写面同一形（码 ＋ 那一句 ＋ 下层原话；命令表那一层把原话交给应答的复制详情）。
/// 不借流那一层的类型：文件管理后端只许够 platform / common（`module_boundary_guard`）。
pub type Fail = WriteFail;

/// 写面拒绝 ⇒ 失败：码由那个枚举自己答，原话（有的话）随失败交出去。
fn refused(e: WriteRefusal) -> Fail {
    WriteFail::from(e)
}

/// 没有原话的那几形（参数不对 · 家问不到）。
fn plain(f: (&'static str, String)) -> Fail {
    WriteFail::from(f)
}

/// 这台后端的家（暂存区拼在它底下）。
fn home_dir() -> Result<PathBuf, Fail> {
    crate::platform::paths::home_dir()
        .ok_or(("io_failed", copy_text("beFilesCommit.staging.noHome", &[])))
        .map_err(plain)
}

/// 一次提交成功 ⇒ 顺手扫孤儿（暂存区清理「孤儿」那一格的事件）。
fn sweep_after(face: &Face, home: &Path, key: &str) {
    let now = crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    sweep_stale(face, home, now, key);
}

crate::control::files_write::acts! {
    FilesCommitUploadReq => FilesPathBytesReply, |r, f| commit_upload_of(f, r, &crate::common::time::Tz::default()).map_err(CommitRefused::flat);
    FilesStageChunkReq => FilesStageChunkReply, |r, f| stage_chunk_of(f, r);
    FilesCommitTextReq => FilesWrittenReply, |r, f| commit_text_of(f, r);
}

/// 上传提交没成：落点有同名、要问（那一句 ＋ 同名清单，码 `exists`）· 别的（码 ＋ 那一句）。
#[derive(Debug)]
pub(crate) enum CommitRefused {
    Clash(String, cmd_wire::batch::Conflicts),
    Fail(Fail),
}

impl From<Fail> for CommitRefused {
    fn from(f: Fail) -> Self {
        CommitRefused::Fail(f)
    }
}

impl CommitRefused {
    /// 只认码与那一句的出口（写面那张分派表 · 资产域的门）：同名照码 `exists` 交那一句。
    pub(crate) fn flat(self) -> Fail {
        match self {
            CommitRefused::Clash(said, _) => WriteFail::from((super::files_write::EXISTS, said)),
            CommitRefused::Fail(f) => f,
        }
    }
}

/// `files-commit-upload`（同名清单里的时刻按 `tz` 写）。
pub(crate) fn commit_upload_of(
    face: &Face,
    req: wire::FilesCommitUploadReq,
    tz: &crate::common::time::Tz,
) -> Result<wire::FilesPathBytesReply, CommitRefused> {
    commit_upload_at(face, &home_dir()?, req, tz)
}

/// [`commit_upload_of`] 去掉「家在哪」那一问之后的全部 —— 判据从这里进（不改进程的 `HOME`：
/// 那是全进程共享的，改了会波及同一进程里并发跑的别的判据）。整份摘要必须显式给（缺席在解入参那一步就拒了）。
///
/// 落点已有同名 ⇒ 照 `on_exists`（同成组复制那一条，缺 ⇒ `ask`）：`ask` ⇒ 拼块之前就回同名清单，块与暂存件都留着，
/// 改个答原样再交一次就接着提交；`overwrite` ⇒ 盖；`keep_both` ⇒ 另起一个名（同成组复制的起名）；`skip` ⇒ `bad_args`。
pub fn commit_upload_at(
    face: &Face,
    home: &Path,
    req: wire::FilesCommitUploadReq,
    tz: &crate::common::time::Tz,
) -> Result<wire::FilesPathBytesReply, CommitRefused> {
    use cmd_wire::batch::OnExists;
    let root = root_of(&req.root, "root")?;
    // `rel` 也收 `{"b16": …}`：本机落点是非 UTF-8 名（有损名下载在 Linux 上按原始字节落名）时经这条提交。
    let mut rel = root_of(&req.rel, "rel")?;
    super::files_write::names_ok(req.single.unwrap_or(false), &rel)?;
    let on = req.on_exists.unwrap_or_default();
    if on == OnExists::Skip {
        return Err(plain((
            "bad_args",
            crate::common::contract::malformed("`on_exists: skip` has nothing to commit"),
        ))
        .into());
    }
    let clash = |rel: &Path| {
        walk_in_root(Last::Link, &root, rel)
            .ok()
            .and_then(|d| super::files_batch::look(&d).map(|t| (d, t)))
    };
    if let Some((dest, there)) = clash(&rel) {
        match on {
            OnExists::Ask => return Err(asked(home, &req, &dest, there, tz)),
            OnExists::KeepBoth => {
                rel = (2..=crate::common::batch_book::MINT_TRIES)
                    .map(|k| crate::common::batch_book::minted(&rel, k))
                    .find(|r| clash(r).is_none())
                    .ok_or_else(|| {
                        plain((
                            super::files_write::EXISTS,
                            copy_text(
                                "beFilesWrite.swap.exists",
                                &[("path", &dest.display().to_string())],
                            ),
                        ))
                    })?;
            }
            OnExists::Overwrite | OnExists::Skip => {}
        }
    }
    // 块形（SFTP 起始目录不是后端 home ⇒ 窗口改走后端链路分块写）：先拼成暂存件，下面照旧同一条提交。
    if let Some(chunks) = req.chunks {
        let bytes = req.bytes.ok_or_else(|| {
            plain((
                "bad_args",
                crate::common::contract::malformed("`chunks` given without `bytes`"),
            ))
        })?;
        super::files_upload_chunks::assemble_part(face, home, &req.key, chunks, bytes)
            .map_err(refused)?;
    }
    let overwrite = on == OnExists::Overwrite;
    let (landed, bytes) = commit_upload(
        face,
        home,
        &req.key,
        &root,
        &rel,
        overwrite,
        &req.expect.sha256.0,
    )
    .map_err(|e| {
        // 探的时候不在、提交那一下在了 ⇒ 照同名回（暂存件留着，改个答再交）。
        match clash(&rel).filter(|_| !overwrite) {
            Some((dest, there)) => asked(home, &req, &dest, there, tz),
            None => refused(e).into(),
        }
    })?;
    sweep_after(face, home, &req.key);
    Ok(wire::FilesPathBytesReply {
        path: wire_of(&landed),
        bytes,
    })
}

/// 落点有同名、要问：那一句 ＋ 只一项的同名清单（键 `0`；要放过去的是这份上传件，大小照交来的 `bytes` 或暂存件）。
fn asked(
    home: &Path,
    req: &wire::FilesCommitUploadReq,
    dest: &Path,
    (there, there_size, there_mtime): (wire::FileKind, Option<u64>, Option<i64>),
    tz: &crate::common::time::Tz,
) -> CommitRefused {
    use crate::common::batch_book::{key_of, moment_text};
    let size = req.bytes.or_else(|| {
        staged_path(home, &req.key)
            .ok()
            .and_then(|p| Ro::symlink_metadata(&p).ok())
            .map(|m| m.len())
    });
    let name = dest
        .file_name()
        .map_or_else(Default::default, |n| wire_of(Path::new(n)));
    CommitRefused::Clash(
        copy_text("beFiles.batch.clash", &[("n", "1")]),
        cmd_wire::batch::Conflicts {
            conflicts: vec![cmd_wire::batch::Conflict {
                key: key_of(0),
                name,
                kind: wire::FileKind::File,
                size,
                size_text: size.map(copy_core::size_text),
                mtime_text: None,
                there,
                there_size,
                there_size_text: there_size.map(copy_core::size_text),
                there_mtime_text: moment_text(there_mtime, tz),
                answers: crate::common::batch_book::answers_of(there == wire::FileKind::Dir, false),
            }],
        },
    )
}

/// `files-stage-chunk`。
fn stage_chunk_of(
    face: &Face,
    req: wire::FilesStageChunkReq,
) -> Result<wire::FilesStageChunkReply, Fail> {
    stage_chunk_at(face, &home_dir()?, req)
}

/// [`stage_chunk_of`] 去掉「家在哪」那一问（判据从这里进）。空块 ⇒ `bad_args`（每块至少 1 字节）。
pub fn stage_chunk_at(
    face: &Face,
    home: &Path,
    req: wire::FilesStageChunkReq,
) -> Result<wire::FilesStageChunkReply, Fail> {
    if req.content.bytes().is_empty() {
        return Err(plain((
            "bad_args",
            crate::common::contract::malformed("empty chunk (each chunk is at least 1 byte)"),
        )));
    }
    let n = stage_chunk(face, home, &req.key, req.seq, req.content.bytes()).map_err(refused)?;
    Ok(wire::FilesStageChunkReply { bytes: n })
}

/// `files-commit-text`。
fn commit_text_of(
    face: &Face,
    req: wire::FilesCommitTextReq,
) -> Result<wire::FilesWrittenReply, Fail> {
    commit_text_at(face, &home_dir()?, req)
}

/// [`commit_text_of`] 去掉「家在哪」那一问（判据从这里进）。
pub fn commit_text_at(
    face: &Face,
    home: &Path,
    req: wire::FilesCommitTextReq,
) -> Result<wire::FilesWrittenReply, Fail> {
    let root = root_of(&req.root, "root")?;
    let (chunks, bytes) = (req.chunks, req.bytes);
    let cap = crate::files::READ_TEXT_MAX_BYTES as u64;
    if bytes > cap {
        return Err(plain((
            "bad_args",
            copy_text(
                "beFilesCommit.text.tooLarge",
                &[("bytes", &bytes.to_string()), ("cap", &cap.to_string())],
            ),
        )));
    }
    if chunks == 0 || chunks > bytes {
        return Err(plain((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`chunks` is {chunks} and `bytes` is {bytes}; chunks must be within 1..=bytes"
            )),
        )));
    }
    let (landed, n, sha) = commit_text(
        face,
        home,
        &req.key,
        chunks,
        bytes,
        &root,
        &rel_of(&req.rel),
        &req.expect.sha256.0,
    )
    .map_err(refused)?;
    sweep_after(face, home, &req.key);
    Ok(wire::FilesWrittenReply {
        path: wire_of(&landed),
        bytes: n,
        sha256: wire::Sha256Hex(sha),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/control/files_commit_tests.rs"]
mod tests;
