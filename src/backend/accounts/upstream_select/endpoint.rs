//! **起会话那一发走哪、注入什么** —— 上游选择出的两份成品。
//!
//! | 口 | 答什么 | 谁问 |
//! |---|---|---|
//! | [`relay_for_exec`] | 这个号这一发指到哪个中转地址（或不指；地址不随会话变，不带会话段；怎么交给那一家由它的注入格定）· 不在时拒还是直连 | `ccm` 在最终 exec 那一处（`control/ccm/plan.rs`）；别名预览走 [`relay_for_preview`] |
//! | `apikey-routing` | 这几个号在这台的表里有没有行 · 这台的中转在不在 | 界面经 `chan.call` 直接问（账号页徽章） |
//! | `relay-optin` | 直接敲的那一家（入参 `agent`）也走中转：这台那份用户级设置文件里写没写、对不对 ＋ 要贴的那一段（后端只读、不写那份文件） | 界面经 `chan.call` 直接问（机器页「终端」栏） |
//!
//! 起会话只有 `ccm` 一处：环境、中转地址由那台机器上的 `ccm` 自己定，界面只交一行 `ccm …`。
//!
//! # 人群只有一份：[`super::file_face::rows_at`]
//!
//! 「表里有哪几行」= `table::build` 真收进表的那几行（与中转装表同一个函数）。`accounts-list` 并表、
//! `apikey-routing`、`ccm` 起会话三处读的都是它。
//!
//! # 决策表
//!
//! | # | 情况 | 答 |
//! |---|---|---|
//! | ① | agent 是凭据文件那一家、named 账号推出的 id 在表里 | `/s/<agent>/<id>`，中转不在 ⇒ **拒**（非它不可） |
//! | ② | 全量注入开关关着（默认） | 不注入 |
//! | ③ | agent 没在适配层登记默认上游 | 不注入（注进去每一发都被中转拒，对用户与起不来同形） |
//! | ④ | `/t/` 标签：named ⇒ id；账号 0 ⇒ [`BASE_ACCOUNT_SEGMENT`]；没表态 ⇒ [`UNDECLARED_ACCOUNT_SEGMENT`]；推不出 id ⇒ 不注入 | —— |
//! | ⑤ | 标签与表里某一行同名 | 不注入（`/t/` 有行那一格会把这条会话自己的鉴权头送去那一行的上游） |
//! | ⑥ | 其余 | `/t/<agent>/<标签>`，中转不在 ⇒ **直连**（有它更好） |
//!
//! 地址里没有会话段：会话 id 归 claude 自己，中转从它请求里自带的头认会话（`relay::Destinations::stream_label_headers`）。
//!
//! ① 与 ⑥ 的降级刻意不同（`whenDown`）：① 非它不可，⑥ 有它更好。

use super::CREDENTIALS_FILE_AGENT;
use crate::agents::{SettingsBaseUrl, SettingsEnvFace, SettingsUnreadable};
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

/// **ccm 在最终 exec 那一处问的那一句**：这一发指到哪个中转地址（不带钥匙；`None` = 不注入）。
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
        &|port, url| {
            keyed_for_exec(url, key_kind_of(agent), &get).is_some()
                && std::net::TcpStream::connect(("127.0.0.1", port)).is_ok()
        },
    )
}

/// `ccm` exec 那一刻：认地址环境变量的那一家，插全权那一把（进 agent 进程环境）。
pub(crate) fn keyed_for_env(url: &str, get: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    keyed_for_exec(url, crate::relay::KeyKind::Full, get)
}

/// `ccm` exec 那一刻：地址拼进参数的那一家，插只许直通那一把（argv 同机别的用户读得到）。
pub(crate) fn keyed_for_args(url: &str, get: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    keyed_for_exec(url, crate::relay::KeyKind::Pass, get)
}

/// 非得经 shell 那一趟、地址拼进参数的那一家：照字面插进那个词的只许直通那一把（这台家目录下根钥匙派生的那一个）。只读。
pub(crate) fn pass_key_for_shell(home: &std::path::Path) -> Option<String> {
    crate::relay::pass_key_on_disk(home)
}

/// 一条不带钥匙的中转地址 ⇒ 插上这台盘上 `kind` 那把钥匙的那一形（agent 进程要的就是它）。
/// 插钥匙只经中转那一处（`relay::keyed_with_key_on_disk`）；没有家目录 / 钥匙不在 / 地址不是构造口的产物 ⇒ `None`。只读。
pub(crate) fn keyed_for_exec(
    url: &str,
    kind: crate::relay::KeyKind,
    get: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    let home = crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into))?;
    crate::relay::keyed_with_key_on_disk(&home, url, kind)
}

/// 路由名那一家的地址插哪一把钥匙：地址只能经命令行参数交给它的那一家（注入格 `Inject::Args`）⇒ 只许直通那一把
/// （argv 同机别的用户读得到、它也会写进自己的日志）；其余 ⇒ 全权那一把。起会话与「直接敲的也走中转」同一把，
/// 两处贴出来的地址一模一样。
pub(crate) fn key_kind_of(agent: &str) -> crate::relay::KeyKind {
    match crate::agents::inject_of_route(agent) {
        Some(crate::agents::Inject::Args(_)) => crate::relay::KeyKind::Pass,
        _ => crate::relay::KeyKind::Full,
    }
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
        &|port, _| crate::relay::our_relay_listening(port),
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
    listening: &dyn Fn(u16, &str) -> bool,
) -> Result<Option<String>, String> {
    let registered =
        super::Upstreams::from_env(&|k| std::env::var(k).ok()).is_some_and(|u| u.has(agent));
    match decide_launch(agent, account, all_sessions, port, routed, registered) {
        Endpoint::None => Ok(None),
        Endpoint::Inject { url, .. } if listening(port, &url) => Ok(Some(url)),
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
    pub(crate) fn name(self) -> &'static str {
        match self {
            OptinState::Installed => "installed",
            OptinState::Stale => "stale",
            OptinState::Absent => "absent",
            OptinState::Other => "other",
            OptinState::Unreadable => "unreadable",
        }
    }
}

/// 一条地址是不是我们那一形（带钥匙段的、或没带钥匙段的构造口产物）。读者：设置文件判态 · `ccm` 不继承别的号的中转地址。
pub(crate) fn ours(url: &str) -> bool {
    relay_route_core::split_keyed_base_url(url).is_some()
        || relay_route_core::base_url_shape_ok(url)
}

/// ★ 判态（纯函数）：读出来的那一格 × 现在该贴的那一条（`None` ＝ 生成不了）。
pub(crate) fn optin_state(found: &SettingsBaseUrl, expected: Option<&str>) -> OptinState {
    match found {
        SettingsBaseUrl::Unreadable(_) => OptinState::Unreadable,
        SettingsBaseUrl::Unset => OptinState::Absent,
        SettingsBaseUrl::Set(u) if Some(u.as_str()) == expected => OptinState::Installed,
        SettingsBaseUrl::Set(u) if ours(u) => OptinState::Stale,
        SettingsBaseUrl::Set(_) => OptinState::Other,
    }
}

/// `relay-optin`：入参 `{agent}`（适配器 id）→ 那一家的 `{state, note, missing, source, snippet, listening}`。
///
/// 该贴的那一条 ＝ 决策表里「没表态是哪个号」那一发（[`decide_launch`]，全量注入按「是」—— 贴这一段就是用户自己选了全量）插上这台的钥匙；
/// 已装 ⇒ 不再带那一段（钥匙只在要贴的时候才出这台）。只读：那份文件由用户自己合并，后端一个字节不写。
pub(crate) fn answer_optin(args: &Value) -> EndpointAnswer {
    let agent = agent_arg(args)?;
    let home = crate::platform::paths::home_dir()
        .ok_or(("failed", copy_text("beUpstreamEndpoint.optin.noHome", &[])))?;
    let face = crate::agents::settings_env_face(agent)
        .ok_or(("failed", copy_text("beUpstreamEndpoint.optin.noAgent", &[])))?;
    let registered =
        super::Upstreams::from_env(&|k| std::env::var(k).ok()).is_some_and(|u| u.has(agent));
    Ok(optin_at(
        &home,
        agent,
        &face,
        &super::file_face::machine_rows(),
        registered,
        &crate::relay::our_relay_listening,
    ))
}

/// 一家「直接敲的也走中转」此刻的样子（帧命令 `relay-optin` 与「要你动手」那一件同读这一份）。
pub(crate) struct OptinReport {
    pub(crate) state: OptinState,
    /// 那份文件为什么读不了（`unreadable` 才有，其余空串）。
    pub(crate) note: String,
    /// 那一段为什么生成不了；已装 / 生成得了 ⇒ 空串。
    pub(crate) missing: String,
    pub(crate) source: std::path::PathBuf,
    /// 要贴的那条地址（带钥匙）；已装或生成不了 ⇒ `None`。
    pub(crate) url: Option<String>,
}

/// 这台、这一家的 [`OptinReport`]（家目录 · 表 · 已登记都按这台现取）。没有家目录 / 那一家没有这一形 ⇒ 拒的那一句。
pub(crate) fn optin_report(agent: &str) -> Result<OptinReport, (&'static str, String)> {
    let home = crate::platform::paths::home_dir()
        .ok_or(("failed", copy_text("beUpstreamEndpoint.optin.noHome", &[])))?;
    let face = crate::agents::settings_env_face(agent)
        .ok_or(("failed", copy_text("beUpstreamEndpoint.optin.noAgent", &[])))?;
    let registered =
        super::Upstreams::from_env(&|k| std::env::var(k).ok()).is_some_and(|u| u.has(agent));
    Ok(optin_report_at(
        &home,
        agent,
        &face,
        &super::file_face::machine_rows(),
        registered,
    ))
}

/// [`optin_report`] 的本体（家目录 · 那一家 · 表 · 已登记都是参数，判据喂夹具）。
pub(crate) fn optin_report_at(
    home: &std::path::Path,
    agent: &str,
    face: &SettingsEnvFace,
    routed: &[String],
    registered: bool,
) -> OptinReport {
    let (file, found) = (face.read)(home);
    let expected = match decide_launch(
        agent,
        &LaunchAccount::Undeclared,
        true,
        PORT,
        routed,
        registered,
    ) {
        Endpoint::Inject { url, .. } => Some(
            crate::relay::keyed_with_key_on_disk(home, &url, key_kind_of(agent))
                .ok_or(copy_text("beUpstreamEndpoint.optin.noKey", &[])),
        ),
        Endpoint::None => None,
    };
    let current = expected
        .as_ref()
        .and_then(|e| e.as_ref().ok())
        .map(String::as_str);
    let state = optin_state(&found, current);
    let url = (state != OptinState::Installed)
        .then_some(current)
        .flatten()
        .map(str::to_string);
    // 两句各说各的：`note` 说那份文件为什么读不了；`missing` 说那一段为什么生成不了（已装 / 生成得了 ⇒ 空）。
    let note = match &found {
        SettingsBaseUrl::Unreadable(SettingsUnreadable::BadShape) => {
            copy_text("beUpstreamEndpoint.optin.badShape", &[])
        }
        SettingsBaseUrl::Unreadable(why) => {
            let why = match why {
                SettingsUnreadable::Io(e) => e.clone(),
                SettingsUnreadable::NotFile => copy_text("beUpstreamEndpoint.optin.notFile", &[]),
                SettingsUnreadable::TooLarge(cap) => copy_text(
                    "beUpstreamEndpoint.optin.tooLarge",
                    &[("cap", &cap.to_string())],
                ),
                SettingsUnreadable::BadShape => String::new(),
            };
            copy_text("beUpstreamEndpoint.optin.unreadable", &[("why", &why)])
        }
        _ => String::new(),
    };
    let missing = match expected {
        _ if state == OptinState::Installed => String::new(),
        Some(Ok(_)) => String::new(),
        Some(Err(why)) => why,
        None => copy_text("beUpstreamEndpoint.optin.noRoute", &[]),
    };
    OptinReport {
        state,
        note,
        missing,
        source: file,
        url,
    }
}

/// [`OptinReport`] ⇒ `relay-optin` 的成品（要贴的那一段按那一家的格式拼）。
pub(crate) fn optin_at(
    home: &std::path::Path,
    agent: &str,
    face: &SettingsEnvFace,
    routed: &[String],
    registered: bool,
    listening: &dyn Fn(u16) -> bool,
) -> Value {
    let r = optin_report_at(home, agent, face, routed, registered);
    json!({
        "state": r.state.name(),
        "note": r.note,
        "missing": r.missing,
        "source": r.source.display().to_string(),
        "snippet": r.url.as_deref().map(face.snippet),
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

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/upstream_select/endpoint_tests.rs"]
mod tests;
