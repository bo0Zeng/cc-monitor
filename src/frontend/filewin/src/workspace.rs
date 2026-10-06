//! 窗口的最外一层：**标签页 ＋ 双栏 ＋ 复制到另一栏**（预览开关也挂在这儿，
//! 预览本身住 [`super::preview`]：右侧一块，跟焦点那一栏）。
//!
//! 设计住第三节；这里只留落地要知道的。
//!
//! # 一、形状：`FileWindow` 不拆，它就是一个标签页
//!
//! ```text
//! Workspace
//!   sides: [Side; 1 或 2]      一栏 / 两栏
//!   focus: 栏号               键盘 · 拖入 · 预览 · 「复制到另一栏」的源，都跟它
//! Side  { tabs: [Tab], active }
//! Tab   { id, pane: FileWindow }   // 一个目录视图的全部状态，一个字段没搬
//! ```
//!
//! ⇒ 从前几百条判据全在 `FileWindow` 上，这一刀一条不动；`eframe::App` 从 `FileWindow` 挪到这里。
//! 同一个窗口里所有标签页 / 两栏看的都是**同一台机器**（同一个 `Source`、同一条通道）：换机器就开另一个窗口。
//!
//! # 二、焦点那道闸（为什么非有不可）
//!
//! 两栏同时画 ⇒ 两个 `frame_body` 都会去接这一帧的按键与拖入。不闸的话按一下 Delete 两边各删一次、
//! 拖一个文件进来两边各传一次。⇒ 只有焦点那一栏的那个标签 `focused = true`（[`Workspace::sync_focus`]），
//! 其余一律 `false`（`FileWindow::keys_blocked` / `take_drops` 多问的就是它）。
//! 在哪一栏里按下鼠标，焦点就给哪一栏（这一帧按下、下一帧生效）。
//!
//! # 三、关标签 / 收右栏的规矩
//!
//! 那个标签手上**有活**（有一问摆着、有东西在传 / 在复制 / 在写 / 在读写文本）⇒ 不关，说是哪一件
//! （`FileWindow::busy_reason`）。后台那几趟任务不随标签走，关掉就再也没人把结局摆给你看。
//! 关**整个窗口**同样先看（[`Workspace::guard_close`]）：改了没存 ⇒ 切到那一页摆它那一问；
//! 手上有活 ⇒ 切过去说同一句话；同一句话摆着时再点一次关窗 ⇒ 照关（用户坚持）。
//!
//! # ⚠ 买不到什么
//!
//! - 真窗口真画在屏幕上（要图形会话）；判据跑的是生产那个 [`Workspace::frame`]，读这一帧画出来的字。
//! - **拖**一行到另一栏做了（行上命中矩形换成 `click_and_drag`，松手落在另一栏 ⇒ 同一个「复制到另一栏」入口，
//!   [`Workspace::settle_drag`]）；判据喂的是合成指针事件，真鼠标买不到。
//! - 后台标签（不在任何一栏上）的「一问」要切回去才看得见；标签名前那个「●」就是为这个。

use super::copy::CopyJob;
use super::shell::FileWindow;
use super::source::Listed;
use copy_core::copy_text;

/// 一个标签页：一个目录视图。
pub struct Tab {
    /// 这个标签的 egui 身份（不随下标移位：关掉左边那个，右边这个的滚动位置不串）。
    pub id: u64,
    pub pane: FileWindow,
}

/// 一栏：一排标签页，当前显示第几个。
pub struct Side {
    pub tabs: Vec<Tab>,
    pub active: usize,
}

/// 关窗那一问要列的：没保存的编辑页（名字）＋ 还在跑的那几趟（`(族名, [(图标, 名字, 右端那一格)])`）。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CloseAsk {
    pub unsaved: Vec<String>,
    pub running: Vec<(String, Vec<(&'static str, String, String)>)>,
}

impl CloseAsk {
    /// 关了什么都不丢。
    pub fn is_empty(&self) -> bool {
        self.unsaved.is_empty() && self.running.is_empty()
    }
}

/// 窗口的最外一层。
pub struct Workspace {
    sides: Vec<Side>,
    focus: usize,
    next_id: u64,
    /// 上一件「做不了」时说的那句话（`None` ＝ 没话说）。下一次点工具条 / 标签栏就清。
    notice: Option<String>,
    /// 〔预览〕开着吗 ＋ 它自己的状态（`None` ＝ 关着）。逻辑住 [`super::preview`]。
    pub preview: Option<super::preview::Preview>,
    /// 这扇窗的样子（开窗种子带来的；左栏开另一台时原样交给新窗口）。`None` ＝ 判据那一形。
    pub theme: Option<filewin_contract::Theme>,
    /// 左栏摊开着吗（命令栏最左那颗切）。窗口状态，关窗即没。
    pub sidebar_open: bool,
    /// 这台机器的家目录（左栏「家目录」那一格；问一次，整扇窗共用）。
    pub(super) home: super::chrome::Home,
    /// 左栏「家目录」在问的时候被点了：问到了就去。
    pub(super) home_go: bool,
    /// 左栏「其他机器」那一问的结局。
    pub(super) other: super::chrome::OtherSlot,
    /// 整窗缩放（记在视图文件里，下次开窗照它）。
    pub zoom: Zoom,
    /// 右下角那一摞回执（一次性的那几句：键位做不成的原因 · 跳到隐藏文件 · 开另一台 …）。
    pub toasts: super::kit::Toasts,
    /// 带［撤销］的回执：记号 → 撤销要做的那几件（点了交给焦点那一栏直接做）。
    undo_for: std::collections::HashMap<u64, Vec<super::writeops::WriteOp>>,
    undo_tag: u64,
    /// 上一句收成回执的话（同一句摆着不再收第二次）。
    toasted: Option<String>,
    /// 上一帧发出去的窗口标题（变了才再发）。
    title: Option<String>,
    /// 窗口底部那张「进度」表（每个标签页拿的是同一份；关掉标签页不带走它上面的那几趟）。
    pub progress: super::progress::Progress,
    /// 那台此刻连没连着（每个标签页拿同一份）。
    pub link: super::chrome::LinkState,
    /// 上一次照着「又连上了」重列过的次数（与 `link.ups()` 比）。
    link_ups: u64,
    /// 关窗那一问摆着（稿 17）。
    closing_ask: bool,
    /// 那一问答了「全部保存后关闭」：每一页存成了就关。
    close_after_save: bool,
    /// 那一问答了「仍然关闭」/ 那一问摆着时又点了一次 ×：下一次关窗请求放行。
    force_close: bool,
}

/// 缩放的上下限与一步多少（Ctrl + = / -）。
pub const ZOOM_MIN: f32 = 0.8;
pub const ZOOM_MAX: f32 = 2.0;
/// 缩放的档位（规范 `I8` 与稿 ⑫）。
pub const ZOOM_STOPS: [f32; 8] = [0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

/// 整窗缩放（egui 的 `zoom_factor`）：Ctrl + = / Ctrl + - / Ctrl + 0、Ctrl + 滚轮改它；
/// 记在 monitor 交来的那份视图文件里（`{"zoom": 1.2}`），下次开窗照它。没有那份文件 ⇒ 照样能缩放，只是不记。
/// 主界面的「字号」是另一回事（随开窗的样子交过来的绝对字号）；这里是在它之上整体放大缩小。
#[derive(Clone, Debug, Default)]
pub struct Zoom {
    file: Option<std::path::PathBuf>,
    /// 文件里记着的那个数（没记过 ⇒ 1）。
    saved: Option<f32>,
}

impl Zoom {
    /// 读那份文件（读不到 / 读不懂 ⇒ 按 1 起，下一次改了就写一份新的）。
    pub fn open(file: Option<std::path::PathBuf>) -> Self {
        let saved = file
            .as_deref()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .and_then(|v| v.get("zoom").and_then(serde_json::Value::as_f64))
            .map(|z| (z as f32).clamp(ZOOM_MIN, ZOOM_MAX));
        Self { file, saved }
    }

    /// 开窗时照哪个数缩放。
    pub fn factor(&self) -> f32 {
        self.saved.unwrap_or(1.0)
    }

    /// 这一帧的缩放与记着的不同 ⇒ 记下来（两位小数，同一个数不重写）。
    fn remember(&mut self, z: f32) {
        let z = (z * 100.0).round() / 100.0;
        if self.saved == Some(z) || (self.saved.is_none() && z == 1.0) {
            return;
        }
        self.saved = Some(z);
        if let Some(f) = &self.file {
            if let Err(e) = host_core::atomic_write_json(f, &serde_json::json!({ "zoom": z })) {
                tracing::warn!(error = %e, "zoom not saved");
            }
        }
    }
}

/// 工具条上那几颗 —— **唯一住址**（判据按同一个常量去找它画出来的字）。
pub static SPLIT_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinWorkspace.label.split", &[]));
pub static PREVIEW_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinWorkspace.label.preview", &[]));
pub static COPY_ACROSS_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinWorkspace.label.copyAcross", &[]));

/// 标签页快捷键想干什么。**只是意图**，做不做由 [`Workspace::apply_tab_keys`] 过闸。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKey {
    /// Ctrl+T（macOS ⌘T）＝ 焦点那一栏的「＋」。
    New,
    /// Ctrl+W（macOS ⌘W）＝ 焦点那一栏当前标签的「×」（关的是标签页，不是窗口）。
    Close,
}

/// 这一帧的事件 → 标签页快捷键（按到达顺序，只认按下）。
///
/// ⚠ 与 `select::intents` 分住两处是**归属**，不是风格：列表的键归那个目录视图，标签页归工作区
/// （一个目录视图不知道自己在哪一栏、有几个兄弟）。两张键位表不相交（`select` 那张里带 Ctrl 的只有 Ctrl+A），
/// Ctrl 按着时 egui-winit 不发 `Text` ⇒ 也不会同时触发打字跳转。
pub fn tab_keys(events: &[egui::Event]) -> Vec<TabKey> {
    events
        .iter()
        .filter_map(|ev| match ev {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers: m,
                ..
            } if m.command && !m.shift && !m.alt => match key {
                egui::Key::T => Some(TabKey::New),
                egui::Key::W => Some(TabKey::Close),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

impl Workspace {
    /// 从开窗那一个标签页起步（它身上已经挂好了通道 · 运行时 · 书签 · 字体）。
    pub fn new(mut first: FileWindow) -> Self {
        first.in_workspace = true;
        let progress = first.progress.clone();
        let link = first.link.clone();
        let mut w = Self {
            sides: vec![Side {
                tabs: vec![Tab { id: 0, pane: first }],
                active: 0,
            }],
            focus: 0,
            next_id: 1,
            notice: None,
            preview: None,
            theme: None,
            sidebar_open: true,
            home: Default::default(),
            home_go: false,
            other: Default::default(),
            zoom: Zoom::default(),
            toasts: Default::default(),
            undo_for: Default::default(),
            undo_tag: 0,
            toasted: None,
            title: None,
            progress,
            link,
            link_ups: 0,
            closing_ask: false,
            close_after_save: false,
            force_close: false,
        };
        w.sync_focus();
        w
    }

    /// 几栏（1 或 2）。
    pub fn sides(&self) -> usize {
        self.sides.len()
    }

    /// 第 `side` 栏有几个标签页。
    pub fn tabs_on(&self, side: usize) -> usize {
        self.sides[side].tabs.len()
    }

    /// 第 `side` 栏当前显示第几个标签页。
    pub fn active_on(&self, side: usize) -> usize {
        self.sides[side].active
    }

    /// 焦点在第几栏。
    pub fn focus(&self) -> usize {
        self.focus
    }

    /// 第 `side` 栏正显示的那个目录视图。
    pub fn pane_on(&self, side: usize) -> &FileWindow {
        let s = &self.sides[side];
        &s.tabs[s.active].pane
    }

    /// 同上，可改。
    pub fn pane_on_mut(&mut self, side: usize) -> &mut FileWindow {
        let s = &mut self.sides[side];
        &mut s.tabs[s.active].pane
    }

    /// 第 `side` 栏第 `i` 个标签页（判据用：后台标签也看得到）。
    pub fn tab(&self, side: usize, i: usize) -> &FileWindow {
        &self.sides[side].tabs[i].pane
    }

    /// 上一件做不了时那句话。
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// 在命令栏上说一句（下一次点工具条 / 标签栏就清）。
    pub(super) fn set_notice(&mut self, n: String) {
        self.notice = Some(n);
    }

    pub(super) fn set_notice_none(&mut self) {
        self.notice = None;
    }

    /// 焦点只给焦点那一栏的那个标签，其余一律关掉（本模块头注 §二）。
    fn sync_focus(&mut self) {
        for (k, s) in self.sides.iter_mut().enumerate() {
            for (i, t) in s.tabs.iter_mut().enumerate() {
                t.pane.focused = k == self.focus && i == s.active;
            }
        }
    }

    /// 把焦点给第 `side` 栏。
    pub fn focus_side(&mut self, side: usize) {
        if side < self.sides.len() {
            self.focus = side;
            self.sync_focus();
        }
    }

    /// 照 `like` 那个目录视图的样子起一个新的，落在 `cwd`：同一台机器、同一条通道、同一份书签、
    /// 同一个字体结论（字体装在整个窗口上，不是装在某一个标签上）。**建完就列一趟目录。**
    /// 名字不是合法 UTF-8 的目录照样落得下（带着 `like` 当前目录的原始字节）。
    fn spawn_pane(like: &FileWindow) -> FileWindow {
        let mut p = FileWindow::seeded(
            like.source.clone(),
            like.cwd.clone(),
            like.rt.clone(),
            Vec::<Listed>::new(),
        );
        p.cwd_raw = like.cwd_raw.clone();
        if let Some(line) = like.line.clone() {
            p.attach_line(line);
        }
        p.shelf = like.shelf.clone();
        p.machines = like.machines.clone();
        p.font = like.font.clone();
        p.progress = like.progress.clone();
        p.link = like.link.clone();
        p.in_workspace = true;
        p.reload();
        p
    }

    fn mint_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// 在第 `side` 栏开一个新标签页，落在这一栏当前的目录上，并切过去。
    pub fn open_tab(&mut self, side: usize) -> bool {
        if side >= self.sides.len() {
            return false;
        }
        let pane = Self::spawn_pane(self.pane_on(side));
        self.add_tab(side, pane)
    }

    /// 把一个建好的目录视图作为新标签页挂到第 `side` 栏，并切过去（焦点随之给这一栏）。
    pub fn add_tab(&mut self, side: usize, mut pane: FileWindow) -> bool {
        if side >= self.sides.len() {
            return false;
        }
        pane.progress = self.progress.clone();
        pane.link = self.link.clone();
        pane.in_workspace = true;
        let id = self.mint_id();
        let s = &mut self.sides[side];
        s.tabs.push(Tab { id, pane });
        s.active = s.tabs.len() - 1;
        self.focus = side;
        self.notice = None;
        self.sync_focus();
        true
    }

    /// 把一个建好的目录视图挂成右栏（只在一栏时；焦点给它）。回值 ＝ 真的挂上了。
    pub fn add_side(&mut self, mut pane: FileWindow) -> bool {
        if self.sides.len() != 1 {
            return false;
        }
        pane.progress = self.progress.clone();
        pane.link = self.link.clone();
        pane.in_workspace = true;
        let id = self.mint_id();
        self.sides.push(Side {
            tabs: vec![Tab { id, pane }],
            active: 0,
        });
        self.focus = 1;
        self.notice = None;
        self.sync_focus();
        true
    }

    /// 切到第 `side` 栏第 `i` 个标签页（焦点随之给这一栏）。
    pub fn select_tab(&mut self, side: usize, i: usize) -> bool {
        if side >= self.sides.len() || i >= self.sides[side].tabs.len() {
            return false;
        }
        self.sides[side].active = i;
        self.focus = side;
        self.notice = None;
        self.sync_focus();
        true
    }

    /// 关掉第 `side` 栏第 `i` 个标签页。**一栏最后一个不关**（要关就关窗口 / 收掉这一栏）；
    /// **手上有活的不关**，出声说是哪一件。
    pub fn close_tab(&mut self, side: usize, i: usize) -> bool {
        if side >= self.sides.len() || i >= self.sides[side].tabs.len() {
            return false;
        }
        if self.sides[side].tabs.len() == 1 {
            self.notice = Some(copy_text("rsFilewinWorkspace.closeTab.last", &[]).into());
            return false;
        }
        // 编辑页：改了没存 ⇒ 切过去、摆「关闭 x · 未保存」那一问（答了再关，`settle_tab_wants`）；没改 ⇒ 直接关。
        if self.sides[side].tabs[i].pane.edit_tab {
            let dirty = self.sides[side].tabs[i]
                .pane
                .editing()
                .is_some_and(super::editor::Pane::dirty);
            if dirty {
                self.select_tab(side, i);
                self.sides[side].tabs[i].pane.close_edit();
                return false;
            }
            return self.drop_tab(side, i);
        }
        if let Some(why) = self.sides[side].tabs[i].pane.busy_reason() {
            self.notice = Some(copy_text(
                "rsFilewinWorkspace.closeTab.busy",
                &[("why", &why.to_string())],
            ));
            return false;
        }
        self.drop_tab(side, i)
    }

    /// 真把第 `side` 栏第 `i` 个标签页拿掉（不再问）。它开过的那几趟照跑（「进度」表里的行），落地后由窗口这一级重列目录。
    fn drop_tab(&mut self, side: usize, i: usize) -> bool {
        if self.sides[side].tabs.len() <= 1 || i >= self.sides[side].tabs.len() {
            return false;
        }
        let s = &mut self.sides[side];
        s.tabs[i].pane.orphan_jobs();
        s.tabs.remove(i);
        if s.active > i || s.active >= s.tabs.len() {
            s.active = s.active.saturating_sub(1);
        }
        self.notice = None;
        self.sync_focus();
        true
    }

    /// 每帧一次：目录页要开的那一份 ⇒ 在同一栏开一个编辑页（同一份已开着 ⇒ 切过去，不开第二份）；
    /// 编辑页点了面包屑 / 「在列表里显示」⇒ 同一栏切到 / 开一个目录页到那里；编辑页要关自己 ⇒ 关掉。
    pub fn settle_tab_wants(&mut self, ctx: Option<egui::Context>) {
        for k in 0..self.sides.len() {
            let mut i = 0;
            while i < self.sides[k].tabs.len() {
                if let Some(w) = self.sides[k].tabs[i].pane.want_edit.take() {
                    let open = self.sides[k].tabs.iter().position(|t| {
                        t.pane.edit_tab
                            && t.pane.edit_path().as_deref() == Some(w.row.path.as_str())
                    });
                    match open {
                        Some(at) => {
                            self.select_tab(k, at);
                        }
                        None => {
                            let mut p = Self::spawn_editor(&self.sides[k].tabs[i].pane);
                            p.begin_edit_at(w.row, w.at, w.refused, ctx.clone());
                            self.add_tab(k, p);
                        }
                    }
                }
                if let Some(dir) = self.sides[k].tabs[i].pane.want_dir.take() {
                    let reveal = self.sides[k].tabs[i].pane.want_reveal.take();
                    let there = self.sides[k]
                        .tabs
                        .iter()
                        .position(|t| !t.pane.edit_tab && t.pane.cwd == dir);
                    let at = match there {
                        Some(at) => {
                            self.select_tab(k, at);
                            at
                        }
                        None => {
                            let mut p = Self::spawn_pane(&self.sides[k].tabs[i].pane);
                            p.navigate_to(dir);
                            self.add_tab(k, p);
                            self.sides[k].tabs.len() - 1
                        }
                    };
                    if let Some(n) = reveal {
                        self.sides[k].tabs[at].pane.set_reveal(&n);
                    }
                }
                if std::mem::take(&mut self.sides[k].tabs[i].pane.want_close) {
                    if self.drop_tab(k, i) {
                        continue;
                    }
                }
                i += 1;
            }
        }
    }

    /// 照 `like` 那个目录页的样子起一个**编辑页**（同一台 · 同一条线 · 同一份书签 / 字体 / 进度表 / 连接状态），不列目录。
    fn spawn_editor(like: &FileWindow) -> FileWindow {
        let mut p = FileWindow::seeded(
            like.source.clone(),
            like.cwd.clone(),
            like.rt.clone(),
            Vec::<Listed>::new(),
        );
        p.cwd_raw = like.cwd_raw.clone();
        if let Some(line) = like.line.clone() {
            p.attach_line(line);
        }
        p.shelf = like.shelf.clone();
        p.machines = like.machines.clone();
        p.font = like.font.clone();
        p.progress = like.progress.clone();
        p.link = like.link.clone();
        p.in_workspace = true;
        p.edit_tab = true;
        p
    }

    /// 开 / 收双栏。开 ⇒ 右边长出一栏（一个标签页，落在焦点那一栏的目录上），焦点给它；
    /// 收 ⇒ 右栏整栏收掉（里面有哪个标签手上有活就不收，出声）。回值 ＝ 真的变了。
    pub fn set_split(&mut self, on: bool) -> bool {
        match (on, self.sides.len()) {
            (true, 1) => {
                let pane = Self::spawn_pane(self.pane_on(self.focus));
                return self.add_side(pane);
            }
            (false, 2) => {
                if let Some(why) = self.sides[1].tabs.iter().find_map(|t| t.pane.busy_reason()) {
                    self.notice = Some(copy_text(
                        "rsFilewinWorkspace.setSplit.busy",
                        &[("why", &why.to_string())],
                    ));
                    return false;
                }
                if let Some(mut s) = self.sides.pop() {
                    for t in &mut s.tabs {
                        t.pane.orphan_jobs();
                    }
                }
                self.focus = 0;
            }
            _ => return false,
        }
        self.notice = None;
        self.sync_focus();
        true
    }

    /// 这一帧的 Ctrl+T / Ctrl+W。回值 ＝ 认出了几件（做不了的那几形由 `open_tab` / `close_tab` 出声）。
    ///
    /// 🔴 **闸与列表同一道**：焦点那一栏当前那个标签的 `keys_blocked`（四道闸 ——
    /// 模态框 · 右键菜单 · 搜索命中那一摞 · 控件拿着键盘焦点）。不另立一道：分成两份的症状是
    /// 「框开着，按 Ctrl+W 把框底下那个标签关了」。
    pub fn apply_tab_keys(&mut self, ctx: &egui::Context) -> usize {
        if self.pane_on(self.focus).keys_blocked(ctx) {
            return 0;
        }
        let keys = ctx.input(|i| tab_keys(&i.events));
        for k in &keys {
            let side = self.focus;
            match k {
                TabKey::New => {
                    self.open_tab(side);
                }
                TabKey::Close => {
                    let i = self.active_on(side);
                    self.close_tab(side, i);
                }
            }
        }
        keys.len()
    }

    /// 开 / 关预览。关 ＝ 整块状态扔掉（在飞的那一趟回来没人收，无害）。
    pub fn set_preview(&mut self, on: bool) {
        self.preview = on.then(super::preview::Preview::default);
    }

    /// 🔴 **复制到另一栏**：焦点那一栏选中的**那一摞**（文件与目录都行）→ 另一栏当前目录、同名。
    ///
    /// 〔「复制目录 · 批量复制」〕此前只收「恰好一个文件」。今天走一摞复制
    /// （[`super::copy::run_copy_batch`]：逐件探目标 → 撞名的目录整摞不做 → 撞名的文件一次问完 → 逐件发），
    /// 后端 `files-copy`（目录带 `recursive: true`），**起在目标那一栏上** ⇒ 问与结局画在那一侧，跑完那一栏重列目录。
    /// `§6.3`：每一项都能复制才给（有损名那一项在，整摞不做、出声）。回值 ＝ 真的起来了。
    pub fn copy_to_other(&mut self, ctx: Option<egui::Context>) -> bool {
        if self.sides.len() != 2 {
            self.notice = Some(copy_text("rsFilewinWorkspace.copyToOther.needSplit", &[]).into());
            return false;
        }
        let from = self.pane_on(self.focus);
        let rows = from.picked_rows();
        if rows.is_empty() {
            self.notice = Some(copy_text("rsFilewinSelect.refusal.none", &[]));
            return false;
        }
        if let Some(r) = rows.iter().find(|r| !super::copy::copyable(r)) {
            self.notice = Some(copy_text(
                "rsFilewinWorkspace.copyToOther.unaddressable",
                &[("name", &r.name.to_string())],
            ));
            return false;
        }
        let other = 1 - self.focus;
        let dest_dir = self.pane_on(other).cwd.clone();
        let dest_path = self.pane_on(other).cwd_path();
        if dest_path == from.cwd_path() {
            self.notice = Some(copy_text("rsFilewinWorkspace.copyToOther.sameDir", &[]).into());
            return false;
        }
        let jobs: Vec<CopyJob> = rows
            .iter()
            .map(|r| {
                // 〔有损名全寻址〕源 ＝ 源那一栏当前目录的字节 ＋ 名字的字节；目标 ＝ 另一栏当前目录的字节 ＋ 同一个名字。
                let src = from.row_path(r);
                let dst = super::shell::join_path(&dest_path, &super::shell::name_bytes(r));
                CopyJob {
                    from: src.shown.clone(),
                    to: if dst.is_lossy() {
                        dst.shown.clone()
                    } else {
                        super::writeops::join_remote(&dest_dir, &r.name)
                    },
                    name: r.name.clone(),
                    is_dir: r.is_dir,
                    from_raw: src.raw,
                    to_raw: dst.raw,
                }
            })
            .collect();
        self.notice = None;
        self.pane_on_mut(other).start_copy_batch(jobs, ctx)
    }

    /// 标签上写什么：当前目录的最后一段（根就写 `/`）；手上有事的前面加「●」。
    pub fn tab_title(pane: &FileWindow) -> String {
        if pane.edit_tab {
            return pane.edit_name();
        }
        let tail = super::source::remote_basename(&pane.cwd);
        let tail = if tail.is_empty() { "/" } else { tail };
        tail.to_string()
    }

    /// 🔴 **点了关窗**（这一帧的输入里有关窗请求）：什么都不会丢 ⇒ 直接关；有没保存的编辑页 / 还在跑的那几趟 ⇒
    /// 拦下（`CancelClose`）、摆「关闭文件窗口 · devbox」那一问（稿 17，[`Self::close_ask_ui`]）。
    /// 那一问摆着时又点了一次 × ⇒ 照关（修缺陷那一路交的，认）。答了「全部保存后关闭」⇒ 每一页存成了就关；有一页没存成 ⇒ 作罢（那一页上的条说为什么）。
    pub fn guard_close(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            if self.close_after_save && !self.panes().any(|p| p.edits.save_pending()) {
                self.close_after_save = false;
                if !self.panes().any(|p| p.editing().is_some_and(|e| e.dirty())) {
                    self.force_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            return;
        }
        if self.force_close || self.closing_ask || self.close_ask().is_empty() {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        self.closing_ask = true;
    }

    /// 关窗会打断什么：没保存的编辑页（名字）＋ 「进度」表里还在跑的那几趟（按族）。
    pub fn close_ask(&self) -> CloseAsk {
        let unsaved = self
            .panes()
            .filter(|p| p.editing().is_some_and(super::editor::Pane::dirty))
            .map(FileWindow::edit_name)
            .collect();
        CloseAsk {
            unsaved,
            running: self.progress.running_for_close(),
        }
    }

    /// 摆出关窗那一问（有东西会丢时；点 × 那一下走 [`Self::guard_close`]，截图与判据直接调它）。回值 ＝ 摆出来了。
    pub fn begin_close_ask(&mut self) -> bool {
        if self.close_ask().is_empty() {
            return false;
        }
        self.closing_ask = true;
        true
    }

    /// 关窗那一问正摆着吗。
    pub fn closing_ask(&self) -> bool {
        self.closing_ask
    }

    /// 「关闭文件窗口 · devbox」（规范 `C10` · `I12`）：按族列出会打断的（未保存 · 传输中 · 删除中 · 解压中 · 计算大小中）；
    /// 按钮「取消」（焦点）·「全部保存后关闭」（只在有没保存的时出现）·「仍然关闭」（危险）。
    pub fn close_ask_ui(&mut self, ctx: &egui::Context) {
        if !self.closing_ask {
            return;
        }
        let ask = self.close_ask();
        if ask.is_empty() {
            // 问着的工夫那几件都完了 ⇒ 没什么可问的了，收掉（不替人关窗）。
            self.closing_ask = false;
            return;
        }
        let p = super::theme::palette(ctx);
        let machine = self.pane_on(self.focus).source.label();
        let mut buttons = vec![(
            copy_text("rsFilewinWorkspace.closeAsk.cancel", &[]),
            super::kit::Btn::Plain,
        )];
        let save_all = !ask.unsaved.is_empty();
        if save_all {
            buttons.push((
                copy_text("rsFilewinWorkspace.closeAsk.saveAll", &[]),
                super::kit::Btn::Plain,
            ));
        }
        buttons.push((
            copy_text("rsFilewinWorkspace.closeAsk.force", &[]),
            super::kit::Btn::Danger,
        ));
        let force_at = buttons.len() - 1;
        let hit = super::kit::dialog(
            ctx,
            "filewin-close-ask",
            &copy_text(
                "rsFilewinWorkspace.closeAsk.title",
                &[("machine", &machine)],
            ),
            |ui| {
                egui::Frame::new()
                    .fill(p.bg2)
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let head = |ui: &mut egui::Ui, t: String| {
                            ui.label(egui::RichText::new(t).small().strong().color(p.text2));
                        };
                        if !ask.unsaved.is_empty() {
                            head(ui, copy_text("rsFilewinWorkspace.closeAsk.unsaved", &[]));
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(egui_phosphor::regular::PENCIL_SIMPLE)
                                        .color(p.text2),
                                );
                                ui.label(
                                    egui::RichText::new(ask.unsaved.join(&copy_text(
                                        "rsFilewinWorkspace.closeAsk.listSep",
                                        &[],
                                    )))
                                    .color(p.text),
                                );
                            });
                        }
                        for (group, rows) in &ask.running {
                            ui.add_space(4.0);
                            head(ui, group.clone());
                            for (icon, name, right) in rows {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(*icon).color(p.text2));
                                    ui.label(egui::RichText::new(name).color(p.text));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                egui::RichText::new(right).small().color(p.text2),
                                            );
                                        },
                                    );
                                });
                            }
                        }
                    });
            },
            &buttons,
            0,
            0,
        );
        match hit {
            Some(0) => self.closing_ask = false,
            Some(i) if i == force_at => {
                self.closing_ask = false;
                self.force_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(_) if save_all => {
                self.closing_ask = false;
                self.close_after_save = true;
                for s in &mut self.sides {
                    for t in &mut s.tabs {
                        if t.pane.editing().is_some_and(super::editor::Pane::dirty) {
                            t.pane.save_edit(Some(ctx.clone()));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// 状态栏那颗「125%」点了：回 100% 并记下来。
    pub fn zoom_reset(&mut self) {
        self.zoom.remember(1.0);
    }

    /// 这一帧的缩放手势：Ctrl + = / + 放大一步 · Ctrl + - 缩小一步 · Ctrl + 0 回 100% · Ctrl + 滚轮按滚的量；
    /// 改了就记下来。回值 ＝ 这一帧之后的缩放。
    pub fn apply_zoom(&mut self, ctx: &egui::Context) -> f32 {
        use egui::{Key, KeyboardShortcut as K, Modifiers as M};
        let mut z = ctx.zoom_factor();
        // 档位照稿：80 · 90 · 100 · 110 · 125 · 150 · 175 · 200%（键盘一档一档走；滚轮照滚的量，落在两档之间也行）。
        let step = |z: f32, d: f32| {
            if d > 0.0 {
                ZOOM_STOPS
                    .iter()
                    .copied()
                    .find(|&s| s > z + 0.001)
                    .unwrap_or(ZOOM_MAX)
            } else {
                ZOOM_STOPS
                    .iter()
                    .rev()
                    .copied()
                    .find(|&s| s < z - 0.001)
                    .unwrap_or(ZOOM_MIN)
            }
        };
        ctx.input_mut(|i| {
            if i.consume_shortcut(&K::new(M::COMMAND, Key::Num0)) {
                z = 1.0;
            }
            while i.consume_shortcut(&K::new(M::COMMAND, Key::Equals))
                || i.consume_shortcut(&K::new(M::COMMAND, Key::Plus))
            {
                z = step(z, 1.0);
            }
            while i.consume_shortcut(&K::new(M::COMMAND, Key::Minus)) {
                z = step(z, -1.0);
            }
            z *= i.zoom_delta();
        });
        let z = z.clamp(ZOOM_MIN, ZOOM_MAX);
        if z != ctx.zoom_factor() {
            ctx.set_zoom_factor(z);
        }
        self.zoom.remember(z);
        z
    }

    /// 每帧一次：「进度」表落地那几趟（[`super::progress::Progress::settle`]）；
    /// 开它的标签页已经关了 / 换了一块新看板的那几趟 ⇒ 正开在那个目录的标签页重列一次。
    pub fn settle_progress(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let dirs = self.progress.settle(now);
        if dirs.is_empty() {
            return;
        }
        for s in &mut self.sides {
            for t in &mut s.tabs {
                if dirs.contains(&t.pane.cwd) {
                    t.pane.reload();
                }
            }
        }
    }

    /// 起「那台连没连着」那条订阅（整扇窗一次，拿焦点那一栏的线）；断了又连上 ⇒ 每个标签页重列一遍（过期那一屏换成新的）。
    pub fn settle_link(&mut self, ctx: Option<egui::Context>) {
        let f = self.focus;
        let p = self.pane_on(f);
        if let (Some(rt), Some(line)) = (p.rt.clone(), p.line.clone()) {
            self.link.watch(&rt, line, p.source.origin(), ctx);
        }
        let ups = self.link.ups();
        if ups != self.link_ups {
            self.link_ups = ups;
            for s in &mut self.sides {
                for t in &mut s.tabs {
                    t.pane.reload();
                }
            }
        }
    }

    /// 断线条上「重新连接」：叫醒 monitor 里那台的连接循环（不等退避睡满）。
    pub fn retry_link(&mut self) -> bool {
        let p = self.pane_on(self.focus);
        let (Some(rt), Some(line)) = (p.rt.clone(), p.line.clone()) else {
            return false;
        };
        let origin = p.source.origin();
        rt.spawn(async move { super::source::kick_link(&line, &origin).await });
        true
    }

    /// 「进度」表上按下的那一颗：停 ⇒ 拨那一趟的撤单；重试 / 接着传 ⇒ 在焦点那一栏起一趟新的（同一台机器、同一条线）。
    pub fn apply_progress_act(
        &mut self,
        a: super::progress::Act,
        ctx: Option<egui::Context>,
    ) -> bool {
        use super::progress::Act;
        let f = self.focus;
        match a {
            Act::Stop(id) => self.progress.stop(id),
            Act::RetryUpload(items) | Act::Resume(items) => {
                self.pane_on_mut(f).start_drop_items(items, ctx)
            }
            Act::RetryDownload { src, dest } => {
                self.pane_on_mut(f).start_pull(&src, &dest, false, ctx)
            }
        }
    }

    /// 每一栏的每一个标签页。
    fn panes(&self) -> impl Iterator<Item = &FileWindow> {
        self.sides
            .iter()
            .flat_map(|s| s.tabs.iter().map(|t| &t.pane))
    }

    /// 🔴 **每一帧的正文**（`eframe::App::ui` 只剩一句委派，判据直接喂它 —— 同 `FileWindow::frame_body`）。
    pub fn frame(&mut self, ui: &mut egui::Ui) {
        // ── 目录页要开的那一份 ⇒ 开一个编辑页（同一份已开着 ⇒ 切过去）；编辑页要去的目录 · 要关自己 ──
        self.settle_tab_wants(Some(ui.ctx().clone()));
        // ── 整窗缩放：Ctrl + = / - / 0 · Ctrl + 滚轮 ──
        let ctx = ui.ctx().clone();
        self.apply_zoom(&ctx);
        // ── 标签页快捷键：先于两栏的正文（那里才是列表接键盘的地方）──
        self.apply_tab_keys(&ctx);
        // ── 导航键（Alt+← / → · 鼠标侧键 · F5 · Ctrl+L · Ctrl+F）· 开另一台那一问落地 ──
        self.apply_nav_keys(&ctx);
        self.settle_other();
        // ── 「进度」表：新落地的那几趟记下时刻 · 失败就摊开 · 开它的标签页已经不管了的，由这一级重列目录 ──
        self.settle_progress();
        // ── 那台连没连着：订一次那条流；断了又连上 ⇒ 每个标签页重列一遍 ──
        self.settle_link(Some(ctx.clone()));
        // ── 窗口的框：工具条 · 命令栏 · 状态栏 · 「进度」表 · 左栏（`chrome.rs`）──
        self.chrome_ui(ui);
        // ── 预览（右侧一块，跟焦点那一栏）──
        let mut preview_act = None;
        if self.preview.is_some() {
            let ctx = ui.ctx().clone();
            let side = &self.sides[self.focus];
            let pane = &side.tabs[side.active].pane;
            if let Some(p) = self.preview.as_mut() {
                p.follow(pane, Some(ctx.clone()));
                // 窗口窄了预览跟着让（不把两栏挤没）。
                let room = ctx.content_rect().width();
                egui::Panel::right("filewin-preview-panel")
                    .frame(super::theme::pane_frame(&ctx))
                    .resizable(true)
                    .default_size(380f32.min(room * 0.4))
                    .min_size(260f32.min(room * 0.3))
                    .max_size(room * 0.6)
                    // 内容先铺满：不然面板会把「这一帧用了多宽」记成自己的宽，开出来就只剩一条窄缝。
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        let act = p.ui(ui);
                        if act.is_some() {
                            preview_act = act;
                        }
                    });
            }
        }
        // 预览头上点了「编辑」「下载」⇒ 照焦点那一栏选中的那一项做（同右键菜单那一件）。
        if let Some(a) = preview_act {
            let f = self.focus;
            let action = match a {
                super::preview::PreviewAct::Edit => super::select::Action::Edit,
                super::preview::PreviewAct::Download => super::select::Action::Download,
            };
            self.pane_on_mut(f).perform(action, Some(ctx.clone()));
        }
        // ── 一栏 / 两栏：主底那一块（没有它，框之间露出来的是 egui 自带的灰黑底）──
        let bg = super::theme::palette(ui.ctx()).bg;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(bg))
            .show(ui, |ui| self.sides_ui(ui));
        // ── 「回主目录」（打不开那一条上点的；主目录住这一级）──
        let f = self.focus;
        if std::mem::take(&mut self.pane_on_mut(f).want_home) {
            match self.home_known() {
                Some(Ok(h)) => self.pane_on_mut(f).navigate_to(h),
                _ => {
                    self.home_go = true;
                    self.ask_home(Some(ctx.clone()));
                }
            }
        }
        // ── 窗口标题跟着焦点那一栏当前的标签页 ──
        let title = super::shell::window_title(
            &Self::tab_title(self.pane_on(f)),
            &self.pane_on(f).source.label(),
        );
        if self.title.as_deref() != Some(title.as_str()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = Some(title);
        }
        // ── 窗口那一级一次性的那几句（开另一台 · 关不了的原因 …）⇒ 右下角回执；键位做不成的那一下浮在状态栏左端 ──
        for s in &mut self.sides {
            for t in &mut s.tabs {
                if let Some(r) = t.pane.receipt.take() {
                    self.toasts.push(r, None);
                }
                if let Some((r, undo)) = t.pane.receipt_undo.take() {
                    self.undo_tag += 1;
                    self.undo_for.insert(self.undo_tag, undo);
                    self.toasts.push_tagged(
                        r,
                        Some(copy_text("rsFilewinWriteops.receipt.undo", &[])),
                        self.undo_tag,
                    );
                }
            }
        }
        let said = self.notice.clone();
        if said != self.toasted {
            if let Some(t) = &said {
                self.toasts.push(t.clone(), None);
            }
            self.toasted = said;
        }
        if let Some(tag) = self.toasts.show(&ctx, 36.0) {
            if let Some(ops) = self.undo_for.remove(&tag) {
                let f = self.focus;
                self.pane_on_mut(f).start_undo(ops, Some(ctx.clone()));
            }
        }
        // 已经过了时的那几条回执带的撤销不再留着。
        let live: std::collections::HashSet<u64> =
            self.toasts.items.iter().map(|t| t.tag).collect();
        self.undo_for.retain(|k, _| live.contains(k));
        self.close_ask_ui(&ctx);
    }

    /// 一栏 / 两栏并排摆（在主底那一块里）。
    fn sides_ui(&mut self, ui: &mut egui::Ui) {
        let whole = ui.available_rect_before_wrap();
        let n = self.sides.len();
        let gap = ui.spacing().item_spacing.x;
        let w = (whole.width() - gap * (n as f32 - 1.0)) / n as f32;
        let pressed_at = ui.input(|i| {
            (i.pointer.primary_pressed() || i.pointer.secondary_pressed())
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        let mut focus_to: Option<usize> = None;
        let mut rects: Vec<egui::Rect> = Vec::with_capacity(n);
        for k in 0..n {
            let rect = egui::Rect::from_min_size(
                egui::pos2(whole.left() + k as f32 * (w + gap), whole.top()),
                egui::vec2(w, whole.height()),
            );
            rects.push(rect);
            if pressed_at.is_some_and(|p| rect.contains(p)) {
                focus_to = Some(k);
            }
            ui.scope_builder(
                egui::UiBuilder::new()
                    .id_salt(("filewin-side", k))
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::LEFT)),
                |ui| self.side_ui(ui, k),
            );
        }
        // 两栏之间一道淡边线。
        if n == 2 {
            let x = rects[0].right() + gap / 2.0;
            ui.painter().line_segment(
                [egui::pos2(x, whole.top()), egui::pos2(x, whole.bottom())],
                egui::Stroke::new(1.0, super::theme::palette(ui.ctx()).border_soft),
            );
        }
        ui.allocate_rect(whole, egui::Sense::hover());
        if let Some(k) = focus_to {
            if k != self.focus {
                self.focus_side(k);
            }
        }
        self.settle_drag(ui, &rects);
    }

    /// 〔「行拖到另一栏的手势」〕有一栏在拖：拖着时在指针旁说一句「复制 N 项到另一栏」
    /// （只在指针落在另一栏里时说）；**松手**那一帧落在另一栏 ⇒ 走「复制到另一栏」**那一个入口**（[`Self::copy_to_other`]，
    /// 不另起一条复制路）；落在本栏 / 窗外 ⇒ 什么都不做。只有一栏时没有「另一栏」，拖了也不做（与按钮同）。不往 OS 拖出。
    fn settle_drag(&mut self, ui: &mut egui::Ui, rects: &[egui::Rect]) {
        let Some(k) = (0..self.sides.len()).find(|&k| self.pane_on(k).dragging) else {
            return;
        };
        let (released, pos) = ui.input(|i| {
            (
                i.pointer.any_released(),
                i.pointer.interact_pos().or(i.pointer.latest_pos()),
            )
        });
        let over_other = self.sides.len() == 2
            && pos.is_some_and(|p| rects.get(1 - k).is_some_and(|r| r.contains(p)));
        if released {
            for s in 0..self.sides.len() {
                self.pane_on_mut(s).dragging = false;
            }
            if over_other {
                self.focus_side(k);
                let ctx = ui.ctx().clone();
                self.copy_to_other(Some(ctx));
            }
            return;
        }
        if let (true, Some(p)) = (over_other, pos) {
            let n = self.pane_on(k).selection().len();
            let hint = copy_text("rsFilewinWorkspace.drag.hint", &[("n", &n.to_string())]);
            ui.ctx()
                .layer_painter(egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::new("filewin-drag-hint"),
                ))
                .text(
                    p + egui::vec2(14.0, 14.0),
                    egui::Align2::LEFT_TOP,
                    hint,
                    egui::FontId::proportional(14.0),
                    ui.visuals().strong_text_color(),
                );
        }
    }

    /// 一栏：标签栏（规范 `C5` 下划线式；图标 ＋ 名字 ＋ ×（悬停与当前才显）＋ 忙点；末尾「＋」；放不下右端「▾」列出全部）
    /// ＋ 当前那个标签页的正文。
    fn side_ui(&mut self, ui: &mut egui::Ui, k: usize) {
        let mut pick: Option<usize> = None;
        let mut close: Option<usize> = None;
        let mut new_tab = false;
        let p = super::theme::palette(ui.ctx());
        let bar = ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), super::kit::TAB_H),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let s = &self.sides[k];
                let room = ui.available_width() - 64.0;
                let mut used = 0.0;
                let mut hidden: Vec<usize> = Vec::new();
                for (i, t) in s.tabs.iter().enumerate() {
                    let est = super::kit::TAB_MAX
                        .min(80.0 + Self::tab_title(&t.pane).chars().count() as f32 * 8.0);
                    if used + est > room && i != s.active {
                        hidden.push(i);
                        continue;
                    }
                    used += est;
                    // 忙点：改了没存 ⇒ `--text-2` 实心点；这一页开的那几趟还有在跑的 / 手上攥着一问 ⇒ `--success`。
                    let busy = if t.pane.editing().is_some_and(|e| e.dirty()) {
                        Some(p.text2)
                    } else if !t.pane.edit_tab
                        && (t.pane.has_running_jobs() || t.pane.busy_reason().is_some())
                    {
                        Some(p.success)
                    } else {
                        None
                    };
                    let icon = if t.pane.edit_tab {
                        egui_phosphor::regular::PENCIL_SIMPLE
                    } else if t.pane.listing.is_loading() {
                        egui_phosphor::regular::CIRCLE_NOTCH
                    } else {
                        egui_phosphor::regular::FOLDER_SIMPLE
                    };
                    let (r, x) =
                        super::kit::tab(ui, icon, &Self::tab_title(&t.pane), i == s.active, busy);
                    if x {
                        close = Some(i);
                    } else if r.on_hover_text(&t.pane.cwd).clicked() {
                        pick = Some(i);
                    }
                }
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(egui_phosphor::regular::PLUS).color(p.text2),
                        )
                        .frame_when_inactive(false),
                    )
                    .on_hover_text(copy_text("rsFilewinWorkspace.sideUi.newTabHint", &[]))
                    .clicked()
                {
                    new_tab = true;
                }
                if !hidden.is_empty() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        super::kit::menu(
                            ui,
                            egui::RichText::new(egui_phosphor::regular::CARET_DOWN).color(p.text2),
                            |ui| {
                                for (i, t) in s.tabs.iter().enumerate() {
                                    if ui
                                        .selectable_label(i == s.active, Self::tab_title(&t.pane))
                                        .clicked()
                                    {
                                        pick = Some(i);
                                        ui.close();
                                    }
                                }
                            },
                        );
                    });
                }
            },
        );
        ui.painter().line_segment(
            [
                bar.response.rect.left_bottom(),
                bar.response.rect.right_bottom(),
            ],
            egui::Stroke::new(1.0, p.border_soft),
        );
        if let Some(i) = pick {
            self.select_tab(k, i);
        }
        if let Some(i) = close {
            // 一栏只剩一个标签页：× 照样在，点了浮一句（`close_tab` 说）。
            self.close_tab(k, i);
        }
        if new_tab {
            self.open_tab(k);
        }
        let s = &mut self.sides[k];
        let t = &mut s.tabs[s.active];
        ui.push_id(("filewin-tab", t.id), |ui| t.pane.frame_body(ui));
    }
}

/// 两个目录的最长公共前缀。都只在根下相交 ⇒ `/`。
///
/// ⚠ **自己一刀都不切**：拿 [`super::source::breadcrumbs`]（远端路径那一族切法的唯一住址，只认 `/`）
/// 各切一摞前缀，取两摞共有的最长那一个 ⇒ 不多出第四份切法
/// （`source_tests::every_place_that_splits_a_remote_path_is_declared` 钉着那张表）。
pub fn common_dir(a: &str, b: &str) -> String {
    let theirs: Vec<String> = super::source::breadcrumbs(b)
        .into_iter()
        .map(|(_, full)| full)
        .collect();
    super::source::breadcrumbs(a)
        .into_iter()
        .map(|(_, full)| full)
        .take_while(|f| theirs.contains(f))
        .last()
        .unwrap_or_else(|| "/".to_string())
}

/// 跨目录那一形的 `files-copy` 参数：`root` ＝ 两个父目录的公共前缀，`from` / `to` ＝ 各自相对它的那一段。
///
/// ⚠ 同目录那一形（「复制为」那个框）走的是 `copy::copy_args`，它**刻意拒**跨目录（防「当前目录」与
///   「那一行的路径」写法不一致时拼到别处去）—— 这里是另一件事（用户明说「放到另一栏那个目录」），
///   所以另起一个，不去放宽那一道。后端的 `from` / `to` 本来就收多段相对路径（逐段过词法围栏）。
pub fn across_args(job: &CopyJob, overwrite: bool) -> Result<serde_json::Value, String> {
    // 〔有损名全寻址〕按**字节**切（合法 UTF-8 时与按串切逐字节同：`common_dir` 与它同一个按段比的口径）。
    let (from, to) = (job.from_path(), job.to_path());
    let root = super::source::common_dir_bytes(&from.parent().bytes(), &to.parent().bytes());
    let root_path = super::source::RemotePath::from_bytes(&root);
    let rel = |p: &super::source::RemotePath| -> Result<serde_json::Value, String> {
        let b = p.bytes();
        let cut = if root == b"/" { 1 } else { root.len() + 1 };
        (b.starts_with(&root)
            && b.len() > cut
            && (root == b"/" || b.get(root.len()) == Some(&b'/')))
        .then(|| super::source::wire_bytes(&b[cut..]))
        .ok_or_else(|| {
            copy_text(
                "rsFilewinWorkspace.acrossArgs.notUnder",
                &[("path", &p.shown), ("root", &root_path.shown)],
            )
        })
    };
    let mut v = serde_json::json!({
        "root": root_path.wire(),
        "from": rel(&from)?,
        "to": rel(&to)?,
        "overwrite": overwrite,
    });
    super::copy::mark_recursive(&mut v, job);
    Ok(v)
}

/// 跨目录复制那一趟（经通道问后端 `files-copy`）。回这一件复制了什么（目录那一件是整棵）。
pub async fn copy_across(
    line: &super::source::Line,
    origin: &super::source::Origin,
    job: &CopyJob,
    overwrite: bool,
) -> Result<super::copy::Copied, String> {
    let args = across_args(job, overwrite)?;
    let d = super::source::ask(
        line,
        origin,
        super::copy::CMD_COPY,
        &args,
        super::copy::COPY_BUDGET,
    )
    .await?;
    super::copy::copied_from_reply(job, &d)
}

impl eframe::App for Workspace {
    /// 关窗那一下在这里接（窗口最小化时 eframe 只跑这一段，不跑 `ui`）。
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.guard_close(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }

    /// 窗口底色就是主题的主底（不是 eframe 缺省那层半透明的灰黑）。
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/workspace_tests.rs"]
mod tests;
