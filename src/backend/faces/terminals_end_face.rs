//! 帧面 `terminals-end`（typed，只在本机后端答全）：终端名单「结束所选」· 起会话占位［结束］那一批 —— 收全部句柄、按台分组、
//! 本机那一组同进程逐项结束（`terminal-end` 的本体 `control/kill.rs::end_terminal`），别的台各拿自己那一组问它常驻的
//! `terminals-end`（那一份只含它自己的终端），逐条结局照入参原序摆好，合那一句交 `batch_core::sessions::summary_of`（同一批会话那一句）。
//!
//! 那台够不着 · 常驻不在 · 太旧 · 答了失败 ⇒ 那一组每一项都是 `failed`，那一句与复制详情照那一台的失败写。

use crate::common::said::Said;
use crate::control::kill::{TerminalEndReq, TerminalEnded};
use crate::control::session_batch as batch;
use crate::dial::remote_ask::{Remote, Table};
use crate::wire::typed::{codes, Command};
use batch_core::sessions::{summary_of, Row, Summary};
use cmd_wire::batch::{SessionState, SessionsOp};
use cmd_wire::board::{TerminalOutcome, TerminalsEndItem, TerminalsEndReply, TerminalsEndReq};
use cmd_wire::Machine;
use copy_core::copy_text;
use std::future::Future;

codes! {
    /// `terminals-end` 自己没有别的码（哪台没成只进那一组的逐条结局）。
    pub(crate) enum TerminalsEndErr {}
}

/// `terminals-end`：一批终端跨几台（本机同进程结束，别的台经常驻问它自己那一组），合成一份再答。
pub(crate) struct TerminalsEndCmd;

impl Command for TerminalsEndCmd {
    const NAME: &'static str = "terminals-end";
    type Req = TerminalsEndReq;
    type Reply = TerminalsEndReply;
    type Err = TerminalsEndErr;
}

impl crate::wire::typed::Handle for TerminalsEndCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: TerminalsEndReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl Future<Output = Result<TerminalsEndReply, TerminalsEndErr>> + Send + 'static {
        async move {
            let until = cx.until;
            across(
                req,
                |r| here(r, until),
                &cx.tz,
                until,
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
            )
            .await
            .map_err(TerminalsEndErr::BadArgs)
        }
    }
}

/// 本机那一组：先读一次这台「终端」视图那几行（标题照它），再逐项结束（每项装 `terminal-end` 那一档期限）。
async fn here(
    req: TerminalsEndReq,
    until: Option<crate::platform::child::Until>,
) -> Result<TerminalsEndReply, Said> {
    let results = tokio::task::spawn_blocking(move || {
        let board = crate::faces::terminals_board_face::here(
            &crate::faces::terminals_board_face::BoardHereReq {
                client: req.client.clone(),
            },
        );
        let title_of = |terminal: &str| -> String {
            board
                .as_ref()
                .ok()
                .and_then(|b| b.rows.iter().find(|r| r.terminal == terminal))
                .map_or_else(|| terminal.to_string(), |r| r.title.0.clone())
        };
        req.items
            .iter()
            .map(|it| {
                one(&it.terminal, title_of(&it.terminal), || {
                    let _budget = crate::platform::child::Budget::capped(
                        crate::control::terminals::TERMINAL_PREVIEW_CAP,
                        until,
                    );
                    crate::control::kill::end_terminal(
                        crate::host::tmux::On::default(),
                        crate::host::managed::installed(),
                        &TerminalEndReq {
                            terminal: Some(it.terminal.clone()),
                            sid: None,
                            client: req.client.clone(),
                        },
                    )
                })
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| Said::from(e.to_string()))?;
    Ok(reply_of(results))
}

/// 结束一项：成了 ⇒ 成了；那台拒了 ⇒ 没成（码 · 那一句 · 复制详情）。每台那一跳答的是它自己 ⇒ `machine` 是本机那一个。
pub(crate) fn one(
    terminal: &str,
    title: String,
    end: impl FnOnce() -> Result<TerminalEnded, Said>,
) -> TerminalOutcome {
    let mut o = TerminalOutcome {
        machine: Machine::local(),
        terminal: terminal.to_string(),
        outcome: SessionState::Done,
        why: None,
        said: None,
        copy_detail: String::new(),
        title,
    };
    if let Err(s) = end() {
        let f = crate::wire::detail::failed_of("terminal-end", s.code_or_fallback(), s.clone());
        o.outcome = SessionState::Failed;
        o.why = Some(f.code);
        o.said = Some(f.said);
        o.copy_detail = f.detail;
    }
    o
}

/// 逐条结局（照入参原序，远端的标题已套好机器名）⇒ 合好的回包；那一句同结束一批会话（`summary_of`）。
pub(crate) fn reply_of(results: Vec<TerminalOutcome>) -> TerminalsEndReply {
    let verb = crate::faces::sessions_batch_face::verb_of(&SessionsOp::Stop);
    let rows: Vec<Row> = results
        .iter()
        .map(|o| Row {
            title: &o.title,
            state: o.outcome,
            said: o.said.as_deref(),
            detail: &o.copy_detail,
        })
        .collect();
    let Summary {
        said,
        failed,
        lines,
        copy_detail,
    } = summary_of(&verb, &rows);
    TerminalsEndReply {
        results,
        said,
        failed,
        lines,
        copy_detail,
    }
}

/// 合各台：校验 → 按台分组（照第一次出现的先后）→ 每台各问自己那一组 → 逐条照入参原序摆好、远端的标题套机器名 → 合那一句。
/// `local` ＝ 本机那一组怎么做（生产 [`here`]，判据给替身）；入参不对 ⇒ `Err`（`bad_args` 那一句）。
pub(crate) async fn across<'a, L, F>(
    req: TerminalsEndReq,
    local: L,
    tz: &'a crate::Tz,
    until: Option<crate::platform::child::Until>,
    table: &'a Table,
    remote: &'a dyn Remote,
) -> Result<TerminalsEndReply, String>
where
    L: FnOnce(TerminalsEndReq) -> F,
    F: Future<Output = Result<TerminalsEndReply, Said>> + Send + 'a,
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
                it.terminal, it.machine
            )));
        }
    }
    let mut groups: Vec<(Machine, TerminalsEndReq)> = Vec::new();
    for it in &req.items {
        let mine = TerminalsEndItem {
            machine: Machine::local(),
            terminal: it.terminal.clone(),
        };
        match groups.iter_mut().find(|(m, _)| *m == it.machine) {
            Some((_, g)) => g.items.push(mine),
            None => groups.push((
                it.machine.clone(),
                TerminalsEndReq {
                    items: vec![mine],
                    client: req.client.clone(),
                },
            )),
        }
    }
    let got = crate::dial::remote_ask::ask_apiece::<TerminalsEndCmd, _, _, _>(
        groups, local, tz, until, table, remote,
    )
    .await?;
    let answered =
        |it: &TerminalsEndItem| got.iter().find(|(m, _)| *m == it.machine).map(|(_, r)| r);
    let results = req
        .items
        .iter()
        .map(|it| outcome_of(it, answered(it)))
        .collect();
    Ok(reply_of(results))
}

/// 一项的结局：那台答回来的那一条（换成这一项的那台、远端的标题套机器名）；那台没成 ⇒ 那台那一份失败。
fn outcome_of(
    it: &TerminalsEndItem,
    answered: Option<&Result<TerminalsEndReply, Said>>,
) -> TerminalOutcome {
    let found = match answered {
        Some(Ok(r)) => match r.results.iter().find(|o| o.terminal == it.terminal) {
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
            crate::wire::detail::failed_of(TerminalsEndCmd::NAME, s.code_or_fallback(), s.clone());
        TerminalOutcome {
            machine: it.machine.clone(),
            terminal: it.terminal.clone(),
            outcome: SessionState::Failed,
            why: Some(f.code),
            said: Some(f.said),
            copy_detail: f.detail,
            title: it.terminal.clone(),
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
#[path = "../../../tests/backend/faces/terminals_end_face_tests.rs"]
mod tests;
