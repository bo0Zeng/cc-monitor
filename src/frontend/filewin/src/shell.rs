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
//! 把「进程 DPI 归谁管」明确判给 Tauri。〔09-24：窗口进程独立之后翻回 `true`，见下一节。〕
//!
//! ⚠ **这一格虚拟机够用，可以说「验过了」** —— 它问的是**调用顺序与全局状态归属**，
//! 是逻辑题，不是显卡题。⚠ 但**别把它读宽**：记着
//! KVM 虚拟机不是物理机 ⇒ **「真机上 DPI 缩放长什么样」那一格仍然没有读数**
//! （那台机器 DPI 缩放 100%、单显示器、QXL 虚拟显卡）。
//!
//! 出事的形状仍然照记：哪天 egui 窗口比 Tauri 主窗先建（或 winit 换了默认值），
//! WebView2 那侧的 DPI 行为会跟着变 —— 而本仓在 Windows DPI/WebView 上
//! 已经吃过亏（那一族、以及 `F12 nudge` 那处 WebView2 bounds 修正）。
//! 这一改正是把那个形状从「靠顺序碰巧对上」变成「只有一个人设它」。
//!
//! ## 🔴 上面那个处置的**前提没了**，开关翻回 `true`
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
//! # ⚠ 没做到的，写在这儿而不是藏着
//!
//! - **本机跑不了真窗口**（`XDG_SESSION_TYPE=tty`，无图形会话）⇒ [`open_detached`]
//!   这条路在本机**没有端到端读数**；有的是 scratchpad 那个 `tao ＋ eframe` 原型的
//!   三趟读数（见 `super` 头注）与编译期的三档 `cargo check`。
//!   ⚠ 第二刀在这条上多欠一句：**鼠标真的双击一下那一行会怎样，本机判不了**
//!   （没有图形会话就没有真事件源）。判据喂的是**合成事件**，
//!   它买的是「egui 收到这串事件之后认出来的是哪一行」——
//!   逐条与那次现打的读数写在 [`super::rows`] 的 `paint_one_row` 头注里。
//! - **Windows 上一次都没跑过。**
//! - **预览 · 双栏 · 标签页做了**，住 [`super::workspace`]（最外一层）与
//!   [`super::preview`]；本文件的 [`FileWindow`] 照旧是「一个目录视图」，一个标签页就是一个它。
//!   〔此前这一格写的是「预览 · 双栏仍然一个都没有」（ 代价第 2 条）。〕
//! **多选与右键菜单做了**（还有键盘）：纯的那一份（选中态 · 键位 ·
//!   「能做什么」那张表）住 [`super::select`]，接到窗口上的那几跳住本文件
//!   [`FileWindow::apply_keys`] / [`FileWindow::apply_pick_click`] /
//!   [`FileWindow::apply_menu_click`] / [`FileWindow::perform`]。
//!   🔴 **写操作一条新路都没长**：键盘与菜单做的每一件，都落回行上那几颗按钮已经在走的
//!   那几个 `begin_*`（以及删除那一摞的 `start_writes`）⇒ 一次问完 · 只经通道说 `call`
//!   这几道闸一道都没绕开。
//!   ⚠ **拖放从这一条里划出去了**：第二刀做了「拖入本机文件 → 上传到当前远端目录」
//!   那一半（见 [`FileWindow::start_drop`] 与 [`super::transfer`]）；
//!   往外拖（下载）做了（[`super::download`]）；窗口之间互拖没做。
//! - （大文件编辑改流式）**仍然没做** —— 这一刀没有编辑面。
//! - （拖入多文件先一次问完再并行）**第二刀做了**，
//!   住 [`super::transfer::run_drop`]；三段的顺序就是那个函数的结构，判据钉的是顺序与并行度。

use copy_core::copy_text;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::copy::{CopyBoard, CopyJob, CopyPrompt};
use super::find::{self, SearchBoard};
use super::fonts::{self, FontState};
use super::grep::{self, GrepBoard, GrepTally};
use super::rows::{show_hit_rows, HitTally, RenderTally};
use super::select::{self, Action, Intent, Selection, TypeAhead};
use super::source::{Line, Listed, Sort, Source};
use super::transfer::{DropBoard, Pending};
use super::writeops::{is_writable, PromptKind, WriteBoard, WriteOp, WritePrompt};

/// 接上通道时问那台能力事实的期限（一问一答，同读侧那几问的量级）。
const OFFER_WITHIN: std::time::Duration = std::time::Duration::from_secs(10);
/// 「在此打开终端」那一问的期限（monitor 接下它：问本机后端渲那一行 ＋ 开窗，都在本机）：给足握手余量即可。
const TERMINAL_WITHIN: std::time::Duration = std::time::Duration::from_secs(10);

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

/// 开过几次右键菜单 —— 每次开菜单换一个 egui id（理由住 [`MenuAt::serial`]）。
///
/// 🔴**进程级，不是每个目录视图一个**：双栏时两栏同时画在一个 egui 上下文里，
/// 各数各的话两栏的第一个菜单撞同一个 id ⇒ 在另一栏右键那一下「只关不开」（与序号本来要防的同一形）。
static MENU_SERIAL: AtomicU64 = AtomicU64::new(0);

/// 最近一个窗口在**第一帧**上复核字体的结果。`None` = 还没有任何窗口复核过。
///
/// 🔴 判据拿它买的是「装字体这一步**真的接在开窗那条路上**」——
/// **扫源码买不到这个**（本仓 09-20 栽过一次：源码扫描那条与行为那条买的不是同一样东西，
/// 前者看不见「按钮接没接到方法上」）。⇒ 由 Xvfb 那条真开窗的判据读它。
static FONT_VERDICT: Mutex<Option<FontState>> = Mutex::new(None);

/// 就地那一格那一趟的落点（UI 线程读，tokio 那条写）。
type InlineDone = Arc<Mutex<Option<(u64, Result<String, super::source::Failed>)>>>;

pub fn font_verdict() -> Option<FontState> {
    FONT_VERDICT.lock().unwrap().clone()
}

pub fn windows_opened() -> u64 {
    WINDOWS_OPENED.load(Ordering::SeqCst)
}

pub fn open_requested() -> u64 {
    OPEN_REQUESTED.load(Ordering::SeqCst)
}

// `any_thread_hook`（winit 事件循环建在次线程上 · 本进程自己管 DPI，三个平台各一句）住 `platform.rs`。

/// **「存到哪儿」那一问的缺省落点** —— 用户的 home。
///
/// 🔴〔2026-09-23 本机侧退役〕**它换了唯一消费者，如实记**：从前它是工具栏上
/// 那颗「本机」按钮的落脚点（那颗按钮连同整个本机侧已经不在了，见
/// `source.rs` 头注那块墓碑）；今天它只有一个消费者 ——
/// [`super::download::default_dest`]（往外拖时「存到哪儿」那一格的缺省值）。
/// ⇒ 它**不再是文件管理器的一部分**，是**往外传**那条路上的一格。
///
/// 〔P5 收家目录〕家目录取法走全仓那一条规矩（`creds_core::store::home_dir`，monitor 与后端同一家），不再自己调 `dirs`。
///
/// ⚠ 它**只把 home 当一条缺省路径**，不去读 home 里的任何东西 —— 零次 `read_dir`。
/// 拿不到 home 就退到 `.`（当前工作目录），**不猜一个路径出来**。
pub fn local_home() -> String {
    creds_core::store::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string())
}

/// 没连上通道时，每一件要问后端的事说的那一句（`D11`：不退回 SFTP、不静默）。
pub static NO_LINE: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinShell.noLine.message", &[]));

// 「在此打开终端」那一串（`build_open_terminal_cmd`〔散文墓碑〕· `build_open_terminal_cmd_at`〔散文墓碑〕· `cd_then_shell`〔散文墓碑〕）
//   随「窗口只交意图」搬进本机后端：`src/backend/dial/terminal.rs::command_for_cwd`（期望串原样搬去 `tests/backend/dial_terminal_tests.rs`）。

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
    // 这里原先还有一格「上一趟走没走成主路」（退路那一句话的落点）。
    // 退路整条拿掉之后（`D11`，理由住 `source.rs` 那一节）那一格没有可说的了，一起摘掉。
    /// 🔴 上一趟**被后端截断了吗**。
    ///
    /// 它必须画出来：「这个目录里就这么多」与「后端只给了前 N 条」
    /// 在屏幕上长得一样，而用户会据此以为某个文件不存在。
    pub truncated: Arc<std::sync::atomic::AtomicBool>,
    /// 隐藏文件切成不显示时，收起来的那几行（不在 [`Self::rows`] 里，下标不受它们影响）。
    pub hidden: Arc<Mutex<Vec<Listed>>>,
    /// 隐藏文件显示着吗（缺省 `true`）。一屏落地时按它分。
    pub show_hidden: Arc<std::sync::atomic::AtomicBool>,
    /// 此刻选的那一档排序（[`FileWindow::set_sort`] 先改它再重排）：一屏落地时出发那一档与它不同 ⇒ 按它重排。
    pub sort: Arc<Mutex<Sort>>,
    /// 上一趟有几项读不出来（后端照数，没列出）。
    pub unreadable: Arc<AtomicU64>,
    /// 上一趟目录里一共读到几项（截断时那一句要它）。
    pub total: Arc<AtomicU64>,
    /// 上一趟目录打不开：哪一种（后端的码）＋ 系统原话（进「复制详情」）。
    pub open_fail: Arc<Mutex<Option<(super::source::OpenFail, String)>>>,
    /// 摆着的这一屏是什么时候列到的（UNIX 秒；0 ＝ 还没列到过）。断线条「离线 · 采样 13:40」那个时刻。
    pub landed: Arc<AtomicU64>,
}

impl Default for Listing {
    fn default() -> Self {
        Self {
            rows: Arc::new(Mutex::new(Vec::new())),
            error: Arc::new(Mutex::new(None)),
            epoch: Arc::new(AtomicU64::new(0)),
            inflight: Arc::new(AtomicU64::new(0)),
            truncated: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            hidden: Arc::new(Mutex::new(Vec::new())),
            show_hidden: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            sort: Arc::new(Mutex::new(Sort::default())),
            unreadable: Arc::new(AtomicU64::new(0)),
            total: Arc::new(AtomicU64::new(0)),
            open_fail: Arc::new(Mutex::new(None)),
            landed: Arc::new(AtomicU64::new(0)),
        }
    }
}

/// 落一趟**经后端**的列目录（比 [`store_if_current`] 多一格：截断）。
///
/// 🔴 与它**刻意是两个函数**：没问成的那几形（无运行时 / 无通道）交不出这一格，
/// 而给它编一个「不知道」的第三态，只会让「没问过」与「问了没被截断」混在一起。
/// `by` ＝ 出发时那一档排序：落地时用户已经换了档 ⇒ 按现在那一档重排（表头指着哪一列，行就按哪一列）。
pub fn store_listed_if_current(
    l: &Listing,
    mine: u64,
    r: Result<(Vec<Listed>, super::source::Cut), String>,
    by: Sort,
) -> bool {
    match r {
        Ok((rows, cut)) => {
            // ⚠ 落行那一步会判号并可能整份丢掉，而截断那一格描述的是**同一趟**，
            //   不许一半落一半不落 ⇒ 只在真落了之后写它。
            let kept = store_if_current(l, mine, Ok(rows));
            if kept {
                l.truncated.store(cut.truncated, Ordering::SeqCst);
                l.unreadable.store(cut.unreadable, Ordering::SeqCst);
                l.total.store(cut.total, Ordering::SeqCst);
                *l.open_fail.lock().unwrap() = None;
                // 落地之后才读那一档：在这之前换的档由这里补排，在这之后换的由 `set_sort` 就地排。
                let now = *l.sort.lock().unwrap();
                if now != by {
                    super::source::sort_rows(&mut l.rows.lock().unwrap(), now);
                    super::source::sort_rows(&mut l.hidden.lock().unwrap(), now);
                }
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
            let (shown, hidden) = if l.show_hidden.load(Ordering::SeqCst) {
                (v, Vec::new())
            } else {
                v.into_iter()
                    .partition(|r| !super::kind::is_hidden(&r.name))
            };
            *l.rows.lock().unwrap() = shown;
            *l.hidden.lock().unwrap() = hidden;
            *l.error.lock().unwrap() = None;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            l.landed.store(now, Ordering::SeqCst);
        }
        Err(e) => {
            *l.rows.lock().unwrap() = Vec::new();
            l.hidden.lock().unwrap().clear();
            *l.error.lock().unwrap() = Some(e);
        }
    }
    true
}

impl Listing {
    /// 开一趟。回的是这一趟的号：每趟一个新号，只收**最后发出的那一趟**
    /// （同一个目录两趟在飞，后发的先回、先发的后回 ⇒ 先发的那份丢掉，不把刚删的文件又摆回来）。
    pub fn start(&self) -> u64 {
        self.inflight.fetch_add(1, Ordering::SeqCst);
        self.epoch.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// 换目录：号 +1（在飞的那些从此全部作废）、内容清空。
    ///
    /// ⚠ **清空是刻意的**：留着上一个目录的行、只改路径栏，是最贵的那种假象
    /// —— 用户会对着 B 的路径删 A 的文件。
    pub fn invalidate(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.rows.lock().unwrap().clear();
        self.hidden.lock().unwrap().clear();
        *self.error.lock().unwrap() = None;
    }

    /// 切隐藏文件显不显示：显示 ⇒ 收起来的那几行并回来、按 `sort` 重排；不显示 ⇒ 挪到一边。
    pub fn set_show_hidden(&self, on: bool, sort: Sort) {
        self.show_hidden.store(on, Ordering::SeqCst);
        let mut rows = self.rows.lock().unwrap();
        let mut hidden = self.hidden.lock().unwrap();
        if on {
            rows.append(&mut hidden);
            super::source::sort_rows(&mut rows, sort);
        } else {
            let (shown, gone): (Vec<Listed>, Vec<Listed>) = std::mem::take(&mut *rows)
                .into_iter()
                .partition(|r| !super::kind::is_hidden(&r.name));
            *rows = shown;
            *hidden = gone;
        }
    }

    pub fn is_loading(&self) -> bool {
        self.inflight.load(Ordering::SeqCst) > 0
    }
}

/// 窗口的全部状态。
/// 🔴**「就是这个文件」** —— 一次 reveal 的两半。
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
    /// 〔「有损名的进目录…整条寻址链要换成字节」〕当前目录不是合法 UTF-8 时它的原始字节
    /// （`cwd` 是它的有损显示串）。`None` ⇒ `cwd` 就是真字节。发出去的一律经 [`Self::cwd_path`] / [`Self::row_path`]。
    pub cwd_raw: Option<Vec<u8>>,
    pub listing: Listing,
    pub tally: RenderTally,
    /// 命中那一摞这一帧画了什么。**与 [`Self::tally`] 刻意是两个类型** ——
    /// [`HitTally`] 里没有「谁被点了」这个概念，逐条理由住那个类型的头注。
    pub hits_tally: HitTally,
    /// 远端列目录要在 tokio 上跑，而**判据里大量窗口拿不到运行时**，所以是 `Option`。
    ///
    /// ⚠ 从前这句话写的是「本机不需要，所以是 `Option`」—— 本机那一侧退役之后
    /// 那个理由不成立了，但这一格**照旧是 `Option`**，换了一条真的理由：
    /// 起窗那条路（`super::proc`）与判据都可能手上没有运行时，而那时它要**出声**
    /// （[`Self::reload`] 里那一支），不许假装列了个空目录。
    pub rt: Option<tokio::runtime::Handle>,
    /// 🔴**通道**：窗口进程够后端的唯一一条路（`source::Line`）。
    ///
    /// 是 `Option` 的理由与 [`Self::rt`] 同：判据里大量窗口不连后端（只画行、只点按钮）。
    /// 没有它的时候，每一件要问后端的事都**出声**（[`NO_LINE`]），不静默、不退回 SFTP（`D11`）。
    pub line: Option<Line>,
    /// 这台机器的能力事实（接上通道时问一次，`chan::wire::Offer`）：做不到的那几件在菜单上置灰并说为什么。
    pub offer: std::sync::Arc<std::sync::Mutex<Option<comms_inward::chan::wire::Offer>>>,
    /// 拖入那一摞的状态机（**先一次问完，再并行传**）。
    pub board: DropBoard,
    /// 已经消化过几趟拖入。`board.rounds()` 走在它前面 ⇒ 该重列一次目录了。
    ///
    /// ⚠ 为什么不让那条 tokio 任务自己 `reload()`：它手上没有 `&mut self`，
    /// 而把 `cwd` 也塞进 `Arc` 只为了让它能读，是把窗口状态搬到共享内存里去。
    /// ⇒ 换个方向：任务只记「我跑完了」，**换目录这件事一直留在 UI 线程手上**。
    seen_rounds: u64,
    /// 第二段：零流量复制那一趟的状态机（**问覆盖 · 进度 · 裁决**）。
    pub copy_board: CopyBoard,
    /// 算大小那一摞的看板（在算哪一项 · 上一摞的结局）。
    pub size_board: super::size::SizeBoard,
    /// 解压那一趟的看板（在解哪一个 · 撞名那一问 · 结局）。
    pub extract_board: super::extract::ExtractBoard,
    /// 已经消化过几趟解压（同 [`Self::seen_rounds`]）。
    seen_extract_rounds: u64,
    /// 正在读的那一份的原始字节（到货时交给编辑面；`(显示路径, 字节)`）。
    edit_raw: Option<(String, Vec<u8>)>,
    /// 〔「原生选文件框」〕选择框（生产是操作系统自己的，判据换一个假的）＋ 结局落点 ＋ 下载那一问上「没选到」那句话。
    pub picker: std::sync::Arc<dyn super::picker::Picker>,
    pick_board: super::picker::PickBoard,
    /// 「复制为」那个框。`None` = 没在问名字。**UI 线程自己的**（理由见 [`CopyPrompt`]）。
    copy_prompt: Option<CopyPrompt>,
    /// 「复制到另一台」下拉里的机器（开窗种子带来；新标签页照抄）。
    pub machines: Vec<String>,
    /// 「复制到另一台」那一问（UI 线程自己的）。
    cross_prompt: Option<super::cross_copy::CrossPrompt>,
    /// 那一问里「放到」那一块小目录选择器（那台的文件夹；`cross_copy::DirPick`）。
    pub cross_pick: super::cross_copy::DirPick,
    /// 复制到另一台那一趟的看板（盖不盖那一问 · 一条进度 · 结局）。
    pub cross_board: super::cross_copy::CrossBoard,
    /// 已经消化过几趟复制（同 [`Self::seen_rounds`]，两条路各一个数）。
    seen_copy_rounds: u64,
    /// `fonts.rs`：这个窗口的字体装没装上、复核没复核过。
    ///
    /// 🔴 默认是 `NotInstalled` 而**不是**「一切正常」——
    /// 判据直接 `FileWindow::new(...)` 建出来的窗口就是这一态，而它**会在界面上出声**。
    /// 「装字体那一步被谁摘了」因此不可能安静地过去。
    pub font: FontState,
    /// 搜索那一趟的共享落点（UI 线程读，tokio 那条写）。
    pub search: SearchBoard,
    /// 搜索框里那几个字。**UI 线程自己的**（同 [`Self::copy_prompt`] 的理由：
    /// 它是一个正在被编辑的草稿，不该出现在两条线程共享的那份状态里）。
    query: String,
    /// 搜索范围「整台机器」（UI 线程自己的；关 ⇒ 当前目录以下）。
    search_whole: bool,
    /// 结果表按哪一列排（排是后端排的，这里只记用户点了哪一列）。
    hit_sort: find::FindSort,
    /// 结果表上选中的那一条（下标只指命中这一摞）。
    hit_pick: Option<usize>,
    /// 按内容搜那一趟的共享落点（UI 线程读，tokio 那条写；`super::grep`）。
    pub grep: GrepBoard,
    /// 按内容搜那个框里的字（UI 线程自己的草稿，同 [`Self::query`]）。
    grep_query: String,
    /// 命中那一摞这一帧交出来的东西（被点了哪一条 —— **命中这一摞**的下标）。
    pub grep_tally: GrepTally,
    /// 🔴：那四条写操作的状态机（**一次问完 · 结果**）。
    pub write_board: WriteBoard,
    /// 「叫什么名字 / 改成什么权限」那个框。`None` = 没在问。
    /// **UI 线程自己的**（理由见 [`WritePrompt`]）。
    write_prompt: Option<WritePrompt>,
    /// 改权限那个框的**现值**那一趟（UI 线程读，tokio 那条写；`writeops::ModeProbe`）。
    pub mode_probe: super::writeops::ModeProbe,
    /// 就地那一格（改名 · 新建）答完发出去的那一趟落在这儿：`(第几趟, 成了 ⇒ 新名字 / 没成 ⇒ 码 ＋ 那句话)`。
    inline_done: InlineDone,
    /// 就地那一格发出去的趟数（晚到的上一趟不许落到这一格上）· 这一格在飞吗 · 下一帧要选中前几个字。
    inline_gen: u64,
    inline_busy: bool,
    inline_select: Option<usize>,
    /// 已经消化过几摞写操作（同 [`Self::seen_rounds`]，每条路各一个数）。
    seen_write_rounds: u64,
    /// 🔴**往外拖**那一趟的共享落点（进度 · 结局）。
    pub pull: super::download::DownloadBoard,
    /// 「存到哪儿 / 盖掉它吗」那两问。`None` = 没在问。
    /// **UI 线程自己的**（同 [`Self::write_prompt`] 的理由：它是一个正在被编辑的草稿）。
    /// 系统存盘框开着时要下的那一项：`(远端整条路径, 名字)`（选完落点就起下载；取消 ⇒ 什么都不做）。
    pull_want: Option<(String, String)>,
    /// 那一问摆着的那一行，若整条路径不是 UTF-8：`(整条路径的字节, 显示名, 名字的字节)`。
    pull_raw: Option<(Vec<u8>, String, Vec<u8>)>,
    /// 🔴编辑那一趟的共享落点（读到货 · 存结局）。
    pub edits: super::editor::EditBoard,
    /// 打开着的那一份文本。`None` = 没在编辑。**UI 线程自己的**。
    editing: Option<super::editor::Pane>,
    /// 关窗那一问正摆着吗（改了没存）。
    asking_discard: bool,
    /// **按哪一列排、正着还是反着**（点表头改）。缺省按名称。
    pub sort: Sort,
    /// 详情视图的列宽（拖表头上的分隔线改）。
    pub cols: super::rows::Columns,
    /// 后退 · 前进两摞（每个标签页自己一份；最近的在末尾）。
    pub(super) back: Vec<super::source::RemotePath>,
    pub(super) ahead: Vec<super::source::RemotePath>,
    /// 地址栏正在手输吗（`Some` ＝ 输入框里那一串）。
    pub(super) addr_edit: Option<String>,
    /// 搜索框在「内容」那一段（UI 线程自己的）。
    pub(super) search_content: bool,
    /// 打不开那一条上点了「回主目录」（主目录住窗口那一级：由 `Workspace` 接过去）。
    pub(super) want_home: bool,
    /// 这一趟列目录从哪一刻起在等（骨架行 300 ms 后才画）。
    loading_since: Option<f64>,
    /// 窗口底部那张「进度」表（一扇窗一份，所有标签页共用；`Workspace` 交下来）。逻辑住 [`super::progress`]。
    pub progress: super::progress::Progress,
    /// 那台此刻连没连着（一扇窗一份，`Workspace` 交下来；`link` 那条流写进来）。断着 ⇒ 列表留着上次的内容、写类按钮灰。
    pub link: super::chrome::LinkState,
    /// 一次性的回执（「路径已复制」…）：窗口那一级每帧收走、摆成右下角的回执（规范 `C13`）。
    pub(super) receipt: Option<String>,
    /// 带［撤销］的回执（改名 · 改权限做完）：那句话 ＋ 撤销要做的那几件。窗口那一级收走，点了撤销交回 [`Self::start_undo`]。
    pub(super) receipt_undo: Option<(String, Vec<WriteOp>)>,
    /// 列表上按了空格（「看一眼」开 / 收）：窗口那一级每帧收走。
    pub(super) want_peek: bool,
    /// 这个标签页开过的那几趟里，每一类最近那一趟在表里的号
    /// （换一块新看板时，上一趟改由窗口那一级在落地时重列目录，[`Self::track_job`]）。
    job_ids: Vec<(&'static str, u64)>,
    /// 用户按过「重建索引」（换目录就清）：新鲜度那一行这时也摆出来，答他刚问的那一下。
    pub(super) status_wanted: bool,
    /// 「属性」那一问（`None` ＝ 没摆）。逻辑住 [`super::props`]。
    pub(super) props: Option<super::props::Props>,
    /// 🔴〔补齐五项〕「在此打开终端」那一下**说了什么**（`None` = 没点过 / 上一下没话说）。
    ///
    /// # 为什么这一格非有不可
    ///
    /// 那条命令在 **POSIX 上恒定开不了窗**（壳回「不开窗」那个结局 `TerminalOpen::NoWindow`，对这一跳回拒绝
    /// `no_window` ＋ `rsLaunch.posix.noTerminalWindow` 那一句），在 Windows 上也可能失败
    /// （那台远端的配置没存全）。一次失败与一次成功在屏幕上长得一样
    /// ⇒ 用户点了按钮、什么都没发生、也没有一句话 —— 那正是本仓的头号病形。
    /// ⇒ 结果落在这一格，界面上画出来，判据读同一个值。
    term_notice: Arc<Mutex<Option<String>>>,
    /// 🔴**「就是这个文件」** —— 要高亮的那一行的名字 ＋ 滚过去了没有。
    ///
    /// 它是 `P3`（老面板退役）的最后一格功能前置：老面板 `open(revealPath)`
    /// 那一形（会话工具卡 → 文件跳转，`src/frontend/ui/cards/index.ts::openRemoteFileInSftp`）
    /// 在这之前窗口**一处都没有**。
    ///
    /// ⚠ 两个字段刻意分开：**高亮要一直留着**（一帧的高亮在连续重绘的窗口上等于看不见），
    /// 而**滚只滚一次**（每帧都滚就把用户自己的滚动按住了）。
    reveal: Option<Reveal>,
    /// 🔴**选中态**（哪几行 · 键盘光标 · Shift 的锚）。**UI 线程自己的**。
    ///
    /// ⚠ 按名字记（理由住 [`super::select`] 头注 §二）。换目录清空；
    ///   一摞写操作跑完只清「选中」、留着光标（[`Self::settle_finished_writes`]）。
    selection: Selection,
    /// 打字跳转攒着的那几个字。
    type_ahead: TypeAhead,
    /// 键盘把光标挪到了第几行 ⇒ 这一帧画列表时要不要把它滚进视野。**只滚一次**。
    key_scroll: Option<usize>,
    /// 摆着的那个右键菜单（`None` = 没摆）。
    menu: Option<MenuAt>,
    /// 键盘 / 菜单那一下**做不了**时说的那句话（`None` = 没话说）。
    ///
    /// ⚠ 刻意不写进 `listing.error`：那一格只在下一趟列目录时才清，
    ///   而「打字跳转没找到」是一句**下一次按键就过时**的话 ⇒ 下一次按键 / 点击就清掉。
    key_notice: Option<String>,
    /// 书签（一个窗口一份，所有标签页 / 两栏共用；逻辑住 [`super::bookmarks`]）。
    /// `None` ＝ 没接上（判据里直接建的窗口）⇒ 书签栏不画。生产那条开窗路恒是 `Some`。
    pub shelf: Option<super::bookmarks::Shelf>,
    /// 这个目录视图此刻**是不是焦点那一个**（双栏 / 标签页时只有一个是）。
    ///
    /// `false` ⇒ 不接键盘、不接拖入（[`Self::keys_blocked`] / [`Self::take_drops`]）。缺省 `true`：
    /// 单独建出来的目录视图（判据里那几百个）就是唯一那一个。谁来改它：[`super::workspace`]。
    pub focused: bool,
    /// 〔「行拖到另一栏的手势」〕有一行（连同它所在的那一摞选中）被拖起、还没松手。
    ///   松在哪儿由 [`super::workspace::Workspace`] 看（它才知道另一栏在哪）；它收摊时清掉这一格。
    pub dragging: bool,
    /// 起过几摞写操作（与 `write_board.rounds()` 比 ⇒ 有没有还没回话的；关标签那一问用）。
    writes_started: u64,
    /// 起过几摞上传 / 复制 / 跨机复制（与各自看板的 `rounds()` 比 ⇒ 那一类还有一趟没完）。
    drops_started: u64,
    copies_started: u64,
    crosses_started: u64,
    /// 框里那一问做不成的原因（画在框里；框外列表上方那一行被暗底盖着）。框收掉就清。
    pub(super) prompt_error: Arc<Mutex<Option<String>>>,
    /// 搜索命中那一摞画好的行（按那一问的号与落地趟数缓存：没变就不重建）。
    hit_rows: (u64, u64, Arc<Vec<super::rows::HitRow>>),
    /// 这个标签页是一份文本的**编辑页**（稿 ⑤：标签上铅笔 ＋ 文件名，正文是编辑面、不是列表）。
    pub edit_tab: bool,
    /// 这个目录视图挂在窗口里（`Workspace` 置）：打开一份文本 ⇒ 交给窗口开一个编辑页，不在这一页里开。
    pub(super) in_workspace: bool,
    /// 要开的那一份（目录页交给窗口那一级：开一个编辑页 / 切到已开着的那一页）。
    pub(super) want_edit: Option<EditWant>,
    /// 编辑页头上点了面包屑那一段 / 「在列表里显示」：窗口那一级开一个目录页到那里（或切到已在那里的那一页）。
    pub(super) want_dir: Option<String>,
    /// 「在列表里显示」要高亮的那个名字（随 [`Self::want_dir`] 交给那个目录页）。
    pub(super) want_reveal: Option<String>,
    /// 编辑页要关掉自己（那一问答了「不保存」，或答了「保存」而存成了）。
    pub(super) want_close: bool,
    /// 那一问答了「保存」：存成就关。
    close_after_save: bool,
    /// 打不开 / 不能编辑那一句（编辑页中央画；`None` ＝ 没出问题）。
    pub(super) edit_refused: Option<String>,
    /// 编辑页的那一行（「下载到本机」「重试」要它）。
    edit_row: Option<Listed>,
    /// 编辑页的查找条摊开着吗（Ctrl+F 开 · Esc / × 收）。
    pub(super) find_open: bool,
}

/// 目录页交给窗口的「开这一份」：那一行 ＋ 它的整条路径（有损名按字节）＋ 本地预判做不了的原因。
#[derive(Clone, Debug)]
pub struct EditWant {
    pub row: Listed,
    pub at: super::source::RemotePath,
    pub refused: Option<String>,
}

/// 一个摆着的右键菜单：**在哪儿 · 列哪几项 · 对几项说话**。
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

/// 菜单上一项都没有时摆的那一句（有损名那一档：什么都做不了，但要说出来）。
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
        // 接上就问一次那台的能力事实（不等它：没回来之前菜单照常，点了由那台的回话兜）。
        if let Some(h) = &self.rt {
            let (l, origin, slot) = (line.clone(), self.source.origin(), self.offer.clone());
            h.spawn(async move {
                let budget = comms_inward::chan::wire::Budget {
                    until: std::time::Instant::now() + OFFER_WITHIN,
                    cancel: Default::default(),
                };
                if let Ok(o) = l.offer(&origin, budget).await {
                    *slot.lock().unwrap() = o;
                }
            });
        }
        self.line = Some(line);
    }

    /// 和那台断着、而这一件要碰那台 ⇒ 「离线 · 只读」（菜单 · 命令栏悬停 · 键盘按了的回话同一句）。
    pub fn offline_refusal(&self, a: Action) -> Option<String> {
        self.offline_cmd(super::chrome::Cmd::Act(a))
    }

    /// 🔴 **断线时哪几件做不了 —— 唯一一处**：命令栏每一颗 · 菜单每一项 · 键盘都问它。
    /// 要碰那台机器的（新建 · 上传 · 终端 · 复制到另一栏 · 选中项上的每一件，打开目录除外）⇒ 「离线 · 只读」；
    /// 只改这一扇窗怎么看的（左栏 · 显示隐藏文件 · 双栏 · 预览）照常。打开目录照常点得动：进去那一下自己会说连不上。
    pub fn offline_cmd(&self, c: super::chrome::Cmd) -> Option<String> {
        use super::chrome::Cmd;
        let needs_machine = match c {
            Cmd::Sidebar | Cmd::Hidden | Cmd::Split | Cmd::Preview => false,
            Cmd::Act(Action::Open) => false,
            Cmd::Mkdir | Cmd::NewFile | Cmd::Upload | Cmd::Term | Cmd::Across | Cmd::Act(_) => true,
        };
        (needs_machine && self.link.offline())
            .then(|| copy_text("rsFilewinChrome.link.readOnly", &[]))
    }

    /// 这件事在这台机器上做不到 ⇒ 那句为什么（菜单置灰的 hover · 键盘按了的回话）；做得到 / 没把握 ⇒ `None`。
    pub fn unavailable_here(&self, a: Action) -> Option<String> {
        let op = match a {
            Action::Chmod => "files-chmod",
            _ => return None,
        };
        let offer = self.offer.lock().unwrap();
        offer.as_ref()?.unavailable(op)?; // 码不上屏：只说做不到
        Some(copy_text("rsFilewinShell.menu.unavailableHere", &[]))
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
        // 🔴〔补齐五项〕第一屏走的是 `Vec<Listed>`（〔09-28 裁 3〕窗口进程自己列的，`proc::first_screen`）
        //    ⇒ 链接与时间两格第一屏就有。入参收 `Into<Listed>` 是为了
        //    判据夹具照旧能喂 `Vec<Row>`（那一形 ＝ `Listed::plain`，两格都「没送」）。
        // ⚠ **这一屏刻意不再排一次**：第一屏列的时候（〔09-28 裁 3〕窗口进程的 `proc::first_screen` → `source::list_dir`）已经按缺省那一档排过
        //   （`source::rows_from_ls_data`），再排一遍是恒等。
        //   在这儿插一次 `sort_rows` 试过一趟，读数如实记：
        //   `shell_tests::a_write_from_the_menu_reaches_the_right_row` 当场红 ——
        //   它喂的夹具是乱序的，于是「第 1 行是哪一行」被改掉了。
        //   ⇒ 那是一次**谁都没要求的行为变更**（生产上零收益，判据上真伤），撤掉。
        //   用户换档那一下由 [`FileWindow::set_sort`] 就地重排，不经这里。
        *listing.rows.lock().unwrap() = rows.into_iter().map(Into::into).collect();
        Self {
            source,
            cwd,
            cwd_raw: None,
            listing,
            tally: RenderTally::default(),
            hits_tally: HitTally::default(),
            rt,
            line: None,
            offer: Default::default(),
            board: DropBoard::default(),
            seen_rounds: 0,
            copy_board: CopyBoard::default(),
            size_board: super::size::SizeBoard::default(),
            extract_board: super::extract::ExtractBoard::default(),
            seen_extract_rounds: 0,
            edit_raw: None,
            picker: super::picker::native(),
            pick_board: super::picker::PickBoard::default(),
            copy_prompt: None,
            machines: Vec::new(),
            cross_prompt: None,
            cross_pick: Default::default(),
            cross_board: super::cross_copy::CrossBoard::default(),
            seen_copy_rounds: 0,
            font: FontState::NotInstalled,
            search: SearchBoard::default(),
            query: String::new(),
            search_whole: false,
            hit_sort: find::FindSort::default(),
            hit_pick: None,
            grep: GrepBoard::default(),
            grep_query: String::new(),
            grep_tally: GrepTally::default(),
            write_board: WriteBoard::default(),
            write_prompt: None,
            mode_probe: super::writeops::ModeProbe::default(),
            inline_done: Default::default(),
            inline_gen: 0,
            inline_busy: false,
            inline_select: None,
            seen_write_rounds: 0,
            pull: super::download::DownloadBoard::default(),
            pull_want: None,
            pull_raw: None,
            edits: super::editor::EditBoard::default(),
            editing: None,
            asking_discard: false,
            sort: Sort::default(),
            cols: super::rows::Columns::default(),
            back: Vec::new(),
            ahead: Vec::new(),
            addr_edit: None,
            search_content: false,
            want_home: false,
            loading_since: None,
            progress: Default::default(),
            link: Default::default(),
            receipt: None,
            receipt_undo: None,
            want_peek: false,
            job_ids: Vec::new(),
            props: None,
            status_wanted: false,
            term_notice: Arc::new(Mutex::new(None)),
            reveal: None,
            selection: Selection::default(),
            type_ahead: TypeAhead::default(),
            key_scroll: None,
            menu: None,
            key_notice: None,
            shelf: None,
            focused: true,
            dragging: false,
            writes_started: 0,
            drops_started: 0,
            copies_started: 0,
            crosses_started: 0,
            prompt_error: Arc::new(Mutex::new(None)),
            hit_rows: (u64::MAX, u64::MAX, Arc::new(Vec::new())),
            edit_tab: false,
            in_workspace: false,
            want_edit: None,
            want_dir: None,
            want_reveal: None,
            want_close: false,
            close_after_save: false,
            edit_refused: None,
            edit_row: None,
            find_open: false,
        }
    }

    /// 重新列一次当前目录。**本机同步做完；远端扔给 tokio，不堵住 UI 线程。**
    pub fn reload(&self) {
        // 和那台断着、手上摆着上一屏 ⇒ 不去问（问了只会把那一屏换成一句「连不上」）：列表进过期态、照常摆着，
        //   连上之后窗口那一级把每个标签页重列一遍（`Workspace::settle_link`）。
        if self.link.offline() && !self.listing.rows.lock().unwrap().is_empty() {
            return;
        }
        let l = self.listing.clone();
        let mine = l.start();
        // 〔有损名全寻址〕发出去的是线上那一形（有损目录 ⇒ `{"b16": …}`）。
        let cwd = self.cwd_path().wire();
        // 🔴**先问后端，问不到才退回旧路** ——
        //    用户指令的第 1 步，逐条理由住 `source.rs` 那一段头注。
        // 🔴「问不到才退回旧路」那半句**拿掉了**（`D11`）：只问后端。
        match &self.rt {
            // ── 有运行时 ⇒ 主路（`files-ls` on 这个 origin）───────────
            Some(h) => {
                // 🔴有运行时但没连上通道 ⇒ 出声（`D11`：不退回 SFTP）。
                let Some(line) = self.line.clone() else {
                    store_if_current(&l, mine, Err(NO_LINE.to_string()));
                    return;
                };
                let origin = self.source.origin();
                // ⚠ 带**这一刻**选的那一档走；落地时用户已换了档 ⇒ [`store_listed_if_current`] 按现在那一档重排。
                let by = self.sort;
                h.spawn(async move {
                    let r = super::source::list_via_backend_at(
                        &line,
                        &origin,
                        cwd,
                        super::source::LS_LIMIT,
                        by,
                    )
                    .await;
                    // 打不开（后端说是哪一种）⇒ 记下那一种，列表顶上画那一条 ＋ 出路。
                    let fail = r.as_ref().err().and_then(|f| {
                        super::source::OpenFail::of_code(f.code.as_deref())
                            .map(|k| (k, f.said.clone()))
                    });
                    if store_listed_if_current(&l, mine, r.map_err(|f| f.said), by) {
                        *l.open_fail.lock().unwrap() = fail;
                    }
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
        self.navigate_to_at(super::source::RemotePath::plain(&path));
    }

    /// 〔有损名全寻址〕当前目录（显示串 ＋ 可能有的字节）。
    pub fn cwd_path(&self) -> super::source::RemotePath {
        super::source::RemotePath::of(&self.cwd, self.cwd_raw.as_deref())
    }

    /// 列表里那一行的整条路径 ＝ 当前目录的字节 ＋ `/` ＋ 名字的字节（有损名用后端送的那一段字节）。
    pub fn row_path(&self, r: &super::source::Listed) -> super::source::RemotePath {
        join_path(&self.cwd_path(), &name_bytes(r))
    }

    // 〔散文墓碑〕这里原有 `refused_in_lossy_cwd`（W5-FILES：有损目录里上传 / 搜索 / 开终端出声拒）——
    //   三件都改成按字节做了（搜索 `find::run_search_at` · 上传 `Pending::remote_dir_raw` · 终端 `build_open_terminal_cmd_at`〔散文墓碑〕，今天是后端 `dial/terminal.rs::command_for_cwd`），它没了调用方。

    /// 〔有损名全寻址〕进一个目录（显示串 ＋ 可能有的字节）。与 [`Self::navigate_to`] 同一套收摊。
    pub fn navigate_to_at(&mut self, at: super::source::RemotePath) {
        self.go_at(at, Hist::Push);
    }

    /// 后退一步（回值 ＝ 真的换了目录）。
    pub fn go_back(&mut self) -> bool {
        match self.back.pop() {
            Some(p) => self.go_at(p, Hist::Back),
            None => false,
        }
    }

    /// 前进一步（回值 ＝ 真的换了目录）。
    pub fn go_forward(&mut self) -> bool {
        match self.ahead.pop() {
            Some(p) => self.go_at(p, Hist::Forward),
            None => false,
        }
    }

    /// 后退 / 前进还有没有可走的（工具条那两颗灰不灰）。
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }
    pub fn can_go_forward(&self) -> bool {
        !self.ahead.is_empty()
    }

    /// 换目录的唯一一处：记历史（新走一步 ⇒ 前进那一摞清掉）、收摊、重列。
    fn go_at(&mut self, at: super::source::RemotePath, hist: Hist) -> bool {
        if at == self.cwd_path() {
            return false;
        }
        let here = self.cwd_path();
        match hist {
            Hist::Push => {
                self.back.push(here);
                self.ahead.clear();
            }
            Hist::Back => self.ahead.push(here),
            Hist::Forward => self.back.push(here),
        }
        self.addr_edit = None;
        self.props = None;
        self.status_wanted = false;
        let path = at.shown.clone();
        // 🔴换了目录，那一行就不在这儿了 ⇒ 高亮清掉。
        //    留着的话，新目录里**恰好同名**的另一个文件会被高亮 ——
        //    而用户会以为那就是他要找的那个。
        self.reveal = None;
        // 🔴选中态按名字记 ⇒ 新目录里**同名**的那几个不是同一样东西。
        //    留着的话按 Delete 删的是新目录里恰好同名的文件。
        self.selection.clear();
        self.type_ahead.clear();
        self.key_scroll = None;
        self.menu = None;
        self.key_notice = None;
        self.cwd = path;
        self.cwd_raw = at.raw;
        self.listing.invalidate();
        self.reload();
        // 换目录时现读一次书签：别的窗口刚加的那几条从这里进来（小文件一次读，不是每帧）。
        if let Some(s) = &self.shelf {
            s.refresh();
        }
        true
    }

    /// 上一级。已经在顶上就什么都不做（[`parent_dir`] 到顶回原值）。
    pub fn navigate_up(&mut self) {
        // 按字节切（合法 UTF-8 时与 `parent_dir` 逐字节同）。
        let up = self.cwd_path().parent();
        self.navigate_to_at(up);
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
    /// 那条纪律：64 万行那一档每帧一次 `sort` 直接把帧时打穿
    /// （虚拟滚动省的是**画**，不是遍历）。⇒ 排序只在两个时刻发生：
    /// **一屏落地**（[`super::source::list_dir`]）与**用户换档**（这里）。
    pub fn set_sort(&mut self, sort: impl Into<Sort>) -> bool {
        let sort = sort.into();
        if self.sort == sort {
            return false;
        }
        self.sort = sort;
        // 先记下这一档，再排手上这一摞（在路上的那一趟落地时比这一档，见 `store_listed_if_current`）。
        *self.listing.sort.lock().unwrap() = sort;
        super::source::sort_rows(&mut self.listing.rows.lock().unwrap(), sort);
        super::source::sort_rows(&mut self.listing.hidden.lock().unwrap(), sort);
        true
    }

    /// 点了表头那一列（同一列再点 ⇒ 反过来）。
    pub fn click_header(&mut self, by: super::source::SortBy) -> bool {
        let next = self.sort.after_click(by);
        self.set_sort(next)
    }

    /// 隐藏文件显示着吗（缺省显示，淡一级）。
    pub fn shows_hidden(&self) -> bool {
        self.listing.show_hidden.load(Ordering::SeqCst)
    }

    /// 切隐藏文件显不显示：收起来的那几行挪到一边（不是删），选中里它们那几项一并去掉。
    pub fn set_show_hidden(&mut self, on: bool) -> bool {
        if self.shows_hidden() == on {
            return false;
        }
        self.listing.set_show_hidden(on, self.sort);
        if !on {
            let rows = self.listing.rows.lock().unwrap();
            let keep: std::collections::BTreeSet<String> = rows
                .iter()
                .map(|r| select::pick_key(r).into_owned())
                .collect();
            self.selection.retain(&keep);
        }
        true
    }

    /// 「在此打开终端」那一下说了什么（`None` = 没话说）。判据与界面看同一个值。
    pub fn term_notice(&self) -> Option<String> {
        self.term_notice.lock().unwrap().clone()
    }

    /// 在**当前这个目录**里给用户开一个真终端。回值 = 真的发出去了。
    ///
    /// 窗口只交**意图**：经它那条通道 `call` 一条 monitor 自己接的
    /// [`filewin_contract::TERMINAL_OPEN_OP`]（寻址 ＝ 这台 · 参数 `{cwd}`）。之后三步都在 monitor 那一侧
    /// （`chan/host.rs::terminal_open`）：补这台的机器事实 → 问本机后端 `terminal-ssh` 渲那一行（`cd` 那一串也在那里拼，
    /// `src/backend/dial/terminal.rs::command_for_cwd`）→ `launch::open_terminal_window` 开窗 —— 与主界面开终端同一条路。
    /// 窗口不拼命令、不认识 monitor 的配置、不起进程；那一问不成 ⇒ 对端那句原话画在窗口上（`D11`，不退回自己拼）。
    ///
    /// # ⚠ 它在 Linux 上**恒定「失败」，而那不是缺陷**
    ///
    /// POSIX 上壳回「不开窗」那个结局（这一跳收到拒绝 `no_window` ＋ `rsLaunch.posix.noTerminalWindow` 那一句）
    /// —— 那是一条**既定设计**，不是没做完。
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
        let Some(line) = self.line.clone() else {
            *self.term_notice.lock().unwrap() = Some(NO_LINE.to_string());
            return false;
        };
        // 有损目录：当前目录按字节交（线上形 `{"b16": …}`），`cd` 那一串在后端走唯一的 quote 的字节形。
        let args = filewin_contract::terminal_open_args(self.cwd_path().wire());
        let origin = self.source.origin();
        let slot = self.term_notice.clone();
        *slot.lock().unwrap() = None;
        h.spawn(async move {
            let opened = super::source::ask(
                &line,
                &origin,
                filewin_contract::TERMINAL_OPEN_OP,
                &args,
                TERMINAL_WITHIN,
            )
            .await;
            let said = opened.err().map(|why| {
                copy_text(
                    "rsFilewinShell.terminal.failed",
                    &[("why", &why.to_string())],
                )
            });
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
    /// （编辑 · 下载在行上与菜单里；预览是右侧那块面板，跟着选中走，不靠双击）。
    pub fn activate(&mut self, i: usize) -> bool {
        let target = {
            let rows = self.listing.rows.lock().unwrap();
            match rows.get(i) {
                // 有损名目录：没带字节就进不去（寻址不到）；带了 ⇒ 按字节进。
                Some(r) if r.opens_as_dir() && (!r.lossy_name || r.raw_name.is_some()) => {
                    join_path(&self.cwd_path(), &name_bytes(r))
                }
                _ => return false,
            }
        };
        let before = self.cwd_path();
        self.navigate_to_at(target);
        self.cwd_path() != before
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
    /// 那条纪律的反面。⇒ 锁与扫描都收进来，一次克隆都没有。
    pub fn take_reveal_offset(&mut self, pitch: f32) -> Option<Result<f32, String>> {
        // ① 先取出要找的名字（只读借用，随即放掉）。
        let want = match self.reveal.as_ref() {
            Some(r) if !r.scrolled => r.name.clone(),
            _ => return None,
        };
        // ② 握锁扫一遍（**只扫一遍，只在这一次 reveal 里**，不是每帧）。
        let (mut found, hidden) = {
            let rows = self.listing.rows.lock().unwrap();
            let hidden = self.listing.hidden.lock().unwrap();
            // ⚠ 列表还没到货 ⇒ **这一帧不算「滚过了」**，下一帧再来
            //   （不然会在空列表上判成「那一行不在」）。
            if rows.is_empty() && hidden.is_empty() {
                return None;
            }
            let found = super::rows::reveal_index(&rows, &want);
            let hidden = found.is_none() && super::rows::reveal_index(&hidden, &want).is_some();
            (found, hidden)
        };
        // 它是一个隐藏文件、而隐藏文件关着 ⇒ 打开显示隐藏文件、照样跳过去，说一句（不说「可能被删了」）。
        if hidden {
            self.set_show_hidden(true);
            found = super::rows::reveal_index(&self.listing.rows.lock().unwrap(), &want);
            self.key_notice = Some(copy_text(
                "rsFilewinShell.reveal.hiddenShown",
                &[("want", &want)],
            ));
        }
        // ③ 锁放掉了，这里才改自己。
        if let Some(r) = self.reveal.as_mut() {
            r.scrolled = true;
        }
        match found {
            // 🔴 像素那一步在这儿，用的是那**唯一住址**的步距（`rows::row_pitch`）——
            //    第一版这里乘的是行高，漏了行间距，滚偏 14%。
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

    /// 🔴**发一趟搜索** —— 那一件在窗口上的落点。
    ///
    /// 回值 = 真的发出去了一趟（框是空的又不是按按钮、或这个窗口没有 tokio 运行时 / 没接上通道 ⇒ `false`，后两种出声）。
    /// 发的是**原样的搜索词**（语法这一侧不读）；「只搜当前目录」开着 ⇒ 带上当前目录，否则后端搜家目录。
    /// 搜索**只有后端那一条路**（SFTP 给不了搜索），后端没起来就出声，不退回本地遍历。
    ///
    /// `force_rebuild` 是那颗「重建索引」按钮；平时要不要重走由后端回的那三个判断决定（[`super::find`] 头注）。
    pub fn fire_search(&mut self, ctx: Option<egui::Context>, force_rebuild: bool) -> bool {
        if force_rebuild {
            self.status_wanted = true;
        }
        let asked = find::Asked {
            query: self.query.clone(),
            under: (!self.search_whole).then(|| self.cwd_path()),
            machine: self.search_whole,
            sort: self.hit_sort,
        };
        self.hit_pick = None;
        self.search.attach(ctx);
        self.search.invalidate(&asked);
        if asked.query.trim().is_empty() && !force_rebuild {
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
        // 有损目录里也搜：范围与浏览名单按字节发。
        let cwd = self.cwd_path();
        h.spawn(async move {
            find::run_search_at(board, line, origin, cwd, asked, mine, force_rebuild).await;
        });
        true
    }

    /// 命中那一摞滚到底了：后端说后面还有、没有一趟在飞 ⇒ 同号再要下一屏。回值 = 发出去了。
    pub fn fire_more(&mut self, ctx: Option<egui::Context>) -> bool {
        let (Some(h), Some(line)) = (self.rt.clone(), self.line.clone()) else {
            return false;
        };
        let Some((mine, asked, offset)) = self.search.claim_more() else {
            return false;
        };
        self.search.attach(ctx);
        let board = self.search.clone();
        let origin = self.source.origin();
        h.spawn(async move {
            find::fetch_more(board, line, origin, mine, asked, offset).await;
        });
        true
    }

    /// 命中那一摞画成的行：那一问的号与落地趟数都没变 ⇒ 用上一帧那一份（几千条命中不每帧整份克隆、重拼）。
    pub fn hit_rows(&mut self) -> Arc<Vec<super::rows::HitRow>> {
        let key = (self.search.current(), self.search.rounds());
        if (self.hit_rows.0, self.hit_rows.1) != key {
            let rows = self.search.hit_rows();
            self.hit_rows = (key.0, key.1, Arc::new(rows));
        }
        self.hit_rows.2.clone()
    }

    /// 搜索范围：`true` ＝ 整台机器，`false` ＝ 当前目录以下（判据用；界面上是状态行那个下拉）。
    pub fn set_search_whole(&mut self, on: bool) {
        self.search_whole = on;
    }

    /// 点了结果表的一列表头 ⇒ 换排序、按新序从头再问（排是后端排的）。
    pub fn click_hit_header(&mut self, col: find::SortCol, ctx: Option<egui::Context>) -> bool {
        self.hit_sort = self.hit_sort.clicked(col);
        self.fire_search(ctx, false)
    }

    /// 结果表眼下按哪一列排。
    pub fn hit_sort(&self) -> find::FindSort {
        self.hit_sort
    }

    /// 打开第 `i` 条文件名命中：文件 ⇒ 进它所在的目录、高亮它；目录 ⇒ 进去。清掉搜索框（屏幕换回目录列表）。
    /// 回值 = 真的跳了（下标对得上这一摞）。
    pub fn jump_to_find_hit(&mut self, i: usize) -> bool {
        let Some(hit) = self.search.hit(i) else {
            return false;
        };
        let at = super::source::RemotePath::from_bytes(&hit.path);
        if hit.dir {
            self.navigate_to_at(at);
        } else {
            let name = String::from_utf8_lossy(hit.name_bytes()).to_string();
            self.navigate_to_at(at.parent());
            self.set_reveal(&name);
        }
        self.query.clear();
        self.hit_pick = None;
        true
    }

    /// **发一趟按内容搜**：根 = 窗口现在在看的那个目录（同文件名搜索那一条取舍），要找的那一串 = 那个框里的字。
    /// 回值 = 真的发出去了（框是空的 / 没有运行时 / 没接上通道 ⇒ `false`，后两种出声）。上一趟还在飞 ⇒ 先撤掉它。
    pub fn fire_grep(&mut self, ctx: Option<egui::Context>) -> bool {
        let needle = self.grep_query.trim().to_string();
        self.grep.attach(ctx);
        if needle.is_empty() {
            return false;
        }
        let Some(h) = self.rt.clone() else {
            self.grep
                .say(&copy_text("rsFilewinShell.search.noRuntime", &[]));
            return false;
        };
        let Some(line) = self.line.clone() else {
            self.grep.say(NO_LINE.as_str());
            return false;
        };
        let (mine, cancel) = self.grep.start(&needle);
        let board = self.grep.clone();
        let origin = self.source.origin();
        let root = self.cwd_path();
        h.spawn(async move {
            grep::run_grep(board, line, origin, root, needle, mine, cancel).await;
        });
        true
    }

    /// 按内容搜那个框里的字（判据用；生产那一侧直接改字段）。
    pub fn set_grep_query(&mut self, q: &str) {
        self.search_content = true;
        self.grep_query = q.to_string();
    }

    /// 这一帧该画「按内容搜」的命中：文件名那个框是空的、按内容那个框里有字。
    pub fn showing_grep(&self) -> bool {
        self.search_content
            && !self.grep_query.trim().is_empty()
            && self.grep.shown().needle == self.grep_query.trim()
    }

    /// 点了第 `i` 条按内容搜的命中 ⇒ 进它所在的目录、高亮它；清掉那个框（屏幕换回目录列表，那一行亮着）。
    /// 回值 = 真的跳了（下标对得上这一摞）。
    pub fn jump_to_grep_hit(&mut self, i: usize) -> bool {
        let Some(o) = self.grep.shown().outcome else {
            return false;
        };
        let Some((dir, name)) = grep::jump_target(&o, i) else {
            return false;
        };
        self.navigate_to_at(dir);
        self.set_reveal(&name);
        self.grep_query.clear();
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
    /// 🔴 三段的顺序不在这里，在 [`crate::transfer::run_drop`] 的结构里 ——
    /// 这里只负责把「问谁 · 怎么问 · 怎么传」三个口接上去。
    /// 接错了会被 `transfer_tests` 逮住的是**那个函数**，不是本函数；
    /// 而本函数接不上（**没运行时**）要**出声**，判据见 `shell_tests`。
    /// ⚠ 从前这句话是「没运行时 / 本机源」—— 本机源那一支不在了（见 `Source` 头注）。
    pub fn start_drop(&mut self, items: Vec<Pending>, ctx: Option<egui::Context>) -> bool {
        if items.is_empty() {
            return false;
        }
        // 有损目录里也传：暂存区那一段与目录无关，探在不在与提交按目录的字节寻址（此前 W5-FILES 在这里出声拒）。
        let items: Vec<Pending> = items
            .into_iter()
            .map(|p| Pending {
                remote_dir_raw: self.cwd_raw.clone(),
                ..p
            })
            .collect();
        self.start_drop_items(items, ctx)
    }

    /// 同 [`Self::start_drop`]，只是这一摞的目标目录照它们自己带着的（「进度」表上「重试 n 个」「接着传」走这里）。
    pub fn start_drop_items(&mut self, items: Vec<Pending>, ctx: Option<egui::Context>) -> bool {
        if items.is_empty() {
            return false;
        }
        let Some(h) = self.rt.clone() else {
            *self.say_slot() = Some(copy_text("rsFilewinShell.upload.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.say_slot() = Some(NO_LINE.to_string());
            return false;
        };
        let origin = self.source.origin();
        // 🔴一摞一块新看板（「进度」表里独立的一行）：上一摞还在飞也不碍事、互不清掉。
        //    新看板的取消台是新的 ⇒ 上一摞按过的取消拦不下这一摞。
        let board = self.board.fresh_like();
        self.board = board.clone();
        self.seen_rounds = 0;
        // 🔴 **把窗口交给看板**，它自己会在「有问题要问 / 进度动了 / 跑完了」时敲一下。
        //   不交的话：进度条要等用户下次动鼠标才跳一格（egui 只在有事发生时才画下一帧）。
        board.attach(ctx);
        board.set_total(items.len());
        self.drops_started += 1;
        let dir = super::source::parent_dir(&items[0].remote_path);
        let id = self.progress.add(
            super::progress::Trip::Upload {
                board: board.clone(),
                items: items.clone(),
                dir: super::source::remote_basename(&dir).to_string(),
            },
            Some(dir),
            None,
        );
        self.track_job("upload", id);
        h.spawn(async move {
            // 上传那一腿也经通道（开单 → 起跑并看 → 提交），拿同一条线 ＋ 同一个地址。
            let (up_line, up_origin) = (line.clone(), origin.clone());
            let ask_board = board.clone();
            let up_board = board.clone();
            let probe_board = board.clone();
            let out = super::transfer::run_drop(
                items,
                super::transfer::lanes(),
                move |p| {
                    let line = line.clone();
                    let origin = origin.clone();
                    let seen = probe_board.clone();
                    async move {
                        // 在 ⇒ 顺手记下那台的大小与修改时间（同名那张表「那台」那一格）。
                        let got =
                            super::transfer::stat_remote_at(&line, &origin, p.path_wire()).await;
                        if let Some(v) = &got {
                            seen.note_there(
                                &p.name,
                                v.get("size").and_then(serde_json::Value::as_u64),
                                v.get("mtime_text")
                                    .and_then(serde_json::Value::as_str)
                                    .map(str::to_string),
                            );
                        }
                        got.is_some()
                    }
                },
                move |clashes| {
                    let rx = ask_board.ask(clashes);
                    async move { rx.await.unwrap_or_default() }
                },
                move |p| {
                    let (line, origin) = (up_line.clone(), up_origin.clone());
                    let b = up_board.clone();
                    // 🔴**取消那道闸在这儿**：按过取消之后，还没起的那几件
                    //   一件都不起，而且这一趟的 `transfer_id` 由那道闸造并登记
                    //   （两件事一个落点，理由住 `launch_unless_cancelled`）。
                    async move {
                        let desk = b.cancels();
                        let name = p.name.clone();
                        super::transfer::launch_unless_cancelled(&desk, &name, |_id| async move {
                            super::transfer::upload_remote(&line, &origin, &p, &b).await
                        })
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
        // ⚠复制那一摞也在问的时候同样不接 —— 两个模态框叠起来，
        //   「一次问完」就变成「答错了哪一个都不知道」。
        // ⚠写操作那一摞同理，而它的代价更大：那个框上「都别做」与
        //   上传那个框上「全都不覆盖」叠在一起，答错一个就是删错东西。
        // 不是焦点那一个 ⇒ 不接（两栏都接的话，拖一个文件进来两栏各传一次）。
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
    // 第二段：**零流量复制**接到这一侧
    // ════════════════════════════════════════════════════════════════════

    /// 正摆着的那个「复制为」框（判据与 [`Self::copy_ui`] 用）。
    pub fn copy_prompt(&self) -> Option<&CopyPrompt> {
        self.copy_prompt.as_ref()
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
        *self.prompt_error.lock().unwrap() = None;
        let row = {
            let rows = self.listing.rows.lock().unwrap();
            match rows.get(i) {
                Some(r) if super::copy::copyable(r) => r.clone(),
                _ => return false,
            }
        };
        let mut p = CopyPrompt::for_row(&self.cwd, &row);
        // 〔有损名全寻址〕源与当前目录的字节跟着框走（合法 UTF-8 ⇒ 两格都是 `None`，与此前逐字节同）。
        p.from_raw = self.row_path(&row).raw;
        p.dir_raw = self.cwd_raw.clone();
        self.copy_prompt = Some(p);
        true
    }

    /// 收掉那个框，什么都不做。
    pub fn cancel_copy(&mut self) {
        self.copy_prompt = None;
        *self.prompt_error.lock().unwrap() = None;
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
            *self.say_slot() = Some(copy_text(
                "rsFilewinShell.copy.badName",
                &[("name", &p.new_name.to_string())],
            ));
            return false;
        };
        if !self.start_copy(job, ctx) {
            return false;
        }
        self.copy_prompt = None;
        *self.prompt_error.lock().unwrap() = None;
        true
    }

    /// 起一趟 `§5` 第二段：**先问会不会覆盖，再动手，回来把裁决摆出来。**
    ///
    /// 🔴 三段的顺序不在这里，在 [`super::copy::run_copy`] 的结构里 ——
    /// 这里只负责把「问谁 · 怎么问 · 怎么起」三个口接上去（同 [`Self::start_drop`]）。
    /// 而本函数接不上（**没运行时**）要**出声**，判据见 `shell_tests`。
    pub fn start_copy(&mut self, job: CopyJob, ctx: Option<egui::Context>) -> bool {
        let Some(h) = self.rt.clone() else {
            *self.say_slot() = Some(copy_text("rsFilewinShell.copy.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.say_slot() = Some(NO_LINE.to_string());
            return false;
        };
        let origin = self.source.origin();
        let board = self.fresh_copy_board(1, &job.name);
        self.copies_started += 1;
        // 🔴 把窗口交给看板（同 `start_drop`）：「在跑」与结局都是从 tokio 那条线程写进来的，
        //    不敲一下，屏幕要等用户下次动鼠标才更新。
        board.attach(ctx);
        h.spawn(async move {
            let (probe_line, probe_origin) = (line.clone(), origin.clone());
            let ask_board = board.clone();
            let run_board = board.clone();
            let out = super::copy::run_copy(
                job,
                // 目录那一件不问覆盖（目录复制不合并）：目标已在由后端拒、原话摆出来。
                move |j| async move {
                    !j.is_dir && super::copy::probe_target(&probe_line, &probe_origin, &j).await
                },
                move |j| {
                    let rx = ask_board.ask(j);
                    async move { rx.await.unwrap_or(false) }
                },
                // 经通道问后端 `files-copy`；第二个参数是覆盖策略（问过且答了「覆盖」）。
                //   后端这一趟取消不掉 ⇒ 不再走取消那道闸（理由住 `copy.rs` 头注）。
                move |j, overwrite| async move {
                    run_board.begin(&j.name);
                    super::copy::copy_remote(&line, &origin, &j, overwrite).await
                },
            )
            .await;
            board.finish(out);
        });
        true
    }

    /// **一摞复制到这一栏**（「复制到另一栏」的按钮与拖，`Workspace::copy_to_other` 起在目标那一栏上）：
    /// 三段在 [`super::copy::run_copy_batch`] 的结构里（逐件探 → 撞名目录整摞不做 → 撞名文件一次问完 → 逐件发），
    /// 这里只接「问谁 · 怎么问 · 怎么起」三个口。参数是跨目录那一形（[`super::workspace::across_args`]）。
    /// 〔FW34 那一版〕只收一件、只收文件；今天一件就是一摞里只有一件。
    pub fn start_copy_batch(&mut self, jobs: Vec<CopyJob>, ctx: Option<egui::Context>) -> bool {
        let Some(h) = self.rt.clone() else {
            *self.say_slot() = Some(copy_text("rsFilewinShell.copy.noRuntime", &[]));
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.say_slot() = Some(NO_LINE.to_string());
            return false;
        };
        let origin = self.source.origin();
        let first = jobs.first().map(|j| j.name.clone()).unwrap_or_default();
        let board = self.fresh_copy_board(jobs.len(), &first);
        board.attach(ctx);
        self.copies_started += 1;
        let total = jobs.len();
        h.spawn(async move {
            let (probe_line, probe_origin) = (line.clone(), origin.clone());
            let ask_board = board.clone();
            let run_board = board.clone();
            let seq = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let out = super::copy::run_copy_batch(
                jobs,
                move |j| {
                    let (l, o) = (probe_line.clone(), probe_origin.clone());
                    async move { super::copy::probe_target(&l, &o, &j).await }
                },
                move |js| {
                    let rx = ask_board.ask_many(js);
                    async move { rx.await.unwrap_or(false) }
                },
                move |j, overwrite| {
                    let (l, o, b) = (line.clone(), origin.clone(), run_board.clone());
                    let k = seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    async move {
                        b.begin(&format!("{}（{k}/{total}）", j.name));
                        super::workspace::copy_across(&l, &o, &j, overwrite).await
                    }
                },
            )
            .await;
            board.finish(out);
        });
        true
    }

    /// 机器上复制的一趟一块新看板（「进度」表里独立的一行）。
    fn fresh_copy_board(&mut self, n: usize, name: &str) -> CopyBoard {
        let board = CopyBoard::default();
        self.copy_board = board.clone();
        self.seen_copy_rounds = 0;
        let no_stop = self.no_stop(super::copy::CMD_COPY);
        let id = self.progress.add(
            super::progress::Trip::Copy {
                board: board.clone(),
                n,
                name: name.to_string(),
            },
            Some(self.cwd.clone()),
            no_stop,
        );
        self.track_job("copy", id);
        board
    }

    /// 解压跑完一趟就重列当前目录（新目录要出现在列表里）。同 [`Self::settle_finished_copies`]。
    pub fn settle_finished_extracts(&mut self) -> bool {
        let now = self.extract_board.rounds();
        if now == self.seen_extract_rounds {
            return false;
        }
        self.seen_extract_rounds = now;
        self.reload();
        true
    }

    /// 第 `i` 行那份文件「解压到这里」：发 `files-extract`，撞名就问（[`super::extract::run`]）。
    /// 接不上（没运行时 / 没通道）⇒ 出声。回值 ＝ 真的起来了。
    pub fn start_extract(&mut self, i: usize, ctx: Option<egui::Context>) -> bool {
        let Some(h) = self.rt.clone() else {
            *self.listing.error.lock().unwrap() =
                Some(copy_text("rsFilewinShell.size.noRuntime", &[]));
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string());
            return false;
        };
        let Some((rel, name)) = self
            .listing
            .rows
            .lock()
            .unwrap()
            .get(i)
            .filter(|r| !r.opens_as_dir())
            .map(|r| (super::source::wire_bytes(&name_bytes(r)), r.name.clone()))
        else {
            return false;
        };
        let root = self.cwd_path().wire();
        let origin = self.source.origin();
        // 一趟一块新看板（「进度」表里独立的一行）。
        let board = super::extract::ExtractBoard::default();
        self.extract_board = board.clone();
        self.seen_extract_rounds = 0;
        board.attach(ctx);
        board.begin(&name);
        let no_stop = self.no_stop(super::extract::CMD_EXTRACT);
        let id = self.progress.add(
            super::progress::Trip::Extract {
                board: board.clone(),
                name: name.clone(),
            },
            Some(self.cwd.clone()),
            no_stop,
        );
        self.track_job("extract", id);
        h.spawn(async move {
            let asker = board.clone();
            let o = super::extract::run(&line, &origin, &root, &rel, |said| async move {
                asker.ask(said).await.unwrap_or(false)
            })
            .await;
            board.finish(&name, o);
        });
        true
    }

    /// 摆出「复制到另一台」那一问（机器名 ＋ 目标目录，空 ＝ 那台的 home）。回值 ＝ 真的摆出来了。
    pub fn begin_cross(&mut self, i: usize) -> bool {
        *self.prompt_error.lock().unwrap() = None;
        let Some((name, src)) = self
            .listing
            .rows
            .lock()
            .unwrap()
            .get(i)
            .filter(|r| !r.opens_as_dir())
            .map(|r| (r.name.clone(), self.row_path(r)))
        else {
            return false;
        };
        // 缺省机器：除开这一台、先挑别的远端，没有才是本机（下拉里改）；落点缺省那台上次选到的，没有就那台的主目录（选择器打开时去问）。
        let here = self.source.origin().0;
        let others: Vec<&String> = self.machines.iter().filter(|m| **m != here).collect();
        let machine = others
            .iter()
            .find(|m| m.as_str() != super::cross_copy::LOCAL_ORIGIN)
            .or(others.first())
            .map(|m| super::cross_copy::shown_machine(m))
            .unwrap_or_default();
        self.cross_prompt = Some(super::cross_copy::CrossPrompt {
            name,
            src,
            machine,
            dir: String::new(),
        });
        self.cross_pick = Default::default();
        true
    }

    /// 那一问里正在填的（判据与 [`Self::cross_ui`] 用）。
    pub fn cross_prompt_mut(&mut self) -> Option<&mut super::cross_copy::CrossPrompt> {
        self.cross_prompt.as_mut()
    }

    /// 答完那一问 ⇒ 起那一趟（机器名空 ⇒ 框留着、出声）。回值 ＝ 真的起来了。
    pub fn confirm_cross(&mut self, ctx: Option<egui::Context>) -> bool {
        let Some(p) = self.cross_prompt.clone() else {
            return false;
        };
        if p.machine.trim().is_empty() {
            *self.say_slot() = Some(copy_text("rsFilewinCrossCopy.prompt.noMachine", &[]));
            return false;
        }
        // 落点 ＝ 选择器上选到的（还没读出来 ⇒ 空 ＝ 那台的主目录，`cross_copy::target_dir`）。
        let pick = self.cross_pick.shown();
        let p = if pick.machine == super::cross_copy::origin_of(&p.machine) && !pick.path.is_empty()
        {
            super::cross_copy::remember_dir(&pick.machine, &pick.target());
            super::cross_copy::CrossPrompt {
                dir: pick.target(),
                ..p
            }
        } else {
            p
        };
        let Some(h) = self.rt.clone() else {
            *self.say_slot() = Some(copy_text("rsFilewinShell.size.noRuntime", &[]));
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.say_slot() = Some(NO_LINE.to_string());
            return false;
        };
        self.cross_prompt = None;
        *self.prompt_error.lock().unwrap() = None;
        // 一趟一块新看板（「进度」表里独立的一行）。
        self.cross_board = super::cross_copy::CrossBoard::default();
        self.cross_board.attach(ctx);
        self.crosses_started += 1;
        let p = super::cross_copy::CrossPrompt {
            machine: super::cross_copy::origin_of(&p.machine),
            ..p
        };
        let id = self.progress.add(
            super::progress::Trip::Cross {
                board: self.cross_board.clone(),
                name: p.name.clone(),
                machine: super::cross_copy::shown_machine(&p.machine),
            },
            None,
            None,
        );
        self.track_job("cross", id);
        super::cross_copy::spawn(&h, line, self.source.origin(), p, self.cross_board.clone());
        true
    }

    /// 画「复制到另一台」那一问（kit 的对话框，稿 13）：「复制到」下拉（别的机器 · 本机）·「放到」一块小目录选择器
    /// （面包屑 ＋ 那台的文件夹，单击选中、双击进去，右上「新建文件夹」）·「目标 gpu-01:/home/user/inbox」· ［取消］［复制到 gpu-01］。
    fn cross_ui(&mut self, ui: &mut egui::Ui) {
        let Some(mut p) = self.cross_prompt.clone() else {
            return;
        };
        let pal = super::theme::palette(ui.ctx());
        let machine = super::cross_copy::origin_of(&p.machine);
        // 选择器跟着下拉那台走：换了机器（或刚打开）⇒ 去那台上次的落点 / 主目录。
        let pick = self.cross_pick_follow(ui, &machine);
        let here = self.source.origin().0;
        let machines: Vec<String> = self
            .machines
            .iter()
            .filter(|m| **m != here)
            .map(|m| super::cross_copy::shown_machine(m))
            .collect();
        let err = self.prompt_error();
        let mut go_to: Option<String> = None;
        let mut picked: Option<String> = None;
        let mut mkdir = false;
        let shown_machine = p.machine.clone();
        let hit = super::kit::dialog(
            ui.ctx(),
            "filewin-cross-copy",
            &copy_text("rsFilewinCrossCopy.prompt.heading", &[("name", &p.name)]),
            |ui| {
                ui.label(
                    egui::RichText::new(copy_text("rsFilewinCrossCopy.prompt.machine", &[]))
                        .color(pal.text2),
                );
                let w = ui.available_width();
                egui::ComboBox::from_id_salt("filewin-cross-machine")
                    .width(w)
                    .selected_text(format!(
                        "{}  {}",
                        egui_phosphor::regular::DESKTOP,
                        shown_machine
                    ))
                    .show_ui(ui, |ui| {
                        for m in &machines {
                            ui.selectable_value(&mut p.machine, m.clone(), m);
                        }
                    });
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(copy_text("rsFilewinCrossCopy.prompt.dir", &[]))
                        .color(pal.text2),
                );
                egui::Frame::new()
                    .fill(pal.bg)
                    .corner_radius(8.0)
                    .stroke(egui::Stroke::new(1.0, pal.border_soft))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        cross_crumbs(ui, &pal, &pick, &mut go_to, &mut mkdir);
                        ui.separator();
                        cross_dirs(ui, &pal, &pick, &mut go_to, &mut picked);
                    });
                ui.add_space(6.0);
                if !pick.path.is_empty() {
                    let target = copy_text(
                        "rsFilewinCrossCopy.prompt.hint",
                        &[("machine", &shown_machine), ("path", &pick.target())],
                    );
                    ui.add(
                        egui::Label::new(egui::RichText::new(&target).size(12.0).color(pal.text2))
                            .truncate(),
                    )
                    .on_hover_text(target);
                }
                if let Some(e) = &err {
                    ui.label(egui::RichText::new(e).size(12.0).color(pal.error_text));
                }
            },
            &[
                (
                    copy_text("rsFilewinShell.copyUi.cancel", &[]),
                    super::kit::Btn::Plain,
                ),
                (
                    copy_text(
                        "rsFilewinCrossCopy.prompt.go",
                        &[("machine", &shown_machine)],
                    ),
                    super::kit::Btn::Primary,
                ),
            ],
            1,
            0,
        );
        self.cross_after(
            ui,
            p,
            &pick,
            hit,
            CrossClicks {
                picked,
                go_to,
                mkdir,
            },
        );
    }

    /// 那一问画完之后：选中 / 进去 / 新建文件夹 · 取消 · 确认。
    fn cross_after(
        &mut self,
        ui: &egui::Ui,
        p: super::cross_copy::CrossPrompt,
        pick: &super::cross_copy::Pick,
        hit: Option<usize>,
        clicks: CrossClicks,
    ) {
        let CrossClicks {
            picked,
            go_to,
            mkdir,
        } = clicks;
        if let Some(n) = picked {
            self.cross_pick.pick(&n);
        }
        if let (Some(h), Some(line)) = (self.rt.clone(), self.line.clone()) {
            if let Some(to) = go_to {
                self.cross_pick
                    .go(&h, &line, &pick.machine, &to, Some(ui.ctx().clone()));
            } else if mkdir {
                self.cross_pick.mkdir(&h, &line, Some(ui.ctx().clone()));
            }
        }
        self.cross_prompt = Some(p);
        match hit {
            Some(0) => {
                self.cross_prompt = None;
                *self.prompt_error.lock().unwrap() = None;
            }
            Some(_) => {
                let ctx = ui.ctx().clone();
                self.confirm_cross(Some(ctx));
            }
            None => {}
        }
    }

    /// 「放到」那块选择器跟着下拉那台走：换了机器（或刚打开）⇒ 去那台上次的落点 / 主目录。交回此刻摆的那一份。
    fn cross_pick_follow(&mut self, ui: &egui::Ui, machine: &str) -> super::cross_copy::Pick {
        let mut pick = self.cross_pick.shown();
        if !machine.is_empty() && pick.machine != *machine {
            if let (Some(h), Some(line)) = (self.rt.clone(), self.line.clone()) {
                let start = super::cross_copy::last_dir(machine).unwrap_or_default();
                self.cross_pick
                    .go(&h, &line, machine, &start, Some(ui.ctx().clone()));
                pick = self.cross_pick.shown();
            }
        }
        pick
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
        let (_, esc) = modal(ui.ctx(), "filewin-copy-as", |ui| {
            ui.heading(copy_text(
                "rsFilewinShell.copyUi.heading",
                &[("name", &p.src_name.to_string())],
            ));
            go |= prompt_field(ui, &mut p.new_name);
            self.prompt_error_ui(ui);
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
        if cancel || esc {
            self.cancel_copy();
        } else if go {
            let ctx = ui.ctx().clone();
            self.confirm_copy(Some(ctx));
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // 🔴：**新建目录 · 改名 · 删除 · 改权限**
    // ════════════════════════════════════════════════════════════════════

    /// 正摆着的那个框（判据与 [`Self::write_ui`] 用）。
    pub fn write_prompt(&self) -> Option<&WritePrompt> {
        self.write_prompt.as_ref()
    }

    /// 同 [`Self::write_prompt`]，可改（就地那一格里正在编辑的字）。
    pub fn write_prompt_mut(&mut self) -> Option<&mut WritePrompt> {
        self.write_prompt.as_mut()
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
        self.begin_inline(WritePrompt::for_new(&self.cwd, false))
    }

    /// 摆出就地那一格（改名 · 新建目录 · 新建空文件）：下一帧焦点给它、选中主名 / 整个名字。
    pub(super) fn begin_inline(&mut self, p: WritePrompt) -> bool {
        *self.prompt_error.lock().unwrap() = None;
        self.inline_select = Some(p.select_chars());
        self.inline_busy = false;
        self.inline_gen += 1;
        self.write_prompt = Some(p);
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
        self.begin_inline(WritePrompt::for_rename(&self.cwd, &row))
    }

    /// 摆出「把第 `i` 行的权限改成」那个框。回值 = 真的摆出来了。
    pub fn begin_chmod(&mut self, i: usize) -> bool {
        self.begin_chmod_rows(&[i])
    }

    /// 摆出「把这几行的权限改成」那个框（批量改权限；一行时与 [`Self::begin_chmod`] 同一个框）。
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
        *self.prompt_error.lock().unwrap() = None;
        self.write_prompt = Some(WritePrompt::for_chmod_many(&self.cwd, &refs));
        // 框一摆出来就逐项问现值（`files-stat` 的 `mode`）；答回来之后框上说、只预填一次。
        let paths: Vec<String> = rows.iter().map(|r| r.path.clone()).collect();
        self.start_mode_probe(paths);
        !refs.is_empty()
    }

    /// 起「现值」那一趟。没有运行时 / 没有通道 ⇒ 当场落「全读不到」（框上说「读不到现在的权限」，不猜、不静默）。
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
        self.describe_for_delete(&row);
        self.start_writes(vec![super::writeops::delete_op(&row)], ctx)
    }

    /// 第 `i` 行，且它**能被写**。`None` ⇒ 不接（越界 / 有损名）。
    ///
    /// ⚠ 从前这句话是「越界 / 有损名 / **本机源**，已出声」—— 本机源那一档不在了
    /// （同 [`Self::begin_mkdir`] 那条）。它不是**静默**掉的：那个状态写不出来。
    fn writable_row(&mut self, i: usize) -> Option<super::source::Listed> {
        let rows = self.listing.rows.lock().unwrap();
        match rows.get(i) {
            // 交出去的是**整个 `Listed`**：此前只交那五格（链接与时间写操作不读），
            //   现在写操作要读 `raw_name`（有损名的原始字节，改名 · 删除 · 改权限都走它）。
            Some(r) if is_writable(r) => Some(r.clone()),
            _ => None,
        }
    }

    /// 收掉那个框，什么都不做。
    pub fn cancel_write(&mut self) {
        self.write_prompt = None;
        self.inline_busy = false;
        self.inline_gen += 1;
        *self.prompt_error.lock().unwrap() = None;
    }

    /// 框里那几个字 → 一摞真操作。回值 = 真的起来了。
    ///
    /// ⚠ 输入不合法 ⇒ **框留着、出声**，不静默收掉（同 [`Self::confirm_copy`]：
    /// 收掉的话用户点了确认什么都没发生，与成功长得一模一样）。
    pub fn confirm_write(&mut self, ctx: Option<egui::Context>) -> bool {
        let Some(p) = self.write_prompt.clone() else {
            return false;
        };
        if p.is_inline() {
            return self.commit_inline(ctx);
        }
        // 批量改权限那个框一次出 N 件 ⇒ `to_ops`（新建目录 / 改名恒一件）。
        let ops = match p.to_ops() {
            Ok(ops) => ops,
            Err(why) => {
                *self.say_slot() = Some(why);
                return false;
            }
        };
        if !self.start_writes(ops, ctx) {
            return false;
        }
        self.write_prompt = None;
        *self.prompt_error.lock().unwrap() = None;
        true
    }

    /// 就地那一格答完（回车 / 点别处）⇒ 发那一趟；那一格留着、只读，回来了再收（[`Self::settle_inline`]）。
    /// 填错 ⇒ 那一格下面说一句、留着；改名改成原名 ⇒ 当没改（不报错）。回值 ＝ 真的发出去了。
    pub fn commit_inline(&mut self, ctx: Option<egui::Context>) -> bool {
        let Some(p) = self.write_prompt.clone() else {
            return false;
        };
        if self.inline_busy {
            return false;
        }
        let go = match p.to_inline() {
            Ok(Some(go)) => go,
            Ok(None) => {
                self.cancel_write();
                return false;
            }
            Err(why) => {
                *self.prompt_error.lock().unwrap() = Some(why);
                self.inline_select = Some(p.select_chars());
                return false;
            }
        };
        let Some(h) = self.rt.clone() else {
            *self.prompt_error.lock().unwrap() =
                Some(copy_text("rsFilewinShell.writes.noRuntime", &[]));
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.prompt_error.lock().unwrap() = Some(NO_LINE.to_string());
            return false;
        };
        *self.prompt_error.lock().unwrap() = None;
        self.inline_busy = true;
        self.inline_gen += 1;
        let gen = self.inline_gen;
        let origin = self.source.origin();
        let root_raw = self.cwd_raw.clone();
        let cwd = self.cwd_path();
        let slot = self.inline_done.clone();
        let name = super::writeops::clean_name(&p.text);
        h.spawn(async move {
            let got = match go {
                super::writeops::InlineGo::Op(op) => {
                    super::writeops::apply_typed(&line, &origin, &op, root_raw.as_deref(), &name)
                        .await
                        .map(|_| ())
                }
                super::writeops::InlineGo::Create(_) => {
                    super::create::create_typed_coded(&line, &origin, &cwd, &name).await
                }
            };
            *slot.lock().unwrap() = Some((gen, got.map(|()| name)));
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
        true
    }

    /// 就地那一趟回来了：成了 ⇒ 收那一格、重列、选中新名字（改名另给带［撤销］的回执）；
    /// 名字被占了 ⇒ 那一格下面说「x 已存在」、焦点回去；别的没成 ⇒ 那一格下面说那句话。回值 ＝ 收到了。
    pub fn settle_inline(&mut self) -> bool {
        let Some((gen, got)) = self.inline_done.lock().unwrap().take() else {
            return false;
        };
        if gen != self.inline_gen {
            return false;
        }
        self.inline_busy = false;
        match got {
            Ok(name) => {
                if let Some(p) = self.write_prompt.take() {
                    if let (PromptKind::Rename { from, raw: None }, None) = (&p.kind, &self.cwd_raw)
                    {
                        let to = super::writeops::join_remote(&p.dir, &name);
                        self.receipt_undo = Some((
                            copy_text("rsFilewinWriteops.inline.renamed", &[("name", &name)]),
                            vec![WriteOp::Rename {
                                from: to,
                                to: from.clone(),
                                raw: None,
                            }],
                        ));
                    }
                }
                *self.prompt_error.lock().unwrap() = None;
                self.selection.clear_picked();
                self.set_reveal(&name);
                self.reload();
            }
            Err(f) => {
                let why = if f.code.as_deref() == Some(super::writeops::EXISTS) {
                    let name = self
                        .write_prompt
                        .as_ref()
                        .map(|p| p.text.trim().to_string())
                        .unwrap_or_default();
                    copy_text("rsFilewinWriteops.inline.exists", &[("name", &name)])
                } else {
                    f.said
                };
                *self.prompt_error.lock().unwrap() = Some(why);
                self.inline_select = self.write_prompt.as_ref().map(WritePrompt::select_chars);
            }
        }
        true
    }

    /// 回执上点了［撤销］⇒ 那几件直接做（不问、不再出回执），做完重列。
    pub fn start_undo(&mut self, ops: Vec<WriteOp>, ctx: Option<egui::Context>) -> bool {
        self.start_writes_as(ops, ctx, true)
    }

    /// 起一摞 `§4.6.4`：**一次问完，才动手。**（不再先过围栏）
    ///
    /// 🔴 三段的顺序不在这里，在 [`super::writeops::run_writes`] 的结构里 ——
    /// 这里只负责把「怎么问 · 怎么做」两个口接上去（同 [`Self::start_drop`]）。
    /// 而本函数接不上（**没运行时**）要**出声**，判据见 `shell_tests`。
    pub fn start_writes(&mut self, ops: Vec<WriteOp>, ctx: Option<egui::Context>) -> bool {
        self.start_writes_as(ops, ctx, false)
    }

    /// 同 [`Self::start_writes`]；`quiet` ⇒ 这一摞是撤销本身（做完不再出回执）。
    fn start_writes_as(
        &mut self,
        ops: Vec<WriteOp>,
        ctx: Option<egui::Context>,
        quiet: bool,
    ) -> bool {
        if ops.is_empty() {
            return false;
        }
        let Some(h) = self.rt.clone() else {
            *self.say_slot() = Some(copy_text("rsFilewinShell.writes.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.say_slot() = Some(NO_LINE.to_string());
            return false;
        };
        let origin = self.source.origin();
        let board = self.write_board.clone();
        board.attach(ctx);
        // 这一摞里有文件夹要删 ⇒ 「进度」里一行（删大目录要一会儿）；只删文件的一下就完，不占一行。
        let deletes: Vec<&WriteOp> = ops
            .iter()
            .filter(|o| matches!(o, WriteOp::Delete { .. }))
            .collect();
        if deletes
            .iter()
            .any(|o| matches!(o, WriteOp::Delete { is_dir: true, .. }))
        {
            let name = match deletes.first() {
                Some(WriteOp::Delete { path, .. }) => {
                    super::source::remote_basename(path).to_string()
                }
                _ => String::new(),
            };
            let no_stop = self.no_stop("files-delete");
            let id = self.progress.add(
                super::progress::Trip::Delete {
                    board: board.clone(),
                    base: board.rounds(),
                    name,
                    n: deletes.len(),
                },
                Some(self.cwd.clone()),
                no_stop,
            );
            self.track_job("delete", id);
        }
        self.writes_started += 1;
        // 〔有损名全寻址〕写的对象恒是当前目录的直接子项 ⇒ 根就是当前目录；有损 ⇒ 根发字节。
        // 撤销那一摞带的是整条路径（字符串）⇒ 根照路径切，不借这一栏的字节根；当前目录是有损名 ⇒ 不给撤销（字符串寻址不到）。
        let root_raw = if quiet { None } else { self.cwd_raw.clone() };
        let undoable = root_raw.is_none();
        // 做成了的那几件 ＋ 撤销它们的那几件（改权限：后端回的 `before`）—— 回执据它说、［撤销］据它做。
        let kept: Arc<Mutex<(Vec<WriteOp>, Vec<WriteOp>)>> = Arc::default();
        let sink = kept.clone();
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
                    let root_raw = root_raw.clone();
                    let sink = sink.clone();
                    async move {
                        let reply = super::writeops::apply_remote_coded(
                            &line,
                            &origin,
                            &op,
                            root_raw.as_deref(),
                        )
                        .await
                        .map_err(|f| f.said)?;
                        let mut k = sink.lock().unwrap();
                        if let Some(u) =
                            super::writeops::chmod_undo(&op, &reply).filter(|_| undoable)
                        {
                            k.1.push(u);
                        }
                        k.0.push(op);
                        Ok(())
                    }
                },
            )
            .await;
            let (done, undo) = std::mem::take(&mut *kept.lock().unwrap());
            board.finish_with(out, done, undo, quiet);
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
        // 做完的回执（删除 · 改权限 ＋［撤销］· 没做成的那一句）⇒ 窗口那一级摆成右下角回执。
        if let Some((text, undo)) = self.write_board.take_receipt() {
            if undo.is_empty() {
                self.receipt = Some(text);
            } else {
                self.receipt_undo = Some((text, undo));
            }
        }
        // 删掉 / 改了名的那几个名字已经不在了 ⇒ 选中清掉（光标留着，理由住 `Selection::clear_picked`）。
        self.selection.clear_picked();
        self.reload();
        true
    }

    // ═══════════════════════════════════════════════════════════════════
    // 🔴往外拖：**两问，然后拉**
    // ═══════════════════════════════════════════════════════════════════

    /// 系统存盘框开着时要下的那一项（判据看同一个值）。
    pub fn pull_want(&self) -> Option<&(String, String)> {
        self.pull_want.as_ref()
    }

    /// 〔「原生选文件框」〕起一趟原生选择框（在 tokio 那条线程上，不堵 UI 线程）。
    /// 上传 ⇒ 选一个或几个文件；下载 ⇒ 选保存位置（建议名 ＝ 那一问里现在的名字）。接不上（没运行时）⇒ 说一句。回值 ＝ 真的起了。
    pub fn start_pick(
        &mut self,
        purpose: super::picker::Purpose,
        ctx: Option<egui::Context>,
    ) -> bool {
        use super::picker::{PickKind, Purpose};
        let kind = match purpose {
            Purpose::Upload => PickKind::OpenFiles,
            // 建议名按本机平台改成合法的（Windows 不认的字换成 `_`），落在系统的「下载」文件夹。
            Purpose::Download => PickKind::SaveFile {
                suggested_name: self
                    .pull_want
                    .as_ref()
                    .map(|(_, n)| super::download::local_name(n, crate::platform::WINDOWS_NAMES))
                    .unwrap_or_default(),
            },
        };
        let Some(h) = self.rt.clone() else {
            *self.say_slot() = Some(copy_text("rsFilewinPicker.pick.noRuntime", &[]));
            return false;
        };
        let board = self.pick_board.clone();
        board.attach(ctx);
        let picker = self.picker.clone();
        h.spawn(async move {
            let got = picker.pick(kind).await;
            board.deliver(purpose, got);
        });
        true
    }

    /// 选择框的结局落回那一问：上传 ⇒ 路径接进框里；下载 ⇒ 保存位置填进「存到哪儿」。没选到 ⇒ 那一问上说一句、框不动。
    pub fn settle_pick(&mut self) -> bool {
        self.settle_pick_with(None)
    }

    /// 选择框的结局：上传 ⇒ 选到的那几份走拖入那一条；下载 ⇒ 起那一趟（覆盖由系统存盘框自己问过了）；没选（取消）⇒ 什么都不做。
    pub fn settle_pick_with(&mut self, ctx: Option<egui::Context>) -> bool {
        use super::picker::Purpose;
        let Some((purpose, picked)) = self.pick_board.take() else {
            return false;
        };
        match purpose {
            Purpose::Upload => {
                let paths: Vec<String> = picked
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                if !paths.is_empty() {
                    let items = self.pending_for(&paths);
                    self.start_drop(items, ctx);
                }
            }
            Purpose::Download => {
                let want = self.pull_want.take();
                match (want, picked.and_then(|v| v.into_iter().next())) {
                    (Some((src, _)), Some(dest)) => {
                        self.start_pull(&src, &dest.to_string_lossy(), true, ctx);
                    }
                    _ => self.pull_raw = None,
                }
            }
        }
        true
    }

    pub fn begin_pull(&mut self, i: usize) -> bool {
        let Some(row) = self.listing.rows.lock().unwrap().get(i).cloned() else {
            return false;
        };
        self.begin_pull_row(row)
    }

    /// 同 [`Self::begin_pull`]，那一行直接给（编辑页「下载到本机」走这里：它手上没有列表）。
    pub fn begin_pull_row(&mut self, row: Listed) -> bool {
        *self.prompt_error.lock().unwrap() = None;
        // 有损名带着字节 / 有损目录里的行也拉得下来（按字节，`lossy_pull.rs`）。
        if !super::download::is_downloadable_listed(&row) {
            return false;
        }
        let full = self.row_path(&row);
        self.pull_raw = full
            .is_lossy()
            .then(|| (full.bytes(), row.name.clone(), name_bytes(&row)));
        self.pull_want = Some((row.path.clone(), row.name.clone()));
        self.start_pick(super::picker::Purpose::Download, None)
    }

    /// 收掉那个框，什么都不做。
    fn pull_local(&self, dest: &str) -> (serde_json::Value, Option<String>) {
        match &self.pull_raw {
            Some((_, shown, raw)) => super::lossy_pull::local_dest(dest, shown, raw),
            None => (serde_json::Value::String(dest.to_string()), None),
        }
    }

    /// 答完当前这一问 → 下一步。回值 = **这一下真的推进了**。
    ///
    /// 🔴 三支各自的下一跳完全不同（理由住 `download::DestVerdict` 的头注）：
    /// 不合法 ⇒ **框留着 ＋ 出声**（收掉的话「点了确认什么都没发生」与成功同形）；
    /// 要确认 ⇒ 换成第二问；可以做 ⇒ 起那一趟并收掉框。
    pub fn start_pull(
        &mut self,
        src_path: &str,
        dest: &str,
        overwrite: bool,
        ctx: Option<egui::Context>,
    ) -> bool {
        let Some(h) = self.rt.clone() else {
            *self.say_slot() = Some(copy_text("rsFilewinShell.pull.noRuntime", &[]).into());
            return false;
        };
        // 下载经通道开单、订阅进度 —— 要那条线 ＋ 那台机器的地址。
        let Some(line) = self.line.clone() else {
            *self.say_slot() = Some(NO_LINE.to_string());
            return false;
        };
        let origin = self.source.origin();
        // 一趟一块新看板（「进度」表里独立的一行）：上一趟还在下也不碍事。
        let board = super::download::DownloadBoard::default();
        self.pull = board.clone();
        board.attach(ctx);
        let src = src_path.to_string();
        let to = dest.to_string();
        let id = self.progress.add(
            super::progress::Trip::Download {
                board: board.clone(),
                name: super::source::remote_basename(&src).to_string(),
                src: src.clone(),
                dest: to.clone(),
            },
            None,
            None,
        );
        self.track_job("download", id);
        // 有损名：远端按字节（就地拷进暂存区再下）、本机按平台落名；否则与此前逐字同一条路。
        let (local, note) = self.pull_local(dest);
        let raw_src = self.pull_raw.take().map(|(b, _, _)| b);
        board.set_note(note);
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
                let (local, raw_src) = (local.clone(), raw_src.clone());
                async move {
                    match raw_src {
                        Some(bytes) => {
                            super::lossy_pull::pull_by_bytes(
                                &line, &origin, &bytes, local, overwrite, &b,
                            )
                            .await
                        }
                        None => super::download::pull_one(&line, &origin, &src, &to, &b).await,
                    }
                }
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
    // 🔴改一份远端文本
    // ═══════════════════════════════════════════════════════════════════

    /// 打开着的那一份（`None` = 没在编辑）。判据与界面看同一个值。
    pub fn editing(&self) -> Option<&super::editor::Pane> {
        self.editing.as_ref()
    }

    /// 打开着的那一份（可改：截图与判据摆「盘上被改过」那一形用）。
    pub fn editing_mut(&mut self) -> Option<&mut super::editor::Pane> {
        self.editing.as_mut()
    }

    /// 编辑框里那些字（生产那个 `TextEdit` 要的 `&mut String` 从这儿出来）。
    ///
    /// ⚠ **它是生产代码，不是测试钩子**（编辑面那个 `TextEdit` 拿的就是这一个 `&mut`）。
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
        let Some((row, at, refused)) = self.listing.rows.lock().unwrap().get(i).map(|r| {
            (
                r.clone(),
                self.row_path(r),
                super::editor::why_not_editable_listed(r),
            )
        }) else {
            return false;
        };
        // 挂在窗口里的目录页：交给窗口开一个编辑页（同一份已开着 ⇒ 切过去），这一页还是目录。
        if self.in_workspace && !self.edit_tab {
            self.want_edit = Some(EditWant { row, at, refused });
            return true;
        }
        self.begin_edit_at(row, at, refused, ctx)
    }

    /// 读一份来编辑（编辑页自己调；单独建的目录视图也走这里，判据那一形）。
    /// 本地预判做不了 ⇒ 编辑页中央说那一句（不是编辑页 ⇒ 列表上方说）。
    pub fn begin_edit_at(
        &mut self,
        listed: Listed,
        at: super::source::RemotePath,
        refused: Option<String>,
        ctx: Option<egui::Context>,
    ) -> bool {
        // 读着一份时不起第二趟：后到的那一份会把正在改的那一份换掉。
        if let Some(p) = self.edits.open_pending() {
            *self.listing.error.lock().unwrap() = Some(copy_text(
                "rsFilewinShell.edit.stillOpening",
                &[("path", &p)],
            ));
            return false;
        }
        self.edit_row = Some(listed.clone());
        let row = listed.row.clone();
        // 🔴 本地预判 —— 出声，不灰置。
        if let Some(why) = refused {
            let said = copy_text(
                "rsFilewinShell.edit.refused",
                &[("name", &row.name.to_string()), ("why", &why.to_string())],
            );
            if self.edit_tab {
                self.edit_refused = Some(said);
            } else {
                *self.listing.error.lock().unwrap() = Some(said);
            }
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
        self.edit_raw = at.raw.clone().map(|b| (row.path.clone(), b));
        h.spawn(async move {
            use super::editor::Arrived;
            // 读文本经通道问后端（`files-read-text`），不再拨 SFTP。路径按字节发。
            let got = super::editor::read_text_at(&line, &origin, &at).await;
            board.deliver(match got {
                Ok(Some(o)) => Arrived::Text {
                    path: row.path.clone(),
                    name: row.name.clone(),
                    text: o.text,
                    sha256: o.sha256,
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
            Arrived::Text {
                path,
                name,
                text,
                sha256,
            } => {
                let mut pane = super::editor::Pane::opened(&path, &name, text, sha256);
                // 有损名：读的时候记下的字节交给编辑面（存盘走它）。
                pane.raw_path = self
                    .edit_raw
                    .take()
                    .filter(|(p, _)| *p == path)
                    .map(|(_, b)| b);
                self.editing = Some(pane);
            }
            Arrived::NotText { path } => {
                let said = super::editor::not_text_notice(&path);
                if self.edit_tab {
                    self.edit_refused = Some(said);
                } else {
                    *self.listing.error.lock().unwrap() = Some(said);
                }
            }
            Arrived::Failed { path, why } => {
                let said = copy_text(
                    "rsFilewinShell.edit.readFailed",
                    &[("path", &path.to_string()), ("why", &why.to_string())],
                );
                if self.edit_tab {
                    self.edit_refused = Some(said);
                } else {
                    *self.listing.error.lock().unwrap() = Some(said);
                }
            }
        }
        true
    }

    /// 存回去。回值 = **真的发出去了**。
    pub fn save_edit(&mut self, ctx: Option<egui::Context>) -> bool {
        // 上一趟还没回来：再发一趟带的是旧摘要，回来就是一场假冲突。
        if self.edits.save_pending() {
            return false;
        }
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
            let at = super::source::RemotePath::of(&p.path, p.raw_path.as_deref());
            let r =
                super::editor::write_text_at(&line, &origin, &at, &p.text, p.expect_sha256()).await;
            board.deliver_save(r);
        });
        true
    }

    /// 存盘撞上「盘上那份在你打开之后被改过了」之后，人点了**仍然覆盖**。回值 = 真的发出去了。
    ///
    /// 先重读一趟拿盘上此刻那一份的摘要、再以它为 `expect` 存（`editor::overwrite_anyway`）—— CAS 仍在。
    pub fn overwrite_edit(&mut self, ctx: Option<egui::Context>) -> bool {
        if self.edits.save_pending() {
            return false;
        }
        let Some(p) = self.editing.clone() else {
            return false;
        };
        if p.over_cap() {
            return self.save_edit(ctx);
        }
        let (Some(h), Some(line)) = (self.rt.clone(), self.line.clone()) else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string());
            return false;
        };
        let origin = self.source.origin();
        let board = self.edits.clone();
        board.attach(ctx);
        board.begin_save(&p.path);
        h.spawn(async move {
            let at = super::source::RemotePath::of(&p.path, p.raw_path.as_deref());
            let r = super::editor::overwrite_anyway_at(&line, &origin, &at, &p.text).await;
            board.deliver_save(r);
        });
        true
    }

    /// 存盘撞上 stale 之后，人点了**丢掉我的改动、重新打开**：编辑面收掉、同一份重读一遍。
    /// 回值 = 真的发出去了（拿不到运行时 / 通道 ⇒ 编辑面照旧留着，一个字不丢）。
    pub fn reopen_edit(&mut self, ctx: Option<egui::Context>) -> bool {
        if self.edits.save_pending() {
            return false;
        }
        let Some(p) = self.editing.as_ref() else {
            return false;
        };
        let (path, name) = (p.path.clone(), p.name.clone());
        let at = super::source::RemotePath::of(&path, p.raw_path.as_deref());
        self.edit_raw = at.raw.clone().map(|b| (path.clone(), b));
        let (Some(h), Some(line)) = (self.rt.clone(), self.line.clone()) else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string());
            return false;
        };
        self.discard_edit();
        let origin = self.source.origin();
        let board = self.edits.clone();
        board.attach(ctx);
        board.begin_open(&path);
        h.spawn(async move {
            use super::editor::Arrived;
            let got = super::editor::read_text_at(&line, &origin, &at).await;
            board.deliver(match got {
                Ok(Some(o)) => Arrived::Text {
                    path,
                    name,
                    text: o.text,
                    sha256: o.sha256,
                },
                Ok(None) => Arrived::NotText { path },
                Err(why) => Arrived::Failed { path, why },
            });
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
            Ok(s) => {
                p.mark_saved(&s.sent, s.sha256);
                // 那一问答了「保存」：存成了（且路上没再敲字）⇒ 关这一页。
                if self.close_after_save && !p.dirty() {
                    self.want_close = true;
                }
            }
            Err(super::editor::SaveError::Stale(why)) => p.mark_stale(why),
            Err(super::editor::SaveError::Failed(why)) => p.mark_failed(why),
        }
        self.close_after_save = false;
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

    /// 🔴画**编辑面**：在飞指示 ＋ 那一份文本 ＋ 存的结局 ＋ 关窗那一问。
    ///
    /// ⚠ 它是模态的（同别的几摞）：一份文本改着的时候不该同时去改目录结构。
    /// 〔「查找替换」〕从编辑框此刻的光标 / 选区起找查找框里那几个字（往后找从选区尾起、往前找从选区头起，
    /// 到头绕回）；找到 ⇒ 把它选中、焦点给编辑框；没找到 ⇒ 查找栏上说一句。大文件模式没有这件事（`§5.5`）。回值 ＝ 找到了。
    pub fn find_in_editor(&mut self, ctx: &egui::Context, backward: bool) -> bool {
        let Some(p) = self.editing.as_mut() else {
            return false;
        };
        let id = super::bigfile::normal_editor_id(&p.path);
        if p.big.is_big() {
            return false;
        }
        let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        let (a, b) = st.cursor.char_range().map_or((0, 0), |r| {
            let r = r.as_sorted_char_range();
            (r.start.0, r.end.0)
        });
        let from = if backward { a } else { b };
        match super::editor::find_from(&p.text, &p.find.needle, from, backward) {
            Some((s, e)) => {
                st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(s),
                    egui::text::CCursor::new(e),
                )));
                st.store(ctx, id);
                ctx.memory_mut(|m| m.request_focus(id));
                p.find.notice = None;
                p.reveal = true;
                true
            }
            None => {
                p.find.notice = Some(copy_text(
                    "rsFilewinShell.editor.findNone",
                    &[("needle", &p.find.needle.to_string())],
                ));
                false
            }
        }
    }

    /// 编辑面查找栏那一格（判据与界面同一个口）。没开编辑面 ⇒ `None`。
    pub fn find_bar_mut(&mut self) -> Option<&mut super::editor::FindBar> {
        self.editing.as_mut().map(|p| &mut p.find)
    }

    /// 替换：`all` ⇒ 全文替换，说换了几处；否则 ⇒ 选区恰好是查找串就换掉它，再找下一个（不是 ⇒ 只找下一个）。
    /// 回值 ＝ 换了几处。改的是编辑框那一份字（与敲键同一个 `String`）；存盘照旧要点「存」。
    pub fn replace_in_editor(&mut self, ctx: &egui::Context, all: bool) -> usize {
        let Some(p) = self.editing.as_mut() else {
            return 0;
        };
        let id = super::bigfile::normal_editor_id(&p.path);
        if p.big.is_big() || p.find.needle.is_empty() {
            return 0;
        }
        if all {
            let (t, n) = super::editor::replace_all(&p.text, &p.find.needle, &p.find.with);
            p.text = t;
            p.find.notice = Some(if n == 0 {
                copy_text(
                    "rsFilewinShell.editor.findNone",
                    &[("needle", &p.find.needle.to_string())],
                )
            } else {
                copy_text(
                    "rsFilewinShell.editor.replacedAll",
                    &[("n", &n.to_string())],
                )
            });
            return n;
        }
        let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        let (a, b) = st.cursor.char_range().map_or((0, 0), |r| {
            let r = r.as_sorted_char_range();
            (r.start.0, r.end.0)
        });
        let picked: String = p.text.chars().skip(a).take(b.saturating_sub(a)).collect();
        let mut n = 0;
        if a < b && picked == p.find.needle {
            p.text = super::editor::replace_chars(&p.text, a, b, &p.find.with);
            let after = a + p.find.with.chars().count();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(after),
            )));
            st.store(ctx, id);
            n = 1;
        }
        self.find_in_editor(ctx, false);
        n
    }

    /// 🔴画**编辑页**（稿 ⑤：一份文本一个标签页；高度只随窗口变、绝不随内容长）：
    /// 头条（面包屑 · 状态字 · 查找 · 保存 · ⋯）→ 条（盘上被改过 · 保存失败 · 断线）→ 编辑面（行号槽 ＋ 正文，横竖都在面里滚）；
    /// 查找条浮在编辑面右上；底条那一行住窗口的状态栏（[`Self::editor_status`]）。
    /// 键：Ctrl+S 保存（在存着时不起第二趟）· Ctrl+F 查找条 · Esc 只收查找条，**从不关编辑页**。
    fn edit_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let big = self.editing.as_ref().is_some_and(|e| e.big.is_big());
        let mut acts = EditActs {
            save: self.edit_keys(ui, big),
            ..EditActs::default()
        };
        let f = self.edit_facts(big);
        self.edit_head(ui, &f, &mut acts);
        if acts.find_toggle {
            self.find_open = !self.find_open;
            if self.find_open {
                ui.memory_mut(|m| m.request_focus(egui::Id::new(FIND_ID)));
            }
        }
        self.edit_banners(ui, &f, &mut acts);
        let body = self.edit_surface(ui, &mut acts);
        self.edit_find_bar(&ctx, body, big);
        self.edit_discard_ask(&ctx, &f.name, &mut acts);
        self.apply_edit_acts(&ctx, &acts);
    }

    /// 编辑页的键：Ctrl+S（交回「要存」）· Ctrl+F 开查找条 · Esc 只收查找条。
    fn edit_keys(&mut self, ui: &mut egui::Ui, big: bool) -> bool {
        let save = ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S));
        if !big && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
            self.find_open = true;
            ui.memory_mut(|m| m.request_focus(egui::Id::new(FIND_ID)));
        }
        if self.find_open
            && !self.asking_discard
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.find_open = false;
        }
        save
    }

    /// 这一帧编辑页画的是哪一份、什么状态。
    fn edit_facts(&self, big: bool) -> EditFacts {
        let pane = self.editing.clone();
        let path = pane
            .as_ref()
            .map(|e| e.path.clone())
            .or_else(|| self.edits.opening())
            .or_else(|| self.edit_row.as_ref().map(|r| r.path.clone()))
            .unwrap_or_else(|| self.cwd.clone());
        let name = super::source::remote_basename(&path).to_string();
        let idle = !self.edits.save_pending();
        let dirty = pane.as_ref().is_some_and(super::editor::Pane::dirty);
        let over = pane.as_ref().is_some_and(super::editor::Pane::over_cap);
        EditFacts {
            pane,
            path,
            name,
            idle,
            dirty,
            over,
            big,
        }
    }

    /// 头条：面包屑 · 状态字 · ⋯ · 保存 · 查找。
    fn edit_head(&mut self, ui: &mut egui::Ui, f: &EditFacts, acts: &mut EditActs) {
        // ── 头条（36，`--bg-2`）──
        super::kit::strip(ui, |ui| {
            let mut go = self.edit_crumbs(ui, f);
            self.edit_state(ui, f);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.edit_head_acts(ui, f, acts, &mut go);
            });
            if let Some(d) = go {
                self.want_dir = Some(d);
            }
        });
    }

    /// 头条左边：那台 › 面包屑（只留最后两级）› 文件名；点哪一段回到那一级（交回要去的目录）。
    fn edit_crumbs(&self, ui: &mut egui::Ui, f: &EditFacts) -> Option<String> {
        let p = super::theme::palette(ui.ctx());
        let (path, name) = (&f.path, &f.name);
        let machine = self.source.label();
        let crumbs = super::source::breadcrumbs(&super::source::parent_dir(&path));
        let mut go = None;
        if ui
            .add(
                egui::Button::new(egui::RichText::new(&machine).color(p.text2))
                    .frame_when_inactive(false),
            )
            .clicked()
        {
            go = Some(self.cwd.clone());
        }
        // 前几段折成「…」：只留最后两级目录（窄档同一条规矩）。
        let tail = crumbs.len().saturating_sub(2);
        let sep = egui_phosphor::regular::CARET_RIGHT;
        if tail > 1 {
            ui.label(egui::RichText::new(sep).color(p.faint));
            ui.label(
                egui::RichText::new(copy_text("rsFilewinEditPage.crumb.more", &[])).color(p.text2),
            );
        }
        for (label, full) in crumbs.iter().skip(tail.max(1)) {
            ui.label(egui::RichText::new(sep).color(p.faint));
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(label).color(p.text2))
                        .frame_when_inactive(false),
                )
                .on_hover_text(full)
                .clicked()
            {
                go = Some(full.clone());
            }
        }
        ui.label(egui::RichText::new(sep).color(p.faint));
        ui.label(egui::RichText::new(name).color(p.text));
        go
    }

    /// 状态字：在读 · 在存 · 未保存（带那颗点）· 几点存的。
    fn edit_state(&self, ui: &mut egui::Ui, f: &EditFacts) {
        let p = super::theme::palette(ui.ctx());
        let (pane, dirty) = (&f.pane, f.dirty);
        let state = if self.edits.opening().is_some() {
            Some(copy_text("rsFilewinEditPage.state.reading", &[]))
        } else if self.edits.saving().is_some() {
            Some(copy_text("rsFilewinEditPage.state.saving", &[]))
        } else if dirty {
            Some(copy_text("rsFilewinEditPage.state.unsaved", &[]))
        } else {
            pane.as_ref().and_then(|e| e.saved_at).map(|t| {
                copy_text(
                    "rsFilewinEditPage.state.saved",
                    &[("time", &super::source::mtime_text(t).short)],
                )
            })
        };
        if let Some(s) = state {
            ui.add_space(8.0);
            if dirty {
                // 未保存那颗 6px 实心点（与标签上那颗同一个样子）。
                let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().circle_filled(r.center(), 3.0, p.text2);
            }
            ui.label(egui::RichText::new(s).small().color(p.text2));
        }
    }

    /// 头条右边（从右往左）：⋯ 菜单 · 保存 · 查找。
    fn edit_head_acts(
        &mut self,
        ui: &mut egui::Ui,
        f: &EditFacts,
        acts: &mut EditActs,
        go: &mut Option<String>,
    ) {
        let p = super::theme::palette(ui.ctx());
        let (pane, path, name) = (&f.pane, &f.path, &f.name);
        let (idle, dirty, over, big) = (f.idle, f.dirty, f.over, f.big);
        super::kit::menu(
            ui,
            egui::RichText::new(egui_phosphor::regular::DOTS_THREE).size(16.0),
            |ui| {
                if ui
                    .button(copy_text("rsFilewinEditPage.more.reload", &[]))
                    .clicked()
                {
                    acts.reopen = true;
                    ui.close();
                }
                if ui
                    .button(copy_text("rsFilewinEditPage.more.download", &[]))
                    .clicked()
                {
                    acts.pull = true;
                    ui.close();
                }
                if ui
                    .button(copy_text("rsFilewinEditPage.more.reveal", &[]))
                    .clicked()
                {
                    *go = Some(super::source::parent_dir(path));
                    self.want_reveal = Some(name.clone());
                    ui.close();
                }
            },
        );
        let label = if self.edits.saving().is_some() {
            copy_text("rsFilewinEditPage.state.saving", &[])
        } else {
            copy_text("rsFilewinShell.editor.save", &[])
        };
        let can = pane.is_some() && dirty && idle && !over;
        let why = if over {
            copy_text(
                "rsFilewinEditPage.foot.overCap",
                &[("n", &pane.as_ref().map_or(0, |e| -e.headroom()).to_string())],
            )
        } else {
            String::new()
        };
        let b = egui::Button::new(egui::RichText::new(&label).color(if can {
            p.text
        } else {
            p.text2
        }))
        .fill(if can { p.accent_strong } else { p.hover });
        let r = ui.add_enabled(can, b);
        if !why.is_empty() {
            r.clone().on_disabled_hover_text(&why);
        }
        if r.clicked() {
            acts.save = true;
        }
        if !big
            && super::kit::ghost(
                ui,
                egui_phosphor::regular::MAGNIFYING_GLASS,
                "",
                pane.is_some(),
                "",
            )
            .on_hover_text(copy_text("rsFilewinEditPage.find.hint", &[]))
            .clicked()
        {
            acts.find_toggle = true;
        }
    }

    /// 编辑面顶上那几条：盘上被改过 · 保存失败 · 存成了没拿到摘要 · 断线。
    fn edit_banners(&mut self, ui: &mut egui::Ui, f: &EditFacts, acts: &mut EditActs) {
        let (pane, idle, dirty) = (&f.pane, f.idle, f.dirty);
        // ── 条：盘上被改过 · 保存失败 · 断线（字一个不动）──
        if let Some(e) = pane {
            if e.stale {
                match super::kit::banner(
                    ui,
                    super::kit::Tone::Warn,
                    &copy_text("rsFilewinEditPage.stale.line", &[("name", &e.name)]),
                    &[
                        super::editor::OVERWRITE_LABEL.clone(),
                        super::editor::REOPEN_LABEL.clone(),
                    ],
                ) {
                    Some(0) if idle => acts.overwrite = true,
                    Some(1) if idle => acts.reopen = true,
                    _ => {}
                }
            } else if let Some(Err(why)) = &e.last_save {
                if super::kit::banner(
                    ui,
                    super::kit::Tone::Error,
                    &copy_text("rsFilewinShell.editor.saveFailed", &[("why", why)]),
                    &[copy_text("rsFilewinEditPage.action.retry", &[])],
                )
                .is_some()
                    && idle
                {
                    acts.save = true;
                }
            } else if let Some(note) = e.save_note.as_ref().filter(|_| !dirty) {
                // 存成了、没拿到新摘要：那一句挂着，给一颗「重新读取」（重读拿到摘要，之后照常存）。
                if super::kit::banner(
                    ui,
                    super::kit::Tone::Warn,
                    note,
                    &[copy_text("rsFilewinEditPage.more.reload", &[])],
                )
                .is_some()
                    && idle
                {
                    acts.reopen = true;
                }
            }
            if self.link.offline() {
                super::kit::banner(
                    ui,
                    super::kit::Tone::Warn,
                    &copy_text(
                        "rsFilewinEditPage.offline.line",
                        &[("machine", &self.source.label())],
                    ),
                    &[],
                );
            }
        }
    }

    /// 编辑面（或打不开时那一句、在读时的骨架）；交回编辑面那一块（查找条贴着它的右上）。
    fn edit_surface(&mut self, ui: &mut egui::Ui, acts: &mut EditActs) -> egui::Rect {
        let p = super::theme::palette(ui.ctx());
        // ── 编辑面 ──
        let body = ui.available_rect_before_wrap();
        if let Some(why) = self.edit_refused.clone() {
            match super::kit::empty_state(
                ui,
                egui_phosphor::regular::FILE_X,
                &why,
                &[
                    copy_text("rsFilewinEditPage.action.download", &[]),
                    copy_text("rsFilewinEditPage.action.retry", &[]),
                ],
            ) {
                Some(0) => acts.pull = true,
                Some(1) => acts.retry_open = true,
                _ => {}
            }
        } else if self.edits.opening().is_some() && self.editing.is_none() {
            super::kit::skeleton_rows(ui, 8);
        } else if self.editing.is_some() {
            egui::Frame::new().fill(p.bg).show(ui, |ui| {
                ui.set_min_size(body.size());
                let h = ui.available_height();
                super::bigfile::show(ui, self.editing.as_mut(), h);
            });
        }
        body
    }

    /// 查找条（浮在编辑面右上）与它那几个动作。
    fn edit_find_bar(&mut self, ctx: &egui::Context, body: egui::Rect, big: bool) {
        // ── 查找条（浮在编辑面右上，`--card`）──
        let mut find_act = None;
        if self.find_open && !big {
            // 「3 / 12」：一共几处 · 光标在第几处（选区起点之前有几处 ＋ 1；不在任何一处上 ⇒ 0）。
            let count = self
                .editing
                .as_ref()
                .filter(|e| !e.find.needle.is_empty())
                .map(|e| {
                    let at =
                        egui::TextEdit::load_state(&ctx, super::bigfile::normal_editor_id(&e.path))
                            .and_then(|s| s.cursor.char_range())
                            .map_or(0, |r| r.as_sorted_char_range().start.0);
                    let starts: Vec<usize> = e
                        .text
                        .match_indices(e.find.needle.as_str())
                        .map(|(b, _)| e.text[..b].chars().count())
                        .collect();
                    let k = starts.iter().position(|&s| s == at).map_or(0, |i| i + 1);
                    (k, starts.len())
                });
            if let Some(e) = self.editing.as_mut() {
                let at = egui::pos2(body.right() - 16.0, body.top() + 8.0);
                egui::Area::new(egui::Id::new(("filewin-find-bar", &e.path)))
                    .order(egui::Order::Foreground)
                    .pivot(egui::Align2::RIGHT_TOP)
                    .fixed_pos(at)
                    .show(&ctx, |ui| {
                        egui::Frame::popup(&ctx.global_style()).show(ui, |ui| {
                            find_act = find_row(ui, &mut e.find, count);
                        });
                    });
            }
        }
        if let Some(a) = find_act {
            match a {
                FindAct::Next => {
                    self.find_in_editor(&ctx, false);
                }
                FindAct::Prev => {
                    self.find_in_editor(&ctx, true);
                }
                FindAct::Replace => {
                    self.replace_in_editor(&ctx, false);
                }
                FindAct::ReplaceAll => {
                    self.replace_in_editor(&ctx, true);
                }
                FindAct::Close => self.find_open = false,
            }
        }
    }

    /// 关这一页那一问（改了没存）：取消 · 不保存 · 保存。
    fn edit_discard_ask(&mut self, ctx: &egui::Context, name: &str, acts: &mut EditActs) {
        let p = super::theme::palette(ctx);
        // ── 关这一页那一问（改了没存；规范 `C10`）：取消 · 不保存 · 保存（焦点在「保存」）──
        if self.asking_discard {
            let hit = super::kit::dialog(
                &ctx,
                "filewin-edit-discard",
                &copy_text("rsFilewinShell.editor.unsaved", &[("name", &name)]),
                |ui| {
                    ui.label(
                        egui::RichText::new(copy_text("rsFilewinShell.editor.unsavedWarn", &[]))
                            .color(p.text),
                    );
                },
                &[
                    (
                        copy_text("rsFilewinShell.editor.keep", &[]),
                        super::kit::Btn::Plain,
                    ),
                    (
                        copy_text("rsFilewinShell.editor.discard", &[]),
                        super::kit::Btn::Plain,
                    ),
                    (
                        copy_text("rsFilewinShell.editor.saveClose", &[]),
                        super::kit::Btn::Primary,
                    ),
                ],
                2,
                0,
            );
            match hit {
                Some(0) => self.keep_editing(),
                Some(1) => {
                    self.discard_edit();
                    self.want_close = true;
                }
                Some(2) => {
                    self.asking_discard = false;
                    self.close_after_save = true;
                    acts.save = true;
                }
                _ => {}
            }
        }
    }

    /// 这一帧点了什么就做什么（一帧至多一件，存优先）。
    fn apply_edit_acts(&mut self, ctx: &egui::Context, acts: &EditActs) {
        if acts.save {
            self.save_edit(Some(ctx.clone()));
        } else if acts.overwrite {
            self.overwrite_edit(Some(ctx.clone()));
        } else if acts.reopen {
            self.reopen_edit(Some(ctx.clone()));
        } else if acts.retry_open {
            if let Some(r) = self.edit_row.clone() {
                let at = self.row_path(&r);
                self.edit_refused = None;
                self.begin_edit_at(r, at, None, Some(ctx.clone()));
            }
        } else if acts.pull {
            if let Some(r) = self.edit_row.clone() {
                self.begin_pull_row(r);
            }
        }
    }

    /// 编辑页那一份的路径（开着的 / 在读的 / 那一行的）；不是编辑页 ⇒ `None`。
    pub fn edit_path(&self) -> Option<String> {
        self.editing
            .as_ref()
            .map(|e| e.path.clone())
            .or_else(|| self.edits.opening())
            .or_else(|| self.edit_row.as_ref().map(|r| r.path.clone()))
    }

    /// 编辑页标签上（与窗口标题里）那个名字。
    pub fn edit_name(&self) -> String {
        match &self.editing {
            Some(e) => e.name.clone(),
            None => self
                .edit_path()
                .map(|p| super::source::remote_basename(&p).to_string())
                .unwrap_or_default(),
        }
    }

    /// 编辑页在前台时状态栏那一行（稿 ⑤ 底条）：「行 12 · 列 5 · UTF-8 · LF · 41.6 KB / 8 MB」；
    /// 大文件模式左端多「大文件模式 · 长行不换行 · 无查找替换」。超上限那一句另回（画在右端，`--error`）。
    pub fn editor_status(&self, ctx: &egui::Context) -> Option<(String, Option<String>)> {
        let e = self.editing.as_ref()?;
        let sep = copy_text("rsFilewinProgress.detail.sep", &[]);
        let mut parts = Vec::new();
        if e.big.is_big() {
            parts.push(copy_text("rsFilewinShell.editor.findBig", &[]));
        } else {
            let at = egui::TextEdit::load_state(ctx, super::bigfile::normal_editor_id(&e.path))
                .and_then(|s| s.cursor.char_range())
                .map_or(0, |r| r.primary.index.0);
            let (line, col) = super::editor::line_col(&e.text, at);
            parts.push(copy_text(
                "rsFilewinEditPage.foot.at",
                &[("line", &line.to_string()), ("col", &col.to_string())],
            ));
        }
        parts.push("UTF-8".to_string());
        parts.push(
            if e.text.contains("\r\n") {
                "CRLF"
            } else {
                "LF"
            }
            .to_string(),
        );
        parts.push(copy_text(
            "rsFilewinEditPage.foot.size",
            &[
                ("size", &super::rows::human_size(e.text.len() as u64)),
                (
                    "cap",
                    &super::rows::human_size(super::editor::MAX_EDIT_BYTES as u64),
                ),
            ],
        ));
        let over = e.over_cap().then(|| {
            copy_text(
                "rsFilewinEditPage.foot.overCap",
                &[("n", &(-e.headroom()).to_string())],
            )
        });
        Some((parts.join(&sep), over))
    }

    /// 画「叫什么名字 / 改成什么权限」那个框。**模态** —— 定下来之前不接别的。
    ///
    /// ⚠ 与 [`super::writeops::WriteBoard::ui`] 分开两处，因为它们的状态住在两个地方：
    /// 这个框是 UI 线程自己的，那一摞（确认 / 结果）是跨线程的。
    fn write_ui(&mut self, ui: &mut egui::Ui) {
        // 就地那一格（改名 · 新建）画在列表那一行里（`frame_body`），不在这里。
        let Some(mut p) = self.write_prompt.clone().filter(|p| !p.is_inline()) else {
            return;
        };
        // 改权限（规范 `C10`，稿 10）：标题「改权限 · deploy.sh」·「当前 644」· 3 × 3 勾 ·「数字写法」两边联动 · ［取消］［改权限］。
        // 现值答回来了 ⇒ 九格与那一栏预填至多一次（`writeops::apply_prefill`）。
        self.mode_probe.attach(ui.ctx().clone());
        let readout = self.mode_probe.readout();
        if let Some(r) = &readout {
            super::writeops::apply_prefill(&mut p, r);
        }
        let pal = super::theme::palette(ui.ctx());
        let err = self.prompt_error();
        let mut typed = false;
        let mut toggled: Option<usize> = None;
        let mut enter = false;
        let heads = [
            copy_text("rsFilewinWriteops.chmod.read", &[]),
            copy_text("rsFilewinWriteops.chmod.write", &[]),
            copy_text("rsFilewinWriteops.chmod.exec", &[]),
        ];
        let who = [
            copy_text("rsFilewinWriteops.chmod.owner", &[]),
            copy_text("rsFilewinWriteops.chmod.group", &[]),
            copy_text("rsFilewinWriteops.chmod.other", &[]),
        ];
        let hit = super::kit::dialog(
            ui.ctx(),
            "filewin-write-prompt",
            &p.heading(),
            |ui| {
                let now = match &readout {
                    Some(r) => r.line.clone(),
                    None => copy_text("rsFilewinShell.mode.reading", &[]),
                };
                ui.label(egui::RichText::new(now).color(pal.text2));
                ui.add_space(12.0);
                egui::Grid::new("filewin-chmod-grid")
                    .spacing(egui::vec2(36.0, 6.0))
                    .show(ui, |ui| {
                        ui.label("");
                        for h in &heads {
                            ui.label(egui::RichText::new(h).color(pal.text2));
                        }
                        ui.end_row();
                        for (r, w) in who.iter().enumerate() {
                            ui.label(w);
                            for c in 0..3 {
                                let k = r * 3 + c;
                                let r = match p.grid[k] {
                                    Some(on) => {
                                        let mut on = on;
                                        ui.checkbox(&mut on, "")
                                    }
                                    // 「—」：多选时各项这一位不一样 / 读不到 ⇒ 不改这一位，点一下才定。
                                    None => ui.add(
                                        egui::Checkbox::without_text(&mut false)
                                            .indeterminate(true),
                                    ),
                                };
                                if r.clicked() {
                                    toggled = Some(k);
                                }
                            }
                            ui.end_row();
                        }
                    });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(copy_text("rsFilewinWriteops.chmod.octal", &[]));
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut p.text)
                            .desired_width(88.0)
                            .font(egui::TextStyle::Monospace),
                    );
                    typed = r.changed();
                    enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                });
                if let Some(e) = &err {
                    ui.label(egui::RichText::new(e).size(12.0).color(pal.error_text));
                }
            },
            &[
                (
                    copy_text("rsFilewinWriteops.delete.cancel", &[]),
                    super::kit::Btn::Plain,
                ),
                (
                    copy_text("rsFilewinWriteops.chmod.go", &[]),
                    super::kit::Btn::Primary,
                ),
            ],
            1,
            0,
        );
        if let Some(k) = toggled {
            p.toggle_bit(k);
        }
        if typed {
            p.typed();
        }
        self.write_prompt = Some(p);
        match hit {
            Some(0) => self.cancel_write(),
            Some(_) => {
                let ctx = ui.ctx().clone();
                self.confirm_write(Some(ctx));
            }
            None if enter => {
                let ctx = ui.ctx().clone();
                self.confirm_write(Some(ctx));
            }
            None => {}
        }
    }
}

// ════════════════════════════════════════════════════════════════════════
// 🔴键盘 · 多选 · 右键菜单 —— 接到窗口上的那几跳
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

    /// 〔只给截图那一格〕再 Ctrl 点上这几个名字（选中态本体那一口，同点击走的那一个）。
    #[cfg(test)]
    pub(crate) fn pick_also(&mut self, names: &[&str]) {
        let rows = self.listing.rows.lock().unwrap().clone();
        for n in names {
            if let Some(k) = rows.iter().position(|r| r.name == *n) {
                self.selection.click(&rows, k, egui::Modifiers::COMMAND);
            }
        }
    }

    /// 选中的**恰好那一项**叫什么；`Err(n)` ＝ 选中了 `n` 项（`n ≠ 1`）。
    /// O(1)：只问选中态，不扫列表（预览每帧都问它）。
    pub fn picked_name(&self) -> Result<String, usize> {
        match self.selection.len() {
            1 => self.selection.names().pop().ok_or(0),
            n => Err(n),
        }
    }

    /// 选中的那一摞（按列表的显示序）。选中态按 `pick_key` 记，这里按同一把键取回行。
    pub fn picked_rows(&self) -> Vec<Listed> {
        let rows = self.listing.rows.lock().unwrap();
        self.selection
            .picked_indices(&rows)
            .into_iter()
            .filter_map(|i| rows.get(i).cloned())
            .collect()
    }

    /// 按名字找那一行（O(n)：只在「选中的那一项换了」时调，不是每帧）。
    pub fn row_named(&self, name: &str) -> Option<Listed> {
        self.listing
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.name == name)
            .cloned()
    }

    /// 记下这一类最近那一趟在表里的号；上一趟（这个标签页手上已经换了一块新看板）改由窗口那一级在落地时重列目录。
    fn track_job(&mut self, kind: &'static str, id: u64) {
        match self.job_ids.iter_mut().find(|(k, _)| *k == kind) {
            Some(slot) => {
                self.progress.orphan(slot.1);
                slot.1 = id;
            }
            None => self.job_ids.push((kind, id)),
        }
    }

    /// 这个标签页开的那几趟里还有在跑的吗（标签上那个忙点）。
    pub fn has_running_jobs(&self) -> bool {
        let jobs = self.progress.jobs();
        self.job_ids.iter().any(|(_, id)| {
            jobs.iter()
                .any(|j| j.id == *id && j.shown() && j.state() == super::progress::State::Running)
        })
    }

    /// 这个标签页要关了：它开过的那几趟照跑，落地后由窗口那一级重列目录。
    pub fn orphan_jobs(&mut self) {
        for (_, id) in self.job_ids.drain(..) {
            self.progress.orphan(id);
        }
    }

    /// 那台后端的阻塞档（机器上复制 · 解压 · 算大小 · 删除）开跑之后撤不动 ⇒ 「停」灰着时悬停那一句。
    /// 撤得动撤不动是那台握手时交来的事实（`Offer::withdraw`）；还没问到 ⇒ 照它是阻塞档说。
    fn no_stop(&self, op: &str) -> Option<String> {
        let machine = self.source.label();
        let offer = self.offer.lock().unwrap();
        let offered = offer
            .as_ref()
            .is_some_and(|o| o.withdraw(op) == comms_inward::chan::wire::Withdraw::Asked);
        // 撤得动的那几条窗口这一侧也没有撤它的口（一问一答、阻塞到回话）⇒ 照样灰；说法分两种。
        Some(if offered {
            copy_text("rsFilewinProgress.stop.noHandle", &[])
        } else {
            copy_text(
                "rsFilewinProgress.stop.uncancellable",
                &[("machine", &machine)],
            )
        })
    }

    /// 这个标签页**关得了吗**（`None` ＝ 关得了）—— 关标签页 / 收右栏之前问它。
    ///
    /// 有一问摆着、有一份文本开着、在读写文本 ⇒ 说是哪一件。
    /// 在传 / 在复制 / 在删的那几趟**不算**：它们是「进度」表里的行，不随标签页走，关了照跑（稿：目录标签页直接关）。
    pub fn busy_reason(&self) -> Option<String> {
        if self.editing.is_some() {
            return Some(copy_text("rsFilewinShell.busy.editorOpen", &[]).into());
        }
        self.held_reason()
    }

    /// 这个标签页手上攥着、关了就丢的那几件（一问摆着 · 在读写文本 · 一摞改名 / 新建还没回话）。
    fn held_reason(&self) -> Option<String> {
        if self.asking_up() {
            return Some(copy_text("rsFilewinShell.busy.pendingAsk", &[]).into());
        }
        if let Some(p) = self.edits.opening().or_else(|| self.edits.saving()) {
            return Some(copy_text(
                "rsFilewinShell.busy.io",
                &[("path", &p.to_string())],
            ));
        }
        if self.writes_started > self.write_board.rounds() {
            return Some(copy_text("rsFilewinShell.busy.writes", &[]).into());
        }
        None
    }

    /// 关整个窗口时问它：同 [`Self::busy_reason`]，只是开着一份**没改过**的文本不算（关了不丢东西；
    /// 改了没存的那一份由编辑面自己那一问管）；「进度」表里还在跑的那几趟算（关窗就没了）。
    pub fn work_reason(&self) -> Option<String> {
        self.held_reason()
            .or_else(|| self.progress.running_titles().into_iter().next())
    }

    /// 键盘 / 菜单那一下做不了时说的那句话。
    pub fn key_notice(&self) -> Option<&str> {
        self.key_notice.as_deref()
    }

    /// 摆着的那个右键菜单。
    pub fn menu(&self) -> Option<&MenuAt> {
        self.menu.as_ref()
    }

    /// 要人填字的那几个框（改名 · 新建 · 改权限 · 复制为 · 复制到另一台）有一个摆着吗。
    fn prompt_up(&self) -> bool {
        self.write_prompt.is_some() || self.copy_prompt.is_some() || self.cross_prompt.is_some()
    }

    /// 做不成的那一下说的话落在哪：有框摆着 ⇒ 框里（[`Self::prompt_error`]）；否则 ⇒ 列表上方那一行。
    pub(super) fn say_slot(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        if self.prompt_up() {
            self.prompt_error.lock().unwrap()
        } else {
            self.listing.error.lock().unwrap()
        }
    }

    /// 摆着的框里那一句「为什么没做成」（判据与界面看同一个值）。
    pub fn prompt_error(&self) -> Option<String> {
        self.prompt_error.lock().unwrap().clone()
    }

    /// 框里那一句（红字，紧跟在输入框下面）。
    fn prompt_error_ui(&self, ui: &mut egui::Ui) {
        if let Some(e) = self.prompt_error() {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
    }

    /// 有一个模态框摆着吗（上传那一问 · 复制那两问 · 写操作那两问 · 新建空文件那个框 · 往外拖那两问 · 编辑面）。
    ///
    /// 🔴 **一处**：拖入那一口（[`Self::take_drops`]）与键盘那一口（[`Self::apply_keys`]）
    /// 问的是同一个函数 —— 分成两份的症状是「编辑面开着，按 Delete 删掉了列表里的文件」。
    fn modal_up(&self) -> bool {
        self.asking_up() || self.editing.is_some() || self.edit_tab
    }

    /// 编辑面以外的那几个框有没有一个摆着。
    fn asking_up(&self) -> bool {
        self.board.is_asking()
            || self.copy_board.is_asking()
            // 解压撞名那一问。
            || self.extract_board.is_asking()
            // 复制到另一台：填机器名那一问 · 盖不盖那一问。
            || self.cross_prompt.is_some()
            || self.cross_board.is_asking()
            || self.copy_prompt.is_some()
            || self.write_board.is_asking()
            || self.write_prompt.is_some()

            || self.props.is_some()
    }

    /// 键盘这一帧该不该归列表。**五道闸**，任一成立就不接：
    ///
    /// 0. 这个目录视图不是焦点那一个（双栏时另一栏、后台标签页）—— 不闸的话按一下 Delete 两栏各删一次；
    /// 1. 有模态框摆着（[`Self::modal_up`]）—— 键是给那个框的；
    /// 2. 右键菜单摆着 —— Esc / 点别处先把它收掉；
    /// 3. 画的是搜索命中那一摞 —— 那一摞交不出下标（`rows::HitTally` 头注那条），
    ///    按 Delete 删的会是**另一摞**里同一个下标的文件；
    /// 4. 有控件拿着键盘焦点（搜索框里正在打字、一颗按钮刚被 Tab 到）—— 字是给它的。
    ///    ⚠ 点一下列表里的行，焦点就交出去了（egui 点别处即交；行不可聚焦），键盘回到列表。
    /// 导航键（后退前进 · 刷新 · 地址栏 · 搜索框）那道闸：模态框 · 菜单 · 文字框拿着键盘时不接；
    /// 与列表那道不同，画着搜索命中那一摞时照样接（导航不碰任何一摞的下标）。
    pub fn nav_keys_blocked(&self, ctx: &egui::Context) -> bool {
        !self.focused || self.modal_up() || self.menu.is_some() || ctx.egui_wants_keyboard_input()
    }

    /// 在列表上方说一句（键盘那一路同一个出口：下一次按键就清）。
    pub fn set_key_notice(&mut self, n: String) {
        self.key_notice = Some(n);
    }

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
            Intent::Peek => {
                self.want_peek = true;
                true
            }
            Intent::NewFolder => self.begin_mkdir(),
            Intent::Download => self.perform(Action::Download, ctx),
            Intent::Upload => self.start_pick(super::picker::Purpose::Upload, ctx),
            Intent::CopyPath => {
                let paths: Vec<String> = self
                    .picked_rows()
                    .into_iter()
                    .map(|r| r.path.clone())
                    .collect();
                if paths.is_empty() {
                    self.key_notice = Some(copy_text("rsFilewinSelect.refusal.none", &[]));
                    return false;
                }
                if let Some(c) = &ctx {
                    c.copy_text(paths.join("\n"));
                }
                self.receipt = Some(copy_text("rsFilewinShell.receipt.pathCopied", &[]));
                true
            }
            Intent::Clear => {
                let had = !self.selection.is_empty();
                self.selection.clear();
                had
            }
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
                [i] => Some((*i, rows[*i].opens_as_dir())),
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
        if let Some(why) = self.unavailable_here(a) {
            self.key_notice = Some(why);
            return false;
        }
        if let Some(why) = self.offline_refusal(a) {
            self.key_notice = Some(why);
            return false;
        }
        match (a, idx.as_slice()) {
            (Action::Delete, _) => self.delete_picked(&idx, ctx),
            (Action::Open, [i]) => self.activate(*i),
            (Action::Edit, [i]) => self.begin_edit(*i, ctx),
            (Action::Copy, [i]) => self.begin_copy(*i),
            (Action::Download, [i]) => self.begin_pull(*i),
            // 一项或多项。
            (Action::Size, _) => self.start_sizes(&idx, ctx),
            // 恰好一份文件。
            (Action::Extract, [i]) => self.start_extract(*i, ctx),
            // 恰好一份文件。
            (Action::CrossCopy, [i]) => self.begin_cross(*i),
            (Action::Rename, [i]) => self.begin_rename(*i),
            // 一项或多项：同一个框（多项时框上说件数）。
            (Action::Chmod, _) => self.begin_chmod_rows(&idx),
            (Action::Properties, [i]) => self.begin_props(*i, ctx),
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
            self.describe_for_delete(&row);
            ops.push(super::writeops::delete_op(&row));
        }
        self.start_writes(ops, ctx)
    }

    /// 删除那一问里这一行右端那一格：文件夹写「文件夹」（不写项数，前一路定的）、别的写大小；图标同列表。
    fn describe_for_delete(&self, row: &super::source::Listed) {
        let meta = if row.is_dir {
            copy_text("rsFilewinWriteops.delete.rowDir", &[])
        } else {
            super::rows::human_size(row.size)
        };
        self.write_board.describe(
            &row.path,
            super::kind::icon(super::kind::icon_kind(row)),
            meta,
        );
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

    /// **胶水**：列表说「第 `i` 行被拖起了」→ 拖的是哪一摞（它在选中里 ⇒ 整摞；
    /// 不在 ⇒ 改成只选它 —— 与右键同一个手感，[`select::Selection::pick_for_menu`]）→ 记下「在拖」。
    /// 松手落在另一栏 ⇒ 工作区走「复制到另一栏」那一个入口（不另起一条复制路）。
    pub fn apply_drag_start(&mut self) -> bool {
        let Some(i) = self.tally.drag_started else {
            return false;
        };
        {
            let rows = self.listing.rows.lock().unwrap();
            self.selection.pick_for_menu(&rows, i);
        }
        self.dragging = true;
        true
    }

    /// 〔「算目录大小」〕选中的这几项逐项问后端 `files-size`（顺序发），跑完一句话摆出来。
    /// 纯读 ⇒ 不重列目录。接不上（没运行时 / 没通道）⇒ 出声。回值 ＝ 真的起来了。
    pub fn start_sizes(&mut self, idx: &[usize], ctx: Option<egui::Context>) -> bool {
        let Some(h) = self.rt.clone() else {
            *self.listing.error.lock().unwrap() =
                Some(copy_text("rsFilewinShell.size.noRuntime", &[]));
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string());
            return false;
        };
        let items: Vec<(serde_json::Value, String)> = {
            let rows = self.listing.rows.lock().unwrap();
            // 〔有损名全寻址〕路径按字节发。
            idx.iter()
                .filter_map(|&i| rows.get(i))
                .map(|r| (self.row_path(r).wire(), r.name.clone()))
                .collect()
        };
        let origin = self.source.origin();
        // 一摞一块新看板（「进度」表里独立的一行）。
        let board = super::size::SizeBoard::default();
        self.size_board = board.clone();
        board.attach(ctx);
        let no_stop = self.no_stop(super::size::CMD_SIZE);
        let first = items.first().map(|(_, n)| n.clone()).unwrap_or_default();
        let id = self.progress.add(
            super::progress::Trip::Size {
                board: board.clone(),
                name: first,
            },
            None,
            no_stop,
        );
        self.track_job("size", id);
        h.spawn(async move {
            let mut out = Vec::with_capacity(items.len());
            for (path, name) in items {
                board.begin(&name);
                out.push(
                    super::size::size_remote(&line, &origin, &path, &name)
                        .await
                        .map_err(|e| (name, e)),
                );
            }
            board.finish(out);
        });
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
                // 这台做不到的那一件置灰，hover 说为什么。
                let blocked = self.unavailable_here(*a);
                let b = ui.add_enabled(blocked.is_none(), egui::Button::new(a.label(m.n)));
                let b = match &blocked {
                    Some(why) => b.on_disabled_hover_text(why),
                    None => b,
                };
                if b.clicked() {
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
    /// 🔴**每一帧的正文。** 从 `eframe::App::ui` 里剥出来的，
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
    /// 那一句委派今天住 [`super::workspace::Workspace`]（它才是 `eframe::App`）：
    /// 每个标签页的正文就是这里，外面只多了标签栏 · 双栏 · 预览那一层。
    pub fn frame_body(&mut self, ui: &mut egui::Ui) {
        self.font_frame(ui);
        if self.edit_frame(ui) {
            return;
        }
        // 工具条（后退 · 前进 · 上一级 · 地址栏 · 搜索格）与命令栏不在这里画：
        //   它们作用于焦点那一栏，由最外一层画在窗口顶上（[`Self::toolbar_ui`] · [`Self::command_bar_ui`]，住 `chrome.rs`）。
        // 🔴**键盘** —— 在画列表之前接：这一帧按的键，这一帧的列表就要画出结果
        //    （光标那一圈、滚进视野）。能不能接由 `keys_blocked` 那四道闸说了算。
        //    ⚠ 滚进视野要**上一帧**真物化的那一段 ⇒ 在 `tally` 被清零之前取。
        let (prev_first, prev_last) = (self.tally.first_row, self.tally.last_row);
        {
            let ctx = ui.ctx().clone();
            self.apply_keys(&ctx);
        }
        self.drop_zone_ui(ui);
        self.notices_ui(ui);
        self.search_status_frame(ui);
        self.boards_frame(ui);
        self.settle_frame(ui);
        // 🔴搜索框里有字 ⇒ 画命中，否则画当前目录。**二选一，不并排** ——
        //    并排会让「你现在看的是哪一摞」变成一个要靠标题猜的问题。
        if self.showing_hits() {
            self.hits_frame(ui);
        } else if self.showing_grep() {
            self.grep_frame(ui);
        } else {
            self.listing_frame(ui, prev_first, prev_last);
        }
        self.row_glue(ui);
    }

    /// 字体：第一帧复核；出了问题那一条一直摆着。
    fn font_frame(&mut self, ui: &mut egui::Ui) {
        // 🔴 **第一帧**才复核得了字体 —— 之前碰 `fonts_mut` 会 panic，
        //    而本仓 release 是 `panic = "abort"`（理由逐条住 `fonts.rs §四`）。
        if self.font.settle(ui.ctx()) {
            *FONT_VERDICT.lock().unwrap() = Some(self.font.clone());
        }
        // 字体出问题就**一直**摆在这儿，不自动消失：
        // 它是一个「到你改掉为止都成立的状态」，不是一次性事件
        // （同 `INVARIANTS §12` 对「设置没生效」那条的判法）。
        if let Some(note) = self.font.notice() {
            super::kit::banner(ui, super::kit::Tone::Warn, &note, &[]);
        }
    }

    /// 编辑页那一形：正文是编辑面 ⇒ 画它、交 `true`（这一帧别的都不画）。
    fn edit_frame(&mut self, ui: &mut egui::Ui) -> bool {
        // 编辑页（或单独建的视图正在编辑一份）：正文是编辑面，不是列表。
        if self.edit_tab || self.editing.is_some() || self.edits.opening().is_some() {
            self.settle_opened_edits();
            self.settle_saved_edits();
            {
                let ctx = ui.ctx().clone();
                self.settle_pick_with(Some(ctx));
            }
            self.edit_page(ui);
            return true;
        }
        false
    }

    fn drop_zone_ui(&mut self, ui: &mut egui::Ui) {
        // 从桌面拖着文件经过 ⇒ 列表区一层虚线框「松开上传 → 目录（n 个文件）」（稿 20）；松手那一下由 `take_drops` 接。
        let hovering = ui.input(|i| i.raw.hovered_files.len());
        if hovering > 0 && self.focused && !self.modal_up() {
            let dir = super::source::remote_basename(&self.cwd).to_string();
            super::kit::drop_zone(
                ui.ctx(),
                ui.available_rect_before_wrap(),
                &copy_text(
                    "rsFilewinShell.drop.release",
                    &[("dir", &dir), ("n", &hovering.to_string())],
                ),
            );
        }
    }

    /// 列表上面那几条：开终端说的话 · 目录打不开 · 截断。
    fn notices_ui(&mut self, ui: &mut egui::Ui) {
        // 一次性的那几句（键位做不成的原因 · 跳到隐藏文件 · …）不画在这里：由窗口那一级收成右下角的回执（`Workspace::frame`）。
        // 开终端那一下说的话 —— 摆着不走（到你换台机器 / 换个系统为止都成立的状态）⇒ 一条警告条。
        if let Some(said) = self.term_notice() {
            super::kit::banner(ui, super::kit::Tone::Warn, &said, &[]);
        }
        // 目录打不开：哪一种（后端的码）说一句 ＋ 出路「回上一级」「回主目录」；系统原话进「复制详情」。
        let open_fail = self.listing.open_fail.lock().unwrap().clone();
        if let Some((kind, raw)) = open_fail {
            let name = super::source::remote_basename(&self.cwd).to_string();
            let key_text = match kind {
                super::source::OpenFail::NotFound => {
                    copy_text("rsFilewinShell.open.notFound", &[("name", &name)])
                }
                super::source::OpenFail::Denied => {
                    copy_text("rsFilewinShell.open.denied", &[("name", &name)])
                }
                super::source::OpenFail::NotDir => {
                    copy_text("rsFilewinShell.open.notDir", &[("name", &name)])
                }
                super::source::OpenFail::Other => {
                    copy_text("rsFilewinShell.open.other", &[("name", &name)])
                }
            };
            // 〔复制详情条带 §5.3〕同一颗按钮（错误条最右）：复制出去的首行是屏上那句，下面是路径与原话。
            let lines = copy_core::detail::Detail::new()
                .item(copy_core::detail::Label::Path, &self.cwd)
                .item(copy_core::detail::Label::Raw, &raw)
                .render();
            let body = format!("{key_text}\n{lines}");
            let detail_id = ui.id().with("open-fail-detail");
            match super::kit::banner_with_detail(
                ui,
                super::kit::Tone::Error,
                &key_text,
                &[
                    copy_text("rsFilewinShell.open.up", &[]),
                    copy_text("rsFilewinShell.open.home", &[]),
                ],
                Some((detail_id, &body)),
            ) {
                Some(0) => self.navigate_up(),
                Some(1) => self.want_home = true,
                _ => {}
            }
        } else if let Some(e) = self.listing.error.lock().unwrap().clone() {
            super::kit::banner(ui, super::kit::Tone::Error, &e, &[]);
        }
        // 🔴 截断也要出声 —— 「这个目录里就这么多」与「后端只给了前 N 条」在屏幕上长得一样。
        if self.listing.truncated.load(Ordering::SeqCst) && !self.showing_hits() {
            let total = self.listing.total.load(Ordering::SeqCst);
            let said = copy_text(
                "rsFilewinShell.frame.truncated",
                &[
                    ("n", &(super::source::LS_LIMIT).to_string()),
                    ("total", &total.to_string()),
                ],
            );
            if super::kit::banner(
                ui,
                super::kit::Tone::Warn,
                &said,
                &[copy_text("rsFilewinShell.frame.searchByName", &[])],
            )
            .is_some()
            {
                self.search_content = false;
                ui.ctx()
                    .memory_mut(|m| m.request_focus(egui::Id::new(SEARCH_BOX_ID)));
            }
        }
    }

    /// 搜索那两条状态行（按名字 · 按内容）；目录换了而范围是「当前目录以下」⇒ 先按新目录再搜。
    fn search_status_frame(&mut self, ui: &mut egui::Ui) {
        // 范围是「当前目录以下」、框里有字、而目录换了 ⇒ 范围变了，按新目录再搜一趟。
        if !self.search_whole
            && !self.query.trim().is_empty()
            && self.search.asked_under().as_ref() != Some(&self.cwd_path())
        {
            let ctx = ui.ctx().clone();
            self.fire_search(Some(ctx), false);
        }
        // 搜索结果顶上那一条状态行（范围 · 个数 · 文件清单多久前 · 刷新）：只在搜索时摆。
        if self.showing_hits() || self.search.is_running() || self.status_wanted {
            let machine = self.source.label();
            if let Some(a) = self.search.status_ui(ui, &machine, self.search_whole) {
                let ctx = ui.ctx().clone();
                match a {
                    find::SearchAction::Refresh => {
                        self.fire_search(Some(ctx), true);
                    }
                    find::SearchAction::Scope(whole) => {
                        self.search_whole = whole;
                        self.fire_search(Some(ctx), false);
                    }
                    find::SearchAction::Retry => {
                        self.fire_search(Some(ctx), false);
                    }
                    find::SearchAction::RetryMore => {
                        self.search.retry_more();
                        self.fire_more(Some(ctx));
                    }
                    find::SearchAction::CopyPath(path) => {
                        ctx.copy_text(path);
                        self.receipt = Some(copy_text("rsFilewinShell.receipt.pathCopied", &[]));
                    }
                }
            }
        }
        // 按内容搜的状态行（在搜 ＋「停」· 几处几个文件 · 跳过几个［详情］）。
        if self.grep.is_running() || self.showing_grep() {
            if self.grep.status_ui(ui) {
                let ctx = ui.ctx().clone();
                self.fire_grep(Some(ctx));
            }
        }
    }

    /// 画在列表之前的那几问（属性 · 每一趟开头那一问 · 写类确认）。
    fn boards_frame(&mut self, ui: &mut egui::Ui) {
        // 「属性」那一问。
        self.props_ui(ui);
        // 后台那几趟（上传 · 下载 · 复制 · 解压 · 算大小 · 删除）的进度、停与结局不在这里画：
        //   它们是窗口底部「进度」表里的一行（`super::progress`，由 `Workspace` 画）。这里只剩每一趟开头那一问（模态，画在列表之前）。
        let machine = self.source.label();
        self.board.ui(ui, &machine);
        self.copy_board.ui(ui);
        self.extract_board.ui(ui);
        self.cross_board.ui(ui);
        self.cross_ui(ui);
        self.copy_ui(ui);
        // 🔴`§4.6.4` 那一摞：一次问完的确认框 ／ 结果（「被围栏挡住那几句话」那一段删了）。
        //    同样模态、同样画在列表之前。
        self.write_board.ui(ui);
        self.write_ui(ui);
    }

    /// 画列表之前先消化到货（选文件框 · 编辑 · 拖入 · 各趟结局 · 就地那一格），再把这一帧的计数清零。
    fn settle_frame(&mut self, ui: &mut egui::Ui) {
        // 系统选文件框 / 存盘框选完了 ⇒ 上传走拖入那一条（先一次问完同名，再并行传）· 下载起那一趟。
        {
            let ctx = ui.ctx().clone();
            self.settle_pick_with(Some(ctx));
        }
        // 🔴编辑那一摞：**先消化到货，再画** ——
        //    反了的话这一帧画的是上一帧的状态（读完了却还显示「正在读」）。
        self.settle_opened_edits();
        self.settle_saved_edits();
        self.take_drops(&ui.ctx().clone());
        self.settle_finished_drops();
        self.settle_finished_copies();
        self.settle_finished_extracts();
        self.settle_finished_writes();
        self.settle_inline();
        // 每帧从零数起 —— 这两个数是「这一帧物化了多少行」，不是累计。
        self.tally = RenderTally::default();
        self.hits_tally = HitTally::default();
        self.grep_tally = GrepTally::default();
    }

    /// 文件名命中那一摞。
    fn hits_frame(&mut self, ui: &mut egui::Ui) {
        let hits = self.hit_rows();
        if hits.is_empty() && self.search.has_outcome() && !self.search.index_missing() {
            self.no_hits_ui(ui);
        } else if self.search.has_outcome() {
            self.hit_rows_frame(ui, &hits);
        }
    }

    fn no_hits_ui(&mut self, ui: &mut egui::Ui) {
        // 没结果：中央一句 ＋（范围是当前目录以下时）「搜整台机器」。
        let q = self.query.clone();
        let whole = self.search_whole;
        let mut go_whole = false;
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            ui.label(find::no_match_line(&q));
            if !whole
                && ui
                    .button(copy_text("rsFilewinFind.action.searchMachine", &[]))
                    .clicked()
            {
                go_whole = true;
            }
        });
        if go_whole {
            self.search_whole = true;
            let ctx = ui.ctx().clone();
            self.fire_search(Some(ctx), false);
        }
    }

    fn hit_rows_frame(&mut self, ui: &mut egui::Ui, hits: &[super::rows::HitRow]) {
        let tail = if self.search.page_failed() {
            super::rows::HitTail::Failed
        } else if self.search.more_to_come() {
            super::rows::HitTail::More
        } else {
            super::rows::HitTail::End
        };
        // 🔴收数口是 [`HitTally`]，**不是** `self.tally` —— 它交出的下标只指命中这一摞，
        //    而下面那几条胶水索引的是 `listing.rows`（另一摞东西）。
        show_hit_rows(
            ui,
            &hits,
            self.hit_sort,
            self.hit_pick,
            tail,
            &mut self.hits_tally,
        );
        let ctx = ui.ctx().clone();
        if let Some(col) = self.hits_tally.sort_click.take() {
            self.click_hit_header(col, Some(ctx.clone()));
        }
        if let Some(i) = self.hits_tally.picked.take() {
            self.hit_pick = Some(i);
        }
        if std::mem::take(&mut self.hits_tally.retry_more) {
            self.search.retry_more();
            self.fire_more(Some(ctx.clone()));
        }
        // 滚到底（最后一行露出来了）⇒ 往下再要一屏（后端说还有才发；失败过就等「重试」）。
        if !hits.is_empty() && self.hits_tally.last_row >= hits.len() {
            self.fire_more(Some(ctx));
        }
    }

    fn grep_frame(&mut self, ui: &mut egui::Ui) {
        // 按内容搜的命中：每行点得开（跳到那份文件），收数口是 [`GrepTally`]（它的下标只指这一摞）。
        let rows: Vec<grep::GrepRow> = self
            .grep
            .shown()
            .outcome
            .map(|o| grep::grep_rows(&o))
            .unwrap_or_default();
        grep::show_grep_rows(ui, &rows, &mut self.grep_tally);
    }

    /// 当前目录那一摞：表头 · 加载中 / 空目录 / 列表（含就地那一格）。
    fn listing_frame(&mut self, ui: &mut egui::Ui, prev_first: usize, prev_last: usize) {
        let jump = self.listing_jump(ui, prev_first, prev_last);
        let want = self.reveal.as_ref().map(|r| r.name.clone());
        // 表头：点一列排序（再点反向），拖分隔线改列宽。
        if let Some(by) = super::rows::show_header(ui, &mut self.cols, self.sort) {
            self.click_header(by);
        }
        let rows = self.listing.rows.lock().unwrap();
        let loading = self.listing.is_loading();
        let failed = self.listing.error.lock().unwrap().is_some();
        // 就地新建那一格（列表里冒出来的那一行）：空目录里也照样画列表。
        let new_here = self
            .write_prompt
            .as_ref()
            .is_some_and(|p| matches!(p.kind, PromptKind::Mkdir | PromptKind::NewFile));
        if rows.is_empty() && loading && !new_here {
            // 加载：300 ms 内什么都不画，之后骨架行。
            let since = *self
                .loading_since
                .get_or_insert_with(|| ui.input(|i| i.time));
            if ui.input(|i| i.time) - since > 0.3 {
                super::kit::skeleton_rows(ui, 5);
            } else {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100));
            }
        } else if rows.is_empty()
            && !failed
            && !new_here
            && self.listing.hidden.lock().unwrap().is_empty()
        {
            drop(rows);
            self.empty_dir_ui(ui);
        } else {
            self.loading_since = None;
            let unreadable = self.listing.unreadable.load(Ordering::SeqCst);
            let tail = (unreadable > 0).then(|| {
                copy_text(
                    "rsFilewinShell.frame.unreadable",
                    &[("n", &unreadable.to_string())],
                )
            });
            // 就地那一格：新建 ⇒ 文件夹那一段之后插一行（名字缺省、选中）；改名 ⇒ 那一行的名字格换成输入框。
            let (mut text, shown, at) =
                inline_rows(&rows, self.write_prompt.as_ref().filter(|p| p.is_inline()));
            let err = self.prompt_error();
            let mut cell = at.map(|row| super::rows::InlineCell {
                row,
                text: &mut text,
                error: err.as_deref(),
                select: self.inline_select.take(),
                busy: self.inline_busy,
                outcome: None,
            });
            super::rows::show_file_rows_with_tail(
                ui,
                shown.as_deref().unwrap_or(&rows),
                &mut self.tally,
                jump,
                want.as_deref(),
                Some(&self.selection),
                &self.cols,
                tail.as_deref(),
                cell.as_mut(),
            );
            let outcome = cell.and_then(|c| c.outcome);
            drop(rows);
            // 插进来的那一行不是目录里的一项 ⇒ 点到的下标挪回目录那一摞（点在那一行上 ＝ 没点）。
            if let (Some(_), Some(i)) = (&shown, at) {
                tally_skip_inserted(&mut self.tally, i);
            }
            if let Some(p) = self.write_prompt.as_mut().filter(|p| p.is_inline()) {
                p.text = text;
            }
            match outcome {
                Some(true) => {
                    let ctx = ui.ctx().clone();
                    self.commit_inline(Some(ctx));
                }
                Some(false) => self.cancel_write(),
                None => {}
            }
        }
    }

    /// 这一帧要滚到哪：reveal 那一下优先，否则键盘挪出视野的光标。
    fn listing_jump(
        &mut self,
        ui: &mut egui::Ui,
        prev_first: usize,
        prev_last: usize,
    ) -> Option<f32> {
        // 🔴reveal 的两半在这里落地：**算**出偏移（只算一次）＋ 高亮那个名字。
        //    ⚠ 偏移是算的不是找的 —— 那条纪律（`show_rows` 才是主语）。
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
        // 键盘挪了光标 ⇒ 不在视野里才滚（reveal 那一下优先：它也是「只滚一次」）。
        let key_jump = self
            .key_scroll
            .take()
            .and_then(|i| select::scroll_for(i, prev_first, prev_last, pitch));
        jump.or(key_jump)
    }

    /// 空目录：一句 ＋「上传」「新建」。
    fn empty_dir_ui(&mut self, ui: &mut egui::Ui) {
        self.loading_since = None;
        match super::kit::empty_state(
            ui,
            egui_phosphor::regular::FOLDER_SIMPLE,
            &copy_text("rsFilewinShell.frame.empty", &[]),
            &[
                format!(
                    "{} {}",
                    egui_phosphor::regular::UPLOAD_SIMPLE,
                    copy_text("rsFilewinShell.frame.emptyUpload", &[])
                ),
                format!(
                    "{} {}",
                    egui_phosphor::regular::PLUS,
                    copy_text("rsFilewinChrome.command.new", &[])
                ),
            ],
        ) {
            Some(0) => {
                let ctx = ui.ctx().clone();
                self.run_command(super::chrome::Cmd::Upload, Some(ctx));
            }
            Some(1) => {
                self.begin_mkdir();
            }
            _ => {}
        }
    }

    /// 画完之后那几条胶水：点了命中就跳 · 单击 / 右键 / 拖起一行 · 菜单。
    fn row_glue(&mut self, ui: &mut egui::Ui) {
        // ⚠ 这三条只对**目录列表**那一摞有意义（下标索引的是 `listing.rows`）。
        //   命中那一摞交不出下标 —— 第四刀靠的是「那个函数不画可点控件」这条纪律，
        //   第五刀换成了**类型**（上面那一段）。
        // 按内容搜那一摞的胶水：点了第 i 条 ⇒ 跳过去（下标只指命中那一摞，与下面几条胶水不相干）。
        if let Some(i) = self.grep_tally.jump.take() {
            self.jump_to_grep_hit(i);
        }
        // 文件名命中那一摞：点了第 i 条 ⇒ 跳过去。
        if let Some(i) = self.hits_tally.jump.take() {
            self.jump_to_find_hit(i);
        }
        self.apply_click();
        // 🔴单击改选中 · 右键摆菜单。然后画菜单（它在最上层）。行上没有按钮：动作全走菜单与键盘（同一个 `perform`）。
        self.apply_pick_click();
        // 第八条胶水：拖起一行。
        self.apply_drag_start();
        let at = ui
            .ctx()
            .input(|i| i.pointer.interact_pos())
            .unwrap_or_default();
        self.apply_menu_click(at);
        self.menu_ui(ui);
    }

    /// 工具条上搜索那一格：框内左侧两段「名字 | 内容」· 输入框 · 右侧 ×（有字时）· 在飞指示。
    /// 「名字」🔴 **`changed()` 就发** —— 一敲就出（没有去抖：在飞的旧那几趟后端按号收手、这一侧按号丢掉，结果不会错）；
    /// 「内容」回车才发（按内容搜要把整棵树读一遍）。范围与「刷新」在结果顶上那一条状态行里（`frame_body`）。
    pub(super) fn search_box(&mut self, ui: &mut egui::Ui, compact: Option<f32>) {
        let p = super::theme::palette(ui.ctx());
        let mut fire = false;
        let mut fire_grep = false;
        let mut switch: Option<bool> = None;
        let mut clear = false;
        let w = compact.unwrap_or(SEARCH_BOX_WIDTH);
        let focused = ui.memory(|m| m.has_focus(egui::Id::new(SEARCH_BOX_ID)));
        egui::Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .stroke(egui::Stroke::new(
                1.0,
                if focused { p.accent } else { p.border_soft },
            ))
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(4, 2))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (content, label) in [
                    (false, copy_text("rsFilewinShell.search.byName", &[])),
                    (true, copy_text("rsFilewinShell.search.byContent", &[])),
                ] {
                    let on = self.search_content == content;
                    if super::kit::toggle(ui, "", &label, on).clicked() && !on {
                        switch = Some(content);
                    }
                }
                let (buf, hint) = if self.search_content {
                    (
                        &mut self.grep_query,
                        copy_text("rsFilewinShell.search.hintContent", &[]),
                    )
                } else {
                    (
                        &mut self.query,
                        copy_text("rsFilewinShell.search.hint", &[]),
                    )
                };
                let has_text = !buf.is_empty();
                let r = ui.add(
                    egui::TextEdit::singleline(buf)
                        .id(egui::Id::new(SEARCH_BOX_ID))
                        .frame(egui::Frame::NONE)
                        .desired_width((w - 110.0).max(60.0))
                        .hint_text(hint),
                );
                if self.search_content {
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        fire_grep = true;
                    }
                } else if r.changed() {
                    fire = true;
                }
                if self.search.is_running() || self.grep.is_running() {
                    ui.spinner();
                }
                if has_text
                    && ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(egui_phosphor::regular::X).color(p.text2),
                            )
                            .frame(false),
                        )
                        .clicked()
                {
                    clear = true;
                }
            });
        if let Some(content) = switch {
            // 换一种搜法：框里的字跟着过去（名字 → 内容不立刻读整棵树，等回车）。
            if content {
                self.grep_query = std::mem::take(&mut self.query);
            } else {
                self.query = std::mem::take(&mut self.grep_query);
                fire = true;
            }
            self.search_content = content;
            ui.memory_mut(|m| m.request_focus(egui::Id::new(SEARCH_BOX_ID)));
        }
        if clear {
            self.query.clear();
            self.grep_query.clear();
            self.grep.stop();
        }
        let ctx = ui.ctx().clone();
        if fire {
            self.fire_search(Some(ctx.clone()), false);
        }
        if fire_grep {
            self.fire_grep(Some(ctx));
        }
    }

    /// 搜索框在哪一种：`true` ＝ 按内容（判据 · Ctrl+Shift+F 用）。
    pub fn set_search_content(&mut self, on: bool) {
        if on != self.search_content {
            if on {
                self.grep_query = std::mem::take(&mut self.query);
            } else {
                self.query = std::mem::take(&mut self.grep_query);
            }
            self.search_content = on;
        }
    }
}

// `early_failure` · `EARLY_FAILURE_BUDGET`（开窗之后看它是不是当场就退了）随「起进程那一侧」留在 monitor：壳里 `filewin/proc.rs`。

/// 工具条上那个搜索框的 egui id（Ctrl+F 把焦点给它）。
pub const SEARCH_BOX_ID: &str = "filewin-search-box";
/// 窗口最小多大（逻辑像素）：再小，工具条收进「⋯」之后地址栏也摆不下了。
pub const MIN_WINDOW: [f32; 2] = [640.0, 400.0];

/// 窗口标题：「{当前标签名} · {机器} · 文件」（任务栏里几扇窗分得出在哪个目录）。
pub fn window_title(tab: &str, machine: &str) -> String {
    let tab = if tab.is_empty() { "/" } else { tab };
    copy_text(
        "rsFilewinShell.window.title",
        &[("tab", &tab.to_string()), ("machine", &machine.to_string())],
    )
}
/// 搜索框平时多宽（窗口窄时收窄，见 `chrome.rs` 工具条）。
pub const SEARCH_BOX_WIDTH: f32 = 260.0;

/// 换目录时历史怎么记。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hist {
    /// 新走一步（地址栏 · 面包屑 · 双击 · 上一级 · 书签）。
    Push,
    Back,
    Forward,
}

/// 在**次线程**上开一个文件管理窗口。立刻返回，不阻塞调用方。
///
/// ⚠ `eframe::run_native` 在它自己那条线程上是**阻塞到窗口关闭**的；
/// 这里把它整个丢进 `std::thread::spawn` ⇒ 对调用方是非阻塞的。
///
/// 🔴**这两个函数今天跑在窗口进程里，不在 monitor 里** ——
/// 调用方是 [`super::proc::child_main`]。它们**不带种子**的这一支（本函数）
/// 生产上没人走：入口那条命令恒是先列一趟再把那一屏交出去。
pub fn open_detached(
    source: Source,
    cwd: String,
    rt: Option<tokio::runtime::Handle>,
) -> std::thread::JoinHandle<Result<(), String>> {
    open_detached_seeded(
        source,
        cwd,
        rt,
        None,
        Vec::new(),
        None,
        None,
        None,
        Vec::new(),
        None,
        None,
    )
}

/// 同 [`open_detached`]，但**带着已经列好的那一屏**开窗。
///
/// 🔴 用它而不是 `open_detached` 的理由住 [`FileWindow::seeded`]：
/// 入口那条命令为了能在 webview 那侧出声，已经列过一趟了，别再打第二次往返。
/// 🔴**为什么窗口进程里还是「起一条次线程」而不是直接占 `main`**
///
/// 两条，都不是省事：
/// ① `with_any_thread(true)` 那条路是这个窗口**唯一被实地量过**的形态
///    （那四趟读数、Xvfb 台架那一摞都是在它上面打的）。
///    换成「占 `main` 线程」是换一个**没有读数**的配置，而换了被测对象就要重打读数。
/// ② 那个 hook 里还挂着 Windows 的进程 DPI 归属那一句（今天是 `with_dpi_aware(builder, true)`），
///    它有自己的判据。绕开 hook 就是把那一句一起绕开。
/// ⇒ 保持不动：窗口进程的 `main` 只负责读种子、起运行时、`join` 这条线程。
pub fn open_detached_seeded(
    source: Source,
    cwd: String,
    rt: Option<tokio::runtime::Handle>,
    // 🔴通道（`None` = 判据那一形：不连后端）。
    line: Option<Line>,
    rows: Vec<Listed>,
    // 🔴`reveal` = 开窗就高亮这一行（`None` = 不高亮）。
    //    那是老面板 `open(revealPath)` 那一形（会话工具卡 → 文件跳转）。
    reveal: Option<String>,
    // 书签文件（monitor 算好交过来；`None` ＝ 数据目录解不出来，书签栏上出声）。
    bookmarks: Option<std::path::PathBuf>,
    // 视图文件（记缩放；同书签那份，monitor 算好交过来；`None` ＝ 照样能缩放，只是不记）。
    view: Option<std::path::PathBuf>,
    // 「复制到另一台」下拉里的机器（开窗种子带来的）。
    machines: Vec<String>,
    // 开出来第一拍夹进这块工作区（`None` ＝ 不夹）。
    work_area: Option<host_core::WorkArea>,
    // 窗口的样子（开窗种子带来的那一套；`None` ＝ 判据那一形，照 egui 当下的样子画）。
    theme: Option<filewin_contract::Theme>,
) -> std::thread::JoinHandle<Result<(), String>> {
    OPEN_REQUESTED.fetch_add(1, Ordering::SeqCst);
    std::thread::spawn(move || {
        let title = window_title(super::source::remote_basename(&cwd), &source.label());
        let opts = eframe::NativeOptions {
            event_loop_builder: Some(Box::new(crate::platform::any_thread_hook)),
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 800.0])
                .with_min_inner_size(MIN_WINDOW),
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
                // 🔴开窗就高亮那一行。**在这里设而不是进 `seeded` 的签名** ——
                //    `seeded` 有 5 处调用点（判据 4 处），而 reveal 只有开窗那一条路用得上。
                if let Some(name) = reveal {
                    w.set_reveal(&name);
                }
                // 书签：按这台机器的 origin 读一次。
                w.shelf = Some(super::bookmarks::Shelf::open(bookmarks, &w.source.origin()));
                w.machines = machines;
                // 样子：主界面那一套配色 · 字号 · 间距。
                if let Some(t) = &theme {
                    super::theme::install(&cc.egui_ctx, t);
                }
                // 第一拍：读文件 ＋ `set_fonts`。**这里复核不了**（`fonts.rs §四`）。
                w.font = FontState::Pending(fonts::install(&cc.egui_ctx, theme.as_ref()));
                // 最外一层是 `Workspace`（标签页 ＋ 双栏 ＋ 预览），开窗那一个目录视图是它的第一个标签页。
                let mut ws = super::workspace::Workspace::new(w);
                ws.theme = theme;
                // 上次的缩放（主界面的字号随样子交过来，这里在它之上整体缩放）。
                ws.zoom = super::workspace::Zoom::open(view);
                // 帧日志：设了 `CCM_FILEWIN_FRAME_LOG=<文件>` 才开（收集「一闪一闪」的证据）。
                ws.frame_log = super::frame_log::FrameLog::from_env();
                cc.egui_ctx.set_zoom_factor(ws.zoom.factor());
                Ok(Box::new(FitOnce {
                    inner: ws,
                    work: work_area,
                }) as Box<dyn eframe::App>)
            }),
        )
        .map_err(|e| copy_text("rsFilewinShell.window.openFailed", &[("e", &e.to_string())]))
    })
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/shell_tests.rs"]
mod tests;

// 键盘 · 多选 · 右键菜单接到窗口上的那一摞（每一条都真跑 `frame_body`）。
#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/shell_keys_tests.rs"]
mod keys_tests;

/// 开窗后第一拍（窗口几何有了的那一拍）把窗口夹进 monitor 交来的工作区，之后原样转交。
struct FitOnce<A> {
    inner: A,
    work: Option<host_core::WorkArea>,
}

impl<A: eframe::App> eframe::App for FitOnce<A> {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.inner.logic(ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Some(work) = self.work {
            // 几何是逻辑点、而逻辑点随整窗缩放变 ⇒ 按此刻的 pixels_per_point（含缩放）换算。
            let ppp = Some(ui.ctx().pixels_per_point());
            let (outer, inner) = ui.ctx().input(|i| {
                let v = i.viewport();
                (v.outer_rect, v.inner_rect)
            });
            if let (Some(outer), Some(inner), Some(ppp)) = (outer, inner, ppp) {
                self.work = None;
                for cmd in fit_commands(outer, inner, ppp, work) {
                    ui.ctx().send_viewport_cmd(cmd);
                }
            }
        }
        self.inner.ui(ui, frame);
    }
}

/// [`FitOnce`] 那一拍发什么（纯函数）：egui 的几何是逻辑点，工作区是物理像素 ⇒ 换算后交给同一个判定
/// `crate::fit_into_work_area`（Tauri 那几扇窗也用它），再换回逻辑点。本来就在里面 ⇒ 空。
pub(crate) fn fit_commands(
    outer: egui::Rect,
    inner: egui::Rect,
    ppp: f32,
    work: host_core::WorkArea,
) -> Vec<egui::ViewportCommand> {
    let px = |v: f32| (v * ppp).round();
    let Some(((w, h), (x, y))) = host_core::fit_into_work_area(
        (px(outer.min.x) as i32, px(outer.min.y) as i32),
        (px(outer.width()) as u32, px(outer.height()) as u32),
        (px(inner.width()) as u32, px(inner.height()) as u32),
        work,
    ) else {
        return Vec::new();
    };
    vec![
        egui::ViewportCommand::InnerSize(egui::vec2(w as f32 / ppp, h as f32 / ppp)),
        egui::ViewportCommand::OuterPosition(egui::pos2(x as f32 / ppp, y as f32 / ppp)),
    ]
}

/// 编辑面查找栏上按了哪一颗。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FindAct {
    Next,
    Prev,
    Replace,
    ReplaceAll,
    Close,
}

/// 一行里的一句话：放不下就截成「…」、悬停看全句；`reserve` ＝ 它后面那几颗控件要留的宽。
pub(crate) fn fit_label(ui: &mut egui::Ui, text: String, reserve: f32) {
    let room = (ui.available_width() - reserve).max(0.0);
    ui.scope(|ui| {
        ui.set_max_width(room);
        ui.add(egui::Label::new(text.as_str()).truncate())
            .on_hover_text(text);
    });
}

/// 模态框离窗口边留多少。
const MODAL_EDGE: f32 = 8.0;

/// 摆一个模态框，外加两件每个框都要的事：框不比窗口宽（窄窗口里不被裁掉）；
/// Esc ＝ 取消（只认最上面那个框；下拉开着时 Esc 先收下拉）。回 `(框里画的结果, 按了 Esc)`。
pub(crate) fn modal<R>(
    ctx: &egui::Context,
    id: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> (R, bool) {
    let chrome = egui::Frame::popup(&ctx.global_style())
        .total_margin()
        .sum()
        .x;
    let room = (ctx.content_rect().width() - chrome - 2.0 * MODAL_EDGE).max(0.0);
    let r = egui::Modal::new(egui::Id::new(id)).show(ctx, |ui| {
        ui.set_max_width(room);
        add(ui)
    });
    let esc = r.is_top_modal
        && !r.any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    (r.inner, esc)
}

/// 框里那个单行输入框：框刚摆出来（没有谁拿着焦点）就把焦点给它；回 ＝ 在它里面按了回车。
pub(crate) fn prompt_field(ui: &mut egui::Ui, text: &mut String) -> bool {
    let r = ui.text_edit_singleline(text);
    let entered = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if !entered && ui.memory(|m| m.focused().is_none()) {
        r.request_focus();
    }
    entered
}

/// 查找框的 egui id（Ctrl+F 把焦点给它）。
const FIND_ID: &str = "filewin-editor-find";

/// 编辑面那一截查找替换：查找框 · 上一个 · 下一个 · 替换框 · 替换 · 全部替换 · 上一下那句话。
/// 在查找框里按回车 ＝「下一个」。回这一帧按了哪一颗。
fn find_row(
    ui: &mut egui::Ui,
    f: &mut super::editor::FindBar,
    count: Option<(usize, usize)>,
) -> Option<FindAct> {
    use egui_phosphor::regular as ph;
    let mut act = None;
    let p = super::theme::palette(ui.ctx());
    // 第一行：查找框（框内左侧放大镜、右侧「3 / 12」）· ↑ ↓ · ×；第二行：替换为 · 替换 · 全部替换（稿 03）。
    ui.horizontal(|ui| {
        let r = ui.add(
            egui::TextEdit::singleline(&mut f.needle)
                .id(egui::Id::new(FIND_ID))
                .hint_text(format!(
                    "{} {}",
                    ph::MAGNIFYING_GLASS,
                    copy_text("rsFilewinShell.editor.findLabel", &[])
                ))
                .desired_width(180.0),
        );
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            act = Some(FindAct::Next);
        }
        if let Some((k, n)) = count {
            ui.label(
                egui::RichText::new(copy_text(
                    "rsFilewinEditPage.find.count",
                    &[("k", &k.to_string()), ("n", &n.to_string())],
                ))
                .small()
                .color(if n == 0 { p.error } else { p.text2 }),
            );
        }
        let icon = |ui: &mut egui::Ui, glyph: &str, hint: String| {
            ui.add(egui::Button::new(glyph).frame_when_inactive(false))
                .on_hover_text(hint)
                .clicked()
        };
        if icon(
            ui,
            ph::CARET_UP,
            copy_text("rsFilewinShell.editor.findPrev", &[]),
        ) {
            act = Some(FindAct::Prev);
        }
        if icon(
            ui,
            ph::CARET_DOWN,
            copy_text("rsFilewinShell.editor.findNext", &[]),
        ) {
            act = Some(FindAct::Next);
        }
        if icon(ui, ph::X, copy_text("rsFilewinEditPage.find.close", &[])) {
            act = Some(FindAct::Close);
        }
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut f.with)
                .hint_text(copy_text("rsFilewinShell.editor.replaceLabel", &[]))
                .desired_width(180.0),
        );
        if ui
            .button(copy_text("rsFilewinShell.editor.replaceOne", &[]))
            .clicked()
        {
            act = Some(FindAct::Replace);
        }
        if ui
            .button(copy_text("rsFilewinShell.editor.replaceAll", &[]))
            .clicked()
        {
            act = Some(FindAct::ReplaceAll);
        }
    });
    if let Some(n) = &f.notice {
        ui.label(egui::RichText::new(n).small().color(p.text2));
    }
    act
}

/// 〔有损名全寻址〕一行的名字的真字节：有损 ⇒ 后端送的那一段（`raw_name`），否则 ⇒ 名字的 UTF-8。
pub fn name_bytes(r: &super::source::Listed) -> Vec<u8> {
    r.raw_name
        .clone()
        .unwrap_or_else(|| r.name.as_bytes().to_vec())
}

/// 目录 ＋ `/` ＋ 名字（按字节拼；根上不重复那个 `/`）。
pub fn join_path(dir: &super::source::RemotePath, name: &[u8]) -> super::source::RemotePath {
    let mut b = dir.bytes();
    while b.len() > 1 && b.last() == Some(&b'/') {
        b.pop();
    }
    if b.last() != Some(&b'/') {
        b.push(b'/');
    }
    b.extend_from_slice(name);
    super::source::RemotePath::from_bytes(&b)
}

/// 就地那一格要画成什么：输入框里的字 · 插了新行之后的那一摞（新建才有）· 那一格在第几行。
/// 新建 ⇒ 文件夹那一段之后插一行；改名 ⇒ 就是那一行。
fn inline_rows(
    rows: &[super::source::Listed],
    prompt: Option<&WritePrompt>,
) -> (String, Option<Vec<super::source::Listed>>, Option<usize>) {
    let mut text = String::new();
    let mut shown: Option<Vec<super::source::Listed>> = None;
    let mut at: Option<usize> = None;
    if let Some(p) = prompt {
        text = p.text.clone();
        match &p.kind {
            PromptKind::Rename { from, .. } => {
                at = rows.iter().position(|r| &r.path == from);
            }
            kind => {
                let i = rows.iter().rposition(|r| r.is_dir).map_or(0, |i| i + 1);
                let mut v = rows.to_vec();
                v.insert(
                    i,
                    super::source::Listed::plain(super::source::Row {
                        name: text.clone(),
                        path: super::writeops::join_remote(&p.dir, &text),
                        is_dir: matches!(kind, PromptKind::Mkdir),
                        size: 0,
                        lossy_name: false,
                    }),
                );
                shown = Some(v);
                at = Some(i);
            }
        }
    }
    (text, shown, at)
}

/// 列表里插了一行（就地新建）：点到的下标挪回目录那一摞（点在插进来的那一行上 ＝ 没点）。
fn tally_skip_inserted(t: &mut RenderTally, i: usize) {
    let back = |x: usize| match x.cmp(&i) {
        std::cmp::Ordering::Less => Some(x),
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => Some(x - 1),
    };
    t.clicked = t.clicked.and_then(back);
    t.menu_clicked = t.menu_clicked.and_then(back);
    t.drag_started = t.drag_started.and_then(back);
    t.picked_click = t.picked_click.and_then(|(x, m)| back(x).map(|x| (x, m)));
}

/// 编辑页这一帧的事实（画头条与那几条要用）。
struct EditFacts {
    pane: Option<super::editor::Pane>,
    path: String,
    name: String,
    idle: bool,
    dirty: bool,
    over: bool,
    big: bool,
}

/// 编辑页这一帧点下的动作（画完统一做）。
#[derive(Default)]
struct EditActs {
    save: bool,
    overwrite: bool,
    reopen: bool,
    retry_open: bool,
    pull: bool,
    find_toggle: bool,
}

/// 「放到」那块的面包屑行：那台 › 最后三级（前面折成「…」）；右端「新建文件夹」。
fn cross_crumbs(
    ui: &mut egui::Ui,
    pal: &super::theme::Palette,
    pick: &super::cross_copy::Pick,
    go_to: &mut Option<String>,
    mkdir: &mut bool,
) {
    // 面包屑：那台 › 一段一段（点哪一段回到那一级）；右端「新建文件夹」。
    ui.horizontal(|ui| {
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(super::cross_copy::shown_machine(&pick.machine))
                        .size(12.0)
                        .color(pal.text2),
                )
                .frame(false),
            )
            .clicked()
        {
            *go_to = Some("/".to_string());
        }
        // 只摆最后三级，前面折成一颗「…」（点了到折起来的最深那一级）：那台的主目录可能很深。
        let crumbs: Vec<(String, String)> = super::source::breadcrumbs(&pick.path)
            .into_iter()
            .skip(1)
            .collect();
        let keep = crumbs.len().saturating_sub(3);
        if keep > 0 {
            ui.label(
                egui::RichText::new(egui_phosphor::regular::CARET_RIGHT)
                    .size(10.0)
                    .color(pal.faint),
            );
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(copy_text("rsFilewinEditPage.crumb.more", &[]))
                            .size(12.0)
                            .color(pal.text2),
                    )
                    .frame(false),
                )
                .clicked()
            {
                *go_to = Some(crumbs[keep - 1].1.clone());
            }
        }
        for (seg, at) in crumbs.into_iter().skip(keep) {
            ui.label(
                egui::RichText::new(egui_phosphor::regular::CARET_RIGHT)
                    .size(10.0)
                    .color(pal.faint),
            );
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(&seg).size(12.0).color(pal.text))
                        .frame(false),
                )
                .clicked()
            {
                *go_to = Some(at);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let r = ui.add_enabled(
                !pick.path.is_empty(),
                egui::Button::new(
                    egui::RichText::new(format!(
                        "{} {}",
                        egui_phosphor::regular::PLUS,
                        copy_text("rsFilewinWriteops.inline.newDir", &[])
                    ))
                    .size(12.0),
                )
                .frame(false),
            );
            *mkdir = r.clicked();
        });
    });
}

/// 那台的文件夹清单（至多五行高）：单击选中、双击进去。
fn cross_dirs(
    ui: &mut egui::Ui,
    pal: &super::theme::Palette,
    pick: &super::cross_copy::Pick,
    go_to: &mut Option<String>,
    picked: &mut Option<String>,
) {
    let row_h = super::theme::metrics(ui.ctx()).row_h;
    egui::ScrollArea::vertical()
        .max_height(row_h * 5.0)
        .auto_shrink([false, true])
        .show(ui, |ui| match &pick.dirs {
            None => super::kit::skeleton_rows(ui, 3),
            Some(Err(e)) => {
                ui.label(egui::RichText::new(e).color(pal.error_text));
            }
            Some(Ok(dirs)) if dirs.is_empty() => {
                ui.label(
                    egui::RichText::new(copy_text("rsFilewinCrossCopy.prompt.noDirs", &[]))
                        .color(pal.text2),
                );
            }
            Some(Ok(dirs)) => {
                for d in dirs {
                    let on = pick.picked.as_deref() == Some(d.as_str());
                    let (rect, r) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), row_h),
                        egui::Sense::click(),
                    );
                    if on {
                        ui.painter().rect_filled(rect, 4.0, pal.picked);
                    } else if r.hovered() {
                        ui.painter().rect_filled(rect, 4.0, pal.hover);
                    }
                    ui.painter().text(
                        rect.left_center() + egui::vec2(8.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        format!("{}  {}", egui_phosphor::regular::FOLDER_SIMPLE, d),
                        egui::TextStyle::Body.resolve(ui.style()),
                        pal.text,
                    );
                    if r.double_clicked() {
                        *go_to = Some(super::writeops::join_remote(&pick.path, d));
                    } else if r.clicked() {
                        *picked = Some(d.clone());
                    }
                }
            }
        });
}

/// 「复制到另一台」那一问里这一帧点下的：选中哪个文件夹 · 进到哪 · 新建文件夹。
struct CrossClicks {
    picked: Option<String>,
    go_to: Option<String>,
    mkdir: bool,
}
