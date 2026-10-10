//! **大文件模式**：只排视口内的行。
//!
//! 用户逐字：「大文件编辑只排视口内的行, 详细思考优化」·「大文件怎么判断. 你思考怎么做」。
//! 这儿只留实现要知道的几条（`D2`：一件事只写一处）。
//!
//! # 一、怎么判断（[`judge`]）
//!
//! 卡顿有**两个**互不相关的来源：全文有多少字节（egui 每帧那笔
//! O(段数) 的记账）· 最长一行有多少字节（epaint 不可再分的排版单位就是一段）。
//! ⇒ **两条任一成立**就进大文件模式：最长一行 > [`BIG_LINE_BYTES`] · 全文 > [`BIG_TOTAL_BYTES`]。
//! 两个数由 [`derive_threshold`] 从 [`LINE_READING`] / [`TOTAL_READING`] 两份现打读数推出来，
//! 判据钉「推算式 == 常量」—— 改了读数不改常量、或改了常量不改读数，当场红。
//!
//! 🔴 一旦进了大文件模式，这一份打开着的文本**就一直是大文件模式**（不回落）：
//! 来回切会丢掉光标、选区和撤销栈，而用户敲一个键把最长一行拉过线又删回来是常事。
//!
//! # 二、大文件模式长什么样（[`Doc`]）
//!
//! **不用 `TextEdit`**，自己画。已经把「窗口化 `TextEdit`」量死了：
//! 它的撤销栈快照的是窗口 ⇒ 滚一屏再 Ctrl+Z 会把旧窗口的字写进新窗口的字节区间；
//! 而本刀还要**横向**也只排可见那一段 —— 一扇横竖都切过的窗口，
//! 跨行选中删除时「窗口外那半截算不算被删」没有正确答案。
//! ⇒ 全文、光标、选区、撤销栈都住**全文坐标**（字节偏移），界面只是一层投影：
//!
//! 1. **只排视口内的行**：`ScrollArea::show_viewport` 给出可见矩形 ⇒ 行号区间
//!    `⌊y₀/行高⌋ .. ⌈y₁/行高⌉`，只有这些行会被交给 epaint。
//! 2. **超长行只排横向可见那一段**：长行不换行；横向按「字」计列，
//!    可见列 `⌊x₀/字宽⌋ .. +⌈宽/字宽⌉+1`，只有这一截交给 epaint；
//!    没排全的那一行在行尾（或视口右缘）标「这一行共 N 字」。
//! 3. **编辑只重排改到的那几行**：每一行（的可见段）是一份独立的 `LayoutJob`，
//!    走 epaint 自己的排版缓存 ⇒ 没改到的行这一帧是缓存命中，**不排**；
//!    改到的行内容变了 ⇒ 缓存未命中 ⇒ 只排那几行。
//!    ⚠ **刻意不自己再缓存一份 `Arc<Galley>`**：字体图集满 80 % 或缩放一变，
//!    epaint 会整个重建图集，自己留着的 galley 指着旧图集的纹理坐标 ⇒ 字画成乱码；
//!    epaint 自己的缓存会跟着清。
//! 4. **行结构增量维护**（[`Lines::splice`]）：一次编辑只改受影响那几行的表项，
//!    其余行起点整体平移 —— 不重扫全文。
//!
//! # 三、它**买不到**什么（逐条，别读宽）
//!
//! - 横向按「字」计列：宽字（中文）与制表符比一列宽 ⇒ 含宽字的长行在横向滚动时，
//!   那一截的起点按列数摆、里面的字按真字宽排 ⇒ 滚过宽字时会有半格的跳动。
//!   纯 ASCII 的长行（压成一行的 JSON / 日志 —— 正是这一族的主体）没有这个问题。
//! - 没有软换行、没有语法高亮、没有查找替换（`editor.rs` 头注 §三 原样成立）。
//! - 输入法：预编辑只画不写、提交才写进全文 —— **本机没有输入法**
//!   （`XDG_SESSION_TYPE=tty`），这一格一趟都没跑过，原样留在「判不了」里。
//! - 光标不闪（闪要一个定时重绘，本仓的零定时器那条纪律）。
//! - 每帧剩下的 O(全文) 不在本模块：`shell.rs` 的编辑面每帧克隆一次 `Pane`
//!   （连全文）、[`Pane::dirty`] 比一次全文 —— 那是拷贝与比较，不是排版；
//!   在今天 256 KiB 的读上限下是几十微秒。
//!   而中间插一个字的 `String::replace_range` 是 O(全文) 的搬移（16 MiB 3.4 ms）。

use copy_core::copy_text;
use std::ops::Range;
use std::sync::{Arc, Mutex};

use egui::text::{CCursor, LayoutJob};
use egui::{
    Event, EventFilter, FontId, Galley, Id, ImeEvent, Key, Modifiers, Pos2, Rect, ScrollArea,
    Sense, Stroke, TextStyle, Ui, Vec2,
};

use super::editor::Pane;

// ═══════════════════════════════════════════════════════════════════════
// 一、怎么判断：两个阈值 ＝ 两份读数推出来的
// ═══════════════════════════════════════════════════════════════════════

/// 一帧的预算（60 Hz），微秒。
pub const FRAME_BUDGET_US: u64 = 16_667;

/// 每一维分到的份数：**三分之一帧**。
///
/// 两个来源互相独立、会**叠加**（一份 256 KiB、里面还有一行 32 KiB 的文件两笔都付）
/// ⇒ 两条都刚好不越线的最坏文件吃掉 2/3 帧，剩 1/3 给上屏、`shell.rs` 那两笔
/// O(全文) 的拷贝与比较、以及用户那台比开发机慢的机器。
pub const BUDGET_SHARE: u64 = 3;

/// 一份现打读数：普通路径（生产那个 `TextEdit`）**每敲一个键**那一帧，
/// 在 `at_bytes` 那么大的参照语料上，三趟里**最坏**那一趟的耗时。
///
/// 🔴「敲一个键」逐字是**真敲**：控件有焦点、这一帧收到一个 `Event::Text`。
/// 上一版读数在帧与帧之间 `push` 一个字，控件那一帧只是「文本变了、重排一次」；真敲键还要走
/// 控件自己的事件路径（撤销器前后各克隆一次全文 · 字下标换字节下标 · **改完再排一次**）——
/// 全文那一维现打贵 2.2 倍，上一版的 1 MiB 门槛因此放宽了一倍（逮到它的是经窗口生产路径量出来的
/// 1 MiB 规整文本每键 10.7–16.7 ms，与「1 MiB 只要 7.49 ms」对不上）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reading {
    pub at_bytes: u64,
    pub worst_us: u64,
}

/// 全文那一维的读数（每行 64 字节的规整文本；release 档；墙钟；真敲键）。
///
/// 参照档怎么挑：扫描（64 · 128 · 256 · 512 KiB · 1 MiB）里**第一档最坏读数越过预算**
/// 的那一档 —— 离要推的那个数最近、线性外推最短。2026-09-24 现打三次 × 三趟（负载 5–8 / 24 核）：
/// 256 KiB 最坏 5.43 ms（没越）· **512 KiB 最坏 11.90 ms（越了）**。
/// 来源：`tests::the_readings_behind_the_two_thresholds`，逐趟读数住。
pub const TOTAL_READING: Reading = Reading {
    at_bytes: 512 * 1024,
    worst_us: 11_900,
};

/// 最长一行那一维的读数（整份压成一行；release 档；墙钟；真敲键）。参照档挑法同上：
/// 扫描（4 · 8 · 16 · 32 · 64 KiB）里 16 KiB 最坏 3.30 ms（没越）· **32 KiB 最坏 6.62 ms（越了）**。
pub const LINE_READING: Reading = Reading {
    at_bytes: 32 * 1024,
    worst_us: 6_620,
};

/// 从一份读数推阈值：**预算 × 参照字节 ÷ 最坏耗时**，再取对数意义上最近的 2 的幂。
///
/// 取 2 的幂不是为了好看：同一档语料九趟之间差到 1.4 倍（本拍现打 512 KiB 7.67–11.90 ms），
/// 推出来的数本来就只准到这个量级 ⇒
/// 「最近的 2 的幂」（误差 ≤ √2 倍）与读数的精度同阶，多保留的位数都是噪声。
///
/// ⚠ 如实登记它的代价：本拍推出来 27 496 → 32 KiB（进位）、244 741 → 256 KiB（进位）；
/// 刚好不越线的文件实际吃到约 0.3–0.4 帧；两条都刚好不越线的最坏文件 ≈ 6.62 ＋ 5.43 ＝ 12.1 ms，
/// **仍在一帧之内**。
pub const fn derive_threshold(r: Reading) -> usize {
    let raw = (FRAME_BUDGET_US / BUDGET_SHARE) as u128 * r.at_bytes as u128 / r.worst_us as u128;
    nearest_pow2(raw) as usize
}

/// 对数意义上最近的 2 的幂（`x ≥ √2·下界` ⇒ 进位）。
pub const fn nearest_pow2(x: u128) -> u128 {
    if x <= 1 {
        return 1;
    }
    let lo = 1u128 << (127 - x.leading_zeros());
    if x * x >= 2 * lo * lo {
        lo * 2
    } else {
        lo
    }
}

/// 最长一行超过这么多字节 ⇒ 大文件模式。**住这一处**；判据钉它 == `derive_threshold(LINE_READING)`。
pub const BIG_LINE_BYTES: usize = 32 * 1024;

/// 全文超过这么多字节 ⇒ 大文件模式。**住这一处**；判据钉它 == `derive_threshold(TOTAL_READING)`。
pub const BIG_TOTAL_BYTES: usize = 256 * 1024;

/// 一份文本量出来的两个数。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Why {
    pub longest_line: usize,
    pub total: usize,
}

impl Why {
    pub fn line_over(&self) -> bool {
        self.longest_line > BIG_LINE_BYTES
    }
    pub fn total_over(&self) -> bool {
        self.total > BIG_TOTAL_BYTES
    }
}

/// 量一份文本（一趟 `\n` 切段，O(n)）。
pub fn measure(text: &str) -> Why {
    Why {
        longest_line: text.split('\n').map(str::len).max().unwrap_or(0),
        total: text.len(),
    }
}

/// 进不进大文件模式：两条**任一**成立（严格大于）。
pub fn judge(text: &str) -> Option<Why> {
    let w = measure(text);
    (w.line_over() || w.total_over()).then_some(w)
}

// ═══════════════════════════════════════════════════════════════════════
// 二、行结构：增量维护的行起点 ＋ 每行字数
// ═══════════════════════════════════════════════════════════════════════

/// 一份文本的行结构。「行」＝ 按 `\n` 切出来的段（与 epaint 切段同一个单位）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lines {
    /// `starts[i]` ＝ 第 `i` 行第一个字节的偏移。恒非空（`starts[0] == 0`）。
    starts: Vec<usize>,
    /// `chars[i]` ＝ 第 `i` 行（不含 `\n`）有几个字。与 `starts` 等长。
    chars: Vec<usize>,
    len: usize,
    /// 最宽那一行的字数；`None` ＝ 过期了、下次要的时候重算。
    widest: Option<usize>,
}

impl Lines {
    /// 扫一遍全文建表。O(n)，只在打开 / 外部换掉全文时走。
    pub fn build(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        let mut me = Self {
            chars: Vec::with_capacity(starts.len()),
            starts,
            len: text.len(),
            widest: None,
        };
        for i in 0..me.starts.len() {
            let n = me.line_str(text, i).chars().count();
            me.chars.push(n);
        }
        me
    }

    pub fn count(&self) -> usize {
        self.starts.len()
    }

    pub fn total_bytes(&self) -> usize {
        self.len
    }

    pub fn start(&self, line: usize) -> usize {
        self.starts.get(line).copied().unwrap_or(self.len)
    }

    /// 第 `line` 行内容的末尾 —— **不含那个 `\n`**。
    pub fn content_end(&self, line: usize) -> usize {
        match self.starts.get(line + 1) {
            Some(&next) => next - 1,
            None => self.len,
        }
    }

    pub fn chars(&self, line: usize) -> usize {
        self.chars.get(line).copied().unwrap_or(0)
    }

    pub fn line_str<'a>(&self, text: &'a str, line: usize) -> &'a str {
        &text[self.start(line)..self.content_end(line)]
    }

    /// 字节偏移 `at` 落在第几行。
    pub fn line_of(&self, at: usize) -> usize {
        self.starts.partition_point(|&s| s <= at).saturating_sub(1)
    }

    /// 最宽那一行有几个字（横向滚动的总宽度靠它）。
    pub fn widest(&mut self) -> usize {
        if self.widest.is_none() {
            self.widest = Some(self.chars.iter().copied().max().unwrap_or(0));
        }
        self.widest.unwrap_or(0)
    }

    /// 🔴 **一次编辑之后增量更新**：`text` 是**改过之后**的全文，
    /// 改的是原先的 `[at, at + removed)`（`removed_text` 是被删掉的那段），换成了 `inserted`。
    ///
    /// 只动受影响那几行的表项；其后的行起点整体平移 `inserted.len() − removed`。
    /// 由 `tests::incremental_lines_equal_a_rebuild_after_every_edit` 对着
    /// `str::split('\n')` 那份独立答案逐步钉住。
    pub fn splice(&mut self, text: &str, at: usize, removed_text: &str, inserted: &str) {
        let removed = removed_text.len();
        let la = self.line_of(at);
        // 被删掉的 `\n` 对应的行起点：落在 (at, at + removed] 里的那些。
        let lo = la + 1;
        let hi = self.starts.partition_point(|&s| s <= at + removed);
        let delta = inserted.len() as isize - removed as isize;
        let fresh: Vec<usize> = inserted
            .match_indices('\n')
            .map(|(i, _)| at + i + 1)
            .collect();
        let old_widest = self.widest;
        let old_first = self.chars(la);
        let old_touched_widest = old_widest.is_some_and(|w| self.chars[la..hi].contains(&w));
        for s in &mut self.starts[hi..] {
            *s = (*s as isize + delta) as usize;
        }
        let n_new = fresh.len();
        self.starts.splice(lo..hi, fresh);
        self.len = text.len();
        // 每行字数：单行内改（两侧都没有 `\n`）走差量，不重数整行 —— 那是打字的常态，
        // 而一行可以有几 MiB。
        let new_counts: Vec<usize> = if removed_text.contains('\n') || inserted.contains('\n') {
            (la..=la + n_new)
                .map(|i| self.line_str(text, i).chars().count())
                .collect()
        } else {
            vec![old_first - removed_text.chars().count() + inserted.chars().count()]
        };
        let new_max = new_counts.iter().copied().max().unwrap_or(0);
        self.chars.splice(la..hi, new_counts);
        self.widest = match old_widest {
            Some(w) if new_max >= w => Some(new_max),
            Some(w) if !old_touched_widest => Some(w),
            _ => None,
        };
    }
}

/// 一行里第 `k` 个字从第几个字节开始（`k` 过了行尾 ⇒ 行的字节长）。
///
/// ⚠ `ascii`（调用方从「字数 == 字节数」知道）⇒ O(1)；否则按 64 字节一块数「字首字节」
/// 跳过整块、最后一块逐字节走 —— 每帧对每一可见行要走一次，长行上不能用 `char_indices().nth`。
pub fn byte_at_char(s: &str, k: usize, ascii: bool) -> usize {
    if k == 0 {
        return 0;
    }
    if ascii {
        return k.min(s.len());
    }
    let b = s.as_bytes();
    let is_start = |x: u8| (x as i8) >= -0x40;
    let mut i = 0;
    let mut left = k;
    while i + 64 <= b.len() {
        let n = b[i..i + 64].iter().filter(|&&x| is_start(x)).count();
        if n > left {
            break;
        }
        left -= n;
        i += 64;
    }
    while i < b.len() {
        if is_start(b[i]) {
            if left == 0 {
                return i;
            }
            left -= 1;
        }
        i += 1;
    }
    b.len()
}

// ═══════════════════════════════════════════════════════════════════════
// 三、大文件模式那一面
// ═══════════════════════════════════════════════════════════════════════

/// 不知道摆多高时一屏几行（判据那一形；生产上编辑面的高由它那一栏剩下的高给，见 [`show`]）。
pub const VIEW_ROWS: usize = 24;

/// `rows` 行等宽字有多高。
pub fn view_height(ui: &Ui, rows: usize) -> f32 {
    let font = TextStyle::Monospace.resolve(ui.style());
    rows as f32 * ui.fonts_mut(|f| f.row_height(&font))
}

/// 横向总宽在最宽那一行之后再留几列（放「这一行共 N 字」）。
const TAG_ROOM: usize = 20;

/// 撤销栈最多留几步（每一步只存改动那一截，不存全文）。
const MAX_UNDO: usize = 1000;

/// 普通路径那个 `TextEdit` 的 id —— 切进大文件模式那一下，从它的状态里把光标接过来。
const NORMAL_ID: &str = "filewin-editor-text";

/// 普通路径那个 `TextEdit` 的 id —— 编辑面的查找替换（`shell.rs`）要读 / 设它的选区。
/// 按那份文件的路径分开（几个编辑页各是各的输入状态，两栏同时摆着两页也不互相抢）。
pub(crate) fn normal_editor_id(path: &str) -> Id {
    Id::new((NORMAL_ID, path))
}

/// 焦点在编辑面上时，方向键与 Tab 归编辑面（不拿去切焦点）。
const FILTER: EventFilter = EventFilter {
    tab: true,
    horizontal_arrows: true,
    vertical_arrows: true,
    escape: false,
};

/// 一步编辑（全文坐标）。撤销 ＝ 把 `[at, at+inserted)` 换回 `removed`。
#[derive(Clone, Debug)]
struct Step {
    at: usize,
    removed: String,
    inserted: String,
    before: (usize, usize),
    after: (usize, usize),
    /// 连续打字并成一步。
    typing: bool,
}

/// 这一帧交给排版的一行（上一帧那份留着做鼠标命中 —— 用户点的是他上一帧看见的东西）。
#[derive(Clone)]
struct Laid {
    line: usize,
    /// 可见段的字区间 `[s0, s1)`；`n` ＝ 整行字数。
    s0: usize,
    s1: usize,
    n: usize,
    /// 可见段在全文里的字节区间。
    bytes: Range<usize>,
    /// 可见段左缘（内容坐标）＝ `s0 × 字宽`。
    x: f32,
    galley: Option<Arc<Galley>>,
}

/// 这一帧排了什么 —— 判据与诊断靠它说话（每帧从零数）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    /// 可见的行号区间 `[first_line, end_line)`。
    pub first_line: usize,
    pub end_line: usize,
    /// 可见的列：从第几个字起、几列。
    pub first_col: usize,
    pub cols: usize,
    /// 交给排版的：几行（非空段）、几个字、几个字节。
    pub lines_laid: usize,
    pub chars_laid: usize,
    pub bytes_laid: usize,
    /// 标了「这一行共 N 字」的那几行：`(行号, N)`。
    pub tags: Vec<(usize, usize)>,
}

/// 大文件模式的全部状态。住在 [`Pane`] 上（[`BigSlot`]），关掉编辑面就一起没了。
pub struct Doc {
    lines: Lines,
    /// 全文的指纹（长度 ＋ 缓冲区地址）：本模块以外有人换了全文 ⇒ 重建。
    fp: (usize, usize),
    why: Why,
    cur: usize,
    anc: usize,
    /// 上下移动时想回到的那一列（字）。
    goal: Option<usize>,
    undo: Vec<Step>,
    redo: Vec<Step>,
    preedit: String,
    /// 下一帧把光标滚进视野。
    follow: bool,
    /// 下一帧钉死滚动偏移（量帧 / 跳转用）。
    pin: Option<Vec2>,
    /// 这一屏摆得下几行（PageUp / PageDown 走这么多）。
    page: usize,
    laid: Vec<Laid>,
    tally: Tally,
}

fn fingerprint(text: &str) -> (usize, usize) {
    (text.len(), text.as_ptr() as usize)
}

fn floor_boundary(text: &str, mut at: usize) -> usize {
    at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

impl Doc {
    pub fn new(text: &str, why: Why, cursor: usize) -> Self {
        let c = floor_boundary(text, cursor);
        Self {
            lines: Lines::build(text),
            fp: fingerprint(text),
            why,
            cur: c,
            anc: c,
            goal: None,
            undo: Vec::new(),
            redo: Vec::new(),
            preedit: String::new(),
            follow: c > 0,
            // 滚动状态按 id 存在 egui 里、跨文件共用 ⇒ 新打开一份从头看起（有接过来的光标就再滚过去）。
            pin: Some(Vec2::ZERO),
            page: VIEW_ROWS,
            laid: Vec::new(),
            tally: Tally::default(),
        }
    }

    pub fn why(&self) -> Why {
        self.why
    }
    pub fn lines(&self) -> &Lines {
        &self.lines
    }
    pub fn tally(&self) -> &Tally {
        &self.tally
    }
    /// 光标与选区另一端（字节偏移）。
    pub fn cursor(&self) -> (usize, usize) {
        (self.cur, self.anc)
    }
    /// 下一帧把滚动偏移钉在 `off`（内容坐标，像素）。
    pub fn pin_offset(&mut self, off: Vec2) {
        self.pin = Some(off);
    }

    /// 有人在本模块之外换了全文 ⇒ 行结构、撤销栈都作废，重建。
    ///
    /// ⚠ 漏判面（如实登记）：长度与缓冲区地址都不变的**原地**改写看不见。
    /// 生产上全文只经本模块改；判据里换全文是整份赋值（地址会变）。
    fn revalidate(&mut self, text: &str) {
        if fingerprint(text) == self.fp {
            return;
        }
        self.lines = Lines::build(text);
        self.fp = fingerprint(text);
        self.cur = floor_boundary(text, self.cur);
        self.anc = floor_boundary(text, self.anc);
        self.undo.clear();
        self.redo.clear();
        self.laid.clear();
    }

    fn sel(&self) -> Range<usize> {
        self.cur.min(self.anc)..self.cur.max(self.anc)
    }

    /// 🔴 **全文唯一的改写口**：换掉 `[range)`，行结构增量跟上，记一步撤销。
    fn replace(&mut self, text: &mut String, range: Range<usize>, with: &str, typing: bool) {
        let before = (self.cur, self.anc);
        let removed = text[range.clone()].to_string();
        text.replace_range(range.clone(), with);
        self.lines.splice(text, range.start, &removed, with);
        self.fp = fingerprint(text);
        let c = range.start + with.len();
        self.cur = c;
        self.anc = c;
        self.goal = None;
        self.follow = true;
        self.redo.clear();
        let merge = typing
            && removed.is_empty()
            && !with.contains('\n')
            && self.undo.last().is_some_and(|s| {
                s.typing && s.removed.is_empty() && s.at + s.inserted.len() == range.start
            });
        if merge {
            if let Some(s) = self.undo.last_mut() {
                s.inserted.push_str(with);
                s.after = (c, c);
            }
        } else {
            self.undo.push(Step {
                at: range.start,
                removed,
                inserted: with.to_string(),
                before,
                after: (c, c),
                typing,
            });
            if self.undo.len() > MAX_UNDO {
                self.undo.remove(0);
            }
        }
    }

    fn insert(&mut self, text: &mut String, s: &str, typing: bool) {
        let r = self.sel();
        self.replace(text, r, s, typing);
    }

    fn undo(&mut self, text: &mut String) {
        let Some(s) = self.undo.pop() else { return };
        let r = s.at..s.at + s.inserted.len();
        text.replace_range(r, &s.removed);
        self.lines.splice(text, s.at, &s.inserted, &s.removed);
        self.fp = fingerprint(text);
        (self.cur, self.anc) = s.before;
        self.follow = true;
        self.redo.push(s);
    }

    fn redo(&mut self, text: &mut String) {
        let Some(s) = self.redo.pop() else { return };
        let r = s.at..s.at + s.removed.len();
        text.replace_range(r, &s.inserted);
        self.lines.splice(text, s.at, &s.removed, &s.inserted);
        self.fp = fingerprint(text);
        (self.cur, self.anc) = s.after;
        self.follow = true;
        self.undo.push(s);
    }

    fn move_to(&mut self, at: usize, extend: bool) {
        self.cur = at;
        if !extend {
            self.anc = at;
        }
        self.follow = true;
    }

    fn prev_char(text: &str, at: usize) -> usize {
        text[..at].char_indices().next_back().map_or(0, |(i, _)| i)
    }

    fn next_char(text: &str, at: usize) -> usize {
        text[at..].chars().next().map_or(at, |c| at + c.len_utf8())
    }

    /// 字的类别：词字 · 空白 · 其余（词跳转与双击选词用）。
    fn class(c: char) -> u8 {
        if c.is_alphanumeric() || c == '_' {
            0
        } else if c.is_whitespace() {
            1
        } else {
            2
        }
    }

    fn word_left(text: &str, at: usize) -> usize {
        let mut it = text[..at].char_indices().rev().peekable();
        while it.next_if(|&(_, c)| c.is_whitespace()).is_some() {}
        let Some(&(_, c)) = it.peek() else { return 0 };
        let k = Self::class(c);
        let mut pos = at;
        for (i, c) in it {
            if Self::class(c) != k {
                break;
            }
            pos = i;
        }
        pos.min(at)
    }

    fn word_right(text: &str, at: usize) -> usize {
        let mut it = text[at..].char_indices().peekable();
        while it.next_if(|&(_, c)| c.is_whitespace()).is_some() {}
        let Some(&(_, c)) = it.peek() else {
            return text.len();
        };
        let k = Self::class(c);
        for (i, c) in it {
            if Self::class(c) != k {
                return at + i;
            }
        }
        text.len()
    }

    /// 第 `line` 行里，字节 `at` 前面有几个字。
    fn col_of(&self, text: &str, at: usize) -> usize {
        let line = self.lines.line_of(at);
        let s = self.lines.start(line);
        if self.lines.chars(line) == self.lines.content_end(line) - s {
            return at - s;
        }
        text[s..at].chars().count()
    }

    fn at_col(&self, text: &str, line: usize, col: usize) -> usize {
        let s = self.lines.line_str(text, line);
        let n = self.lines.chars(line);
        self.lines.start(line) + byte_at_char(s, col.min(n), n == s.len())
    }

    fn vertical(&mut self, text: &str, delta: isize, extend: bool) {
        let line = self.lines.line_of(self.cur);
        let goal = self.goal.unwrap_or_else(|| self.col_of(text, self.cur));
        let target = (line as isize + delta).clamp(0, self.lines.count() as isize - 1) as usize;
        let at = self.at_col(text, target, goal);
        self.move_to(at, extend);
        self.goal = Some(goal);
    }

    fn on_key(&mut self, text: &mut String, key: Key, m: Modifiers) {
        let shift = m.shift;
        let word = m.command || m.alt;
        let sel = self.sel();
        let line = self.lines.line_of(self.cur);
        let keep_goal = matches!(
            key,
            Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown
        );
        match key {
            Key::Z if m.command && shift => self.redo(text),
            Key::Z if m.command => self.undo(text),
            Key::Y if m.command => self.redo(text),
            Key::A if m.command => {
                self.anc = 0;
                self.cur = text.len();
            }
            Key::Enter => self.insert(text, "\n", false),
            Key::Tab => self.insert(text, "\t", true),
            Key::Backspace => {
                if !sel.is_empty() {
                    self.replace(text, sel, "", false);
                } else if self.cur > 0 {
                    let from = if word {
                        Self::word_left(text, self.cur)
                    } else {
                        Self::prev_char(text, self.cur)
                    };
                    self.replace(text, from..self.cur, "", false);
                }
            }
            Key::Delete => {
                if !sel.is_empty() {
                    self.replace(text, sel, "", false);
                } else if self.cur < text.len() {
                    let to = if word {
                        Self::word_right(text, self.cur)
                    } else {
                        Self::next_char(text, self.cur)
                    };
                    self.replace(text, self.cur..to, "", false);
                }
            }
            Key::ArrowLeft => {
                let at = if !shift && !sel.is_empty() {
                    sel.start
                } else if word {
                    Self::word_left(text, self.cur)
                } else {
                    Self::prev_char(text, self.cur)
                };
                self.move_to(at, shift);
            }
            Key::ArrowRight => {
                let at = if !shift && !sel.is_empty() {
                    sel.end
                } else if word {
                    Self::word_right(text, self.cur)
                } else {
                    Self::next_char(text, self.cur)
                };
                self.move_to(at, shift);
            }
            Key::ArrowUp => self.vertical(text, -1, shift),
            Key::ArrowDown => self.vertical(text, 1, shift),
            Key::PageUp => self.vertical(text, -(self.page as isize), shift),
            Key::PageDown => self.vertical(text, self.page as isize, shift),
            Key::Home => {
                let at = if m.command { 0 } else { self.lines.start(line) };
                self.move_to(at, shift);
            }
            Key::End => {
                let at = if m.command {
                    text.len()
                } else {
                    self.lines.content_end(line)
                };
                self.move_to(at, shift);
            }
            _ => {}
        }
        if !keep_goal {
            self.goal = None;
        }
    }

    fn on_event(&mut self, ui: &Ui, text: &mut String, e: &Event) {
        match e {
            Event::Text(s) if !s.is_empty() && s != "\n" && s != "\r" => self.insert(text, s, true),
            Event::Paste(s) if !s.is_empty() => self.insert(text, s, false),
            Event::Copy => {
                let r = self.sel();
                if !r.is_empty() {
                    ui.ctx().copy_text(text[r].to_string());
                }
            }
            Event::Cut => {
                let r = self.sel();
                if !r.is_empty() {
                    ui.ctx().copy_text(text[r.clone()].to_string());
                    self.replace(text, r, "", false);
                }
            }
            Event::Ime(ImeEvent::Preedit { text: t, .. }) => self.preedit = t.clone(),
            Event::Ime(ImeEvent::Commit(t)) => {
                self.preedit.clear();
                if !t.is_empty() {
                    self.insert(text, t, false);
                }
            }
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => self.on_key(text, *key, *modifiers),
            _ => {}
        }
    }

    /// 内容坐标 `p` 落在全文的哪个字节上 —— 按**上一帧排出来的那几行**命中。
    fn hit(&self, text: &str, p: Vec2, row_h: f32, char_w: f32) -> usize {
        let n = self.lines.count();
        let line = ((p.y / row_h).floor().max(0.0) as usize).min(n - 1);
        if let Some(l) = self.laid.iter().find(|l| l.line == line) {
            if let Some(g) = &l.galley {
                if p.x >= l.x {
                    let k = g.cursor_from_pos(Vec2::new(p.x - l.x, row_h * 0.5)).index.0;
                    let seg = &text[l.bytes.clone()];
                    return l.bytes.start + byte_at_char(seg, k, l.s1 - l.s0 == seg.len());
                }
                return l.bytes.start;
            }
        }
        let col = (p.x / char_w).round().max(0.0) as usize;
        self.at_col(text, line, col)
    }

    fn select_word(&mut self, text: &str, at: usize) {
        let Some(c) = text[at..]
            .chars()
            .next()
            .or_else(|| text[..at].chars().next_back())
        else {
            return;
        };
        let k = Self::class(c);
        let mut a = at;
        for (i, c) in text[..at].char_indices().rev() {
            if c == '\n' || Self::class(c) != k {
                break;
            }
            a = i;
        }
        let mut b = at;
        for (i, c) in text[at..].char_indices() {
            if c == '\n' || Self::class(c) != k {
                break;
            }
            b = at + i + c.len_utf8();
        }
        self.anc = a;
        self.cur = b;
    }

    /// 画这一面：说明那一行 ＋ 高 `view_h` 的视口（内容在里面横竖滚）。
    pub fn ui(&mut self, ui: &mut Ui, text: &mut String, view_h: f32) {
        self.revalidate(text);
        let top = ui.cursor().top();
        ui.label(self.notice());
        let font = TextStyle::Monospace.resolve(ui.style());
        let (row_h, char_w) =
            ui.fonts_mut(|f| (f.row_height(&font), f.glyph_width(&font, '0').max(1.0)));
        // 说明那一行也算在给的高里。
        let view_h = (view_h - (ui.cursor().top() - top)).max(row_h);
        self.page = ((view_h / row_h).floor() as usize).max(1);
        let mut area = ScrollArea::both()
            .id_salt("filewin-bigfile")
            .auto_shrink([false, false])
            .max_height(view_h)
            .min_scrolled_height(view_h)
            .content_margin(0.0);
        if let Some(off) = self.pin.take() {
            area = area.scroll_offset(off);
        }
        area.show_viewport(ui, |ui, vp| {
            self.body(ui, text, vp, view_h, &font, row_h, char_w);
        });
    }

    /// 编辑面顶上那一行：为什么进了这个模式。
    pub fn notice(&self) -> String {
        let w = self.why;
        let size = copy_core::size_text(w.total as u64);
        match (w.total_over(), w.line_over()) {
            (true, true) => copy_text(
                "rsFilewinBigfile.notice.both",
                &[
                    ("size", &size.to_string()),
                    ("longestLine", &w.longest_line.to_string()),
                ],
            ),
            (true, false) => copy_text(
                "rsFilewinBigfile.notice.size",
                &[("size", &size.to_string())],
            ),
            _ => copy_text(
                "rsFilewinBigfile.notice.longest",
                &[("longestLine", &w.longest_line.to_string())],
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn body(
        &mut self,
        ui: &mut Ui,
        text: &mut String,
        vp: Rect,
        view_h: f32,
        font: &FontId,
        row_h: f32,
        char_w: f32,
    ) {
        let content = Vec2::new(
            (self.lines.widest() + TAG_ROOM) as f32 * char_w,
            (self.lines.count() as f32 * row_h).max(view_h),
        );
        let (rect, resp) = ui.allocate_exact_size(content, Sense::click_and_drag());
        let origin = rect.min;
        let id = resp.id;

        // ① 鼠标：按上一帧排出来的那几行命中。
        if resp.clicked() || resp.drag_started() {
            resp.request_focus();
        }
        if let Some(pos) = resp.interact_pointer_pos() {
            let at = self.hit(text, pos - origin, row_h, char_w);
            let (pressed, shift) = ui.input(|i| (i.pointer.primary_pressed(), i.modifiers.shift));
            if resp.double_clicked() {
                self.select_word(text, at);
            } else if pressed && !shift {
                self.move_to(at, false);
                self.follow = false;
            } else if pressed || resp.dragged() {
                self.move_to(at, true);
                self.follow = false;
            }
            self.goal = None;
        }

        // ② 键盘 / 文字：只在有焦点时收。
        let focused = ui.memory(|m| m.has_focus(id));
        if focused {
            ui.memory_mut(|m| m.set_focus_lock_filter(id, FILTER));
            for e in ui.input(|i| i.filtered_events(&FILTER)) {
                self.on_event(ui, text, &e);
            }
        }

        // ③ 只排视口内的行、每行只排可见那一段。
        self.lay_out(ui, text, vp, font, row_h, char_w);

        // ④ 画。
        self.paint(ui, text, vp, origin, focused, font, row_h, char_w);
    }

    fn lay_out(&mut self, ui: &Ui, text: &str, vp: Rect, font: &FontId, row_h: f32, char_w: f32) {
        let n = self.lines.count();
        let first = ((vp.min.y / row_h).floor().max(0.0) as usize).min(n);
        let end = ((vp.max.y / row_h).ceil().max(0.0) as usize).min(n);
        let c0 = (vp.min.x / char_w).floor().max(0.0) as usize;
        let cols = (vp.width() / char_w).ceil().max(0.0) as usize + 1;
        let color = ui.visuals().text_color();
        let mut t = Tally {
            first_line: first,
            end_line: end,
            first_col: c0,
            cols,
            ..Tally::default()
        };
        self.laid.clear();
        for line in first..end {
            let ls = self.lines.start(line);
            let s = self.lines.line_str(text, line);
            let nc = self.lines.chars(line);
            let s0 = c0.min(nc);
            let s1 = (c0 + cols).min(nc);
            let ascii = nc == s.len();
            let b0 = byte_at_char(s, s0, ascii);
            let b1 = b0 + byte_at_char(&s[b0..], s1 - s0, ascii);
            let seg = &s[b0..b1];
            let galley = (!seg.is_empty()).then(|| {
                let job = LayoutJob::simple_singleline(seg.to_owned(), font.clone(), color);
                ui.fonts_mut(|f| f.layout_job(job))
            });
            if galley.is_some() {
                t.lines_laid += 1;
                t.chars_laid += s1 - s0;
                t.bytes_laid += seg.len();
            }
            self.laid.push(Laid {
                line,
                s0,
                s1,
                n: nc,
                bytes: ls + b0..ls + b1,
                x: s0 as f32 * char_w,
                galley,
            });
        }
        self.tally = t;
    }

    /// 可见段里字节 `at` 的横坐标（内容坐标）；在段左 / 段右 ⇒ 视口外一点点（画选区时会被裁掉）。
    fn x_in(l: &Laid, text: &str, at: usize, vp: Rect, char_w: f32) -> f32 {
        if at < l.bytes.start {
            return vp.min.x - char_w;
        }
        if at > l.bytes.end {
            return vp.max.x + char_w;
        }
        match &l.galley {
            Some(g) => {
                let k = text[l.bytes.start..at].chars().count();
                l.x + g.pos_from_cursor(CCursor::new(k)).min.x
            }
            None => l.x,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint(
        &mut self,
        ui: &mut Ui,
        text: &str,
        vp: Rect,
        origin: Pos2,
        focused: bool,
        font: &FontId,
        row_h: f32,
        char_w: f32,
    ) {
        let painter = ui.painter().clone();
        let vis = ui.visuals().clone();
        let to_screen = |x: f32, y: f32| Pos2::new(origin.x + x, origin.y + y);
        painter.rect_filled(
            Rect::from_min_max(to_screen(vp.min.x, vp.min.y), to_screen(vp.max.x, vp.max.y)),
            0.0,
            vis.extreme_bg_color,
        );
        let sel = self.sel();
        let tag_font = FontId::proportional(font.size * 0.9);
        let mut tags = Vec::new();
        let mut cursor_rect = None;
        for l in &self.laid {
            let y = l.line as f32 * row_h;
            let ls = self.lines.start(l.line);
            let le = self.lines.content_end(l.line);
            // 选区（含跨过这一行末尾那个 `\n`）。
            if !sel.is_empty() && sel.start <= le && sel.end > ls {
                let a = sel.start.max(ls);
                let b = sel.end.min(le);
                let xa = Self::x_in(l, text, a, vp, char_w);
                let mut xb = Self::x_in(l, text, b, vp, char_w);
                if sel.end > le {
                    xb += char_w * 0.5;
                }
                painter.rect_filled(
                    Rect::from_min_max(to_screen(xa, y), to_screen(xb.max(xa), y + row_h)),
                    0.0,
                    vis.selection.bg_fill,
                );
            }
            if let Some(g) = &l.galley {
                painter.galley(to_screen(l.x, y), g.clone(), vis.text_color());
            }
            // 光标（只在它落在这一行的可见段里时画）。
            if self.lines.line_of(self.cur) == l.line
                && (l.bytes.start..=l.bytes.end).contains(&self.cur)
            {
                let x = Self::x_in(l, text, self.cur, vp, char_w);
                cursor_rect = Some(Rect::from_min_max(
                    to_screen(x, y),
                    to_screen(x + 2.0, y + row_h),
                ));
            }
            // 🔴 没排全的那一行：标「这一行共 N 字」。整行都在视口左边（一个字都看不见）的不标 ——
            //    标了也画在视口外，白排一次。
            if l.s0 < l.n && (l.s0 > 0 || l.s1 < l.n) {
                let g = painter.layout_no_wrap(
                    copy_text("rsFilewinBigfile.paint.lineLen", &[("n", &l.n.to_string())]),
                    tag_font.clone(),
                    vis.weak_text_color(),
                );
                let w = g.size().x;
                let x = if l.s1 < l.n {
                    vp.max.x - w - char_w
                } else {
                    l.x + l.galley.as_ref().map_or(0.0, |g| g.size().x) + char_w
                };
                let r = Rect::from_min_size(to_screen(x, y), Vec2::new(w, row_h));
                painter.rect_filled(r.expand(1.0), 2.0, vis.extreme_bg_color);
                painter.rect_stroke(
                    r.expand(1.0),
                    2.0,
                    Stroke::new(1.0, vis.weak_text_color()),
                    egui::StrokeKind::Outside,
                );
                painter.galley(r.min, g, vis.weak_text_color());
                tags.push((l.line, l.n));
            }
        }
        self.tally.tags = tags;

        let cursor_content = self.cursor_content_rect(text, row_h, char_w, origin, cursor_rect);
        if focused {
            if let Some(r) = cursor_rect {
                painter.rect_filled(r, 0.0, vis.text_cursor.stroke.color);
                if !self.preedit.is_empty() {
                    let g = painter.layout_no_wrap(
                        self.preedit.clone(),
                        font.clone(),
                        vis.text_color(),
                    );
                    let pr = Rect::from_min_size(r.min, g.size());
                    painter.rect_filled(pr, 0.0, vis.extreme_bg_color);
                    painter.galley(pr.min, g, vis.text_color());
                    painter.hline(pr.x_range(), pr.max.y, Stroke::new(1.0, vis.text_color()));
                }
            }
            let to_global = ui
                .ctx()
                .layer_transform_to_global(ui.layer_id())
                .unwrap_or_default();
            let whole =
                Rect::from_min_max(to_screen(vp.min.x, vp.min.y), to_screen(vp.max.x, vp.max.y));
            ui.output_mut(|o| {
                o.ime = Some(egui::output::IMEOutput {
                    purpose: egui::IMEPurpose::Normal,
                    rect: to_global * whole,
                    cursor_rect: to_global * cursor_content,
                    should_interrupt_composition: false,
                });
            });
        }
        if self.follow {
            self.follow = false;
            ui.scroll_to_rect(cursor_content.expand2(Vec2::new(char_w * 4.0, 0.0)), None);
        }
    }

    /// 光标的矩形（屏幕坐标）：画出来了就用画出来那个，否则按列数推（给滚进视野用）。
    fn cursor_content_rect(
        &self,
        text: &str,
        row_h: f32,
        char_w: f32,
        origin: Pos2,
        painted: Option<Rect>,
    ) -> Rect {
        if let Some(r) = painted {
            return r;
        }
        let line = self.lines.line_of(self.cur);
        let x = self.col_of(text, self.cur) as f32 * char_w;
        let y = line as f32 * row_h;
        Rect::from_min_size(Pos2::new(origin.x + x, origin.y + y), Vec2::new(2.0, row_h))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 四、挂在 `Pane` 上的那一格 ＋ 生产入口
// ═══════════════════════════════════════════════════════════════════════

/// [`Pane`] 上那一格：`None` ＝ 普通路径；`Some` ＝ 已进大文件模式。
///
/// ⚠ `Arc<Mutex>` 而不是直接放一个 `Option<Doc>`：`shell.rs` 的编辑面每帧
/// `self.editing.clone()` 一次，直接放会每帧把行表与撤销栈整份复制一遍；
/// 共享一份是对的 —— 那个克隆只拿去读名字、脏不脏、存的结局。
#[derive(Clone, Default)]
pub struct BigSlot(Arc<Mutex<Option<Doc>>>);

impl std::fmt::Debug for BigSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let on = self.lock().is_some();
        write!(f, "BigSlot({})", if on { "big" } else { "normal" })
    }
}

impl BigSlot {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Doc>> {
        match self.0.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    /// 进了大文件模式没有。
    pub fn is_big(&self) -> bool {
        self.lock().is_some()
    }

    /// 上一帧排了什么（没进模式 ⇒ `None`）。
    pub fn tally(&self) -> Option<Tally> {
        self.lock().as_ref().map(|d| d.tally.clone())
    }

    /// 在那份 `Doc` 上做点事（判据钉滚动偏移、读光标用）。
    pub fn with<R>(&self, f: impl FnOnce(&mut Doc) -> R) -> Option<R> {
        self.lock().as_mut().map(f)
    }
}

/// 从普通路径那个 `TextEdit` 的状态里把光标接过来（字 → 字节）。
fn carried_cursor(ctx: &egui::Context, path: &str, text: &str) -> usize {
    egui::TextEdit::load_state(ctx, normal_editor_id(path))
        .and_then(|s| s.cursor.char_range())
        .map_or(0, |r| {
            let k = r.primary.index.0;
            byte_at_char(text, k, false)
        })
}

/// 🔴 **编辑面那一格文字的唯一入口**（`shell.rs` 只挂这一行）。
///
/// 没进模式 ⇒ 每帧判一次（O(n)，而普通路径本来每帧就是 O(n)）；
/// 进了 ⇒ 一直留在大文件模式。两条路都占满给它的宽、高 `view_h`，内容在里面滚
/// （长文件不把编辑面撑出窗口）；光标挪出视野就跟过去。
pub fn show(ui: &mut Ui, pane: Option<&mut Pane>, view_h: f32) {
    let Some(p) = pane else { return };
    let fresh = std::mem::take(&mut p.fresh);
    let slot = p.big.clone();
    let mut g = slot.lock();
    if g.is_none() {
        if let Some(why) = judge(&p.text) {
            let at = carried_cursor(ui.ctx(), &p.path, &p.text);
            *g = Some(Doc::new(&p.text, why, at));
        }
    }
    match g.as_mut() {
        Some(doc) => doc.ui(ui, &mut p.text, view_h),
        None => {
            // 横竖两条滚动条都在编辑面里（稿 ⑤）：长行不折（行号槽与正文一行对一行），宽了横着滚。
            let mut area = ScrollArea::both()
                .id_salt(("filewin-editor-scroll", &p.path))
                .auto_shrink([false, false])
                .max_height(view_h)
                .min_scrolled_height(view_h);
            if fresh {
                area = area.scroll_offset(Vec2::ZERO);
            }
            let reveal = std::mem::take(&mut p.reveal);
            let id = normal_editor_id(&p.path);
            let faint = ui.visuals().weak_text_color();
            area.show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    // 行号槽：右对齐、淡一级，与正文同一份等宽字（一行对一行）。
                    let lines = p.text.split('\n').count();
                    let width = lines.to_string().len();
                    let gutter: String = (1..=lines)
                        .map(|n| format!("{n:>width$}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    ui.vertical(|ui| {
                        ui.add_space(2.0);
                        ui.add(
                            egui::Label::new(egui::RichText::new(gutter).monospace().color(faint))
                                .selectable(false)
                                .extend(),
                        );
                    });
                    let mut no_wrap = |ui: &Ui, buf: &dyn egui::TextBuffer, _w: f32| {
                        let font = egui::TextStyle::Monospace.resolve(ui.style());
                        let job = egui::text::LayoutJob::simple(
                            buf.as_str().to_owned(),
                            font,
                            ui.visuals().text_color(),
                            f32::INFINITY,
                        );
                        ui.fonts_mut(|f| f.layout_job(job))
                    };
                    // 当前行底色那一块先占一个位（画在字下面），控件画完拿光标那一行的位置填上。
                    let band = ui.painter().add(egui::Shape::Noop);
                    let out = egui::TextEdit::multiline(&mut p.text)
                        .id(id)
                        .desired_rows(VIEW_ROWS)
                        .desired_width(f32::INFINITY)
                        .min_size(Vec2::new(0.0, view_h))
                        .code_editor()
                        .frame(egui::Frame::NONE)
                        .layouter(&mut no_wrap)
                        .show(ui);
                    if let Some(r) = out.state.cursor.char_range() {
                        let at = out
                            .galley
                            .pos_from_cursor(r.primary)
                            .translate(out.galley_pos.to_vec2());
                        let row =
                            egui::Rect::from_x_y_ranges(out.response.rect.x_range(), at.y_range());
                        ui.painter().set(
                            band,
                            egui::Shape::rect_filled(
                                row,
                                0.0,
                                super::theme::palette(ui.ctx()).hover,
                            ),
                        );
                    }
                    // 查找 / 替换把光标挪到了视野外 ⇒ 滚过去（打字与方向键由控件自己滚）。
                    if let Some(r) = reveal.then(|| out.state.cursor.char_range()).flatten() {
                        let at = out.galley.pos_from_cursor(r.primary).translate(
                            out.galley_pos.to_vec2() - Vec2::new(out.galley.rect.left(), 0.0),
                        );
                        ui.scroll_to_rect(at, None);
                    }
                });
            });
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/bigfile_tests.rs"]
mod tests;
