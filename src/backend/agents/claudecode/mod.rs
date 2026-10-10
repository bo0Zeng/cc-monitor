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
//! 通用层经注册表那一行（`agents::REGISTRY`）够到本层的各格，一处都不直呼本层（`agent_locality_guard` 判据④零命中）。

pub(crate) mod accounts;
pub(crate) mod assets;
// 后台命令在记录里的样子（起 · 任务号 · 收场通知）。
pub(crate) mod background;
// 分叉的记录变换 ＋ 按 sid 找那份会话文件（原共享 crate `branch-core`，只剩后端用）。
pub(crate) mod branch;
// 工具词表（卡型 · 判活进程名）：从 monitor `adapter.rs` 画像表与共享 crate `agent-tools-core` 收进来。
pub(crate) mod cards;
// 记录链（`uuid` / `parentUuid`）⇒ 通用层算主线外清单要的事实。
pub(crate) mod chain;
// 盘上一行 ⇒ 通用记录（翻译表只住这里）。
pub(crate) mod drift;
pub(crate) mod record_of;
// 「足迹」里的 Claude 布局（`~/.claude/…` 的基准 · settings 两个作用域）。
pub(crate) mod footprint;
pub(crate) mod liveness;
pub(crate) mod mcp;
pub(crate) mod parse;
pub mod paths;
pub(crate) mod pidfile;
// 回包头里的额度那一族 → 通用的额度快照。
pub(crate) mod quota;
pub(crate) mod records;
// 官方客户端报一个号的用量（`claude -p /usage`）→ 通用的额度窗口：命令行与读法。
pub(crate) mod resume;
pub(crate) mod usage;
// 子运行（子 agent）的形状：对账键 · 归属 · 派出链接 · 记录住址 · 请求自报身份的头。
pub(crate) mod runs;
pub(crate) mod schema;
pub(crate) mod steps;
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
    fallback: super::Fallback::One("https://api.anthropic.com"),
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
    window_slot: Some(quota::slot_of),
    window_key: Some(quota::key_of),
    usage: Some(usage::FACE),
    login: Some(LOGIN),
    limit_reply: Some(quota::limit_reply),
    // 认 `ANTHROPIC_BASE_URL`：插上钥匙的整条地址进这个变量。
    inject: super::Inject::Env(paths::BASE_URL_ENV),
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
    // 60 秒：旁边那把是 proper-lockfile 的形状，持有方每 5 秒刷一次修改时刻、10 秒没刷算过期（按锁的形状推断，没核那一家的源码）；
    // 我方持锁时不刷修改时刻，最长一趟是令牌端点那一发（每次读写期限 15 秒）。取 60 秒，活着的持有方（它的、我们的）都不会被判过期。
    lock_stale_ms: 60_000,
    identity_file: accounts::identity_file,
    identity_in: accounts::identity_in,
    rewrite_identity: accounts::rewrite_identity,
    base_dir: accounts::shared_root_in,
};

/// 请求压缩上下文用的那一句（注册表 `Adapter.compact_request` 那一格）：在会话里敲的斜杠命令。
pub(crate) const COMPACT_REQUEST: &str = "/compact";

/// 本家的**起会话事实**（注册表 `Adapter.launch` 那一格）：`ccm` 按它起、界面按生成物 `agent-profile-table.ts` 读，同一份。
pub(crate) const LAUNCH: super::LaunchFace = super::LaunchFace {
    adapter_id: UPSTREAM.route_id,
    display_name: "Claude Code",
    speaker_name: "Claude",
    default_launcher: resume::DEFAULT_COMMAND,
    launcher_alias: Some(resume::LAUNCHER_ALIAS),
    resume_token: resume::RESUME_TOKEN,
    preset_sid: Some(resume::SESSION_ID_FLAG),
    launch_args: &[],
    nested_env: resume::NESTED_ENV,
    is_default: true,
    resume_command: resume::resume_command,
    session_name_prefix: resume::SESSION_NAME_PREFIX,
    // 不经 env 交 cc-bus 身份：它自己读得到 tmux，交了反而盖掉 `@cc_id` 的细分。
    needs_bus_id: false,
    has_identity: true,
    has_pidfiles: true,
    models: Some(resume::MODEL_ALIASES),
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
/// 本机布局：家目录（`CLAUDE_CONFIG_DIR` / `~/.claude`）· pidfile 目录 `<家>/sessions` · 进程名单兜底认 cmdline。
pub(crate) const LOCAL: super::LocalFace = super::LocalFace {
    home_at: paths::home_of,
    pidfile_dir: paths::sessions_root,
    cmdline_may_be_agent: liveness::cmdline_may_be_agent,
    tasks_dir: Some(paths::tasks_root),
    background_of: pidfile::background_of,
    activity_of: pidfile::activity_of,
    wait_of: pidfile::wait_of,
};

pub(crate) const MCP: super::McpFace = super::McpFace { read: mcp::read };

/// 记录解释面（注册表 `Adapter.records` 那一格）。
pub(crate) const RECORDS: super::RecordFace = super::RecordFace {
    parse: parse::translated,
    sid: records::session_id_of,
    is_session_file: records::is_session_file,
    tree: Some(super::RecordTree {
        root: paths::projects_root,
        file_name: records::session_file_name,
    }),
    turn_end: Some(turn::turn_end_uuid_of),
    chain: Some(chain::chain_fact),
    find_session: Some(branch::find_session_file),
    branch: Some(branch::build_branch_records),
    drift: Some(drift::report),
    text: Some(super::TextFace {
        main: text::extract_text_blocks,
        tool: text::extract_tool_text,
        user: text::user_text_of_record,
        result: steps::result_of,
    }),
    delete: Some(super::SessionDelete {
        locate: paths::session_file_for_delete,
        is_record: paths::is_session_record_path,
    }),
    response_id: Some(runs::response_id),
    run_of: Some(runs::run_of),
    child_link: Some(runs::child_link),
    background: Some(background::marks),
    children: Some(super::ChildFace {
        sources: runs::sources,
        owner: runs::owner,
        find: runs::find,
        hint: runs::hint,
        written: runs::written,
    }),
    project_dir: Some(parse::project_dir),
};

pub(crate) const ASSETS: super::AssetFace = super::AssetFace {
    scan: assets::scan,
    skills_root: assets::skills_root,
    project_skills_root: assets::project_skills_root,
    user_mcp_file: assets::claude_json,
    project_mcp_file: assets::PROJECT_MCP_FILE,
    servers_key: assets::SERVERS_KEY,
    plugins: assets::plugins,
};
