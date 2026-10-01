//! **起会话那一发走哪、注入什么** —— 上游选择出的两份成品。
//!
//! | 口 | 答什么 | 谁问 |
//! |---|---|---|
//! | [`relay_for_exec`] | 这个号这一发往 `ANTHROPIC_BASE_URL` 里写哪个中转地址（或不写；地址不随会话变，不带会话段）· 不在时拒还是直连 | `ccm` 在最终 exec 那一处（`control/ccm/plan.rs`）；别名预览走 [`relay_for_preview`] |
//! | `apikey-routing` | 这几个号在这台的表里有没有行 · 这台的中转在不在 | 界面经 `chan.call` 直接问（账号页徽章） |
//!
//! 起会话只有 `ccm` 一处：环境、中转地址由那台机器上的 `ccm` 自己定，界面只交一行 `ccm …`。
//!
//! # 人群只有一份：[`super::file_face::rows_at`]
//!
//! 「表里有哪几行」= `table::build` 真收进表的那几行（与中转装表同一个函数）。`accounts-list` 并表、
//! `apikey-routing`、`ccm` 起会话三处读的都是它。
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
//! ⚠ ① 与 ⑥ 的降级**刻意不同**（`whenDown`）—— 「把这两种降级写成一样是最容易犯的错」。

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

/// 决策表的结局（不含「中转在不在」—— 那是另一件事实，[`relay_with`] 另探）。
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
    port: u16,
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
        return match base_url(port, RouteMode::Substitute, agent, id) {
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
    match base_url(port, RouteMode::Passthrough, agent, label) {
        Some(url) => Endpoint::Inject {
            url,
            when_down: WhenDown::Direct,
            account: None,
        },
        None => Endpoint::None,
    }
}

/// 全量注入开关的环境变量（`/t/` 那几格）：默认开，`=0` 才关。读的是**起 agent 那个进程**（`ccm`）自己的环境。
pub(crate) const ALL_SESSIONS_ENV: &str = "CCM_RELAY_ALL_SESSIONS";

/// [`ALL_SESSIONS_ENV`] 的值 ⇒ 开没开（缺席 / 其余任何值 ⇒ 开）。
pub(crate) fn all_sessions_on(get: &dyn Fn(&str) -> Option<String>) -> bool {
    get(ALL_SESSIONS_ENV).map_or(true, |v| v != "0")
}

/// 同机中转住的口：常驻后端被交的那个（`crate::relay::ENV_PORT`）；没交 / 认不出 ⇒ 默认口。
pub(crate) fn relay_port(get: &dyn Fn(&str) -> Option<String>) -> u16 {
    get(crate::relay::ENV_PORT)
        .and_then(|v| v.trim().parse::<u16>().ok())
        .filter(|p| *p != 0)
        .unwrap_or(PORT)
}

/// **ccm 在最终 exec 那一处问的那一句**：这一发往 `ANTHROPIC_BASE_URL` 里写哪个地址（不带钥匙；`None` = 不注入）。
/// ccm 是一次性进程，中转住同机的常驻后端里 ⇒「在不在」= 这台家目录下的钥匙读得到，且回环口连得上（连不上立刻被拒，不等）。
/// 钥匙那一格先判：口上的是别人（同机另一个用户）的中转时，这个用户没有那一把，当它不在。
pub(crate) fn relay_for_exec(
    agent: &str,
    account: &LaunchAccount,
) -> Result<Option<String>, String> {
    let get = |k: &str| std::env::var(k).ok();
    relay_with(
        agent,
        account,
        all_sessions_on(&get),
        relay_port(&get),
        &super::file_face::machine_rows(),
        &|port| {
            relay_key(&get).is_some() && std::net::TcpStream::connect(("127.0.0.1", port)).is_ok()
        },
    )
}

/// 这台机器上中转钥匙文件里那一把（家目录底下 `relay_route_core::KEY_FILE_REL`）。不在 / 形状不对 ⇒ `None`。只读。
pub(crate) fn relay_key(get: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let home = crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into))?;
    let raw = std::fs::read_to_string(home.join(relay_route_core::KEY_FILE_REL)).ok()?;
    let k = raw.trim();
    relay_route_core::key_shape_ok(k).then(|| k.to_string())
}

/// 同上，答的是常驻后端自己（别名预览 `ccm-print`）：中转就在本进程里，读本进程的监听状态。
pub(crate) fn relay_for_preview(
    agent: &str,
    account: &LaunchAccount,
) -> Result<Option<String>, String> {
    let get = |k: &str| std::env::var(k).ok();
    relay_with(
        agent,
        account,
        all_sessions_on(&get),
        relay_port(&get),
        &super::file_face::machine_rows(),
        &crate::relay::our_relay_listening,
    )
}

/// 上两条的本体：人群与「中转在不在」注入（判据喂夹具，不碰真家目录、不连真口）。
/// 非它不可（API 号代入 `/s/`）而没在听 ⇒ `Err(那一句)`；有它更好（`/t/`）而没在听 ⇒ 这一发直连（`None`）。
pub(crate) fn relay_with(
    agent: &str,
    account: &LaunchAccount,
    all_sessions: bool,
    port: u16,
    routed: &[String],
    listening: &dyn Fn(u16) -> bool,
) -> Result<Option<String>, String> {
    let registered = super::Upstreams::from_env(&|k| std::env::var(k).ok())
        .is_some_and(|u| u.of(agent).is_some());
    match decide_launch(agent, account, all_sessions, port, routed, registered) {
        Endpoint::None => Ok(None),
        Endpoint::Inject { url, .. } if listening(port) => Ok(Some(url)),
        Endpoint::Inject {
            when_down: WhenDown::Refuse,
            account,
            ..
        } => Err(copy_text(
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
        )),
        Endpoint::Inject {
            when_down: WhenDown::Direct,
            ..
        } => Ok(None),
    }
}

/// 一条不带钥匙的中转地址 ＋ 钥匙 ⇒ agent 进程环境里那一形（`http://127.0.0.1:<口>/<钥匙>/<前缀>/…`）。
/// 拆不开（不是构造口的产物）⇒ `None`。
pub(crate) fn keyed_base_url(base_url: &str, key: &str) -> Option<String> {
    let (head, tail) = base_url_halves(base_url)?;
    Some(format!("{head}{key}{tail}"))
}

/// 中转地址拆成「`http://主机:口/`」与「`/s/…` 那一截」两半（钥匙段插在中间）。不是构造口的产物 ⇒ `None`。
pub(crate) fn base_url_halves(base_url: &str) -> Option<(&str, &str)> {
    if !relay_route_core::base_url_shape_ok(base_url) {
        return None;
    }
    let rest = base_url.strip_prefix("http://")?;
    let at = "http://".len() + rest.find('/')?;
    Some((&base_url[..=at], &base_url[at..]))
}

/// `apikey-routing`：入参 `{agent, configDirs}` → `{routed, running}`（界面账号页那两格事实）。
///
/// - `routed`：传进来的那些 configDir 里，这台表里**有对应行**的那几个（原样回，规则住 `acct-core`）。
///   ⚠ 它答「表里有这一行」，不答「那把 key 能不能用」。
/// - `running`：这个进程里**我们的**中转在不在听（读宿主自己的监听状态 `relay::our_relay_listening`，与别名预览同一个判准）。
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

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/upstream_select/endpoint_tests.rs"]
mod tests;
