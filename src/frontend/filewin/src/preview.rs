//! **预览**：窗口右侧一块只读面板，显示焦点那一栏里选中的**恰好一个文件**：文本（认得的代码按语法上色）· 图片。
//!
//! 设计住第四节；这里只留落地要知道的。
//!
//! # 一、读：已有那条命令，自己的上限
//!
//! 经通道问后端 `files-read-text`（编辑器读文本走的同一条命令），读不读、读多少、按哪种语法上色都照列表那一行上
//! 后端给的门（`gates.preview`）；不许 ⇒ 摆出后端写好的那一句，不读（不截一半来预览：截断的文本会被当成全文）。
//!
//! # 二、节奏：同一时刻最多一趟在飞，最新的赢 —— 零定时器
//!
//! 光标挪到 A ⇒ 发 A；A 还在飞时又挪到 B、C ⇒ 只记下「要 C」；A 落地 ⇒ 丢掉（已经不是要的那一项），
//! 立刻发 C。⇒ 连按三下线上恰好两趟（第一项 ＋ 最后一项），中间那几项一个字节都不读。
//! 没有去抖的定时器（`rust_timer_registry` 不动），靠的是「落地那一刻再看一眼要的是谁」。
//!
//! # 三、画：只排视口内的行
//!
//! 行起点在到货时算一次；长行按窗格宽度折行，每一行折完多高在宽度变了 / 换了文本时量一次，每帧只排看得见的那几行。
//!
//! # 四、图片：后端按上限给字节，窗口解码
//!
//! 门说当图读 ⇒ 按它给的上限读；读与解码住 [`super::picture`]：与查看页共用那一份（同一张图只读一次、只解一次，谁先要谁读）。
//!
//! # ⚠ 买不到什么
//!
//! - 二进制、PDF、视频预览（说为什么不预览）。
//! - 真远端上一趟的时延读数（判据挂的是合成后端）。

use copy_core::copy_text;
use std::sync::{Arc, Mutex};

use super::shell::{FileWindow, NO_LINE};

/// 面板上摆着的是什么。
#[derive(Clone, PartialEq, Eq)]
pub enum View {
    /// 没有可读的（没选 / 选了几项 / 目录 / 太大 …），那句话说清为什么。
    Idle(String),
    /// 正在读这一份。
    Loading(String),
    /// 读到了。`starts` ＝ 每一行在 `text` 里的起点（到货时算一次）；`lang` ＝ 按哪种语法上色（`None` ＝ 照纯文本画）。
    Text {
        path: String,
        text: String,
        starts: Vec<usize>,
        lang: Option<String>,
    },
    /// 一张图（与查看页共用那一份；`size` ＝ 原图多大）。
    Image {
        path: String,
        size: [usize; 2],
        tex: egui::TextureHandle,
    },
    /// 读了，没读成（不是文本 / 读不到），那句话。
    Said(String),
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            View::Idle(s) => write!(f, "Idle({s:?})"),
            View::Loading(p) => write!(f, "Loading({p:?})"),
            View::Text { path, lang, .. } => write!(f, "Text({path:?}, {lang:?})"),
            View::Image { path, size, .. } => write!(f, "Image({path:?}, {size:?})"),
            View::Said(s) => write!(f, "Said({s:?})"),
        }
    }
}

/// 一趟读文本的结局（`None` ＝ 后端说不是文本），由 tokio 那条线写、UI 线程取。
type Arrival = (String, Result<Option<String>, String>);

/// 要的这一份怎么读。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Want {
    Text,
    Image,
}

/// 预览的状态（**UI 线程自己的**，只有到货那一格跨线程）。
pub struct Preview {
    /// 上一帧看的是哪一项（当前目录 ＋ 选中的那个名字 / 选中了几项）。变了才重新判。
    key: Option<(String, Result<String, usize>)>,
    /// 要显示的那一份（`None` ＝ 这一项不读）。
    want: Option<String>,
    /// 那一份发出去的线上路径（当前目录有损时按字节寻址，[`FileWindow::row_path`]）。
    wire: cmd_wire::files::WirePath,
    /// 那一份按文本读还是按图读。
    how: Want,
    /// 那一份许读多少（门给的）。
    cap: u64,
    /// 那一份按哪种语法上色（门给的；纯文本 ⇒ `None`）。
    lang: Option<String>,
    /// 按图读时那一张的钥匙（[`super::picture::Pictures`] 里认它）。
    pic: Option<super::picture::PicKey>,
    /// 起过读、还在读的那一张（同一时刻最多一张在读：最新的赢，同文本那一条）。
    pic_wait: Option<super::picture::PicKey>,
    /// 上色那一套缓存：每一份文本到货时按行切好的上色结果（行号 → 那一行的排版）。
    painted: Vec<egui::text::LayoutJob>,
    /// 折行之后每一行的顶（键 ＝ 哪一份文本 ＋ 窗格多宽）：宽度变了 / 换了文本才重量。
    wrapped: std::cell::RefCell<Option<((String, u64, i32), Vec<f32>)>>,
    view: View,
    /// 有一趟在飞吗（UI 线程记的）。
    inflight: bool,
    /// 发出去过几趟（判据数它，也与线上那本账对拍）。
    fired: u64,
    slot: Arc<Mutex<Option<Arrival>>>,
    /// 头上那一行：选中的那一份（名字 · 大小 · 修改时间 · 能不能编辑 · 能不能下载）。没选中 / 多选 ⇒ `None`。
    head: Option<Head>,
}

/// 预览头上那一行要的几格（都是那一行列表项上已有的）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Head {
    pub name: String,
    /// 画哪种图标（后端判的）。
    pub class: cmd_wire::files::FileClass,
    pub size: u64,
    /// 修改时间的短写法（列表那一行上后端写好的那一格）。
    pub mtime_text: Option<String>,
    pub edit: bool,
    /// 「查看」（窗口解得开的图：开一个查看页，顶替「编辑」）。
    pub view: bool,
    pub download: bool,
    /// 计划反查：这一份归的那一格（`(片名, 那一格)`）；目录不在任何一片里 · 没人声明 ⇒ `None`。
    pub owner: Option<(String, super::plan::Owner)>,
}

/// 那一栏里这一份归哪一格（反查结果跟着列目录异步到，所以每次 [`Preview::follow`] 都现查一次）。
fn owner_of(pane: &FileWindow, name: &str) -> Option<(String, super::plan::Owner)> {
    let plan = pane.listing.plan.lock().unwrap();
    let d = plan.as_ref()?;
    d.owner(name).map(|o| (d.slice.clone(), o.clone()))
}

/// 预览头上点了哪一颗（做事落回焦点那一栏）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewAct {
    Edit,
    /// 开一个查看页（图片）。
    View,
    Download,
    /// 「归属」那一块上的［在计划里看］。
    OpenInPlan,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            key: None,
            want: None,
            wire: cmd_wire::files::WirePath::default(),
            how: Want::Text,
            cap: 0,
            lang: None,
            pic: None,
            pic_wait: None,
            painted: Vec::new(),
            wrapped: Default::default(),
            view: View::Idle(PICK_ONE.to_string()),
            inflight: false,
            fired: 0,
            slot: Arc::new(Mutex::new(None)),
            head: None,
        }
    }
}

/// 预览头「归属」那一块：左边一道状态色（做完绿 · 其余灰）；状态图标 ＋ 标题 ·［在计划里看］；
/// 一行「{片} · 块「…」· 签 …」；被两格声明 ⇒ 一行红字；对账「坏」⇒ 一行 pb 给的原因。回值 ＝ 点了［在计划里看］。
fn owner_block(ui: &mut egui::Ui, slice: &str, o: &super::plan::Owner) -> bool {
    let p = super::theme::palette(ui.ctx());
    let m = super::theme::metrics(ui.ctx());
    let tint = match o.status {
        super::plan::Status::Done => p.success,
        _ => p.text2,
    };
    let mut clicked = false;
    let r = egui::Frame::new()
        .fill(p.card)
        .corner_radius(m.radius_m)
        .inner_margin(egui::Margin {
            left: 12,
            right: 10,
            top: 8,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(super::plan::status_icon(o.status)).color(tint));
                ui.add(
                    egui::Label::new(egui::RichText::new(&o.title).color(p.text).strong())
                        .truncate(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let b = ui.add(
                        egui::Label::new(
                            egui::RichText::new(super::plan::open_label()).color(p.accent),
                        )
                        .sense(egui::Sense::click()),
                    );
                    if b.clicked() {
                        clicked = true;
                    }
                    b.on_hover_cursor(egui::CursorIcon::PointingHand);
                });
            });
            ui.label(
                egui::RichText::new(super::plan::owner_line(slice, o))
                    .color(p.text2)
                    .small(),
            );
            if !o.dup.is_empty() {
                ui.label(
                    egui::RichText::new(format!(
                        "{} {}",
                        egui_phosphor::regular::WARNING,
                        super::plan::dup_text(o)
                    ))
                    .color(p.error)
                    .small(),
                );
            }
            if let Some(why) = &o.broken {
                ui.label(egui::RichText::new(why).color(p.warn).small());
            }
        });
    // 左边那一道状态色。
    let band = r.response.rect;
    ui.painter().rect_filled(
        egui::Rect::from_min_max(band.min, egui::pos2(band.left() + 3.0, band.bottom())),
        egui::CornerRadius {
            nw: m.radius_m as u8,
            sw: m.radius_m as u8,
            ne: 0,
            se: 0,
        },
        tint,
    );
    clicked
}

/// 什么都没选时那一句。
pub static PICK_ONE: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinPreview.pickOne.message", &[]));

/// 每一行的起点（第 0 行从 0 起；`\n` 之后是下一行）。
pub fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

impl Preview {
    /// 面板上摆着什么（判据与界面看同一个值）。
    pub fn view(&self) -> &View {
        &self.view
    }

    /// 发出去过几趟。
    pub fn fired(&self) -> u64 {
        self.fired
    }

    /// 🔴 **每帧调一次**：先收到货，再看「要的是谁」变没变，最后决定发不发。
    pub fn follow(&mut self, pane: &FileWindow, ctx: Option<egui::Context>) {
        if let Some(h) = self.head.as_mut() {
            h.owner = owner_of(pane, &h.name);
        }
        // ① 收货：是要的那一份才摆出来，不是就丢掉（光标已经挪走了）。
        let arrived = self.slot.lock().unwrap().take();
        if let Some((path, r)) = arrived {
            self.inflight = false;
            if self.want.as_deref() == Some(path.as_str()) {
                self.painted.clear();
                self.view = match r {
                    Ok(Some(text)) => {
                        let lang = self.lang.clone();
                        if let (Some(l), Some(c)) = (&lang, &ctx) {
                            self.painted = highlight_lines(c, &text, l);
                        }
                        View::Text {
                            starts: line_starts(&text),
                            path,
                            text,
                            lang,
                        }
                    }
                    Ok(None) => View::Said(copy_text(
                        "rsFilewinPreview.follow.notText",
                        &[("name", &(super::source::remote_basename(&path)).to_string())],
                    )),
                    Err(why) => View::Said(copy_text(
                        "rsFilewinPreview.follow.failed",
                        &[("why", &why.to_string())],
                    )),
                };
            }
        }
        // ①′ 图：在读的那一张读完了没有；要的那一张好了 ⇒ 摆出来（共用那一份，不另解）。
        self.settle_picture(pane, ctx.as_ref());
        // ② 要的是谁：选中的那一项换了才重新判（按名字，O(1)；找那一行的元数据是 O(n)，只在换了的那一帧做一次）。
        let key = (pane.cwd.clone(), pane.picked_name());
        if self.key.as_ref() != Some(&key) {
            self.key = Some(key.clone());
            self.decide(pane, key.1);
        }
        // ③ 发：要一份、还没摆出来、也没有一趟在飞 ⇒ 发。
        let busy = self.inflight || self.pic_wait.is_some();
        if let (Some(p), View::Loading(_), false) = (self.want.clone(), &self.view, busy) {
            self.fire(pane, p, ctx);
        }
    }

    /// 此刻这块面板还摆着 / 还在等的那几张图（窗口每帧报给 [`super::picture::Pictures::keep_only`]）。
    pub fn picture_keys(&self) -> Vec<super::picture::PicKey> {
        self.pic
            .iter()
            .chain(self.pic_wait.iter())
            .cloned()
            .collect()
    }

    /// 图那一支的收货：在读的那一张读完了（成了 / 没成 / 被扔了）⇒ 不再等它；要的那一张好了 ⇒ 摆出来。
    fn settle_picture(&mut self, pane: &FileWindow, ctx: Option<&egui::Context>) {
        use super::picture::Look;
        if let Some(k) = &self.pic_wait {
            if !matches!(pane.pictures.look(k, None), Some(Look::Reading { .. })) {
                self.pic_wait = None;
            }
        }
        let (Some(key), View::Loading(_)) = (self.pic.clone(), &self.view) else {
            return;
        };
        let Some(path) = self.want.clone() else {
            return;
        };
        match pane.pictures.look(&key, ctx) {
            Some(Look::Ready(p)) => {
                self.view = View::Image {
                    path,
                    size: [p.native[0] as usize, p.native[1] as usize],
                    tex: p.tex,
                };
            }
            Some(Look::Failed(why)) => {
                self.view = View::Said(copy_text(
                    "rsFilewinPreview.follow.failed",
                    &[("why", &why)],
                ));
            }
            _ => {}
        }
    }

    /// 选中的那一项 → 要不要读、不读的话说什么。
    fn decide(&mut self, pane: &FileWindow, picked: Result<String, usize>) {
        self.want = None;
        self.pic = None;
        self.head = None;
        let name = match picked {
            Ok(n) => n,
            Err(0) => {
                self.view = View::Idle(PICK_ONE.to_string());
                return;
            }
            Err(n) => {
                self.view = View::Idle(copy_text(
                    "rsFilewinPreview.decide.many",
                    &[("n", &n.to_string())],
                ));
                return;
            }
        };
        let Some(r) = pane.row_named(&name) else {
            self.view = View::Idle(copy_text(
                "rsFilewinPreview.decide.gone",
                &[("name", &name.to_string())],
            ));
            return;
        };
        if !r.opens_as_dir() {
            let can = super::select::actions_for(&[&r]);
            self.head = Some(Head {
                name: r.name.clone(),
                class: r.class,
                size: r.size,
                mtime_text: r.mtime_text.clone(),
                edit: can.contains(&super::select::Action::Edit),
                view: can.contains(&super::select::Action::View),
                download: can.contains(&super::select::Action::Download),
                owner: owner_of(pane, &r.name),
            });
        }
        if r.opens_as_dir() {
            self.view = View::Idle(copy_text(
                "rsFilewinPreview.decide.isDir",
                &[("name", &name.to_string())],
            ));
            return;
        }
        if r.lossy_name {
            self.view = View::Idle(copy_text(
                "rsFilewinPreview.decide.badName",
                &[("name", &name.to_string())],
            ));
            return;
        }
        match r.gates.as_ref().map(|g| &g.preview) {
            None => {
                self.view = View::Idle(copy_text("rsFilewinSource.gates.unlisted", &[]));
            }
            Some(cmd_wire::files::PreviewGate::No(said)) => {
                self.view = View::Idle(said.clone());
            }
            Some(cmd_wire::files::PreviewGate::Image(cap)) => {
                self.how = Want::Image;
                self.cap = *cap;
                self.pic = Some(super::picture::PicKey::of(&r));
                self.wire = pane.row_path(&r).path();
                self.want = Some(r.path.clone());
                self.view = View::Loading(r.path.clone());
            }
            Some(cmd_wire::files::PreviewGate::Text(t)) => {
                self.how = Want::Text;
                self.cap = t.cap;
                self.lang = t.lang.clone();
                self.wire = pane.row_path(&r).path();
                self.want = Some(r.path.clone());
                self.view = View::Loading(r.path.clone());
            }
        }
    }

    /// 发一趟 `files-read-text`。没运行时 / 没通道 ⇒ 出声，不发。
    fn fire(&mut self, pane: &FileWindow, path: String, ctx: Option<egui::Context>) {
        let (Some(h), Some(line)) = (pane.rt.clone(), pane.line.clone()) else {
            self.view = View::Said(NO_LINE.to_string());
            self.want = None;
            return;
        };
        let origin = pane.source.origin();
        if self.how == Want::Image {
            // 图：交给共用那一份（已有 ⇒ 不再读，`fired` 不加）。
            let Some(key) = self.pic.clone() else {
                return;
            };
            let side = ctx
                .as_ref()
                .map_or(2048, |c| c.input(|i| i.max_texture_side))
                .min(super::picture::MAX_SIDE as usize) as u32;
            if pane.pictures.load(
                &key,
                super::picture::Load {
                    rt: h,
                    line,
                    origin,
                    wire: self.wire.clone(),
                    cap: self.cap,
                    side,
                    ctx,
                },
            ) {
                self.fired += 1;
            }
            self.pic_wait = Some(key);
            return;
        }
        let slot = self.slot.clone();
        self.inflight = true;
        self.fired += 1;
        let wire = self.wire.clone();
        let max_bytes = self.cap;
        h.spawn(async move {
            let req = cmd_wire::files::ReadTextReq {
                path: wire,
                max_bytes,
            };
            let r = super::editor::text_from_reply(
                super::source::ask_typed(&line, &origin, &req, super::editor::READ_BUDGET).await,
            )
            .map_err(|f| f.said);
            *slot.lock().unwrap() = Some((path, r));
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
    }

    /// 画这块面板。
    pub fn ui(&self, ui: &mut egui::Ui) -> Option<PreviewAct> {
        let p = super::theme::palette(ui.ctx());
        let mut act = None;
        // 头：文件名（14 / 600）＋「571 B · 13:36」＋「编辑」「下载」；没选中一份 ⇒ 只写「预览」。
        match &self.head {
            Some(h) => {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(super::kind::icon(h.class)).color(p.text2));
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&h.name)
                                .size(14.0)
                                .strong()
                                .color(p.text),
                        )
                        .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if h.download
                            && super::kit::ghost(
                                ui,
                                egui_phosphor::regular::DOWNLOAD_SIMPLE,
                                &super::download::DOWNLOAD_LABEL,
                                true,
                                "",
                            )
                            .clicked()
                        {
                            act = Some(PreviewAct::Download);
                        }
                        if h.view
                            && super::kit::ghost(
                                ui,
                                egui_phosphor::regular::EYE,
                                &super::imgview::VIEW_LABEL,
                                true,
                                "",
                            )
                            .clicked()
                        {
                            act = Some(PreviewAct::View);
                        }
                        if h.edit
                            && super::kit::ghost(
                                ui,
                                egui_phosphor::regular::PENCIL_SIMPLE,
                                &super::editor::EDIT_LABEL,
                                true,
                                "",
                            )
                            .clicked()
                        {
                            act = Some(PreviewAct::Edit);
                        }
                    });
                });
                let mut meta = copy_core::size_text(h.size);
                if let Some(t) = &h.mtime_text {
                    meta = copy_text("rsFilewinPreview.ui.meta", &[("size", &meta), ("time", t)]);
                }
                ui.label(egui::RichText::new(meta).color(p.text2).small());
                ui.add_space(6.0);
                if let Some((slice, o)) = &h.owner {
                    if owner_block(ui, slice, o) {
                        act = Some(PreviewAct::OpenInPlan);
                    }
                    ui.add_space(6.0);
                }
            }
            None => {
                ui.strong(&copy_text("rsFilewinPreview.ui.title", &[]));
            }
        }
        match &self.view {
            View::Idle(s) | View::Said(s) => {
                ui.label(s);
            }
            View::Loading(p) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    super::shell::fit_label(
                        ui,
                        copy_text(
                            "rsFilewinPreview.ui.reading",
                            &[("name", &(super::source::remote_basename(p)).to_string())],
                        ),
                        0.0,
                    );
                });
            }
            View::Text {
                path, text, starts, ..
            } => {
                egui::Frame::new()
                    .fill(ui.visuals().code_bg_color)
                    .corner_radius(6)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("filewin-preview")
                            .auto_shrink([false; 2])
                            .show_viewport(ui, |ui, vp| self.text_ui(ui, vp, path, text, starts));
                    });
            }
            View::Image { size, tex, .. } => {
                ui.label(
                    egui::RichText::new(copy_text(
                        "rsFilewinPreview.ui.imageSize",
                        &[("w", &size[0].to_string()), ("h", &size[1].to_string())],
                    ))
                    .color(super::theme::palette(ui.ctx()).text2)
                    .small(),
                );
                let avail = ui.available_size();
                let (w, h) = (size[0] as f32, size[1] as f32);
                let k = (avail.x / w).min(avail.y / h).min(1.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Image::new((tex.id(), egui::vec2(w * k, h * k))));
                });
            }
        }
        act
    }
}

impl Preview {
    /// 一行文本的排版：按窗格宽度折行（上了色的那一份照它的颜色，没上色的照正文色）。
    fn line_job(
        &self,
        ui: &egui::Ui,
        text: &str,
        starts: &[usize],
        i: usize,
        width: f32,
    ) -> egui::text::LayoutJob {
        // 上过色的空行没有任何一段 ⇒ 排出来高 0（空行会塌掉）⇒ 照正文排。
        let mut job = match self.painted.get(i).filter(|j| !j.sections.is_empty()) {
            Some(j) => j.clone(),
            None => {
                let end = starts.get(i + 1).map_or(text.len(), |e| e - 1);
                egui::text::LayoutJob::simple(
                    text[starts[i]..end].trim_end_matches('\r').to_string(),
                    egui::TextStyle::Monospace.resolve(ui.style()),
                    ui.visuals().text_color(),
                    width,
                )
            }
        };
        job.wrap.max_width = width;
        job
    }

    /// 🔴 **折行 ＋ 只排视口内的行**：每一行折完多高在宽度变了 / 换了一份文本时量一次（`tops[i]` ＝ 第 i 行的顶），
    /// 每帧按视口二分出第一行，只排看得见的那几行。
    fn text_ui(&self, ui: &mut egui::Ui, vp: egui::Rect, path: &str, text: &str, starts: &[usize]) {
        let width = ui.available_width().max(40.0).floor();
        // 认的是这一份文本本身（不只是字节数）：同一个文件换了内容、恰好一样长时，旧的 `tops` 行数对不上
        // （`preview_tests::same_size_new_text_with_more_lines_does_not_reuse_the_old_wrap`）。正文封顶 64 KiB，每帧算一遍摘要不贵。
        let digest = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            text.hash(&mut h);
            h.finish()
        };
        let key = (path.to_string(), digest, width as i32);
        let mut cache = self.wrapped.borrow_mut();
        if cache.as_ref().map(|(k, _)| k) != Some(&key) {
            let mut tops = Vec::with_capacity(starts.len() + 1);
            let mut y = 0.0;
            for i in 0..starts.len() {
                tops.push(y);
                y += ui
                    .painter()
                    .layout_job(self.line_job(ui, text, starts, i, width))
                    .size()
                    .y;
            }
            tops.push(y);
            *cache = Some((key, tops));
        }
        let Some((_, tops)) = cache.as_ref() else {
            return;
        };
        ui.set_min_height(tops.last().copied().unwrap_or(0.0));
        let origin = ui.max_rect().min;
        let color = ui.visuals().text_color();
        let mut i = tops.partition_point(|&t| t <= vp.min.y).saturating_sub(1);
        // `tops` 与 `starts` 同一份文本量出来、一样长（上面那把钥匙担保）；仍按 `get` 取，对不上就停画，不崩。
        while let Some(&top) = tops.get(i).filter(|&&t| i < starts.len() && t < vp.max.y) {
            let g = ui
                .painter()
                .layout_job(self.line_job(ui, text, starts, i, width));
            ui.painter().galley(origin + egui::vec2(0.0, top), g, color);
            i += 1;
        }
    }
}

/// 整份文本按 `lang` 上色，再按行切开（每行一份排版，画的时候只排看得见的那几行）。
/// 语法库认不得这种语言 ⇒ 空（照纯文本画）。
pub fn highlight_lines(ctx: &egui::Context, text: &str, lang: &str) -> Vec<egui::text::LayoutJob> {
    use egui_extras::syntax_highlighting::{highlight, CodeTheme};
    let style = ctx.global_style();
    let theme = CodeTheme::from_style(&style);
    let whole = highlight(ctx, &style, &theme, text, lang);
    if whole.sections.is_empty() || whole.sections.len() == 1 && text.contains('\n') {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut sec = whole.sections.iter().peekable();
    let starts = line_starts(text);
    for (k, &start) in starts.iter().enumerate() {
        let end = starts.get(k + 1).copied().unwrap_or(text.len());
        let mut job = egui::text::LayoutJob::default();
        let line = text[start..end].trim_end_matches(['\n', '\r']);
        job.text = line.to_string();
        while let Some(s) = sec.peek() {
            let (a, b) = (s.byte_range.start.0, s.byte_range.end.0);
            if a >= end {
                break;
            }
            let (a, b) = (
                a.max(start) - start,
                b.min(start + line.len()).saturating_sub(start),
            );
            if b > a {
                job.sections.push(egui::text::LayoutSection {
                    leading_space: 0.0,
                    byte_range: egui::text::ByteIndex(a)..egui::text::ByteIndex(b),
                    format: s.format.clone(),
                });
            }
            if s.byte_range.end.0 <= end {
                sec.next();
            } else {
                break;
            }
        }
        out.push(job);
    }
    out
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/preview_tests.rs"]
mod tests;
