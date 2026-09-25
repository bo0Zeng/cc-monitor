//! 〔NT2 · S1〕**脱离常驻那条载体的后端：stderr 落进一份有上限、滚动的文件**（`设计/15 §4.7 S1`）。
//!
//! # 为什么
//!
//! 本机后端脱离常驻时 stdio 全 null（monitor 那侧 `spawn_detached`：管子接到 monitor ⇒ monitor 一退它就 broken-pipe 退出）。
//! 而全部 SSH 与本机中转都在这个进程里 ⇒ host key 警告、中转起不来的原因、watch 失败、panic —— 全进 `/dev/null`。
//! `15 §S1` 逐字：「受害的都是事件源的失效告知 …… 说出来了，没人听」。主会话 4C 第二批裁：落本机日志文件（有上限、滚动），
//! 设置页「日志」里看得到。
//!
//! # 形状
//!
//! - **谁决定写不写、写哪**：宿主交 [`ENV`]（monitor 只在起**脱离**那条载体时交：路径 `<monitor 数据目录>/logs/backend/stderr.log`，
//!   那一层目录由它建好）。没交 ⇒ 什么都不做（stdio 载体的 stderr 已经进 monitor 滚动日志；远端经 SSH exec 起的流模式没人交）。
//! - **接**：fd 2 换成一根管子的写端（`platform::stderr_pipe`），一条线程阻塞读管子（内核事件，零定时器）、写进文件。
//!   ⇒ 本进程此后一切写 stderr 的（`tracing` · `eprintln!` · panic · 继承 stderr 的子进程）都落盘。
//! - **上限 ＋ 滚动**：每份 ≤ [`CAP_BYTES`]；这一块写进去会超 ⇒ 当前那份原子挪成旧的（盖掉上一份旧的）、`O_EXCL` 新建一份当前的。
//!   盘上恒 ≤ 两份。起来那一刻已有当前那份（上一次常驻留下的）⇒ 先挪成旧的 —— 上一次的最后一段留着看。
//! - **写不进去不拖垮后端**：建不了 / 写失败 ⇒ 照旧把管子读空（丢字节、记数、下一块再试着新建），
//!   绝不让写 stderr 的那一方堵在一根满了的管子上。读线程不 `panic`。
//!
//! # 后端自有状态（`readonly_guard` 第四层）
//!
//! 这两份文件是**后端自己的**诊断输出，不是用户数据；路径由宿主交（与 `CCM_HISTORY_METADATA` 同一个先例）。
//! 动词只用第四层的闭集：`O_EXCL` 新建 · 原子挪（不建目录、不截断、不追加）。写口 [`install_from_env`] 只从 `main.rs` 进。
//!
//! # 买不到（如实）
//!
//! - 进程被 `SIGKILL` / abort 的那一刻还在管子里的几行落不了盘。
//! - 远端后端的 stderr（`15 §S1` 表后两行）：本件不接。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// 宿主交的那一格：当前那份文件的**完整路径**。缺席 / 空白 ⇒ 不接。
pub const ENV: &str = "CCM_BACKEND_STDERR_LOG";

/// 每份的字节上限（当前那份 ＋ 旧的那份 ⇒ 盘上 ≤ 两倍）。
pub const CAP_BYTES: u64 = 4 << 20;

/// 一次从管子里读多少。
const CHUNK: usize = 16 * 1024;

/// 当前那份的路径 ⇒ 旧的那份的路径（`stderr.log` ⇒ `stderr.old.log`）。
pub(crate) fn old_path_of(cur: &Path) -> PathBuf {
    cur.with_extension("old.log")
}

/// 滚过一次之后，新那份的第一行。
pub(crate) fn roll_note(old: &Path) -> String {
    format!(
        "[stderr-log] 上一份挪成 {}（每份上限 {CAP_BYTES} 字节）；再早的那一份丢了\n",
        old.display()
    )
}

/// 写那两份文件的那一半（纯文件 I/O，判据直接喂它字节）。
pub(crate) struct Roller {
    cur: PathBuf,
    old: PathBuf,
    cap: u64,
    file: Option<std::fs::File>,
    written: u64,
    /// 写不进去而丢掉的字节（只增）。
    dropped: u64,
}

impl Roller {
    /// 起：已有当前那份 ⇒ 挪成旧的；新建一份当前的。建不了也照样回一个（之后每一块都再试一次）。
    pub(crate) fn start(cur: PathBuf, cap: u64) -> Roller {
        let old = old_path_of(&cur);
        let mut r = Roller {
            cur,
            old,
            cap,
            file: None,
            written: 0,
            dropped: 0,
        };
        r.roll();
        r
    }

    /// 当前那份挪成旧的（不在就不挪），再 `O_EXCL` 新建一份当前的。
    /// 真挪了一份 ⇒ 新那份的第一行说清「上一份挪去了哪、再早的那一份丢了」（丢要带身份，不静默）。
    fn roll(&mut self) {
        self.file = None;
        self.written = 0;
        let moved = match std::fs::rename(&self.cur, &self.old) {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            // 挪不动 ⇒ 新建也会撞上它（O_EXCL），这一块丢掉，下一块再试
            Err(_) => return,
        };
        self.file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.cur)
            .ok();
        if moved {
            let note = roll_note(&self.old);
            if let Some(f) = self.file.as_mut() {
                if f.write_all(note.as_bytes()).is_ok() {
                    self.written = note.len() as u64;
                }
            }
        }
    }

    /// 写一块。这一块写进去会超上限（且当前那份不是空的）⇒ 先滚。
    pub(crate) fn write(&mut self, chunk: &[u8]) {
        if chunk.is_empty() {
            return;
        }
        if self.file.is_none() || (self.written > 0 && self.written + chunk.len() as u64 > self.cap)
        {
            self.roll();
        }
        let ok = match self.file.as_mut() {
            Some(f) => f.write_all(chunk).is_ok(),
            None => false,
        };
        if ok {
            self.written += chunk.len() as u64;
        } else {
            self.file = None;
            self.dropped += chunk.len() as u64;
        }
    }

    /// 丢掉的字节数（判据用）。
    pub(crate) fn dropped(&self) -> u64 {
        self.dropped
    }
}

/// 把 `from` 读到底，一块一块交给 `roller`。读出错（`Interrupted` 之外）或 EOF 才返回。
pub(crate) fn pump<R: Read>(mut from: R, mut roller: Roller) -> Roller {
    let mut buf = vec![0u8; CHUNK];
    loop {
        match from.read(&mut buf) {
            Ok(0) => return roller,
            Ok(n) => roller.write(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return roller,
        }
    }
}

/// 装上之后的结局（给宿主那一句日志；它本身也会落进那份文件）。
#[derive(Debug, PartialEq, Eq)]
pub enum Installed {
    /// 没被交路径 ⇒ stderr 原样不动。
    NotAsked,
    /// 接好了：此后 stderr 落在这份文件里。
    Logging(PathBuf),
    /// 被交了路径但接不上（非 unix / 建不了管子）⇒ stderr 原样不动。
    Failed(String),
}

impl Installed {
    pub fn said(&self) -> String {
        match self {
            Installed::NotAsked => "[stderr] 没被交日志路径 ⇒ stderr 原样".to_string(),
            Installed::Logging(p) => format!(
                "[stderr] 本进程的输出落在 {}（每份上限 {} 字节，留一份旧的）",
                p.display(),
                CAP_BYTES
            ),
            Installed::Failed(why) => format!("[stderr] 被交了日志路径但接不上：{why}"),
        }
    }
}

/// **写口**：宿主交了 [`ENV`] ⇒ 把 fd 2 接进管子、起读线程落盘。只从 `main.rs` 流模式那一处进（第四层那扇门）。
pub fn install_from_env(get: &dyn Fn(&str) -> Option<String>) -> Installed {
    let Some(path) = get(ENV).filter(|v| !v.trim().is_empty()) else {
        return Installed::NotAsked;
    };
    let cur = PathBuf::from(path);
    // 次序是承重的：**先有读的人，再换 fd**。反过来的话，线程起不来那一刻 fd 2 已经指着一根没人读的管子，
    // 写满 64 KiB 之后本进程每一个写 stderr 的都会堵住。
    let (hand, take) = std::sync::mpsc::sync_channel::<std::io::PipeReader>(1);
    let target = cur.clone();
    let spawned = std::thread::Builder::new()
        .name("stderr-log".into())
        .spawn(move || {
            // 交不来读端（换 fd 失败）⇒ 发送端被丢 ⇒ 这里拿到 `Err`，线程就此收工。
            if let Ok(rd) = take.recv() {
                pump(rd, Roller::start(target, CAP_BYTES));
            }
        });
    if let Err(e) = spawned {
        return Installed::Failed(format!("起不了读线程：{e}"));
    }
    match crate::platform::stderr_pipe::capture_stderr() {
        Ok(rd) => {
            let _ = hand.send(rd);
            Installed::Logging(cur)
        }
        Err(e) => Installed::Failed(e),
    }
}

#[cfg(test)]
#[path = "../../tests/backend/stderr_log_tests.rs"]
mod tests;
