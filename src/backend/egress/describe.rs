//! 一份出网描述：走不走代理、走哪个、带不带鉴权、哪些目的地不走代理。纯逻辑，不开连接、不读环境。

use std::net::IpAddr;

/// 一份出网描述（这台的，或以后某个会话的）。
#[derive(Clone, PartialEq, Eq)]
pub struct Egress {
    pub(crate) proxy: Setting,
    pub(crate) no_proxy: NoProxy,
}

/// 代理那一格。
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Setting {
    /// 没设代理：直连。
    Direct,
    /// 经这个代理。
    Via(Proxy),
    /// 设了、读不懂：一条都不连（不静默改直连 —— 设了代理的人要的不是直连）。带那一格的名字与读不懂的理由。
    Unreadable { from: String, why: &'static str },
}

/// 一个代理。
#[derive(Clone, PartialEq, Eq)]
pub struct Proxy {
    pub kind: Kind,
    pub host: String,
    pub port: u16,
    /// 代理鉴权（URL 里 `user:pass@` 那一段，已按百分号解码）。HTTP 走 Basic，SOCKS5 走用户名口令子协商。
    pub auth: Option<(String, String)>,
}

impl std::fmt::Debug for Proxy {
    /// 不印口令：描述会进日志。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Proxy")
            .field("kind", &self.kind)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("auth", &self.auth.as_ref().map(|(u, _)| u))
            .finish()
    }
}

impl std::fmt::Debug for Egress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.proxy {
            Setting::Direct => f.write_str("Egress(direct)"),
            Setting::Via(p) => write!(f, "Egress({p:?}, no_proxy {:?})", self.no_proxy.0),
            Setting::Unreadable { from, why } => write!(f, "Egress(unreadable {from}: {why})"),
        }
    }
}

/// 代理那一种。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `http://`（或不写协议）：`CONNECT 主机:端口`。
    Http,
    /// `socks5h://`：名字交给代理解析。
    Socks5Remote,
    /// `socks5://`：名字在这台解析，交给代理的是地址（同 curl 的读法）。
    Socks5Local,
}

/// 免代理表（`no_proxy` 那一格）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NoProxy(pub(crate) Vec<Skip>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Skip {
    /// `*`：全部不走代理。
    All,
    /// 一个名字：它自己与它底下的子域（`example.com` 与 `.example.com` 同义）。
    Domain(String),
    /// 一个地址或一段地址（`198.51.100.0/24`；不带 `/` ⇒ 只那一个）。
    Net(IpAddr, u8),
}

/// 到一个目的地怎么走。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hop<'a> {
    Direct,
    Via(&'a Proxy),
    /// 代理那一格读不懂：不连。
    Refuse {
        from: &'a str,
        why: &'static str,
    },
}

/// 读这台环境时认的名字，按先后（小写在前，同 curl）：目的地是 https 与 SSH 这两种，不认 `http_proxy`（它只管明文 http，而明文上游只许回环）。
pub const PROXY_NAMES: &[&str] = &["https_proxy", "HTTPS_PROXY", "all_proxy", "ALL_PROXY"];
/// 免代理表认的名字，按先后。
pub const NO_PROXY_NAMES: &[&str] = &["no_proxy", "NO_PROXY"];

impl Egress {
    /// 直连（没装这台的描述时就是它）。
    pub fn direct() -> Egress {
        Egress {
            proxy: Setting::Direct,
            no_proxy: NoProxy::default(),
        }
    }

    /// 照一张取值函数读（宿主交它自己的环境；空串当没设）。`proxy` 那几个名字取第一个设了的，免代理表同理。
    pub fn from_env(get: &dyn Fn(&str) -> Option<String>) -> Egress {
        let first = |names: &[&'static str]| {
            names.iter().find_map(|n| {
                get(n)
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .map(|v| (*n, v))
            })
        };
        let no_proxy = first(NO_PROXY_NAMES).map_or_else(String::new, |(_, v)| v);
        match first(PROXY_NAMES) {
            None => Egress {
                proxy: Setting::Direct,
                no_proxy: NoProxy::parse(&no_proxy),
            },
            Some((name, url)) => Egress::named(name, &url, &no_proxy),
        }
    }

    /// 一份写明的描述：代理 URL ＋ 免代理表（逗号或空白分隔）。以后按会话的描述也从这里造。
    pub fn via(url: &str, no_proxy: &str) -> Egress {
        Egress::named("proxy", url, no_proxy)
    }

    fn named(from: &str, url: &str, no_proxy: &str) -> Egress {
        Egress {
            proxy: match parse_proxy(url) {
                Ok(p) => Setting::Via(p),
                Err(why) => Setting::Unreadable {
                    from: from.to_string(),
                    why,
                },
            },
            no_proxy: NoProxy::parse(no_proxy),
        }
    }

    /// 到 `host` 怎么走：回环恒直连（代理够不到这台的回环）· 免代理表命中 ⇒ 直连 · 否则照代理那一格。
    pub fn hop_for(&self, host: &str) -> Hop<'_> {
        let h = bare_host(host);
        if is_loopback(&h) {
            return Hop::Direct;
        }
        match &self.proxy {
            Setting::Direct => Hop::Direct,
            Setting::Unreadable { from, why } => Hop::Refuse { from, why },
            Setting::Via(_) if self.no_proxy.skips(&h) => Hop::Direct,
            Setting::Via(p) => Hop::Via(p),
        }
    }
}

/// 去方括号、去尾点、小写。
fn bare_host(host: &str) -> String {
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn is_loopback(h: &str) -> bool {
    match h.parse::<IpAddr>() {
        Ok(ip) => ip.to_canonical().is_loopback(),
        Err(_) => h == "localhost" || h.ends_with(".localhost"),
    }
}

impl NoProxy {
    pub(crate) fn parse(s: &str) -> NoProxy {
        NoProxy(
            s.split(|c: char| c == ',' || c.is_whitespace())
                .map(str::trim)
                .filter(|e| !e.is_empty())
                .map(|e| {
                    if e == "*" {
                        return Skip::All;
                    }
                    let e = e.to_ascii_lowercase();
                    let (addr, bits) = match e.split_once('/') {
                        Some((a, b)) => (a.to_string(), b.parse::<u8>().ok()),
                        None => (e.clone(), None),
                    };
                    let addr_bare = addr.trim_start_matches('[').trim_end_matches(']');
                    if let Ok(ip) = addr_bare.parse::<IpAddr>() {
                        let full = if ip.is_ipv4() { 32 } else { 128 };
                        return Skip::Net(ip, bits.unwrap_or(full).min(full));
                    }
                    Skip::Domain(
                        e.trim_start_matches("*.")
                            .trim_start_matches('.')
                            .trim_end_matches('.')
                            .to_string(),
                    )
                })
                .collect(),
        )
    }

    fn skips(&self, h: &str) -> bool {
        let ip = h.parse::<IpAddr>().ok();
        self.0.iter().any(|s| match s {
            Skip::All => true,
            Skip::Domain(d) => !d.is_empty() && (h == d || h.ends_with(&format!(".{d}"))),
            Skip::Net(net, bits) => ip.is_some_and(|ip| in_net(ip, *net, *bits)),
        })
    }
}

fn in_net(ip: IpAddr, net: IpAddr, bits: u8) -> bool {
    match (ip.to_canonical(), net.to_canonical()) {
        (IpAddr::V4(a), IpAddr::V4(n)) => {
            let mask = if bits == 0 {
                0
            } else {
                u32::MAX << (32 - u32::from(bits.min(32)))
            };
            u32::from(a) & mask == u32::from(n) & mask
        }
        (IpAddr::V6(a), IpAddr::V6(n)) => {
            let mask = if bits == 0 {
                0
            } else {
                u128::MAX << (128 - u32::from(bits.min(128)))
            };
            u128::from(a) & mask == u128::from(n) & mask
        }
        _ => false,
    }
}

/// 代理 URL ⇒ [`Proxy`]。认 `http://` · `socks5://` · `socks5h://`，不写协议当 `http://`；不写端口照 curl 当 1080。
/// 读不懂 ⇒ 一句理由（ASCII 技术话，进复制详情的原话那一格）。
pub(crate) fn parse_proxy(url: &str) -> Result<Proxy, &'static str> {
    let url = url.trim();
    let (kind, rest) = match url.split_once("://") {
        None => (Kind::Http, url),
        Some((scheme, rest)) => (
            match scheme.to_ascii_lowercase().as_str() {
                "http" => Kind::Http,
                "socks5h" => Kind::Socks5Remote,
                "socks5" => Kind::Socks5Local,
                _ => return Err("unsupported proxy scheme (http, socks5, socks5h)"),
            },
            rest,
        ),
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let (userinfo, hostport) = match authority.rsplit_once('@') {
        Some((u, h)) => (Some(u), h),
        None => (None, authority),
    };
    let (host, port) = if let Some(v6) = hostport.strip_prefix('[') {
        let (h, after) = v6.split_once(']').ok_or("bad proxy host")?;
        let port = match after.strip_prefix(':') {
            Some(p) => Some(p),
            None if after.is_empty() => None,
            None => return Err("bad proxy host"),
        };
        (h.to_string(), port)
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), Some(p)),
            None => (hostport.to_string(), None),
        }
    };
    if host.is_empty() {
        return Err("proxy host missing");
    }
    let port = match port {
        None | Some("") => 1080,
        Some(p) => p.parse::<u16>().map_err(|_| "bad proxy port")?,
    };
    let auth = match userinfo {
        None => None,
        Some(u) => {
            let (user, pass) = u.split_once(':').unwrap_or((u, ""));
            Some((pct_decode(user)?, pct_decode(pass)?))
        }
    };
    Ok(Proxy {
        kind,
        host,
        port,
        auth,
    })
}

fn pct_decode(s: &str) -> Result<String, &'static str> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = b
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or("bad percent escape in proxy credentials")?;
            out.push(hex);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "proxy credentials are not UTF-8")
}
