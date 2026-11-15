//! 订阅号的登录令牌：读那个号此刻的访问令牌；快过期就用刷新令牌续，续完加锁、整份原子写回那个号的凭据文件
//! （与那个号自己的 claude 共用同一份文件、同一套锁）。
//!
//! - 令牌一律包成秘密（`creds_core::SecretKey`）：不进日志、不进 argv、不进帧、不进报错原文。
//! - 那一家的文件格式、令牌端点、锁的名字住适配层（[`LoginFace`]）；这里只有「怎么读、怎么续、怎么写回」。
//! - 续失败照实说（哪个号、为什么），不静默跳过、不猜。
//! - 一家声明账号 0 不续不写（[`crate::agents::BaseLogin`]）⇒ 它的账号 0 只读：令牌没过期照用；要续的那一下不续、照实说，不拿锁、不写，不记成续不上；号载体那几个号照续。
//! - 续成了而写回碰上那一家正在存盘（它的整份写锁在）⇒ 新令牌不丢：先在内存里照用，等锁放开再写回（[`writeback`]）。

pub(crate) mod missed;
pub(crate) mod store;
pub(crate) mod writeback;

use crate::agents::LoginFace;
use crate::relay::Base;
use copy_core::copy_text;
use creds_core::token::{merge_tokens, refresh_body, secret_in};
use creds_core::SecretKey;
use serde_json::{Map, Value};
use std::path::Path;

/// 令牌端点：往哪儿续 ＋ 那一发的期限（期限由调用方给）。
pub struct TokenEndpoint {
    pub base: Base,
    pub rest: String,
    pub deadline: std::time::Duration,
}

impl TokenEndpoint {
    /// 那一家登记的缺省令牌端点（写不出 ⇒ `None`）。
    pub fn of(face: &LoginFace, deadline: std::time::Duration) -> Option<Self> {
        Self::at(face.token_url, deadline)
    }

    /// 一个令牌端点地址（写不出 ⇒ `None`）。
    pub fn at(url: &str, deadline: std::time::Duration) -> Option<Self> {
        let (scheme_host, rest) = split_url(url)?;
        Some(Self {
            base: crate::relay::base_of(scheme_host).ok()?,
            rest: rest.to_string(),
            deadline,
        })
    }
}

/// 续登录令牌那一发的期限（全仓只这一份：换号那一路给缺省端点 · 只有出处那一形都用它）。
#[expect(
    clippy::disallowed_methods,
    reason = "续订阅号令牌那一发的 socket 读写期限（`oauth::TOKEN_DEADLINE`，交给 `TokenEndpoint` → 中转的一问一答原语）：一次阻塞的上限，到点那一发报错返回、不重试"
)]
pub(crate) const TOKEN_DEADLINE: std::time::Duration = std::time::Duration::from_millis(15_000);

fn split_url(url: &str) -> Option<(&str, &str)> {
    let after = url.find("://")? + 3;
    let slash = url[after..].find('/').map_or(url.len(), |i| after + i);
    Some((&url[..slash], &url[slash..]))
}

/// 回包头与体各自的上限（令牌端点的回包很小）；超了整发作错、往上交，不截断。
const ANSWER_CAP: cap_core::Cap = cap_core::Cap {
    bytes: 64 * 1024,
    over: cap_core::Over::HardError,
};

/// 为什么这个号此刻拿不出能用的访问令牌。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unusable {
    /// 那个号没登录（凭据文件不在 / 里面没有令牌）。
    NotLoggedIn,
    /// 凭据文件读不出来（一句原因词 ＋ 下层原话；不带内容）。
    Unreadable(crate::common::said::Said),
    /// 访问令牌过期了，又没有刷新令牌。
    Expired,
    /// 别的进程正在续这个号（锁有人持着）。
    Busy,
    /// 令牌端点不收这个刷新令牌（状态码）—— 要重新登录。
    Refused(u16),
    /// 续期那一发没成（断开 / 回包读不懂），一句原因 ＋ 可缺的原话。
    NotRenewed(crate::common::said::Said),
    /// 续好了，写不回那份文件（一句原因词 ＋ 原话）。
    WriteFailed(crate::common::said::Said),
    /// 那一家没给全续期要的那几格（令牌端点写不出、凭据里也没记出处）⇒ 不替它续。
    CannotRenew,
    /// 这是那一家的账号 0，它声明了账号 0 不由我方续、写（[`crate::agents::BaseLogin::Untouched`]）。
    BaseUntouched,
}

impl Unusable {
    /// 这一次没拿到算不算那个号的事实（记成「续不上」· 发额度节点都只问它）：别人正在续 · 那一家给不全 · 账号 0 不归我方续 ⇒ 不算。
    pub(crate) fn is_account_fact(&self) -> bool {
        !matches!(
            self,
            Unusable::Busy | Unusable::CannotRenew | Unusable::BaseUntouched
        )
    }

    /// 对外那一句（点名是哪个号）＋ 下层原话（只进日志：换号那一路 `rotate.rs` 记 `logged()`）。
    pub(crate) fn said(&self, account: &str) -> crate::common::said::Said {
        let plain = |s: String| crate::common::said::Said::from(s);
        match self {
            Unusable::NotLoggedIn => plain(copy_text(
                "beManual.list.accountLogin",
                &[("account", account)],
            )),
            Unusable::Unreadable(why) => why.clone().wrap(|w| {
                copy_text(
                    "beOauth.unusable.unreadable",
                    &[("account", account), ("why", w)],
                )
            }),
            Unusable::Expired => plain(copy_text(
                "beOauth.unusable.expired",
                &[("account", account)],
            )),
            Unusable::Busy => plain(copy_text("beOauth.unusable.busy", &[("account", account)])),
            // 端点答的码是原话：进日志那一行，不上句子。
            Unusable::Refused(status) => crate::common::said::Said::with_raw(
                copy_text("beOauth.unusable.rejected", &[("account", account)]),
                format!("HTTP {status}"),
            ),
            Unusable::NotRenewed(why) => why.clone().wrap(|w| {
                copy_text(
                    "beOauth.unusable.failed",
                    &[("account", account), ("why", w)],
                )
            }),
            Unusable::WriteFailed(why) => why.clone().wrap(|w| {
                copy_text(
                    "beOauth.unusable.writeFailed",
                    &[("account", account), ("why", w)],
                )
            }),
            Unusable::CannotRenew => plain(copy_text(
                "beOauth.unusable.cannotRenew",
                &[("account", account)],
            )),
            Unusable::BaseUntouched => plain(copy_text(
                "beOauth.unusable.baseUntouched",
                &[("account", account)],
            )),
        }
    }
}

/// 盘上那一份此刻的样子。
struct Now {
    doc: Map<String, Value>,
    access: Option<SecretKey>,
    refresh: Option<SecretKey>,
    expires_ms: Option<u64>,
}

fn look(dir: &Path, face: &LoginFace) -> Result<Now, Unusable> {
    now_of(store::read(dir, face), face)
}

fn now_of(read: store::Read, face: &LoginFace) -> Result<Now, Unusable> {
    let doc = match read {
        store::Read::Absent => return Err(Unusable::NotLoggedIn),
        // `Unusable` 那几句只进日志（换号那一路 `rotate.rs`）⇒ 原话跟着走，记日志时拼在那一句后面。
        store::Read::Unreadable(why) => return Err(Unusable::Unreadable(why)),
        store::Read::Present(d) => d,
    };
    let expires_ms = match face.expiry {
        crate::agents::Expiry::Cell(cell) => doc
            .get(face.section)
            .and_then(|s| s.get(cell))
            .and_then(Value::as_u64),
        crate::agents::Expiry::Read(read) => read(&doc),
    };
    Ok(Now {
        access: secret_in(&doc, face.section, face.access),
        refresh: secret_in(&doc, face.section, face.refresh),
        expires_ms,
        doc,
    })
}

/// 还能用多久之内算「快过期」：同那一家自己的余量。
fn fresh(n: &Now, face: &LoginFace, now_ms: u64) -> bool {
    n.access.is_some()
        && n.expires_ms
            .is_none_or(|e| e > now_ms.saturating_add(face.margin_ms))
}

/// 这个号什么时候由我方续：那一家声明的（`AccountsFace.renew`）＋ 此刻有没有活会话在用这个号（`Idle` 才问，问的是账号库那一侧）。
#[derive(Clone, Copy)]
pub(crate) struct When {
    pub renew: crate::agents::Renew,
    /// 号目录此刻有没有活会话在用（`Idle` 才问）。
    pub in_use: fn(&Path) -> bool,
    /// 这个号是不是那一家的账号 0（不设号载体时那份配置）：是 ⇒ 照那一家的 [`LoginFace::base_login`] 续或不续。
    pub base: bool,
}

impl When {
    /// `kind` 那一家的号在这台上：那一家声明的续期条件（`agents::renew_of`）＋ 进程入口接上的「号目录此刻有没有活会话在用」（[`on_dir_in_use`]）
    /// ＋ 这个号是不是账号 0（`base`）。
    pub(crate) fn here(kind: &str, base: bool) -> When {
        When {
            renew: crate::agents::renew_of(kind),
            in_use: dir_in_use_here,
            base,
        }
    }
}

/// 「号目录此刻有没有活会话在用」那一手：判法在账号库那一侧（`accounts::manage::library::dir_in_use_here`），服务这一块不引它，由进程入口接上。
static DIR_IN_USE: std::sync::OnceLock<fn(&Path) -> bool> = std::sync::OnceLock::new();

/// 进程入口接上那一手；接过一次 ⇒ `Err`（第一次那一手留着）。
pub fn on_dir_in_use(f: fn(&Path) -> bool) -> Result<(), fn(&Path) -> bool> {
    DIR_IN_USE.set(f)
}

/// 没接上 ⇒ 说不出有谁在用：当没人在用（同原来那一形，照常续）。
fn dir_in_use_here(dir: &Path) -> bool {
    DIR_IN_USE.get().is_some_and(|f| f(dir))
}

/// 这个号此刻能用的访问令牌：没过期直接给；快过期就续（加锁、写回）再给。
/// 拿不到的每一次记一行进后端日志（真去续的那一发是 [`renew_line`]：号 `account` · 成败 · 状态码 · 错误码），并记成那个号的事实（[`missed`]）。
/// `own`：写自家状态的句柄 —— 续了要写回时从它铸这个号的凭据窄句柄（[`store::creds_of`]）。
/// `endpoint`：调用方给的令牌端点（那一家登记的缺省那一个；写不出 ⇒ `None`）。凭据里记了出处的号照出处续（[`LoginFace::origin`]）；
/// 两样都没有 ⇒ [`Unusable::CannotRenew`]。
/// `when`：那一家的号什么时候由我方续（[`When`]）；是账号 0、那一家声明账号 0 不续不写 ⇒ 没过期照用，要续时 [`Unusable::BaseUntouched`]，不拿锁、不写、不记。
pub(crate) fn access_token(
    own: &crate::platform::fs::Own,
    dir: &Path,
    face: &LoginFace,
    endpoint: Option<&TokenEndpoint>,
    now_ms: u64,
    account: &str,
    when: When,
) -> Result<SecretKey, Unusable> {
    let mut logged = false;
    let r = fetch(own, dir, face, endpoint, now_ms, account, when, &mut logged);
    if let Err(u) = &r {
        if !logged {
            tracing::warn!("[auth] {}", u.said(account).logged());
        }
    }
    // 不归我方续的账号 0 那一句不是号的事实（`Unusable::is_account_fact`），记账那一下照它不记。
    missed::note(dir, face, now_ms / 1000, &r);
    r
}

/// 这个号是那一家声明不续不写的账号 0（[`crate::agents::BaseLogin::Untouched`]）。
fn left_to_itself(face: &LoginFace, when: When) -> bool {
    when.base && face.base_login == crate::agents::BaseLogin::Untouched
}

/// 盘上那一份此刻拿它怎么办（只读判；[`fetch`] 与 [`unrenewed_base`] 同用这一处）。
enum Next {
    /// 没过期：照用。
    Use(SecretKey),
    /// 要续。
    Renew,
    /// 不续，照实回这一句。
    Stop(Unusable),
}

fn next_of(n: &Now, face: &LoginFace, when: When, now_ms: u64) -> Next {
    if fresh(n, face, now_ms) {
        return match &n.access {
            Some(a) => Next::Use(a.clone()),
            None => Next::Stop(Unusable::NotLoggedIn),
        };
    }
    if n.access.is_none() && n.refresh.is_none() {
        return Next::Stop(Unusable::NotLoggedIn);
    }
    if left_to_itself(face, when) {
        return Next::Stop(Unusable::BaseUntouched);
    }
    Next::Renew
}

/// ★ 这个号此刻是不是「不归我方续的账号 0、令牌又快过期 / 过期了」（只读：不续、不拿锁、不写；换号判接不接得上问它）。
pub(crate) fn unrenewed_base(dir: &Path, face: &LoginFace, when: When, now_ms: u64) -> bool {
    left_to_itself(face, when)
        && look(dir, face).is_ok_and(|n| {
            matches!(
                next_of(&n, face, when, now_ms),
                Next::Stop(Unusable::BaseUntouched)
            )
        })
}

#[expect(
    clippy::too_many_arguments,
    reason = "拿令牌那一趟要的几样（写句柄 · 盘 · 那一家的登录面 · 令牌端点 · 时刻 · 号 · 什么时候续 · 记没记过日志）各由调用方给，同 [`access_token`] 一一对应"
)]
fn fetch(
    own: &crate::platform::fs::Own,
    dir: &Path,
    face: &LoginFace,
    endpoint: Option<&TokenEndpoint>,
    now_ms: u64,
    account: &str,
    when: When,
    logged: &mut bool,
) -> Result<SecretKey, Unusable> {
    // 刚续成、写回还在等那一家放写锁 ⇒ 照用内存里那份，不再续一次（等到期限没放开 ⇒ 这里照实报一次写回失败）。
    if let Some(r) = writeback::served(dir, face, now_ms) {
        return r;
    }
    let n = look(dir, face)?;
    match next_of(&n, face, when, now_ms) {
        Next::Use(a) => return Ok(a),
        Next::Stop(u) => return Err(u),
        Next::Renew => {}
    }
    if when.renew == crate::agents::Renew::Idle && (when.in_use)(dir) {
        return Err(Unusable::Busy);
    }
    let creds = store::creds_of(own, dir, face).map_err(Unusable::NotRenewed)?;
    let mut later: Option<Renewed> = None;
    let r = match store::with_refresh_lock(&creds, face, |reclaimed, held| {
        refresh_locked(
            dir,
            face,
            endpoint,
            now_ms,
            account,
            reclaimed,
            (&creds, held),
            &mut later,
            logged,
        )
    }) {
        Ok(store::Locked::Held(r)) => r,
        Ok(store::Locked::Busy) => Err(Unusable::Busy),
        Err(why) => Err(Unusable::NotRenewed(why)),
    };
    // 续期锁已经放了：写回交给等写锁放开的那一位（文件事件叫醒，有总期限）。
    if let Some(renewed) = later {
        writeback::defer(own, dir, face, account, renewed);
    }
    r
}

/// 在续期锁里（同一时刻一个号只有一个在续：跨进程锁让同进程的几条线程也排队）：
/// 再看一眼盘（前一个刚续过就用它的）→ 发续期 → 比对盘上的刷新令牌还是不是发出去那个 → 整份写回。
#[expect(
    clippy::too_many_arguments,
    reason = "续期锁里那一趟要的几样（盘 · 那一家的登录面 · 令牌端点 · 时刻 · 号 · 收回的旧令牌 · 写锁 · 回写的去处）各由调用方给，拼成结构体只给这一个函数用"
)]
fn refresh_locked(
    dir: &Path,
    face: &LoginFace,
    endpoint: Option<&TokenEndpoint>,
    now_ms: u64,
    account: &str,
    reclaimed: &store::Reclaimed,
    write: Writing<'_>,
    later: &mut Option<Renewed>,
    logged: &mut bool,
) -> Result<SecretKey, Unusable> {
    let n = look(dir, face)?;
    let mut trail = Trail {
        reclaimed: reclaimed.0.clone(),
        ..Trail::default()
    };
    if fresh(&n, face, now_ms) {
        let r = n.access.ok_or(Unusable::NotLoggedIn);
        if !trail.reclaimed.is_empty() {
            // 收回了过期锁、而盘上已经是新令牌（别人续过）⇒ 不去续，这一行照记。
            trail.yielded = true;
            tracing::info!("{}", renew_line(account, &trail, &r));
        }
        return r;
    }
    let Some(posted) = n.refresh.as_ref() else {
        return Err(Unusable::Expired);
    };
    let r = renew(face, endpoint, now_ms, &n, posted, &mut trail, write, later);
    let line = renew_line(account, &trail, &r);
    *logged = true;
    if r.is_ok() {
        tracing::info!("{line}");
    } else {
        tracing::warn!("{line}");
    }
    r
}

/// 续期那一发留下的、可以进日志的几格（全是闭集或数字，没有一格取自令牌）。
#[derive(Default)]
struct Trail {
    /// 令牌端点回的状态码；没连上 ⇒ `None`。
    status: Option<u16>,
    /// 令牌端点回的错误码（回包 `error` 那一格，只收 [`error_code`] 认得的形状）。
    code: Option<String>,
    /// 没成时卡在哪（固定的短词）。
    why: Option<String>,
    /// 续成了，但盘上的刷新令牌已被那个号自己换掉 ⇒ 用盘上的、不写回。
    yielded: bool,
    /// 续成了，写回时那一家正持着整份写锁 ⇒ 新令牌先在内存里照用，写回等它放开（[`writeback`]）。
    deferred: bool,
    /// 拿锁时当无主收回的那几把（[`store::Reclaimed`]）。
    reclaimed: Vec<&'static str>,
}

/// 令牌端点回包里 `error` 那一格：只收 OAuth 错误码的形状（小写字母与下划线，至多 40 个），别的一概不收。
fn error_code(body: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(body).ok()?;
    let code = v.get("error")?.as_str()?;
    (!code.is_empty()
        && code.len() <= 40
        && code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
    .then(|| code.to_string())
}

/// 续期那一发记进后端日志的一行：号 · 成 / 败 / 让位 · 状态码 · 错误码 · 原因。只用 [`Trail`] 那几格 ⇒ 不带令牌、不带回包体。
fn renew_line(account: &str, t: &Trail, r: &Result<SecretKey, Unusable>) -> String {
    let outcome = match r {
        Ok(_) if t.yielded => "让位",
        Ok(_) => "成",
        Err(_) => "败",
    };
    let why = match r {
        Ok(_) if t.deferred => Some("write-deferred".to_string()),
        Ok(_) => None,
        Err(Unusable::Refused(_)) => Some("refused".to_string()),
        Err(Unusable::WriteFailed(_)) => Some("write-failed".to_string()),
        Err(Unusable::Expired) => Some("expired".to_string()),
        Err(Unusable::NotLoggedIn) => Some("not-logged-in".to_string()),
        Err(Unusable::Unreadable(_)) => Some("unreadable".to_string()),
        Err(Unusable::Busy) => Some("busy".to_string()),
        Err(Unusable::CannotRenew) => Some("cannot-renew".to_string()),
        Err(Unusable::BaseUntouched) => Some("base-untouched".to_string()),
        Err(Unusable::NotRenewed(_)) => t.why.clone(),
    };
    format!(
        "[auth] 续期：号 {account} · {outcome} · 状态码 {} · 错误码 {} · 原因 {} · 收回过期锁 {}",
        t.status.map_or_else(|| "—".to_string(), |s| s.to_string()),
        t.code.as_deref().unwrap_or("—"),
        why.as_deref().unwrap_or("—"),
        if t.reclaimed.is_empty() {
            "—".to_string()
        } else {
            t.reclaimed.join("、")
        },
    )
}

/// 令牌端点那一发没连上 / 没读完：句子只带原因词，原话跟着（只进日志）。
fn unreached(trail: &mut Trail, e: &std::io::Error) -> Unusable {
    trail.why = Some(format!("transport({})", e.kind()));
    Unusable::NotRenewed(crate::common::said::Said::with_raw(
        copy_core::io_reason(e.kind()),
        e,
    ))
}

/// 发续期 → 读回包 → 写前比对 → 整份写回；一路把能进日志的几格记进 `trail`。
/// 写回那一下要的两样：这个号的凭据窄句柄 ＋ 持着的那把配置目录锁。
type Writing<'a> = (
    &'a crate::platform::fs::AccountCreds,
    &'a crate::platform::fs::CredsLock,
);

#[expect(
    clippy::too_many_arguments,
    reason = "发一次续期要的几样（登录面 · 令牌端点 · 时刻 · 盘上那一份 · 发出去的刷新令牌 · 留痕 · 写锁 · 回写的去处）由 `refresh_locked` 逐样递下来"
)]
fn renew(
    face: &LoginFace,
    endpoint: Option<&TokenEndpoint>,
    now_ms: u64,
    n: &Now,
    posted: &SecretKey,
    trail: &mut Trail,
    (creds, held): Writing<'_>,
    later: &mut Option<Renewed>,
) -> Result<SecretKey, Unusable> {
    // 凭据里记了出处 ⇒ 照出处（地址认不出就不续：不把这个号的刷新令牌发去缺省那一家）；没记 ⇒ 调用方给的缺省那一个。
    let origin = face.origin.and_then(|read| read(&n.doc));
    let from_origin;
    let endpoint = match &origin {
        Some(o) => {
            from_origin = TokenEndpoint::at(
                &o.token_url,
                endpoint.map_or(TOKEN_DEADLINE, |e| e.deadline),
            );
            from_origin.as_ref()
        }
        None => endpoint,
    };
    let Some(endpoint) = endpoint else {
        trail.why = Some("cannot-renew".to_string());
        return Err(Unusable::CannotRenew);
    };
    let sec = n.doc.get(face.section);
    let client = origin
        .as_ref()
        .and_then(|o| o.client_id.clone())
        .or_else(|| {
            sec.and_then(|s| s.get(face.client_field))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| face.client_id.to_string());
    // 这一家记授权范围 ⇒ 请求带上盘上那一串（读不到发空串）；不记 ⇒ 请求体里没有 `scope` 这一格。
    let scope = face.scopes.map(|cell| {
        sec.and_then(|s| s.get(cell))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
    });
    let mut plain = vec![
        ("grant_type", "refresh_token"),
        ("client_id", client.as_str()),
    ];
    if let Some(scope) = &scope {
        plain.push(("scope", scope));
    }
    let body = refresh_body(&plain, ("refresh_token", posted));
    let answer = crate::relay::fetch(
        &crate::relay::open_here,
        &endpoint.base,
        "POST",
        &endpoint.rest,
        &[("Content-Type", "application/json")],
        body.as_bytes(),
        endpoint.deadline,
        ANSWER_CAP.as_len(),
    )
    .map_err(|e| unreached(trail, &e))?;
    trail.status = Some(answer.status);
    if answer.status != 200 {
        trail.code = error_code(&answer.body);
        trail.why = Some("status".to_string());
        return Err(if matches!(answer.status, 400 | 401) {
            Unusable::Refused(answer.status)
        } else {
            Unusable::NotRenewed(crate::common::said::Said::with_raw(
                copy_text("beOauth.refresh.status", &[]),
                format!("HTTP {}", answer.status),
            ))
        });
    }
    let got: Map<String, Value> = match serde_json::from_slice(&answer.body) {
        Ok(Value::Object(m)) => m,
        _ => {
            trail.why = Some("not-json".to_string());
            return Err(Unusable::NotRenewed(crate::common::said::Said::from(
                copy_text("beOauth.refresh.unparsable", &[]),
            )));
        }
    };
    let wrapped = Map::from_iter([(String::from("t"), Value::Object(got.clone()))]);
    // 回包里的期限（秒）：过期时刻记在一格里的那一形缺它不行；交那一家读的那一形缺了就照它的读法读写回后的那一份。
    let expires_in = got.get("expires_in").and_then(Value::as_u64);
    let (Some(access), true) = (
        secret_in(&wrapped, "t", "access_token"),
        expires_in.is_some() || matches!(face.expiry, crate::agents::Expiry::Read(_)),
    ) else {
        trail.why = Some("missing".to_string());
        return Err(Unusable::NotRenewed(crate::common::said::Said::from(
            copy_text("beOauth.refresh.unparsable", &[]),
        )));
    };
    // 这一家声明另换的那几格：回包里没有（null · 空串同）⇒ 那一格不动；有而不是一串、或过不了它的校验 ⇒ 整次不落盘，照实说回包读不懂。
    let mut cells = Vec::new();
    for c in face.renewed_cells {
        let raw = match got.get(c.answer) {
            None | Some(Value::Null) => continue,
            Some(Value::String(s)) if s.trim().is_empty() => continue,
            Some(v) => v.as_str(),
        };
        let Some(raw) = raw.filter(|s| c.check.is_none_or(|ok| ok(s))) else {
            trail.why = Some("bad-cell".to_string());
            return Err(Unusable::NotRenewed(crate::common::said::Said::from(
                copy_text("beOauth.refresh.unparsable", &[]),
            )));
        };
        cells.push((c.cell, SecretKey::new(raw)));
    }
    let mut renewed = Renewed {
        posted: posted.clone(),
        refresh: secret_in(&wrapped, "t", "refresh_token"),
        expires_ms: now_ms.saturating_add(expires_in.unwrap_or(0).saturating_mul(1000)),
        at_ms: now_ms,
        // 写不写进凭据只在 `renewed_doc` 判（这一家不记授权范围 ⇒ 不写）。
        scopes: got
            .get("scope")
            .and_then(Value::as_str)
            .map(|s| s.split_whitespace().map(Value::from).collect::<Vec<_>>())
            .filter(|l| !l.is_empty()),
        cells,
        access,
    };
    if let (None, crate::agents::Expiry::Read(read)) = (expires_in, face.expiry) {
        renewed.expires_ms = read(&renewed_doc(face, &n.doc, &renewed)).unwrap_or(now_ms);
    }
    match write_back(creds, face, held, &renewed) {
        Err(why) => Err(Unusable::WriteFailed(why)),
        Ok(store::Locked::Busy) => {
            trail.deferred = true;
            let access = renewed.access.clone();
            *later = Some(renewed);
            Ok(access)
        }
        Ok(store::Locked::Held(Err(u))) => Err(u),
        Ok(store::Locked::Held(Ok(WriteBack::Yielded(disk_access)))) => {
            trail.yielded = true;
            disk_access.ok_or(Unusable::Expired)
        }
        Ok(store::Locked::Held(Ok(WriteBack::Written))) => Ok(renewed.access),
    }
}

/// 续成了、要写回那一节的那一份（令牌都包成秘密）。`posted` = 发出去的那个刷新令牌（写前比对用）。
#[derive(Clone)]
pub(crate) struct Renewed {
    posted: SecretKey,
    access: SecretKey,
    refresh: Option<SecretKey>,
    expires_ms: u64,
    /// 续成的那一刻（毫秒；那一家要另记「几点续的」时写它，[`LoginFace::renewed_at`]）。
    at_ms: u64,
    scopes: Option<Vec<Value>>,
    /// 那一家声明另换的几格（[`LoginFace::renewed_cells`]）里收下的：那一节里的格名 ⇒ 新值。
    cells: Vec<(&'static str, SecretKey)>,
}

impl Renewed {
    /// 新的访问令牌（等写回期间每一发各交一份）与它几点过期。
    pub(crate) fn access(&self) -> (&SecretKey, u64) {
        (&self.access, self.expires_ms)
    }
}

/// 写回的结局（写锁拿到了的那一支）。
pub(crate) enum WriteBack {
    /// 写进去了。
    Written,
    /// 盘上的刷新令牌已经不是发出去那个（那个号自己的 claude 抢先续过）⇒ 用盘上的、不覆盖（盘上的访问令牌，可缺）。
    Yielded(Option<SecretKey>),
}

/// **续成之后写回只此一处**（续期那一发当场写 · 等写锁放开后再写，都经它）：在那一家的整份写锁里重读盘 → 比对
/// → 只换那一节里那几格、整份原子写（[`store::rewrite`]）。写锁有人持着 ⇒ `Busy`，一个字节不写。
pub(crate) fn write_back(
    creds: &crate::platform::fs::AccountCreds,
    face: &LoginFace,
    held: &crate::platform::fs::CredsLock,
    r: &Renewed,
) -> Result<store::Locked<Result<WriteBack, Unusable>>, crate::common::said::Said> {
    store::rewrite(creds, face, held, |read| match now_of(read, face) {
        Err(u) => (None, Err(u)),
        Ok(disk)
            if !disk
                .refresh
                .as_ref()
                .is_some_and(|x| x.same_secret(&r.posted)) =>
        {
            (None, Ok(WriteBack::Yielded(disk.access)))
        }
        Ok(disk) => (
            Some(renewed_doc(face, &disk.doc, r)),
            Ok(WriteBack::Written),
        ),
    })
}

/// 续成之后的那一份：整份原样，只换那一节里访问 · 刷新令牌、那一家声明另换的几格（[`LoginFace::renewed_cells`]）、
/// 声明了才换的授权范围（＋ 那一家要记的「几点续的」那一格）。
/// 过期时刻记在一格里的那一形才写回那一格；交那一家读的那一形（解令牌）不另写。
fn renewed_doc(face: &LoginFace, disk: &Map<String, Value>, r: &Renewed) -> Map<String, Value> {
    let mut plain: Vec<(&str, Value)> = Vec::new();
    if let crate::agents::Expiry::Cell(cell) = face.expiry {
        plain.push((cell, Value::from(r.expires_ms)));
    }
    if let (Some(cell), Some(list)) = (face.scopes, &r.scopes) {
        plain.push((cell, Value::Array(list.clone())));
    }
    let mut secrets: Vec<(&str, &SecretKey)> = vec![(face.access, &r.access)];
    if let Some(rt) = r.refresh.as_ref() {
        secrets.push((face.refresh, rt));
    }
    secrets.extend(r.cells.iter().map(|(cell, v)| (*cell, v)));
    let mut doc = merge_tokens(disk, face.section, &secrets, &plain);
    if let Some((key, value)) = face.renewed_at {
        doc.insert(key.to_string(), value(r.at_ms));
    }
    doc
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/oauth/oauth_tests.rs"]
mod tests;
