//! **`files.grep`：在一个目录底下按内容搜** ——
//! 「文件管理器**做**按内容搜（那台后端执行、有字节与条数上界、可撤；标准文件管理器能力）」。
//!
//! 在那台机器上走一遍、只回命中的那几份文件（每份一行：第一处命中所在的行号 ＋ 那一行的一段 ＋ 这份里命中了几行），
//! 字节不过网（「零流量」同 `files.size`）。纯读，整族照旧一个字节不写（边界①）。
//!
//! # 走法（逐条）
//!
//! - **显式栈，不递归**（理由同 `index::build`：目录深度是用户数据说了算的）；同一层按名字字节排序（结果可复现）。
//! - **不跟链接**：链接记数（`links`），不读它指向的东西 —— 它不一定在这棵树里，跟进去就是出界（㉜「不跟符号链接出界」）。
//!   顶上那一格也按链接本身判（`symlink_metadata`）：根是一条链接 ⇒ 不进去。
//! - 挂在底下的**别的文件系统不进去**（`skipped_mounts`，设备号走 `platform::paths::device_of`，同 `files.size`）。
//! - 只看普通文件；看着像二进制（前 [`BINARY_SNIFF_LEN`] 字节里有 NUL）⇒ 跳过记数（`skipped_binary`）；
//!   比 [`FILE_MAX_BYTES`] 大 ⇒ 跳过记数（`skipped_large`）；读不了 ⇒ `unreadable`。都不中断。
//! - 匹配：字节子串，可选只忽略 ASCII 大小写；按行（`\n` 切）数命中行。
//!
//! # 上界（㉜「有字节与条数上界」）
//!
//! - 条数：命中文件数到 `limit`（缺省 [`DEFAULT_LIMIT`]，至多 [`MAX_LIMIT`]）就停 ⇒ `stopped: "hits"`；
//! - 字节：一趟累计读进来的字节到 [`TOTAL_MAX_BYTES`] 就停 ⇒ `stopped: "bytes"`；单份超过 [`FILE_MAX_BYTES`] 不读；
//! - 停了 ⇒ `truncated: true`（后面还有没看的），界面要说出来。
//!
//! # 可撤（㉜「可撤」）
//!
//! 入口吃一个取消位（[`search_with`] 的 `cancel`），每进一项看一次：置位 ⇒ 当场收手。帧面那一臂（`files-grep`，异步档）
//! 在被 `cancel` 掉时（future 被丢）把这个位置上（`mod.rs::answer_grep_cancellable` 的守卫），阻塞线程上的这一趟随即停。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// 缺省命中文件数上限。
pub const DEFAULT_LIMIT: usize = 200;
/// 调用方给的 `limit` 最多到这里（再大就压回来）。
pub const MAX_LIMIT: usize = 1000;
/// 单份文件超过这个字节数不读（`skipped_large`）。
pub const FILE_MAX_BYTES: u64 = 8 << 20;
/// 一趟累计读进来的字节上限（到了就停，`stopped: "bytes"`）。
pub const TOTAL_MAX_BYTES: u64 = 256 << 20;
/// 判二进制看开头这么长（字节数）。⚠ 它不是一道上界（什么都不因它被截掉）：只是「看多远才算看过开头」。
pub const BINARY_SNIFF_LEN: usize = 8 << 10;
/// 要找的那一串最长多少字节。
pub const NEEDLE_MAX_BYTES: usize = 256;
/// 回送的那一段（命中所在的那一行里、命中前后一截）最长多少字节。
pub const SNIPPET_MAX_BYTES: usize = 200;

/// 一份命中的文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepHit {
    /// 整条路径的字节。
    pub path: Vec<u8>,
    /// 第一处命中所在的行号（从 1 起）。
    pub line: u64,
    /// 那一行里命中前后的一截（至多 [`SNIPPET_MAX_BYTES`]；首尾各剥掉空白）。
    pub text: Vec<u8>,
    /// 这份里命中了几行。
    pub matches: u64,
    /// 前 [`LINES_PER_FILE`] 处命中：行号 · 那一行的一截 · 那一截里命中的字节区间。
    pub lines: Vec<GrepLine>,
}

/// 一处命中。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepLine {
    pub line: u64,
    pub text: Vec<u8>,
    /// `text` 里对上 `needle` 的那一段 `[起, 止)`（剥空白后找不回 ⇒ `None`）。
    pub mark: Option<(usize, usize)>,
}

/// 每份文件最多回几处命中（其余只数进 `matches`）。
pub const LINES_PER_FILE: usize = 20;

/// 为什么停在这里。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    /// 走完了。
    Done,
    /// 命中文件数到了上限。
    Hits,
    /// 读进来的字节到了上限。
    Bytes,
    /// 被撤了。
    Cancelled,
}

/// 一趟下来的全部。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grepped {
    pub hits: Vec<GrepHit>,
    pub files: u64,
    pub bytes: u64,
    pub links: u64,
    pub skipped_binary: u64,
    pub skipped_large: u64,
    pub skipped_mounts: u64,
    pub unreadable: u64,
    pub stopped: Stopped,
}

/// 一趟的入参（已解析）。
#[derive(Debug, Clone)]
pub struct GrepArgs {
    pub needle: Vec<u8>,
    pub ignore_ascii_case: bool,
    pub limit: usize,
}

/// 两道字节上界（生产 = [`CAPS`]；判据给小的，好在小语料上真的撞到它们）。
#[derive(Debug, Clone, Copy)]
pub struct Caps {
    pub file_max_bytes: u64,
    pub total_max_bytes: u64,
}

/// 生产那两道上界（真正按它们停 / 跳的地方在 `look_at`：跳过的那一份 `warn!` 说出路径，停了回 `stopped`）。
#[rustfmt::skip]
pub const CAPS: Caps = Caps { file_max_bytes: FILE_MAX_BYTES, total_max_bytes: TOTAL_MAX_BYTES };

/// 生产入口：设备号走 `platform/`、上界是 [`CAPS`]。顶上那一格读不到 ⇒ `Err`（调用方回 `unreadable`）。
pub fn search(top: &Path, args: &GrepArgs, cancel: &AtomicBool) -> std::io::Result<Grepped> {
    search_with(top, args, cancel, crate::platform::paths::device_of, CAPS)
}

/// [`search`] 的本体，设备号与上界由调用方给（判据注入「这个子目录在另一个设备上」· 小上界）。
pub fn search_with(
    top: &Path,
    args: &GrepArgs,
    cancel: &AtomicBool,
    device_of: impl Fn(&Path) -> Option<u64>,
    caps: Caps,
) -> std::io::Result<Grepped> {
    // 按链接本身判（不跟）：`Path` 上那个方法，不走 `std` 文件系统模块那一形（本族的副作用派生按那几个前缀认写动词）。
    let md = top.symlink_metadata()?;
    let mut g = Grepped {
        hits: Vec::new(),
        files: 0,
        bytes: 0,
        links: 0,
        skipped_binary: 0,
        skipped_large: 0,
        skipped_mounts: 0,
        unreadable: 0,
        stopped: Stopped::Done,
    };
    if md.file_type().is_symlink() {
        g.links = 1;
        return Ok(g);
    }
    if !md.is_dir() {
        if md.is_file() {
            look_at(top, md.len(), args, caps, &mut g);
        }
        return Ok(g);
    }
    let top_dev = device_of(top);
    let mut stack: Vec<PathBuf> = vec![top.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            g.unreadable += 1;
            continue;
        };
        let mut entries: Vec<_> = listing.flatten().collect();
        entries.sort_by(|a, b| {
            super::raw::path_bytes(Path::new(&a.file_name()))
                .cmp(super::raw::path_bytes(Path::new(&b.file_name())))
        });
        let mut subdirs: Vec<PathBuf> = Vec::new();
        for entry in entries {
            if cancel.load(Ordering::Relaxed) {
                g.stopped = Stopped::Cancelled;
                return Ok(g);
            }
            let Ok(ft) = entry.file_type() else {
                g.unreadable += 1;
                continue;
            };
            let path = entry.path();
            if ft.is_symlink() {
                g.links += 1;
            } else if ft.is_dir() {
                if top_dev.is_some() && device_of(&path) != top_dev {
                    g.skipped_mounts += 1;
                    continue;
                }
                subdirs.push(path);
            } else if ft.is_file() {
                let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
                look_at(&path, len, args, caps, &mut g);
                if g.stopped != Stopped::Done {
                    return Ok(g);
                }
            }
        }
        // 栈是后进先出：倒着压，出栈就是按名字的正序。
        stack.extend(subdirs.into_iter().rev());
    }
    Ok(g)
}

/// 看一份普通文件（`len` = 它的表观大小）。上界到了就把 `g.stopped` 置上。
fn look_at(path: &Path, len: u64, args: &GrepArgs, caps: Caps, g: &mut Grepped) {
    if len > caps.file_max_bytes {
        // 跳过要说清是哪一份（字节上界那一档的纪律）；少见（单份 > 8 MiB），不会刷屏。
        tracing::warn!(
            "files.grep: skip a file larger than {} bytes ({len} bytes): {}",
            caps.file_max_bytes,
            super::raw::to_json(super::raw::path_bytes(path))
        );
        g.skipped_large += 1;
        return;
    }
    if g.bytes.saturating_add(len) > caps.total_max_bytes {
        g.stopped = Stopped::Bytes;
        return;
    }
    let Ok(body) = std::fs::read(path) else {
        g.unreadable += 1;
        return;
    };
    g.files += 1;
    g.bytes = g.bytes.saturating_add(body.len() as u64);
    if body[..body.len().min(BINARY_SNIFF_LEN)].contains(&0) {
        g.skipped_binary += 1;
        return;
    }
    let mut lines: Vec<GrepLine> = Vec::new();
    let mut matches = 0u64;
    for (i, line) in body.split(|b| *b == b'\n').enumerate() {
        if let Some(at) = find_at(line, &args.needle, args.ignore_ascii_case) {
            matches += 1;
            if lines.len() < LINES_PER_FILE {
                let text = snippet(line, at, args.needle.len());
                let mark = find_at(&text, &args.needle, args.ignore_ascii_case)
                    .map(|a| (a, a + args.needle.len()));
                lines.push(GrepLine {
                    line: i as u64 + 1,
                    text,
                    mark,
                });
            }
        }
    }
    if let Some(first) = lines.first() {
        g.hits.push(GrepHit {
            path: super::raw::path_bytes(path).to_vec(),
            line: first.line,
            text: first.text.clone(),
            matches,
            lines,
        });
        if g.hits.len() >= args.limit {
            g.stopped = Stopped::Hits;
        }
    }
}

/// `needle` 在 `hay` 里第一次出现的偏移（可选只忽略 ASCII 大小写）。空串 ⇒ 0。
pub fn find_at(hay: &[u8], needle: &[u8], ignore_ascii_case: bool) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| {
        let w = &hay[i..i + needle.len()];
        if ignore_ascii_case {
            w.eq_ignore_ascii_case(needle)
        } else {
            w == needle
        }
    })
}

/// 那一行里命中前后的一截：至多 [`SNIPPET_MAX_BYTES`]、命中尽量居中；那一行是有效 UTF-8 ⇒ 边界对齐到字符；首尾剥空白与 `\r`。
pub fn snippet(line: &[u8], at: usize, n: usize) -> Vec<u8> {
    let room = SNIPPET_MAX_BYTES.saturating_sub(n);
    let mut start = at.saturating_sub(room / 2);
    let mut end = (at + n + (room - (at - start))).min(line.len());
    if end - start > SNIPPET_MAX_BYTES {
        end = start + SNIPPET_MAX_BYTES;
    }
    if let Ok(s) = std::str::from_utf8(line) {
        while start > 0 && !s.is_char_boundary(start) {
            start -= 1;
        }
        while end < line.len() && !s.is_char_boundary(end) {
            end += 1;
        }
    }
    let cut = &line[start..end];
    let b = cut
        .iter()
        .position(|c| !c.is_ascii_whitespace())
        .unwrap_or(cut.len());
    let e = cut
        .iter()
        .rposition(|c| !c.is_ascii_whitespace())
        .map_or(b, |p| p + 1);
    cut[b..e.max(b)].to_vec()
}

#[cfg(test)]
#[path = "../../../tests/backend/files/grep_tests.rs"]
mod tests;
