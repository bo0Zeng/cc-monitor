//! 开窗（`24e` 的窗口壳）：把 egui 那条事件循环放到**次线程**，主线程留给 Tauri／`tao`。
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
//! 之前这一节的结论是读源码推出来的，处置写着「建议，没做 —— 没有 Windows 机器，
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
//! 🚫 **买不到**：真 Windows 上一趟都没跑。这一改的依据是上面那张
//! 09-20 的四格真机读数 ＋ 「窗口进程里没有 `tao`」这条源码事实，是逻辑题，不是新读数。

use super::*;

/// 最近一个窗口在**第一帧**上复核字体的结果。`None` = 还没有任何窗口复核过。
///
/// 🔴 判据拿它买的是「装字体这一步**真的接在开窗那条路上**」——
/// **扫源码买不到这个**（本仓 09-20 栽过一次：源码扫描那条与行为那条买的不是同一样东西，
/// 前者看不见「按钮接没接到方法上」）。⇒ 由 Xvfb 那条真开窗的判据读它。
pub(super) static FONT_VERDICT: Mutex<Option<FontState>> = Mutex::new(None);

// `any_thread_hook`（winit 事件循环建在次线程上 · 本进程自己管 DPI，三个平台各一句）住 `platform.rs`。

// `early_failure` · `EARLY_FAILURE_BUDGET`（开窗之后看它是不是当场就退了）随「起进程那一侧」留在 monitor：壳里 `filewin/proc.rs`。

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

/// 在**次线程**上开一个文件管理窗口。立刻返回，不阻塞调用方。
///
/// ⚠ `eframe::run_native` 在它自己那条线程上是**阻塞到窗口关闭**的；
/// 这里把它整个丢进 `std::thread::spawn` ⇒ 对调用方是非阻塞的。
///
/// 🔴**它跑在窗口进程里，不在 monitor 里** —— 调用方是 [`crate::proc::child_main`]。
///
/// **带着已经列好的那一屏**开窗（理由住 [`FileWindow::seeded`]）：
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
    // 已经列好的那一屏（手上那一份：记号 ＋ 行；之后的重问带着它的记号）。
    first: crate::source::Shown,
    // 🔴`reveal` = 开窗就高亮这一行（`None` = 不高亮）。
    //    会话工具卡 → 文件跳转那一形用它。
    reveal: Option<String>,
    // 视图文件（记缩放与预览开关；monitor 算好交过来；`None` ＝ 照样能缩放、开预览，只是不记）。
    view: Option<std::path::PathBuf>,
    // 「复制到另一台」下拉里的机器（开窗种子带来的）。
    machines: Vec<String>,
    // 开出来第一拍夹进这块工作区（`None` ＝ 不夹）；之后改缩放再夹按的是那一刻所在那块屏（[`fit_step`]）。
    work_area: Option<host_core::WorkArea>,
    // 窗口的样子（开窗种子带来的那一套；`None` ＝ 判据那一形，照 egui 当下的样子画）。
    theme: Option<filewin_contract::Theme>,
) -> std::thread::JoinHandle<Result<(), Said>> {
    std::thread::spawn(move || {
        let title = window_title(crate::source::remote_basename(&cwd), &source.label());
        let mut opts = eframe::NativeOptions {
            event_loop_builder: Some(Box::new(crate::platform::any_thread_hook)),
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 800.0])
                .with_min_inner_size(MIN_WINDOW),
            ..Default::default()
        };
        // 渲染器挑哪块显卡（Windows：真显卡在前、WARP 垫底；别处缺省）。
        crate::platform::renderer_options(&mut opts);
        eframe::run_native(
            &title,
            opts,
            Box::new(move |cc| {
                let mut w = FileWindow::seeded(source, cwd, rt, Vec::<Listed>::new());
                w.listing.adopt(first);
                if let Some(line) = line {
                    w.attach_line(line);
                }
                // 🔴开窗就高亮那一行。**在这里设而不是进 `seeded` 的签名** ——
                //    `seeded` 有 5 处调用点（判据 4 处），而 reveal 只有开窗那一条路用得上。
                if let Some(name) = reveal {
                    w.set_reveal(&name);
                }
                // 书签：问这台机器的后端一次（本机与远端同一形）。
                w.shelf = Some(crate::bookmarks::Shelf::open(
                    w.rt.clone(),
                    w.line.clone(),
                    &w.source.origin(),
                    &w.cwd,
                ));
                w.machines = machines;
                // 样子：主界面那一套配色 · 字号 · 间距。
                if let Some(t) = &theme {
                    crate::theme::install(&cc.egui_ctx, t);
                }
                // 最外一层是 `Workspace`（标签页 ＋ 双栏 ＋ 预览），开窗那一个目录视图是它的第一个标签页。
                let mut ws = crate::workspace::Workspace::new(w);
                // 第一拍：系统字体在另一条线程上载入，第一帧不等它（`fonts.rs §四`）。
                ws.fonts = Some(fonts::load(&cc.egui_ctx, theme.clone()));
                ws.theme = theme;
                // 上次的缩放（主界面的字号随样子交过来，这里在它之上整体缩放）· 预览开没开。
                ws.restore_view(crate::workspace::ViewMemo::open(view));
                // 帧日志：设了 `CCM_FILEWIN_FRAME_LOG=<文件>` 才开（收集「一闪一闪」的证据）。
                ws.frame_log = crate::frame_log::FrameLog::from_env();
                cc.egui_ctx.set_zoom_factor(ws.memo.zoom());
                Ok(Box::new(FitOnce {
                    inner: ws,
                    work: work_area,
                    fitted_at: None,
                }) as Box<dyn eframe::App>)
            }),
        )
        // 句子只说没开成；eframe 的原话跟着走（窗口进程把它印上 stderr，monitor 接进日志与复制详情）。
        .map_err(|e| Said::with_raw(copy_text("rsFilewinShell.window.openFailed", &[]), e))
    })
}

/// 把窗口夹进工作区，之后原样转交：开窗后第一拍（窗口几何有了的那一帧）夹进 monitor 交来的那块；
/// 之后系统缩放一变（改了显示缩放 / 拖到另一块缩放不同的屏）再夹一次，按那一刻窗口所在那块屏的工作区（[`fit_step`]）。
pub(super) struct FitOnce<A> {
    inner: A,
    work: Option<host_core::WorkArea>,
    /// 上次夹的时候系统缩放是多少（`None` ＝ 还没夹过）。
    fitted_at: Option<f32>,
}

impl<A: eframe::App> eframe::App for FitOnce<A> {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.inner.logic(ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // 几何是逻辑点、而逻辑点随整窗缩放变 ⇒ 按此刻的 pixels_per_point（含缩放）换算；系统缩放单看 native 那一份。
        let ppp = ui.ctx().pixels_per_point();
        let (outer, inner, native) = ui.ctx().input(|i| {
            let v = i.viewport();
            (v.outer_rect, v.inner_rect, v.native_pixels_per_point)
        });
        if let (Some(outer), Some(inner), Some(native)) = (outer, inner, native) {
            for cmd in fit_step(
                &mut self.fitted_at,
                native,
                ppp,
                outer,
                inner,
                self.work,
                |(x, y)| host_core::work_area_at(x, y),
            ) {
                ui.ctx().send_viewport_cmd(cmd);
            }
        }
        self.inner.ui(ui, frame);
    }
}

/// [`FitOnce`] 每一帧那一步（纯函数；`fitted_at` 是它的记性）：还没夹过 ⇒ 夹进开窗种子那块（`seed`，`None` ＝ 不夹）；
/// 系统缩放（`native`）比上次夹时变了 ⇒ 按外框中点（像素）问此刻那块屏的工作区（`now`；问不到 ⇒ 不夹 —— 不拿开窗那块屏去拽它）再夹；
/// 别的帧一条命令都不发（人自己拖大的窗口不跟他抢）。
pub(crate) fn fit_step(
    fitted_at: &mut Option<f32>,
    native: f32,
    ppp: f32,
    outer: egui::Rect,
    inner: egui::Rect,
    seed: Option<host_core::WorkArea>,
    now: impl FnOnce((i32, i32)) -> Option<host_core::WorkArea>,
) -> Vec<egui::ViewportCommand> {
    let work = match *fitted_at {
        None => seed,
        Some(was) if (was - native).abs() > 1e-3 => {
            let c = outer.center();
            now(((c.x * ppp).round() as i32, (c.y * ppp).round() as i32))
        }
        Some(_) => return Vec::new(),
    };
    *fitted_at = Some(native);
    work.map(|w| fit_commands(outer, inner, ppp, w))
        .unwrap_or_default()
}

/// [`FitOnce`] 那一帧发什么（纯函数）：egui 的几何是逻辑点，工作区是物理像素 ⇒ 换算后交给同一个判定
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
