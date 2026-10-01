//! **本机起会话**（新起 / resume / 接回）的计划与渲染 —— 原 monitor `history.rs` 那一整条，
//! 搬进本机后端出成品；monitor 只剩「开一个终端窗口跑这串」（`open_local_terminal`）。
//!
//! 一条命令串 = 中转前缀（这一发要不要经中转，[`crate::accounts::upstream_select::endpoint::launch_relay`]）
//! ＋ 身份前缀（`CCM_LAUNCH_ID`，交回调用方回填 sid）＋ 本体（POSIX：先试 `ccm` 容器路、渲不出来退旧路；Windows：PowerShell 旧路）。
//! 事实（平台 · `ccm` 装没装 · 中转答什么 · 目录在不在）由 [`Facts`] 给，[`plan`] 是纯的 —— 判据喂确定的事实驱动生产那一条。

use super::ccm_invocation as ci;
use super::payload;
use crate::platform::shell::{posix, powershell};
use copy_core::copy_text;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;

/// 进程环境里那一格的名字（读侧 `observe/accounts_query.rs` 的 `LAUNCH_ID_ENV`，`the_launch_id_var_is_one_name_on_both_halves` 钉两侧）。
pub(crate) const LAUNCH_ID_VAR: &str = "CCM_LAUNCH_ID";

/// `launch-local` 的入参。`agent` 那几格是当前 agent 的画像（界面那份 `AGENT_PROFILE` 的同一个来源）。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct LocalLaunchRequest {
    pub(crate) action: LocalAction,
    /// 只用来核「新起」那一格的目录在不在；终端的工作目录由 monitor 开窗口时给。
    #[serde(default)]
    pub(crate) cwd: Option<String>,
    /// F34 自定义启动命令（空 / 纯空白 = 没设）。
    #[serde(default)]
    pub(crate) launcher: Option<String>,
    /// 三态：缺席 = 调用方没表态（继承环境）· `base` = 显式账号 0（`unset`）· `named` = 具名账号。
    #[serde(default)]
    pub(crate) account: Option<LaunchAccount>,
    /// 要建进 tmux 时的会话名（界面铸名口铸的；这里不铸 —— 撞名避让只有一个家）。
    #[serde(default)]
    pub(crate) tmux_name: Option<String>,
    pub(crate) agent: AgentFacts,
    /// 全量注入开关（monitor 进程环境 `CCM_RELAY_ALL_SESSIONS`，「随入参交给那台后端」）。
    pub(crate) all_sessions: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum LocalAction {
    New,
    Resume {
        sid: String,
    },
    /// 接回一个已有的 tmux 会话（名字走 `tmuxName`）：不起 agent ⇒ 不要账号、中转、身份。
    Attach,
}

/// 「这次拉起用哪个号」—— 形状与界面 `LOCAL_LAUNCH_ACCOUNT_WIRE`（生成物）同一份。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum LaunchAccount {
    Base,
    Named {
        #[serde(rename = "configDir")]
        config_dir: String,
        /// `--account <名>` 说得出的那个名字；`None` ⇒ 只说得出目录 ⇒ 进不了 `ccm` 容器路（§35），退旧路。
        #[serde(default)]
        name: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct AgentFacts {
    /// 适配器 id（上游选择按它挑那一行）。
    pub(crate) id: String,
    pub(crate) default_launcher: String,
    /// shell 集成 wrapper（claude 是 `cc`）：探得到就用它，探不到回退 `default_launcher`。
    #[serde(default)]
    pub(crate) launcher_alias: Option<String>,
    pub(crate) resume_flag: String,
}

/// 这台 `ccm` 探出来的样子（`bash -lic` 里 `ccm -- --ccm-probe` 那一行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CcmSeen {
    pub(crate) installed: bool,
    pub(crate) caps: BTreeSet<String>,
}

/// [`plan`] 要的四样事实。生产那一份是 [`Facts::PRODUCTION`]。
#[derive(Clone, Copy)]
pub(crate) struct Facts {
    /// 本地终端是 PowerShell（Windows）还是 POSIX shell。
    pub(crate) windows: bool,
    pub(crate) probe_ccm: fn() -> CcmSeen,
    /// 这一发的中转地址（入参同帧命令 `launch-endpoint`）：`Ok(None)` 不注入 · `Err` 拒。
    pub(crate) relay: fn(&Value) -> Result<Option<String>, (&'static str, String)>,
    pub(crate) is_dir: fn(&str) -> bool,
}

impl Facts {
    pub(crate) const PRODUCTION: Facts = Facts {
        windows: crate::platform::shell::LOCAL_TERMINAL_IS_POWERSHELL,
        probe_ccm: probe_local_ccm,
        relay: crate::accounts::upstream_select::endpoint::launch_relay,
        is_dir: dir_exists,
    };
}

fn dir_exists(p: &str) -> bool {
    std::path::Path::new(p).is_dir()
}

/// 一次拉起的成品。`launch_id` = 铸进进程环境的那个身份 token（接回那一格不起 agent ⇒ `None`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Planned {
    pub(crate) cmd: String,
    pub(crate) launch_id: Option<String>,
}

static NO_TMUX_NAME: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsHistory.launch.noSessionName", &[]));
static OLD_PATH_CANNOT_ATTACH: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsHistory.launch.cannotAttach", &[]));
/// 中转那一格（`ANTHROPIC_BASE_URL`）容器路还说不出 ⇒ 退旧路（只进日志，不对外）。
const RELAY_KEEPS_THE_OLD_PATH: &str =
    "relay prefix present: the ccm container path cannot carry it yet";

/// 整条计划。拒 ⇒ `Err(带 REFUSE: 标的一句)`。
pub(crate) fn plan(req: &LocalLaunchRequest, facts: &Facts) -> Result<Planned, String> {
    if let LocalAction::Attach = req.action {
        if facts.windows {
            return Err(payload::refuse(copy_text("rsHistory.attach.windows", &[])));
        }
        let cmd = render_ccm(req, &req.agent, facts).map_err(payload::refuse)?;
        return Ok(Planned {
            cmd,
            launch_id: None,
        });
    }
    if let (LocalAction::New, Some(dir)) = (&req.action, req.cwd.as_deref()) {
        if !dir.is_empty() && !(facts.is_dir)(dir) {
            return Err(payload::refuse(copy_text(
                "rsHistory.newSession.noDir",
                &[("cwd", dir)],
            )));
        }
    }
    let relay = relay_prefix(req, facts)?;
    let base = if facts.windows {
        build_ps(req).map_err(payload::refuse)?
    } else {
        let rendered = if relay.is_empty() {
            render_ccm(req, &req.agent, facts)
        } else {
            Err(RELAY_KEEPS_THE_OLD_PATH.to_string())
        };
        match rendered {
            Ok(cmd) => cmd,
            Err(why) => {
                tracing::debug!("launch-local: ccm 容器路渲不出来 → 旧路：{why}");
                build_posix(req).map_err(payload::refuse)?
            }
        }
    };
    let token = identity_token(&req.action)?;
    let prefix = identity_prefix(&token, facts.windows);
    let cmd = relay + &prefix + &base;
    Ok(Planned {
        cmd,
        launch_id: Some(token),
    })
}

/// 中转前缀：问上游选择（与帧命令 `launch-endpoint` 同一个函数），按平台渲成一句。
fn relay_prefix(req: &LocalLaunchRequest, facts: &Facts) -> Result<String, String> {
    let args = serde_json::json!({
        "agent": req.agent.id,
        "account": account_wire(req.account.as_ref()),
        "allSessions": req.all_sessions,
    });
    let url = (facts.relay)(&args).map_err(|(_, said)| payload::refuse(said))?;
    Ok(match url {
        None => String::new(),
        Some(u) if facts.windows => payload::relay_env_prefix_ps(&u),
        Some(u) => payload::relay_env_prefix_posix(&u),
    })
}

fn account_wire(account: Option<&LaunchAccount>) -> Value {
    match account {
        None => Value::Null,
        Some(LaunchAccount::Base) => serde_json::json!({ "kind": "base" }),
        Some(LaunchAccount::Named { config_dir, name }) => {
            serde_json::json!({ "kind": "named", "configDir": config_dir, "name": name })
        }
    }
}

/// 身份 token：resume 用那个 sid（过得了段闸时），否则现铸一个（`payload::route_key_for_session` 同一条规则）。
fn identity_token(action: &LocalAction) -> Result<String, String> {
    let sid = match action {
        LocalAction::Resume { sid } => Some(sid.as_str()),
        LocalAction::New | LocalAction::Attach => None,
    };
    payload::route_key_for_session(sid).map_err(payload::refuse)
}

fn identity_prefix(token: &str, windows: bool) -> String {
    if windows {
        powershell::set_env(LAUNCH_ID_VAR, token)
    } else {
        posix::export(LAUNCH_ID_VAR, &shell_quote_core::posix_quote(token))
    }
}

// ─── 旧路：直接起拉起器（F34 自定义命令优先 → wrapper 别名探测 → 默认）───

/// F34：拼进 shell 之前过全仓那一张命令片段白名单（`shell_quote_core::launcher_refused_char`）。空 / 纯空白 = 没设。
fn checked_launcher(launcher: Option<&str>) -> Result<Option<String>, String> {
    let Some(l) = launcher.map(str::trim).filter(|l| !l.is_empty()) else {
        return Ok(None);
    };
    if let Some(c) = shell_quote_core::launcher_refused_char(l) {
        return Err(copy_text(
            "rsHistory.launcher.badChars",
            &[("launcher", &format!("{l:?}")), ("c", &format!("{c:?}"))],
        ));
    }
    Ok(Some(l.to_string()))
}

enum Choice {
    Fixed(String),
    Probe {
        alias: String,
        preferred: String,
        fallback: String,
    },
}

/// 用哪个命令（两种 shell 共用的那半决定）。旧路产不出「接回」：它只会拼一个拉起器，拿它表达接回会另起一条。
fn choice(req: &LocalLaunchRequest) -> Result<Choice, String> {
    let agent = &req.agent;
    let suffix = |bin: &str| -> Result<String, String> {
        match &req.action {
            LocalAction::Resume { sid } => Ok(format!("{bin} {} {sid}", agent.resume_flag)),
            LocalAction::New => Ok(bin.to_string()),
            LocalAction::Attach => Err(OLD_PATH_CANNOT_ATTACH.to_string()),
        }
    };
    if let LocalAction::Resume { sid } = &req.action {
        // 〔`INVARIANTS §47` ①〕sid 规则只有一份。
        if !shell_quote_core::session_id_ok(sid) {
            return Err(format!("refuse resume: invalid session_id {sid:?}"));
        }
    }
    if let Some(l) = checked_launcher(req.launcher.as_deref())? {
        return Ok(Choice::Fixed(suffix(&l)?));
    }
    let def = agent.default_launcher.as_str();
    Ok(match agent.launcher_alias.as_deref() {
        Some(alias) => Choice::Probe {
            alias: alias.to_string(),
            preferred: suffix(alias)?,
            fallback: suffix(def)?,
        },
        None => Choice::Fixed(suffix(def)?),
    })
}

/// POSIX 旧路。`command -v` 找得到 shell 函数与别名（命令跑在加载了 rc 的 shell 里）。
fn build_posix(req: &LocalLaunchRequest) -> Result<String, String> {
    let prefix = config_dir_prefix_posix(req.account.as_ref())?;
    Ok(prefix
        + &match choice(req)? {
            Choice::Fixed(cmd) => cmd,
            Choice::Probe {
                alias,
                preferred,
                fallback,
            } => posix::if_command(&alias, &preferred, &fallback),
        })
}

/// PowerShell 旧路（Windows 本机）。
fn build_ps(req: &LocalLaunchRequest) -> Result<String, String> {
    let prefix = config_dir_prefix_ps(req.account.as_ref())?;
    Ok(prefix
        + &match choice(req)? {
            Choice::Fixed(cmd) => cmd,
            Choice::Probe {
                alias,
                preferred,
                fallback,
            } => powershell::if_command(&alias, &preferred, &fallback),
        })
}

/// POSIX 账号前缀。缺席 ⇒ 空串（继承）；`base` ⇒ `unset`（不是「什么都不加」—— 那会被 rc 里的默认号顶掉）。
fn config_dir_prefix_posix(account: Option<&LaunchAccount>) -> Result<String, String> {
    match account {
        None => Ok(String::new()),
        Some(LaunchAccount::Base) => {
            payload::config_dir_prefix_posix(Some(&payload::Account::Base))
        }
        Some(LaunchAccount::Named { config_dir, .. }) => {
            let d = config_dir.trim();
            if d.is_empty() {
                return Err(copy_text("rsHistory.configDir.empty", &[]));
            }
            if !payload::config_dir_command_safe(d) {
                return Err(copy_text(
                    "rsHistory.configDir.invalid",
                    &[("dir", &format!("{d:?}"))],
                ));
            }
            payload::config_dir_prefix_posix(Some(&payload::Account::Named { config_dir: d }))
        }
    }
}

/// PowerShell 账号前缀。与 POSIX 那条的实质差别只在「什么算绝对路径」（盘符 / UNC / 两种分隔符）。
fn config_dir_prefix_ps(account: Option<&LaunchAccount>) -> Result<String, String> {
    match account {
        None => Ok(String::new()),
        Some(LaunchAccount::Base) => Ok(powershell::clear_env(payload::account_env())),
        Some(LaunchAccount::Named { config_dir, .. }) => {
            let d = config_dir.trim();
            if d.is_empty() {
                return Err(copy_text("rsHistory.configDir.empty", &[]));
            }
            validate_config_dir_ps(d)?;
            Ok(powershell::set_env(payload::account_env(), d))
        }
    }
}

fn validate_config_dir_ps(dir: &str) -> Result<(), String> {
    let b = dir.as_bytes();
    let drive = b.len() >= 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b[2] == b'\\' || b[2] == b'/');
    let absolute = dir.starts_with('/') || drive || dir.starts_with("\\\\");
    let dotdot = dir.contains("/../")
        || dir.ends_with("/..")
        || dir.contains("\\..\\")
        || dir.ends_with("\\..");
    if !absolute || dir == "/" || dotdot || dir.chars().any(payload::is_command_unsafe_char) {
        return Err(copy_text(
            "rsHistory.configDir.invalid",
            &[("dir", &format!("{dir:?}"))],
        ));
    }
    Ok(())
}

// ─── `ccm` 容器路（POSIX 本机）───

/// 探 → 渲。说不出容器名先拒（不必先付一次 `bash -lic`；名字只从界面铸名口来）。
fn render_ccm(
    req: &LocalLaunchRequest,
    agent: &AgentFacts,
    facts: &Facts,
) -> Result<String, String> {
    if req.tmux_name.as_deref().is_none_or(str::is_empty) {
        return Err(NO_TMUX_NAME.to_string());
    }
    let seen = (facts.probe_ccm)();
    render_ccm_with(req, agent, &seen)
}

/// 上一条的纯函数半 —— `ccm` 装没装、有哪些能力由调用方给。
fn render_ccm_with(
    req: &LocalLaunchRequest,
    agent: &AgentFacts,
    seen: &CcmSeen,
) -> Result<String, String> {
    let Some(name) = req.tmux_name.as_deref().filter(|n| !n.is_empty()) else {
        return Err(NO_TMUX_NAME.to_string());
    };
    let sanitized = checked_launcher(req.launcher.as_deref())?;
    let act = match &req.action {
        LocalAction::Resume { sid } => ci::Action::Resume { sid },
        LocalAction::New => ci::Action::New,
        LocalAction::Attach => ci::Action::Attach { name },
    };
    // 账号三态逐态对：`base` ⇒ `--base`；具名且说得出名字 ⇒ `--account <名>`；只说得出目录 ⇒ §35 短路（退旧路）；
    // 缺席 ⇒ 继承（绝不映射成 `--base`：那是「显式不注入」，与继承不是一回事）。
    let account = match req.account.as_ref() {
        Some(LaunchAccount::Base) => ci::CliAccount::Base,
        Some(LaunchAccount::Named { name, .. }) => ci::CliAccount::Named {
            name: name.as_deref(),
        },
        None => ci::CliAccount::Inherit,
    };
    let spec = ci::CliSpec {
        is_ssh: false,
        local_posix: true,
        action: act,
        container: ci::Container::Tmux {
            name,
            send_into: false,
        },
        cwd: None,
        account,
        ccm_sid: match &req.action {
            LocalAction::Resume { sid } => Some(sid.as_str()),
            LocalAction::New | LocalAction::Attach => None,
        },
        model: None,
        launcher: sanitized.as_deref().unwrap_or(&agent.default_launcher),
        default_launcher: &agent.default_launcher,
        args: &[],
        ccm_path: "ccm",
    };
    ci::render_ccm_invocation(&spec, &seen.caps, seen.installed).map_err(|r| r.reason())
}

/// 探这台的 `ccm`：在加载了 rc 的 `bash` 里问 PATH 上那个 `ccm` 会哪些。期限交给子进程（`timeout` 前缀，零定时器）。
/// 没有 `bash` / 起不来 / 超时 / 答非所问 ⇒ 按没装办（诚实降级到旧路，不是安全边界）。
fn probe_local_ccm() -> CcmSeen {
    let cmd = format!(
        "{} && ccm -- --ccm-probe || printf 'NO_CCM\\n'",
        posix::has_command("ccm")
    );
    const DEADLINE_SECS: u64 = 5;
    let not_installed = CcmSeen {
        installed: false,
        caps: BTreeSet::new(),
    };
    let Ok(bash) = crate::plugin::discover::find("bash", &[], true, "") else {
        return not_installed;
    };
    match crate::plugin::invoke::run(&bash, &["-lic", &cmd], DEADLINE_SECS, &[]) {
        Ok(done) if done.code == Some(0) => parse_probe(&String::from_utf8_lossy(&done.stdout)),
        _ => not_installed,
    }
}

/// `--ccm-probe` 那几行（`control/ccm/mod.rs::probe_output_for` 写的形状）⇒ 装没装 ＋ 能力集。首行不是 `name=ccm` ⇒ 没装。
fn parse_probe(out: &str) -> CcmSeen {
    let mut lines = out.lines();
    if lines.next() != Some("name=ccm") {
        return CcmSeen {
            installed: false,
            caps: BTreeSet::new(),
        };
    }
    let caps = lines
        .find_map(|l| l.strip_prefix("capabilities="))
        .map(|c| {
            c.split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    CcmSeen {
        installed: true,
        caps,
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/launch_render/local_tests.rs"]
mod tests;
