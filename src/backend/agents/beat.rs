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
pub(crate) struct Beat<'a> {
    /// 这一条的 id（原样；没有 ⇒ `None`）。轮次与大纲只认非空的那种。
    pub(crate) id: Option<Cow<'a, str>>,
    /// 这一条的时刻（原样，ISO 8601）。
    pub(crate) at: Option<Cow<'a, str>>,
    pub(crate) side: Side,
    /// 用户那侧里宿主自动注入的那种（不是人这一刻发的）。
    pub(crate) injected: bool,
    /// 这一条里做了的事，文件序。
    pub(crate) acts: Vec<Act<'a>>,
    /// 这一次请求的用量（只有代理那侧写）。
    pub(crate) usage: Option<Usage<'a>>,
    /// 这一条说的此刻型号（原值）。
    pub(crate) model: Option<Cow<'a, str>>,
    /// 那一家自己记的全会话花费。
    pub(crate) cost: Option<CostSaid>,
    /// 此刻许可档（原值）。
    pub(crate) mode: Option<Cow<'a, str>>,
    /// 这份会话是从哪个会话分叉来的（源 sid）。
    pub(crate) forked_from: Option<String>,
}

/// 这一条是谁那侧写的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Side {
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
pub(crate) enum Act<'a> {
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
    Retry,
    /// 用户那侧这一条是谁说的。
    Said(UserText),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopWhy {
    /// 说完了。
    Done,
    /// 这一条本身就是报错卡。
    Failed,
}

/// 一个工具在「需手动」里是哪一种。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallAsk {
    Plain,
    /// 在等回答的（提问工具）。
    Answer,
    /// 在等批准计划的。
    Plan,
}

/// 一次请求的用量（各家照这个口径换算好）：新输入（不含缓存）· 读缓存 · 写缓存总数 · 写缓存两档（分不出档 ⇒ 都 `None`）· 输出 ·
/// 请求键（同一次请求写出几条 ⇒ 同一个键）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Usage<'a> {
    pub(crate) input: u64,
    pub(crate) cache_read: u64,
    pub(crate) cache_write: u64,
    pub(crate) write_5m: Option<u64>,
    pub(crate) write_1h: Option<u64>,
    pub(crate) output: u64,
    pub(crate) request: Option<Cow<'a, str>>,
}

/// 那一家自己记的全会话花费：微美元 · 有没有定不了价的型号（有 ⇒ 数只是下限）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CostSaid {
    pub(crate) micros: u64,
    pub(crate) partial: bool,
}

/// 快路要看的此刻状态（事实累加器给）。
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Watch {
    /// 分叉来源已经定了（不用再找）。
    pub(crate) fork_locked: bool,
    /// 有一串重试还没下文。
    pub(crate) retry_open: bool,
    /// 有没结果的工具调用。
    pub(crate) pending: bool,
    /// 有还没收场的后台命令。
    pub(crate) background: bool,
    /// 只问 MCP 那一格（扩展页「谁断开」）。
    pub(crate) mcp_only: bool,
}

/// 一行人话至多几个字（工具主参数 · 最后一段正文的头一行）。
pub(crate) const SAY_CHARS: usize = 160;

/// 头一个非空行，压成一行、至多 [`SAY_CHARS`] 字（超 ⇒ 截断加 `…`）。空 ⇒ `None`。
pub(crate) fn one_line(s: &str) -> Option<String> {
    let line = s.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut out: String = line.chars().take(SAY_CHARS).collect();
    if line.chars().count() > SAY_CHARS {
        out.push('…');
    }
    Some(out)
}

/// 子串在不在：按首字节跳着找，命中首字节再比后面（大工具输出那几行每行要问好几次，逐窗比较太慢）。快路用。
pub(crate) fn contains(hay: &[u8], needle: &[u8]) -> bool {
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
    pub(crate) fn said(&self) -> Option<&UserText> {
        self.acts.iter().find_map(|a| match a {
            Act::Said(u) => Some(u),
            _ => None,
        })
    }

    /// 一拍停在 `why` 上了没有。
    pub(crate) fn stops(&self, why: StopWhy) -> bool {
        self.acts.contains(&Act::Stop(why))
    }

    /// 一拍带着工具结果。
    pub(crate) fn has_result(&self) -> bool {
        self.acts.iter().any(|a| matches!(a, Act::Result { .. }))
    }
}
