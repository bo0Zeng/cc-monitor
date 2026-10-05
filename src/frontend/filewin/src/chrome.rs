//! 窗口的框：工具条（后退 · 前进 · 上一级 · 刷新 · 地址栏 · 搜索格）· 命令栏（不依赖选中就能做的那几件与开关）·
//! 左栏（书签 · 这台机器 · 其他机器）· 状态栏。都作用于焦点那一栏当前那个标签页；画法照资源管理器。
//!
//! 这里只画、只收手势；做事落回 `FileWindow` / `Workspace` 已有的那几个口（换目录 · 新建 · 上传 · 开终端 · 开另一台）。
//! 历史 · 列宽 · 左栏收起 · 隐藏文件开关都是窗口状态，关窗即没。

use copy_core::copy_text;
use egui_phosphor::regular as ph;
use std::sync::{Arc, Mutex};

use super::shell::FileWindow;
use super::source::breadcrumbs;
use super::theme::{bar_frame, metrics, palette};
use super::workspace::Workspace;

/// 地址栏手输那一格的 egui id（Ctrl+L 把焦点给它）。
pub const ADDR_ID: &str = "filewin-address";
/// 工具条上搜索那一格占多宽（搜索框 ＋ 重建 ＋ 在飞指示）。
const SEARCH_SLOT: f32 = 440.0;
/// 地址栏至少留多宽；窗口窄到留不出来 ⇒ 搜索那一格收窄，「只搜当前目录」与「重建索引」收进「⋯」。
const ADDR_MIN: f32 = 160.0;
/// 左栏多宽（可收起，不拖宽：书签那几格的字超出就截成「…」）。
const SIDEBAR_WIDTH: f32 = 220.0;
/// 面包屑最多摆几段（再深就把开头几段收成「…」）。
const CRUMBS_SHOWN: usize = 6;

/// 工具条 / 命令栏那种按钮：平时不画框，悬停一层淡底（资源管理器的手感）；开关 / 当前所在的那一格另配
/// `.selected(on).frame_when_inactive(on)`，开着时画成选中（强调色的淡底）。
fn flat<'a>(atoms: impl egui::IntoAtoms<'a>) -> egui::Button<'a> {
    egui::Button::new(atoms).frame_when_inactive(false)
}

/// 只有图标的那种（悬停说它是什么）。
fn icon_button(ui: &mut egui::Ui, icon: &str, tip: String, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        flat(egui::RichText::new(icon).size(18.0)).min_size(egui::vec2(32.0, 28.0)),
    )
    .on_hover_text(&tip)
    .on_disabled_hover_text(tip)
    .clicked()
}

/// 命令栏上的一颗是哪一件。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cmd {
    Mkdir,
    NewFile,
    Upload,
    Term,
    Grep,
    Hidden,
    Split,
    Preview,
    Across,
}

/// 命令栏上的一颗：图标 · 字 · 开关态（`None` ＝ 不是开关）· 悬停说明 · 第几组（组与组之间画一道竖线）。
pub(crate) struct CmdItem {
    pub cmd: Cmd,
    icon: &'static str,
    pub label: String,
    on: Option<bool>,
    hint: Option<String>,
    group: u8,
}

impl CmdItem {
    pub(crate) fn new(cmd: Cmd, icon: &'static str, label: &str, group: u8) -> Self {
        Self {
            cmd,
            icon,
            label: label.to_string(),
            on: None,
            hint: None,
            group,
        }
    }
    pub(crate) fn toggle(mut self, on: bool) -> Self {
        self.on = Some(on);
        self
    }
    fn hint(mut self, h: String) -> Self {
        self.hint = Some(h);
        self
    }
    fn button(&self) -> egui::Button<'_> {
        let on = self.on.unwrap_or(false);
        flat((self.icon, self.label.as_str()))
            .selected(on)
            .frame_when_inactive(on)
    }
}

/// 「更多」那一颗的悬停说明（命令栏放不下的那几颗收在它里面）。
pub static MORE_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinChrome.command.more", &[]));

/// 画命令栏上那一排：放得下的摆在栏上，放不下的从右往左收进「更多」（⋯）。回这一帧按了哪一件。
/// 每一颗的宽按上一帧画出来的量（头一帧没量过就按字估）。
pub(crate) fn command_row(ui: &mut egui::Ui, items: &[CmdItem]) -> Option<Cmd> {
    let id = egui::Id::new("filewin-cmd-width");
    let pad = ui.spacing().button_padding.x * 2.0 + ui.spacing().icon_spacing;
    let font = egui::TextStyle::Button.resolve(ui.style());
    let width = |ui: &egui::Ui, it: &CmdItem| -> f32 {
        ui.data(|d| d.get_temp::<f32>(id.with(it.cmd)))
            .unwrap_or_else(|| {
                let text = format!("{}{}", it.icon, it.label);
                pad + ui.fonts_mut(|f| {
                    f.layout_no_wrap(text, font.clone(), ui.visuals().text_color())
                        .size()
                        .x
                })
            })
    };
    let gap = ui.spacing().item_spacing.x;
    let sep = gap * 2.0 + 1.0;
    let more = ui.spacing().interact_size.y + pad + gap;
    let avail = ui.available_width();
    let mut need = Vec::with_capacity(items.len());
    let mut x = 0.0;
    for (k, it) in items.iter().enumerate() {
        if k > 0 && items[k - 1].group != it.group {
            x += sep;
        }
        x += width(ui, it) + gap;
        need.push(x);
    }
    let shown = if need.last().is_none_or(|&w| w <= avail) {
        items.len()
    } else {
        need.iter().take_while(|&&w| w + more <= avail).count()
    };
    let mut hit = None;
    for (k, it) in items.iter().take(shown).enumerate() {
        if k > 0 && items[k - 1].group != it.group {
            ui.separator();
        }
        let mut r = ui.add(it.button());
        ui.data_mut(|d| d.insert_temp(id.with(it.cmd), r.rect.width()));
        if let Some(h) = &it.hint {
            r = r.on_hover_text(h);
        }
        if r.clicked() {
            hit = Some(it.cmd);
        }
    }
    if shown < items.len() {
        ui.menu_button(egui::RichText::new(ph::DOTS_THREE).size(18.0), |ui| {
            for it in &items[shown..] {
                let mut r = ui.add(it.button());
                if let Some(h) = &it.hint {
                    r = r.on_hover_text(h);
                }
                if r.clicked() {
                    hit = Some(it.cmd);
                }
            }
        })
        .response
        .on_hover_text(MORE_LABEL.as_str());
    }
    hit
}

/// 地址栏里开头收起几段：从当前那一级往前数，放得下几段摆几段（当前那一级总摆着，太长自己截成「…」）。
/// 回 `(收起几段, 摆不摆开头那个「…」)`：连「…」都摆不下时只摆当前那一级。
fn crumbs_skip(ui: &egui::Ui, crumbs: &[(String, String)]) -> (usize, bool) {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let gap = ui.spacing().item_spacing.x;
    let w = |t: &str, pad: f32| {
        pad + gap
            + ui.fonts_mut(|f| {
                f.layout_no_wrap(t.to_string(), font.clone(), ui.visuals().text_color())
                    .size()
                    .x
            })
    };
    let caret = w(ph::CARET_RIGHT, 0.0);
    let dots = w(ph::DOTS_THREE, 0.0);
    let pad = 2.0 * ui.spacing().button_padding.x;
    let room = ui.available_width();
    let mut used = 0.0;
    let mut start = crumbs.len();
    for k in (0..crumbs.len()).rev() {
        let one = caret + w(&crumbs[k].0, pad);
        let head = if k > 0 { caret + dots } else { 0.0 };
        if k + 1 < crumbs.len() && used + one + head > room {
            break;
        }
        used += one;
        start = k;
    }
    (start, start > 0 && used + caret + dots <= room)
}

/// 工具条想干什么（闭包里只收，画完再做：闭包借着 `self`）。
enum Nav {
    Back,
    Forward,
    Up,
    Refresh,
    Go(String),
    Edit,
    Commit,
    Cancel,
    Star,
}

impl FileWindow {
    /// 🔴 **工具条**：后退 · 前进 · 上一级 · 刷新 · 地址栏（面包屑；点空白处或 Ctrl+L 变成可输入的路径，回车去、Esc 退）· 搜索格。
    pub fn toolbar_ui(&mut self, ui: &mut egui::Ui) {
        let mut nav: Option<Nav> = None;
        let lossy = self.cwd_raw.is_some();
        ui.horizontal_centered(|ui| {
            if icon_button(
                ui,
                ph::ARROW_LEFT,
                copy_text("rsFilewinChrome.nav.back", &[]),
                self.can_go_back(),
            ) {
                nav = Some(Nav::Back);
            }
            if icon_button(
                ui,
                ph::ARROW_RIGHT,
                copy_text("rsFilewinChrome.nav.forward", &[]),
                self.can_go_forward(),
            ) {
                nav = Some(Nav::Forward);
            }
            let at_top = self.cwd_path().parent() == self.cwd_path();
            if icon_button(
                ui,
                ph::ARROW_UP,
                copy_text("rsFilewinChrome.nav.up", &[]),
                !at_top,
            ) {
                nav = Some(Nav::Up);
            }
            if icon_button(
                ui,
                ph::ARROW_CLOCKWISE,
                copy_text("rsFilewinChrome.nav.refresh", &[]),
                true,
            ) {
                nav = Some(Nav::Refresh);
            }
            let avail = ui.available_width();
            let compact = avail - SEARCH_SLOT < ADDR_MIN;
            let search_w = if compact {
                (avail * 0.3).clamp(60.0, super::shell::SEARCH_BOX_WIDTH)
            } else {
                super::shell::SEARCH_BOX_WIDTH
            };
            let tail = if compact {
                search_w + 2.0 * ui.spacing().interact_size.y + 3.0 * ui.spacing().item_spacing.x
            } else {
                SEARCH_SLOT
            };
            let w = (avail - tail).max(0.0);
            let h = 30.0;
            if let Some(buf) = self.addr_edit.as_mut() {
                let r = ui.add_sized(
                    [w, h],
                    egui::TextEdit::singleline(buf)
                        .id(egui::Id::new(ADDR_ID))
                        .vertical_align(egui::Align::Center),
                );
                if !r.has_focus() && !r.lost_focus() {
                    r.request_focus();
                }
                if r.lost_focus() {
                    nav = Some(if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        Nav::Commit
                    } else {
                        Nav::Cancel
                    });
                }
            } else {
                let p = palette(ui.ctx());
                let (rect, bg) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());
                ui.painter().rect(
                    rect,
                    6.0,
                    ui.visuals().extreme_bg_color,
                    egui::Stroke::new(
                        1.0,
                        if bg.hovered() {
                            p.border
                        } else {
                            p.border_soft
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
                // 右端那颗星：当前目录在不在书签里（☆ 加 · ★ 去，同浏览器）；有损目录 / 没接书签不画。
                let star = match (&self.shelf, lossy) {
                    (Some(shelf), false) => Some(if shelf.contains(&self.cwd) {
                        (
                            super::bookmarks::STAR_ON.as_str(),
                            super::bookmarks::DROP_LABEL.as_str(),
                            p.accent,
                        )
                    } else {
                        (
                            super::bookmarks::STAR_OFF.as_str(),
                            super::bookmarks::ADD_LABEL.as_str(),
                            p.text2,
                        )
                    }),
                    _ => None,
                };
                // 画在已经占好的那一块里，不再向外占地（路径再长也不把搜索格挤出窗口）。
                ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect.shrink2(egui::vec2(8.0, 0.0)))
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                )
                .scope(|ui| {
                    if let Some((glyph, tip, color)) = star {
                        if ui
                            .add(flat(egui::RichText::new(glyph).size(17.0).color(color)))
                            .on_hover_text(tip)
                            .clicked()
                        {
                            nav = Some(Nav::Star);
                        }
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        // 路径再长也只占星左边那一块：画和点都裁在这里，长名字盖不到星、抢不走它的点击。
                        ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                        ui.spacing_mut().item_spacing.x = 2.0;
                        ui.label(egui::RichText::new(ph::HARD_DRIVES).color(p.text2))
                            .on_hover_text(self.source.label());
                        let crumbs = breadcrumbs(&self.cwd);
                        // 机器名与当前那一级放不下同时摆 ⇒ 先让机器名（悬停盘符看它，窗口标题上也有）。
                        let font = egui::TextStyle::Body.resolve(ui.style());
                        let width = |ui: &egui::Ui, t: &str| {
                            ui.fonts_mut(|f| {
                                f.layout_no_wrap(t.to_string(), font.clone(), p.text)
                                    .size()
                                    .x
                            })
                        };
                        let last = crumbs.last().map_or(0.0, |(seg, _)| width(ui, seg));
                        let host = self.source.label();
                        if width(ui, &host) + last + 40.0 <= ui.available_width() {
                            ui.label(egui::RichText::new(host).color(p.text2));
                        }
                        let (fit, dots) = crumbs_skip(ui, &crumbs);
                        let skip = fit.max(crumbs.len().saturating_sub(CRUMBS_SHOWN));
                        if skip > 0 && (dots || fit < skip) {
                            ui.label(egui::RichText::new(ph::CARET_RIGHT).color(p.faint));
                            ui.label(egui::RichText::new(ph::DOTS_THREE).color(p.faint));
                        }
                        for (seg, full) in crumbs.into_iter().skip(skip) {
                            ui.label(egui::RichText::new(ph::CARET_RIGHT).color(p.faint));
                            // 当前这一级不画成按钮（点了什么都不会发生）；有损目录里只画不点（那一摞前缀寻址不到）。
                            if full == self.cwd || lossy {
                                ui.add(
                                    egui::Label::new(egui::RichText::new(seg).color(p.text))
                                        .selectable(false)
                                        .truncate(),
                                );
                            } else if ui.add(flat(seg)).clicked() {
                                nav = Some(Nav::Go(full));
                            }
                        }
                    });
                });
                if bg.clicked() && !lossy {
                    nav = Some(Nav::Edit);
                }
            }
            self.search_box(ui, compact.then_some(search_w));
        });
        match nav {
            Some(Nav::Back) => {
                self.go_back();
            }
            Some(Nav::Forward) => {
                self.go_forward();
            }
            Some(Nav::Up) => self.navigate_up(),
            Some(Nav::Refresh) => self.reload(),
            Some(Nav::Go(p)) => self.navigate_to(p),
            Some(Nav::Edit) => self.begin_address_edit(),
            Some(Nav::Commit) => {
                self.commit_address();
            }
            Some(Nav::Cancel) => self.addr_edit = None,
            Some(Nav::Star) => {
                if let Some(shelf) = &self.shelf {
                    shelf.toggle(&self.cwd);
                }
            }
            None => {}
        }
    }

    /// 地址栏变成可输入（Ctrl+L / 点空白处）：输入框里先摆着当前目录。有损目录不给输（那一串寻址不到）。
    pub fn begin_address_edit(&mut self) {
        if self.cwd_raw.is_none() {
            self.addr_edit = Some(self.cwd.clone());
        }
    }

    /// 地址栏里手输的那一串回车了：去那儿（要从 `/` 起头）。回值 ＝ 真的换了目录。
    pub fn commit_address(&mut self) -> bool {
        let Some(buf) = self.addr_edit.take() else {
            return false;
        };
        let want = super::bookmarks::normalize_dir(buf.trim());
        if !want.starts_with('/') {
            self.set_key_notice(copy_text("rsFilewinChrome.address.notAbsolute", &[]));
            return false;
        }
        let before = self.cwd.clone();
        self.navigate_to(want);
        self.cwd != before
    }

    /// 地址栏现在是不是在手输（判据用）。
    pub fn address_editing(&self) -> Option<&str> {
        self.addr_edit.as_deref()
    }

    /// 🔴 **命令栏左半**（不依赖选中就能做的那几件）：新建目录 · 新建文件 · 上传 · 在此打开终端 · 按内容搜 · 隐藏文件开关。
    pub(crate) fn command_items(&self) -> Vec<CmdItem> {
        let hidden = self.shows_hidden();
        vec![
            CmdItem::new(
                Cmd::Mkdir,
                ph::FOLDER_PLUS,
                &super::writeops::MKDIR_LABEL,
                0,
            ),
            CmdItem::new(
                Cmd::NewFile,
                ph::FILE_PLUS,
                &super::create::NEW_FILE_LABEL,
                0,
            ),
            CmdItem::new(
                Cmd::Upload,
                ph::UPLOAD_SIMPLE,
                &super::upload::UPLOAD_LABEL,
                0,
            ),
            CmdItem::new(Cmd::Term, ph::TERMINAL_WINDOW, &TERMINAL_LABEL, 0),
            CmdItem::new(Cmd::Grep, ph::FILE_MAGNIFYING_GLASS, &GREP_LABEL, 0)
                .toggle(self.grep_open),
            CmdItem::new(
                Cmd::Hidden,
                if hidden { ph::EYE } else { ph::EYE_SLASH },
                &HIDDEN_LABEL,
                1,
            )
            .toggle(hidden)
            .hint(copy_text("rsFilewinChrome.hidden.hint", &[])),
        ]
    }

    /// 命令栏上那几颗里归目录视图的那几件。
    pub(crate) fn run_command(&mut self, c: Cmd, ctx: Option<egui::Context>) {
        match c {
            Cmd::Mkdir => {
                self.begin_mkdir();
            }
            Cmd::NewFile => {
                self.begin_new_file();
            }
            Cmd::Upload => match self.one_at_a_time(super::shell::Trip::Upload) {
                Some(why) => self.set_key_notice(why),
                None => self.upload.open(),
            },
            Cmd::Term => {
                self.open_terminal_here(ctx);
            }
            Cmd::Grep => self.grep_open = !self.grep_open,
            Cmd::Hidden => {
                let on = self.shows_hidden();
                self.set_show_hidden(!on);
            }
            Cmd::Split | Cmd::Preview | Cmd::Across => {}
        }
    }

    /// 在跑的传输有几个（上传 · 下载 · 跨机复制）。
    pub fn transfers_running(&self) -> usize {
        self.board.cancels().in_flight_names().len()
            + usize::from(self.pull.in_flight().is_some())
            + usize::from(self.cross_board.running().is_some())
    }

    /// 状态栏那一行写什么（左半）：项数（另有几项隐藏）· 选中几项（合计大小）。判据与界面看同一个值。
    pub fn status_line(&self) -> String {
        let n = self.listing.rows.lock().unwrap().len();
        let hidden = self.listing.hidden.lock().unwrap().len();
        let mut s = if hidden > 0 {
            copy_text(
                "rsFilewinChrome.status.countHidden",
                &[("n", &n.to_string()), ("hidden", &hidden.to_string())],
            )
        } else {
            copy_text("rsFilewinChrome.status.count", &[("n", &n.to_string())])
        };
        let picked = self.picked_rows();
        if !picked.is_empty() {
            let bytes: u64 = picked.iter().filter(|r| !r.is_dir).map(|r| r.size).sum();
            s.push_str(&copy_text(
                "rsFilewinChrome.status.picked",
                &[
                    ("n", &picked.len().to_string()),
                    ("size", &super::rows::human_size(bytes)),
                ],
            ));
        }
        s
    }

    /// 🔴 **状态栏**：左边项数 · 选中；在列目录时一个转圈；右边「传输：N 个在跑」（点它收起 / 摊开那一摞进度）。
    pub fn status_ui(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.ctx());
        let mut toggle = false;
        ui.horizontal_centered(|ui| {
            let line = self.status_line();
            ui.add(egui::Label::new(egui::RichText::new(&line).color(p.text2)).truncate())
                .on_hover_text(line);
            if self.listing.is_loading() {
                ui.spinner();
                ui.label(
                    egui::RichText::new(copy_text("rsFilewinShell.frame.listing", &[]))
                        .color(p.text2),
                );
            }
            let n = self.transfers_running();
            if n > 0 || self.transfers_unseen() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let caret = if self.transfers_open {
                        ph::CARET_DOWN
                    } else {
                        ph::CARET_RIGHT
                    };
                    let text =
                        copy_text("rsFilewinChrome.status.transfers", &[("n", &n.to_string())]);
                    if ui.add(flat((text, caret))).clicked() {
                        toggle = true;
                    }
                });
            }
        });
        if toggle {
            self.transfers_open = !self.transfers_open;
        }
    }
}

/// 命令栏上那几颗的字 —— **唯一住址**（判据按同一个常量去找它画出来的字）。
pub static TERMINAL_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinShell.frame.terminal", &[]));
pub static GREP_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinChrome.command.grep", &[]));
pub static HIDDEN_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinChrome.command.hidden", &[]));
pub static SIDEBAR_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinChrome.command.sidebar", &[]));

/// 工具条 / 导航那几个键：Alt+← / Alt+→（与鼠标侧键）后退前进 · F5 刷新 · Ctrl+L 地址栏 · Ctrl+F 搜索框。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavKey {
    Back,
    Forward,
    Refresh,
    Address,
    Search,
}

/// 这一帧的事件 → 导航键（按到达顺序，只认按下）。与列表那张键位表（`select::intents`）不相交：
/// 那张里 ← / → 刻意不接、带 Alt 的只有 Alt+↑。
pub fn nav_keys(events: &[egui::Event]) -> Vec<NavKey> {
    events
        .iter()
        .filter_map(|ev| match ev {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers: m,
                ..
            } => match key {
                egui::Key::ArrowLeft if m.alt && !m.command => Some(NavKey::Back),
                egui::Key::ArrowRight if m.alt && !m.command => Some(NavKey::Forward),
                egui::Key::F5 => Some(NavKey::Refresh),
                egui::Key::L if m.command && !m.alt => Some(NavKey::Address),
                egui::Key::F if m.command && !m.alt => Some(NavKey::Search),
                _ => None,
            },
            egui::Event::PointerButton {
                button: egui::PointerButton::Extra1,
                pressed: true,
                ..
            } => Some(NavKey::Back),
            egui::Event::PointerButton {
                button: egui::PointerButton::Extra2,
                pressed: true,
                ..
            } => Some(NavKey::Forward),
            _ => None,
        })
        .collect()
}

/// 左栏：这台机器的家目录问到了没有（问一次，整扇窗共用）。
#[derive(Clone, Default)]
pub struct Home(Arc<Mutex<HomeState>>);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum HomeState {
    #[default]
    NotAsked,
    Asking,
    Known(String),
    Failed(String),
}

/// 左栏「其他机器」点了一台之后那一问的结局（`None` ＝ 还在问 / 没问过）。
pub type OtherSlot = Arc<Mutex<Option<Result<(), String>>>>;

impl Workspace {
    /// 这一帧的导航键（只对焦点那一栏；模态框 · 菜单摆着 · 文字框拿着键盘时不接）。回值 ＝ 认出了几件。
    pub fn apply_nav_keys(&mut self, ctx: &egui::Context) -> usize {
        let f = self.focus();
        if self.pane_on(f).nav_keys_blocked(ctx) {
            return 0;
        }
        let keys = ctx.input(|i| nav_keys(&i.events));
        for k in &keys {
            let pane = self.pane_on_mut(f);
            match k {
                NavKey::Back => {
                    pane.go_back();
                }
                NavKey::Forward => {
                    pane.go_forward();
                }
                NavKey::Refresh => pane.reload(),
                NavKey::Address => pane.begin_address_edit(),
                NavKey::Search => ctx.memory_mut(|m| {
                    m.request_focus(egui::Id::new(super::shell::SEARCH_BOX_ID));
                }),
            }
        }
        keys.len()
    }

    /// 🔴 **窗口的框**：工具条 · 命令栏 · 状态栏 · 左栏，都画在焦点那一栏当前的标签页上。
    pub fn chrome_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let f = self.focus();
        let bar_h = metrics(&ctx).bar_h;
        egui::Panel::top("filewin-toolbar")
            .frame(bar_frame(&ctx))
            .exact_size(bar_h)
            .show_separator_line(false)
            .show(ui, |ui| self.pane_on_mut(f).toolbar_ui(ui));
        let (mut cmd, mut sidebar) = (None, false);
        egui::Panel::top("filewin-commands")
            .frame(bar_frame(&ctx))
            .exact_size(bar_h - 4.0)
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    if ui
                        .add(
                            flat(egui::RichText::new(ph::SIDEBAR_SIMPLE).size(18.0))
                                .selected(self.sidebar_open)
                                .frame_when_inactive(self.sidebar_open),
                        )
                        .on_hover_text(SIDEBAR_LABEL.as_str())
                        .clicked()
                    {
                        sidebar = true;
                    }
                    ui.separator();
                    let two = self.sides() == 2;
                    let mut items = self.pane_on(f).command_items();
                    items.push(
                        CmdItem::new(Cmd::Split, ph::COLUMNS, &super::workspace::SPLIT_LABEL, 2)
                            .toggle(two),
                    );
                    items.push(
                        CmdItem::new(
                            Cmd::Preview,
                            ph::SIDEBAR,
                            &super::workspace::PREVIEW_LABEL,
                            2,
                        )
                        .toggle(self.preview.is_some()),
                    );
                    if two {
                        items.push(CmdItem::new(
                            Cmd::Across,
                            ph::COPY,
                            &super::workspace::COPY_ACROSS_LABEL,
                            2,
                        ));
                    }
                    cmd = command_row(ui, &items);
                    if let Some(n) = self.notice() {
                        // 放不下就截成「…」，悬停看全句。
                        let t = egui::RichText::new(n).color(ui.visuals().warn_fg_color);
                        ui.add(egui::Label::new(t).truncate()).on_hover_text(n);
                    }
                });
            });
        if sidebar {
            self.sidebar_open = !self.sidebar_open;
        }
        match cmd {
            Some(Cmd::Split) => {
                let two = self.sides() == 2;
                self.set_split(!two);
            }
            Some(Cmd::Preview) => {
                let on = self.preview.is_some();
                self.set_preview(!on);
            }
            Some(Cmd::Across) => {
                self.copy_to_other(Some(ctx.clone()));
            }
            Some(c) => self.pane_on_mut(f).run_command(c, Some(ctx.clone())),
            None => {}
        }
        egui::Panel::bottom("filewin-status")
            .frame(bar_frame(&ctx))
            .exact_size(28.0)
            .show_separator_line(false)
            .show(ui, |ui| self.pane_on_mut(f).status_ui(ui));
        if self.sidebar_open {
            // 窗口窄了左栏跟着让（至多占三成），里面的字截成「…」。
            let w = SIDEBAR_WIDTH.min(ctx.content_rect().width() * 0.3);
            egui::Panel::left("filewin-sidebar")
                .frame(bar_frame(&ctx).inner_margin(egui::Margin::same(10)))
                .exact_size(w)
                .show(ui, |ui| self.sidebar_ui(ui));
        }
    }

    /// 🔴 **左栏**：书签（加 / 去当前目录 · 点了跳过去 · × 删掉）· 这台机器（家目录 · 根目录）· 其他机器（点了开那台的窗口）。
    fn sidebar_ui(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.ctx());
        let f = self.focus();
        let mut go: Option<String> = None;
        let mut open_other: Option<String> = None;
        let mut want_home = false;
        let section = |ui: &mut egui::Ui, title: String| {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(title).color(p.text2).small());
        };
        // 就是当前所在的那一格：画成选中（强调色的淡底，与列表选中同一种）。
        let here_dir = super::bookmarks::normalize_dir(&self.pane_on(f).cwd);
        let item = |ui: &mut egui::Ui,
                    icon: &str,
                    text: &str,
                    tip: &str,
                    enabled: bool,
                    here: bool|
         -> bool {
            ui.add_enabled(
                enabled,
                flat((icon, text, egui::Atom::grow()))
                    .selected(here)
                    .frame_when_inactive(here)
                    .min_size(egui::vec2(ui.available_width(), 26.0))
                    .wrap_mode(egui::TextWrapMode::Truncate),
            )
            .on_hover_text(tip)
            .on_disabled_hover_text(tip)
            .clicked()
        };
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                // ── 书签 ──
                ui.horizontal(|ui| {
                    section(ui, copy_text("rsFilewinChrome.side.bookmarks", &[]));
                });
                let pane = self.pane_on(f);
                if let Some(shelf) = pane.shelf.clone() {
                    let cwd = super::bookmarks::normalize_dir(&pane.cwd);
                    let list = shelf.list();
                    if list.is_empty() {
                        ui.label(
                            egui::RichText::new(copy_text("rsFilewinChrome.side.noBookmarks", &[]))
                                .color(p.faint)
                                .small(),
                        );
                    }
                    let mut drop: Option<String> = None;
                    for d in list {
                        let tail = super::source::remote_basename(&d);
                        let tail = if tail.is_empty() { "/" } else { tail };
                        let here = super::bookmarks::normalize_dir(&d) == cwd;
                        // × 先贴右边，名字占剩下的宽度、太长就截断（缺省会截第一段字 —— 那是星的图标，名字就整串溢出去、点不到）。
                        let row = egui::vec2(ui.available_width(), 26.0);
                        ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(flat(super::bookmarks::REMOVE_LABEL.as_str()).small())
                                .on_hover_text(copy_text("rsFilewinBookmarks.bar.removeHint", &[]))
                                .clicked()
                            {
                                drop = Some(d.clone());
                            }
                            let w = ui.available_width();
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                if ui
                                    .add(
                                        flat((ph::STAR, egui::AtomExt::atom_shrink(tail, true), egui::Atom::grow()))
                                            .selected(here)
                                            .frame_when_inactive(here)
                                            .min_size(egui::vec2(w, 26.0))
                                            .wrap_mode(egui::TextWrapMode::Truncate),
                                    )
                                    .on_hover_text(&d)
                                    .clicked()
                                {
                                    go = Some(d.clone());
                                }
                            });
                        });
                    }
                    if let Some(n) = shelf.notice() {
                        ui.colored_label(ui.visuals().warn_fg_color, n);
                    }
                    if let Some(d) = drop {
                        shelf.remove(&d);
                    }
                }
                // ── 这台机器 ──
                ui.add_space(8.0);
                section(ui, copy_text("rsFilewinChrome.side.thisMachine", &[]));
                let home = self.home.0.lock().unwrap().clone();
                // 问不到：悬停看得见原因，点一下再问；还在问：点了就记着，问到了就去。
                let home_tip = match &home {
                    HomeState::Known(h) => h.clone(),
                    HomeState::Failed(why) => why.clone(),
                    _ => copy_text("rsFilewinChrome.side.homeAsking", &[]),
                };
                if item(
                    ui,
                    ph::HOUSE,
                    &copy_text("rsFilewinChrome.side.home", &[]),
                    &home_tip,
                    true,
                    matches!(&home, HomeState::Known(h) if super::bookmarks::normalize_dir(h) == here_dir),
                ) {
                    match &home {
                        HomeState::Known(h) => go = Some(h.clone()),
                        HomeState::Failed(_) => {
                            want_home = true;
                            self.home_go = true;
                        }
                        _ => self.home_go = true,
                    }
                }
                if matches!(home, HomeState::NotAsked) {
                    want_home = true;
                }
                if let (true, HomeState::Known(h)) = (self.home_go, &home) {
                    self.home_go = false;
                    go = Some(h.clone());
                }
                if item(
                    ui,
                    ph::HARD_DRIVE,
                    &copy_text("rsFilewinChrome.side.root", &[]),
                    "/",
                    true,
                    here_dir == "/",
                ) {
                    go = Some("/".to_string());
                }
                // ── 其他机器 ──
                let others = self.other_machines();
                if !others.is_empty() {
                    ui.add_space(8.0);
                    section(ui, copy_text("rsFilewinChrome.side.otherMachines", &[]));
                    for m in others {
                        let tip = copy_text("rsFilewinChrome.side.openOther", &[("machine", &m)]);
                        if item(ui, ph::DESKTOP_TOWER, &m, &tip, true, false) {
                            open_other = Some(m);
                        }
                    }
                }
            });
        if want_home {
            self.ask_home(Some(ui.ctx().clone()));
        }
        if let Some(d) = go {
            self.pane_on_mut(f).navigate_to(d);
        }
        if let Some(m) = open_other {
            self.open_other(&m, Some(ui.ctx().clone()));
        }
    }

    /// 「其他机器」那一摞：开窗种子带来的机器名单，去掉这一台与本机（本机没有文件窗口）。
    pub fn other_machines(&self) -> Vec<String> {
        let pane = self.pane_on(0);
        let me = pane.source.label();
        pane.machines
            .iter()
            .filter(|m| **m != me && m.as_str() != super::cross_copy::LOCAL_ORIGIN)
            .cloned()
            .collect()
    }

    /// 问一次这台机器的家目录（左栏「家目录」那一格用）。
    pub fn ask_home(&mut self, ctx: Option<egui::Context>) {
        let pane = self.pane_on(0);
        let slot = self.home.0.clone();
        let (Some(h), Some(line)) = (pane.rt.clone(), pane.line.clone()) else {
            *slot.lock().unwrap() = HomeState::Failed(super::shell::NO_LINE.to_string());
            return;
        };
        *slot.lock().unwrap() = HomeState::Asking;
        let origin = pane.source.origin();
        h.spawn(async move {
            let got = super::source::ask(
                &line,
                &origin,
                super::source::CMD_HOME,
                &serde_json::json!({}),
                super::proc::FIRST_SCREEN_BUDGET,
            )
            .await
            .and_then(|d| super::source::home_from_reply(&d));
            *slot.lock().unwrap() = match got {
                Ok(home) => HomeState::Known(home),
                Err(e) => HomeState::Failed(e),
            };
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
    }

    /// 家目录问到了吗（判据用：`Some(Ok(路径))` · `Some(Err(原话))` · `None` ＝ 还没问到）。
    pub fn home_known(&self) -> Option<Result<String, String>> {
        match self.home.0.lock().unwrap().clone() {
            HomeState::Known(h) => Some(Ok(h)),
            HomeState::Failed(e) => Some(Err(e)),
            _ => None,
        }
    }

    /// 「其他机器」点了 `machine`：请 monitor 照开窗入口另起那台的窗口（一窗一机，这扇窗不换机器）。
    /// 样子沿用这扇窗的那一套。没通道 / 没样子 ⇒ 出声，不发。
    pub fn open_other(&mut self, machine: &str, ctx: Option<egui::Context>) -> bool {
        let pane = self.pane_on(0);
        let (Some(h), Some(line)) = (pane.rt.clone(), pane.line.clone()) else {
            self.set_notice(super::shell::NO_LINE.to_string());
            return false;
        };
        let Some(theme) = self.theme.clone() else {
            self.set_notice(copy_text("rsFilewinChrome.other.noTheme", &[]));
            return false;
        };
        let slot = self.other.clone();
        *slot.lock().unwrap() = None;
        let origin = super::source::Origin(machine.to_string());
        let what = machine.to_string();
        self.set_notice(copy_text(
            "rsFilewinChrome.other.opening",
            &[("machine", &what)],
        ));
        h.spawn(async move {
            let got = super::source::ask(
                &line,
                &origin,
                filewin_contract::FILEWIN_OPEN_OP,
                &filewin_contract::filewin_open_args(&theme),
                OTHER_BUDGET,
            )
            .await
            .map(|_| ());
            *slot.lock().unwrap() = Some(got);
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
        true
    }

    /// 「其他机器」那一问落地了 ⇒ 成了不出声（新窗口自己出现就是回应），没成把原话摆在命令栏上。
    pub fn settle_other(&mut self) {
        let got = self.other.lock().unwrap().take();
        match got {
            Some(Ok(())) => self.set_notice_none(),
            Some(Err(e)) => self.set_notice(e),
            None => {}
        }
    }
}

/// 开另一台的窗口那一问的期限：monitor 起进程 ＋ 那扇窗列完第一屏才回（同开窗入口）。
pub const OTHER_BUDGET: std::time::Duration = std::time::Duration::from_secs(40);

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/chrome_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/chrome_testing.rs"]
pub(crate) mod testing;
