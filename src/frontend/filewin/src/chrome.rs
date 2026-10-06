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
    Sidebar,
    Mkdir,
    NewFile,
    Upload,
    Term,
    /// 「选中的」那一组：照右键菜单那一张表（[`super::select::actions_for`]）做。
    Act(super::select::Action),
    Across,
    Hidden,
    Split,
    Preview,
}

/// 「更多」那一颗的悬停说明。
pub static MORE_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinChrome.command.more", &[]));

/// 窗口宽度档（规范 `I8` 与稿 ⑫；按缩放之后的逻辑宽度算）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Wide,
    Mid,
    Narrow,
}

impl Tier {
    pub fn of(width: f32) -> Self {
        if width >= 1200.0 {
            Self::Wide
        } else if width >= 800.0 {
            Self::Mid
        } else {
            Self::Narrow
        }
    }
}

/// 命令栏「选中的」那一组常驻的几件，其余收在它后面的「⋯」里。
const PICK_SHOWN: [super::select::Action; 4] = [
    super::select::Action::Download,
    super::select::Action::CrossCopy,
    super::select::Action::Rename,
    super::select::Action::Delete,
];
const PICK_MORE: [super::select::Action; 6] = [
    super::select::Action::Edit,
    super::select::Action::Copy,
    super::select::Action::Chmod,
    super::select::Action::Properties,
    super::select::Action::Size,
    super::select::Action::Extract,
];

fn pick_icon(a: super::select::Action) -> &'static str {
    use super::select::Action as A;
    match a {
        A::Download => ph::DOWNLOAD_SIMPLE,
        A::CrossCopy => ph::SWAP,
        A::Rename => ph::PENCIL_SIMPLE,
        A::Delete => ph::TRASH,
        A::Edit => ph::NOTE_PENCIL,
        A::Copy => ph::COPY_SIMPLE,
        A::Chmod => ph::LOCK_KEY,
        A::Properties => ph::INFO,
        A::Size => ph::RULER,
        A::Extract => ph::FILE_ZIP,
        A::Open => ph::FOLDER_OPEN,
    }
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
                    if ui
                        .add(flat(
                            egui::RichText::new(ph::ARROW_CLOCKWISE)
                                .size(16.0)
                                .color(p.text2),
                        ))
                        .on_hover_text(copy_text("rsFilewinChrome.nav.refresh", &[]))
                        .clicked()
                    {
                        nav = Some(Nav::Refresh);
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        // 路径再长也只占星左边那一块：画和点都裁在这里，长名字盖不到星、抢不走它的点击。
                        ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                        ui.spacing_mut().item_spacing.x = 2.0;
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
                        // 机器标签：一颗带框的小牌（与主窗口 tab 上的同一种）；放不下当前那一级时先让它。
                        if width(ui, &host) + last + 60.0 <= ui.available_width() {
                            egui::Frame::new()
                                .stroke(egui::Stroke::new(1.0, p.border))
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::symmetric(6, 1))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{} {host}", ph::MONITOR))
                                            .color(p.text2)
                                            .small(),
                                    );
                                });
                            ui.add_space(4.0);
                        }
                        let (fit, dots) = crumbs_skip(ui, &crumbs);
                        let skip = fit.max(crumbs.len().saturating_sub(CRUMBS_SHOWN));
                        if skip > 0 && (dots || fit < skip) {
                            ui.label(egui::RichText::new(ph::CARET_RIGHT).color(p.faint));
                            // 收起的那几级：「…」是一个下拉，点开看得到、点得进（有损目录里只看不点）。
                            let hidden: Vec<(String, String)> = crumbs[..skip].to_vec();
                            super::kit::menu(
                                ui,
                                egui::RichText::new(ph::DOTS_THREE).color(p.text2),
                                |ui| {
                                    for (seg, full) in hidden {
                                        if ui.add_enabled(!lossy, egui::Button::new(seg)).clicked()
                                        {
                                            nav = Some(Nav::Go(full));
                                            ui.close();
                                        }
                                    }
                                },
                            );
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

    /// 「选中的」那一件此刻能不能做：能 ⇒ `None`；不能 ⇒ 悬停说的那一句（没选中 · 这几项做不了 · 这台做不到）。
    pub fn command_blocked(&self, a: super::select::Action) -> Option<String> {
        let rows = self.listing.rows.lock().unwrap();
        let idx = self.selection().picked_indices(&rows);
        if idx.is_empty() {
            return Some(copy_text("rsFilewinChrome.command.pickFirst", &[]));
        }
        let picked: Vec<&super::source::Listed> = idx.iter().map(|&i| &rows[i]).collect();
        if !super::select::actions_for(&picked).contains(&a) {
            return Some(super::select::refusal(a, idx.len()));
        }
        drop(rows);
        self.unavailable_here(a)
    }

    /// 🔴 **命令栏**：左 侧栏开关 ｜ 这个目录（新建 ▾ · 上传 · 终端）｜ 选中的（下载 · 复制到另一台 · 改名 · 删除 · ⋯）｜ 右 看法（显示隐藏文件 · 双栏 · 预览）。
    /// 没选中时「选中的」那几颗灰着、悬停说原因，位置不跳。宽度档：中档「选中的」与「看法」只剩图标；窄档只留「新建 ▾」「上传」＋「⋯」。
    pub fn command_bar_ui(&mut self, ui: &mut egui::Ui, view: ViewState) -> Option<Cmd> {
        use super::kit;
        let tier = Tier::of(ui.ctx().content_rect().width());
        let mut hit = None;
        let n = self.selection().len();
        let sel_label = |a: super::select::Action| a.label(n.max(1));
        ui.horizontal_centered(|ui| {
            if kit::toggle(ui, ph::SIDEBAR_SIMPLE, "", view.sidebar)
                .on_hover_text(SIDEBAR_LABEL.as_str())
                .clicked()
            {
                hit = Some(Cmd::Sidebar);
            }
            ui.separator();
            // ── 这个目录 ──
            let new_label = NEW_LABEL.clone();
            super::kit::menu(ui, (ph::PLUS, new_label.as_str(), ph::CARET_DOWN), |ui| {
                if ui
                    .button((ph::FOLDER_PLUS, NEW_FOLDER_ITEM.as_str()))
                    .clicked()
                {
                    hit = Some(Cmd::Mkdir);
                    ui.close();
                }
                if ui.button((ph::FILE_PLUS, NEW_FILE_ITEM.as_str())).clicked() {
                    hit = Some(Cmd::NewFile);
                    ui.close();
                }
            });
            let up = super::upload::UPLOAD_LABEL.as_str();
            if kit::ghost(ui, ph::UPLOAD_SIMPLE, up, true, "").clicked() {
                hit = Some(Cmd::Upload);
            }
            if tier == Tier::Narrow {
                super::kit::menu(ui, egui::RichText::new(ph::DOTS_THREE).size(16.0), |ui| {
                    if ui.button(TERMINAL_LABEL.as_str()).clicked() {
                        hit = Some(Cmd::Term);
                    }
                    ui.separator();
                    for a in PICK_SHOWN.into_iter().chain(PICK_MORE) {
                        let why = self.command_blocked(a);
                        let r = ui.add_enabled(why.is_none(), egui::Button::new(sel_label(a)));
                        let r = match &why {
                            Some(w) => r.on_disabled_hover_text(w),
                            None => r,
                        };
                        if r.clicked() {
                            hit = Some(Cmd::Act(a));
                        }
                    }
                    ui.separator();
                    for (c, label, on) in [
                        (Cmd::Hidden, HIDDEN_LABEL.as_str(), self.shows_hidden()),
                        (Cmd::Split, super::workspace::SPLIT_LABEL.as_str(), view.two),
                        (
                            Cmd::Preview,
                            super::workspace::PREVIEW_LABEL.as_str(),
                            view.preview,
                        ),
                    ] {
                        if ui.add(egui::Button::selectable(on, label)).clicked() {
                            hit = Some(c);
                        }
                    }
                })
                .on_hover_text(MORE_LABEL.as_str());
                return;
            }
            if kit::ghost(ui, ph::TERMINAL_WINDOW, TERMINAL_LABEL.as_str(), true, "")
                .on_hover_text(copy_text("rsFilewinChrome.command.termHint", &[]))
                .clicked()
            {
                hit = Some(Cmd::Term);
            }
            ui.separator();
            // ── 选中的 ──
            let icons_only = tier != Tier::Wide;
            for a in PICK_SHOWN {
                let why = self.command_blocked(a);
                let label = sel_label(a);
                let r = kit::ghost(
                    ui,
                    pick_icon(a),
                    if icons_only { "" } else { &label },
                    why.is_none(),
                    why.as_deref().unwrap_or(""),
                );
                let r = if icons_only && why.is_none() {
                    r.on_hover_text(&label)
                } else {
                    r
                };
                if r.clicked() {
                    hit = Some(Cmd::Act(a));
                }
            }
            if view.two {
                let r = kit::ghost(
                    ui,
                    ph::COPY,
                    if icons_only {
                        ""
                    } else {
                        super::workspace::COPY_ACROSS_LABEL.as_str()
                    },
                    true,
                    "",
                );
                let r = if icons_only {
                    r.on_hover_text(super::workspace::COPY_ACROSS_LABEL.as_str())
                } else {
                    r
                };
                if r.clicked() {
                    hit = Some(Cmd::Across);
                }
            }
            super::kit::menu(ui, egui::RichText::new(ph::DOTS_THREE).size(16.0), |ui| {
                for a in PICK_MORE {
                    let why = self.command_blocked(a);
                    let r = ui.add_enabled(
                        why.is_none(),
                        egui::Button::new(format!("{} {}", pick_icon(a), sel_label(a))),
                    );
                    let r = match &why {
                        Some(w) => r.on_disabled_hover_text(w),
                        None => r,
                    };
                    if r.clicked() {
                        hit = Some(Cmd::Act(a));
                        ui.close();
                    }
                }
            })
            .on_hover_text(MORE_LABEL.as_str());
            // ── 看法（右端）──
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let short = tier != Tier::Wide;
                for (c, icon, label, on) in [
                    (
                        Cmd::Preview,
                        ph::SIDEBAR,
                        super::workspace::PREVIEW_LABEL.as_str(),
                        view.preview,
                    ),
                    (
                        Cmd::Split,
                        ph::COLUMNS,
                        super::workspace::SPLIT_LABEL.as_str(),
                        view.two,
                    ),
                    (
                        Cmd::Hidden,
                        ph::EYE,
                        HIDDEN_LABEL.as_str(),
                        self.shows_hidden(),
                    ),
                ] {
                    let mut r = kit::toggle(ui, icon, if short { "" } else { label }, on);
                    r = if c == Cmd::Hidden {
                        r.on_hover_text(copy_text("rsFilewinChrome.hidden.hint", &[]))
                    } else if short {
                        r.on_hover_text(label)
                    } else {
                        r
                    };
                    if r.clicked() {
                        hit = Some(c);
                    }
                }
            });
        });
        hit
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
            Cmd::Act(a) => {
                self.perform(a, ctx);
            }
            Cmd::Hidden => {
                let on = self.shows_hidden();
                self.set_show_hidden(!on);
            }
            Cmd::Sidebar | Cmd::Split | Cmd::Preview | Cmd::Across => {}
        }
    }

    /// 在跑的传输有几个（上传 · 下载 · 跨机复制）。
    pub fn transfers_running(&self) -> usize {
        self.board.cancels().in_flight_names().len()
            + usize::from(self.pull.in_flight().is_some())
            + usize::from(self.cross_board.running().is_some())
    }

    /// 状态栏那一行写什么（左半）：项数 · 隐藏几项 · 选中几项（合计大小）；搜索结果摆着时写结果数。判据与界面看同一个值。
    pub fn status_line(&self) -> String {
        let mut s = match self.search.total().filter(|_| self.showing_hits()) {
            Some(n) => copy_text("rsFilewinChrome.status.hits", &[("n", &n.to_string())]),
            None => {
                let n = self.listing.rows.lock().unwrap().len();
                let hidden = self.listing.hidden.lock().unwrap().len();
                if hidden > 0 {
                    copy_text(
                        "rsFilewinChrome.status.countHidden",
                        &[("n", &n.to_string()), ("hidden", &hidden.to_string())],
                    )
                } else {
                    copy_text("rsFilewinChrome.status.count", &[("n", &n.to_string())])
                }
            }
        };
        let picked = self.picked_rows();
        if !picked.is_empty() && !self.showing_hits() {
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

    /// 🔴 **状态栏**：左边项数 · 选中；右边「进度」按钮（有在跑的写个数 ＋ 小进度条；有没看过的失败带红点）。
    /// 缩放那一颗由窗口那一级画在它左边（[`Workspace::chrome_ui`]）。
    pub fn status_ui(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.ctx());
        let mut toggle = false;
        // 键位做不成的那一下（「没有以 q 开头的」「一次只能改一个名字」…）浮在左端，顶替项数那一句。
        match self.key_notice() {
            Some(n) => {
                ui.add(egui::Label::new(egui::RichText::new(n).color(p.warn)).truncate());
            }
            None => {
                let line = self.status_line();
                ui.add(egui::Label::new(egui::RichText::new(&line).color(p.text2)).truncate())
                    .on_hover_text(line);
            }
        }
        let n = self.transfers_running();
        let unseen_fail = self.transfers_unseen() && !self.transfers_open;
        if n > 0 || self.transfers_unseen() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let caret = if self.transfers_open {
                    ph::CARET_DOWN
                } else {
                    ph::CARET_UP
                };
                let text = if n > 0 {
                    copy_text("rsFilewinChrome.status.transfers", &[("n", &n.to_string())])
                } else {
                    copy_text("rsFilewinChrome.status.progress", &[])
                };
                let r = ui.add(
                    egui::Button::new(egui::RichText::new(format!("{text} {caret}")).small())
                        .frame_when_inactive(false),
                );
                if unseen_fail {
                    ui.painter().circle_filled(
                        r.rect.left_center() + egui::vec2(8.0, 0.0),
                        3.5,
                        p.error,
                    );
                }
                if r.clicked() {
                    toggle = true;
                }
            });
        }
        if toggle {
            self.transfers_open = !self.transfers_open;
        }
    }
}

/// 命令栏上那几个开关此刻的样子（窗口那一级的状态）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewState {
    pub sidebar: bool,
    pub two: bool,
    pub preview: bool,
}

/// 命令栏上那几颗的字 —— **唯一住址**（判据按同一个常量去找它画出来的字）。
pub static TERMINAL_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinShell.frame.terminal", &[]));
pub static HIDDEN_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinChrome.command.hidden", &[]));
pub static NEW_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinChrome.command.new", &[]));
pub static NEW_FOLDER_ITEM: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinChrome.command.newFolder", &[]));
pub static NEW_FILE_ITEM: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinChrome.command.newFile", &[]));
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
        let mut cmd = None;
        let view = ViewState {
            sidebar: self.sidebar_open,
            two: self.sides() == 2,
            preview: self.preview.is_some(),
        };
        egui::Panel::top("filewin-commands")
            .frame(bar_frame(&ctx))
            .exact_size(bar_h - 4.0)
            .show_separator_line(false)
            .show(ui, |ui| {
                cmd = self.pane_on_mut(f).command_bar_ui(ui, view);
            });
        let sidebar = cmd == Some(Cmd::Sidebar);
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
        let zoom = ctx.zoom_factor();
        let mut reset_zoom = false;
        let k = metrics(&ctx).space;
        egui::Panel::bottom("filewin-status")
            .frame(bar_frame(&ctx).inner_margin(egui::Margin::symmetric(k[4] as i8, 0)))
            .exact_size(28.0)
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    // 缩放不是 100% ⇒ 状态栏常驻一颗「125%」，点了回 100%。
                    if (zoom - 1.0).abs() > 0.001 {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(copy_text(
                                            "rsFilewinChrome.status.zoom",
                                            &[("pct", &format!("{:.0}", zoom * 100.0))],
                                        ))
                                        .small(),
                                    )
                                    .frame_when_inactive(false),
                                )
                                .on_hover_text(copy_text("rsFilewinChrome.status.zoomReset", &[]))
                                .clicked()
                            {
                                reset_zoom = true;
                            }
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| self.pane_on_mut(f).status_ui(ui),
                            );
                        });
                    } else {
                        self.pane_on_mut(f).status_ui(ui);
                    }
                });
            });
        if reset_zoom {
            ctx.set_zoom_factor(1.0);
            self.zoom_reset();
        }
        if self.sidebar_open {
            // 窗口窄了左栏跟着让（至多占三成），里面的字截成「…」。
            let w = SIDEBAR_WIDTH.min(ctx.content_rect().width() * 0.3);
            egui::Panel::left("filewin-sidebar")
                .frame(bar_frame(&ctx).inner_margin(egui::Margin::same(10)))
                .exact_size(w)
                .show(ui, |ui| self.sidebar_ui(ui));
        }
    }

    /// 🔴 **左栏**：书签（点了跳过去 · 悬停出 × 删掉）· 这台机器（小标题写机器名；主目录 · 根目录）· 其他机器（点了开那台的窗口）。
    /// 当前所在的那一项左边一道 2px 强调色条（不铺底，与列表的选中分开）。
    fn sidebar_ui(&mut self, ui: &mut egui::Ui) {
        use super::kit::{side_item, SideMark};
        let p = palette(ui.ctx());
        let f = self.focus();
        let mut go: Option<String> = None;
        let mut open_other: Option<String> = None;
        let mut want_home = false;
        let section = |ui: &mut egui::Ui, title: String| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(title).color(p.text2).small().strong());
            ui.add_space(2.0);
        };
        let here_dir = super::bookmarks::normalize_dir(&self.pane_on(f).cwd);
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                // ── 书签 ──
                section(ui, copy_text("rsFilewinChrome.side.bookmarks", &[]));
                let pane = self.pane_on(f);
                if let Some(shelf) = pane.shelf.clone() {
                    let list = shelf.list();
                    if list.is_empty() {
                        ui.label(egui::RichText::new(copy_text("rsFilewinChrome.side.noBookmarks", &[])).color(p.text2).small());
                        ui.label(egui::RichText::new(copy_text("rsFilewinChrome.side.noBookmarksHint", &[])).color(p.faint).small());
                    }
                    let mut drop: Option<String> = None;
                    for d in list {
                        let tail = super::source::remote_basename(&d);
                        let tail = if tail.is_empty() { "/" } else { tail };
                        let here = super::bookmarks::normalize_dir(&d) == here_dir;
                        let r = side_item(ui, ph::STAR, tail, here, SideMark::None).on_hover_text(&d);
                        if r.clicked() {
                            go = Some(d.clone());
                        }
                        // × 只在悬停这一行时出（贴右端）。
                        if r.hovered() || r.contains_pointer() {
                            let xr = egui::Rect::from_center_size(
                                egui::pos2(r.rect.right() - 14.0, r.rect.center().y),
                                egui::vec2(18.0, 18.0),
                            );
                            let xresp = ui
                                .interact(xr, r.id.with("drop"), egui::Sense::click())
                                .on_hover_text(copy_text("rsFilewinBookmarks.bar.removeHint", &[]));
                            ui.painter().text(
                                xr.center(),
                                egui::Align2::CENTER_CENTER,
                                ph::X,
                                egui::FontId::proportional(12.0),
                                if xresp.hovered() { p.text } else { p.text2 },
                            );
                            if xresp.clicked() {
                                drop = Some(d.clone());
                                go = None;
                            }
                        }
                    }
                    if let Some(n) = shelf.notice() {
                        ui.colored_label(p.warn, n);
                    }
                    if let Some(d) = drop {
                        shelf.remove(&d);
                    }
                }
                // ── 这台机器（小标题就写机器名）──
                ui.add_space(8.0);
                section(ui, self.pane_on(f).source.label());
                let home = self.home.0.lock().unwrap().clone();
                let (mark, tip) = match &home {
                    HomeState::Known(h) => (SideMark::None, h.clone()),
                    HomeState::Failed(why) => {
                        tracing::warn!("filewin: home not known: {why}");
                        (SideMark::Warn, copy_text("rsFilewinChrome.side.homeFailed", &[]))
                    }
                    _ => (SideMark::Busy, String::new()),
                };
                let here_home = matches!(&home, HomeState::Known(h) if super::bookmarks::normalize_dir(h) == here_dir);
                let r = side_item(ui, ph::HOUSE, &copy_text("rsFilewinChrome.side.home", &[]), here_home, mark);
                let r = if tip.is_empty() { r } else { r.on_hover_text(&tip) };
                if r.clicked() {
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
                if side_item(ui, ph::HARD_DRIVE, &copy_text("rsFilewinChrome.side.root", &[]), here_dir == "/", SideMark::None)
                    .on_hover_text("/")
                    .clicked()
                {
                    go = Some("/".to_string());
                }
                // ── 其他机器 ──
                let others = self.other_machines();
                if !others.is_empty() {
                    ui.add_space(8.0);
                    section(ui, copy_text("rsFilewinChrome.side.otherMachines", &[]));
                    for m in others {
                        let tip = copy_text("rsFilewinChrome.side.openOther", &[("machine", &m)]);
                        if side_item(ui, ph::MONITOR, &m, false, SideMark::None).on_hover_text(tip).clicked() {
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
