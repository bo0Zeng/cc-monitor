//! **预览**：窗口右侧一块只读面板，显示焦点那一栏里选中的**恰好一个文件**：文本（认得的代码按语法上色）· 图片。
//!
//! 设计住第四节；这里只留落地要知道的。
//!
//! # 一、读：已有那条命令，自己的上限
//!
//! 经通道问后端 `files-read-text`（编辑器读文本走的同一条命令，**不加子命令**），`max_bytes` 给
//! [`PREVIEW_MAX_BYTES`]。那个数刻意比编辑上限小得多：预览**跟着光标走**，↑↓ 一路按下去每一步都是一趟，
//! 而编辑上限是「用户点了编辑」那一下的量。超上限**不读、出声**（不截一半来预览：截断的文本会被当成全文）。
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
//! 认得的图片（png · jpg · gif · webp · bmp）经后端 `files-read-chunk` 逐块读回（每块到后端那一条的上限），
//! 整份不超过 [`IMAGE_MAX_BYTES`]（超了不读、出声）；解码在后台那条线上做，过大的图缩到 [`IMAGE_MAX_SIDE`] 再交给界面。
//!
//! # ⚠ 买不到什么
//!
//! - 二进制、PDF、视频预览（说为什么不预览）。
//! - 真远端上一趟的时延读数（判据挂的是合成后端）。

use copy_core::copy_text;
use std::sync::{Arc, Mutex};

use super::shell::{FileWindow, NO_LINE};

/// 预览读一份文本的上限（字节）。**唯一住址**；登记在 `byte_cap_registry`。
///
/// 超了：**不读、出声**（「有 N，预览只看 M 以内的文件」），不截断。
pub const PREVIEW_MAX_BYTES: u64 = 64 * 1024;

/// 预览一张图的上限（字节）。**唯一住址**；登记在 `byte_cap_registry`。超了：不读、出声。
pub const IMAGE_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// 解出来的图最长边超过它就先缩（面板里放不下更大的，也不往显存里塞一张大图）。
pub const IMAGE_MAX_SIDE: u32 = 2048;

/// 窗口自己解得开的图片扩展名。
pub const DECODABLE: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];

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
    /// 一张图（到货那一拍交给显存）。
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

/// 读回来的是什么。
pub enum Got {
    /// 文本（`None` ＝ 后端说不是文本）。
    Text(Option<String>),
    /// 解好的一张图。
    Image(egui::ColorImage),
}

/// 一趟读的结局，由 tokio 那条线写、UI 线程取。
type Arrival = (String, Result<Got, String>);

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
    wire: serde_json::Value,
    /// 那一份按文本读还是按图读。
    how: Want,
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
    pub size: u64,
    /// 修改时间的短写法（列表那一行上后端写好的那一格）。
    pub mtime_text: Option<String>,
    pub edit: bool,
    pub download: bool,
    /// 计划反查（稿 06）：这一份归的那一格（`(片名, 那一格)`）；目录不在任何一片里 · 没人声明 ⇒ `None`。
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
    Download,
    /// 「归属」那一块上的［在计划里看］。
    OpenInPlan,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            key: None,
            want: None,
            wire: serde_json::Value::Null,
            how: Want::Text,
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

/// 预览头「归属」那一块（稿 06 第 1 张）：左边一道状态色（做完绿 · 其余灰）；状态图标 ＋ 标题 ·［在计划里看］；
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
                    Ok(Got::Text(Some(text))) => {
                        let lang = code_lang(&path);
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
                    Ok(Got::Image(img)) => match &ctx {
                        Some(c) => View::Image {
                            size: img.size,
                            tex: c.load_texture(
                                "filewin-preview-image",
                                img,
                                egui::TextureOptions::LINEAR,
                            ),
                            path,
                        },
                        None => View::Said(NO_LINE.to_string()),
                    },
                    Ok(Got::Text(None)) => View::Said(copy_text(
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
        // ② 要的是谁：选中的那一项换了才重新判（按名字，O(1)；找那一行的元数据是 O(n)，只在换了的那一帧做一次）。
        let key = (pane.cwd.clone(), pane.picked_name());
        if self.key.as_ref() != Some(&key) {
            self.key = Some(key.clone());
            self.decide(pane, key.1);
        }
        // ③ 发：要一份、还没摆出来、也没有一趟在飞 ⇒ 发。
        if let (Some(p), View::Loading(_), false) = (self.want.clone(), &self.view, self.inflight) {
            self.fire(pane, p, ctx);
        }
    }

    /// 选中的那一项 → 要不要读、不读的话说什么。
    fn decide(&mut self, pane: &FileWindow, picked: Result<String, usize>) {
        self.want = None;
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
                size: r.size,
                mtime_text: r.mtime_text.clone(),
                edit: can.contains(&super::select::Action::Edit),
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
        if let Some(ext) = super::kind::ext_of(&r.name)
            .filter(|_| super::kind::kind_of(&r) == super::kind::Kind::Image)
        {
            if !DECODABLE.contains(&ext.as_str()) && ext != "svg" {
                self.view = View::Idle(copy_text(
                    "rsFilewinPreview.decide.imageKind",
                    &[("name", &name.to_string())],
                ));
                return;
            }
            if DECODABLE.contains(&ext.as_str()) {
                if r.size > IMAGE_MAX_BYTES {
                    self.view = View::Idle(copy_text(
                        "rsFilewinPreview.decide.imageTooBig",
                        &[
                            ("name", &name.to_string()),
                            ("size", &super::rows::human_size(r.size)),
                            ("limit", &super::rows::human_size(IMAGE_MAX_BYTES)),
                        ],
                    ));
                    return;
                }
                self.how = Want::Image;
                self.wire = pane.row_path(&r).wire();
                self.want = Some(r.path.clone());
                self.view = View::Loading(r.path.clone());
                return;
            }
        }
        if matches!(
            super::kind::kind_of(&r),
            super::kind::Kind::Archive | super::kind::Kind::Pdf
        ) {
            self.view = View::Idle(copy_text(
                "rsFilewinPreview.decide.binaryKind",
                &[("name", &name.to_string())],
            ));
            return;
        }
        if r.size > PREVIEW_MAX_BYTES {
            self.view = View::Idle(copy_text(
                "rsFilewinPreview.decide.tooBig",
                &[
                    ("name", &name.to_string()),
                    ("size", &(super::rows::human_size(r.size)).to_string()),
                    (
                        "limit",
                        &(super::rows::human_size(PREVIEW_MAX_BYTES)).to_string(),
                    ),
                ],
            ));
            return;
        }
        self.how = Want::Text;
        self.wire = pane.row_path(&r).wire();
        self.want = Some(r.path.clone());
        self.view = View::Loading(r.path.clone());
    }

    /// 发一趟 `files-read-text`。没运行时 / 没通道 ⇒ 出声，不发。
    fn fire(&mut self, pane: &FileWindow, path: String, ctx: Option<egui::Context>) {
        let (Some(h), Some(line)) = (pane.rt.clone(), pane.line.clone()) else {
            self.view = View::Said(NO_LINE.to_string());
            self.want = None;
            return;
        };
        let origin = pane.source.origin();
        let slot = self.slot.clone();
        self.inflight = true;
        self.fired += 1;
        let how = self.how;
        let wire = self.wire.clone();
        h.spawn(async move {
            let r = match how {
                Want::Text => {
                    let args = serde_json::json!({ "path": wire, "max_bytes": PREVIEW_MAX_BYTES });
                    super::editor::text_from_reply(
                        super::source::ask_coded(
                            &line,
                            &origin,
                            super::editor::CMD_READ_TEXT,
                            &args,
                            super::editor::READ_BUDGET,
                        )
                        .await,
                    )
                    .map(Got::Text)
                    .map_err(|f| f.said)
                }
                Want::Image => read_image(&line, &origin, &wire).await.map(Got::Image),
            };
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
                    ui.label(
                        egui::RichText::new(super::kind::icon(super::kind::kind_of_name(
                            &h.name, false, false,
                        )))
                        .color(p.text2),
                    );
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
                let mut meta = super::rows::human_size(h.size);
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

/// 认得的代码 / 标记文本 ⇒ 按哪种语法上色（扩展名，交给语法库认）；认不得 ⇒ `None`。
pub fn code_lang(path: &str) -> Option<String> {
    let name = super::source::remote_basename(path);
    let ext = super::kind::ext_of(name)?;
    let k = super::kind::kind_of(&super::source::Listed::plain(super::source::Row {
        name: name.to_string(),
        path: path.to_string(),
        is_dir: false,
        size: 0,
        lossy_name: false,
    }));
    (k == super::kind::Kind::Code || ext == "md" || ext == "svg").then_some(ext)
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

/// 逐块读回一张图（每块到后端那一条的上限），整份超过 [`IMAGE_MAX_BYTES`] 就停、出声；读完在这条线上解码、过大就缩。
async fn read_image(
    line: &super::source::Line,
    origin: &super::source::Origin,
    path: &serde_json::Value,
) -> Result<egui::ColorImage, String> {
    let mut bytes: Vec<u8> = Vec::new();
    loop {
        let d = super::source::ask(
            line,
            origin,
            CMD_READ_CHUNK,
            &serde_json::json!({ "path": path, "offset": bytes.len(), "len": super::lossy_pull::PULL_CHUNK }),
            super::chunk_upload::CHUNK_BUDGET,
        )
        .await?;
        let body = d["content"]["b16"]
            .as_str()
            .and_then(super::lossy_pull::unhex)
            .ok_or_else(|| copy_text("rsFilewinLossyPull.reply.badChunk", &[]))?;
        bytes.extend_from_slice(&body);
        if bytes.len() as u64 > IMAGE_MAX_BYTES {
            return Err(copy_text(
                "rsFilewinPreview.image.grew",
                &[("limit", &super::rows::human_size(IMAGE_MAX_BYTES))],
            ));
        }
        if d["eof"].as_bool() != Some(false) || body.is_empty() {
            break;
        }
    }
    decode_image(&bytes)
}

/// 解不开：句子只说解不开，解码器的原话记一行日志。
fn undecodable(e: impl std::fmt::Display) -> String {
    tracing::warn!("filewin: image undecodable: {e}");
    copy_text("rsFilewinPreview.image.undecodable", &[])
}

/// 字节 → 一张界面能画的图（过大先缩到最长边 [`IMAGE_MAX_SIDE`]）。解不开 ⇒ 一句（原话进日志）。
pub fn decode_image(bytes: &[u8]) -> Result<egui::ColorImage, String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(undecodable)?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().map_err(undecodable)?;
    let img = if img.width().max(img.height()) > IMAGE_MAX_SIDE {
        img.thumbnail(IMAGE_MAX_SIDE, IMAGE_MAX_SIDE)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        size,
        rgba.as_raw(),
    ))
}

/// 按字节寻址分块读回（读族；图片预览用，与有损名下载同一条）。
pub const CMD_READ_CHUNK: &str = "files-read-chunk";

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/preview_tests.rs"]
mod tests;
