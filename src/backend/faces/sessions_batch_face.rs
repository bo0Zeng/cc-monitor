//! 帧面 `sessions-batch`（typed，只在本机后端答全）：一批会话跨几台的那一条 —— 收全部 id、按台分组、
//! 本机那一组同进程做（停 ＝ `sessions-stop` 的本体；起 ＝ 逐项 `session-open` 的恢复那一形；隐藏 · 固定那几样 ＝ 逐项 `history-annotate` 的本体），别的台各拿自己那一组问它常驻的
//! `sessions-batch`（那一份只含它自己的会话），逐条结局照入参原序摆好，合那一句交 `batch_core::sessions::sessions_reply`（手机核心同一份）。
//!
//! 那台够不着 · 常驻不在 · 太旧 · 答了失败 ⇒ 那一组每一项都是 `failed`，那一句与复制详情照那一台的失败（`cmd_wire::Failed`）写。

use crate::common::said::Said;
use crate::control::session_batch as batch;
use crate::dial::remote_ask::{Remote, Table};
use crate::wire::typed::{codes, Command};
use batch_core::sessions::{sessions_reply, Verb};
use cmd_wire::batch::{
    OpenWindow, SessionItem, SessionOutcome, SessionState, SessionsBatchReply, SessionsBatchReq,
    SessionsOp,
};
use cmd_wire::session::{Landed, Launch, Opened};
use cmd_wire::Machine;
use copy_core::copy_text;
use std::future::Future;

codes! {
    /// `sessions-batch` 自己没有别的码（哪台没成只进那一组的逐条结局）。
    pub enum SessionsBatchErr {}
}

/// `sessions-batch`：一批会话跨几台（本机同进程做，别的台经常驻问它自己那一组），合成一份再答。
pub struct SessionsBatchCmd;

impl Command for SessionsBatchCmd {
    const NAME: &'static str = "sessions-batch";
    type Req = SessionsBatchReq;
    type Reply = SessionsBatchReply;
    type Err = SessionsBatchErr;
}

/// 起会话那一问要的那两样（文件管理面 ＋ 写自家状态的句柄），同 `session-open`。
type OpenHands = <crate::wire::typed::needs::FilesOwn as crate::wire::spec::Needs>::Given;

impl crate::wire::typed::Handle for SessionsBatchCmd {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: SessionsBatchReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl Future<Output = Result<SessionsBatchReply, SessionsBatchErr>> + Send + 'static {
        let (envelope, hands) = cx.split();
        async move {
            let (tz, until) = (envelope.tz.clone(), envelope.until);
            across(
                req,
                |r| here(r, tz, until, hands),
                &envelope.tz,
                until,
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
            )
            .await
            .map_err(SessionsBatchErr::BadArgs)
        }
    }
}

/// 这一样的字：动作那个词（原形）＋ 只有一项时点名的两句。
pub(crate) fn verb_of(op: &SessionsOp) -> Verb {
    match op {
        SessionsOp::Stop => Verb {
            action: copy_text("tabBatch.verb.stop", &[]),
            one_done: |t| copy_text("sessionState.kill.done", &[("title", t)]),
            one_failed: |t| copy_text("tabSessionActions.kill.failed", &[("title", t)]),
        },
        SessionsOp::Start { run, .. } => Verb {
            action: match run {
                Launch::Tmux => copy_text("tabBatch.verb.startTmux", &[]),
                Launch::Direct => copy_text("tabBatch.verb.startWindow", &[]),
                Launch::Managed => copy_text("history.row.resume", &[]),
            },
            one_done: |t| copy_text("tabBatch.one.resumed", &[("title", t)]),
            one_failed: |t| copy_text("tabBatch.one.resumeFailed", &[("title", t)]),
        },
        SessionsOp::Hide => Verb {
            action: copy_text("history.menu.hide", &[]),
            one_done: |t| copy_text("tabBar.hide.done", &[("title", t)]),
            one_failed: |t| copy_text("tabBar.hide.failed", &[("title", t)]),
        },
        SessionsOp::Unhide => Verb {
            action: copy_text("history.menu.unhide", &[]),
            one_done: |t| copy_text("tabBatch.one.unhidden", &[("title", t)]),
            one_failed: |t| copy_text("tabBar.unhide.failed", &[("title", t)]),
        },
        SessionsOp::ClearHide => Verb {
            action: copy_text("tabBatch.verb.clearHide", &[]),
            one_done: |t| copy_text("tabBatch.one.hideUndone", &[("title", t)]),
            one_failed: |t| copy_text("tabBatch.one.hideUndoFailed", &[("title", t)]),
        },
        SessionsOp::Pin => Verb {
            action: copy_text("tabMenu.pin.pin", &[]),
            one_done: |t| copy_text("tabBatch.one.pinned", &[("title", t)]),
            one_failed: |t| copy_text("tabBatch.one.pinFailed", &[("title", t)]),
        },
        SessionsOp::Unpin => Verb {
            action: copy_text("tabMenu.pin.unpin", &[]),
            one_done: |t| copy_text("tabBar.unpinned.toast", &[("title", t)]),
            one_failed: |t| copy_text("tabBatch.one.unpinFailed", &[("title", t)]),
        },
    }
}

/// 本机那一组：照那一样分到停 · 起 · 写注解（隐藏那三样 · 固定那两样）。
async fn here(
    req: SessionsBatchReq,
    tz: crate::common::time::Tz,
    until: Option<crate::platform::child::Until>,
    hands: OpenHands,
) -> Result<SessionsBatchReply, Said> {
    match req.op {
        SessionsOp::Stop => stop_here(req, tz, until).await,
        SessionsOp::Start { run, relayed } => {
            start_here(req, run, !relayed, tz, until, hands).await
        }
        SessionsOp::Hide
        | SessionsOp::Unhide
        | SessionsOp::ClearHide
        | SessionsOp::Pin
        | SessionsOp::Unpin => note_here(req, tz, until, hands.1).await,
    }
}

/// 写注解那几样：逐项写这台的注解（`history-annotate {hide | pinned}` 的本体，同进程）。固定那两样先照注解里那一格判跳过；
/// 隐藏做成了的那几项照改之前那一格分好撤销那几问。
async fn note_here(
    req: SessionsBatchReq,
    tz: crate::common::time::Tz,
    until: Option<crate::platform::child::Until>,
    own: crate::platform::Own,
) -> Result<SessionsBatchReply, Said> {
    use crate::wire::typed::{Codes as _, Handle as _};
    let op = req.op;
    let rt = tokio::runtime::Handle::current();
    let (results, undo) = tokio::task::spawn_blocking(move || {
        let home = crate::history::listing::agent_home();
        let notes = notes();
        let patch = match op {
            SessionsOp::Hide => serde_json::json!({"hide": "on"}),
            SessionsOp::Unhide => serde_json::json!({"hide": "off"}),
            SessionsOp::Pin => serde_json::json!({"pinned": true}),
            SessionsOp::Unpin => serde_json::json!({"pinned": false}),
            _ => serde_json::json!({"hide": "rule"}),
        };
        let pin = match op {
            SessionsOp::Pin => Some(true),
            SessionsOp::Unpin => Some(false),
            _ => None,
        };
        let mut results = Vec::with_capacity(req.items.len());
        let mut undo = Vec::new();
        for it in &req.items {
            let sid = it.sid.as_str();
            let write = || {
                let ask: crate::history::history_annotations::AnnotateReq =
                    serde_json::from_value(serde_json::json!({"sid": sid, "patch": patch}))
                        .map_err(|e| Said::from(e.to_string()))?;
                let cx = crate::wire::typed::Cx {
                    tz: tz.clone(),
                    called: None,
                    until,
                    needs: own.clone(),
                };
                rt.block_on(crate::history::history_annotations::HistoryAnnotateCmd::run(ask, cx))
                    .map(|a| a.prev)
                    .map_err(|e| e.into_said())
            };
            let title = title_here(&home, &notes, sid);
            let (o, u) = match pin {
                Some(on) => {
                    let pinned = notes.get(sid).is_some_and(|e| e.pinned);
                    (
                        pin_one(&it.sid, title, on, pinned, || write().map(|_| ())),
                        None,
                    )
                }
                None => hide_one(&it.sid, title, op, write),
            };
            results.push(o);
            if let Some(u) = u {
                undo_push(&mut undo, u, it.clone());
            }
        }
        (results, undo)
    })
    .await
    .map_err(|e| Said::from(e.to_string()))?;
    Ok(SessionsBatchReply {
        undo,
        ..sessions_reply(&verb_of(&op), results)
    })
}

/// 隐藏那三样的一项：写成了 ⇒ 成了（隐藏 ⇒ 撤销那一问照改之前那一格：没有 ⇒ 回到规则 · `off` ⇒ 取消隐藏 · `on` ⇒ 隐藏；别的两样撤不回）；
/// 写不下去 ⇒ 没成（那一句 ＋ 复制详情）。`write` 回改之前那一格。
pub(crate) fn hide_one(
    sid: &cmd_wire::Sid,
    title: String,
    op: SessionsOp,
    write: impl FnOnce() -> Result<Option<cmd_wire::session::Hide>, Said>,
) -> (SessionOutcome, Option<SessionsOp>) {
    use cmd_wire::session::Hide;
    let mut o = SessionOutcome {
        machine: Machine::local(),
        sid: sid.clone(),
        outcome: SessionState::Done,
        why: None,
        said: None,
        copy_detail: String::new(),
        title,
        session: None,
    };
    match write() {
        Ok(prev) => {
            let undo = (op == SessionsOp::Hide).then_some(match prev {
                None => SessionsOp::ClearHide,
                Some(Hide::Off) => SessionsOp::Unhide,
                Some(Hide::On) => SessionsOp::Hide,
            });
            (o, undo)
        }
        Err(s) => {
            let f =
                crate::wire::detail::failed_of("history-annotate", s.code_or_fallback(), s.clone());
            o.outcome = SessionState::Failed;
            o.why = Some(f.code);
            o.said = Some(f.said);
            o.copy_detail = f.detail;
            (o, None)
        }
    }
}

/// 固定那两样的一项：注解里已经是那一格 ⇒ 跳过（已固定 · 未固定那一句，不写）；不然写那一格：成了 · 写不下去 ⇒ 没成（那一句 ＋ 复制详情）。
pub(crate) fn pin_one(
    sid: &cmd_wire::Sid,
    title: String,
    on: bool,
    pinned: bool,
    write: impl FnOnce() -> Result<(), Said>,
) -> SessionOutcome {
    let mut o = SessionOutcome {
        machine: Machine::local(),
        sid: sid.clone(),
        outcome: SessionState::Done,
        why: None,
        said: None,
        copy_detail: String::new(),
        title,
        session: None,
    };
    if pinned == on {
        let (why, said) = if on {
            ("pinned", copy_text("tabBatch.why.pinned", &[]))
        } else {
            ("not_pinned", copy_text("tabBatch.why.notPinned", &[]))
        };
        o.outcome = SessionState::Skipped;
        o.why = Some(why.to_string());
        o.said = Some(said);
        return o;
    }
    if let Err(s) = write() {
        let f = crate::wire::detail::failed_of("history-annotate", s.code_or_fallback(), s.clone());
        o.outcome = SessionState::Failed;
        o.why = Some(f.code);
        o.said = Some(f.said);
        o.copy_detail = f.detail;
    }
    o
}

/// 撤销那几问里添一项：同一样的并进同一问（照第一次出现的先后）。
fn undo_push(undo: &mut Vec<SessionsBatchReq>, op: SessionsOp, it: SessionItem) {
    match undo.iter_mut().find(|q| q.op == op) {
        Some(q) => q.items.push(it),
        None => undo.push(SessionsBatchReq {
            op,
            items: vec![it],
            client: None,
        }),
    }
}

/// 停：同进程跑 `sessions-stop` 的本体（阻塞档 ⇒ 放进阻塞池、带停那一档的期限）。
async fn stop_here(
    req: SessionsBatchReq,
    tz: crate::common::time::Tz,
    until: Option<crate::platform::child::Until>,
) -> Result<SessionsBatchReply, Said> {
    use crate::wire::typed::{Codes as _, Handle as _};
    let verb = verb_of(&req.op);
    let sids = batch::SidsReq {
        sids: req.items.into_iter().map(|i| i.sid).collect(),
        client: req.client,
    };
    let cx = crate::wire::typed::Cx {
        tz,
        until,
        ..Default::default()
    };
    let rt = tokio::runtime::Handle::current();
    let done = tokio::task::spawn_blocking(move || {
        let _budget = crate::platform::child::Budget::capped(batch::BATCH_CAP, until);
        rt.block_on(crate::faces::session_batch_face::SessionsStop::run(
            sids, cx,
        ))
    })
    .await
    .map_err(|e| Said::from(e.to_string()))?
    .map_err(|e| e.into_said())?;
    Ok(sessions_reply(&verb, done.results))
}

/// 起：逐项挨个问这台的 `session-open`（恢复那一形，只交 sid 与放在哪；号 · 目录 · 哪一家这台从记录里读），
/// 之前先照这台的会话脸判恢复得了没有。`window_here` ＝ 开窗的就是这台（界面直接交来的那一组）。
async fn start_here(
    req: SessionsBatchReq,
    run: Launch,
    window_here: bool,
    tz: crate::common::time::Tz,
    until: Option<crate::platform::child::Until>,
    hands: OpenHands,
) -> Result<SessionsBatchReply, Said> {
    use crate::wire::typed::Handle as _;
    let verb = verb_of(&req.op);
    let rt = tokio::runtime::Handle::current();
    let (results, windows) = tokio::task::spawn_blocking(move || {
        let home = crate::history::listing::agent_home();
        let live = crate::history::live_table::now(&home);
        let notes = notes();
        let mut results = Vec::with_capacity(req.items.len());
        let mut windows = Vec::new();
        for it in &req.items {
            let sid = it.sid.as_str();
            let can = {
                use crate::wire::{Life, SessionFace, SessionFaceExt as _, WireLife};
                let life = match live.life_of(sid) {
                    WireLife::Live { activity } => Life::live(activity, None),
                    other => Life::of_wire(&other),
                };
                SessionFace::of(life).can.resume
            };
            let (o, w) = start_one(&it.sid, title_here(&home, &notes, sid), can, || {
                let _budget = crate::platform::child::Budget::capped(
                    crate::control::session_open::OPEN_CAP,
                    until,
                );
                let ask = cmd_wire::open::SessionOpenReq {
                    what: cmd_wire::open::OpenWhat::Resume {
                        sid: it.sid.clone(),
                    },
                    account: None,
                    run: Some(run),
                    local: window_here,
                    ticket: None,
                    client: req.client.clone(),
                };
                let cx = crate::wire::typed::Cx {
                    tz: tz.clone(),
                    called: None,
                    until,
                    needs: hands.clone(),
                };
                rt.block_on(crate::faces::session_open_face::SessionOpen::run(ask, cx))
                    .map_err(|e| e.0)
            });
            results.push(o);
            windows.extend(w);
        }
        (results, windows)
    })
    .await
    .map_err(|e| Said::from(e.to_string()))?;
    Ok(SessionsBatchReply {
        windows,
        ..sessions_reply(&verb, results)
    })
}

/// 起一项：会话脸说恢复不了 ⇒ 跳过（那一格那一句，不问）；问了 ⇒ 本来就在跑 ⇒ 跳过（在哪个 tmux 会话里）· 落在窗口里 ⇒ 成了、带开窗那一行 ·
/// 别的落处 ⇒ 成了 · 那台拒了 ⇒ 没成（那一句 ＋ 复制详情）。每台那一跳答的是它自己 ⇒ `machine` 是本机那一个。
pub(crate) fn start_one(
    sid: &cmd_wire::Sid,
    title: String,
    can: crate::wire::Allow,
    open: impl FnOnce() -> Result<Opened, Said>,
) -> (SessionOutcome, Option<OpenWindow>) {
    let one = |outcome, why: Option<&str>, said: Option<String>, session: Option<String>| {
        SessionOutcome {
            machine: Machine::local(),
            sid: sid.clone(),
            outcome,
            why: why.map(str::to_string),
            said,
            copy_detail: String::new(),
            title: title.clone(),
            session,
        }
    };
    if let crate::wire::Allow::No { why } = can {
        return (
            one(
                SessionState::Skipped,
                Some(NOT_RESUMABLE),
                Some(why.0),
                None,
            ),
            None,
        );
    }
    match open() {
        Err(s) => {
            let f = crate::wire::detail::failed_of("session-open", s.code_or_fallback(), s.clone());
            (
                SessionOutcome {
                    copy_detail: f.detail,
                    ..one(
                        SessionState::Failed,
                        Some(f.code.as_str()),
                        Some(f.said),
                        None,
                    )
                },
                None,
            )
        }
        Ok(o) if o.landed == Landed::Running => {
            let name = o.session.clone().unwrap_or_default();
            let said = copy_text("tabBatch.why.running", &[("name", &name)]);
            (
                one(
                    SessionState::Skipped,
                    Some("running"),
                    Some(said),
                    o.session,
                ),
                None,
            )
        }
        Ok(o) => {
            let window = match (o.landed, o.line) {
                (Landed::Window, Some(line)) => Some(OpenWindow {
                    machine: Machine::local(),
                    sid: sid.clone(),
                    line,
                    cwd: o.cwd,
                }),
                _ => None,
            };
            (one(SessionState::Done, None, None, o.session), window)
        }
    }
}

/// 会话脸说恢复不了（运行中 · 记录已不在 …）：那一项跳过的原因码。
const NOT_RESUMABLE: &str = "not_resumable";

/// 这台的注解表（读不到 ⇒ 空表：称呼退回记录里的那一份）。
pub(crate) fn notes() -> cmd_wire::listing::Table {
    match crate::history::history_annotations::load() {
        crate::history::history_annotations::Loaded::Read(t) => t,
        _ => Default::default(),
    }
}

/// 这台写的称呼：这条会话的整行标题（`search_rules::session_line`，与标签页同一行）。一批会话各样都照它。
pub(crate) fn title_here(
    home: &std::path::Path,
    notes: &cmd_wire::listing::Table,
    sid: &str,
) -> String {
    use crate::history::search_rules::LabelFacts;
    let custom = notes.get(sid).and_then(|e| e.custom_title.clone());
    let facts = match crate::agents::find_session(home, sid) {
        Ok(at) => LabelFacts {
            custom,
            ..crate::history::listing::label_facts_of(&at.file)
        },
        Err(_) => LabelFacts::unread(sid, custom),
    };
    crate::history::search_rules::session_line(&facts)
}

/// 合各台：校验 → 按台分组（照第一次出现的先后）→ 每台各问自己那一组（转给别的台的起那一组标 `relayed`）→
/// 逐条照入参原序摆好、远端的称呼套机器名 → 合那一句；开窗那几行 · 撤销那几问照原序、换成那一项的那台。
/// `local` ＝ 本机那一组怎么做（生产 [`here`]，判据给替身）；入参不对 ⇒ `Err`（`bad_args` 那一句）。
pub(crate) async fn across<'a, L, F>(
    req: SessionsBatchReq,
    local: L,
    tz: &'a crate::common::time::Tz,
    until: Option<crate::platform::child::Until>,
    table: &'a Table,
    remote: &'a dyn Remote,
) -> Result<SessionsBatchReply, String>
where
    L: FnOnce(SessionsBatchReq) -> F,
    F: Future<Output = Result<SessionsBatchReply, Said>> + Send + 'a,
{
    if req.items.is_empty() || req.items.len() > batch::MAX_BATCH {
        return Err(crate::common::contract::malformed(&format!(
            "`items` must hold 1..={} entries",
            batch::MAX_BATCH
        )));
    }
    for (i, it) in req.items.iter().enumerate() {
        if req.items[..i].contains(it) {
            return Err(crate::common::contract::malformed(&format!(
                "duplicate item: {} on {}",
                it.sid, it.machine
            )));
        }
    }
    let mut groups: Vec<(Machine, SessionsBatchReq)> = Vec::new();
    for it in &req.items {
        let one = SessionItem {
            machine: Machine::local(),
            sid: it.sid.clone(),
        };
        match groups.iter_mut().find(|(m, _)| *m == it.machine) {
            Some((_, g)) => g.items.push(one),
            None => groups.push((
                it.machine.clone(),
                SessionsBatchReq {
                    op: relayed_to(req.op, &it.machine),
                    items: vec![one],
                    client: req.client.clone(),
                },
            )),
        }
    }
    let got = crate::dial::remote_ask::ask_apiece::<SessionsBatchCmd, _, _, _>(
        groups, local, tz, until, table, remote,
    )
    .await?;
    let answered = |it: &SessionItem| got.iter().find(|(m, _)| *m == it.machine).map(|(_, r)| r);
    let results = req
        .items
        .iter()
        .map(|it| outcome_of(it, answered(it)))
        .collect();
    let windows = req
        .items
        .iter()
        .filter_map(|it| {
            let Some(Ok(r)) = answered(it) else {
                return None;
            };
            let w = r.windows.iter().find(|w| w.sid == it.sid)?;
            Some(OpenWindow {
                machine: it.machine.clone(),
                ..w.clone()
            })
        })
        .collect();
    let mut undo = Vec::new();
    for it in &req.items {
        let Some(Ok(r)) = answered(it) else {
            continue;
        };
        if let Some(q) = r
            .undo
            .iter()
            .find(|q| q.items.iter().any(|i| i.sid == it.sid))
        {
            undo_push(&mut undo, q.op, it.clone());
        }
    }
    Ok(SessionsBatchReply {
        windows,
        undo,
        ..sessions_reply(&verb_of(&req.op), results)
    })
}

/// 那一组交给哪台：起那一组转给别的台 ⇒ 标 `relayed`（开窗的不是那台）；别的照原样。
fn relayed_to(op: SessionsOp, machine: &Machine) -> SessionsOp {
    match op {
        SessionsOp::Start { run, relayed } => SessionsOp::Start {
            run,
            relayed: relayed || !machine.is_local(),
        },
        other => other,
    }
}

/// 一项的结局：那台答回来的那一条（换成这一项的那台、远端的称呼套机器名）；那台没成 ⇒ 那台那一份失败。
fn outcome_of(
    it: &SessionItem,
    answered: Option<&Result<SessionsBatchReply, Said>>,
) -> SessionOutcome {
    let found = match answered {
        Some(Ok(r)) => match r.results.iter().find(|o| o.sid == it.sid) {
            Some(o) => Ok(o.clone()),
            None => Err(Said::new(
                "backend_differs",
                copy_core::backend_differs(&it.machine.to_string()),
            )),
        },
        Some(Err(s)) => Err(s.clone()),
        None => Err(Said::from(copy_core::reply_unreadable(
            &it.machine.to_string(),
        ))),
    };
    let mut o = found.unwrap_or_else(|s| {
        let f =
            crate::wire::detail::failed_of(SessionsBatchCmd::NAME, s.code_or_fallback(), s.clone());
        SessionOutcome {
            machine: it.machine.clone(),
            sid: it.sid.clone(),
            outcome: SessionState::Failed,
            why: Some(f.code),
            said: Some(f.said),
            copy_detail: f.detail,
            // 那台没答出这一项：记录没读到那一档（[`crate::history::search_rules::session_line`]）。
            title: crate::history::search_rules::session_line(
                &crate::history::search_rules::LabelFacts::unread(it.sid.as_str(), None),
            ),
            session: None,
        }
    });
    o.machine = it.machine.clone();
    if let Some(name) = it.machine.host_name() {
        o.title = copy_text(
            "tabBatch.line.remote",
            &[("title", &o.title), ("machine", name)],
        );
    }
    o
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/sessions_batch_face_tests.rs"]
mod tests;
