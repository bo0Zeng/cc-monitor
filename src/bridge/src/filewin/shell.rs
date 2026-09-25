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
//! ⇒ 于是 [`any_thread_hook`] 的 Windows 分支当时加了 `with_dpi_aware(false)`，
//! 把「进程 DPI 归谁管」明确判给 Tauri。〔WN1 · 09-24：窗口进程独立之后翻回 `true`，见下一节。〕
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
//! ## 🔴 〔WN1 · 2026-09-24〕上面那个处置的**前提没了**，开关翻回 `true`
//!
//! 上面整节的前提是「同一个进程里有 Tauri」。第十三刀之后这个窗口跑在**自己的进程**里
//! （`cc-monitor-filewin`，躯体 `super::proc::child_main`，进程里一个 `tao` 事件循环都不建）
//! ⇒ 那个进程落在四格表的**上半**（「起 `tao` = 否」），而 `false` 那一格是第 2 行：
//! **全程 `UNAWARE`** —— 没有任何人再替它设 DPI，高 DPI 屏上整窗被系统按位图拉伸、发糊。
//! ⇒ [`any_thread_hook`] 的 Windows 分支改成 `with_dpi_aware(builder, true)`（第 1 行 ⇒ V2，
//! 与主窗口同一个值）。「一个进程里只有一个人设它」这条纪律**照旧成立**：这个进程里只有 winit。
//!
//! 判据两半、两侧异源（住 [`tests`]）：① 那一句的值是 `true`（`pin_line`，`false` 零命中）；
//! ② **前提**：这个 hook 在生产上只经「窗口进程入口 → `child_main` → `open_detached_seeded`」
//! 这一条链被用到，且 `child_main` 那个文件里一个 `tauri` / `tao` 都没有 —— 调用点集合两向相等。
//! 哪天有人又在 monitor 进程里开这个窗口，② 先红，逼他回来重答「进程 DPI 归谁」。
//!
//! 🚫 **买不到**：真 Windows 上一趟都没跑（本路不碰 Win11 虚拟机）。这一改的依据是上面那张
//! 09-20 的四格真机读数 ＋ 「窗口进程里没有 `tao`」这条源码事实，是逻辑题，不是新读数。
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
//! - 〔FW34 2026-09-24〕**预览 · 双栏 · 标签页做了**，住 [`super::workspace`]（最外一层）与
//!   [`super::preview`]；本文件的 [`FileWindow`] 照旧是「一个目录视图」，一个标签页就是一个它。
//!   〔此前这一格写的是「预览 · 双栏仍然一个都没有」（`设计/60 §4 戊` 代价第 2 条）。〕
//!   〔FW1+FW2 2026-09-24〕**多选与右键菜单做了**（还有键盘）：纯的那一份（选中态 · 键位 ·
//!   「能做什么」那张表）住 [`super::select`]，接到窗口上的那几跳住本文件
//!   [`FileWindow::apply_keys`] / [`FileWindow::apply_pick_click`] /
//!   [`FileWindow::apply_menu_click`] / [`FileWindow::perform`]。
//!   🔴 **写操作一条新路都没长**：键盘与菜单做的每一件，都落回行上那几颗按钮已经在走的
//!   那几个 `begin_*`（以及删除那一摞的 `start_writes`）⇒ 一次问完 · 只经通道说 `call`（〔FN1〕围栏那一道 V119 拿掉了）
//!   这几道闸一道都没绕开。
//!   ⚠ **拖放从这一条里划出去了**：第二刀做了「拖入本机文件 → 上传到当前远端目录」
//!   那一半（见 [`FileWindow::start_drop`] 与 [`super::transfer`]）；
//!   往外拖（下载）〔第八刀〕做了（[`super::download`]）；窗口之间互拖没做。
//! - `设计/60 §5.4b`（大文件编辑改流式）**仍然没做** —— 这一刀没有编辑面。
//! - `设计/60 §5.4d`（拖入多文件先一次问完再并行）**第二刀做了**，
//!   住 [`super::transfer::run_drop`]；三段的顺序就是那个函数的结构，判据钉的是顺序与并行度。

use crate::copy_table::copy_text;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::copy::{is_copyable, CopyBoard, CopyJob, CopyPrompt};
use super::find::{self, SearchBoard};
use super::fonts::{self, FontState};
use super::rows::{show_file_rows, show_hit_rows, HitTally, RenderTally};
use super::select::{self, Action, Intent, Selection, TypeAhead};
use super::source::{breadcrumbs, parent_dir, Line, Listed, SortBy, Source};
use super::transfer::{DropBoard, Pending};
use super::writeops::{is_writable, WriteBoard, WriteOp, WritePrompt, MKDIR_LABEL};

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

/// 〔FW2〕开过几次右键菜单 —— 每次开菜单换一个 egui id（理由住 [`MenuAt::serial`]）。
///
/// 🔴〔FW34〕**进程级，不是每个目录视图一个**：双栏时两栏同时画在一个 egui 上下文里，
/// 各数各的话两栏的第一个菜单撞同一个 id ⇒ 在另一栏右键那一下「只关不开」（与序号本来要防的同一形）。
static MENU_SERIAL: AtomicU64 = AtomicU64::new(0);

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
        // 🔴 〔WN1 · 09-24〕**进程 DPI 归这个窗口进程自己管** —— 这个进程里没有 Tauri。
        // 从前是 `false`（「进程 DPI 归 Tauri 管」，同进程时代）；窗口进程独立之后那一格
        // 让它全程 `UNAWARE`。四格读数与改回来的理由住本模块头注「WN1」那一节。
        EventLoopBuilderExtWindows::with_dpi_aware(builder, true);
    }
    #[cfg(target_os = "macos")]
    {
        // 够不着：macOS 没有 with_any_thread。留个明确的编译期落点，
        // 免得哪天上了 macOS 还以为这条路是通的。
        let _ = builder;
    }
}

/// **「存到哪儿」那一问的缺省落点** —— 用户的 home。
///
/// 🔴〔2026-09-23 本机侧退役〕**它换了唯一消费者，如实记**：从前它是工具栏上
/// 那颗「本机」按钮的落脚点（那颗按钮连同整个本机侧已经不在了，见
/// `source.rs` 头注那块墓碑）；今天它只有一个消费者 ——
/// [`super::download::default_dest`]（往外拖时「存到哪儿」那一格的缺省值）。
/// ⇒ 它**不再是文件管理器的一部分**，是**往外传**那条路上的一格。
///
/// 🔴 **抽成一个具名函数不是风格，是登记要求**：`local_read_surface_registry::HOME_REACHES`
/// 按「上一处 `fn 名字`」给每一处 `home_dir()` 归属，写在别人体内的话那一行会被登记成
/// 一个说不出自己在干什么的名字。
///
/// ⚠ 它**只把 home 当一条缺省路径**，不去读 home 里的任何东西 —— 一次 `home_dir()`，
/// 零次 `read_dir`。拿不到 home 就退到 `.`（当前工作目录），**不猜一个路径出来**。
pub fn local_home() -> String {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string())
}

/// 没连上通道时，每一件要问后端的事说的那一句（`D11`：不退回 SFTP、不静默）。
pub static NO_LINE: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinShell.noLine.message", &[]));

/// 「在此打开终端」要在那台远端上跑的那一串。
///
/// # 🔴 它是**第二份**实现，如实登记（第一份在 TS 里）
///
/// 旧面板那颗同名按钮走的是 `src/remote-launch.ts::buildOpenTerminalCmd`，
/// 逐字：`cd <quoted> && exec ${SHELL:-bash} -l`（`cwd` 为空 ⇒ 只有后半段）。
/// 窗口这一侧在 Rust 里，够不着那一份 ⇒ 这里是第二份。
///
/// **两份漂开的症状**：两个面板上同名的按钮把你落在不同的目录 / 不同的 shell 里。
/// 接住它的今天**只有一条**：本函数那两档由 `shell_tests` 钉成**逐字节相等**，
/// 期望串与 TS 那一行**逐字相同**。⚠ 这不是一条跨语言判据 ——
/// 真要那个，得在 `.vitest.ts` 那一侧加一份共享黄金夹具，而
/// **`.vitest.ts` 在本刀的红线里**。⇒ 登记为「有账、没自动对拍」，
/// 而它的**到期日是确定的**：旧面板退役那一刀 TS 那一份会跟着一起走，
/// 那时这一份就成了唯一一份。
///
/// ⚠ 引号走 `shell_quote_core::posix_quote` —— Rust 侧唯一那一份
/// （`quote_singleton_guard` 钉着「只许一个实现」）。**不许在这儿自己拼单引号。**
///
/// ⚠ **不用双引号**：`launch.rs` 会拒掉含双引号的 `remote_cmd`
/// （PowerShell 原生传参畸变那道防线）—— 那一条与 TS 那份注释逐字同源。
pub fn build_open_terminal_cmd(cwd: &str) -> String {
    let shell = "exec ${SHELL:-bash} -l";
    let c = cwd.trim();
    if c.is_empty() {
        shell.to_string()
    } else {
        format!("cd {} && {shell}", shell_quote_core::posix_quote(c))
    }
}

/// 列目录这件事的**共享落点** —— 一次列目录要写的东西全在这儿。
///
/// 🔴 抽成一个结构是因为它要被**两条线程**看：UI 线程读，tokio 那条写。
/// 散成四个 `Arc` 时「哪几个字段必须一起更新」只写在注释里，没有人守着。
#[derive(Clone)]
pub struct Listing {
    pub rows: Arc<Mutex<Vec<Listed>>>,
    pub error: Arc<Mutex<Option<String>>>,
    /// 🔴 **换目录的序号。** 每次 [`FileWindow::navigate_to`] +1。
    ///
    /// 异步回来的那一份**带着它出发时的号**：号不对就整份丢掉。
    /// 没有它的失效形状是真的会出现的：远端目录 A 慢、B 快 ⇒ 用户点进 A 立刻退回 B，
    /// A 的响应后到，把 B 的内容**盖成 A 的**，而路径栏上写着 B。
    pub epoch: Arc<AtomicU64>,
    /// 还有几趟列目录在飞。0 = 列完了。
    pub inflight: Arc<AtomicU64>,
    // 〔F2 · 2026-09-24〕这里原先还有一格「上一趟走没走成主路」（退路那一句话的落点）。
    // 退路整条拿掉之后（`D11`，理由住 `source.rs` 那一节）那一格没有可说的了，一起摘掉。
    /// 🔴 上一趟**被后端截断了吗**。
    ///
    /// 它必须画出来：「这个目录里就这么多」与「后端只给了前 N 条」
    /// 在屏幕上长得一样，而用户会据此以为某个文件不存在。
    pub truncated: Arc<std::sync::atomic::AtomicBool>,
}

impl Default for Listing {
    fn default() -> Self {
        Self {
            rows: Arc::new(Mutex::new(Vec::new())),
            error: Arc::new(Mutex::new(None)),
            epoch: Arc::new(AtomicU64::new(0)),
            inflight: Arc::new(AtomicU64::new(0)),
            truncated: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

/// 落一趟**经后端**的列目录（比 [`store_if_current`] 多一格：截断）。
///
/// 🔴 与它**刻意是两个函数**：没问成的那几形（无运行时 / 无通道）交不出这一格，
/// 而给它编一个「不知道」的第三态，只会让「没问过」与「问了没被截断」混在一起。
pub fn store_listed_if_current(
    l: &Listing,
    mine: u64,
    r: Result<(Vec<Listed>, bool), String>,
) -> bool {
    match r {
        Ok((rows, truncated)) => {
            // ⚠ 落行那一步会判号并可能整份丢掉，而截断那一格描述的是**同一趟**，
            //   不许一半落一半不落 ⇒ 只在真落了之后写它。
            let kept = store_if_current(l, mine, Ok(rows));
            if kept {
                l.truncated.store(truncated, Ordering::SeqCst);
            }
            kept
        }
        Err(e) => store_if_current(l, mine, Err(e)),
    }
}

/// 把一趟列目录的结果落进 [`Listing`] —— **号对不上就丢掉**。
///
/// 回值 = 真的落盘了。**自由函数**（不吃 `FileWindow`）⇒ 不用开窗、不用网络就判得动，
/// 而它正是生产那条路上唯一写 `rows` 的地方（见 [`Listing::start`]）。
pub fn store_if_current(l: &Listing, mine: u64, r: Result<Vec<Listed>, String>) -> bool {
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
/// 🔴〔第十刀〕**「就是这个文件」** —— 一次 reveal 的两半。
///
/// ⚠ 刻意是一个结构而不是 `Option<String>` ＋ 一个 `bool`：那两个散着放的时候
/// 「滚过了但名字清空了」与「名字还在但忘了滚」两种半态都可表示，
/// 而它们在屏幕上一个是「没高亮」、一个是「高亮了但在视野外」。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reveal {
    /// 要高亮那一行的**名字**（不是下标 —— 下标会随目录内容移位）。
    pub name: String,
    /// 滚过去了没有。**只滚一次**（每帧都滚就把用户自己的滚动按住了）。
    pub scrolled: bool,
}

pub struct FileWindow {
    pub source: Source,
    pub cwd: String,
    pub listing: Listing,
    pub tally: RenderTally,
    /// 〔第五刀〕命中那一摞这一帧画了什么。**与 [`Self::tally`] 刻意是两个类型** ——
    /// [`HitTally`] 里没有「谁被点了」这个概念，逐条理由住那个类型的头注。
    pub hits_tally: HitTally,
    /// 远端列目录要在 tokio 上跑，而**判据里大量窗口拿不到运行时**，所以是 `Option`。
    ///
    /// ⚠ 从前这句话写的是「本机不需要，所以是 `Option`」—— 本机那一侧退役之后
    /// 那个理由不成立了，但这一格**照旧是 `Option`**，换了一条真的理由：
    /// 起窗那条路（`super::proc`）与判据都可能手上没有运行时，而那时它要**出声**
    /// （[`Self::reload`] 里那一支），不许假装列了个空目录。
    pub rt: Option<tokio::runtime::Handle>,
    /// 🔴〔F2 · 2026-09-24〕**通道**：窗口进程够后端的唯一一条路（`source::Line`）。
    ///
    /// 是 `Option` 的理由与 [`Self::rt`] 同：判据里大量窗口不连后端（只画行、只点按钮）。
    /// 没有它的时候，每一件要问后端的事都**出声**（[`NO_LINE`]），不静默、不退回 SFTP（`D11`）。
    pub line: Option<Line>,
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
    /// 🔴〔第五刀〕`设计/99 §4.6.4`：那四条写操作的状态机（**一次问完 · 结果**；〔FN1〕围栏那一段 V119 拿掉了）。
    pub write_board: WriteBoard,
    /// 「叫什么名字 / 改成什么权限」那个框。`None` = 没在问。
    /// **UI 线程自己的**（理由见 [`WritePrompt`]）。
    write_prompt: Option<WritePrompt>,
    /// 〔GP1 · 第四波〕改权限那个框的**现值**那一趟（UI 线程读，tokio 那条写；`writeops::ModeProbe`）。
    pub mode_probe: super::writeops::ModeProbe,
    /// 已经消化过几摞写操作（同 [`Self::seen_rounds`]，每条路各一个数）。
    seen_write_rounds: u64,
    /// 🔴〔第八刀〕**往外拖**那一趟的共享落点（进度 · 结局）。
    pub pull: super::download::DownloadBoard,
    /// 「存到哪儿 / 盖掉它吗」那两问。`None` = 没在问。
    /// **UI 线程自己的**（同 [`Self::write_prompt`] 的理由：它是一个正在被编辑的草稿）。
    pull_ask: Option<super::download::Ask>,
    /// 〔F7c · 第三波 09-24〕工具栏「上传」那一问（状态与判定全在 `upload.rs`，这里只挂着）。
    pub upload: super::upload::UploadPrompt,
    /// 🔴〔第九刀〕编辑那一趟的共享落点（读到货 · 存结局）。
    pub edits: super::editor::EditBoard,
    /// 打开着的那一份文本。`None` = 没在编辑。**UI 线程自己的**。
    editing: Option<super::editor::Pane>,
    /// 关窗那一问正摆着吗（改了没存）。
    asking_discard: bool,
    /// 🔴〔补齐五项 2026-09-23〕**按什么排**（工具栏那个下拉的状态）。
    ///
    /// 缺省是 [`SortBy::Name`]，也就是本刀之前那个写死的序（逐字节相同，
    /// 判据住 `source_tests::the_default_order_is_byte_for_byte_what_it_was_before`）。
    pub sort_by: SortBy,
    /// 🔴〔补齐五项〕「在此打开终端」那一下**说了什么**（`None` = 没点过 / 上一下没话说）。
    ///
    /// # 为什么这一格非有不可
    ///
    /// 那条命令在 **POSIX 上恒定失败**（`launch.rs::POSIX_NO_TERMINAL_WINDOW`：
    /// 「本机不是 Windows，刻意不替你挑终端模拟器」），在 Windows 上也可能失败
    /// （那台远端的配置没存全）。一次失败与一次成功在屏幕上长得一样
    /// ⇒ 用户点了按钮、什么都没发生、也没有一句话 —— 那正是本仓的头号病形。
    /// ⇒ 结果落在这一格，界面上画出来，判据读同一个值。
    term_notice: Arc<Mutex<Option<String>>>,
    /// 🔴〔第十刀〕**「就是这个文件」** —— 要高亮的那一行的名字 ＋ 滚过去了没有。
    ///
    /// 它是 `P3`（老面板退役）的最后一格功能前置：老面板 `open(revealPath)`
    /// 那一形（会话工具卡 → 文件跳转，`src/cards/index.ts::openRemoteFileInSftp`）
    /// 在这之前窗口**一处都没有**。
    ///
    /// ⚠ 两个字段刻意分开：**高亮要一直留着**（一帧的高亮在连续重绘的窗口上等于看不见），
    /// 而**滚只滚一次**（每帧都滚就把用户自己的滚动按住了）。
    reveal: Option<Reveal>,
    /// 🔴〔FW1+FW2〕**选中态**（哪几行 · 键盘光标 · Shift 的锚）。**UI 线程自己的**。
    ///
    /// ⚠ 按名字记（理由住 [`super::select`] 头注 §二）。换目录清空；
    ///   一摞写操作跑完只清「选中」、留着光标（[`Self::settle_finished_writes`]）。
    selection: Selection,
    /// 〔FW1〕打字跳转攒着的那几个字。
    type_ahead: TypeAhead,
    /// 〔FW1〕键盘把光标挪到了第几行 ⇒ 这一帧画列表时要不要把它滚进视野。**只滚一次**。
    key_scroll: Option<usize>,
    /// 〔FW2〕摆着的那个右键菜单（`None` = 没摆）。
    menu: Option<MenuAt>,
    /// 〔FW1+FW2〕键盘 / 菜单那一下**做不了**时说的那句话（`None` = 没话说）。
    ///
    /// ⚠ 刻意不写进 `listing.error`：那一格只在下一趟列目录时才清，
    ///   而「打字跳转没找到」是一句**下一次按键就过时**的话 ⇒ 下一次按键 / 点击就清掉。
    key_notice: Option<String>,
    /// 〔F7b〕「新建空文件叫什么」那个框。`None` = 没在问。逻辑住 [`super::create`]。
    pub(super) new_file: Option<super::create::NewFilePrompt>,
    /// 〔FW34〕书签（一个窗口一份，所有标签页 / 两栏共用；逻辑住 [`super::bookmarks`]）。
    /// `None` ＝ 没接上（判据里直接建的窗口）⇒ 书签栏不画。生产那条开窗路恒是 `Some`。
    pub shelf: Option<super::bookmarks::Shelf>,
    /// 〔FW34〕这个目录视图此刻**是不是焦点那一个**（双栏 / 标签页时只有一个是）。
    ///
    /// `false` ⇒ 不接键盘、不接拖入（[`Self::keys_blocked`] / [`Self::take_drops`]）。缺省 `true`：
    /// 单独建出来的目录视图（判据里那几百个）就是唯一那一个。谁来改它：[`super::workspace`]。
    pub focused: bool,
    /// 〔FW34〕起过几摞写操作（与 `write_board.rounds()` 比 ⇒ 有没有还没回话的；关标签那一问用）。
    writes_started: u64,
}

/// 〔FW2〕一个摆着的右键菜单：**在哪儿 · 列哪几项 · 对几项说话**。
///
/// 🔴 那几项是**开菜单那一刻**按 [`select::actions_for`] 算好的快照 ——
/// 每帧重算要扫一遍整摞行找选中（64 万行那一档每帧一趟 O(n)）。
/// 快照不会过时：菜单摆着的时候键盘不接（[`FileWindow::apply_keys`] 的闸），
/// 点别处菜单先关；而点菜单上那一项时 [`FileWindow::perform`] **再问一次**那张表。
#[derive(Clone, Debug, PartialEq)]
pub struct MenuAt {
    pub at: egui::Pos2,
    pub actions: Vec<Action>,
    pub n: usize,
    /// 🔴 第几次开菜单。egui 的弹层按「上一帧有没有这个 id 的响应」判「刚打开」：
    ///   摆着一个菜单时在另一行上再右键一下，同一个 id 会被当成「开着时有人点了别处」
    ///   当场关掉 ⇒ 右键第二下只关不开。每次开菜单换一个 id 就没有这一形。
    pub serial: u64,
}

/// 〔FW2〕菜单上一项都没有时摆的那一句（有损名那一档：什么都做不了，但要说出来）。
pub static MENU_EMPTY: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinShell.menuEmpty.message", &[]));

impl FileWindow {
    pub fn new(source: Source, cwd: String, rt: Option<tokio::runtime::Handle>) -> Self {
        let w = Self::seeded(source, cwd, rt, Vec::<Listed>::new());
        w.reload();
        w
    }

    /// 接上通道（`proc::child_main` 拨通之后调；判据里接一台合成后端）。
    pub fn attach_line(&mut self, line: Line) {
        self.line = Some(line);
    }

    /// 带着**已经列好的那一屏**建窗 —— [`super::entry::open_file_window`] 走这条。
    ///
    /// 🔴 为什么要有它：那条命令为了能在 webview 那侧**出声**（目录列不出来就别开窗），
    /// 已经先列过一趟了。再让 `new()` 列第二趟，就是对同一个远端目录连打两次往返。
    pub fn seeded(
        source: Source,
        cwd: String,
        rt: Option<tokio::runtime::Handle>,
        rows: impl IntoIterator<Item = impl Into<Listed>>,
    ) -> Self {
        let listing = Listing::default();
        // 🔴〔补齐五项〕进程边界那一屏走的是 `Vec<Listed>`（`proc::OpenRequest::rows`）
        //    ⇒ 链接与时间两格**过得了这条边界**，第一屏就有。入参收 `Into<Listed>` 是为了
        //    判据夹具照旧能喂 `Vec<Row>`（那一形 ＝ `Listed::plain`，两格都「没送」）。
        // ⚠ **这一屏刻意不再排一次**：入口那条命令列的时候已经按缺省那一档排过
        //   （`source::rows_from_ls_data`），再排一遍是恒等。
        //   在这儿插一次 `sort_rows` 试过一趟，读数如实记：
        //   `shell_tests::a_write_click_from_the_list_reaches_the_right_row` 当场红 ——
        //   它喂的夹具是乱序的，于是「第 1 行是哪一行」被改掉了。
        //   ⇒ 那是一次**谁都没要求的行为变更**（生产上零收益，判据上真伤），撤掉。
        //   用户换档那一下由 [`FileWindow::set_sort`] 就地重排，不经这里。
        *listing.rows.lock().unwrap() = rows.into_iter().map(Into::into).collect();
        Self {
            source,
            cwd,
            listing,
            tally: RenderTally::default(),
            hits_tally: HitTally::default(),
            rt,
            line: None,
            board: DropBoard::default(),
            seen_rounds: 0,
            copy_board: CopyBoard::default(),
            copy_prompt: None,
            seen_copy_rounds: 0,
            font: FontState::NotInstalled,
            search: SearchBoard::default(),
            query: String::new(),
            write_board: WriteBoard::default(),
            write_prompt: None,
            mode_probe: super::writeops::ModeProbe::default(),
            seen_write_rounds: 0,
            pull: super::download::DownloadBoard::default(),
            pull_ask: None,
            upload: super::upload::UploadPrompt::default(),
            edits: super::editor::EditBoard::default(),
            editing: None,
            asking_discard: false,
            sort_by: SortBy::default(),
            term_notice: Arc::new(Mutex::new(None)),
            reveal: None,
            selection: Selection::default(),
            type_ahead: TypeAhead::default(),
            key_scroll: None,
            menu: None,
            key_notice: None,
            new_file: None,
            shelf: None,
            focused: true,
            writes_started: 0,
        }
    }

    /// 重新列一次当前目录。**本机同步做完；远端扔给 tokio，不堵住 UI 线程。**
    pub fn reload(&self) {
        let l = self.listing.clone();
        let mine = l.start();
        let cwd = self.cwd.clone();
        // 🔴〔第十二刀 2026-09-22〕**先问后端，问不到才退回旧路** ——
        //    用户指令的第 1 步，逐条理由住 `source.rs` 那一段头注。
        // 🔴〔F2 · 2026-09-24〕「问不到才退回旧路」那半句**拿掉了**（`D11`）：只问后端。
        match &self.rt {
            // ── 有运行时 ⇒ 主路（`files-ls` on 这个 origin）───────────
            Some(h) => {
                // 🔴〔F2〕有运行时但没连上通道 ⇒ 出声（`D11`：不退回 SFTP）。
                let Some(line) = self.line.clone() else {
                    store_if_current(&l, mine, Err(NO_LINE.to_string()));
                    return;
                };
                let source = self.source.clone();
                // ⚠ 带**这一刻**选的那一档走。用户在飞行途中换了档 ⇒ [`Self::set_sort`]
                //   会把落地的那一摞就地重排，所以两种顺序都不会错。
                let by = self.sort_by;
                h.spawn(async move {
                    let r = super::source::list_dir(&line, &source, &cwd, by).await;
                    store_listed_if_current(&l, mine, r);
                });
            }
            // ── 没有运行时 ⇒ 问不了后端，也走不了 SFTP 那条退路 ⇒ **出声**。
            //
            // 🔴〔2026-09-23 本机侧退役〕这一支从前是两支：本机那一支同步
            //    `read_dir` 一趟、把「这一屏是 monitor 自己列的」写进裁决格。
            //    本机侧不在了（`source.rs` 头注那块墓碑）⇒ 现在只剩「出声」这一种结局。
            // ⚠ **不许**把它改回「静默交一个空列表」：那与「这个目录真的是空的」
            //    在屏幕上分不开，而那正是本仓的头号病形。
            None => {
                store_if_current(
                    &l,
                    mine,
                    Err(copy_text("rsFilewinShell.reload.noRuntime", &[]).into()),
                );
            }
        }
    }

    /// 换到 `path` 并重列。**在飞的那几趟从此全部作废**（见 [`Listing::epoch`]）。
    pub fn navigate_to(&mut self, path: String) {
        if path == self.cwd {
            return;
        }
        // 🔴〔第十刀〕换了目录，那一行就不在这儿了 ⇒ 高亮清掉。
        //    留着的话，新目录里**恰好同名**的另一个文件会被高亮 ——
        //    而用户会以为那就是他要找的那个。
        self.reveal = None;
        // 🔴〔FW1+FW2〕选中态按名字记 ⇒ 新目录里**同名**的那几个不是同一样东西。
        //    留着的话按 Delete 删的是新目录里恰好同名的文件。
        self.selection.clear();
        self.type_ahead.clear();
        self.key_scroll = None;
        self.menu = None;
        self.key_notice = None;
        self.cwd = path;
        self.listing.invalidate();
        self.reload();
        // 〔FW34〕换目录时现读一次书签：别的窗口刚加的那几条从这里进来（小文件一次读，不是每帧）。
        if let Some(s) = &self.shelf {
            s.refresh();
        }
    }

    /// 上一级。已经在顶上就什么都不做（[`parent_dir`] 到顶回原值）。
    pub fn navigate_up(&mut self) {
        let up = parent_dir(&self.cwd);
        self.navigate_to(up);
    }

    /// 换一种排序，并**把手上这一摞就地重排**。回值 = 真的换了（同一档 ⇒ `false`）。
    ///
    /// # 🔴 为什么不是「记下来，下次列目录的时候用」
    ///
    /// 那样点了下拉之后屏幕上一动不动，要等用户自己按一下刷新 ——
    /// 而「点了没反应」与「这个目录本来就是这个序」在屏幕上分不开。
    ///
    /// # ⚠ 为什么不是「每帧排一次」
    ///
    /// `设计/60 §4 戊` 那条纪律：64 万行那一档每帧一次 `sort` 直接把帧时打穿
    /// （虚拟滚动省的是**画**，不是遍历）。⇒ 排序只在两个时刻发生：
    /// **一屏落地**（[`super::source::list_dir`]）与**用户换档**（这里）。
    pub fn set_sort(&mut self, by: SortBy) -> bool {
        if self.sort_by == by {
            return false;
        }
        self.sort_by = by;
        super::source::sort_rows(&mut self.listing.rows.lock().unwrap(), by);
        true
    }

    /// 「在此打开终端」那一下说了什么（`None` = 没话说）。判据与界面看同一个值。
    pub fn term_notice(&self) -> Option<String> {
        self.term_notice.lock().unwrap().clone()
    }

    /// 在**当前这个目录**里给用户开一个真终端。回值 = 真的发出去了。
    ///
    /// # 🔴 它为什么直接 `await` 那条 `#[tauri::command]`
    ///
    /// 与 [`super::source::list_remote`] 同一条理由（住 `source` 头注）：
    /// `launch::launch_remote_terminal` 同时就是一个普通的 `pub async fn`，
    /// 而窗口与 app 在**同一份代码**里编出来 ⇒ 这里是一次普通函数调用，
    /// 不过 IPC、不过 serde。它是全仓**唯一**的开窗出口
    /// （`launcher_identity_registry` 把它记成 `L2`），所以这里不许另拼一条 `ssh`。
    ///
    /// ⚠ 这条边是**新长出来的一条** app 侧依赖 ⇒ 同一拍进了
    /// `boundary_tests::REGISTERED`（`Kind::Terminal`）。那张表少一行就红。
    ///
    /// # ⚠ 它在 Linux 上**恒定「失败」，而那不是缺陷**
    ///
    /// POSIX 上 `launch_powershell_window` 回的是 `POSIX_NO_TERMINAL_WINDOW`
    /// （逐字：「刻意不替你挑终端模拟器」）—— 那是一条**既定设计**，不是没做完。
    /// 旧面板那颗同名按钮今天是同一个结局（它把那句话丢进一个 toast）。
    /// ⇒ 本窗口把那句话摆在工具栏下面（[`Self::term_notice`]），**不假装成功**。
    ///
    /// ⚠ **买不到什么，两条**：① 真有一个终端窗口弹出来 —— 那要 Windows
    /// ＋ 一个图形会话，本机两样都没有；② 那句话里「命令已复制」的**复制**那一半
    /// 是前端剪贴板兜底干的活（`remote-launch-run.ts`），**这个窗口没有它**
    /// ⇒ 那半句在这儿是假的。如实登记为**没做**（旧面板那颗按钮同样没有）。
    pub fn open_terminal_here(&mut self, ctx: Option<egui::Context>) -> bool {
        let Some(h) = self.rt.clone() else {
            *self.term_notice.lock().unwrap() =
                Some(copy_text("rsFilewinShell.terminal.noRuntime", &[]).into());
            return false;
        };
        let origin = self.source.origin();
        let cmd = build_open_terminal_cmd(&self.cwd);
        let slot = self.term_notice.clone();
        *slot.lock().unwrap() = None;
        h.spawn(async move {
            // 〔合并主线 dfc7c4e9：T4 给这条命令加了 `rbind_token`〕`None` —— 与旧面板那颗同名按钮
            //   逐字同形（`src/sftp/panel.ts` 那一处不带令牌）：「在此打开终端」开的是一个裸 shell，
            //   不是一场要被 ↗ 找回来的会话，没有令牌可铸。
            let said = match crate::launch::launch_remote_terminal(origin.0, cmd, None).await {
                Ok(()) => None,
                Err(why) => Some(copy_text(
                    "rsFilewinShell.terminal.failed",
                    &[("why", &why.to_string())],
                )),
            };
            *slot.lock().unwrap() = said;
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
        true
    }

    /// 点开第 `i` 行。**目录进去，文件不动。**
    ///
    /// 回值 = 真的换了目录。对文件是明确的「什么都不做」：文件那一侧各有各的口
    /// （编辑 · 下载在行上与菜单里；〔FW34〕预览是右侧那块面板，跟着选中走，不靠双击）。
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

    /// 现在高亮着哪一行的名字（`None` = 没有）。判据与界面看同一个值。
    pub fn reveal_name(&self) -> Option<&str> {
        self.reveal.as_ref().map(|r| r.name.as_str())
    }

    /// 摆一次 reveal：「进这个目录，并且高亮 `name` 那一行」。
    ///
    /// ⚠ 它**不判那一行在不在** —— 那要等目录列回来（异步）。
    /// 「列回来了却没有那一行」这一形由 [`Self::take_reveal_offset`] 说出来。
    pub fn set_reveal(&mut self, name: &str) {
        self.reveal = Some(Reveal {
            name: name.to_string(),
            scrolled: false,
        });
    }

    /// 这一帧要不要滚、滚到哪儿。**只在第一次调用时给值**（之后回 `None`）。
    ///
    /// 🔴 回值里那个 `Err` 是**「那一行不在这一摞里」** —— 文件刚被删了 / 改名了。
    /// 调用方要把它说出来：静默什么都不做与「跳过去了」在屏幕上同形。
    /// ⚠ **它自己拿那把锁，而且只握到扫完为止** —— 第一版我让它吃一个 `&[Row]`，
    /// 于是调用方得先 `rows.lock().unwrap().clone()` 才借得出 `&mut self`（E0502）
    /// ⇒ **那会每帧克隆整摞行**。编得过，但在 64 万条量纲上正是
    /// `设计/60 §4 戊` 那条纪律的反面。⇒ 锁与扫描都收进来，一次克隆都没有。
    pub fn take_reveal_offset(&mut self, pitch: f32) -> Option<Result<f32, String>> {
        // ① 先取出要找的名字（只读借用，随即放掉）。
        let want = match self.reveal.as_ref() {
            Some(r) if !r.scrolled => r.name.clone(),
            _ => return None,
        };
        // ② 握锁扫一遍（**只扫一遍，只在这一次 reveal 里**，不是每帧）。
        let found = {
            let rows = self.listing.rows.lock().unwrap();
            // ⚠ 列表还没到货 ⇒ **这一帧不算「滚过了」**，下一帧再来
            //   （不然会在空列表上判成「那一行不在」）。
            if rows.is_empty() {
                return None;
            }
            super::rows::reveal_index(&rows, &want)
        };
        // ③ 锁放掉了，这里才改自己。
        if let Some(r) = self.reveal.as_mut() {
            r.scrolled = true;
        }
        match found {
            // 🔴 像素那一步在这儿，用的是那**唯一住址**的步距（`rows::row_pitch`）——
            //    第一版这里乘的是 `ROW_HEIGHT`，漏了行间距，滚偏 14%。
            Some(i) => Some(Ok(i as f32 * pitch)),
            None => {
                // 找不到就把高亮也撤掉 —— 留着等于在屏幕上标一个不存在的东西。
                self.reveal = None;
                Some(Err(copy_text(
                    "rsFilewinShell.reveal.gone",
                    &[("want", &want.to_string())],
                )))
            }
        }
    }

    /// 🔴〔第四刀〕**发一趟搜索** —— `设计/60 §3.5` 那一件在窗口上的落点。
    ///
    /// 回值 = 真的发出去了一趟（子串是空的、或这个窗口没有 tokio 运行时 ⇒ `false`）。
    ///
    /// # 本机与远端**同一条路**
    ///
    /// 走那条长连接上的 `files-find`（[`Source::origin`] 给的是登记表里的键）。
    /// ⚠ 与列目录**刻意不同**：列目录**有一条退路**（后端问不到就退回 SFTP，
    /// 见 [`Self::reload`] 与 `super::source::list_dir`），而搜索**只有后端那一条**
    /// （`设计/60 §2 档①`：SFTP 给不了搜索）。
    /// ⇒ 后端没起来时这里拿到的是「没有可用的控制通道」那句话，而**不是**
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
            self.search
                .say(&copy_text("rsFilewinShell.search.noRuntime", &[]));
            return false;
        };
        let Some(line) = self.line.clone() else {
            self.search.say(NO_LINE.as_str());
            return false;
        };
        let mine = self.search.start();
        let board = self.search.clone();
        let origin = self.source.origin();
        let root = self.cwd.clone();
        h.spawn(async move {
            find::run_search(board, line, origin, root, needle, mine, force_rebuild).await;
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
    /// 🔴〔2026-09-23 本机侧退役〕**这里少了一道闸，而那是对的。** 从前开头有一句
    /// `if !self.source.is_remote() { return Vec::new(); }` —— 它防的是
    /// 「窗口看着本机时把本机文件拖进本机」。`Source` 收成 newtype 之后
    /// 那个状态**写不出来了**（理由住 `super::source::Source` 头注）⇒ 那道闸
    /// 不是被删掉了，是它要守的东西整条不在了。
    pub fn pending_for(&self, local_paths: &[String]) -> Vec<Pending> {
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
    /// 而本函数接不上（**没运行时**）要**出声**，判据见 `shell_tests`。
    /// ⚠ 从前这句话是「没运行时 / 本机源」—— 本机源那一支不在了（见 `Source` 头注）。
    pub fn start_drop(&mut self, items: Vec<Pending>, ctx: Option<egui::Context>) -> bool {
        if items.is_empty() {
            return false;
        }
        let Some(h) = self.rt.clone() else {
            *self.listing.error.lock().unwrap() =
                Some(copy_text("rsFilewinShell.upload.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string());
            return false;
        };
        let origin = self.source.origin();
        let board = self.board.clone();
        // 🔴 **把窗口交给看板**，它自己会在「有问题要问 / 进度动了 / 跑完了」时敲一下。
        //   不交的话：进度条要等用户下次动鼠标才跳一格（egui 只在有事发生时才画下一帧）。
        board.attach(ctx);
        // 🔴〔第五刀〕新的一摞 ⇒ 取消台复位。不复位的后果是具体的：
        //    上一摞按过取消 ⇒ 这一摞一件都起不来，而屏幕上看起来是「拖进去没反应」。
        board.cancels().reset();
        h.spawn(async move {
            // 〔F7c〕上传那一腿也经通道（开单 → 起跑并看 → 提交），拿同一条线 ＋ 同一个地址。
            let (up_line, up_origin) = (line.clone(), origin.clone());
            let ask_board = board.clone();
            let up_board = board.clone();
            let out =
                super::transfer::run_drop(
                    items,
                    super::transfer::lanes(),
                    move |p| {
                        let line = line.clone();
                        let origin = origin.clone();
                        async move {
                            super::transfer::probe_remote(&line, &origin, &p.remote_path).await
                        }
                    },
                    move |clashes| {
                        let rx = ask_board.ask(clashes);
                        async move { rx.await.unwrap_or_default() }
                    },
                    move |p| {
                        let (line, origin) = (up_line.clone(), up_origin.clone());
                        let b = up_board.clone();
                        // 🔴〔第五刀〕**取消那道闸在这儿**：按过取消之后，还没起的那几件
                        //   一件都不起，而且这一趟的 `transfer_id` 由那道闸造并登记
                        //   （两件事一个落点，理由住 `launch_unless_cancelled`）。
                        async move {
                            let desk = b.cancels();
                            let name = p.name.clone();
                            super::transfer::launch_unless_cancelled(
                                &desk,
                                &name,
                                |_id| async move {
                                    super::transfer::upload_remote(&line, &origin, &p, &b).await
                                },
                            )
                            .await
                        }
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
        // ⚠〔第五刀〕写操作那一摞同理，而它的代价更大：那个框上「都别做」与
        //   上传那个框上「全都不覆盖」叠在一起，答错一个就是删错东西。
        // 〔FW34〕不是焦点那一个 ⇒ 不接（两栏都接的话，拖一个文件进来两栏各传一次）。
        if !self.focused || self.modal_up() {
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
            *self.listing.error.lock().unwrap() = Some(copy_text(
                "rsFilewinShell.drop.noNames",
                &[("n", &(dropped.len()).to_string())],
            ));
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
    /// ⚠ **这里只剩一道闸了，如实记**：`super::copy::is_copyable` —— 目录与有损名
    /// 一律不接。列表上那两档压根不画那颗按钮，这里是第二道，防的是
    /// 「按钮没了、调用还在」（那正是死值验刀 2 那一形）。
    ///
    /// 🔴〔2026-09-23 本机侧退役〕**第二道闸（「远端才有」）删了。** `copy-data` 是
    /// SFTP 协议的扩展，本机复制压根不经 SFTP ⇒ 从前本机源上要出声拒。
    /// 今天窗口**只可能**看着一台远端（`Source` 是 newtype）⇒ 那句话说不出口了。
    pub fn begin_copy(&mut self, i: usize) -> bool {
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
            *self.listing.error.lock().unwrap() = Some(copy_text(
                "rsFilewinShell.copy.badName",
                &[("name", &p.new_name.to_string())],
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
    /// 而本函数接不上（**没运行时**）要**出声**，判据见 `shell_tests`。
    pub fn start_copy(&mut self, job: CopyJob, ctx: Option<egui::Context>) -> bool {
        self.start_copy_via(job, ctx, false)
    }

    /// 〔FW34〕同 [`Self::start_copy`]，但目标在**另一个目录**（「复制到另一栏」）：
    /// 三段一个字没变（探 → 一次问覆盖 → 才动手），只是动手那一下的参数换成跨目录那一形
    /// （[`super::workspace::across_args`]）。起在**目标那一栏**上 ⇒ 问与结局画在那一侧。
    pub fn start_copy_across(&mut self, job: CopyJob, ctx: Option<egui::Context>) -> bool {
        self.start_copy_via(job, ctx, true)
    }

    fn start_copy_via(&mut self, job: CopyJob, ctx: Option<egui::Context>, across: bool) -> bool {
        let Some(h) = self.rt.clone() else {
            *self.listing.error.lock().unwrap() =
                Some(copy_text("rsFilewinShell.copy.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string());
            return false;
        };
        let origin = self.source.origin();
        let board = self.copy_board.clone();
        // 🔴 把窗口交给看板（同 `start_drop`）：「在跑」与结局都是从 tokio 那条线程写进来的，
        //    不敲一下，屏幕要等用户下次动鼠标才更新。
        board.attach(ctx);
        h.spawn(async move {
            let (probe_line, probe_origin) = (line.clone(), origin.clone());
            let ask_board = board.clone();
            let run_board = board.clone();
            let out =
                super::copy::run_copy(
                    job,
                    move |j| async move {
                        super::copy::probe_target(&probe_line, &probe_origin, &j).await
                    },
                    move |j| {
                        let rx = ask_board.ask(j);
                        async move { rx.await.unwrap_or(false) }
                    },
                    // 〔F7a〕经通道问后端 `files-copy`；第二个参数是覆盖策略（问过且答了「覆盖」）。
                    //   后端这一趟取消不掉 ⇒ 不再走取消那道闸（理由住 `copy.rs` 头注）。
                    move |j, overwrite| async move {
                        run_board.begin(&j.name);
                        if across {
                            super::workspace::copy_across(&line, &origin, &j, overwrite).await
                        } else {
                            super::copy::copy_remote(&line, &origin, &j, overwrite).await
                        }
                    },
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
            ui.heading(copy_text(
                "rsFilewinShell.copyUi.heading",
                &[("name", &p.src_name.to_string())],
            ));
            ui.text_edit_singleline(&mut p.new_name);
            ui.label(&copy_text("rsFilewinShell.copyUi.sameDirOnly", &[]));
            ui.horizontal(|ui| {
                if ui.button(super::copy::COPY_LABEL.as_str()).clicked() {
                    go = true;
                }
                if ui
                    .button(&copy_text("rsFilewinShell.copyUi.cancel", &[]))
                    .clicked()
                {
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

    // ════════════════════════════════════════════════════════════════════
    // 🔴〔第五刀〕`设计/99 §4.6.4`：**新建目录 · 改名 · 删除 · 改权限**
    // ════════════════════════════════════════════════════════════════════

    /// 正摆着的那个框（判据与 [`Self::write_ui`] 用）。
    pub fn write_prompt(&self) -> Option<&WritePrompt> {
        self.write_prompt.as_ref()
    }

    /// 摆出「新建目录」那个框。回值 = 真的摆出来了。
    ///
    /// 🔴〔2026-09-23 本机侧退役〕**`remote_only()` 那道共用闸整条删了。**
    /// 它从前是四个入口（新建目录 · 改名 · 删除 · 权限）的共用住址，
    /// 干的事是「窗口看着本机 ⇒ 出声拒」（理由：这四条全要一条 SFTP 会话）。
    /// `Source` 收成 newtype 之后那个状态**写不出来** ⇒ 四处各少一句
    /// `if !self.remote_only() { return false; }`，那不是四道闸被删掉了，
    /// 是它们要守的那个状态整条不在了（逐条理由住 `super::source::Source` 头注）。
    pub fn begin_mkdir(&mut self) -> bool {
        self.write_prompt = Some(WritePrompt::for_mkdir(&self.cwd));
        true
    }

    /// 摆出「把第 `i` 行改名为」那个框。回值 = 真的摆出来了。
    ///
    /// ⚠ **两道闸，刻意重复**（同 [`Self::begin_copy`] 那一条逐字的理由）：
    /// `super::writeops::is_writable` —— 有损名一律不接。列表上那一档压根不画
    /// 那颗按钮，这里是第二道，防的是「按钮没了、调用还在」。
    pub fn begin_rename(&mut self, i: usize) -> bool {
        let Some(row) = self.writable_row(i) else {
            return false;
        };
        self.write_prompt = Some(WritePrompt::for_rename(&self.cwd, &row));
        true
    }

    /// 摆出「把第 `i` 行的权限改成」那个框。回值 = 真的摆出来了。
    pub fn begin_chmod(&mut self, i: usize) -> bool {
        self.begin_chmod_rows(&[i])
    }

    /// 〔FW5〕摆出「把这几行的权限改成」那个框（批量改权限；一行时与 [`Self::begin_chmod`] 同一个框）。
    /// 有一行不能写 ⇒ **整摞不摆**（同 [`Self::delete_picked`] 那条理由）。
    pub fn begin_chmod_rows(&mut self, idx: &[usize]) -> bool {
        let Some(rows) = idx
            .iter()
            .map(|&i| self.writable_row(i))
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        let refs: Vec<&super::source::Listed> = rows.iter().collect();
        self.write_prompt = Some(WritePrompt::for_chmod_many(&self.cwd, &refs));
        // 〔GP1 · 第四波〕框一摆出来就逐项问现值（`files-stat` 的 `mode`）；答回来之后框上说、只预填一次。
        let paths: Vec<String> = rows.iter().map(|r| r.path.clone()).collect();
        self.start_mode_probe(paths);
        !refs.is_empty()
    }

    /// 〔GP1〕起「现值」那一趟。没有运行时 / 没有通道 ⇒ 当场落「全读不到」（框上说「读不到现在的权限」，不猜、不静默）。
    fn start_mode_probe(&mut self, paths: Vec<String>) {
        let gen = self.mode_probe.start();
        let (Some(h), Some(line)) = (self.rt.clone(), self.line.clone()) else {
            self.mode_probe.land(gen, vec![None; paths.len()]);
            return;
        };
        let board = self.mode_probe.clone();
        let origin = self.source.origin();
        h.spawn(async move {
            let modes = super::writeops::probe_modes(&line, &origin, &paths).await;
            board.land(gen, modes);
        });
    }

    /// 🔴 **删除不经那个框** —— 它不向用户要任何输入，它要的是一次**确认**，
    /// 而确认那一步归 [`super::writeops::run_writes`] 的第二段（一次问完）。
    ///
    /// 回值 = 真的起了一摞（`false` = 这一行不能写 / 本机源 / 没有运行时，**且已出声**）。
    pub fn begin_delete(&mut self, i: usize, ctx: Option<egui::Context>) -> bool {
        let Some(row) = self.writable_row(i) else {
            return false;
        };
        self.start_writes(vec![super::writeops::delete_op(&row)], ctx)
    }

    /// 第 `i` 行，且它**能被写**。`None` ⇒ 不接（越界 / 有损名）。
    ///
    /// ⚠ 从前这句话是「越界 / 有损名 / **本机源**，已出声」—— 本机源那一档不在了
    /// （同 [`Self::begin_mkdir`] 那条）。它不是**静默**掉的：那个状态写不出来。
    fn writable_row(&mut self, i: usize) -> Option<super::source::Listed> {
        let rows = self.listing.rows.lock().unwrap();
        match rows.get(i) {
            // 〔FW5〕交出去的是**整个 `Listed`**：此前只交那五格（链接与时间写操作不读），
            //   现在写操作要读 `raw_name`（有损名的原始字节，改名 · 删除 · 改权限都走它）。
            Some(r) if is_writable(r) => Some(r.clone()),
            _ => None,
        }
    }

    /// 收掉那个框，什么都不做。
    pub fn cancel_write(&mut self) {
        self.write_prompt = None;
    }

    /// 框里那几个字 → 一摞真操作。回值 = 真的起来了。
    ///
    /// ⚠ 输入不合法 ⇒ **框留着、出声**，不静默收掉（同 [`Self::confirm_copy`]：
    /// 收掉的话用户点了确认什么都没发生，与成功长得一模一样）。
    pub fn confirm_write(&mut self, ctx: Option<egui::Context>) -> bool {
        let Some(p) = self.write_prompt.clone() else {
            return false;
        };
        // 〔FW5〕批量改权限那个框一次出 N 件 ⇒ `to_ops`（新建目录 / 改名恒一件）。
        let ops = match p.to_ops() {
            Ok(ops) => ops,
            Err(why) => {
                *self.listing.error.lock().unwrap() = Some(why);
                return false;
            }
        };
        if !self.start_writes(ops, ctx) {
            return false;
        }
        self.write_prompt = None;
        true
    }

    /// 起一摞 `§4.6.4`：**一次问完，才动手。**（〔FN1〕原来是「先过围栏，再……」，围栏 V119 拿掉了）
    ///
    /// 🔴 三段的顺序不在这里，在 [`super::writeops::run_writes`] 的结构里 ——
    /// 这里只负责把「怎么问 · 怎么做」两个口接上去（同 [`Self::start_drop`]）。
    /// 而本函数接不上（**没运行时**）要**出声**，判据见 `shell_tests`。
    pub fn start_writes(&mut self, ops: Vec<WriteOp>, ctx: Option<egui::Context>) -> bool {
        if ops.is_empty() {
            return false;
        }
        let Some(h) = self.rt.clone() else {
            *self.listing.error.lock().unwrap() =
                Some(copy_text("rsFilewinShell.writes.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string());
            return false;
        };
        let origin = self.source.origin();
        let board = self.write_board.clone();
        board.attach(ctx);
        self.writes_started += 1;
        h.spawn(async move {
            let ask_board = board.clone();
            let out = super::writeops::run_writes(
                ops,
                move |asking| {
                    let rx = ask_board.ask(asking);
                    async move { rx.await.unwrap_or_default() }
                },
                move |op| {
                    let line = line.clone();
                    let origin = origin.clone();
                    async move { super::writeops::apply_remote(&line, &origin, &op).await }
                },
            )
            .await;
            board.finish(out);
        });
        true
    }

    /// 一摞写操作跑完就重列当前目录（新目录要出现、删掉的要消失）。
    /// 同 [`Self::settle_finished_drops`]。
    pub fn settle_finished_writes(&mut self) -> bool {
        let now = self.write_board.rounds();
        if now == self.seen_write_rounds {
            return false;
        }
        self.seen_write_rounds = now;
        // 〔FW2〕删掉 / 改了名的那几个名字已经不在了 ⇒ 选中清掉（光标留着，理由住 `Selection::clear_picked`）。
        self.selection.clear_picked();
        self.reload();
        true
    }

    // ═══════════════════════════════════════════════════════════════════
    // 🔴〔第八刀〕往外拖：**两问，然后拉**
    // ═══════════════════════════════════════════════════════════════════

    /// 现在在问什么（`None` = 没在问）。判据与界面看同一个值。
    pub fn pull_ask(&self) -> Option<&super::download::Ask> {
        self.pull_ask.as_ref()
    }

    /// 第一问那个框里正在编辑的那几个字（`None` = 现在问的不是落点）。
    ///
    /// 🔴 **它是生产代码，不是测试钩子** —— [`Self::pull_ui`] 要一个 `&mut String`
    /// 去喂 `text_edit_singleline`，而那个 `&mut` 只能从这儿出来。
    ///
    /// ⚠ 一开始我给判据单写了一个 `#[cfg(test)]` 的句柄，**门禁当场拒了**：
    /// `structural_scan::the_split_stays_done_...` 里那条「`src/bridge/src` 的
    /// 测试专用支撑项」是一条**递减棘轮**（上限 15，逐字「不许把上限调上去让今天好过」）。
    /// ⇒ 换成这一个具名访问器之后，**界面与判据走的是同一条路**，而那比一个测试钩子更强：
    /// 判据改的那几个字，正是用户敲进去的那几个字。
    pub fn pull_dest_mut(&mut self) -> Option<&mut String> {
        match self.pull_ask.as_mut() {
            Some(super::download::Ask::Dest { text, .. }) => Some(text),
            _ => None,
        }
    }

    /// 摆出第一问（「存到哪儿」，缺省填 `<本机 home>/<原名>`）。回值 = 真的摆出来了。
    ///
    /// ⚠ **缺省值里那个「本机 home」不是本机文件管理器**（[`local_home`] 头注那条）：
    /// 往外拖就是往本机盘上写一份，落点当然在本机。用户裁的是「本地不需要**文件管理器**」。
    /// 🔴〔2026-09-23 本机侧退役〕开头那道「本机源出声拒」的闸删了 —— 同 [`Self::begin_copy`]。
    pub fn begin_pull(&mut self, i: usize) -> bool {
        let row = {
            let rows = self.listing.rows.lock().unwrap();
            match rows.get(i) {
                Some(r) if super::download::is_downloadable(r) => r.clone(),
                _ => return false,
            }
        };
        self.pull_ask = Some(super::download::Ask::for_row(&row));
        true
    }

    /// 收掉那个框，什么都不做。
    pub fn cancel_pull(&mut self) {
        self.pull_ask = None;
    }

    /// 答完当前这一问 → 下一步。回值 = **这一下真的推进了**。
    ///
    /// 🔴 三支各自的下一跳完全不同（理由住 `download::DestVerdict` 的头注）：
    /// 不合法 ⇒ **框留着 ＋ 出声**（收掉的话「点了确认什么都没发生」与成功同形）；
    /// 要确认 ⇒ 换成第二问；可以做 ⇒ 起那一趟并收掉框。
    pub fn confirm_pull(&mut self, ctx: Option<egui::Context>) -> bool {
        use super::download::{Ask, DestVerdict};
        let Some(ask) = self.pull_ask.clone() else {
            return false;
        };
        match ask {
            // 第二问答了「盖」⇒ 直接做（存在性已经问过，不再判一遍）。
            Ask::Overwrite { src_path, dest, .. } => {
                if !self.start_pull(&src_path, &dest, ctx) {
                    return false;
                }
                self.pull_ask = None;
                true
            }
            Ask::Dest { .. } => {
                match super::download::judge_dest(&ask, super::download::dest_exists) {
                    DestVerdict::Rejected(why) => {
                        *self.listing.error.lock().unwrap() = Some(why);
                        false
                    }
                    DestVerdict::NeedsOverwrite(next) => {
                        self.pull_ask = Some(next);
                        true
                    }
                    DestVerdict::Go { src_path, dest } => {
                        if !self.start_pull(&src_path, &dest, ctx) {
                            return false;
                        }
                        self.pull_ask = None;
                        true
                    }
                }
            }
        }
    }

    /// 真起一趟 —— 扔给 tokio，**不堵住 UI 线程**。
    ///
    /// 🔴 `transfer_id` 经 [`super::transfer::launch_unless_cancelled`] 造
    /// （那是池子取消登记表的唯一造键落点）⇒ 这一趟从此**取消得掉**，
    /// 与上传/复制两条路共用同一张在飞表。
    pub fn start_pull(&mut self, src_path: &str, dest: &str, ctx: Option<egui::Context>) -> bool {
        let Some(h) = self.rt.clone() else {
            *self.listing.error.lock().unwrap() =
                Some(copy_text("rsFilewinShell.pull.noRuntime", &[]).into());
            return false;
        };
        // 〔F7c〕下载经通道开单、订阅进度 —— 要那条线 ＋ 那台机器的地址。
        let Some(line) = self.line.clone() else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string());
            return false;
        };
        let origin = self.source.origin();
        let board = self.pull.clone();
        board.attach(ctx);
        let src = src_path.to_string();
        let to = dest.to_string();
        // ⚠ 走 `source::remote_basename`（**那一对**里的尾段那一份）——
        //   这里原先是第四份 `rsplit('/')`，而那一对的头注逐字说了
        //   「多一份就多一种『Windows 上 `\` 被当分隔符』的机会」。
        //   盘上每一处按 `/` 切的地方由
        //   `source_tests::every_place_that_splits_a_remote_path_is_declared` 逐条钉着。
        let name = super::source::remote_basename(&src).to_string();
        board.begin(&name);
        h.spawn(async move {
            let desk = board.cancels();
            let run = super::transfer::launch_unless_cancelled(&desk, &name, |_id| {
                let b = board.clone();
                let (line, origin) = (line.clone(), origin.clone());
                let src = src.clone();
                let to = to.clone();
                async move { super::download::pull_one(&line, &origin, &src, &to, &b).await }
            })
            .await;
            let (got, total) = board.seen();
            board.finish(match run {
                // ⚠ 字节数报**真读数**（进度那一格最后一个值），不报「应该是多少」。
                Ok(()) => super::download::Outcome::Done {
                    dest: to.clone(),
                    bytes: if got > 0 { got } else { total },
                },
                Err(why) => super::download::Outcome::Failed {
                    dest: to.clone(),
                    why,
                },
            });
        });
        true
    }

    // ═══════════════════════════════════════════════════════════════════
    // 🔴〔第九刀〕改一份远端文本
    // ═══════════════════════════════════════════════════════════════════

    /// 打开着的那一份（`None` = 没在编辑）。判据与界面看同一个值。
    pub fn editing(&self) -> Option<&super::editor::Pane> {
        self.editing.as_ref()
    }

    /// 编辑框里那些字（生产那个 `TextEdit` 要的 `&mut String` 从这儿出来）。
    ///
    /// ⚠ 同 [`Self::pull_dest_mut`]：**它是生产代码，不是测试钩子**
    /// （那条递减棘轮的来历逐字住那一处）。
    pub fn editing_text_mut(&mut self) -> Option<&mut String> {
        self.editing.as_mut().map(|p| &mut p.text)
    }

    /// 关窗那一问摆着吗。
    pub fn asking_discard(&self) -> bool {
        self.asking_discard
    }

    /// 起一趟「打开第 `i` 行」。回值 = **真的发出去了**。
    ///
    /// 🔴 改不了的那几档在这儿就答完了（[`super::editor::why_not_editable`]），
    /// **连那趟往返都不发** —— 而且把**为什么**说出来。
    /// 逐条理由住 `editor.rs` 头注「超了怎么办」那一节。
    pub fn begin_edit(&mut self, i: usize, ctx: Option<egui::Context>) -> bool {
        let row = {
            let rows = self.listing.rows.lock().unwrap();
            match rows.get(i) {
                // ⚠ 拿的是**那五格**（同 `writable_row`）：读一份文本要的是路径、
                //   名字与大小，链接与时间两格它一格都不读。
                Some(r) => r.row.clone(),
                None => return false,
            }
        };
        // 🔴 本地预判 —— 出声，不灰置。
        if let Some(why) = super::editor::why_not_editable(&row) {
            *self.listing.error.lock().unwrap() = Some(copy_text(
                "rsFilewinShell.edit.refused",
                &[("name", &row.name.to_string()), ("why", &why.to_string())],
            ));
            return false;
        }
        let Some(h) = self.rt.clone() else {
            *self.listing.error.lock().unwrap() =
                Some(copy_text("rsFilewinShell.edit.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string());
            return false;
        };
        let origin = self.source.origin();
        let board = self.edits.clone();
        board.attach(ctx);
        board.begin_open(&row.path);
        h.spawn(async move {
            use super::editor::Arrived;
            // 〔F7a〕读文本经通道问后端（`files-read-text`），不再拨 SFTP。
            let got = super::editor::read_text(&line, &origin, &row.path).await;
            board.deliver(match got {
                Ok(Some(text)) => Arrived::Text {
                    path: row.path.clone(),
                    name: row.name.clone(),
                    text,
                },
                Ok(None) => Arrived::NotText { path: row.path },
                Err(why) => Arrived::Failed {
                    path: row.path,
                    why,
                },
            });
        });
        true
    }

    /// 到货了就把编辑面立起来 / 把那句话摆出来。回值 = 这一帧真的消化了一趟。
    ///
    /// ⚠ 到货是**一次性事件**（`take_arrived`）—— 留着的话下一帧会再建一次编辑面，
    /// 把用户已经敲的东西盖掉。
    pub fn settle_opened_edits(&mut self) -> bool {
        use super::editor::Arrived;
        let Some(a) = self.edits.take_arrived() else {
            return false;
        };
        match a {
            Arrived::Text { path, name, text } => {
                self.editing = Some(super::editor::Pane::opened(&path, &name, text));
            }
            Arrived::NotText { path } => {
                *self.listing.error.lock().unwrap() = Some(super::editor::not_text_notice(&path));
            }
            Arrived::Failed { path, why } => {
                *self.listing.error.lock().unwrap() = Some(copy_text(
                    "rsFilewinShell.edit.readFailed",
                    &[("path", &path.to_string()), ("why", &why.to_string())],
                ));
            }
        }
        true
    }

    /// 存回去。回值 = **真的发出去了**。
    pub fn save_edit(&mut self, ctx: Option<egui::Context>) -> bool {
        let Some(p) = self.editing.clone() else {
            return false;
        };
        // 🔴 敲超上限 ⇒ 屏幕上先说，不发那趟注定被池子拒的往返。
        if p.over_cap() {
            *self.listing.error.lock().unwrap() = Some(copy_text(
                "rsFilewinShell.edit.tooBig",
                &[
                    ("n", &(p.text.len()).to_string()),
                    (
                        "limit",
                        &(super::rows::human_size(super::editor::MAX_EDIT_BYTES as u64))
                            .to_string(),
                    ),
                ],
            ));
            return false;
        }
        let Some(h) = self.rt.clone() else {
            *self.listing.error.lock().unwrap() =
                Some(copy_text("rsFilewinShell.save.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string());
            return false;
        };
        let origin = self.source.origin();
        let board = self.edits.clone();
        board.attach(ctx);
        board.begin_save(&p.path);
        h.spawn(async move {
            let r = super::editor::write_text(&line, &origin, &p.path, &p.text).await;
            board.deliver_save(r);
        });
        true
    }

    /// 存的结局到货了就落进那一份上。回值 = 这一帧真的消化了一趟。
    ///
    /// 🔴 失败时 **`text` 一个字都不碰**（那是用户唯一的一份）——
    /// 逐条理由住 `editor::Pane::mark_failed`。
    pub fn settle_saved_edits(&mut self) -> bool {
        let Some(r) = self.edits.take_saved() else {
            return false;
        };
        let Some(p) = self.editing.as_mut() else {
            return false;
        };
        match r {
            Ok(()) => p.mark_saved(),
            Err(why) => p.mark_failed(why),
        }
        true
    }

    /// 关掉编辑面这一下。回值 = **真的关掉了**（改了没存 ⇒ 先摆出那一问，回 `false`）。
    pub fn close_edit(&mut self) -> bool {
        use super::editor::Close;
        let Some(p) = self.editing.as_ref() else {
            return false;
        };
        match super::editor::judge_close(p) {
            Close::Now => {
                self.editing = None;
                self.asking_discard = false;
                true
            }
            Close::NeedsConfirm => {
                self.asking_discard = true;
                false
            }
        }
    }

    /// 那一问答了「丢掉」⇒ 真的关掉。
    pub fn discard_edit(&mut self) {
        self.editing = None;
        self.asking_discard = false;
    }

    /// 那一问答了「不丢」⇒ 收掉问，编辑面留着。
    pub fn keep_editing(&mut self) {
        self.asking_discard = false;
    }

    /// 🔴 **胶水一条**：列表说「第 `i` 行的编辑被点了」→ 窗口接上去。
    pub fn apply_edit_click(&mut self, ctx: Option<egui::Context>) -> bool {
        match self.tally.edit_clicked {
            Some(i) => self.begin_edit(i, ctx),
            None => false,
        }
    }

    /// 🔴 **胶水一条**：列表说「第 `i` 行的下载被点了」→ 窗口接上去。
    ///
    /// 抽成函数的理由与 [`Self::apply_click`] 逐字相同：两头各自都有判据，
    /// 而**中间这一跳写在 `frame_body` 里的话谁都没在看**。
    pub fn apply_pull_click(&mut self) -> bool {
        match self.tally.download_clicked {
            Some(i) => self.begin_pull(i),
            None => false,
        }
    }

    /// 🔴 **胶水三条**：列表说「第 `i` 行的改名 / 删除 / 权限被点了」→ 窗口接上去。
    ///
    /// 抽成函数的理由与 [`Self::apply_click`] / [`Self::apply_copy_click`] 逐字相同：
    /// 两头各自都有判据，而**中间这一跳写在 `frame_body` 里的话谁都没在看**
    /// （第一刀栽过的那一形）。
    ///
    /// 回值 = 这一帧真的接上了一跳。
    pub fn apply_write_clicks(&mut self, ctx: Option<egui::Context>) -> bool {
        if let Some(i) = self.tally.rename_clicked {
            return self.begin_rename(i);
        }
        if let Some(i) = self.tally.chmod_clicked {
            return self.begin_chmod(i);
        }
        if let Some(i) = self.tally.delete_clicked {
            return self.begin_delete(i, ctx);
        }
        false
    }

    /// 🔴〔第九刀〕画**编辑面**：在飞指示 ＋ 那一份文本 ＋ 存的结局 ＋ 关窗那一问。
    ///
    /// ⚠ 它是模态的（同别的几摞）：一份文本改着的时候不该同时去改目录结构。
    fn editor_ui(&mut self, ui: &mut egui::Ui) {
        // ── 在飞指示（读 / 存都要出声，否则「点了没反应」）──
        if let Some(p) = self.edits.opening() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(copy_text(
                    "rsFilewinShell.editor.reading",
                    &[("path", &p.to_string())],
                ));
            });
        }
        if let Some(p) = self.edits.saving() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(copy_text(
                    "rsFilewinShell.editor.saving",
                    &[("path", &p.to_string())],
                ));
            });
        }
        let Some(pane) = self.editing.clone() else {
            return;
        };
        // ── 关窗那一问（改了没存）。**它排在编辑面之前** —— 两个模态叠着时
        //    egui 画后一个，而这一问是更要紧的那一个。
        if self.asking_discard {
            let (mut discard, mut keep) = (false, false);
            egui::Modal::new(egui::Id::new("filewin-edit-discard")).show(ui.ctx(), |ui| {
                ui.heading(copy_text(
                    "rsFilewinShell.editor.unsaved",
                    &[("name", &pane.name.to_string())],
                ));
                ui.colored_label(
                    egui::Color32::from_rgb(0xFF, 0xA5, 0x00),
                    &copy_text("rsFilewinShell.editor.unsavedWarn", &[]),
                );
                ui.horizontal(|ui| {
                    if ui
                        .button(&copy_text("rsFilewinShell.editor.discard", &[]))
                        .clicked()
                    {
                        discard = true;
                    }
                    if ui
                        .button(&copy_text("rsFilewinShell.editor.keep", &[]))
                        .clicked()
                    {
                        keep = true;
                    }
                });
            });
            if discard {
                self.discard_edit();
            } else if keep {
                self.keep_editing();
            }
            return;
        }
        let (mut save, mut close) = (false, false);
        egui::Modal::new(egui::Id::new("filewin-editor")).show(ui.ctx(), |ui| {
            ui.heading(format!(
                "{}{}",
                pane.name,
                if pane.dirty() { " *" } else { "" }
            ));
            ui.label(&pane.path);
            // 🔴 敲超上限 ⇒ 这一行**一直**摆着（它是一个到你改掉为止都成立的状态）。
            if pane.over_cap() {
                ui.colored_label(
                    egui::Color32::RED,
                    copy_text(
                        "rsFilewinShell.editor.overLimit",
                        &[
                            (
                                "limit",
                                &(super::rows::human_size(super::editor::MAX_EDIT_BYTES as u64))
                                    .to_string(),
                            ),
                            ("n", &(-pane.headroom()).to_string()),
                        ],
                    ),
                );
            }
            if let Some(r) = pane.last_save.clone() {
                match r {
                    Ok(()) => ui.colored_label(
                        egui::Color32::from_rgb(0x3C, 0xB3, 0x71),
                        &copy_text("rsFilewinShell.editor.saved", &[]),
                    ),
                    // 原话原样画出去（围栏那句 / 连接失败那句 …）。
                    Err(why) => ui.colored_label(
                        egui::Color32::RED,
                        copy_text(
                            "rsFilewinShell.editor.saveFailed",
                            &[("why", &why.to_string())],
                        ),
                    ),
                };
            }
            super::bigfile::show(ui, self.editing.as_mut());
            ui.horizontal(|ui| {
                if ui
                    .button(&copy_text("rsFilewinShell.editor.save", &[]))
                    .clicked()
                {
                    save = true;
                }
                if ui
                    .button(&copy_text("rsFilewinShell.editor.close", &[]))
                    .clicked()
                {
                    close = true;
                }
            });
        });
        if save {
            let ctx = ui.ctx().clone();
            self.save_edit(Some(ctx));
        } else if close {
            self.close_edit();
        }
    }

    /// 🔴〔第八刀〕画**往外拖**那一摞：两问（模态）＋ 进度 ＋ 上一趟的结局。
    ///
    /// ⚠ 与 [`Self::write_ui`] 同一个结构，但两问的第二问**没有输入框** ——
    /// 它是一个是非题（「盖掉它？」），给一个框反而让用户以为还能改路径。
    fn pull_ui(&mut self, ui: &mut egui::Ui) {
        use super::download::{Ask, Outcome};
        // ── 进度：在飞的时候一直画着（`DownloadBoard` 会敲窗口，所以它会动）──
        if let Some(name) = self.pull.in_flight() {
            let (got, total) = self.pull.seen();
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(copy_text(
                    "rsFilewinShell.pull.progress",
                    &[
                        ("name", &name.to_string()),
                        ("got", &(super::rows::human_size(got)).to_string()),
                        ("total", &(super::rows::human_size(total)).to_string()),
                    ],
                ));
            });
        }
        // ── 上一趟的结局：**成功也出声** ──
        //    只在失败时说话的话，「拖完了」与「点了没反应」在屏幕上长得一样。
        if let Some(o) = self.pull.last() {
            match o {
                Outcome::Done { dest, bytes } => ui.colored_label(
                    egui::Color32::from_rgb(0x3C, 0xB3, 0x71),
                    copy_text(
                        "rsFilewinShell.pull.done",
                        &[
                            ("dest", &dest.to_string()),
                            ("size", &(super::rows::human_size(bytes)).to_string()),
                        ],
                    ),
                ),
                // 🔴 原话原样画出去（围栏那句、连接失败那句 …）——
                //    改写它就等于让用户看不到下层到底说了什么。
                Outcome::Failed { dest, why } => ui.colored_label(
                    egui::Color32::RED,
                    copy_text(
                        "rsFilewinShell.pull.failed",
                        &[("dest", &dest.to_string()), ("why", &why.to_string())],
                    ),
                ),
            };
        }
        // ── 那两问 ──
        let Some(ask) = self.pull_ask.clone() else {
            return;
        };
        let (mut go, mut cancel) = (false, false);
        egui::Modal::new(egui::Id::new("filewin-pull-prompt")).show(ui.ctx(), |ui| match ask {
            Ask::Dest { .. } => {
                ui.heading(copy_text(
                    "rsFilewinShell.pull.askWhere",
                    &[("name", &(ask.src_name()).to_string())],
                ));
                let Some(text) = self.pull_dest_mut() else {
                    return;
                };
                ui.text_edit_singleline(text);
                ui.label(&copy_text("rsFilewinShell.pull.dirHint", &[]));
                ui.horizontal(|ui| {
                    if ui
                        .button(&copy_text("rsFilewinShell.pull.ok", &[]))
                        .clicked()
                    {
                        go = true;
                    }
                    if ui
                        .button(&copy_text("rsFilewinShell.pull.cancel", &[]))
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            }
            Ask::Overwrite { ref dest, .. } => {
                ui.heading(&copy_text("rsFilewinShell.pull.exists", &[]));
                ui.label(format!("{dest}"));
                // 🔴 说清代价：`download_inner` 是 `.part` → `rename` 上位，
                //    原处那个文件没有备份、盖了就回不来。
                ui.colored_label(
                    egui::Color32::from_rgb(0xFF, 0xA5, 0x00),
                    &copy_text("rsFilewinShell.pull.overwriteWarn", &[]),
                );
                ui.horizontal(|ui| {
                    if ui
                        .button(&copy_text("rsFilewinShell.pull.overwrite", &[]))
                        .clicked()
                    {
                        go = true;
                    }
                    if ui
                        .button(&copy_text("rsFilewinShell.pull.cancel", &[]))
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            }
        });
        if cancel {
            self.cancel_pull();
        } else if go {
            let ctx = ui.ctx().clone();
            self.confirm_pull(Some(ctx));
        }
    }

    /// 画「叫什么名字 / 改成什么权限」那个框。**模态** —— 定下来之前不接别的。
    ///
    /// ⚠ 与 [`super::writeops::WriteBoard::ui`] 分开两处，因为它们的状态住在两个地方：
    /// 这个框是 UI 线程自己的，那一摞（确认 / 结果）是跨线程的。
    fn write_ui(&mut self, ui: &mut egui::Ui) {
        let Some(mut p) = self.write_prompt.clone() else {
            return;
        };
        let (mut go, mut cancel) = (false, false);
        // 〔GP1 · 第四波〕改权限那个框：现值答回来了 ⇒ 预填至多一次（`writeops::apply_prefill`）。
        let readout = matches!(p.kind, super::writeops::PromptKind::Chmod { .. }).then(|| {
            self.mode_probe.attach(ui.ctx().clone());
            self.mode_probe.readout()
        });
        if let Some(Some(r)) = &readout {
            super::writeops::apply_prefill(&mut p, r);
        }
        egui::Modal::new(egui::Id::new("filewin-write-prompt")).show(ui.ctx(), |ui| {
            ui.heading(p.heading());
            match &readout {
                Some(Some(r)) => {
                    ui.label(r.line.as_str());
                }
                Some(None) => {
                    ui.label(copy_text("rsFilewinShell.mode.reading", &[]));
                }
                None => {}
            }
            ui.text_edit_singleline(&mut p.text);
            ui.label(&copy_text("rsFilewinShell.write.sameDirOnly", &[]));
            ui.horizontal(|ui| {
                if ui
                    .button(&copy_text("rsFilewinShell.write.ok", &[]))
                    .clicked()
                {
                    go = true;
                }
                if ui
                    .button(&copy_text("rsFilewinShell.write.cancel", &[]))
                    .clicked()
                {
                    cancel = true;
                }
            });
        });
        self.write_prompt = Some(p);
        if cancel {
            self.cancel_write();
        } else if go {
            let ctx = ui.ctx().clone();
            self.confirm_write(Some(ctx));
        }
    }
}

// ════════════════════════════════════════════════════════════════════════
// 🔴〔FW1+FW2 2026-09-24〕键盘 · 多选 · 右键菜单 —— 接到窗口上的那几跳
// ════════════════════════════════════════════════════════════════════════
//
// 纯的那一份（选中态 · 键位翻译 · 「能做什么」那张表）住 `select.rs`；这里只有胶水，
// 而每一条胶水都是一个具名方法（同 `apply_click` 那条理由：写在 `frame_body` 里的话，
// 两头各有判据、中间这一跳谁都没在看）。

impl FileWindow {
    /// 选中态（判据与界面看同一个值）。
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// 〔FW34〕选中的**恰好那一项**叫什么；`Err(n)` ＝ 选中了 `n` 项（`n ≠ 1`）。
    /// O(1)：只问选中态，不扫列表（预览每帧都问它）。
    pub fn picked_name(&self) -> Result<String, usize> {
        match self.selection.len() {
            1 => self.selection.names().pop().ok_or(0),
            n => Err(n),
        }
    }

    /// 〔FW34〕按名字找那一行（O(n)：只在「选中的那一项换了」时调，不是每帧）。
    pub fn row_named(&self, name: &str) -> Option<Listed> {
        self.listing
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.name == name)
            .cloned()
    }

    /// 〔FW34〕这个目录视图**手上有没有活**（`None` ＝ 没有）—— 关标签页 / 收右栏之前问它。
    ///
    /// 有一问摆着、有一份文本开着、有东西在传 / 在复制 / 在写 / 在读写文本 ⇒ 说是哪一件。
    /// 后台那几趟任务不随标签页走，关掉就再也没人把结局摆给你看 ⇒ 这时候不许关。
    /// ⚠ 「新建空文件」那一趟不在里面（它借写操作的结果板、却不经 [`Self::start_writes`]，一趟不到一秒）。
    pub fn busy_reason(&self) -> Option<String> {
        if self.editing.is_some() {
            return Some(copy_text("rsFilewinShell.busy.editorOpen", &[]).into());
        }
        if self.modal_up() {
            return Some(copy_text("rsFilewinShell.busy.pendingAsk", &[]).into());
        }
        if let Some(p) = self.edits.opening().or_else(|| self.edits.saving()) {
            return Some(copy_text(
                "rsFilewinShell.busy.io",
                &[("path", &p.to_string())],
            ));
        }
        if let Some(n) = self.copy_board.running() {
            return Some(copy_text(
                "rsFilewinShell.busy.copying",
                &[("n", &n.to_string())],
            ));
        }
        if let Some(n) = self.pull.in_flight() {
            return Some(copy_text(
                "rsFilewinShell.busy.downloading",
                &[("n", &n.to_string())],
            ));
        }
        let up = self.board.cancels().in_flight_names();
        if !up.is_empty() {
            return Some(copy_text(
                "rsFilewinShell.busy.uploading",
                &[(
                    "names",
                    &(up.join(&copy_text("rsFilewinShell.busy.listSep", &[]))).to_string(),
                )],
            ));
        }
        if self.writes_started > self.write_board.rounds() {
            return Some(copy_text("rsFilewinShell.busy.writes", &[]).into());
        }
        None
    }

    /// 键盘 / 菜单那一下做不了时说的那句话。
    pub fn key_notice(&self) -> Option<&str> {
        self.key_notice.as_deref()
    }

    /// 摆着的那个右键菜单。
    pub fn menu(&self) -> Option<&MenuAt> {
        self.menu.as_ref()
    }

    /// 有一个模态框摆着吗（上传那一问 · 复制那两问 · 写操作那两问 · 新建空文件那个框〔F7b〕· 往外拖那两问 · 编辑面）。
    ///
    /// 🔴 **一处**：拖入那一口（[`Self::take_drops`]）与键盘那一口（[`Self::apply_keys`]）
    /// 问的是同一个函数 —— 分成两份的症状是「编辑面开着，按 Delete 删掉了列表里的文件」。
    fn modal_up(&self) -> bool {
        self.board.is_asking()
            || self.copy_board.is_asking()
            || self.copy_prompt.is_some()
            || self.write_board.is_asking()
            || self.write_prompt.is_some()
            || self.new_file.is_some()
            || self.pull_ask.is_some()
            // 〔F7c〕工具栏「上传」那一问（框开着时键盘不许动列表）。
            || self.upload.is_open()
            || self.editing.is_some()
    }

    /// 键盘这一帧该不该归列表。**五道闸**，任一成立就不接：
    ///
    /// 0. 〔FW34〕这个目录视图不是焦点那一个（双栏时另一栏、后台标签页）—— 不闸的话按一下 Delete 两栏各删一次；
    /// 1. 有模态框摆着（[`Self::modal_up`]）—— 键是给那个框的；
    /// 2. 右键菜单摆着 —— Esc / 点别处先把它收掉；
    /// 3. 画的是搜索命中那一摞 —— 那一摞交不出下标（`rows::HitTally` 头注那条），
    ///    按 Delete 删的会是**另一摞**里同一个下标的文件；
    /// 4. 有控件拿着键盘焦点（搜索框里正在打字、一颗按钮刚被 Tab 到）—— 字是给它的。
    ///    ⚠ 点一下列表里的行，焦点就交出去了（egui 点别处即交；行不可聚焦），键盘回到列表。
    pub fn keys_blocked(&self, ctx: &egui::Context) -> bool {
        !self.focused
            || self.modal_up()
            || self.menu.is_some()
            || self.showing_hits()
            || ctx.egui_wants_keyboard_input()
    }

    /// 🔴 **胶水**：这一帧 egui 收到的按键 → 意图 → 一件一件做。回值 = 做了几件。
    pub fn apply_keys(&mut self, ctx: &egui::Context) -> usize {
        if self.keys_blocked(ctx) {
            return 0;
        }
        let (events, now) = ctx.input(|i| (i.events.clone(), i.time));
        let its = select::intents(&events);
        let n = its.len();
        for it in its {
            self.apply_intent(it, now, Some(ctx.clone()));
        }
        n
    }

    /// 做一件意图。回值 = 真的做成了（做不了的那几形**已出声**，见 [`Self::key_notice`]）。
    ///
    /// `now` 是 egui 的输入时钟（秒），只给打字跳转那一格比「隔了多久」用。
    pub fn apply_intent(&mut self, it: Intent, now: f64, ctx: Option<egui::Context>) -> bool {
        self.key_notice = None;
        match it {
            Intent::Step { by, extend } => self.step(by, extend),
            Intent::Edge { end, extend } => {
                let len = self.listing.rows.lock().unwrap().len();
                self.go_to(select::edge_target(len, end), extend)
            }
            Intent::Parent => {
                let before = self.cwd.clone();
                self.navigate_up();
                self.cwd != before
            }
            Intent::SelectAll => {
                let rows = self.listing.rows.lock().unwrap();
                self.selection.select_all(&rows);
                !rows.is_empty()
            }
            Intent::Open => self.open_picked(ctx),
            Intent::Delete => self.perform(Action::Delete, ctx),
            Intent::Rename => self.perform(Action::Rename, ctx),
            Intent::Type(t) => {
                let prefix = self.type_ahead.feed(now, &t).to_string();
                let hit = {
                    let rows = self.listing.rows.lock().unwrap();
                    select::jump_target(&rows, &prefix)
                        .map(|i| (i, self.selection.move_to(&rows, i, false)))
                };
                match hit {
                    Some((i, _)) => {
                        self.key_scroll = Some(i);
                        true
                    }
                    None => {
                        self.key_notice = Some(copy_text(
                            "rsFilewinShell.intent.noPrefix",
                            &[("prefix", &prefix.to_string())],
                        ));
                        false
                    }
                }
            }
        }
    }

    /// ↑↓：光标挪一步，并记下「这一帧要滚进视野」。
    fn step(&mut self, by: isize, extend: bool) -> bool {
        let target = {
            let rows = self.listing.rows.lock().unwrap();
            select::step_target(rows.len(), self.selection.cursor_index(&rows), by)
        };
        self.go_to(target, extend)
    }

    /// 光标落到第 `target` 行（`None` = 空列表，什么都不做）。
    fn go_to(&mut self, target: Option<usize>, extend: bool) -> bool {
        if let Some(t) = target {
            let rows = self.listing.rows.lock().unwrap();
            self.selection.move_to(&rows, t, extend);
        }
        self.key_scroll = target;
        target.is_some()
    }

    /// 回车：选中的**恰好一项** ⇒ 目录进去、文件编辑。
    ///
    /// ⚠ 文件那一支**直接**走 [`Self::begin_edit`]，不先问 [`select::actions_for`]：
    /// 那张表对「超编辑上限」只会说「没有编辑这一项」，而 `begin_edit` 会说**为什么**
    /// （多了多少字节）—— 两者判的是同一个函数（`editor::is_editable` 就是
    /// `why_not_editable` 的 `is_none()`），所以准不准一致，只是这一支说得更具体。
    fn open_picked(&mut self, ctx: Option<egui::Context>) -> bool {
        let one = {
            let rows = self.listing.rows.lock().unwrap();
            match self.selection.picked_indices(&rows).as_slice() {
                [i] => Some((*i, rows[*i].is_dir)),
                _ => None,
            }
        };
        match one {
            Some((_, true)) => self.perform(Action::Open, ctx),
            Some((i, false)) => self.begin_edit(i, ctx),
            None => {
                self.key_notice = Some(select::refusal(Action::Open, self.selection.len()));
                false
            }
        }
    }

    /// 🔴 **对选中那几项做一件事 —— 菜单与键盘的唯一执行口。**
    ///
    /// 先**再问一次** [`select::actions_for`]（菜单是开菜单那一刻的快照，键盘压根没问过）：
    /// 不在表里 ⇒ 出声、不做。在表里 ⇒ 落回行上那几颗按钮**已经在走**的那几个 `begin_*`
    /// —— 一条新写路都没长（一次问完 · 只经通道说 `call` 全在那几个函数后面）。
    pub fn perform(&mut self, a: Action, ctx: Option<egui::Context>) -> bool {
        let (idx, allowed) = {
            let rows = self.listing.rows.lock().unwrap();
            let idx = self.selection.picked_indices(&rows);
            let picked: Vec<&super::source::Listed> = idx.iter().map(|&i| &rows[i]).collect();
            let allowed = select::actions_for(&picked);
            (idx, allowed)
        };
        if !allowed.contains(&a) {
            self.key_notice = Some(select::refusal(a, idx.len()));
            return false;
        }
        match (a, idx.as_slice()) {
            (Action::Delete, _) => self.delete_picked(&idx, ctx),
            (Action::Open, [i]) => self.activate(*i),
            (Action::Edit, [i]) => self.begin_edit(*i, ctx),
            (Action::Copy, [i]) => self.begin_copy(*i),
            (Action::Download, [i]) => self.begin_pull(*i),
            (Action::Rename, [i]) => self.begin_rename(*i),
            // 〔FW5〕一项或多项：同一个框（多项时框上说件数）。
            (Action::Chmod, _) => self.begin_chmod_rows(&idx),
            // `actions_for` 只对恰好一项给出单项动作 ⇒ 这一支走不到；
            // 真走到了也**出声**，不静默。
            _ => {
                self.key_notice = Some(select::refusal(a, idx.len()));
                false
            }
        }
    }

    /// 删掉第 `idx` 那几行 —— **一摞**，走 [`Self::start_writes`]（⇒ `run_writes`：
    /// 一次问完 → 串行做）。「批量底层已做好」说的就是那个函数。
    ///
    /// 🔴 每一行照旧过 [`Self::writable_row`] 那道第二闸；有一行过不去 ⇒ **整摞不起**并出声
    /// （起一摞「删 3 项」却悄悄只删 2 项，正是「选中态 == 批量那一摞」这条相等的反面）。
    fn delete_picked(&mut self, idx: &[usize], ctx: Option<egui::Context>) -> bool {
        let mut ops = Vec::with_capacity(idx.len());
        for &i in idx {
            let Some(row) = self.writable_row(i) else {
                self.key_notice = Some(select::refusal(Action::Delete, idx.len()));
                return false;
            };
            ops.push(super::writeops::delete_op(&row));
        }
        self.start_writes(ops, ctx)
    }

    /// 🔴 **胶水**：列表说「第 `i` 行被单击了（带着这几个修饰键）」→ 选中态跟着变。
    ///
    /// ⚠ 「点行这一下把键盘交回列表」**不在这儿做**：egui 自己在「点了别处」时让拿着焦点的
    /// 控件交出焦点（输入框与按钮都是；`SurrenderFocusOn::Clicks` 是缺省），而点一下可聚焦的
    /// 控件**并不**给它焦点（按钮要 Tab 过去才拿得到）⇒ 点完行之后没人拿着焦点，键盘自然归列表。
    /// 〔死值验现打〕第一版这里还有一句「显式清焦点」、行上还把 `Sense::click()` 换成了不可聚焦的那一档，
    /// 理由都是「点行会把焦点给行」—— **两刀各摘一次，一条判据都不红**，读 egui 源码核实那个前提不成立
    /// ⇒ 两处都撤回了。承重的是 egui 那条缺省行为，由
    /// `clicking_a_row_takes_the_keyboard_back_from_a_focused_button` 钉着（egui 换缺省那天它红）。
    pub fn apply_pick_click(&mut self) -> bool {
        let Some((i, mods)) = self.tally.picked_click else {
            return false;
        };
        {
            let rows = self.listing.rows.lock().unwrap();
            self.selection.click(&rows, i, mods);
        }
        self.key_notice = None;
        true
    }

    /// 🔴 **胶水**：列表说「第 `i` 行被右键点了」→ 选中态按右键的手感变 → 摆出菜单。
    ///
    /// `at` = 菜单摆在哪儿（指针那一刻的位置；判据直接喂）。
    pub fn apply_menu_click(&mut self, at: egui::Pos2) -> bool {
        let Some(i) = self.tally.menu_clicked else {
            return false;
        };
        if self.modal_up() {
            return false;
        }
        let (actions, n) = {
            let rows = self.listing.rows.lock().unwrap();
            self.selection.pick_for_menu(&rows, i);
            let idx = self.selection.picked_indices(&rows);
            let picked: Vec<&super::source::Listed> = idx.iter().map(|&i| &rows[i]).collect();
            (select::actions_for(&picked), idx.len())
        };
        self.key_notice = None;
        self.menu = Some(MenuAt {
            at,
            actions,
            n,
            serial: MENU_SERIAL.fetch_add(1, Ordering::SeqCst) + 1,
        });
        true
    }

    /// 菜单那一层的 egui id（判据按它找菜单画在哪一块）。
    pub fn menu_id(serial: u64) -> egui::Id {
        egui::Id::new(("filewin-row-menu", serial))
    }

    /// 画右键菜单；点了哪一项就交给 [`Self::perform`]。点别处 / Esc ⇒ 收掉。
    fn menu_ui(&mut self, ui: &mut egui::Ui) {
        let Some(m) = self.menu.clone() else {
            return;
        };
        let mut open = true;
        let mut chosen: Option<Action> = None;
        egui::Popup::new(
            Self::menu_id(m.serial),
            ui.ctx().clone(),
            egui::PopupAnchor::Position(m.at),
            ui.layer_id(),
        )
        .kind(egui::PopupKind::Menu)
        .open_bool(&mut open)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
        .show(|ui| {
            if m.actions.is_empty() {
                ui.label(MENU_EMPTY.as_str());
            }
            for a in &m.actions {
                if ui.button(a.label(m.n)).clicked() {
                    chosen = Some(*a);
                }
            }
        });
        if chosen.is_some() || !open {
            self.menu = None;
        }
        if let Some(a) = chosen {
            let ctx = ui.ctx().clone();
            self.perform(a, Some(ctx));
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
    /// 〔FW34〕那一句委派今天住 [`super::workspace::Workspace`]（它才是 `eframe::App`）：
    /// 每个标签页的正文就是这里，外面只多了标签栏 · 双栏 · 预览那一层。
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
        // 🔴〔2026-09-23 本机侧退役〕**这条工具栏上少了一对按钮，逐条记清。**
        //
        // 从前这里有一颗「本机」（远端态下画）与一颗「回 <机器名>」（本机态下画），
        // 以及两个收在帧尾的 `go_local` / `go_remote` 标志（按钮在 `ui.horizontal`
        // 的闭包里借着 `&mut self` 的一部分 ⇒ 跳转不能在闭包里做）。
        // 两颗按钮连同那一对函数都不在了 —— 用户裁决与那条白名单原文住
        // `super::source` 头注那块墓碑。
        // ⚠ **帧尾消化 `mkdir` 这一格照旧留着**：它与那两颗按钮是同一个借用理由，
        //   而「新建目录」那条功能一个字没动。别顺手把它也内联回闭包里。
        // 🔴〔补齐五项 2026-09-23〕**这个闭包里一个跳转都不做，三件事全收在帧尾。**
        //    理由与 `mkdir` 那一格逐字相同（上面那一节）：闭包借着 `&mut self` 的一部分
        //    ⇒ 在里面调 `self.navigate_to` / `self.set_sort` / `self.open_terminal_here`
        //    编不过。`⬆ 上一级` 与 `刷新` 两颗**例外**：它们调的那两个方法
        //    在这个闭包里借得出来（现状如此，别读成「跳转可以在闭包里做」）。
        let mut mkdir = false;
        let mut new_file = false; // 〔F7b〕同 `mkdir` 的借用理由，收在帧尾
        let mut go: Option<String> = None;
        let mut pick: Option<SortBy> = None;
        let mut term = false;
        ui.horizontal(|ui| {
            if ui
                .button(&copy_text("rsFilewinShell.frame.up", &[]))
                .clicked()
            {
                self.navigate_up();
            }
            if ui
                .button(&copy_text("rsFilewinShell.frame.refresh", &[]))
                .clicked()
            {
                self.reload();
            }
            // 🔴〔第五刀〕「新建目录」—— 它是四条写操作里**唯一**不针对某一行的那条
            //    （另外三条在行上），所以它的落点是工具栏。
            if ui.button(MKDIR_LABEL.as_str()).clicked() {
                mkdir = true;
            }
            // 〔F7b〕「新建空文件」—— 同样不针对某一行，所以同样在工具栏（逻辑住 `create.rs`）。
            if ui.button(super::create::NEW_FILE_LABEL.as_str()).clicked() {
                new_file = true;
            }
            // 〔F7c〕「上传」—— 选完走拖入那一条（`upload.rs` 头注）。
            if ui.button(super::upload::UPLOAD_LABEL.as_str()).clicked() {
                self.upload.open();
            }
            // 🔴〔补齐五项〕「在此打开终端」—— 旧面板表头上那颗。
            //    它在 POSIX 上恒定「失败」，而那是既定设计（逐条住 `open_terminal_here`）。
            if ui
                .button(&copy_text("rsFilewinShell.frame.terminal", &[]))
                .clicked()
            {
                term = true;
            }
            // 🔴〔补齐五项〕**排序那个下拉** —— 旧面板表头上那个 `<select>` 的对应物。
            //    ⚠ 人群走 `SortBy::ALL`，**不在这儿另写一份名单**：写第二份的症状是
            //      「加了一档但下拉里没有」，而那是编译器看不见的。
            egui::ComboBox::from_id_salt("filewin-sort")
                .selected_text(copy_text(
                    "rsFilewinShell.frame.sort",
                    &[("by", &self.sort_by.label())],
                ))
                .show_ui(ui, |ui| {
                    for by in SortBy::ALL {
                        // ⚠ 不直接 `&mut self.sort_by`：换档要**连手上这一摞一起重排**
                        //   （`set_sort`），而那件事在这个闭包里做不了 ⇒ 收在帧尾。
                        if ui
                            .selectable_label(self.sort_by == by, by.label())
                            .clicked()
                        {
                            pick = Some(by);
                        }
                    }
                });
            if self.listing.is_loading() {
                ui.spinner();
                ui.label(&copy_text("rsFilewinShell.frame.listing", &[]));
            }
            // 〔FW2〕选中了不止一项 ⇒ 说一声几项（一项时那块选中色自己就说清了）。
            //   ⚠ 摆在工具栏上而不是另起一行：另起一行会在选中第二项的那一下把整张列表往下推。
            let n = self.selection.len();
            if n > 1 {
                ui.label(copy_text(
                    "rsFilewinShell.frame.selected",
                    &[("n", &n.to_string())],
                ));
            }
        });
        // 🔴〔补齐五项〕**面包屑** —— 从 `/a/b/c/d` 回 `/a` 只要一下，不用点四次「上一级」。
        //    ⚠ 路径切分走 `source::breadcrumbs`（与 `parent_dir` / `remote_basename`
        //      同住一处，逐条理由住那个函数的头注）—— 这一行**不许自己切**。
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{} :", self.source.label()));
            for (seg, full) in breadcrumbs(&self.cwd) {
                // 当前这一级**不画成按钮**：点它什么都不会发生（`navigate_to` 同路径直接返回）
                // ⇒ 画成按钮就是一颗点了没反应的按钮。
                if full == self.cwd {
                    ui.strong(seg);
                } else if ui.small_button(seg).clicked() {
                    go = Some(full);
                }
            }
        });
        // 〔FW34〕书签栏（★ 切换当前目录 ＋ 一排书签）。点了哪一条也收在帧尾跳（同面包屑）。
        if let Some(shelf) = self.shelf.clone() {
            if let Some(d) = shelf.bar_ui(ui, &self.cwd) {
                go = Some(d);
            }
        }
        if mkdir {
            self.begin_mkdir();
        }
        if new_file {
            self.begin_new_file();
        }
        if let Some(by) = pick {
            self.set_sort(by);
        }
        if let Some(path) = go {
            self.navigate_to(path);
        }
        if term {
            let ctx = ui.ctx().clone();
            self.open_terminal_here(Some(ctx));
        }
        // 🔴〔FW1〕**键盘** —— 在画列表之前接：这一帧按的键，这一帧的列表就要画出结果
        //    （光标那一圈、滚进视野）。能不能接由 `keys_blocked` 那四道闸说了算。
        //    ⚠ 滚进视野要**上一帧**真物化的那一段 ⇒ 在 `tally` 被清零之前取。
        let (prev_first, prev_last) = (self.tally.first_row, self.tally.last_row);
        {
            let ctx = ui.ctx().clone();
            self.apply_keys(&ctx);
        }
        if let Some(said) = self.key_notice.clone() {
            ui.colored_label(egui::Color32::from_rgb(0xFF, 0xA5, 0x00), said);
        }
        // 🔴〔补齐五项〕开终端那一下说的话 —— **摆着不走**（同字体那条：
        //    它是一个「到你换台机器 / 换个系统为止都成立的状态」，不是一次性事件）。
        if let Some(said) = self.term_notice() {
            ui.colored_label(egui::Color32::from_rgb(0xFF, 0xA5, 0x00), said);
        }
        if let Some(e) = self.listing.error.lock().unwrap().clone() {
            ui.colored_label(egui::Color32::RED, e);
        }
        // 〔F2〕这里原先画「这一屏没走后端：…」（退路那一句）。退路没了，那一句也没了。
        // 🔴 截断也要出声 —— 「这个目录里就这么多」与「后端只给了前 N 条」
        //    在屏幕上长得一样，而用户会据此以为某个文件不存在。
        if self.listing.truncated.load(Ordering::SeqCst) {
            ui.colored_label(
                egui::Color32::from_rgb(0xFF, 0xA5, 0x00),
                copy_text(
                    "rsFilewinShell.frame.truncated",
                    &[("n", &(super::source::LS_LIMIT).to_string())],
                ),
            );
        }
        // 🔴〔第四刀〕搜索那一行 ＋ **新鲜度那一行**（`设计/60 §3.5.3` 那条 ⬜）。
        self.search_row(ui);
        // `§5.4d` 那一摞：确认框 ／ 进度。**画在列表之前** —— 它是模态的。
        self.board.ui(ui);
        // `§5` 第二段那一摞：覆盖确认 ／ 进度 ／ **上一趟走的是哪条路**。同样模态、同样在前。
        self.copy_board.ui(ui);
        self.copy_ui(ui);
        // 🔴〔第五刀〕`§4.6.4` 那一摞：一次问完的确认框 ／ 结果（〔FN1〕「被围栏挡住那几句话」那一段删了）。
        //    同样模态、同样画在列表之前。
        self.write_board.ui(ui);
        self.write_ui(ui);
        // 〔F7b〕新建空文件那个框（同样模态、同样在前）。
        self.new_file_ui(ui);
        // 🔴〔第八刀〕往外拖那一摞：两问 ／ 进度 ／ 结局。同样模态、同样在前。
        self.pull_ui(ui);
        // 〔F7c〕「上传」那一问：确定之后走拖入那一条（先一次问完覆盖，再并行传）。
        let up_dir = self.cwd.clone();
        if let Some(items) = self.upload.ui(ui, &up_dir) {
            let ctx = ui.ctx().clone();
            self.start_drop(items, Some(ctx));
        }
        // 🔴〔第九刀〕编辑那一摞：**先消化到货，再画** ——
        //    反了的话这一帧画的是上一帧的状态（读完了却还显示「正在读」）。
        self.settle_opened_edits();
        self.settle_saved_edits();
        self.editor_ui(ui);
        let ctx = ui.ctx().clone();
        self.take_drops(&ctx);
        self.settle_finished_drops();
        self.settle_finished_copies();
        self.settle_finished_writes();
        ui.separator();
        // 每帧从零数起 —— 这两个数是「这一帧物化了多少行」，不是累计。
        self.tally = RenderTally::default();
        self.hits_tally = HitTally::default();
        // 🔴〔第四刀〕搜索框里有字 ⇒ 画命中，否则画当前目录。**二选一，不并排** ——
        //    并排会让「你现在看的是哪一摞」变成一个要靠标题猜的问题。
        if self.showing_hits() {
            let hits: Vec<String> = self
                .search
                .shown()
                .outcome
                .map(|o| o.hits.iter().map(|h| h.display()).collect())
                .unwrap_or_default();
            // 🔴〔第五刀〕收数口是 [`HitTally`]，**不是** `self.tally` ——
            //    于是命中那一摞**在类型上**交不出任何一个下标，而下面那三条胶水
            //    索引的是 `listing.rows`（另一摞东西）。逐条理由住那个类型的头注。
            show_hit_rows(ui, &hits, &mut self.hits_tally);
        } else {
            // 🔴〔第十刀〕reveal 的两半在这里落地：**算**出偏移（只算一次）＋ 高亮那个名字。
            //    ⚠ 偏移是算的不是找的 —— `设计/60 §4 戊` 那条纪律（`show_rows` 才是主语）。
            // ⚠ **先问 reveal（它自己拿锁），再拿锁画** —— 顺序反了就要克隆整摞行。
            let pitch = super::rows::row_pitch(ui);
            let jump = match self.take_reveal_offset(pitch) {
                Some(Ok(y)) => Some(y),
                Some(Err(why)) => {
                    *self.listing.error.lock().unwrap() = Some(why);
                    None
                }
                None => None,
            };
            // 〔FW1〕键盘挪了光标 ⇒ 不在视野里才滚（reveal 那一下优先：它也是「只滚一次」）。
            let key_jump = self
                .key_scroll
                .take()
                .and_then(|i| select::scroll_for(i, prev_first, prev_last, pitch));
            let jump = jump.or(key_jump);
            let want = self.reveal.as_ref().map(|r| r.name.clone());
            let rows = self.listing.rows.lock().unwrap();
            show_file_rows(
                ui,
                &rows,
                &mut self.tally,
                jump,
                want.as_deref(),
                Some(&self.selection),
            );
        }
        // ⚠ 这三条只对**目录列表**那一摞有意义（下标索引的是 `listing.rows`）。
        //   命中那一摞交不出下标 —— 第四刀靠的是「那个函数不画可点控件」这条纪律，
        //   第五刀换成了**类型**（上面那一段）。
        self.apply_click();
        self.apply_copy_click();
        self.apply_write_clicks(Some(ctx.clone()));
        // 🔴〔第八刀〕第四条胶水。**不许写在这行之外** —— 理由住 `apply_pull_click`。
        self.apply_pull_click();
        // 🔴〔第九刀〕第五条胶水。
        self.apply_edit_click(Some(ctx.clone()));
        // 🔴〔FW2〕第六、七条胶水：单击改选中 · 右键摆菜单。然后画菜单（它在最上层）。
        self.apply_pick_click();
        let at = ctx.input(|i| i.pointer.interact_pos()).unwrap_or_default();
        self.apply_menu_click(at);
        self.menu_ui(ui);
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
            ui.label(&copy_text("rsFilewinShell.search.label", &[]));
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .desired_width(220.0)
                    .hint_text(&copy_text("rsFilewinShell.search.hint", &[])),
            );
            if r.changed() {
                fire = true;
            }
            if ui
                .button(&copy_text("rsFilewinShell.search.rebuild", &[]))
                .clicked()
            {
                rebuild = true;
            }
            if self.search.is_running() {
                ui.spinner();
                ui.label(&copy_text("rsFilewinShell.search.running", &[]));
            }
        });
        self.search.ui(ui);
        if fire || rebuild {
            let ctx = ui.ctx().clone();
            self.fire_search(Some(ctx), rebuild);
        }
    }
}

/// 那件事**是不是当场就失败了** —— 是（＝预算内就结束了）回 `true`。
///
/// # 🔴 它补的是一个**静默成功**（本仓头号病形）
///
/// 入口那条命令此前把开窗的句柄丢掉（`let _ = …`）⇒ 无论窗口起没起来，
/// webview 那侧看到的都是 `Ok(行数)`。而**有一条路当时必然走到那里**：
/// winit 全进程只许建一个事件循环 ⇒ 同一个进程里第二次开窗必然失败
/// ⇒ 用户把窗口关掉之后再点一次，屏幕上什么都没有，**而且界面说「成功」**。
///
/// 🔴〔第十三刀 2026-09-23〕**那条必然失败的路已经不存在了** —— 开窗改成起一个
/// 独立进程（住 [`super::proc`]），第二趟是一个新进程、一个新事件循环。
/// 本函数**没有随之退役**，因为它买的那件事一个字没变：**「当场就死了」不许被报成成功。**
/// 今天它的生产调用方是 [`super::proc::open_in_new_process`]（判的是「那个**进程**
/// 是不是在开窗预算内就退了」），判据那一侧还照旧喂线程。
///
/// # 🔴 为什么签名从「一个线程句柄」换成一个闭包
///
/// 被判的东西从**线程**变成了**进程**，而这条轮询在全仓只许有一处
/// （它是 `rust_timer_registry` 里登记的那个 `wait-for-condition`，
/// 抄第二份就等于多一个没人登记的节拍）。⇒ 把「结束了没有」这一问抽成入参，
/// 线程那一侧问 `is_finished()`、进程那一侧问 `try_wait()`，**轮询只此一份**。
///
/// # 为什么「等一小会儿」这个做法在这里是够的
///
/// 它分得开的是**早失败**与**起来了**：起不来那一形在毫秒级就结束
/// （二进制不对 · 没有图形会话 · 种子解不出），而**起来了**的那一条会一直
/// 占着那条线程／那个进程直到窗口关闭 ⇒ 预算内必然「还在跑」。
///
/// ⚠ **它买不到「窗口真的出现在屏幕上」** —— 那要一个图形会话（`super` 头注）。
/// 它买到的是：**「那条线程当场就死了」不再被报成成功。**
/// ⚠ 也买不到「慢失败」：真起了循环之后才炸的那一形，预算内看不见，
/// 照旧只落在那条线程的 stderr 上。如实登记，别读宽。
///
/// ⚠ 预算刻意**短**：它是加在用户那一次点击上的延迟。
/// 300ms 是「人感觉不到」与「毫秒级失败一定抓得到」之间的取值。
/// ⚠ 刻意**不**注入一个假时钟：`is_finished()` 与 `sleep` 照样走真时钟，
/// 注进来的那个只会让签名多一格而买不到任何东西
/// （第一版写了它，复看时删掉 —— 注入只在它真能换掉一条 IO 时才值得，
///  同 `download::judge_dest` 那一处是真换掉了「碰盘」）。
pub fn early_failure(mut finished: impl FnMut() -> bool, budget: std::time::Duration) -> bool {
    let t0 = std::time::Instant::now();
    while t0.elapsed() < budget {
        if finished() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

/// 入口那条命令用的预算。**独立常量**，因为判据要按它喂合成输入。
pub const EARLY_FAILURE_BUDGET: std::time::Duration = std::time::Duration::from_millis(300);

/// 在**次线程**上开一个文件管理窗口。立刻返回，不阻塞调用方。
///
/// ⚠ `eframe::run_native` 在它自己那条线程上是**阻塞到窗口关闭**的；
/// 这里把它整个丢进 `std::thread::spawn` ⇒ 对调用方是非阻塞的。
///
/// 🔴〔第十三刀〕**这两个函数今天跑在窗口进程里，不在 monitor 里** ——
/// 调用方是 [`super::proc::child_main`]。它们**不带种子**的这一支（本函数）
/// 生产上没人走：入口那条命令恒是先列一趟再把那一屏交出去。
pub fn open_detached(
    source: Source,
    cwd: String,
    rt: Option<tokio::runtime::Handle>,
) -> std::thread::JoinHandle<Result<(), String>> {
    open_detached_seeded(source, cwd, rt, None, Vec::new(), None, None)
}

/// 同 [`open_detached`]，但**带着已经列好的那一屏**开窗。
///
/// 🔴 用它而不是 `open_detached` 的理由住 [`FileWindow::seeded`]：
/// 入口那条命令为了能在 webview 那侧出声，已经列过一趟了，别再打第二次往返。
/// 🔴〔第十三刀〕**为什么窗口进程里还是「起一条次线程」而不是直接占 `main`**
///
/// 两条，都不是省事：
/// ① `with_any_thread(true)` 那条路是这个窗口**唯一被实地量过**的形态
///    （`真相源/99 §八` 那四趟读数、Xvfb 台架那一摞都是在它上面打的）。
///    换成「占 `main` 线程」是换一个**没有读数**的配置，而换了被测对象就要重打读数。
/// ② 那个 hook 里还挂着 Windows 的进程 DPI 归属那一句（〔WN1〕今天是 `with_dpi_aware(builder, true)`），
///    它有自己的判据。绕开 hook 就是把那一句一起绕开。
/// ⇒ 保持不动：窗口进程的 `main` 只负责读种子、起运行时、`join` 这条线程。
pub fn open_detached_seeded(
    source: Source,
    cwd: String,
    rt: Option<tokio::runtime::Handle>,
    // 🔴〔F2〕通道（`None` = 判据那一形：不连后端）。
    line: Option<Line>,
    rows: Vec<Listed>,
    // 🔴〔第十刀〕`reveal` = 开窗就高亮这一行（`None` = 不高亮）。
    //    那是老面板 `open(revealPath)` 那一形（会话工具卡 → 文件跳转）。
    reveal: Option<String>,
    // 〔FW34〕书签文件（monitor 算好交过来；`None` ＝ 数据目录解不出来，书签栏上出声）。
    bookmarks: Option<std::path::PathBuf>,
) -> std::thread::JoinHandle<Result<(), String>> {
    OPEN_REQUESTED.fetch_add(1, Ordering::SeqCst);
    std::thread::spawn(move || {
        let title = copy_text(
            "rsFilewinShell.window.title",
            &[("source", &(source.label()).to_string())],
        );
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
                if let Some(line) = line {
                    w.attach_line(line);
                }
                // 🔴〔第十刀〕开窗就高亮那一行。**在这里设而不是进 `seeded` 的签名** ——
                //    `seeded` 有 5 处调用点（判据 4 处），而 reveal 只有开窗那一条路用得上。
                if let Some(name) = reveal {
                    w.set_reveal(&name);
                }
                // 〔FW34〕书签：按这台机器的 origin 读一次。
                w.shelf = Some(super::bookmarks::Shelf::open(bookmarks, &w.source.origin()));
                // 第一拍：读文件 ＋ `set_fonts`。**这里复核不了**（`fonts.rs §四`）。
                w.font = FontState::Pending(fonts::install(&cc.egui_ctx));
                // 〔FW34〕最外一层是 `Workspace`（标签页 ＋ 双栏 ＋ 预览），开窗那一个目录视图是它的第一个标签页。
                Ok(Box::new(super::workspace::Workspace::new(w)) as Box<dyn eframe::App>)
            }),
        )
        .map_err(|e| copy_text("rsFilewinShell.window.openFailed", &[("e", &e.to_string())]))
    })
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/shell_tests.rs"]
mod tests;

// 〔FW1+FW2〕键盘 · 多选 · 右键菜单接到窗口上的那一摞（每一条都真跑 `frame_body`）。
#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/shell_keys_tests.rs"]
mod keys_tests;
