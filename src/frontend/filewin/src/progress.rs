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

use super::batch::{BatchBoard, Phase};
use super::download::DownloadBoard;
use super::extract::ExtractBoard;
use super::size::SizeBoard;
use super::transfer::{DropBoard, Pending};

/// 已完成的行最多留几行（更早的自动清掉）。
pub const KEEP_FINISHED: usize = 50;

/// 一趟是什么、它那块看板在哪。
#[derive(Clone)]
pub enum Trip {
    /// 上传一摞：`dir` 是传到的那个目录（显示名），`items` 是这一摞（重试失败的那几个要用），`ask` 是整组开单那一问（同名冲突框）。
    Upload {
        board: DropBoard,
        items: Vec<Pending>,
        dir: String,
        ask: BatchBoard,
    },
    /// 下载一个文件到本机 `dest`。
    Download {
        board: DownloadBoard,
        name: String,
        src: String,
        dest: String,
    },
    /// 一组一条命令（复制为 · 复制到另一栏 · 复制到另一台 · 删除 · 改权限）：那块看板带着票、进度、冲突框与结局。
    Batch {
        board: BatchBoard,
        icon: &'static str,
    },
    /// 算大小。
    Size { board: SizeBoard, name: String },
    /// 解压到这里。
    Extract { board: ExtractBoard, name: String },
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
    /// 接着传这一摞里没传的（上传停了之后；再开单，续传先对尾块）。
    Resume(Vec<Pending>),
    /// 一组里没成的那几项另起一组（另一张票）：`at` ＝ 寻址的那台，`home` ＝ 在哪台的后端里做（同原来那一组）。
    RetryBatch {
        plan: super::batch::Plan,
        icon: &'static str,
        at: super::source::Origin,
        home: super::source::Origin,
    },
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
                Trip::Batch { board, icon } => {
                    let name = board.title();
                    match board.plan().verb {
                        super::batch::Verb::Copy => {
                            let right = copy_text(
                                "rsFilewinWorkspace.closeAsk.copy",
                                &[("pct", &pct(j).to_string())],
                            );
                            put(transfers, (*icon, name, right));
                        }
                        super::batch::Verb::Delete => put(
                            copy_text("rsFilewinWorkspace.closeAsk.deleting", &[]),
                            (*icon, name, String::new()),
                        ),
                        super::batch::Verb::Chmod
                        | super::batch::Verb::Download
                        | super::batch::Verb::Upload => {
                            put(transfers, (*icon, name, String::new()))
                        }
                    }
                }
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
            Trip::Batch { board, .. } => board.stop(),
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
            Trip::Upload { ask, .. } => !ask.is_asking() && ask.phase() != Phase::Dropped,
            Trip::Batch { board, .. } => !board.is_asking() && board.phase() != Phase::Dropped,
            Trip::Extract { board, .. } => !board.is_asking(),
            Trip::Download { .. } | Trip::Size { .. } => true,
        }
    }

    /// 人在那一问上答了「取消」⇒ 这一趟没开过，整行拿掉。
    fn dropped(&self) -> bool {
        match &self.trip {
            Trip::Batch { board, .. } | Trip::Upload { ask: board, .. } => {
                board.phase() == Phase::Dropped
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
            Trip::Batch { board, .. } => match board.phase() {
                Phase::Running | Phase::Asking | Phase::Cut(_) => State::Running,
                Phase::Ended(r)
                    if r.outcomes
                        .iter()
                        .any(|o| o.state == cmd_wire::batch::ItemState::Cancelled) =>
                {
                    State::Stopped
                }
                Phase::Ended(r)
                    if r.outcomes.iter().any(|o| {
                        matches!(
                            o.state,
                            cmd_wire::batch::ItemState::Failed | cmd_wire::batch::ItemState::Exists
                        )
                    }) =>
                {
                    State::Failed
                }
                Phase::Ended(_) | Phase::Dropped | Phase::Opened => State::Done,
                Phase::Failed(_) => State::Failed,
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
        }
    }

    /// 这一趟的字节读数 `(已走, 共)`（说不出 ⇒ `(0, 0)`）。
    pub fn amounts(&self) -> (u64, u64) {
        match &self.trip {
            Trip::Upload { board, .. } => board.totals(),
            Trip::Download { board, .. } => board.seen(),
            Trip::Batch { board, .. } => board.tick().bytes.unwrap_or((0, 0)),
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
            Trip::Batch { board, .. } => board.title(),
            Trip::Size { name, .. } => {
                copy_text("rsFilewinProgress.size.running", &[("name", name)])
            }
            Trip::Extract { name, .. } => {
                copy_text("rsFilewinProgress.extract.running", &[("name", name)])
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
                                    copy_text("rsFilewinBatch.btn.retry", &[]),
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
                                    copy_text("rsFilewinBatch.btn.resume", &[]),
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
                            copy_text("rsFilewinBatch.btn.resume", &[]),
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
            Trip::Batch { board, icon } => {
                let plan = board.plan();
                let tick = board.tick();
                match board.phase() {
                    Phase::Cut(line) => View {
                        state,
                        icon,
                        title: line,
                        detail: String::new(),
                        frac: Some(tick.frac()),
                        nums: String::new(),
                        copy: None,
                        button: stop_button(self),
                    },
                    Phase::Ended(r) => View {
                        state,
                        icon: icon_for(icon),
                        title: r.said.clone(),
                        detail: super::batch::failed_lines(&plan, &r).join(&sep),
                        frac: None,
                        nums: ended,
                        copy: super::batch::copy_details(&plan, &r),
                        button: super::batch::retry_plan(&plan, &r).zip(board.route()).map(
                            |(plan, (at, home))| {
                                (
                                    copy_text("rsFilewinBatch.btn.retry", &[]),
                                    Ok(Act::RetryBatch {
                                        plan,
                                        icon,
                                        at,
                                        home,
                                    }),
                                )
                            },
                        ),
                    },
                    Phase::Failed(f) => View {
                        state,
                        icon: icon_for(icon),
                        title: board.title(),
                        detail: f.said.clone(),
                        frac: None,
                        nums: ended,
                        copy: super::source::Failed::copy_body(
                            &format!("{}\n{}", board.title(), f.said),
                            &f.detail,
                        ),
                        button: None,
                    },
                    Phase::Running | Phase::Asking | Phase::Dropped | Phase::Opened => {
                        let (got, total) = tick.bytes.unwrap_or((0, 0));
                        View {
                            state,
                            icon,
                            title: self.title(),
                            detail: if tick.total > 0 {
                                copy_text(
                                    "rsFilewinBatch.row.items",
                                    &[
                                        ("done", &tick.done.to_string()),
                                        ("total", &tick.total.to_string()),
                                    ],
                                )
                            } else {
                                String::new()
                            },
                            frac: Some(tick.frac()),
                            nums: bytes_nums(got, total),
                            copy: None,
                            button: if board.stopping() {
                                None
                            } else {
                                stop_button(self)
                            },
                        }
                    }
                }
            }
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

/// 画那张表，回这一帧按了哪颗按钮 ＋ 把每一行都摆下要多高（表头 ＋ 这一帧真排出来的各行；调用方下一帧照它定表高、夹到上限，超了在表里滚）。
pub fn table_ui(ui: &mut egui::Ui, progress: &Progress, offline: bool) -> (Option<Act>, f32) {
    let top = ui.max_rect().top();
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
    let rows = egui::ScrollArea::vertical()
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
    (act, rows.inner_rect.top() - top + rows.content_size.y)
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/progress_tests.rs"]
mod tests;
