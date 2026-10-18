//! `S2`（2026-08-14）：**agent 适配层** —— 每个 agent 一份，装它**专属**的知识。
//!
//! # 它为什么存在
//!
//! 〔用 08-14〕逐字：「**现在的后端几乎都是兼容claudecode, 那就标清楚, 分离清楚,
//! 后面搞兼容其他agent的时候才方便**」。
//!
//! 病灶不是"代码散"，是**「加一个 agent 要改哪几处」这份清单只住在人的脑子里**。
//! `S2` 开工前实测：codex 的知识在三处，而三处**长得完全不一样** ——
//! `usage_query.rs` 里是 `aggregate(claude_dir).and_then(|()| aggregate_codex())`，
//! `resolve_query.rs` 里是 `spec.agent_kind.trim() == "codex"`，
//! grep 出其中一处**找不到另一处**。接第三个 agent 的人得先把它们找出来，而没有东西会告诉他有几处。
//!
//! # 分界：**格式知识**进来，**值判别**留在通用层
//!
//! `D3` 逐字：「agent 维度只许出现在**值**里，不许出现在**字段名**里」。照它推：
//!
//! | 归这里 | 留通用层 |
//! |---|---|
//! | 会话文件长什么样（信封 / 事件名 / 用量字段在哪一层） | `agent_kind == "codex"` 这种**值**上的派发 |
//! | 会话住哪个目录、文件怎么命名 | 派发之后两边**共用**的形状（校验、错误出口、CommandPlan 骨架） |
//! | 起会话/resume 的**命令形状**与默认命令名 | |
//!
//! 两条判据钉住这个分界，都住 [`crate::agent_locality_guard`]：
//! ① 专有的格式针**只许**在 `agents/<名>/` 下出现；
//! ② 通用层里**一个 agent 名字面量都没有** —— 按哪一家做什么，只问适配层那一格（如 [`LaunchFace`]）。
//!
//! # ⚠ 两个 agent 都在了，但**故意还没有 trait**
//!
//! `D4` 逐字要求「接口由**现有能力反推**，不凭空设计」。`S3` 把 Claude 那半也搬进来之后，
//! 两个实现终于摆在一起了 —— 而**它们的形状差得比预想大**：
//! `codex/` 是 `parse`+`usage`+`resume` 三块（自带一整套记录抽取器），
//! `claudecode/` 是 `paths`+`records`+`liveness`+`accounts`+`resume` 五块（记录抽取住在通用机器里）。
//! 唯一严格对称的只有 `resume`。⇒ 现在就立 trait 会得到一个"两边都别扭"的抽象。
//! 立接口的动作留给 `L2`，由 `S6`（最小假 agent）**反过来**逼出真正需要的那几个方法。
//!
//! ⚠ 另一半如实说：那个 `claude_dir` **参数名已经清了**
//!（8 个文件；生产段 `claude_dir` 64 行 → 3 行，剩下的 3 处全是冻结的 wire 字段名；
//! `agent_locality_guard` 判据③钉住不许长回来）。
//! 通用层也不直呼任何一家（`agents::<名>::…` 零处，`agent_locality_guard` 判据④）：
//! 每一件 agent 知识都在注册表那一行的某一格里，通用层按 kind 问那一格。
//!
//! # **注册表**：本文件从"目录索引"变成了"这台机器认得哪几个 agent"
//!
//! [`REGISTRY`] 是这台机器认得的那几家。
//!
//! ## 为什么注册表住这里，而不是通用层的某个新文件
//!
//! 「加一个 agent 要改哪几处」这份清单本来就**必然**包含本文件 ——
//! 下面那两行 `pub(crate) mod …` 不加，新的适配层根本编不进来。
//! 把注册表放在别处只会让必改的文件从 1 个变成 2 个。
//! 判据④要通用层直呼适配层零处，而注册表这几行**方向相反** —— 它该随 agent 数增长。
//! ⇒ 登记在 `agent_locality_guard::AGENT_REGISTRY_SITES`、从④的人群里扣掉；
//! 扣出人群的**对价**是判据⑦把本文件的处数钉死成 `REGISTRY.len()`（**一家一行**），
//! 谁想把别处的直呼挪进来，当场红。

use crate::platform::fs::Ro;
pub(crate) use cmd_wire::needs::AskKind;
// 通用记录里那几样的形状住 `cmd-wire`（手机按同一份解）；后端在它们上面写字的那几处是下面的 `*Ext`。
pub(crate) use cmd_wire::record::{
    Answer, ApiReason, AskOption, AskQuestion, ChildRunTag, Pasted, PatchHunk, PlanTitle,
    ResultKind, Speaker, StepAsk, StepBody, StepResult, TextChange, ToolCard, ToolStep, UserText,
};
use std::path::{Path, PathBuf};

// 待答请求的成品（`needs.ask`）与选项写字（通用）。
pub(crate) mod asks;
// 一条记录的「一拍」：轮次 · 会话事实 · 大纲要问一条记录的那几句（通用形，各家答）。
pub(crate) mod beat;
pub mod claudecode;
// 上游协议的流面（按协议分，不按 agent 分）。
pub(crate) mod codex;
// 漂移账的通用形（各家的账本交出来的都是它）。
pub(crate) mod drift;
/// 足迹申报表的词（各家用它申报自己布局里的那几条；足迹那边读）。
pub(crate) mod footprint_decl;
pub(crate) mod mainline;
pub(crate) mod record;
pub(crate) mod sse_anthropic;
pub(crate) mod sse_openai_responses;

/// 最小假 agent（判据用）：正文住 `tests/`，只在测试档编、不进 [`REGISTRY`]
/// （`fake::tests::the_fixture_agent_never_ships` 钉住）。
#[cfg(test)]
#[path = "../../../tests/backend/agents/fake/mod.rs"]
pub(crate) mod fake;

/// 一个 agent 适配层在注册表里的样子。
///
/// ⚠ **刻意不是 trait**（`D4` 逐字「接口由现有能力反推，不凭空设计」；`S4b §2` 复述过一遍）。
/// 今天两家能对称答出来的只有"我叫什么"与"我的 home 在哪"这两问 ——
/// 那就先把这两问定成数据，剩下的等 `S6` 用最小假 agent **反过来**逼。
/// 用函数指针而不是方法，正是为了让这一步**不需要**先决定 trait 长什么样。
pub(crate) struct Adapter {
    /// wire 上的 `agent_kind` 值。**由适配层自己提供** —— 通用层里一个 agent 名的
    /// 字面量都不该有（`D3`）。
    pub(crate) kind: &'static str,
    /// **账号维度在这一家上的载体** —— 切账号靠改哪个环境变量。`None` = 这一家没有账号维度。
    ///
    /// 加这一格的理由：终端 `ccm` 面（`control/ccm/`）要设 / 清 / 读它，
    /// 而那个名字**是某一家的知识**。让通用层直接 `use agents::claudecode::paths::…`
    /// 会在 `agent_locality_guard` 的「④ 通用层直呼适配层」上凭空多出 5 处 ——
    /// 而那个数是 `G1` 成功标准②的头条数字、**只许降**。
    /// ⇒ 把它收进注册表：**加一个 agent 本来就该改的那一行**，正是这一格该在的地方。
    pub(crate) account_env: Option<&'static str>,
    /// monitor 那份 `config.json` 里**旧的**那一格「这一家的数据目录」的键名（`None` ＝ 这一家从来没有那一格）。
    /// ⚠ 只给换形那一段（`control/prefs_migration.rs`）用：新的那一格是 `prefs.json` 的 `agentHomes`（按 kind）。
    /// 下一版随 `prefs_migration.rs` 一起删。键名是那一家的知识，所以住适配层、经 [`old_home_pref_of`] 一处读。
    pub(crate) old_home_pref: Option<&'static str>,
    /// 这个 agent 在本机的 home 目录候选。`None` = 连候选都说不出。只答"该在哪"。
    pub(crate) home: fn() -> Option<PathBuf>,
    /// 这一家的**资产面**（skill · 项目级 MCP 住哪、怎么认出来）。
    /// `None` = 这一家今天没有可记进资产目录的东西。
    ///
    /// ⚠ 收进注册表而不是让 `asset_catalog.rs` 直呼 `agents::<名>::` —— 理由与 [`Adapter::account_env`] 同一条：
    /// 后者会让 `agent_locality_guard` 判据④的读数（只许降）凭空上涨。
    pub(crate) assets: Option<AssetFace>,
    /// 这一家的**合成历史**面：它的会话不在「按项目目录分」的记录树里（Codex 按日期分），
    /// 历史清单由通用层把它的会话并进来。`None` = 这一家的历史走记录树那一条（Claude）或没有历史。
    ///
    /// ⚠ 收进注册表而不是让 `history_list.rs` 直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]。
    pub(crate) history: Option<HistoryFace>,
    /// 这一家的**默认上游**（中转 `/t/` 直通、表里没有那一行时发到哪）。`None` = 未登记 ⇒ 上游选择拒（404 ＋ 原因头，FIX3 之前是 502），不回落。
    ///
    /// 「**写死, 跟着适配层**」：先前这一格住上游选择自己那张表
    /// （`accounts::upstream_select` 的每 agent 一行表），与「跟着适配层」不是同一格（自陈待对齐）。
    /// 今天上游选择**只从这里读**（`default_upstreams`）—— 加一家的默认上游，改的就是注册表里那一行。
    pub(crate) upstream: Option<DefaultUpstream>,
    /// 这一家的 **MCP 读**（server 表住哪几层、项目表住哪 —— 那一家的格式知识）。`None` = 这一家今天没有。
    /// 收进注册表而不是让帧面宿主直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]（判据④的读数只许降）。
    pub(crate) mcp: Option<McpFace>,
    /// 这一家的**足迹面**：落在它自己布局里的那几条申报 ＋ 申报路径里 `~/<它的家>/…` 怎么认 ＋ 用户级 settings 住哪。
    /// 收进注册表而不是让 `footprint/` 直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]（判据④的读数只许降）。
    pub(crate) footprint: Option<FootprintFace>,
    /// 这一家的**账号库布局**：一个身份由哪几份文件组成 · 没设账号时的配置根 · 登录邮箱在哪读。`None` = 这一家今天没有多账号。
    /// 收进注册表而不是让 `accounts/manage/` 直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]（判据④的读数只许降）。
    pub(crate) accounts: Option<AccountsFace>,
    /// 这一家的**记录解释**：一行原文在渲染模型里是什么 · sid 怎么从文件名来 · 轮次边沿 · 漂移账。
    /// 收进注册表而不是让通用层直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]（判据④的读数只许降）。
    pub(crate) records: Option<RecordFace>,
    /// tmux 前台命令（`#{pane_current_command}`）是这几个之一 ⇒ 那个 pane 跑的是这一家（从前界面按画像表自己判）。
    /// `None` ＝ 今天没人考据过。收进注册表而不是让调用方直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]。
    pub(crate) processes: Option<&'static [&'static str]>,
    /// 这一家的**起会话事实**（默认启动器 · shell wrapper · resume 字面量 · 嵌套标记）；`None` ＝ 这一家不由我们起。
    pub(crate) launch: Option<LaunchFace>,
    /// 请求压缩上下文用的那一句（送进会话所在的终端，换号重启「先压缩」那一步）；`None` ＝ 这一家不支持。
    pub(crate) compact_request: Option<&'static str>,
    /// 清空会话用的那一句（［+］面板「清空会话」经 `session-say` 送）；`None` ＝ 这一家不支持。
    pub(crate) clear_request: Option<&'static str>,
    /// 这一家在本机的布局（家目录解析 · pidfile · 进程认法）；`None` ＝ 这一家今天不由后端盯着判活。
    pub(crate) local: Option<LocalFace>,
    /// 这一家的会话**怎么判活**（全仓只此一处声明）：流式 watcher 挑哪一路跟、冷读问「此刻谁持着它」、resume 前问「是不是已在别处跑着」都读它。
    pub(crate) live: LiveBy,
    /// 这一家的**答题口**：会话里弹批准 / 提问 / 批计划的框时，经它的 hook 把请求交给常驻后端、把人的答案交回去
    /// （[`AskFace`]）；`None` ＝ 这一家今天没有，那几种框只能在终端里答。
    pub(crate) asks: Option<AskFace>,
}

/// 一家的**答题口**（[`Adapter::asks`]）：那一家的 hook 输入长什么样、答案写成什么、起会话时怎么挂上 hook，只住适配层；
/// 待答登记表、选项怎么写字、谁先答算谁的住通用层（`control::asks`）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct AskFace {
    /// 起会话时垫在启动器后面的那几个词（只给这一个会话挂上答题 hook，不碰用户自己的设置）；入参 ＝ 这台 `ccm` 的绝对路径。
    pub(crate) launch_args: fn(&str) -> Vec<String>,
    /// 会话级设置那个旗标（透传里已经有它 ⇒ 不另垫，经 [`AskFace::merge`] 并进用户那一份）。
    pub(crate) launch_flag: &'static str,
    /// 用户自己给了那个旗标 ⇒ 把答题 hook 并进他那一份（原样留着他别的一切，只往那一种 hook 的列表末尾加一项）；
    /// 入参 ＝ 他那一份解析出来的值 · ccm 入口。形状不是那一家认的 ⇒ `None`（不挂，这条会话只能在终端里答批准）。
    pub(crate) merge: fn(&serde_json::Value, &str) -> Option<serde_json::Value>,
    /// hook 交来的那一份 ⇒ 通用的待答请求；不是要人答的那一种（别的事件 · 形状认不出）⇒ `None`（hook 空手退）。
    pub(crate) read: fn(&serde_json::Value) -> Option<AskIn>,
    /// 人的答案 ⇒ hook 要打到 stdout 的那一整份；`None` ＝ 不给决定（框照常在终端里问）。
    pub(crate) decide: fn(&AskIn, &Verdict) -> Option<serde_json::Value>,
    /// 换了启动器那一支：答题 hook 做成一个小插件、经环境交下去（启动器后面的词不动）；`None` ＝ 这一家没有这一路（照「自定义启动器」标挂不上）。
    pub(crate) plugin: Option<PluginFace>,
}

/// 一家的**会话插件**那一路（[`AskFace::plugin`]）：插件目录经哪个环境变量交下去、里面放什么、哪些情形挂不上。
#[derive(Debug, Clone, Copy)]
pub(crate) struct PluginFace {
    /// 那一家认的插件目录清单变量（多个目录用这台的路径分隔符连）。
    pub(crate) env: &'static str,
    /// 认这个变量的最低版本。
    pub(crate) min_version: [u64; 3],
    /// 插件目录里的每一份文件：（相对路径, 内容）；入参 ＝ ccm 绝对路径。
    pub(crate) files: fn(&str) -> Vec<(&'static str, String)>,
    /// 这台的 managed 策略文件（起会话前读；服务端下发的那一份事先读不到）。
    pub(crate) policy_files: fn() -> Vec<std::path::PathBuf>,
    /// 一份 managed 策略不许这一路（设了变量起不来 · 插件里的 hook 被拦）。
    pub(crate) policy_blocks: fn(&serde_json::Value) -> bool,
    /// `PATH` 上那一家装的是哪一版（认不出 ⇒ `None`）。
    pub(crate) installed_version: fn(Option<&str>) -> Option<[u64; 3]>,
    /// 那一家导给它起的每个子进程的嵌套标记（ccm 起会话前清掉它 ⇒ 发问的进程起来时带着它 ＝ 会话里 Bash 起的子进程）。
    pub(crate) child_marker: &'static str,
}

/// 一次待答请求（适配层翻好的通用形；那一家的字段名不出适配层）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AskIn {
    /// 哪条会话（那一家的会话 id）。
    pub(crate) sid: String,
    /// 种类：批准 · 回答 · 批计划（只这三种；别的框 hook 管不着）。
    pub(crate) kind: AskKind,
    /// 工具名原样。
    pub(crate) tool: String,
    /// 一行摘要（钉条里那一段：命令 · 路径 · 问题头一行）；说不出 ⇒ `None`。
    pub(crate) what: Option<String>,
    /// 要批的东西的全文（命令全文 · 文件路径与改动 · 计划原文），不截。
    pub(crate) detail: String,
    /// 那一家建议的「以后不再问」那几条：写好的字 ＋ 原样（答的时候原样交回）。
    pub(crate) suggestions: Vec<AskSuggestion>,
    /// 提问那一种的题目（与过程里那张提问卡同一形）；别的种类为空。
    pub(crate) questions: Vec<AskQuestion>,
    /// 是子 agent 发的 ⇒ 它的 id 与种类（那一家原样）。
    pub(crate) agent_id: Option<String>,
    pub(crate) agent_type: Option<String>,
    /// 这个请求**挡着终端**：hook 没答完之前终端里不出框（那一家先跑 hook 再弹框的那几种，如后台子 agent）。
    /// 为真 ⇒ 多给一项「改到终端里答」，且不能拿「终端那边答了」判它关了（终端里根本没有框）。
    pub(crate) blocks_terminal: bool,
    /// 那一家交来的工具参数原样（答的时候适配层要它）。
    pub(crate) input: serde_json::Value,
}

/// 「允许，且以后不再问」那一条建议。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AskSuggestion {
    /// 写成人话的那一段（「本次会话里允许访问 /a/b」）。
    pub(crate) text: String,
    pub(crate) raw: serde_json::Value,
}

/// 人的答案（通用形，适配层翻成那一家的决定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// 允许。`rule` ＝ 带上第几条建议（以后不再问）；`mode` ＝ 批计划时切到哪一档；`answers` ＝ 提问那一种每题的答案（题目原文, 答案）。
    Allow {
        rule: Option<usize>,
        mode: Option<PlanMode>,
        answers: Vec<(String, String)>,
    },
    /// 拒绝。`note` ＝ 给它看的那句话；`stop` ＝ 拒绝并停下这一轮。
    Deny { note: Option<String>, stop: bool },
}

/// 批计划之后切到哪一档。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanMode {
    /// 自动模式（那一家替人判要不要问）。
    Auto,
    /// 自动接受编辑。
    AcceptEdits,
    /// 逐个确认。
    Manual,
}

/// `kind` 那一家的答题口（[`Adapter::asks`]）；认不出 / 没有 ⇒ `None`。
pub(crate) fn ask_face_among(registry: &[Adapter], kind: &str) -> Option<AskFace> {
    adapter_among(registry, kind).and_then(|a| a.asks)
}

/// 同上，生产注册表。
pub(crate) fn ask_face(kind: &str) -> Option<AskFace> {
    ask_face_among(REGISTRY, kind)
}

/// 各家经环境交会话插件的那个变量（[`PluginFace::env`]，去重）：会话事实判「插件到没到那个进程里」按它们读。
pub(crate) fn plugin_envs() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = REGISTRY
        .iter()
        .filter_map(|a| a.asks.and_then(|f| f.plugin).map(|p| p.env))
        .collect();
    v.dedup();
    v
}

/// 〔加一个 agent 只改 `agents/`〕一家的起会话事实 —— **唯一的家**。
/// 从前住两处（monitor `adapter.rs` · 后端 `control/ccm/`，靠金样 `agent-profile-golden.tsv` 对着）；今天 `ccm` 按注册表读
/// （[`launch_face_among`]），界面与 monitor 读从这里生成的 `src/frontend/ui/generated/agent-profile-table.ts`。
// 不派生 `PartialEq`：带着一个函数指针（resume 命令形），函数地址相等不是一个有意义的比较。
#[derive(Debug, Clone, Copy)]
pub(crate) struct LaunchFace {
    /// 适配器 id（界面起会话那一发交回，上游选择按它挑那一行）。
    pub(crate) adapter_id: &'static str,
    /// 这一家对用户的叫法（「{名} 会话还不能选账号」这类话里用）。
    pub(crate) display_name: &'static str,
    /// 消息流里说话的那一方叫什么（卡头 · 刻度悬停「{名}：…」）—— 短名，不是产品全称。
    /// 生成器那一格见 `models`。
    // 后端里的读者只有生成器（`agents_tests.rs::render_agent_profile_table`，测试档）：它写进 `generated/agent-profile-table.ts`，界面读那一份。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) speaker_name: &'static str,
    /// 默认启动器（无候选时的命令基底）。
    pub(crate) default_launcher: &'static str,
    /// shell 集成 wrapper：探得到先用它，探不到回退默认启动器；没有 ⇒ `None`。
    pub(crate) launcher_alias: Option<&'static str>,
    /// resume 那个字面量：`--` 开头 ＝ flag 形（`claude --resume <sid>`），否则 ＝ 子命令形（`codex resume <sid>`）。
    pub(crate) resume_token: &'static str,
    /// 起**新**会话时先定好 sid 的那个旗标（`claude --session-id <uuid>`）：起会话框选了规则 ⇒ 后端起之前按这个 sid 写好来源。
    /// 这一家不认 ⇒ `None`（那就不许起的时候带规则）。
    pub(crate) preset_sid: Option<&'static str>,
    /// 起会话带第一句时，能不能把它作为启动参数交过去（交给那一家的那一半末尾、`--` 之后的位置参数：那一家自己排着，启动画面 · 信任框过了才交进会话）。
    /// 不能 ⇒ 带第一句的起会话回 `bad_args`。
    pub(crate) first_prompt_arg: bool,
    /// 起会话时选权限档那个旗标与可选的档（起会话框 · 手机起会话带它；不给 `bypassPermissions` 那种越过批准的档）。这一家没有 ⇒ `None`。
    pub(crate) permission: Option<PermissionFace>,
    /// 这一家把「我是哪个会话」导给它起的子进程的那个环境变量（会话血缘：`ccm` 在一个会话的 shell 里被调用时读它当父）。
    /// 这一家不导 ⇒ `None`。
    pub(crate) self_sid_env: Option<&'static str>,
    /// `ccm` 起这一家（新起与 resume）时垫在交给它的那一串最前面的参数。
    pub(crate) launch_args: &'static [&'static str],
    /// 起之前要清掉的嵌套会话标记（顺序决定载荷字节）。
    pub(crate) nested_env: &'static [&'static str],
    /// 不说是哪一家时就起这一家（`ccm` 不给 `--agent` · resume 规格里没写或写了认不出的 `agentKind` · 载荷内核）。
    /// 注册表里恰一家为真（`agents_tests.rs` 钉着）。
    pub(crate) is_default: bool,
    /// resume 命令怎么拼：`(启动器, sid) → 整条命令`。
    pub(crate) resume_command: fn(&str, &str) -> String,
    /// resume 那个 tmux 会话名的前缀（会话名 ＝ `<前缀>-<sid 前 8 个字符>`）。
    pub(crate) session_name_prefix: &'static str,
    /// 这一家自己够不着 tmux socket ⇒ 起它时要把 cc-bus 身份（tmux 会话名）经 `CC_BUS_ID` 交进去。
    ///
    /// ⚠ 它答的是「**要不要**」，不是「**能不能**」：值恒来自 tmux 的会话名（`control::ccm::BUS_ID_RECIPE`），
    /// 没有 tmux 就没有这个值 —— 那是载体没了，不是这一家做不到。要不要请一个 `sh` 进来另由 `ccm` 的 `needs_shell` 判。
    pub(crate) needs_bus_id: bool,
    /// 这一家的会话有身份面（`@ccm_sid` 回填）。
    pub(crate) has_identity: bool,
    /// 这一家认得的模型名与写给人看的字（起会话框 · 账号页「默认模型」的选项；不选 ＝ 跟着这一家自己的默认）。没考据过 ⇒ `None`。
    pub(crate) models: Option<&'static [Choice]>,
    /// 回到这一家缺省模型的那一项（原值 ＋ 字）：会话开着时换模型交它；起会话不交（不选就是默认）。没有 ⇒ `None`。
    pub(crate) model_default: Option<Choice>,
    /// 记录里写的模型原值 ⇒ 写给人看的名字（会话事实 `modelText`）；认不出 ⇒ `None`。这一家没考据过 ⇒ `None`。
    pub(crate) model_text: Option<fn(&str) -> Option<String>>,
    /// 记录里写的模型原值 ⇒ 换模型清单里**唯一**对得上的那一项的原值（会话事实 `modelPick`，抽屉勾它）；对不上 · 不止一项 ⇒ `None`。没考据过 ⇒ `None`。
    pub(crate) model_pick: Option<fn(&str) -> Option<&'static str>>,
    /// 会话开着时怎么换模型；这一家换不了 ⇒ `None`。
    pub(crate) model_switch: Option<ModelSwitch>,
}

impl LaunchFace {
    /// 换模型清单里 `model` 那一项（可选的模型 ＋「默认」那一项）；不在清单里 ⇒ `None`。
    pub(crate) fn model_choice(&self, model: &str) -> Option<Choice> {
        self.models
            .into_iter()
            .flatten()
            .chain(self.model_default.as_ref())
            .find(|c| c.value == model)
            .copied()
    }
}

/// 会话开着时换模型的说法（[`LaunchFace::model_switch`]）：往会话里送一行，再从记录里此后长出的那一条读回换成了什么。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ModelSwitch {
    /// 送进会话的那一行（交的是 [`LaunchFace::models`] 里的原值）。
    pub(crate) command: fn(&str) -> String,
    /// 从记录里的一条认出读回的那一句；不是 ⇒ `None`。
    pub(crate) read: fn(&serde_json::Value) -> Option<ModelSaid>,
    /// 换模型顺带改了什么（换模型抽屉底那一行）；没有 ⇒ `None`。
    pub(crate) note: Option<fn() -> String>,
}

/// 换模型读回的那一句（[`ModelSwitch::read`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelSaid {
    /// 那一家写的显示名（原样）。
    pub(crate) name: String,
    /// 那一家说「没变」（本来就是它）。
    pub(crate) kept: bool,
    /// 那一家顺带说的思考档（有才有，原样）。
    pub(crate) effort: Option<String>,
}

/// 一个可选值：交给那一家的原值 ＋ 写给人看的那一句（模型 · 权限档）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Choice {
    /// 原值（交给那一家的就是它，也是界面交回的那一格）。
    pub(crate) value: &'static str,
    /// 写给人看的那一句（经文案表取，键在适配层那一处写成字面量）。
    pub(crate) text: fn() -> String,
}

/// 一家起会话时选权限档的说法（[`LaunchFace::permission`]）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct PermissionFace {
    /// 交给那一家的旗标（`claude --permission-mode <档>`）。
    pub(crate) flag: &'static str,
    /// 可选的档（原值 ＋ 字）。
    pub(crate) modes: &'static [Choice],
    /// 会话会处在、却不给选的档（原值 ＋ 字：越过批准的那几档 · 别名）。会话事实 · 屏底认出来的档都要有字，字从这里与 [`Self::modes`] 取。
    pub(crate) shown: &'static [Choice],
    /// 会话开着时怎么切档；这一家切不了 ⇒ `None`。
    pub(crate) switch: Option<ModeSwitch>,
}

/// 会话开着时切权限档的说法（[`PermissionFace::switch`]）：那一家只有「按一下换到下一档」的循环，没有直接切到某档的命令。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ModeSwitch {
    /// 换到下一档按哪个键（`terminal-input` 键表里的名字）。
    pub(crate) key: &'static str,
    /// 循环里最多几档（按满这么多下还没到 ⇒ 不在循环里）。
    pub(crate) cycle_max: usize,
    /// 从此刻那一屏（纯文本行）认出此刻的档（原值）；认不出 ⇒ `None`。
    pub(crate) read: fn(&[String]) -> Option<&'static str>,
}

impl PermissionFace {
    /// 这一档写给人看的那一句（会话事实 · 切档的回包都读它）：可选的档 ＋ 只认不选的档（[`PermissionFace::shown`]）里找；都没有 ⇒ `None`（不猜）。
    pub(crate) fn text_of(&self, mode: &str) -> Option<String> {
        self.modes
            .iter()
            .chain(self.shown)
            .find(|c| c.value == mode)
            .map(|c| (c.text)())
    }
}

/// `kind` 那一家的 `mode` 这一档写给人看的那一句（[`PermissionFace::text_of`]）。认不出这一家 · 这一家没有权限档 · 认不出这一档 ⇒ `None`。
pub(crate) fn permission_mode_text(kind: &str, mode: &str) -> Option<String> {
    launch_face_among(REGISTRY, kind)?.permission?.text_of(mode)
}

/// `kind` 那一家记录里的模型原值 `raw` 写给人看的名字（[`LaunchFace::model_text`]）。认不出这一家 · 这一家没有这一格 · 认不出这个值 ⇒ `None`。
pub(crate) fn model_text(kind: &str, raw: &str) -> Option<String> {
    launch_face_among(REGISTRY, kind)?.model_text?(raw)
}

/// `kind` 那一家记录里的模型原值 `raw` 是换模型清单里哪一项（[`LaunchFace::model_pick`]）。认不出 ⇒ `None`。
pub(crate) fn model_pick(kind: &str, raw: &str) -> Option<&'static str> {
    launch_face_among(REGISTRY, kind)?.model_pick?(raw)
}

/// `kind` 那一家换模型清单里 `value` 那一项的字（[`LaunchFace::model_choice`]）。认不出这一家 · 不在清单里 ⇒ `None`。
pub(crate) fn model_choice_text(kind: &str, value: &str) -> Option<String> {
    launch_face_among(REGISTRY, kind)?
        .model_choice(value)
        .map(|c| (c.text)())
}

/// 某一家（wire 上的 kind）在给定注册表里的起会话事实。认不出 ⇒ `None`。
/// 生产走 [`REGISTRY`]；判据喂合成注册表（含夹具家），通用层因此只看这一格、不看名字。
pub(crate) fn launch_face_among(registry: &[Adapter], kind: &str) -> Option<LaunchFace> {
    registry
        .iter()
        .find(|a| a.kind == kind)
        .and_then(|a| a.launch)
}

/// 给定注册表里声明「不说是哪一家时就是我」（[`LaunchFace::is_default`]）的那一家：`(kind, 起会话事实)`。没有 ⇒ `None`。
pub(crate) fn default_launch_among(registry: &[Adapter]) -> Option<(&'static str, LaunchFace)> {
    registry
        .iter()
        .find_map(|a| a.launch.filter(|f| f.is_default).map(|f| (a.kind, f)))
}

/// 生产注册表里默认那一家的 kind。注册表恰有一家声明默认（判据钉着）；没有 ⇒ 空串（下游按认不出处理）。
pub(crate) fn default_kind() -> &'static str {
    default_launch_among(REGISTRY).map_or("", |(kind, _)| kind)
}

/// 收 agent 名字的入口用哪一家：**全仓只有这一种认法**（[`pick_kind_among`] 认 wire 上的 kind；要路由名的 [`pick_route`] 也先认 kind）。
///
/// - 没写 / 写空（两侧空白不算字）⇒ 注册表里声明默认（[`LaunchFace::is_default`]）的那一家；
/// - 写了注册表里由我们起的一家 ⇒ 就是它（大小写敏感）；
/// - 写了注册表里没有的名字 ⇒ 报错，那句话列出认得的那几家（从注册表取）。**不落默认**：拼错了就跑成另一家，比起不来更坏。
///
/// 回 `(kind, 起会话事实)`；`Err` 是给用户看的那一句。
fn pick_among(
    registry: &[Adapter],
    name: Option<&str>,
    name_of: fn(&'static str, &LaunchFace) -> &'static str,
) -> Result<(&'static str, LaunchFace), String> {
    let launchable = || {
        registry
            .iter()
            .filter_map(|a| a.launch.map(|f| (a.kind, f)))
    };
    let name = name.map(str::trim).unwrap_or_default();
    let hit = if name.is_empty() {
        launchable().find(|(_, f)| f.is_default)
    } else {
        launchable().find(|(k, f)| name_of(k, f) == name)
    };
    hit.ok_or_else(|| {
        let known: Vec<&str> = launchable().map(|(k, f)| name_of(k, &f)).collect();
        copy_core::copy_text(
            "beAgents.pick.unknown",
            &[("agent", name), ("known", &known.join(" / "))],
        )
    })
}

/// 按 wire 上的 kind 认（`ccm --ccm-agent` · resume 规格的 `agentKind` · cc-bus 派生的 `tool` · 起会话那一发的 `agent`）。规则见 [`pick_among`]。
pub(crate) fn pick_kind_among(
    registry: &[Adapter],
    name: Option<&str>,
) -> Result<(&'static str, LaunchFace), String> {
    pick_among(registry, name, |kind, _| kind)
}

/// [`pick_kind_among`] 在生产注册表上。
pub(crate) fn pick_kind(name: Option<&str>) -> Result<(&'static str, LaunchFace), String> {
    pick_kind_among(REGISTRY, name)
}

pub(crate) use cmd_wire::AgentKind;

/// 一家在后端里面的路由名（中转路由键第 1 段 · 额度账 · 轮换簿 · apikey 表按它记）。不上线：没有 serde，
/// 只从线上的 kind 换得出来（[`pick_route`]）；出口一律换回 kind（[`kind_of_route`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RouteId(&'static str);

impl RouteId {
    /// 存进账 · 表那一侧的那个串。
    pub(crate) fn id(self) -> &'static str {
        self.0
    }
}

/// 线上的 kind（每条命令收的 `agent` 都是它）⇒ 那一家的路由名。规则同 [`pick_kind`]（空 ⇒ 默认那一家；认不出 ⇒ 那一句）。
pub(crate) fn pick_route(kind: Option<&AgentKind>) -> Result<RouteId, String> {
    pick_kind(kind.map(AgentKind::as_str)).map(|(_, f)| RouteId(f.adapter_id))
}

/// 账 · 表里记的路由名 ⇒ 线上的 kind（回包里的 `agent` 都写它）；注册表里没有这个路由名 ⇒ 照原样（账上留着的别家旧条目）。
pub(crate) fn kind_of_route(route_id: &str) -> AgentKind {
    AgentKind::new(
        REGISTRY
            .iter()
            .find(|a| a.launch.is_some_and(|f| f.adapter_id == route_id))
            .map_or(route_id, |a| a.kind),
    )
}

/// `kind` 那一家的 shell wrapper 名（[`LaunchFace::launcher_alias`]）。认不出 / 没有 ⇒ `None`。
/// 别名清单的名字由它派生：`<它>`（当前目录起）· `<它>t`（tmux 里起）· `<它>a`（接回）· 每个号 `<号><它>` / `<号><它>t`。
pub(crate) fn wrapper_alias(kind: &str) -> Option<&'static str> {
    launch_face_among(REGISTRY, kind).and_then(|f| f.launcher_alias)
}

/// 各家导给子进程的「我是哪个会话」变量（注册序、去重；[`LaunchFace::self_sid_env`]）。`ccm` 按它认父；
/// 后端自己起的子进程不该带着它们（常驻后端若是在某个会话里起的，环境里就有那个会话的编号）。
pub(crate) fn self_sid_envs() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for e in REGISTRY
        .iter()
        .filter_map(|a| a.launch.as_ref()?.self_sid_env)
    {
        if !v.contains(&e) {
            v.push(e);
        }
    }
    v
}

/// 把各家「我是哪个会话」的变量登记进起子进程原语的不往下传名单（`platform::child_env::also_internal`）。
/// 入口（`main.rs`）在分流之前调一次：之后这个进程起的每个子进程都看不见它们。
pub fn install_child_env_filter() {
    crate::platform::child_env::also_internal(self_sid_envs());
}

/// 由我们起的那几家（带 [`LaunchFace`] 的，注册表序）—— `ccm --agent` 的闭集就是它，不另写一份。
pub(crate) fn launchable_kinds() -> Vec<&'static str> {
    REGISTRY
        .iter()
        .filter(|a| a.launch.is_some())
        .map(|a| a.kind)
        .collect()
}

/// [`RecordFace::branch`] 的形状。
pub(crate) type BranchFn =
    fn(&[serde_json::Value], &str, &str, &str) -> Result<Vec<serde_json::Value>, String>;

/// 按 sid 找到的那份会话记录：在哪一家的记录根下 · 文件本身。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionAt {
    pub(crate) root: PathBuf,
    pub(crate) file: PathBuf,
}

/// 按 sid 找会话文件 —— 通用层按 sid 找记录的唯一入口：逐家在**它自己的**记录根下问（[`records_root_of`]），谁认得算谁。
/// `tree_home` 是记录树那一家的家目录（账号根）。没有哪一家答得了 ⇒ 照实拒（都答不出 ⇒ 第一家说的那一句）。
pub(crate) fn find_session(tree_home: &Path, sid: &str) -> Result<SessionAt, String> {
    find_session_among(REGISTRY, tree_home, sid)
}

/// [`find_session`] 的可喂夹具那一半。
pub(crate) fn find_session_among(
    registry: &[Adapter],
    tree_home: &Path,
    sid: &str,
) -> Result<SessionAt, String> {
    let mut first_err = None;
    for a in registry {
        let (Some(r), Some(root)) = (a.records, records_root_of(a, tree_home)) else {
            continue;
        };
        match (r.find_session)(&root, sid) {
            Ok(file) => return Ok(SessionAt { root, file }),
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    Err(first_err.unwrap_or_else(|| format!("no agent adapter can locate session {sid:?}")))
}

/// 一家的会话记录住哪个根：记录树那一家 ⇒ `tree_home` 下那棵树；合成历史那一家 ⇒ 它自己报的根。说不出 ⇒ `None`。
fn records_root_of(a: &Adapter, tree_home: &Path) -> Option<PathBuf> {
    match a.records?.tree {
        Some(t) => Some((t.root)(tree_home)),
        None => (a.history?.root)(),
    }
}

/// `kind` 那一家的分叉记录变换 —— 通用层（`control/fork_write.rs`）分叉的唯一入口。那一家答不了 ⇒ 照实拒。
pub(crate) fn build_branch_records(
    kind: &str,
    lines: &[serde_json::Value],
    message_uuid: &str,
    src_sid: &str,
    new_sid: &str,
) -> Result<Vec<serde_json::Value>, String> {
    match record_face(kind).and_then(|r| r.branch) {
        Some(f) => f(lines, message_uuid, src_sid, new_sid),
        None => Err("no agent adapter can fork a session".to_string()),
    }
}

/// 这个 tmux 前台命令是不是注册表里某一家的进程（[`Adapter::processes`]）—— 批量停 / 起认窗格时每一行的 `agent`。
pub(crate) fn is_agent_process(command: &str) -> bool {
    REGISTRY
        .iter()
        .filter_map(|a| a.processes)
        .any(|names| names.contains(&command))
}

/// 一家的记录解释面：函数指针（同 [`Adapter::home`]，不立 trait）。
#[derive(Clone, Copy)]
pub(crate) struct RecordFace {
    /// 一行（[`Line`]：原文与它在文件里的起点字节偏移，没有自己身份的记录拿它合成 id，[`line_id`]）⇒ 通用记录那一形
    /// （空行 / 纯 BOM ⇒ `Ok(None)`；连 JSON 都不是 ⇒ `Err`，调用方照占号、不出成品）。
    pub(crate) parse: fn(&Line<'_>) -> Result<Option<Translated>, String>,
    /// 会话文件 ⇒ 它的 sid（这一家的文件命名）。
    pub(crate) sid: fn(&Path) -> Option<String>,
    /// 这个路径是不是这一家的一份会话记录（按文件形态判：后缀 / 命名）。
    pub(crate) is_session_file: fn(&Path) -> bool,
    /// 这一家的记录树（会话按项目目录分、住在家目录下）：根在哪 · 新会话文件怎么命名。`None` ＝ 这一家的会话不住记录树。
    pub(crate) tree: Option<RecordTree>,
    /// 这一行说这条会话此刻进到哪一态（一轮开始 ⇒ 在跑 · 一轮完了 / 中止 ⇒ 闲着）；不说 ⇒ `None`。
    /// 没有进程状态文件的那一家靠它（[`LocalFace::activity_of`] 是有状态文件那一家的同一问）；`None` 这一格 ＝ 这一家的记录不说。
    pub(crate) activity: Option<fn(&Line<'_>) -> Option<SessionActivity>>,
    /// 一条已解析的记录 ⇒ 它会翻成哪一类通用记录（不进界面 ⇒ `None`）：只看几格判别字段，不走整条翻译
    /// （骨架索引每行一问，大会话上万行）。与 `parse` 出的记录的类逐条一致（各家对拍判据钉）。`None` ＝ 这一家不出骨架。
    pub(crate) class: Option<fn(&serde_json::Value) -> Option<record::RecordClass>>,
    /// 一行原文 ⇒ 它在记录链上的事实（主线外清单由通用层 [`mainline`] 按它算）。`None` ＝ 这一家的记录没有链（清单恒空）。
    pub(crate) chain: Option<fn(&Line<'_>) -> Option<mainline::ChainFact>>,
    /// 在这一家自己的记录根（[`records_root_of`]）下按 sid 找那份会话文件。每一家都得答：恢复 · 「记录还在不在」· 按 sid 读会话都经它。
    pub(crate) find_session: fn(&Path, &str) -> Result<PathBuf, String>,
    /// 分叉的记录变换：`(记录, 分叉点 uuid, 源 sid, 新 sid)` ⇒ 新会话的记录（原共享 crate `branch-core`）。`None` ＝ 这一家不分叉。
    pub(crate) branch: Option<BranchFn>,
    /// 这一家的漂移账（看不懂的记录类型记在哪）⇒ 各面的快照（通用形 [`drift::DriftFaceReport`]）；`None` ＝ 这一家不记。
    pub(crate) drift: Option<fn() -> Vec<drift::DriftFaceReport>>,
    /// 这一家的记录文本面（正文 / 工具内容怎么抽 · CLI 注入怎么剥）；`None` ＝ 这一家不进搜索 / 摘录那几条通用路。
    pub(crate) text: Option<TextFace>,
    /// 删历史会话那一条的两问（按 sid 找那一份 · 它是不是一份会话记录）；`None` ＝ 这一家不删。
    pub(crate) delete: Option<SessionDelete>,
    /// 一条记录与流对账用的键（同一次上游应答写出的记录都带它）。`None` ＝ 这一家的流对不上记录（活卡不定稿，只靠收尾撤）。
    pub(crate) response_id: Option<fn(&serde_json::Value) -> Option<String>>,
    /// 这条记录属于哪个子运行（主运行的记录 ⇒ `None`）。`None` 这一格 ＝ 这一家没有子运行。
    pub(crate) run_of: Option<fn(&serde_json::Value) -> Option<RunMark>>,
    /// 父记录里说到子运行的那几条：派出（父侧工具调用 id、标签，知道时也给出子运行是哪个）· 收场（派出那一方说它完了 / 败了 / 被叫停）。
    pub(crate) child_link: Option<fn(&serde_json::Value) -> Vec<ChildLink>>,
    /// 子运行的记录住哪（由父记录路径推出）。`None` ＝ 这一家没有单独存放的子运行记录。
    pub(crate) children: Option<ChildFace>,
    /// 一条记录里说到后台命令的那几笔（起了一条 · 它拿到了任务号 · 它收场了）。`None` ＝ 这一家没有后台命令。
    pub(crate) background: Option<fn(&serde_json::Value) -> Vec<BgMark>>,
    /// 会话的项目目录（会话起在哪个目录）：只读记录开头（[`first_in_head`]，有上界）。`None` 这一格 ＝ 这一家的记录里没有这件事。
    pub(crate) project_dir: Option<fn(&Path) -> Option<String>>,
    /// 一条已解析的记录 ⇒ 那一家在这一条里说的此刻各 MCP 服务器的样子（[`McpSaid`]）；不是这种记录 ⇒ `None`。
    pub(crate) mcp_said: Option<fn(&serde_json::Value) -> Option<McpSaid>>,
    /// 一条已解析的记录 ⇒ 它是不是「认不出、又不成卡」的那种行（这一家的簿记行）⇒ 它的类型名；不是 ⇒ `None`。
    /// 不成卡是这一家的翻译表判的（`parse` 那一格给它不出记录），这一格只让会话事实照实记几行、哪几种。`None` 这一格 ＝ 这一家没有这种行。
    pub(crate) quiet_unread: Option<fn(&serde_json::Value) -> Option<String>>,
    /// 一条已解析的记录 ⇒ 它的一拍（[`beat::BeatFace`]：轮次 · 会话事实 · 大纲只吃它）。`None` ＝ 这一家不出轮次 · 事实 · 大纲。
    pub(crate) beats: Option<beat::BeatFace>,
    /// 这份会话是从哪个会话分叉来的（[`ForkFace`]）。`None` ＝ 这一家不分叉。
    pub(crate) fork: Option<ForkFace>,
}

/// 一家怎么说分叉来源：一条记录 ⇒ (源 sid, 分叉处的记录 id) · 只拿到那一格本身时 ⇒ 同一对（列表那一路只解了几格）。
#[derive(Clone, Copy)]
pub(crate) struct ForkFace {
    pub(crate) of_record: fn(&serde_json::Value) -> Option<(String, String)>,
    pub(crate) of_field: fn(&serde_json::Value) -> Option<(String, String)>,
}

/// `kind` 那一家：这条记录的一拍。那一家不出 ⇒ `None`。
pub(crate) fn beat_of<'a>(kind: &str, v: &'a serde_json::Value) -> Option<beat::Beat<'a>> {
    record_face(kind)
        .and_then(|r| r.beats)
        .and_then(|b| (b.beat)(v))
}

/// `kind` 那一家：这一行有没有可能改动会话事实（快路）。那一家不出一拍 ⇒ 不会。
pub(crate) fn beat_hint(kind: &str, line: &[u8], watch: &beat::Watch) -> bool {
    record_face(kind)
        .and_then(|r| r.beats)
        .is_some_and(|b| (b.hint)(line, watch))
}

/// `kind` 那一家：这个工具在「需手动」里是哪一种。那一家不说 ⇒ 别的。
pub(crate) fn call_ask(kind: &str, tool: &str) -> beat::CallAsk {
    record_face(kind)
        .and_then(|r| r.beats)
        .map_or(beat::CallAsk::Plain, |b| (b.ask)(tool))
}

/// `kind` 那一家：这条记录说的分叉来源。
pub(crate) fn fork_origin_of(kind: &str, v: &serde_json::Value) -> Option<(String, String)> {
    record_face(kind)
        .and_then(|r| r.fork)
        .and_then(|f| (f.of_record)(v))
}

/// `kind` 那一家：分叉来源那一格本身 ⇒ 那一对。
pub(crate) fn fork_pair_of(kind: &str, field: &serde_json::Value) -> Option<(String, String)> {
    record_face(kind)
        .and_then(|r| r.fork)
        .and_then(|f| (f.of_field)(field))
}

/// 一家的记录树：会话按项目目录分，住在家目录下的一棵树里。
#[derive(Clone, Copy)]
pub(crate) struct RecordTree {
    /// 家目录 ⇒ 记录树的根（其下每个项目一个目录）。
    pub(crate) root: fn(&Path) -> PathBuf,
    /// sid ⇒ 新会话文件的文件名（分叉落盘用）。
    pub(crate) file_name: fn(&str) -> String,
}

/// 一家在本机的布局：家目录怎么解析 · 会话进程留的 pidfile 住哪 · 进程 cmdline 怎么认是它。
#[derive(Clone, Copy)]
pub(crate) struct LocalFace {
    /// 家目录：给了账号配置目录就是它；没给 ⇒ `from_env` 时照进程环境，否则默认家目录。
    pub(crate) home_at: fn(config_dir: Option<&Path>, from_env: bool) -> PathBuf,
    /// 家目录 ⇒ pidfile 目录（判活 · 账号归属读它）。
    pub(crate) pidfile_dir: fn(&Path) -> PathBuf,
    /// 小写后的 cmdline 可不可能是这一家的进程（判活的冒名兜底）。
    pub(crate) cmdline_may_be_agent: fn(&str) -> bool,
    /// 家目录 ⇒ 任务列表的根（其下每个会话一个目录）。`None` ＝ 这一家没有任务列表。
    pub(crate) tasks_dir: Option<fn(&Path) -> PathBuf>,
    /// 进程状态文件（已解析）⇒ 是不是后台会话（不是人坐在终端里对话的那种，不成 tab）。
    pub(crate) background_of: fn(&serde_json::Value) -> bool,
    /// 进程状态文件 ⇒ 此刻在干什么；说不清 ⇒ `None`。
    pub(crate) activity_of: fn(&serde_json::Value) -> Option<SessionActivity>,
    /// 进程状态文件 ⇒ 在等人时等的是什么框；没说 / 说不清 ⇒ `None`。
    pub(crate) wait_of: fn(&serde_json::Value) -> Option<WaitOn>,
}

/// 一条活会话此刻在干什么（与哪一家无关的几态）：线上形住 `cmd-wire`。
pub use cmd_wire::frame::SessionActivity;

/// 一条会话在等人时，**等的是什么框**（与哪一家无关；适配层从那一家的词翻过来，翻不出 ⇒ 不给）。
/// 「要人做哪种事」由它配上记录里没结果的那一步判（`observe::recognize::facts::needs_of`），不在这里判。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WaitOn {
    /// 批准框（工具调用 · 提问 · 计划都弹在这一类里，分哪种看那一步是什么工具）。
    Permission,
    /// 沙箱里的命令要联网，等放行。
    Network,
    /// 协作的另一个运行（worker）发来的批准请求。
    Worker,
    /// 它提了一个会话目标，等确认。
    Goal,
    /// 要填 / 要答（提问 · MCP 那一侧要的输入）。
    Input,
    /// 别的对话框开着（选项 · 提示 · 设置），等选。
    Dialog,
}

/// 一条子运行记录说了什么：属于哪个运行 · 是不是它的终局 · 它做的那件事（行上「最近：…」）·
/// 它交回了一次工具调用的结果没有（之前那次工具调用就不再算在等）· 失败收场时的原话。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct RunMark {
    pub(crate) run: String,
    pub(crate) end: Option<RunEnd>,
    pub(crate) did: Option<RunDid>,
    pub(crate) answered: bool,
    pub(crate) error: Option<String>,
}

/// 子运行怎么收场的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunEnd {
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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ChildLink {
    pub(crate) tool: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) kind: Option<String>,
    pub(crate) run: Option<String>,
    /// 派出那一方说这个子运行已经收场了；没说 ⇒ `None`。
    pub(crate) end: Option<RunEnd>,
    /// 派出那一方收到的报错原话（失败收场时，说得出才有）。
    pub(crate) error: Option<String>,
    /// 后台派出：派出那一方当场拿到的只是「已启动」，没等它。
    pub(crate) background: bool,
}

/// 后台命令的一笔账（适配层从那一家的记录翻过来，与哪一家无关）：
/// 起了一条（工具调用 id · 那条命令原样）· 它当场拿到了任务号 · 它收场了（完成 / 失败 / 被叫停都算，只认是哪一条）。
/// 哪几条此刻还在跑、句子怎么写在核心（`observe::recognize::facts`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BgMark {
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

/// `kind` 那一家：一条已解析的记录里说到后台命令的那几笔（[`RecordFace::background`]；那一家没有 ⇒ 空）。
pub(crate) fn background_marks(kind: &str, v: &serde_json::Value) -> Vec<BgMark> {
    record_face(kind)
        .and_then(|r| r.background)
        .map(|f| f(v))
        .unwrap_or_default()
}

/// 计划卡标题那三句（形状在 `cmd-wire`，写字在这里）。
pub(crate) trait PlanTitleExt {
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

/// 耗时那一截的占位（出口填）。
pub(crate) const DUR_SLOT: &str = "{dur}";

/// 过程一步的结果那一句（形状在 `cmd-wire`，写字在这里）。
pub(crate) trait StepResultExt {
    /// ★ 右侧那一句（唯一一处）。`read` ＝ 这是读文件那一类的结果（`lines` 才说成「读了几行」；命令输出几行 · 搜索命中几行不说）。
    fn written(self, read: bool) -> Self;
}

impl StepResultExt for StepResult {
    fn written(mut self, read: bool) -> Self {
        let w = |s: String| crate::common::cells::Words(s);
        let (text, timed) = if self.rejected {
            (copy_core::copy_text("stream.step.rejected", &[]), None)
        } else if !self.ok {
            (
                copy_core::copy_text("stream.step.failed", &[]),
                Some(copy_core::copy_text(
                    "stream.step.failedFor",
                    &[("dur", DUR_SLOT)],
                )),
            )
        } else if let Some(a) = &self.answer {
            // 提问 / 计划答了什么（卡片底行那一句）。
            (
                match a {
                    Answer::Approved => copy_core::copy_text("interactive.done.approved", &[]),
                    Answer::Picked { options } => copy_core::copy_text(
                        "interactive.done.picked",
                        &[("option", &options.join(" / "))],
                    ),
                },
                None,
            )
        } else {
            let diff: Vec<String> = [
                self.added.filter(|n| *n > 0).map(|n| format!("+{n}")),
                self.removed.filter(|n| *n > 0).map(|n| format!("−{n}")),
            ]
            .into_iter()
            .flatten()
            .collect();
            if !diff.is_empty() {
                (diff.join(" "), None)
            } else if let Some(n) = self.lines.filter(|_| read) {
                (
                    copy_core::copy_text("stream.step.lines", &[("n", &n.to_string())]),
                    None,
                )
            } else if let Some(n) = self.files {
                (
                    copy_core::copy_text("stream.step.files", &[("n", &n.to_string())]),
                    None,
                )
            } else {
                (String::new(), Some(DUR_SLOT.to_string()))
            }
        };
        self.text = w(text);
        self.timed = timed.map(w);
        self
    }
}

/// [`Speaker::kind`] 那一份闭集（骨架读回来认词用）。
pub(crate) const SPEAKER_KINDS: [&str; 14] = [
    "human",
    "slashCommand",
    "bashInput",
    "bashOutput",
    "commandOutput",
    "taskNotification",
    "agentMessage",
    "peerSession",
    "coordinator",
    "agentTask",
    "system",
    "compactSummary",
    "interrupt",
    "toolResult",
];

/// 「谁说的」那一侧后端写字的一种（形状在 `cmd-wire`）。
pub(crate) trait SpeakerExt {
    /// 压缩摘要那一种：数好正文多少字（按字，不按 UTF-16）。
    fn compact(text: &str) -> Self;
}

impl SpeakerExt for Speaker {
    fn compact(text: &str) -> Self {
        Self::CompactSummary {
            chars_text: crate::common::cells::Words(copy_core::count_text(
                text.chars().count() as u64
            )),
        }
    }
}

/// 判过「谁说的」的成品上后端认的那一句（形状在 `cmd-wire`）。
pub(crate) trait UserTextExt {
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

/// 子运行的记录住哪：父记录路径 ⇒ 此刻在盘上的子运行记录路径们（不在 ⇒ 空）。通用 watcher 拿这些路径走与主记录同一条事件管线。
#[derive(Clone, Copy)]
pub(crate) struct ChildFace {
    pub(crate) sources: fn(&Path) -> Vec<PathBuf>,
    /// 一个路径**若是**某份父记录的子运行记录（形状对得上 `sources` 会收的那种）⇒ 那份父记录的路径；否则 `None`。
    /// 文件事件来了只对它答得出的才去找，且只在那一份父记录底下找。
    pub(crate) owner: fn(&Path) -> Option<PathBuf>,
    /// 记录树根底下，id 为这个的子运行的记录（[`Self::owner`] 再对到它的父记录）；没有 ⇒ `None`。计划读面把接手 id 对到会话时用。
    pub(crate) find: fn(&Path, &str) -> Option<PathBuf>,
    /// 父记录的一行原文可能说到子运行（[`RecordFace::child_link`] 会答出东西）—— 便宜的预筛：漏判不许，多判无妨。
    /// 只读尾巴的那条流接上会话时，靠它从父记录已有的那一截里只挑这几行解析。
    pub(crate) hint: fn(&str) -> bool,
    /// 一条记录自己写着的时刻（自 1970 起的毫秒；运行表的开始 · 最近动静 · 收场三个时刻用）；记录里没写 ⇒ `None`（通用层退回读到它的时刻）。
    pub(crate) written: fn(&serde_json::Value) -> Option<u64>,
    /// 一份子运行记录（[`Self::sources`] 收的那种）⇒ 派出它的那一刻就落了盘的对上（[`ChildLink`] 的「对上」一形：父侧工具调用 id ＋ 子运行）。
    /// 前台派出的子运行要跑完父侧才回结果，在那之前卡和运行只能靠它对上；还没落盘 / 这一家不落 ⇒ `None`（通用层下次读到这份记录再问）。
    pub(crate) spawned: fn(&Path) -> Option<ChildLink>,
}

/// 一个上游协议的流面：把一个原始流事件（SSE `data:` 后面那段原文）折成归一事件。认不出 ⇒ 空。
/// 按上游协议分，不按 agent 分：哪家走哪个协议由那一家的 [`DefaultUpstream::stream`] 说。
#[derive(Clone, Copy)]
pub(crate) struct StreamFace {
    pub(crate) fold: fn(&str) -> Vec<StreamEv>,
    /// 一件被截断的事件（只有开头那一截）⇒ 只从开头认得出的那几格（类型 · 应答标识）；认不出 ⇒ 空。不许出字。
    pub(crate) fold_clipped: fn(&str) -> Vec<StreamEv>,
    /// 一件事件里说出的这一发用量（[`Spent`]）；没说 ⇒ `None`。`None` 这一格 ＝ 这个协议的流说不出用量。
    pub(crate) spent: Option<fn(&str) -> Option<Spent>>,
}

/// 一件流事件里说出的这一发用量：折合输入 token（各类 token 按那一家的价比折成输入 token 计）；
/// `prompt` 有 ＝ 这一件是一发应答的开头（数「几发」用），带这一发的上下文。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Spent {
    pub(crate) units: f64,
    pub(crate) prompt: Option<Prompt>,
}

/// 一发的上下文：`tokens` ＝ 输入 ＋ 写缓存 ＋ 读缓存（原数，不折）；`tier` ＝ 这一发写缓存用的那一档（只读不写 ⇒ 说不出）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Prompt {
    pub(crate) tokens: u64,
    pub(crate) tier: Option<Tier>,
}

/// 写缓存的一档：缓存留几秒 · 写比读多几倍（折合输入 token 的倍数之差）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Tier {
    pub(crate) ttl: u64,
    pub(crate) extra: f64,
}

/// 归一流事件（界面只收这个，不收任何一家的原始事件）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum StreamEv {
    /// 一次应答开始：对账键（与记录的 [`RecordFace::response_id`] 同一个值域）。
    Start { rid: String },
    /// 第 `i` 块开始了。
    Block {
        #[cfg_attr(test, ts(type = "number"))]
        i: u64,
        kind: BlockKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        tool: Option<String>,
    },
    /// 第 `i` 块的一段文字。
    Text {
        #[cfg_attr(test, ts(type = "number"))]
        i: u64,
        s: String,
    },
    /// 上游说完了（`ok` ＝ 正常说完；`false` ＝ 上游报错收尾）。
    Stop { ok: bool },
}

/// 归一流里一块是什么：线上形住 `cmd-wire::live`（活卡每一块的种类）。
pub use cmd_wire::live::BlockKind;

/// 通用层要的那一组面（记录 ＋ 子运行住址）：流式 watcher 跟的那一家给；判据喂别的一组（假适配层）。
#[derive(Clone, Copy)]
pub(crate) struct RunFaces {
    pub(crate) response_id: Option<fn(&serde_json::Value) -> Option<String>>,
    pub(crate) run_of: Option<fn(&serde_json::Value) -> Option<RunMark>>,
    pub(crate) child_link: Option<fn(&serde_json::Value) -> Vec<ChildLink>>,
    pub(crate) children: Option<ChildFace>,
}

impl RunFaces {
    /// 一家的记录面 ⇒ 它的运行面。
    pub(crate) fn of(r: &RecordFace) -> Self {
        Self {
            response_id: r.response_id,
            run_of: r.run_of,
            child_link: r.child_link,
            children: r.children,
        }
    }

    /// 什么都不填 ⇒ 没有子运行、流对不上记录。
    pub(crate) const NONE: Self = Self {
        response_id: None,
        run_of: None,
        child_link: None,
        children: None,
    };

    pub(crate) fn response_id(&self, v: &serde_json::Value) -> Option<String> {
        self.response_id.and_then(|f| f(v))
    }

    pub(crate) fn run_of(&self, v: &serde_json::Value) -> Option<RunMark> {
        self.run_of.and_then(|f| f(v))
    }

    pub(crate) fn child_links(&self, v: &serde_json::Value) -> Vec<ChildLink> {
        self.child_link.map(|f| f(v)).unwrap_or_default()
    }

    pub(crate) fn sources(&self, parent: &Path) -> Vec<PathBuf> {
        self.children
            .map(|c| (c.sources)(parent))
            .unwrap_or_default()
    }

    pub(crate) fn owner(&self, p: &Path) -> Option<PathBuf> {
        self.children.and_then(|c| (c.owner)(p))
    }

    pub(crate) fn hint(&self, line: &str) -> bool {
        self.children.is_some_and(|c| (c.hint)(line))
    }

    pub(crate) fn written(&self, v: &serde_json::Value) -> Option<u64> {
        self.children.and_then(|c| (c.written)(v))
    }

    pub(crate) fn spawned(&self, child: &Path) -> Option<ChildLink> {
        self.children.and_then(|c| (c.spawned)(child))
    }
}

/// `kind` 那一家的运行面（没有 ⇒ [`RunFaces::NONE`]）。
pub(crate) fn run_faces(kind: &str) -> RunFaces {
    record_face(kind).map_or(RunFaces::NONE, |r| RunFaces::of(&r))
}

/// `kind` 那一家的一条已解析记录在会话里属于哪个运行（主运行 ⇒ `None`）—— 通用层（大纲 · 骨架索引）判「这条是不是子运行的」的唯一入口。
/// `kind` 那一家这条记录会翻成哪一类通用记录（骨架行的 `t`）；那一家答不了 ⇒ `None`。
pub(crate) fn record_class_of(kind: &str, v: &serde_json::Value) -> Option<record::RecordClass> {
    record_face(kind).and_then(|r| r.class).and_then(|f| f(v))
}

pub(crate) fn run_of_record(kind: &str, v: &serde_json::Value) -> Option<RunMark> {
    run_faces(kind).run_of(v)
}

/// 一家的记录文本面（原共享 crate `search-core` 里 Claude 记录文本那一半）：函数指针（同 [`Adapter::home`]，不立 trait）。
/// 通用层（全局搜索 · 会话内查找 · 历史摘录 · 用户输入列表 · 骨架索引）经 [`main_text`] · [`tool_text`] · [`user_text_of`] · [`human_speech`] 够它，不按名字够。
#[derive(Clone, Copy)]
pub(crate) struct TextFace {
    /// 一条记录的 `message.content` ⇒ 正文文本块。
    pub(crate) main: fn(&serde_json::Value) -> String,
    /// 同上 ⇒ 工具内容（`is_assistant`：工具入参 · 思考；否则：工具结果）。
    pub(crate) tool: fn(&serde_json::Value, bool) -> String,
    /// 一条已解析的记录 ⇒ 它是谁说的（[`UserText`]）；不是用户角色的记录 ⇒ `None`。
    pub(crate) user: fn(&serde_json::Value) -> Option<UserText>,
    /// 一条已解析的记录是不是进界面的一句排队插话（人打的）⇒ 它的原文（已 trim）；不是 ⇒ `None`。
    pub(crate) queued: fn(&serde_json::Value) -> Option<String>,
    /// 一条已解析的记录在消息流里落在哪张卡上（[`Lands`]）：查找 · 搜索的单位就是流里画的那几张卡。
    pub(crate) lands: fn(&serde_json::Value) -> Lands,
}

/// 一条记录在消息流里落在哪（[`TextFace::lands`]，判法是各家的格式知识；界面照它跳、不自己推）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Lands {
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

/// `kind` 那一家怎么抽正文 —— 通用层够它的唯一入口。那一家答不了 ⇒ 空串（不搜、不摘）。
pub(crate) fn main_text(kind: &str, content: &serde_json::Value) -> String {
    text_face(kind).map_or_else(String::new, |t| (t.main)(content))
}

/// `kind` 那一家怎么抽工具内容。那一家答不了 ⇒ 空串。
pub(crate) fn tool_text(kind: &str, content: &serde_json::Value, is_assistant: bool) -> String {
    text_face(kind).map_or_else(String::new, |t| (t.tool)(content, is_assistant))
}

/// `kind` 那一家判这条记录是谁说的。那一家答不了 ⇒ `None`。
pub(crate) fn user_text_of(kind: &str, v: &serde_json::Value) -> Option<UserText> {
    text_face(kind).and_then(|t| (t.user)(v))
}

/// `kind` 那一家判这条记录是不是一句排队插话（进界面的那种）⇒ 原文。那一家答不了 ⇒ `None`。
pub(crate) fn queued_text_of(kind: &str, v: &serde_json::Value) -> Option<String> {
    text_face(kind).and_then(|t| (t.queued)(v))
}

/// `kind` 那一家说这条记录在流里落在哪。那一家答不了 ⇒ 自己那张卡。
pub(crate) fn lands_of(kind: &str, v: &serde_json::Value) -> Lands {
    text_face(kind).map_or(Lands::Own, |t| (t.lands)(v))
}

/// 人在这条记录里说的话（[`UserText::speech`]）；不是人说的 ⇒ `None`。
pub(crate) fn human_speech(kind: &str, v: &serde_json::Value) -> Option<String> {
    user_text_of(kind, v)?.speech()
}

/// 删历史会话那一条要问适配层的两件事 —— 那是记录布局的知识，文件管理写面不认；
/// 由门（命令注册那一处）经 [`locate_session_for_delete`] · [`is_session_record`] 这一个窄口递给它。
#[derive(Clone, Copy)]
pub(crate) struct SessionDelete {
    /// sid ⇒ 本机要删的那一份（找 → 解到底 → 恰是那一形）。
    pub(crate) locate: fn(&str) -> Result<PathBuf, String>,
    /// 这一份是不是会话记录的形状（写面删之前再问一次）。
    pub(crate) is_record: fn(&Path) -> bool,
}

/// 窄口之一：sid ⇒ 要删的那一份。逐家问（声明了删会话两问的那几家），谁认得算谁；
/// 都答不出 ⇒ 第一家说的那一句；没有哪一家答得了 ⇒ 照实拒。
pub(crate) fn locate_session_for_delete(sid: &str) -> Result<PathBuf, String> {
    let mut first_err = None;
    for d in REGISTRY.iter().filter_map(|a| a.records?.delete) {
        match (d.locate)(sid) {
            Ok(p) => return Ok(p),
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    Err(first_err
        .unwrap_or_else(|| copy_core::copy_text("beFilesWrite.session.noReader", &[("id", sid)])))
}

/// 窄口之二：这一份是不是会话记录 —— 这份记录归哪一家（[`record_face_of`]）就问哪一家；那一家答不了 ⇒ 不是。
pub(crate) fn is_session_record(p: &Path) -> bool {
    record_face_of(p)
        .and_then(|r| r.delete)
        .is_some_and(|d| (d.is_record)(p))
}

/// 一行原文翻成的那一形 —— 适配层给，通用层只搬（[`record::Record`] 的格通用层一个都不读，只按 [`QueueMark`] 配打字时刻）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Translated {
    /// 这一行在界面里是什么；缺 ＝ 不进界面（照占号、不出成品）。
    pub(crate) record: Option<record::Record>,
    /// 这条记录自己的 `cwd`（带它的那一类才有）。
    pub(crate) cwd: Option<String>,
    /// 排队那一对：打字那一刻（不出记录）· 被插进那一轮的那一条（它的 `at` 要换成打字时刻）。
    pub(crate) queue: Option<QueueMark>,
}

/// 排队消息的两头。配对按 `text`（人打的那句原文）：同一句重打 ⇒ 取最近一次。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum QueueMark {
    /// 人在一轮跑着时打了这一句（`at` ＝ 打字时刻）。
    Typed { text: String, at: String },
    /// 这一条 `queued` 记录插的是这一句。
    Taken { text: String },
}

/// **一行记录**，交给适配层那几问（成品 · 链事实 · 轮次结束 · 此刻在干什么 · 运行簿）的同一份：原文（去掉 BOM 与两端空白）＋
/// 起点字节 ＋ 惰性解析的 `Value`。头一个要整行的那一问解一次，之后的都吃这一份 —— 一行只从文本解一次（瓶颈 #8）。
/// 只要几格的那几问（[`Line::read`]）在没人解过整行时直接从文本解那几格（预读那一路只要链事实，不必整行）。
pub(crate) struct Line<'a> {
    text: &'a str,
    start: u64,
    value: std::cell::OnceCell<Option<serde_json::Value>>,
}

impl<'a> Line<'a> {
    /// `raw` ＝ 一行原文（不含换行；BOM 与两端空白这里去掉），`start` ＝ 它在文件里的起点字节偏移。
    pub(crate) fn new(raw: &'a str, start: u64) -> Self {
        Line {
            text: raw.trim_start_matches('\u{feff}').trim(),
            start,
            value: std::cell::OnceCell::new(),
        }
    }

    /// 同 [`Self::new`]，当场把整行解好：几问都要整行的那一路（实时 `send_line`）用它，只要几格的那几问也从这一份里取。
    pub(crate) fn whole(raw: &'a str, start: u64) -> Self {
        let line = Self::new(raw, start);
        line.value();
        line
    }

    /// 原文（预筛用：认得出不是这一类的行就不解）。
    pub(crate) fn text(&self) -> &'a str {
        self.text
    }

    /// 起点字节偏移（没有自己身份的记录拿它合成 id，[`line_id`]）。
    pub(crate) fn start(&self) -> u64 {
        self.start
    }

    /// 是不是空行（纯空白 / 纯 BOM）。
    pub(crate) fn is_blank(&self) -> bool {
        self.text.is_empty()
    }

    /// 整行（解过就共用那一份）；连 JSON 都不是 ⇒ `None`。
    pub(crate) fn value(&self) -> Option<&serde_json::Value> {
        self.value
            .get_or_init(|| {
                count_parse();
                serde_json::from_str(self.text).ok()
            })
            .as_ref()
    }

    /// 整行拿走（解过就是那一份，没解过当场解）；连 JSON 都不是 ⇒ `None`。
    pub(crate) fn into_value(self) -> Option<serde_json::Value> {
        self.value();
        self.value.into_inner().flatten()
    }

    /// 只要几格：整行解过就从那一份里取（树上走，不再解文本），没解过就从文本直接解那几格。
    pub(crate) fn read<T: serde::de::DeserializeOwned>(&self) -> serde_json::Result<T> {
        match self.value.get() {
            Some(Some(v)) => T::deserialize(v),
            _ => {
                count_parse();
                serde_json::from_str(self.text)
            }
        }
    }
}

thread_local! {
    /// 本线程到此刻从**文本**解析了几次记录行（[`Line::value`] · [`Line::read`] 记）：判据数「一行解几遍」。
    pub(crate) static LINE_PARSES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn count_parse() {
    LINE_PARSES.with(|c| c.set(c.get() + 1));
}

/// 一行在文件里的起点字节偏移 ⇒ 没有自己身份的记录用的那个 id（会话内唯一、各条读路给出的都一样）。
pub(crate) fn line_id(start: u64) -> String {
    format!("@{start}")
}

/// 注册表里 `kind` 那一家。认不出 ⇒ `None`。
fn adapter_among<'a>(registry: &'a [Adapter], kind: &str) -> Option<&'a Adapter> {
    registry.iter().find(|a| a.kind == kind)
}

/// 给定注册表里**恰一家**满足 `has` ⇒ 它的 kind；一家都没有、或不止一家 ⇒ `None`（显式失败，不按注册序挑）。
/// 请求里没说是哪一家、而那一格今天只有一家声明的那几处用它（调用行写明是哪一格）。
pub(crate) fn sole_kind_among(
    registry: &[Adapter],
    has: impl Fn(&Adapter) -> bool,
) -> Option<&'static str> {
    let mut hit = registry.iter().filter(|a| has(a));
    match (hit.next(), hit.next()) {
        (Some(a), None) => Some(a.kind),
        _ => None,
    }
}

/// [`sole_kind_among`] 在生产注册表上。
pub(crate) fn sole_kind(has: impl Fn(&Adapter) -> bool) -> Option<&'static str> {
    sole_kind_among(REGISTRY, has)
}

/// 资产面那一家（读 / 装 skill · 用户级 MCP 文件）：注册表里**唯一**声明了资产面的那一家（今天只有 Claude）；两家都声明 ⇒ `None`、照实拒。
/// 资产域 · 账号库的 MCP 共享 · cc-bus 钩子都问它（原住 `assets/mod.rs`）。
/// ⚠ Codex 的 skill 根来了（扩展位置带上程序这一维），改这里：由请求说是哪一家。
pub(crate) fn asset_kind() -> Option<&'static str> {
    sole_kind(|a| a.assets.is_some())
}

/// `kind` 那一家的记录解释面。认不出 ⇒ `None`。
pub(crate) fn record_face(kind: &str) -> Option<RecordFace> {
    adapter_among(REGISTRY, kind).and_then(|a| a.records)
}

/// `kind` 那一家的记录树（[`RecordFace::tree`]）。认不出 / 那一家的会话不住记录树 ⇒ `None`。
pub(crate) fn record_tree_among(registry: &[Adapter], kind: &str) -> Option<RecordTree> {
    adapter_among(registry, kind).and_then(|a| a.records?.tree)
}

/// `kind` 那一家家目录 `home` 下记录树的根。那一家没有记录树 ⇒ `None`。
pub(crate) fn records_root_among(registry: &[Adapter], kind: &str, home: &Path) -> Option<PathBuf> {
    record_tree_among(registry, kind).map(|t| (t.root)(home))
}

/// 记录树根底下 id 为 `child_id` 的子运行挂在哪个会话底下（父记录的会话 id）；对不上 ⇒ `None`。问的是记录树那一家（[`record_tree_kind`]）。
pub(crate) fn child_parent_sid(records_root: &Path, child_id: &str) -> Option<String> {
    child_parent_sid_among(REGISTRY, record_tree_kind()?, records_root, child_id)
}

/// [`child_parent_sid`] 的可喂夹具那一半：问 `kind` 那一家。
pub(crate) fn child_parent_sid_among(
    registry: &[Adapter],
    kind: &str,
    records_root: &Path,
    child_id: &str,
) -> Option<String> {
    let r = adapter_among(registry, kind)?.records?;
    let ch = r.children?;
    let parent = (ch.owner)(&(ch.find)(records_root, child_id)?)?;
    (r.sid)(&parent)
}

/// [`records_root_among`] 在生产注册表上、记录树那一家（[`record_tree_kind`]）。
pub(crate) fn records_root(home: &Path) -> Option<PathBuf> {
    records_root_among(REGISTRY, record_tree_kind()?, home)
}

/// [`RecordTree::file_name`]：`kind` 那一家新会话文件的文件名。那一家没有记录树 ⇒ `None`。
pub(crate) fn session_file_name(kind: &str, sid: &str) -> Option<String> {
    record_tree_among(REGISTRY, kind).map(|t| (t.file_name)(sid))
}

/// 这份会话记录归的那一家（[`record_kind_of`]）给 `sid` 起的文件名。那一家没有记录树 ⇒ `None`。
pub(crate) fn session_file_name_of(p: &Path, sid: &str) -> Option<String> {
    session_file_name(record_kind_of(p)?, sid)
}

/// `kind` 那一家认不认这份文件是它的会话记录（[`RecordFace::is_session_file`]）。认不出这一家 ⇒ `false`。
pub(crate) fn is_session_file_among(registry: &[Adapter], kind: &str, p: &Path) -> bool {
    adapter_among(registry, kind)
        .and_then(|a| a.records)
        .is_some_and(|r| (r.is_session_file)(p))
}

/// [`is_session_file_among`] 在生产注册表上、记录树那一家。
pub(crate) fn is_tree_session_file(p: &Path) -> bool {
    record_tree_kind().is_some_and(|k| is_session_file_among(REGISTRY, k, p))
}

/// 这份文件归的那一家（[`record_face_of`]）认不认它是一份会话记录。
pub(crate) fn is_own_session_file(p: &Path) -> bool {
    record_face_of(p).is_some_and(|r| (r.is_session_file)(p))
}

/// 后端盯着的那一家（[`tree_local_face`]）的家目录（[`LocalFace::home_at`]）。
/// 注册表里没有那一家 ⇒ 给了配置目录就是它，否则空路径，并记一句警告（生产注册表里恒有那一家）。
pub fn home_at(config_dir: Option<&Path>, from_env: bool) -> PathBuf {
    match tree_local_face() {
        Some(f) => (f.home_at)(config_dir, from_env),
        None => {
            tracing::warn!("no agent in the registry keeps a record tree with a local layout");
            config_dir.map(Path::to_path_buf).unwrap_or_default()
        }
    }
}

/// 后端盯着的那一家（[`tree_local_face`]）家目录 `home` 下的 pidfile 目录（[`LocalFace::pidfile_dir`]）。
/// 没有那一家 ⇒ 家目录下一个不会有 pidfile 的位置（读出零份）。判活与账号归属读同一处。
pub(crate) fn pidfile_dir(home: &Path) -> PathBuf {
    tree_local_face().map_or_else(|| home.join(NO_PIDFILES), |f| (f.pidfile_dir)(home))
}

/// 后端盯着的那一家的进程状态文件 `v` 说这是后台会话（[`LocalFace::background_of`]）。没有那一家 ⇒ `false`。
pub(crate) fn pidfile_background(v: &serde_json::Value) -> bool {
    tree_local_face().is_some_and(|f| (f.background_of)(v))
}

/// 后端盯着的那一家的进程状态文件 `v` 说此刻在干什么（[`LocalFace::activity_of`]）。没有那一家 / 说不清 ⇒ `None`。
pub(crate) fn pidfile_activity(v: &serde_json::Value) -> Option<SessionActivity> {
    tree_local_face().and_then(|f| (f.activity_of)(v))
}

/// 后端盯着的那一家的进程状态文件 `v` 说在等什么框（[`LocalFace::wait_of`]）。没有那一家 / 没说 / 说不清 ⇒ `None`。
pub(crate) fn pidfile_wait(v: &serde_json::Value) -> Option<WaitOn> {
    tree_local_face().and_then(|f| (f.wait_of)(v))
}

/// 注册表里没有判活那一家时 [`pidfile_dir`] 指的那个名字（不建、不写，只读出零份）。
const NO_PIDFILES: &str = ".no-pidfile-dir";

/// `kind` 那一家在本机的布局（[`Adapter::local`]）。认不出 / 没有 ⇒ `None`。
pub(crate) fn local_face_among(registry: &[Adapter], kind: &str) -> Option<LocalFace> {
    adapter_among(registry, kind).and_then(|a| a.local)
}

/// 后端盯着的那一家（记录树那一家，[`record_tree_kind`]）在本机的布局。
pub(crate) fn tree_local_face() -> Option<LocalFace> {
    local_face_among(REGISTRY, record_tree_kind()?)
}

/// 一份已过围栏的会话记录是哪一家的：落在某一家合成历史的根下 ⇒ 那一家；否则 ⇒ 家目录记录树那一家（[`record_tree_kind`]）。
pub(crate) fn record_kind_of(path: &Path) -> Option<&'static str> {
    record_kind_among(REGISTRY, path)
}

/// [`record_kind_of`] 的可喂夹具那一半。
pub(crate) fn record_kind_among(registry: &[Adapter], path: &Path) -> Option<&'static str> {
    // 根解开链接再比（Windows 上那一形带扩展长度前缀）⇒ 两边照同一种规范化比（`is_under`）。
    let under = |root: PathBuf| {
        let root = Ro::canonicalize(&root).unwrap_or(root);
        crate::platform::paths::is_under(path, &root)
    };
    registry
        .iter()
        .find(|a| a.history.and_then(|h| (h.root)()).is_some_and(under))
        .map(|a| a.kind)
        .or_else(|| record_tree_kind_among(registry))
}

/// 这份会话记录归的那一家的记录解释面（[`record_kind_of`]）。
pub(crate) fn record_face_of(path: &Path) -> Option<RecordFace> {
    record_face_among(REGISTRY, path)
}

/// [`record_face_of`] 的可喂夹具那一半。
pub(crate) fn record_face_among(registry: &[Adapter], path: &Path) -> Option<RecordFace> {
    record_kind_among(registry, path)
        .and_then(|k| adapter_among(registry, k))
        .and_then(|a| a.records)
}

/// 家目录记录树那一家的 kind：**恰一家**有记录解释面、却没有合成历史面（它的会话住在按项目分的记录树里）。
/// 历史清单的记录树那一支、全文搜索、流式 watcher 跟的都是它。不止一家 ⇒ `None`（说不清是哪一家，不猜）。
pub(crate) fn record_tree_kind() -> Option<&'static str> {
    record_tree_kind_among(REGISTRY)
}

fn record_tree_kind_among(registry: &[Adapter]) -> Option<&'static str> {
    sole_kind_among(registry, |a| a.history.is_none() && a.records.is_some())
}

/// 流式 watcher 跟的一家：它的记录住哪个根 · 记录解释面 · 它的会话靠什么判活。
#[derive(Clone)]
pub(crate) struct Followed {
    pub(crate) kind: &'static str,
    /// 记录根（其下每份会话记录一个文件；子目录怎么分是那一家的事）。
    pub(crate) root: PathBuf,
    pub(crate) face: RecordFace,
    pub(crate) live: LiveBy,
}

/// 一家的会话怎么判活（[`Adapter::live`]）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LiveBy {
    /// 每个活会话的进程在 pidfile 目录（[`LocalFace::pidfile_dir`]）里留一份进程状态文件：pidfile 在、进程在 ⇒ 活。
    Pidfiles,
    /// 没有 pidfile：会话在世期间写记录的那个进程一直开着那份记录 ⇒ 有进程开着它写就是活，
    /// 那个进程退了或把它关了就是走了（`platform::writers`）。
    HeldOpen,
    /// 判不了（这一家既不留 pidfile、也不一直开着记录写，或没人考据过）。
    Unknown,
}

/// 某一家（wire 上的 kind）在给定注册表里怎么判活；认不出 ⇒ [`LiveBy::Unknown`]。
pub(crate) fn live_by_among(registry: &[Adapter], kind: &str) -> LiveBy {
    registry
        .iter()
        .find(|a| a.kind == kind)
        .map_or(LiveBy::Unknown, |a| a.live)
}

/// 生产注册表里那一家怎么判活（[`live_by_among`]）。
pub(crate) fn live_by(kind: &str) -> LiveBy {
    live_by_among(REGISTRY, kind)
}

/// 流式 watcher 跟的那几家（注册序）：家目录记录树那一家（[`record_tree_kind`]，`tree_home` 是它的家目录，判活认 pidfile）·
/// 记录住在合成历史根下、会话在世期间一直开着记录写的那几家（判活认写者）。别的不跟。
pub(crate) fn followed(tree_home: &Path) -> Vec<Followed> {
    followed_among(REGISTRY, tree_home)
}

/// [`followed`] 的可喂夹具那一半。
pub(crate) fn followed_among(registry: &[Adapter], tree_home: &Path) -> Vec<Followed> {
    let tree_kind = record_tree_kind_among(registry);
    registry
        .iter()
        .filter_map(|a| {
            let face = a.records?;
            let follows = match a.live {
                LiveBy::Pidfiles => Some(a.kind) == tree_kind,
                LiveBy::HeldOpen => true,
                LiveBy::Unknown => false,
            };
            let root = records_root_of(a, tree_home).filter(|_| follows)?;
            Some(Followed {
                kind: a.kind,
                root,
                face,
                live: a.live,
            })
        })
        .collect()
}

/// 找项目目录时记录开头至多读多少字节。带它的那一条实测在前一 KiB 内；上界是给几百 MB 的大会话的：不整读。
pub(crate) const HEAD_CAP: u64 = 1 << 20;

/// 记录开头逐条交给 `pick`，第一个给出值的胜；读满 [`HEAD_CAP`] 字节就停（截在半截的那一行解析不出、不算）。
/// 读满了还没找到 ⇒ `None`，并在日志里说是哪份文件（调用方退到「没有这一格」）。
pub(crate) fn first_in_head(
    p: &Path,
    pick: impl Fn(&serde_json::Value) -> Option<String>,
) -> Option<String> {
    use std::io::{BufRead, Read};
    let file = Ro::open(p).ok()?;
    let mut r = std::io::BufReader::new(file.take(HEAD_CAP));
    let mut line = Vec::new();
    let mut read = 0u64;
    loop {
        line.clear();
        let n = r.read_until(b'\n', &mut line).ok()?;
        if n == 0 {
            if read >= HEAD_CAP {
                tracing::warn!(
                    "{}: nothing found within the head cap; giving up",
                    p.display()
                );
            }
            return None;
        }
        read += n as u64;
        let one = line.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&line);
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(one) {
            if let Some(got) = pick(&v) {
                return Some(got);
            }
        }
    }
}

/// 这份会话记录的项目目录：记录归哪一家就问哪一家（[`RecordFace::project_dir`]）；那一家不声明 / 开头里没有 ⇒ `None`。
pub(crate) fn project_dir_of(p: &Path) -> Option<String> {
    project_dir_among(REGISTRY, p)
}

/// [`project_dir_of`] 的可喂夹具那一半。
pub(crate) fn project_dir_among(registry: &[Adapter], p: &Path) -> Option<String> {
    record_face_among(registry, p)
        .and_then(|f| f.project_dir)
        .and_then(|f| f(p))
}

/// 这份记录归的那一家若有合成历史面（记录不住记录树、首句另有读法）⇒ 那一家的首句摘录法
/// （与历史清单那一行同一个，[`HistoryFace::excerpt`]）；归记录树那一家 · 认不出 ⇒ `None`。
pub(crate) fn synth_excerpt_among(registry: &[Adapter], p: &Path) -> Option<fn(&Path) -> String> {
    let kind = record_kind_among(registry, p)?;
    adapter_among(registry, kind)?.history.map(|h| h.excerpt)
}

/// 注册表里每一家的漂移账并成一份（注册序；不记的跳过）—— 帧命令 `drift-report` 的读法入口。
pub(crate) fn drift_report() -> drift::DriftReport {
    drift::DriftReport {
        faces: REGISTRY
            .iter()
            .filter_map(|a| a.records.and_then(|r| r.drift))
            .flat_map(|f| f())
            .collect(),
    }
}

/// 见 [`Adapter::footprint`]。
#[derive(Clone, Copy)]
pub(crate) struct FootprintFace {
    pub(crate) tools: &'static [footprint_decl::ToolSpec],
    pub(crate) env: &'static [footprint_decl::UnmanagedEnv],
    pub(crate) under_agent_home: fn(&str) -> Option<&str>,
    pub(crate) user_settings: fn(&Path) -> [PathBuf; 2],
}

/// 一份身份文件的原生根：没设账号环境变量时它住哪。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityRoot {
    /// 住 `<配置根>/<名>`（设了账号环境变量就跟着走）。
    ConfigDir,
    /// 住 `$HOME/<名>`（不跟账号环境变量走）。
    Home,
}

/// 一份身份文件是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityClass {
    /// 身份本体：必须 600、必须每个号一份、绝不从别的号复制。
    Secret,
    /// 本机状态：每个号一份（共享会串号，或高频写互相覆盖）。
    State,
    /// 跟着别的项走的附属物（备份之类）。
    Derived,
}

/// 见 [`Adapter::accounts`]。
#[derive(Clone, Copy)]
pub(crate) struct AccountsFace {
    /// 每个号各有一份的那几项：`(名, 原生根, 类别)`；共享库顶层其余每一项都链回共享库。
    pub(crate) identity: &'static [(&'static str, IdentityRoot, IdentityClass)],
    /// 那一家按需才在配置根下建、各号该共用的目录：共享库里还没有也照样链（先在共享库里建出来）。
    pub(crate) shared_on_demand: &'static [&'static str],
    /// 账号级配置文件的名字（原生根是家目录的那一份，账号 0 的住 `$HOME`）。
    pub(crate) config_file: &'static str,
    /// 那份配置文件里装用户级 MCP 的顶层键（账号之间同步的就是它，别的键不碰）。
    pub(crate) user_mcp_key: &'static str,
    /// 没设账号时的配置根（= 各号链回去的共享库）。
    pub(crate) shared_root: fn(&Path) -> PathBuf,
    /// 一个配置根下登录的邮箱（读不到 ⇒ `None`）。
    pub(crate) email_in: fn(&Path) -> Option<String>,
    /// 各号的订阅号登录那份文件的格式（各号之间同步 MCP 登录读写它）；没有 ⇒ `None`。
    pub(crate) login: Option<LoginFace>,
    /// 在 app 里登录一个号那一形（起那一家的登录子命令、认链接、判粘回来的东西）；不在 app 里登 ⇒ `None`。
    pub(crate) sign_in: Option<SignInFace>,
    /// 账号归属读会话进程环境时读哪几个键（账号 · 上游地址）。
    pub(crate) session_env: SessionEnvKeys,
    /// 一个配置根下、对某个 cwd 的信任状态 ⇒ 一行 JSON（`{trusted, known, error}`）；读不了 ⇒ `(码, 原话)`。
    pub(crate) trust_in: fn(root: &Path, cwd: &str) -> Result<String, (String, String)>,
    /// 「这个目录信任过」在那份配置文件里记在哪一格（各号之间同步信任 · 起会话前预标都只碰它）；没有这件事的家 ⇒ `None`。
    pub(crate) trust: Option<TrustCells>,
}

/// 信任记在配置文件里的那一格：顶层 `table` 是「目录 → 那个目录的一项」的表；`flag` 是那一项里记「信任过」的那一格
/// （`None` ⇒ 那一项本身就是 `true`）。通用层只按这几格读写，不认任何一家的键名。
#[derive(Clone, Copy)]
pub(crate) struct TrustCells {
    pub(crate) table: &'static str,
    pub(crate) flag: Option<&'static str>,
    /// 一个工作目录要标表里哪几个键（这一家进程查 / 存信任认的那几形，如解开符号链接 · 换分隔符 · 所在仓的根）；
    /// 通用层只把这几个键都标上，不判那一家先查哪一个。
    pub(crate) dir_keys: fn(&str) -> Vec<String>,
}

/// 账号归属从会话进程环境里读的那两个键，收成一处：账号（配置根）· 上游地址。
#[derive(Clone, Copy)]
pub(crate) struct SessionEnvKeys {
    pub(crate) config_dir: &'static str,
    pub(crate) base_url: &'static str,
    /// 这条会话自己的设置文件会不会压过进程环境里的上游地址：`(配置根, cwd, 家目录)` ⇒ 说不清就是 `true`。
    pub(crate) settings_may_set_base_url: fn(Option<&Path>, &Path, Option<&Path>) -> bool,
}

/// `kind` 那一家的账号库布局。认不出 / 那一家没有 ⇒ `None`。
pub(crate) fn accounts_face(kind: &str) -> Option<AccountsFace> {
    accounts_face_among(REGISTRY, kind)
}

/// 这台机器上账号库的布局：唯一声明了账号库的那一家（[`sole_kind`]；两家都声明 ⇒ `None`、照实拒）。
/// 账号库布局 · 账号归属读会话进程环境 · 信任预检都问它。
pub(crate) fn account_library_face() -> Option<AccountsFace> {
    sole_kind(|a| a.accounts.is_some()).and_then(accounts_face)
}

/// [`accounts_face`] 的可喂夹具那一半。
pub(crate) fn accounts_face_among(registry: &[Adapter], kind: &str) -> Option<AccountsFace> {
    adapter_among(registry, kind).and_then(|a| a.accounts)
}

/// `kind` 那一家家目录 `agent_home` 下用户级的那份设置文件（足迹面 `user_settings` 的第一份）。那一家没有足迹面 ⇒ `None`。
pub(crate) fn user_settings_of(kind: &str, agent_home: &Path) -> Option<PathBuf> {
    adapter_among(REGISTRY, kind)
        .and_then(|a| a.footprint)
        .map(|f| {
            let [user, _] = (f.user_settings)(agent_home);
            user
        })
}

/// 注册表里带足迹面的那几家（按注册表顺序）。
pub(crate) fn footprint_faces() -> impl Iterator<Item = FootprintFace> {
    REGISTRY.iter().filter_map(|a| a.footprint)
}

/// 一家的 MCP 读面：函数指针（同 [`Adapter::home`]，不立 trait）。入参是项目目录（可缺）＋ 判状态要看的那几个家。
#[derive(Clone, Copy)]
pub(crate) struct McpFace {
    pub(crate) read: fn(Option<&Path>, &McpLook) -> McpRead,
    /// 在那一家的会话里登录 MCP 服务器要敲的那条命令（扩展页「去登录」· 待办那一件复制它）。
    pub(crate) login_command: &'static str,
    /// 在 app 里登录一个 MCP 服务器那一形（子命令最后再接服务器名）；`None` ＝ 只能在会话里登。
    pub(crate) sign_in: Option<SignInFace>,
    /// 各号家目录里记「要登录」的那份文件（盯它：翻过来 ⇒ 会话外的需手动清单可能变了）。
    pub(crate) needs_login_file: Option<&'static str>,
}

/// 判状态要看的：这台各号的家目录（账号库里的名字 · 那个号的家；没设账号的那一份名字是 `None`）＋ 此刻（epoch ms）。
/// 账号库是通用层的知识，由通用层收齐交给那一家；那一家只认自己家目录里的文件。
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct McpLook {
    pub homes: Vec<(Option<String>, PathBuf)>,
    pub now_ms: u64,
}

/// 一条 MCP server 此刻的状态（中立值；各家的字由适配层翻成它；线上名即 serde 名，闭集）。
/// 配置层（`mcp-read`）只说说得准的：停用（配置里写着）· 要登录（那一家最近一次连它时记下的、还在有效期内）· 其余 `Unknown`；
/// 会话事实（`history-facts` 的 `mcp`）只列那一家在记录里说有毛病的：要登录 · 断开 · 还在连。两处都不说「连上了」。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) enum McpStatus {
    NeedsLogin,
    Failed,
    Pending,
    Disabled,
    #[default]
    Unknown,
}

/// 一条记录里那一家说的各 MCP 服务器此刻的样子：三张表各一格；这一条没写的那一格 ⇒ `None`（沿用上一条说的）。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct McpSaid {
    pub pending: Option<Vec<String>>,
    pub needs_login: Option<Vec<String>>,
    /// 断开的：名字 · 那一家写的原话（没写 ⇒ `None`）。
    pub failed: Option<Vec<(String, Option<String>)>>,
}

/// `kind` 那一家在这条记录里说的 MCP 样子。那一家不说 / 不是这种记录 ⇒ `None`。
pub(crate) fn mcp_said_of(kind: &str, v: &serde_json::Value) -> Option<McpSaid> {
    record_face(kind)
        .and_then(|r| r.mcp_said)
        .and_then(|f| f(v))
}

/// `kind` 那一家：这条记录是认不出、又不成卡的那种行 ⇒ 它的类型名（会话事实 `quietUnread` 记数用）。
pub(crate) fn quiet_unread_of(kind: &str, v: &serde_json::Value) -> Option<String> {
    record_face(kind)
        .and_then(|r| r.quiet_unread)
        .and_then(|f| f(v))
}

/// 一家读出来的 MCP 事实：条目（user / local / project）· 用过的项目目录（`dirs`）· 读不出来的那几份（说出来，不当成空）。
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct McpRead {
    pub entries: Vec<McpEntry>,
    pub dirs: Vec<String>,
    pub problems: Vec<String>,
}

/// 一条 MCP server：`server` 原样（宽容，未知字段不丢）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct McpEntry {
    pub scope: &'static str,
    pub name: String,
    pub server: serde_json::Value,
    pub source: String,
    pub status: McpStatus,
    /// 要登录：在哪几个号里（账号库里的名字，排好序）；没设账号的那一份不出名字。别的状态恒空。
    pub login_in: Vec<String>,
    /// 要登录：最近一次看到是何时（epoch ms）；别的状态 `None`。
    pub seen_ms: Option<u64>,
}

/// `kind` 那一家的 MCP 读面，读一遍。`None` = 那一家不认得 MCP。
pub(crate) fn mcp_read_among(
    registry: &[Adapter],
    kind: &str,
    project_dir: Option<&Path>,
    look: &McpLook,
) -> Option<McpRead> {
    adapter_among(registry, kind)
        .and_then(|a| a.mcp)
        .map(|f| (f.read)(project_dir, look))
}

/// `kind` 那一家登录 MCP 服务器的那条命令（[`McpFace::login_command`]）。那一家不认得 MCP ⇒ `None`。
pub(crate) fn mcp_login_command(kind: &str) -> Option<&'static str> {
    adapter_among(REGISTRY, kind)
        .and_then(|a| a.mcp)
        .map(|f| f.login_command)
}

/// `kind` 那一家的 MCP 面（[`McpFace`]）。那一家不认得 MCP ⇒ `None`。
pub(crate) fn mcp_face(kind: &str) -> Option<McpFace> {
    adapter_among(REGISTRY, kind).and_then(|a| a.mcp)
}

/// 这台注册表里各家记「MCP 要登录」的那份文件名（watcher 认它）。
pub(crate) fn mcp_needs_login_files() -> Vec<&'static str> {
    REGISTRY
        .iter()
        .filter_map(|a| a.mcp.and_then(|m| m.needs_login_file))
        .collect()
}

/// `kind` 那一家没设账号时的家目录（注册表 `home` 那一格）。认不出 / 说不出 ⇒ `None`。
pub(crate) fn home_of_kind(kind: &str) -> Option<PathBuf> {
    adapter_among(REGISTRY, kind).and_then(|a| (a.home)())
}

/// [`mcp_read_among`] 对本机注册表 —— 帧命令 `mcp-read` 的读法入口。
pub(crate) fn mcp_read(kind: &str, project_dir: Option<&Path>, look: &McpLook) -> Option<McpRead> {
    mcp_read_among(REGISTRY, kind, project_dir, look)
}

/// 一家的默认上游：路由里叫它什么 · 盖掉内置默认的那个旋钮 · 内置默认。三格焊在一起
/// （分开取就写得出「A 家的旋钮配 B 家的默认值」—— 与上游选择那张表的 `Row` 焊住上游与 key 同一条理由）。
pub(crate) struct DefaultUpstream {
    /// 中转路由键第 1 段里这一家的名字（monitor 起会话拼 `/t/<它>/…`）。⚠ 与 [`Adapter::kind`]（wire 上的 `agent_kind`）
    /// 是两个值域：Claude 在 wire 上叫 `claude`、在路由里叫 `claude-code` —— 各有各的既有契约，不在这里对齐。
    pub(crate) route_id: &'static str,
    /// 盖掉内置默认的环境变量名。**每家一个**（「每家一个」：不留「覆盖哪一家说不清」的全局旋钮）。
    pub(crate) env: &'static str,
    /// 没配 `env` 时这一家发到哪儿（配了 ⇒ 一律发到那个值）。
    pub(crate) fallback: Fallback,
    /// 这一家的请求里**它自己带着会话标识**的那个头（中转拿它给流打标签）。`None` = 说不出 ⇒ 流不带标签。
    pub(crate) session_header: Option<&'static str>,
    /// 这一家的上游说哪种流协议（归一流的折法）。`None` ＝ 它的流不折（活卡认不得）。
    pub(crate) stream: Option<StreamFace>,
    /// 请求本身就说出「我是哪个子运行」的那个头（值 ＝ 子运行的标识，与 [`RecordFace::run_of`] 同一个值域）。
    /// 声明了 ⇒ 带头的请求归那个子运行、不带头的就是主运行（当场定）；`None` ＝ 请求认不出运行 ⇒ 通用层按记录对账归位。
    pub(crate) owner_header: Option<&'static str>,
    /// API key 凭据文件（界面给账号配第三方 key 时写的那一份）里的行挂在这一家名下。
    /// 那份文件没有 agent 这一维 ⇒ 至多一家为真；没有一家为真 ⇒ 上游选择起不来（fail-closed）。
    pub(crate) owns_credentials_file: bool,
    /// 直接敲的这一家也走中转（可选，用户自己贴、后端只读）：它的用户级设置文件里写进程环境的那一块。
    /// `None` ＝ 这一家没有这一形，只能经起会话注入。
    pub(crate) settings_env: Option<SettingsEnvFace>,
    /// 请求里说明「这一轮用的是扩展上下文」的那一项：（头名, 列表里那一项的前缀）。中转只记它在不在，会话事实据此定上限。
    /// `None` ＝ 这一家的请求说不出。
    pub(crate) context_mark: Option<(&'static str, &'static str)>,
    /// 回包状态 ＋ 回包头（＋ 此刻，unix 秒）→ 通用的额度快照：这一家的读法（头名只住那一家）。
    /// `None` ＝ 这一家的回包说不出额度；`Some` 答 `None` ＝ 这一个回包里没有额度信息。
    pub(crate) quota: Option<QuotaRead>,
    /// 额度窗口名 → 语义位（`5h` / `7d`；别的窗口 ⇒ `None`）。界面与换号记录只认语义位，不认各家的窗口名。
    pub(crate) window_slot: Option<fn(&str) -> Option<&'static str>>,
    /// 额度窗口名 → 窗口键（轮换的上限 · 单段预算按它配；通用层当不透明的键，只认 `5h` · `7d` 两个缺省键）。
    /// 不算用量窗口的那几档（超额）⇒ `None`。
    pub(crate) window_key: Option<fn(&str) -> Option<String>>,
    /// 让这一家的官方客户端报一次某个号的用量（不花模型额度）：怎么起 · 怎么把输出读成窗口。`None` ＝ 这一家没有这一形。
    pub(crate) usage: Option<UsageFace>,
    /// 订阅号登录那一格：令牌住哪、什么格式、怎么续、锁叫什么。`None` ＝ 这一家没有可换的订阅号登录。
    pub(crate) login: Option<LoginFace>,
    /// 这一家自己认得的「用满」回包（轮换的硬上限用它：这一发不发上游、回这一份）。`None` ＝ 给不出 ⇒ 对它硬上限不成立、按软阈值办。
    pub(crate) limit_reply: Option<LimitReplyOf>,
    /// 起会话时怎么把这一家指到中转（`ccm` 最终 exec 那一处照这一格做）。是 [`Inject::Env`] 的那一家，
    /// 继承来的那一条地址（用户自己的端点 · 别的号的中转）也只看那个变量。
    pub(crate) inject: Inject,
}

/// 一份「用满」回包：状态行里状态码那一截 · 头 · 体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LimitReply {
    pub(crate) status: &'static str,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

/// （几点重置 unix 秒, 此刻 unix 秒, 卡着的那个窗口的窗口键 `5h` · `7d` · `7d:<模型>`，说不出 ⇒ `None`）→ 这一家的「用满」回包。
pub(crate) type LimitReplyOf = fn(u64, u64, Option<&str>) -> LimitReply;

/// 一家「让官方客户端报用量」那一形：起哪个程序 · 带什么参数 · 号的配置目录交给哪一格环境 · 起它时摘 / 设哪几格 · 输出怎么读。
/// 通用层（帧命令 `quota-probe`）只照它起、照它读，读成的窗口记进额度账、标来源 `usage`。
#[derive(Debug, Clone, Copy)]
pub(crate) struct UsageFace {
    pub(crate) program: &'static str,
    pub(crate) args: &'static [&'static str],
    /// 号的配置目录交给这一格；账号 0（默认配置目录）⇒ 摘掉它。
    pub(crate) dir_env: &'static str,
    /// 起它时摘掉的（会盖掉那个号自己的登录的那几格）。
    pub(crate) env_remove: &'static [&'static str],
    /// 起它时设上的（如让输出里的时刻按 UTC 写）。
    pub(crate) env_set: &'static [(&'static str, &'static str)],
    /// （标准输出, 此刻 unix 秒）→ 窗口；读不懂 ⇒ `Err(哪一处读不懂)`（不猜）。
    pub(crate) read: fn(&str, u64) -> Result<Vec<QuotaWindow>, String>,
}

/// 路由第 1 段 → 那一家的「报用量」那一形。
pub(crate) fn usage_of(route_id: &str) -> Option<UsageFace> {
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .find(|u| u.route_id == route_id)
        .and_then(|u| u.usage)
}

/// 路由第 1 段 → 那一家的「用满」回包（见 [`DefaultUpstream::limit_reply`]）。
pub(crate) fn limit_reply_of(route_id: &str) -> Option<LimitReplyOf> {
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .find(|u| u.route_id == route_id)
        .and_then(|u| u.limit_reply)
}

/// 怎么把一家指到中转：只管地址放哪。地址由上游选择给（不带钥匙），`ccm` exec 那一刻插钥匙，插哪一把按地址的路由。
#[derive(Clone, Copy)]
pub(crate) enum Inject {
    /// 插上钥匙的地址放进这个环境变量。
    Env(&'static str),
    /// 地址拼成这几个参数（垫在透传之前）：地址只能经命令行参数交给它的那一家。计划里是不带钥匙的地址；
    /// 这一家只走 `/t/`（`/s/` 只给凭据文件那一家）⇒ 插进 argv 的恒是只许直通那一把。
    Args(fn(&str) -> Vec<String>),
}

/// 某一家（wire 上的 kind）在给定注册表里怎么指到中转。认不出 / 没登记默认上游 ⇒ `None`。
pub(crate) fn inject_among(registry: &[Adapter], kind: &str) -> Option<Inject> {
    registry
        .iter()
        .find(|a| a.kind == kind)
        .and_then(|a| a.upstream.as_ref())
        .map(|u| u.inject)
}

/// [`inject_among`] 在生产注册表上。
pub(crate) fn inject_of(kind: &str) -> Option<Inject> {
    inject_among(REGISTRY, kind)
}

/// 一家的内置默认上游：一个，或按这一发自己带没带某个头二选一（同一家两种登录各有真上游时；只看头名、不看值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fallback {
    One(&'static str),
    ByHeader {
        header: &'static str,
        present: &'static str,
        absent: &'static str,
    },
}

/// 一家在 app 里登录一个号那一形（本体 `accounts/sign_in.rs`：起它、转链接、递码；令牌那一家自己写）。
#[derive(Clone, Copy)]
pub(crate) struct SignInFace {
    /// 登录子命令（程序照这台的 `PATH` 找）。
    pub(crate) program: &'static str,
    pub(crate) args: &'static [&'static str],
    /// 号的配置目录交给这一格；账号 0（默认配置目录）⇒ 摘掉它。
    pub(crate) dir_env: &'static str,
    /// 起它时摘掉的（会盖掉那个号自己的登录的那几格）。
    pub(crate) env_remove: &'static [&'static str],
    /// 输出的一行里那条登录链接（只认那一家的登录页）；没有 ⇒ `None`。
    pub(crate) link_in: fn(&str) -> Option<String>,
    /// 粘回来的东西对不对得上这一趟的链接（格式 ＋ 这一趟的 state）。
    pub(crate) paste_fits: fn(pasted: &str, link: &str) -> bool,
    /// 输出的一行说「粘的码不对」（子命令接着等）。
    pub(crate) said_bad_paste: fn(&str) -> bool,
    /// 它要终端才肯问（起在伪终端里：`platform::child::Child::converse_in_terminal`）。
    pub(crate) terminal: bool,
    /// 粘回来的是什么（界面据此说「粘登录页上给的码」还是「粘浏览器地址栏里那一整条」）。
    pub(crate) paste: PasteShape,
    /// 它自己在它那一台上开浏览器打开登录页（不带「别开浏览器」那一类旗标）。
    pub(crate) opens_browser: bool,
}

/// 粘回来的那一段是什么（中立值；线上名由帧面那一份给）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PasteShape {
    /// 登录页上给的那一段码。
    Code,
    /// 浏览器回跳到本机那一整条地址（回不来时地址栏里那一条）。
    RedirectUrl,
}

/// 一家的订阅号登录的格式知识（读写与续期在账号域 `accounts/oauth/`，这里只有这一家的名字与地址）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct LoginFace {
    /// 配置目录里那份凭据文件的名字。
    pub(crate) creds_file: &'static str,
    /// 凭据文件里放令牌的那一节，与节里的几格。
    pub(crate) section: &'static str,
    pub(crate) access: &'static str,
    pub(crate) refresh: &'static str,
    /// 访问令牌几点过期（毫秒时间戳）。
    pub(crate) expires_ms: &'static str,
    pub(crate) scopes: &'static str,
    /// 这个号登录时用的客户端 id（缺 ⇒ [`Self::client_id`]）。
    pub(crate) client_field: &'static str,
    /// 令牌端点（续期发到这里）与缺省的客户端 id。
    pub(crate) token_url: &'static str,
    pub(crate) client_id: &'static str,
    /// 还剩这么多毫秒就算快过期（同那一家自己的余量）。
    pub(crate) margin_ms: u64,
    /// 续期锁：配置目录里那一把的名字 · 配置目录旁边那一把的后缀（那一家自己续期时拿的同一套）。
    pub(crate) lock_inside: &'static str,
    pub(crate) lock_beside: Option<&'static str>,
    /// 续期锁目录的修改时刻比现在早过这么多毫秒 ⇒ 持有方已经不在，当无主收回（删了重拿）。
    pub(crate) lock_stale_ms: u64,
    /// 那一家**整份写**这份凭据文件时拿的 `mkdir` 锁（配置目录里那一把的名字）：它在锁里重读盘、只改自己那一格、整份写回。
    /// 我方任何一次写这份文件也在这把锁里重读、比对、写（续期写回 · 各号之间同步 MCP 登录）。没有这一把的家 ⇒ `None`。
    pub(crate) write_lock: Option<&'static str>,
    /// 那把写锁的修改时刻早过这么多毫秒 ⇒ 当无主收回。
    pub(crate) write_lock_stale_ms: u64,
    /// 续成了、写回时那把写锁有人持着 ⇒ 新令牌先留在内存里照用，等它放开（文件事件）再写回；最多等这么多毫秒，
    /// 到期还没放开就照实报写回失败（那个号可能要重新登录）。比 [`Self::write_lock_stale_ms`] 长：到期那一刻再试一次，持有方早已不在的锁就收回了。
    pub(crate) write_wait_ms: u64,
    /// 同一份文件里另有一节「每个服务器一条」的登录（Claude 的 MCP 登录）；各号之间同步它。没有 ⇒ `None`。
    pub(crate) entries: Option<EntryLogins>,
    /// 一个号的账号身份住哪份文件：（配置目录, 账号 0 时的家目录）→ 那份文件。
    pub(crate) identity_file: fn(&Path, Option<&Path>) -> PathBuf,
    /// 那份文件里的账号身份（请求里跟着鉴权一起换的那一格的值）；读不到 ⇒ `None`。
    pub(crate) identity_in: fn(&Path) -> Option<String>,
    /// 请求体里那一格账号身份是谁（这一发实际在用的那份登录）；没有这一格 / 读不出 / 是空串 ⇒ `None`。
    pub(crate) identity_of_body: fn(&[u8]) -> Option<String>,
    /// 请求体里那一格账号身份换成给的值（按量号给空串）；只动那几个字节。
    pub(crate) rewrite_identity: fn(&[u8], &str) -> IdentityCell,
    /// 账号 0（没设配置目录的那个号）的配置目录：家目录 → 它。
    pub(crate) base_dir: fn(&Path) -> PathBuf,
}

/// 凭据文件里「每个服务器一条」的那一节登录的格式（各号之间同步 MCP 登录用；通用层只按这几格读写）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct EntryLogins {
    /// 那一节的顶层键（键 → 一条：访问令牌 · 刷新令牌 · 过期时刻 · 客户端登记 …，整条原样搬）。
    pub(crate) section: &'static str,
    pub(crate) access: &'static str,
    pub(crate) refresh: &'static str,
    pub(crate) expires_ms: &'static str,
    /// 那一家续某一条时在配置目录里拿的 `mkdir` 锁的名字（一段名字）：键 → 名字。
    pub(crate) renew_lock: fn(&str) -> String,
    /// 那把续期锁的修改时刻早过这么多毫秒 ⇒ 当无主收回。
    pub(crate) renew_lock_stale_ms: u64,
}

/// 请求体里账号身份那一格换没换成。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IdentityCell {
    /// 请求体里没有这一格 ⇒ 原样发。
    Absent,
    /// 换好了的整份请求体（只差那一格）。
    Rewritten(Vec<u8>),
    /// 有这一格，但认不准是哪几个字节 ⇒ 不能拿这份请求体换号。
    Unsure,
}

/// 路由第 1 段 → 那一家的额度窗口语义位读法。
pub(crate) fn window_slot_of(route_id: &str) -> Option<fn(&str) -> Option<&'static str>> {
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .find(|u| u.route_id == route_id)
        .and_then(|u| u.window_slot)
}

/// 路由第 1 段 → 那一家的额度窗口键读法。
pub(crate) fn window_key_of(route_id: &str) -> Option<fn(&str) -> Option<String>> {
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .find(|u| u.route_id == route_id)
        .and_then(|u| u.window_key)
}

/// 路由第 1 段 → 那一家的订阅号登录格式。
pub(crate) fn login_of(route_id: &str) -> Option<LoginFace> {
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .find(|u| u.route_id == route_id)
        .and_then(|u| u.login)
}

/// 一家的额度读法（见 [`DefaultUpstream::quota`]）。
pub(crate) type QuotaRead = fn(u16, &[(String, String)], u64) -> Option<QuotaReading>;

/// 那一家（kind）⇒ 它上游的流里怎么读出每发用量（[`StreamFace::spent`]）。账上记的路由名先经 [`kind_of_route`] 换。
pub(crate) fn spent_of(kind: &AgentKind) -> Option<fn(&str) -> Option<Spent>> {
    REGISTRY
        .iter()
        .find(|a| a.kind == kind.as_str())
        .and_then(|a| a.upstream.as_ref())
        .and_then(|u| u.stream?.spent)
}

/// 路由第 1 段 → 那一家的额度读法。
pub(crate) fn quota_read_of(route_id: &str) -> Option<QuotaRead> {
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .find(|u| u.route_id == route_id)
        .and_then(|u| u.quota)
}

/// 限流器对一发的结论（各家的值翻成这三档）：线上形住 `cmd-wire`（`quota-read` 带它）。
pub(crate) use cmd_wire::quota::QuotaStatus;

/// 一个额度窗口此刻的样子。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuotaWindow {
    /// 窗口名（那一家读法表里的值，如 5 小时 · 7 天）。
    pub(crate) name: String,
    /// 已用比例（通常 0–1，可以超过 1）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) used: Option<f64>,
    /// 几点重置（unix 秒）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) resets_at: Option<u64>,
    /// 越过了哪条预警线（比例）；没越过就没有。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) warned_at: Option<f64>,
}

/// 超额（付费额外用量）那一档。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuotaOverage {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) status: Option<QuotaStatus>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) resets_at: Option<u64>,
    /// 为什么不可用（那一家的原值）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) disabled: Option<String>,
    /// 这一发正在用超额。
    pub(crate) in_use: bool,
}

/// 一个回包读出来的额度快照（通用形状，账号域只认它）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuotaReading {
    /// 限流器的结论；回包里没说 ⇒ `None`。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) status: Option<QuotaStatus>,
    /// 这一发被上游拒了（没被服务）。
    pub(crate) refused: bool,
    /// 此刻卡着的那个窗口名。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) limiting: Option<String>,
    /// 卡着的那个窗口几点重置（被拒时：到这一刻之前这个号用不了）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) resets_at: Option<u64>,
    pub(crate) windows: Vec<QuotaWindow>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) overage: Option<QuotaOverage>,
}

/// 一家的用户级设置文件里「上游地址」那一格：住哪 · 怎么读出来 · 怎么合进去（格式知识与那一次只读都在这一家）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct SettingsEnvFace {
    /// 家目录（`$HOME`）→ 那份文件（各号共用的那一份）＋ 里面写的上游地址（**只读**）。
    pub(crate) read: fn(&Path) -> (PathBuf, SettingsBaseUrl),
    /// （那份文件现在的内容, 地址）→ 合好的整份（只算不写；「待办」按它算 diff）。现在的内容读不懂 ⇒ `None`。
    pub(crate) merge: fn(&str, &str) -> Option<String>,
    /// 地址住那份文件里哪一格（「待办」那一件的位置行）。
    pub(crate) slot: &'static str,
}

/// 设置文件里上游地址那一格读出来的样子。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SettingsBaseUrl {
    /// 文件不在，或没写（或写了空串）。
    Unset,
    /// 写了这个地址。
    Set(String),
    /// 读不了 / 读不懂 ⇒ 装没装说不清。
    Unreadable(SettingsUnreadable),
}

/// 为什么读不了。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SettingsUnreadable {
    /// 读的时候出错（权限 · I/O），带系统的原话。
    Io(String),
    /// 那不是一个普通文件。
    NotFile,
    /// 超过这个字节数。
    TooLarge(u64),
    /// 读得到但读不懂（坏 JSON · 顶层或那一块不是对象 · 值不是串）。
    BadShape,
}

/// 声明拥有 API key 凭据文件（[`DefaultUpstream::owns_credentials_file`]）的第一家的路由名；没有一家 ⇒ 空串（上游选择据此起不来）。
pub(crate) const fn credentials_file_agent() -> &'static str {
    let mut i = 0;
    while i < REGISTRY.len() {
        if let Some(u) = &REGISTRY[i].upstream {
            if u.owns_credentials_file {
                return u.route_id;
            }
        }
        i += 1;
    }
    ""
}

/// 登记了默认上游的每一家：`(路由名, 那一格)`。**上游选择读默认上游的唯一入口**。
pub(crate) fn default_upstreams() -> impl Iterator<Item = &'static DefaultUpstream> {
    REGISTRY.iter().filter_map(|a| a.upstream.as_ref())
}

/// 声明了「直接敲的也走中转」那一格的各家（注册表序）：（路由名, 给人看的名字, 那一格）。
pub(crate) fn settings_env_families() -> Vec<(&'static str, &'static str, SettingsEnvFace)> {
    REGISTRY
        .iter()
        .filter_map(|a| {
            let u = a.upstream.as_ref()?;
            let name = a.launch.map_or(u.route_id, |l| l.display_name);
            Some((u.route_id, name, u.settings_env?))
        })
        .collect()
}

/// 各家登记的会话标识头（注册序、去重）：中转经上游选择拿到这份名单，按它从请求里认会话 —— 会话 id 归 agent 自己。
pub(crate) fn session_headers() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for h in REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref()?.session_header)
    {
        if !v.contains(&h) {
            v.push(h);
        }
    }
    v
}

/// 各家登记的「请求说明用的是扩展上下文」的那一项（注册序、去重）：中转按它记每个会话的请求带没带。
pub(crate) fn context_marks() -> Vec<(&'static str, &'static str)> {
    let mut v: Vec<(&'static str, &'static str)> = Vec::new();
    for m in REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref()?.context_mark)
    {
        if !v.contains(&m) {
            v.push(m);
        }
    }
    v
}

/// 各家登记的「请求说出子运行」的头（注册序、去重）：中转按它从请求里认运行。
pub(crate) fn owner_headers() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for h in REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref()?.owner_header)
    {
        if !v.contains(&h) {
            v.push(h);
        }
    }
    v
}

/// 一家上游在流归位那里的样子：它的流协议面 ＋ 它的请求是否自报运行（声明了 [`DefaultUpstream::owner_header`]）。
#[derive(Clone, Copy)]
pub(crate) struct StreamFamily {
    pub(crate) face: StreamFace,
    /// 声明了自报运行的头 ⇒ 带头的归那个子运行、不带头的就是主运行（当场定，不挂起）。
    pub(crate) owns: bool,
}

/// 各家上游的流协议面（注册序）。一条应答用哪一个，由通用层按头一件事认（认得出「开始」的那一个）。
pub(crate) fn stream_families() -> Vec<StreamFamily> {
    REGISTRY
        .iter()
        .filter_map(|a| {
            let u = a.upstream.as_ref()?;
            Some(StreamFamily {
                face: u.stream?,
                owns: u.owner_header.is_some(),
            })
        })
        .collect()
}

/// 一家的合成历史面：函数指针（同 [`Adapter::home`]，不立 trait）。
#[derive(Clone, Copy)]
pub(crate) struct HistoryFace {
    /// 这台机器上这一家的会话（没装 ⇒ 空）。
    pub(crate) sessions: fn() -> Vec<SynthSession>,
    /// 一份会话的首条真用户话（列表摘要）。
    pub(crate) excerpt: fn(&Path) -> String,
    /// 这一家会话记录的根（没装 / 说不出 ⇒ `None`）。按路径读会话那几条命令（`history-read` 等）的
    /// 围栏认它 —— 历史清单列出来的这一家的会话，要能按同一条路打开（本机冷读也走后端之后，本机 Codex 会话靠它）。
    pub(crate) root: fn() -> Option<PathBuf>,
}

/// 合成历史里的一个会话（通用层按 `cwd` 分组成项目）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SynthSession {
    pub sid: String,
    pub path: PathBuf,
    /// 分组键；缺 ⇒ 空串（归「(<kind>)」组）。
    pub cwd: String,
    pub mtime_ms: i64,
}

/// 注册表里有合成历史面的每一家：`(kind, 面)`（注册序）。
pub(crate) fn history_faces() -> Vec<(&'static str, HistoryFace)> {
    REGISTRY
        .iter()
        .filter_map(|a| a.history.map(|h| (a.kind, h)))
        .collect()
}

/// 内容搜索（只扫记录树）不覆盖、而这台上又有它的会话记录的那几家：它们对用户的叫法（注册序）。
pub(crate) fn content_search_skips() -> Vec<&'static str> {
    REGISTRY
        .iter()
        .filter(|a| {
            a.history
                .is_some_and(|h| (h.root)().is_some_and(|r| r.is_dir()))
        })
        .map(|a| a.launch.map_or(a.kind, |l| l.display_name))
        .collect()
}

/// 注册表里每一家合成历史面给的记录根（注册序；说不出的跳过）。按路径读会话的围栏在 Claude 的
/// `projects/` 之外还认这几个（`observe/query/locate.rs::validate_session_path`）。
pub(crate) fn history_roots() -> Vec<PathBuf> {
    history_faces()
        .into_iter()
        .filter_map(|(_, h)| (h.root)())
        .collect()
}

/// 一家的资产面：函数指针（同 [`Adapter::home`]，不立 trait）。几件布局知识：怎么扫 · skill 住哪 · 用户级 MCP 住哪。
#[derive(Clone, Copy)]
pub(crate) struct AssetFace {
    /// 按这台机器的环境现解根、现扫：用户级那一份 ＋ 交进来的那几个项目目录（这台上开过会话的项目）。
    /// 用户级 MCP 读哪份文件由调用方交（有账号库的机器上是各号共用的那一份，格式与本家那份同一种）。
    /// 交回原始事实（还没有摘要 —— 摘要是通用层的机器）。
    pub(crate) scan: fn(projects: &[String], user_mcp: Option<&Path>) -> Sightings,
    /// 这一家用户级 skill 的根（「装到这台」读 / 写 skill 的落点从这里来；与扫描同一个家）。
    pub(crate) skills_root: fn() -> Option<PathBuf>,
    /// 一个项目目录里 skill 的根（项目级 skill 的落点，与扫描同一个家）。
    pub(crate) project_skills_root: fn(project: &Path) -> PathBuf,
    /// 这一家自己存用户级 MCP 的那份文件（没有账号库的机器上，用户级 MCP 就是它）。
    pub(crate) user_mcp_file: fn() -> Option<PathBuf>,
    /// 项目级 MCP 配置的文件名（项目目录下；增 / 改 / 删 · 推 / 拉的落点）。
    pub(crate) project_mcp_file: &'static str,
    /// MCP 配置文件里装 server 表的那个顶层键。
    pub(crate) servers_key: &'static str,
    /// 一个配置根底下装着的插件：插件根 ＋ 它清单里的名字（用户级 skill 目录里放进来的 · 插件缓存里每个版本一个）。
    pub(crate) plugins: fn(config_root: &Path) -> Vec<(PathBuf, String)>,
    /// 一条会话能用的命令与 skill：那条会话的项目目录（说不出 ⇒ `None`）＋ 它用的那个号的配置根，现扫。
    pub(crate) session_scan: fn(project: Option<&Path>, config_root: &Path) -> SessionAssets,
    /// 这一家内置的斜杠命令（不住文件里的那几条）。
    pub(crate) builtin_commands: &'static [BuiltinCommand],
}

/// 一条内置的斜杠命令（[`AssetFace::builtin_commands`]）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct BuiltinCommand {
    /// 命令名（不带 `/`）。
    pub(crate) name: &'static str,
    /// 写给人看的简介（经文案表取，键在适配层写成字面量）。
    pub(crate) text: fn() -> String,
    /// 要跟参数（填进输入框后停住等补）。
    pub(crate) needs_args: bool,
    /// 发之前先确认（会丢掉东西的那几条）。
    pub(crate) confirm: bool,
}

/// 一条会话能用的命令与 skill（[`AssetFace::session_scan`] 的原始事实，字由通用层写）。
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct SessionAssets {
    /// 本项目的在前、号的在后，各按名字排。
    pub(crate) commands: Vec<CommandSeen>,
    pub(crate) skills: Vec<SkillSeen>,
    /// 读不出来的那几份（一句一份）。
    pub(crate) problems: Vec<String>,
}

/// 看到的一条住在文件里的斜杠命令。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CommandSeen {
    pub(crate) name: String,
    /// 住在本项目里（否则住在号的配置根里）。
    pub(crate) project: bool,
    pub(crate) description: Option<String>,
    pub(crate) needs_args: bool,
}

/// 看到的一个 skill：`project` = `None` 是用户级，`Some(项目目录)` 是那个项目里的。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SkillSeen {
    pub project: Option<String>,
    pub name: String,
    /// 那个 skill 的目录（可能是一条链接：通用层按它跟到底走一遍）。
    pub dir: PathBuf,
    pub description: Option<String>,
}

/// 看到的一条 MCP server：`project` 同 [`SkillSeen::project`]；`file` 是它住的那份配置文件。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct McpSeen {
    pub project: Option<String>,
    pub name: String,
    /// 那一条原样的定义。
    pub def: serde_json::Value,
    pub file: PathBuf,
}

/// 一家适配层看到的原始资产事实。**还没有摘要**（通用层 `asset_catalog.rs` 算）。
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Sightings {
    pub skills: Vec<SkillSeen>,
    pub mcp: Vec<McpSeen>,
    /// 读不出来的那几份（一句话一份）—— 「这台没有」与「这台那份读不出来」不许合成一句。
    pub problems: Vec<String>,
}

/// 这几个配置根底下装着的插件（插件根 ＋ 名字）：逐家问（注册序）、配置根的次序。
pub(crate) fn plugins(config_roots: &[PathBuf]) -> Vec<(PathBuf, String)> {
    REGISTRY
        .iter()
        .filter_map(|a| a.assets)
        .flat_map(|f| config_roots.iter().flat_map(move |r| (f.plugins)(r)))
        .collect()
}

/// 注册表里每一家有资产面的，各扫一遍（注册序）。
pub(crate) fn asset_sightings(projects: &[String], user_mcp: Option<&Path>) -> Vec<Sightings> {
    REGISTRY
        .iter()
        .filter_map(|a| a.assets.map(|f| (f.scan)(projects, user_mcp)))
        .collect()
}

/// `kind` 那一家的资产面。认不出 / 没有 ⇒ `None`。
pub(crate) fn asset_face(kind: &str) -> Option<AssetFace> {
    asset_face_among(REGISTRY, kind)
}

/// `kind` 那一家的资产面。
fn asset_face_among(registry: &[Adapter], kind: &str) -> Option<AssetFace> {
    adapter_among(registry, kind).and_then(|a| a.assets)
}

/// `kind` 那一家的用户级 skill 根。那一家没有资产面 ⇒ `None`。
pub(crate) fn skills_root(kind: &str) -> Option<PathBuf> {
    skill_root_among(REGISTRY, kind, None)
}

/// `kind` 那一家的 skill 根：`None` = 用户级（同 [`skills_root`]），`Some(项目目录)` = 那个项目里的。
pub(crate) fn skill_root_at(kind: &str, project: Option<&Path>) -> Option<PathBuf> {
    skill_root_among(REGISTRY, kind, project)
}

/// [`skill_root_at`] 的可喂夹具那一半。
pub(crate) fn skill_root_among(
    registry: &[Adapter],
    kind: &str,
    project: Option<&Path>,
) -> Option<PathBuf> {
    let f = asset_face_among(registry, kind)?;
    match project {
        None => (f.skills_root)(),
        Some(p) => Some((f.project_skills_root)(p)),
    }
}

/// `kind` 那一家的用户级 MCP 住的那份文件。
pub(crate) fn user_mcp_file(kind: &str) -> Option<PathBuf> {
    asset_face_among(REGISTRY, kind).and_then(|f| (f.user_mcp_file)())
}

/// **这个后端认得哪几个 agent**。加一个 agent = 加一行（+ 上面加一行 `mod`）。
///
/// ⚠ 它与 `agent_locality_guard::tests::HOMES`（判据用的"agent 家"清单）**必须一样长**，
/// 由 `every_agent_adapter_has_exactly_one_registry_entry` 双向钉住：
/// 建了 `agents/<名>/` 却不登记 ⇒ 那家永远"看不见"，而没有任何东西会说。
/// ⚠ **一家一行**（不是每字段一行）：判据⑦把本文件直呼适配层的**行数**钉成 `REGISTRY.len()`。
#[rustfmt::skip]
pub(crate) const REGISTRY: &[Adapter] = &[
    Adapter { kind: claudecode::AGENT_KIND, home: claudecode::home, account_env: Some(claudecode::paths::CONFIG_DIR_ENV), old_home_pref: Some(claudecode::OLD_HOME_PREF), assets: Some(claudecode::ASSETS), history: None, upstream: Some(claudecode::UPSTREAM), mcp: Some(claudecode::MCP), footprint: Some(claudecode::footprint::FACE), accounts: Some(claudecode::accounts::FACE), records: Some(claudecode::RECORDS), processes: Some(claudecode::cards::PROCESS_NAMES), launch: Some(claudecode::LAUNCH), compact_request: Some(claudecode::COMPACT_REQUEST), clear_request: Some(claudecode::CLEAR_REQUEST), local: Some(claudecode::LOCAL), live: claudecode::LIVE, asks: Some(claudecode::asks::FACE) },
    // codex 今天没有账号维度（`account_env: None`）：选号对它说不出，起法与 `ccm` 都明说不行。
    Adapter { kind: codex::AGENT_KIND,      home: codex::home,      account_env: None, old_home_pref: None, assets: None, history: Some(codex::HISTORY), upstream: Some(codex::relay::UPSTREAM), mcp: None, footprint: None, accounts: None, records: Some(codex::RECORDS), processes: None, launch: Some(codex::LAUNCH), compact_request: None, clear_request: None, local: None, live: codex::LIVE, asks: None },
];

/// 某一家的账号载体（环境变量名）。认不出这家 ⇒ `None`。
///
/// ⚠ **它是通用层拿这个名字的唯一入口** —— 直接 `use agents::<名>::…` 会让
/// `agent_locality_guard` 判据④的读数凭空上涨，而那个数只许降（见 [`Adapter::account_env`]）。
pub(crate) fn account_env_of(kind: &str) -> Option<&'static str> {
    REGISTRY
        .iter()
        .find(|a| a.kind == kind)
        .and_then(|a| a.account_env)
}

/// 注册表里每一家的 `(kind, config.json 里旧的数据目录键名)`（没有那一格的不列）。
/// ⚠ 只给换形那一段用（[`Adapter::old_home_pref`]），下一版随 `control/prefs_migration.rs` 一起删；**通用层读这一格只经这里**。
pub(crate) fn old_home_pref_of(kind: &str) -> Option<&'static str> {
    REGISTRY
        .iter()
        .find(|a| a.kind == kind)
        .and_then(|a| a.old_home_pref)
}

/// 注册表里的每一家（注册表序）。
pub(crate) fn registered_kinds() -> impl Iterator<Item = &'static str> {
    REGISTRY.iter().map(|a| a.kind)
}

/// 某一家找上游读的那个环境变量（注入格是 [`Inject::Env`] 的那个变量）。认不出这家 / 没登记上游 / 地址只能拼进参数 ⇒ `None`
/// （`ccm` 对它不往环境里注入中转地址、也不清继承来的那一条）。
pub(crate) fn base_url_env_of(kind: &str) -> Option<&'static str> {
    match inject_of(kind) {
        Some(Inject::Env(v)) => Some(v),
        _ => None,
    }
}

/// 某一家清空会话用的那一句（[`Adapter::clear_request`]）。认不出这家 / 这一家不支持 ⇒ `None`。
pub(crate) fn clear_request_of(kind: &str) -> Option<&'static str> {
    REGISTRY
        .iter()
        .find(|a| a.kind == kind)
        .and_then(|a| a.clear_request)
}

/// 某一家请求压缩上下文用的那一句（[`Adapter::compact_request`]）。认不出这家 / 这一家不支持 ⇒ `None`。
pub(crate) fn compact_request_of(kind: &str) -> Option<&'static str> {
    REGISTRY
        .iter()
        .find(|a| a.kind == kind)
        .and_then(|a| a.compact_request)
}

/// `kind` 那一家的这条记录是不是压缩之后续接用的摘要（同渲染模型里 `compactSummary` 那一种来源）。
pub(crate) fn is_compact_summary(kind: &str, v: &serde_json::Value) -> bool {
    user_text_of(kind, v).is_some_and(|t| matches!(t.speaker, Speaker::CompactSummary { .. }))
}

#[cfg(test)]
#[path = "../../../tests/backend/agents_tests.rs"]
mod tests;
