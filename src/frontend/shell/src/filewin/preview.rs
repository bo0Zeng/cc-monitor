//! 〔FW34 · 第四波 2026-09-24〕**预览**：窗口右侧一块只读面板，显示焦点那一栏里选中的**恰好一个文件**的文本。
//!
//! 设计住 `调研/第四波记录/FW34.md` 第四节；这里只留落地要知道的。
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
//! 行起点在到货时算一次；每帧按 `show_rows` 只排看得见的那几行（与大文件模式同一条纪律）。
//!
//! # ⚠ 买不到什么
//!
//! - 图片 / 二进制预览（后端只有读文本那一条；说「不是文本」）。
//! - 真远端上一趟的时延读数（判据挂的是合成后端）。

use crate::copy_table::copy_text;
use std::sync::{Arc, Mutex};

use super::shell::{FileWindow, NO_LINE};

/// 预览读一份文本的上限（字节）。**唯一住址**；登记在 `byte_cap_registry`。
///
/// 超了：**不读、出声**（「有 N，预览只看 M 以内的文件」），不截断。
pub const PREVIEW_MAX_BYTES: u64 = 64 * 1024;

/// 面板上摆着的是什么。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    /// 没有可读的（没选 / 选了几项 / 目录 / 太大 …），那句话说清为什么。
    Idle(String),
    /// 正在读这一份。
    Loading(String),
    /// 读到了。`starts` ＝ 每一行在 `text` 里的起点（到货时算一次）。
    Text {
        path: String,
        text: String,
        starts: Vec<usize>,
    },
    /// 读了，没读成（不是文本 / 读不到），那句话。
    Said(String),
}

/// 一趟读的结局，由 tokio 那条线写、UI 线程取。
type Arrival = (String, Result<Option<String>, String>);

/// 预览的状态（**UI 线程自己的**，只有到货那一格跨线程）。
pub struct Preview {
    /// 上一帧看的是哪一项（当前目录 ＋ 选中的那个名字 / 选中了几项）。变了才重新判。
    key: Option<(String, Result<String, usize>)>,
    /// 要显示的那一份（`None` ＝ 这一项不读）。
    want: Option<String>,
    view: View,
    /// 有一趟在飞吗（UI 线程记的）。
    inflight: bool,
    /// 发出去过几趟（判据数它，也与线上那本账对拍）。
    fired: u64,
    slot: Arc<Mutex<Option<Arrival>>>,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            key: None,
            want: None,
            view: View::Idle(PICK_ONE.to_string()),
            inflight: false,
            fired: 0,
            slot: Arc::new(Mutex::new(None)),
        }
    }
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
        // ① 收货：是要的那一份才摆出来，不是就丢掉（光标已经挪走了）。
        let arrived = self.slot.lock().unwrap().take();
        if let Some((path, r)) = arrived {
            self.inflight = false;
            if self.want.as_deref() == Some(path.as_str()) {
                self.view = match r {
                    Ok(Some(text)) => View::Text {
                        starts: line_starts(&text),
                        path,
                        text,
                    },
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
        if r.is_dir {
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
        h.spawn(async move {
            let args = serde_json::json!({ "path": path, "max_bytes": PREVIEW_MAX_BYTES });
            let r = super::editor::text_from_reply(
                super::source::ask_coded(
                    &line,
                    &origin,
                    super::editor::CMD_READ_TEXT,
                    &args,
                    super::editor::READ_BUDGET,
                )
                .await,
            );
            *slot.lock().unwrap() = Some((path, r));
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
    }

    /// 画这块面板。
    pub fn ui(&self, ui: &mut egui::Ui) {
        ui.strong(&copy_text("rsFilewinPreview.ui.title", &[]));
        match &self.view {
            View::Idle(s) | View::Said(s) => {
                ui.label(s);
            }
            View::Loading(p) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(copy_text(
                        "rsFilewinPreview.ui.reading",
                        &[("name", &(super::source::remote_basename(p)).to_string())],
                    ));
                });
            }
            View::Text { path, text, starts } => {
                ui.label(path);
                let row_h = ui.text_style_height(&egui::TextStyle::Monospace);
                egui::ScrollArea::both()
                    .id_salt("filewin-preview")
                    .auto_shrink([false; 2])
                    .show_rows(ui, row_h, starts.len(), |ui, range| {
                        for i in range {
                            let end = starts.get(i + 1).map_or(text.len(), |e| e - 1);
                            let line = text[starts[i]..end].trim_end_matches('\r');
                            ui.add(
                                egui::Label::new(egui::RichText::new(line).monospace()).extend(),
                            );
                        }
                    });
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/filewin/preview_tests.rs"]
mod tests;
