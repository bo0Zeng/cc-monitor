//! `files-read` 这一族：后端侧只读的文件面 —— 搜索（SFTP 只能递归 `READDIR`、N 次往返，协议里也没有常驻索引）及其同族的纯读
//! （列目录 · 元数据 · 读文本 / 按块读 · 问 home · 算大小 · grep）。写面住 `control/files_write.rs`，SFTP 只做传输。
//!
//! 整族一个字节都不写：读盘只经 [`crate::platform::fs::Ro`]，拿不到写句柄（`readonly_guard` 另把整个后端的写盘形状钉成登记表）。
//! 四个 target 同一份源码、同一组能力；保鲜是定期重走 ＋ 浏览目录挂监听（[`browse_watch`]，各平台语义不对等）。
//! 「定期重走」的周期住 [`index::REWALK_INTERVAL_SECS`]，由 `files.index.status` 交出去，界面显示那个延迟。
//!
//! 线上名与能力名的翻译只有一处：每条 [`Ask`] 自己的 `CAPABILITY`（`files.ls` ↔ `files-ls`）。
//! 索引怎么建、什么时候变新是机制的线上面：`files-index-rebuild`（[`index::rebuild_once`]）· `files-browse`（[`browse_watch::set_browsing`]）；
//! 节拍归调用方（后端零定时器）—— 没人发 rebuild，`files.find` 就回 `index_missing: true`。
//! 常驻那一份索引是进程级的 ⇒ 这几条要在同一条连接上才配得起来；CLI 面一次 exec 一个进程，建好的索引随进程没了。

use crate::platform::fs::Ro;
use cmd_wire::files::{self as wire, FileKind, WirePath};
use std::path::{Path, PathBuf};

pub mod browse_watch;
// 一项是哪一种 · 读得起来的那几道门（`files.ls` · `files.find` 每行带上）。
pub mod class;
pub mod grep;
pub mod index;
// Everything 式搜索词：解析 ＋ 逐条匹配（`files.find` 的 `query` 只在这里被读懂）。
pub mod query;
pub mod raw;
// `files.size`（算目录大小）。
pub mod size;

// ══════════════════════ 命令面（形状照 `inbound::CommandSpec` 的处理器）══════════════════════

/// 一条能力的答案：成功交 JSON，失败交 [`Refused`]（码 ＋ 那一句 ＋ 下层原话）。
pub type Answer<T> = Result<T, Refused>;

/// 一条能力的失败：码 ＋ 给人看的那一句 ＋ 下层原话（可缺：系统报错原文不进句子，由命令表交进复制详情）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub code: &'static str,
    pub said: String,
    pub raw: Option<String>,
}

impl From<(&'static str, String)> for Refused {
    /// 没有下层原话的那几形（参数 · 路径 · 上限）。
    fn from((code, said): (&'static str, String)) -> Refused {
        Refused {
            code,
            said,
            raw: None,
        }
    }
}

impl Refused {
    /// 码 ＋ 那一句 ＋ 下层原话（空 ⇒ 不带）。
    fn with_raw(code: &'static str, said: String, raw: impl std::fmt::Display) -> Refused {
        let raw = raw.to_string();
        Refused {
            code,
            said,
            raw: (!raw.trim().is_empty()).then_some(raw),
        }
    }
}

/// 每次回送的条数上限的默认值（调用方给 `limit` 就用它的；给 0 或不给 ⇒ 用这个）。
/// 理由是「一次往返」：一次查询可以命中五万多条，一次全推过去就不再是零流量搜索。截断时 `truncated` 与 `total_hits` 会说出来。
pub const DEFAULT_LIMIT: usize = 1000;

fn limit_of(limit: Option<u64>) -> usize {
    match limit {
        Some(n) if n > 0 => n as usize,
        _ => DEFAULT_LIMIT,
    }
}

/// 入参里的一个路径 ⇒ `PathBuf`。形状在解入参那一步就判过了（字符串或 `{"b16": …}`）；这里只拒空的。
fn path_of(p: &WirePath, which: &str) -> Result<std::path::PathBuf, (&'static str, String)> {
    if p.0.is_empty() {
        return Err((
            "bad_path",
            crate::common::contract::malformed(&format!("`{which}` is empty")),
        ));
    }
    Ok(raw::to_path_buf(&p.0))
}

/// `files.browse` 的 `dirs`：每项同 [`path_of`]。
///
/// ⚠ **空数组是合法的**，语义是「用户现在什么都没在看」⇒ 名单清空。
/// 「少了 `dirs`」（解入参那一步回 `bad_args`）与「`dirs` 是空的」**是两件事**：前者是调用方漏了参数，
/// 后者是它真的要卸掉全部 —— 静默地把前者当后者办，就是悄悄把 watch 全拆了。
fn dirs_of(dirs: &[WirePath]) -> Result<Vec<std::path::PathBuf>, (&'static str, String)> {
    dirs.iter()
        .enumerate()
        .map(|(i, d)| path_of(d, &format!("dirs[{i}]")))
        .collect()
}

/// 本族的回包里一个路径（原始字节）。
fn wire_path(b: &[u8]) -> WirePath {
    WirePath(b.to_vec())
}

/// 一个时刻换成 Unix 纪元秒。
///
/// ⚠ **参数刻意写成 `io::Result<SystemTime>` 而不是那个元数据类型**：那个类型只有 `platform/fs/` 许点名（`readonly_guard` 的命名空间那一条），
/// 这里只要时刻那一格。
/// ⇒ 两头都不碰：**不提那个类型名**。这不是绕过护栏，是不去动它。
fn epoch_secs(t: std::io::Result<std::time::SystemTime>) -> Option<u64> {
    t.ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// 一个条目的类型（闭集四个，住 [`FileKind`]）。
///
/// ⚠ 参数是三个 `bool` 而不是那个类型，理由同 [`epoch_secs`]。
fn kind_of(is_dir: bool, is_file: bool, is_symlink: bool) -> FileKind {
    if is_dir {
        FileKind::Dir
    } else if is_file {
        FileKind::File
    } else if is_symlink {
        FileKind::Symlink
    } else {
        FileKind::Other
    }
}

/// 索引里的类型字节（`query::KIND_*`）→ 线上那个词。
const KINDS_BY_CODE: [FileKind; 4] = [
    FileKind::File,
    FileKind::Dir,
    FileKind::Symlink,
    FileKind::Other,
];

/// 正被浏览的某个目录有了动静、重列过 ⇒ 那个目录（[`browse_watch::dir_changes`]；推 `changed {files_dir}` 的那一头订它）。
pub fn dir_changes() -> tokio::sync::broadcast::Receiver<String> {
    browse_watch::dir_changes().subscribe()
}

/// 目录列表的备忘：一种问法（目录 · 排法 · 只列文件夹 · 上限）一份，份数同盯着的目录上限。
static LS_VIEWS: std::sync::LazyLock<crate::common::views::Views> =
    std::sync::LazyLock::new(|| crate::common::views::Views::new(browse_watch::MAX_BROWSE_WATCHES));

/// 此刻列好的那一份（交给备忘比：问一次只取一次）。
struct LsNow {
    key: String,
    now: std::cell::Cell<Option<(wire::LsHead, Vec<(String, wire::LsEntry)>)>>,
}

impl crate::common::views::ViewSource for LsNow {
    type Head = wire::LsHead;
    type Item = wire::LsEntry;
    fn ask_key(&self) -> String {
        self.key.clone()
    }
    fn now(&self) -> (wire::LsHead, Vec<(String, wire::LsEntry)>) {
        self.now.take().expect("一问只比一次")
    }
}

/// `files.ls` —— 列一个目录的直接子项，按手上那一份的记号答「没变 / 变了的那几行 / 整份」（[`crate::common::views`]）。
fn ls(req: wire::LsReq, tz: &crate::common::time::Tz) -> Answer<wire::LsReply> {
    let key = format!(
        "{}\0{}\0{}\0{}",
        req.path.key(),
        serde_json::to_string(&req.sort.unwrap_or_default()).unwrap_or_default(),
        req.dirs_only.unwrap_or(false),
        limit_of(req.limit),
    );
    let (held, range) = (req.held.clone(), req.range);
    let now = ls_now(req, tz)?;
    let src = LsNow {
        key,
        now: std::cell::Cell::new(Some(now)),
    };
    Ok(LS_VIEWS.answer(&src, held.as_ref(), range))
}

/// 此刻那一份：排好、截好的每一行（键 ＝ 末段名）＋ 头。`dirs_only` ⇒ 只列文件夹（含指向目录的链接），截断与计数都只数文件夹。
fn ls_now(
    req: wire::LsReq,
    tz: &crate::common::time::Tz,
) -> Answer<(wire::LsHead, Vec<(String, wire::LsEntry)>)> {
    let dir = path_of(&req.path, "path")?;
    let limit = limit_of(req.limit);
    let dirs_only = req.dirs_only.unwrap_or(false);
    // 打不开的原因分几种码（界面按码说「路径不存在 / 无权限 / 不是目录」，系统原话进详情）。
    let rd = Ro::read_dir(&dir).map_err(|e| {
        let code = match e.kind() {
            std::io::ErrorKind::NotFound => "not_found",
            std::io::ErrorKind::PermissionDenied => "denied",
            std::io::ErrorKind::NotADirectory => "not_dir",
            _ => "unreadable",
        };
        Refused::with_raw(
            code,
            copy_core::copy_text(
                "beFilesRead.ls.unreadable",
                &[("kind", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        )
    })?;
    // 读不出的项照数（不静默少一项）：窗口按 `unreadable` 说一句。
    let mut unreadable = 0usize;
    let sort = req.sort.unwrap_or(wire::LsSort {
        by: wire::LsSortBy::Name,
        reverse: false,
    });
    // 先收齐、排好，再截：截掉的那几项不写钟面字（按名称排时只有链接要读元数据）。
    let mut picked: Vec<Picked> = Vec::new();
    for e in readable(rd, &mut unreadable) {
        // 🔴 类型走 `DirEntry::file_type()`：它**不跟 symlink**
        //（Linux 上直接来自目录项里的 d_type），所以指向别处的链接不会被当成它的目标。
        let kind = e
            .file_type()
            .map(|t| kind_of(t.is_dir(), t.is_file(), t.is_symlink()))
            .unwrap_or(FileKind::Other);
        // 链接要跟一次才知道指向的是不是目录（只列文件夹 · 目录在前都看它）；按大小 · 时间排要每一项的元数据。
        let md = (kind == FileKind::Symlink
            || matches!(sort.by, wire::LsSortBy::Size | wire::LsSortBy::Mtime))
        .then(|| Ro::metadata(e.path()).ok())
        .flatten();
        let dir_like = kind == FileKind::Dir || md.as_ref().is_some_and(|m| m.is_dir());
        if dirs_only && !dir_like {
            continue;
        }
        let num = md.map_or(0, |m| match sort.by {
            wire::LsSortBy::Size => m.len(),
            wire::LsSortBy::Mtime => epoch_secs(m.modified()).unwrap_or(0),
            wire::LsSortBy::Name | wire::LsSortBy::Type => 0,
        });
        picked.push(Picked {
            path: e.path(),
            kind,
            dir_like,
            num,
        });
    }
    let total = picked.len() as u64;
    picked.sort_by_cached_key(|p| ls_order(p, sort));
    picked.truncate(limit);
    let entries = picked
        .into_iter()
        .map(|p| ls_row(p, tz))
        .map(|e| (e.name.key(), e))
        .collect::<Vec<_>>();
    let head = wire::LsHead {
        truncated: total > entries.len() as u64,
        up: dir.parent().map(|u| wire_path(raw::path_bytes(u))),
        crumbs: crumbs_of(&dir),
        total,
        unreadable: unreadable as u64,
    };
    Ok((head, entries))
}

/// 列出来、还没排的一项（`num` ＝ 按大小 / 时间排时那一格，按名称 / 类型排时不读、恒 0）。
struct Picked {
    path: PathBuf,
    kind: FileKind,
    dir_like: bool,
    num: u64,
}

/// 那一列的值（一次排序里每项同一形）。
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Col {
    Text(Vec<u8>),
    Num(u64),
    Type(wire::FileClass, Option<String>),
}

/// 那一列正着还是反着（一次排序里每项同一形）。
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Dir {
    Asc(Col),
    Desc(std::cmp::Reverse<Col>),
}

/// 一项的排序键：目录（含指向目录的链接）在前 · 那一列（第一下：名称 · 类型 A 在前，大小 · 时间大的在前；`reverse` 只反这一格）·
/// 名称小写 · 原字节。
fn ls_order(p: &Picked, sort: wire::LsSort) -> (bool, Dir, Vec<u8>, Vec<u8>) {
    let name = p.path.file_name().map(raw_name).unwrap_or_default();
    // 名称小写：合法 UTF-8 按字符小写；不是 ⇒ 只把 ASCII 那几个字节小写（字节一路不改形）。
    let lower = match std::str::from_utf8(&name) {
        Ok(s) => s.to_lowercase().into_bytes(),
        Err(_) => name.to_ascii_lowercase(),
    };
    let col = match sort.by {
        wire::LsSortBy::Name => Col::Text(lower.clone()),
        wire::LsSortBy::Size | wire::LsSortBy::Mtime => Col::Num(p.num),
        wire::LsSortBy::Type => {
            let (c, e) =
                class::type_key(&name, p.kind == FileKind::Dir, p.kind == FileKind::Symlink);
            Col::Type(c, e)
        }
    };
    let col = if sort.descending() {
        Dir::Desc(std::cmp::Reverse(col))
    } else {
        Dir::Asc(col)
    };
    (!p.dir_like, col, lower, name)
}

/// 排好、进了这一屏的一项 ⇒ 线上那一行（钟面字只给进了屏的写）。
fn ls_row(p: Picked, tz: &crate::common::time::Tz) -> wire::LsEntry {
    let kind = p.kind;
    let md = Ro::metadata(&p.path).ok();
    let name = p.path.file_name().map(raw_name).unwrap_or_default();
    let link = kind == FileKind::Symlink;
    let link_dir = link && md.as_ref().is_some_and(|m| m.is_dir());
    let is_dir = kind == FileKind::Dir;
    let mut row = wire::LsEntry {
        path: wire_path(raw::path_bytes(&p.path)),
        class: class::class_of_entry(&name, is_dir, link, link_dir),
        type_text: class::type_text(&name, is_dir, link, link_dir, link && md.is_none()),
        gates: (!p.dir_like).then(|| class::gates_of(&name, md.as_ref().map_or(0, |m| m.len()))),
        name: WirePath(name),
        kind,
        size: None,
        size_text: None,
        mtime_secs: None,
        mtime_text: None,
        mtime_full: None,
        link_dir: None,
        link_to: None,
    };
    if let Some(md) = md {
        row.size = Some(md.len());
        // 目录（含指向目录的链接）的大小是文件系统记账的数，不上屏：不写字。
        row.size_text = (!is_dir && !link_dir).then(|| copy_core::size_text(md.len()));
        if let Some(t) = epoch_secs(md.modified()) {
            let (short, full) = crate::common::time::mtime_texts_now(t, tz);
            row.mtime_secs = Some(t);
            row.mtime_text = Some(short);
            row.mtime_full = Some(full);
        }
        // 链接指向的是不是目录（跟链接那一次 `metadata` 顺带的）：窗口据此给「打开」。断链不出这一格。
        if kind == FileKind::Symlink {
            row.link_dir = Some(md.is_dir());
            row.link_to = Some(if md.is_dir() {
                wire::LinkTo::Dir
            } else {
                wire::LinkTo::File
            });
        }
    } else if kind == FileKind::Symlink {
        // 跟不过去 ⇒ 断了的链接（指向的东西不在 / 读不到）。
        row.link_to = Some(wire::LinkTo::Missing);
    }
    row
}

/// 一段名字的原始字节。
fn raw_name(n: &std::ffi::OsStr) -> Vec<u8> {
    n.as_encoded_bytes().to_vec()
}

/// 根到 `dir` 的每一段（根在最前）：照这台的路径规则切（`Path::components`；Windows 的盘符与根并成一段）。
fn crumbs_of(dir: &Path) -> Vec<wire::Crumb> {
    let mut out: Vec<wire::Crumb> = Vec::new();
    let mut at = PathBuf::new();
    for c in dir.components() {
        at.push(c.as_os_str());
        let name = raw_name(c.as_os_str());
        match (c, out.last_mut()) {
            // 盘符之后紧跟的根（`C:` ＋ `\`）并进同一段。
            (std::path::Component::RootDir, Some(last)) => {
                last.name.0.extend_from_slice(&name);
                last.path = wire_path(raw::path_bytes(&at));
            }
            (std::path::Component::CurDir | std::path::Component::ParentDir, _) => {}
            _ => out.push(wire::Crumb {
                name: WirePath(name),
                path: wire_path(raw::path_bytes(&at)),
            }),
        }
    }
    out
}

/// 目录项里读得出的那几条；读不出的数进 `unreadable`（不静默跳过）。
fn readable<'a, E, I>(items: I, unreadable: &'a mut usize) -> impl Iterator<Item = E> + 'a
where
    I: IntoIterator<Item = std::io::Result<E>>,
    I::IntoIter: 'a,
{
    items.into_iter().filter_map(move |e| match e {
        Ok(e) => Some(e),
        Err(_) => {
            *unreadable += 1;
            None
        }
    })
}

/// `files.stat` —— 一个路径的元数据。它跟 symlink（拿链接指向的那个东西的元数据）：不跟的读法是另一个动词（`Ro::symlink_metadata`），
/// 这一条不用它。需要区分链接本身时，先用 `files.ls` 看它父目录那一行的 `kind`。
fn stat(req: wire::StatReq, tz: &crate::common::time::Tz) -> Answer<wire::StatReply> {
    let path = path_of(&req.path, "path")?;
    let md = Ro::metadata(&path).map_err(|e| {
        Refused::with_raw(
            "unreadable",
            copy_core::copy_text(
                "beFilesRead.size.unreadable",
                &[("kind", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        )
    })?;
    let t = md.file_type();
    let mtime = epoch_secs(md.modified());
    let texts = mtime.map(|ms| crate::common::time::mtime_texts_now(ms, tz));
    Ok(wire::StatReply {
        path: wire_path(raw::path_bytes(&path)),
        up: path.parent().map(|u| wire_path(raw::path_bytes(u))),
        kind: kind_of(t.is_dir(), t.is_file(), t.is_symlink()),
        size: md.len(),
        readonly: md.permissions().readonly(),
        // unix 权限位（低 12 位：rwx×3 ＋ setuid / setgid / sticky）—— 同一次 `metadata`、同样跟链接，
        //   与本条其余几格同源。非 unix 平台**缺席**（不是 0：0 是一个真能设的权限值，报 0 等于说假话）；
        //   那边改权限本来就回 `no_unix_mode`（`control/files_write.rs::change_mode_reporting`）。
        mode: crate::platform::fs::Ro::mode(&md).map(|m| m & 0o7777),
        mtime_secs: mtime,
        mtime_text: texts.as_ref().map(|t| t.0.clone()),
        mtime_full: texts.map(|t| t.1),
        // 属主（跟链接，同上几格）：用户名，查不到名字给 uid 数字串；非 unix 给 `null`。
        owner: crate::platform::paths::owner_of(&path),
        // 路径**本身**是不是链接（`readlink` 读得出 ⇒ 是）：是 ⇒ 目标原文（原始字节形）；否 ⇒ `null`。
        link_target: crate::platform::paths::link_target_of(&path)
            .map(|t| wire_path(raw::path_bytes(&t))),
    })
}
/// `files.find` —— 在常驻索引里查。**只回送命中。**
///
/// `query` 是窗口原样发来的搜索词（解析只在 [`query::parse`]）；`seq` ＋ `stream` 让同一个搜索框的旧那一趟收手；
/// `under` 给了 ⇒ 只搜那个目录底下，不给 ⇒ 家目录；`offset` / `limit` 只回这一屏。
fn find(req: wire::FindReq, tz: &crate::common::time::Tz) -> Answer<wire::FindReply> {
    let matcher = query::parse(&req.query).map_err(|e| ("bad_query", e.said()))?;
    let under = match &req.under {
        None => None,
        Some(p) => {
            path_of(p, "under")?;
            Some(p.0.clone())
        }
    };
    let offset = req.offset.unwrap_or(0) as usize;
    let superseded = || {
        (
            "superseded",
            copy_core::copy_text("beFilesRead.find.superseded", &[]),
        )
    };
    let ticket = match req.seq {
        Some(n) => {
            Some(index::ticket(req.stream.as_deref().unwrap_or(""), n).map_err(|_| superseded())?)
        }
        None => None,
    };
    let sort_key = req.sort.unwrap_or(wire::FindSort::Relevance);
    let sort = index::Sort {
        key: index::SortKey::of_wire(sort_key),
        desc: req.desc.unwrap_or(false),
    };
    // 「整台机器」：范围由这台自己定（文件系统的根），界面不猜平台。
    let machine = req.scope == Some(wire::FindScope::Machine);
    let home = home_var().map(std::path::PathBuf::from);
    let root = machine.then(|| machine_root(home.as_deref()));
    let under = root.or(under);
    let r = index::find(&index::FindArgs {
        query: &matcher,
        under: under.as_deref(),
        home: home.as_deref().map(raw::path_bytes),
        offset,
        limit: limit_of(req.limit),
        ticket: ticket.as_ref(),
        sort,
    })
    .map_err(|_| superseded())?;
    let start = under
        .clone()
        .or_else(|| home.as_deref().map(|h| raw::path_bytes(h).to_vec()));
    let pairs = |v: &[(usize, usize)]| v.iter().map(|(a, b)| [*a as u64, *b as u64]).collect();
    Ok(wire::FindReply {
        hits: r
            .hits
            .iter()
            .map(|h| {
                let kind = KINDS_BY_CODE
                    .get(h.kind as usize)
                    .copied()
                    .unwrap_or(FileKind::Other);
                wire::FilesFindHit {
                    path: wire_path(&h.path),
                    class: class::type_key(
                        index::name_bytes(&h.path),
                        kind == FileKind::Dir,
                        kind == FileKind::Symlink,
                    )
                    .0,
                    kind,
                    location: wire_path(index::location_of(&h.path, start.as_deref())),
                    size: h.meta.size,
                    mtime_secs: h.meta.mtime_secs,
                    mtime_text: h
                        .meta
                        .mtime_secs
                        .map(|t| crate::common::time::mtime_texts_now(t, tz).0),
                    marks: pairs(&matcher.marks(&h.path)),
                }
            })
            .collect(),
        start: start.as_deref().map(wire_path),
        sort: sort_key,
        desc: sort.desc,
        total_hits: r.total_hits as u64,
        truncated: r.truncated,
        scanned: r.scanned as u64,
        index_age_secs: r.index_age_secs,
        index_missing: r.index_missing,
        stale: r.stale,
        index_root: r.index_root.as_deref().map(wire_path),
        out_of_index: r.out_of_index,
        cover_root: r.cover_root.as_deref().map(wire_path),
        seq: req.seq,
        offset: offset as u64,
    })
}
/// `path` 相对 `root` 那一段（不在它底下 ⇒ 原样；就是它 ⇒ 最后一段名字）。
fn rel_to<'p>(path: &'p [u8], root: &[u8]) -> &'p [u8] {
    let sep = |b: u8| b == b'/' || (cfg!(windows) && b == b'\\');
    let mut r = root;
    while r.len() > 1 && r.last().is_some_and(|&b| sep(b)) {
        r = &r[..r.len() - 1];
    }
    if path.len() > r.len()
        && path.starts_with(r)
        && (sep(path[r.len()]) || r.last().is_some_and(|&b| sep(b)))
    {
        let mut rest = &path[r.len()..];
        while rest.first().is_some_and(|&b| sep(b)) {
            rest = &rest[1..];
        }
        return rest;
    }
    if path == r {
        return path
            .iter()
            .rposition(|&b| sep(b))
            .map_or(path, |i| &path[i + 1..]);
    }
    path
}

/// 「整台机器」那一档的范围：unix 是 `/`；Windows 是家目录所在那块盘的根（说不出 ⇒ `C:\\`）。
fn machine_root(home: Option<&std::path::Path>) -> Vec<u8> {
    if cfg!(windows) {
        let drive = home
            .map(raw::path_bytes)
            .filter(|h| h.len() >= 2 && h[1] == b':' && h[0].is_ascii_alphabetic())
            .map(|h| h[..2].to_vec())
            .unwrap_or_else(|| b"C:".to_vec());
        [drive, b"\\".to_vec()].concat()
    } else {
        b"/".to_vec()
    }
}

/// `files.index.status` —— 新鲜度 ／ 条目数 ／ 常驻字节 ／ **声明的重走周期**。
fn index_status() -> wire::IndexStatusReply {
    let s = index::status();
    wire::IndexStatusReply {
        index_missing: s.index_missing,
        entries: s.entries as u64,
        resident_bytes: s.resident_bytes as u64,
        unreadable_dirs: s.unreadable_dirs as u64,
        unreadable_paths: unreadable_paths(),
        truncated: s.truncated,
        skipped_mounts: s.skipped_mounts as u64,
        age_secs: s.age_secs,
        rewalk_interval_secs: s.rewalk_interval_secs,
        stale: s.stale,
        browse_watches: s.browse_watches as u64,
        browse_watch_cap: s.browse_watch_cap as u64,
        cold_first_build_secs: s.cold_first_build_secs,
    }
}

/// `files.index.rebuild` —— 走一遍，就一遍，做完返回。
///
/// 根读不进去 ⇒ 拒，常驻那一份一个字节不动：[`index::build`] 对一个打不开的根不会失败（只把 `unreadable_dirs` 加一、交一份空快照），
/// 而 [`index::rebuild_once`] 会把常驻那一份整份换掉 ⇒ 路径打错一个字母，好索引就被空的顶掉、回参还像一次成功的重走。
/// 所以换之前先探一次根，打不开回 `unreadable`（判据：换之前的条目数 == 被拒之后的条目数）。
/// 根底下读不进去的子目录照旧落在 `unreadable_dirs` 里（不是 0 就说明这份索引有洞）。
fn index_rebuild(req: wire::RebuildReq) -> Answer<wire::RebuildReply> {
    let root = match &req.path {
        None => home_var()
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or((
                "no_home",
                copy_core::copy_text("beFilesRead.rebuild.noHome", &[]),
            ))?,
        Some(p) => path_of(p, "path")?,
    };
    Ro::read_dir(&root).map_err(|e| {
        Refused::with_raw(
            "unreadable",
            copy_core::copy_text(
                "beFilesRead.rebuild.rootUnreadable",
                &[("kind", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        )
    })?;
    let stats = index::rebuild_once(&root).ok_or_else(|| {
        (
            "already_rebuilding",
            copy_core::copy_text(
                "beFilesRead.rebuild.busy",
                &[("n", &index::rebuild_skipped().to_string())],
            ),
        )
    })?;
    Ok(wire::RebuildReply {
        path: wire_path(raw::path_bytes(&root)),
        entries: stats.entries as u64,
        resident_bytes: stats.resident_bytes as u64,
        unreadable_dirs: stats.unreadable_dirs as u64,
        unreadable_paths: unreadable_paths(),
        truncated: stats.truncated,
        skipped_mounts: stats.skipped_mounts as u64,
    })
}

/// 常驻索引里读不进去的那几个目录（前 20 个）。
fn unreadable_paths() -> Vec<WirePath> {
    index::unreadable_paths()
        .into_iter()
        .map(WirePath)
        .collect()
}

/// `files.browse` —— 告诉后端「用户现在在看哪几个目录」。
///
/// [`browse_watch::set_browsing`] 登记名单 ＋ 当场把那几个目录各重列一遍（结果进 overlay，查询时盖掉大索引里的对应条目）；
/// [`browse_watch::keep_watching`] 让进程里那一个监听器跟上名单 ⇒ 浏览的目录此后一有动静 overlay 就跟着重列。
/// 买不到：watch 绑 inode 不绑路径 · 内核队列溢出 · 窗口关了没人发空名单（最后那份名单的 watch 留到下一次）。
/// `rejected` 必须跟着回去，并同拍带上 `browse_watch_cap`：只回一个数、不说上限，调用方没法判该少送几个。
fn browse(req: wire::BrowseReq) -> Answer<wire::BrowseReply> {
    let dirs = dirs_of(&req.dirs)?;
    let applied = browse_watch::set_browsing(&dirs);
    let w = browse_watch::keep_watching();
    Ok(wire::BrowseReply {
        added: applied.added as u64,
        removed: applied.removed as u64,
        rejected: applied.rejected as u64,
        browse_watch_cap: browse_watch::MAX_BROWSE_WATCHES as u64,
        watching: w.watching as u64,
        watch_failed: w.failed as u64,
        watch_error: w.error,
    })
}
/// `files.read.text` 一趟最多肯交多少字节：一帧应答整个进内存、整个过线的天花板（JSON 转义最坏 ×6 = 48 MiB，
/// 仍在 monitor 读后端一行 · 通道一帧 64 MiB 之内）。编辑上限就是它（[`class::gates_of`]）；
/// 调用方要的 `max_bytes` 超过它 ⇒ `bad_args`，不偷偷夹小。
pub const READ_TEXT_MAX_BYTES: usize = 8 * 1024 * 1024;

/// `files-read-chunk` 一块最多多少原始字节（b16 翻倍后一帧应答仍远小于 monitor 读一行的上限）。
/// 调用方给的 `len` 越界 ⇒ `bad_args`、不夹小（同 [`READ_TEXT_MAX_BYTES`] 那一条理由）。
pub const READ_CHUNK_MAX_BYTES: u64 = 256 * 1024;

// 摘要算法（流式那一形 ＋ 十六进制长度）住 `common/digest.rs`：传输台（原生那一块）上传时也边传边算，两块各引各的。
pub use crate::common::digest::{ContentDigest, SHA256_HEX_LEN};

/// 一份字节的 SHA-256，64 位小写十六进制 —— CAS 摘要形 `expect: {"sha256": …}` 整份那一形（算法本身住 `common/digest.rs`）。
///
/// 读的那一趟（[`read_text`]）对交出去的字节算它；写面（`control/files_write.rs::overwrite_text_expecting`）
/// 拿它比「盘上此刻那一份」、写成之后对新内容再算一次交回去。住读族这一侧，是因为读族不许伸手进写面
/// （写面只有 `stream/inbound/` 一扇门），反过来写面借读族一个纯函数不开新门。
pub fn content_sha256(bytes: &[u8]) -> String {
    let mut d = ContentDigest::new();
    d.update(bytes);
    d.finish()
}

/// 一份文件的 [`content_sha256`]，**逐块读**（64 KiB 一块）。读不出 ⇒ 原样的 IO 错。
pub fn file_sha256(path: &std::path::Path) -> std::io::Result<String> {
    use std::io::Read as _;
    let mut f = Ro::open(path)?;
    let mut d = ContentDigest::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok(d.finish());
        }
        d.update(&buf[..n]);
    }
}

/// `files.read.chunk`：`{path, offset, len}` → `{path, offset, size, eof, content: {b16}}`。
/// 只读普通文件（不是 ⇒ `not_text`，与读族同一个码）；`offset` 越过末尾 ⇒ 空块、`eof: true`。
fn read_chunk(req: wire::ReadChunkReq) -> Answer<wire::ReadChunkReply> {
    use std::io::{Read as _, Seek as _};
    let path = path_of(&req.path, "path")?;
    let (offset, len) = (req.offset, req.len);
    if len == 0 || len > READ_CHUNK_MAX_BYTES {
        return Err(Refused::from((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`len` is {len}; accepted range 1..={READ_CHUNK_MAX_BYTES}, not clamped"
            )),
        )));
    }
    let md = Ro::metadata(&path).map_err(|e| {
        Refused::with_raw(
            "unreadable",
            copy_core::copy_text(
                "beFilesRead.size.unreadable",
                &[("kind", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        )
    })?;
    if !md.is_file() {
        return Err(Refused::from((
            "not_text",
            copy_core::copy_text("reason.fs.notRegular", &[]),
        )));
    }
    let broke = |e: std::io::Error| {
        Refused::with_raw(
            "unreadable",
            copy_core::copy_text(
                "beFilesRead.text.readBroke",
                &[("kind", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        )
    };
    let mut f = Ro::open(&path).map_err(|e| {
        Refused::with_raw(
            "unreadable",
            copy_core::copy_text(
                "beFilesRead.text.openFailed",
                &[("kind", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        )
    })?;
    f.seek(std::io::SeekFrom::Start(offset)).map_err(broke)?;
    let mut buf: Vec<u8> = Vec::new();
    f.take(len).read_to_end(&mut buf).map_err(broke)?;
    let size = md.len();
    Ok(wire::ReadChunkReply {
        path: wire_path(raw::path_bytes(&path)),
        offset,
        size,
        eof: offset + buf.len() as u64 >= size,
        content: wire::HexBytes(buf),
    })
}

/// `files.read.text` —— 读一份文本。超上限整趟拒，不截断（截断过的文本存回去会写坏文件）。
///
/// | 码 | 什么时候 |
/// |---|---|
/// | `too_large` | `stat` 出来的大小超过 `max_bytes`；或者读的时候比 `stat` 时大（文件在长）|
/// | `not_text` | 不是普通文件 · 含 NUL 字节 · 不是合法 UTF-8 |
/// | `unreadable` | 读不到（不存在 / 没权限）|
///
/// 大小判两次：`stat` 与真读之间文件可以被换大，第二道用 `take(max + 1)` 读 —— 最多多读一个字节就知道「超了」。
fn read_text(req: wire::ReadTextReq) -> Answer<wire::ReadTextReply> {
    let path = path_of(&req.path, "path")?;
    let max = req.max_bytes;
    if max == 0 || max > READ_TEXT_MAX_BYTES as u64 {
        return Err(Refused::from((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`max_bytes` is {max}; accepted range 1..={READ_TEXT_MAX_BYTES}, not clamped"
            )),
        )));
    }
    let md = Ro::metadata(&path).map_err(|e| {
        Refused::with_raw(
            "unreadable",
            copy_core::copy_text(
                "beFilesRead.size.unreadable",
                &[("kind", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        )
    })?;
    if !md.is_file() {
        return Err(Refused::from((
            "not_text",
            copy_core::copy_text("reason.fs.notRegular", &[]),
        )));
    }
    if md.len() > max {
        return Err(Refused::from((
            "too_large",
            copy_core::copy_text(
                "beFilesRead.text.tooLarge",
                &[
                    ("size", &md.len().to_string()),
                    ("max", &max.to_string()),
                    ("over", &(md.len() - max).to_string()),
                ],
            ),
        )));
    }
    let f = Ro::open(&path).map_err(|e| {
        Refused::with_raw(
            "unreadable",
            copy_core::copy_text(
                "beFilesRead.text.openFailed",
                &[("kind", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        )
    })?;
    let mut buf: Vec<u8> = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(f, max + 1), &mut buf).map_err(|e| {
        Refused::with_raw(
            "unreadable",
            copy_core::copy_text(
                "beFilesRead.text.readBroke",
                &[("kind", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        )
    })?;
    if buf.len() as u64 > max {
        return Err(Refused::from((
            "too_large",
            copy_core::copy_text("beFilesRead.text.grew", &[("max", &max.to_string())]),
        )));
    }
    if buf.contains(&0) {
        return Err(Refused::from((
            "not_text",
            copy_core::copy_text("beFilesRead.text.hasNul", &[]),
        )));
    }
    let n = buf.len();
    // 交出去的那份字节的摘要：编辑器存回去时原样交回当 CAS 的 `expect`（算法住写面那一处）。
    let sha256 = content_sha256(&buf);
    let text = String::from_utf8(buf).map_err(|e| {
        (
            "not_text",
            copy_core::copy_text(
                "beFilesRead.text.notUtf8",
                &[("at", &e.utf8_error().valid_up_to().to_string())],
            ),
        )
    })?;
    Ok(wire::ReadTextReply {
        path: wire_path(raw::path_bytes(&path)),
        text,
        bytes: n as u64,
        sha256,
    })
}

/// `files.size` —— 走法与诚实边界住 [`size`] 头注。
fn size_of(req: wire::SizeReq) -> Answer<wire::SizeReply> {
    let path = path_of(&req.path, "path")?;
    let m = size::measure(&path).map_err(|e| {
        Refused::with_raw(
            "unreadable",
            copy_core::copy_text(
                "beFilesRead.size.unreadable",
                &[("kind", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        )
    })?;
    Ok(wire::SizeReply {
        path: wire_path(raw::path_bytes(&path)),
        bytes: m.bytes,
        files: m.files,
        dirs: m.dirs,
        links: m.links,
        other: m.other,
        skipped_mounts: m.skipped_mounts,
        unreadable_dirs: m.unreadable_dirs,
    })
}

/// 后端这个进程的用户 home（环境里那一格，原样）。空串算没有。与 SFTP 那一问（`realpath(".")`）是同一个答案的两个出处
/// （sshd 按账号给 `HOME` 并把 SFTP 子系统的起点放在同一处）；有人在登录脚本里改了 `HOME` 时答改过之后的那个 —— shell 与 Claude 认的也是它。
fn home_var() -> Option<std::ffi::OsString> {
    crate::platform::paths::home_dir().map(std::path::PathBuf::into_os_string)
}

/// `files.grep` 的入参：`needle` 与 `path` 同两种形（字符串 / `{"b16"}`），
/// 非空、至多 [`grep::NEEDLE_MAX_BYTES`] 字节；`limit` 缺省 [`grep::DEFAULT_LIMIT`]、至多 [`grep::MAX_LIMIT`]。
fn grep_args(
    req: &wire::GrepReq,
) -> Result<(std::path::PathBuf, grep::GrepArgs), (&'static str, String)> {
    let path = path_of(&req.path, "path")?;
    let needle = req.needle.0.clone();
    if needle.is_empty() || needle.len() > grep::NEEDLE_MAX_BYTES {
        return Err((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`needle` must be 1..={} bytes",
                grep::NEEDLE_MAX_BYTES
            )),
        ));
    }
    let limit = match req.limit {
        Some(n) if n > 0 => (n as usize).min(grep::MAX_LIMIT),
        _ => grep::DEFAULT_LIMIT,
    };
    Ok((
        path,
        grep::GrepArgs {
            needle,
            ignore_ascii_case: req.ignore_ascii_case.unwrap_or(false),
            limit,
        },
    ))
}

/// `files.grep` 的成品（见 [`grep`] 头注）。
fn grep_reply(top: &std::path::Path, limit: usize, g: &grep::Grepped) -> wire::GrepReply {
    let top_bytes = raw::path_bytes(top);
    wire::GrepReply {
        path: wire_path(top_bytes),
        hits: g
            .hits
            .iter()
            .map(|h| wire::GrepHit {
                path: wire_path(&h.path),
                rel: wire_path(rel_to(&h.path, top_bytes)),
                line: h.line,
                text: wire_path(&h.text),
                matches: h.matches,
                lines: h
                    .lines
                    .iter()
                    .map(|l| wire::GrepLine {
                        line: l.line,
                        text: wire_path(&l.text),
                        marks: l.mark.iter().map(|(a, b)| [*a as u64, *b as u64]).collect(),
                    })
                    .collect(),
            })
            .collect(),
        files: g.files,
        bytes: g.bytes,
        links: g.links,
        skipped_binary: g.skipped_binary,
        skipped_large: g.skipped_large,
        skipped_mounts: g.skipped_mounts,
        unreadable: g.unreadable,
        limit: limit as u64,
        truncated: matches!(g.stopped, grep::Stopped::Hits | grep::Stopped::Bytes),
        stopped: match g.stopped {
            grep::Stopped::Hits => Some(wire::GrepStopped::Hits),
            grep::Stopped::Bytes => Some(wire::GrepStopped::Bytes),
            grep::Stopped::Done | grep::Stopped::Cancelled => None,
        },
    }
}

fn grep_unreadable(e: &std::io::Error) -> Refused {
    Refused::with_raw(
        "unreadable",
        copy_core::copy_text(
            "beFilesRead.grep.unreadable",
            &[("kind", &copy_core::io_reason(e.kind()))],
        ),
        e,
    )
}

/// `files.grep` —— 同步那一臂（取消位由调用方给）。
fn grep_with(
    req: &wire::GrepReq,
    cancel: &std::sync::atomic::AtomicBool,
) -> Answer<wire::GrepReply> {
    let (top, a) = grep_args(req)?;
    let g = grep::search(&top, &a, cancel).map_err(|e| grep_unreadable(&e))?;
    Ok(grep_reply(&top, a.limit, &g))
}

/// `files.grep` 的命令面那一臂：在阻塞池里搜，这一份 future 被丢（`cancel` 命中）⇒ 拉起取消位，搜的那一头在下一份文件前收手。
pub async fn answer_grep_cancellable(req: wire::GrepReq) -> Answer<wire::GrepReply> {
    struct RaiseOnDrop(std::sync::Arc<std::sync::atomic::AtomicBool>);
    impl Drop for RaiseOnDrop {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let _raise = RaiseOnDrop(flag.clone());
    tokio::task::spawn_blocking(move || grep_with(&req, &flag))
        .await
        .unwrap_or_else(|e| {
            Err(Refused::with_raw(
                "unreadable",
                copy_core::copy_text(
                    "beFilesRead.grep.unreadable",
                    &[("kind", &copy_core::io_reason(std::io::ErrorKind::Other))],
                ),
                &e,
            ))
        })
}

/// `files.home` —— 这台机器上「开在 home」那个起点。
fn home() -> Answer<wire::FilesPathReply> {
    home_from(home_var())
}

/// 那个起点必须是一个绝对路径（空 / 相对 ⇒ `no_home`，不开一个空窗口）。
fn home_from(h: Option<std::ffi::OsString>) -> Answer<wire::FilesPathReply> {
    let h = h.filter(|v| !v.is_empty()).ok_or((
        "no_home",
        copy_core::copy_text("beFilesRead.home.missing", &[]),
    ))?;
    let p = std::path::PathBuf::from(h);
    if !p.is_absolute() {
        return Err(Refused::from((
            "no_home",
            copy_core::copy_text("beFilesRead.home.relative", &[]),
        )));
    }
    Ok(wire::FilesPathReply {
        path: wire_path(raw::path_bytes(&p)),
    })
}

/// 本族一条读命令的入参 ⇒ 它的回包与本体。命令表那一扇门只认 [`answer`]：入参的类型就选中了那条能力，抄错能力名这件事不可表示。
pub trait Ask {
    /// [`CAPABILITIES`] 里那一行的名字。
    const CAPABILITY: &'static str;
    type Reply;
    fn answer(self, tz: &crate::common::time::Tz) -> Answer<Self::Reply>;
}

impl Ask for wire::LsReq {
    const CAPABILITY: &'static str = "files.ls";
    type Reply = wire::LsReply;
    fn answer(self, tz: &crate::common::time::Tz) -> Answer<wire::LsReply> {
        ls(self, tz)
    }
}

impl Ask for wire::StatReq {
    const CAPABILITY: &'static str = "files.stat";
    type Reply = wire::StatReply;
    fn answer(self, tz: &crate::common::time::Tz) -> Answer<wire::StatReply> {
        stat(self, tz)
    }
}

impl Ask for wire::FindReq {
    const CAPABILITY: &'static str = "files.find";
    type Reply = wire::FindReply;
    fn answer(self, tz: &crate::common::time::Tz) -> Answer<wire::FindReply> {
        find(self, tz)
    }
}

impl Ask for wire::IndexStatusReq {
    const CAPABILITY: &'static str = "files.index.status";
    type Reply = wire::IndexStatusReply;
    fn answer(self, _: &crate::common::time::Tz) -> Answer<wire::IndexStatusReply> {
        Ok(index_status())
    }
}

impl Ask for wire::RebuildReq {
    const CAPABILITY: &'static str = "files.index.rebuild";
    type Reply = wire::RebuildReply;
    fn answer(self, _: &crate::common::time::Tz) -> Answer<wire::RebuildReply> {
        index_rebuild(self)
    }
}

impl Ask for wire::BrowseReq {
    const CAPABILITY: &'static str = "files.browse";
    type Reply = wire::BrowseReply;
    fn answer(self, _: &crate::common::time::Tz) -> Answer<wire::BrowseReply> {
        browse(self)
    }
}

impl Ask for wire::ReadTextReq {
    const CAPABILITY: &'static str = "files.read.text";
    type Reply = wire::ReadTextReply;
    fn answer(self, _: &crate::common::time::Tz) -> Answer<wire::ReadTextReply> {
        read_text(self)
    }
}

impl Ask for wire::HomeReq {
    const CAPABILITY: &'static str = "files.home";
    type Reply = wire::FilesPathReply;
    fn answer(self, _: &crate::common::time::Tz) -> Answer<wire::FilesPathReply> {
        home()
    }
}

impl Ask for wire::SizeReq {
    const CAPABILITY: &'static str = "files.size";
    type Reply = wire::SizeReply;
    fn answer(self, _: &crate::common::time::Tz) -> Answer<wire::SizeReply> {
        size_of(self)
    }
}

impl Ask for wire::ReadChunkReq {
    const CAPABILITY: &'static str = "files.read.chunk";
    type Reply = wire::ReadChunkReply;
    fn answer(self, _: &crate::common::time::Tz) -> Answer<wire::ReadChunkReply> {
        read_chunk(self)
    }
}

/// `files.grep` 的同步那一臂（取消不掉；命令面走 [`answer_grep_cancellable`]）。
impl Ask for wire::GrepReq {
    const CAPABILITY: &'static str = "files.grep";
    type Reply = wire::GrepReply;
    fn answer(self, _: &crate::common::time::Tz) -> Answer<wire::GrepReply> {
        grep_with(&self, &std::sync::atomic::AtomicBool::new(false))
    }
}

/// 🔴 **这一族的唯一入口**（命令表文件管理那一族经它调读的那几条）。
pub fn answer<A: Ask>(req: A, tz: &crate::common::time::Tz) -> Answer<A::Reply> {
    req.answer(tz)
}

#[cfg(test)]
#[path = "../../../tests/backend/files/answer_tests.rs"]
pub(crate) mod tests;

// **文件管理后端模块**与原生后端那条边界的判据
// （用户逐字「后端要模块化, 即原生后端＋文件管理后端. 现在先解耦清楚」）。
// 挂在这里而不是 `lib.rs`：那份文件的模块声明那几行归另一路（A1）。
#[cfg(test)]
#[path = "../../../tests/backend/files/module_boundary_guard.rs"]
mod module_boundary_guard;
