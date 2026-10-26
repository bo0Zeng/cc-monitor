//! 线上的 agent 格（哪一家）一律是 [`AgentKind`]：逐格交给只收 `AgentKind` 的函数，哪一格还是串就编不过。
//! 线上字节不变由各金样钉着（transparent：序列化成的仍是那个串）。

use super::AgentKind;
use crate::listing::{HistoryGroup, HistoryRow, RawRow};
use crate::needs::{LoginStartReq, ManualItem};
use crate::record::Record;
use crate::search::SessionHits;
use crate::session::{ForkFacts, TermSession};

fn kind(_: &AgentKind) {}

fn maybe(_: &Option<AgentKind>) {}

/// 每一格 agent 都是 `AgentKind`（改成串的那一格当场编不过）。
#[allow(clippy::too_many_arguments)]
fn cells(
    h: &HistoryRow,
    g: &HistoryGroup,
    r: &RawRow,
    s: &SessionHits,
    m: &ManualItem,
    l: &LoginStartReq,
    t: &TermSession,
    f: &ForkFacts,
    rec: &Record,
) {
    kind(&h.agent);
    kind(&g.agent);
    kind(&r.agent);
    kind(&s.agent);
    if let ManualItem::AccountLogin { agent, .. } = m {
        kind(agent);
    }
    match l {
        LoginStartReq::Account { agent, .. } | LoginStartReq::Mcp { agent, .. } => kind(agent),
    }
    maybe(&t.agent);
    kind(&f.agent);
    kind(&rec.agent);
}

#[test]
fn every_wire_agent_cell_is_an_agent_kind() {
    let _ = cells as fn(_, _, _, _, _, _, _, _, _);
}

#[test]
fn an_agent_kind_goes_on_the_wire_as_the_bare_string() {
    let t: TermSession = serde_json::from_str(r#"{"sid":"s1","agent":"codex"}"#).unwrap();
    assert_eq!(t.agent, Some(AgentKind::new("codex")));
    assert_eq!(serde_json::to_string(&t).unwrap(), r#"{"sid":"s1","agent":"codex"}"#);
}
