//! cc-bus 信封：**唯一一处校验**（`bus-envelope`）。`cc-send` 发之前把要发的那一封交来，这里核完、盖上 id 与时间，
//! 交回规范信封（收件箱里存的就是它）；不合格当场拒，一条校验一个码。脚本那一侧不再判任何一格。
//!
//! 各格的规矩：
//! - `from` / `to`：只许会话 id（值域是 [`cmd_wire::Sid`] 那一份，不另写）。
//! - `tier`：1 ⇒ 敲门（对方闲着才送）；2 ⇒ 只进收件箱。缺 ⇒ 2。
//! - `act`：问 · 答 · 请 · 告知 · 交回 · 更正（[`ACTS`]）；答 · 交回 · 更正必带 `re`（[`NEEDS_RE`]）。
//! - `title`：必填、一行、不超过 [`TITLE_MAX_CHARS`] 字。
//! - `re`：消息 id 的形状（[`msg_id_ok`]，就是 [`seal`] 盖出来的那一形）。
//! - `refs`：只核前缀形状（路径 · `pb:<片>/<格号>` · `msg:<id>` · `commit:<hash>` · URL）；cc-bus 不认得 pb，不查那一格在不在。
//! - 正文：不超过 [`TEXT_MAX_BYTES`] 字节，长的写成文件、给路径。正文怎么写的纪律只写进 skill，这里不核。

use super::cc_bus_wire::{BusEnvelope, BusEnvelopeReq};

/// 拒的那一对：码 ＋ 原话（哪一格、什么值；给人看的那一句由码表 `envelope_said` 出）。
pub(crate) type Refusal = (&'static str, String);

/// 六个词。
pub(crate) const ACTS: &[&str] = &["问", "答", "请", "告知", "交回", "更正"];
/// 必带 `re` 的那三个。
pub(crate) const NEEDS_RE: &[&str] = &["答", "交回", "更正"];
/// 标题的上界（按字算）。
pub(crate) const TITLE_MAX_CHARS: usize = 80;
/// 正文的上界（按字节算）。
pub(crate) const TEXT_MAX_BYTES: usize = 1200;
/// `tier` 缺省。
pub(crate) const TIER_DEFAULT: u8 = 2;

/// 核过的那一封（还没盖 id 与时间）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Checked {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) tier: u8,
    pub(crate) act: String,
    pub(crate) title: String,
    pub(crate) re: Option<String>,
    pub(crate) refs: Vec<String>,
    pub(crate) text: String,
}

/// 消息 id 的形状：`m` 打头，后面 12..=24 位小写十六进制（[`seal`] 盖的就是这一形）。
pub(crate) fn msg_id_ok(s: &str) -> bool {
    s.strip_prefix('m').is_some_and(|h| {
        (12..=24).contains(&h.len()) && h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

fn sid_ok(s: &str) -> bool {
    cmd_wire::Sid::try_from(s.to_string()).is_ok()
}

/// 一条引用的前缀形状对不对（只核形状，不查指的东西在不在）。
fn ref_ok(r: &str) -> bool {
    if r.is_empty() || r.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    if let Some(rest) = r.strip_prefix("pb:") {
        return matches!(rest.split_once('/'), Some((a, b)) if !a.is_empty() && !b.is_empty() && !b.contains('/'));
    }
    if let Some(rest) = r.strip_prefix("msg:") {
        return msg_id_ok(rest);
    }
    if let Some(rest) = r.strip_prefix("commit:") {
        return (7..=40).contains(&rest.len()) && rest.bytes().all(|b| b.is_ascii_hexdigit());
    }
    if let Some(rest) = r.strip_prefix("https://").or_else(|| r.strip_prefix("http://")) {
        return !rest.is_empty();
    }
    // 余下当路径。认不出的 `前缀:`（`ftp://` · `foo:bar`）不当路径：冒号出现在第一个 `/` 之前就是一个没登记的前缀。
    match (r.find(':'), r.find('/')) {
        (Some(c), Some(s)) => c > s,
        (Some(_), None) => false,
        _ => true,
    }
}

/// 核一封信：照次序一格一格判，第一格不过就拒。
pub(crate) fn check(req: &BusEnvelopeReq) -> Result<Checked, Refusal> {
    if !sid_ok(&req.from) {
        return Err(("bad_from", format!("`from` is not a session id: {:?}", req.from)));
    }
    if !sid_ok(&req.to) {
        return Err(("bad_to", format!("`to` is not a session id: {:?}", req.to)));
    }
    let tier = req.tier.unwrap_or(TIER_DEFAULT);
    if !(1..=2).contains(&tier) {
        return Err(("bad_tier", format!("`tier` must be 1 or 2, got {tier}")));
    }
    if !ACTS.contains(&req.act.as_str()) {
        return Err((
            "bad_act",
            format!("`act` must be one of {ACTS:?}, got {:?}", req.act),
        ));
    }
    let title = req.title.trim();
    let chars = title.chars().count();
    if title.is_empty() || title.contains(['\n', '\r']) || chars > TITLE_MAX_CHARS {
        return Err((
            "bad_title",
            format!("`title` must be one non-empty line of at most {TITLE_MAX_CHARS} chars, got {chars} chars: {title:?}"),
        ));
    }
    let re = req.re.as_deref().map(str::trim).filter(|s| !s.is_empty());
    match re {
        None if NEEDS_RE.contains(&req.act.as_str()) => {
            return Err(("need_re", format!("`act` {:?} needs `re` (the message id it answers)", req.act)));
        }
        Some(r) if !msg_id_ok(r) => {
            return Err(("bad_re", format!("`re` is not a message id: {r:?}")));
        }
        _ => {}
    }
    let refs = req.refs.clone().unwrap_or_default();
    if let Some(bad) = refs.iter().find(|r| !ref_ok(r)) {
        return Err((
            "bad_ref",
            format!("`refs` item has no known shape (path · pb:<slice>/<cell> · msg:<id> · commit:<hash> · URL): {bad:?}"),
        ));
    }
    let text = req.text.clone().unwrap_or_default();
    if text.len() > TEXT_MAX_BYTES {
        return Err((
            "too_long",
            format!("body is {} bytes, limit {TEXT_MAX_BYTES}", text.len()),
        ));
    }
    Ok(Checked {
        from: req.from.clone(),
        to: req.to.clone(),
        tier,
        act: req.act.clone(),
        title: title.to_string(),
        re: re.map(str::to_string),
        refs,
        text,
    })
}

/// 盖 id 与时间 —— 纯函数。`now_ms` 是 UNIX 毫秒，`salt` 取低 16 位垫在后面（同一毫秒两封不撞）。
pub(crate) fn seal(c: Checked, now_ms: i64, salt: u64) -> BusEnvelope {
    let ms = now_ms.max(0);
    let secs = ms / 1000;
    let (y, mo, d) = crate::common::time::civil_from_days(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    BusEnvelope {
        id: format!("m{:x}{:04x}", ms, salt & 0xffff),
        ts: format!(
            "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}Z",
            s / 3600,
            s / 60 % 60,
            s % 60
        ),
        from: c.from,
        to: c.to,
        tier: c.tier,
        act: c.act,
        title: c.title,
        re: c.re,
        refs: c.refs,
        text: c.text,
    }
}

/// 生产那一份：此刻的时间 ＋ 一点盐（纳秒尾 ⊕ 进程号 ⊕ 本进程里的序号）。
pub(crate) fn envelope_now(req: &BusEnvelopeReq) -> Result<BusEnvelope, Refusal> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let checked = check(req)?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let salt = nanos ^ u64::from(std::process::id()) ^ SEQ.fetch_add(1, Ordering::Relaxed);
    Ok(seal(checked, crate::common::time::now_ms(), salt))
}

#[cfg(test)]
#[path = "../../../tests/backend/control/cc_bus_envelope_tests.rs"]
mod tests;
