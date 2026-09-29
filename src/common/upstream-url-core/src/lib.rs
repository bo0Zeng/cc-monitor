//! **上游 base URL 能不能用** —— 全仓唯一的一份（〔DUP3 · 主会话 09-26 裁 J9〕`设计/01 §5` D1 · `设计/90 §3` 判据 2）。
//!
//! 先前三份各管一截：写口 `creds-core` 的形状关 · 中转 `relay/upstream.rs::Base::parse` · 上游选择装表时的「明文只许回环」；
//! 界面还有一份用 `new URL()` 的（与后端漂开）。今天：[`parse`]（形状）＋ [`upstream_is_loopback`] ＝ [`usable`]；
//! 界面读 monitor 从这里现生成的式子（`src/frontend/ui/generated/judgment-rules.ts`），两侧由金样 `tests/__fixtures__/upstream-url.golden.json` 逐条对。
//! 本 crate 只判、不说：句子归调用处（同后端 `control/gate_rules.rs::TmuxNameIssue` 的先例）。
//!
//! 〔RE · 第四波 D 段〕**两个消费者**（`设计/00 §1.2` 共享 crate 那张表）：后端生产（上游选择装表 `accounts/upstream_select/table.rs` ·
//! 中转 `src/comms/outward/upstream.rs`）；monitor 只经生成物（`tests/frontend/shell/backend/control/payload_judgment_rules.rs`
//! 现生成 `judgment-rules.ts` 的 J9 那段），monitor 生产代码零引用（依赖在 `[dev-dependencies]`）。

/// 认得的两种协议与「走不走 TLS」。**闭集只有这一个住址**（报错与生成物都从这里派生）。
pub const SCHEMES: &[(&str, bool)] = &[("https", true), ("http", false)];

/// 逐字算回环的那个名字（大小写不敏感）。
pub const LOOPBACK_NAME: &str = "localhost";

/// 解析出来的上游基址。`path` 是路径前缀：带前导 `/`、不带尾随 `/`，`""` = 没有。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamUrl {
    pub tls: bool,
    pub host: String,
    pub port: u16,
    pub path: String,
}

/// 形状不对的那几形（检查顺序即这里的顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeIssue {
    Whitespace,
    NotUrl,
    BadScheme,
    NoHost,
    HasQuery,
    BadPort,
    DoubleSlash,
}

impl ShapeIssue {
    /// 线上 / 金样 / 生成物里的名字。
    pub fn code(self) -> &'static str {
        match self {
            ShapeIssue::Whitespace => "whitespace",
            ShapeIssue::NotUrl => "notUrl",
            ShapeIssue::BadScheme => "badScheme",
            ShapeIssue::NoHost => "noHost",
            ShapeIssue::HasQuery => "hasQuery",
            ShapeIssue::BadPort => "badPort",
            ShapeIssue::DoubleSlash => "doubleSlash",
        }
    }
}

/// 用不了的理由：形状不对，或明文而主机不是回环（一把 key 明着过网线）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unusable {
    Shape(ShapeIssue),
    PlaintextOffLoopback,
}

impl Unusable {
    /// 同 [`ShapeIssue::code`]；明文那一格叫 `plaintextOffLoopback`。
    pub fn code(self) -> &'static str {
        match self {
            Unusable::Shape(i) => i.code(),
            Unusable::PlaintextOffLoopback => "plaintextOffLoopback",
        }
    }
}

/// 形状：无空白 · `<协议>://` · 协议在 [`SCHEMES`] · authority 非空、无 `?#` · 最后一个 `:` 之后是 u16（整段 `[…]` 不带口 ⇒ 默认口）· 主机非空 ·
/// 路径前缀无 `?#`、去掉尾 `/` 之后不以 `//` 开头。逐步照搬先前 `Base::parse`（外加写口那一格「无空白」）。
pub fn parse(url: &str) -> Result<UpstreamUrl, ShapeIssue> {
    if url.chars().any(char::is_whitespace) {
        return Err(ShapeIssue::Whitespace);
    }
    let (scheme, rest) = url.split_once("://").ok_or(ShapeIssue::NotUrl)?;
    let tls = SCHEMES
        .iter()
        .find(|(s, _)| *s == scheme)
        .map(|(_, t)| *t)
        .ok_or(ShapeIssue::BadScheme)?;
    let (authority, raw_path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if authority.is_empty() {
        return Err(ShapeIssue::NoHost);
    }
    if authority.contains('?') || authority.contains('#') {
        return Err(ShapeIssue::HasQuery);
    }
    let default_port = if tls { 443 } else { 80 };
    let (host, port) = match authority.rsplit_once(':') {
        // 〔TAIL · DUP3 §5 ④〕整段就是一对方括号（`[::1]`）⇒ 里面的 `:` 是 IPv6 的，不是端口分隔符。
        Some(_) if is_bracketed(authority) => (authority, default_port),
        Some((h, p)) => (h, p.parse::<u16>().map_err(|_| ShapeIssue::BadPort)?),
        None => (authority, default_port),
    };
    if host.is_empty() {
        return Err(ShapeIssue::NoHost);
    }
    Ok(UpstreamUrl {
        tls,
        host: host.to_string(),
        port,
        path: normalize_prefix(raw_path)?,
    })
}

/// authority 整段是 `[…]`：以 `[` 开头、以 `]` 结尾、中间再没有 `]`（生成物的 `badPort` 式子按同一形排除它）。
fn is_bracketed(authority: &str) -> bool {
    authority.len() >= 2
        && authority.starts_with('[')
        && authority.ends_with(']')
        && !authority[1..authority.len() - 1].contains(']')
}

/// 路径前缀：去掉没有意义的尾 `/`，不做任何猜测；带 `?#` 或以 `//` 开头（会被读成 authority）⇒ 拒。
fn normalize_prefix(raw: &str) -> Result<String, ShapeIssue> {
    if raw.contains('?') || raw.contains('#') {
        return Err(ShapeIssue::HasQuery);
    }
    let trimmed = raw.trim_end_matches('/');
    if trimmed.starts_with("//") {
        return Err(ShapeIssue::DoubleSlash);
    }
    Ok(trimmed.to_string())
}

/// 这个上游主机是不是**本机回环**（决定明文 http 可不可用）：剥掉方括号，能解析成 IP ⇒ `IpAddr::is_loopback`
/// （整个 `127/8` 与 `::1` 的任何写法）；否则逐字（忽略大小写）等于 [`LOOPBACK_NAME`] —— 那是约定，不是 DNS 证过的事实。
/// ⚠ 与中转进门的 `Host` 头判定（后端 `relay/door.rs::host_header_is_loopback_literal`，只认三个字面量）是**两个判定**。
pub fn upstream_is_loopback(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    match h.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => h.eq_ignore_ascii_case(LOOPBACK_NAME),
    }
}

/// **能不能用**：[`parse`] ＋「明文且主机不是回环 ⇒ [`Unusable::PlaintextOffLoopback`]」。写口 · 装表 · 界面都是这一条。
pub fn usable(url: &str) -> Result<UpstreamUrl, Unusable> {
    let u = parse(url).map_err(Unusable::Shape)?;
    if !u.tls && !upstream_is_loopback(&u.host) {
        return Err(Unusable::PlaintextOffLoopback);
    }
    Ok(u)
}

#[cfg(test)]
#[path = "../../../../tests/common/upstream-url-core/lib_tests.rs"]
mod tests;
