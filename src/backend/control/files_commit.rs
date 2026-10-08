//! 上传的提交：把暂存区里一份传完的件挪进用户指定的目标。
//!
//! SFTP 只做传输、只写进我们自己的暂存区（`~/.cc-monitor/staging/<key>.part`，断点续传的本钱也住那儿），
//! 传完由这里提交 ⇒ 真正落进用户目录的那一下只有后端文件管理这一处。
//!
//! 住 `control/` 是层的归属（会改变世界）；它是文件管理后端的成员（`tests/backend/files/module_boundary_guard.rs`）。
//! 不住 `files/`：那一族整族纯读。它在 `readonly_guard` 第三层上：改动动词只用闭集里那几个 · 每个会改动的函数先过路径解析
//! （借 `files_write` 那一份）· 后端生产树里引用得到本模块的只有 `stream/inbound/` 那一条命令。
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
//! （同目录暂存旁名 ＋ 换名上位：权限位沿用、链接解到底、不跨盘；代价写在 `files_write::overwrite_text` 头注）。
//! 一块一份文件、不往一份上续写：续写那几种开法都在第三层禁表上（护栏是不剥注释的子串扫描，这里不写它们的字面名字）。
//! 块数没有单独的上限：每块至少 1 字节 ⇒ 块数 ≤ 总长 ≤ `files::READ_TEXT_MAX_BYTES`。
//! 清理：提交不论成败都删掉这一次的块（窗口手上还有全文，重存换新键）；中途断掉留下的块由 [`sweep_stale`] 收。

use crate::control::files_write::{
    content_sha256, opener, overwrite_text_expecting, read_nofollow, resolve_in_root,
    sha256_expect_of, Answer, ManageCommand, WriteRefusal,
};
use copy_core::copy_text;
use std::path::{Path, PathBuf};

/// 暂存区相对 `$HOME` 的那一段。值只住契约 crate（`relay_route_core::STAGING_DIR_REL`，monitor 的数据位置页按它列出）；
/// 往里写的一侧（`dial/sftp.rs::STAGING_ROOT` · `control/transfer.rs`）同在本 crate，由
/// `dial_sftp_tests::the_declared_write_roots_are_exactly_staging_and_bin` 直接比。
pub const STAGING_DIR: &str = relay_route_core::STAGING_DIR_REL;

/// 暂存件的键长（十六进制位数）。造键的那一侧（`control/transfer.rs::staging_key`）同一个数（直接引用）。
pub const KEY_LEN: usize = 32;

/// 暂存件的后缀。
pub const PART_SUFFIX: &str = ".part";

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
        args: &["bytes", "chunks", "expect", "key", "overwrite", "rel", "root"],
        fields: &["bytes", "path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
    },
    // ── 存盘装不进一行时的两步（头注「第二种件」一节）──────────────
    ManageCommand {
        name: "files-stage-chunk",
        purpose: "把存盘的一块写进暂存区 `<key>.<seq>.chunk`（`O_EXCL` 新建：同一块写第二次就拒；暂存区不在就建）",
        args: &["content", "key", "seq"],
        fields: &["bytes"],
        codes: &["bad_args", "io_failed", "refused"],
    },
    ManageCommand {
        name: "files-commit-text",
        purpose: "按块号读回 `0..chunks` 块、拼起来、总长必须等于 `bytes`，再覆盖写目标（原子换，与 \
               `files-write-text` 同一个原语：跟链接、只收已在的普通文件）；`expect: {sha256}` 必给，\
               盘上那份对不上 ⇒ `stale`、一个字节不写；不论成败都删掉这些块",
        args: &["bytes", "chunks", "expect", "key", "rel", "root"],
        fields: &["bytes", "path", "sha256"],
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
    },
];

/// 本面命令名（给 `readonly_guard` 第三层那条「门里够得到的命令」对拍用）。
pub fn commit_command_names() -> Vec<&'static str> {
    COMMIT_COMMANDS.iter().map(|c| c.name).collect()
}

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

/// 提交：暂存件 → 目标。成功回 `(落点, 字节数)`。第一件事是过路径解析（第三层 ③ 逐函数扫这个顺序）。
///
/// 改名上位之前先对整份摘要：暂存件的 SHA-256 必须等于 `expect_sha256`（传输台一边传一边算、窗口原样交来）。
/// 不等 ⇒ 这份暂存件是坏的（失败后晚到的写在中间留了洞之类）⇒ 删掉它、回 `stale`、目标一个字节不动，调用方从 0 重传。
/// 核与改名之间仍有窗（暂存区是我们自己的目录，窗里没人该碰它）。
pub fn commit_upload(
    home: &Path,
    key: &str,
    root: &Path,
    rel: impl AsRef<Path>,
    overwrite: bool,
    expect_sha256: &str,
) -> Result<(PathBuf, u64), WriteRefusal> {
    commit_upload_in(
        home,
        key,
        root,
        rel.as_ref(),
        overwrite,
        expect_sha256,
        false,
    )
}

/// [`commit_upload`] 的本体。`cross_device` ＝ 判据注入「改名上位回 `EXDEV`」（跨盘在测试里造不出来）；生产恒 `false`。
pub fn commit_upload_in(
    home: &Path,
    key: &str,
    root: &Path,
    rel: &Path,
    overwrite: bool,
    expect_sha256: &str,
    cross_device: bool,
) -> Result<(PathBuf, u64), WriteRefusal> {
    let dest = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let staged = staged_path(home, key)?;
    // 不跟链接地看暂存件一眼：它必须是一份普通文件（一条链接当「源」＝ 挪走链接指向之外的东西，不许）。
    let meta = std::fs::symlink_metadata(&staged).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesCommit.upload.stagedMissing",
            &[
                ("path", &staged.display().to_string()),
                ("e", &e.to_string()),
            ],
        ))
    })?;
    if !meta.file_type().is_file() {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesCommit.upload.stagedNotRegular",
            &[("path", &staged.display().to_string())],
        )));
    }
    let bytes = meta.len();
    let got = crate::files::file_sha256(&staged).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesCommit.upload.stagedUnreadable",
            &[
                ("path", &staged.display().to_string()),
                ("e", &e.to_string()),
            ],
        ))
    })?;
    if got != expect_sha256 {
        let staging = home.join(STAGING_DIR);
        let removed = resolve_in_root(&staging, format!("{key}{PART_SUFFIX}"))
            .ok()
            .is_some_and(|at| std::fs::remove_file(at).is_ok());
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
    if overwrite {
        land_staged(home, key, root, rel, &dest, cross_device).map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.write.failed",
                &[("path", &dest.display().to_string()), ("e", &e.to_string())],
            ))
        })?;
        return Ok((dest, bytes));
    }
    // 不覆盖：先占位（`O_EXCL`，目标已在 ⇒ 当场失败），再改名上位盖掉自己那个占位。
    opener()
        .write(true)
        .create_new(true)
        .open(&dest)
        .map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesCommit.upload.exists",
                &[("path", &dest.display().to_string()), ("e", &e.to_string())],
            ))
        })?;
    if let Err(e) = land_staged(home, key, root, rel, &dest, cross_device) {
        // 撤掉自己那个 0 字节的占位（它是这一次刚建的，不是用户既有数据）；撤不掉要说出来，不然用户目录里留一份 0 字节文件、重试撞「目标已经在了」。
        let undo = std::fs::remove_file(&dest);
        return Err(WriteRefusal::Io(copy_text(
            "beFilesCommit.upload.moveFailedNote",
            &[
                ("path", &dest.display().to_string()),
                ("e", &e.to_string()),
                ("note", &placeholder_note(&dest, &undo)),
            ],
        )));
    }
    Ok((dest, bytes))
}

/// 把暂存件挪到 `dest`：先改名上位（同盘，原子）；回 `EXDEV` ⇒ 复制 ＋ 删：在目标同目录用 `O_EXCL` 建一个暂存旁名
/// （过路径解析）→ 从暂存件读、写满 → 旁名改名上位（同目录，原子）→ 删暂存件。任一步失败 ⇒ 删掉旁名，暂存件留着（续传本钱）。
/// 权限位两支一致（都是进程缺省，受 umask）。不添动词（改名 · `O_EXCL` 新建 · 写 · 删文件，全在闭集里）。
fn land_staged(
    home: &Path,
    key: &str,
    root: &Path,
    rel: &Path,
    dest: &Path,
    cross_device: bool,
) -> Result<(), String> {
    let staging = home.join(STAGING_DIR);
    let staged = resolve_in_root(&staging, format!("{key}{PART_SUFFIX}"))?;
    let side_rel = {
        let p = rel;
        let name = p.file_name().ok_or_else(|| {
            copy_text(
                "beFilesCommit.exdev.noName",
                &[("rel", &rel.display().to_string())],
            )
        })?;
        let mut side = std::ffi::OsString::from(".");
        side.push(name);
        side.push(format!(".ccm-commit-{key}.part"));
        p.with_file_name(side)
    };
    let side = resolve_in_root(root, &side_rel)?;
    let moved = if cross_device {
        Err(std::io::Error::from(std::io::ErrorKind::CrossesDevices))
    } else {
        std::fs::rename(&staged, dest)
    };
    match moved {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() != std::io::ErrorKind::CrossesDevices => return Err(e.to_string()),
        Err(_) => {}
    }
    let copied = (|| -> std::io::Result<()> {
        let mut reader = opener().read(true).open(&staged)?;
        let mut writer = opener().write(true).create_new(true).open(&side)?;
        std::io::copy(&mut reader, &mut writer)?;
        writer.sync_all()?;
        drop(writer);
        std::fs::rename(&side, dest)
    })();
    if let Err(e) = copied {
        let _ = std::fs::remove_file(&side);
        return Err(copy_text(
            "beFilesCommit.exdev.copyFailed",
            &[("e", &e.to_string())],
        ));
    }
    // 目标已经落好；暂存件删不掉不算这一趟失败（不覆盖那一支失败时会撤占位 —— 那就把刚落好的删了）：
    // 它是我们自己暂存区里的一份，孤儿扫（`sweep_stale`）按期限收。
    let _ = std::fs::remove_file(&staged);
    Ok(())
}

/// 提交失败之后撤占位那一步的结局 ⇒ 接在报错后面的那半句。撤掉了 ⇒ 空（没什么要用户做的）。
pub(crate) fn placeholder_note(dest: &Path, undo: &std::io::Result<()>) -> String {
    match undo {
        Ok(()) => String::new(),
        Err(e) => copy_text(
            "beFilesCommit.upload.placeholderLeft",
            &[("path", &dest.display().to_string()), ("e", &e.to_string())],
        ),
    }
}

// ═══════════════ 存盘的块：写一块 · 读回拼起来提交 · 删掉 ═══════════════

/// 暂存区在盘上的位置；不在就建（两层：`~/.cc-monitor` 与它底下的 `staging`，已在不算错）。
///
/// 每一层**先过路径解析**（以上一层为根）再建 —— 第三层 ③ 逐函数扫这个顺序。
/// ⚠ `~/.cc-monitor` 若是用户自己放的一条链接，跟过去（后端自己的家，与第四层 `exit_policy` 同一个家）。
fn ensure_staging(home: &Path) -> Result<PathBuf, WriteRefusal> {
    let mut at = home.to_path_buf();
    for seg in STAGING_DIR.split('/') {
        let next = resolve_in_root(&at, seg).map_err(WriteRefusal::Refused)?;
        // 这一趟建出来的那一层建的那一下就是 0700、已在的不动（`own_dir`：后端建自家目录的那一个函数）。
        crate::common::own_dir::ensure_private_dir(&next).map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.write.failed",
                &[("path", &next.display().to_string()), ("e", &e.to_string())],
            ))
        })?;
        if !std::fs::metadata(&next).is_ok_and(|m| m.is_dir()) {
            return Err(WriteRefusal::Refused(copy_text(
                "beFilesCommit.staging.notDir",
                &[("path", &next.display().to_string())],
            )));
        }
        at = next;
    }
    Ok(at)
}

/// **写一块**：`<key>.<seq>.chunk`，`O_EXCL` 新建。回写进去的字节数。
///
/// 同一块已经在（重发 / 两次存盘撞了键）⇒ 拒，一个字节不盖。写到一半失败 ⇒ 删掉自己刚建的那一份。
pub fn stage_chunk(home: &Path, key: &str, seq: u64, bytes: &[u8]) -> Result<u64, WriteRefusal> {
    use std::io::Write as _;
    if !is_key(key) {
        return Err(WriteRefusal::Refused(crate::common::contract::malformed(
            &format!("staging key must be {KEY_LEN} lowercase hex chars (got {key:?})"),
        )));
    }
    let dir = ensure_staging(home)?;
    let at = resolve_in_root(&dir, &chunk_name(key, seq)).map_err(WriteRefusal::Refused)?;
    let mut f = opener()
        .write(true)
        .create_new(true)
        .open(&at)
        .map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesCommit.chunk.createFailed",
                &[
                    ("seq", &seq.to_string()),
                    ("path", &at.display().to_string()),
                    ("e", &e.to_string()),
                ],
            ))
        })?;
    if let Err(e) = f.write_all(bytes) {
        drop(f);
        let _ = std::fs::remove_file(&at);
        return Err(WriteRefusal::Io(copy_text(
            "beFilesCommit.chunk.writeBroke",
            &[
                ("seq", &seq.to_string()),
                ("path", &at.display().to_string()),
                ("e", &e.to_string()),
            ],
        )));
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
        let meta = std::fs::symlink_metadata(&p).map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesCommit.chunk.missing",
                &[
                    ("seq", &seq.to_string()),
                    ("chunks", &chunks.to_string()),
                    ("path", &p.display().to_string()),
                    ("e", &e.to_string()),
                ],
            ))
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
        let got = read_nofollow(&p).map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesCommit.chunk.unreadable",
                &[
                    ("seq", &seq.to_string()),
                    ("path", &p.display().to_string()),
                    ("e", &e.to_string()),
                ],
            ))
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
    if std::fs::symlink_metadata(dir.join(chunk_name(key, chunks))).is_ok() {
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
pub fn drop_chunks(home: &Path, key: &str) {
    let dir = home.join(STAGING_DIR);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in rd.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if parse_chunk_name(&name).is_none_or(|(k, _)| k != key) {
            continue;
        }
        let Ok(at) = resolve_in_root(&dir, &name) else {
            continue;
        };
        if std::fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_file()) {
            let _ = std::fs::remove_file(&at);
        }
    }
}

/// **提交存盘**：读回 `0..chunks` 块 ⇒ 覆盖写 `root/rel`（写面那一份原子换的原语，路径解析在它里面）。
/// 回 `(落点, 字节数)`。**不论成败**都删掉这一次的块。
pub fn commit_text(
    home: &Path,
    key: &str,
    chunks: u64,
    bytes: u64,
    root: &Path,
    rel: &str,
    expect_sha256: &str,
) -> Result<(PathBuf, u64, String), WriteRefusal> {
    // 与 `files-write-text` 同一道 CAS（`overwrite_text_expecting`）：两支存盘同一种结果，含「盘上被改过 ⇒ stale」。
    let r = gather_chunks(home, key, chunks, bytes).and_then(|body| {
        overwrite_text_expecting(root, rel, &body, expect_sha256)
            .map(|at| (at, body.len() as u64, content_sha256(&body)))
    });
    drop_chunks(home, key);
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
/// - 每一处删之前先过以暂存区为根的路径解析（[`resolve_in_root`]）：暂存区里一条指出去的链接不删。
/// - 尽力而为：列不出、删不掉都不挡调用方。只在事件上跑（提交成功那一刻，见 [`answer_commit`]），后端零定时器。
pub fn sweep_stale(home: &Path, now_secs: u64, keep: &str) -> Vec<String> {
    let dir = home.join(STAGING_DIR);
    let Ok(rd) = std::fs::read_dir(&dir) else {
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
        let Ok(at) = resolve_in_root(&dir, &name) else {
            continue;
        };
        let Ok(meta) = std::fs::symlink_metadata(&at) else {
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
            && std::fs::remove_file(&at).is_ok()
        {
            gone.push(name);
        }
    }
    gone
}

/// 取一个路径参数（字符串 或 `{"b16": …}`，与写面同一口径）。
fn path_arg(args: &serde_json::Value, key: &str) -> Result<PathBuf, (&'static str, String)> {
    let v = args.get(key).ok_or((
        "bad_path",
        crate::common::contract::malformed(&format!("missing `{key}`")),
    ))?;
    let raw = crate::files::raw::from_json(v).ok_or((
        "bad_path",
        crate::common::contract::malformed(&format!(
            "`{key}` must be a string or {{\"b16\": \"<hex>\"}}"
        )),
    ))?;
    if raw.is_empty() {
        return Err((
            "bad_path",
            crate::common::contract::malformed(&format!("`{key}` is empty")),
        ));
    }
    Ok(crate::files::raw::to_path_buf(&raw))
}

/// 取一个字符串参数。
fn str_arg<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str, (&'static str, String)> {
    args.get(key).and_then(serde_json::Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed(&format!("missing `{key}` or not a string")),
    ))
}

fn answer_commit(args: &serde_json::Value) -> Answer {
    answer_commit_at(&home_dir()?, args)
}

/// [`answer_commit`] 去掉「家在哪」那一问之后的全部 —— 判据从这里进（不改进程的 `HOME`：
/// 那是全进程共享的，改了会波及同一进程里并发跑的别的判据）。
fn answer_commit_at(home: &Path, args: &serde_json::Value) -> Answer {
    let root = path_arg(args, "root")?;
    // `rel` 也收 `{"b16": …}`：本机落点是非 UTF-8 名（有损名下载在 Linux 上按原始字节落名）时经这条提交。
    let rel = path_arg(args, "rel")?;
    let key = str_arg(args, "key")?.to_string();
    // 覆盖策略必须显式给：默认成哪一边都是替用户做了一个他没做的决定。
    let overwrite = args
        .get("overwrite")
        .and_then(serde_json::Value::as_bool)
        .ok_or((
            "bad_args",
            crate::common::contract::malformed("missing `overwrite` (true / false, no default)"),
        ))?;
    // 整份摘要**必给**（传输台 done 帧交的那个）：没有「不核就上位」这一形。
    let expect = sha256_expect_of(args)?;
    // 块形（SFTP 起始目录不是后端 home ⇒ 窗口改走后端链路分块写）：先拼成暂存件，下面照旧同一条提交。
    if args.get("chunks").is_some() {
        let (chunks, bytes) = (u64_arg(args, "chunks")?, u64_arg(args, "bytes")?);
        super::files_upload_chunks::assemble_part(home, &key, chunks, bytes)
            .map_err(|e| (e.code(), e.message().to_string()))?;
    }
    let (landed, bytes) = commit_upload(home, &key, &root, &rel, overwrite, &expect)
        .map_err(|e| (e.code(), e.message().to_string()))?;
    // 暂存区清理「孤儿」那一格的事件：一次提交成功。
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    sweep_stale(home, now, &key);
    Ok(serde_json::json!({
        "path": crate::files::raw::to_json(crate::files::raw::path_bytes(&landed)),
        "bytes": bytes,
    }))
}

/// 取一个非负整数参数。
fn u64_arg(args: &serde_json::Value, key: &str) -> Result<u64, (&'static str, String)> {
    args.get(key).and_then(serde_json::Value::as_u64).ok_or((
        "bad_args",
        crate::common::contract::malformed(&format!(
            "missing `{key}` or not a non-negative integer"
        )),
    ))
}

/// 这台后端的家（暂存区拼在它底下）。
fn home_dir() -> Result<PathBuf, (&'static str, String)> {
    crate::platform::paths::home_dir()
        .ok_or(("io_failed", copy_text("beFilesCommit.staging.noHome", &[])))
}

/// `files-stage-chunk`。
fn answer_stage_at(home: &Path, args: &serde_json::Value) -> Answer {
    let key = str_arg(args, "key")?.to_string();
    let seq = u64_arg(args, "seq")?;
    let v = args.get("content").ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `content` (at least 1 byte)"),
    ))?;
    let bytes = crate::files::raw::from_json(v).ok_or((
        "bad_args",
        crate::common::contract::malformed("`content` must be a string or {\"b16\": \"<hex>\"}"),
    ))?;
    if bytes.is_empty() {
        return Err((
            "bad_args",
            crate::common::contract::malformed("empty chunk (each chunk is at least 1 byte)"),
        ));
    }
    let n =
        stage_chunk(home, &key, seq, &bytes).map_err(|e| (e.code(), e.message().to_string()))?;
    Ok(serde_json::json!({ "bytes": n }))
}

/// `files-commit-text`。
fn answer_commit_text_at(home: &Path, args: &serde_json::Value) -> Answer {
    let root = path_arg(args, "root")?;
    let rel = str_arg(args, "rel")?.to_string();
    let key = str_arg(args, "key")?.to_string();
    let chunks = u64_arg(args, "chunks")?;
    let bytes = u64_arg(args, "bytes")?;
    let cap = crate::files::READ_TEXT_MAX_BYTES as u64;
    if bytes > cap {
        return Err((
            "bad_args",
            copy_text(
                "beFilesCommit.text.tooLarge",
                &[("bytes", &bytes.to_string()), ("cap", &cap.to_string())],
            ),
        ));
    }
    if chunks == 0 || chunks > bytes {
        return Err((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`chunks` is {chunks} and `bytes` is {bytes}; chunks must be within 1..=bytes"
            )),
        ));
    }
    let expect = sha256_expect_of(args)?;
    let (landed, n, sha) = commit_text(home, &key, chunks, bytes, &root, &rel, &expect)
        .map_err(|e| (e.code(), e.message().to_string()))?;
    // 提交成功 ⇒ 顺手扫孤儿（同上传那一条的事件）。
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    sweep_stale(home, now, &key);
    Ok(serde_json::json!({
        "path": crate::files::raw::to_json(crate::files::raw::path_bytes(&landed)),
        "bytes": n,
        "sha256": sha,
    }))
}

/// 这一面的**唯一入口**（形状照写面那一个）。
pub fn answer_wire(wire_name: &str, args: &serde_json::Value) -> Answer {
    match wire_name {
        "files-commit-upload" => answer_commit(args),
        "files-stage-chunk" => answer_stage_at(&home_dir()?, args),
        "files-commit-text" => answer_commit_text_at(&home_dir()?, args),
        other => Err((
            "bad_args",
            crate::common::contract::malformed(&format!("unknown command `{other}`")),
        )),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/files_commit_tests.rs"]
mod tests;
