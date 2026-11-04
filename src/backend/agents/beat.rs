//! 一条记录的「一拍」：通用层（轮次 · 会话事实 · 大纲）要问一条记录的那几句，各家适配层照自己的格式答（[`BeatFace`]）。
//!
//! 字段都是「通用层要问的那一句」，不是哪一家的格名；判别规则（划段 · 合计 · 重试的结局 · 近因序）只在 observe 那一处，
//! 这里只翻格式。子运行 · 派出与收场 · 后台命令 · MCP · 认不出的簿记行 · 排队插话已经有各自的通用形（`RecordFace` 那几格），
//! 不收进这里。

use std::borrow::Cow;

use super::UserText;

/// 一家怎么读出一条记录的一拍（注册表 `RecordFace.beats` 那一格）。
#[derive(Clone, Copy)]
pub(crate) struct BeatFace {
    /// 一条已解析的记录 ⇒ 它的一拍；这一家不认的记录 ⇒ `None`（通用层当它什么都没说）。
    pub(crate) beat: for<'a> fn(&'a serde_json::Value) -> Option<Beat<'a>>,
    /// 快路：这一行（原文，不含结尾换行）有没有可能改动会话事实。**只许放过、不许误拦**：凡是会改动事实的行必为真。
    /// 只认这一家的格式那一半；「行里带着某个还挂着的调用 id」那一半是通用的，由 observe 自己看。
    pub(crate) hint: fn(line: &[u8], watch: &Watch) -> bool,
    /// 一个工具名在「需手动」里是哪一种（提问 · 批准计划 · 别的）。
    pub(crate) ask: fn(tool: &str) -> CallAsk,
}

/// 一条记录的一拍。
#[derive(Debug, Clone, PartialEq)]
pub struct Beat<'a> {
    /// 这一条的 id（原样；没有 ⇒ `None`）。轮次与大纲只认非空的那种。
    pub id: Option<Cow<'a, str>>,
    /// 这一条的时刻（原样，ISO 8601）。
    pub at: Option<Cow<'a, str>>,
    pub side: Side,
    /// 用户那侧里宿主自动注入的那种（不是人这一刻发的）。
    pub injected: bool,
    /// 这一条里做了的事，文件序。
    pub acts: Vec<Act<'a>>,
    /// 这一次请求的用量（只有代理那侧写）。
    pub usage: Option<Usage<'a>>,
    /// 这一条说的此刻型号（原值）。
    pub model: Option<Cow<'a, str>>,
    /// 那一家自己记的全会话花费。
    pub cost: Option<CostSaid>,
    /// 此刻许可档（原值）。
    pub mode: Option<Cow<'a, str>>,
    /// 这份会话是从哪个会话分叉来的（源 sid）。
    pub forked_from: Option<String>,
}

/// 这一条是谁那侧写的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Side {
    /// 代理那侧（回复 · 工具调用 · 思考）。
    Reply,
    /// 用户角色那侧（人话 · 工具结果 · 通知 · 注入）。
    Input,
    /// 簿记（不在对话里的那些）。
    #[default]
    Note,
}

/// 一条里的一件事（闭集）。
#[derive(Debug, Clone, PartialEq)]
pub enum Act<'a> {
    /// 调了一次工具：调用 id（没有 ⇒ 空串）· 工具名 · 主参数一行（那一家的工具词表给）· 它写了哪个文件（写类工具才有）。
    Call {
        id: Cow<'a, str>,
        name: Cow<'a, str>,
        what: Option<String>,
        edits: Option<Cow<'a, str>>,
    },
    /// 一次工具调用的结果：对的是哪次调用（没说 ⇒ `None`）· 失败了（出错且不是人拒的）。
    Result {
        of: Option<Cow<'a, str>>,
        failed: bool,
    },
    Thinking,
    /// 正文一段（原文）。
    Text(Cow<'a, str>),
    Image,
    /// 这一轮停在这里。
    Stop(StopWhy),
    /// 一次要重试的上游失败（一串里的一条）。
    Retry(RetrySaid),
    /// 用户那侧这一条是谁说的。
    Said(UserText),
}

/// 一次重试那一条说了什么：原因种类 · 第几次 · 最多几次（那一家没写 ⇒ 缺）。会话事实那一串照它写整句。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetrySaid {
    pub reason: crate::agents::ApiReason,
    pub attempt: Option<u32>,
    pub max: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopWhy {
    /// 说完了。
    Done,
    /// 这一条本身就是报错卡。
    Failed,
}

/// 一个工具在「需手动」里是哪一种。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallAsk {
    Plain,
    /// 在等回答的（提问工具）。
    Answer,
    /// 在等批准计划的。
    Plan,
}

/// 一次请求的用量（各家照这个口径换算好）：新输入（不含缓存）· 读缓存 · 写缓存总数 · 写缓存两档（分不出档 ⇒ 都 `None`）· 输出 ·
/// 请求键（同一次请求写出几条 ⇒ 同一个键）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Usage<'a> {
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub write_5m: Option<u64>,
    pub write_1h: Option<u64>,
    pub output: u64,
    pub request: Option<Cow<'a, str>>,
}

/// 那一家自己记的全会话花费：微美元 · 有没有定不了价的型号（有 ⇒ 数只是下限）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostSaid {
    pub micros: u64,
    pub partial: bool,
}

/// 快路要看的此刻状态（事实累加器给）。
#[derive(Debug, Clone, Copy, Default)]
pub struct Watch {
    /// 分叉来源已经定了（不用再找）。
    pub fork_locked: bool,
    /// 有一串重试还没下文。
    pub retry_open: bool,
    /// 有没结果的工具调用。
    pub pending: bool,
    /// 有还没收场的后台命令。
    pub background: bool,
    /// 只问 MCP 那一格（扩展页「谁断开」）。
    pub mcp_only: bool,
}

/// 一行人话至多几个字（工具主参数 · 最后一段正文的头一行）。
pub const SAY_CHARS: usize = 160;

/// 只做准备、说不出用意的那几个打头的词（换目录 · 设变量 · 设 shell 选项 · 读进环境 · 空命令）。
const SETUP_WORDS: &[&str] = &[
    "cd", "pushd", "popd", "export", "unset", "set", "shopt", "umask", "local", "readonly",
    "declare", "typeset", "source", ".", ":", "true",
];

/// 一条 shell 命令里说得出用意的那一段起到末尾（需手动 · 钉条摆的「批什么」）：按顶层的 `;` · `&&` · `||` · `|` · `&` · 换行切段
/// （引号 · `$(…)` · 反引号里的 · 反斜杠转义的 · `2>&1` `&>` 这类重定向不切），跳过只做准备的段（纯赋值 `F=…` · [`SETUP_WORDS`] 打头 · 注释），
/// 段首的环境赋值（`LC_ALL=C sort`）也跳过。全是准备段 ⇒ 用意说不出、值也不摆：第一个词加「…」（[`setup_head`]）。
pub(crate) fn command_intent(cmd: &str) -> Cow<'_, str> {
    let b = cmd.as_bytes();
    let (mut i, mut start) = (0, 0);
    let cut = |from: usize, to: usize| intent_in(&cmd[from..to]).map(|off| &cmd[from + off..]);
    while i < b.len() {
        let c = b[i];
        if let Some(end) = quoted_end(b, i) {
            i = end;
            continue;
        }
        match c {
            b'\\' => i += 1,
            b'#' if i == start || b[i - 1].is_ascii_whitespace() => {
                // 注释：到行尾都不算。
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'&' if (i > 0 && b[i - 1] == b'>') || b.get(i + 1) == Some(&b'>') => {}
            b';' | b'\n' | b'|' | b'&' => {
                if let Some(s) = cut(start, i) {
                    return Cow::Borrowed(s.trim_end());
                }
                if matches!(c, b'|' | b'&') && b.get(i + 1) == Some(&c) {
                    i += 1;
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    match cut(start.min(b.len()), b.len()) {
        Some(s) => Cow::Borrowed(s.trim_end()),
        None => setup_head(cmd).map_or(Cow::Borrowed(cmd), Cow::Owned),
    }
}

/// 全是准备段的命令摆成什么：第一个词（注释跳过）加「…」，赋值词只留到 `=`（`F=…`）；一个词都没有 ⇒ `None`。
fn setup_head(cmd: &str) -> Option<String> {
    let b = cmd.as_bytes();
    let mut i = 0;
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            return None;
        }
        if b[i] != b'#' {
            break;
        }
        while i < b.len() && b[i] != b'\n' {
            i += 1;
        }
    }
    let from = i;
    while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b';' | b'&' | b'|') {
        if let Some(end) = quoted_end(b, i) {
            i = end;
        } else {
            i += if b[i] == b'\\' { 2 } else { 1 };
        }
    }
    let mut end = i.min(b.len());
    while !cmd.is_char_boundary(end) {
        end += 1;
    }
    let word = &cmd[from..end];
    Some(match word.split_once('=') {
        Some((name, _)) if shell_quote_core::is_assignment(word) => format!("{name}=…"),
        _ => format!("{word} …"),
    })
}

/// `b[i]` 起是一段引起来的（`'…'` · `"…"` · `` `…` `` · `$(…)`，后三样里可以再套，`$(…)` 里认括号的层数）⇒ 它收尾之后的位置
/// （没收尾 ⇒ 末尾）；不是 ⇒ `None`。单引号里什么都不认；别的里面反斜杠转义下一个字节。
fn quoted_end(b: &[u8], i: usize) -> Option<usize> {
    let (close, mut j) = match (b[i], b.get(i + 1)) {
        (b'\'', _) => {
            return Some(
                b[i + 1..]
                    .iter()
                    .position(|&c| c == b'\'')
                    .map_or(b.len(), |p| i + p + 2),
            )
        }
        (b'"' | b'`', _) => (b[i], i + 1),
        (b'$', Some(b'(')) => (b')', i + 2),
        _ => return None,
    };
    let mut depth = 0usize;
    while j < b.len() {
        let c = b[j];
        if c == b'\\' {
            j += 2;
            continue;
        }
        if c == close && depth == 0 {
            return Some(j + 1);
        }
        // 双引号里的单引号是字面；反引号里不再套。
        let nests = match close {
            b'`' => false,
            b'"' => c != b'\'',
            _ => true,
        };
        if let Some(end) = nests.then(|| quoted_end(b, j)).flatten() {
            j = end;
            continue;
        }
        if close == b')' {
            match c {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
        }
        j += 1;
    }
    Some(b.len())
}

/// 一段里用意从哪起（相对这一段的字节位置）：跳过段首的赋值词，剩下第一个词是准备词 · 没有词 ⇒ `None`。
fn intent_in(seg: &str) -> Option<usize> {
    let b = seg.as_bytes();
    let mut i = 0;
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] == b'#' {
            return None;
        }
        let from = i;
        while i < b.len() && !b[i].is_ascii_whitespace() {
            if let Some(end) = quoted_end(b, i) {
                i = end;
            } else {
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
        }
        let word = &seg[from..i.min(b.len())];
        if shell_quote_core::is_assignment(word) {
            continue;
        }
        return (!SETUP_WORDS.contains(&word)).then_some(from);
    }
}

/// 头一个非空行，压成一行、至多 [`SAY_CHARS`] 字（超 ⇒ 截断加 `…`）。空 ⇒ `None`。
pub fn one_line(s: &str) -> Option<String> {
    copy_core::first_line(s).map(|l| copy_core::clip(l, SAY_CHARS))
}

/// 一段思考的块：收着时那一行（`preview`）与「一行人话」同口径（[`one_line`]）；空正文 ⇒ 空串。两家适配层只经它建。
pub fn thinking(text: String) -> crate::agents::record::Block {
    let preview = one_line(&text).unwrap_or_default();
    crate::agents::record::Block::Thinking { text, preview }
}

/// 子串在不在：按首字节跳着找，命中首字节再比后面（大工具输出那几行每行要问好几次，逐窗比较太慢）。快路用。
pub fn contains(hay: &[u8], needle: &[u8]) -> bool {
    let Some((&first, rest)) = needle.split_first() else {
        return true;
    };
    let mut from = 0;
    while let Some(p) = hay[from..].iter().position(|&b| b == first) {
        let at = from + p;
        if hay[at + 1..].starts_with(rest) {
            return true;
        }
        from = at + 1;
    }
    false
}

impl Beat<'_> {
    /// 一拍里「谁说的」那一件。
    pub fn said(&self) -> Option<&UserText> {
        self.acts.iter().find_map(|a| match a {
            Act::Said(u) => Some(u),
            _ => None,
        })
    }

    /// 一拍停在 `why` 上了没有。
    pub fn stops(&self, why: StopWhy) -> bool {
        self.acts.contains(&Act::Stop(why))
    }

    /// 一拍带着工具结果。
    pub fn has_result(&self) -> bool {
        self.acts.iter().any(|a| matches!(a, Act::Result { .. }))
    }
}
