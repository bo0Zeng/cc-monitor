//! 窗口底部那张「进度」表：**一趟一行**（上传一摞 · 下载 · 机器上复制 · 复制到另一台 · 解压 · 算大小 · 删除），
//! 各自独立、可同时几趟、互不清掉。
//!
//! - 住**窗口那一级**：所有标签页与两栏共用一份（[`super::workspace::Workspace`] 交给每个目录视图同一个 [`Progress`]）。
//!   关掉一个还有一趟在跑的标签页 ⇒ 那一趟照跑，结局照样落在这张表上（每一趟的状态住它自己那块看板里，不住标签页上）。
//! - 每一趟一块**自己的**看板（`DropBoard` · `DownloadBoard` · …，开一趟就新建一块）—— 从前一类只有一块、
//!   第二趟起来会把第一趟的取消、进度与结局一起清掉，所以只能一类一次一趟。
//! - 表里每一行的字、进度、按钮全从那一块看板的读数现算（[`Job::view`]），这里不记第二份。
//! - 停得了停不了：上传 / 下载 / 复制到另一台走传输台（停订即撤）；机器上复制 · 解压 · 算大小 · 删除是那台后端的阻塞档，
//!   它在握手时说了哪几条撤不动（`Offer::withdraw`）⇒「停」灰着、悬停说那台撤不动。
//! - 有失败 ⇒ 表自动展开，状态栏「进度」上一个红点；收起后红点留着，直到人自己再点开看过。已完成的只留最近 50 行。

use std::sync::{Arc, Mutex};

use copy_core::copy_text;

use super::copy::{CopyBoard, CopyOutcome};
use super::cross_copy::CrossBoard;
use super::download::DownloadBoard;
use super::extract::ExtractBoard;
use super::size::SizeBoard;
use super::transfer::{DropBoard, Pending};
use super::writeops::WriteBoard;

/// 已完成的行最多留几行（更早的自动清掉）。
pub const KEEP_FINISHED: usize = 50;

/// 一趟是什么、它那块看板在哪。
#[derive(Clone)]
pub enum Trip {
    /// 上传一摞：`dir` 是传到的那个目录（显示名），`items` 是这一摞（重试失败的那几个要用）。
    Upload {
        board: DropBoard,
        items: Vec<Pending>,
        dir: String,
    },
    /// 下载一个文件到本机 `dest`。
    Download {
        board: DownloadBoard,
        name: String,
        src: String,
        dest: String,
    },
    /// 在那台机器上复制（复制一份 · 复制到另一栏）。
    Copy {
        board: CopyBoard,
        n: usize,
        name: String,
    },
    /// 复制到另一台。
    Cross {
        board: CrossBoard,
        name: String,
        machine: String,
    },
    /// 算大小。
    Size { board: SizeBoard, name: String },
    /// 解压到这里。
    Extract { board: ExtractBoard, name: String },
    /// 删除（这一摞里有文件夹）：那块看板是标签页那一块写操作看板，`base` ＝ 开这一趟时它已经跑完过几趟。
    Delete {
        board: WriteBoard,
        base: u64,
        name: String,
        n: usize,
    },
}

/// 一行此刻的状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Running,
    Done,
    Failed,
    Stopped,
}

/// 一行上那颗按钮按下去做什么。
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    /// 停这一趟。
    Stop(u64),
    /// 重传这一摞里失败的那几个（上传）。
    RetryUpload(Vec<Pending>),
    /// 再下一次（下载）。
    RetryDownload { src: String, dest: String },
    /// 接着传这一摞里没传的（上传停了之后）。
    Resume(Vec<Pending>),
}

/// 一行画成什么（全从看板读数现算，判据与界面看同一份）。
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub state: State,
    pub icon: &'static str,
    pub title: String,
    pub detail: String,
    /// `None` ＝ 不画进度条；`Some(None)` ＝ 不确定进度；`Some(Some(f))` ＝ 走到几成。
    pub frac: Option<Option<f32>>,
    /// 右边那一格（已传 / 共 · 约多久 · 结束的时刻）。
    pub nums: String,
    /// 那颗按钮：字 ＋ 按下做什么；`Err(为什么)` ＝ 灰着、悬停说为什么。
    pub button: Option<(String, Result<Act, String>)>,
    /// ［复制详情］复制出去的整段（首行是标题；没成的那几件各带出错那一端写的详情）；`None` ＝ 不出按钮。
    pub copy: Option<String>,
}

/// 表里的一行。
#[derive(Clone)]
pub struct Job {
    pub id: u64,
    pub trip: Trip,
    /// 这一趟落在哪个远端目录（跑完要重列它）；`None` ＝ 不碰任何目录（下载 · 算大小）。
    pub dir: Option<String>,
    /// 开这一趟的那一刻：算速度 · 约多久。
    pub started: std::time::Instant,
    /// 跑完那一刻（UNIX 秒）；`None` ＝ 还在跑。
    pub ended: Option<u64>,
    /// 失败了还没被看过（展开过表就算看过）。
    pub unseen_fail: bool,
    /// 这一趟落地后由窗口这一级重列目录（开它的那个标签页已经不在了 / 已经换了一块看板）。
    pub orphan: bool,
    /// 那台撤不动这一类 ⇒ 「停」灰着时悬停说的那句（握手那一刻交来的事实）。
    pub no_stop: Option<String>,
}

#[derive(Default)]
struct Inner {
    jobs: Vec<Job>,
    next: u64,
    /// 表摊开着吗。
    open: bool,
}

/// 那张表（一扇窗一份，所有目录视图共用）。
#[derive(Clone, Default)]
pub struct Progress(Arc<Mutex<Inner>>);

impl Progress {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        match self.0.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    /// 是不是同一张表（两个目录视图拿的是不是同一份）。
    pub fn same(&self, other: &Progress) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// 开一行。回它的号。
    pub fn add(&self, trip: Trip, dir: Option<String>, no_stop: Option<String>) -> u64 {
        let mut g = self.lock();
        g.next += 1;
        let id = g.next;
        g.jobs.push(Job {
            id,
            trip,
            dir,
            started: std::time::Instant::now(),
            ended: None,
            unseen_fail: false,
            orphan: false,
            no_stop,
        });
        id
    }

    /// 这一行改由窗口这一级在落地时重列目录（开它的标签页关了 / 那个标签页又开了同一类的下一趟）。
    pub fn orphan(&self, id: u64) {
        if let Some(j) = self.lock().jobs.iter_mut().find(|j| j.id == id) {
            j.orphan = true;
        }
    }

    /// 一份快照（判据与画表用）。
    pub fn jobs(&self) -> Vec<Job> {
        self.lock().jobs.clone()
    }

    /// 摊开着吗。
    pub fn is_open(&self) -> bool {
        self.lock().open
    }

    /// 人点了状态栏那一颗：摊开 / 收起。人自己摊开 ⇒ 失败都算看过了（表自己因失败摊开的那一下不算，[`Self::settle`]）。
    pub fn set_open(&self, on: bool) {
        let mut g = self.lock();
        g.open = on;
        if on {
            for j in &mut g.jobs {
                j.unseen_fail = false;
            }
        }
    }

    /// 在跑的有几趟。
    pub fn running(&self) -> usize {
        self.lock()
            .jobs
            .iter()
            .filter(|j| j.state() == State::Running && j.shown())
            .count()
    }

    /// 表里有没有一行（没有 ⇒ 状态栏那颗「进度」不画）。
    pub fn is_empty(&self) -> bool {
        !self.lock().jobs.iter().any(Job::shown)
    }

    /// 有没有没看过的失败（状态栏那颗上的红点）。
    pub fn unseen_fail(&self) -> bool {
        self.lock().jobs.iter().any(|j| j.unseen_fail)
    }

    /// 在跑的那几趟合起来走到几成（状态栏那颗上的小进度条；说不出 ⇒ `None`）。
    pub fn overall(&self) -> Option<f32> {
        let g = self.lock();
        let (mut got, mut total) = (0u64, 0u64);
        for j in g.jobs.iter().filter(|j| j.state() == State::Running) {
            let (a, b) = j.amounts();
            got += a;
            total += b;
        }
        (total > 0).then(|| got as f32 / total as f32)
    }

    /// 清掉已完成的（成了 · 失败 · 停了）。
    pub fn clear_finished(&self) {
        self.lock().jobs.retain(|j| j.state() == State::Running);
    }

    /// 关窗前那一问用：还在跑的那几趟（标题）。
    pub fn running_titles(&self) -> Vec<String> {
        let g = self.lock();
        g.jobs
            .iter()
            .filter(|j| j.state() == State::Running && j.shown())
            .map(|j| j.title())
            .collect()
    }

    /// 关窗那一问用：还在跑的那几趟，按族（传输中 · 删除中 · 解压中 · 计算大小中），每行 `(图标, 名字, 右端那一格)`。
    pub fn running_for_close(&self) -> Vec<(String, Vec<(&'static str, String, String)>)> {
        use egui_phosphor::regular as ph;
        let g = self.lock();
        let mut groups: Vec<(String, Vec<(&'static str, String, String)>)> = Vec::new();
        let mut put = |group: String, row: (&'static str, String, String)| match groups
            .iter_mut()
            .find(|(t, _)| *t == group)
        {
            Some((_, rows)) => rows.push(row),
            None => groups.push((group, vec![row])),
        };
        let pct = |j: &Job| {
            let (got, total) = j.amounts();
            if total == 0 {
                0
            } else {
                got * 100 / total
            }
        };
        for j in g
            .jobs
            .iter()
            .filter(|j| j.state() == State::Running && j.shown())
        {
            let transfers = copy_text("rsFilewinWorkspace.closeAsk.transfers", &[]);
            match &j.trip {
                Trip::Upload { board, items, .. } => {
                    let name = board
                        .cancels()
                        .in_flight_names()
                        .first()
                        .cloned()
                        .or_else(|| items.first().map(|p| p.name.clone()))
                        .unwrap_or_default();
                    let right = copy_text(
                        "rsFilewinWorkspace.closeAsk.up",
                        &[("pct", &pct(j).to_string())],
                    );
                    put(transfers, (ph::UPLOAD_SIMPLE, name, right));
                }
                Trip::Download { name, .. } => {
                    let right = copy_text(
                        "rsFilewinWorkspace.closeAsk.down",
                        &[("pct", &pct(j).to_string())],
                    );
                    put(transfers, (ph::DOWNLOAD_SIMPLE, name.clone(), right));
                }
                Trip::Cross { name, .. } => {
                    let right = copy_text(
                        "rsFilewinWorkspace.closeAsk.copy",
                        &[("pct", &pct(j).to_string())],
                    );
                    put(transfers, (ph::ARROWS_LEFT_RIGHT, name.clone(), right));
                }
                Trip::Copy { name, .. } => put(transfers, (ph::COPY, name.clone(), String::new())),
                Trip::Delete { name, .. } => put(
                    copy_text("rsFilewinWorkspace.closeAsk.deleting", &[]),
                    (ph::TRASH, name.clone(), String::new()),
                ),
                Trip::Extract { name, .. } => put(
                    copy_text("rsFilewinWorkspace.closeAsk.extracting", &[]),
                    (ph::FILE_ZIP, name.clone(), String::new()),
                ),
                Trip::Size { name, .. } => put(
                    copy_text("rsFilewinWorkspace.closeAsk.sizing", &[]),
                    (ph::RULER, name.clone(), String::new()),
                ),
            }
        }
        groups
    }

    /// 停第 `id` 趟。回值 ＝ 真的拨下了。
    pub fn stop(&self, id: u64) -> bool {
        let g = self.lock();
        let Some(j) = g.jobs.iter().find(|j| j.id == id) else {
            return false;
        };
        match &j.trip {
            Trip::Upload { board, .. } => {
                board.cancels().request();
                true
            }
            Trip::Download { board, .. } => {
                board.cancels().request();
                true
            }
            Trip::Cross { board, .. } => {
                board.cancel();
                true
            }
            _ => false,
        }
    }

    /// 每帧一次：新落地的那几趟记下时刻；失败的 ⇒ 摊开表（摊开着就算看过）；超出的已完成行清掉；
    /// 取消了的删除那一问（人答了「取消」）整行拿掉。回**要由窗口这一级重列的那几个目录**（它们的标签页已经不管了）。
    pub fn settle(&self, now_secs: u64) -> Vec<String> {
        let mut g = self.lock();
        let mut reload = Vec::new();
        let mut opened = false;
        g.jobs.retain(|j| !j.dropped());
        for j in &mut g.jobs {
            if j.ended.is_some() || j.state() == State::Running {
                continue;
            }
            j.ended = Some(now_secs);
            if j.state() == State::Failed {
                opened = true;
                j.unseen_fail = true;
            }
            if j.orphan {
                if let Some(d) = &j.dir {
                    reload.push(d.clone());
                }
            }
        }
        // 有失败 ⇒ 表自己摊开（不算「看过」：红点留着，到人自己收起又点开、或清掉那一行为止）。
        if opened {
            g.open = true;
        }
        let finished = g.jobs.iter().filter(|j| j.ended.is_some()).count();
        if finished > KEEP_FINISHED {
            let mut drop = finished - KEEP_FINISHED;
            g.jobs.retain(|j| {
                if drop > 0 && j.ended.is_some() {
                    drop -= 1;
                    false
                } else {
                    true
                }
            });
        }
        reload
    }
}

impl Job {
    /// 这一行画不画：那一趟开头那一问（同名要不要盖 · 删除 n 项？· 撞名另起一个？）摆着时还不算开跑。
    pub fn shown(&self) -> bool {
        match &self.trip {
            Trip::Upload { board, .. } => !board.is_asking(),
            Trip::Copy { board, .. } => !board.is_asking(),
            Trip::Cross { board, .. } => !board.is_asking(),
            Trip::Extract { board, .. } => !board.is_asking(),
            Trip::Delete { board, .. } => !board.is_asking(),
            Trip::Download { .. } | Trip::Size { .. } => true,
        }
    }

    /// 人在那一问上答了「取消」⇒ 这一趟没开过，整行拿掉。
    fn dropped(&self) -> bool {
        match &self.trip {
            Trip::Delete { board, base, .. } => {
                board.rounds() > *base
                    && board
                        .last()
                        .is_some_and(|o| o.ok == 0 && o.failed.is_empty())
            }
            Trip::Copy { board, .. } => {
                board.rounds() > 0 && matches!(board.last(), Some(CopyOutcome::Skipped))
            }
            _ => false,
        }
    }

    /// 此刻的状态（从那块看板读）。
    pub fn state(&self) -> State {
        match &self.trip {
            Trip::Upload { board, .. } => {
                if board.rounds() == 0 {
                    return State::Running;
                }
                let o = board.last().unwrap_or_default();
                if board.cancels().is_cancelled() {
                    State::Stopped
                } else if o.failed.is_empty() {
                    State::Done
                } else {
                    State::Failed
                }
            }
            Trip::Download { board, .. } => match (board.rounds(), board.last()) {
                (0, _) => State::Running,
                (_, Some(super::download::Outcome::Done { .. })) => State::Done,
                _ if board.cancels().is_cancelled() => State::Stopped,
                _ => State::Failed,
            },
            Trip::Copy { board, .. } => match (board.rounds(), board.last()) {
                (0, _) => State::Running,
                (_, Some(CopyOutcome::Failed(_) | CopyOutcome::Refused(_))) => State::Failed,
                (_, Some(CopyOutcome::Batch(r))) if !r.failed.is_empty() => State::Failed,
                _ => State::Done,
            },
            Trip::Cross { board, .. } => match (board.rounds(), board.last()) {
                (0, _) => State::Running,
                (_, Some(super::cross_copy::Outcome::Failed { .. }))
                    if board.pull.cancels().is_cancelled() =>
                {
                    State::Stopped
                }
                (_, Some(super::cross_copy::Outcome::Failed { .. })) => State::Failed,
                _ => State::Done,
            },
            Trip::Size { board, .. } => match (board.rounds(), board.last()) {
                (0, _) => State::Running,
                (_, Some(r)) if r.iter().any(Result::is_err) => State::Failed,
                _ => State::Done,
            },
            Trip::Extract { board, .. } => match (board.rounds(), board.last()) {
                (0, _) => State::Running,
                (_, Some((_, super::extract::Outcome::Failed(_)))) => State::Failed,
                _ => State::Done,
            },
            Trip::Delete { board, base, .. } => {
                if board.rounds() <= *base {
                    return State::Running;
                }
                match board.last() {
                    Some(o) if !o.failed.is_empty() => State::Failed,
                    _ => State::Done,
                }
            }
        }
    }

    /// 这一趟的字节读数 `(已走, 共)`（说不出 ⇒ `(0, 0)`）。
    pub fn amounts(&self) -> (u64, u64) {
        match &self.trip {
            Trip::Upload { board, .. } => board.totals(),
            Trip::Download { board, .. } => board.seen(),
            Trip::Cross { board, name, .. } => {
                let (g, t) = board.seen(name);
                (g / 2, t / 2)
            }
            _ => (0, 0),
        }
    }

    /// 那一行的标题（在跑时那一句）。
    pub fn title(&self) -> String {
        match &self.trip {
            Trip::Upload { items, dir, .. } => copy_text(
                "rsFilewinProgress.upload.running",
                &[("n", &items.len().to_string()), ("dir", dir)],
            ),
            Trip::Download { name, .. } => {
                copy_text("rsFilewinProgress.download.running", &[("name", name)])
            }
            Trip::Copy { n, name, .. } => {
                if *n == 1 {
                    copy_text("rsFilewinProgress.copy.runningOne", &[("name", name)])
                } else {
                    copy_text("rsFilewinProgress.copy.running", &[("n", &n.to_string())])
                }
            }
            Trip::Cross { name, machine, .. } => copy_text(
                "rsFilewinProgress.cross.running",
                &[("name", name), ("machine", machine)],
            ),
            Trip::Size { name, .. } => {
                copy_text("rsFilewinProgress.size.running", &[("name", name)])
            }
            Trip::Extract { name, .. } => {
                copy_text("rsFilewinProgress.extract.running", &[("name", name)])
            }
            Trip::Delete { name, n, .. } => {
                if *n == 1 {
                    copy_text("rsFilewinProgress.delete.runningOne", &[("name", name)])
                } else {
                    copy_text("rsFilewinProgress.delete.running", &[("n", &n.to_string())])
                }
            }
        }
    }

    /// 一行画成什么。
    pub fn view(&self) -> View {
        use egui_phosphor::regular as ph;
        let state = self.state();
        let ended = self
            .ended
            .map(|t| super::source::mtime_text(t).short)
            .unwrap_or_default();
        let stop_button = |me: &Job| -> Option<(String, Result<Act, String>)> {
            let label = copy_text("rsFilewinProgress.action.stop", &[]);
            Some(match &me.no_stop {
                Some(why) => (label, Err(why.clone())),
                None => (label, Ok(Act::Stop(me.id))),
            })
        };
        let bytes_nums = |got: u64, total: u64| -> String {
            if total == 0 {
                return String::new();
            }
            let base = copy_text(
                "rsFilewinProgress.nums.bytes",
                &[
                    ("got", &copy_core::size_text(got)),
                    ("total", &copy_core::size_text(total)),
                ],
            );
            match eta(got, total, self.started.elapsed().as_secs_f64()) {
                Some(left) => copy_text(
                    "rsFilewinProgress.nums.eta",
                    &[("bytes", &base), ("left", &left)],
                ),
                None => base,
            }
        };
        let frac_of = |got: u64, total: u64| -> Option<Option<f32>> {
            Some((total > 0).then(|| got as f32 / total as f32))
        };
        let icon_for = |running: &'static str| match state {
            State::Running => running,
            State::Done => ph::CHECK,
            State::Failed => ph::X_CIRCLE,
            State::Stopped => ph::STOP_CIRCLE,
        };
        let sep = copy_text("rsFilewinProgress.detail.sep", &[]);
        fn fails_of<W: std::fmt::Display>(failed: &[(String, W)], sep: &str) -> String {
            failed
                .iter()
                .map(|(n, why)| {
                    copy_text(
                        "rsFilewinProgress.detail.failedOne",
                        &[("name", n), ("why", &why.to_string())],
                    )
                })
                .collect::<Vec<_>>()
                .join(sep)
        }
        let fails = |failed: &[(String, String)]| fails_of(failed, &sep);
        match &self.trip {
            Trip::Upload { board, items, .. } => {
                let o = board.last();
                let (got, total) = board.totals();
                match state {
                    State::Running => {
                        let now_names = board.cancels().in_flight_names();
                        let done = board.done_count();
                        let mut detail = match now_names.first() {
                            Some(n) => copy_text(
                                "rsFilewinProgress.upload.detail",
                                &[
                                    ("name", n),
                                    ("k", &(done + 1).min(items.len()).to_string()),
                                    ("n", &items.len().to_string()),
                                ],
                            ),
                            None => String::new(),
                        };
                        // 这一窗的上传改走后端链路分块写了 ⇒ 说一句为什么（一窗一次的那个事实，跟着每一摞的那一行走）。
                        if let Some(why) = board.via_backend() {
                            let said =
                                copy_text("rsFilewinChunkUpload.route.switched", &[("why", &why)]);
                            detail = if detail.is_empty() {
                                said
                            } else {
                                format!("{detail}{sep}{said}")
                            };
                        }
                        View {
                            state,
                            icon: ph::UPLOAD_SIMPLE,
                            title: self.title(),
                            detail,
                            frac: frac_of(got, total),
                            nums: bytes_nums(got, total),
                            copy: None,
                            button: stop_button(self),
                        }
                    }
                    State::Done => {
                        let o = o.unwrap_or_default();
                        let mut detail = String::new();
                        if o.skipped > 0 {
                            detail = copy_text(
                                "rsFilewinProgress.upload.skipped",
                                &[("n", &o.skipped.to_string())],
                            );
                        }
                        if !o.redone.is_empty() {
                            let r = copy_text(
                                "rsFilewinTransfer.ui.redone",
                                &[("names", &o.redone.join(&sep))],
                            );
                            detail = if detail.is_empty() {
                                r
                            } else {
                                format!("{detail}{sep}{r}")
                            };
                        }
                        View {
                            state,
                            icon: icon_for(ph::UPLOAD_SIMPLE),
                            title: copy_text(
                                "rsFilewinProgress.upload.done",
                                &[("n", &o.ok.to_string())],
                            ),
                            detail,
                            frac: None,
                            nums: ended,
                            copy: None,
                            button: None,
                        }
                    }
                    State::Failed => {
                        let o = o.unwrap_or_default();
                        let again: Vec<Pending> = items
                            .iter()
                            .filter(|p| o.failed.iter().any(|(n, _)| *n == p.name))
                            .cloned()
                            .collect();
                        View {
                            state,
                            icon: icon_for(ph::UPLOAD_SIMPLE),
                            title: copy_text(
                                "rsFilewinProgress.upload.failed",
                                &[("n", &o.failed.len().to_string())],
                            ),
                            detail: fails_of(&o.failed, &sep),
                            frac: None,
                            nums: ended,
                            copy: super::source::Failed::copy_many(
                                &copy_text(
                                    "rsFilewinProgress.upload.failed",
                                    &[("n", &o.failed.len().to_string())],
                                ),
                                &o.failed,
                            ),
                            button: (!again.is_empty()).then(|| {
                                (
                                    copy_text(
                                        "rsFilewinProgress.action.retryN",
                                        &[("n", &again.len().to_string())],
                                    ),
                                    Ok(Act::RetryUpload(again)),
                                )
                            }),
                        }
                    }
                    State::Stopped => {
                        let o = o.unwrap_or_default();
                        let rest: Vec<Pending> = items
                            .iter()
                            .filter(|p| o.failed.iter().any(|(n, _)| *n == p.name))
                            .cloned()
                            .collect();
                        View {
                            state,
                            icon: icon_for(ph::UPLOAD_SIMPLE),
                            title: copy_text(
                                "rsFilewinProgress.upload.stopped",
                                &[
                                    ("a", &o.ok.to_string()),
                                    (
                                        "b",
                                        &items.len().saturating_sub(o.ok + o.skipped).to_string(),
                                    ),
                                ],
                            ),
                            detail: String::new(),
                            frac: None,
                            nums: ended,
                            copy: None,
                            button: (!rest.is_empty()).then(|| {
                                (
                                    copy_text("rsFilewinProgress.action.resume", &[]),
                                    Ok(Act::Resume(rest)),
                                )
                            }),
                        }
                    }
                }
            }
            Trip::Download {
                board,
                src,
                dest,
                name,
            } => {
                let (got, total) = board.seen();
                match (state, board.last()) {
                    (State::Running, _) => View {
                        state,
                        icon: ph::DOWNLOAD_SIMPLE,
                        title: self.title(),
                        detail: copy_text("rsFilewinProgress.download.detail", &[("dest", dest)]),
                        frac: frac_of(got, total),
                        nums: bytes_nums(got, total),
                        copy: None,
                        button: stop_button(self),
                    },
                    (State::Done, Some(super::download::Outcome::Done { dest, .. })) => View {
                        state,
                        icon: icon_for(ph::DOWNLOAD_SIMPLE),
                        title: copy_text("rsFilewinProgress.download.done", &[("path", &dest)]),
                        detail: board.note().unwrap_or_default(),
                        frac: None,
                        nums: ended,
                        copy: None,
                        button: None,
                    },
                    (State::Stopped, _) => View {
                        state,
                        icon: icon_for(ph::DOWNLOAD_SIMPLE),
                        title: copy_text("rsFilewinProgress.download.stopped", &[("name", name)]),
                        detail: String::new(),
                        frac: None,
                        nums: ended,
                        copy: None,
                        button: Some((
                            copy_text("rsFilewinProgress.action.resume", &[]),
                            Ok(Act::RetryDownload {
                                src: src.clone(),
                                dest: dest.clone(),
                            }),
                        )),
                    },
                    (_, o) => View {
                        state,
                        icon: icon_for(ph::DOWNLOAD_SIMPLE),
                        title: copy_text("rsFilewinProgress.download.failed", &[("name", name)]),
                        detail: match &o {
                            Some(super::download::Outcome::Failed { why, .. }) => why.said.clone(),
                            _ => String::new(),
                        },
                        frac: None,
                        nums: ended,
                        copy: match &o {
                            Some(super::download::Outcome::Failed { why, .. }) => {
                                super::source::Failed::copy_body(
                                    &format!(
                                        "{}\n{}",
                                        copy_text(
                                            "rsFilewinProgress.download.failed",
                                            &[("name", name)]
                                        ),
                                        why.said
                                    ),
                                    &why.detail,
                                )
                            }
                            _ => None,
                        },
                        button: Some((
                            copy_text("rsFilewinProgress.action.retry", &[]),
                            Ok(Act::RetryDownload {
                                src: src.clone(),
                                dest: dest.clone(),
                            }),
                        )),
                    },
                }
            }
            Trip::Copy { board, .. } => match (state, board.last()) {
                (State::Running, _) => View {
                    state,
                    icon: ph::COPY,
                    title: self.title(),
                    detail: board.running().unwrap_or_default(),
                    frac: Some(None),
                    nums: String::new(),
                    copy: None,
                    button: stop_button(self),
                },
                (_, o) => {
                    let (title, detail) = match &o {
                        Some(CopyOutcome::Batch(r)) if !r.failed.is_empty() => (
                            copy_text(
                                "rsFilewinProgress.copy.failed",
                                &[("n", &r.failed.len().to_string())],
                            ),
                            fails(&r.failed),
                        ),
                        Some(CopyOutcome::Batch(r)) => (
                            copy_text(
                                "rsFilewinProgress.copy.done",
                                &[("n", &r.done.len().to_string())],
                            ),
                            if r.skipped.is_empty() {
                                String::new()
                            } else {
                                copy_text(
                                    "rsFilewinProgress.copy.skipped",
                                    &[("names", &r.skipped.join(&sep))],
                                )
                            },
                        ),
                        Some(CopyOutcome::Done { bytes, .. }) => (
                            copy_text("rsFilewinProgress.copy.done", &[("n", "1")]),
                            copy_core::size_text(*bytes),
                        ),
                        Some(o) => (
                            copy_text("rsFilewinProgress.copy.failed", &[("n", "1")]),
                            super::copy::outcome_notice(o).text,
                        ),
                        None => (String::new(), String::new()),
                    };
                    View {
                        state,
                        icon: icon_for(ph::COPY),
                        copy: match board.last() {
                            Some(CopyOutcome::Failed(f)) => super::source::Failed::copy_body(
                                &format!("{title}\n{detail}"),
                                &f.detail,
                            ),
                            _ => None,
                        },
                        title,
                        detail,
                        frac: None,
                        nums: ended,
                        button: None,
                    }
                }
            },
            Trip::Cross {
                board,
                name,
                machine,
            } => match (state, board.last()) {
                (State::Running, _) => {
                    let (got, total) = self.amounts();
                    View {
                        state,
                        icon: ph::ARROWS_LEFT_RIGHT,
                        title: self.title(),
                        detail: String::new(),
                        frac: frac_of(got, total),
                        nums: bytes_nums(got, total),
                        copy: None,
                        button: stop_button(self),
                    }
                }
                (_, Some(super::cross_copy::Outcome::Done { machine, path, .. })) => View {
                    state,
                    icon: icon_for(ph::ARROWS_LEFT_RIGHT),
                    title: copy_text(
                        "rsFilewinProgress.cross.done",
                        &[
                            ("name", name),
                            ("machine", &super::cross_copy::shown_machine(&machine)),
                        ],
                    ),
                    detail: path,
                    frac: None,
                    nums: ended,
                    copy: None,
                    button: None,
                },
                (_, Some(super::cross_copy::Outcome::Skipped { path, .. })) => View {
                    state,
                    icon: icon_for(ph::ARROWS_LEFT_RIGHT),
                    title: copy_text("rsFilewinProgress.cross.skipped", &[("name", name)]),
                    detail: path,
                    frac: None,
                    nums: ended,
                    copy: None,
                    button: None,
                },
                (_, o) => View {
                    state,
                    icon: icon_for(ph::ARROWS_LEFT_RIGHT),
                    title: if state == State::Stopped {
                        copy_text(
                            "rsFilewinProgress.cross.stopped",
                            &[("name", name), ("machine", machine)],
                        )
                    } else {
                        copy_text(
                            "rsFilewinProgress.cross.failed",
                            &[("name", name), ("machine", machine)],
                        )
                    },
                    detail: match (state, &o) {
                        (State::Failed, Some(super::cross_copy::Outcome::Failed { why, .. })) => {
                            why.said.clone()
                        }
                        _ => String::new(),
                    },
                    frac: None,
                    nums: ended,
                    copy: match (state, &o) {
                        (State::Failed, Some(super::cross_copy::Outcome::Failed { why, .. })) => {
                            super::source::Failed::copy_body(
                                &format!(
                                    "{}\n{}",
                                    copy_text(
                                        "rsFilewinProgress.cross.failed",
                                        &[("name", name), ("machine", machine)],
                                    ),
                                    why.said
                                ),
                                &why.detail,
                            )
                        }
                        _ => None,
                    },
                    button: None,
                },
            },
            Trip::Size { board, .. } => match (state, board.last()) {
                (State::Running, _) => View {
                    state,
                    icon: ph::RULER,
                    title: self.title(),
                    detail: board.running().unwrap_or_default(),
                    frac: Some(None),
                    nums: String::new(),
                    copy: None,
                    button: stop_button(self),
                },
                (_, r) => View {
                    state,
                    icon: icon_for(ph::RULER),
                    title: r
                        .as_deref()
                        .map(super::size::outcome_text)
                        .unwrap_or_default(),
                    detail: String::new(),
                    frac: None,
                    nums: ended,
                    copy: None,
                    button: None,
                },
            },
            Trip::Extract { board, .. } => match (state, board.last()) {
                (State::Running, _) => View {
                    state,
                    icon: ph::FILE_ZIP,
                    title: self.title(),
                    detail: String::new(),
                    frac: Some(None),
                    nums: String::new(),
                    copy: None,
                    button: stop_button(self),
                },
                (_, last) => View {
                    state,
                    icon: icon_for(ph::FILE_ZIP),
                    title: last
                        .as_ref()
                        .map(|(n, o)| super::extract::outcome_text(n, o))
                        .unwrap_or_default(),
                    detail: String::new(),
                    frac: None,
                    nums: ended,
                    copy: last
                        .as_ref()
                        .and_then(|(n, o)| super::extract::outcome_copy(n, o)),
                    button: None,
                },
            },
            Trip::Delete { board, n, .. } => match (state, board.last()) {
                (State::Running, _) => View {
                    state,
                    icon: ph::TRASH,
                    title: self.title(),
                    detail: String::new(),
                    frac: Some(None),
                    nums: String::new(),
                    copy: None,
                    button: stop_button(self),
                },
                (State::Failed, Some(o)) => View {
                    state,
                    icon: icon_for(ph::TRASH),
                    title: copy_text(
                        "rsFilewinProgress.delete.failed",
                        &[("n", &o.failed.len().to_string())],
                    ),
                    detail: fails_of(&o.failed, &sep),
                    frac: None,
                    nums: ended,
                    copy: super::source::Failed::copy_many(
                        &copy_text(
                            "rsFilewinProgress.delete.failed",
                            &[("n", &o.failed.len().to_string())],
                        ),
                        &o.failed,
                    ),
                    button: None,
                },
                (_, o) => View {
                    state,
                    icon: icon_for(ph::TRASH),
                    title: copy_text(
                        "rsFilewinProgress.delete.done",
                        &[("n", &o.map_or(*n, |o| o.ok).to_string())],
                    ),
                    detail: String::new(),
                    frac: None,
                    nums: ended,
                    copy: None,
                    button: None,
                },
            },
        }
    }
}

/// 约多久（照这一趟到此刻的平均速度）；刚开头（不足 2 秒或一个字节都没走）不说。
pub fn eta(got: u64, total: u64, elapsed: f64) -> Option<String> {
    if got == 0 || got >= total || elapsed < 2.0 {
        return None;
    }
    let left = (total - got) as f64 / (got as f64 / elapsed);
    let secs = left.ceil() as u64;
    Some(if secs < 60 {
        copy_text(
            "rsFilewinProgress.eta.secs",
            &[("n", &secs.max(1).to_string())],
        )
    } else if secs < 3600 {
        copy_text(
            "rsFilewinProgress.eta.mins",
            &[("n", &secs.div_ceil(60).to_string())],
        )
    } else {
        copy_text(
            "rsFilewinProgress.eta.hours",
            &[("n", &secs.div_ceil(3600).to_string())],
        )
    })
}

/// 画那张表（窗口底部一块）。回这一帧按了哪颗按钮。
/// 表里一行此刻画成什么：断线时在跑的那几行写「等待重连」（不报失败）。
fn row_view(j: &Job, offline: bool) -> View {
    let mut v = j.view();
    if offline && v.state == State::Running {
        v.detail = copy_text("rsFilewinProgress.row.waitLink", &[]);
    }
    v
}

/// 表要多高才把每一行（含失败摊开的那几行小字）都摆下：表头 32 ＋ 每行的高 ＋ 上下边距。调用方再夹到它的上限（超了在表里滚）。
pub fn table_height(progress: &Progress, offline: bool) -> f32 {
    32.0 + progress
        .jobs()
        .iter()
        .filter(|j| j.shown())
        .map(|j| super::kit::task_row_height(&row_view(j, offline)))
        .sum::<f32>()
        + 12.0
}

pub fn table_ui(ui: &mut egui::Ui, progress: &Progress, offline: bool) -> Option<Act> {
    let jobs = progress.jobs();
    let mut act = None;
    let mut clear = false;
    let mut collapse = false;
    super::kit::table_head(
        ui,
        &copy_text("rsFilewinProgress.head.title", &[]),
        &copy_text("rsFilewinProgress.head.clear", &[]),
        &mut clear,
        &mut collapse,
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for j in jobs.iter().filter(|j| j.shown()) {
                let v = row_view(j, offline);
                let top = ui.cursor().top();
                if let Some(a) = super::kit::task_row(ui, j.id, &v) {
                    act = Some(a);
                }
                // 刚失败的那一行（自动摊开表的那一下）滚进视野、整行看得见。
                if v.state == State::Failed && j.ended.is_some_and(|t| now.saturating_sub(t) <= 2) {
                    let r = egui::Rect::from_x_y_ranges(
                        ui.max_rect().x_range(),
                        top..=ui.cursor().top(),
                    );
                    ui.scroll_to_rect(r, None);
                }
            }
        });
    if clear {
        progress.clear_finished();
    }
    if collapse {
        progress.set_open(false);
    }
    act
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/progress_tests.rs"]
mod tests;
