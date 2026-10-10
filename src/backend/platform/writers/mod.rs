//! 「谁开着哪份文件写」这一族平台原语：一趟扫完本机进程表，答几份文件此刻各被哪些进程开着写（[`writers_of`]）；
//! 盯一棵目录树里文件的新建 · 打开 · 写完关闭这三种动静，攒一小批交一次（[`watch_opens`]）。
//!
//! 用它的：会话流的 watcher —— 有的 agent 不留 pidfile，它的会话在世期间写记录的那个进程一直开着那份记录
//! （适配层声明，`agents::RecordFace::held_open`）⇒「有进程开着它写」就是这条会话活着的事实，那个进程退了（pidfd）
//! 或把它关了（写完关闭）就是它走了。
//!
//! 认「写」看的是打开方式（Linux：`/proc/<pid>/fdinfo/<fd>` 的 `flags` 里的读写位），不看进程名像不像谁。
//!
//! # 按平台分文件
//!
//! `linux` 是 `/proc` ＋ inotify 实现；`fallback` 是别的平台的诚实空壳：[`writers_of`] 答 `None`（判不了，不是「没人在写」），
//! [`watch_opens`] 不挂（调用方据此不宣告这一类会话，帧上不冒充判过）。

use std::collections::HashMap;
use std::path::PathBuf;

#[cfg(not(target_os = "linux"))]
mod fallback;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(not(target_os = "linux"))]
pub(crate) use fallback::{absolute, watch_opens, writers_of, OpenEar};
#[cfg(target_os = "linux")]
pub(crate) use linux::{absolute, watch_opens, writers_of, OpenEar};

/// 开着某份文件写的一个进程：pid ＋ 它的启动时刻（挂 pidfd 看守时挡 PID 复用用；读不到 ⇒ `None`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Writer {
    pub(crate) pid: u32,
    pub(crate) start: Option<u64>,
}

/// 一趟扫出来的：每份被开着写的文件 ⇒ 开着它写的那几个进程（pid 升序）。没人写的文件不在里面。
pub(crate) type Writers = HashMap<PathBuf, Vec<Writer>>;

/// [`watch_opens`] 交的一批动静（同一批里同一份文件只记一次）。路径是绝对路径（根先规范化过）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OpenBatch {
    /// 新建了或被打开了的文件（谁打开的、读还是写，这一层不知道 —— 要的话扫一趟 [`writers_of`]）。
    pub(crate) opened: Vec<PathBuf>,
    /// 被一个开着它写的进程关掉了的文件。
    pub(crate) closed: Vec<PathBuf>,
    /// 内核那一侧的事件队列溢出过：这一批之前的动静可能丢了 ⇒ 调用方整树重扫一趟。
    pub(crate) overflowed: bool,
}

impl OpenBatch {
    pub(crate) fn is_empty(&self) -> bool {
        self.opened.is_empty() && self.closed.is_empty() && !self.overflowed
    }
}
