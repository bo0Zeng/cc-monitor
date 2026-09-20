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
use super::source::{list_local, list_remote, Row, Source};

/// 开过几个窗口 —— 给「同进程里到底起没起来」一个可观测的数。
static WINDOWS_OPENED: AtomicU64 = AtomicU64::new(0);

pub fn windows_opened() -> u64 {
    WINDOWS_OPENED.load(Ordering::SeqCst)
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

/// 窗口的全部状态。
pub struct FileWindow {
    pub source: Source,
    pub cwd: String,
    pub rows: Arc<Mutex<Vec<Row>>>,
    pub error: Arc<Mutex<Option<String>>>,
    pub tally: RenderTally,
    /// 远端要在 tokio 上跑；本机不需要，所以是 `Option`。
    pub rt: Option<tokio::runtime::Handle>,
}

impl FileWindow {
    pub fn new(source: Source, cwd: String, rt: Option<tokio::runtime::Handle>) -> Self {
        let w = Self {
            source,
            cwd,
            rows: Arc::new(Mutex::new(Vec::new())),
            error: Arc::new(Mutex::new(None)),
            tally: RenderTally::default(),
            rt,
        };
        w.reload();
        w
    }

    /// 重新列一次当前目录。**本机同步做完；远端扔给 tokio，不堵住 UI 线程。**
    pub fn reload(&self) {
        let rows = self.rows.clone();
        let error = self.error.clone();
        let cwd = self.cwd.clone();
        match &self.source {
            Source::Local => {
                let r = list_local(std::path::Path::new(&cwd));
                store(&rows, &error, r);
            }
            Source::Remote(cfg) => {
                let cfg = cfg.clone();
                match &self.rt {
                    Some(h) => {
                        h.spawn(async move {
                            let r = list_remote(&cfg, &cwd).await;
                            store(&rows, &error, r);
                        });
                    }
                    None => {
                        *error.lock().unwrap() =
                            Some("远端目录要一个 tokio 运行时，这个窗口没拿到".into());
                    }
                }
            }
        }
    }
}

fn store(
    rows: &Arc<Mutex<Vec<Row>>>,
    error: &Arc<Mutex<Option<String>>>,
    r: Result<Vec<Row>, String>,
) {
    match r {
        Ok(v) => {
            *rows.lock().unwrap() = v;
            *error.lock().unwrap() = None;
        }
        Err(e) => *error.lock().unwrap() = Some(e),
    }
}

impl eframe::App for FileWindow {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.horizontal(|ui| {
            ui.label(format!("{} : {}", self.source.label(), self.cwd));
            if ui.button("刷新").clicked() {
                self.reload();
            }
        });
        if let Some(e) = self.error.lock().unwrap().clone() {
            ui.colored_label(egui::Color32::RED, e);
        }
        ui.separator();
        let rows = self.rows.lock().unwrap();
        // 每帧从零数起 —— 这个数是「这一帧物化了多少行」，不是累计。
        self.tally = RenderTally::default();
        show_file_rows(ui, &rows, &mut self.tally, None);
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
            Box::new(move |_cc| Ok(Box::new(FileWindow::new(source, cwd, rt)))),
        )
        .map_err(|e| format!("开窗失败: {e}"))
    })
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/shell_tests.rs"]
mod tests;
