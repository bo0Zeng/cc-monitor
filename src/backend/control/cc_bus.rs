//! cc-bus 的读面（`bus-list` · `bus-inbox` · `bus-peek` · `bus-unread`）与两阶段读的第二跳（`bus-commit`）。
//! 都问收件箱那一处（[`store`]）：收件箱住这台后端的数据目录，读写只在那里，入口脚本不读不写。
//! 发信那一条（`bus-send`）的本体在 [`super::cc_bus_wire`]：核信封（[`super::cc_bus_envelope`]）→ 范围（[`super::cc_bus_scope`]）→
//! 规矩与落盘（[`rules`] · [`store`]）。
//!
//! - `bus-peek`：纯读，交给 agent 读的那一段（这里照信封写好）＋ 令牌；不推已读位置。
//! - `bus-commit`：拿 `bus-peek` 的令牌在锁里推已读位置（CAS）。`cc-bus recv` ＝ peek 接 commit；Stop 钩子喂回成功之后才 commit。
//! - `bus-inbox`：界面只读看收件箱尾巴（不推位置，不是 recv）；`bus-list`：名单 ＋ 待读。

pub(crate) mod rules;
pub(crate) mod store;

use copy_core::copy_text;

use super::cc_bus_wire::{
    BusAgent, BusCommitReq, BusCommitted, BusEnvelope, BusInbox, BusInboxItem, BusInboxReq,
    BusList, BusPeekReq, BusPeeked, BusUnread, BusUnreadReq,
};

/// 敲门那一句：给收件那条 agent 的输入，只填发信人（会话 id，信封核过形状）；标题与正文不进去 —— 进去就成了对方的指令，
/// 正文只经 Stop 钩子喂回。不是给人看的话，不进文案表。
fn knock_text(from: &str) -> String {
    format!("🔔 cc-bus: 你有来自 {from} 的新消息,运行 cc-bus recv 读取并按内容处理")
}

/// 敲门：往收件那条会话的输入队（这一台，会话的主人）投一句，发自 `cc-bus`，id 由那一封的 id 来（同一封重投只入队一次）；
/// 什么时候送进去归投递那一侧（闲着才送 · 在等人不送 · 一次一条）。投不进 ⇒ 记一行、回 `false`：信已在收件箱里，轮末 Stop 钩子照样喂回。
pub(crate) fn knock(own: &crate::platform::Own, env: &BusEnvelope, now_ms: i64) -> bool {
    let Ok(sid) = cmd_wire::Sid::try_from(env.to.clone()) else {
        tracing::info!("cc-bus knock skipped: {} is not a session id", env.to);
        return false;
    };
    let req = cmd_wire::queue::SessionQueuePutReq {
        machine: cmd_wire::Machine::local(),
        sid,
        id: format!("cc-bus-{}", env.id),
        end: "cc-bus".to_string(),
        n: None,
        at_ms: now_ms,
        text: knock_text(&env.from),
        attachments: vec![],
        rewind: None,
    };
    let put = crate::history::session_queue::put(own, &req, now_ms);
    crate::history::session_queue::moved();
    match put {
        Ok(_) => true,
        Err(f) => {
            tracing::info!("cc-bus knock not queued for {}: {}", env.to, f.code);
            false
        }
    }
}

/// 命令级错误：`(code, message)`。
use crate::common::said::Said;

/// 读面要的那个目录：这台没有数据目录 ⇒ 照实说。
fn here() -> Result<std::path::PathBuf, Said> {
    store::dir_here().ok_or_else(|| {
        (
            "failed".to_string(),
            copy_text("beOwnState.path.noHome", &[]),
        )
    })
}

/// 〔`INVARIANTS §47` ①〕读面收的 id 先过形状判定（`shell_quote_core::bus_id_ok`：非空 · 不以 `-` 开头 · 只含 `[A-Za-z0-9_-]`，
/// 全仓唯一一份）：id 要拼进收件箱的路径。判不过 ⇒ `bad_id`。
fn refuse_bad_bus_id(v: &str, said: impl FnOnce(&str) -> String) -> Result<(), Said> {
    if shell_quote_core::bus_id_ok(v) {
        return Ok(());
    }
    Err(crate::common::said::Said::new("bad_id", said(&format!("{v:?}"))))
}

/// 后端对一条会话判的那两样（判官住能读观测层的帧面宿主，`faces::bus_list_face`）：叫它什么 · 此刻有没有谁持着它。
/// 这台没有它的记录 · 那一家判不了活 ⇒ 对应那一格是 `None`（不是「不在」）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Seen {
    pub(crate) title: Option<String>,
    pub(crate) live: Option<bool>,
}

/// 总线名单 —— `bus-list` 读它。行是这台的收件箱（[`store::inboxes`]）；叫它什么 · 在不在问 `seen`（后端那一处）。
pub(crate) fn list_for_inbound(seen: &dyn Fn(&str) -> Seen) -> Result<BusList, Said> {
    let mut rows = store::inboxes(&here()?).map_err(|m| ("failed".to_string(), m))?;
    rows.sort();
    Ok(list_reply(roster_agents(&rows, seen)))
}

/// 名单行（会话 id · 待读）→ `bus-list` 那一格（纯函数）。标题判不出 ⇒ 照实就是会话 id；在不在判不出 ⇒ `null`（界面说「查不到」）。
pub(crate) fn roster_agents(rows: &[(String, u64)], seen: &dyn Fn(&str) -> Seen) -> Vec<BusAgent> {
    rows.iter()
        .map(|(id, unread)| {
            let s = seen(id);
            BusAgent {
                id: id.clone(),
                title: s.title.unwrap_or_else(|| id.clone()),
                unread: *unread as i64,
                live: s.live,
            }
        })
        .collect()
}

/// `bus-list` 的成品 `{agents}`：跨语言金样 `tests/__fixtures__/cc-bus-control.golden.json` 拿同一个构造器对拍。
pub(crate) fn list_reply(agents: Vec<BusAgent>) -> BusList {
    BusList { agents }
}

/// `bus-inbox` 缺省看几行 / 最多看几行 / 回显最多带多少字节（超了保尾、说 `truncated`）。
const INBOX_LINES_DEFAULT: u64 = 200;
const INBOX_LINES_MAX: u64 = 2000;
const INBOX_CAP: usize = 4 * 1024 * 1024;

/// `bus-inbox` 的入参 —— 纯函数。id 先过形状判定（拒码 `bad_id`）。
pub(crate) fn parse_inbox(req: &BusInboxReq) -> Result<(String, u64), Said> {
    if req.id.is_empty() {
        return Err(crate::common::said::Said::new(
            "bad_args",
            crate::common::contract::malformed("missing `id`"),
        ));
    }
    refuse_bad_bus_id(&req.id, |v| copy_text("beCcBus.inbox.badId", &[("id", v)]))?;
    let lines = match req.lines {
        None => INBOX_LINES_DEFAULT,
        Some(n) if (1..=INBOX_LINES_MAX).contains(&n) => n,
        Some(_) => {
            return Err(crate::common::said::Said::new(
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "`lines` must be an integer in 1..={INBOX_LINES_MAX}"
                )),
            ))
        }
    };
    Ok((req.id.clone(), lines))
}

/// `bus-inbox`：只读看一个收件箱的尾巴（不推已读位置）。
pub(crate) fn inbox_for_inbound(req: &BusInboxReq) -> Result<BusInbox, Said> {
    let (id, lines) = parse_inbox(req)?;
    let rows = store::tail(&here()?, &id, lines as usize).map_err(|m| ("failed".to_string(), m))?;
    Ok(inbox_reply(&rows))
}

/// 收件箱的那几行 → `bus-inbox` 成品 —— 纯函数。逐行解析（坏行跳过计数、不因坏行丢好行）；超上限保尾并说 `truncated`。
pub(crate) fn inbox_reply(rows: &[String]) -> BusInbox {
    let mut kept = 0usize;
    let from = rows
        .iter()
        .rposition(|l| {
            kept += l.len() + 1;
            kept > INBOX_CAP
        })
        .map_or(0, |i| i + 1);
    let mut messages = Vec::new();
    let mut skipped = 0usize;
    for line in &rows[from..] {
        if line.trim().is_empty() {
            continue;
        }
        // 一行就是一封规范信封（`bus-send` 盖的那一份）；不是的（坏 JSON · 别的工具写的 · 少格）算坏行。
        match serde_json::from_str::<BusEnvelope>(line) {
            Ok(m) => messages.push(BusInboxItem {
                act_said: super::cc_bus_wire::act_said(m.act),
                envelope: m,
            }),
            Err(_) => skipped += 1,
        }
    }
    BusInbox {
        messages,
        skipped,
        truncated: from > 0,
    }
}

/// 读面收的会话 id（`bus-peek` · `bus-commit` · `bus-unread`）：同一道形状判定。
fn checked_id(id: &str) -> Result<(), Said> {
    refuse_bad_bus_id(id, |v| copy_text("beCcBus.inbox.badId", &[("id", v)]))
}

/// 给 agent 读的那一段 —— 纯函数。一封一段：头一行线上码（act · tier，不放显示用的词）· 标题 · 引用（有才印）· 正文 · 怎么回；
/// 读不懂的行跳过，末尾说跳过了几条。回 `(那一段, 几封, 跳过几条)`。
pub(crate) fn render(lines: &[String]) -> (String, u64, u64) {
    let mut out = String::new();
    let (mut shown, mut skipped) = (0u64, 0u64);
    for line in lines {
        let Ok(m) = serde_json::from_str::<BusEnvelope>(line) else {
            skipped += 1;
            continue;
        };
        shown += 1;
        let act = serde_json::to_value(m.act)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        out.push_str(&format!(
            "[cc-bus] id={} from={} ts={} tier={} act={act}",
            m.id, m.from, m.ts, m.tier
        ));
        if let Some(re) = &m.re {
            out.push_str(&format!(" re={re}"));
        }
        out.push_str(&format!("\ntitle: {}\n", m.title));
        if !m.refs.is_empty() {
            out.push_str(&format!("refs: {}\n", m.refs.join(" ")));
        }
        out.push_str(&m.text);
        out.push_str(&format!(
            "\n(reply: cc-bus send {} --act answer --re {} --title \"…\")\n\n",
            m.from, m.id
        ));
    }
    if skipped > 0 {
        out.push_str(&copy_text(
            "beCcBus.peek.skipped",
            &[("n", &skipped.to_string())],
        ));
        out.push('\n');
    }
    (out, shown, skipped)
}

/// `bus-peek`：纯读。没有新的 ⇒ 正文空、令牌 `null`。
pub(crate) fn peek_for_inbound(req: &BusPeekReq) -> Result<BusPeeked, Said> {
    checked_id(&req.id)?;
    let max = match req.max_lines {
        None => None,
        Some(n) if n >= 1 => Some(n as usize),
        Some(_) => {
            return Err(crate::common::said::Said::new(
                "bad_args",
                crate::common::contract::malformed("`max_lines` must be a positive integer"),
            ))
        }
    };
    let got = store::peek(&here()?, &req.id, max).map_err(|m| ("failed".to_string(), m))?;
    Ok(match got {
        None => BusPeeked {
            text: String::new(),
            token: None,
            count: 0,
            skipped: 0,
        },
        Some(p) => {
            let (text, count, skipped) = render(&p.lines);
            BusPeeked {
                text,
                token: Some(p.token()),
                count,
                skipped,
            }
        }
    })
}

/// `bus-commit`：拿令牌推已读位置。
pub(crate) fn commit_for_inbound(
    own: &crate::platform::Own,
    req: &BusCommitReq,
) -> Result<BusCommitted, Said> {
    checked_id(&req.id)?;
    store::commit(own, &req.id, &req.token)
        .map(|read| BusCommitted { read })
        .map_err(|(c, m)| (c.to_string(), m))
}

/// `bus-unread`：未读几封（只读）。
pub(crate) fn unread_for_inbound(req: &BusUnreadReq) -> Result<BusUnread, Said> {
    checked_id(&req.id)?;
    store::unread(&here()?, &req.id)
        .map(|unread| BusUnread { unread })
        .map_err(|m| ("failed".to_string(), m))
}

#[cfg(test)]
#[path = "../../../tests/backend/control/cc_bus_tests.rs"]
mod tests;
