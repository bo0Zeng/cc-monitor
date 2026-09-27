//! 〔FW34 · 第四波 2026-09-24〕文件窗口的**书签** —— 老面板 7 项里剩下的最后一项
//! （用户裁「1. 全补」；`设计/60 §14.4` 那条「书签没人做」）。
//!
//! 设计与被比较过的另几条路住 `调研/第四波记录/FW34.md` 第一、二节；这里只留落地要知道的。
//!
//! # 一、存哪 · 谁写
//!
//! - **一份文件**：`<monitor 数据目录>/filewin-bookmarks.json`，形状 `{ "<origin>": ["/abs/dir", …] }`。
//!   它是 **monitor 自己的状态**（与 `config.json` 同一族），**不是用户文件** ⇒ 不走后端写面。
//!   按机器分的那个键与老面板逐字同一个取法（`RemoteConfig::origin_label`：label 空才退 host）。
//! - **路径由 monitor 定**：开窗那一跳算好、放进开窗种子（[`super::proc::OpenRequest`]）交过来，
//!   窗口进程自己不找数据目录 ⇒ 判据可以把它指到临时目录，真数据目录一个字节都不碰。
//! - **写者可以是几个窗口进程、一个函数**：每个窗口（★ / ×）都走
//!   [`mutate`]：**上锁 → 现读 → 改 → 原子换**。
//!   - 锁：旁件 `<文件>.lock` 上的独占锁。一窗一进程 ⇒ 两个窗口可以同时开着同一台机器，
//!     各自读-改-写会**静默丢掉**另一个刚加的那一条（判据 `two_writers_lose_nothing` 钉着）。
//!   - 原子换：[`crate::utils::atomic_write_json`]（写旁名再换名）⇒ 读的一方永远读到完整的一份。
//! - 文件读不懂（被人手改坏了）⇒ **不覆盖**，原话报出来：覆盖掉就是把别的机器的书签一起清了。
//!
//! # 二、为什么它是这个窗口里唯一落盘的东西
//!
//! 用户裁「窗口生命周期就是销毁」：标签页、双栏、预览都是窗口状态，关窗即没。
//! 书签不是窗口状态，是用户**存下来的地方**（老面板就跨重启留着）。
//!
//! # ⚠ 买不到什么
//!
//! - 别的窗口刚加的书签**不会立刻**出现在这个窗口上：本窗口在 ★ / × 那一下与每次换目录时现读
//!   （小文件一次读，不是每帧；不上定时器、不 watch）。
//! - 真 Windows 上 `LockFileEx` 那一支没跑过（本机只量了 Linux 的 `flock`）。

use crate::copy_table::copy_text;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::source::Origin;

/// 书签文件的名字（住 monitor 数据目录下）。**唯一住址**。
pub const FILE_NAME: &str = "filewin-bookmarks.json";

/// 整份书签：机器 → 那台机器上的目录（保序、不重样）。
pub type Book = BTreeMap<String, Vec<String>>;

/// 书签文件在 `dir` 下的全路径。
pub fn file_in(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

/// 锁旁件的路径（与书签文件同目录，名字后面接 `.lock`）。
fn lock_path(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_os_string();
    name.push(".lock");
    PathBuf::from(name)
}

/// 目录路径归一 —— 与老面板 `src/sftp/paths.ts::normalize` **逐字同一个算法**：
/// 连续的 `/` 折成一个；只剩 `/` 就是根；否则去掉尾巴上那一个 `/`。
///
/// ⚠ 空串照旧是空串、相对路径照旧是相对的（老面板也不补前导 `/`）；
///   空串由调用方（[`toggle_in`]）挡掉，不在这里编一个路径出来。
pub fn normalize_dir(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut prev_slash = false;
    for c in path.chars() {
        if c == '/' {
            if !prev_slash {
                out.push(c);
            }
            prev_slash = true;
        } else {
            out.push(c);
            prev_slash = false;
        }
    }
    if out == "/" {
        return out;
    }
    if out.ends_with('/') {
        out.pop();
    }
    out
}

/// 在 `list` 里切换 `dir`：在就删、不在就加到最后。回值 ＝ 切完之后它在不在。
///
/// 🔴 比较走归一之后的形（老面板那一下是「原样或去尾 `/` 在就算在」，归一之后是同一件事）。
/// 空路径不加（回 `false`，`list` 不动）。
pub fn toggle_in(list: &mut Vec<String>, dir: &str) -> bool {
    let d = normalize_dir(dir);
    if d.is_empty() {
        return false;
    }
    if let Some(at) = list.iter().position(|x| normalize_dir(x) == d) {
        list.remove(at);
        false
    } else {
        list.push(d);
        true
    }
}

/// 从 `list` 里删掉 `dir`（按归一之后的形比）。回值 ＝ 真的删了一条。
pub fn remove_in(list: &mut Vec<String>, dir: &str) -> bool {
    let d = normalize_dir(dir);
    let before = list.len();
    list.retain(|x| normalize_dir(x) != d);
    list.len() != before
}

/// 读整份。文件不在 ⇒ 空的一份（还没存过书签）；读得到却读不懂 ⇒ `Err`（**不当成空的**）。
pub fn read_book(file: &Path) -> Result<Book, String> {
    let raw = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Book::new()),
        Err(e) => {
            return Err(copy_text(
                "rsFilewinBookmarks.read.failed",
                &[("e", &e.to_string())],
            ))
        }
    };
    serde_json::from_str(&raw).map_err(|e| {
        copy_text(
            "rsFilewinBookmarks.read.unreadable",
            &[
                ("file", &(file.display()).to_string()),
                ("e", &e.to_string()),
            ],
        )
    })
}

/// 拿锁。锁旁件没有就建一个（空文件，只拿来上锁）；锁随回值那个句柄一起放。
fn lock_store(file: &Path) -> Result<std::fs::File, String> {
    let lock = lock_path(file);
    if let Some(dir) = lock.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| copy_text("rsFilewinBookmarks.store.failed", &[("e", &e.to_string())]))?;
    }
    let f = std::fs::File::create(&lock)
        .map_err(|e| copy_text("rsFilewinBookmarks.store.failed", &[("e", &e.to_string())]))?;
    f.lock()
        .map_err(|e| copy_text("rsFilewinBookmarks.store.failed", &[("e", &e.to_string())]))?;
    Ok(f)
}

/// 🔴 **唯一的写口**：上锁 → 现读 → `f` 改 → 原子换。回 `(f 的回值, 改完的整份)`。
///
/// 两个进程（窗口 · monitor）的每一次写都走这里 ⇒ 不丢更新（锁）、读的一方读不到半份（原子换）。
pub fn mutate<R>(file: &Path, f: impl FnOnce(&mut Book) -> R) -> Result<(R, Book), String> {
    let _held = lock_store(file)?;
    let mut book = read_book(file)?;
    let r = f(&mut book);
    book.retain(|_, v| !v.is_empty());
    crate::utils::atomic_write_json(file, &book)
        .map_err(|e| copy_text("rsFilewinBookmarks.mutate.failed", &[("e", &e.to_string())]))?;
    Ok((r, book))
}

/// 窗口里的书签 —— **一个窗口一份**，所有标签页 / 两栏共用（克隆便宜）。
#[derive(Clone)]
pub struct Shelf(Arc<Mutex<ShelfState>>);

struct ShelfState {
    /// 书签文件。`None` ＝ monitor 解不出数据目录（这一形要出声，见 [`NO_DATA_DIR`]）。
    file: Option<PathBuf>,
    /// 哪台机器（书签文件里那一格的键 ＝ 它的线上字面，与老面板那把键的后缀同一个取法）。
    origin: Origin,
    /// 这台机器上的书签（上一次读 / 写之后的样子）。
    list: Vec<String>,
    /// 上一次读 / 写失败时那句话（`None` ＝ 没话说）。
    notice: Option<String>,
}

/// 数据目录解不出来时书签栏上那一句。
pub static NO_DATA_DIR: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinBookmarks.store.noDataDir", &[]));

/// 书签栏上那颗切换按钮：当前目录不在书签里时。
pub static ADD_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinBookmarks.label.add", &[]));
/// 当前目录已经在书签里时。
pub static DROP_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinBookmarks.label.drop", &[]));
/// 每条书签后面那颗「删掉」。
pub static REMOVE_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinBookmarks.label.remove", &[]));

impl Shelf {
    /// 开窗时建：读一次那台机器的书签。
    pub fn open(file: Option<PathBuf>, origin: &Origin) -> Self {
        let s = Shelf(Arc::new(Mutex::new(ShelfState {
            file,
            origin: origin.clone(),
            list: Vec::new(),
            notice: None,
        })));
        s.refresh();
        s
    }

    /// 这台机器上的书签（最近一次读 / 写之后的样子）。
    pub fn list(&self) -> Vec<String> {
        self.0.lock().unwrap().list.clone()
    }

    /// 要画在书签栏上的那句话（`None` ＝ 没话说）。
    pub fn notice(&self) -> Option<String> {
        let s = self.0.lock().unwrap();
        if s.file.is_none() {
            return Some(NO_DATA_DIR.to_string());
        }
        s.notice.clone()
    }

    /// `dir` 在不在书签里（按归一之后的形比）。
    pub fn contains(&self, dir: &str) -> bool {
        let d = normalize_dir(dir);
        self.0
            .lock()
            .unwrap()
            .list
            .iter()
            .any(|x| normalize_dir(x) == d)
    }

    /// 从盘上现读一次（换目录时调；别的窗口刚加的书签从这里进来）。
    pub fn refresh(&self) {
        let mut s = self.0.lock().unwrap();
        let Some(file) = s.file.clone() else {
            return;
        };
        match read_book(&file) {
            Ok(book) => {
                s.list = book
                    .get(s.origin.as_wire_str())
                    .cloned()
                    .unwrap_or_default();
                s.notice = None;
            }
            Err(e) => s.notice = Some(e),
        }
    }

    /// 一次写：走 [`mutate`]（上锁、现读、改、原子换），把这台机器那一格换成盘上最新的样子。
    fn write(&self, f: impl FnOnce(&mut Vec<String>) -> bool) -> bool {
        let mut s = self.0.lock().unwrap();
        let Some(file) = s.file.clone() else {
            return false;
        };
        let key = s.origin.as_wire_str().to_string();
        match mutate(&file, |book| f(book.entry(key.clone()).or_default())) {
            Ok((changed, book)) => {
                s.list = book.get(&key).cloned().unwrap_or_default();
                s.notice = None;
                changed
            }
            Err(e) => {
                s.notice = Some(e);
                false
            }
        }
    }

    /// 切换 `dir`（★）。回值 ＝ 切完之后它在书签里。
    pub fn toggle(&self, dir: &str) -> bool {
        let d = dir.to_string();
        self.write(move |list| toggle_in(list, &d))
    }

    /// 删掉 `dir`（×）。回值 ＝ 真的删了一条。
    pub fn remove(&self, dir: &str) -> bool {
        let d = dir.to_string();
        self.write(move |list| remove_in(list, &d))
    }

    /// 画书签栏：切换按钮 ＋ 一排书签。回值 ＝ 用户点了哪一条（要跳过去的目录）。
    ///
    /// ⚠ 跳转**不在这里做**：这一行画在 `FileWindow::frame_body` 借着 `&mut self` 的那段里，
    ///   跳转收在帧尾（同面包屑那一格的理由）。
    pub fn bar_ui(&self, ui: &mut egui::Ui, cwd: &str) -> Option<String> {
        let mut go = None;
        let mut toggle = false;
        let mut drop: Option<String> = None;
        let here = self.contains(cwd);
        ui.horizontal_wrapped(|ui| {
            if ui
                .small_button(if here {
                    DROP_LABEL.as_str()
                } else {
                    ADD_LABEL.as_str()
                })
                .clicked()
            {
                toggle = true;
            }
            for d in self.list() {
                if ui
                    .small_button(&d)
                    .on_hover_text(&copy_text("rsFilewinBookmarks.bar.jumpHint", &[]))
                    .clicked()
                {
                    go = Some(d.clone());
                }
                if ui
                    .small_button(REMOVE_LABEL.as_str())
                    .on_hover_text(&copy_text("rsFilewinBookmarks.bar.removeHint", &[]))
                    .clicked()
                {
                    drop = Some(d);
                }
            }
        });
        if let Some(n) = self.notice() {
            ui.colored_label(egui::Color32::from_rgb(0xFF, 0xA5, 0x00), n);
        }
        if toggle {
            self.toggle(cwd);
        }
        if let Some(d) = drop {
            self.remove(&d);
        }
        go
    }
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/bookmarks_tests.rs"]
mod tests;
