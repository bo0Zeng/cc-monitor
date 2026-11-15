//! Codex 适配层 —— backend 里所有 Codex 专属知识的唯一住址。
//!
//! | 子模块 | 装什么 |
//! |---|---|
//! | [`parse`] | rollout 记录的信封与字段抽取、会话目录定位 |
//! | [`resume`] | 起会话 / resume 的命令形状、默认命令名 · 权限档 · 模型与清单 · 会话开着时换档 · 换模型 · 压缩 · 清空投的那一句 |
//! | [`assets`] | skill 四处住处 · 会话能用的那几个 · 插件缓存 `plugins/cache/<市场>/<插件>/<版本>/` 与 `.codex-plugin/plugin.json` |
//! | [`mcp`] | `config.toml` 的 `[mcp_servers.<名>]` 读改 · `codex mcp login` · 存成文件的 MCP 登录 `.credentials.json` |
//! | [`accounts`] | 号的载体 `CODEX_HOME` · `auth.json` 凭据与续期写回 · 目录信任 |
//! | [`sign_in`] | `codex login` / `--device-auth` / `logout` / `login status` 那几形 |
//! | [`footprint`] | 各号碰的那几份 · `hooks.json` 的 Stop 钩子与它的信任 |
//!
//! 适配层的接口是四类能力（会话发现与判活 · 会话内容读 · 用量 · 起会话 / resume）。「这台机器上有没有这个 agent」已有（[`home`] + `agents::visible_homes`）；
//! 「有哪些会话、活没活」：没有 pidfile ⇒ 声明 [`LIVE`] ＝ 开着写（在世期间一直开着记录写），流式 watcher 据此认写者判活
//! （`observe/watcher.rs` 的「开着即活」那一路，记录根是合成历史面的那个根）。
//! 发现出来的东西不报给仓外（报出去是一次跨仓契约变更，是一次纯发布决策）。
//! 不要从「这里有个模块」推断「Codex 支持完整」。

// 账号面：号目录（`CODEX_HOME`）· 凭据 · 续期写回 · 目录信任。
pub mod accounts;
// MCP 面：`config.toml` 的 `[mcp_servers.<名>]` 读改 · `codex mcp login` · 存成文件的登录住哪。
pub mod mcp;
// 资产面：skill 住处（`$CODEX_HOME/skills` · `~/.agents/skills` · 项目里两处）· 插件缓存与清单。
pub mod assets;
// 答题口：起会话时经 `-c` 挂的会话级钩子（连信任哈希）· 钩子入参翻成统一事件。
pub mod asks;
// 一次性的 `codex app-server`（握手 · 一条请求 · 认回包）：报用量与按轮分叉共用。
pub(crate) mod app_server;
// 按轮分叉（`thread/fork` 叉到那一轮为止）。
pub(crate) mod branch;
// 一次性跑一件活（`codex exec`，结果写进结果文件）。
pub(crate) mod exec;
// 报用量（`account/rateLimits/read`）与开窗那一句。
pub(crate) mod usage;
// 足迹面：各号碰的那几份 · cc-bus 的 Stop 钩子那一格（只诊断不写）。
pub mod footprint;
// 一条记录答成一拍（轮次 · 会话事实 · 大纲只吃它）。
pub(crate) mod beat;
// 历史清单那一面（会话枚举 ＋ 首条真用户话）：本机的 Codex 合成项目 / 会话。
pub(crate) mod history;
// 钩子信任：信任键 · 信任哈希 · 版本门（Codex 内部的规则）。
pub(crate) mod hook_trust;
pub(crate) mod parse;
// 记录分类 ＋ 翻成通用记录。
pub mod record;
// 经中转那一份（默认上游 · 会话头 · 怎么指到中转 · 直接敲的那份配置里的地址）。
pub mod relay;
pub mod resume;
// 删会话交给 Codex 自己的命令（`codex delete <uuid> --force`）。
pub(crate) mod sessions_cli;
// 登录一个号那几形（`codex login` …）。
pub mod sign_in;
// 文本面：全文搜索 · 会话内查找 · 历史清单读这一家的记录。
pub(crate) mod text;

/// 这一家的历史清单面（注册表 `Adapter.history` 那一格；通用层经注册表够到它，不直呼本模块）。
pub const HISTORY: crate::agents::HistoryFace = crate::agents::HistoryFace {
    sessions: history::sessions,
    excerpt: history::first_user_excerpt,
    roots: history::records_roots,
};
/// 记录解释面（注册表 `Adapter.records` 那一格）。
pub const RECORDS: crate::agents::RecordFace = crate::agents::RecordFace {
    parse: record::translated,
    sid: parse::codex_sid_from_path,
    is_session_file: history::is_session_file,
    // 会话按日期分、不住按项目分的记录树（历史由合成历史面并进来）。
    tree: None,
    // 没有进程状态文件 ⇒ 此刻在干什么照记录里的事件翻（判法只在 `record::activity_of`）。
    activity: Some(record::activity_of),
    // 骨架那一问：照整条翻译的类（这一家的会话不大，不另写一份便宜判法 ⇒ 与翻译恒一致）。
    chain: None,
    find_session: history::find_session,
    // 自己叉：`codex fork` 只叉整条 ⇒ 经一次性 app-server 的 `thread/fork` 叉到那一条所在的那一轮为止。
    branch: Some(super::BranchFace::Own(branch::fork)),
    // 认不出的记录类型 · 压缩存的 rollout 记在这一家自己的账上（`record::drift_snapshot`）。
    drift: Some(record::drift_snapshot),
    text: Some(text::TEXT),
    // 删会话交 Codex 自己的命令（状态库跟着改），不直接删 rollout。
    delete: Some(super::SessionDelete::Command(sessions_cli::delete_command)),
    // 归档 · 取消归档交 Codex 自己的 `codex archive` · `codex unarchive`；归档了的 rollout 平铺住 `archived_sessions/`。
    archive: Some(super::ArchiveFace {
        archive: sessions_cli::archive_command,
        unarchive: sessions_cli::unarchive_command,
        archived: sessions_cli::is_archived,
    }),
    // 子运行 ＝ 派出的子线程（记录根下自己的一份 rollout，开头写着父线程；`record::CHILDREN`）。父那一侧在 `item_completed` 里说派出 · 收场（`record::child_link`）。
    // 它的流经中转时按请求自带的 `thread-id` 当场归位（不靠对账键）。
    response_id: None,
    run_of: None,
    child_link: Some(record::child_link),
    background: None,
    children: Some(record::CHILDREN),
    project_dir: Some(history::project_dir),
    mcp_said: None,
    quiet_unread: None,
    skim: None,
    beats: Some(beat::BEATS),
};

/// 本 agent 在 wire 上的 **`agent_kind` 值**。
///
/// 值域由既有契约定死（`"codex"`=codex，**大小写敏感** ·
/// `session_added.agent_kind` 发 `"codex"`），改它 = 改跨仓契约。
///
/// 它住在**适配层**而不是通用层，是为了让 `agents::REGISTRY` 那张注册表里
/// 一个 agent 名的字面量都没有（顺带：`agent_locality_guard` 判据②数的正是通用层里
/// 带引号的 `"codex"`，把它写在注册表里会**当场**多一条 kind 派发登记 —— 而注册表
/// 根本不在做派发）。
pub const AGENT_KIND: &str = "codex";

/// tmux 前台命令算 Codex 的会话：原生装的报 `codex`；npm 装的经 Node 包一层，前台报 `node`（已在 Claude 那张表里）。注册表 `Adapter.processes` 那一格。
pub(crate) const PROCESS_NAMES: &[&str] = &["codex"];

/// 聊天附件的引用写法（注册表 `Adapter.attach` 那一格）：图片单独括号粘贴一次绝对路径（收成图片附件）；
/// 别的文件正文后面一行一个相对路径（没有 `@` 展开）。没在真 Codex 上核过。
/// 哪几种算图片：通用表（`agents::PASTED_IMAGES`）＋ [`IMAGES_EXTRA`]。
pub(crate) const ATTACH: super::AttachRefer = super::AttachRefer {
    image: |a| super::Refer::Paste(a.abs.to_string()),
    file: |a| super::Refer::Line(a.rel.to_string()),
    images_extra: Some(IMAGES_EXTRA),
};

/// Codex 比通用表多认的图片扩展名：它的输入框按粘进来的路径能不能 `image::image_dimensions` 判（codex-rs rust-v0.167.0
/// `tui/src/bottom_pane/chat_composer.rs`），格式照扩展名定；那一版的 `image` 0.25.9 只开 jpeg · png · gif · webp，
/// 这四种认的扩展名是 png · apng · jpg · jpeg · jfif · gif · webp ⇒ 多出这两个。没开的格式（bmp · tiff …）解不了，照普通文件写。
const IMAGES_EXTRA: &[&str] = &["apng", "jfif"];

/// 怎么判活（注册表 `Adapter.live` 那一格）：认开着写：没有 pidfile，会话在世期间写记录的那个任务一直开着这份 rollout（codex-rs `rollout/recorder.rs` 的写者任务持有文件；
/// 新会话第一轮落盘时才建文件，resume 打开旧的那份接着写）。⚠ 只读过源码，没在真机上实测过。
pub(crate) const LIVE: crate::agents::LiveBy = crate::agents::LiveBy::HeldOpen;

/// 本家的起会话事实（注册表 `Adapter.launch` 那一格）。适配器 id 就是 wire 上的 kind（也是中转路由里它的名字）；没有 shell wrapper。
pub(crate) const LAUNCH: crate::agents::LaunchFace = crate::agents::LaunchFace {
    adapter_id: AGENT_KIND,
    speaker_name: "Codex",
    default_launcher: resume::DEFAULT_COMMAND,
    launcher_alias: Some(resume::LAUNCHER_ALIAS),
    resume_token: resume::RESUME_TOKEN,
    resume_short: None,
    preset_sid: None,
    // `codex [OPTIONS] [PROMPT]` · `codex resume [SESSION_ID] [PROMPT]`：位置参数的第一句；它的记录要第一句进去才建。
    first_prompt_arg: true,
    // 档名 ⇒ `-a <策略> -s <沙箱>`；会话中途投 `/permissions` 那一句直接切。
    permission: Some(super::PermissionFace {
        modes: resume::PERMISSION_MODES,
        shown: resume::SHOWN_MODES,
        switch: Some(super::ModeSwitch::Command(resume::permission_command)),
    }),
    self_sid_env: Some(resume::SELF_SID_ENV),
    launch_args: resume::LAUNCH_ARGS,
    nested_env: resume::NESTED_ENV,
    is_default: false,
    // 没有静态的那一张：清单照这台装的那一版自己列（`codex debug models --bundled`）；记录里的模型原值写给人看的名字不考据。
    models: None,
    model_args: Some(resume::model_args),
    model_list: Some(resume::MODEL_LIST),
    model_default: None,
    model_text: None,
    model_pick: None,
    model_switch: Some(resume::MODEL_SWITCH),
    // 它也有双击 Esc（回到上一句重编辑），按法与屏都和 Claude 那一套不同、没实测过 ⇒ 不申报，编辑已发的一句照实回「不支持」。
    rewind: None,
    // `codex exec`：跑完就退，最后那段话写进结果文件（参数与依据见 [`exec`]）。一件活不铸来处 ⇒ 中转照地址里交的那个号认。
    task: Some(exec::TASK),
};

/// 本 agent 在这台机器上的 home 目录 —— **只答"它该在哪"，不答"在不在"**。
///
/// 与 Claude 那家的**真实差别**：这里可能答不出来（`None`）——
/// 认 `$CODEX_HOME`（[`parse::HOME_ENV`]，非空），否则 `$HOME/.codex`；
/// **两个环境变量都没有**时它没有第三条退路（Claude 那边有 cwd 兜底）。
/// `None` = 连一个候选路径都说不出 ⇒ 一定看不见。读的是这个进程的环境（[`home_from`] 喂它）。
pub(crate) fn home() -> Option<std::path::PathBuf> {
    home_from(&|k| std::env::var_os(k))
}

/// [`home`] 的规矩本体：环境变量从 `get` 读（判据喂合成的）。
fn home_from(get: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> Option<std::path::PathBuf> {
    if let Some(h) = get(parse::HOME_ENV).filter(|h| !h.is_empty()) {
        return Some(std::path::PathBuf::from(h));
    }
    crate::platform::paths::home_dir_from(get).map(|h| h.join(".codex"))
}

/// 资产面（注册表 `Adapter.assets` 那一格）：布局知识住 [`assets`]。
/// MCP 那几格（用户级 / 项目级那份 `config.toml`、`mcp_servers` 那一节）是 TOML：核心今天只对缺省那一家（Claude）按 JSON 读写它们，
/// Codex 这几格只申报事实。
pub(crate) const ASSETS: super::AssetFace = super::AssetFace {
    scan: assets::scan,
    skills_root: assets::skills_root,
    project_skills_root: assets::project_skills_root,
    user_mcp_file: || home().map(|h| h.join(accounts::CONFIG_FILE)),
    project_mcp_file: PROJECT_MCP_FILE,
    servers_key: mcp::SERVERS_KEY,
    plugins: assets::plugins,
    plugin_cells: Some(assets::PLUGIN_CELLS),
    session_scan: assets::session_scan,
    // 内置斜杠命令的简介要文案表里的键，还没登 ⇒ 先不列。
    builtin_commands: &[],
};

/// MCP 读面（注册表 `Adapter.mcp` 那一格的值）：读法住 [`mcp`]。Codex 不在盘上记「要登录」⇒ 没有那份文件；
/// `codex mcp login` 开本机回调口等浏览器回来、不收粘贴 ⇒ 不申报 app 里登录那一形，只给命令。
pub const MCP: super::McpFace = super::McpFace {
    read: mcp::read,
    login: Some(mcp::login_argv),
    sign_in: None,
    needs_login_file: None,
};

/// 项目级 MCP 住的那份文件（相对项目目录）：项目配置层 `.codex/config.toml`。
const PROJECT_MCP_FILE: &str = ".codex/config.toml";

/// 注册表里 Codex 那一行（`agents::REGISTRY` 列它）：逐格指到这一家的面，没有的照实 `None`。
pub const ADAPTER: super::Adapter = super::Adapter {
    kind: AGENT_KIND,
    display_name: Some("Codex"),
    home,
    // 一号一个 `CODEX_HOME` 目录（`accounts`）。
    account_env: Some(parse::HOME_ENV),
    assets: Some(ASSETS),
    history: Some(HISTORY),
    upstream: Some(relay::UPSTREAM),
    mcp: Some(MCP),
    footprint: Some(footprint::FACE),
    accounts: Some(accounts::FACE),
    records: Some(RECORDS),
    processes: Some(PROCESS_NAMES),
    launch: Some(LAUNCH),
    compact_request: Some(resume::COMPACT_REQUEST),
    clear_request: Some(resume::CLEAR_REQUEST),
    local: None,
    live: LIVE,
    asks: Some(asks::FACE),
    attach: Some(ATTACH),
};

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/mod_tests.rs"]
mod mod_tests;
