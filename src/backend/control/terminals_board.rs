//! 「终端」视图那张名单（成品）：每台把自己的终端名单写成行（[`rows_here`]），本机后端把各台合成一份（[`merge`]）。
//!
//! - 只列 cc-monitor 起的（名单那一行 `started_by.client` 有值）。
//! - 一台里的先后：在等人 → 有终端窗口连着 → 后台；同一档按最近动静新的在前，再按句柄（稳定）。
//! - 一台没答 ⇒ 那一组带那一句，行是那台上次答的那一份（点一律「状态不明」），不静默漏掉那一台。
//! - 字全在这里写：标题 · 第二行那几段 · 小签 · 行尾时刻 · 组头那一句 · 筛选四格的名字与数。

use crate::common::cells::Words;
use cmd_wire::board::{
    BoardDown, BoardFilter, BoardGroup, BoardHere, BoardRow, BoardTag, TerminalsBoard,
};
use cmd_wire::frame::{SessionDot, SessionFace};
use cmd_wire::session::{TermState, TerminalItem, TerminalsList};
use cmd_wire::Machine;
use copy_core::copy_text;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

/// 一行里那条会话这台知道的几样（由调用方从这台的会话事实里取）。
#[derive(Debug, Clone)]
pub(crate) struct SessionBits {
    pub(crate) title: String,
    pub(crate) account: Option<String>,
    pub(crate) face: SessionFace,
}

/// 前台程序名是这几个之一 ⇒ 没会话的那个终端算闲着（只剩 shell）。只比 tmux 报的前台程序名，不起 shell。
const SHELLS: &str = "bash zsh fish sh dash ksh tcsh csh nu pwsh powershell";

fn is_shell(program: &str) -> bool {
    let p = program.trim_start_matches('-');
    SHELLS.split_whitespace().any(|s| s == p)
}

/// 在等人 0 · 有终端窗口连着 1 · 后台 2。
fn band(dot: SessionDot, attached: bool) -> u8 {
    match (dot, attached) {
        (SessionDot::NeedsYou, _) => 0,
        (_, true) => 1,
        (_, false) => 2,
    }
}

/// 行尾那一格：距最近动静多久（短时长：`45s` · `6m` · `2h` · `3d`）。
pub(crate) fn at_text(last_activity: u64, now: u64) -> Words {
    Words(copy_core::short_duration(
        now.saturating_sub(last_activity).saturating_mul(1000),
    ))
}

/// 一个终端写成一行（不是 cc-monitor 起的 ⇒ `None`）。
fn row_of(
    t: &TerminalItem,
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
            (b.face.dot == SessionDot::NeedsYou).then(|| b.face.name.clone()),
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
            let dot = if is_shell(&t.program) {
                SessionDot::Idle
            } else {
                SessionDot::Running
            };
            let name = if dot == SessionDot::Idle {
                copy_text("beTermBoard.dot.shell", &[])
            } else {
                copy_text("beTermBoard.dot.program", &[])
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
            "beTermBoard.chip.windows",
            &[("n", &t.clients.len().to_string())],
        )));
    } else {
        chips.push(Words(copy_text("beTermBoard.chip.background", &[])));
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
        t.last_activity,
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
            at_text: at_text(t.last_activity, now),
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
pub(crate) fn rows_here(
    list: &TerminalsList,
    session: &dyn Fn(&str) -> Option<SessionBits>,
    now: u64,
    home: Option<&Path>,
) -> BoardHere {
    let mut rows: Vec<(u8, u64, BoardRow)> = list
        .terminals
        .iter()
        .filter_map(|t| row_of(t, session, now, home))
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
pub(crate) enum Got {
    Answered(BoardHere, String),
    Down(String),
}

/// 各台上次答的那一份（没答时照它画、标「名单采样」）：进程里一份，只记最后一次。
pub(crate) struct LastSeen(Mutex<HashMap<String, (BoardHere, String)>>);

impl LastSeen {
    pub(crate) fn new() -> Self {
        LastSeen(Mutex::new(HashMap::new()))
    }
}

pub(crate) static LAST_SEEN: std::sync::LazyLock<LastSeen> =
    std::sync::LazyLock::new(LastSeen::new);

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

/// 那台叫什么（本机 ⇒「本机」）。
fn name_of(m: &Machine) -> Words {
    if m.is_local() {
        Words(copy_text("beTermBoard.group.local", &[]))
    } else {
        Words(m.to_string())
    }
}

/// 各台的结局合成一份（照 `got` 的先后，一台一组）；答了的记进 `last`，没答的照 `last` 里那一份画。
pub(crate) fn merge(got: Vec<(Machine, Got)>, last: &LastSeen) -> TerminalsBoard {
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
                let name = name_of(&machine).0;
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
        let name = name_of(&machine).0;
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
            name: name_of(&machine),
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
        (BoardTag::All, copy_text("beTermBoard.filter.all", &[])),
        (
            BoardTag::Session,
            copy_text("beTermBoard.filter.session", &[]),
        ),
        (BoardTag::Bare, copy_text("beTermBoard.filter.bare", &[])),
        (
            BoardTag::Background,
            copy_text("beTermBoard.filter.background", &[]),
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
    let face = resident_client::book::unseen(Words(comms_inward::chan::said::offline(name)));
    r.dot = face.dot;
    r.dot_name = face.name;
    r.badge = None;
}

#[cfg(test)]
#[path = "../../../tests/backend/control/terminals_board_tests.rs"]
mod tests;
