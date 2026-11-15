//! 额度那几格：`DefaultUpstream` 里给额度账与换号的格，入口一律按路由名。

use super::*;

/// 一件流事件里说出的这一发用量：折合输入 token（各类 token 按那一家的价比折成输入 token 计）；
/// `prompt` 有 ＝ 这一件是一发应答的开头（数「几发」用），带这一发的上下文。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spent {
    pub units: f64,
    pub prompt: Option<Prompt>,
}

/// 一发的上下文：`tokens` ＝ 输入 ＋ 写缓存 ＋ 读缓存（原数，不折）；`tier` ＝ 这一发写缓存用的那一档（只读不写 ⇒ 说不出）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prompt {
    pub tokens: u64,
    pub tier: Option<Tier>,
}

/// 写缓存的一档：缓存留几秒 · 写比读多几倍（折合输入 token 的倍数之差）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tier {
    pub ttl: u64,
    pub extra: f64,
}

/// 一份「用满」回包：状态行里状态码那一截 · 头 · 体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitReply {
    pub status: &'static str,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// （几点重置 unix 秒, 此刻 unix 秒, 卡着的那个窗口的窗口键 `5h` · `7d` · `7d:<模型>`，说不出 ⇒ `None`）→ 这一家的「用满」回包。
pub(crate) type LimitReplyOf = fn(u64, u64, Option<&str>) -> LimitReply;

/// 一家「让官方客户端报用量」那一形：起哪个程序 · 带什么参数 · 号的配置目录交给哪一格环境 · 起它时摘 / 设哪几格 ·
/// 起来后往 stdin 喂哪几行 · 读到哪一行就够 · 输出怎么读。
/// 通用层（帧命令 `quota-probe`）只照它起、照它喂、照它读，读成的窗口记进额度账、标来源 `usage`。
#[derive(Debug, Clone, Copy)]
pub struct UsageFace {
    pub program: &'static str,
    pub args: &'static [&'static str],
    /// 号的配置目录交给这一格；账号 0（默认配置目录）⇒ 摘掉它。
    pub dir_env: &'static str,
    /// 起它时摘掉的（会盖掉那个号自己的登录的那几格）。
    pub env_remove: &'static [&'static str],
    /// 起它时设上的（如让输出里的时刻按 UTC 写）。
    pub env_set: &'static [(&'static str, &'static str)],
    /// 起来后依次喂进它 stdin 的那几行（每行自带换行）。空 ⇒ 不喂、stdin 照旧关着。
    pub stdin: &'static [&'static str],
    /// 标准输出读到哪一行就够了：够了就收掉进程、不等它自己退（读到的那几行连这一行交给 `read`）。`None` ⇒ 等它退。
    pub enough: Option<fn(&str) -> bool>,
    /// （标准输出, 此刻 unix 秒）→ 窗口；读不懂 ⇒ `Err(哪一处读不懂)`（不猜）。
    pub read: fn(&str, u64) -> Result<Vec<QuotaWindow>, String>,
}

/// 一家「开窗那一句」那一形：通用层（`accounts::quota::warm`）让本二进制当 ccm 跑 `ccm <args> -- --account <号> …`，
/// `args` 是交给那一家程序的参数（模型 · 关掉工具与 MCP · 不落会话记录 · 换系统提示 · 那一句），只住适配层。
#[derive(Debug, Clone, Copy)]
pub struct WarmFace {
    pub args: &'static [&'static str],
    /// 起它时设上的（关掉思考 · 不发非必要的请求）。
    pub env_set: &'static [(&'static str, &'static str)],
}

/// 路由第 1 段 → 那一家的「开窗那一句」那一形。
pub fn warm_of(route: RouteId) -> Option<WarmFace> {
    upstream_of(route).and_then(|u| u.warm)
}

/// 路由第 1 段 → 那一家的「报用量」那一形。
pub fn usage_of(route: RouteId) -> Option<UsageFace> {
    upstream_of(route).and_then(|u| u.usage)
}

/// 路由第 1 段 → 那一家的额度窗口语义位读法。
pub fn window_slot_of(route: RouteId) -> Option<fn(&str) -> Option<&'static str>> {
    upstream_of(route).and_then(|u| u.window_slot)
}

/// 路由第 1 段 → 那一家的额度窗口键读法。
pub fn window_key_of(route: RouteId) -> Option<fn(&str) -> Option<String>> {
    upstream_of(route).and_then(|u| u.window_key)
}

/// 路由第 1 段 → 那一家的订阅号登录格式。
pub fn login_of(route: RouteId) -> Option<LoginFace> {
    upstream_of(route).and_then(|u| u.login)
}

/// 一家的额度读法（见 [`DefaultUpstream::quota`]）。
pub(crate) type QuotaRead = fn(u16, &[(String, String)], u64) -> Option<QuotaReading>;

/// 那一家（kind）⇒ 它上游的流里怎么读出每发用量（[`StreamFace::spent`]）。账上记的路由名先经 [`kind_of_route`] 换。
pub fn spent_of(kind: &AgentKind) -> Option<fn(&str) -> Option<Spent>> {
    adapter_among(REGISTRY, kind.as_str())
        .and_then(|a| a.upstream.as_ref())
        .and_then(|u| u.stream?.spent)
}

/// 路由第 1 段 → 那一家的额度读法，读出来的被拒与否已经过 [`judged`]（各家不另判）。
pub fn quota_read_of(
    route: RouteId,
) -> Option<impl Fn(u16, &[(String, String)], u64) -> Option<QuotaReading>> {
    let read = upstream_of(route).and_then(|u| u.quota)?;
    Some(move |status: u16, headers: &[(String, String)], now: u64| {
        judged(read, status, headers, now)
    })
}

/// 限流器对一发的结论（各家的值翻成这三档）：线上形住 `cmd-wire`（`quota-read` 带它）。
pub use cmd_wire::quota::QuotaStatus;

/// 一个额度窗口此刻的样子。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    /// 窗口名（那一家读法表里的值，如 5 小时 · 7 天）。
    pub name: String,
    /// 已用比例（通常 0–1，可以超过 1）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub used: Option<f64>,
    /// 几点重置（unix 秒）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub resets_at: Option<u64>,
    /// 越过了哪条预警线（比例）；没越过就没有。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub warned_at: Option<f64>,
}

/// 超额（付费额外用量）那一档。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaOverage {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub status: Option<QuotaStatus>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub resets_at: Option<u64>,
    /// 为什么不可用（那一家的原值）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub disabled: Option<String>,
    /// 这一发正在用超额。
    pub in_use: bool,
}

/// 一个回包读出来的额度快照（通用形状，账号域只认它）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaReading {
    /// 限流器的结论；回包里没说 ⇒ `None`。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub status: Option<QuotaStatus>,
    /// 这一发被上游拒了（没被服务）。
    pub refused: bool,
    /// 此刻卡着的那个窗口名。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub limiting: Option<String>,
    /// 卡着的那个窗口几点重置（被拒时：到这一刻之前这个号用不了）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub resets_at: Option<u64>,
    pub windows: Vec<QuotaWindow>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub overage: Option<QuotaOverage>,
}

/// 按名取回包头（不分大小写；两端空白去掉，空值当没有）：各家的读法都调它，只这一份。
pub fn header<'h>(headers: &'h [(String, String)], name: &str) -> Option<&'h str> {
    // 扫的是这一个回包的头，不是注册表：头名相同的第一条。
    for (k, v) in headers {
        if k.eq_ignore_ascii_case(name) {
            let v = v.trim();
            return (!v.is_empty()).then_some(v);
        }
    }
    None
}

/// 被拒却说不出几点能再用（那一窗没说重置、也没有 `retry-after`）⇒ 当它这么多秒之后回来。各家不另设。
pub const REFUSED_BRIEFLY: u64 = 60;

/// HTTP 通用的「请求太多」状态码（不是哪一家的；中转记日志那一处也照它）。
pub const TOO_MANY_REQUESTS: u16 = 429;

/// HTTP 通用的「几秒后再试」头（不是哪一家的）。
pub const RETRY_AFTER: &str = "retry-after";

/// ★ 回包算不算「被拒」的唯一判法：照这一家的读法 `read` 读出窗口，再判。
/// 429 而且有一窗用到 100、或这一家的读数说被拒（它点名的那一窗用量没到 100 也算）⇒ 被拒，到那几窗的重置里晚的那个；
/// 都没说重置 ⇒ 照 `retry-after`；也没有 ⇒ [`REFUSED_BRIEFLY`] 秒之后。429 而一窗都没满 ⇒ 短时限流：照常读数、不算被拒。
/// 读法自己写的 `refused` 不作数。
pub(crate) fn judged(
    read: QuotaRead,
    status: u16,
    headers: &[(String, String)],
    now: u64,
) -> Option<QuotaReading> {
    let mut r = read(status, headers, now)?;
    let full: Vec<&QuotaWindow> = r
        .windows
        .iter()
        .filter(|w| w.used.is_some_and(|u| u >= 1.0))
        .collect();
    let said = r.status == Some(QuotaStatus::Rejected);
    r.refused = status == TOO_MANY_REQUESTS && (said || !full.is_empty());
    if r.refused {
        let held = full
            .iter()
            .filter_map(|w| w.resets_at)
            .chain(said.then_some(r.resets_at).flatten())
            .max();
        r.resets_at = held.or_else(|| {
            let wait = header(headers, RETRY_AFTER)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|n| n.is_finite() && *n >= 0.0)
                .map_or(REFUSED_BRIEFLY, |n| n.round() as u64);
            Some(now.saturating_add(wait))
        });
    }
    Some(r)
}

#[cfg(test)]
#[path = "../../../tests/backend/agents/quota_face_tests.rs"]
mod tests;
