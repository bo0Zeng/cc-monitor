//! **文件管理器按内容搜** —— 在窗口上的落点（客户端侧的命令面与消费面）。
//!
//! 后端那一半住 `src/backend/files/grep.rs`（帧命令 `files-grep`：那台后端走一遍、有字节与命中数上界、可撤、不跟链接）。
//! 这里只做三件：发那一问（根 = 窗口现在在看的那个目录，同文件名搜索那一条取舍）· 收成品（严格收，路径 / 那一段走原始字节）·
//! 画出来（一句总述 ＋ 命中那一摞，点一条跳到那份文件所在的目录并高亮它）。
//!
//! # 与文件名搜索（`super::find`）刻意不同的三处
//!
//! - **按回车 / 按钮才发**，不是打一个字发一趟：按内容搜要把整棵树读一遍，比查常驻索引贵几个数量级。
//! - **可撤**：「停」那颗按钮拨这一趟的撤单手柄（[`GrepBoard::stop`]）⇒ 通道当场回收、撤单转给那台后端，后端那一趟随即收手。
//! - **命中点得开**：点一条 ⇒ 进它所在的目录、高亮那个名字（[`GrepTally::jump`] 交出去的是**命中这一摞**的下标，
//!   类型上与目录列表那一摞的下标分开 —— 同 `rows::HitTally` 那条理由）。
//!
//! # 一个字节都不写
//!
//! 只发命令、读应答、画字；没有写动词、没有定时器（撤单是用户按的，不是一个节拍）。

use crate::Held;
use copy_core::copy_text;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

use super::source::{Line, Origin};

/// 按内容搜的线上名。
pub const CMD_GREP: &str = "files-grep";

/// 一趟的往返上限：那台后端走一整棵树、至多读 256 MiB（后端的上界）；给足冷缓存的余量。
/// ⚠ 上界是后端那几个数（字节 · 命中份数），这一侧不持一份；这个期限只管「这一趟最多等多久」。
const CALL_TIMEOUT_SECS: u64 = 120;

/// 一份命中的文件（原始字节：只在画的那一刻有损地转成人话）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GrepHit {
    pub path: Vec<u8>,
    /// 相对搜索起点的那一段（后端算的）。
    pub rel: Vec<u8>,
    pub line: u64,
    pub text: Vec<u8>,
    pub matches: u64,
    /// 前几处命中（后端给的那几处；其余只数进 `matches`）。
    pub lines: Vec<GrepLine>,
}

/// 一处命中：行号 · 那一行的一截 · 那一截里命中的字节区间。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GrepLine {
    pub line: u64,
    pub text: Vec<u8>,
    pub marks: Vec<(usize, usize)>,
}

impl GrepHit {
    /// 画到屏幕上的路径（相对起点；后端没给 ⇒ 全路径）。有损。
    pub fn display(&self) -> String {
        let p = if self.rel.is_empty() {
            &self.path
        } else {
            &self.rel
        };
        String::from_utf8_lossy(p).to_string()
    }
}

/// 结果表上的一行：文件一行（相对路径 ＋「n 处」）、下面每处一行（行号 ＋ 正文）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GrepRow {
    File {
        hit: usize,
        path: String,
        n: u64,
    },
    Line {
        hit: usize,
        line: u64,
        text: String,
        marks: Vec<(usize, usize)>,
    },
}

/// 一趟的命中 ⇒ 按文件分组的行。
pub fn grep_rows(o: &GrepOutcome) -> Vec<GrepRow> {
    let mut out = Vec::new();
    for (i, h) in o.hits.iter().enumerate() {
        out.push(GrepRow::File {
            hit: i,
            path: h.display(),
            n: h.matches,
        });
        for l in &h.lines {
            let ok = std::str::from_utf8(&l.text).is_ok();
            out.push(GrepRow::Line {
                hit: i,
                line: l.line,
                text: String::from_utf8_lossy(&l.text).to_string(),
                marks: if ok { l.marks.clone() } else { Vec::new() },
            });
        }
    }
    out
}

/// 一趟 `files-grep` 的成品。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GrepOutcome {
    pub hits: Vec<GrepHit>,
    pub files: u64,
    pub bytes: u64,
    pub links: u64,
    pub skipped_binary: u64,
    pub skipped_large: u64,
    pub skipped_mounts: u64,
    pub unreadable: u64,
    pub truncated: bool,
    /// `"hits"` / `"bytes"` / 走完了（`None`）。
    pub stopped: Option<String>,
}

fn bad() -> String {
    copy_text("rsFilewinSource.said.badReply", &[])
}

fn u(d: &Value, k: &str) -> Result<u64, String> {
    d.get(k).and_then(Value::as_u64).ok_or_else(bad)
}

/// `files-grep` 的 `data` ⇒ [`GrepOutcome`]。严格收：缺一格 / 类型不对 ⇒ 整份读不懂（不替后端补值）。
pub fn decode_grep(d: &Value) -> Result<GrepOutcome, String> {
    let arr = d.get("hits").and_then(Value::as_array).ok_or_else(bad)?;
    let mut hits = Vec::with_capacity(arr.len());
    for h in arr {
        let line = u(h, "line")?;
        let text = h
            .get("text")
            .and_then(super::find::decode_path)
            .ok_or_else(bad)?;
        let mut lines = Vec::new();
        match h.get("lines") {
            None => lines.push(GrepLine {
                line,
                text: text.clone(),
                marks: Vec::new(),
            }),
            Some(Value::Array(ls)) => {
                for l in ls {
                    lines.push(GrepLine {
                        line: u(l, "line")?,
                        text: l
                            .get("text")
                            .and_then(super::find::decode_path)
                            .ok_or_else(bad)?,
                        marks: l
                            .get("marks")
                            .and_then(Value::as_array)
                            .map(|a| {
                                a.iter()
                                    .filter_map(|m| {
                                        Some((
                                            m.get(0)?.as_u64()? as usize,
                                            m.get(1)?.as_u64()? as usize,
                                        ))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    });
                }
            }
            Some(_) => return Err(bad()),
        }
        hits.push(GrepHit {
            path: h
                .get("path")
                .and_then(super::find::decode_path)
                .ok_or_else(bad)?,
            rel: h
                .get("rel")
                .and_then(super::find::decode_path)
                .unwrap_or_default(),
            line,
            text,
            matches: u(h, "matches")?,
            lines,
        });
    }
    let stopped = match d.get("stopped") {
        Some(Value::Null) => None,
        Some(Value::String(s)) if s == "hits" || s == "bytes" => Some(s.clone()),
        _ => return Err(bad()),
    };
    Ok(GrepOutcome {
        hits,
        files: u(d, "files")?,
        bytes: u(d, "bytes")?,
        links: u(d, "links")?,
        skipped_binary: u(d, "skipped_binary")?,
        skipped_large: u(d, "skipped_large")?,
        skipped_mounts: u(d, "skipped_mounts")?,
        unreadable: u(d, "unreadable")?,
        truncated: d
            .get("truncated")
            .and_then(Value::as_bool)
            .ok_or_else(bad)?,
        stopped,
    })
}

/// 那一问的入参：根 ＋ 要找的那一串（原样，不改大小写）。
pub fn grep_args(root: &super::source::RemotePath, needle: &str) -> Value {
    serde_json::json!({ "path": root.wire(), "needle": needle })
}

/// 结果顶上那一行：「41 处 · 18 个文件」。
pub fn summary_line(o: &GrepOutcome) -> String {
    let m: u64 = o.hits.iter().map(|h| h.matches).sum();
    copy_text(
        "rsFilewinGrep.summary.line",
        &[("m", &m.to_string()), ("n", &o.hits.len().to_string())],
    )
}

/// 跳过了几份（不是文本 · 太大 · 另一个盘 · 读不了；链接另记）：「跳过 12（过大 / 非文本）」；一份都没跳过 ⇒ `None`。
pub fn skipped_line(o: &GrepOutcome) -> Option<String> {
    let k = o.skipped_binary + o.skipped_large + o.skipped_mounts + o.unreadable + o.links;
    (k > 0).then(|| copy_text("rsFilewinGrep.summary.skipped", &[("k", &k.to_string())]))
}

/// 「详情」里分行：不是文本 · 太大 · 在另一个盘上 · 读不了 · 链接（为 0 的不列）。
pub fn skipped_detail(o: &GrepOutcome) -> Vec<String> {
    let mut out = Vec::new();
    for (n, line) in [
        (
            o.skipped_binary,
            copy_text(
                "rsFilewinGrep.detail.binary",
                &[("n", &o.skipped_binary.to_string())],
            ),
        ),
        (
            o.skipped_large,
            copy_text(
                "rsFilewinGrep.detail.large",
                &[("n", &o.skipped_large.to_string())],
            ),
        ),
        (
            o.skipped_mounts,
            copy_text(
                "rsFilewinGrep.detail.mounts",
                &[("n", &o.skipped_mounts.to_string())],
            ),
        ),
        (
            o.unreadable,
            copy_text(
                "rsFilewinGrep.detail.unreadable",
                &[("n", &o.unreadable.to_string())],
            ),
        ),
        (
            o.links,
            copy_text("rsFilewinGrep.detail.links", &[("n", &o.links.to_string())]),
        ),
    ] {
        if n > 0 {
            out.push(line);
        }
    }
    out
}

/// 停在哪一道上界（走完了 ⇒ `None`）。
pub fn stopped_line(o: &GrepOutcome) -> Option<String> {
    match o.stopped.as_deref() {
        Some("hits") => Some(copy_text(
            "rsFilewinGrep.summary.stoppedHits",
            &[("n", &o.hits.len().to_string())],
        )),
        Some("bytes") => Some(copy_text("rsFilewinGrep.summary.stoppedBytes", &[])),
        _ => None,
    }
}

/// 界面这一刻该画什么。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GrepShown {
    /// 这一份是给哪一串的答案。
    pub needle: String,
    pub outcome: Option<GrepOutcome>,
    pub notice: Option<String>,
}

/// 按内容搜的**共享落点**（UI 线程读、tokio 那条写；形状同 `super::find::SearchBoard`）。
///
/// `epoch`：换了一串 / 又按了一次 ⇒ 号 +1，晚到的旧答案整份丢掉。
/// `cancel`：这一趟的撤单手柄（「停」拨它；新开一趟先把上一趟的拨掉）。
#[derive(Clone, Default)]
pub struct GrepBoard {
    inner: Arc<Mutex<GrepShown>>,
    epoch: Arc<AtomicU64>,
    inflight: Arc<AtomicU64>,
    rounds: Arc<AtomicU64>,
    cancel: Arc<Mutex<Option<comms_inward::chan::wire::CancelToken>>>,
    ctx: Arc<Mutex<Option<egui::Context>>>,
}

impl GrepBoard {
    pub fn attach(&self, ctx: Option<egui::Context>) {
        if ctx.is_some() {
            *self.ctx.held() = ctx;
        }
    }

    fn poke(&self) {
        if let Some(c) = self.ctx.held().as_ref() {
            c.request_repaint();
        }
    }

    /// 开一趟：上一趟还在飞就先撤掉它；回这一趟的号与撤单手柄。
    pub fn start(&self, needle: &str) -> (u64, comms_inward::chan::wire::CancelToken) {
        let token = comms_inward::chan::wire::CancelToken::new();
        if let Some(old) = self.cancel.held().replace(token.clone()) {
            old.cancel();
        }
        let mine = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        self.inflight.fetch_add(1, Ordering::SeqCst);
        let mut s = self.inner.held();
        s.needle = needle.to_string();
        s.outcome = None;
        s.notice = None;
        (mine, token)
    }

    /// 「停」：拨这一趟的撤单手柄（在飞那一问当场回收，撤单转给那台后端）。没有在飞的 ⇒ 什么都不做。
    pub fn stop(&self) -> bool {
        match self.cancel.held().take() {
            Some(t) => {
                t.cancel();
                true
            }
            None => false,
        }
    }

    pub fn is_running(&self) -> bool {
        self.inflight.load(Ordering::SeqCst) > 0
    }

    pub fn rounds(&self) -> u64 {
        self.rounds.load(Ordering::SeqCst)
    }

    pub fn shown(&self) -> GrepShown {
        self.inner.held().clone()
    }

    /// 摆一句话（不经网络的那几档失败）。
    pub fn say(&self, notice: &str) {
        self.inner.held().notice = Some(notice.to_string());
        self.rounds.fetch_add(1, Ordering::SeqCst);
        self.poke();
    }

    /// 落一趟的结果 —— 号对不上就丢掉。回值 = 真的落了。
    pub fn store_if_current(&self, mine: u64, r: Result<GrepOutcome, String>) -> bool {
        self.inflight.fetch_sub(1, Ordering::SeqCst);
        if self.epoch.load(Ordering::SeqCst) != mine {
            return false;
        }
        {
            let mut s = self.inner.held();
            match r {
                Ok(o) => s.outcome = Some(o),
                Err(e) => s.notice = Some(e),
            }
        }
        *self.cancel.held() = None;
        self.rounds.fetch_add(1, Ordering::SeqCst);
        self.poke();
        true
    }

    /// 结果顶上那一条状态行：在搜「搜索中…」＋「停」；搜完「41 处 · 18 个文件 · 跳过 12（…）［详情］」；
    /// 出了事 ⇒ 出错条 ＋「重试」。回 `true` ＝ 点了「重试」。
    pub fn status_ui(&self, ui: &mut egui::Ui) -> bool {
        let s = self.shown();
        let p = super::theme::palette(ui.ctx());
        if let (Some(n), None) = (&s.notice, &s.outcome) {
            if !self.is_running() {
                return super::kit::banner(
                    ui,
                    super::kit::Tone::Error,
                    n,
                    &[copy_text("rsFilewinFind.action.retry", &[])],
                )
                .is_some();
            }
        }
        let mut stop = false;
        super::kit::strip(ui, |ui| {
            if self.is_running() {
                ui.spinner();
                ui.label(
                    egui::RichText::new(copy_text("rsFilewinGrep.summary.running", &[]))
                        .color(p.text2),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(copy_text("rsFilewinGrep.action.stop", &[]))
                        .clicked()
                    {
                        stop = true;
                    }
                });
            } else if let Some(o) = &s.outcome {
                ui.label(egui::RichText::new(summary_line(o)).color(p.text2));
                if let Some(k) = skipped_line(o) {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(k).color(p.text2));
                    let lines = skipped_detail(o);
                    super::kit::menu(ui, copy_text("rsFilewinGrep.action.detail", &[]), |ui| {
                        for l in lines {
                            ui.label(l);
                        }
                    });
                }
                if let Some(t) = stopped_line(o) {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(t).color(p.warn));
                    });
                }
            }
        });
        if stop {
            self.stop();
        }
        false
    }
}

/// 跑一趟并把结果落进板子。**窗口那一侧 `spawn` 的就是它。**
pub async fn run_grep(
    board: GrepBoard,
    line: Line,
    origin: Origin,
    root: super::source::RemotePath,
    needle: String,
    mine: u64,
    cancel: comms_inward::chan::wire::CancelToken,
) {
    let r = match super::source::ask_coded_cancellable(
        &line,
        &origin,
        CMD_GREP,
        &grep_args(&root, &needle),
        Duration::from_secs(CALL_TIMEOUT_SECS),
        cancel,
    )
    .await
    {
        Ok(v) => decode_grep(&v),
        Err(f) => Err(f.said),
    };
    board.store_if_current(mine, r);
}

/// 命中那一摞这一帧交出去的东西：物化了几行 · 被打开了哪一处（**命中这一摞**的下标与行号，不是目录列表的）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GrepTally {
    pub rows_materialized: usize,
    pub total_rows: usize,
    /// 打开的那一份（命中下标）＋ 哪一行（点文件那一行 ⇒ 第一处）。
    pub jump: Option<usize>,
    pub jump_line: Option<u64>,
}

/// 画按文件分组的结果（虚拟滚动，同目录列表同一个行高）：文件一行（图标 ＋ 相对路径 ＋「n 处」）、
/// 下面每处一行（行号右对齐 ＋ 那一行正文，命中的字加底色）。点一行 ⇒ [`GrepTally::jump`]。
pub fn show_grep_rows(ui: &mut egui::Ui, rows: &[GrepRow], tally: &mut GrepTally) {
    tally.total_rows = rows.len();
    let p = super::theme::palette(ui.ctx());
    let m = super::theme::metrics(ui.ctx());
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .id_salt("filewin-grep-hits")
            .show_rows(ui, m.row_h, rows.len(), |ui, range| {
                for i in range {
                    tally.rows_materialized += 1;
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), m.row_h),
                        egui::Sense::hover(),
                    );
                    let resp = ui.interact(
                        rect,
                        ui.id().with(("filewin-grep-row", i)),
                        egui::Sense::CLICK,
                    );
                    if resp.hovered() {
                        ui.painter().rect_filled(
                            rect.shrink2(egui::vec2(2.0, 1.0)),
                            m.radius_m,
                            p.hover,
                        );
                    }
                    let painter = ui.painter().clone();
                    let cy = rect.center().y;
                    let font = egui::TextStyle::Body.resolve(ui.style());
                    let mono = egui::TextStyle::Monospace.resolve(ui.style());
                    match &rows[i] {
                        GrepRow::File { hit, path, n } => {
                            let ic = painter.layout_no_wrap(
                                egui_phosphor::regular::FILE_TEXT.to_string(),
                                font.clone(),
                                p.text2,
                            );
                            painter.galley(
                                egui::pos2(rect.left() + 10.0, cy - ic.size().y / 2.0),
                                ic,
                                p.text2,
                            );
                            let g = painter.layout_no_wrap(path.clone(), font.clone(), p.text);
                            let gw = g.size().x;
                            painter.with_clip_rect(rect).galley(
                                egui::pos2(rect.left() + 34.0, cy - g.size().y / 2.0),
                                g,
                                p.text,
                            );
                            let c = painter.layout_no_wrap(
                                copy_text("rsFilewinGrep.hit.count", &[("n", &n.to_string())]),
                                font.clone(),
                                p.text2,
                            );
                            painter.galley(
                                egui::pos2(rect.left() + 44.0 + gw, cy - c.size().y / 2.0),
                                c,
                                p.text2,
                            );
                            if resp.clicked() {
                                tally.jump = Some(*hit);
                                tally.jump_line = None;
                            }
                        }
                        GrepRow::Line {
                            hit,
                            line,
                            text,
                            marks,
                        } => {
                            let no =
                                painter.layout_no_wrap(line.to_string(), mono.clone(), p.faint);
                            painter.galley(
                                egui::pos2(
                                    rect.left() + 80.0 - no.size().x,
                                    cy - no.size().y / 2.0,
                                ),
                                no,
                                p.faint,
                            );
                            let mut job = super::kit::marked(ui, text, marks, p.text2);
                            for sec in &mut job.sections {
                                sec.format.font_id = mono.clone();
                            }
                            job.wrap = egui::text::TextWrapping {
                                max_width: (rect.width() - 100.0).max(0.0),
                                max_rows: 1,
                                break_anywhere: true,
                                overflow_character: Some('…'),
                            };
                            let g = painter.layout_job(job);
                            painter.galley(
                                egui::pos2(rect.left() + 92.0, cy - g.size().y / 2.0),
                                g,
                                p.text2,
                            );
                            if resp.clicked() {
                                tally.jump = Some(*hit);
                                tally.jump_line = Some(*line);
                            }
                        }
                    }
                }
            });
    });
}

/// 点了第 `i` 条命中 ⇒ `(它所在的目录, 它的名字)`；下标越界 ⇒ `None`。
pub fn jump_target(o: &GrepOutcome, i: usize) -> Option<(super::source::RemotePath, String)> {
    let hit = o.hits.get(i)?;
    let at = super::source::RemotePath::from_bytes(&hit.path);
    let name = hit.path.rsplit(|b| *b == b'/').next().unwrap_or(&hit.path);
    Some((at.parent(), String::from_utf8_lossy(name).to_string()))
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/grep_tests.rs"]
mod tests;
