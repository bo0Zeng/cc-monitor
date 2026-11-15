//! 「终端」视图那两条（typed）与它的成品：`terminals-board-here`（这台把自己的终端名单写成行，[`rows_here`]）·
//! `terminals-board`（本机后端问到各台、合成一份，[`merge`]）。
//!
//! - 只列 cc-monitor 起的（名单那一行 `started_by.client` 有值）。
//! - 一台里的先后：在等人 → 有终端窗口连着 → 后台；同一档按最近动静新的在前，再按句柄（稳定）。
//! - 一台没答 ⇒ 那一组带那一句，行是那台上次答的那一份（点一律「状态不明」），不静默漏掉那一台。
//! - 字全在这里写：标题 · 第二行那几段 · 小签 · 行尾时刻 · 组头那一句 · 筛选四格的名字与数。

use crate::common::cells::Words;
use crate::control::terminals as t;
use crate::control::terminals_wire::TerminalsListReq;
use crate::host::tmux::On;
use crate::wire::typed::{self, codes, Command};
use cmd_wire::board::{
    BoardDown, BoardFilter, BoardGroup, BoardHere, BoardRow, BoardTag, TerminalsBoard,
};
use cmd_wire::frame::{SessionDot, SessionFace};
use cmd_wire::session::{TermState, TerminalItem};
use cmd_wire::Machine;
use copy_core::copy_text;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

/// `terminals-board-here` 的入参。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct BoardHereReq {
    /// 自报的前端（决定每行的 `can_end`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub(crate) client: Option<String>,
}

codes! {
    /// `terminals-board-here` 的码：列名单那两发 tmux。
    pub enum BoardHereErr {
        /// 列不出终端名单。
        Unobservable = "unobservable",
        /// 那一发 `tmux` 过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

/// `terminals-board-here`：这台的终端名单写成「终端」视图的那几行。阻塞档（起 tmux · 读活会话表）。
pub struct TerminalsBoardHereCmd;

impl Command for TerminalsBoardHereCmd {
    const NAME: &'static str = "terminals-board-here";
    type Req = BoardHereReq;
    type Reply = BoardHere;
    type Err = BoardHereErr;
}

impl crate::wire::typed::Handle for TerminalsBoardHereCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: BoardHereReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<BoardHere, BoardHereErr> {
        here(&req)
    }
}

/// [`TerminalsBoardHereCmd`] 的本体（同步：本机那一台在 `terminals-board` 里也经它答）。
pub(crate) fn here(req: &BoardHereReq) -> Result<BoardHere, BoardHereErr> {
    let list = t::listed_on(
        On::default(),
        &TerminalsListReq {
            client: req.client.clone(),
        },
    )
    .map_err(|f| match f.code.as_deref() {
        Some("no_tmux") => {
            BoardHereErr::Unobservable(crate::common::contract::joined(f.said, f.raw))
        }
        _ => typed::of_fail(f, BoardHereErr::Unobservable),
    })?;
    let agent_home = crate::agents::local_homes();
    let doing = crate::history::live_table::now(&agent_home).doing();
    let accounts = crate::accounts::manage::library::session_accounts_here(&agent_home);
    let annotations = match crate::history::history_annotations::load() {
        crate::history::history_annotations::Loaded::Read(t) => t,
        _ => Default::default(),
    };
    let session = |sid: &str| -> Option<SessionBits> {
        use crate::wire::SessionFaceExt as _;
        let activity = doing.get(sid)?;
        Some(SessionBits {
            title: crate::history::session_hidden::title_here(sid, &annotations),
            account: accounts
                .iter()
                .find(|a| a.sid.as_deref() == Some(sid))
                .and_then(|a| a.account.clone()),
            // 在等人、挂着的那一条（最早挂上的）界面答得了 ⇒ 待答请求那一颗，同标签页那张脸（换点只经 `asking`）。
            face: crate::wire::SessionFace::of(crate::wire::Life::Live {
                activity: *activity,
                host: Some(crate::wire::Housed::In(crate::wire::TerminalHost::Tmux)),
            })
            .asking(crate::wire::answerable(
                crate::history::needs_changed::pending_of(sid).first(),
            )),
        })
    };
    let now = crate::common::time::now_secs();
    Ok(rows_here(
        &list,
        &session,
        now,
        crate::platform::paths::home_dir().as_deref(),
    ))
}

/// `terminals-board` 的入参：问哪几台（本机逐字 `"<local>"`）· 自报的前端。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct BoardReq {
    pub(crate) machines: Vec<cmd_wire::Machine>,
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub(crate) client: Option<String>,
}

codes! {
    /// `terminals-board` 自己没有别的码（哪台没答只进那一组的 `down`）。
    pub enum BoardErr {}
}

/// `terminals-board`：本机后端问到各台的那几行（本机同进程答，别的台经常驻问 `terminals-board-here`），合成一份再答。
pub struct TerminalsBoardCmd;

impl Command for TerminalsBoardCmd {
    const NAME: &'static str = "terminals-board";
    type Req = BoardReq;
    type Reply = TerminalsBoard;
    type Err = BoardErr;
}

impl crate::wire::typed::Handle for TerminalsBoardCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: BoardReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<TerminalsBoard, BoardErr> {
        let ask = BoardHereReq {
            client: req.client.clone(),
        };
        let mine = ask.clone();
        let local = async move {
            tokio::task::spawn_blocking(move || here(&mine))
                .await
                .map_err(|e| crate::common::said::Said::from(e.to_string()))?
                .map_err(|e| {
                    use crate::wire::typed::Codes as _;
                    e.into_said()
                })
        };
        let got = crate::dial::remote_ask::ask_each::<TerminalsBoardHereCmd, _>(
            &req.machines,
            &ask,
            local,
            &cx.tz,
            cx.until,
            &crate::dial::remote_ask::REACH,
            &crate::dial::remote_ask::DialRemote,
        )
        .await
        .map_err(BoardErr::BadArgs)?;
        let now = crate::common::time::now_secs() as i64;
        let at = crate::common::time::fmt_at(now, now, &cx.tz);
        let got = got
            .into_iter()
            .map(|(m, r)| {
                let g = match r {
                    Ok(here) => Got::Answered(here, at.clone()),
                    Err(s) => Got::Down(s.said),
                };
                (m, g)
            })
            .collect();
        Ok(merge(got, &LAST_SEEN))
    }
}

/// 一行里那条会话这台知道的几样（由调用方从这台的会话事实里取）。
#[derive(Debug, Clone)]
pub struct SessionBits {
    pub title: String,
    pub account: Option<String>,
    pub face: SessionFace,
}

/// 在等人 0 · 有终端窗口连着 1 · 后台 2。
fn band(dot: SessionDot, attached: bool) -> u8 {
    match (dot.waits_for_human(), attached) {
        (true, _) => 0,
        (false, true) => 1,
        (false, false) => 2,
    }
}

/// 行尾那一格：距最近动静多久（短时长：`45s` · `6m` · `2h` · `3d`）。
pub(crate) fn at_text(last_activity: u64, now: u64) -> Words {
    Words(copy_core::short_duration(
        now.saturating_sub(last_activity).saturating_mul(1000),
    ))
}

/// 一个终端写成一行（不是 cc-monitor 起的 ⇒ `None`）。`last_activity` ＝ 它最近动静（秒）：排先后 · 行尾时刻。
fn row_of(
    t: &TerminalItem,
    last_activity: u64,
    session: &dyn Fn(&str) -> Option<SessionBits>,
    now: u64,
    home: Option<&Path>,
) -> Option<(u8, u64, BoardRow)> {
    t.started_by.client.as_ref()?;
    let attached = !t.clients.is_empty();
    let sid = t.session.as_ref().map(|s| s.sid.clone());
    let bits = sid.as_deref().and_then(session);
    let cwd = copy_core::home_short(&t.cwd, home);
    let (dot, dot_name, title, mono, badge, first) = match (&sid, &bits) {
        (Some(_), Some(b)) => (
            b.face.dot,
            b.face.name.clone(),
            b.title.clone(),
            false,
            b.face.dot.waits_for_human().then(|| b.face.name.clone()),
            b.account.clone().map(Words),
        ),
        (Some(_), None) => {
            let face = fallback_face(t.state);
            (
                face.dot,
                face.name.clone(),
                t.title.clone(),
                false,
                None,
                None,
            )
        }
        (None, _) => {
            let dot = if crate::platform::shell::is_shell_program(&t.program) {
                SessionDot::Idle
            } else {
                SessionDot::Running
            };
            let name = if dot == SessionDot::Idle {
                copy_text("beSession.activity.idle", &[])
            } else {
                copy_text("backend.status.connected", &[])
            };
            (
                dot,
                Words(name),
                t.title.clone(),
                true,
                None,
                Some(Words(copy_text("beTermBoard.meta.noSession", &[]))),
            )
        }
    };
    let meta = Words(match &first {
        Some(who) => copy_text("beTermBoard.meta.line", &[("who", &who.0), ("cwd", &cwd)]),
        None => cwd.clone(),
    });
    let mut chips = Vec::new();
    if attached {
        chips.push(Words(copy_text(
            "terminal.tag.windows",
            &[("n", &t.clients.len().to_string())],
        )));
    } else {
        chips.push(Words(copy_text("terminal.tag.background", &[])));
    }
    if sid.is_some() && t.state == TermState::ProgramExited {
        chips.push(dot_name.clone());
    }
    let mut tags = vec![if sid.is_some() {
        BoardTag::Session
    } else {
        BoardTag::Bare
    }];
    if !attached {
        tags.push(BoardTag::Background);
    }
    let find = format!("{title}\n{cwd}\n{}", t.name).to_lowercase();
    Some((
        band(dot, attached),
        last_activity,
        BoardRow {
            key: t.terminal.clone(),
            terminal: t.terminal.clone(),
            name: t.name.clone(),
            sid,
            dot,
            dot_name,
            title: Words(title),
            window_title: Words::default(),
            mono,
            badge,
            meta,
            chips,
            at_text: at_text(last_activity, now),
            tags,
            find,
            can_end: t.can.end.clone(),
        },
    ))
}

/// 名单里挂着 sid、这台却说不出那条会话的：程序已退出 ⇒ 可接回那一张；否则「状态不明」。
fn fallback_face(state: TermState) -> SessionFace {
    use crate::wire::SessionFaceExt as _;
    match state {
        TermState::ProgramExited => SessionFace::of(crate::wire::Life::Reconnectable),
        _ => SessionFace::of(crate::wire::Life::Unclear),
    }
}

/// 这台的终端名单写成那几行、排好（`session` ＝ 按 sid 取这台的会话事实；`home` ＝ 这台家目录，目录写成 `~/…`）。
pub fn rows_here(
    list: &t::TermRows,
    session: &dyn Fn(&str) -> Option<SessionBits>,
    now: u64,
    home: Option<&Path>,
) -> BoardHere {
    let mut rows: Vec<(u8, u64, BoardRow)> = list
        .rows
        .iter()
        .filter_map(|r| row_of(&r.item, r.last_activity, session, now, home))
        .collect();
    rows.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.2.key.cmp(&b.2.key))
    });
    BoardHere {
        rows: rows.into_iter().map(|r| r.2).collect(),
        complete: list.complete,
    }
}

/// 一台问到的结局：答了（那份 ＋ 答的时刻 `HH:MM`）· 没答（那一句）。
pub enum Got {
    Answered(BoardHere, String),
    Down(String),
}

/// 各台上次答的那一份（没答时照它画、标「名单采样」）：进程里一份，只记最后一次。
pub struct LastSeen(Mutex<HashMap<String, (BoardHere, String)>>);

impl Default for LastSeen {
    fn default() -> Self {
        Self::new()
    }
}

impl LastSeen {
    pub fn new() -> Self {
        LastSeen(Mutex::new(HashMap::new()))
    }
}

pub static LAST_SEEN: std::sync::LazyLock<LastSeen> = std::sync::LazyLock::new(LastSeen::new);

/// 组头那一句。
fn sub_of(rows: &[BoardRow]) -> Words {
    let n = rows.len();
    let bg = rows
        .iter()
        .filter(|r| r.tags.contains(&BoardTag::Background))
        .count();
    if bg == 0 {
        Words(copy_text(
            "beTermBoard.group.count",
            &[("n", &n.to_string())],
        ))
    } else {
        Words(copy_text(
            "beTermBoard.group.countBg",
            &[("n", &n.to_string()), ("bg", &bg.to_string())],
        ))
    }
}

/// 各台的结局合成一份（照 `got` 的先后，一台一组）；答了的记进 `last`，没答的照 `last` 里那一份画。
pub fn merge(got: Vec<(Machine, Got)>, last: &LastSeen) -> TerminalsBoard {
    let mut seen = last.0.lock().unwrap_or_else(|e| e.into_inner());
    let mut groups = Vec::new();
    for (machine, g) in got {
        let prefix = format!("{machine}\u{0}");
        let (here, down) = match g {
            Got::Answered(here, at) => {
                seen.insert(machine.to_string(), (here.clone(), at));
                (here, None)
            }
            Got::Down(said) => {
                let name = copy_core::machine_word(&machine);
                match seen.get(&machine.to_string()) {
                    Some((old, at)) => {
                        let mut old = old.clone();
                        for r in &mut old.rows {
                            stale(r, &name);
                        }
                        let said = copy_text(
                            "beTermBoard.group.downSampled",
                            &[("machine", &name), ("at", at)],
                        );
                        (old, Some(BoardDown { said: Words(said) }))
                    }
                    None => {
                        let said = copy_text(
                            "beTermBoard.group.down",
                            &[("machine", &name), ("said", &said)],
                        );
                        (
                            BoardHere {
                                rows: vec![],
                                complete: true,
                            },
                            Some(BoardDown { said: Words(said) }),
                        )
                    }
                }
            }
        };
        let name = copy_core::machine_word(&machine);
        let rows: Vec<BoardRow> = here
            .rows
            .into_iter()
            .map(|mut r| {
                r.key = format!("{prefix}{}", r.key);
                r.window_title = Words(copy_text(
                    "beTermBoard.window.title",
                    &[("title", &r.title.0), ("machine", &name)],
                ));
                r
            })
            .collect();
        groups.push(BoardGroup {
            name: Words(copy_core::machine_word(&machine)),
            sub: sub_of(&rows),
            machine,
            down,
            complete: here.complete,
            rows,
        });
    }
    let count = |tag: BoardTag| -> u32 {
        let n = groups
            .iter()
            .flat_map(|g| g.rows.iter())
            .filter(|r| tag == BoardTag::All || r.tags.contains(&tag))
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    };
    let filters = [
        (BoardTag::All, copy_text("extPage.filter.all", &[])),
        (
            BoardTag::Session,
            copy_text("beTermBoard.filter.session", &[]),
        ),
        (BoardTag::Bare, copy_text("beTermBoard.filter.bare", &[])),
        (
            BoardTag::Background,
            copy_text("terminal.tag.background", &[]),
        ),
    ]
    .into_iter()
    .map(|(tag, label)| BoardFilter {
        tag,
        label: Words(label),
        count: count(tag),
    })
    .collect();
    TerminalsBoard { filters, groups }
}

/// 上次那一份里的一行：那台此刻看不见 ⇒ 点照「状态不明」、去掉在等的徽标。
fn stale(r: &mut BoardRow, name: &str) {
    let face = face_core::book::unseen(Words(copy_core::offline(name)));
    r.dot = face.dot;
    r.dot_name = face.name;
    r.badge = None;
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/terminals_board_face_tests.rs"]
mod tests;
