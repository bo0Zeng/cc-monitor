//! 一段助手正文解析成的块树：后端解析一次（`md_core::parse`），桌面与手机只照它排版。
//!
//! 形状照两端真画的那十几种，不交整份 Markdown 语法树；颜色不上线，代码高亮只交类（[`HlClass`] 的序号），
//! 两端照各自主题的色票上色。偏移一律按 UTF-16 码元（JS 与 Kotlin 的字符串都是 UTF-16，拿来就能切）。

use serde::{Deserialize, Serialize};

/// 一段正文的块树（代码块不带高亮）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Doc {
    pub blocks: Vec<MdBlock>,
    /// 长正文分段：第 i 段（i ≥ 1）从 `blocks[segs[i-1]]` 起；只有一段时为空。
    pub segs: Vec<u32>,
}

/// 一段的代码高亮（拉一段那一下的回包；高亮只走这一条路，树里的代码块不带）：
/// 那一段里每个代码块（顶层与嵌在列表 · 引用 · 提示块 · 脚注里的，先序）各一份 `hl`，
/// 平铺的三元组 `[起点差, 长, 类]…`；没写语言、语言认不出 ⇒ 空。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SegHl {
    pub hl: Vec<Vec<u32>>,
}

/// `prose-hl` 的入参：一段里每个代码块（先序，同 [`SegHl`] 的数法）的语言与原文；回包的 `hl[n]` 对 `codes[n]`。
/// 按内容拉、不按记录行：两端手上本来就有整棵树，不在记录行里的正文（计划卡等）也走这一条。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct ProseHlReq {
    pub codes: Vec<HlCode>,
}

/// 一个代码块：`lang` 照树上 [`MdBlock::Code`] 的那一格（没写 ⇒ 空），`x` 是原文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct HlCode {
    pub lang: Option<String>,
    pub x: String,
}

/// 一个块。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum MdBlock {
    /// 段落。
    P {
        k: Vec<Inl>,
    },
    /// 标题 1–6。
    H {
        level: u8,
        k: Vec<Inl>,
    },
    /// 代码块：`lang` 是归一过的语言名（没写 ⇒ 空）；`x` 是代码原文（不带末尾换行）；
    /// `cont` ⇒ 接着上一块（超长代码切开的后几块）。高亮不在树上，按段另取（每块一份 `[起点差, 长, 类]…`，
    /// 起点差相对上一段的末尾，类是 [`HlClass`] 的序号，没高亮的地方不出段）。
    Code {
        lang: Option<String>,
        x: String,
        cont: bool,
    },
    /// 块级公式：`tex` 是原文（复制 · 朗读用），`m` 是核心排好的画单；排不出 ⇒ 空，两端照原文等宽排。
    Math {
        tex: String,
        m: Option<MathBox>,
    },
    /// 列表：`start` 有 ⇒ 有序、从它起编号。
    List {
        start: Option<u32>,
        tight: bool,
        items: Vec<MdItem>,
    },
    Quote {
        blocks: Vec<MdBlock>,
    },
    /// `> [!NOTE]` 那五种提示块。
    Alert {
        kind: AlertKind,
        blocks: Vec<MdBlock>,
    },
    Table {
        align: Vec<MdAlign>,
        head: Vec<Vec<Inl>>,
        rows: Vec<Vec<Vec<Inl>>>,
    },
    Rule,
    /// 脚注，收在末尾一块。
    Notes {
        items: Vec<MdNote>,
    },
}

/// 列表的一项：`task` 有 ⇒ 任务项（真 ⇒ 勾上）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct MdItem {
    pub task: Option<bool>,
    pub blocks: Vec<MdBlock>,
}

/// 一条脚注：`n` 与正文里 [`InlNode::Fn`] 的 `n` 对得上。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct MdNote {
    pub n: u32,
    pub blocks: Vec<MdBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

/// 表格一列的对齐（`none` ⇒ 没写）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum MdAlign {
    None,
    Left,
    Center,
    Right,
}

/// 行内：纯字直接是字符串（占绝大多数，省掉标签），别的带 `t`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum Inl {
    S(String),
    N(InlNode),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum InlNode {
    B {
        k: Vec<Inl>,
    },
    I {
        k: Vec<Inl>,
    },
    Del {
        k: Vec<Inl>,
    },
    /// 行内代码。
    C {
        x: String,
    },
    /// 行内公式：同 [`MdBlock::Math`]，按行内的样式排（更矮、上下标更紧）。
    M {
        tex: String,
        m: Option<MathBox>,
    },
    /// 链接：只有放行的 scheme（http · https · mailto）才成链接。
    A {
        href: String,
        k: Vec<Inl>,
    },
    Img {
        src: String,
        alt: String,
    },
    Br,
    /// 脚注引用。
    Fn {
        n: u32,
    },
    /// 计划格引用：正文里写着的那串编号（`x`，原样）指着 `ids` 那几格；`label` 是那几格的标题照先后接好的一串（后端照 pb 写好；
    /// 空 ⇒ 由界面照 `ids` 写，计划页里那一种）。只由后端认出引用之后插进树（`md_core::with_refs` · `md_core::parse_marked`），解析本身不出它。
    Ref {
        x: String,
        ids: Vec<String>,
        label: String,
    },
}

/// 一条排好的公式（核心排，两端只照着画）。几何一律是千分之一 em 的整数，乘上那一处的字号（÷ 1000）就是像素；
/// 原点在左上、y 朝下，基线在 `y = h`；`w` 宽、`h` 基线以上、`d` 基线以下。颜色不上线，跟正文色。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct MathBox {
    pub w: i32,
    pub h: i32,
    pub d: i32,
    pub items: Vec<MathItem>,
}

/// 画单里的一笔。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum MathItem {
    /// 一个字形：字体 `f` 里码位 `u` 那个字，基线左端在 (`x`, `y`)，字号乘 `s` ‰（上下标更小）。
    G {
        f: MathFont,
        u: u32,
        x: i32,
        y: i32,
        s: i32,
    },
    /// 一条横线（分数线 · 上划线 · 下划线）：左端 (`x`, `y`)，`y` 是线的中线，粗 `th`；`dash` ⇒ 虚线。
    L {
        x: i32,
        y: i32,
        w: i32,
        th: i32,
        dash: bool,
    },
    /// 实心矩形：左上 (`x`, `y`)。
    R {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    },
    /// 一条路径（伸缩的根号 · 括号 · 箭头）：命令里的坐标相对 (`x`, `y`)；`fill` ⇒ 填，否则描边。
    P {
        x: i32,
        y: i32,
        fill: bool,
        d: Vec<MathPath>,
    },
}

/// 路径命令。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum MathPath {
    M { x: i32, y: i32 },
    L { x: i32, y: i32 },
    C { x1: i32, y1: i32, x2: i32, y2: i32, x: i32, y: i32 },
    Q { x1: i32, y1: i32, x: i32, y: i32 },
    Z,
}

/// 画字形用的字体：KaTeX 那一套（两端各自带同一份字体文件，`src/shared/fonts/katex/`）；
/// `Sys` ⇒ KaTeX 里没有的字（汉字 · 表情），用系统字体。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum MathFont {
    Ams,
    CaligraphicRegular,
    FrakturRegular,
    FrakturBold,
    MainRegular,
    MainBold,
    MainItalic,
    MainBoldItalic,
    MathItalic,
    MathBoldItalic,
    SansSerifRegular,
    SansSerifBold,
    SansSerifItalic,
    ScriptRegular,
    Size1,
    Size2,
    Size3,
    Size4,
    Typewriter,
    Sys,
}

/// 代码高亮的类：高亮三元组里第三格是这里的序号（`as u32`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum HlClass {
    Keyword = 0,
    String = 1,
    Comment = 2,
    Number = 3,
    Function = 4,
    Type = 5,
    Constant = 6,
    Variable = 7,
    Attribute = 8,
    Tag = 9,
    Inserted = 10,
    Deleted = 11,
    Meta = 12,
}
