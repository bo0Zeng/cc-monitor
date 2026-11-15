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
    /// 依次：此刻活着、那台说在等人的会话 · 计划里要人看的格 · 会话外的（需登录的号 · MCP · 权限太宽的自家秘密文件）；一项都没有 ⇒ 空。
    pub items: Vec<NeedsItem>,
    /// 上一条请求收了场的会话（各留最近一次，那条会话下一次挂上请求才换掉），按会话 id 排。
    pub ended: Vec<AskEndRow>,
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
        /// 那一行会走的那一句（[`NeedsClock`]）；不知道等了多久（`needs.waitedMs` 为 `null`）⇒ 缺，出口照抄 `needs.text`。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        clock: Option<NeedsClock>,
        session: NeedsRow,
    },
    /// 计划里要人看的一条：哪个工作区 · 哪一片 · 落在哪一格（没有格可落：顶块没有根格 · 不通过那一条不在哪个权力范围里 ⇒ 缺，进那一片的大纲）。
    Plan {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        rank: u64,
        workspace: String,
        slice: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        cell: Option<NeedsCell>,
    },
    /// 会话外的一项（需登录的号 · MCP · 权限太宽的自家秘密文件）。
    Manual {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        rank: u64,
        manual: ManualItem,
    },
}

/// 在等的会话那一行**会走的那一句**（写好的字，`{dur}` 是已等多久的空位）：已等多久只一种算法 —— `needs.waitedMs`（那台答出那一刻）
/// ＋ 本机从收到这一份起又走过的（不跨机器减）；出口把它当成起点在「收到时刻 − `waitedMs`」的会走的钟，交各自填钟的那一个读口。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct NeedsClock {
    /// 状态那一句（「等批准 · {dur}」，种类那一格是 `needs.text`）：桌面需手动浮层 · 手机需手动屏那一行。
    pub text: Words,
    /// 钉条第一行（「等批准 · Bash · {dur}」，种类那一格是 `needs.head`）。
    pub head: Words,
}

/// 计划那一项落在的那一格：编号（计划页照它选中）· 后端写好的「编号 标题」（行上照抄）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct NeedsCell {
    pub id: String,
    pub label: String,
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
        agent: crate::AgentKind,
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
    /// 号表里一个号写了注册表里没有的家（`agent` ＝ 号表原样）：认不出是哪一家的号，起不了、也登不了。
    /// 没有后端命令能修（`fix` 恒 `null`）：界面点它去设置里那个号（锚点照 `agent` ＋ `account`），改号表里那一格或删号；
    /// 那颗按钮上的字（`go`）由后端写好。
    AccountNoAgent {
        agent: String,
        account: String,
        said: String,
        go: String,
        fix: Option<ManualFix>,
    },
    /// 一份自家秘密文件（API key 凭据文件 · 中转钥匙文件）权限太宽 / 查不出：`said` ＝ 哪份 ＋ 路径 ＋ 宽在哪；
    /// app 里没有解法（`fix` 恒 `null`），`why` ＝ 怎么手动修（查不出那一种 `said` 里已说要手动确认 ⇒ 缺）。权限改好，下一问就不列了。
    SecretFile {
        said: String,
        fix: Option<ManualFix>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        why: Option<String>,
    },
    /// 一份自家状态文件（这台的偏好）格式比此后端读得懂的还老：读口当没有、照缺省用。`said` ＝ 哪份 ＋ 盘上第几版 ＋ 此后端认哪几版；
    /// app 里没有解法（`fix` 恒 `null`），`why` ＝ 怎么办与下次写时原文留在哪。重写成这一版（改一次设置）或删掉，下一问就不列了。
    OwnFile {
        said: String,
        fix: Option<ManualFix>,
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
    /// 一个号：账号库里的哪个号（账号 0 是 `0`）。家由后端照号表里那个号带的那一格认，界面不交；
    /// 报了家（`agent`）却与号表对不上 ⇒ `bad_args`，不照它起登录。
    Account {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        agent: Option<crate::AgentKind>,
        account: String,
    },
    /// 一个号里的一个 MCP 服务器：哪一家 · 哪个号（没设账号 ⇒ `null`：那一家的默认配置目录）· 服务器名（`mcp-read` 里那一条）·
    /// 那一条配置所属的项目目录（项目级 · 本地级要它；用户级 ⇒ `null`）。
    Mcp {
        agent: crate::AgentKind,
        account: Option<String>,
        server: String,
        project_dir: Option<String>,
    },
}

/// `sessions-needs-merge` 的回包（几台合法与分列只在 `face_core::needs::merge_needs`，本机后端与手机核心同一份）：各台在等人的每一项分两列——
/// 待答请求（界面答得了的会话）· 需手动（答不了的会话 · 计划要看的格 · 会话外的），各自先答的在前、各带几项 · 头那一句 · 说得准说不准；
/// 会话外那一截读不出的那几台（各带那一句，那几台的会外项这时不在需手动那一列里）；没问到的那几台（不计，各带那一句）。
/// 头那一句 · 窗口标题 · 数说得准说不准都在合的那一下写好，出口照摆。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct NeedsMerged {
    /// 待答请求：挂着、此刻在界面里答得了的会话（`needs.ask` 在）。头那一句：准 ⇒「待答请求 n」，不准 ⇒「待答请求」。
    pub asks: NeedsColumn,
    /// 需手动：答不了的会话 · 计划要看的格 · 会话外的。头那一句：准 ⇒「需手动 n」，不准 ⇒「需手动」。
    pub manual: NeedsColumn,
    pub unreadable: Vec<NeedsUnreadable>,
    pub down: Vec<NeedsDown>,
    /// 答了的那几台各自 `ended` 那几条（上一条请求怎么收的场），带上是哪台：聊天屏那一块照本台那一条摆。
    pub ended: Vec<NeedsEnded>,
    /// 窗口标题：非零那几列 ⇒「cc-monitor · 待答请求 n · 需手动 m」（待答请求在前，零的那一列不说）；都 0 ⇒「cc-monitor」。
    pub window_title: Words,
}

/// 合好的一列：那几项（先答的在前）· 几项（各台加起来）· 头那一句 · 数说得准说不准
/// （待答请求：没有哪台还在读、没问到；需手动：再加没有哪台会外读不出）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct NeedsColumn {
    pub items: Vec<NeedsAt>,
    pub total: u32,
    pub head: Words,
    pub sure: bool,
}

/// 合好的一条收了场的：哪台 · 那一条原样（`sessions-needs.ended` 的一项）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct NeedsEnded {
    pub origin: crate::Machine,
    pub sid: String,
    pub end: AskEnd,
}

/// 没问到的一台：哪台 · 那一句（够不着 · 那台常驻不在 · 那台答了失败 …；句里叫出了那台）· 复制详情（没有 ⇒ 空）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct NeedsDown {
    pub origin: crate::Machine,
    pub said: String,
    pub detail: String,
}

/// 合好的一项：哪台 · 那一项原样。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct NeedsAt {
    pub origin: crate::Machine,
    pub item: NeedsItem,
}

/// 会话外那一截读不出的一台：哪台 · 那台写的那一句（长按 · 悬停看）· 那一行（「<那台> · 需手动清单无法读取」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct NeedsUnreadable {
    pub origin: crate::Machine,
    pub said: Words,
    pub line: Words,
}

/// 一条会话上一条请求怎么收的场（后端 `control::asks` 那一本账写，会话事实 `askEnd` 与 `sessions-needs.ended` 同一份）：
/// `said` 写好的那一句（替人答了 ⇒「10 分钟无人答 · 已自动答：…」· 别的出口答上了 ⇒「已有人答过」· 终端答了或关了 ⇒「已在终端答过或已关闭」）；
/// `stays` ⇒ 请求没了之后那一行一直留着（切走再回来也在），只有替人答的那一种；别的只在画着的那条请求没了、不是这一端答掉的那一下说。
/// 那条会话下一次挂上请求就换掉。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct AskEnd {
    pub said: Words,
    pub stays: bool,
}

/// 收了场的一条：会话 id ＋ 那一格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct AskEndRow {
    pub sid: String,
    pub end: AskEnd,
}

/// 一条会话「无人答时自动答」那一格（注解条目里那一份，也是 `history-annotate` 的 `patch.autoAnswer`）。
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

/// 「无人答时自动答」可选的一档（会话事实 `autoAnswerChoices` 的一项）：几分钟 ＋ 写好的字（`0` ⇒「关」）。
/// 几档与字只由后端 `control::asks::auto_choices` 一处写；会话事实那一层只装它。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct AutoChoice {
    pub min: u32,
    pub text: Words,
}

/// 一条在等人的会话（`sessions-needs` 会话那一项）：会话 id ＋ 它在等什么（同 `history-facts.needs` 那一份成品）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct NeedsRow {
    pub sid: String,
    /// 叫这条会话什么（同清单那一行的标题）。
    pub label: crate::cells::Words,
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
#[serde(try_from = "NeedsWire", into = "NeedsWire")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, as = "NeedsWire", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Needs {
    pub kind: NeedsKind,
    pub tool: Option<String>,
    pub call: Option<String>,
    pub what: Option<String>,
    pub text: Words,
    pub head: Words,
    pub head_code: Option<String>,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub rank: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub waited_ms: Option<u64>,
    /// 这一次从几时起等：后端只存这一份（状态一句会走的起点 · 过程那一轮的起点都读它），线上 `sinceMs` 也是它。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub since_ms: Option<u64>,
    /// 怎么答：答得了 · 答不了二选一（[`NeedsReply`]），造不出两格对不上的那一形。
    pub reply: NeedsReply,
}

/// 一份需手动怎么答（唯一判在后端 `agents::asks::Asked::manual`）。线上摊成 `ask` · `askHead` · `askPage` · `manualText` 四格（[`NeedsWire`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeedsReply {
    /// 界面答得了：挂着的那一条（选项非空）· 答题块抬头 · 一屏一题那一端的抬头（只提问那一种有）。
    Ask {
        ask: Ask,
        head: Words,
        page: Option<Words>,
    },
    /// 界面答不了：写好的那一句。
    Manual(Words),
}

/// 线上那一份 `needs`（格与 [`Needs`] 一一对上，答的那一格摊成四格）：只在写出去 · 读回来那一下用，读回来经 [`Needs::try_from`] 查四格对得上。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
struct NeedsWire {
    pub kind: NeedsKind,
    /// 等的是哪个工具调用（工具名原样）；判不出 ⇒ `null`。
    pub tool: Option<String>,
    /// 那个调用的 id（记录里 `tool_use` 的 `id`）：界面据此把过程里那一步画成「在等你批准」；判不出 ⇒ `null`。
    pub call: Option<String>,
    /// 批准：那一步的主参数（命令取说得出用意的那一段）；回答：问题原文头一行；计划 / 判不出 ⇒ `null`。
    pub what: Option<String>,
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
    /// 到这一份答出的那一刻已经等了多久（毫秒），**同在那台的钟上**相减，不跨机器减：pidfile 说在等的 ⇒ 读那份 pidfile 的时刻减它的 `statusUpdatedAt`；
    /// pidfile 没说在等、只照挂着的待答请求补出的 ⇒ 答出这一份的时刻减挂上那一刻。起点比那一刻还晚（那台的钟往回拨过）⇒ 0。没有起点 ⇒ `null`。会走的钟：各出口从收到那一刻起按本机钟接着加（时长读口对同一份金样）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub waited_ms: Option<u64>,
    /// 这一次从几时起等（那台的钟，epoch 毫秒）：与脸上在等那一句会走的起点（`clock.from`）同一个来源，出口拿它认「这一份是不是脸这一轮在等的那一份」
    /// （对不上 ⇒ 是上一轮的，不画）。说不出 ⇒ 缺（出口照事实画、不猜）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub since_ms: Option<u64>,
    /// 这一条可以怎么答（挂着的待答请求，`control::asks`）：选项的字 · 样子 · 段 · 要不要附话都写好；没有挂着的 · 挂着的那一条界面答不了（后端 `agents::asks::Asked::manual` 一处判，挡着终端的那一种）⇒ `null`，那一句在 `manualText`。
    pub ask: Option<Ask>,
    /// `ask` 为 `null` 时写好的那一句（界面答不了：「只能在终端里答」· 挂着的那一条在终端里答 ⇒「在终端里答」· 答题口没接上的原因）；有 `ask` ⇒ `null`。
    /// **不变式**：`ask` 为 `null` ⇔ 这一格有字；`ask` 在 ⇒ 它的选项非空。两格只经 [`Needs::answer_with`] · [`Needs::manual`] 一起改。
    pub manual_text: Option<Words>,
    /// 答题块抬头那一句：有 `ask` ⇒ `head`，提问那一种再带题数（「{head} · n 题」）；没有 `ask` ⇒ 缺。出口照抄。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub ask_head: Option<Words>,
    /// 一屏一题那一端（手机抽屉）的抬头：提问那一种 ⇒「{head} · {i}/n」，共几题写好，第几题留 `{i}` 那个空给出口填（1 起）
    /// （翻到哪一页是出口自己的状态）；别的种类 · 没有 `ask` ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub ask_page: Option<Words>,
}

/// 造一份 [`Needs`] 要交的那几格：答的那四格（`ask` · `manualText` · 两格抬头）不在这里，由 [`Needs::new`] 与两个写口管；语气恒 `need`，也不交。不上线。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeedsBase {
    pub kind: NeedsKind,
    pub tool: Option<String>,
    pub call: Option<String>,
    pub what: Option<String>,
    pub text: Words,
    pub head: Words,
    pub head_code: Option<String>,
    pub rank: u64,
    pub waited_ms: Option<u64>,
    pub since_ms: Option<u64>,
}

/// 答的内容与答不了那一句是同一判的两面（判在后端 `agents::asks::Asked::manual` 一处）：类型上二选一（[`NeedsReply`]），换只经两个写口。
impl Needs {
    /// 造一份：先是界面答不了那一态（`manual` 那一句摆在答的位置上）；答得了再经 [`Needs::answer_with`] 配上。
    pub fn new(base: NeedsBase, manual: Words) -> Self {
        let NeedsBase {
            kind,
            tool,
            call,
            what,
            text,
            head,
            head_code,
            rank,
            waited_ms,
            since_ms,
        } = base;
        Self {
            kind,
            tool,
            call,
            what,
            text,
            head,
            head_code,
            rank,
            waited_ms,
            since_ms,
            reply: NeedsReply::Manual(manual),
        }
    }

    /// 界面答得了：配上那一条成品与两格抬头。
    pub fn answer_with(&mut self, ask: Ask, head: Words, page: Option<Words>) {
        self.reply = NeedsReply::Ask { ask, head, page };
    }

    /// 界面答不了：那一句摆在答的位置上。
    pub fn manual(&mut self, text: Words) {
        self.reply = NeedsReply::Manual(text);
    }

    /// 界面答得了 ⇒ 挂着的那一条；答不了 ⇒ `None`。
    pub fn ask(&self) -> Option<&Ask> {
        match &self.reply {
            NeedsReply::Ask { ask, .. } => Some(ask),
            NeedsReply::Manual(_) => None,
        }
    }
}

impl From<Needs> for NeedsWire {
    fn from(n: Needs) -> Self {
        let (ask, ask_head, ask_page, manual_text) = match n.reply {
            NeedsReply::Ask { ask, head, page } => (Some(ask), Some(head), page, None),
            NeedsReply::Manual(text) => (None, None, None, Some(text)),
        };
        Self {
            kind: n.kind,
            tool: n.tool,
            call: n.call,
            what: n.what,
            text: n.text,
            head: n.head,
            head_code: n.head_code,
            tone: Tone::Need,
            rank: n.rank,
            waited_ms: n.waited_ms,
            since_ms: n.since_ms,
            ask,
            manual_text,
            ask_head,
            ask_page,
        }
    }
}

/// 线上读回来的那一份进同一个构造口：答的四格只许是 [`NeedsReply`] 两种之一（`ask` 在 ⇒ 选项非空 · 带抬头 · 不带 `manualText`；
/// `ask` 缺 ⇒ 只带 `manualText`），对不上 ⇒ 拒（那一份不是哪台后端写得出的）。
impl TryFrom<NeedsWire> for Needs {
    type Error = &'static str;

    fn try_from(w: NeedsWire) -> Result<Self, Self::Error> {
        let reply = match (w.ask, w.ask_head, w.ask_page, w.manual_text) {
            (Some(ask), Some(head), page, None) if !ask.choices.is_empty() => {
                NeedsReply::Ask { ask, head, page }
            }
            (None, None, None, Some(text)) => NeedsReply::Manual(text),
            _ => {
                return Err(
                    "needs: ask/askHead/askPage/manualText disagree (answerable = ask with choices + askHead, no manualText; otherwise manualText only)",
                )
            }
        };
        let mut n = Needs::new(
            NeedsBase {
                kind: w.kind,
                tool: w.tool,
                call: w.call,
                what: w.what,
                text: w.text,
                head: w.head,
                head_code: w.head_code,
                rank: w.rank,
                waited_ms: w.waited_ms,
                since_ms: w.since_ms,
            },
            Words(String::new()),
        );
        n.reply = reply;
        Ok(n)
    }
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
    /// 子运行（agent）发的 ⇒ 是哪一趟 ＋ 写好的那一句；主会话 ⇒ `null`。
    pub run: Option<AskRun>,
    /// 一行摘要（命令里说得出用意的那一段 · 路径 · 第一道题）；说不出 ⇒ `null`。
    pub what: Option<String>,
    /// 要批的东西的全文（命令全文 · 写哪个文件写成什么 · 计划原文），不截。
    pub detail: String,
    /// 能选的几项，排好序（「以后不再问」那几项不放第一位）。
    pub choices: Vec<Choice>,
    /// 提问那一种的题目；别的种类为空。
    pub questions: Vec<Question>,
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
    /// 那一趟的 id（那一家原样）。
    pub id: String,
    /// 谁要批，写好的那一句：名（派出时的标签）· 类别（那一家原样，如 `general-purpose`），缺哪样少哪样；出口照抄。
    pub text: Words,
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
    /// 长什么样（出口照它画，不按 id · 次序 · 段自己挑）。
    pub look: ChoiceLook,
    /// 要不要附一句话：none · optional · required。
    pub note: NoteNeed,
    /// 排在哪一段（出口照它分段，不按 id 认；一段一行 / 一组）。
    pub row: ChoiceRow,
    /// 答的时候带不带每题的答（`AnswerReq.picks`）：只有提交答案那一颗。
    pub picks: bool,
}

/// 一颗选项长什么样：主按钮（main 段第一颗，只这一颗）· 危险（停下整轮）· 普通 · 弱（有题时的不回答）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "NeedsChoiceLook",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub enum ChoiceLook {
    Main,
    Danger,
    Plain,
    Ghost,
}

/// 选项排在哪一段：批准分两段（照做 · 拒绝一族）；批计划 · 有题的提问全在 main 一段。
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
    /// 题头那一句，写好的（多选带「多选」那一说法）；没有 ⇒ `null`。
    pub head: Option<String>,
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
#[serde(rename_all = "lowercase")]
pub enum AskKind {
    Approve,
    Answer,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub note: Option<String>,
    /// 提问那一种选 `answer` 时必给：题 id ⇒ 那一题的答，每题都要答、单选至多一项。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub picks: Option<std::collections::BTreeMap<String, AnswerPick>>,
}

/// `sessions-needs` 的入参（没有格）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionsNeedsReq {}

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

/// `needs-answer` 的应答：答上了 · 别处已答（那一句写好，出口照摆、块收起）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Answered {
    pub result: AnswerResult,
    /// `gone` 时写好的那一句（已有人答过 · 已在终端答过或已关闭 · 替人答过的那一句）；`answered` ⇒ 不上线。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub said: Option<Words>,
}

/// `needs-answer` 的结局：答上了 · 别处已答（别的出口先答了、终端那边答了或撤了：这一下没答进去，也不用再答）。
/// 别的没答成走各自的码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum AnswerResult {
    Answered,
    Gone,
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/needs_tests.rs"]
mod tests;
