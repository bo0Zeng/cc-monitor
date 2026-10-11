//! **待答登记表**（常驻后端进程里一份，通用、不认是哪一家）：会话里弹批准 / 提问 / 批计划的框时，那一家的答题 hook
//! 经 `needs-ask` 把请求挂进来、阻塞等；任一出口（桌面 · 手机）经 `needs-answer` 答，**第一个答案落下**，其余回 `already_answered`；
//! hook 那条连接断了（终端那边先答了 · 被打断 · 到点）⇒ 撤掉，之后再答回 `closed_elsewhere`。挂上 · 撤掉 · 答掉都推 `changed {needs, key: sid}`。
//!
//! 成品挂在会话的 `needs` 上（[`dress`]）：`needs.ask` 是「这一条可以怎么答」—— 选项的字、语气、要不要附一句话都由这里写好，出口不按工具名取字；
//! pidfile 说在等、却没有挂着的请求（用户自己起的会话 · 只能本人答的框 · 工作区信任框 …）⇒ `ask: null`，`manualText` 写好「只能在终端里答」。
//!
//! 那一家的字段名不进这里：读 hook 输入 · 写决定都经注册表 `Adapter.asks`（[`crate::agents::AskFace`]）。

pub mod hook;

use crate::agents::{AskIn, AskKind, PlanMode, Verdict};
use crate::common::cells::{Tone, Words};
use crate::observe::facts_query::{Needs, NeedsKind};
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// 一条待答请求的成品（`needs.ask`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Ask {
    /// 这一次请求的 id（后端发的，不是那一家的）：答的时候原样交回。
    pub(crate) id: String,
    /// 种类（同 `needs.kind` 那一套：approve · answer · plan）。
    pub(crate) kind: NeedsKind,
    /// 工具名原样。
    pub(crate) tool: String,
    /// 子 agent 发的 ⇒ 它的 id 与种类（那一家原样）；主会话 ⇒ `null`。
    pub(crate) agent_id: Option<String>,
    pub(crate) agent_type: Option<String>,
    /// 一行摘要（命令头一行 · 路径 · 第一道题）；说不出 ⇒ `null`。
    pub(crate) what: Option<String>,
    /// 要批的东西的全文（命令全文 · 写哪个文件写成什么 · 计划原文），不截。
    pub(crate) detail: String,
    /// 能选的几项，排好序（「以后不再问」那几项不放第一位）。
    pub(crate) choices: Vec<Choice>,
    /// 提问那一种的题目；别的种类为空。
    pub(crate) questions: Vec<Question>,
    /// 何时挂上的（epoch ms，这台的钟）。
    pub(crate) since_ms: u64,
}

/// 一个选项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Choice {
    /// 答的时候交回的那个词（闭集见 [`verdict_of`]）。
    pub(crate) id: String,
    pub(crate) text: Words,
    pub(crate) tone: Tone,
    /// 要不要附一句话：none · optional · required。
    pub(crate) note: NoteNeed,
}

/// 选项要不要附一句话。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum NoteNeed {
    None,
    Optional,
    Required,
}

/// 一道题（提问那一种）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Question {
    /// `q0` · `q1` …（答的时候 `picks` 按它键）。
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) header: Option<String>,
    pub(crate) multi: bool,
    pub(crate) options: Vec<QuestionOption>,
    /// 恒 `true`：每题都许写自由文本（那一家框里的「Type something」）。
    pub(crate) other: bool,
}

/// 一道题的一个选项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct QuestionOption {
    /// `q0o0` …
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) detail: Option<String>,
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
    TX.get_or_init(|| tokio::sync::broadcast::channel(64).0)
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
pub(crate) async fn ask(conn: u64, args: &Value) -> Result<Value, (&'static str, String)> {
    let agent = args.get("agent").and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed("needs-ask: `agent` must be a string"),
    ))?;
    let hook = args.get("hook").ok_or((
        "bad_args",
        crate::common::contract::malformed("needs-ask: `hook` is required"),
    ))?;
    let Some(asked) = crate::agents::ask_face(agent).and_then(|f| (f.read)(hook)) else {
        return Ok(serde_json::json!({ "decision": null }));
    };
    let rx = open(agent, asked, conn);
    // 撤掉（连接断了 / 别处关了）⇒ 发端丢了 ⇒ 不给决定。
    let decision = rx.await.ok().flatten();
    Ok(serde_json::json!({ "decision": decision }))
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
    let (tx, rx) = tokio::sync::oneshot::channel();
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

/// 成品：选项按种类写好字。
pub(crate) fn product_of(id: &str, a: &AskIn, since_ms: u64) -> Ask {
    let c = |id: &str, key: &str, tone: Tone, note: NoteNeed| Choice {
        id: id.to_string(),
        text: Words(copy_text(key, &[])),
        tone,
        note,
    };
    let mut choices = Vec::new();
    match a.kind {
        AskKind::Approve => {
            choices.push(c(
                "allow",
                "beAsk.choice.allow",
                Tone::Plain,
                NoteNeed::None,
            ));
            for (i, s) in a.suggestions.iter().enumerate() {
                choices.push(Choice {
                    id: format!("always-{i}"),
                    text: Words(copy_text("beAsk.choice.allowAlways", &[("rule", &s.text)])),
                    tone: Tone::Plain,
                    note: NoteNeed::None,
                });
            }
            choices.push(c("deny", "beAsk.choice.deny", Tone::Warn, NoteNeed::None));
            choices.push(c(
                "deny-note",
                "beAsk.choice.denyNote",
                Tone::Warn,
                NoteNeed::Required,
            ));
            choices.push(c(
                "deny-stop",
                "beAsk.choice.denyStop",
                Tone::Warn,
                NoteNeed::None,
            ));
        }
        AskKind::Plan => {
            choices.push(c(
                "plan-auto",
                "beAsk.choice.planAuto",
                Tone::Plain,
                NoteNeed::None,
            ));
            choices.push(c(
                "plan-edits",
                "beAsk.choice.planEdits",
                Tone::Plain,
                NoteNeed::None,
            ));
            choices.push(c(
                "plan-manual",
                "beAsk.choice.planManual",
                Tone::Plain,
                NoteNeed::None,
            ));
            choices.push(c(
                "plan-keep",
                "beAsk.choice.planKeep",
                Tone::Warn,
                NoteNeed::Optional,
            ));
        }
        AskKind::Question => {
            if !a.questions.is_empty() {
                choices.push(c(
                    "answer",
                    "beAsk.choice.answer",
                    Tone::Plain,
                    NoteNeed::None,
                ));
            }
            choices.push(c(
                "dismiss",
                "beAsk.choice.dismiss",
                Tone::Warn,
                NoteNeed::None,
            ));
        }
    }
    if a.blocks_terminal {
        choices.push(c(
            "release",
            "beAsk.choice.release",
            Tone::Plain,
            NoteNeed::None,
        ));
    }
    Ask {
        id: id.to_string(),
        kind: match a.kind {
            AskKind::Approve => NeedsKind::Approve,
            AskKind::Question => NeedsKind::Answer,
            AskKind::Plan => NeedsKind::Plan,
        },
        tool: a.tool.clone(),
        agent_id: a.agent_id.clone(),
        agent_type: a.agent_type.clone(),
        what: a.what.clone(),
        detail: a.detail.clone(),
        choices,
        questions: a
            .questions
            .iter()
            .enumerate()
            .map(|(qi, q)| Question {
                id: format!("q{qi}"),
                text: q.question.clone(),
                header: q.header.clone(),
                multi: q.multi,
                options: q
                    .options
                    .iter()
                    .enumerate()
                    .map(|(oi, o)| QuestionOption {
                        id: format!("q{qi}o{oi}"),
                        text: o.label.clone(),
                        detail: o.description.clone(),
                    })
                    .collect(),
                other: true,
            })
            .collect(),
        since_ms,
    }
}

/// 格目录的样本（每一格都有值：选项 · 题 · 选项说明 · 子 agent 那两格）。
pub(crate) fn specimen() -> Ask {
    product_of(
        "ask-1",
        &AskIn {
            sid: "s".into(),
            kind: AskKind::Question,
            tool: "t".into(),
            what: Some("w".into()),
            detail: "d".into(),
            suggestions: Vec::new(),
            questions: vec![crate::agents::AskQuestion {
                header: Some("h".into()),
                question: "q".into(),
                multi: false,
                options: vec![crate::agents::AskOption {
                    label: "l".into(),
                    description: Some("d".into()),
                }],
            }],
            agent_id: Some("a".into()),
            agent_type: Some("t".into()),
            blocks_terminal: true,
            input: Value::Null,
        },
        1,
    )
}

type Refusal = (&'static str, String);

fn refuse(code: &'static str, key: &str) -> Refusal {
    (code, copy_text(key, &[]))
}

/// 选的那一项 ⇒ 通用答案（`None` ＝ 改到终端里答：不给决定）。闭集：approve 那几项 `allow` · `always-<i>` · `deny` · `deny-note` · `deny-stop`；
/// plan `plan-auto` · `plan-edits` · `plan-manual` · `plan-keep`；question `answer` · `dismiss`；挡着终端的另有 `release`。
pub(crate) fn verdict_of(
    a: &AskIn,
    product: &Ask,
    choice: &str,
    note: Option<&str>,
    picks: Option<&Value>,
) -> Result<Option<Verdict>, Refusal> {
    let spec = product
        .choices
        .iter()
        .find(|c| c.id == choice)
        .ok_or_else(|| refuse("bad_choice", "beAsk.answer.badChoice"))?;
    let note = note.map(str::trim).filter(|n| !n.is_empty());
    if spec.note == NoteNeed::Required && note.is_none() {
        return Err(refuse("note_required", "beAsk.answer.noteRequired"));
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
            _ => return Err(refuse("bad_choice", "beAsk.answer.badChoice")),
        },
    }))
}

/// `picks` ＝ `{<题 id>: {options: [<选项 id>…], text: <自由文本>}}`；每题都要有答案，单选至多一项；
/// 多选几项用「, 」连（那一家框里多选的写法），自由文本接在后面。
fn answers_of(product: &Ask, picks: Option<&Value>) -> Result<Vec<(String, String)>, Refusal> {
    let bad = || refuse("bad_args", "beAsk.answer.picks");
    let picks = picks.and_then(Value::as_object).ok_or_else(bad)?;
    let mut out = Vec::new();
    for q in &product.questions {
        let p = picks.get(&q.id).ok_or_else(bad)?;
        let mut parts: Vec<String> = Vec::new();
        for o in p
            .get("options")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let o = o.as_str().ok_or_else(bad)?;
            let opt = q.options.iter().find(|x| x.id == o).ok_or_else(bad)?;
            parts.push(opt.text.clone());
        }
        if let Some(t) = p
            .get("text")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            parts.push(t.to_string());
        }
        if parts.is_empty() || (!q.multi && parts.len() > 1) {
            return Err(bad());
        }
        out.push((q.text.clone(), parts.join(", ")));
    }
    Ok(out)
}

/// `needs-answer` 的本体：`{sid, ask, choice, note?, picks?}` ⇒ `{result: "answered"}`；
/// 别处先答了 ⇒ `already_answered`；hook 那头已经走了（终端那边答了 · 被打断）⇒ `closed_elsewhere`；没这一条 ⇒ `not_found`。
pub(crate) fn answer(args: &Value) -> Result<Value, Refusal> {
    let s = |k: &str| args.get(k).and_then(Value::as_str);
    let (Some(sid), Some(id), Some(choice)) = (s("sid"), s("ask"), s("choice")) else {
        return Err((
            "bad_args",
            crate::common::contract::malformed(
                "needs-answer: `sid`, `ask`, `choice` must be strings",
            ),
        ));
    };
    let mut b = book();
    let Some(o) = b.open.get(id).filter(|o| o.ask.sid == sid) else {
        return Err(match b.settled(id) {
            Some(Settled::Answered) => refuse("already_answered", "beAsk.answer.already"),
            Some(Settled::Closed) => refuse("closed_elsewhere", "beAsk.answer.closed"),
            None => refuse("not_found", "beAsk.answer.notFound"),
        });
    };
    let verdict = verdict_of(&o.ask, &o.product, choice, s("note"), args.get("picks"))?;
    let decision = match &verdict {
        Some(v) => match crate::agents::ask_face(&o.agent).and_then(|f| (f.decide)(&o.ask, v)) {
            Some(d) => Some(d),
            None => return Err(refuse("bad_choice", "beAsk.answer.badChoice")),
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
        Ok(serde_json::json!({ "result": "answered" }))
    } else {
        Err(refuse("closed_elsewhere", "beAsk.answer.closed"))
    }
}

/// 会话的 `needs` 配上挂着的请求：有挂着的 ⇒ `ask` 是最早挂上的那一条（pidfile 没说在等 —— 挡着终端的那种 —— 就照它补出一份 `needs`）；
/// pidfile 说在等却没有挂着的 ⇒ `ask: null`、`manualText` 写好「只能在终端里答」。
pub(crate) fn dress(needs: Option<Needs>, sid: &str) -> Option<Needs> {
    dress_with(needs, pending(sid).into_iter().next(), now_ms())
}

/// [`dress`] 的纯那一半（判据喂夹具）。
pub(crate) fn dress_with(needs: Option<Needs>, ask: Option<Ask>, now_ms: u64) -> Option<Needs> {
    use crate::observe::facts_query as f;
    match (needs, ask) {
        (Some(mut n), Some(a)) => {
            n.manual_text = None;
            n.ask = Some(a);
            Some(n)
        }
        (Some(mut n), None) => {
            n.ask = None;
            n.manual_text = Some(Words(copy_text("beAsk.manual.terminal", &[])));
            Some(n)
        }
        (None, Some(a)) => {
            let kind = a.kind;
            let waited = now_ms.saturating_sub(a.since_ms);
            Some(Needs {
                kind,
                tool: Some(a.tool.clone()),
                call: None,
                what: a.what.clone(),
                since_ms: Some(a.since_ms),
                text: f::needs_words(kind),
                head: f::needs_head(kind, Some(&a.tool)),
                head_code: match kind {
                    NeedsKind::Approve | NeedsKind::Answer => a.what.clone(),
                    _ => None,
                },
                tone: Tone::Need,
                rank: f::NEEDS_BY_DANGER
                    .iter()
                    .position(|k| *k == kind)
                    .map_or(u8::MAX, |i| i as u8),
                waited_ms: Some(waited),
                waited_text: Some(Words(copy_core::short_duration(waited))),
                ask: Some(a),
                manual_text: None,
            })
        }
        (None, None) => None,
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/observe/asks_tests.rs"]
mod tests;
