//! **通用记录**的线上形（`history-read` · `history-lines` 的回包连同记录本身）：两侧同一份 —— 后端各家的适配层把一行翻成这里的
//! [`Record`]（判定与写字在后端 `agents/`），手机嵌的 Rust 核心按这里的类型解。
//!
//! 公共四格 ＋ 时刻字：`agent`（哪一家）· `id`（会话内非空、唯一）· `at`（记录时刻）· `timeText`（看的那一台钟上的 `HH:MM`，出口那一下写，界面照抄）· `t`（哪一类）。
//! `t` 的闭集：`said` · `reply` · `retry` · `title` · `queued` · `unread`；内容块 [`Block`] 是两家共有的词。
//!
//! 回包里的那一份记录后端照原样的一团写（出口的声明可能去过格），所以 [`ReadRow`] · [`RecordLine`] 及其回包对记录那一格是泛型：
//! 后端写 `Value`（缺省），手机按完整记录 [`Record`] 一条一条地解（`replies` 表里登记的是 `HistoryRead<OrUnread>`：解不开的那一条成认不出那一条，整份不失败）；TS 一侧照 `Record` 出（`ts(concrete)`）。

use crate::cells::{Tone, Words};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// 一条通用记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "LineRecord",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub struct Record {
    /// 哪一家（注册表里那一家的 `kind`）。
    pub agent: String,
    /// 这条记录在本会话里的身份：非空、会话内唯一（主线外清单、分叉、跳转都按它认）。
    pub id: String,
    /// 记录时刻（ISO-8601 原样）；记录里没有 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub at: Option<String>,
    /// `at` 在看的那一台钟上的钟面 `HH:MM`（出口那一下按请求 / 流带来的时区写，[`Record::stamp`]；界面照抄、不换算）；没有时刻 / 解不出 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<String>"))]
    pub time_text: Option<Words>,
    /// `at` 的毫秒（自 1970；出口那一下随钟面一起写）：界面算「两条记录之间多久」只用这一格（交那一个读口），不自己解析 `at`；解不出 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub at_ms: Option<i64>,
    /// 哪一类与它自己的格。
    #[serde(flatten)]
    pub body: Body,
}

/// [`Record`] 的类别（`t`）与各自的格。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "LineRecordBody",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub enum Body {
    /// 人那一侧说的一条（人打的字 · 斜杠命令 · 工具结果 · 系统注入 … 谁说的由 `who` 说）。
    #[serde(rename_all = "camelCase")]
    Said {
        /// 谁说的、要显示的正文（判定只在适配层）。
        who: UserText,
        blocks: Vec<Block>,
        /// 这条里每个工具结果的一句（工具调用 id ⇒ [`StepResult`]）。没有 ⇒ 缺。
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        #[cfg_attr(
            any(test, feature = "ts"),
            ts(optional, as = "Option<BTreeMap<String, StepResult>>")
        )]
        results: BTreeMap<String, StepResult>,
        /// 这条记录自己的工作目录。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        cwd: Option<String>,
    },
    /// 代理的回复。
    #[serde(rename_all = "camelCase")]
    Reply {
        blocks: Vec<Block>,
        /// 型号名原样。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        model: Option<String>,
        /// 代理那一侧自动写的应答（不是模型说的，不建卡）。
        auto_reply: bool,
        /// 工具调用 id ⇒ 卡型（普通工具卡不列）。
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        #[cfg_attr(
            any(test, feature = "ts"),
            ts(optional, as = "Option<BTreeMap<String, ToolCard>>")
        )]
        cards: BTreeMap<String, ToolCard>,
        /// 工具调用 id ⇒ 一行人话。
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        #[cfg_attr(
            any(test, feature = "ts"),
            ts(optional, as = "Option<BTreeMap<String, ToolStep>>")
        )]
        steps: BTreeMap<String, ToolStep>,
        /// 派出子运行的那几次工具调用 ⇒ 标签与类别。
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        #[cfg_attr(
            any(test, feature = "ts"),
            ts(optional, as = "Option<BTreeMap<String, ChildRunTag>>")
        )]
        runs: BTreeMap<String, ChildRunTag>,
        /// 这条是上游最终失败写的报错（报错正文在 `blocks` 的正文里）。不是 ⇒ 缺。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        error: Option<ReplyError>,
    },
    /// 一次上游失败、将重试（「重试中 N/M」那条细条）。
    Retry {
        reason: ApiReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        attempt: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        max: Option<u32>,
    },
    /// 会话标题（代理起的 · 人改的）。
    Title { text: String, by: TitleBy },
    /// 人在一轮跑着时插进去的一句（`at` 是打字的时刻）。
    Queued { who: UserText },
    /// 这一家的这一行，适配层认不出（没见过的类型 · 见过的类型、形状变了）。出口照 `text` 画一行，不按 `type` 分支。
    /// 相邻同类由出记录页那一遍并成一条（[`unread_text`] 重写 `text`、`count` 记几条）。
    Unread {
        why: UnreadWhy,
        /// 那一行自己说的类型原样（标签，出口不许按它取字）；没有 ⇒ `null`。
        #[serde(rename = "type")]
        kind: Option<String>,
        /// 写好的一句（[`unread_text`]）。
        #[cfg_attr(any(test, feature = "ts"), ts(as = "String"))]
        text: Words,
        /// 恒 `warn`。
        #[cfg_attr(any(test, feature = "ts"), ts(type = "\"warn\""))]
        tone: Tone,
        /// 原文截到 [`EXCERPT_BYTES`]（按字符截，不截半个字）：给［复制详情］，出口照抄、不解析。
        #[cfg_attr(any(test, feature = "ts"), ts(as = "String"))]
        excerpt: Words,
        /// 并了几条（适配层每行给 1）。
        count: u32,
    },
}

/// 认不出那一行的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum UnreadWhy {
    /// 这一家没见过的类型。
    UnknownType,
    /// 见过的类型、形状变了（更值得警惕）。
    ParseFailed,
}

/// 报错回复的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ReplyError {
    pub reason: ApiReason,
    /// HTTP 状态码（只有 HTTP 类错误有）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub status: Option<u32>,
}

/// 标题是谁起的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum TitleBy {
    Agent,
    User,
}

/// 内容块（两家共有的词）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum Block {
    Text {
        text: String,
    },
    Thinking {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        #[cfg_attr(any(test, feature = "ts"), ts(type = "unknown"))]
        input: serde_json::Value,
    },
    ToolResult {
        /// 对的是哪次工具调用。
        #[serde(rename = "for")]
        of: String,
        /// 结果内容（正文 / 图片块；原文是一个字符串时已归成一个正文块）。
        content: Vec<Block>,
        #[serde(rename = "isError")]
        is_error: bool,
    },
    Image {
        /// 图片来源原样（形状照那一家的写法透传）。
        #[cfg_attr(any(test, feature = "ts"), ts(type = "unknown"))]
        source: serde_json::Value,
    },
}

/// 一个 `tool_use` 在界面上画成哪一种卡 —— **通用的值域**；哪个工具名算哪一种是各家的格式知识（`agents/<名>/`，
/// 注册表 [`RecordFace::tool_card`]）。没有这一格 ＝ 普通工具卡。随记录成品带出（`JsonlRecord::Assistant` 的 `toolCards`），
/// 界面只按它画、不认工具名（判定只在后端 · `§2.1` 加一个 agent 只改 `agents/`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "lowercase")]
pub enum ToolCard {
    /// 展开 ＝ 子会话（折叠卡里嵌着渲染子 agent 的记录）。
    Agent,
    /// agent 在等用户决定（默认展开，不收成过程里的一步）。
    Interactive,
    /// 写类工具：展开按行级 diff 画（[`StepBody::Changes`]）。
    Diff,
    /// 结果默认按 Markdown 画。
    Md,
    /// 跑一行命令的工具：展开画命令本身（[`StepBody::Command`]，说明在 [`ToolStep::note`]），不画入参 JSON。
    Command,
}

/// 记录成品里「这次工具调用派出了一个子运行」的那一格（父侧工具调用 id ⇒ 它）：界面按它给那张工具卡起名，不认工具名与入参。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ChildRunTag {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub kind: Option<String>,
}

/// 过程里一步的「一行人话」与展开要画的东西：assistant 记录成品的 `toolSteps`（`tool_use.id` ⇒ 它）。
/// 怎么从入参里读出这几格是各家的格式知识（`agents/<名>/steps.rs`）；界面只按这几格画，入参只当原文整份显示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ToolStep {
    /// 工具名（原样）。
    pub tool: String,
    /// 主参数（命令 · 路径 · 搜索词 · 网址 · 任务说明）：一行（换行压成空格），至多 200 字（按字符），截了以「…」收尾；界面不再截。认不出主参数 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub arg: Option<String>,
    /// 主参数是不是路径（界面「中间省略」与「打开文件」按它）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub path: bool,
    /// 说明（命令那一句注一类）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub note: Option<String>,
    /// 这一步说的那份文件：绝对路径原样（界面「在文件窗口里定位」按它）；不是路径 · 相对路径 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub file: Option<String>,
    /// 展开那一下画什么（改动 · 命令）；没有 ⇒ 界面照原文画入参。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub body: Option<StepBody>,
    /// 这一家认不认得这个工具（`false` ⇒ 界面画问号、给原文）。
    pub known: bool,
    /// 认不出的工具：结果到了之后右侧那一句（「未识别结果」）；认得的 ⇒ 缺（照结果那一句）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "string"))]
    pub text: Option<Words>,
    /// 提问 / 计划那几张卡要画的东西（从入参里读出来的，各家的格式知识）：界面只按这一格建卡，不认入参结构。
    /// 不是这两类、或入参形状不对 ⇒ 缺（界面画普通工具卡）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub ask: Option<StepAsk>,
}

/// 一步展开要画的东西（[`ToolStep::body`]）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StepBody {
    /// 改了一份文件的哪几处（新写整份 ⇒ 一处、改前为空）。
    Changes { changes: Vec<TextChange> },
    /// 跑的那一行命令原样（多行、不截）。
    Command { line: String },
}

/// 一处改动：改前 · 改后的原文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct TextChange {
    pub old: String,
    pub new: String,
}

/// 提问 / 计划卡的内容（[`ToolStep::ask`]）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StepAsk {
    /// 在问人：一道或几道题，每道题几个选项。
    Questions { questions: Vec<AskQuestion> },
    /// 交一份计划等人批（正文是 Markdown 原文）＋ 卡片标题那三态写好的字。
    Plan { text: String, title: PlanTitle },
}

/// 计划卡的标题（[`StepAsk::Plan`]）：三态都由核心写好、状态格打头（同钉条第一行「等批准 · 计划」）；
/// 界面按这一步的结果挑一句照抄（还没结果 ⇒ `waiting`；批了 ⇒ `approved`；没批 ⇒ `kept`），不另写字。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanTitle {
    /// 等着批。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub waiting: Words,
    /// 批了。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub approved: Words,
    /// 没批（接着规划）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub kept: Words,
}

/// 一道题（[`StepAsk::Questions`]）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct AskQuestion {
    /// 题前那一小格（原文）。没有 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub header: Option<String>,
    /// 题目（原文）。
    pub question: String,
    /// 可以选几项（`true` ＝ 多选）。
    pub multi: bool,
    pub options: Vec<AskOption>,
}

/// 一个选项（原文）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct AskOption {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub description: Option<String>,
}

/// 过程里一步的结果一句（B5 后一半 ＋ B7）：user 记录成品的 `toolResults`（`tool_result.tool_use_id` ⇒ 它）。
/// 数是从结果里读的（各家的格式知识），界面只按这几格拼字；读不出的格缺。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct StepResult {
    /// 成功（`false` ＝ 失败或被拒）。
    pub ok: bool,
    /// 人没批准这一步（批准框里选了「不」/ 计划没批）。只在 `ok == false` 时可能为真。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub rejected: bool,
    /// 读了几行（读文件）/ 输出几行（命令）/ 命中几行（搜索）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub lines: Option<u32>,
    /// 改动：加了几行 · 删了几行。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub added: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub removed: Option<u32>,
    /// 命中几个文件（搜索 / 列文件）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub files: Option<u32>,
    /// 提问 / 计划的结果（B7）：批准了 · 选了哪几项。没批准走 `rejected`。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub answer: Option<Answer>,
    /// 命令失败时的退出码（结果里写着才有）。
    #[serde(rename = "exitCode", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub exit_code: Option<i32>,
    /// 这次改动说的是哪个文件（结果里写着的路径，原样）。只有改文件那几类结果有。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub file: Option<String>,
    /// 逐段的改动本身（[`PatchHunk`]）。新建整份文件 / 没有改动 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub patch: Option<Vec<PatchHunk>>,
    /// diff 太大、只给了前几段（`added` / `removed` 仍是整份的数）。
    #[serde(
        default,
        rename = "patchTruncated",
        skip_serializing_if = "std::ops::Not::not"
    )]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub patch_truncated: bool,
    /// 过程那一行右侧那一句（[`StepResult::written`] 写）：改动 `+38 −6` · 读了几行（只读文件那一类）· 几个文件 ·
    /// 失败 · 未批准 · 提问 / 计划答了什么（已选「…」· 已批准）；都说不上 ⇒ 空串（出口写耗时）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub text: Words,
    /// 同一句带耗时的那一形（耗时那一截留 `{dur}`，出口填两条记录之间的用时）：失败 · 都说不上的那一步才有；没有 ⇒ 缺（照 `text`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "string"))]
    pub timed: Option<Words>,
    /// 结果的首行预览：第一条非空行、去掉两头空白、至多 60 字（按字符），截了以「…」收尾；界面不再截。结果是空的 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub preview: Option<String>,
    /// 结果正文多少字（按字符，与预览同一份正文）：派出子运行那张卡写「交回结果 · N 字」读它（出口省掉正文之后照样有）。结果是空的 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub chars: Option<u32>,
    /// 派出子运行那次调用的结果是哪一种（[`ResultKind`]）：派出卡照它画（回执不画「交回结果」）。不是派出子运行的结果 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub kind: Option<ResultKind>,
}

/// 派出子运行那次调用拿到的结果是哪一种（各家的格式知识，适配层判；界面只照它画）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub enum ResultKind {
    /// 后台派出当场回的「已启动」回执：子运行还在跑，这不是它交回的东西；收场看运行表。
    Launched,
    /// 子运行交回的结果（前台派出那次调用拿到的结果）。
    Handback,
}

/// 一段改动（统一 diff 的一个 hunk）。行正文带着头字（` ` 没变 · `+` 加的 · `-` 删的）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PatchHunk {
    /// 改之前这段从第几行起（1 起）· 占几行；改之后同。
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<String>,
}

/// 提问 / 计划答了什么（B7）。界面写「已批准」/「已选「{option}」」，不显示 Claude Code 的英文原句。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Answer {
    /// 计划批准了。
    Approved,
    /// 提问选了这几项（选项原文，按问题顺序；一题多选的那题各项都列）。
    Picked { options: Vec<String> },
}

/// 报错 / 重试的原因种类（B6）：界面按它出一词（服务器过载 · 额度满 · 网络中断 · 需登录 · 上下文超长 · 原因不明），不认状态码与报错原文。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub enum ApiReason {
    Overloaded,
    Quota,
    Network,
    Auth,
    Context,
    Unknown,
}

/// 一条用户角色的记录（或一条排队消息）是**谁说的** —— 通用的值域；怎么认是各家的格式知识（`agents/<名>/`，
/// 注册表 [`TextFace::user`]）。随记录成品带出（`userText.speaker`），界面按它决定画不画、画成哪种，不认正文里的标记。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Speaker {
    /// 人打的字（含粘贴、采纳的建议、忙时排队打的）。
    Human,
    /// 人敲的斜杠命令。
    SlashCommand { name: String, args: String },
    /// 人敲的 `!` 命令。
    BashInput { command: String },
    /// `!` 命令的输出回显。
    BashOutput { stdout: String, stderr: String },
    /// 本地命令（斜杠命令）的输出回显。
    CommandOutput,
    /// 后台任务（子 agent / 后台命令）的收场通知。
    TaskNotification {
        #[serde(rename = "taskId", skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        task_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        summary: Option<String>,
        #[serde(rename = "toolUseId", skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        tool_use_id: Option<String>,
    },
    /// 同一会话里的子 agent 发来的话；`handback` ＝ 它交回的报告。`from` 是子 agent 的 id。
    AgentMessage {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        from: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        name: Option<String>,
        handback: bool,
        /// 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        body: Option<String>,
    },
    /// 另一个会话（另一个实例）发来的话。
    PeerSession {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        from: Option<String>,
        /// 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        body: Option<String>,
    },
    /// （子 agent 那一侧）主会话后来发给它的话。
    Coordinator {
        /// 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        body: Option<String>,
    },
    /// （子 agent 那一侧）主会话派给它的活。
    AgentTask,
    /// 系统注入：提醒、技能展开、续跑样板、定时触发、额度恢复后的续跑等。
    /// 默认不露；会话头「⋯」里开了「显示系统注入」才画（一条一行、收着）。
    System {
        /// 注入的原文（剥过两头空白）；这一家给不出 ⇒ 缺。只随记录成品给界面，不进日志。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        body: Option<String>,
    },
    /// 上下文压缩后续接用的摘要（只经 [`Speaker::compact`] 来）。
    #[serde(rename_all = "camelCase")]
    CompactSummary {
        /// 摘要有多少字（写好的数，`copy_core::count_text`；界面拼进「摘要 {n} 字」）。
        #[cfg_attr(any(test, feature = "ts"), ts(as = "String"))]
        chars_text: Words,
    },
    /// 中断标记（人按了 Esc）。
    Interrupt,
    /// 工具结果回灌。
    ToolResult,
}

/// 一条用户角色记录判过「谁说的」的成品（user 记录与排队消息的 `userText`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct UserText {
    pub speaker: Speaker,
    /// 要显示的正文（剥过注入、trim 过）：人说的话 · 压缩摘要 · 派给子 agent 的活；别的来源是空串。
    pub text: String,
    /// `text` 里人粘贴进来的块（UTF-16 下标，含两头的标记）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<Vec<Pasted>>"))]
    pub pasted: Vec<Pasted>,
}

/// 人粘贴进来的一块在 `text` 里的位置（`[start, end)`，UTF-16 下标，含两头的标记）＋ 正文那一截（`[bodyStart, bodyEnd)`）与它的行数。
/// 界面据此把超过 12 行的折起来、不超过的只露正文（不认标记的写法）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Pasted {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub id: Option<String>,
    pub start: u32,
    pub end: u32,
    pub body_start: u32,
    pub body_end: u32,
    /// 正文的行数（两头的空行不算）。
    pub lines: u32,
}

/// 行摘要的一条（`history-read.rows[]`；成品 `read_row`，格目录里登记、冻结）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, concrete(R = Record), bound = "", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ReadRow<R = Value> {
    /// 这一行（含 `\n`）之后那个字节的偏移；没 `\n` 收尾的残尾 ⇒ `null`。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub end: Option<u64>,
    /// 这一行正文的摘要（[`line_hash`]；64 位，过 JSON 数会失精度，只拿来比相等）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub hash: u64,
    /// 通用记录（缺 ＝ 不进界面，照占号）。出口的声明可能去过格（`faces/project.rs`），所以是原样的一团。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<Record>"))]
    pub record: Option<R>,
    /// 那一行记的工作目录（只在有记录时带）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub cwd: Option<String>,
}

/// 记录行的一条（`history-page.lines[]` · `history-lines.lines[]`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, concrete(R = Record), bound = "", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RecordLine<R = Value> {
    /// 这份记录是哪个会话的（那一家按路径认）。
    pub session_id: String,
    /// 那份记录的路径。
    pub path: String,
    /// 行号（可计行，从 0 起）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub seq: u64,
    /// 这一行（含 `\n`）之后那个字节；相邻认不出的几行并成这一条时 ＝ 最后那一行之后。没 `\n` 收尾的残尾 ⇒ `null`。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub end: Option<u64>,
    /// 那一行记的工作目录；没有 ⇒ `null`。
    pub cwd: Option<String>,
    /// 通用记录。出口的声明可能去过格（`faces/project.rs`），所以是原样的一团。
    #[cfg_attr(any(test, feature = "ts"), ts(as = "Record"))]
    pub record: R,
}

/// `history-read` 的回包（带 `view` 时 `rows[]` 那几格可缺）。
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, concrete(R = Record), bound = "", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct HistoryRead<R = Value> {
    /// 这一页里每个可计行一条（空白 / 纯 BOM 行不占）。
    #[cfg_attr(any(test, feature = "ts"), ts(as = "Vec<ReadRow<Record>>"))]
    pub rows: Vec<ReadRow<R>>,
    /// 下一页从这里起（＝ `offset` ＋ 这一页的原始字节数）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub next: u64,
    /// 区间到头了（`until` 或读时的文件长度）。
    pub eof: bool,
}

/// `history-lines` 的回包。
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, concrete(R = Record), bound = "", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct HistoryLines<R = Value> {
    /// 第一行的行号。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub from: u64,
    /// 下一段从这一行起（恒 ＝ `from` ＋ 这一段的可计行数）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub next: u64,
    /// 读到了最后一个完整行之后。
    pub eof: bool,
    /// `[from, next)` 里最后一个可计行之后那个字节（一行都没交 ⇒ `from` 之前最后一个可计行之后）：续点 ＝（路径 · 它 · `next`）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub end: u64,
    /// `[from, next)` 里进界面的那些（不进界面的照占号、不出现）。
    #[cfg_attr(any(test, feature = "ts"), ts(as = "Vec<RecordLine<Record>>"))]
    pub lines: Vec<RecordLine<R>>,
}

impl Speaker {
    /// 线上 `kind` 那一格（骨架索引的 `sp` 也用它）。
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::SlashCommand { .. } => "slashCommand",
            Self::BashInput { .. } => "bashInput",
            Self::BashOutput { .. } => "bashOutput",
            Self::CommandOutput => "commandOutput",
            Self::TaskNotification { .. } => "taskNotification",
            Self::AgentMessage { .. } => "agentMessage",
            Self::PeerSession { .. } => "peerSession",
            Self::Coordinator { .. } => "coordinator",
            Self::AgentTask => "agentTask",
            Self::System { .. } => "system",
            Self::CompactSummary { .. } => "compactSummary",
            Self::Interrupt => "interrupt",
            Self::ToolResult => "toolResult",
        }
    }
}

impl UserText {
    /// 没有正文要显示的那几种来源。
    pub fn of(speaker: Speaker) -> Self {
        Self {
            speaker,
            text: String::new(),
            pasted: Vec::new(),
        }
    }
}

impl Default for UserText {
    fn default() -> Self {
        Self::of(Speaker::System { body: None })
    }
}

/// 原文摘录的上界（字节）：与漂移账的样例同一个上界、同一个截断（[`excerpt_of`]）。
pub const EXCERPT_BYTES: usize = 400;

/// 按字符边界把原文截到 [`EXCERPT_BYTES`] 以内（漂移账的样例也走它）。
pub fn excerpt_of(raw: &str) -> String {
    if raw.len() <= EXCERPT_BYTES {
        return raw.to_string();
    }
    let mut end = EXCERPT_BYTES;
    while !raw.is_char_boundary(end) {
        end -= 1;
    }
    raw[..end].to_string()
}

/// 认不出那一条的一句：「认不出的记录 · {type}」/「记录形状变了 · {type}」，并了几条 ⇒ 后面带「 · {n} 条」。
/// 后端适配层认不出的那一行与客户端解不开的那一条（[`OrUnread`]）同这一句。
pub fn unread_text(why: UnreadWhy, kind: Option<&str>, count: u32) -> Words {
    use copy_core::copy_text;
    let kind = kind
        .filter(|k| !k.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| copy_text("record.unread.noType", &[]));
    let n = count.to_string();
    let text = match (why, count > 1) {
        (UnreadWhy::UnknownType, false) => {
            copy_text("record.unread.unknownType", &[("type", &kind)])
        }
        (UnreadWhy::UnknownType, true) => copy_text(
            "record.unread.unknownTypeMany",
            &[("type", &kind), ("n", &n)],
        ),
        (UnreadWhy::ParseFailed, false) => {
            copy_text("record.unread.parseFailed", &[("type", &kind)])
        }
        (UnreadWhy::ParseFailed, true) => copy_text(
            "record.unread.parseFailedMany",
            &[("type", &kind), ("n", &n)],
        ),
    };
    Words(text)
}

impl Body {
    /// 认不出的一行（每行一条，`count` 1）：一句 ＋ 原文摘录。
    pub fn unread(why: UnreadWhy, kind: Option<String>, raw: &str) -> Self {
        Self::Unread {
            why,
            text: unread_text(why, kind.as_deref(), 1),
            kind,
            tone: Tone::Warn,
            excerpt: Words(excerpt_of(raw)),
            count: 1,
        }
    }
}

/// `t` 的闭集（[`Body`] 各支的标签；判据逐支核过）：解不开的那一条按它分「没见过的类」与「形状变了」。
const BODY_TAGS: &[&str] = &["said", "reply", "retry", "title", "queued", "unread"];

/// 客户端解记录那一层：[`Record`] 外面包一层，解得开 ⇒ 那一条原样；解不开 ⇒ 认不出那一支（[`Body::unread`]，带原样的类名与摘录），
/// 不让整份回包 / 整帧失败（后端照原样的一团写记录，记录的形状不进形状指纹：那一家新出一种记录不该让整页读不出）。
/// 线上照 [`Record`] 写（`transparent`）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct OrUnread(pub Record);

impl OrUnread {
    /// 一团 JSON ⇒ 一条（解不开 ⇒ 认不出那一条：类在 [`BODY_TAGS`] 里 ⇒ 形状变了，否则没见过的类；
    /// 身份原样有就用，没有 ⇒ 按原文摘一个稳定的；时刻那几格型对就带上）。
    pub fn of(raw: Value) -> Self {
        if let Ok(r) = Record::deserialize(&raw) {
            return Self(r);
        }
        let s = |k: &str| raw.get(k).and_then(Value::as_str).map(str::to_string);
        let kind = s("t");
        let why = match kind.as_deref() {
            Some(t) if BODY_TAGS.contains(&t) => UnreadWhy::ParseFailed,
            _ => UnreadWhy::UnknownType,
        };
        let text = raw.to_string();
        let id = s("id")
            .filter(|i| !i.is_empty())
            .unwrap_or_else(|| format!("unread:{:016x}", crate::fnv::fnv1a64(text.as_bytes())));
        Self(Record {
            agent: s("agent").unwrap_or_default(),
            id,
            at: s("at"),
            time_text: s("timeText").map(Words),
            at_ms: raw.get("atMs").and_then(Value::as_i64),
            body: Body::unread(why, kind, &text),
        })
    }
}

impl<'de> Deserialize<'de> for OrUnread {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Value::deserialize(d).map(Self::of)
    }
}

#[cfg(test)]
mod lenient_tests {
    use super::*;

    /// [`BODY_TAGS`] 与 [`Body`] 各支的标签逐一对上（多一支、改一个名都红）。
    #[test]
    fn the_tag_list_is_every_body_tag() {
        let u = UserText::default();
        let each = [
            Body::Said {
                who: u.clone(),
                blocks: vec![],
                results: BTreeMap::new(),
                cwd: None,
            },
            Body::Reply {
                blocks: vec![],
                model: None,
                auto_reply: false,
                cards: BTreeMap::new(),
                steps: BTreeMap::new(),
                runs: BTreeMap::new(),
                error: None,
            },
            Body::Retry {
                reason: ApiReason::Unknown,
                attempt: None,
                max: None,
            },
            Body::Title {
                text: String::new(),
                by: TitleBy::Agent,
            },
            Body::Queued { who: u },
            Body::unread(UnreadWhy::UnknownType, None, ""),
        ];
        let tags: Vec<String> = each
            .iter()
            .map(|b| {
                // 穷举：新添一支这里编不过。
                match b {
                    Body::Said { .. }
                    | Body::Reply { .. }
                    | Body::Retry { .. }
                    | Body::Title { .. }
                    | Body::Queued { .. }
                    | Body::Unread { .. } => {}
                }
                serde_json::to_value(b).unwrap()["t"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(tags, BODY_TAGS);
    }
}
