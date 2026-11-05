//! **待答登记表**（常驻后端进程里一份，通用、不认是哪一家）：会话里弹批准 / 提问 / 批计划的框时，那一家的答题 hook
//! 经 `needs-ask` 把请求挂进来、阻塞等；任一出口（桌面 · 手机）经 `needs-answer` 答，**第一个答案落下**，其余回「别处已答」（`gone` ＋ 那一句）；
//! hook 那条连接断了（终端那边先答了 · 被打断 · 到点）⇒ 撤掉，之后再答同样回 `gone`。挂上 · 撤掉 · 答掉都推 `changed {needs, key: sid}`。
//!
//! 成品挂在会话的 `needs` 上（`observe::recognize::facts::dress_ask`）：`needs.ask` 是「这一条可以怎么答」—— 选项的字、语气、要不要附一句话都由这里写好，出口不按工具名取字；
//! pidfile 说在等、却没有挂着的请求（用户自己起的会话 · 只能本人答的框 · 工作区信任框 …）⇒ `ask: null`，`manualText` 写好「只能在终端里答」。
//!
//! **无人答时自动答**（只管提问；批准工具 · 批计划是安全闸，一律不自动答 —— 只此一处判，[`auto_of`]）：这条会话的注解开着
//! `autoAnswer` ⇒ 挂上的提问自己带期限（`needs-ask` 那条在等的，零定时器：期限跟着这一次挂着的走），等满了还没人答就替人答 ——
//! 每一题答成「其他」＋ 那一句。
//!
//! **上一条请求怎么收的场**（答上了 · 替人答了 · hook 那头先走了）都在这里写成那一句（[`Settled::end`]），记在会话上
//! （`sessions-needs.ended` · 会话事实 `askEnd`），这条会话下一次挂上请求才换掉；两端照摆，不各自兜底。
//!
//! 那一家的字段名不进这里：读 hook 输入 · 写决定都经注册表 `Adapter.asks`（[`crate::agents::AskFace`]）。

pub mod hook;

use crate::agents::asks::{product_of, Ask, NoteNeed};
use crate::agents::{AskIn, PlanMode, Verdict};
use crate::common::cells::Words;
use crate::common::said::Said;
/// 挂上 · 撤掉 · 答掉都报这一声（`history::needs_changed`）。
use crate::history::needs_changed::moved;
pub(crate) use cmd_wire::needs::{
    AnswerPick as Pick, AnswerReq, AnswerResult, Answered, AskEnd, AutoAnswer, AutoChoice,
};
use copy_core::copy_text;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// `needs-ask` 的应答：人的答案写成那一家要的那一整份（hook 原样打出去）；`null` ＝ 不给决定。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct AskReply {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "unknown"))]
    pub decision: Option<Value>,
}

/// `needs-ask` 的入参：哪一家（线上的 kind）＋ 那一家的 hook 交来的那一份原样。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct AskReq {
    pub agent: crate::agents::AgentKind,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "unknown"))]
    pub hook: Value,
}

crate::wire::typed::codes! {
    /// `needs-ask` 没有别的码（不是要人答的那一种 ⇒ `{decision: null}`，不是失败）。
    pub enum AskErr {}
}

/// `needs-ask`（帧面硬臂：挂的那条跟着本连接走；本体真异步，`cancel` 打断 ＝ 不给决定）。
pub struct NeedsAsk;

impl crate::wire::typed::Command for NeedsAsk {
    const NAME: &'static str = "needs-ask";
    type Req = AskReq;
    type Reply = AskReply;
    type Err = AskErr;
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

/// 一条请求怎么收的场（收场那一句只在 [`Settled::end`] 写）。
enum Settled {
    /// 某个出口答上了。
    Answered,
    /// 没人答、替人答了：那一句（「{dur}无人答 · 已自动答：…」）。
    Auto(Words),
    /// hook 那头先走了（终端那边答了 · 被打断 · 到点）。
    Closed,
}

impl Settled {
    /// **收场那一句的唯一一处**：会话事实 `askEnd` · `sessions-needs.ended` · 之后再答回的 `gone` 那一句都照它。
    fn end(self) -> AskEnd {
        match self {
            Settled::Answered => AskEnd {
                said: Words(copy_text("beAsk.answer.already", &[])),
                stays: false,
            },
            Settled::Auto(said) => AskEnd { said, stays: true },
            Settled::Closed => AskEnd {
                said: Words(copy_text("askBlock.gone.elsewhere", &[])),
                stays: false,
            },
        }
    }
}

#[derive(Default)]
struct Book {
    open: BTreeMap<String, Open>,
    /// 收了场的那几条（请求 id ⇒ 那一格），之后再答照它回 `gone`。
    settled: VecDeque<(String, AskEnd)>,
    /// 会话 id ⇒ 上一条请求收场的那一格（这条会话下一次挂上请求就摘掉）。
    ended: BTreeMap<String, AskEnd>,
}

/// 收了场的记几条（再早的答回 `not_found`）；收场那一格也只留这么多条会话。
const SETTLED_KEEP: usize = 256;

impl Book {
    /// 收场：那条请求记下那一格、那条会话的「上一条请求怎么了」换成它；回那一格。
    fn settle(&mut self, id: String, sid: String, how: Settled) -> AskEnd {
        let end = how.end();
        self.settled.push_back((id, end.clone()));
        while self.settled.len() > SETTLED_KEEP {
            self.settled.pop_front();
        }
        self.ended.insert(sid, end.clone());
        while self.ended.len() > SETTLED_KEEP {
            self.ended.pop_first();
        }
        end
    }
    fn settled(&self, id: &str) -> Option<&AskEnd> {
        self.settled
            .iter()
            .rev()
            .find(|(i, _)| i == id)
            .map(|(_, e)| e)
    }
}

fn book() -> std::sync::MutexGuard<'static, Book> {
    static BOOK: std::sync::OnceLock<Mutex<Book>> = std::sync::OnceLock::new();
    BOOK.get_or_init(|| {
        crate::history::needs_changed::answer_asked(sids, pending);
        Mutex::default()
    })
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// 一条连接（hook 那一头）挂的请求跟着它走：它一丢 ⇒ 它挂的全撤掉（入方向读循环每条连接一份）。
pub struct Desk {
    conn: u64,
}

impl Default for Desk {
    fn default() -> Self {
        Self::new()
    }
}

impl Desk {
    pub fn new() -> Desk {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Desk {
            conn: NEXT.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// 这条连接的号（交给 `needs-ask` 的处理器：处理器不拿着 [`Desk`] 本身，否则它丢不掉）。
    pub fn conn(&self) -> u64 {
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
/// `setting` ＝ 这条会话「无人答时自动答」那一格（注解里那一份，调用方现读交进来）。
pub async fn ask(
    conn: u64,
    req: AskReq,
    setting: impl FnOnce(&str) -> Option<AutoAnswer>,
) -> AskReply {
    let Some(asked) = crate::agents::ask_face(&req.agent).and_then(|f| (f.read)(&req.hook)) else {
        return AskReply { decision: None };
    };
    let auto = auto_of(&asked, setting(&asked.sid).as_ref());
    let (id, mut rx) = open_id(&req.agent, asked, conn);
    // 撤掉（连接断了 / 别处关了）⇒ 发端丢了 ⇒ 不给决定。
    let decision = match auto {
        None => rx.await.ok().flatten(),
        Some(auto) => tokio::select! {
            r = &mut rx => r.ok().flatten(),
            () = due(auto.after_min) => {
                match answer_for_nobody(&id, &auto) {
                    Some(d) => Some(d),
                    // 刚好有人先答了 / 撤了：照那一头交来的。
                    None => rx.await.ok().flatten(),
                }
            }
        },
    };
    AskReply { decision }
}

/// 挂着的这一条自己带的期限（等满 `min` 分钟醒一次）。
#[expect(
    clippy::disallowed_methods,
    reason = "会话设了「无人答时自动答」的那一条提问自己带的一次性期限，不是节拍：随这一条收场一起没，那一格关着就不起"
)]
async fn due(min: u32) {
    tokio::time::sleep(std::time::Duration::from_secs(u64::from(min) * 60)).await;
}

/// 子运行发的那一条：派出它时的标签（会话记录照 sid 在这台的记录树里找，[`crate::agents::asks::run_label`]）；主会话发的 · 找不到 ⇒ `None`。
fn label_of(asked: &AskIn) -> Option<String> {
    let run = asked.agent_id.as_deref()?;
    let at =
        crate::agents::find_session(&crate::history::listing::agent_home(), &asked.sid).ok()?;
    crate::agents::asks::run_label(&at.file, run)
}

/// 挂上一条（`conn` ＝ 它那条连接）：回它的号与等答的那一头。
pub fn open_id(
    agent: &str,
    asked: AskIn,
    conn: u64,
) -> (String, tokio::sync::oneshot::Receiver<Option<Value>>) {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = format!("ask-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let product = product_of(
        &id,
        &asked,
        label_of(&asked).as_deref(),
        crate::common::time::now_ms(),
    );
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<Value>>();
    let sid = asked.sid.clone();
    {
        let mut b = book();
        b.ended.remove(&sid);
        b.open.insert(
            id.clone(),
            Open {
                agent: agent.to_string(),
                ask: asked,
                product,
                conn,
                tx,
            },
        );
    }
    moved(&sid);
    (id, rx)
}

/// 替人答的那一句的缺省（交给那一家的话）。
pub fn auto_say() -> String {
    copy_text("beAsk.auto.say", &[])
}

/// 「无人答时自动答」可选的几档（分钟；`0` ＝ 关）。上限同注解那一侧的值域（`history::history_annotations::AUTO_MAX_MIN`）。
const AUTO_CHOICES: [u32; 4] = [0, 5, 15, 30];

/// 自动答可选的那几档 —— **唯一一处**：几档 ＋ 此刻那一格（`now`；关 ⇒ `None`）不在几档里 ⇒ 多列一项，按分钟排好；字都写好。
pub fn auto_choices(now: Option<u32>) -> Vec<AutoChoice> {
    let mut mins = AUTO_CHOICES.to_vec();
    if let Some(m) = now.filter(|m| !mins.contains(m)) {
        mins.push(m);
        mins.sort_unstable();
    }
    mins.into_iter()
        .map(|min| AutoChoice {
            min,
            text: Words(if min == 0 {
                copy_text("beAsk.auto.off", &[])
            } else {
                copy_core::short_duration(u64::from(min) * 60_000)
            }),
        })
        .collect()
}

/// 这台挂上的这一条要不要、多久后替人答 —— **唯一的判定**：只有提问那一种；批准工具 · 批计划永远不（安全闸）。
/// 那一格的值域在注解那一侧（`history::history_annotations::auto_checked`）。
pub(crate) fn auto_of(a: &AskIn, setting: Option<&AutoAnswer>) -> Option<AutoAnswer> {
    if a.kind != crate::agents::AskKind::Question {
        return None;
    }
    setting.filter(|s| s.after_min > 0).cloned()
}

/// 等满了：还挂着 ⇒ 每一题答成「其他」＋ 那一句（没读出题的那一种 ⇒ 不回答、附那一句），收场、留记录 ⇒ 那一家要的那一份；
/// 已经不在（有人先答了 · 撤了）/ 那一家写不出 ⇒ `None`。
fn answer_for_nobody(id: &str, auto: &AutoAnswer) -> Option<Value> {
    let say = auto.say.clone().unwrap_or_else(auto_say);
    let mut b = book();
    let o = b.open.get(id)?;
    let verdict = if o.product.questions.is_empty() {
        Verdict::Deny {
            note: Some(say.clone()),
            stop: false,
        }
    } else {
        Verdict::Allow {
            rule: None,
            mode: None,
            answers: o
                .product
                .questions
                .iter()
                .map(|q| (q.text.clone(), say.clone()))
                .collect(),
        }
    };
    let decision = crate::agents::ask_face(&o.agent).and_then(|f| (f.decide)(&o.ask, &verdict))?;
    let o = b.open.remove(id).expect("刚找到的那一条");
    let sid = o.ask.sid;
    let said = Words(copy_text(
        "beAsk.auto.answered",
        &[
            (
                "dur",
                &copy_core::format_duration(u64::from(auto.after_min) * 60_000),
            ),
            ("say", &say),
        ],
    ));
    b.settle(id.to_string(), sid.clone(), Settled::Auto(said));
    drop(b);
    moved(&sid);
    Some(decision)
}

/// 这条会话上一条请求收场的那一格（会话事实 `askEnd`；下一次挂上请求前一直在）；还没收过场 ⇒ `None`。
pub fn ended(sid: &str) -> Option<AskEnd> {
    book().ended.get(sid).cloned()
}

/// 收了场的那几条（`sessions-needs.ended`），按会话 id 排。
pub fn ended_rows() -> Vec<cmd_wire::needs::AskEndRow> {
    book()
        .ended
        .iter()
        .map(|(sid, end)| cmd_wire::needs::AskEndRow {
            sid: sid.clone(),
            end: end.clone(),
        })
        .collect()
}

/// 这条连接挂的全撤掉（之后再答 ⇒ `gone`）。
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
                out.push((id.clone(), o.ask.sid.clone()));
                b.settle(id, o.ask.sid, Settled::Closed);
            }
        }
        out
    };
    for (_, sid) in gone {
        moved(&sid);
    }
}

/// 某条会话此刻挂着的请求（挂上的先后）。
pub fn pending(sid: &str) -> Vec<Ask> {
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

/// 选的那一项 ⇒ 通用答案（`None` ＝ 改到终端里答：不给决定）。闭集：approve 那几项 `allow` · `always-<i>` · `deny` · `deny-note` · `deny-stop`；
/// plan `plan-auto` · `plan-edits` · `plan-manual` · `plan-keep`；question `answer` · `dismiss`；挡着终端的另有 `release`。
pub(crate) fn verdict_of(
    a: &AskIn,
    product: &Ask,
    choice: &str,
    note: Option<&str>,
    picks: Option<&BTreeMap<String, Pick>>,
) -> Result<Option<Verdict>, Said> {
    let spec = product
        .choices
        .iter()
        .find(|c| c.id == choice)
        .ok_or_else(|| Said::new("bad_choice", copy_text("beAsk.answer.badChoice", &[])))?;
    let note = note.map(str::trim).filter(|n| !n.is_empty());
    if spec.note == NoteNeed::Required && note.is_none() {
        return Err(Said::new(
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
                return Err(Said::new(
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
) -> Result<Vec<(String, String)>, Said> {
    let bad = || Said::new("bad_args", copy_text("beAsk.answer.picks", &[]));
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

crate::wire::typed::codes! {
    /// `needs-answer` 的码。
    pub enum AnswerErr {
        /// 没这一条（或不是这个会话的）。
        NotFound = "not_found",
        /// 选项不在这一条的选项里 / 那一家写不出这个决定。
        BadChoice = "bad_choice",
        /// 选项要附一句话，没给。
        NoteRequired = "note_required",
    }
}

/// `needs-answer`：答一条挂着的待答请求，第一个答案落下。锁一下登记表、交给 hook 那一头的通道：不碰盘、不起进程、不等 ⇒ 异步档。
pub struct NeedsAnswerCmd;

impl crate::wire::typed::Command for NeedsAnswerCmd {
    crate::wire::typed::by_call!(AnswerReq);
    type Err = AnswerErr;
}

impl crate::wire::typed::Handle for NeedsAnswerCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: AnswerReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Answered, AnswerErr>> + Send + 'static {
        std::future::ready(
            answer(req).map_err(crate::wire::typed::of_fail_with(AnswerErr::NotFound)),
        )
    }
}

/// 别处已答那一种结局 ＋ 那一句。
fn gone(said: Words) -> Answered {
    Answered {
        result: AnswerResult::Gone,
        said: Some(said),
    }
}

/// `needs-answer` 的本体 ⇒ `{result: "answered"}`；别处先答了（别的出口 · 替人答过）· hook 那头已经走了（终端那边答了 · 被打断）
/// ⇒ `{result: "gone", said}`（不是错：这一下没答进去，也不用再答）；没这一条 ⇒ `not_found`。
pub fn answer(req: AnswerReq) -> Result<Answered, Said> {
    let (sid, id, choice) = (req.sid.as_str(), req.ask.as_str(), req.choice.as_str());
    let mut b = book();
    let Some(o) = b.open.get(id).filter(|o| o.ask.sid == sid) else {
        return match b.settled(id) {
            Some(end) => Ok(gone(end.said.clone())),
            None => Err(Said::new(
                "not_found",
                copy_text("beAsk.answer.notFound", &[]),
            )),
        };
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
                return Err(Said::new(
                    "bad_choice",
                    copy_text("beAsk.answer.badChoice", &[]),
                ))
            }
        },
        None => None,
    };
    let o = b.open.remove(id).expect("刚找到的那一条");
    let delivered = o.tx.send(decision).is_ok();
    let how = if delivered {
        Settled::Answered
    } else {
        Settled::Closed
    };
    let end = b.settle(id.to_string(), o.ask.sid, how);
    drop(b);
    moved(sid);
    Ok(if delivered {
        Answered {
            result: AnswerResult::Answered,
            said: None,
        }
    } else {
        gone(end.said)
    })
}
