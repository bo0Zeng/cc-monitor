//! 订阅号的登录令牌：读那个号此刻的访问令牌；快过期就用刷新令牌续，续完加锁、整份原子写回那个号的凭据文件
//! （与那个号自己的 claude 共用同一份文件、同一套锁）。
//!
//! - 令牌一律包成秘密（`creds_core::SecretKey`）：不进日志、不进 argv、不进帧、不进报错原文。
//! - 那一家的文件格式、令牌端点、锁的名字住适配层（[`LoginFace`]）；这里只有「怎么读、怎么续、怎么写回」。
//! - 续失败照实说（哪个号、为什么），不静默跳过、不猜。

pub(crate) mod store;

use crate::agents::LoginFace;
use crate::relay::Base;
use copy_core::copy_text;
use creds_core::token::{merge_tokens, refresh_body, secret_in};
use creds_core::SecretKey;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// 令牌端点：往哪儿续 ＋ 那一发的期限（期限由调用方给）。
pub(crate) struct TokenEndpoint {
    pub(crate) base: Base,
    pub(crate) rest: String,
    pub(crate) deadline: std::time::Duration,
}

impl TokenEndpoint {
    /// 那一家登记的令牌端点。
    pub(crate) fn of(face: &LoginFace, deadline: std::time::Duration) -> Option<Self> {
        let (scheme_host, rest) = split_url(face.token_url)?;
        Some(Self {
            base: Base::parse(scheme_host).ok()?,
            rest: rest.to_string(),
            deadline,
        })
    }
}

fn split_url(url: &str) -> Option<(&str, &str)> {
    let after = url.find("://")? + 3;
    let slash = url[after..].find('/').map_or(url.len(), |i| after + i);
    Some((&url[..slash], &url[slash..]))
}

/// 回包体上限（令牌端点的回包很小）。
const ANSWER_CAP: usize = 64 * 1024;

/// 为什么这个号此刻拿不出能用的访问令牌。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unusable {
    /// 那个号没登录（凭据文件不在 / 里面没有令牌）。
    NotLoggedIn,
    /// 凭据文件读不出来（只有原因，不带内容）。
    Unreadable(String),
    /// 访问令牌过期了，又没有刷新令牌。
    Expired,
    /// 别的进程正在续这个号（锁有人持着）。
    Busy,
    /// 令牌端点不收这个刷新令牌（状态码）—— 要重新登录。
    Refused(u16),
    /// 续期那一发没成（连不上 / 回包读不懂），一句原因。
    NotRenewed(String),
    /// 续好了，写不回那份文件。
    WriteFailed(String),
}

impl Unusable {
    /// 对外那一句（点名是哪个号）。
    pub(crate) fn said(&self, account: &str) -> String {
        match self {
            Unusable::NotLoggedIn => {
                copy_text("beOauth.unusable.notLoggedIn", &[("account", account)])
            }
            Unusable::Unreadable(why) => copy_text(
                "beOauth.unusable.unreadable",
                &[("account", account), ("why", why)],
            ),
            Unusable::Expired => copy_text("beOauth.unusable.expired", &[("account", account)]),
            Unusable::Busy => copy_text("beOauth.unusable.busy", &[("account", account)]),
            Unusable::Refused(status) => copy_text(
                "beOauth.unusable.rejected",
                &[("account", account), ("status", &status.to_string())],
            ),
            Unusable::NotRenewed(why) => copy_text(
                "beOauth.unusable.failed",
                &[("account", account), ("why", why)],
            ),
            Unusable::WriteFailed(why) => copy_text(
                "beOauth.unusable.writeFailed",
                &[("account", account), ("why", why)],
            ),
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
    let doc = match store::read(dir, face) {
        store::Read::Absent => return Err(Unusable::NotLoggedIn),
        store::Read::Unreadable(why) => return Err(Unusable::Unreadable(why)),
        store::Read::Present(d) => d,
    };
    let expires_ms = doc
        .get(face.section)
        .and_then(|s| s.get(face.expires_ms))
        .and_then(Value::as_u64);
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

/// 这个号此刻能用的访问令牌：没过期直接给；快过期就续（加锁、写回）再给。
pub(crate) fn access_token(
    dir: &Path,
    face: &LoginFace,
    endpoint: &TokenEndpoint,
    now_ms: u64,
) -> Result<SecretKey, Unusable> {
    let n = look(dir, face)?;
    if fresh(&n, face, now_ms) {
        return n.access.ok_or(Unusable::NotLoggedIn);
    }
    if n.access.is_none() && n.refresh.is_none() {
        return Err(Unusable::NotLoggedIn);
    }
    match store::with_refresh_lock(dir, face, || refresh_locked(dir, face, endpoint, now_ms)) {
        Ok(store::Locked::Held(r)) => r,
        Ok(store::Locked::Busy) => Err(Unusable::Busy),
        Err(why) => Err(Unusable::NotRenewed(why)),
    }
}

/// 在续期锁里（同一时刻一个号只有一个在续：跨进程锁让同进程的几条线程也排队）：
/// 再看一眼盘（前一个刚续过就用它的）→ 发续期 → 比对盘上的刷新令牌还是不是发出去那个 → 整份写回。
fn refresh_locked(
    dir: &Path,
    face: &LoginFace,
    endpoint: &TokenEndpoint,
    now_ms: u64,
) -> Result<SecretKey, Unusable> {
    let n = look(dir, face)?;
    if fresh(&n, face, now_ms) {
        return n.access.ok_or(Unusable::NotLoggedIn);
    }
    let Some(posted) = n.refresh else {
        return Err(Unusable::Expired);
    };
    let sec = n.doc.get(face.section);
    let client = sec
        .and_then(|s| s.get(face.client_field))
        .and_then(Value::as_str)
        .unwrap_or(face.client_id)
        .to_string();
    let scope = sec
        .and_then(|s| s.get(face.scopes))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let body = refresh_body(
        &[
            ("grant_type", "refresh_token"),
            ("client_id", &client),
            ("scope", &scope),
        ],
        ("refresh_token", &posted),
    );
    let answer = crate::relay::fetch(
        &endpoint.base,
        "POST",
        &endpoint.rest,
        &[("Content-Type", "application/json")],
        body.as_bytes(),
        endpoint.deadline,
        ANSWER_CAP,
    )
    .map_err(|e| Unusable::NotRenewed(e.kind().to_string()))?;
    if answer.status != 200 {
        return Err(if matches!(answer.status, 400 | 401) {
            Unusable::Refused(answer.status)
        } else {
            Unusable::NotRenewed(copy_text(
                "beOauth.refresh.status",
                &[("status", &answer.status.to_string())],
            ))
        });
    }
    let got: Map<String, Value> = match serde_json::from_slice(&answer.body) {
        Ok(Value::Object(m)) => m,
        _ => {
            return Err(Unusable::NotRenewed(copy_text(
                "beOauth.refresh.notJson",
                &[],
            )))
        }
    };
    let wrapped = Map::from_iter([(String::from("t"), Value::Object(got.clone()))]);
    let (Some(access), Some(expires_in)) = (
        secret_in(&wrapped, "t", "access_token"),
        got.get("expires_in").and_then(Value::as_u64),
    ) else {
        return Err(Unusable::NotRenewed(copy_text(
            "beOauth.refresh.missing",
            &[],
        )));
    };
    let refresh = secret_in(&wrapped, "t", "refresh_token");
    // 写前比对：盘上的刷新令牌已经不是发出去那个了 ⇒ 别人（那个号自己的 claude）抢先续过，用它的，不覆盖。
    let disk = look(dir, face)?;
    if !disk
        .refresh
        .as_ref()
        .is_some_and(|r| r.same_secret(&posted))
    {
        return disk.access.ok_or(Unusable::Expired);
    }
    let mut plain: Vec<(&str, Value)> = vec![(
        face.expires_ms,
        Value::from(now_ms.saturating_add(expires_in.saturating_mul(1000))),
    )];
    if let Some(scope) = got.get("scope").and_then(Value::as_str) {
        let list: Vec<Value> = scope.split_whitespace().map(Value::from).collect();
        if !list.is_empty() {
            plain.push((face.scopes, Value::Array(list)));
        }
    }
    let mut secrets: Vec<(&str, &SecretKey)> = vec![(face.access, &access)];
    if let Some(r) = refresh.as_ref() {
        secrets.push((face.refresh, r));
    }
    let doc = merge_tokens(&disk.doc, face.section, &secrets, &plain);
    store::write_tokens(dir, face, &doc).map_err(Unusable::WriteFailed)?;
    Ok(access)
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/oauth/oauth_tests.rs"]
mod tests;
