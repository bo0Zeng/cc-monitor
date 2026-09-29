//! 〔W5-FILES · 第五波 · 2026-09-26〕**原生选文件框** —— `设计/60 §6.2`「原生选文件框 · 上传今天是问一句本机路径」。
//!
//! 上传那一问（`upload.rs`）与下载「存到哪儿」那一问（`download.rs` / `shell.rs::pull_ui`）各加一颗「选择…」：
//! 点了在 tokio 那条线程上起操作系统自己的选择框（`rfd`：Linux 走 GTK3、Windows 走 IFileDialog、macOS 走 NSOpenPanel），
//! 选完把路径**填进那个框**，人再点「确定」—— 判定照旧走那个框自己的判定（框与选择框两个入口不分岔、全程可判）。
//! 设计住 `调研/第四波记录/W5-FILES.md` §2.13。
//!
//! # 依赖
//!
//! `rfd` 与 `tauri-plugin-dialog` 同一个版本、同一组 feature（`gtk3` · `common-controls-v6`）⇒ 锁文件不多一个 crate。
//! 选择框本身经 [`Picker`] 注入：生产是 [`NativePicker`]，判据喂一个假的（真弹框要图形会话）。
//!
//! # 买不到什么
//!
//! - 真图形会话上真弹出来、真点（本机无图形会话）；真 Windows（虚拟机那一路）。
//! - 选择框起不来（没有图形会话 / 门户不在）与「人点了取消」在 `rfd` 那一层分不开 —— 两者都回「没选」，窗口上照实说「没有选到」。

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

/// 要选什么。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickKind {
    /// 上传：选一个或几个本机文件。
    OpenFiles,
    /// 下载：选一个保存位置（带着建议的文件名）。
    SaveFile { suggested_name: String },
}

/// 这一趟为谁选（结局落回哪一问）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Upload,
    Download,
}

pub type PickFuture = Pin<Box<dyn Future<Output = Option<Vec<PathBuf>>> + Send>>;

/// 一个选择框。`None` ＝ 没选（取消 / 起不来）。
pub trait Picker: Send + Sync {
    fn pick(&self, kind: PickKind) -> PickFuture;
}

/// 生产那一份：操作系统自己的选择框（`rfd` 的异步口；GTK 那一支自己起线程）。
pub struct NativePicker;

impl Picker for NativePicker {
    fn pick(&self, kind: PickKind) -> PickFuture {
        Box::pin(async move {
            match kind {
                PickKind::OpenFiles => rfd::AsyncFileDialog::new()
                    .pick_files()
                    .await
                    .map(|v| v.into_iter().map(|h| h.path().to_path_buf()).collect()),
                PickKind::SaveFile { suggested_name } => rfd::AsyncFileDialog::new()
                    .set_file_name(suggested_name)
                    .save_file()
                    .await
                    .map(|h| vec![h.path().to_path_buf()]),
            }
        })
    }
}

/// 进程里那一份生产选择框。
pub fn native() -> Arc<dyn Picker> {
    Arc::new(NativePicker)
}

/// 选完的结局在 tokio 那条线程上落下、UI 线程下一帧取走（形状照别的看板：敲一下窗口）。
#[derive(Clone, Default)]
pub struct PickBoard {
    slot: Arc<Mutex<Option<(Purpose, Option<Vec<PathBuf>>)>>>,
    ctx: Arc<Mutex<Option<egui::Context>>>,
}

impl PickBoard {
    pub fn attach(&self, ctx: Option<egui::Context>) {
        *self.ctx.lock().unwrap() = ctx;
    }

    pub fn deliver(&self, purpose: Purpose, picked: Option<Vec<PathBuf>>) {
        *self.slot.lock().unwrap() = Some((purpose, picked));
        if let Some(c) = self.ctx.lock().unwrap().as_ref() {
            c.request_repaint();
        }
    }

    pub fn take(&self) -> Option<(Purpose, Option<Vec<PathBuf>>)> {
        self.slot.lock().unwrap().take()
    }
}

/// 上传那个框：把选到的路径**接在**框里已有的字后面，一行一个（框里已有的不动、重复的不再加）。
pub fn append_lines(existing: &str, picked: &[PathBuf]) -> String {
    let mut lines: Vec<String> = existing
        .lines()
        .map(str::to_string)
        .filter(|l| !l.trim().is_empty())
        .collect();
    for p in picked {
        let s = p.to_string_lossy().to_string();
        if !lines.contains(&s) {
            lines.push(s);
        }
    }
    lines.join("\n")
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/filewin/picker_tests.rs"]
mod tests;
