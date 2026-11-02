//! 聊天附件的提交：暂存区里传完的那一份落进会话工作目录下 `.ccm-attach/<编号>/<原名>`（形状住 [`crate::common::attachment_dir`]），
//! 连同开传前那一问（收不收）· 草稿上撤掉那一份 · 每落一份顺手清掉超过七天的。上传提交的一支：落那一下就是 [`super::commit_upload_at`]。
//!
//! - 上限是这里的常量（单份 · 一条的份数与总量 · 一个附件目录），只在这台判。
//! - 工作目录不可写 ⇒ 拒（`not_writable`），不回落到别处：引用写的是相对工作目录的路径，换了地方 agent 就找不到。
//! - 清理只靠事件：草稿上撤 ⇒ [`drop_one`]；每落一份 ⇒ [`sweep`]；会话结束不删（记录里还引着它）。后端零定时器。
//! - 附件目录里只碰我们自己的形状（编号目录 · 那一份 `.gitignore`），别的名字一个不碰；删之前先过以附件目录为根的路径解析。

use crate::common::attachment_dir as layout;
use crate::common::root_path::{walk_in_root, Last};
use crate::common::said::Said;
use layout::{DIR, GITIGNORE, GITIGNORE_BODY};

/// 这一支的失败（同上传提交那一形）。
use crate::platform::fs::{Face, Ro};
use cmd_wire::attachment::{
    AttachItem, AttachRefused, AttachVerdict, SessionAttachmentCommitReply,
    SessionAttachmentCommitReq,
};
use copy_core::{copy_text, io_reason, size_text};
use relay_route_core::{Cap, Over};
use std::path::{Path, PathBuf};

/// 单份上限（超了那一份不收，说清为什么）。
pub const MAX_ONE_BYTES: Cap = Cap {
    bytes: 50 * 1024 * 1024,
    over: Over::Reject,
};
/// 一条最多几份。
pub const MAX_COUNT: usize = 10;
/// 一条合计上限。
pub const MAX_MESSAGE_BYTES: Cap = Cap {
    bytes: 100 * 1024 * 1024,
    over: Over::Reject,
};
/// 一个工作目录的附件目录合计上限。
pub const MAX_DIR_BYTES: Cap = Cap {
    bytes: 1024 * 1024 * 1024,
    over: Over::Reject,
};
/// 多久没动过的编号目录在下一次落的时候清掉（秒）。
pub const STALE_SECS: u64 = 7 * 24 * 3600;
/// 编号撞了（同一分钟、同一个尾巴）换尾巴再试几次。
const MINT_TRIES: u16 = 16;

/// 不收的那一种 ⇒ 码（命令级码与 [`AttachRefused`] 同一组词）。
pub fn code_of(why: AttachRefused) -> &'static str {
    match why {
        AttachRefused::IsDir => "is_dir",
        AttachRefused::TooBig => "too_big",
        AttachRefused::TooMany => "too_many",
        AttachRefused::TooMuch => "too_much",
        AttachRefused::DirFull => "dir_full",
        AttachRefused::NotWritable => "not_writable",
        AttachRefused::NoSession => "no_session",
    }
}

/// 不收的那一种 ⇒ 那一句。
pub fn said_of(why: AttachRefused) -> String {
    match why {
        AttachRefused::IsDir => copy_text("beAttach.no.isDir", &[]),
        AttachRefused::TooBig => copy_text(
            "beAttach.no.tooBig",
            &[("cap", &size_text(MAX_ONE_BYTES.bytes))],
        ),
        AttachRefused::TooMany => {
            copy_text("beAttach.no.tooMany", &[("n", &MAX_COUNT.to_string())])
        }
        AttachRefused::TooMuch => copy_text(
            "beAttach.no.tooMuch",
            &[("cap", &size_text(MAX_MESSAGE_BYTES.bytes))],
        ),
        AttachRefused::DirFull => copy_text(
            "beAttach.no.dirFull",
            &[("cap", &size_text(MAX_DIR_BYTES.bytes))],
        ),
        AttachRefused::NotWritable => copy_text("beAttach.no.notWritable", &[]),
        AttachRefused::NoSession => copy_text("beAttach.no.noSession", &[]),
    }
}

/// 不收 ⇒ 失败（码 ＋ 那一句）。
pub fn refused(why: AttachRefused) -> Said {
    Said::new(code_of(why), said_of(why))
}

fn verdict(it: &AttachItem, why: Option<AttachRefused>) -> AttachVerdict {
    let class = crate::common::file_class::class_of_name(it.name.as_bytes(), it.dir);
    AttachVerdict {
        ok: why.is_none(),
        class,
        class_text: crate::common::file_class::label(class),
        why,
        said: why.map(said_of),
    }
}

/// 附件目录此刻合计多大（编号目录里的普通文件；读不出的不算）。
pub fn dir_usage(cwd: &Path) -> u64 {
    let root = cwd.join(DIR);
    let Ok(rd) = Ro::read_dir(&root) else {
        return 0;
    };
    rd.flatten()
        .filter(|e| e.file_name().to_str().is_some_and(layout::is_id))
        .filter_map(|e| Ro::read_dir(e.path()).ok())
        .flat_map(|d| d.flatten())
        .filter_map(|f| Ro::symlink_metadata(f.path()).ok())
        .filter(|m| m.file_type().is_file())
        .map(|m| m.len())
        .sum()
}

/// 工作目录在、是目录、看得出可写 ⇒ `None`；否则那一种。权限位说只读才算不可写（真写不写得进，落的那一下再判）。
fn cwd_state(cwd: &Path) -> Option<AttachRefused> {
    match Ro::metadata(cwd) {
        Ok(m) if m.is_dir() && !m.permissions().readonly() => None,
        Ok(m) if m.is_dir() => Some(AttachRefused::NotWritable),
        _ => Some(AttachRefused::NoSession),
    }
}

/// **开传前那一问**：这一条草稿里的全部附件（按草稿里的先后）逐份答收不收。纯读。
/// 先判这一份自己（目录 · 单份上限），再判它放进这一条之后（份数 · 合计 · 附件目录还装不装得下）；不收的那一份不占这一条的份数与总量。
pub fn check(cwd: &Path, items: &[AttachItem]) -> Vec<AttachVerdict> {
    if let Some(why) = cwd_state(cwd) {
        return items.iter().map(|it| verdict(it, Some(why))).collect();
    }
    let used = dir_usage(cwd);
    let (mut count, mut sum) = (0usize, 0u64);
    items
        .iter()
        .map(|it| {
            let why = if it.dir {
                Some(AttachRefused::IsDir)
            } else if it.bytes > MAX_ONE_BYTES.bytes {
                Some(AttachRefused::TooBig)
            } else if count >= MAX_COUNT {
                Some(AttachRefused::TooMany)
            } else if sum + it.bytes > MAX_MESSAGE_BYTES.bytes {
                Some(AttachRefused::TooMuch)
            } else if used + sum + it.bytes > MAX_DIR_BYTES.bytes {
                Some(AttachRefused::DirFull)
            } else {
                None
            };
            if why.is_none() {
                count += 1;
                sum += it.bytes;
            }
            verdict(it, why)
        })
        .collect()
}

/// 盘上那一步没成 ⇒ 失败：权限不够 · 只读盘 ⇒ `not_writable`；别的 ⇒ `io_failed`（`said` 拿 `[(path, …), (why, …)]` 写那一句 ＋ 原话）。
fn io_fail(path: &Path, e: &std::io::Error, said: impl FnOnce(&str, &str) -> String) -> Said {
    if matches!(
        e.kind(),
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem
    ) {
        return refused(AttachRefused::NotWritable);
    }
    let at = path.display().to_string();
    let why = io_reason(e.kind());
    Said::with_raw(said(&at, &why), e).coded("io_failed")
}

fn path_fail(said: String) -> Said {
    Said::new("refused", said)
}

/// 附件目录在（不在就建）、里面那份 `.gitignore` 在（不在就写一行 `*`）。附件目录是一条链接或不是目录 ⇒ 拒，不往里写。
fn ensure_root(face: &Face, cwd: &Path) -> Result<PathBuf, Said> {
    let at = walk_in_root(Last::Link, cwd, DIR).map_err(path_fail)?;
    match Ro::symlink_metadata(&at) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            return Err(path_fail(copy_text(
                "beAttach.commit.notDir",
                &[("path", &at.display().to_string())],
            )));
        }
        Err(_) => match face.make_dir(&at) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => {
                return Err(io_fail(&at, &e, |at, why| {
                    copy_text("beAttach.commit.mkdirFailed", &[("path", at), ("why", why)])
                }));
            }
        },
    }
    let root = cwd.join(DIR);
    let ignore = walk_in_root(Last::Link, &root, GITIGNORE).map_err(path_fail)?;
    if Ro::symlink_metadata(&ignore).is_err() {
        use std::io::Write as _;
        let written = face
            .create_new(&ignore)
            .and_then(|mut f| f.write_all(GITIGNORE_BODY.as_bytes()));
        match written {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => {
                return Err(io_fail(&ignore, &e, |at, why| {
                    copy_text(
                        "beAttach.commit.ignoreFailed",
                        &[("path", at), ("why", why)],
                    )
                }));
            }
        }
    }
    Ok(root)
}

/// 那台钟上的月日时分（编号的前半）。
fn clock(at_secs: i64, tz: &crate::common::time::Tz) -> (u32, u32, u32, u32) {
    let local = tz.local(at_secs);
    let (_, month, day) = crate::common::time::civil_from_days(local.div_euclid(86_400));
    let secs = local.rem_euclid(86_400);
    let hm = |n: i64| u32::try_from(n).unwrap_or(0);
    (month, day, hm(secs / 3600), hm((secs % 3600) / 60))
}

/// 建一个新编号目录：尾巴从摘要的前四位起，撞了往后挪。回 (编号, 目录)。
fn mint(
    face: &Face,
    root: &Path,
    at: (u32, u32, u32, u32),
    sha256: &str,
) -> Result<(String, PathBuf), Said> {
    let seed = u16::from_str_radix(sha256.get(..4).unwrap_or("0"), 16).unwrap_or(0);
    let mut last = None;
    for k in 0..MINT_TRIES {
        let id = layout::id_of(at.0, at.1, at.2, at.3, seed.wrapping_add(k));
        let dir = walk_in_root(Last::Link, root, &id).map_err(path_fail)?;
        match face.make_dir(&dir) {
            Ok(()) => return Ok((id, dir.into_path_buf())),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = Some(dir),
            Err(e) => {
                return Err(io_fail(&dir, &e, |at, why| {
                    copy_text("beAttach.commit.mkdirFailed", &[("path", at), ("why", why)])
                }));
            }
        }
    }
    let at = last.map(|d| d.display().to_string()).unwrap_or_default();
    Err(Said::new(
        crate::files::write::EXISTS,
        copy_text("beFilesWrite.swap.exists", &[("path", &at)]),
    ))
}

/// **落一份**：判单份与附件目录上限 ⇒ 附件目录与 `.gitignore` 在 ⇒ 新编号目录 ⇒ 暂存件（或先把块拼起来）落进去、核整份摘要
/// （[`super::commit_upload_at`]，同上传提交那一条）⇒ 顺手清超过七天的。没落成 ⇒ 删掉刚建的那个空编号目录。
/// `home` 是暂存区所在的家；`at_secs` · `tz` 定编号前半（那台的钟）。
pub fn commit(
    face: &Face,
    home: &Path,
    cwd: &Path,
    req: SessionAttachmentCommitReq,
    at_secs: i64,
    tz: &crate::common::time::Tz,
) -> Result<SessionAttachmentCommitReply, Said> {
    if let Some(why) = cwd_state(cwd) {
        return Err(refused(why));
    }
    if req.bytes > MAX_ONE_BYTES.bytes {
        return Err(refused(AttachRefused::TooBig));
    }
    if dir_usage(cwd) + req.bytes > MAX_DIR_BYTES.bytes {
        return Err(refused(AttachRefused::DirFull));
    }
    let root = ensure_root(face, cwd)?;
    let sha = req.expect.sha256.0.clone();
    let (id, dir) = mint(face, &root, clock(at_secs, tz), &sha)?;
    let landed = super::commit_upload_at(
        face,
        home,
        cmd_wire::files::FilesCommitUploadReq {
            root: super::wire_of(&dir),
            rel: cmd_wire::files::WirePath(req.name.clone().into_bytes()),
            key: req.key.clone(),
            on_exists: None,
            expect: req.expect.clone(),
            chunks: req.chunks,
            bytes: Some(req.bytes),
            single: Some(true),
        },
        tz,
    );
    let landed = match landed {
        Ok(l) => l,
        Err(e) => {
            if let Ok(d) = walk_in_root(Last::Link, &root, &id) {
                if let Err(gone) = face.delete_dir(&d) {
                    tracing::warn!(
                        "[session-attachment-commit] 没落成，空编号目录 {} 没删掉：{gone}",
                        d.display()
                    );
                }
            }
            return Err(e.flat());
        }
    };
    sweep(face, cwd, u64::try_from(at_secs).unwrap_or(0), &id);
    let name = req.name;
    Ok(SessionAttachmentCommitReply {
        rel: layout::rel_of(&id, &name),
        path: crate::files::raw::to_path_buf(landed.path.bytes())
            .display()
            .to_string(),
        class: crate::common::file_class::class_of_name(name.as_bytes(), false),
        bytes: landed.bytes,
        sha256: cmd_wire::files::Sha256Hex(sha),
        id,
        name,
    })
}

/// [`commit`] 的生产那一份：暂存区在这台后端的家底下，编号前半取这台此刻的钟。
pub fn commit_here(
    face: &Face,
    cwd: &Path,
    req: SessionAttachmentCommitReq,
) -> Result<SessionAttachmentCommitReply, Said> {
    let home = super::home_dir()?;
    let tz = crate::common::time::Tz::here();
    commit(
        face,
        &home,
        cwd,
        req,
        crate::common::time::now_secs() as i64,
        &tz,
    )
}

/// **撤一份**：删掉那个编号目录（里面的普通文件 · 链接各删掉那一项，再删空目录）。本来就不在 ⇒ `false`。编号不合形 ⇒ `bad_args`。
pub fn drop_one(face: &Face, cwd: &Path, id: &str) -> Result<bool, Said> {
    if !layout::is_id(id) {
        return Err(Said::new(
            "bad_args",
            crate::common::contract::malformed(&format!("not an attachment id: {id:?}")),
        ));
    }
    let root = cwd.join(DIR);
    if Ro::symlink_metadata(&root).is_err() {
        return Ok(false);
    }
    let dir = walk_in_root(Last::Link, &root, id).map_err(path_fail)?;
    match Ro::symlink_metadata(&dir) {
        Ok(m) if m.is_dir() => {}
        Ok(_) | Err(_) => return Ok(false),
    }
    let inner = dir.to_path_buf();
    if let Ok(rd) = Ro::read_dir(&inner) {
        for entry in rd.flatten() {
            let Ok(at) = walk_in_root(Last::Link, &inner, entry.file_name()) else {
                continue;
            };
            if Ro::symlink_metadata(&at).is_ok_and(|m| !m.is_dir()) {
                face.delete_file(&at).map_err(|e| {
                    io_fail(&at, &e, |at, why| {
                        copy_text("beAttach.drop.failed", &[("path", at), ("why", why)])
                    })
                })?;
            }
        }
    }
    face.delete_dir(&dir).map_err(|e| {
        io_fail(&dir, &e, |at, why| {
            copy_text("beAttach.drop.failed", &[("path", at), ("why", why)])
        })
    })?;
    Ok(true)
}

/// **清理**：附件目录里修改时间早于 `now_secs - STALE_SECS` 的编号目录删掉（`keep` 那一个不碰）。回删掉的编号。尽力而为，删不掉不挡调用方。
pub fn sweep(face: &Face, cwd: &Path, now_secs: u64, keep: &str) -> Vec<String> {
    let root = cwd.join(DIR);
    let Ok(rd) = Ro::read_dir(&root) else {
        return Vec::new();
    };
    let mut gone = Vec::new();
    for entry in rd.flatten() {
        let Some(id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !layout::is_id(&id) || id == keep {
            continue;
        }
        let Some(mtime) = Ro::symlink_metadata(entry.path())
            .ok()
            .filter(|m| m.is_dir())
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        else {
            continue;
        };
        if now_secs.saturating_sub(mtime.as_secs()) > STALE_SECS
            && drop_one(face, cwd, &id).unwrap_or(false)
        {
            gone.push(id);
        }
    }
    gone
}

#[cfg(test)]
#[path = "../../../../tests/backend/files/session_attachment_tests.rs"]
mod tests;
