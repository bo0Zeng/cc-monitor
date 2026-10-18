//! **系统的选文件框 / 存盘框**：命令栏「上传」直接弹选文件框（可多选）、「下载」直接弹存盘框（建议名按本机平台改合法、
//! 缺省落在系统的「下载」文件夹、同名由系统框自己问）。在 tokio 那条线程上起操作系统自己的框
//! （`rfd`：Linux 走 GTK3、Windows 走 IFileDialog、macOS 走 NSOpenPanel），结局由 `shell.rs` 的 `settle_pick_with` 接上。
//! 设计住 §2.13。
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
    /// 下载多项：选一个本机目录（缺省落在系统的「下载」文件夹）。
    Folder,
}

/// 这一趟为谁选（结局落回哪一问）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Upload,
    Download,
    /// 下载多项（选目录）。
    DownloadMany,
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
                PickKind::SaveFile { suggested_name } => {
                    let d = downloads_dir().map_or_else(rfd::AsyncFileDialog::new, |d| {
                        rfd::AsyncFileDialog::new().set_directory(d)
                    });
                    match save_type(&suggested_name, crate::platform::WINDOWS_NAMES) {
                        Some(ext) => d.add_filter(format!("*.{ext}"), &[ext.as_str()]),
                        None => d,
                    }
                    .set_file_name(suggested_name)
                    .save_file()
                    .await
                    .map(|h| vec![h.path().to_path_buf()])
                }
                PickKind::Folder => downloads_dir()
                    .map_or_else(rfd::AsyncFileDialog::new, |d| {
                        rfd::AsyncFileDialog::new().set_directory(d)
                    })
                    .pick_folder()
                    .await
                    .map(|h| vec![h.path().to_path_buf()]),
            }
        })
    }
}

/// 存盘框带上的那一类（缺省扩展名）：`windows` ＝ 本机是 Windows ⇒ 建议名的扩展名（没有 ⇒ `None`）；别的平台 ⇒ `None`。
/// Windows 的存盘框照「隐藏已知文件类型的扩展名」把 `n1.txt` 显示成 `n1`；保存类型只有「全部」时框里看不出扩展名、
/// 人改了名也不补。带上这一类 ⇒ 保存类型写着 `*.txt`、缺省扩展名是它（改成没有扩展名的名字照样补上）。别的平台框里照原名显示，不加。
pub fn save_type(suggested: &str, windows: bool) -> Option<String> {
    if !windows {
        return None;
    }
    std::path::Path::new(suggested)
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| !e.is_empty())
        .map(str::to_string)
}

/// 进程里那一份生产选择框。
/// 系统的「下载」文件夹（存盘框缺省落在这儿）：Linux 读 `user-dirs.dirs` 的 `XDG_DOWNLOAD_DIR`（中文环境是「下载」），
/// 别的平台 ⇒ 主目录底下的 `Downloads`；不在盘上 ⇒ `None`（由系统框自己定）。
pub fn downloads_dir() -> Option<PathBuf> {
    let home = PathBuf::from(super::shell::local_home());
    let xdg = std::fs::read_to_string(home.join(".config/user-dirs.dirs"))
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("XDG_DOWNLOAD_DIR="))
                .map(|v| {
                    v.trim_matches('\u{22}')
                        .replace("$HOME", &home.to_string_lossy())
                })
        })
        .map(PathBuf::from);
    xdg.into_iter()
        .chain(std::iter::once(home.join("Downloads")))
        .find(|d| d.is_dir())
}

#[cfg(not(test))]
pub fn native() -> Arc<dyn Picker> {
    Arc::new(NativePicker)
}

/// 测试构建里的缺省：一律答「没选」（要选到东西的判据自己注入一个）。
#[cfg(test)]
pub fn native() -> Arc<dyn Picker> {
    struct NoDialog;
    impl Picker for NoDialog {
        fn pick(&self, _kind: PickKind) -> PickFuture {
            Box::pin(async { None })
        }
    }
    Arc::new(NoDialog)
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

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/picker_tests.rs"]
mod tests;
