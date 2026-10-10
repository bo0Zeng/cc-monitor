//! Linux 实现：进程表读 `/proc/<pid>/fd` 与 `fdinfo`；动静走 inotify（订新建 · 打开 · 关闭 · 移入，不订改动 ——
//! 追加写一行不该引出任何东西）。整个文件 `#![cfg(target_os = "linux")]`。
//!
//! # 一批怎么攒（不靠时间）
//!
//! 线程无期限地等 inotify 醒；醒了把此刻排着的事件一口气读完，这就是一批（[`Fold`]）。批里「打开了又关掉」的当场抵掉
//! （读的人：建历史清单、按偏移读，都是读完就关）—— 只有这一批读完时还开着的才报成「打开了」。等的过程中来的动静留在内核队列里，
//! 下一次醒来一起读 ⇒ 调用方处理上一批（扫一趟进程表）的耗时就是合并窗口。

#![cfg(target_os = "linux")]

use super::{OpenBatch, Writer, Writers};
use std::collections::{BTreeSet, HashMap};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};

/// 一趟扫完本机进程表：`wanted` 认的文件此刻各被哪些进程开着写。`/proc` 读不了 ⇒ `None`（判不了）。
/// 别人的进程读不了它的 fd 表 ⇒ 跳过（同一个用户的 agent 进程总读得到）。
pub(crate) fn writers_of(wanted: &dyn Fn(&Path) -> bool) -> Option<Writers> {
    let procs = std::fs::read_dir("/proc").ok()?;
    let mut out: Writers = HashMap::new();
    for ent in procs.flatten() {
        let Some(pid) = ent.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(fds) = std::fs::read_dir(ent.path().join("fd")) else {
            continue;
        };
        let mut start: Option<Option<u64>> = None;
        for fd in fds.flatten() {
            let Ok(target) = std::fs::read_link(fd.path()) else {
                continue;
            };
            if !wanted(&target) {
                continue;
            }
            let info = std::fs::read_to_string(ent.path().join("fdinfo").join(fd.file_name()))
                .unwrap_or_default();
            if !opened_for_write(&info) {
                continue;
            }
            let start = *start.get_or_insert_with(|| crate::platform::proc::proc_starttime(pid));
            let list = out.entry(target).or_default();
            if !list.iter().any(|w| w.pid == pid) {
                list.push(Writer { pid, start });
            }
        }
    }
    for v in out.values_mut() {
        v.sort_by_key(|w| w.pid);
    }
    Some(out)
}

/// `fdinfo` 那一份文本 ⇒ 这个 fd 是不是开着写的（`flags:` 是八进制，读写位是低两位）。认不出 ⇒ 不算。
pub(crate) fn opened_for_write(fdinfo: &str) -> bool {
    fdinfo
        .lines()
        .find_map(|l| l.strip_prefix("flags:"))
        .and_then(|v| u32::from_str_radix(v.trim(), 8).ok())
        .is_some_and(|f| {
            let acc = f & libc::O_ACCMODE as u32;
            acc == libc::O_WRONLY as u32 || acc == libc::O_RDWR as u32
        })
}

/// 树里每个目录上订的那几种。
const TREE_MASK: u32 = libc::IN_CREATE
    | libc::IN_OPEN
    | libc::IN_CLOSE_WRITE
    | libc::IN_CLOSE_NOWRITE
    | libc::IN_MOVED_TO;
/// 根还不在时，在它最近一个已在的祖先上订的那几种（等路上那一层出现）。
const WAIT_MASK: u32 = libc::IN_CREATE | libc::IN_MOVED_TO | libc::IN_ONLYDIR;

/// 一道活着的耳朵。一丢 ⇒ 收的线程跟着退出（经停机那根管子叫醒它，不靠超时）。
pub(crate) struct OpenEar {
    stop: OwnedFd,
}

impl Drop for OpenEar {
    fn drop(&mut self) {
        let b = [1u8];
        // SAFETY：`stop` 是本结构独占的管子写端；写一个字节、不解引用别的指针。
        let n = unsafe { libc::write(self.stop.as_raw_fd(), b.as_ptr().cast(), 1) };
        if n < 0 {
            // 写不进多半是收的那一头已经退了（管子读端随线程关掉）—— 那正是要的结局，只留一行。
            tracing::debug!(
                "writers-ear 停机信号没写进去: {}",
                std::io::Error::last_os_error()
            );
        }
    }
}

/// 盯 `root` 这棵树（含之后长出来的子目录；根还不在 ⇒ 等它出现）里文件的新建 · 打开 · 写完关闭：
/// 每醒一次交一批（见模块头注），`on_batch` 回 `false` ⇒ 不再听（接的那一头没了）。
/// 新长出来的目录挂上时，里面已有的文件当作「打开了」报一次（挂上之前写进去的那一下听不见）。挂不上 ⇒ `None`。
pub(crate) fn watch_opens(
    root: &Path,
    mut on_batch: impl FnMut(OpenBatch) -> bool + Send + 'static,
) -> Option<OpenEar> {
    // SAFETY：只创建一个新 fd，不解引用指针；<0 按 errno 处理。
    let ino = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    if ino < 0 {
        tracing::warn!("inotify_init1 失败: {}", std::io::Error::last_os_error());
        return None;
    }
    // SAFETY：`ino` 是刚由内核分配、无人持有的合法 fd。
    let ino = unsafe { OwnedFd::from_raw_fd(ino) };
    let mut pipe = [0i32; 2];
    // SAFETY：`pipe` 是两个 i32 的栈数组，与 `pipe2` 要的形状一致。
    if unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC) } < 0 {
        tracing::warn!("pipe2 失败: {}", std::io::Error::last_os_error());
        return None;
    }
    // SAFETY：`pipe2` 成功 ⇒ 两个都是刚分配、无人持有的合法 fd。
    let (stop_rx, stop_tx) =
        unsafe { (OwnedFd::from_raw_fd(pipe[0]), OwnedFd::from_raw_fd(pipe[1])) };
    let mut tree = Tree::new(ino, absolute(root));
    let first = tree.arm();
    let spawned = std::thread::Builder::new()
        .name("writers-ear".to_string())
        .spawn(move || {
            let mut fold = Fold::default();
            let mut out = fold.finish_with(first.into_iter());
            loop {
                if !out.is_empty() && !on_batch(std::mem::take(&mut out)) {
                    return;
                }
                let mut pfd = [
                    libc::pollfd {
                        fd: tree.ino.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: stop_rx.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                ];
                // SAFETY：两个栈上的合法 pollfd，nfds 与之匹配；timeout=-1 = 无限等；两个 fd 的所有权都在本闭包里。
                let n = unsafe { libc::poll(pfd.as_mut_ptr(), 2, -1) };
                if n < 0 {
                    let err = std::io::Error::last_os_error();
                    if err.kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    tracing::warn!("writers-ear poll 失败、不再听: {err}");
                    return;
                }
                if pfd[1].revents != 0 {
                    return;
                }
                if pfd[0].revents != 0 {
                    let found = tree.drain(&mut fold);
                    out = fold.finish_with(found.into_iter());
                }
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("起 writers-ear 线程失败: {e}");
        return None;
    }
    Some(OpenEar { stop: stop_tx })
}

/// 根规范化成绝对路径：在的那一截照 `canonicalize`，还不在的那一截原样接上（readlink 出来的路径与它可比）。
pub(crate) fn absolute(root: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut at = root.to_path_buf();
    loop {
        if let Ok(c) = std::fs::canonicalize(&at) {
            let mut out = c;
            for m in missing.iter().rev() {
                out.push(m);
            }
            return out;
        }
        match (at.file_name().map(|n| n.to_os_string()), at.parent()) {
            (Some(n), Some(parent)) => {
                missing.push(n);
                at = parent.to_path_buf();
            }
            _ => return root.to_path_buf(),
        }
    }
}

/// 一批的折叠：文件级事件逐件进来（[`Fold::apply`]），一批读完收成 [`OpenBatch`]（[`Fold::finish_with`]）。
/// 跨批记着每份文件此刻被开着几次（只数耳朵挂上之后看见的打开；挂上之前就开着的，关掉时不减成负的）。
#[derive(Default)]
pub(crate) struct Fold {
    open: HashMap<PathBuf, u32>,
    touched: BTreeSet<PathBuf>,
    forced: BTreeSet<PathBuf>,
    closed: BTreeSet<PathBuf>,
    overflowed: bool,
}

impl Fold {
    /// 一件文件级的事件（`mask` 是 inotify 的那几位）。
    pub(crate) fn apply(&mut self, p: PathBuf, mask: u32) {
        if mask & libc::IN_OPEN != 0 {
            *self.open.entry(p.clone()).or_default() += 1;
            self.touched.insert(p.clone());
        }
        if mask & libc::IN_CREATE != 0 {
            self.touched.insert(p.clone());
        }
        if mask & libc::IN_MOVED_TO != 0 {
            // 移进来的是一份已经写好的文件：谁开着它，只有进程表说得出。
            self.forced.insert(p.clone());
        }
        if mask & (libc::IN_CLOSE_WRITE | libc::IN_CLOSE_NOWRITE) != 0 {
            if let Some(n) = self.open.get_mut(&p) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    self.open.remove(&p);
                }
            }
        }
        if mask & libc::IN_CLOSE_WRITE != 0 {
            self.closed.insert(p);
        }
    }

    pub(crate) fn overflow(&mut self) {
        self.overflowed = true;
    }

    /// 一批读完：这一批里新建 / 打开过、此刻还开着的 ∪ `found`（新挂上的目录里已有的文件）∪ 移进来的 ⇒ `opened`；写完关闭过的 ⇒ `closed`。
    pub(crate) fn finish_with(&mut self, found: impl Iterator<Item = PathBuf>) -> OpenBatch {
        let touched = std::mem::take(&mut self.touched);
        let mut opened: BTreeSet<PathBuf> = std::mem::take(&mut self.forced);
        opened.extend(touched.into_iter().filter(|p| self.open.contains_key(p)));
        opened.extend(found);
        OpenBatch {
            opened: opened.into_iter().collect(),
            closed: std::mem::take(&mut self.closed).into_iter().collect(),
            overflowed: std::mem::take(&mut self.overflowed),
        }
    }
}

/// 挂着的那几道：树里每个目录一道（[`TREE_MASK`]）；根不在时祖先上一道（[`WAIT_MASK`]）。
struct Tree {
    ino: OwnedFd,
    root: PathBuf,
    dirs: HashMap<i32, PathBuf>,
    waiting: Option<i32>,
}

impl Tree {
    fn new(ino: OwnedFd, root: PathBuf) -> Self {
        Tree {
            ino,
            root,
            dirs: HashMap::new(),
            waiting: None,
        }
    }

    fn add(&self, dir: &Path, mask: u32) -> Option<i32> {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
        // SAFETY：`c` 是以 NUL 收尾的合法 C 串，活过这次调用；`ino` 是本结构独占的 inotify fd。
        let wd = unsafe { libc::inotify_add_watch(self.ino.as_raw_fd(), c.as_ptr(), mask) };
        (wd >= 0).then_some(wd)
    }

    /// 按盘上此刻的样子挂：根在 ⇒ 整棵挂上（交回里面已有的文件）；不在 ⇒ 在最近一个已在的祖先上等。
    fn arm(&mut self) -> Vec<PathBuf> {
        if self.root.is_dir() {
            if let Some(wd) = self.waiting.take() {
                // SAFETY：同 `add`；卸一个本 fd 上挂过的 wd，卸不掉（已随目录没了）无妨。
                unsafe { libc::inotify_rm_watch(self.ino.as_raw_fd(), wd) };
            }
            let root = self.root.clone();
            return self.add_tree(&root);
        }
        let mut at = self.root.parent();
        while let Some(a) = at {
            if a.is_dir() {
                if let Some(wd) = self.add(a, WAIT_MASK) {
                    self.waiting = Some(wd);
                }
                break;
            }
            at = a.parent();
        }
        Vec::new()
    }

    /// 挂上 `top` 和它底下每一层目录，交回里面已有的文件。
    fn add_tree(&mut self, top: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for e in walkdir::WalkDir::new(top)
            .into_iter()
            .filter_map(Result::ok)
        {
            let p = e.path();
            if e.file_type().is_dir() {
                if let Some(wd) = self.add(p, TREE_MASK) {
                    self.dirs.insert(wd, p.to_path_buf());
                }
            } else if e.file_type().is_file() {
                files.push(p.to_path_buf());
            }
        }
        files
    }

    /// 把此刻排着的事件全读出来折进 `fold`；交回这一趟新挂上的目录里已有的文件。
    fn drain(&mut self, fold: &mut Fold) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            // SAFETY：`buf` 是一段可写的堆内存，长度如实交出；`ino` 是本结构独占的 fd（非阻塞）。
            let n = unsafe { libc::read(self.ino.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                return found;
            }
            let n = n as usize;
            let mut o = 0;
            while o + 16 <= n {
                let word = |i: usize| {
                    u32::from_ne_bytes([buf[o + i], buf[o + i + 1], buf[o + i + 2], buf[o + i + 3]])
                };
                let wd = word(0) as i32;
                let mask = word(4);
                let len = word(12) as usize;
                let name_end = (o + 16 + len).min(n);
                let raw = &buf[o + 16..name_end];
                let name = &raw[..raw.iter().position(|&b| b == 0).unwrap_or(raw.len())];
                self.on_event(wd, mask, name, fold, &mut found);
                o = name_end;
            }
        }
    }

    fn on_event(
        &mut self,
        wd: i32,
        mask: u32,
        name: &[u8],
        fold: &mut Fold,
        found: &mut Vec<PathBuf>,
    ) {
        use std::os::unix::ffi::OsStrExt;
        if mask & libc::IN_Q_OVERFLOW != 0 {
            fold.overflow();
            return;
        }
        if Some(wd) == self.waiting {
            // 祖先上那一道：路上那一层出现了 ⇒ 按盘上此刻的样子重挂（根整棵出现了就交回里面已有的文件）。
            if mask & (libc::IN_CREATE | libc::IN_MOVED_TO) != 0 {
                self.waiting = None;
                // SAFETY：同 `arm`。
                unsafe { libc::inotify_rm_watch(self.ino.as_raw_fd(), wd) };
                found.extend(self.arm());
            }
            return;
        }
        if mask & libc::IN_IGNORED != 0 {
            // 这一道随目录没了：是根 ⇒ 回去等它再出现。
            if self.dirs.remove(&wd).as_deref() == Some(self.root.as_path()) {
                self.dirs.clear();
                found.extend(self.arm());
            }
            return;
        }
        let Some(dir) = self.dirs.get(&wd) else {
            return;
        };
        if name.is_empty() {
            return; // 目录自己被打开（列目录）之类：不是文件的动静
        }
        let p = dir.join(std::ffi::OsStr::from_bytes(name));
        if mask & libc::IN_ISDIR != 0 {
            if mask & (libc::IN_CREATE | libc::IN_MOVED_TO) != 0 {
                found.extend(self.add_tree(&p));
            }
            return;
        }
        fold.apply(p, mask);
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/platform/writers/linux_tests.rs"]
mod tests;
