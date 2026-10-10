//! Codex 适配层 —— backend 里所有 Codex 专属知识的唯一住址。
//!
//! | 子模块 | 装什么 |
//! |---|---|
//! | [`parse`] | rollout 记录的信封与字段抽取、会话目录定位 |
//! | [`resume`] | resume 的命令形状与会话名前缀、默认命令名 |
//!
//! 适配层的接口是四类能力（会话发现与判活 · 会话内容读 · 用量 · 起会话 / resume）。「这台机器上有没有这个 agent」已有（[`home`] + `agents::visible_homes`），
//! 「有哪些会话、活没活」那一格是空的（要扫会话树 + 判活）⇒ Codex 会话不出现在流式 watcher 里。
//! 发现出来的东西不上线：`main.rs` 的 `homes: Vec::new()` 那一行留着（填 `homes` 是一次跨仓契约变更，是一次纯发布决策）。
//! 不要从「这里有个模块」推断「Codex 支持完整」。

// 历史清单那一面（会话枚举 ＋ 首条真用户话）：本机的 Codex 合成项目 / 会话。
pub(crate) mod history;
pub(crate) mod parse;
// 记录分类 ＋ 翻成通用记录。
pub(crate) mod record;
// 经中转那一份（默认上游 · 会话头 · 怎么指到中转 · 直接敲的那份配置里的地址）。
pub(crate) mod relay;
pub(crate) mod resume;

/// 这一家的历史清单面（注册表 `Adapter.history` 那一格；通用层经注册表够到它，不直呼本模块）。
pub(crate) const HISTORY: crate::agents::HistoryFace = crate::agents::HistoryFace {
    sessions: history::sessions,
    excerpt: history::first_user_excerpt,
    root: history::records_root,
};
/// 记录解释面（注册表 `Adapter.records` 那一格）。轮次边沿与漂移账这一家今天不报。
pub(crate) const RECORDS: crate::agents::RecordFace = crate::agents::RecordFace {
    parse: record::translated,
    sid: parse::codex_sid_from_path,
    is_session_file: history::is_session_file,
    // 会话按日期分、不住按项目分的记录树（历史由合成历史面并进来）。
    tree: None,
    turn_end: None,
    class: None,
    chain: None,
    // Codex 的会话不在按项目分的记录树里、今天也不分叉 ⇒ 这两格没有。
    find_session: None,
    branch: None,
    drift: None,
    // Codex 的记录不进全局搜索 / 摘录那几条通用路（它们只走记录树那一家）⇒ 不给文本面。
    text: None,
    delete: None,
    // Codex 的记录里今天没有委派出去的运行 ⇒ 不声明子运行。它的流经中转时按请求自带的 `thread-id` 当场归位（不靠对账键）。
    response_id: None,
    run_of: None,
    child_link: None,
    background: None,
    children: None,
    project_dir: Some(history::project_dir),
};

/// 本 agent 在 wire 上的 **`agent_kind` 值**。
///
/// 值域由既有契约定死（`ResumeSpec.agentKind` 逐字「`"codex"`=codex，**大小写敏感**」·
/// `session_added.agent_kind` 发 `"codex"`），改它 = 改跨仓契约。
///
/// 它住在**适配层**而不是通用层，是为了让 `agents::REGISTRY` 那张注册表里
/// 一个 agent 名的字面量都没有（顺带：`agent_locality_guard` 判据②数的正是通用层里
/// 带引号的 `"codex"`，把它写在注册表里会**当场**多一条 kind 派发登记 —— 而注册表
/// 根本不在做派发）。
pub(crate) const AGENT_KIND: &str = "codex";

/// 本家的起会话事实（注册表 `Adapter.launch` 那一格）。适配器 id 就是 wire 上的 kind（也是中转路由里它的名字）；没有 shell wrapper。
pub(crate) const LAUNCH: crate::agents::LaunchFace = crate::agents::LaunchFace {
    adapter_id: AGENT_KIND,
    display_name: "Codex",
    speaker_name: "Codex",
    default_launcher: resume::DEFAULT_COMMAND,
    launcher_alias: None,
    resume_token: resume::RESUME_TOKEN,
    preset_sid: None,
    self_sid_env: None,
    launch_args: resume::LAUNCH_ARGS,
    nested_env: resume::NESTED_ENV,
    is_default: false,
    resume_command: resume::resume_command,
    session_name_prefix: resume::SESSION_NAME_PREFIX,
    // 它的沙箱够不着 tmux socket ⇒ cc-bus 身份经 `CC_BUS_ID` 交进去。
    needs_bus_id: true,
    has_identity: false,
    has_pidfiles: false,
    models: None,
};

/// 本 agent 在这台机器上的 home 目录 —— **只答"它该在哪"，不答"在不在"**。
///
/// 与 Claude 那家的**真实差别**：这里可能答不出来（`None`）——
/// [`parse::resolve_codex_dir`] 认 `$CODEX_HOME`，否则 `$HOME/.codex`；
/// **两个环境变量都没有**时它没有第三条退路（Claude 那边有 cwd 兜底）。
/// `None` = 连一个候选路径都说不出 ⇒ 一定看不见。
pub(crate) fn home() -> Option<std::path::PathBuf> {
    parse::resolve_codex_dir()
}
