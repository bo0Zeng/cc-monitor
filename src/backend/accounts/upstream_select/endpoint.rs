//! **起会话那一发走哪、注入什么** —— 上游选择在帧面上出的两份成品。
//!
//! | 帧命令 | 答什么 | 谁问 |
//! |---|---|---|
//! | `launch-endpoint` | 这个号这一发往 `ANTHROPIC_BASE_URL` 里写哪个中转地址（或不写；地址不随会话变，不带会话段）· 这台的中转在不在 · 不在时拒还是直连 | monitor 起会话那一侧（本机与远端同一条：`history::relay_endpoint_on`） |
//! | `apikey-routing` | 这几个号在这台的表里有没有行 · 这台的中转在不在 | 界面经 `chan.call` 直接问（账号页徽章） |
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
    let agent = str_arg(args, "agent")?;
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
    let agent = str_arg(args, "agent")?;
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

fn str_arg<'a>(args: &'a Value, k: &str) -> Result<&'a str, (&'static str, String)> {
    args.get(k).and_then(Value::as_str).ok_or((
        "bad_args",
        copy_text("beUpstreamEndpoint.args.missingString", &[("k", k)]),
    ))
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
