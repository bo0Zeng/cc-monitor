//! **起会话那一发走哪、注入什么** —— 上游选择在帧面上出的两份成品。
//!
//! | 帧命令 | 答什么 | 谁问 |
//! |---|---|---|
//! | `launch-endpoint` | 这个号这一发往 `ANTHROPIC_BASE_URL` 里写哪个中转地址（或不写；地址不随会话变，不带会话段）· 这台的中转在不在 · 不在时拒还是直连 | monitor 起会话那一侧（本机与远端同一条：`history::relay_endpoint_on`） |
//! | `apikey-routing` | 这几个号在这台的表里有没有行 · 这台的中转在不在 | 界面经 `chan.call` 直接问（账号页徽章） |
//! | `relay-optin` | 直接敲的那一家也走中转：这台那份用户级设置文件里写没写、对不对 ＋ 要贴的那一段（后端只读、不写那份文件） | 界面经 `chan.call` 直接问（机器页「终端」栏） |
//!
//! # 为什么搬到这里（必须拆 1）
//!
//! 先前那张「注入什么」的表住 monitor（`payload::relay_endpoint_for`），它要的两样事实
//! （表里有哪几行 · 中转在不在）monitor 自己读凭据文件、自己连回环口去拿 —— 人群与后端装表那一步**各算一份**
//! （头注自认的残留：`base_url` 写坏的那一行，界面说「经本机中转」，中转 404）。今天两样事实都在这台后端手里，
//! 表也就搬来：monitor 只转交入参、执行成品（远端「不在就起、有界等」那一截要定时器，后端零定时器 ⇒ 留 monitor）。
//!
//! # 人群只有一份：[`super::file_face::rows_at`]
//!
//! 「表里有哪几行」= `table::build` 真收进表的那几行（与中转装表同一个函数）。`accounts-list` 并表、
//! `apikey-routing`、`launch-endpoint` 三处读的都是它。
//!
//! # 决策表（逐行搬来；F5 那一行是主会话 4D 新裁）
//!
//! | # | 情况 | 答 |
//! |---|---|---|
//! | ① | agent 是凭据文件那一家、named 账号推出的 id 在表里 | `/s/<agent>/<id>`，中转不在 ⇒ **拒**（非它不可） |
//! | ② | 全量注入开关关着（默认） | 不注入 |
//! | ③ | agent 没在适配层登记默认上游（codex） | 不注入（注进去每一发都被中转拒，对用户与起不来同形） |
//! | ④ | `/t/` 标签：named ⇒ id；账号 0 ⇒ [`BASE_ACCOUNT_SEGMENT`]；**没表态 ⇒ [`UNDECLARED_ACCOUNT_SEGMENT`]**（F5）；推不出 id ⇒ 不注入 | —— |
//! | ⑤ | 标签与表里某一行同名 | 不注入（`/t/` 有行那一格会把这条会话自己的鉴权头送去那一行的上游） |
//! | ⑥ | 其余 | `/t/<agent>/<标签>`，中转不在 ⇒ **直连**（有它更好） |
//!
//! 地址里没有会话段：会话 id 归 claude 自己，中转从它请求里自带的头认会话（`relay::Destinations::stream_label_headers`）。
//!
//! ⚠ ① 与 ⑥ 的降级**刻意不同**，而且从此写在线上（`whenDown`）—— 「把这两种降级写成一样是最容易犯的错」。

use super::CREDENTIALS_FILE_AGENT;
use crate::agents::{SettingsBaseUrl, SettingsEnvFace};
use copy_core::copy_text;
use relay_route_core::{base_url, RouteMode, PORT};
use serde_json::{json, Value};

/// 本族的应答：`data` 或 `(code, message)`。
pub(crate) type EndpointAnswer = Result<Value, (&'static str, String)>;

/// 账号 0（不注入 `CLAUDE_CONFIG_DIR` 那一档）在 `/t/` 路由里的账号段。只是标签（`/t/` 从不查它的 key）。
pub(crate) const BASE_ACCOUNT_SEGMENT: &str = "0";

/// 起会话时**没表态**是哪个号（没建账号库的机器上本机起会话就是这一形）在 `/t/` 里的账号段。
///
/// 不借账号 0 的 `0`：没表态 ≠ 账号 0（建了账号库的机器上，没表态时 pane 可能落到 shell rc 里导出的那个配置根上），
/// 流标签不该替它说是哪个号。与表里某一行同名的风险同 `0`，由决策表第 ⑤ 行挡。
pub(crate) const UNDECLARED_ACCOUNT_SEGMENT: &str = "_";

/// 起会话时说的是哪个号（线上 `account`：`{"kind":"named","configDir":…}` · `{"kind":"base"}` · 缺席 / `null`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LaunchAccount {
    /// 一个具名账号（`configDir` 的末段目录名就是表里的 id，规则住 `acct-core`）。
    Named { config_dir: String },
    /// 账号 0。
    Base,
    /// 调用方没表态。
    Undeclared,
}

/// 中转不在时怎么办。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WhenDown {
    /// 拒绝起会话（`/s/`：这个号只有经中转换上第三方 key 才发得出去）。
    Refuse,
    /// 照旧直连、不注入（`/t/`：过中转只为拿到流）。
    Direct,
}

/// 决策表的结局（不含「中转在不在」—— 那是另一件事实，[`answer_launch`] 另探）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Endpoint {
    /// 不注入。
    None,
    /// 注入这个地址。
    Inject {
        url: String,
        when_down: WhenDown,
        /// `/s/` 那一格的表 id（给调用方说拒绝理由时点名）；`/t/` 为 `None`。
        account: Option<String>,
    },
}

/// ★★ **决策表的唯一实现**（纯函数）。`routed` = 这台表里有哪几行（[`super::file_face::rows_at`]）；
/// `registered` = 这一家 agent 在适配层登记了默认上游没有。
pub(crate) fn decide_launch(
    agent: &str,
    account: &LaunchAccount,
    all_sessions: bool,
    routed: &[String],
    registered: bool,
) -> Endpoint {
    let has_row = |id: &str| agent == CREDENTIALS_FILE_AGENT && routed.iter().any(|r| r == id);
    let id = match account {
        LaunchAccount::Named { config_dir } => acct_core::apikey_account_id_of_dir(config_dir),
        LaunchAccount::Base | LaunchAccount::Undeclared => None,
    };
    // ①
    if let Some(id) = id.as_deref().filter(|id| has_row(id)) {
        return match base_url(PORT, RouteMode::Substitute, agent, id) {
            Some(url) => Endpoint::Inject {
                url,
                when_down: WhenDown::Refuse,
                account: Some(id.to_string()),
            },
            // 表里的行都过了段闸（装表那一步同一个谓词），agent 是凭据文件那一家 ⇒ 走不到；走到了也不拼一条会 404 的地址。
            None => Endpoint::None,
        };
    }
    // ② ③
    if !all_sessions || !registered {
        return Endpoint::None;
    }
    // ④
    let label = match account {
        LaunchAccount::Named { .. } => match id.as_deref() {
            Some(id) => id,
            None => return Endpoint::None,
        },
        LaunchAccount::Base => BASE_ACCOUNT_SEGMENT,
        LaunchAccount::Undeclared => UNDECLARED_ACCOUNT_SEGMENT,
    };
    // ⑤
    if has_row(label) {
        return Endpoint::None;
    }
    // ⑥（标签当不了路由段 ⇒ 拼不出 ⇒ 不注入：「有它更好」不为它拒绝起会话）
    match base_url(PORT, RouteMode::Passthrough, agent, label) {
        Some(url) => Endpoint::Inject {
            url,
            when_down: WhenDown::Direct,
            account: None,
        },
        None => Endpoint::None,
    }
}

/// `launch-endpoint`：入参 `{agent, account?, allSessions}` → 成品 `{baseUrl}`（`null` = 不注入）。
/// 「中转不在时拒还是直连」也在这里判完（原先 monitor 的 `relay_endpoint_on`〔散文墓碑〕 拿四格再判一遍）：
/// 非它不可（API 号代入）而没在听 ⇒ 码 `relay_down` ＋ 一句；有它更好（`/t/` 直通）而没在听 ⇒ 这一发直连。
pub(crate) fn answer_launch(args: &Value) -> EndpointAnswer {
    launch_relay(args).map(|u| json!({ "baseUrl": u }))
}

/// [`answer_launch`] 的成品本身（本机起会话 `control/launch_render/local.rs` 进程内直接问它，不绕帧）。
pub(crate) fn launch_relay(args: &Value) -> Result<Option<String>, (&'static str, String)> {
    launch_relay_with(
        args,
        &super::file_face::machine_rows(),
        &crate::relay::our_relay_listening,
    )
}

/// [`launch_relay`] 的本体：人群与「中转在不在」注入（判据喂夹具，不碰真家目录、不连真口）。
pub(crate) fn launch_relay_with(
    args: &Value,
    routed: &[String],
    listening: &dyn Fn(u16) -> bool,
) -> Result<Option<String>, (&'static str, String)> {
    let agent = agent_arg(args)?;
    let all_sessions = args.get("allSessions").and_then(Value::as_bool).ok_or((
        "bad_args",
        copy_text(
            "beUpstreamEndpoint.args.missingBool",
            &[("k", "allSessions")],
        ),
    ))?;
    let account = account_arg(args)?;
    let registered = super::Upstreams::from_env(&|k| std::env::var(k).ok())
        .is_some_and(|u| u.of(agent).is_some());
    match decide_launch(agent, &account, all_sessions, routed, registered) {
        Endpoint::None => Ok(None),
        Endpoint::Inject { url, .. } if listening(PORT) => Ok(Some(url)),
        Endpoint::Inject {
            when_down: WhenDown::Refuse,
            account,
            ..
        } => Err((
            "relay_down",
            copy_text(
                "rsHistory.relay.downRefused",
                &[
                    ("account", &format!("{account:?}")),
                    (
                        "where",
                        &copy_text("beUpstreamEndpoint.relay.thisMachine", &[]),
                    ),
                    (
                        "why",
                        &copy_text("beUpstreamEndpoint.relay.notListening", &[]),
                    ),
                ],
            ),
        )),
        Endpoint::Inject {
            when_down: WhenDown::Direct,
            ..
        } => {
            tracing::info!("中转没在听 ⇒ 这一发照旧直连（`/t/` 那一格是「有它更好」）");
            Ok(None)
        }
    }
}

/// `apikey-routing`：入参 `{agent, configDirs}` → `{routed, running}`（界面账号页那两格事实）。
///
/// - `routed`：传进来的那些 configDir 里，这台表里**有对应行**的那几个（原样回，规则住 `acct-core`）。
///   ⚠ 它答「表里有这一行」，不答「那把 key 能不能用」。
/// - `running`：这个进程里**我们的**中转在不在听（读宿主自己的监听状态 `relay::our_relay_listening`，与 `launch-endpoint` 同一个判准）。
pub(crate) fn answer_routing(args: &Value) -> EndpointAnswer {
    answer_routing_with(
        args,
        &super::file_face::machine_rows(),
        &crate::relay::our_relay_listening,
    )
}

/// [`answer_routing`] 的本体（同上，事实注入）。
pub(crate) fn answer_routing_with(
    args: &Value,
    routed: &[String],
    listening: &dyn Fn(u16) -> bool,
) -> EndpointAnswer {
    let agent = agent_arg(args)?;
    let dirs: Vec<String> = args
        .get("configDirs")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter()
                .map(|v| v.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
        })
        .ok_or((
            "bad_args",
            copy_text(
                "beUpstreamEndpoint.args.missingStrings",
                &[("k", "configDirs")],
            ),
        ))?;
    Ok(json!({
        "routed": acct_core::apikey_routed_subset(&dirs, routed, agent, CREDENTIALS_FILE_AGENT),
        "running": listening(PORT),
    }))
}

/// 读那份设置文件的上限（几 KB 的配置；超了按「读不了」说，不当没装）。
const OPTIN_SETTINGS_CAP_BYTES: u64 = 1 << 20;

/// 读那份设置文件：不在 ⇒ `Ok(None)`；不是普通文件 / 超上限 / 读不动 ⇒ `Err(为什么)`（降级成「读不了」并说出来，不当没装）。
fn read_settings(file: &std::path::Path) -> Result<Option<String>, String> {
    match std::fs::metadata(file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
        Ok(m) if !m.is_file() => Err(copy_text("beUpstreamEndpoint.optin.notFile", &[])),
        Ok(m) if m.len() > OPTIN_SETTINGS_CAP_BYTES => {
            let cap = OPTIN_SETTINGS_CAP_BYTES.to_string();
            let error = copy_text("beUpstreamEndpoint.optin.tooLarge", &[("cap", &cap)]);
            Err(error)
        }
        Ok(_) => std::fs::read_to_string(file)
            .map(Some)
            .map_err(|e| e.to_string()),
    }
}

/// 那份设置文件里的地址和现在该贴的那一条比，是哪一态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OptinState {
    /// 贴过，且等于现在那一条。
    Installed,
    /// 是我们那一形（回环 ＋ 钥匙段 ＋ 我们的路由），但不等于现在那一条（钥匙 / 端口 / 路由换了）。
    Stale,
    /// 没写。
    Absent,
    /// 写了别的上游地址（不是我们那一形）。
    Other,
    /// 读不了 / 读不懂 ⇒ 装没装说不清。
    Unreadable,
}

impl OptinState {
    fn name(self) -> &'static str {
        match self {
            OptinState::Installed => "installed",
            OptinState::Stale => "stale",
            OptinState::Absent => "absent",
            OptinState::Other => "other",
            OptinState::Unreadable => "unreadable",
        }
    }
}

/// 一条地址是不是我们那一形（带钥匙段的、或没带钥匙段的构造口产物）。
fn ours(url: &str) -> bool {
    relay_route_core::split_keyed_base_url(url).is_some()
        || relay_route_core::base_url_shape_ok(url)
}

/// ★ 判态（纯函数）：读出来的那一格 × 现在该贴的那一条（`None` ＝ 生成不了）。
pub(crate) fn optin_state(found: &SettingsBaseUrl, expected: Option<&str>) -> OptinState {
    match found {
        SettingsBaseUrl::Unreadable => OptinState::Unreadable,
        SettingsBaseUrl::Unset => OptinState::Absent,
        SettingsBaseUrl::Set(u) if Some(u.as_str()) == expected => OptinState::Installed,
        SettingsBaseUrl::Set(u) if ours(u) => OptinState::Stale,
        SettingsBaseUrl::Set(_) => OptinState::Other,
    }
}

/// `relay-optin`：入参 `{}` → `{state, note, missing, source, snippet, listening}`。
///
/// 该贴的那一条 ＝ 决策表里「没表态是哪个号」那一发（[`decide_launch`]，全量注入按「是」—— 贴这一段就是用户自己选了全量）插上这台的钥匙；
/// 已装 ⇒ 不再带那一段（钥匙只在要贴的时候才出这台）。只读：那份文件由用户自己合并，后端一个字节不写。
pub(crate) fn answer_optin(_args: &Value) -> EndpointAnswer {
    let home = crate::platform::paths::home_dir()
        .ok_or(("failed", copy_text("beUpstreamEndpoint.optin.noHome", &[])))?;
    let (agent, face) = crate::agents::settings_env_face()
        .ok_or(("failed", copy_text("beUpstreamEndpoint.optin.noAgent", &[])))?;
    let registered = super::Upstreams::from_env(&|k| std::env::var(k).ok())
        .is_some_and(|u| u.of(agent).is_some());
    Ok(optin_at(
        &home,
        agent,
        &face,
        &super::file_face::machine_rows(),
        registered,
        &crate::relay::our_relay_listening,
    ))
}

/// [`answer_optin`] 的本体（家目录 · 那一家 · 表 · 已登记 · 中转在不在都是参数，判据喂夹具）。
pub(crate) fn optin_at(
    home: &std::path::Path,
    agent: &str,
    face: &SettingsEnvFace,
    routed: &[String],
    registered: bool,
    listening: &dyn Fn(u16) -> bool,
) -> Value {
    let file = (face.file)(home);
    let (found, unread) = match read_settings(&file) {
        Ok(None) => (SettingsBaseUrl::Unset, None),
        Ok(Some(raw)) => ((face.base_url)(&raw), None),
        Err(error) => (SettingsBaseUrl::Unreadable, Some(error)),
    };
    let expected = match decide_launch(agent, &LaunchAccount::Undeclared, true, routed, registered)
    {
        Endpoint::Inject { url, .. } => Some(
            crate::relay::keyed_with_key_on_disk(home, &url)
                .ok_or(copy_text("beUpstreamEndpoint.optin.noKey", &[])),
        ),
        Endpoint::None => None,
    };
    let current = expected
        .as_ref()
        .and_then(|e| e.as_ref().ok())
        .map(String::as_str);
    let state = optin_state(&found, current);
    let snippet = (state != OptinState::Installed)
        .then_some(current)
        .flatten()
        .map(|u| (face.snippet)(u));
    // 两句各说各的：`note` 说那份文件为什么读不了；`missing` 说那一段为什么生成不了（已装 / 生成得了 ⇒ 空）。
    let note = match (&found, unread) {
        (SettingsBaseUrl::Unreadable, Some(why)) => {
            copy_text("beUpstreamEndpoint.optin.unreadable", &[("why", &why)])
        }
        (SettingsBaseUrl::Unreadable, None) => copy_text("beUpstreamEndpoint.optin.badShape", &[]),
        _ => String::new(),
    };
    let missing = match expected {
        _ if state == OptinState::Installed => String::new(),
        Some(Ok(_)) => String::new(),
        Some(Err(why)) => why,
        None => copy_text("beUpstreamEndpoint.optin.noRoute", &[]),
    };
    json!({
        "state": state.name(),
        "note": note,
        "missing": missing,
        "source": file.display().to_string(),
        "snippet": snippet,
        "listening": listening(PORT),
    })
}

/// 线上 `agent`（适配器 id）→ 那一家。缺席 / 不是串 ⇒ `bad_args`（入参形状，调用方没表态是哪一家就不猜）；
/// 空串 ⇒ 默认那一家；注册表里没有 ⇒ `bad_args`，那句话列出认得的几家（不当成「这一家没有表」静默往下走）。
/// 认法住 `agents::pick_adapter`。
pub(crate) fn agent_arg(args: &Value) -> Result<&'static str, (&'static str, String)> {
    let name = args.get("agent").and_then(Value::as_str).ok_or((
        "bad_args",
        copy_text("beUpstreamEndpoint.args.missingString", &[("k", "agent")]),
    ))?;
    crate::agents::pick_adapter(Some(name)).map_err(|say| ("bad_args", say))
}

/// 线上 `account` → [`LaunchAccount`]。认不出的形 ⇒ `bad_args`（不猜成「没表态」）。
fn account_arg(args: &Value) -> Result<LaunchAccount, (&'static str, String)> {
    match args.get("account") {
        None | Some(Value::Null) => Ok(LaunchAccount::Undeclared),
        Some(Value::Object(o)) => match o.get("kind").and_then(Value::as_str) {
            Some("base") => Ok(LaunchAccount::Base),
            Some("named") => o
                .get("configDir")
                .and_then(Value::as_str)
                .map(|d| LaunchAccount::Named {
                    config_dir: d.to_string(),
                })
                .ok_or_else(|| {
                    (
                        "bad_args",
                        copy_text(
                            "beUpstreamEndpoint.account.namedNoDir",
                            &[("field", "account")],
                        ),
                    )
                }),
            _ => Err((
                "bad_args",
                copy_text(
                    "beUpstreamEndpoint.account.badKind",
                    &[("field", "account")],
                ),
            )),
        },
        Some(_) => Err((
            "bad_args",
            copy_text(
                "beUpstreamEndpoint.account.badShape",
                &[("field", "account")],
            ),
        )),
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/upstream_select/endpoint_tests.rs"]
mod tests;
