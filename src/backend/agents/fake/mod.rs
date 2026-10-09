//! 最小假 agent（只在测试档）：一个与 Claude、Codex **每一格都不同形**的第三家，
//! 通用判据拿它再跑一遍 —— 跑得通才算通用层没硬绑某一家。
//!
//! 它住 `agents/<名>/`、进 `agent_locality_guard::HOMES`，但**不进** `REGISTRY`，模块声明带 `#[cfg(test)]`
//! （`the_fixture_agent_never_ships` 双向钉着）；`agent_locality_guard::FIXTURE_HOMES` 登记它并带天花板。
//!
//! # 它的每一种能力都与 Claude 不同形
//!
//! | 能力 | Claude | 本假 agent | 注册表那一格 |
//! |---|---|---|---|
//! | 会话记录根 | `<home>/projects` | `<home>/convos` | `RecordFace.tree.root` |
//! | 会话文件判定 | `.jsonl` | `sess-*.ndjson` | `RecordFace.is_session_file` |
//! | 会话文件命名 | `<sid>.jsonl` | `sess-<sid>.ndjson` | `RecordFace.tree.file_name` |
//! | pidfile 目录 | `<home>/sessions` | `<home>/live` | `LocalFace.pidfile_dir` |
//! | 账号环境变量名 | `CLAUDE_CONFIG_DIR` | `CCM_FAKE_ACCOUNT_DIR` | `AccountsFace.session_env` |
//! | 账号信任判定 | `.claude.json` → `projects[cwd].hasTrustDialogAccepted` | `fake-config.json` → `trusted[cwd]` | `AccountsFace.trust_in` |
//! | 判活 cmdline | 含 `claude` / `node` | 含 `fakeagent` | `LocalFace.cmdline_may_be_agent` |
//! | 解析本机 home | `$CLAUDE_CONFIG_DIR` 否则 `$HOME/.claude`（**恒有值**） | `$CCM_FAKE_AGENT_HOME`，**没有默认** | `Adapter.home` · `LocalFace.home_at` |
//! | resume 默认命令 | `claude` | `fakeagent` | `LaunchFace.default_launcher` |
//! | resume 命令形 | `<base> --resume <sid>`（flag） | `<base> revive <sid>`（子命令） | `LaunchFace.resume_command` |
//! | resume 会话名前缀 | `cc` | `fk` | `LaunchFace.session_name_prefix` |
//!
//! 不同形是要紧的：照抄 Claude 的布局的话，通用层拿 Claude 的知识去解释它的 home 恰好也能读出东西，
//! 「走通」就证明不了通用层容得下第二种布局。
//!
//! # 走全流程
//!
//! [`walk`] 把假 agent 推过六段，每一段都经通用层：发现（`agents::visible_among`）· 宣告（`wire::Frame::Hello`）·
//! 读会话 / 判活 / 账号（注册表里这一家那几格，经 `agents::*_among`）· resume（`resolve_query::resolve_json_among`）。
//! 挖掉任一种能力（[`FakeCaps::without`]），那一格的面就拼不出来，流程停在点得出名字的那一段。

use std::path::{Path, PathBuf};

/// 本假 agent 在 wire 上的 `agent_kind` 值。
///
/// ⚠ 它**永远不会**出现在真 `hello.homes` 里 —— 本层不进 `agents::REGISTRY`
/// （`the_fixture_agent_never_ships` 钉住）。
// 子运行形状（与 Claude Code 每一格都不同形；通用层的运行判据拿它跑）。
pub(crate) mod runs;

pub(crate) const AGENT_KIND: &str = "fake";

/// 解析本机 home 的环境变量。⚠ **刻意没有默认值**（Claude 那家恒有值）——
/// 少了这个差别，`visible_among` 里 `None` 那条分支就永远不会被这家走一遍。
const HOME_ENV: &str = "CCM_FAKE_AGENT_HOME";

/// 能力 8：解析本机 home。`None` = 连候选路径都说不出。
pub(crate) fn home() -> Option<PathBuf> {
    std::env::var_os(HOME_ENV).map(PathBuf::from)
}

/// 能力 1：会话记录根。**`convos` 不是 `projects`**。
pub(crate) fn records_root(home: &Path) -> PathBuf {
    home.join("convos")
}

/// 能力 2：这个路径是不是一份会话记录。**`.ndjson` 不是 `.jsonl`**。
pub(crate) fn is_session_file(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "ndjson")
        && p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("sess-"))
}

/// 能力 3：会话 id → 记录文件名。**带前缀**，与 Claude 的裸 `<sid>.<ext>` 不同形。
pub(crate) fn session_file_name(sid: &str) -> String {
    format!("sess-{sid}.ndjson")
}

/// 能力 4：pidfile 目录。**`live` 不是 `sessions`**。
pub(crate) fn pidfile_root(home: &Path) -> PathBuf {
    home.join("live")
}

/// 能力 5：账号（配置根）环境变量名。
pub(crate) const ACCOUNT_DIR_ENV: &str = "CCM_FAKE_ACCOUNT_DIR";

/// 能力 6：某个配置根下、对某个 cwd 的信任判定。
///
/// 形状与 Claude 那家**不同**：文件名不同、字段路径不同（`trusted[cwd]` 而不是
/// `projects[cwd].hasTrustDialogAccepted`）。
pub(crate) fn trust_of_config(root: &Path, cwd: &str) -> Result<bool, String> {
    let p = root.join("fake-config.json");
    let bytes = std::fs::read(&p).map_err(|e| format!("fake_config_unreadable: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("fake_config_invalid: {e}"))?;
    Ok(v.get("trusted")
        .and_then(|t| t.get(cwd))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false))
}

/// 能力 7：判活时"这个 cmdline 看起来像不像本 agent"。
///
/// ⚠ 与 Claude 那家**方向相同但词表不同**（那边是「明显不像才判冒名」，含 `claude`/`node`）。
pub(crate) fn cmdline_may_be_agent(lower: &str) -> bool {
    lower.trim().is_empty() || lower.contains("fakeagent")
}

/// 能力 10：无 `launchCandidate` 时的默认命令基底。
pub(crate) const DEFAULT_COMMAND: &str = "fakeagent";

/// resume 那个字面量：子命令形，动词与 Codex 也不同。
pub(crate) const RESUME_TOKEN: &str = "revive";

/// 能力 11：resume 命令 —— **子命令形，且动词与 Codex 也不同**。
pub(crate) fn resume_command(base: &str, session_id: &str) -> String {
    format!("{base} {RESUME_TOKEN} {session_id}")
}

/// 能力 12：resume 会话名前缀（Claude `cc` / Codex `cx`）。
pub(crate) const SESSION_NAME_PREFIX: &str = "fk";

/// 起会话事实（`Adapter.launch` 那一格）。**组合与两家都不同**：Claude 是「有身份面 · 留 pidfile · 不要 cc-bus 身份」，
/// Codex 是「要 cc-bus 身份，其余都没有」；这一家是「要 cc-bus 身份 · 有身份面 · 不留 pidfile」。
/// 通用层若在哪一格上按名字认人，喂这一家就会答错（`fake_tests.rs` 的 ccm 规划那几条）。
pub(crate) const LAUNCH: crate::agents::LaunchFace = crate::agents::LaunchFace {
    adapter_id: AGENT_KIND,
    display_name: "Fake",
    speaker_name: "Fake",
    default_launcher: DEFAULT_COMMAND,
    launcher_alias: None,
    resume_token: RESUME_TOKEN,
    preset_sid: None,
    launch_args: &[],
    nested_env: &["FAKEAGENT_PARENT"],
    is_default: false,
    resume_command,
    session_name_prefix: SESSION_NAME_PREFIX,
    needs_bus_id: true,
    has_identity: true,
    has_pidfiles: false,
    models: None,
};

// ─────────────────────────────────────────────────────────────────────────────
// 能力表 + 走全流程的 driver
// ─────────────────────────────────────────────────────────────────────────────

/// 各种能力的**名字**，与 `agent_locality_guard::tests::CAPABILITY_FACES`（每一种住注册表哪一格）逐条对账。
///
/// ⚠ 顺序即 [`FakeCaps`] 字段序，[`FakeCaps::without`] 按名字挖洞时靠它。
pub(crate) const CAPABILITIES: &[&str] = &[
    "会话记录根",
    "会话文件判定",
    "会话文件命名",
    "pidfile 目录",
    "账号环境变量名",
    "账号信任判定",
    "判活 cmdline",
    "解析本机 home",
    "resume 默认命令",
    "resume 命令形",
    "resume 会话名前缀",
];

/// 假 agent 交出来的一整套能力。每一项都是 `Option`：反向夹具要能「把某一种能力删掉」，而删掉之后流程必须在一个说得出话的地方停下来；
/// 用 `Option` 而不是手工注释掉一个函数，那个反向夹具才是常驻判据。
#[derive(Clone)]
pub(crate) struct FakeCaps {
    pub(crate) records_root: Option<fn(&Path) -> PathBuf>,
    pub(crate) is_session_file: Option<fn(&Path) -> bool>,
    pub(crate) session_file_name: Option<fn(&str) -> String>,
    pub(crate) pidfile_root: Option<fn(&Path) -> PathBuf>,
    pub(crate) account_dir_env: Option<&'static str>,
    pub(crate) trust_of_config: Option<fn(&Path, &str) -> Result<bool, String>>,
    pub(crate) cmdline_may_be_agent: Option<fn(&str) -> bool>,
    pub(crate) home: Option<fn() -> Option<PathBuf>>,
    pub(crate) default_command: Option<&'static str>,
    pub(crate) resume_command: Option<fn(&str, &str) -> String>,
    pub(crate) session_name_prefix: Option<&'static str>,
}

impl FakeCaps {
    /// 还剩几种能力在（[`without`](Self::without) 的自检用）。
    pub(crate) fn present(&self) -> usize {
        [
            self.records_root.is_some(),
            self.is_session_file.is_some(),
            self.session_file_name.is_some(),
            self.pidfile_root.is_some(),
            self.account_dir_env.is_some(),
            self.trust_of_config.is_some(),
            self.cmdline_may_be_agent.is_some(),
            self.home.is_some(),
            self.default_command.is_some(),
            self.resume_command.is_some(),
            self.session_name_prefix.is_some(),
        ]
        .iter()
        .filter(|b| **b)
        .count()
    }

    /// 完整的一套（正题用）。
    pub(crate) fn full() -> Self {
        Self {
            records_root: Some(records_root),
            is_session_file: Some(is_session_file),
            session_file_name: Some(session_file_name),
            pidfile_root: Some(pidfile_root),
            account_dir_env: Some(ACCOUNT_DIR_ENV),
            trust_of_config: Some(trust_of_config),
            cmdline_may_be_agent: Some(cmdline_may_be_agent),
            home: Some(home),
            default_command: Some(DEFAULT_COMMAND),
            resume_command: Some(resume_command),
            session_name_prefix: Some(SESSION_NAME_PREFIX),
        }
    }

    /// 挖掉**一种**能力（反向夹具用）。名字必须是 [`CAPABILITIES`] 里的一条 —— 写错就 panic，
    /// 不静默返回原样（静默的话反向夹具会**变成正题**再跑一遍，而它会绿）。
    pub(crate) fn without(&self, capability: &str) -> Self {
        assert!(
            CAPABILITIES.contains(&capability),
            "`{capability}` 不是一种登记过的能力 —— 反向夹具挖了个不存在的洞，\
             这一格此刻在把正题当反向跑（而正题是绿的）。已登记的：{CAPABILITIES:?}"
        );
        let mut c = self.clone();
        match capability {
            "会话记录根" => c.records_root = None,
            "会话文件判定" => c.is_session_file = None,
            "会话文件命名" => c.session_file_name = None,
            "pidfile 目录" => c.pidfile_root = None,
            "账号环境变量名" => c.account_dir_env = None,
            "账号信任判定" => c.trust_of_config = None,
            "判活 cmdline" => c.cmdline_may_be_agent = None,
            "解析本机 home" => c.home = None,
            "resume 默认命令" => c.default_command = None,
            "resume 命令形" => c.resume_command = None,
            "resume 会话名前缀" => c.session_name_prefix = None,
            other => unreachable!("`{other}` 在 CAPABILITIES 里却没有对应字段 —— 两处漂开了"),
        }
        c
    }
}

/// 全流程的 6 段。名字进错误信息，所以是常量而不是字面量。
pub(crate) const STAGES: &[&str] = &[
    "发现",
    "宣告（hello.homes）",
    "读会话",
    "判活",
    "账号",
    "resume",
];

/// 流程停下来的原因：点名是哪一段缺了哪一种能力（不许静默当成「这个 agent 没有会话」）。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    /// 假 agent 自己少了一种能力 —— 点名是**哪一段**缺了**哪一种**。
    MissingCapability {
        stage: &'static str,
        capability: &'static str,
    },
}

/// 假 agent 的 MCP 读面（夹具家那一份布局）：只认 `<项目>/.fake-mcp.json` 的 `servers` 表，一律记成 project 段。
/// 它要证的是「通用层经注册表那一格读 MCP、不认识任何一家的文件名」—— 判据 `fake_tests.rs::the_fake_agents_mcp_face_is_read_through_the_generic_layer`。
pub(crate) const MCP: crate::agents::McpFace = crate::agents::McpFace { read: read_mcp };

fn read_mcp(project_dir: Option<&Path>) -> crate::agents::McpRead {
    let mut out = crate::agents::McpRead::default();
    let Some(dir) = project_dir else { return out };
    let file = dir.join(".fake-mcp.json");
    let Ok(text) = std::fs::read_to_string(&file) else {
        return out;
    };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(v) => {
            for (name, server) in v
                .get("servers")
                .and_then(|s| s.as_object())
                .into_iter()
                .flatten()
            {
                out.entries.push(crate::agents::McpEntry {
                    scope: "project",
                    name: name.clone(),
                    server: server.clone(),
                    source: file.display().to_string(),
                });
            }
        }
        Err(e) => out
            .problems
            .push(format!("读 {} 失败：{e}", file.display())),
    }
    out
}

/// 只带 home 的一行（其余各面都没有）；各段在它上面补那一段要的面。
pub(crate) fn bare_row(home: fn() -> Option<PathBuf>) -> crate::agents::Adapter {
    crate::agents::Adapter {
        kind: AGENT_KIND,
        home,
        account_env: None,
        assets: None,
        history: None,
        upstream: None,
        mcp: None,
        footprint: None,
        accounts: None,
        records: None,
        processes: None,
        launch: None,
        compact_request: None,
        local: None,
    }
}

/// 一整行：本假 agent 的全部面（判据「同一批通用判据对三家各跑一遍」用）。
pub(crate) fn full_row(home: fn() -> Option<PathBuf>) -> crate::agents::Adapter {
    crate::agents::Adapter {
        account_env: Some(ACCOUNT_DIR_ENV),
        mcp: Some(MCP),
        records: Some(records_face(
            records_root,
            is_session_file,
            session_file_name,
        )),
        local: Some(local_face(pidfile_root, cmdline_may_be_agent)),
        accounts: Some(accounts_face(ACCOUNT_DIR_ENV)),
        launch: Some(LAUNCH),
        ..bare_row(home)
    }
}

/// 记录解释面：只带记录树与文件判定（本假 agent 的记录内容通用判据用不到 ⇒ 解析恒「这一行不出成品」）。
fn records_face(
    root: fn(&Path) -> PathBuf,
    is_session_file: fn(&Path) -> bool,
    file_name: fn(&str) -> String,
) -> crate::agents::RecordFace {
    crate::agents::RecordFace {
        parse: |_| Ok(None),
        sid: session_id_of,
        is_session_file,
        tree: Some(crate::agents::RecordTree { root, file_name }),
        turn_end: None,
        chain: Some(chain_fact),
        price: None,
        find_session: None,
        branch: None,
        drift: None,
        text: None,
        delete: None,
        response_id: None,
        run_of: None,
        child_link: None,
        children: None,
        project_dir: None,
    }
}

/// 链事实（与 Claude 每一格都不同名）：`{"node", "up", "when", "role": human | bot | note, "cut": bool, "words"}`；
/// 排队那句是 `{"queued": "…"}`。只有 `human` / `bot` 进界面。
pub(crate) fn chain_fact(raw: &str) -> Option<crate::agents::mainline::ChainFact> {
    use crate::agents::mainline::{ChainFact, Link};
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let s = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    if let Some(q) = s("queued") {
        return Some(ChainFact::Queued(q));
    }
    let role = s("role")?;
    Some(ChainFact::Node(Link {
        id: s("node")?,
        parent: s("up"),
        at: s("when")?,
        said: role == "human",
        reply: role == "bot",
        interrupt: v.get("cut").and_then(serde_json::Value::as_bool) == Some(true),
        text: (role == "human").then(|| s("words").unwrap_or_default()),
        shown: role != "note",
    }))
}

/// `sess-<sid>.ndjson` ⇒ sid。
fn session_id_of(p: &Path) -> Option<String> {
    let name = p.file_name()?.to_str()?;
    Some(
        name.strip_prefix("sess-")?
            .strip_suffix(".ndjson")?
            .to_string(),
    )
}

/// 本机布局面。家目录：给了账号配置目录就是它，否则 [`home`]（没有默认 ⇒ 空路径）。
fn local_face(
    pidfile_dir: fn(&Path) -> PathBuf,
    cmdline_may_be_agent: fn(&str) -> bool,
) -> crate::agents::LocalFace {
    crate::agents::LocalFace {
        home_at: |config_dir, _| {
            config_dir
                .map(Path::to_path_buf)
                .or_else(home)
                .unwrap_or_default()
        },
        pidfile_dir,
        cmdline_may_be_agent,
        tasks_dir: Some(|h| h.join("todo")),
        background_of: |v| v.get("bg").and_then(serde_json::Value::as_bool) == Some(true),
        activity_of: |_| None,
    }
}

/// 账号库面：只带通用判据会问的那两格（会话进程环境的键 · 信任预检），其余空着。
fn accounts_face(account_env: &'static str) -> crate::agents::AccountsFace {
    crate::agents::AccountsFace {
        identity: &[],
        config_file: "fake-config.json",
        user_mcp_key: "servers",
        shared_root: |h| h.join("shared"),
        email_in: |_| None,
        watched: &[],
        session_env: crate::agents::SessionEnvKeys {
            config_dir: account_env,
            base_url: "CCM_FAKE_UPSTREAM",
            settings_may_set_base_url: |_, _, _| false,
        },
        trust_in: trust_line,
        trust: Some(TRUST_CELLS),
    }
}

/// 信任那一格：`fake-config.json` → `trusted[<目录>] = true`（目录那一项本身就是那个布尔；键原样）。
pub(crate) const TRUST_CELLS: crate::agents::TrustCells = crate::agents::TrustCells {
    table: "trusted",
    flag: None,
    dir_keys: |cwd| vec![cwd.to_string()],
};

/// [`trust_of_config`] 说成账号库面那一形（`{trusted, known, error}` 一行；读不了 ⇒ `(码, 原话)`）。
fn trust_line(root: &Path, cwd: &str) -> Result<String, (String, String)> {
    let trusted = trust_of_config(root, cwd).map_err(|e| ("fake_config".to_string(), e))?;
    Ok(serde_json::json!({ "trusted": trusted, "known": true, "error": null }).to_string())
}

/// 把假 agent 推过全流程，每一段都经通用层（见模块头注）。成功 = 走完的段名。
pub(crate) fn walk(caps: &FakeCaps, fixture_home: &Path) -> Result<Vec<&'static str>, Stop> {
    let mut done: Vec<&'static str> = Vec::new();

    // ── ① 发现：**真的走通用层**（注册表 × 判准）。
    let home_fn = caps.home.ok_or(Stop::MissingCapability {
        stage: STAGES[0],
        capability: "解析本机 home",
    })?;
    // `Adapter.home` 是裸函数指针，所以"加一个 agent"在这一层真的只是多一条记录。
    // MCP 读面同拍长一格：假 agent 的布局是 `<项目>/.fake-mcp.json` 的 `servers` 表（见 [`MCP`]）。
    let adapter = crate::agents::Adapter {
        mcp: Some(MCP),
        ..bare_row(home_fn)
    };
    let discovered = crate::agents::visible_among(std::slice::from_ref(&adapter));
    if discovered.len() != 1 {
        return Err(Stop::MissingCapability {
            stage: STAGES[0],
            capability: "解析本机 home",
        });
    }
    done.push(STAGES[0]);

    // ── ② 宣告：**真的走通用层**（塞进 `Hello.homes` 并序列化成一行）。
    let line = crate::stream::wire::to_line(&crate::stream::wire::Frame::Hello {
        v: 1,
        build_id: "s6".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/c".into(),
        homes: discovered.clone(),
        capabilities: vec![],
        emits: vec![],
        commands: vec![],
        // 握手帧的「这台做不到」：第三家 agent 不带命令，空表 ⇒ 省略 ⇒ 下面那串期望字节不变。
        unavailable: vec![],
        host_env: Default::default(),
        uncancellable: vec![],
    })
    .map_err(|_| Stop::MissingCapability {
        stage: STAGES[1],
        capability: "解析本机 home",
    })?;
    if !line.contains(&format!("\"agent_kind\":\"{AGENT_KIND}\"")) {
        return Err(Stop::MissingCapability {
            stage: STAGES[1],
            capability: "解析本机 home",
        });
    }
    done.push(STAGES[1]);

    // ── ③ 读会话：经通用层（注册表里这一家的记录树根 · 会话文件判定 · 新会话文件命名）。
    let root_fn = caps.records_root.ok_or(Stop::MissingCapability {
        stage: STAGES[2],
        capability: "会话记录根",
    })?;
    let is_session = caps.is_session_file.ok_or(Stop::MissingCapability {
        stage: STAGES[2],
        capability: "会话文件判定",
    })?;
    let file_name = caps.session_file_name.ok_or(Stop::MissingCapability {
        stage: STAGES[2],
        capability: "会话文件命名",
    })?;
    let reg = [crate::agents::Adapter {
        records: Some(records_face(root_fn, is_session, file_name)),
        ..bare_row(home_fn)
    }];
    let mut found: Vec<PathBuf> = Vec::new();
    let records_root = crate::agents::records_root_among(&reg, AGENT_KIND, fixture_home);
    if let Some(Ok(projects)) = records_root.map(std::fs::read_dir) {
        let mut dirs: Vec<PathBuf> = projects.flatten().map(|e| e.path()).collect();
        dirs.sort();
        for d in dirs {
            if let Ok(files) = std::fs::read_dir(&d) {
                let mut ps: Vec<PathBuf> = files.flatten().map(|e| e.path()).collect();
                ps.sort();
                found.extend(ps.into_iter().filter(|p| {
                    p.is_file() && crate::agents::is_session_file_among(&reg, AGENT_KIND, p)
                }));
            }
        }
    }
    if found.is_empty() {
        return Err(Stop::MissingCapability {
            stage: STAGES[2],
            capability: "会话记录根",
        });
    }
    // 命名口径与判定口径必须互洽：拿 sid 算出来的名字得等于扫出来的那个文件名。
    let sid = FIXTURE_SESSION_ID;
    let named = crate::agents::record_tree_among(&reg, AGENT_KIND).map(|t| (t.file_name)(sid));
    if !found
        .iter()
        .any(|p| p.file_name().and_then(|n| n.to_str()) == named.as_deref())
    {
        return Err(Stop::MissingCapability {
            stage: STAGES[2],
            capability: "会话文件命名",
        });
    }
    done.push(STAGES[2]);

    // ── ④ 判活：经通用层（注册表里这一家的 pidfile 目录 + cmdline 认法）。
    let pid_root = caps.pidfile_root.ok_or(Stop::MissingCapability {
        stage: STAGES[3],
        capability: "pidfile 目录",
    })?;
    let cmdline = caps.cmdline_may_be_agent.ok_or(Stop::MissingCapability {
        stage: STAGES[3],
        capability: "判活 cmdline",
    })?;
    let reg = [crate::agents::Adapter {
        local: Some(local_face(pid_root, cmdline)),
        ..bare_row(home_fn)
    }];
    let local =
        crate::agents::local_face_among(&reg, AGENT_KIND).ok_or(Stop::MissingCapability {
            stage: STAGES[3],
            capability: "pidfile 目录",
        })?;
    let pidfiles = std::fs::read_dir((local.pidfile_dir)(fixture_home))
        .map(|rd| rd.flatten().count())
        .unwrap_or(0);
    if pidfiles == 0 {
        return Err(Stop::MissingCapability {
            stage: STAGES[3],
            capability: "pidfile 目录",
        });
    }
    if !(local.cmdline_may_be_agent)("fakeagent revive x")
        || (local.cmdline_may_be_agent)("/usr/bin/vim")
    {
        return Err(Stop::MissingCapability {
            stage: STAGES[3],
            capability: "判活 cmdline",
        });
    }
    done.push(STAGES[3]);

    // ── ⑤ 账号：经通用层（注册表里这一家账号库面的会话环境键 + 信任预检）。
    let env_name = caps.account_dir_env.ok_or(Stop::MissingCapability {
        stage: STAGES[4],
        capability: "账号环境变量名",
    })?;
    caps.trust_of_config.ok_or(Stop::MissingCapability {
        stage: STAGES[4],
        capability: "账号信任判定",
    })?;
    let reg = [crate::agents::Adapter {
        accounts: Some(accounts_face(env_name)),
        ..bare_row(home_fn)
    }];
    let face =
        crate::agents::accounts_face_among(&reg, AGENT_KIND).ok_or(Stop::MissingCapability {
            stage: STAGES[4],
            capability: "账号环境变量名",
        })?;
    if face.session_env.config_dir.trim().is_empty() {
        return Err(Stop::MissingCapability {
            stage: STAGES[4],
            capability: "账号环境变量名",
        });
    }
    let trusted = (face.trust_in)(fixture_home, FIXTURE_CWD)
        .ok()
        .and_then(|line| serde_json::from_str::<serde_json::Value>(&line).ok())
        .is_some_and(|v| v["trusted"] == true);
    if !trusted {
        return Err(Stop::MissingCapability {
            stage: STAGES[4],
            capability: "账号信任判定",
        });
    }
    done.push(STAGES[4]);

    // ── ⑥ resume：默认命令 + 命令形 + 会话名前缀 —— **真的走通用层**（resume 规格 → 注册表里这一家那一格）。
    let base = caps.default_command.ok_or(Stop::MissingCapability {
        stage: STAGES[5],
        capability: "resume 默认命令",
    })?;
    let cmd = caps.resume_command.ok_or(Stop::MissingCapability {
        stage: STAGES[5],
        capability: "resume 命令形",
    })?;
    let prefix = caps.session_name_prefix.ok_or(Stop::MissingCapability {
        stage: STAGES[5],
        capability: "resume 会话名前缀",
    })?;
    if base.trim().is_empty() || prefix.trim().is_empty() || !cmd(base, sid).contains(sid) {
        return Err(Stop::MissingCapability {
            stage: STAGES[5],
            capability: "resume 命令形",
        });
    }
    let registry = [crate::agents::Adapter {
        launch: Some(crate::agents::LaunchFace {
            default_launcher: base,
            resume_command: cmd,
            session_name_prefix: prefix,
            ..LAUNCH
        }),
        ..bare_row(home_fn)
    }];
    let spec = format!("{{\"agentKind\":\"{AGENT_KIND}\",\"sessionId\":\"{sid}\"}}");
    let plan =
        crate::control::resolve_query::resolve_json_among(&registry, &spec).map_err(|_| {
            Stop::MissingCapability {
                stage: STAGES[5],
                capability: "resume 命令形",
            }
        })?;
    let head: String = sid.chars().take(8).collect();
    if plan["command"] != serde_json::json!(cmd(base, sid))
        || plan["sessionName"] != serde_json::json!(format!("{prefix}-{head}"))
    {
        return Err(Stop::MissingCapability {
            stage: STAGES[5],
            capability: "resume 会话名前缀",
        });
    }
    done.push(STAGES[5]);

    Ok(done)
}

/// 夹具里那条会话的 id。**只出现在夹具里**，不是任何真实会话。
pub(crate) const FIXTURE_SESSION_ID: &str = "00000000-0000-4000-8000-0000000000f6";
/// 夹具里那条会话的 cwd。
pub(crate) const FIXTURE_CWD: &str = "/home/u/proj";

#[cfg(test)]
#[path = "../../../../tests/backend/agents/fake_tests.rs"]
mod tests;
