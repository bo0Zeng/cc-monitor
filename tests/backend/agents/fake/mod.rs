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
//! | 账号信任判定 | `.claude.json` → `projects[cwd].hasTrustDialogAccepted` | `fake-config.json` → `trusted[cwd]` | `AccountsFace.trust` |
//! | 判活 cmdline | 含 `claude` / `node` | 含 `fakeagent` | `LocalFace.cmdline_may_be_agent` |
//! | 解析本机 home | `$CLAUDE_CONFIG_DIR` 否则 `$HOME/.claude`（**恒有值**） | `$CCM_FAKE_AGENT_HOME`，**没有默认** | `Adapter.home` · `LocalFace.home_at` |
//! | resume 默认命令 | `claude` | `fakeagent` | `LaunchFace.default_launcher` |
//! | resume 命令形 | `--resume`（flag） | `revive`（子命令） | `LaunchFace.resume_token` |
//! | 项目里的 skill 根 | `<项目>/.claude/skills` | `<项目>/.fake-agent/abilities` | `AssetFace.project_skills_root` |
//! | 各号的 MCP 配置 | `.claude.json` 顶层 `mcpServers`（JSON） | `fake-mcp.conf` 一行一个 `server <名> <定义>`（行式） | `AccountsFace.mcp` |
//!
//! 不同形是要紧的：照抄 Claude 的布局的话，通用层拿 Claude 的知识去解释它的 home 恰好也能读出东西，
//! 「走通」就证明不了通用层容得下第二种布局。
//!
//! # 走全流程
//!
//! [`walk`] 把假 agent 推过五段，每一段都经通用层：发现（注册表那一行的 home）·
//! 读会话 / 判活 / 账号（注册表里这一家那几格，经 `agents::*_among`）· resume（注册表里这一家的起会话事实，经 `agents::pick_kind_among`）。
//! 挖掉任一种能力（[`FakeCaps::without`]），那一格的面就拼不出来，流程停在点得出名字的那一段。

use crate::platform::fs::Ro;
use cap_core::{Cap, Over};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// 本假 agent 读自己那几份夹具文件（配置 · MCP · 派出记录）的上限：夹具只有几行，超了即拒收。
pub(crate) const FAKE_FILE_CAP: Cap = Cap {
    bytes: 1024 * 1024,
    over: Over::Reject,
};

/// 本假 agent 在 wire 上的 `agent_kind` 值。
///
/// ⚠ 本层不进 `agents::REGISTRY`
/// （`the_fixture_agent_never_ships` 钉住）。
// 子运行形状（与 Claude Code 每一格都不同形；通用层的运行判据拿它跑）。
pub mod runs;
// 回退菜单与它的一个终端（与 Claude 每一屏都不同形；通用层的回退那一串拿它跑）。
// 终端接上控制面口子的那一半（`rewind_io.rs`）挂在 `control/rewind.rs` 底下。
#[path = "rewind.rs"]
pub mod rewind;

pub const AGENT_KIND: &str = "fake";

// 各面的填法住各自一份（对着 `src/backend/agents/<面>_face.rs`）；本份只留那一行的拼法 · 能力表 · `walk`。
mod accounts;
mod ask;
mod assets;
// 两家替身的足迹面（足迹汇总认多家 · 按号申报的判据拿它跑）。
pub mod footprint;
mod launch;
mod local;
mod record;
// 报用量那一形（往 stdin 喂两行、读够就收）：通用层的报用量判据直接拿它跑。
pub mod upstream;

use accounts::accounts_face;
pub use accounts::{
    trust_of_config, ACCOUNT_DIR_ENV, LINE_TRUST_CELLS, MCP_CELLS, MCP_FILE, NO_PASTE_FACE,
    SHOW_CODE_FACE, TRUST_CELLS,
};
pub use ask::{ASK, ASK_PLUGIN, GATE, TABLE_FLAG};
pub use assets::{
    project_skills_root, second_project_skills_root, two_families, ASSETS, ASSETS_B,
    ASSETS_USER_SKILLS, MCP, MCP_NO_LOGIN, PLUGINS, USER_SKILLS,
};
pub use launch::{attach_rows, DEFAULT_COMMAND, LAUNCH, PLAIN_KIND, RESUME_TOKEN};
use local::local_face;
pub use local::{cmdline_may_be_agent, home, pidfile_root, HOME_ENV};
use record::records_face;
pub use record::{
    chain_fact, found_in_the_other, is_session_file, own_fork_calls, records_root,
    session_file_name,
};

// ─────────────────────────────────────────────────────────────────────────────
// 能力表 + 走全流程的 driver
// ─────────────────────────────────────────────────────────────────────────────

/// 各种能力的**名字**，与 `agent_locality_guard::tests::CAPABILITY_FACES`（每一种住注册表哪一格）逐条对账。
///
/// ⚠ 顺序即 [`FakeCaps`] 字段序，[`FakeCaps::without`] 按名字挖洞时靠它。
pub const CAPABILITIES: &[&str] = &[
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
];

/// 能力 6：读那份配置判这个目录信不信任。
pub type TrustOfConfig = fn(&Path, &str) -> Result<bool, String>;

/// 假 agent 交出来的一整套能力。每一项都是 `Option`：反向夹具要能「把某一种能力删掉」，而删掉之后流程必须在一个说得出话的地方停下来；
/// 用 `Option` 而不是手工注释掉一个函数，那个反向夹具才是常驻判据。
#[derive(Clone)]
pub struct FakeCaps {
    pub records_root: Option<fn(&Path) -> PathBuf>,
    pub is_session_file: Option<fn(&Path) -> bool>,
    pub session_file_name: Option<fn(&str) -> String>,
    pub pidfile_root: Option<fn(&Path) -> PathBuf>,
    pub account_dir_env: Option<&'static str>,
    pub trust_of_config: Option<TrustOfConfig>,
    pub cmdline_may_be_agent: Option<fn(&str) -> crate::agents::CmdlineSays>,
    pub home: Option<fn() -> Option<PathBuf>>,
    pub default_command: Option<&'static str>,
    pub resume_token: Option<&'static str>,
}

impl FakeCaps {
    /// 还剩几种能力在（[`without`](Self::without) 的自检用）。
    pub fn present(&self) -> usize {
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
            self.resume_token.is_some(),
        ]
        .iter()
        .filter(|b| **b)
        .count()
    }

    /// 完整的一套（正题用）。
    pub fn full() -> Self {
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
            resume_token: Some(RESUME_TOKEN),
        }
    }

    /// 挖掉**一种**能力（反向夹具用）。名字必须是 [`CAPABILITIES`] 里的一条 —— 写错就 panic，
    /// 不静默返回原样（静默的话反向夹具会**变成正题**再跑一遍，而它会绿）。
    pub fn without(&self, capability: &str) -> Self {
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
            "resume 命令形" => c.resume_token = None,
            other => unreachable!("`{other}` 在 CAPABILITIES 里却没有对应字段 —— 两处漂开了"),
        }
        c
    }
}

/// 全流程的 5 段。名字进错误信息，所以是常量而不是字面量。
pub const STAGES: &[&str] = &["发现", "读会话", "判活", "账号", "resume"];

/// 流程停下来的原因：点名是哪一段缺了哪一种能力（不许静默当成「这个 agent 没有会话」）。
#[derive(Debug, PartialEq, Eq)]
pub enum Stop {
    /// 假 agent 自己少了一种能力 —— 点名是**哪一段**缺了**哪一种**。
    MissingCapability {
        stage: &'static str,
        capability: &'static str,
    },
}

/// 只带 home 的一行（其余各面都没有）；各段在它上面补那一段要的面。
pub(crate) fn bare_row(home: fn() -> Option<PathBuf>) -> crate::agents::Adapter {
    crate::agents::Adapter {
        kind: AGENT_KIND,
        display_name: None,
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
        clear_request: None,
        local: None,
        live: crate::agents::LiveBy::Unknown,
        asks: None,
        attach: None,
    }
}

/// 一整行：本假 agent 的全部面（判据「同一批通用判据对三家各跑一遍」用）。
pub fn full_row(home: fn() -> Option<PathBuf>) -> crate::agents::Adapter {
    crate::agents::Adapter {
        display_name: Some("Fake"),
        account_env: Some(ACCOUNT_DIR_ENV),
        assets: Some(ASSETS),
        mcp: Some(MCP),
        records: Some(records_face(
            records_root,
            is_session_file,
            session_file_name,
        )),
        local: Some(local_face(pidfile_root, cmdline_may_be_agent)),
        live: crate::agents::LiveBy::Unknown,
        asks: None,
        attach: None,
        accounts: Some(accounts_face(ACCOUNT_DIR_ENV)),
        launch: Some(LAUNCH),
        ..bare_row(home)
    }
}

/// 另一家替身（判「两家各有号」那几条用）：本假 agent 的全部面，换一个 `kind`、叫法、号载体与 wrapper 名；`is_default` ⇒ 它是默认起的那一家
/// （清单里没写是哪一家的号归它）。两家同形、只差这几格：通用层若不按号的家取，两家的号就会串；叫法各家各一个（[`said_of`]），
/// 读错了注册表或串了家，说出来的名字就对不上。
pub fn family_row(
    kind: &'static str,
    account_env: &'static str,
    wrapper: &'static str,
    is_default: bool,
    home: fn() -> Option<PathBuf>,
) -> crate::agents::Adapter {
    crate::agents::Adapter {
        kind,
        display_name: Some(said_of(kind)),
        account_env: Some(account_env),
        accounts: Some(accounts_face(account_env)),
        launch: Some(crate::agents::LaunchFace {
            adapter_id: kind,
            launcher_alias: Some(wrapper),
            is_default,
            ..LAUNCH
        }),
        ..full_row(home)
    }
}

/// [`family_row`] 那一家行头上的叫法：`替身<kind>`（各家各一个，判据照它断言「各家认各家」）。
pub fn said_of(kind: &str) -> &'static str {
    Box::leak(format!("替身{kind}").into_boxed_str())
}

/// 不由我们起的一家（没有起会话面）：行头的叫法是 `said`（`None` ＝ 没声明），其余各面同 [`full_row`]。
pub fn unlaunched_row(kind: &'static str, said: Option<&'static str>) -> crate::agents::Adapter {
    crate::agents::Adapter {
        kind,
        display_name: said,
        launch: None,
        ..full_row(home)
    }
}

/// 账号域那一份信任判定（各号同步信任的本体，住适配层之上）：调用方交进来，`walk` 第④段经它读。
pub type TrustJudge = fn(&crate::agents::TrustCells, &serde_json::Value) -> BTreeSet<String>;

/// 把假 agent 推过全流程，每一段都经通用层（见模块头注）。成功 = 走完的段名。
pub fn walk(
    caps: &FakeCaps,
    fixture_home: &Path,
    trusted: TrustJudge,
) -> Result<Vec<&'static str>, Stop> {
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
    if !(adapter.home)().is_some_and(|h| h.is_dir()) {
        return Err(Stop::MissingCapability {
            stage: STAGES[0],
            capability: "解析本机 home",
        });
    }
    done.push(STAGES[0]);

    // ── ② 读会话：经通用层（注册表里这一家的记录树根 · 会话文件判定 · 新会话文件命名）。
    let root_fn = caps.records_root.ok_or(Stop::MissingCapability {
        stage: STAGES[1],
        capability: "会话记录根",
    })?;
    let is_session = caps.is_session_file.ok_or(Stop::MissingCapability {
        stage: STAGES[1],
        capability: "会话文件判定",
    })?;
    let file_name = caps.session_file_name.ok_or(Stop::MissingCapability {
        stage: STAGES[1],
        capability: "会话文件命名",
    })?;
    let reg = [crate::agents::Adapter {
        records: Some(records_face(root_fn, is_session, file_name)),
        ..bare_row(home_fn)
    }];
    let mut found: Vec<PathBuf> = Vec::new();
    let records_root = crate::agents::records_root_among(&reg, AGENT_KIND, fixture_home);
    if let Some(Ok(projects)) = records_root.map(Ro::read_dir) {
        let mut dirs: Vec<PathBuf> = projects.flatten().map(|e| e.path()).collect();
        dirs.sort();
        for d in dirs {
            if let Ok(files) = Ro::read_dir(&d) {
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
            stage: STAGES[1],
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
            stage: STAGES[1],
            capability: "会话文件命名",
        });
    }
    done.push(STAGES[1]);

    // ── ③ 判活：经通用层（注册表里这一家的 pidfile 目录 + cmdline 认法）。
    let pid_root = caps.pidfile_root.ok_or(Stop::MissingCapability {
        stage: STAGES[2],
        capability: "pidfile 目录",
    })?;
    let cmdline = caps.cmdline_may_be_agent.ok_or(Stop::MissingCapability {
        stage: STAGES[2],
        capability: "判活 cmdline",
    })?;
    let reg = [crate::agents::Adapter {
        local: Some(local_face(pid_root, cmdline)),
        live: crate::agents::LiveBy::Unknown,
        asks: None,
        attach: None,
        ..bare_row(home_fn)
    }];
    let local =
        crate::agents::local_face_among(&reg, AGENT_KIND).ok_or(Stop::MissingCapability {
            stage: STAGES[2],
            capability: "pidfile 目录",
        })?;
    let pidfiles = Ro::read_dir((local.pidfile_dir)(fixture_home))
        .map(|rd| rd.flatten().count())
        .unwrap_or(0);
    if pidfiles == 0 {
        return Err(Stop::MissingCapability {
            stage: STAGES[2],
            capability: "pidfile 目录",
        });
    }
    if (local.cmdline_may_be_agent)("fakeagent revive x") != crate::agents::CmdlineSays::Session
        || (local.cmdline_may_be_agent)("/usr/bin/vim") == crate::agents::CmdlineSays::Session
    {
        return Err(Stop::MissingCapability {
            stage: STAGES[2],
            capability: "判活 cmdline",
        });
    }
    done.push(STAGES[2]);

    // ── ④ 账号：经通用层（注册表里这一家账号库面的会话环境键 + 信任预检）。
    let env_name = caps.account_dir_env.ok_or(Stop::MissingCapability {
        stage: STAGES[3],
        capability: "账号环境变量名",
    })?;
    caps.trust_of_config.ok_or(Stop::MissingCapability {
        stage: STAGES[3],
        capability: "账号信任判定",
    })?;
    let reg = [crate::agents::Adapter {
        accounts: Some(accounts_face(env_name)),
        ..bare_row(home_fn)
    }];
    let face =
        crate::agents::accounts_face_among(&reg, AGENT_KIND).ok_or(Stop::MissingCapability {
            stage: STAGES[3],
            capability: "账号环境变量名",
        })?;
    if face.session_env.config_dir.trim().is_empty() {
        return Err(Stop::MissingCapability {
            stage: STAGES[3],
            capability: "账号环境变量名",
        });
    }
    // 信任经通用层读：那一家说的配置文件名 ＋ 信任那一格（`AccountsFace.trust`），本体是各号同步信任那一份。
    let trusted = face.trust.is_some_and(|cells| {
        Ro::read_capped(fixture_home.join(face.config_file), FAKE_FILE_CAP)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .is_some_and(|v| trusted(&cells, &v).contains(FIXTURE_CWD))
    });
    if !trusted {
        return Err(Stop::MissingCapability {
            stage: STAGES[3],
            capability: "账号信任判定",
        });
    }
    done.push(STAGES[3]);

    // ── ⑤ resume：默认命令 + 命令形 —— **真的走通用层**（按 kind 在注册表里认那一家，读它那一格起会话事实；
    //    `ccm` 起 resume 那一行就是照这一格拼的，见 `fake_tests.rs` 的 ccm 规划那几条）。
    let base = caps.default_command.ok_or(Stop::MissingCapability {
        stage: STAGES[4],
        capability: "resume 默认命令",
    })?;
    let token = caps.resume_token.ok_or(Stop::MissingCapability {
        stage: STAGES[4],
        capability: "resume 命令形",
    })?;
    if base.trim().is_empty() || token.trim().is_empty() {
        return Err(Stop::MissingCapability {
            stage: STAGES[4],
            capability: "resume 命令形",
        });
    }
    let registry = [crate::agents::Adapter {
        launch: Some(crate::agents::LaunchFace {
            default_launcher: base,
            resume_token: token,
            ..LAUNCH
        }),
        ..bare_row(home_fn)
    }];
    let (_, face) = crate::agents::pick_kind_among(&registry, Some(AGENT_KIND)).map_err(|_| {
        Stop::MissingCapability {
            stage: STAGES[4],
            capability: "resume 命令形",
        }
    })?;
    if face.default_launcher != base || face.resume_token != token {
        return Err(Stop::MissingCapability {
            stage: STAGES[4],
            capability: "resume 命令形",
        });
    }
    done.push(STAGES[4]);

    Ok(done)
}

/// 夹具里那条会话的 id。**只出现在夹具里**，不是任何真实会话。
pub const FIXTURE_SESSION_ID: &str = "00000000-0000-4000-8000-0000000000f6";
/// 夹具里那条会话的 cwd。
pub const FIXTURE_CWD: &str = "/home/u/proj";

// 走全流程的判据（`fake_tests.rs`）经 ccm_line · control · history · observe ⇒ 挂在主 crate（`cross/agents.rs`）。
