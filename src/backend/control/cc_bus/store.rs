//! cc-bus 的收件箱：住这台后端的数据目录 `cc-bus/`，读写都只在这里。
//!
//! - `<会话 id>.jsonl`：那条会话的收件箱，一行一封规范信封（`bus-send` 盖好的那一份），只往后加。
//! - `state.json`：各会话的已读位置 ＋ 发信规矩的账（[`super::rules::Ledger`]）。
//!
//! 写：拿 `cc-bus/` 那一把目录锁，读—改—整份原子写（`Own`）。投一封 ＝ 锁里判规矩 → 收件箱加一行 → 写账；
//! 收件箱没写成 ⇒ 账也不写（投到了才记数）。推已读位置 ＝ 锁里比令牌（CAS）→ 写账。
//! 读：不拿锁（每份都是整份原子换的，读到的总是某一份完整的）。

use super::rules::Ledger;
use crate::common::said::Said;
use crate::control::cc_bus_wire::BusEnvelope;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 数据目录底下那一层。
pub(crate) const DIR: &str = "cc-bus";

/// 状态文件（已读位置 ＋ 账）：版本 · 上限照声明表。
const STATE: crate::common::own_state::Doc<State> =
    crate::common::own_state::Doc::of(&relay_route_core::CC_BUS_STATE_FILE);
const STATE_NAME: &str = relay_route_core::file_name_of(STATE.rel);

/// 一份收件箱（不带版本，上限照声明表）。
const INBOX: crate::common::own_state::Doc<Vec<u8>> =
    crate::common::own_state::Doc::of(&relay_route_core::CC_BUS_INBOX_FILE);

/// 状态文件那一形（顶层 `version` 归读写口）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct State {
    /// 各会话读到第几行（行号从 1 起；0 ＝ 一行没读）。
    #[serde(default)]
    pub(crate) pos: BTreeMap<String, u64>,
    #[serde(default)]
    pub(crate) ledger: Ledger,
}

/// 这台的收件箱目录（只读那几处用；写经 `Own`）。这台没有数据目录 ⇒ `None`。
pub(crate) fn dir_here() -> Option<PathBuf> {
    crate::platform::paths::data_home().map(|d| d.join(DIR))
}

/// 读出来的一段新信：令牌三格 ＋ 那几行原文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Peeked {
    /// 读那一刻的已读位置（令牌的 from，与盘上那个值逐字相等）。
    pub(crate) from: u64,
    /// 这一段的末行行号。
    pub(crate) to: u64,
    /// 末行的指纹。
    pub(crate) anchor: u64,
    /// 这一段的每一行（原文）。
    pub(crate) lines: Vec<String>,
}

impl Peeked {
    /// 两阶段读的令牌：`<from>.<to>.<末行指纹>`。
    pub(crate) fn token(&self) -> String {
        format!("{}.{}.{:016x}", self.from, self.to, self.anchor)
    }
}

/// 一行的指纹（令牌的末一格；与它比的是 commit 那一刻同一行现算的）。
fn anchor(line: &str) -> u64 {
    cmd_wire::fnv::fnv1a64(line.as_bytes())
}

/// 收件箱的路径。`sid` 先过了形状（会话 id，或读面那几条的 `bus_id_ok`），拼不出目录外。
fn inbox_of(dir: &Path, sid: &str) -> PathBuf {
    dir.join(format!("{sid}.jsonl"))
}

/// 读一份收件箱的每一行（末行没换行也算一行）。不在 ⇒ 空。
fn lines(dir: &Path, sid: &str) -> Result<Vec<String>, String> {
    use crate::common::own_state::Read;
    match INBOX.read_with(&inbox_of(dir, sid), Ok) {
        Read::Absent => Ok(vec![]),
        Read::Present(b) => Ok(String::from_utf8_lossy(&b)
            .split_terminator('\n')
            .map(str::to_string)
            .collect()),
        Read::Unreadable(why) => Err(why.said),
    }
}

/// 读状态文件。不在 ⇒ 空的那一份；读不出来 ⇒ 那一句（写的那几处据此不覆盖）。
fn state(dir: &Path) -> Result<State, String> {
    use crate::common::own_state::Read;
    match STATE.read(&dir.join(STATE_NAME)) {
        Read::Absent => Ok(State::default()),
        Read::Present(s) => Ok(s),
        Read::Unreadable(why) => Err(why.said),
    }
}

/// 起点：已读位置比行数大（收件箱被截短）⇒ 从头读。
fn start(pos: u64, total: u64) -> u64 {
    if pos > total {
        0
    } else {
        pos
    }
}

fn io(why: impl Into<String>) -> Said {
    Said::new("io_failed", why.into())
}

/// 拿 `cc-bus/` 那一把锁（目录不在先建：数据目录 ＋ 这一层）。
fn locked(
    own: &crate::platform::Own,
) -> Result<(crate::platform::fs::OwnPath, crate::platform::fs::OwnLock), Said> {
    let root = own
        .root()
        .ok_or_else(|| io(copy_core::copy_text("beOwnState.path.noHome", &[])))?;
    let dir = root.join(DIR).map_err(|e| io(e.said))?;
    for d in [&root, &dir] {
        own.ensure_dir(d).map_err(|e| {
            io(copy_core::copy_text(
                "reason.fs.writeFailed",
                &[
                    ("path", &d.as_path().display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ))
        })?;
    }
    let held = own.lock(&dir).map_err(|e| io(Said::from(e).said))?;
    Ok((dir, held))
}

/// 锁里写回状态文件。
fn put_state(
    own: &crate::platform::Own,
    dir: &crate::platform::fs::OwnPath,
    held: &crate::platform::fs::OwnLock,
    s: &State,
) -> Result<(), Said> {
    STATE.write(own, dir, held, s).map_err(|e| io(e.said))
}

/// 投一封：锁里判规矩，收 ⇒ 收件人的收件箱加一行、记账；规矩说要敲 ⇒ 放锁之后敲门（[`super::knock`]），回敲进去了没有。
/// 拒 ⇒ 码 ＋ 原话，一个字节不写。
pub(crate) fn deliver(
    own: &crate::platform::Own,
    env: &BusEnvelope,
    now_ms: i64,
) -> Result<bool, Said> {
    let (dir, held) = locked(own)?;
    let mut s = state(dir.as_path()).map_err(io)?;
    let knock = super::rules::admit(&mut s.ledger, env, now_ms)?;
    let line = serde_json::to_string(env).map_err(|e| io(e.to_string()))?;
    let mut body = lines(dir.as_path(), &env.to).map_err(io)?.join("\n");
    if !body.is_empty() {
        body.push('\n');
    }
    body.push_str(&line);
    body.push('\n');
    let inbox = dir
        .join(&format!("{}.jsonl", env.to))
        .map_err(|e| io(e.said))?;
    own.write(&inbox, &held, body.as_bytes())
        .map_err(|e| io(e.said))?;
    put_state(own, &dir, &held, &s)?;
    drop(held);
    Ok(knock && super::knock(own, env, now_ms))
}

/// 拆令牌：`<from>.<to>.<16 位十六进制>`，`to` 至少是 1。
fn parse_token(t: &str) -> Option<(u64, u64, u64)> {
    let mut it = t.split('.');
    let from = it.next()?.parse().ok()?;
    let to: u64 = it.next()?.parse().ok()?;
    let a = it.next()?;
    if it.next().is_some() || a.len() != 16 || to == 0 {
        return None;
    }
    Some((from, to, u64::from_str_radix(a, 16).ok()?))
}

/// 两阶段读的第二跳：令牌对得上 ⇒ 把那条会话的已读位置推到 `to`，回推到了第几行。
/// 盘上的位置不是令牌的 from ⇒ `stale`（别人推过了：手上那段照常处理）；第 to 行变了 ⇒ `changed`；令牌坏 ⇒ `bad_token`。
pub(crate) fn commit(own: &crate::platform::Own, sid: &str, token: &str) -> Result<u64, Said> {
    let (from, to, want) = parse_token(token)
        .ok_or_else(|| Said::new("bad_token", format!("not a peek token: {token:?}")))?;
    let (dir, held) = locked(own)?;
    let mut s = state(dir.as_path()).map_err(io)?;
    let rows = lines(dir.as_path(), sid).map_err(io)?;
    let total = rows.len() as u64;
    let cur = s.pos.get(sid).copied().unwrap_or(0);
    if cur != from {
        return Err(Said::new(
            "stale",
            format!("token says from={from}, the position is already {cur}"),
        ));
    }
    let now = rows.get(to as usize - 1).map(|l| anchor(l));
    if now != Some(want) {
        return Err(Said::new(
            "changed",
            format!("line {to} is not the one the token saw (inbox has {total} lines)"),
        ));
    }
    if to <= cur && cur <= total {
        return Err(Said::new(
            "bad_token",
            format!("to={to} is not past from={cur} and the inbox ({total} lines) was not cut"),
        ));
    }
    s.pos.insert(sid.to_string(), to);
    put_state(own, &dir, &held, &s)?;
    Ok(to)
}

/// 两阶段读的第一跳：纯读。没有新的 ⇒ `None`。`max` ＝ 这一段最多几行。
pub(crate) fn peek(dir: &Path, sid: &str, max: Option<usize>) -> Result<Option<Peeked>, String> {
    let rows = lines(dir, sid)?;
    let total = rows.len() as u64;
    let pos = state(dir)?.pos.get(sid).copied().unwrap_or(0);
    let begin = start(pos, total);
    if total <= begin {
        return Ok(None);
    }
    let to = match max {
        Some(m) => (begin + m as u64).min(total),
        None => total,
    };
    let lines = rows[begin as usize..to as usize].to_vec();
    Ok(Some(Peeked {
        from: pos,
        to,
        anchor: anchor(&rows[to as usize - 1]),
        lines,
    }))
}

/// 未读几封（收件箱行数 − 已读位置；位置比行数大 ⇒ 按 0）。
pub(crate) fn unread(dir: &Path, sid: &str) -> Result<u64, String> {
    let total = lines(dir, sid)?.len() as u64;
    let pos = state(dir)?.pos.get(sid).copied().unwrap_or(0);
    Ok(total - start(pos, total))
}

/// 名单：每个会话 id 形状的收件箱一行（会话 id · 未读）。目录不在 ⇒ 空。
pub(crate) fn inboxes(dir: &Path) -> Result<Vec<(String, u64)>, String> {
    let rd = match crate::platform::fs::Ro::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => {
            return Err(copy_core::copy_text(
                "reason.fs.readFailed",
                &[
                    ("path", &dir.display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ))
        }
    };
    let mut out = vec![];
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(sid) = name.strip_suffix(".jsonl") else {
            continue;
        };
        if cmd_wire::Sid::try_from(sid.to_string()).is_err() {
            continue;
        }
        out.push((sid.to_string(), unread(dir, sid)?));
    }
    Ok(out)
}

/// 收件箱末 `n` 行（原文，只读，不推位置）。
pub(crate) fn tail(dir: &Path, sid: &str, n: usize) -> Result<Vec<String>, String> {
    let rows = lines(dir, sid)?;
    Ok(rows[rows.len().saturating_sub(n)..].to_vec())
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/cc_bus_store_tests.rs"]
mod tests;
