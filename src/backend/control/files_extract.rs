//! 解压（`files-extract`）＋ 「建链接」那一个动词（`Face::make_link`）的调用方。
//!
//! - 复制链接本身（不跟进去，目标文本原样；= GNU `cp -R` 缺省的 `-P`）⇒ [`land_link`]（复制目录与解压共用）。
//! - 「解压到这里」：用 Rust 库（`tar` · `flate2` · `zip`，不调外部命令）解到同目录下以包名命名的新目录；每一条先过路径解析
//!   （挡 zip-slip：`..` / 绝对路径 / 链接出根），撞名就问；支持 zip · tar · tar.gz · tgz，其余格式说「不认这种包」。
//!
//! # 两趟，照复制目录（`files_write::copy_tree`）
//!
//! 1. 计划（只读，[`plan`]）：逐条目判路径 —— 只许普通段（`..` / 绝对 / 盘符 ⇒ 整趟拒；`.` 与空段剥掉）；链接的目标必须相对、
//!    且从它所在的目录起算词法上不出落点；硬链接只许指向包里前面出现过的普通文件（落成一份拷贝）；
//!    设备 / 管道 ⇒ 整趟拒；条目挂在包里一条链接或一份文件底下 ⇒ 整趟拒；同名文件两次 ⇒ 整趟拒；条目数 ≤ `TREE_ENTRY_CAP`。
//!    包里没写出来的上级目录补成目录。一个改动都没有。
//! 2. 执行：建落点目录（已在 ⇒ `exists`，一个字节不动）→ 目录 → 文件（`O_EXCL` 新建 · 写满 · 权限位只取 rwx 低 9 位）→
//!    硬链接（拷贝）→ 链接（最后建：先建的话后面的文件会穿过它写）；每一条当场过 [`resolve_in_root`]。
//!    中途失败 ⇒ 倒序撤掉这一趟自己建的（含落点目录）。
//!
//! 解出的字节之和有上限（[`Caps`]：包的 [`EXPAND_RATIO_CAP`] 倍，小包至少 [`EXPAND_FLOOR_BYTES`]）：计划趟按包里写的大小先判，
//! 执行趟按真写出的字节再判（包里写的大小可以撒谎）；超了整趟拒、撤掉已建的。
//!
//! 认下的：修改时间 / 扩展属性 / 属主 / 特殊权限位都不还原。库自带的「一步解到盘上」不用（不经我们的路径解析）。

use super::files_write::{
    resolve_existing_in_root, resolve_in_root, wire_of, Act, ManageCommand, WriteFail,
    WriteRefusal, TREE_ENTRY_CAP,
};
use crate::platform::fs::{Face, Ro, RoFile};
use cmd_wire::files as wire;
use copy_core::{copy_text, io_reason};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

//
// ══════════════════════════════════════════════════════════════════════════
// 建链接（复制目录与解压共用）
// ══════════════════════════════════════════════════════════════════════════

/// zip 里一条链接的目标文本最多多长（Linux `PATH_MAX` 同量级）；超了整趟拒、不截断。
pub const LINK_TARGET_MAX_BYTES: u64 = 4096;

/// 解出的字节之和最多是包本身的几倍。量过：源码 · 依赖 · 编译产物打的 tar.gz 展开 3–5 倍、日志约 10 倍，全零的 zip 约 1000 倍。
pub const EXPAND_RATIO_CAP: u64 = 100;

/// 小包不按倍数卡：解出的字节之和在这以内总放行。
pub const EXPAND_FLOOR_BYTES: u64 = 1 << 30;

/// 一趟解压的两条上限：条目数 ＋ 解出的字节之和（包的 `ratio` 倍、至少 `floor`）。
#[derive(Clone, Copy, Debug)]
pub struct Caps {
    pub entries: usize,
    pub ratio: u64,
    pub floor: u64,
}

/// 线上那条命令用的上限。
pub const CAPS: Caps = Caps {
    entries: TREE_ENTRY_CAP,
    ratio: EXPAND_RATIO_CAP,
    floor: EXPAND_FLOOR_BYTES,
};

impl Caps {
    /// 这个包最多解出多少字节。
    fn bytes_for(&self, archive_len: u64) -> u64 {
        archive_len.saturating_mul(self.ratio).max(self.floor)
    }
}

fn over_bytes(cap: u64) -> Fail {
    refused(copy_text(
        "beFilesExtract.plan.overBytes",
        &[("cap", &copy_core::size_text(cap))],
    ))
}

/// 这个平台建不建得了链接（计划趟据此决定「链接 ⇒ 记下来」还是「链接 ⇒ 整趟拒」）。
pub const LINKS_SUPPORTED: bool = cfg!(unix);

/// 在 `root ＋ rel` 上建一条链接，目标文本 ＝ `target`（原样，不解、不判 —— 它指到哪是它的内容，同 `cp -P`）。
/// 链接自己那条路径先过 [`resolve_in_root`]；已在（含一条链接）⇒ 系统拒，一个字节不动。
#[cfg(unix)]
pub fn land_link(
    face: &Face,
    root: &Path,
    rel: &Path,
    target: &Path,
) -> Result<PathBuf, WriteRefusal> {
    let at = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    face.make_link(target, &at).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "reason.fs.writeFailed",
                &[
                    ("path", &at.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    Ok(at.into_path_buf())
}

/// 非 unix：没有「建链接」这个动词（计划趟看 [`LINKS_SUPPORTED`] 已整趟拒，走不到这里；兜底仍说清）。
#[cfg(not(unix))]
pub fn land_link(
    _face: &Face,
    root: &Path,
    rel: &Path,
    _target: &Path,
) -> Result<PathBuf, WriteRefusal> {
    let at = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    Err(WriteRefusal::Refused(copy_text(
        "beFilesLink.land.unsupported",
        &[("path", &at.display().to_string())],
    )))
}

//
// ══════════════════════════════════════════════════════════════════════════
// 解压
// ══════════════════════════════════════════════════════════════════════════

/// 认得的包（按名字后缀，不分大小写）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Zip,
    Tar,
    TarGz,
}

/// 后缀 → 种类；认不得 ⇒ `None`（线上 `unsupported`「不认这种包」）。`.tar.gz` 先于 `.gz` 判（单 `.gz` 不是包）。
pub fn kind_of(name: &[u8]) -> Option<Kind> {
    let low: Vec<u8> = name.to_ascii_lowercase();
    let ends = |s: &str| low.ends_with(s.as_bytes()) && low.len() > s.len();
    if ends(".zip") {
        Some(Kind::Zip)
    } else if ends(".tar.gz") || ends(".tgz") {
        Some(Kind::TarGz)
    } else if ends(".tar") {
        Some(Kind::Tar)
    } else {
        None
    }
}

/// 包名去掉认得的那个后缀（落点目录的缺省名；窗口用同一个口径显示，但名字由窗口定、这里只给判据用）。
pub fn stem_of(name: &[u8]) -> Option<Vec<u8>> {
    let low = name.to_ascii_lowercase();
    [".tar.gz", ".tgz", ".tar", ".zip"]
        .iter()
        .find(|s| low.ends_with(s.as_bytes()) && low.len() > s.len())
        .map(|s| name[..name.len() - s.len()].to_vec())
}

/// 计划里的一条：相对落点的那一段 ＋ 它是什么 ＋ 它在包里的序号（执行趟按序号认回来）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum What {
    Dir,
    /// 普通文件；`mode` = 包里写的权限位（只留 rwx 低 9 位；没写 ⇒ 进程缺省）。
    File {
        index: usize,
        mode: Option<u32>,
        /// 包里写的大小（计划趟据它先判总量；执行趟按真写出的再判）。
        size: u64,
    },
    /// 符号链接，目标文本原样。
    Link(PathBuf),
    /// 硬链接：指向包里前面那一份普通文件（相对落点）⇒ 落成一份拷贝。
    Copy(PathBuf),
}

/// 一次解压的计划。
#[derive(Debug, Default)]
pub struct Plan {
    /// 按相对段排好：父目录恒在子项之前（`Path` 的比较按段）。
    pub entries: BTreeMap<PathBuf, What>,
    /// 包里写的、要解出的字节之和（普通文件 ＋ 硬链接落成的拷贝）。
    pub declared: u64,
}

/// 一趟解压做了什么。
#[derive(Debug, PartialEq, Eq)]
pub struct Extracted {
    pub path: PathBuf,
    pub files: usize,
    pub dirs: usize,
    pub links: usize,
    pub bytes: u64,
}

type Fail = WriteFail;

fn refused(said: String) -> Fail {
    WriteFail::from(("refused", said))
}

/// 盘上 / 包里这一步没成：句子带原因词，下层原话进复制详情。
fn io_failed(said: String, raw: impl std::fmt::Display) -> Fail {
    WriteFail {
        code: "io_failed",
        said,
        raw: Some(raw.to_string()),
    }
}

fn shown(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// 包里一条名字（原始字节）→ 相对落点的一段。空段与 `.` 剥掉；`..` · 绝对 · 盘符 ⇒ 拒；全剥光 ⇒ `Ok(None)`（落点自己）。
/// zip 规范只用 `/`，Windows 上打的包常写 `\`，照 7-Zip / Python 的读法当分隔符；tar 里 `\` 只在落点是 Windows 时才是分隔符。
pub fn entry_rel(name: &[u8], zip: bool) -> Result<Option<PathBuf>, String> {
    use ssh_dial::plain_path::{plain_segments, Dots, Fault};
    match plain_segments(name, zip || cfg!(windows), Dots::Strip) {
        Ok(segs) if segs.is_empty() => Ok(None),
        Ok(segs) => Ok(Some(crate::files::raw::to_path_buf(&segs.join(&b'/')))),
        Err(Fault::Parent) => Err(copy_text(
            "beFilesExtract.path.upward",
            &[("name", &shown(name))],
        )),
        Err(Fault::Absolute | Fault::Dot) => Err(copy_text(
            "beFilesExtract.path.absolute",
            &[("name", &shown(name))],
        )),
    }
}

/// 链接目标从链接所在目录起算，词法上不出落点（相对 · 不以 `/` 起头 · 逐段 `..` 不越过落点）。
pub fn link_stays_inside(link_rel: &Path, target: &[u8]) -> bool {
    if target.is_empty() || target.first() == Some(&b'/') {
        return false;
    }
    let mut depth: i64 = link_rel.components().count() as i64 - 1;
    for seg in target.split(|b| *b == b'/') {
        match seg {
            b"" | b"." => {}
            b".." => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => depth += 1,
        }
    }
    true
}

/// 包里一条条目的种类（两种包各自翻成这一个口径）。
enum Raw {
    Dir,
    File(Option<u32>),
    Link(Vec<u8>),
    Hard(Vec<u8>),
    Skip,
    /// 设备 / 管道之类：带着「它是什么」那几个字（已经从文案表取好）。
    Other(String),
}

/// 把一条条目并进计划。
fn admit(
    plan: &mut Plan,
    index: usize,
    name: &[u8],
    raw: Raw,
    zip: bool,
    size: u64,
) -> Result<(), Fail> {
    let rel = match entry_rel(name, zip) {
        Ok(Some(r)) => r,
        Ok(None) => return Ok(()),
        Err(m) => return Err(refused(m)),
    };
    let what = match raw {
        Raw::Skip => return Ok(()),
        Raw::Dir => What::Dir,
        Raw::File(mode) => What::File {
            index,
            mode: mode.map(|m| m & 0o777),
            size,
        },
        Raw::Link(to) => {
            if !LINKS_SUPPORTED {
                return Err(refused(copy_text(
                    "beFilesExtract.entry.linkUnsupported",
                    &[("name", &shown(name))],
                )));
            }
            if !link_stays_inside(&rel, &to) {
                return Err(refused(copy_text(
                    "beFilesExtract.entry.linkEscapes",
                    &[("name", &shown(name)), ("to", &shown(&to))],
                )));
            }
            What::Link(crate::files::raw::to_path_buf(&to))
        }
        Raw::Hard(to) => {
            let target = match entry_rel(&to, zip) {
                Ok(Some(t)) => t,
                _ => {
                    return Err(refused(copy_text(
                        "beFilesExtract.entry.hardEscapes",
                        &[("name", &shown(name)), ("to", &shown(&to))],
                    )))
                }
            };
            let Some(What::File { size, .. }) = plan.entries.get(&target) else {
                return Err(refused(copy_text(
                    "beFilesExtract.entry.hardEscapes",
                    &[("name", &shown(name)), ("to", &shown(&to))],
                )));
            };
            plan.declared = plan.declared.saturating_add(*size);
            What::Copy(target)
        }
        Raw::Other(what) => {
            return Err(refused(copy_text(
                "beFilesExtract.entry.special",
                &[("name", &shown(name)), ("what", &what)],
            )))
        }
    };
    if let What::File { size, .. } = what {
        plan.declared = plan.declared.saturating_add(size);
    }
    match (plan.entries.get(&rel), &what) {
        (None, _) => {
            plan.entries.insert(rel, what);
        }
        (Some(What::Dir), What::Dir) => {}
        (Some(_), _) => {
            return Err(refused(copy_text(
                "beFilesExtract.entry.twice",
                &[("name", &shown(name))],
            )));
        }
    }
    Ok(())
}

/// 收尾：补包里没写出来的上级目录；条目挂在一条链接 / 一份文件底下 ⇒ 拒（挡「先放一条指到别处的链接、再往它底下写」）。
fn close_plan(plan: &mut Plan, cap: usize) -> Result<(), Fail> {
    let mut parents: BTreeSet<PathBuf> = BTreeSet::new();
    for rel in plan.entries.keys() {
        let mut up = rel.parent();
        while let Some(p) = up.filter(|p| !p.as_os_str().is_empty()) {
            parents.insert(p.to_path_buf());
            up = p.parent();
        }
    }
    for p in parents {
        match plan.entries.get(&p) {
            None => {
                plan.entries.insert(p, What::Dir);
            }
            Some(What::Dir) => {}
            Some(_) => {
                return Err(refused(copy_text(
                    "beFilesExtract.entry.underNonDir",
                    &[("name", &p.display().to_string())],
                )))
            }
        }
    }
    if plan.entries.len() > cap {
        return Err(refused(copy_text(
            "beFilesExtract.plan.overCap",
            &[("cap", &cap.to_string())],
        )));
    }
    Ok(())
}

fn open_archive(path: &Path) -> Result<RoFile, Fail> {
    Ro::open_nofollow(path).map_err(|e| {
        io_failed(
            copy_text(
                "beFilesExtract.archive.unreadable",
                &[
                    ("path", &path.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            e,
        )
    })
}

fn broken(path: &Path, e: impl std::fmt::Display) -> Fail {
    io_failed(
        copy_text(
            "beFilesExtract.archive.broken",
            &[("path", &path.display().to_string())],
        ),
        e,
    )
}

fn tar_raw<R: Read>(e: &tar::Entry<'_, R>) -> Raw {
    use tar::EntryType as T;
    let h = e.header();
    match h.entry_type() {
        T::Directory => Raw::Dir,
        T::Regular | T::Continuous | T::GNUSparse => Raw::File(h.mode().ok()),
        T::Symlink => Raw::Link(
            e.link_name_bytes()
                .map(|c| c.into_owned())
                .unwrap_or_default(),
        ),
        T::Link => Raw::Hard(
            e.link_name_bytes()
                .map(|c| c.into_owned())
                .unwrap_or_default(),
        ),
        T::XGlobalHeader | T::XHeader | T::GNULongName | T::GNULongLink => Raw::Skip,
        T::Char | T::Block => Raw::Other(copy_text("beFilesExtract.kind.device", &[])),
        T::Fifo => Raw::Other(copy_text("beFilesExtract.kind.fifo", &[])),
        _ => Raw::Other(copy_text("beFilesExtract.kind.other", &[])),
    }
}

fn tar_reader(kind: Kind, f: RoFile) -> Box<dyn Read> {
    let buf = std::io::BufReader::new(f);
    match kind {
        Kind::TarGz => Box::new(flate2::read::GzDecoder::new(buf)),
        _ => Box::new(buf),
    }
}

/// **计划趟（只读）**：读一遍包，逐条目判。一个改动都没有。
pub fn plan(archive: &Path, kind: Kind, cap: usize, bytes_cap: u64) -> Result<Plan, Fail> {
    let mut plan = Plan::default();
    let mut count = 0usize;
    match kind {
        Kind::Zip => {
            let mut z =
                zip::ZipArchive::new(open_archive(archive)?).map_err(|e| broken(archive, e))?;
            for i in 0..z.len() {
                count += 1;
                if count > cap {
                    return Err(refused(copy_text(
                        "beFilesExtract.plan.overCap",
                        &[("cap", &cap.to_string())],
                    )));
                }
                let mut f = z.by_index(i).map_err(|e| broken(archive, e))?;
                let name = f.name().as_bytes().to_vec();
                let raw = if f.is_dir() {
                    Raw::Dir
                } else if f.is_symlink() {
                    // zip 里链接的目标文本是那一条的正文；多读一个字节判超限（超了整趟拒，不截一半当目标）。
                    let mut to = Vec::new();
                    f.by_ref()
                        .take(LINK_TARGET_MAX_BYTES + 1)
                        .read_to_end(&mut to)
                        .map_err(|e| broken(archive, e))?;
                    if to.len() as u64 > LINK_TARGET_MAX_BYTES {
                        return Err(refused(copy_text(
                            "beFilesExtract.entry.linkTooLong",
                            &[
                                ("name", &shown(&name)),
                                ("cap", &LINK_TARGET_MAX_BYTES.to_string()),
                            ],
                        )));
                    }
                    Raw::Link(to)
                } else {
                    Raw::File(f.unix_mode())
                };
                let size = f.size();
                admit(&mut plan, i, &name, raw, true, size)?;
            }
        }
        Kind::Tar | Kind::TarGz => {
            let mut a = tar::Archive::new(tar_reader(kind, open_archive(archive)?));
            for (i, e) in a.entries().map_err(|e| broken(archive, e))?.enumerate() {
                count += 1;
                if count > cap {
                    return Err(refused(copy_text(
                        "beFilesExtract.plan.overCap",
                        &[("cap", &cap.to_string())],
                    )));
                }
                let e = e.map_err(|e| broken(archive, e))?;
                let name = e.path_bytes().into_owned();
                let raw = tar_raw(&e);
                admit(&mut plan, i, &name, raw, false, e.size())?;
            }
        }
    }
    close_plan(&mut plan, cap)?;
    if plan.declared > bytes_cap {
        return Err(over_bytes(bytes_cap));
    }
    Ok(plan)
}

/// 落一份普通文件：先过路径解析 → `O_EXCL` 新建 → 写满 → 权限位（给了才设）。回写了几个字节。
fn land_file(
    face: &Face,
    root: &Path,
    rel: &Path,
    body: &mut dyn Read,
    mode: Option<u32>,
    left: &mut u64,
) -> Result<u64, Fail> {
    let at = resolve_in_root(root, rel).map_err(|m| ("refused", m))?;
    let fail = |e: std::io::Error| {
        io_failed(
            copy_text(
                "reason.fs.writeFailed",
                &[
                    ("path", &at.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            e,
        )
    };
    let mut out = face.create_new(&at).map_err(fail)?;
    // 写一半 / 设权限没成 ⇒ 删掉**我们自己刚建的那一份**（`O_EXCL` 保证它此前不存在），它还没进回滚那张表。
    // 权限位按句柄改（没有路径可被换成链接）。
    let cap = *left;
    let done = std::io::copy(&mut body.take(cap.saturating_add(1)), &mut out).and_then(|n| {
        #[cfg(unix)]
        if let Some(p) = mode.and_then(Ro::perms_from_mode) {
            out.set_permissions(p)?;
        }
        #[cfg(not(unix))]
        let _ = mode;
        Ok(n)
    });
    drop(out);
    match done {
        Ok(n) if n <= cap => {
            *left = cap - n;
            Ok(n)
        }
        Ok(_) => {
            face.delete_file(&at).ok();
            Err(over_bytes(cap))
        }
        Err(e) => {
            face.delete_file(&at).ok();
            Err(fail(e))
        }
    }
}

/// 建一个目录（先过路径解析）。
fn land_dir(face: &Face, root: &Path, rel: &Path) -> Result<(), Fail> {
    let at = resolve_in_root(root, rel).map_err(|m| ("refused", m))?;
    face.make_dir(&at).map_err(|e| {
        io_failed(
            copy_text(
                "reason.fs.writeFailed",
                &[
                    ("path", &at.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            e,
        )
    })
}

/// 回滚：倒序删掉这一趟**自己建的**（每条先过路径解析）。回撤不掉的那一条的说法 ＋ 系统原话（有的话）。
fn undo(face: &Face, root: &Path, made: &[(PathBuf, bool)]) -> Option<(String, Option<String>)> {
    for (rel, is_dir) in made.iter().rev() {
        let at = match resolve_in_root(root, rel) {
            Ok(a) => a,
            Err(m) => return Some((m, None)),
        };
        let done = if *is_dir {
            face.delete_dir(&at)
        } else {
            face.delete_file(&at)
        };
        if let Err(e) = done {
            return Some((
                copy_text(
                    "beFilesExtract.undo.at",
                    &[
                        ("path", &at.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                Some(e.to_string()),
            ));
        }
    }
    None
}

/// 执行趟：照计划落（`into` 相对 `root`，已由调用方建好、记进 `made`）。
fn execute(
    face: &Face,
    root: &Path,
    into: &Path,
    archive: &Path,
    kind: Kind,
    plan: &Plan,
    made: &mut Vec<(PathBuf, bool)>,
    bytes_cap: u64,
) -> Result<u64, Fail> {
    let mut left = bytes_cap;
    for (rel, w) in &plan.entries {
        if *w == What::Dir {
            land_dir(face, root, &into.join(rel))?;
            made.push((into.join(rel), true));
        }
    }
    let files: BTreeMap<usize, (&PathBuf, Option<u32>)> = plan
        .entries
        .iter()
        .filter_map(|(r, w)| match w {
            What::File { index, mode, .. } => Some((*index, (r, *mode))),
            _ => None,
        })
        .collect();
    let mut bytes = 0u64;
    match kind {
        Kind::Zip => {
            let mut z =
                zip::ZipArchive::new(open_archive(archive)?).map_err(|e| broken(archive, e))?;
            for (i, (rel, mode)) in &files {
                let mut f = z.by_index(*i).map_err(|e| broken(archive, e))?;
                bytes += land_file(face, root, &into.join(rel), &mut f, *mode, &mut left)?;
                made.push((into.join(rel), false));
            }
        }
        Kind::Tar | Kind::TarGz => {
            let mut a = tar::Archive::new(tar_reader(kind, open_archive(archive)?));
            for (i, e) in a.entries().map_err(|e| broken(archive, e))?.enumerate() {
                let Some((rel, mode)) = files.get(&i) else {
                    continue;
                };
                let mut e = e.map_err(|e| broken(archive, e))?;
                bytes += land_file(face, root, &into.join(rel), &mut e, *mode, &mut left)?;
                made.push((into.join(rel), false));
            }
        }
    }
    for (rel, w) in &plan.entries {
        if let What::Copy(from) = w {
            let src =
                resolve_existing_in_root(root, into.join(from)).map_err(|m| ("refused", m))?;
            let mut body = open_archive(&src)?;
            let mode = match plan.entries.get(from) {
                Some(What::File { mode, .. }) => *mode,
                _ => None,
            };
            bytes += land_file(face, root, &into.join(rel), &mut body, mode, &mut left)?;
            made.push((into.join(rel), false));
        }
    }
    for (rel, w) in &plan.entries {
        if let What::Link(to) = w {
            land_link(face, root, &into.join(rel), to)
                .map_err(|e| (e.code(), e.message().to_string()))?;
            made.push((into.join(rel), false));
        }
    }
    Ok(bytes)
}

/// **解压**：`root ＋ rel` 那个包解到 `root ＋ into`（一个此前不存在的新目录）。
pub fn extract(face: &Face, root: &Path, rel: &Path, into: &Path) -> Result<Extracted, Fail> {
    extract_with(face, root, rel, into, CAPS)
}

/// [`extract`] 的本体，上限由调用方给（判据拿小上限验「超了整趟拒」）。
pub fn extract_with(
    face: &Face,
    root: &Path,
    rel: &Path,
    into: &Path,
    caps: Caps,
) -> Result<Extracted, Fail> {
    let name = crate::files::raw::path_bytes(rel);
    let base = name.rsplit(|b| *b == b'/').next().unwrap_or(name);
    let kind = kind_of(base).ok_or_else(|| {
        (
            "unsupported",
            copy_text(
                "beFilesExtract.archive.unknownKind",
                &[("name", &shown(base))],
            ),
        )
    })?;
    let archive = resolve_existing_in_root(root, rel).map_err(|m| ("refused", m))?;
    let dest = resolve_in_root(root, into).map_err(|m| ("refused", m))?;
    if Ro::symlink_metadata(&dest).is_ok() {
        return Err(WriteFail::from((
            "exists",
            copy_text(
                "beFilesExtract.into.exists",
                &[("path", &dest.display().to_string())],
            ),
        )));
    }
    let len = Ro::metadata(&archive).map(|m| m.len()).unwrap_or(0);
    let bytes_cap = caps.bytes_for(len);
    let plan = plan(&archive, kind, caps.entries, bytes_cap)?;
    // 落点目录：`create_dir` 自己就是「不在才建」（与探的那一眼之间有人建了 ⇒ 仍是 `exists`）。
    if let Err(e) = face.make_dir(&dest) {
        return Err(if e.kind() == std::io::ErrorKind::AlreadyExists {
            WriteFail::from((
                "exists",
                copy_text(
                    "beFilesExtract.into.exists",
                    &[("path", &dest.display().to_string())],
                ),
            ))
        } else {
            io_failed(
                copy_text(
                    "reason.fs.writeFailed",
                    &[
                        ("path", &dest.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                e,
            )
        });
    }
    let mut made: Vec<(PathBuf, bool)> = vec![(into.to_path_buf(), true)];
    match execute(
        face, root, into, &archive, kind, &plan, &mut made, bytes_cap,
    ) {
        Ok(bytes) => {
            let count = |f: fn(&What) -> bool| plan.entries.values().filter(|w| f(w)).count();
            Ok(Extracted {
                path: dest.into_path_buf(),
                files: count(|w| matches!(w, What::File { .. } | What::Copy(_))),
                dirs: count(|w| *w == What::Dir),
                links: count(|w| matches!(w, What::Link(_))),
                bytes,
            })
        }
        Err(f) => {
            let (tail, stuck_raw) = match undo(face, root, &made) {
                None => (copy_text("beFilesExtract.undo.all", &[]), None),
                Some((stuck, raw)) => (
                    copy_text("beFilesExtract.undo.stuck", &[("what", &stuck)]),
                    raw,
                ),
            };
            // 两句原话（停下的那一步 · 撤不掉的那一条）都进复制详情，各占一行。
            let raw: Vec<String> = f.raw.into_iter().chain(stuck_raw).collect();
            Err(WriteFail {
                code: f.code,
                said: copy_text(
                    "beFilesExtract.stopped.say",
                    &[("why", &f.said), ("tail", &tail)],
                ),
                raw: (!raw.is_empty()).then(|| raw.join("\n")),
            })
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  线上面
// ══════════════════════════════════════════════════════════════════════════

/// 本模块的线上命令（与 `inbound::REGISTRY`、`IPC-PROTOCOL.md §10` 三处对拍，同 `files_write::MANAGE_COMMANDS`）。
pub const EXTRACT_COMMANDS: &[ManageCommand] = &[
    ManageCommand {
        name: "files-extract",
        purpose: "extract a zip / tar / tar.gz / tgz archive into a new directory next to it (named after the archive); \
           every entry is path-checked first; see IPC-PROTOCOL `files-extract`",
    },
];

/// `fresh` 那一形最多往后试几个编号（`名 (2)` … `名 (FRESH_TRIES + 1)`）。
pub const FRESH_TRIES: u32 = 999;

/// 落点目录的名字：包名去掉认得的后缀；`n` ≥ 2 ⇒ 后面接 ` (n)`（Nautilus / Finder / Explorer 撞名时的通行写法）。
pub fn into_name(stem: &[u8], n: u32) -> Vec<u8> {
    let mut v = stem.to_vec();
    if n >= 2 {
        v.extend_from_slice(format!(" ({n})").as_bytes());
    }
    v
}

/// 线上那一形：落点由这里按包名定（名字规矩只有这一份）；`fresh` ⇒ 撞名往后取第一个不在的编号。
pub fn extract_here(face: &Face, root: &Path, rel: &Path, fresh: bool) -> Result<Extracted, Fail> {
    let name = crate::files::raw::path_bytes(rel);
    let base = name.rsplit(|b| *b == b'/').next().unwrap_or(name);
    let stem = stem_of(base).ok_or_else(|| {
        (
            "unsupported",
            copy_text(
                "beFilesExtract.archive.unknownKind",
                &[("name", &shown(base))],
            ),
        )
    })?;
    let dir: Vec<u8> = name[..name.len() - base.len()].to_vec();
    let at = |n: u32| {
        let mut v = dir.clone();
        v.extend_from_slice(&into_name(&stem, n));
        crate::files::raw::to_path_buf(&v)
    };
    if !fresh {
        return extract(face, root, rel, &at(1));
    }
    for n in 2..=FRESH_TRIES + 1 {
        match extract(face, root, rel, &at(n)) {
            Err(WriteFail { code: "exists", .. }) => continue,
            other => return other,
        }
    }
    Err(WriteFail::from((
        "exists",
        copy_text(
            "beFilesExtract.into.noFresh",
            &[("n", &FRESH_TRIES.to_string())],
        ),
    )))
}

/// 一格路径（目标根 · 包 · 链接目标）⇒ `PathBuf`；空的 ⇒ `bad_path`（缺 · 形状不对在解入参那一步就回了 `bad_args`）。
fn non_empty(p: &wire::WirePath, key: &str) -> Result<PathBuf, Fail> {
    if p.0.is_empty() {
        return Err(WriteFail::from((
            "bad_path",
            copy_text("beFilesExtract.args.path", &[("key", key)]),
        )));
    }
    Ok(crate::files::raw::to_path_buf(&p.0))
}

super::files_write::acts! {
    FilesExtractReq => FilesTreeReply, |r, f| extract_of(f, r);
    FilesLinkReq => FilesPathReply, |r, f| link_of(f, r);
}

/// `files-extract`。
fn extract_of(face: &Face, req: wire::FilesExtractReq) -> Result<wire::FilesTreeReply, Fail> {
    let root = non_empty(&req.root, "root")?;
    let rel = non_empty(&req.rel, "rel")?;
    let got = extract_here(face, &root, &rel, req.fresh.unwrap_or(false))?;
    Ok(wire::FilesTreeReply {
        path: wire_of(&got.path),
        files: got.files as u64,
        dirs: got.dirs as u64,
        links: got.links as u64,
        bytes: got.bytes,
    })
}

/// `Door::link {root, rel, target}` → `{path}`。
fn link_of(face: &Face, req: wire::FilesLinkReq) -> Result<wire::FilesPathReply, Fail> {
    let root = non_empty(&req.root, "root")?;
    let rel = non_empty(&req.rel, "rel")?;
    let target = non_empty(&req.target, "target")?;
    let at = land_link(face, &root, &rel, &target).map_err(WriteFail::from)?;
    Ok(wire::FilesPathReply { path: wire_of(&at) })
}

#[cfg(test)]
#[path = "../../../tests/backend/control/files_extract_tests.rs"]
mod tests;
