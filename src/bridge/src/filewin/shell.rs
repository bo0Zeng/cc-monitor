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
//! ## ✅ 2026-09-20：**在一台真 Windows 11 桌面上量过了，然后才改的**
//!
//! 之前这一节的结论是读源码推出来的，处置写着「建议，本刀没做 —— 没有 Windows 机器，
//! 改了验不了」。那个理由现在不成立了：本机那台 Win11 虚拟机有活的交互桌面（session 1），
//! 一个 scratchpad 探针（`tao` ＋ `eframe`，与本仓同版本、同 `glow` 后端、
//! 清单与 `monitor.exe` 逐字同形 ⇒ 进程起来时是 `UNAWARE`）在那上面跑出四格：
//!
//! | 起 `tao` | winit `with_dpi_aware` | 进程起来时 | 建完 `tao` 事件循环 | 建完 eframe 之后 |
//! |---|---|---|---|---|
//! | 否 | **true**（默认） | `UNAWARE` | — | **`PER_MONITOR_AWARE_V2`** |
//! | 否 | **false** | `UNAWARE` | — | **`UNAWARE`** |
//! | 是 | true | `UNAWARE` | `PER_MONITOR_AWARE_V2` | `PER_MONITOR_AWARE_V2` |
//! | 是 | false | `UNAWARE` | `PER_MONITOR_AWARE_V2` | `PER_MONITOR_AWARE_V2` |
//!
//! 四格逐条买到的东西：
//!
//! 1. **「两个主人」是真的，不是读源码读出来的担心** —— 第 1 行里**根本没有 `tao`**，
//!    进程照样从 `UNAWARE` 变成 `PER_MONITOR_AWARE_V2` ⇒ **winit 自己确实会设这块进程级状态**。
//! 2. **两边设的确实是同一个值** —— `tao`（第 3/4 行的中间一格）与 winit（第 1 行）
//!    量出来都是 `PER_MONITOR_AWARE_V2`。⚠ `GetProcessDpiAwareness` 对 V1/V2 都回 `2`，
//!    分得开是因为另读了一次线程的 awareness context。
//! 3. **先跑的赢、后跑的是空操作** —— 第 3 行：`tao` 先设成 V2，之后 winit 带着
//!    `dpi_aware=true` 再设一次，**结果没变、也没报错**。
//! 4. 🔴 **`with_dpi_aware(false)` 是活的开关，不是一个装饰** —— 第 1 行对第 2 行，
//!    只差这一个参数，结果一个 V2 一个 `UNAWARE`。
//! 5. 🔴 **所以这一改在今天的生产顺序下是零行为变化** —— 第 3 行与第 4 行**逐格相同**。
//!    改掉的只有一件事：**这个进程里不再有第二个人去设那块全局状态**。
//!
//! ⇒ 于是 [`any_thread_hook`] 的 Windows 分支加了 `with_dpi_aware(false)`，
//! 把「进程 DPI 归谁管」明确判给 Tauri。判据住 [`tests`]。
//!
//! ⚠ **这一格虚拟机够用，可以说「验过了」** —— 它问的是**调用顺序与全局状态归属**，
//! 是逻辑题，不是显卡题。⚠ 但**别把它读宽**：`设计/99 §4.0 G2a` 逐字记着
//! KVM 虚拟机不是物理机 ⇒ **「真机上 DPI 缩放长什么样」那一格仍然没有读数**
//! （那台机器 DPI 缩放 100%、单显示器、QXL 虚拟显卡）。
//!
//! 出事的形状仍然照记：哪天 egui 窗口比 Tauri 主窗先建（或 winit 换了默认值），
//! WebView2 那侧的 DPI 行为会跟着变 —— 而本仓在 Windows DPI/WebView 上
//! 已经吃过亏（`真相源/70` 那一族、以及 `F12 nudge` 那处 WebView2 bounds 修正）。
//! 这一改正是把那个形状从「靠顺序碰巧对上」变成「只有一个人设它」。
//!
//! # ⚠ 没做到的，写在这儿而不是藏着〔第二刀 2026-09-20 订正〕
//!
//! - **本机跑不了真窗口**（`XDG_SESSION_TYPE=tty`，无图形会话）⇒ [`open_detached`]
//!   这条路在本机**没有端到端读数**；有的是 scratchpad 那个 `tao ＋ eframe` 原型的
//!   三趟读数（见 `super` 头注）与编译期的三档 `cargo check`。
//!   ⚠ 第二刀在这条上多欠一句：**鼠标真的双击一下那一行会怎样，本机判不了**
//!   （没有图形会话就没有真事件源）。判据喂的是**合成事件**，
//!   它买的是「egui 收到这串事件之后认出来的是哪一行」——
//!   逐条与那次现打的读数写在 [`super::rows`] 的 `paint_one_row` 头注里。
//! - **Windows 上一次都没跑过。**
//! - 多选 · 预览 · 双栏 · 右键菜单 **仍然一个都没有**（`设计/60 §4 戊` 代价第 2 条）。
//!   ⚠ **拖放从这一条里划出去了**：第二刀做了「拖入本机文件 → 上传到当前远端目录」
//!   那一半（见 [`FileWindow::start_drop`] 与 [`super::transfer`]）；
//!   **往外拖（下载）没做**，窗口之间互拖也没做。
//! - `设计/60 §5.4b`（大文件编辑改流式）**仍然没做** —— 这一刀没有编辑面。
//! - `设计/60 §5.4d`（拖入多文件先一次问完再并行）**第二刀做了**，
//!   住 [`super::transfer::run_drop`]；三段的顺序就是那个函数的结构，判据钉的是顺序与并行度。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::copy::{is_copyable, CopyBoard, CopyJob, CopyPrompt};
use super::find::{self, SearchBoard};
use super::fonts::{self, FontState};
use super::rows::{show_file_rows, show_hit_rows, RenderTally};
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

/// 最近一个窗口在**第一帧**上复核字体的结果。`None` = 还没有任何窗口复核过。
///
/// 🔴 判据拿它买的是「装字体这一步**真的接在开窗那条路上**」——
/// **扫源码买不到这个**（本仓 09-20 栽过一次：源码扫描那条与行为那条买的不是同一样东西，
/// 前者看不见「按钮接没接到方法上」）。⇒ 由 Xvfb 那条真开窗的判据读它。
static FONT_VERDICT: Mutex<Option<FontState>> = Mutex::new(None);

pub fn font_verdict() -> Option<FontState> {
    FONT_VERDICT.lock().unwrap().clone()
}

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
        // 🔴 **进程 DPI 归 Tauri 管** —— winit 不要再去设那块进程级状态。
        // 四格现打读数与逐条论证住本模块头注（2026-09-20，真 Win11 桌面）。
        // ⚠ 今天这一句是零行为变化（Tauri 必然先跑、已经设成 V2）；
        //   它买的是「一个进程里只有一个人设它」。
        EventLoopBuilderExtWindows::with_dpi_aware(builder, false);
    }
    #[cfg(target_os = "macos")]
    {
        // 够不着：macOS 没有 with_any_thread。留个明确的编译期落点，
        // 免得哪天上了 macOS 还以为这条路是通的。
        let _ = builder;
    }
}

/// 「本机」那颗按钮的落脚点 —— 用户的 home。
///
/// 🔴 **抽成一个具名函数不是风格，是登记要求**：`local_read_surface_registry::HOME_REACHES`
/// 按「上一处 `fn 名字`」给每一处 `home_dir()` 归属，写在 `ui()` 里的话那一行会被登记成
/// `("shell.rs", "ui")` —— 一个说不出自己在干什么的名字。
///
/// ⚠ 它**只把 home 当一个起点路径**，不去读 home 里的任何东西
/// （真正列目录的是 [`list_local`]，而它列的是用户之后走到哪就是哪）。
/// 拿不到 home 就退到 `.`（当前工作目录），**不猜一个路径出来**。
pub fn local_home() -> String {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string())
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
    /// `设计/60 §5` 第二段：零流量复制那一趟的状态机（**问覆盖 · 进度 · 裁决**）。
    pub copy_board: CopyBoard,
    /// 「复制为」那个框。`None` = 没在问名字。**UI 线程自己的**（理由见 [`CopyPrompt`]）。
    copy_prompt: Option<CopyPrompt>,
    /// 已经消化过几趟复制（同 [`Self::seen_rounds`]，两条路各一个数）。
    seen_copy_rounds: u64,
    /// `fonts.rs`：这个窗口的字体装没装上、复核没复核过。
    ///
    /// 🔴 默认是 `NotInstalled` 而**不是**「一切正常」——
    /// 判据直接 `FileWindow::new(...)` 建出来的窗口就是这一态，而它**会在界面上出声**。
    /// 「装字体那一步被谁摘了」因此不可能安静地过去。
    pub font: FontState,
    /// 〔第四刀〕`设计/60 §3.5`：搜索那一趟的共享落点（UI 线程读，tokio 那条写）。
    pub search: SearchBoard,
    /// 搜索框里那几个字。**UI 线程自己的**（同 [`Self::copy_prompt`] 的理由：
    /// 它是一个正在被编辑的草稿，不该出现在两条线程共享的那份状态里）。
    query: String,
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
            copy_board: CopyBoard::default(),
            copy_prompt: None,
            seen_copy_rounds: 0,
            font: FontState::NotInstalled,
            search: SearchBoard::default(),
            query: String::new(),
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

    /// 🔴〔第四刀〕**发一趟搜索** —— `设计/60 §3.5` 那一件在窗口上的落点。
    ///
    /// 回值 = 真的发出去了一趟（子串是空的、或这个窗口没有 tokio 运行时 ⇒ `false`）。
    ///
    /// # 本机与远端**同一条路**
    ///
    /// 两侧都走那条长连接上的 `files-find`（[`Source::origin`] 给的是登记表里的键）。
    /// ⚠ 与列目录**刻意不同**：列目录本机走文件系统、远端走 SFTP（[`Self::reload`]），
    /// 而搜索**只有后端那一条**（`设计/60 §2 档①`：SFTP 给不了搜索）。
    /// ⇒ 本机没起后端时这里拿到的是「没有可用的控制通道」那句话，而**不是**
    /// 悄悄退回一趟 `walkdir` —— 那会是第二份搜索实现，而且它没有常驻索引。
    ///
    /// # `force_rebuild` 是那颗按钮，不是一个周期
    ///
    /// `true` 只由界面上那颗「重建索引」来（用户明说「现在就重走」）。
    /// 平时是 `false`，要不要重走由**后端报的** `index_missing` / `stale` 决定
    /// （`设计/60 §3.5.2a`；周期那个数不在这一侧，见 [`super::find`] 头注 §四）。
    pub fn fire_search(&mut self, ctx: Option<egui::Context>, force_rebuild: bool) -> bool {
        let needle = self.query.trim().to_string();
        self.search.attach(ctx);
        self.search.invalidate(&needle);
        if needle.is_empty() && !force_rebuild {
            return false;
        }
        let Some(h) = self.rt.clone() else {
            self.search.say("搜索要一个 tokio 运行时，这个窗口没拿到");
            return false;
        };
        let mine = self.search.start();
        let board = self.search.clone();
        let origin = self.source.origin();
        let root = self.cwd.clone();
        h.spawn(async move {
            find::run_search(board, origin, root, needle, mine, force_rebuild).await;
        });
        true
    }

    /// 这一帧该画命中，还是画当前目录。**纯函数**（判据直接喂它两侧）。
    ///
    /// 🔴 判准是**搜索框里有没有字**，不是「有没有拿到命中」——
    /// 后者会让「搜了、一条都没中」退回目录列表，而那和「没搜」在屏幕上分不开
    /// （用户会以为搜索没生效）。
    pub fn showing_hits(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// 搜索框里现在是什么（判据用；生产那一侧直接改 [`Self::query`]）。
    pub fn query(&self) -> &str {
        &self.query
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
        // 🔴 **把窗口交给看板**，它自己会在「有问题要问 / 进度动了 / 跑完了」时敲一下。
        //   不交的话：进度条要等用户下次动鼠标才跳一格（egui 只在有事发生时才画下一帧）。
        board.attach(ctx);
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
                move |clashes| {
                    let rx = ask_board.ask(clashes);
                    async move { rx.await.unwrap_or_default() }
                },
                move |p| {
                    let cfg = up_cfg.clone();
                    let b = up_board.clone();
                    async move { super::transfer::upload_remote(&cfg, &p, &b).await }
                },
            )
            .await;
            board.finish(out);
        });
        true
    }

    /// 把这一帧 egui 收到的拖入文件接走。
    ///
    /// ⚠ **上一问还没答完就不接新的** —— 两摞问题叠在一个模态框上，
    /// 「一次问完」就变成「两次问完」。
    fn take_drops(&mut self, ctx: &egui::Context) {
        // ⚠〔第三刀〕复制那一摞也在问的时候同样不接 —— 两个模态框叠起来，
        //   「一次问完」就变成「答错了哪一个都不知道」。
        if self.board.is_asking() || self.copy_board.is_asking() || self.copy_prompt.is_some() {
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

    // ════════════════════════════════════════════════════════════════════
    // 〔第三刀〕`设计/60 §5` 第二段：**零流量复制**接到这一侧
    // ════════════════════════════════════════════════════════════════════

    /// 正摆着的那个「复制为」框（判据与 [`Self::copy_ui`] 用）。
    pub fn copy_prompt(&self) -> Option<&CopyPrompt> {
        self.copy_prompt.as_ref()
    }

    /// 🔴 **胶水**：列表说「第 `i` 行的复制被点了」→ 窗口摆出「复制为」那个框。
    ///
    /// 抽成一个函数的理由与 [`Self::apply_click`] 逐字相同：它只有两行，
    /// 而那两行正是「列表带出来的那个下标」与「窗口状态机」之间的**唯一**连接。
    /// 写在 `ui()` 里的话，`show_file_rows` 有判据、`begin_copy` 有判据，
    /// **中间这一跳谁都没在看** —— 那正是第一刀栽过的那一形。
    ///
    /// 回值 = 真的摆出来了。
    pub fn apply_copy_click(&mut self) -> bool {
        match self.tally.copy_clicked {
            Some(i) => self.begin_copy(i),
            None => false,
        }
    }

    /// 摆出「把第 `i` 行复制成什么名字」那个框。回值 = 真的摆出来了。
    ///
    /// ⚠ **两道闸，刻意重复**：
    /// - `super::copy::is_copyable` —— 目录与有损名一律不接。列表上那两档压根不画
    ///   那颗按钮，这里是第二道，防的是「按钮没了、调用还在」（那正是死值验刀 2 那一形）。
    /// - **远端才有** —— `copy-data` 是 SFTP 协议的扩展，本机复制压根不经 SFTP
    ///   （同 `parity_ledger` 里 `sftp_copy` 那一行旁边的理由）。本机源上**出声**，不静默。
    pub fn begin_copy(&mut self, i: usize) -> bool {
        if !self.source.is_remote() {
            *self.listing.error.lock().unwrap() =
                Some("这个窗口现在看的是本机，零流量复制只在远端那一侧成立".into());
            return false;
        }
        let row = {
            let rows = self.listing.rows.lock().unwrap();
            match rows.get(i) {
                Some(r) if is_copyable(r) => r.clone(),
                _ => return false,
            }
        };
        self.copy_prompt = Some(CopyPrompt::for_row(&self.cwd, &row));
        true
    }

    /// 收掉那个框，什么都不做。
    pub fn cancel_copy(&mut self) {
        self.copy_prompt = None;
    }

    /// 框里那个名字 → 一趟真复制。回值 = 真的起来了。
    ///
    /// ⚠ 名字不合法（空 / 带 `/` / 复制成自己）⇒ **框留着、出声**，不静默收掉
    /// —— 收掉的话用户点了「复制」什么都没发生，与成功长得一模一样。
    pub fn confirm_copy(&mut self, ctx: Option<egui::Context>) -> bool {
        let Some(p) = self.copy_prompt.clone() else {
            return false;
        };
        let Some(job) = p.to_job() else {
            *self.listing.error.lock().unwrap() = Some(format!(
                "「{}」不是一个能用的新名字 —— 只能在同一个目录里改名，不许为空、不许带 `/`、不许和原名相同",
                p.new_name
            ));
            return false;
        };
        if !self.start_copy(job, ctx) {
            return false;
        }
        self.copy_prompt = None;
        true
    }

    /// 起一趟 `§5` 第二段：**先问会不会覆盖，再动手，回来把裁决摆出来。**
    ///
    /// 🔴 三段的顺序不在这里，在 [`super::copy::run_copy`] 的结构里 ——
    /// 这里只负责把「问谁 · 怎么问 · 怎么起」三个口接上去（同 [`Self::start_drop`]）。
    /// 而本函数接不上（没运行时 / 本机源）要**出声**，判据见 `shell_tests`。
    pub fn start_copy(&mut self, job: CopyJob, ctx: Option<egui::Context>) -> bool {
        let Source::Remote(cfg) = &self.source else {
            *self.listing.error.lock().unwrap() =
                Some("这个窗口现在看的是本机，零流量复制只在远端那一侧成立".into());
            return false;
        };
        let Some(h) = self.rt.clone() else {
            *self.listing.error.lock().unwrap() =
                Some("复制要一个 tokio 运行时，这个窗口没拿到".into());
            return false;
        };
        let cfg = cfg.clone();
        let board = self.copy_board.clone();
        // 🔴 把窗口交给看板（同 `start_drop`）：进度与裁决都是从 tokio 那条线程写进来的，
        //    不敲一下，屏幕要等用户下次动鼠标才更新。
        board.attach(ctx);
        h.spawn(async move {
            let probe_cfg = cfg.clone();
            let copy_cfg = cfg.clone();
            let ask_board = board.clone();
            let run_board = board.clone();
            let out = super::copy::run_copy(
                job,
                move |j| async move { super::copy::probe_target(&probe_cfg, &j).await },
                move |j| {
                    let rx = ask_board.ask(j);
                    async move { rx.await.unwrap_or(false) }
                },
                move |j| async move { super::copy::copy_remote(&copy_cfg, &j, &run_board).await },
            )
            .await;
            board.finish(out);
        });
        true
    }

    /// 复制跑完一趟就重列当前目录（新文件要出现在列表里）。同 [`Self::settle_finished_drops`]。
    pub fn settle_finished_copies(&mut self) -> bool {
        let now = self.copy_board.rounds();
        if now == self.seen_copy_rounds {
            return false;
        }
        self.seen_copy_rounds = now;
        self.reload();
        true
    }

    /// 画「复制为」那个框。**模态** —— 名字没定下来之前不接别的。
    ///
    /// ⚠ 与 [`super::copy::CopyBoard::ui`] 分开两处，因为它们的状态住在两个地方：
    /// 这个框是 UI 线程自己的，那一摞（问覆盖 / 进度 / 裁决）是跨线程的。
    fn copy_ui(&mut self, ui: &mut egui::Ui) {
        let Some(mut p) = self.copy_prompt.clone() else {
            return;
        };
        let (mut go, mut cancel) = (false, false);
        egui::Modal::new(egui::Id::new("filewin-copy-as")).show(ui.ctx(), |ui| {
            ui.heading(format!("复制 {} 为：", p.src_name));
            ui.text_edit_singleline(&mut p.new_name);
            ui.label("⚠ 只在同一个目录里改名 —— 名字里不许带 `/`。");
            ui.horizontal(|ui| {
                if ui.button(super::copy::COPY_LABEL).clicked() {
                    go = true;
                }
                if ui.button("取消").clicked() {
                    cancel = true;
                }
            });
        });
        self.copy_prompt = Some(p);
        if cancel {
            self.cancel_copy();
        } else if go {
            let ctx = ui.ctx().clone();
            self.confirm_copy(Some(ctx));
        }
    }
}

impl FileWindow {
    /// 🔴〔第四刀〕**每一帧的正文。** 从 `eframe::App::ui` 里剥出来的，
    /// 剥的理由只有一个：**让它进得了执行链**。
    ///
    /// `eframe::App::ui` 的签名逐字要一个 `&mut eframe::Frame`
    /// （`eframe-0.36.2/src/epi.rs` 的 `trait App`），而那个类型在判据里**造不出来**
    /// ⇒ 剥出来之前，这个窗口**每一帧真正画的那段代码一条判据都没有**
    /// （现打：第一~三刀的判据全在零件上 —— `CopyBoard::ui` / `show_file_rows` /
    ///  `outcome_notice` —— 而把这几个零件从这个函数里摘掉，那些判据一条都不会红）。
    /// 这正是本仓那条「判据不在执行链上就等于不存在」。
    ///
    /// ⇒ 从此 `eframe::App::ui` 只剩一句委派，判据直接喂本函数。
    pub fn frame_body(&mut self, ui: &mut egui::Ui) {
        // 🔴 **第一帧**才复核得了字体 —— 之前碰 `fonts_mut` 会 panic，
        //    而本仓 release 是 `panic = "abort"`（理由逐条住 `fonts.rs §四`）。
        if self.font.settle(ui.ctx()) {
            *FONT_VERDICT.lock().unwrap() = Some(self.font.clone());
        }
        // 字体出问题就**一直**摆在这儿，不自动消失：
        // 它是一个「到你改掉为止都成立的状态」，不是一次性事件
        // （同 `INVARIANTS §12` 对「设置没生效」那条的判法）。
        if let Some(note) = self.font.notice() {
            ui.colored_label(egui::Color32::from_rgb(0xFF, 0xA5, 0x00), note);
        }
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
                go_local = Some(local_home());
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
        // 🔴〔第四刀〕搜索那一行 ＋ **新鲜度那一行**（`设计/60 §3.5.3` 那条 ⬜）。
        self.search_row(ui);
        // `§5.4d` 那一摞：确认框 ／ 进度。**画在列表之前** —— 它是模态的。
        self.board.ui(ui);
        // `§5` 第二段那一摞：覆盖确认 ／ 进度 ／ **上一趟走的是哪条路**。同样模态、同样在前。
        self.copy_board.ui(ui);
        self.copy_ui(ui);
        let ctx = ui.ctx().clone();
        self.take_drops(&ctx);
        self.settle_finished_drops();
        self.settle_finished_copies();
        ui.separator();
        // 每帧从零数起 —— 这个数是「这一帧物化了多少行」，不是累计。
        self.tally = RenderTally::default();
        // 🔴〔第四刀〕搜索框里有字 ⇒ 画命中，否则画当前目录。**二选一，不并排** ——
        //    并排会让「你现在看的是哪一摞」变成一个要靠标题猜的问题。
        if self.showing_hits() {
            let hits: Vec<String> = self
                .search
                .shown()
                .outcome
                .map(|o| o.hits.iter().map(|h| h.display()).collect())
                .unwrap_or_default();
            show_hit_rows(ui, &hits, &mut self.tally);
        } else {
            let rows = self.listing.rows.lock().unwrap();
            show_file_rows(ui, &rows, &mut self.tally, None);
        }
        // ⚠ 这两条只对**目录列表**那一摞有意义（下标索引的是 `listing.rows`）。
        //   命中那一摞一个可点控件都没有 ⇒ `tally.clicked` / `copy_clicked` 恒 `None`，
        //   理由与「为什么命中行不经 `show_file_rows`」一起写在
        //   [`super::rows::show_hit_rows`] 的头注里。
        self.apply_click();
        self.apply_copy_click();
    }

    /// 搜索那一行：输入框 ＋「重建索引」＋ 在飞指示，接着是新鲜度那一行。
    ///
    /// 🔴 **`changed()` 就发** —— `设计/60 §3.5` 要的形状逐字是「打字即出结果，不等」。
    /// ⚠ 代价如实记：**没有去抖** ⇒ 每敲一个字一趟往返。去抖要一个定时器，
    ///   而 monitor 侧每一个定时器都要进 `rust_timer_registry` 并说清谁退役它
    ///   ⇒ 那是一件独立的活。在飞的那几趟由 [`super::find::SearchBoard`] 的号作废掉，
    ///   所以**结果不会错**，贵的是往返次数（64 万条量纲上没量过）。
    fn search_row(&mut self, ui: &mut egui::Ui) {
        let mut fire = false;
        let mut rebuild = false;
        ui.horizontal(|ui| {
            ui.label("搜索");
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .desired_width(220.0)
                    .hint_text("文件名里的一段"),
            );
            if r.changed() {
                fire = true;
            }
            if ui.button("重建索引").clicked() {
                rebuild = true;
            }
            if self.search.is_running() {
                ui.spinner();
                ui.label("正在搜…");
            }
        });
        self.search.ui(ui);
        if fire || rebuild {
            let ctx = ui.ctx().clone();
            self.fire_search(Some(ctx), rebuild);
        }
    }
}

impl eframe::App for FileWindow {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame_body(ui);
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
            Box::new(move |cc| {
                let mut w = FileWindow::seeded(source, cwd, rt, rows);
                // 第一拍：读文件 ＋ `set_fonts`。**这里复核不了**（`fonts.rs §四`）。
                w.font = FontState::Pending(fonts::install(&cc.egui_ctx));
                Ok(Box::new(w) as Box<dyn eframe::App>)
            }),
        )
        .map_err(|e| format!("开窗失败: {e}"))
    })
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/shell_tests.rs"]
mod tests;
