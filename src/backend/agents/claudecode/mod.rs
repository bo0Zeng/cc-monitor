//! Claude Code 适配层 —— backend 里 Claude 专属知识的唯一住址（`S3`）。
//!
//! | 子模块 | 装什么 |
//! |---|---|
//! | [`paths`] | 配置目录怎么解析（环境变量名 + `.claude`）· `projects/` 与 `sessions/` 两个子目录 |
//! | [`records`] | 会话记录的后缀与命名（`<sid>.jsonl`） |
//! | [`liveness`] | 判活时"这个 cmdline 看起来像不像 Claude" |
//! | [`cards`] | 工具名 → 卡型 · tmux 前台命令哪几个算它（界面不认工具名，卡型随记录成品带出） |
//! | [`branch`] | 按 sid 找那份会话文件 · 分叉的记录变换 |
//! | [`accounts`] | `.claude.json` 的信任判定（`projects[cwd].hasTrustDialogAccepted`） |
//! | [`resume`] | resume 的命令形状与会话名前缀（与 [`super::codex::resume`] 对称） |
//! | [`assets`] | 资产布局：`skills/<名>/` · `SKILL.md` 的 `description:` · `.claude.json` 的 `projects` × `<项目>/.mcp.json` |
//! | [`text`] | 记录文本：`message.content` 的正文块 / 工具内容怎么抽 · user 正文里 CLI 注入的包装与样板怎么剥 |
//! | [`schema`] · [`parse`] · [`turn`] · [`drift`] | **记录解释**：一行 jsonl 的线上形状（界面收到的就是它）· 抢救与记账 · 轮次边沿 · 漂移账（从 monitor 搬来） |
//!
//! # ⚠ 搬进来的是**知识**，不是**机器**
//!
//! `S3` 的 Bx 实测推翻了"读盘族 = 两千行"这个印象：`observe::query`(923) /
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

pub mod accounts;
pub mod asks;
pub mod assets;
// 后台命令在记录里的样子（起 · 任务号 · 收场通知）。
pub(crate) mod background;
pub(crate) mod beat;
// 分叉的记录变换 ＋ 按 sid 找那份会话文件。
pub(crate) mod branch;
// 工具词表（卡型 · 判活进程名）：从 monitor `adapter.rs` 画像表与共享 crate `agent-tools-core` 收进来。
pub(crate) mod cards;
// 记录链（`uuid` / `parentUuid`）⇒ 通用层算主线外清单要的事实。
pub mod chain;
// 盘上一行 ⇒ 通用记录（翻译表只住这里）。
pub(crate) mod drift;
pub mod record_of;
// 「足迹」里的 Claude 布局（`~/.claude/…` 的基准 · settings 两个作用域）。
pub(crate) mod footprint;
pub mod liveness;
pub mod mcp;
pub mod parse;
pub mod paths;
pub(crate) mod pidfile;
// 回包头里的额度那一族 → 通用的额度快照。
pub mod quota;
pub mod records;
// 官方客户端报一个号的用量（`claude -p /usage`）→ 通用的额度窗口：命令行与读法。
pub mod resume;
// 回退菜单（双击 Esc 的 Rewind）：怎么调出来 · 每一屏怎么认 · 确认页那几项叫什么。
pub(crate) mod rewind;
pub(crate) mod usage;
// 子运行（子 agent）的形状：对账键 · 归属 · 派出链接 · 记录住址 · 请求自报身份的头。
pub mod runs;
pub mod schema;
// 一行记录里通用层要的会话级那几格 · 正文的形状（清单 · 全文搜索 · 骨架索引共用这一份读法）。
pub(crate) mod session_line;
// 用户级设置文件里钩子那一格的读与拼（注册表足迹面那两格）。
pub(crate) mod settings_hooks;
pub mod sign_in;
pub(crate) mod steps;
// 任务文件（注册表 `LocalFace.tasks` 那一格）：字段名与状态那几个值。
pub(crate) mod tasks;
// 记录文本：正文 / 工具内容怎么抽 · CLI 注入怎么剥。
pub(crate) mod text;
// skill 接入面的声明（收件箱那几个人要改的文件 ＋ Claude 数据文件的纵深围栏）：从 monitor 搬来。

/// 本 agent 在 wire 上的 **`agent_kind` 值**。
///
/// ⚠ **是 `claude` 不是 `claudecode`** —— 模块名与 wire 值域是两件事，别顺手对齐：
/// 值域由既有契约定死（缺/`""`/`"claude"`=claude ·
/// `session_added.agent_kind` 对 Claude **省略** ⇒ 缺 = claude），改它 = 改跨仓契约。
///
/// 它住在**适配层**而不是通用层，是为了让 `agents::REGISTRY` 那张注册表里
/// 一个 agent 名的字面量都没有（`D3`：agent 维度只许出现在值里 —— 而这个值的**来源**
/// 也该是那个 agent 自己）。
pub const AGENT_KIND: &str = "claude";

/// 本 agent 的**默认上游**（用户「写死, 跟着适配层」）：路由里叫 `claude-code`；
/// `CCM_AGENT_UPSTREAM_CLAUDE_CODE` 盖掉它（名字照 `CCM_AGENT_UPSTREAM_<agent>` 的形状）；
/// 内置默认是 Anthropic 官方端点。上游选择只经 `agents::default_upstreams` 读它。
pub const UPSTREAM: super::DefaultUpstream = super::DefaultUpstream {
    route_id: "claude-code",
    env: "CCM_AGENT_UPSTREAM_CLAUDE_CODE",
    fallback: super::Fallback::One("https://api.anthropic.com"),
    // 真 claude（2.1.283）每条 `POST /v1/messages` 都带它：UUID 形，== 它落盘的 jsonl 文件名。
    session_header: Some("x-claude-code-session-id"),
    stream: Some(super::sse_anthropic::FACE),
    relay_heads: &[
        super::RelayHead::Run(runs::RUN_HEADER),
        super::RelayHead::Class(runs::REQUEST_CLASS),
        // 1M 上下文的请求在 `anthropic-beta` 里带 `context-1m-<日期>` 那一项；不带 ⇒ 这个模型的默认上下文。
        super::RelayHead::Wide {
            header: "anthropic-beta",
            item: "context-1m",
        },
    ],
    relay_env: runs::RELAY_ENV,
    // 界面给这一家的账号配第三方 key 时写那份凭据文件 ⇒ 文件里的行都是这一家的。
    owns_credentials_file: true,
    // 直接敲的 claude 读 `~/.claude/settings.json` 的 `env` 块（各号的那一份都链回它）。
    settings_env: Some(paths::SETTINGS_ENV),
    quota: Some(quota::reading),
    window_slot: Some(quota::slot_of),
    window_key: Some(quota::key_of),
    usage: Some(usage::FACE),
    warm: Some(WARM),
    login: Some(LOGIN),
    // 换到别的订阅号：`Authorization: Bearer <那个号的访问令牌>`（请求体里的账号身份那一格由 `LOGIN.rewrite_identity` 换）。
    swap_heads: &[super::SwapHead {
        name: "Authorization",
        prefix: "Bearer ",
        cell: super::CredCell::Access,
    }],
    // 起会话前先定会话号（`--session-id`，见 `LAUNCH.preset_sid`）：起会话那个号照路由第 2 段。
    bind: super::Bind::Preset,
    limit_reply: Some(quota::limit_reply),
    // 收到「用满」后，重置落在 24 小时内才挂「等到重置自动续」（2.1.314 读码核过）。
    resumes_within: Some(24 * 3600),
    // 认 `ANTHROPIC_BASE_URL`：插上钥匙的整条地址进这个变量。
    inject: super::Inject::Env(paths::BASE_URL_ENV),
    // 官方客户端认的代理与免代理变量（大小写两形；不认 `ALL_PROXY`）。
    proxy: super::ProxyFace {
        via: &["https_proxy", "HTTPS_PROXY", "http_proxy", "HTTP_PROXY"],
        skip: &["no_proxy", "NO_PROXY"],
    },
};

/// 开窗那一句：`-p` 一发 haiku · 不给工具 · 只读项目级设置 · 不起 MCP · 不落会话记录 · 系统提示换成一句（不换约 6.8k token，
/// 隔几小时才发一次时缓存早过期、按写缓存价付）· 关掉思考。实测一发（claude 2.1.289）：输入约 870 token（自带、去不掉的那一段）· 输出 4 token。
pub(crate) const WARM: super::WarmFace = super::WarmFace {
    args: &[
        "-p",
        "--model",
        "haiku",
        "--tools",
        "",
        "--no-session-persistence",
        "--setting-sources",
        "project",
        "--strict-mcp-config",
        "--output-format",
        "json",
        "--system-prompt",
        "Reply with the single letter a.",
        "a",
    ],
    env_set: &[
        ("MAX_THINKING_TOKENS", "0"),
        ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
    ],
};

/// 凭据文件名（每个号的配置目录下）：订阅号登录与 MCP 登录同一份。全仓只这里写它；包外要它读 [`LOGIN`] 的 `creds_file`。
const CREDENTIALS_NAME: &str = ".credentials.json";

/// 订阅号登录：`<配置目录>/.credentials.json` 的 `claudeAiOauth` 那一节；续期照 claude 自己的做法
/// （JSON 体发到令牌端点、快过期 5 分钟内就续、拿配置目录里外那两把 `mkdir` 锁）。
pub const LOGIN: super::LoginFace = super::LoginFace {
    creds_file: CREDENTIALS_NAME,
    section: "claudeAiOauth",
    access: "accessToken",
    refresh: "refreshToken",
    expiry: super::Expiry::Cell("expiresAt"),
    scopes: Some("scopes"),
    // 续成只换访问 · 刷新令牌（与授权范围、过期时刻）。
    renewed_cells: &[],
    client_field: "clientId",
    token_url: "https://platform.claude.com/v1/oauth/token",
    client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
    // 凭据里不记出处：续期照登记的端点与客户端 id；续成不另记时刻（时刻就在 `expiresAt` 那一格）。
    origin: None,
    renewed_at: None,
    margin_ms: 300_000,
    lock_inside: Some(".oauth_refresh.lock"),
    lock_beside: Some(".lock"),
    // 60 秒：旁边那把是 proper-lockfile 的形状，持有方每 5 秒刷一次修改时刻、10 秒没刷算过期（按锁的形状推断，没核那一家的源码）；
    // 我方持锁时不刷修改时刻，最长一趟是令牌端点那一发（每次读写期限 15 秒）。取 60 秒，活着的持有方（它的、我们的）都不会被判过期。
    lock_stale_ms: 60_000,
    // 2.1.296 起整份写凭据文件（续期写回 · MCP 登录存盘）都在 `<配置目录>/.storage-write` 的 proper-lockfile 锁里
    // （`mkdir <配置目录>/.storage-write.lock`，过期门限 15 秒），锁里重读、只改自己那一格、临时文件 ＋ 改名。10-12 沙箱 strace 实测。
    write_lock: Some(super::WriteLock::Mkdir(STORAGE_WRITE_LOCK)),
    write_lock_stale_ms: STORAGE_WRITE_STALE_MS,
    write_wait_ms: 20_000,
    identity_file: accounts::identity_file,
    identity_in: accounts::identity_in,
    identity_of_body: accounts::identity_of_body,
    rewrite_identity: accounts::rewrite_identity,
    // 账号 0（`~/.claude`）同号载体那几个号：照续、续成写回。
    base_login: super::BaseLogin::Renew,
};

/// claude 整份写 `.credentials.json` 时拿的那把 proper-lockfile 锁（`mkdir`）与它的过期门限：订阅号登录那一面（[`LOGIN`]）
/// 与 MCP 登录那一格（`mcp::LOGINS`，同一份文件）都登这一把。
pub(crate) const STORAGE_WRITE_LOCK: &str = ".storage-write.lock";
pub(crate) const STORAGE_WRITE_STALE_MS: u64 = 15_000;

/// 请求压缩上下文用的那一句（注册表 `Adapter.compact_request` 那一格）：在会话里敲的斜杠命令。
pub(crate) const COMPACT_REQUEST: &str = "/compact";

/// 清空会话用的那一句（注册表 `Adapter.clear_request` 那一格）：在会话里敲的斜杠命令（同一个进程里另起一份新记录，见 `wire::WireLife::Ended` 的 `by`）。
pub(crate) const CLEAR_REQUEST: &str = "/clear";

/// 聊天附件的引用写法（注册表 `Adapter.attach` 那一格）：图片单独括号粘贴一次绝对路径（输入框里收成 `[Image #N]`，
/// 记录里成图片块）；别的文件正文后面一行一个 `@相对路径`（它自己把那一份读进来）。
/// 哪几种算图片照通用表（`agents::PASTED_IMAGES`），这一家不多申报。
pub(crate) const ATTACH: super::AttachRefer = super::AttachRefer {
    image: |a| super::Refer::Paste(a.abs.to_string()),
    file: |a| super::Refer::Line(format!("@{}", a.rel)),
    images_extra: None,
};

/// 怎么判活（注册表 `Adapter.live` 那一格）：认 pidfile（[`LOCAL`] 的 pidfile 目录），不认谁开着记录。
pub(crate) const LIVE: crate::agents::LiveBy = crate::agents::LiveBy::Pidfiles;

/// 本家的**起会话事实**（注册表 `Adapter.launch` 那一格）：`ccm` 按它起、界面按生成物 `agent-profile-table.ts` 读，同一份。
pub const LAUNCH: super::LaunchFace = super::LaunchFace {
    adapter_id: UPSTREAM.route_id,
    speaker_name: "Claude",
    default_launcher: resume::DEFAULT_COMMAND,
    launcher_alias: Some(resume::LAUNCHER_ALIAS),
    resume_token: resume::RESUME_TOKEN,
    resume_short: Some("-r"),
    preset_sid: Some(resume::SESSION_ID_FLAG),
    // `claude [options] [prompt]`：位置参数的提示词在启动画面（信任框在里头）走完之后才交进会话。
    first_prompt_arg: true,
    permission: Some(super::PermissionFace {
        modes: resume::PERMISSION_MODES,
        shown: resume::SHOWN_MODES,
        switch: Some(super::ModeSwitch::Cycle(super::ModeCycle {
            key: "shift-tab",
            cycle_max: resume::MODE_CYCLE_MAX,
            read: resume::mode_on_screen,
        })),
    }),
    self_sid_env: Some(resume::SELF_SID_ENV),
    launch_args: &[],
    nested_env: resume::NESTED_ENV,
    is_default: true,
    models: Some(resume::MODEL_ALIASES),
    model_args: Some(resume::model_args),
    // 不联网列得出模型的子命令 claude 没有 ⇒ 照静态的那一张。
    model_list: None,
    model_default: Some(resume::MODEL_DEFAULT),
    model_text: Some(resume::model_text),
    model_pick: Some(resume::model_pick),
    model_switch: Some(super::ModelSwitch {
        command: resume::model_command,
        read: Some(resume::model_said),
        saves: Some(paths::SETTINGS_FILE),
        note: Some(resume::model_note),
        confirm: Some(super::ScreenBox {
            seen: resume::model_confirm_on_screen,
            key: "enter",
        }),
    }),
    rewind: Some(rewind::FACE),
    // `claude -p`：不开交互界面、跑完就退；结果是 `--output-format json` 那一份的 `result`（它不写结果文件）。
    task: Some(super::TaskFace {
        args: task_args,
        text: super::TaskText::Stdout(task_text),
    }),
};

/// 一次性跑一件活交给 `claude` 的那几个参数：`-p --output-format json -- <说明>`（说明作位置参数、放在 `--` 之后，不被认成旗标）；
/// 结果文件不交（它不写，结果在标准输出）。
fn task_args(say: &str, _out: &str) -> Vec<String> {
    ["-p", "--output-format", "json", "--", say]
        .map(str::to_string)
        .to_vec()
}

/// `claude -p --output-format json` 印的那一份里的 `result`（最后那段话）；不是那一形 ⇒ `None`。
fn task_text(stdout: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(stdout.trim())
        .ok()?
        .get("result")?
        .as_str()
        .map(str::to_string)
}

/// 本 agent 在这台机器上的 home 目录 —— **只答"它该在哪"，不答"在不在"**。
///
/// 「在不在」的判准是通用层的机器（`agents::visible_homes`），不是每家自己定一套 ——
/// 那正是 `S3` 立的分界（知识住适配层、机器留通用层）在发现这件事上的兑现。
///
/// 恒 `Some`：[`paths::resolve_home`] 有默认值（`$HOME/.claude`，再退 cwd 下的 `.claude`），
/// 解析不出来这种事对 Claude 不存在。签名仍取 `Option` 是为了与
/// [`super::codex::home`] 同形（那家真的可能答不出来）。
pub fn home() -> Option<std::path::PathBuf> {
    Some(paths::resolve_home())
}

/// 本家的资产面（注册表 `Adapter.assets` 那一格）：skill 与项目级 MCP 的布局知识住 [`assets`]。
/// 注册表 `mcp` 那一格。
/// 本机布局：家目录（`CLAUDE_CONFIG_DIR` / `~/.claude`）· pidfile 目录 `<家>/sessions` · 进程名单兜底认 cmdline。
pub(crate) const LOCAL: super::LocalFace = super::LocalFace {
    home_at: paths::home_of,
    pidfile_dir: paths::sessions_root,
    cmdline_may_be_agent: liveness::cmdline_may_be_agent,
    tasks: Some(tasks::FACE),
    proc_of: pidfile::proc_of,
};

pub(crate) const MCP: super::McpFace = super::McpFace {
    read: mcp::read,
    login: Some(mcp::login_argv),
    sign_in: Some(sign_in::MCP_FACE),
    needs_login_file: Some(mcp::NEEDS_AUTH_CACHE),
};

/// 记录解释面（注册表 `Adapter.records` 那一格）。
pub const RECORDS: super::RecordFace = super::RecordFace {
    parse: parse::translated,
    sid: records::session_id_of,
    is_session_file: records::is_session_file,
    tree: Some(super::RecordTree {
        root: paths::projects_root,
        file_name: records::session_file_name,
    }),
    // 此刻在干什么由进程状态文件说（`LocalFace.proc_of`），记录不说。
    activity: None,
    chain: Some(chain::chain_fact),
    find_session: branch::find_session_file,
    branch: Some(super::BranchFace::Rewrite(branch::build_branch_records)),
    drift: Some(drift::snapshot),
    text: Some(super::TextFace {
        main: session_line::main_of,
        tool: session_line::tool_of,
        user: text::user_text_of_record,
        lands: record_of::lands_of,
        line: session_line::line_of,
        line_of: session_line::line_of_value,
        shape: session_line::shape_of,
    }),
    delete: Some(super::SessionDelete::File {
        locate: paths::session_file_for_delete_in,
        is_record: paths::is_session_record_path,
    }),
    archive: None,
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
        spawned: runs::spawned,
    }),
    project_dir: Some(parse::project_dir),
    mcp_said: Some(mcp::said_of),
    quiet_unread: Some(record_of::quiet_unread_of),
    skim: Some(skim),
    beats: Some(beat::BEATS),
};

/// 注册表 `RecordFace.skim`：行首就写着类型（簿记行都这么写）、而那一类翻译表不认得 ⇒ 只留身份那几格
/// （`type` · `uuid` · `parentUuid` · `timestamp`，同抢救时取的那几格，[`parse`]）。类型不在行首 · 带转义 · 认得 · 连 JSON 都不是 ⇒ 整行读。
fn skim(line: &super::Line<'_>) -> Option<serde_json::Value> {
    use serde_json::Value;
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Ident {
        uuid: Option<Value>,
        parent_uuid: Option<Value>,
        timestamp: Option<Value>,
    }
    let rest = line.text().strip_prefix(r#"{"type":""#)?;
    let t = rest.get(..rest.find('"')?).filter(|t| !t.contains('\\'))?;
    // 认不认得只问一处：类型 serde 认不出（`quiet_unread_of` 那一问；探针不带身份）。
    record_of::quiet_unread_of(&serde_json::json!({ "type": t }))?;
    let id: Ident = line.read().ok()?;
    let mut m = serde_json::Map::new();
    m.insert("type".into(), t.into());
    for (k, v) in [
        ("uuid", id.uuid),
        ("parentUuid", id.parent_uuid),
        ("timestamp", id.timestamp),
    ] {
        if let Some(v) = v {
            m.insert(k.into(), v);
        }
    }
    Some(Value::Object(m))
}

pub(crate) const ASSETS: super::AssetFace = super::AssetFace {
    scan: assets::scan,
    skills_root: assets::skills_root,
    project_skills_root: assets::project_skills_root,
    user_mcp_file: assets::claude_json,
    project_mcp_file: assets::PROJECT_MCP_FILE,
    servers_key: assets::SERVERS_KEY,
    plugins: assets::plugins,
    plugin_cells: Some(assets::PLUGIN_CELLS),
    session_scan: assets::session_scan,
    builtin_commands: assets::BUILTIN_COMMANDS,
};

/// 注册表里 Claude 那一行（`agents::REGISTRY` 列它）：逐格指到这一家的面，没有的照实 `None`。
pub const ADAPTER: super::Adapter = super::Adapter {
    kind: AGENT_KIND,
    display_name: Some("Claude Code"),
    home,
    account_env: Some(paths::CONFIG_DIR_ENV),
    assets: Some(ASSETS),
    history: None,
    upstream: Some(UPSTREAM),
    mcp: Some(MCP),
    footprint: Some(footprint::FACE),
    accounts: Some(accounts::FACE),
    records: Some(RECORDS),
    processes: Some(cards::PROCESS_NAMES),
    launch: Some(LAUNCH),
    compact_request: Some(COMPACT_REQUEST),
    clear_request: Some(CLEAR_REQUEST),
    local: Some(LOCAL),
    live: LIVE,
    asks: Some(asks::FACE),
    attach: Some(ATTACH),
};
