//! 中转口的**门**〔`INVARIANTS §48.1a` 中转口的钥匙〕：钥匙的形状与进门三问（纯判定：只看请求头与钥匙，不读网络、不读盘）。
//!
//! 钥匙住哪、谁铸、怎么落盘是后端的事（`src/backend/relay/key.rs`，绑上口之后读回或铸好，经 `Relay::new` 交进来）。
//!
//! # 为什么要有这扇门（缺口，不是取舍）
//!
//! 中转听的是回环 TCP，而回环 TCP **没有权限位**：同机任何进程 —— 别的 OS 用户、
//! 浏览器里一张网页向 `127.0.0.1` 发的请求 —— 连得上它。门开着的时候，谁走 `/s/<agent>/<账号>/…`
//! 就能让中转代入那一行的凭据去打上游。路由键第三段（会话 id / nonce）**不是**认证：它是公开可铸的标签。
//!
//! # 进门三问（[`admit`]，顺序固定，都在读请求体之前）
//!
//! 1. 带 `Origin` ⇒ [`Verdict::Browser`]（**403**）—— 浏览器发的请求一律带它；claude CLI 现打不带（`RK1.md §5.1`）；
//! 2. `Host` 不是回环字面量（或没有、或不止一个）⇒ [`Verdict::NotLoopbackHost`]（**421**，防 DNS rebinding）；
//! 3. 钥匙不对 ⇒ [`Verdict::BadKey`]（**403**）；比对定长时间（[`tokens_match`]，与后端控制口同一份）。
//!    钥匙只在路径第一段。门上有两把（[`Keys`]）：全权那一把（[`Scope::All`]）与只许直通那一把（[`Scope::Passthrough`]）。
//!
//! 过了才剥掉 `/<钥匙>`，余下的交给 `route::parse`（一字不改）⇒ 表里没这一行仍是 **404**，与 403 可分。
//! 路由认出来之后再问一句 [`scope_refusal`]：直通那一把打 `/s/`（代入凭据）⇒ **403** `key-scope`。
//!
//! # 为什么要第二把（只许直通）
//!
//! 有的家的上游地址只能经命令行参数交给它（argv 同机别的用户读得到），也会被它原样写进自己的日志。
//! 那一家只拿直通那一把：`/t/` 永不代入凭据、请求得自己带登录头 ⇒ 这把漏了，别人顶多拿我们的中转转发**他自己**的请求、
//! 往活卡里塞几段假流；碰不到 `/s/` 那几行 API key。
//!
//! # ⚠ 诚实边界
//!
//! - 钥匙挡的是「**读不到那个钥匙文件**的人」。能读你家目录（root、你自己的进程、你起的 agent）的，本来就能以你的身份跑东西。
//! - 文件在中转跑着的时候被人删掉 / 改掉 ⇒ 中转手里那一把与盘上对不上，新会话每一发 403（出声，不静默）；重起中转就好。

use super::http1::RequestHead;

/// 门拒绝时回的两个状态行。
pub const FORBIDDEN: &str = "403 Forbidden";
pub const MISDIRECTED: &str = "421 Misdirected Request";

/// 一把钥匙。**刻意不派生 `Debug` / `Display`**：值只经 [`Key::expose`] 一处拿得出来，
/// 谁想把它 `{:?}` 进日志，编译器先拦。
#[derive(Clone)]
pub struct Key(String);

impl Key {
    /// 唯一的取值口：门里比对 · 后端落盘 · 给用户要贴的那一段插钥匙。
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// 从一个串认一把钥匙：形状不对就不认（空 / 短 / 非小写十六进制都算不对）。
    pub fn from_text(s: &str) -> Option<Key> {
        let t = s.trim();
        relay_route_core::key_shape_ok(t).then(|| Key(t.to_string()))
    }
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(…)")
    }
}

/// 门上的两把钥匙。
#[derive(Clone, Debug)]
pub struct Keys {
    /// 全权：`/s/` 与 `/t/` 都过。
    pub full: Key,
    /// 只许直通：只过 `/t/`（[`scope_refusal`]）。
    pub pass: Key,
}

/// 过门的是哪一把。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    All,
    Passthrough,
}

/// 钥匙管不管得到这一条路由：直通那一把打 `/s/` ⇒ 拒绝的说法（状态行 · 原因头 · 一句为什么）；其余 ⇒ `None`。
pub fn scope_refusal(
    scope: Scope,
    mode: super::Mode,
) -> Option<(&'static str, &'static str, &'static str)> {
    (scope == Scope::Passthrough && mode == super::Mode::Substitute).then_some((
        FORBIDDEN,
        "key-scope",
        "relay: this relay key only opens passthrough routes (/t/); substitute routes (/s/) need the full key",
    ))
}

/// 进门三问的结局。
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// 过了：剥掉 `/<钥匙>` 之后的目标（交给 `route::parse`）＋ 过门的是哪一把。
    Pass(String, Scope),
    /// 带 `Origin` —— 浏览器发的。
    Browser,
    /// `Host` 不是回环字面量（或没有 / 不止一个）。
    NotLoopbackHost,
    /// 没钥匙 / 钥匙不对。
    BadKey,
}

impl Verdict {
    /// 拒绝那几格回什么：状态行 ＋ 原因头的值（`server::REASON_HEADER`）＋ 一句为什么。`Pass` ⇒ `None`。
    pub fn refusal(&self) -> Option<(&'static str, &'static str, &'static str)> {
        match self {
            Verdict::Pass(..) => None,
            Verdict::Browser => Some((
                FORBIDDEN,
                "browser-origin",
                "relay: requests carrying an Origin header are refused (browser pages may not use this port)",
            )),
            Verdict::NotLoopbackHost => Some((
                MISDIRECTED,
                "host-not-loopback",
                "relay: the Host header must be a loopback literal (127.0.0.1 / localhost / [::1])",
            )),
            Verdict::BadKey => Some((
                FORBIDDEN,
                "bad-key",
                "relay: missing or wrong relay key (first path segment)",
            )),
        }
    }
}

/// 进门三问。纯函数：只看请求头与钥匙，不读网络、不读盘。
pub fn admit(head: &RequestHead, keys: &Keys) -> Verdict {
    if head
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("origin"))
    {
        return Verdict::Browser;
    }
    let mut hosts = head
        .headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("host"));
    match (hosts.next(), hosts.next()) {
        (Some((_, h)), None) if host_header_is_loopback_literal(h) => {}
        _ => return Verdict::NotLoopbackHost,
    }
    let Some(after) = head.target.strip_prefix('/') else {
        return Verdict::BadKey;
    };
    let (seg, rest) = match after.find('/') {
        Some(i) => (&after[..i], &after[i..]),
        None => (after, ""),
    };
    if rest.is_empty() {
        return Verdict::BadKey;
    }
    // 两把都比（各自定长时间），不按先比中哪把提前收工。
    let full = tokens_match(seg, keys.full.expose());
    let pass = tokens_match(seg, keys.pass.expose());
    match (full, pass) {
        (true, _) => Verdict::Pass(rest.to_string(), Scope::All),
        (false, true) => Verdict::Pass(rest.to_string(), Scope::Passthrough),
        (false, false) => Verdict::BadKey,
    }
}

/// `Host` 那一格是不是回环字面量：`127.0.0.1` · `localhost` · `[::1]`，可带 `:<十进制口>`。大小写不敏感。
/// 防 DNS 重绑，只认三个字面量是设计；与上游那条「这个地址在不在本机」
/// （`upstream_url_core::upstream_is_loopback`，整个 `127/8`）是两个判定，不许并。
pub fn host_header_is_loopback_literal(raw: &str) -> bool {
    let h = raw.trim().to_ascii_lowercase();
    let name = if let Some(r) = h.strip_prefix('[') {
        match r.split_once(']') {
            Some((inner, tail)) if tail.is_empty() || port_ok(tail) => format!("[{inner}]"),
            _ => return false,
        }
    } else {
        match h.rsplit_once(':') {
            Some((n, p)) if port_ok(&format!(":{p}")) => n.to_string(),
            Some(_) => return false,
            None => h.clone(),
        }
    };
    matches!(name.as_str(), "127.0.0.1" | "localhost" | "[::1]")
}

fn port_ok(tail: &str) -> bool {
    tail.strip_prefix(':').is_some_and(|p| {
        !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u16>().is_ok()
    })
}

/// 定长时间的字节比对：**跑完全部**，不提前返回。
///
/// 长度不同直接判不等（长度本来就藏不住，它在 `read_line` 的字节数里）。
///
/// 中转口的门（[`admit`]）与后端控制口比令牌都用这一份 —— 定长比对只许有一个住址。
pub fn tokens_match(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
#[path = "../../../tests/comms/outward/door_tests.rs"]
mod door_tests;
