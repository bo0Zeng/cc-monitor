//! 记录文本面：`TextFace` 一条记录的字怎么抽，与按家问它的那几个入口。

use super::*;

/// 一家的记录文本面：函数指针（同 [`Adapter::home`]，不立 trait）。
/// 通用层（全局搜索 · 会话内查找 · 历史摘录 · 用户输入列表 · 骨架索引）经 [`main_text`] · [`tool_text`] · [`user_text_of`] · [`human_speech`] 够它，不按名字够。
#[derive(Clone, Copy)]
pub(crate) struct TextFace {
    /// 一条已解析的记录 ⇒ 正文文本块（正文住哪一格是那一家的格式知识）。
    pub(crate) main: fn(&serde_json::Value) -> String,
    /// 同上 ⇒ 工具内容（`is_assistant`：工具入参 · 思考；否则：工具结果）。
    pub(crate) tool: fn(&serde_json::Value, bool) -> String,
    /// 一条已解析的记录 ⇒ 它是谁说的（[`record::Heard`]：线上那一形 ＋ 来话的正文原文）；不是用户角色的记录 ⇒ `None`。
    pub(crate) user: fn(&serde_json::Value) -> Option<record::Heard>,
    /// 一条已解析的记录在消息流里落在哪张卡上（[`Lands`]）：查找 · 搜索的单位就是流里画的那几张卡。
    pub(crate) lands: fn(&serde_json::Value) -> Lands,
    /// 一行原文（已去掉换行）⇒ 它说的会话级那几格（[`SessionLine`]）；整行解不成 ⇒ `None`。历史清单那一行的扫描只经它读原文。
    pub(crate) line: fn(&str) -> Option<SessionLine>,
    /// 已解析好的一条 ⇒ 同上那几格（全文搜索索引那一路）。
    pub(crate) line_of: fn(&serde_json::Value) -> SessionLine,
    /// 一条记录正文的形状（[`BodyShape`]，骨架索引粗估高度用）；没有正文那一格 ⇒ `None`。
    pub(crate) shape: fn(&serde_json::Value) -> Option<BodyShape<'_>>,
}

/// 一行会话记录里会话级的那几格（[`TextFace::line`]，那一家的字段名只在适配层读）：清单行与搜索索引同一份，累计在 `history::listing_scan::SessionScan`。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionLine {
    /// 这一条是人那一侧的一句 / 回话；别的记录（标题 · 元数据）⇒ `None`。
    pub role: Option<LineRole>,
    /// 它是一条标题记录 ⇒ 那个标题与谁起的（收不收由通用层 `history::search_rules::names_session` 一处判）。
    pub title: Option<(String, record::TitleBy)>,
    /// 它说这是后台分身会话。
    pub bg: bool,
    /// 记录时刻（毫秒；只有人说的 / 回话那两类才给）。
    pub at_ms: Option<i64>,
    /// 这一条的消息 id（只有人说的 / 回话那两类才给；空串当没有）。搜索命中 · 会话内查找的落点照它跳。
    pub id: Option<String>,
    /// 它写着的工作目录（空串当没有）。
    pub cwd: Option<String>,
    /// 分叉来源（源会话 id, 分叉处的消息 id；只有人说的 / 回话那两类才给）。
    pub forked: Option<(String, String)>,
}

/// [`SessionLine::role`]：人那一侧的一句（首句摘要从这里取）· 回话。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineRole {
    Said,
    Replied,
}

/// 一条记录正文的形状（[`TextFace::shape`]）：正文那几段的字（借着原文）· 折叠单元几个（工具调用 · 工具结果 · 思考 · 图片各一个）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BodyShape<'a> {
    pub prose: Vec<&'a str>,
    pub folded: u32,
}

/// `kind` 那一家：一行原文的会话级那几格（[`TextFace::line`]）。那一家没有文本面 / 解不成 ⇒ `None`。
pub fn session_line(kind: &str, text: &str) -> Option<SessionLine> {
    text_face(kind).and_then(|t| (t.line)(text))
}

/// `kind` 那一家：已解析好的一条的会话级那几格（[`TextFace::line_of`]）。那一家没有文本面 ⇒ 全空。
pub fn session_line_of(kind: &str, v: &serde_json::Value) -> SessionLine {
    text_face(kind).map(|t| (t.line_of)(v)).unwrap_or_default()
}

/// `kind` 那一家：一条记录正文的形状（[`TextFace::shape`]）。那一家没有文本面 / 没有正文 ⇒ `None`。
pub fn body_shape<'a>(kind: &str, v: &'a serde_json::Value) -> Option<BodyShape<'a>> {
    text_face(kind).and_then(|t| (t.shape)(v))
}

/// 一条记录在消息流里落在哪（[`TextFace::lands`]，判法是各家的格式知识；界面照它跳、不自己推）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lands {
    /// 自己那张卡。
    Own,
    /// 流里不画它（代理那一侧自动写的应答之类）⇒ 不搜、不列。
    Nowhere,
    /// 收在这次工具调用（调用 id）画成的那一块里：工具结果流里不自己成卡（派出子运行的结果收在派出卡里）。
    InTool(String),
}

/// `kind` 那一家的文本面。
fn text_face(kind: &str) -> Option<TextFace> {
    record_face(kind).and_then(|r| r.text)
}

/// `kind` 那一家怎么抽一条记录的正文 —— 通用层够它的唯一入口。那一家答不了 ⇒ 空串（不搜、不摘）。
pub fn main_text(kind: &str, v: &serde_json::Value) -> String {
    text_face(kind).map_or_else(String::new, |t| (t.main)(v))
}

/// `kind` 那一家怎么抽一条记录的工具内容。那一家答不了 ⇒ 空串。
pub fn tool_text(kind: &str, v: &serde_json::Value, is_assistant: bool) -> String {
    text_face(kind).map_or_else(String::new, |t| (t.tool)(v, is_assistant))
}

/// `kind` 那一家判这条记录是谁说的。那一家答不了 ⇒ `None`。
pub fn user_text_of(kind: &str, v: &serde_json::Value) -> Option<record::Heard> {
    text_face(kind).and_then(|t| (t.user)(v))
}

/// `kind` 那一家说这条记录在流里落在哪。那一家答不了 ⇒ 自己那张卡。
pub fn lands_of(kind: &str, v: &serde_json::Value) -> Lands {
    text_face(kind).map_or(Lands::Own, |t| (t.lands)(v))
}

/// 人在这条记录里说的话（[`UserText::speech`]）；不是人说的 ⇒ `None`。
pub fn human_speech(kind: &str, v: &serde_json::Value) -> Option<String> {
    user_text_of(kind, v)?.who.speech()
}
