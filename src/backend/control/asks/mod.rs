//! **待答登记表**（常驻后端进程里一份，通用、不认是哪一家）：会话里弹批准 / 提问 / 批计划的框时，那一家的答题 hook
//! 经 `needs-ask` 把请求挂进来、阻塞等；任一出口（桌面 · 手机）经 `needs-answer` 答，**第一个答案落下**，其余回 `already_answered`；
//! hook 那条连接断了（终端那边先答了 · 被打断 · 到点）⇒ 撤掉，之后再答回 `closed_elsewhere`。挂上 · 撤掉 · 答掉都推 `changed {needs, key: sid}`。
//!
//! 成品挂在会话的 `needs` 上（`observe::facts_query::dress_ask`）：`needs.ask` 是「这一条可以怎么答」—— 选项的字、语气、要不要附一句话都由这里写好，出口不按工具名取字；
//! pidfile 说在等、却没有挂着的请求（用户自己起的会话 · 只能本人答的框 · 工作区信任框 …）⇒ `ask: null`，`manualText` 写好「只能在终端里答」。
//!
//! 那一家的字段名不进这里：读 hook 输入 · 写决定都经注册表 `Adapter.asks`（[`crate::agents::AskFace`]）。

pub mod hook;

use crate::agents::asks::{product_of, Ask, NoteNeed};
use crate::agents::{AskIn, PlanMode, Verdict};
use copy_core::copy_text;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// `needs-ask` 的应答：人的答案写成那一家要的那一整份（hook 原样打出去）；`null` ＝ 不给决定。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct AskReply {
    #[cfg_attr(test, ts(type = "unknown"))]
    pub(crate) decision: Option<Value>,
}

/// `needs-ask` 的入参：哪一家（线上的 kind）＋ 那一家的 hook 交来的那一份原样。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct AskReq {
    pub(crate) agent: String,
    #[cfg_attr(test, ts(type = "unknown"))]
    pub(crate) hook: Value,
}

crate::wire::typed::codes! {
    /// `needs-ask` 没有别的码（不是要人答的那一种 ⇒ `{decision: null}`，不是失败）。
    pub(crate) enum AskErr {}
}

/// `needs-ask`（帧面硬臂：挂的那条跟着本连接走；本体真异步，`cancel` 打断 ＝ 不给决定）。
pub(crate) struct NeedsAsk;

impl crate::wire::typed::Command for NeedsAsk {
    const NAME: &'static str = "needs-ask";
    type Req = AskReq;
    type Reply = AskReply;
    type Err = AskErr;
}

/// `needs-answer` 的应答。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Answered {
    /// 恒 `answered`。
    pub(crate) result: AnswerResult,
}

/// `needs-answer` 成功那一形（闭集一个词：失败各走自己的码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) enum AnswerResult {
    Answered,
}

/// 一条挂着的请求。
struct Open {
    agent: String,
    ask: AskIn,
    product: Ask,
    conn: u64,
    /// 交回 hook 那一头：`Some(决定)` ＝ 打出去 · `None` ＝ 空手退（改到终端里答）。
    tx: tokio::sync::oneshot::Sender<Option<Value>>,
}

/// 收了场的那几条（「已经有人答过」与「在别处关掉了」分得清）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Settled {
    Answered,
    Closed,
}

#[derive(Default)]
struct Book {
    open: BTreeMap<String, Open>,
    settled: VecDeque<(String, Settled)>,
}

/// 收了场的记几条（够分「答过 / 关了」；再早的答回 `not_found`）。
const SETTLED_KEEP: usize = 256;

impl Book {
    fn settle(&mut self, id: String, how: Settled) {
        self.settled.push_back((id, how));
        while self.settled.len() > SETTLED_KEEP {
            self.settled.pop_front();
        }
    }
    fn settled(&self, id: &str) -> Option<Settled> {
        self.settled
            .iter()
            .rev()
            .find(|(i, _)| i == id)
            .map(|(_, h)| *h)
    }
}

fn book() -> std::sync::MutexGuard<'static, Book> {
    static BOOK: std::sync::OnceLock<Mutex<Book>> = std::sync::OnceLock::new();
    BOOK.get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// 某条会话挂着的请求变了（挂上 · 撤掉 · 答掉）⇒ 那个会话 id（流的 tap 订它推 `changed {needs, key: sid}`）。
pub(crate) fn changes() -> &'static tokio::sync::broadcast::Sender<String> {
    static TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<String>> =
        std::sync::OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<String>(64).0)
}

fn moved(sid: &str) {
    let _ = changes().send(sid.to_string());
}

fn now_ms() -> u64 {
    crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// 一条连接（hook 那一头）挂的请求跟着它走：它一丢 ⇒ 它挂的全撤掉（入方向读循环每条连接一份）。
pub(crate) struct Desk {
    conn: u64,
}

impl Desk {
    pub(crate) fn new() -> Desk {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Desk {
            conn: NEXT.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// 这条连接的号（交给 `needs-ask` 的处理器：处理器不拿着 [`Desk`] 本身，否则它丢不掉）。
    pub(crate) fn conn(&self) -> u64 {
        self.conn
    }
}

impl Drop for Desk {
    fn drop(&mut self) {
        withdraw_conn(self.conn);
    }
}

/// `needs-ask` 的本体：`{agent, hook}` ⇒ 挂上、等人答 ⇒ `{decision}`（`null` ＝ 不给决定，hook 空手退）。
/// 那一家没有答题口 · hook 输入不是要人答的那一种 ⇒ 立刻 `{decision: null}`（不挂）。
pub(crate) async fn ask(conn: u64, req: AskReq) -> AskReply {
    let Some(asked) = crate::agents::ask_face(&req.agent).and_then(|f| (f.read)(&req.hook)) else {
        return AskReply { decision: None };
    };
    let rx = open(&req.agent, asked, conn);
    // 撤掉（连接断了 / 别处关了）⇒ 发端丢了 ⇒ 不给决定。
    AskReply {
        decision: rx.await.ok().flatten(),
    }
}

/// 挂上一条（`conn` ＝ 它那条连接）。
pub(crate) fn open(
    agent: &str,
    asked: AskIn,
    conn: u64,
) -> tokio::sync::oneshot::Receiver<Option<Value>> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = format!("ask-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let product = product_of(&id, &asked, now_ms());
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<Value>>();
    let sid = asked.sid.clone();
    book().open.insert(
        id,
        Open {
            agent: agent.to_string(),
            ask: asked,
            product,
            conn,
            tx,
        },
    );
    moved(&sid);
    rx
}

/// 这条连接挂的全撤掉（之后再答 ⇒ `closed_elsewhere`）。
pub(crate) fn withdraw_conn(conn: u64) {
    let gone: Vec<(String, String)> = {
        let mut b = book();
        let ids: Vec<String> = b
            .open
            .iter()
            .filter(|(_, o)| o.conn == conn)
            .map(|(i, _)| i.clone())
            .collect();
        let mut out = Vec::new();
        for id in ids {
            if let Some(o) = b.open.remove(&id) {
                out.push((id.clone(), o.ask.sid));
                b.settle(id, Settled::Closed);
            }
        }
        out
    };
    for (_, sid) in gone {
        moved(&sid);
    }
}

/// 某条会话此刻挂着的请求（挂上的先后）。
pub(crate) fn pending(sid: &str) -> Vec<Ask> {
    let mut v: Vec<Ask> = book()
        .open
        .values()
        .filter(|o| o.ask.sid == sid)
        .map(|o| o.product.clone())
        .collect();
    v.sort_by_key(|a| a.since_ms);
    v
}

/// 此刻有请求挂着的会话。
pub(crate) fn sids() -> Vec<String> {
    let mut v: Vec<String> = book().open.values().map(|o| o.ask.sid.clone()).collect();
    v.sort();
    v.dedup();
    v
}

type Refusal = (&'static str, String);

fn refuse(code: &'static str, said: String) -> Refusal {
    (code, said)
}

/// 选的那一项 ⇒ 通用答案（`None` ＝ 改到终端里答：不给决定）。闭集：approve 那几项 `allow` · `always-<i>` · `deny` · `deny-note` · `deny-stop`；
/// plan `plan-auto` · `plan-edits` · `plan-manual` · `plan-keep`；question `answer` · `dismiss`；挡着终端的另有 `release`。
pub(crate) fn verdict_of(
    a: &AskIn,
    product: &Ask,
    choice: &str,
    note: Option<&str>,
    picks: Option<&BTreeMap<String, Pick>>,
) -> Result<Option<Verdict>, Refusal> {
    let spec = product
        .choices
        .iter()
        .find(|c| c.id == choice)
        .ok_or_else(|| refuse("bad_choice", copy_text("beAsk.answer.badChoice", &[])))?;
    let note = note.map(str::trim).filter(|n| !n.is_empty());
    if spec.note == NoteNeed::Required && note.is_none() {
        return Err(refuse(
            "note_required",
            copy_text("beAsk.answer.noteRequired", &[]),
        ));
    }
    let allow = |rule, mode| Verdict::Allow {
        rule,
        mode,
        answers: Vec::new(),
    };
    Ok(Some(match choice {
        "release" => return Ok(None),
        "allow" => allow(None, None),
        "deny" | "dismiss" => Verdict::Deny {
            note: None,
            stop: false,
        },
        "deny-note" => Verdict::Deny {
            note: note.map(str::to_string),
            stop: false,
        },
        "deny-stop" => Verdict::Deny {
            note: None,
            stop: true,
        },
        "plan-auto" => allow(None, Some(PlanMode::Auto)),
        "plan-edits" => allow(None, Some(PlanMode::AcceptEdits)),
        "plan-manual" => allow(None, Some(PlanMode::Manual)),
        "plan-keep" => Verdict::Deny {
            note: Some(note.map_or_else(|| copy_text("beAsk.planKeep.note", &[]), str::to_string)),
            stop: false,
        },
        "answer" => Verdict::Allow {
            rule: None,
            mode: None,
            answers: answers_of(product, picks)?,
        },
        c => match c
            .strip_prefix("always-")
            .and_then(|i| i.parse::<usize>().ok())
        {
            Some(i) if i < a.suggestions.len() => allow(Some(i), None),
            _ => {
                return Err(refuse(
                    "bad_choice",
                    copy_text("beAsk.answer.badChoice", &[]),
                ))
            }
        },
    }))
}

/// `picks` ＝ `{<题 id>: {options: [<选项 id>…], text: <自由文本>}}`；每题都要有答案，单选至多一项；
/// 多选几项用「, 」连（那一家框里多选的写法），自由文本接在后面。
fn answers_of(
    product: &Ask,
    picks: Option<&BTreeMap<String, Pick>>,
) -> Result<Vec<(String, String)>, Refusal> {
    let bad = || refuse("bad_args", copy_text("beAsk.answer.picks", &[]));
    let picks = picks.ok_or_else(bad)?;
    let mut out = Vec::new();
    for q in &product.questions {
        let p = picks.get(&q.id).ok_or_else(bad)?;
        let mut parts: Vec<String> = Vec::new();
        for o in &p.options {
            let opt = q.options.iter().find(|x| x.id == *o).ok_or_else(bad)?;
            parts.push(opt.text.clone());
        }
        if let Some(t) = p.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            parts.push(t.to_string());
        }
        if parts.is_empty() || (!q.multi && parts.len() > 1) {
            return Err(bad());
        }
        out.push((q.text.clone(), parts.join(", ")));
    }
    Ok(out)
}

/// `needs-answer` 的入参。
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct AnswerReq {
    /// 会话 id。
    pub(crate) sid: String,
    /// `needs.ask.id`。
    pub(crate) ask: String,
    /// `needs.ask.choices[].id`。
    pub(crate) choice: String,
    /// 可缺：附的那句话（选项 `note` 是 required 时必给）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) note: Option<String>,
    /// 提问那一种选 `answer` 时必给：题 id ⇒ 那一题的答，每题都要答、单选至多一项。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) picks: Option<BTreeMap<String, Pick>>,
}

/// 提问那一种里一题的答：选了哪几项 ＋ 自由文本。
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[cfg_attr(test, ts(rename = "AskPick"))] // 不叫 `Pick`：TS 里那是内建的工具类型。
#[serde(deny_unknown_fields)]
pub(crate) struct Pick {
    /// 选项 id（`needs.ask.questions[].options[].id`）；缺 ⇒ 没选。
    #[serde(default)]
    #[cfg_attr(test, ts(as = "Option<Vec<String>>", optional))]
    pub(crate) options: Vec<String>,
    /// 自由文本；缺 / 空白 ⇒ 没写。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) text: Option<String>,
}

crate::wire::typed::codes! {
    /// `needs-answer` 的码。
    pub(crate) enum AnswerErr {
        /// 没这一条（或不是这个会话的）。
        NotFound = "not_found",
        /// 别处先答了。
        AlreadyAnswered = "already_answered",
        /// hook 那头已经走了（终端那边答了 · 被打断）。
        ClosedElsewhere = "closed_elsewhere",
        /// 选项不在这一条的选项里 / 那一家写不出这个决定。
        BadChoice = "bad_choice",
        /// 选项要附一句话，没给。
        NoteRequired = "note_required",
    }
}

/// `needs-answer`：答一条挂着的待答请求，第一个答案落下。锁一下登记表、交给 hook 那一头的通道：不碰盘、不起进程、不等 ⇒ 异步档。
pub(crate) struct NeedsAnswerCmd;

impl crate::wire::typed::Command for NeedsAnswerCmd {
    const NAME: &'static str = "needs-answer";
    type Req = AnswerReq;
    type Reply = Answered;
    type Err = AnswerErr;
}

impl crate::wire::typed::Handle for NeedsAnswerCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: AnswerReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Answered, AnswerErr>> + Send + 'static {
        std::future::ready(answer(req).map_err(crate::wire::typed::of_pair(AnswerErr::NotFound)))
    }
}

/// `needs-answer` 的本体 ⇒ `{result: "answered"}`；
/// 别处先答了 ⇒ `already_answered`；hook 那头已经走了（终端那边答了 · 被打断）⇒ `closed_elsewhere`；没这一条 ⇒ `not_found`。
pub(crate) fn answer(req: AnswerReq) -> Result<Answered, Refusal> {
    let (sid, id, choice) = (req.sid.as_str(), req.ask.as_str(), req.choice.as_str());
    let mut b = book();
    let Some(o) = b.open.get(id).filter(|o| o.ask.sid == sid) else {
        return Err(match b.settled(id) {
            Some(Settled::Answered) => {
                refuse("already_answered", copy_text("beAsk.answer.already", &[]))
            }
            Some(Settled::Closed) => {
                refuse("closed_elsewhere", copy_text("beAsk.answer.closed", &[]))
            }
            None => refuse("not_found", copy_text("beAsk.answer.notFound", &[])),
        });
    };
    let verdict = verdict_of(
        &o.ask,
        &o.product,
        choice,
        req.note.as_deref(),
        req.picks.as_ref(),
    )?;
    let decision = match &verdict {
        Some(v) => match crate::agents::ask_face(&o.agent).and_then(|f| (f.decide)(&o.ask, v)) {
            Some(d) => Some(d),
            None => {
                return Err(refuse(
                    "bad_choice",
                    copy_text("beAsk.answer.badChoice", &[]),
                ))
            }
        },
        None => None,
    };
    let o = b.open.remove(id).expect("刚找到的那一条");
    let delivered = o.tx.send(decision).is_ok();
    b.settle(
        id.to_string(),
        if delivered {
            Settled::Answered
        } else {
            Settled::Closed
        },
    );
    drop(b);
    moved(sid);
    if delivered {
        Ok(Answered {
            result: AnswerResult::Answered,
        })
    } else {
        Err(refuse(
            "closed_elsewhere",
            copy_text("beAsk.answer.closed", &[]),
        ))
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/asks_tests.rs"]
mod tests;
