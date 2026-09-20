//! `24e` 的窗口壳：把 egui 那条事件循环放到**次线程**，主线程留给 Tauri／`tao`。
//!
//! 结论与现打依据住 `super` 的头注（`src/filewin/mod.rs`）。这里只放落地。
//!
//! # 这一整个模块就压在一件事上
//!
//! ```text
//! winit `platform_impl::linux::EventLoop::new` 里那道主线程检查
//!     if !attributes.any_thread && !is_main_thread() { panic!(...) }
//! ```
//!
//! [`any_thread_hook`] 就是把那个 `any_thread` 打开的地方。它经
//! `eframe::NativeOptions::event_loop_builder` 交给 eframe，
//! eframe 在 `native::run::create_event_loop` 里于 `builder.build()` **之前**调用它。
//!
//! # 🔴 同进程特有的一条风险：Windows 上**进程级** DPI 感知有两个主人
//!
//! 这条不是「两个事件循环」那一类，它是**同进程**才会有的：
//! `tao` 与 `winit` **各自**有一个「把本进程设成 DPI-aware」的内部函数，各自用一个自己的 `Once` 守着，
//! 而它们调的是**同一个进程级** Win32 接口（`SetProcessDpiAwarenessContext`）。
//! ⇒ 一个进程里两个库都想设这块全局状态，而谁也不知道对方存在。
//!
//! **现打核过两边的源码，今天不会出事，理由要说准**：
//! 两边设的**是同一个值**（`PER_MONITOR_AWARE_V2`，设不上就退 V1），
//! 且 Windows 对「已经设过」返回 FALSE、两边都忽略这个返回值
//! ⇒ 先跑的那个赢，而先跑的必然是 Tauri（主窗在 app 启动时就建了，
//! 这个文件窗口只能由已经跑起来的 app 开）。
//!
//! ⚠ **但那是靠顺序碰巧对上的，不是靠设计**。真出事的形状是：
//! 哪天 egui 窗口比 Tauri 主窗先建（或 winit 换了默认值），
//! WebView2 那侧的 DPI 行为会跟着变 —— 而本仓在 Windows DPI/WebView 上
//! 已经吃过亏（`真相源/70` 那一族、以及 `F12 nudge` 那处 WebView2 bounds 修正）。
//!
//! ⇒ **建议（本刀没做，留给拍板）**：给 [`any_thread_hook`] 的 Windows 分支加一句
//! `with_dpi_aware(false)`，把「进程 DPI 归谁管」明确判给 Tauri。
//! **没直接做的理由**：手上没有 Windows 机器，改了也验不了，
//! 而在一个验不了的平台上动全局状态的默认值，比留着这条注记更危险。
//!
//! # ⚠ 没做到的，写在这儿而不是藏着
//!
//! - **本机跑不了真窗口**（`XDG_SESSION_TYPE=tty`，无图形会话）⇒ [`open_detached`]
//!   这条路在本机**没有端到端读数**；有的是 scratchpad 那个 `tao ＋ eframe` 原型的
//!   三趟读数（见 `super` 头注）与编译期的三档 `cargo check`。
//! - **Windows 上一次都没跑过。**
//! - 多选 · 拖放 · 预览 · 双栏 · 右键菜单 **一个都没有**（`设计/60 §4 戊` 代价第 2 条）。
//! - `设计/60 §5.4b`（大文件编辑改流式）与 `§5.4d`（拖入多文件先一次问完再并行）
//!   这两条转成本窗口需求的东西，**本刀都没做** —— 它们要写面，而这一刀只有读面。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::rows::{show_file_rows, RenderTally};
use super::source::{list_local, list_remote, parent_dir, Row, Source};
use super::transfer::{DropBoard, Pending};

/// 开过几个窗口 —— **egui 那条线程真的跑起来了**几次。
///
/// ⚠ 它在**次线程里**加，所以从调用方看它是**异步**的：
/// `open_detached` 返回的那一刻这个数可能还没动。要「请求过几次」看 [`open_requested`]。
static WINDOWS_OPENED: AtomicU64 = AtomicU64::new(0);

/// 请求过几次开窗 —— **同步**加，在 `thread::spawn` 之前。
///
/// 🔴 **两个数刻意分开，不合成一个。** 本机没有图形会话（`super` 头注）⇒
/// `run_native` 在这台机器上必然失败，`WINDOWS_OPENED` 也就量不到「窗口真的在」。
/// 合成一个数就只能二选一：要么判据没法确定地判（次线程还没跑），
/// 要么这个数的语义被悄悄改成「请求过」而名字还叫「开过」。
/// ⇒ 一个值装一件事：请求面用这个，线程面用上面那个。
static OPEN_REQUESTED: AtomicU64 = AtomicU64::new(0);

pub fn windows_opened() -> u64 {
    WINDOWS_OPENED.load(Ordering::SeqCst)
}

pub fn open_requested() -> u64 {
    OPEN_REQUESTED.load(Ordering::SeqCst)
}

/// 🔴 让 winit 的事件循环能建在**非主线程**上。
///
/// 三个平台各一句；macOS 上**没有**对应的扩展 trait（NSApplication 铁定要主线程），
/// 而本仓承诺的平台是 Windows / Linux ⇒ 今天碰不到那一格。
pub fn any_thread_hook(builder: &mut eframe::EventLoopBuilder<eframe::UserEvent>) {
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
    {
        use winit::platform::wayland::EventLoopBuilderExtWayland;
        use winit::platform::x11::EventLoopBuilderExtX11;
        EventLoopBuilderExtX11::with_any_thread(builder, true);
        EventLoopBuilderExtWayland::with_any_thread(builder, true);
    }
    #[cfg(windows)]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        EventLoopBuilderExtWindows::with_any_thread(builder, true);
    }
    #[cfg(target_os = "macos")]
    {
        // 够不着：macOS 没有 with_any_thread。留个明确的编译期落点，
        // 免得哪天上了 macOS 还以为这条路是通的。
        let _ = builder;
    }
}

/// 列目录这件事的**共享落点** —— 一次列目录要写的东西全在这儿。
///
/// 🔴 抽成一个结构是因为它要被**两条线程**看：UI 线程读，tokio 那条写。
/// 散成四个 `Arc` 时「哪几个字段必须一起更新」只写在注释里，没有人守着。
#[derive(Clone)]
pub struct Listing {
    pub rows: Arc<Mutex<Vec<Row>>>,
    pub error: Arc<Mutex<Option<String>>>,
    /// 🔴 **换目录的序号。** 每次 [`FileWindow::navigate_to`] +1。
    ///
    /// 异步回来的那一份**带着它出发时的号**：号不对就整份丢掉。
    /// 没有它的失效形状是真的会出现的：远端目录 A 慢、B 快 ⇒ 用户点进 A 立刻退回 B，
    /// A 的响应后到，把 B 的内容**盖成 A 的**，而路径栏上写着 B。
    pub epoch: Arc<AtomicU64>,
    /// 还有几趟列目录在飞。0 = 列完了。
    pub inflight: Arc<AtomicU64>,
}

impl Default for Listing {
    fn default() -> Self {
        Self {
            rows: Arc::new(Mutex::new(Vec::new())),
            error: Arc::new(Mutex::new(None)),
            epoch: Arc::new(AtomicU64::new(0)),
            inflight: Arc::new(AtomicU64::new(0)),
        }
    }
}

/// 把一趟列目录的结果落进 [`Listing`] —— **号对不上就丢掉**。
///
/// 回值 = 真的落盘了。**自由函数**（不吃 `FileWindow`）⇒ 不用开窗、不用网络就判得动，
/// 而它正是生产那条路上唯一写 `rows` 的地方（见 [`Listing::start`]）。
pub fn store_if_current(l: &Listing, mine: u64, r: Result<Vec<Row>, String>) -> bool {
    // ⚠ 先减在飞数，再判号 —— 不管这一份要不要，它都已经落地了。
    l.inflight.fetch_sub(1, Ordering::SeqCst);
    if l.epoch.load(Ordering::SeqCst) != mine {
        return false;
    }
    match r {
        Ok(v) => {
            *l.rows.lock().unwrap() = v;
            *l.error.lock().unwrap() = None;
        }
        Err(e) => {
            *l.rows.lock().unwrap() = Vec::new();
            *l.error.lock().unwrap() = Some(e);
        }
    }
    true
}

impl Listing {
    /// 开一趟。回的是这一趟的号（调用方不需要它，但判据要）。
    pub fn start(&self) -> u64 {
        self.inflight.fetch_add(1, Ordering::SeqCst);
        self.epoch.load(Ordering::SeqCst)
    }

    /// 换目录：号 +1（在飞的那些从此全部作废）、内容清空。
    ///
    /// ⚠ **清空是刻意的**：留着上一个目录的行、只改路径栏，是最贵的那种假象
    /// —— 用户会对着 B 的路径删 A 的文件。
    pub fn invalidate(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.rows.lock().unwrap().clear();
        *self.error.lock().unwrap() = None;
    }

    pub fn is_loading(&self) -> bool {
        self.inflight.load(Ordering::SeqCst) > 0
    }
}

/// 窗口的全部状态。
pub struct FileWindow {
    pub source: Source,
    pub cwd: String,
    pub listing: Listing,
    pub tally: RenderTally,
    /// 远端要在 tokio 上跑；本机不需要，所以是 `Option`。
    pub rt: Option<tokio::runtime::Handle>,
    /// `设计/60 §5.4d`：拖入那一摞的状态机（**先一次问完，再并行传**）。
    pub board: DropBoard,
    /// 已经消化过几趟拖入。`board.rounds()` 走在它前面 ⇒ 该重列一次目录了。
    ///
    /// ⚠ 为什么不让那条 tokio 任务自己 `reload()`：它手上没有 `&mut self`，
    /// 而把 `cwd` 也塞进 `Arc` 只为了让它能读，是把窗口状态搬到共享内存里去。
    /// ⇒ 换个方向：任务只记「我跑完了」，**换目录这件事一直留在 UI 线程手上**。
    seen_rounds: u64,
}

impl FileWindow {
    pub fn new(source: Source, cwd: String, rt: Option<tokio::runtime::Handle>) -> Self {
        let w = Self::seeded(source, cwd, rt, Vec::new());
        w.reload();
        w
    }

    /// 带着**已经列好的那一屏**建窗 —— [`super::entry::open_file_window`] 走这条。
    ///
    /// 🔴 为什么要有它：那条命令为了能在 webview 那侧**出声**（目录列不出来就别开窗），
    /// 已经先列过一趟了。再让 `new()` 列第二趟，就是对同一个远端目录连打两次往返。
    pub fn seeded(
        source: Source,
        cwd: String,
        rt: Option<tokio::runtime::Handle>,
        rows: Vec<Row>,
    ) -> Self {
        let listing = Listing::default();
        *listing.rows.lock().unwrap() = rows;
        Self {
            source,
            cwd,
            listing,
            tally: RenderTally::default(),
            rt,
            board: DropBoard::default(),
            seen_rounds: 0,
        }
    }

    /// 重新列一次当前目录。**本机同步做完；远端扔给 tokio，不堵住 UI 线程。**
    pub fn reload(&self) {
        let l = self.listing.clone();
        let mine = l.start();
        let cwd = self.cwd.clone();
        match &self.source {
            Source::Local => {
                let r = list_local(std::path::Path::new(&cwd));
                store_if_current(&l, mine, r);
            }
            Source::Remote(cfg) => {
                let cfg = cfg.clone();
                match &self.rt {
                    Some(h) => {
                        h.spawn(async move {
                            let r = list_remote(&cfg, &cwd).await;
                            store_if_current(&l, mine, r);
                        });
                    }
                    None => {
                        store_if_current(
                            &l,
                            mine,
                            Err("远端目录要一个 tokio 运行时，这个窗口没拿到".into()),
                        );
                    }
                }
            }
        }
    }

    /// 换到 `path` 并重列。**在飞的那几趟从此全部作废**（见 [`Listing::epoch`]）。
    pub fn navigate_to(&mut self, path: String) {
        if path == self.cwd {
            return;
        }
        self.cwd = path;
        self.listing.invalidate();
        self.reload();
    }

    /// 上一级。已经在顶上就什么都不做（[`parent_dir`] 到顶回原值）。
    pub fn navigate_up(&mut self) {
        let up = parent_dir(&self.source, &self.cwd);
        self.navigate_to(up);
    }

    /// 点开第 `i` 行。**目录进去，文件不动。**
    ///
    /// 回值 = 真的换了目录。⚠ 文件那一侧（预览 / 编辑 / 下载）这一刀**没做**，
    /// 所以这里对文件是明确的「什么都不做」，不是「以后再说」。
    pub fn activate(&mut self, i: usize) -> bool {
        let target = {
            let rows = self.listing.rows.lock().unwrap();
            match rows.get(i) {
                Some(r) if r.is_dir => r.path.clone(),
                _ => return false,
            }
        };
        let before = self.cwd.clone();
        self.navigate_to(target);
        self.cwd != before
    }

    /// 切到**本机**那一侧，落在 `home`。
    ///
    /// 🔴 它是 [`list_local`] 今天**唯一的用户可达入口**。没有它，本机那半是
    /// 「盘上有 ≠ 被走到」（`K-R18` 语料八）—— 有判据、没有人走得到。
    pub fn go_local(&mut self, home: String) {
        self.source = Source::Local;
        self.cwd = home;
        self.listing.invalidate();
        self.reload();
    }

    /// 拖进来的那几个本机文件 → 待传清单（**目标目录 = 当前目录**）。
    ///
    /// ⚠ 只在**远端**那一侧成立：本机拖本机是「复制文件」，那是另一件事，本刀不做
    /// ⇒ 本机源上返回空清单（不是静默忽略：调用方据此出声）。
    pub fn pending_for(&self, local_paths: &[String]) -> Vec<Pending> {
        if !self.source.is_remote() {
            return Vec::new();
        }
        local_paths
            .iter()
            .filter_map(|p| Pending::into_remote_dir(p, &self.cwd))
            .collect()
    }

    /// 起一趟 `§5.4d`：**先一次问完，再并行传**。
    ///
    /// 🔴 三段的顺序不在这里，在 [`crate::filewin::transfer::run_drop`] 的结构里 ——
    /// 这里只负责把「问谁 · 怎么问 · 怎么传」三个口接上去。
    /// 接错了会被 `transfer_tests` 逮住的是**那个函数**，不是本函数；
    /// 而本函数接不上（没运行时 / 本机源）要**出声**，判据见 `shell_tests`。
    pub fn start_drop(&mut self, items: Vec<Pending>, ctx: Option<egui::Context>) -> bool {
        if items.is_empty() {
            return false;
        }
        let Source::Remote(cfg) = &self.source else {
            *self.listing.error.lock().unwrap() =
                Some("这个窗口现在看的是本机，拖进来的文件没有远端目标".into());
            return false;
        };
        let Some(h) = self.rt.clone() else {
            *self.listing.error.lock().unwrap() =
                Some("上传要一个 tokio 运行时，这个窗口没拿到".into());
            return false;
        };
        let cfg = cfg.clone();
        let board = self.board.clone();
        h.spawn(async move {
            let probe_cfg = cfg.clone();
            let up_cfg = cfg.clone();
            let ask_board = board.clone();
            let up_board = board.clone();
            let out = super::transfer::run_drop(
                items,
                super::transfer::lanes(),
                move |p| {
                    let cfg = probe_cfg.clone();
                    async move { super::transfer::probe_remote(&cfg, &p.remote_path).await }
                },
                {
                    let ctx = ctx.clone();
                    move |clashes| {
                        let rx = ask_board.ask(clashes);
                        if let Some(c) = ctx.as_ref() {
                            c.request_repaint(); // 问题要立刻画出来，别等下一次鼠标动
                        }
                        async move { rx.await.unwrap_or_default() }
                    }
                },
                move |p| {
                    let cfg = up_cfg.clone();
                    let b = up_board.clone();
                    async move { super::transfer::upload_remote(&cfg, &p, &b).await }
                },
            )
            .await;
            board.finish(out);
            if let Some(c) = ctx.as_ref() {
                c.request_repaint();
            }
        });
        true
    }

    /// 把这一帧 egui 收到的拖入文件接走。
    ///
    /// ⚠ **上一问还没答完就不接新的** —— 两摞问题叠在一个模态框上，
    /// 「一次问完」就变成「两次问完」。
    fn take_drops(&mut self, ctx: &egui::Context) {
        if self.board.is_asking() {
            return;
        }
        let dropped: Vec<String> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_string_lossy().to_string())
                .collect()
        });
        if dropped.is_empty() {
            return;
        }
        let items = self.pending_for(&dropped);
        if items.is_empty() {
            *self.listing.error.lock().unwrap() =
                Some(format!("拖进来的 {} 个东西一个都认不出名字", dropped.len()));
            return;
        }
        self.start_drop(items, Some(ctx.clone()));
    }

    /// 把这一帧列表带出来的那个点击**接上**状态机。
    ///
    /// 🔴 **抽成一个函数是为了让这条胶水本身可判。** 它只有一行，而那一行正是
    /// 「列表说有人点了第 i 行」与「窗口换目录」之间的唯一连接 ——
    /// 写在 `ui()` 里的话，`activate` 有判据、`show_file_rows` 有判据，
    /// 中间这一跳**谁都没在看**（那正是第一刀栽过的那一形：判据钉的是副本，
    /// 生产那一份没人管）。
    ///
    /// 回值 = 真的换了目录。
    pub fn apply_click(&mut self) -> bool {
        match self.tally.clicked {
            Some(i) => self.activate(i),
            None => false,
        }
    }

    /// 传完一趟就重列一次当前目录（新文件要出现在列表里）。
    ///
    /// 回值 = 这一帧真的重列了。**放在 UI 线程**，理由见 [`Self::seen_rounds`]。
    pub fn settle_finished_drops(&mut self) -> bool {
        let now = self.board.rounds();
        if now == self.seen_rounds {
            return false;
        }
        self.seen_rounds = now;
        self.reload();
        true
    }
}

impl eframe::App for FileWindow {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut go_local: Option<String> = None;
        ui.horizontal(|ui| {
            if ui.button("⬆ 上一级").clicked() {
                self.navigate_up();
            }
            if ui.button("刷新").clicked() {
                self.reload();
            }
            if !self.source.is_remote() {
                ui.label("本机");
            } else if ui.button("本机").clicked() {
                go_local = Some(
                    dirs::home_dir()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| ".".to_string()),
                );
            }
            ui.label(format!("{} : {}", self.source.label(), self.cwd));
            if self.listing.is_loading() {
                ui.spinner();
                ui.label("正在列…");
            }
        });
        if let Some(home) = go_local {
            self.go_local(home);
        }
        if let Some(e) = self.listing.error.lock().unwrap().clone() {
            ui.colored_label(egui::Color32::RED, e);
        }
        // `§5.4d` 那一摞：确认框 ／ 进度。**画在列表之前** —— 它是模态的。
        self.board.ui(ui);
        let ctx = ui.ctx().clone();
        self.take_drops(&ctx);
        self.settle_finished_drops();
        ui.separator();
        // 每帧从零数起 —— 这个数是「这一帧物化了多少行」，不是累计。
        self.tally = RenderTally::default();
        {
            let rows = self.listing.rows.lock().unwrap();
            show_file_rows(ui, &rows, &mut self.tally, None);
        }
        self.apply_click();
    }
}

/// 在**次线程**上开一个文件管理窗口。立刻返回，不阻塞调用方（＝ Tauri 主线程）。
///
/// ⚠ `eframe::run_native` 在它自己那条线程上是**阻塞到窗口关闭**的；
/// 这里把它整个丢进 `std::thread::spawn` ⇒ 对调用方是非阻塞的。
pub fn open_detached(
    source: Source,
    cwd: String,
    rt: Option<tokio::runtime::Handle>,
) -> std::thread::JoinHandle<Result<(), String>> {
    open_detached_seeded(source, cwd, rt, Vec::new())
}

/// 同 [`open_detached`]，但**带着已经列好的那一屏**开窗。
///
/// 🔴 用它而不是 `open_detached` 的理由住 [`FileWindow::seeded`]：
/// 入口那条命令为了能在 webview 那侧出声，已经列过一趟了，别再打第二次往返。
pub fn open_detached_seeded(
    source: Source,
    cwd: String,
    rt: Option<tokio::runtime::Handle>,
    rows: Vec<Row>,
) -> std::thread::JoinHandle<Result<(), String>> {
    OPEN_REQUESTED.fetch_add(1, Ordering::SeqCst);
    std::thread::spawn(move || {
        let title = format!("cc-monitor 文件 — {}", source.label());
        let opts = eframe::NativeOptions {
            event_loop_builder: Some(Box::new(any_thread_hook)),
            ..Default::default()
        };
        WINDOWS_OPENED.fetch_add(1, Ordering::SeqCst);
        eframe::run_native(
            &title,
            opts,
            Box::new(move |_cc| Ok(Box::new(FileWindow::seeded(source, cwd, rt, rows)))),
        )
        .map_err(|e| format!("开窗失败: {e}"))
    })
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/shell_tests.rs"]
mod tests;
