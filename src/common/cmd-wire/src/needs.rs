//! 「需手动」那一族的线上形（`sessions-needs` 的回包 · 会话外那一项与它的解法 `login-start` 入参 · `history-facts.needs` 那一份成品 · 挂着的待答请求 `needs.ask`）——
//! 两侧同一份：后端按这里的类型写（判定与写字在后端 `observe::recognize::facts` · `agents::asks`），手机嵌的 Rust 核心按这里的类型解。

use crate::cells::{Tone, Words};
use serde::{Deserialize, Serialize};

/// `sessions-needs` 的回包：这台上需手动的每一项（在等的会话 · 计划需看的格 · 会话外的），那台排好（`rank` 小的在前）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct NeedsList {
    /// 依次：此刻活着、那台说在等人的会话 · 计划里要人看的格 · 会话外的（需登录的号 · MCP）；一项都没有 ⇒ 空。
    pub items: Vec<NeedsItem>,
    /// 提问没人答、已替人答过的会话（各留最近一次，那条会话下一次挂上提问才换掉），按会话 id 排。
    pub answered: Vec<AnsweredRow>,
    /// 会话外那一截读不出（账号库清单在、却读不懂）⇒ 那一句；那一截这时不列（空清单看起来像「没事」）。读得出 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub manual_unreadable: Option<Words>,
}

/// 需手动的一项（`sessions-needs` 里的一项）。`rank` 是先后（小的先答，合并各台也只按它排）：会话照 `needs.rank`，计划格 · 会话外各占会话之后的一档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum NeedsItem {
    /// 一条在等人的会话。
    Session {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        rank: u64,
        session: NeedsRow,
    },
    /// 计划里一片要人看的一格：哪个工作区 · 哪一片 · 那一片里第几条（0 起）。
    Plan {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        rank: u64,
        workspace: String,
        slice: String,
        k: u32,
    },
    /// 会话外的一项（需登录的号 · MCP）。
    Manual {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        rank: u64,
        manual: ManualItem,
    },
}

/// 一项需手动怎么解：问哪条命令、交什么入参。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ManualFix {
    pub command: String,
    pub args: LoginStartReq,
}

/// 会话外的一项需手动（每一项带写好的那一句）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum ManualItem {
    /// 一个订阅号还没登录（`fix` ＝ 在 app 里登：`login-start`）。
    AccountLogin {
        agent: String,
        account: String,
        said: String,
        fix: ManualFix,
    },
    /// 一个 MCP 服务器要登录（那一家最近一次连它时记下的、还在有效期内）。`fix` ＝ 在 app 里登：`login-start {kind: mcp}`；
    /// 那一家不在 app 里登 MCP ⇒ `null`（只能在会话里登），这时 `why` 说为什么没有解法。
    McpLogin {
        /// 在哪个号里；没设账号的那一份 ⇒ `null`。
        account: Option<String>,
        server: String,
        scope: String,
        said: String,
        fix: Option<ManualFix>,
        /// `fix` 为 `null` 时必有：为什么没有解法（写好的那一句）；有 `fix` ⇒ 缺。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        why: Option<String>,
    },
}

/// `login-start` 的入参：登录什么（同一个目标新开一份就替掉旧的；`login-finish` 成了原样交回）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum LoginStartReq {
    /// 一个号：哪一家（`accounts-list` 的 `agent`）· 账号库里的哪个号（账号 0 是 `0`）。
    Account { agent: String, account: String },
    /// 一个号里的一个 MCP 服务器：哪一家 · 哪个号（没设账号 ⇒ `null`：那一家的默认配置目录）· 服务器名（`mcp-read` 里那一条）·
    /// 那一条配置所属的项目目录（项目级 · 本地级要它；用户级 ⇒ `null`）。
    Mcp {
        agent: String,
        account: Option<String>,
        server: String,
        project_dir: Option<String>,
    },
}

/// 替人答过的那一条：会话 id ＋ 写好的那一句（「10 分钟没人答，已自动答：…」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct AnsweredRow {
    pub sid: String,
    pub said: Words,
}

/// 一条会话「没人答时自动答」那一格（注解条目里那一份，也是 `history-annotate` 的 `patch.autoAnswer`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct AutoAnswer {
    /// 挂上的提问等满几分钟还没人答就替人答；补丁里给 0 ＝ 关（摘掉这一格）。
    pub after_min: u32,
    /// 替人答的那一句；缺 ⇒ 后端那一句缺省的。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub say: Option<String>,
}

/// 一条在等人的会话（`sessions-needs` 会话那一项）：会话 id ＋ 它在等什么（同 `history-facts.needs` 那一份成品）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct NeedsRow {
    pub sid: String,
    /// 叫这条会话什么（同清单那一行的标题）。
    pub label: crate::cells::Words,
    /// 此刻这条会话的整份脸。
    pub face: crate::frame::SessionFace,
    pub needs: Needs,
    /// 隐藏那一格（照常列在这里：隐藏不影响需手动）；不隐藏 ⇒ `null`（格恒在）。
    pub hidden: Option<crate::session::Hidden>,
}

/// 「需手动」的种类（要人做哪种事）。判不出 ⇒ `Unknown`（只说在等人，不猜）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum NeedsKind {
    /// 批准一个工具调用（或别的批准框）。
    Approve,
    /// 回答一个问题 / 填那一侧要的输入。
    Answer,
    /// 批准计划。
    Plan,
    /// 放行沙箱里的命令联网。
    Network,
    /// 批准协作的另一个运行发来的请求。
    Worker,
    /// 确认它提的会话目标。
    Goal,
    /// 在开着的对话框里选一项。
    Choose,
    Unknown,
}

/// **需手动**的成品：种类 · 等的那一句 · 从何时起等。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Needs {
    pub kind: NeedsKind,
    /// 等的是哪个工具调用（工具名原样）；判不出 ⇒ `null`。
    pub tool: Option<String>,
    /// 那个调用的 id（记录里 `tool_use` 的 `id`）：界面据此把过程里那一步画成「在等你批准」；判不出 ⇒ `null`。
    pub call: Option<String>,
    /// 批准：那一步的主参数；回答：问题原文头一行；计划 / 判不出 ⇒ `null`。
    pub what: Option<String>,
    /// 何时起等（那台 pidfile 的 `statusUpdatedAt`，epoch ms）；没有 ⇒ `null`。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub since_ms: Option<u64>,
    /// 写好的字（等批准 · 等回答 · 需手动），出口照抄。
    pub text: Words,
    /// 钉条第一行（种类 ＋ 工具名：「等批准 · Bash」· 等批准 · 计划 · 等放行 · 联网 …，[`needs_head`]），出口照抄；短的那一格是 `text`。
    pub head: Words,
    /// 钉条里等宽那一段（批准 · 回答 · 放行带那一步的主参数 / 问题；别的种类不带）⇒ `null`。
    pub head_code: Option<String>,
    /// 语气（恒 `need`）。
    pub tone: Tone,
    /// 先答序（小的先答，出口只按它排）：危险度那一档在高位（顶上那个框先答，再按危险度），同一档里等得越久越小，
    /// 不知道等了多久的排这一档最后。算进的是这一份答出那一刻已等多久 ⇒ 会走的钟那一侧从收到起又等了多久照减（`waitedMs` 为 `null` 的不减）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub rank: u64,
    /// 到这一份答出的那一刻已经等了多久（毫秒）：`sinceMs` 与读那份 pidfile 的时刻**同在那台的钟上**相减，不跨机器减；
    /// 起点比那一刻还晚（那台的钟往回拨过）⇒ 0。没有起点 ⇒ `null`。会走的钟：手机与 CLI 按节拍重问；桌面从收到那一刻起接着加（时长读口对同一份金样）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub waited_ms: Option<u64>,
    /// `waitedMs` 写好的字（短时长那一处 `copy_core::short_duration`，与过程行步骤耗时同一种写法）；没有起点 ⇒ `null`。
    pub waited_text: Option<Words>,
    /// 这一条可以怎么答（挂着的待答请求，`control::asks`）：选项的字 · 语气 · 要不要附话都写好；没有挂着的 ⇒ `null`（只能去终端）。
    pub ask: Option<Ask>,
    /// `ask` 为 `null` 时写好的那一句（「只能在终端里答」）；有 `ask` ⇒ `null`。
    pub manual_text: Option<Words>,
}

/// 一条待答请求的成品（`needs.ask`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "NeedsAsk",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub struct Ask {
    /// 这一次请求的 id（后端发的，不是那一家的）：答的时候原样交回。
    pub id: String,
    /// 种类（同 `needs.kind` 那一套里的三种：approve · answer · plan）。
    pub kind: AskKind,
    /// 工具名原样。
    pub tool: String,
    /// 子运行（子 agent）发的 ⇒ 是哪一趟 `{id, kind}`（那一家原样）；主会话 ⇒ `null`。
    pub run: Option<AskRun>,
    /// 一行摘要（命令头一行 · 路径 · 第一道题）；说不出 ⇒ `null`。
    pub what: Option<String>,
    /// 要批的东西的全文（命令全文 · 写哪个文件写成什么 · 计划原文），不截。
    pub detail: String,
    /// 能选的几项，排好序（「以后不再问」那几项不放第一位）。
    pub choices: Vec<Choice>,
    /// 提问那一种的题目；别的种类为空。
    pub questions: Vec<Question>,
    /// 何时挂上的（epoch ms，这台的钟）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub since_ms: u64,
}

/// 发请求的那一趟子运行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "NeedsAskRun",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub struct AskRun {
    pub id: String,
    /// 它是哪一种（那一家原样，如 `general-purpose`）；没说 ⇒ `null`。
    pub kind: Option<String>,
}

/// 一个选项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "NeedsChoice",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub struct Choice {
    /// 答的时候交回的那个词（闭集见 `control::asks::verdict_of`）。
    pub id: String,
    pub text: Words,
    pub tone: Tone,
    /// 要不要附一句话：none · optional · required。
    pub note: NoteNeed,
    /// 排在哪一段（出口照它分段，不按 id 认）。
    pub row: ChoiceRow,
}

/// 选项排在哪一段：照做那一族 · 不照做那一族（拒绝 · 继续规划）· 放开这一条（改到终端里答 · 有题时的不回答）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "NeedsChoiceRow",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub enum ChoiceRow {
    Main,
    Hold,
    Release,
}

/// 选项要不要附一句话。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "NeedsNote",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub enum NoteNeed {
    None,
    Optional,
    Required,
}

/// 一道题（提问那一种）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "NeedsQuestion",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub struct Question {
    /// `q0` · `q1` …（答的时候 `picks` 按它键）。
    pub id: String,
    pub text: String,
    pub header: Option<String>,
    pub multi: bool,
    pub options: Vec<QuestionOption>,
    /// 恒 `true`：每题都许写自由文本（那一家框里的「Type something」）。
    pub other: bool,
}

/// 一道题的一个选项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "NeedsQuestionOption",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub struct QuestionOption {
    /// `q0o0` …
    pub id: String,
    pub text: String,
    pub detail: Option<String>,
}

/// 待答请求的种类（线上同 `needs.kind` 那一套里的三个词）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum AskKind {
    #[serde(rename = "approve")]
    Approve,
    #[serde(rename = "answer")]
    Question,
    #[serde(rename = "plan")]
    Plan,
}

/// `needs-answer` 的入参。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct AnswerReq {
    /// 会话 id。
    pub sid: String,
    /// `needs.ask.id`。
    pub ask: String,
    /// `needs.ask.choices[].id`。
    pub choice: String,
    /// 可缺：附的那句话（选项 `note` 是 required 时必给）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub note: Option<String>,
    /// 提问那一种选 `answer` 时必给：题 id ⇒ 那一题的答，每题都要答、单选至多一项。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub picks: Option<std::collections::BTreeMap<String, AnswerPick>>,
}

/// 提问那一种里一题的答：选了哪几项 ＋ 自由文本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
// 线上不叫 `Pick`：TS 里那是内建的工具类型。
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "AskPick",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub struct AnswerPick {
    /// 选项 id（`needs.ask.questions[].options[].id`）；缺 ⇒ 没选。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(as = "Option<Vec<String>>", optional))]
    pub options: Vec<String>,
    /// 自由文本；缺 / 空白 ⇒ 没写。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub text: Option<String>,
}

/// `needs-answer` 的应答。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Answered {
    /// 恒 `answered`。
    pub result: AnswerResult,
}

/// `needs-answer` 成功那一形（闭集一个词：失败各走自己的码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum AnswerResult {
    Answered,
}
