//! **agent 适配层**：每一家 agent 一份，装它**专属**的格式知识；通用层按 kind 问注册表那一格，不点名任何一家。
//!
//! # 分界：**格式知识**进来，**值判别**留在通用层
//!
//! | 归这里 | 留通用层 |
//! |---|---|
//! | 会话文件长什么样（信封 / 事件名 / 用量字段在哪一层） | 按 kind **值**问注册表那一格 |
//! | 会话住哪个目录、文件怎么命名 | 问到之后各家**共用**的形状（校验、错误出口） |
//! | 起会话 / resume 的**命令形状**与默认命令名 | |
//!
//! 判据住主 crate 的 `agent_locality_guard`：专有的格式针只许在 `agents/<名>/` 下出现；通用层里没有 agent 名字面量、
//! 没有 `<agent 名>_dir` 那一形的标识符、不直呼 `agents::<名>::…`。
//!
//! # 注册表
//!
//! [`REGISTRY`] 是这台认得的那几家：一家一行 [`Adapter`]，只写 `<名>::ADAPTER`，那一行的本体住 `agents/<名>/mod.rs`。
//! 每一格是一面（函数指针与常量，不立 trait），`None` ＝ 这一家没有这一面；各面住各自一份 `agents/<面>.rs`，经 `pub use` 转出。
//! 注册表住本文件：新一家的 `pub mod <名>;` 本来就写在这里，注册表放别处只会让必改的文件从 1 个变成 2 个。
//! 它是通用层直呼适配层的唯一一处，登记在 `agent_locality_guard::AGENT_REGISTRY_SITES`，处数钉死成 `REGISTRY.len()`。
//!
//! 独立的包 `backend-agents`：只引纯工具与平台两层（引了上层就编不过）。

// 测试段可以直接问系统钟（量具、夹具的时刻）；生产段只许 `common::time::now`（`clippy.toml`）。
// 测试段收尾时丢结果不写理由；生产段丢一个 `Result` 写 `.discard(Why::…)`。
#![cfg_attr(
    test,
    allow(
        clippy::allow_attributes,
        clippy::allow_attributes_without_reason,
        clippy::disallowed_methods,
        clippy::let_underscore_must_use,
        clippy::unused_result_ok,
        reason = "测试段放开禁用名单与吞结果那两条：夹具自己定期限、读写盘、起进程、收尾时丢结果；用不用得到随判据增减，写成 `expect` 会在用不到时报 ⇒ 整包唯一许写 `allow` 的一行"
    )
)]

// 包里照旧写 `crate::agents::…` · `crate::common::…` · `crate::platform::…`。
extern crate self as agents;
use backend_common as common;
use backend_platform as platform;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本包的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

use crate::platform::fs::Ro;
pub use cmd_wire::needs::AskKind;
// 通用记录里那几样的形状住 `cmd-wire`（手机按同一份解）；后端在它们上面写字的那几处是下面的 `*Ext`。
pub use cmd_wire::record::{
    Answer, ApiReason, AskOption, AskQuestion, ChildRunTag, LineDiff, LineDiffKind, LineDiffRow,
    PlanTitle, ResultKind, SaidPart, Speaker, StepAsk, StepBody, StepResult, TextFold, ToolCard,
    ToolStep, UserText,
};
use std::path::{Path, PathBuf};

// 待答请求的成品（`needs.ask`）与选项写字（通用）。
pub mod asks;
// 一条记录的「一拍」：轮次 · 会话事实 · 大纲要问一条记录的那几句（通用形，各家答）。
pub mod beat;
pub mod claudecode;
pub mod codex;
// 漂移账的通用形（各家的账本交出来的都是它）。
pub mod drift;
/// 足迹申报表的词（各家用它申报自己布局里的那几条；足迹那边读）。
pub mod footprint_decl;
pub mod mainline;
pub mod record;
// 上游协议的流面（按协议分，不按 agent 分）。
pub mod sse_anthropic;
pub(crate) mod sse_openai_responses;

/// 最小假 agent（判据用）：正文住 `tests/`，只在测试档与 `test-support`（上层测试开它）下编、不进 [`REGISTRY`]
/// （主 crate 的 `fake_tests::the_fixture_agent_never_ships` 钉住）。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../tests/backend/agents/fake/mod.rs"]
pub mod fake;

// 每一面住各自一份；通用层照旧经 `agents::…` 问，引法不变。
mod accounts_face;
mod ask_face;
mod asset_face;
mod attach_face;
mod footprint_face;
mod history_face;
mod launch_face;
mod local_face;
mod mcp_face;
mod quota_face;
mod record_face;
mod text_face;
mod upstream_face;
pub use accounts_face::*;
pub use ask_face::*;
pub use asset_face::*;
pub use attach_face::*;
pub use footprint_face::*;
pub use history_face::*;
pub use launch_face::*;
pub use local_face::*;
pub use mcp_face::*;
pub use quota_face::*;
pub use record_face::*;
pub use text_face::*;
pub use upstream_face::*;

/// 一个 agent 适配层在注册表里的样子。
///
/// ⚠ **刻意不是 trait**（`D4` 逐字「接口由现有能力反推，不凭空设计」；`S4b §2` 复述过一遍）。
/// 今天两家能对称答出来的只有"我叫什么"与"我的 home 在哪"这两问 ——
/// 那就先把这两问定成数据，剩下的等 `S6` 用最小假 agent **反过来**逼。
/// 用函数指针而不是方法，正是为了让这一步**不需要**先决定 trait 长什么样。
pub struct Adapter {
    /// wire 上的 `agent_kind` 值。**由适配层自己提供** —— 通用层里一个 agent 名的
    /// 字面量都不该有（`D3`）。
    pub kind: &'static str,
    /// 这一家对用户的叫法（产品全称，如 `Codex`）。一家的事实，不跟着起会话面：不由我们起的家也说得出。
    /// `None` ＝ 没声明 ⇒ 对人说照实的那一句、不露 kind。只经 [`family_said_among`] 读。
    pub display_name: Option<&'static str>,
    /// **账号维度在这一家上的载体** —— 切账号靠改哪个环境变量。`None` = 这一家没有账号维度。
    ///
    /// 加这一格的理由：终端 `ccm` 面（`control/ccm/`）要设 / 清 / 读它，
    /// 而那个名字**是某一家的知识**。让通用层直接 `use agents::claudecode::paths::…`
    /// 会在 `agent_locality_guard` 的「④ 通用层直呼适配层」上凭空多出 5 处 ——
    /// 而那个数是 `G1` 成功标准②的头条数字、**只许降**。
    /// ⇒ 把它收进注册表：**加一个 agent 本来就该改的那一行**，正是这一格该在的地方。
    pub account_env: Option<&'static str>,
    /// 这个 agent 在本机的 home 目录候选。`None` = 连候选都说不出。只答"该在哪"。
    pub home: fn() -> Option<PathBuf>,
    /// 这一家的**资产面**（skill · 项目级 MCP 住哪、怎么认出来）。
    /// `None` = 这一家今天没有可记进资产目录的东西。
    ///
    /// ⚠ 收进注册表而不是让 `asset_catalog.rs` 直呼 `agents::<名>::` —— 理由与 [`Adapter::account_env`] 同一条：
    /// 后者会让 `agent_locality_guard` 判据④的读数（只许降）凭空上涨。
    pub assets: Option<AssetFace>,
    /// 这一家的**合成历史**面：它的会话不在「按项目目录分」的记录树里（Codex 按日期分），
    /// 历史清单由通用层把它的会话并进来。`None` = 这一家的历史走记录树那一条（Claude）或没有历史。
    ///
    /// ⚠ 收进注册表而不是让 `history_list.rs` 直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]。
    pub history: Option<HistoryFace>,
    /// 这一家的**默认上游**（中转 `/t/` 直通、表里没有那一行时发到哪）。`None` = 未登记 ⇒ 上游选择拒（404 ＋ 原因头），不回落。
    ///
    /// 写死、跟着适配层：上游选择只从这里读（`default_upstreams`）—— 加一家的默认上游，改的就是注册表里那一行。
    pub upstream: Option<DefaultUpstream>,
    /// 这一家的 **MCP 读**（server 表住哪几层、项目表住哪 —— 那一家的格式知识）。`None` = 这一家今天没有。
    /// 收进注册表而不是让帧面宿主直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]（判据④的读数只许降）。
    pub mcp: Option<McpFace>,
    /// 这一家的**足迹面**：落在它自己布局里的那几条申报 ＋ 申报路径里 `~/<它的家>/…` 怎么认 ＋ 用户级 settings 住哪。
    /// 收进注册表而不是让 `footprint/` 直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]（判据④的读数只许降）。
    pub footprint: Option<FootprintFace>,
    /// 这一家的**账号库布局**：一个身份由哪几份文件组成 · 这一家账号 0 的配置目录 · 登录邮箱在哪读。`None` = 这一家今天没有多账号。
    /// 账号库的事（信任同步 · 账号 0 · MCP 同步）都照家分组，各家各读自己这一格。
    /// 收进注册表而不是让 `accounts/manage/` 直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]（判据④的读数只许降）。
    pub accounts: Option<AccountsFace>,
    /// 这一家的**记录解释**：一行原文在渲染模型里是什么 · sid 怎么从文件名来 · 轮次边沿 · 漂移账。
    /// 收进注册表而不是让通用层直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]（判据④的读数只许降）。
    pub records: Option<RecordFace>,
    /// tmux 前台命令（`#{pane_current_command}`）是这几个之一 ⇒ 那个 pane 跑的是这一家。
    /// `None` ＝ 今天没人考据过。收进注册表而不是让调用方直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]。
    pub processes: Option<&'static [&'static str]>,
    /// 这一家的**起会话事实**（默认启动器 · shell wrapper · resume 字面量 · 嵌套标记）；`None` ＝ 这一家不由我们起。
    pub launch: Option<LaunchFace>,
    /// 请求压缩上下文用的那一句（送进会话所在的终端；读者 `faces/session_commands_face.rs`，会话命令表里「压缩上下文」那一项，手机［+］面板列它）；`None` ＝ 这一家不支持。
    pub compact_request: Option<&'static str>,
    /// 清空会话用的那一句（［+］面板「清空会话」经 `session-queue-put` 投进队）；`None` ＝ 这一家不支持。
    pub clear_request: Option<&'static str>,
    /// 这一家在本机的布局（家目录解析 · pidfile · 进程认法）；`None` ＝ 这一家今天不由后端盯着判活。
    pub local: Option<LocalFace>,
    /// 这一家的会话**怎么判活**（全仓只此一处声明）：流式 watcher 挑哪一路跟、冷读问「此刻谁持着它」、resume 前问「是不是已在别处跑着」都读它。
    pub live: LiveBy,
    /// 这一家的**答题口**：会话里弹批准 / 提问 / 批计划的框时，经它的 hook 把请求交给常驻后端、把人的答案交回去
    /// （[`AskFace`]）；`None` ＝ 这一家今天没有，那几种框只能在终端里答。
    pub asks: Option<AskFace>,
    /// 这一家的**附件引用写法**（[`AttachRefer`]）：聊天里随一句发的附件写成粘贴还是正文后面的一行；`None` ＝ 全写相对路径、一行一个。
    pub attach: Option<AttachRefer>,
}

/// 一条活会话此刻在干什么（与哪一家无关的几态）：线上形住 `cmd-wire`。
pub use cmd_wire::frame::SessionActivity;

/// 一条子运行记录说了什么：属于哪个运行 · 是不是它的终局 · 它做的那件事（行上「最近：…」）·
/// 它交回了一次工具调用的结果没有（之前那次工具调用就不再算在等）· 失败收场时的原话。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunMark {
    pub run: String,
    pub end: Option<RunEnd>,
    pub did: Option<RunDid>,
    pub answered: bool,
    pub error: Option<String>,
}

/// 子运行怎么收场的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    Done,
    Failed,
    /// 被叫停（用户 / 派出它的那一方让它停的）。
    Stopped,
}

/// 一个运行最近做的那件事（通用的值域）：线上形住 `cmd-wire`。
pub use cmd_wire::frame::RunDid;

/// 父记录里说到子运行的一条。三形：
/// - 派出：`tool` 有、`run` 无 ⇒ 这次工具调用派出了一个子运行（标签 · 类别）；
/// - 对上：`tool` 与 `run` 都有 ⇒ 这次调用派出的就是这个子运行（可带收场：前台跑完的那次结果）；
/// - 收场：只有 `run` 与 `end` ⇒ 派出那一方说这个子运行完了 / 败了 / 被叫停（不认识的运行不立新行）。
///
/// 子运行那一侧派出那一刻落盘的对上（`ChildFace::spawned`）也是「对上」那一形，可带标签 · 类别 · `from` · 后台派出。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChildLink {
    pub tool: Option<String>,
    pub label: Option<String>,
    pub kind: Option<String>,
    pub run: Option<String>,
    /// 派出它的那个子运行（主运行派出的 ⇒ `None`）：只有子运行那一侧落盘的对上说得出。
    pub from: Option<String>,
    /// 派出那一方说这个子运行已经收场了；没说 ⇒ `None`。
    pub end: Option<RunEnd>,
    /// 派出那一方收到的报错原话（失败收场时，说得出才有）。
    pub error: Option<String>,
    /// 后台派出：派出那一方当场拿到的只是「已启动」，没等它。
    pub background: bool,
}

/// 后台命令的一笔账（适配层从那一家的记录翻过来，与哪一家无关）：
/// 起了一条（工具调用 id · 那条命令原样）· 它当场拿到了任务号 · 它收场了（完成 / 失败 / 被叫停都算，只认是哪一条）。
/// 哪几条此刻还在跑、句子怎么写在核心（`observe::recognize::facts`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BgMark {
    Started {
        call: String,
        cmd: Option<String>,
    },
    Named {
        call: String,
        task: String,
    },
    Ended {
        call: Option<String>,
        task: Option<String>,
    },
}

/// 计划卡标题那三句（形状在 `cmd-wire`，写字在这里）。
pub trait PlanTitleExt {
    /// 那三句（唯一一处）。
    fn written() -> Self;
}

impl PlanTitleExt for PlanTitle {
    fn written() -> Self {
        use crate::common::cells::Words;
        Self {
            waiting: Words(copy_core::copy_text("needs.bar.plan", &[])),
            approved: Words(copy_core::copy_text("interactive.plan.approved", &[])),
            kept: Words(copy_core::copy_text("interactive.plan.kept", &[])),
        }
    }
}

/// 判过「谁说的」的成品上后端认的那一句（形状在 `cmd-wire`）。
pub trait UserTextExt {
    /// 人在这条里说的话（大纲 · 搜索 · 历史摘录都用它）；不是人说的、或说了个空 ⇒ `None`。
    fn speech(&self) -> Option<String>;
}

impl UserTextExt for UserText {
    fn speech(&self) -> Option<String> {
        let s = match &self.speaker {
            Speaker::Human => self.text.clone(),
            Speaker::SlashCommand { name, args } => format!("{name} {args}").trim().to_string(),
            Speaker::BashInput { command } => format!("!{command}"),
            _ => return None,
        };
        (!s.is_empty()).then_some(s)
    }
}

/// 注册表里 `kind` 那一家。认不出 ⇒ `None`。
pub fn adapter_among<'a>(registry: &'a [Adapter], kind: &str) -> Option<&'a Adapter> {
    registry.iter().find(|a| a.kind == kind)
}

/// 「这一问是哪一家的」**按面取，全仓只此一处判**（`has` 说一家声明没声明那一面）：
/// 说了家 ⇒ 那一家，得声明了那一面（没声明 · 认不出 ⇒ `None`）；
/// 没说 ⇒ 默认起的那一家（`LaunchFace.is_default`），它声明了那一面才算；否则 `None`（不另找「恰一家」）。
/// 各面绑上自己那一格的名字：[`account_kind_among`] · [`mcp_kind_among`] · [`asset_default_among`]。
pub fn kind_with_face_among(
    registry: &[Adapter],
    agent: Option<&str>,
    has: impl Fn(&Adapter) -> bool,
) -> Option<&'static str> {
    let kind = match agent {
        Some(k) => k,
        None => default_launch_among(registry)?.0,
    };
    adapter_among(registry, kind)
        .filter(|a| has(a))
        .map(|a| a.kind)
}

/// **这个后端认得哪几个 agent**。加一个 agent = 加一行（+ 上面加一行 `mod`）。
///
/// 每一行只列那一家的 `ADAPTER`：那一行的本体（逐格是这一家的面）住 `agents/<名>/mod.rs`，本文件不写任何一家的格。
/// ⚠ 它与 `agent_locality_guard::tests::HOMES`（判据用的"agent 家"清单）**必须一样长**，
/// 由 `every_agent_adapter_has_exactly_one_registry_entry` 双向钉住：
/// 建了 `agents/<名>/` 却不登记 ⇒ 那家永远"看不见"，而没有任何东西会说。
/// ⚠ **一家一行**：判据⑦把本文件直呼适配层的**行数**钉成 `REGISTRY.len()`、每家恰一处 `<名>::ADAPTER`。
#[rustfmt::skip]
pub const REGISTRY: &[Adapter] = &[
    claudecode::ADAPTER,
    codex::ADAPTER,
];

#[cfg(test)]
#[path = "../../../tests/backend/agents_tests.rs"]
mod tests;
