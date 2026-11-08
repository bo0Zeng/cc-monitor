//! **落进用户目录只有这一下**（[`land`]）与暂存区（`~/.cc-monitor/staging`）。
//!
//! 用到它的两块之间零互相依赖（文件管理那一块是独立的包，只引纯工具与平台两层；原生那一块的传输台也不引它），共用的只许住 `common/`：
//! - 文件管理那一块：覆盖写 · 读改写 · 复制 · 上传提交（`files/write.rs` · `files/commit.rs`）都经 [`land`] 换上内容；
//! - 原生那一块的传输台（`control/transfer.rs`）：下载先写进本机暂存区（续传的本钱也住那儿），传完经 [`land`] 上位（落点在别的盘上 ⇒ 退回旁名复制：本机多抄一整份，家目录那个盘先要容得下整份）。
//!
//! 暂存区本身的落点（家目录相对）住契约 crate（`relay_route_core::STAGING_DIR_REL`，家目录相对路径那一张表），这里转出；
//! 暂存件的键长与后缀两侧（造键的传输台 · 提交的那台后端）认同一份。

use crate::common::root_path::{walk_in_root, Last};
use crate::common::said::Said;
use crate::platform::fs::{Face, Perms, Resolved, Ro};
use copy_core::{copy_text, io_reason, Discard, Why};
use std::path::{Path, PathBuf};

/// 暂存区相对 `$HOME` 的那一段（值只住 `relay_route_core`）。
pub use relay_route_core::STAGING_DIR_REL as DIR;

/// 暂存件的键长（十六进制位数）。
pub const KEY_LEN: usize = 32;

/// 暂存件的后缀。
pub const PART_SUFFIX: &str = ".part";

/// 这台后端的家（暂存区拼在它底下）；说不出 ⇒ `io_failed`。
pub fn home() -> Result<PathBuf, Said> {
    crate::platform::paths::home_dir()
        .ok_or_else(|| Said::new("io_failed", copy_text("beFilesCommit.staging.noHome", &[])))
}

/// 暂存区在盘上的位置；不在就建（两层：`~/.cc-monitor` 与它底下的 `staging`）。
///
/// 每一层**先过路径解析**（以上一层为根）再问后端自家目录那一判（`Face::own_dir`：建的那一下 0700；已在必须是本人的真目录，
/// 放宽了 ⇒ 收回 0700；`~/.cc-monitor` 是链接 ⇒ 拒、出声，不往里写正文）。拒那一形 `raw` 为空。
pub fn ensure(face: &Face, home: &Path) -> Result<PathBuf, Said> {
    let mut at = home.to_path_buf();
    for seg in DIR.split('/') {
        let next = walk_in_root(Last::Link, &at, seg)?;
        face.own_dir(&next)?;
        at = next.into_path_buf();
    }
    Ok(at)
}

/// 一次 [`land`] 没成，是谁拦的（文件管理那一块照它换成自己的拒绝，传输台照它换成一句失败）。
#[derive(Debug)]
pub enum LandErr {
    /// 路径解析拒 · 形状不对（不是普通文件 · 没有名字）。
    Refused(String),
    /// 那一格要新建、却已经有一项。
    Exists(String),
    /// 那一格要换旧的、却已经不在了。
    Stale(String),
    /// 盘上这一步没成：句子（已带原因词）＋ 系统原话。
    Io { said: String, raw: String },
}

/// 只要一句失败的那一侧（传输台）：句子照交，原话跟着。
impl From<LandErr> for Said {
    fn from(e: LandErr) -> Said {
        match e {
            LandErr::Refused(m) | LandErr::Exists(m) | LandErr::Stale(m) => Said::from(m),
            LandErr::Io { said, raw } => Said::from((said, Some(raw))),
        }
    }
}

impl LandErr {
    fn io(said: String, e: &std::io::Error) -> LandErr {
        LandErr::Io {
            said,
            raw: e.to_string(),
        }
    }
}

/// [`land`] 换上去的内容从哪来。
pub enum Content<'a> {
    Bytes(&'a [u8]),
    /// 一份已在盘上的普通文件（复制的源），不跟链接地读。
    File(&'a Path),
    /// 暂存件（上传的 · 下载的）：同盘时它自己改名上位（不复制），跨盘退回复制；成了之后它就没了。
    Staged(&'a Resolved),
}

/// 那一格原来在不在：调用方判过的那一面；对不上 ⇒ 不动（`New` 而已在 ⇒ `Exists`；`Old` 而不在 ⇒ `Stale`）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Was {
    New,
    Old,
    Either,
}

/// 旁名序号（同一进程连写两次也换一个旁名；写面的备份名也取它）。
pub static SWAP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 这台机器上后端自家目录的权限位：新建在 `~/.cc-monitor` 里的文件只给本人读写。
const OWN_FILE_MODE: u32 = 0o600;

/// **新建**在这台机器自家目录（`$HOME/.cc-monitor` 底下，那个目录是后端认的本人真目录：`platform::fs::inside_own_dir`）里的东西只给本人：
/// 文件 [`OWN_FILE_MODE`] · 目录 `own_dir::PRIVATE_DIR_MODE`（unix；别的平台照常建，那边按父目录继承 ACL）。只管新建：覆盖写沿用原权限位、复制照抄源的。
/// `at` 是过了路径解析的那一条（父目录已解开）。不在自家目录底下 ⇒ 不收紧。
pub fn own_mode(at: &Path, dir: bool) -> Option<Perms> {
    if !crate::platform::fs::inside_own_dir(at) {
        return None;
    }
    let mode = if dir {
        crate::common::own_dir::PRIVATE_DIR_MODE
    } else {
        OWN_FILE_MODE
    };
    Ro::perms_from_mode(mode)
}

/// 一份文件的 `(硬链接数, 属主 uid)`（跟链接地看）。**非 unix 上没有这两个概念 ⇒ `None`**（那边照原子换 / Windows 臂办）。
#[cfg(unix)]
fn links_and_owner(p: &Path) -> Option<(u64, u32)> {
    let m = Ro::metadata(p).ok()?;
    Some((Ro::nlink(&m)?, Ro::uid(&m)?))
}

#[cfg(not(unix))]
fn links_and_owner(_p: &Path) -> Option<(u64, u32)> {
    None
}

/// 这一份目标**不该原子换**的原因（`None` = 该原子换）：换名上位会让 inode 换掉 ⇒
/// 有硬链接（`links > 1`）的另一个名字仍指旧内容；属主不是后端这个用户（只是给了写权限）⇒ 换完属主变成后端用户。
pub fn in_place_reason(links: u64, owner: u32, me: u32) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if links > 1 {
        parts.push(format!("它有 {links} 个硬链接"));
    }
    if owner != me {
        parts.push(format!(
            "它的属主（uid {owner}）不是后端这个用户（uid {me}）"
        ));
    }
    (!parts.is_empty()).then(|| parts.join("、"))
}

/// **落进用户目录**：换掉 `root ＋ rel` 那一份文件的内容（不在就新建）。回 `(落点, 字节数)`。
///
/// - 已在：解到底改真文件（经链接跑出根 ⇒ 拒）；权限位沿用它；有硬链接 / 属主不是后端这个用户 / Windows（换名会换掉 ACL）⇒
///   就地写（截断再写 ＋ 落盘，记一行「这一次不是原子写」）。xattr 不跟过来是已知代价。
/// - 不在：权限位取 `fresh`，缺 ⇒ 落在后端自家目录里的收紧、别处按 umask。上位走不覆盖改名。
/// - 原子那一支：同目录 `O_EXCL` 旁名 → 先定权限位 → 写满 → 落盘 → 上位 → 目录落盘；失败删掉自己的旁名，目标不动。
///   暂存件同盘时省掉旁名（定权限位 ＋ 落盘之后它自己上位）；跨盘（改名回 `CrossesDevices`）退回旁名那一支。
///
/// `between`：写满之后、上位之前（判据插竞争用；生产传空）。
pub fn land(
    face: &Face,
    root: &Path,
    rel: &Path,
    content: Content<'_>,
    fresh: Option<Perms>,
    was: Was,
    between: &mut dyn FnMut(),
) -> Result<(PathBuf, u64), LandErr> {
    let unread = |at: &Path, e: std::io::Error| {
        let at = at.display().to_string();
        let said = copy_text(
            "reason.fs.readFailed",
            &[("path", &at), ("why", &io_reason(e.kind()))],
        );
        LandErr::io(said, &e)
    };
    let unwritten = |at: &Path, e: std::io::Error| {
        let at = at.display().to_string();
        let said = copy_text(
            "reason.fs.writeFailed",
            &[("path", &at), ("why", &io_reason(e.kind()))],
        );
        LandErr::io(said, &e)
    };
    let at = walk_in_root(Last::Link, root, rel).map_err(LandErr::Refused)?;
    let exists = match Ro::symlink_metadata(&at) {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(unread(&at, e)),
    };
    match (was, exists) {
        (Was::New, true) => {
            return Err(LandErr::Exists(copy_text(
                "beFilesWrite.swap.exists",
                &[("path", &at.display().to_string())],
            )))
        }
        (Was::Old, false) => {
            return Err(LandErr::Stale(copy_text(
                "beFilesWrite.overwrite.gone",
                &[("path", &at.display().to_string())],
            )))
        }
        _ => {}
    }
    let (dst, perms, in_place) = if exists {
        let real = walk_in_root(Last::Follow, root, rel).map_err(LandErr::Refused)?;
        let md = Ro::metadata(&real).map_err(|e| unread(&real, e))?;
        if !md.is_file() {
            return Err(LandErr::Refused(copy_text(
                "beFilesWrite.overwrite.notRegular",
                &[("path", &real.display().to_string())],
            )));
        }
        let why = links_and_owner(&real).and_then(|(links, owner)| {
            in_place_reason(links, owner, crate::platform::paths::current_uid())
        });
        if cfg!(windows) {
            tracing::warn!(
                "换内容 {}：Windows 上换名会换掉 ACL —— 就地写",
                real.display()
            );
        } else if let Some(why) = &why {
            tracing::warn!(
                "换内容 {}：这一次不是原子写，因为{why} —— 改成就地写",
                real.display()
            );
        }
        let in_place = cfg!(windows) || why.is_some();
        (real, Some(md.permissions()), in_place)
    } else {
        let perms = fresh.or_else(|| own_mode(&at, false));
        (at, perms, false)
    };
    let staged_gone = |staged: &Resolved| {
        // 内容已经落进目标：暂存件删不掉不算失败（孤儿扫按期限收）。
        face.delete_file(staged).discard(Why::CleanupAfterFailure);
    };
    if in_place {
        let mut f = face.overwrite(&dst).map_err(|e| unwritten(&dst, e))?;
        let n = pour(&content, &mut f)
            .and_then(|n| f.sync_all().map(|()| n))
            .map_err(|e| unwritten(&dst, e))?;
        if let Content::Staged(staged) = content {
            staged_gone(staged);
        }
        return Ok((dst.into_path_buf(), n));
    }
    if let Content::Staged(staged) = &content {
        let n = Ro::symlink_metadata(staged)
            .map_err(|e| unread(staged, e))?
            .len();
        let ready = perms
            .clone()
            .map_or(Ok(()), |p| face.set_permissions(staged, p))
            .and_then(|()| face.sync(staged));
        if let Err(e) = ready {
            return Err(unwritten(staged, e));
        }
        between();
        match place(face, staged, &dst, was == Was::New) {
            Ok(()) => {
                face.sync_dir(&dst).discard(Why::CleanupAfterFailure); // 内容已落盘、上位已成：不成只是断电时可能回到旧名，不为它把已成的一趟报成失败
                return Ok((dst.into_path_buf(), n));
            }
            Err(e) if e.kind() != std::io::ErrorKind::CrossesDevices => {
                return Err(placed_err(&dst, e))
            }
            Err(_) => {}
        }
    }
    let name = dst.file_name().ok_or_else(|| {
        LandErr::Refused(copy_text(
            "beFilesWrite.path.noName",
            &[("path", &rel.display().to_string())],
        ))
    })?;
    let seq = SWAP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // 名字可以不是 UTF-8 ⇒ 旁名按 `OsString` 拼。
    let side_name = crate::platform::fs::side_name::swap(name, std::process::id(), seq);
    let side = dst.sibling(&side_name).ok_or_else(|| {
        LandErr::Refused(copy_text(
            "beFilesWrite.path.noName",
            &[("path", &rel.display().to_string())],
        ))
    })?;
    let mut f = face.create_new(&side).map_err(|e| unwritten(&side, e))?;
    // 权限位在写进内容之前就定：旁名上也不留一刻宽的。
    let written = perms
        .map_or(Ok(()), |p| f.set_permissions(p))
        .and_then(|()| pour(&content, &mut f))
        .and_then(|n| f.sync_all().map(|()| n));
    drop(f);
    let n = match written {
        Ok(n) => n,
        Err(e) => {
            face.delete_file(&side).discard(Why::CleanupAfterFailure);
            return Err(unwritten(&side, e));
        }
    };
    between();
    if let Err(e) = place(face, &side, &dst, was == Was::New) {
        face.delete_file(&side).discard(Why::CleanupAfterFailure);
        return Err(placed_err(&dst, e));
    }
    face.sync_dir(&dst).discard(Why::CleanupAfterFailure); // 同上：上位已成
    if let Content::Staged(staged) = content {
        staged_gone(staged);
    }
    Ok((dst.into_path_buf(), n))
}

/// 把内容倒进一份已开的文件，回字节数。
fn pour(content: &Content<'_>, f: &mut crate::platform::fs::FaceFile) -> std::io::Result<u64> {
    use std::io::Write as _;
    match content {
        Content::Bytes(b) => f.write_all(b).map(|()| b.len() as u64),
        Content::File(p) => std::io::copy(&mut Ro::open_nofollow(p)?, f),
        Content::Staged(s) => std::io::copy(&mut Ro::open_nofollow(s)?, f),
    }
}

/// 上位：`new` ⇒ 不覆盖改名（盘不认那个旗 ⇒ 普通文件走 `link` ＋ `unlink`），那一格已有东西 ⇒ `AlreadyExists`；否则换名顶掉。
fn place(face: &Face, from: &Resolved, to: &Resolved, new: bool) -> std::io::Result<()> {
    if !new {
        return face.move_file(from, to);
    }
    match face.move_noreplace(from, to) {
        Err(e) if crate::platform::fs::noreplace_unsupported(&e) => face.move_by_link(from, to),
        other => other,
    }
}

fn placed_err(dst: &Path, e: std::io::Error) -> LandErr {
    if e.kind() == std::io::ErrorKind::AlreadyExists {
        return LandErr::Exists(copy_text(
            "beFilesWrite.swap.exists",
            &[("path", &dst.display().to_string())],
        ));
    }
    LandErr::io(
        copy_text(
            "reason.fs.writeFailed",
            &[
                ("path", &dst.display().to_string()),
                ("why", &io_reason(e.kind())),
            ],
        ),
        &e,
    )
}
