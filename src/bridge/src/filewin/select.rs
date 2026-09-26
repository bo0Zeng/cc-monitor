//! 〔FW1+FW2 · 2026-09-24〕文件窗口的**选中态** · **键位** · **右键菜单那张表**。
//!
//! 出处：`设计/99 §4.21.1` 那两行 ——
//! FW1「方向键 · 回车 · Delete · F2 · Ctrl+A · 打字跳转」，
//! FW2「多选（**批量底层已做好**，贵的只有选中态）＋ 右键菜单」。
//!
//! # 一、为什么这三件住一处
//!
//! 三件事问的是**同一个问题**：「现在选中的是哪几行、能对它们做什么」。
//!
//! - **选中态**（[`Selection`]）回答「哪几行」；
//! - **菜单那张表**（[`actions_for`]）回答「能做什么」——
//!   🔴 它是**唯一**的判定：右键菜单画哪几项、键盘那几个键准不准做，问的都是它
//!   （窗口那一侧的执行口是 `shell.rs` 里那一个 `perform`）。
//!   分成两份的症状是具体的：菜单上没有「改名」，按 F2 却改得了 —— 或者反过来。
//! - **键位**（[`intents`]）只把 egui 那一帧的事件**翻**成意图，一个动作都不做。
//!
//! ⇒ 这三样都是**纯的**（不碰窗口、不碰通道、不碰锁），判据直接喂它们；
//! 把它们接到窗口上的那几跳（胶水）住 `shell.rs`，由真跑 `frame_body` 的判据看着。
//!
//! # 二、选中态按**名字**记，不按下标
//!
//! 下标会随「换一种排序」「刚好有人新建了一个文件」整摞移位 ——
//! 按下标记的话，用户选中 `a.txt` 之后换了排序，删掉的会是另一个文件。
//! 同一个目录里名字唯一（`rows::reveal_index` 那条高亮按名字判，同一个理由）。
//!
//! ⚠ 代价如实记：**用名字找下标是 O(n)**。它只发生在**一次按键 / 一次点击**里
//! （方向键要知道光标在第几行、Shift 要知道锚在第几行），**不在每一帧里**：
//! 每一帧只问「这一行被选中了吗」（[`Selection::is_picked`]，一次 `BTreeSet` 查找），
//! 而且只问那几十行真被画出来的（虚拟滚动，`rows.rs` 头注那条纪律）。
//!
//! # 三、批量有「删除」「权限」两条，理由逐条
//!
//! `writeops::run_writes` 吃的是 `Vec<WriteOp>`（「批量底层已做好」说的就是它）：
//! 围栏 → **一次问完** → 串行做。N 件删除正好是它的形状；〔FW5〕N 件改权限也是
//! （框里输**一个**八进制数，出 N 件，确认框逐件列出）。
//! 另外几件**刻意是单选**：
//!
//! | 动作 | 为什么只对一项 |
//! |---|---|
//! | 打开 / 编辑 | 一次只进得了一个目录 · 编辑面一次只摆一份文本 |
//! | 复制 | 「复制为」那个框要**一个**新名字 |
//! | 下载 | 「存到哪儿」那一问按**一份**文件问（`download::Ask::for_row`）|
//! | 改名 | 要**一个**新名字 |
//! | ~~权限~~ | 〔散文墓碑〕〔FW5 · 第四波〕原话「那个框今天按**一行**摆；批量改权限要给那个框换形状，而 `writeops.rs` 不在本路写区 ⇒ **没做**」—— 框换成了 N 行（`writeops::WritePrompt::for_chmod_many`），**已做** |
//!
//! # 四、〔FW5〕选中态的键：有损名按**原始字节**记
//!
//! 两个不同字节的有损名（`a\xff` 与 `a\xfe`）解出来是**同一个**显示串 `a�`。按显示串记选中，
//! 点一个就等于选中两个 —— 删一个删掉两个。⇒ 选中态的键走 [`pick_key`]：无损名 = 名字本身；
//! 有损名 = `\0b16:<十六进制>`（名字里不可能有 NUL，与任何真名字都撞不上）。

use crate::copy_table::copy_text;
use std::collections::BTreeSet;

use super::copy::is_copyable;
use super::download::is_downloadable;
use super::editor::is_editable;
use super::source::Listed;
use super::writeops::is_writable;

/// 〔FW5〕一行在选中态里的**键**（理由住本模块头注 §四）。
///
/// ⚠ 无损名回借用（每帧对每一行真被画出来的行问一次 —— 那几十行不分配）；有损名才拼一个串。
pub fn pick_key(r: &Listed) -> std::borrow::Cow<'_, str> {
    match &r.raw_name {
        Some(b) if r.lossy_name => std::borrow::Cow::Owned(format!(
            "\0b16:{}",
            b.iter().map(|x| format!("{x:02x}")).collect::<String>()
        )),
        _ => std::borrow::Cow::Borrowed(r.name.as_str()),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 选中态
// ═══════════════════════════════════════════════════════════════════════

/// 选中了哪几行 ＋ 键盘光标在哪一行 ＋ Shift 扩选的锚。**都按名字记**（头注 §二）。
///
/// 🔴 光标与选中**刻意是两件事**：Ctrl+点一下把某一行**取消**选中之后，
/// 光标仍在那一行（下一次方向键从那儿走）—— 合成一件的话，那一下之后
/// 光标就不知道去哪了。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    picked: BTreeSet<String>,
    cursor: Option<String>,
    anchor: Option<String>,
}

impl Selection {
    /// 这一行被选中了吗。**每帧对每一行真被画出来的行问一次** —— 只做一次集合查找。
    pub fn is_picked(&self, name: &str) -> bool {
        self.picked.contains(name)
    }

    /// 这一行是不是键盘光标所在那一行。
    pub fn is_cursor(&self, name: &str) -> bool {
        self.cursor.as_deref() == Some(name)
    }

    /// 选中了几项。
    pub fn len(&self) -> usize {
        self.picked.len()
    }

    pub fn is_empty(&self) -> bool {
        self.picked.is_empty()
    }

    /// 光标那一行的名字（`None` = 还没有光标）。
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    /// 选中那几项的名字（字典序；判据与状态行用）。
    pub fn names(&self) -> Vec<String> {
        self.picked.iter().cloned().collect()
    }

    /// 全清（换目录时调：那几个名字在新目录里指的不是同一样东西）。
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// 只清选中、留着光标（一摞写操作跑完之后调：删掉的那几个名字已经不在了，
    /// 而光标留着，下一次方向键还从附近走 —— 光标那个名字若也没了，
    /// [`Self::cursor_index`] 回 `None`，方向键从头走）。
    pub fn clear_picked(&mut self) {
        self.picked.clear();
        self.anchor = None;
    }

    /// 光标在这一摞里的下标（O(n)，只在按键那一刻调）。
    pub fn cursor_index(&self, rows: &[Listed]) -> Option<usize> {
        let c = self.cursor.as_deref()?;
        rows.iter().position(|r| pick_key(r) == c)
    }

    /// 选中那几行的下标，**按列表顺序**（批量那一摞按这个序排，问的时候也按这个序摆）。
    ///
    /// ⚠ O(n)：只在「真要做一件事」那一刻调（按了 Delete / 点了菜单上那一项），
    /// 不在每帧里。已经不在这一摞里的名字（刚被删了 / 改了名）**自然落空**，
    /// 不会凭空造出一件操作。
    pub fn picked_indices(&self, rows: &[Listed]) -> Vec<usize> {
        if self.picked.is_empty() {
            return Vec::new();
        }
        rows.iter()
            .enumerate()
            .filter(|(_, r)| self.picked.contains(pick_key(r).as_ref()))
            .map(|(i, _)| i)
            .collect()
    }

    /// 鼠标点了第 `i` 行。回值 = 这一下改了选中态。
    ///
    /// - 不带修饰键：**只选这一行**；
    /// - Ctrl（macOS 上是 ⌘，egui 的 `command`）：这一行**切换**选中，别的不动；
    /// - Shift：从锚到这一行**整段**选中（替换掉原来的选中），锚不动。
    ///   没有锚（或锚那个名字已经不在了）⇒ 当成不带修饰键。
    pub fn click(&mut self, rows: &[Listed], i: usize, mods: egui::Modifiers) -> bool {
        let Some(r) = rows.get(i) else {
            return false;
        };
        let before = self.clone();
        let name = pick_key(r).into_owned();
        if mods.shift {
            if let Some(a) = self.anchor_index(rows) {
                self.span(rows, a, i);
                self.cursor = Some(name);
                return *self != before;
            }
        }
        if mods.command {
            if !self.picked.remove(&name) {
                self.picked.insert(name.clone());
            }
        } else {
            self.picked.clear();
            self.picked.insert(name.clone());
        }
        self.cursor = Some(name.clone());
        self.anchor = Some(name);
        *self != before
    }

    /// 右键点了第 `i` 行：**它已经在选中里 ⇒ 不动**（菜单对整摞选中说话）；
    /// 不在 ⇒ 改成只选它（菜单对它一个说话）。文件管理器的常规手感。
    pub fn pick_for_menu(&mut self, rows: &[Listed], i: usize) {
        let Some(r) = rows.get(i) else {
            return;
        };
        if !self.picked.contains(pick_key(r).as_ref()) {
            self.click(rows, i, egui::Modifiers::NONE);
        }
    }

    /// Ctrl+A：这一摞全选。光标不动（没有光标 ⇒ 放在第一行）。
    pub fn select_all(&mut self, rows: &[Listed]) {
        self.picked = rows.iter().map(|r| pick_key(r).into_owned()).collect();
        if self.cursor_index(rows).is_none() {
            self.cursor = rows.first().map(|r| pick_key(r).into_owned());
        }
        self.anchor = self.cursor.clone();
    }

    /// 光标挪到第 `i` 行。`extend` = Shift 按着（从锚到这一行整段选中）；
    /// 否则只选这一行、锚跟过来。回值 = 真挪到了一行上（越界 / 空列表 ⇒ `false`）。
    pub fn move_to(&mut self, rows: &[Listed], i: usize, extend: bool) -> bool {
        let Some(r) = rows.get(i) else {
            return false;
        };
        let mods = if extend {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::NONE
        };
        let name = pick_key(r).into_owned();
        self.click(rows, i, mods);
        self.cursor = Some(name);
        true
    }

    fn anchor_index(&self, rows: &[Listed]) -> Option<usize> {
        let a = self.anchor.as_deref()?;
        rows.iter().position(|r| pick_key(r) == a)
    }

    fn span(&mut self, rows: &[Listed], a: usize, b: usize) {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        self.picked = rows[lo..=hi]
            .iter()
            .map(|r| pick_key(r).into_owned())
            .collect();
    }
}

/// 方向键 ↑↓（`by` = ±1）之后光标该落在第几行。
///
/// 没有光标时：↓ 落第一行、↑ 落最后一行（「从哪头进来」的常规手感）。
/// 到头了就停在头上（**不绕回**：绕回去的话按住 ↓ 会在长列表里无声地转圈）。
pub fn step_target(len: usize, cursor: Option<usize>, by: isize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    Some(match cursor {
        None => {
            if by >= 0 {
                0
            } else {
                last
            }
        }
        Some(c) => {
            let c = c.min(last) as isize;
            (c + by).clamp(0, last as isize) as usize
        }
    })
}

/// Home / End 之后光标该落在第几行（空列表 ⇒ `None`）。
///
/// ⚠ **不走** [`step_target`] 挪一个「很大的步」：那一版在「还没有光标」时
/// 按 End 落在**第一行**（没光标时正步长从头进）—— 判据当场逮到的就是这一形。
pub fn edge_target(len: usize, end: bool) -> Option<usize> {
    match (len, end) {
        (0, _) => None,
        (n, true) => Some(n - 1),
        (_, false) => Some(0),
    }
}

/// 光标挪到第 `i` 行之后，列表要不要滚、滚到哪儿（像素偏移）。`None` = 它已经在视野里，**别动**
/// （每按一下都滚，就把用户自己的滚动按住了 —— 与 reveal「只滚一次」同一条理由）。
///
/// `first` / `last` 是**上一帧**真物化的那一段（`RenderTally::first_row` / `last_row`，
/// 左闭右开）；`pitch` 走 `rows::row_pitch`（**唯一住址**，第十刀栽过漏了行间距那一形）。
///
/// - 在上面看不见 ⇒ 它滚到**第一行**的位置；
/// - 在下面看不见（或是最后那一行 —— 物化区间最后一行常常只露半截）⇒ 它滚到**倒数第二行**的位置。
pub fn scroll_for(i: usize, first: usize, last: usize, pitch: f32) -> Option<f32> {
    let visible = last.saturating_sub(first);
    if visible == 0 || i < first {
        return Some(i as f32 * pitch);
    }
    if i + 1 >= last {
        let top = (i + 2).saturating_sub(visible);
        return Some(top as f32 * pitch);
    }
    None
}

// ═══════════════════════════════════════════════════════════════════════
// 键位
// ═══════════════════════════════════════════════════════════════════════

/// 一个按键想干什么。**只是意图**，做不做、能不能做由窗口问 [`actions_for`]。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    /// ↑ / ↓（Shift 按着 ⇒ 扩选）。
    Step { by: isize, extend: bool },
    /// Home / End（Shift 按着 ⇒ 扩选）。
    Edge { end: bool, extend: bool },
    /// Alt+↑：上一级（Windows 资源管理器与 macOS Finder 的同位键）。
    Parent,
    /// 回车：打开光标所在那一项（目录进去、文件编辑）。
    Open,
    /// Delete：删掉选中那几项（**一次问完**）。
    Delete,
    /// F2：改名。
    Rename,
    /// Ctrl+A（macOS ⌘A）：全选。
    SelectAll,
    /// 打字跳转：这一帧敲进来的字。
    Type(String),
}

/// 把 egui 这一帧的事件翻成意图（按到达顺序）。**只认按下（含按住连发），不认松开。**
///
/// 键位表（每一格都有一条真喂事件的判据，住 `select_tests` 与 `shell_tests` 末尾那一摞）：
///
/// | 键 | 干什么 |
/// |---|---|
/// | ↑ / ↓ | 光标上下移一行，只选光标那一行 |
/// | Shift+↑ / Shift+↓ | 从锚那一行扩选到光标 |
/// | Home / End | 跳到第一行 / 最后一行（Shift 同样扩选） |
/// | Alt+↑ | 上一级目录 |
/// | 回车 | 打开：目录进去，文件编辑（只对一项） |
/// | Delete | 删除选中的那几项（**一次问完**） |
/// | F2 | 改名（只对一项） |
/// | Ctrl+A | 全选（macOS 上是 ⌘A） |
/// | 直接打字 | 跳到名字以这几个字开头的那一行（不分大小写，停一秒重来） |
///
/// ⚠ 这张表**不做成一个常量**：它不画在窗口上，而字符串常量会被探针对账
/// （`fonts_tests`）与文案普查（CP1）当成界面文字收进去 —— 一份没人看得见的「标签」。
/// ⚠ ← / → 刻意**不接**：列表视图里它们没有公认的手感（资源管理器里不动，Finder 里是展开），
/// 接成「进目录 / 上一级」是替用户发明一套。
///
/// ⚠ Ctrl+A 这一格没有 `Text` 事件跟着：egui-winit 在 Ctrl / ⌘ 按着时**不发** `Text`
/// （它把那当成命令的副作用吞掉）⇒ 不会同时触发「全选」与「跳到 a 开头那一行」。
pub fn intents(events: &[egui::Event]) -> Vec<Intent> {
    use egui::Key;
    let mut out = Vec::new();
    for ev in events {
        match ev {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers: m,
                ..
            } => {
                let it = match key {
                    Key::ArrowUp if m.alt => Some(Intent::Parent),
                    Key::ArrowUp => Some(Intent::Step {
                        by: -1,
                        extend: m.shift,
                    }),
                    Key::ArrowDown if !m.alt => Some(Intent::Step {
                        by: 1,
                        extend: m.shift,
                    }),
                    Key::Home => Some(Intent::Edge {
                        end: false,
                        extend: m.shift,
                    }),
                    Key::End => Some(Intent::Edge {
                        end: true,
                        extend: m.shift,
                    }),
                    Key::Enter => Some(Intent::Open),
                    Key::Delete => Some(Intent::Delete),
                    Key::F2 => Some(Intent::Rename),
                    Key::A if m.command => Some(Intent::SelectAll),
                    _ => None,
                };
                if let Some(it) = it {
                    out.push(it);
                }
            }
            egui::Event::Text(t) if !t.is_empty() => out.push(Intent::Type(t.clone())),
            _ => {}
        }
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════
// 打字跳转
// ═══════════════════════════════════════════════════════════════════════

/// 两次敲字之间隔多久算「重新开始」（秒，egui 的输入时钟）。
///
/// ⚠ **它不是一个定时器**：没有任何东西到点醒来。它只在**下一个字到的那一刻**
/// 拿两个时间戳比一下 —— 节拍源是用户的键盘（同 `find` 头注「节拍源是用户动作」）。
pub const TYPE_AHEAD_RESET_SECS: f64 = 1.0;

/// 打字跳转攒着的那几个字。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypeAhead {
    buf: String,
    last: Option<f64>,
}

impl TypeAhead {
    /// 在 `now`（egui 输入时钟，秒）收到 `text` ⇒ 回现在攒着的整段前缀。
    pub fn feed(&mut self, now: f64, text: &str) -> &str {
        let stale = match self.last {
            Some(t) => now - t > TYPE_AHEAD_RESET_SECS,
            None => true,
        };
        if stale {
            self.buf.clear();
        }
        self.buf.push_str(text);
        self.last = Some(now);
        &self.buf
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// 名字以 `prefix` 开头（**不分大小写**）的第一行。
///
/// ⚠ 逐字符比、不分配：64 万行那一档一次按键扫一遍，每行分配一个小写副本就是 64 万次分配。
pub fn jump_target(rows: &[Listed], prefix: &str) -> Option<usize> {
    if prefix.is_empty() {
        return None;
    }
    rows.iter()
        .position(|r| starts_with_ignoring_case(&r.name, prefix))
}

fn starts_with_ignoring_case(name: &str, prefix: &str) -> bool {
    let mut n = name.chars().flat_map(char::to_lowercase);
    for p in prefix.chars().flat_map(char::to_lowercase) {
        if n.next() != Some(p) {
            return false;
        }
    }
    true
}

// ═══════════════════════════════════════════════════════════════════════
// 能做什么：菜单与键盘共用的**唯一**判定
// ═══════════════════════════════════════════════════════════════════════

/// 对选中那几项能做的一件事。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    /// 进这个目录。
    Open,
    /// 编辑这份文本。
    Edit,
    /// 零流量复制（「复制为」那个框）。
    Copy,
    /// 往外拖（「存到哪儿」那一问）。
    Download,
    /// 〔W5-FILES · `设计/60 §6.2`〕算大小（后端 `files-size`）—— 一项或多项。
    Size,
    Rename,
    /// 改权限 —— 〔FW5〕对一项或多项（头注 §三）。
    Chmod,
    /// 删除 —— 对一项或多项（头注 §三）。
    Delete,
}

/// 「打开」那一项的字。另外六项**复用**行上那几颗按钮的字（各自的唯一住址），
/// 不在这儿另写一份 —— 两份字漂开的症状是「菜单上叫一个名字，行上叫另一个」。
pub static OPEN_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinSelect.label.open", &[]));

impl Action {
    /// 菜单上那一项的字。`n` = 选中了几项（只有「删除」对多项说话时要它）。
    pub fn label(self, n: usize) -> String {
        match self {
            Action::Open => OPEN_LABEL.to_string(),
            Action::Edit => super::editor::EDIT_LABEL.to_string(),
            Action::Copy => super::copy::COPY_LABEL.to_string(),
            Action::Download => super::download::DOWNLOAD_LABEL.to_string(),
            Action::Size => super::size::SIZE_LABEL.to_string(),
            Action::Rename => super::writeops::RENAME_LABEL.to_string(),
            Action::Chmod if n > 1 => {
                copy_text("rsFilewinSelect.label.chmodMany", &[("n", &n.to_string())])
            }
            Action::Chmod => super::writeops::CHMOD_LABEL.to_string(),
            Action::Delete if n > 1 => {
                copy_text("rsFilewinSelect.label.deleteMany", &[("n", &n.to_string())])
            }
            Action::Delete => super::writeops::DELETE_LABEL.to_string(),
        }
    }
}

/// 🔴 **对这几项能做什么 —— 右键菜单画的就是它，键盘准不准做问的也是它。**
///
/// 每一格的判准**都借**已有的那一个唯一住址（`is_editable` / `is_copyable` /
/// `is_downloadable` / `is_writable`）—— 行上那几颗按钮画不画问的正是同几个函数
/// ⇒ 「行上有那颗按钮」与「菜单上有那一项」对单选**逐项同源**。
///
/// 回值按固定顺序（打开/编辑 · 复制 · 下载 · 〔W5-FILES〕算大小 · 改名 · 权限 · 删除）。空 ⇒ 什么都做不了。
pub fn actions_for(picked: &[&Listed]) -> Vec<Action> {
    let mut out = Vec::new();
    match picked {
        [] => {}
        [r] => {
            if r.is_dir {
                out.push(Action::Open);
            } else if is_editable(r) {
                out.push(Action::Edit);
            }
            if is_copyable(r) {
                out.push(Action::Copy);
            }
            if is_downloadable(r) {
                out.push(Action::Download);
            }
            // 〔W5-FILES〕算大小：名字寻址得到就给（文件也收，后端回它自己）。
            if !r.lossy_name {
                out.push(Action::Size);
            }
            if is_writable(r) {
                out.push(Action::Rename);
                out.push(Action::Chmod);
                out.push(Action::Delete);
            }
        }
        many => {
            // 〔W5-FILES〕多项也能算大小 —— 同样要**每一项**都寻址得到（`设计/60 §6.3`）。
            if many.iter().all(|r| !r.lossy_name) {
                out.push(Action::Size);
            }
            // 🔴 多项只有权限与删除，而且要**每一项都能写**才给：
            //    给「删除这 3 项」却只删 2 项（有损名那一项悄悄跳过），
            //    与「选中态 == 批量那一摞」这条相等正相反。
            if many.iter().all(|r| is_writable(r)) {
                out.push(Action::Chmod);
                out.push(Action::Delete);
            }
        }
    }
    out
}

/// 某一件做不了时，屏幕上说的那一句（**不许静默**：按了 F2 没反应与「改完了」同形）。
pub fn refusal(action: Action, n: usize) -> String {
    match (action, n) {
        (_, 0) => copy_text("rsFilewinSelect.refusal.none", &[]),
        (Action::Delete, _) => copy_text("rsFilewinSelect.refusal.deleteBadNames", &[]),
        (Action::Chmod, _) if n > 1 => copy_text("rsFilewinSelect.refusal.chmodBadNames", &[]),
        (Action::Open | Action::Edit, _) => {
            copy_text("rsFilewinSelect.refusal.openOne", &[("n", &n.to_string())])
        }
        (Action::Rename, _) if n > 1 => copy_text(
            "rsFilewinSelect.refusal.renameOne",
            &[("n", &n.to_string())],
        ),
        (a, _) => copy_text(
            "rsFilewinSelect.refusal.cannot",
            &[("label", &(a.label(n)).to_string())],
        ),
    }
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/select_tests.rs"]
mod tests;
