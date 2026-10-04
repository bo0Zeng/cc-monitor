//! Claude Code 适配层 —— backend 里 Claude 专属知识的唯一住址（`S3`）。
//!
//! | 子模块 | 装什么 |
//! |---|---|
//! | [`paths`] | 配置目录怎么解析（环境变量名 + `.claude`）· `projects/` 与 `sessions/` 两个子目录 |
//! | [`records`] | 会话记录的后缀与命名（`<sid>.jsonl`） |
//! | [`liveness`] | 判活时"这个 cmdline 看起来像不像 Claude" |
//! | [`cards`] | 工具名 → 卡型 · tmux 前台命令哪几个算它（界面不认工具名，卡型随记录成品带出） |
//! | [`branch`] | 按 sid 找那份会话文件 · 分叉的记录变换（原共享 crate `branch-core`） |
//! | [`accounts`] | `.claude.json` 的信任判定（`projects[cwd].hasTrustDialogAccepted`） |
//! | [`resume`] | resume 的命令形状与会话名前缀（与 [`super::codex::resume`] 对称） |
//! | [`assets`] | 资产布局：`skills/<名>/` · `SKILL.md` 的 `description:` · `.claude.json` 的 `projects` × `<项目>/.mcp.json` |
//! | [`text`] | 记录文本：`message.content` 的正文块 / 工具内容怎么抽 · user 正文里 CLI 注入的包装与样板怎么剥（原共享 crate `search-core` 的这一半） |
//! | [`schema`] · [`parse`] · [`turn`] · [`drift`] | **记录解释**：一行 jsonl 的线上形状（界面收到的就是它）· 抢救与记账 · 轮次边沿 · 漂移账（从 monitor 搬来） |
//!
//! # ⚠ 搬进来的是**知识**，不是**机器**
//!
//! `S3` 的 Bx 实测推翻了"读盘族 = 两千行"这个印象：`history_query`(923) /
//! `search_query`(676) / `watcher`(4885) 里的 `jsonl` 绝大多数是**变量名与函数名**
//!（`jsonl_path` / `read_jsonl` / `newest_jsonl`）—— 那是**通用的流式读取机器**，与 agent 无关。
//! 真知识只有**带引号的那几处**（后缀判定 6 · 目录名 4 · 身份/账号 ~12 · 判活与 resume ~5）。
//! ⇒ 本层收的是那 ~30 处；机器留在原地，通过本层的一次函数调用拿知识。
//! 这正是 `D1` 那句「通用骨架 ~12000 行与 agent 无关」在代码上的兑现。
//!
//! # ⚠ 本层**不**代表 Claude 那半已经分干净
//!
//! `claude_dir` 那个**参数名已经清了**（生产段 64 行 → 3 行，
//! 剩下的 3 处全是冻结的 wire 字段名）。
//! 而 `watcher`/`accounts_query`/`history_query` **仍然进不了** `S1` 的 `CORE_FILES` ——
//! `S4b` 实测：改完名之后这三个文件在 `S1` 六根针下还剩 **6 / 5 / 5** 处（共 16），其中
//! **12 处是 `crate::agents::claudecode::…` 这个适配层地址本身**（`history_query` 那 5 处**全是**），
//! 另 4 处是散文（`watcher` 两句 warn 里的 `claude` 一词、`accounts_query` 两处
//! `sessions/` 文案）。⇒ 卡点不再是参数名，是**通用层直接写死了一个 agent 的名字** ——
//! 那是 `L2`（接口）的题，归 `S6`。清单在 `agent_locality_guard::ADAPTER_CALL_SITES`。

pub(crate) mod accounts;
pub(crate) mod assets;
// 分叉的记录变换 ＋ 按 sid 找那份会话文件（原共享 crate `branch-core`，只剩后端用）。
pub(crate) mod branch;
// 工具词表（卡型 · 判活进程名）：从 monitor `adapter.rs` 画像表与共享 crate `agent-tools-core` 收进来。
pub(crate) mod cards;
pub(crate) mod drift;
// 「足迹」里的 Claude 布局（`~/.claude/…` 的基准 · settings 两个作用域）。
pub(crate) mod footprint;
pub(crate) mod liveness;
pub(crate) mod mcp;
pub(crate) mod parse;
pub mod paths;
// 回包头里的额度那一族 → 通用的额度快照。
pub(crate) mod quota;
pub(crate) mod records;
pub(crate) mod resume;
// 子运行（子 agent）的形状：对账键 · 归属 · 派出链接 · 记录住址 · 请求自报身份的头。
pub(crate) mod runs;
pub(crate) mod schema;
// 记录文本：正文 / 工具内容怎么抽 · CLI 注入怎么剥（原共享 crate `search-core` 的 Claude 那一半）。
pub(crate) mod text;
// skill 接入面的声明（收件箱那几个人要改的文件 ＋ Claude 数据文件的纵深围栏）：从 monitor 搬来。
pub(crate) mod turn;

/// 本 agent 在 wire 上的 **`agent_kind` 值**。
///
/// ⚠ **是 `claude` 不是 `claudecode`** —— 模块名与 wire 值域是两件事，别顺手对齐：
/// 值域由既有契约定死（`ResumeSpec.agentKind` 逐字「缺/`""`/`"claude"`=claude」·
/// `session_added.agent_kind` 对 Claude **省略** ⇒ 缺 = claude），改它 = 改跨仓契约。
///
/// 它住在**适配层**而不是通用层，是为了让 `agents::REGISTRY` 那张注册表里
/// 一个 agent 名的字面量都没有（`D3`：agent 维度只许出现在值里 —— 而这个值的**来源**
/// 也该是那个 agent 自己）。
pub(crate) const AGENT_KIND: &str = "claude";

/// 本 agent 的**默认上游**（用户「写死, 跟着适配层」）：路由里叫 `claude-code`；
/// `CCM_AGENT_UPSTREAM_CLAUDE_CODE` 盖掉它（名字照 `CCM_AGENT_UPSTREAM_<agent>` 的形状）；
/// 内置默认是 Anthropic 官方端点。上游选择只经 `agents::default_upstreams` 读它。
pub(crate) const UPSTREAM: super::DefaultUpstream = super::DefaultUpstream {
    route_id: "claude-code",
    env: "CCM_AGENT_UPSTREAM_CLAUDE_CODE",
    fallback: "https://api.anthropic.com",
    // 真 claude（2.1.283）每条 `POST /v1/messages` 都带它：UUID 形，== 它落盘的 jsonl 文件名。
    session_header: Some("x-claude-code-session-id"),
    stream: Some(super::sse_anthropic::FACE),
    owner_header: Some(runs::OWNER_HEADER),
    // 界面给这一家的账号配第三方 key 时写那份凭据文件 ⇒ 文件里的行都是这一家的。
    owns_credentials_file: true,
    // 直接敲的 claude 读 `~/.claude/settings.json` 的 `env` 块（各号的那一份都链回它）。
    settings_env: Some(paths::SETTINGS_ENV),
    // 1M 上下文的请求在 `anthropic-beta` 里带 `context-1m-<日期>` 那一项；不带 ⇒ 这个模型的默认上下文。
    context_mark: Some(("anthropic-beta", "context-1m")),
    quota: Some(quota::read),
    login: Some(LOGIN),
};

/// 订阅号登录：`<配置目录>/.credentials.json` 的 `claudeAiOauth` 那一节；续期照 claude 自己的做法
/// （JSON 体发到令牌端点、快过期 5 分钟内就续、拿配置目录里外那两把 `mkdir` 锁）。
pub(crate) const LOGIN: super::LoginFace = super::LoginFace {
    creds_file: acct_core::CREDENTIALS_NAME,
    section: "claudeAiOauth",
    access: "accessToken",
    refresh: "refreshToken",
    expires_ms: "expiresAt",
    scopes: "scopes",
    client_field: "clientId",
    token_url: "https://platform.claude.com/v1/oauth/token",
    client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
    margin_ms: 300_000,
    lock_inside: ".oauth_refresh.lock",
    lock_beside: Some(".lock"),
};

/// 请求压缩上下文用的那一句（注册表 `Adapter.compact_request` 那一格）：在会话里敲的斜杠命令。
pub(crate) const COMPACT_REQUEST: &str = "/compact";

/// 本家的**起会话事实**（注册表 `Adapter.launch` 那一格）：`ccm` 按它起、界面按生成物 `agent-profile-table.ts` 读，同一份。
pub(crate) const LAUNCH: super::LaunchFace = super::LaunchFace {
    adapter_id: UPSTREAM.route_id,
    display_name: "Claude Code",
    default_launcher: resume::DEFAULT_COMMAND,
    launcher_alias: Some(resume::LAUNCHER_ALIAS),
    resume_token: resume::RESUME_TOKEN,
    launch_args: &[],
    nested_env: resume::NESTED_ENV,
    is_default: true,
    resume_command: resume::resume_command,
    session_name_prefix: resume::SESSION_NAME_PREFIX,
    // 不经 env 交 cc-bus 身份：它自己读得到 tmux，交了反而盖掉 `@cc_id` 的细分。
    needs_bus_id: false,
    has_identity: true,
    has_pidfiles: true,
};

/// 本 agent 在这台机器上的 home 目录 —— **只答"它该在哪"，不答"在不在"**。
///
/// 「在不在」的判准是通用层的机器（`agents::visible_homes`），不是每家自己定一套 ——
/// 那正是 `S3` 立的分界（知识住适配层、机器留通用层）在发现这件事上的兑现。
///
/// 恒 `Some`：[`paths::resolve_home`] 有默认值（`$HOME/.claude`，再退 cwd 下的 `.claude`），
/// 解析不出来这种事对 Claude 不存在。签名仍取 `Option` 是为了与
/// [`super::codex::home`] 同形（那家真的可能答不出来）。
pub(crate) fn home() -> Option<std::path::PathBuf> {
    Some(paths::resolve_home())
}

/// 本家的资产面（注册表 `Adapter.assets` 那一格）：skill 与项目级 MCP 的布局知识住 [`assets`]。
/// 注册表 `mcp` 那一格。
pub(crate) const MCP: super::McpFace = super::McpFace { read: mcp::read };

/// 记录解释面（注册表 `Adapter.records` 那一格）。
pub(crate) const RECORDS: super::RecordFace = super::RecordFace {
    parse: parse::parsed_line,
    sid: records::session_id_of,
    turn_end: Some(turn::turn_end_uuid_of),
    find_session: Some(branch::find_session_file),
    branch: Some(branch::build_branch_records),
    drift: Some(drift::report),
    text: Some(super::TextFace {
        main: text::extract_text_blocks,
        tool: text::extract_tool_text,
        user: text::user_text_of_record,
    }),
    delete: Some(super::SessionDelete {
        locate: paths::session_file_for_delete,
        is_record: paths::is_session_record_path,
    }),
    response_id: Some(runs::response_id),
    run_of: Some(runs::run_of),
    child_link: Some(runs::child_link),
    children: Some(super::ChildFace {
        sources: runs::sources,
        owner: runs::owner,
        hint: runs::hint,
    }),
    project_dir: Some(parse::project_dir),
};

pub(crate) const ASSETS: super::AssetFace = super::AssetFace {
    scan: assets::scan,
    skills_root: assets::skills_root,
    project_skills_root: assets::project_skills_root,
    user_mcp_file: assets::claude_json,
};
