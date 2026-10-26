//! **一趟**：后台跑的一件活（上传一摞 · 下载 · 一组 · 解压 · 算大小）在窗口上的那一份状态，与它落地时的那一个结局。
//!
//! - [`Lane`]：各块看板共用的那一格 —— 在做哪一项 · 落地了几趟 · 上一趟的结局 · 敲窗口的那只手。
//!   UI 线程读、tokio 那条写；看板只在它外面加自己那一类独有的（进度读数 · 撞名那一问 · 取消台）。
//! - [`End`] · [`Landing`]：一趟落地时**只判一次**的结局（成了 · 失败 · 停了 ＋ 右下角那一句回执）。
//!   进度表那一行（[`super::progress::Job::landing`]）与开它的那个标签页收场（`FileWindow::settle_trips`）读的是同一份，
//!   不各写一份判法。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// 一趟落地时的结局。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Done,
    Failed,
    Stopped,
}

/// 一趟落地：结局 ＋ 收场时右下角那一句回执（`None` ＝ 不出回执：没成的看「进度」表那一行）。
#[derive(Clone, Debug, PartialEq)]
pub struct Landing {
    pub end: End,
    pub receipt: Option<String>,
}

struct Slot<O> {
    running: Option<String>,
    last: Option<O>,
}

/// 一趟的那一格（跨线程共享，`Clone` 拿到的是同一份）。
pub struct Lane<O> {
    slot: Arc<Mutex<Slot<O>>>,
    /// 落地了几趟 —— 先落结局、再加它（读的那一侧看见 `rounds > 0` 时结局一定已经在了）。
    rounds: Arc<AtomicU64>,
    /// 🔴 **拿来敲窗口的那只手**：结局与进度是 tokio 那条写进来的，而 egui 只在有事发生时才画下一帧 ——
    /// 不敲一下，那一行要等到用户下次动鼠标才变。判据里没有窗口 ⇒ `None`，照常记。
    ctx: Arc<Mutex<Option<egui::Context>>>,
}

impl<O> Clone for Lane<O> {
    fn clone(&self) -> Self {
        Lane {
            slot: self.slot.clone(),
            rounds: self.rounds.clone(),
            ctx: self.ctx.clone(),
        }
    }
}

impl<O> Default for Lane<O> {
    fn default() -> Self {
        Lane {
            slot: Arc::new(Mutex::new(Slot {
                running: None,
                last: None,
            })),
            rounds: Arc::default(),
            ctx: Arc::default(),
        }
    }
}

impl<O: Clone> Lane<O> {
    /// 毒化容忍：一次 panic 不该让后面每一帧都跟着 panic。
    fn lock(&self) -> MutexGuard<'_, Slot<O>> {
        match self.slot.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    /// 新的一格（新的一趟），敲窗口的那只手照抄这一格的。
    pub fn fresh_like(&self) -> Lane<O> {
        let next = Lane::default();
        next.attach(self.ctx());
        next
    }

    fn ctx(&self) -> Option<egui::Context> {
        match self.ctx.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// 把窗口交给它，好让它在有新东西时敲一下。
    pub fn attach(&self, ctx: Option<egui::Context>) {
        match self.ctx.lock() {
            Ok(mut g) => *g = ctx,
            Err(p) => *p.into_inner() = ctx,
        }
    }

    /// 敲一下窗口：「有新东西了，画下一帧」。⚠ 别在拿着别的锁时调（`request_repaint` 走进 egui 自己的锁）。
    pub fn poke(&self) {
        if let Some(c) = self.ctx() {
            c.request_repaint();
        }
    }

    /// 开始做哪一项（那一行的副标题）。
    pub fn begin(&self, name: &str) {
        self.lock().running = Some(name.to_string());
        self.poke();
    }

    /// 在做哪一项（`None` ＝ 没在做）。
    pub fn running(&self) -> Option<String> {
        self.lock().running.clone()
    }

    /// 落地：记下结局、趟数加一、敲窗口。
    pub fn finish(&self, o: O) {
        {
            let mut s = self.lock();
            s.running = None;
            s.last = Some(o);
        }
        self.rounds.fetch_add(1, Ordering::SeqCst);
        self.poke();
    }

    /// 落地了几趟。
    pub fn rounds(&self) -> u64 {
        self.rounds.load(Ordering::SeqCst)
    }

    /// 上一趟的结局。
    pub fn last(&self) -> Option<O> {
        self.lock().last.clone()
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/trip_tests.rs"]
mod tests;
