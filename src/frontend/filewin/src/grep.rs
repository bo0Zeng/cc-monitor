//! 〔FILES3 · 第四波 · 2026-09-28〕**文件管理器按内容搜** —— `设计/99 §2.2 ㉜` 在窗口上的落点（客户端侧的命令面与消费面）。
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrepHit {
    pub path: Vec<u8>,
    pub line: u64,
    pub text: Vec<u8>,
    pub matches: u64,
}

impl GrepHit {
    /// 画到屏幕上的那一行：`路径:行号  那一段`（命中不止一行时带上「共 N 行」）。
    pub fn display(&self) -> String {
        let path = String::from_utf8_lossy(&self.path);
        let text = String::from_utf8_lossy(&self.text);
        if self.matches > 1 {
            copy_text(
                "rsFilewinGrep.hit.lineMany",
                &[
                    ("path", &path.to_string()),
                    ("line", &self.line.to_string()),
                    ("text", &text.to_string()),
                    ("matches", &self.matches.to_string()),
                ],
            )
        } else {
            copy_text(
                "rsFilewinGrep.hit.line",
                &[
                    ("path", &path.to_string()),
                    ("line", &self.line.to_string()),
                    ("text", &text.to_string()),
                ],
            )
        }
    }
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
    copy_text("rsFilewinGrep.reply.badShape", &[])
}

fn u(d: &Value, k: &str) -> Result<u64, String> {
    d.get(k).and_then(Value::as_u64).ok_or_else(bad)
}

/// `files-grep` 的 `data` ⇒ [`GrepOutcome`]。严格收：缺一格 / 类型不对 ⇒ 整份读不懂（不替后端补值）。
pub fn decode_grep(d: &Value) -> Result<GrepOutcome, String> {
    let arr = d.get("hits").and_then(Value::as_array).ok_or_else(bad)?;
    let mut hits = Vec::with_capacity(arr.len());
    for h in arr {
        hits.push(GrepHit {
            path: h
                .get("path")
                .and_then(super::find::decode_path)
                .ok_or_else(bad)?,
            line: u(h, "line")?,
            text: h
                .get("text")
                .and_then(super::find::decode_path)
                .ok_or_else(bad)?,
            matches: u(h, "matches")?,
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

/// 命中那一摞上面那一行：命中几份 · 读了几份多少字节 · 跳过了哪些 · 停在哪一道上界。**上界到了一定说出来。**
pub fn summary_line(o: &GrepOutcome) -> String {
    let mut t = copy_text(
        "rsFilewinGrep.summary.line",
        &[
            ("hits", &o.hits.len().to_string()),
            ("files", &o.files.to_string()),
            ("bytes", &o.bytes.to_string()),
        ],
    );
    let skipped = o.skipped_binary + o.skipped_large + o.skipped_mounts + o.unreadable;
    if skipped + o.links > 0 {
        t.push_str(&copy_text(
            "rsFilewinGrep.summary.skipped",
            &[
                ("binary", &o.skipped_binary.to_string()),
                ("large", &o.skipped_large.to_string()),
                ("mounts", &o.skipped_mounts.to_string()),
                ("unreadable", &o.unreadable.to_string()),
                ("links", &o.links.to_string()),
            ],
        ));
    }
    match o.stopped.as_deref() {
        Some("hits") => t.push_str(&copy_text("rsFilewinGrep.summary.stoppedHits", &[])),
        Some("bytes") => t.push_str(&copy_text("rsFilewinGrep.summary.stoppedBytes", &[])),
        _ => {}
    }
    t
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
    cancel: Arc<Mutex<Option<chan_core::chan::wire::CancelToken>>>,
    ctx: Arc<Mutex<Option<egui::Context>>>,
}

impl GrepBoard {
    pub fn attach(&self, ctx: Option<egui::Context>) {
        if ctx.is_some() {
            *self.ctx.lock().unwrap() = ctx;
        }
    }

    fn poke(&self) {
        if let Some(c) = self.ctx.lock().unwrap().as_ref() {
            c.request_repaint();
        }
    }

    /// 开一趟：上一趟还在飞就先撤掉它；回这一趟的号与撤单手柄。
    pub fn start(&self, needle: &str) -> (u64, chan_core::chan::wire::CancelToken) {
        let token = chan_core::chan::wire::CancelToken::new();
        if let Some(old) = self.cancel.lock().unwrap().replace(token.clone()) {
            old.cancel();
        }
        let mine = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        self.inflight.fetch_add(1, Ordering::SeqCst);
        let mut s = self.inner.lock().unwrap();
        s.needle = needle.to_string();
        s.outcome = None;
        s.notice = None;
        (mine, token)
    }

    /// 「停」：拨这一趟的撤单手柄（在飞那一问当场回收，撤单转给那台后端）。没有在飞的 ⇒ 什么都不做。
    pub fn stop(&self) -> bool {
        match self.cancel.lock().unwrap().take() {
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
        self.inner.lock().unwrap().clone()
    }

    /// 摆一句话（不经网络的那几档失败）。
    pub fn say(&self, notice: &str) {
        self.inner.lock().unwrap().notice = Some(notice.to_string());
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
            let mut s = self.inner.lock().unwrap();
            match r {
                Ok(o) => s.outcome = Some(o),
                Err(e) => s.notice = Some(e),
            }
        }
        *self.cancel.lock().unwrap() = None;
        self.rounds.fetch_add(1, Ordering::SeqCst);
        self.poke();
        true
    }

    /// 总述那一行 ＋ 出了事那句话。
    pub fn ui(&self, ui: &mut egui::Ui) {
        let s = self.shown();
        if let Some(n) = &s.notice {
            ui.colored_label(egui::Color32::RED, n);
        }
        if let Some(o) = &s.outcome {
            if o.truncated {
                ui.colored_label(egui::Color32::from_rgb(0xE0, 0x9A, 0x20), summary_line(o));
            } else {
                ui.label(summary_line(o));
            }
        }
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
    cancel: chan_core::chan::wire::CancelToken,
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

/// 命中那一摞这一帧交出去的东西：物化了几行 · 被点了哪一条（**命中这一摞**的下标，不是目录列表的）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GrepTally {
    pub rows_materialized: usize,
    pub total_rows: usize,
    pub jump: Option<usize>,
}

/// 画命中那一摞（虚拟滚动，同目录列表同一个行高）：每行一颗可点的链接，点了 ⇒ [`GrepTally::jump`]。
pub fn show_grep_rows(ui: &mut egui::Ui, hits: &[String], tally: &mut GrepTally) {
    tally.total_rows = hits.len();
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .id_salt("filewin-grep-hits")
        .show_rows(ui, super::rows::ROW_HEIGHT, hits.len(), |ui, range| {
            for i in range {
                tally.rows_materialized += 1;
                if ui.link(&hits[i]).clicked() {
                    tally.jump = Some(i);
                }
            }
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
