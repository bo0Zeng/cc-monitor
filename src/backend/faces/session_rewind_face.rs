//! 编辑已发的一句在这台的宿主壳：帧面 `session-rewind-options`（发送那一下弹的选项）与输入队投递那一下的回退（[`walk_here`]）。
//!
//! - 选项只读，不碰会话（改动全攒在发件那一端，只有确认那一下随那句进输入队）：会话所在那台照那一家申报的「怎么退」几项交出
//!   （`control/rewind.rs::options_of`），主 agent 在干活才多给「什么时候退」。会话不在这台 ⇒ 经输入队那一跳交给那台
//!   （[`super::session_queue_hop::route`]），够不到 ⇒ `offline`。
//! - 回退那一串的本体住控制面（`control/rewind.rs::walk`，只吃四样 io）；这里接上这台的那几样：看屏 · 按键同 `session-permission`
//!   那一道门，读回走冷开扫描图的用户输入（观测层），会话那一家与此刻在干什么读活会话表。控制面与观测层互不引用，接在这一层。

use super::session_queue_hop::{route, Aimed};
use crate::common::said::Said;
use crate::control::rewind::{
    code_after, options_of, runs_after, walk, After, Plan, RewindFace, RewindIo, UserLine, Walked,
    Why,
};
use crate::control::terminals::{self, On};
use crate::control::terminals_wire::{Sent, SentResult, TerminalInputReq, TerminalPreviewReq};
use crate::dial::remote_ask::{DialRemote, REACH};
use crate::observe::lines::Lines;
use crate::platform::fs::Ro;
use crate::wire::typed::{self, codes, Codes as _, Command};
use cmd_wire::rewind::{Rewind, RewindOptions, SessionRewindOptionsReq};
use cmd_wire::Machine;
use copy_core::copy_text;
use std::io::{Seek as _, SeekFrom};
use std::path::Path;

codes! {
    /// `session-rewind-options` 的码。
    pub enum SessionRewindOptionsErr {
        /// 那台找不到这条会话的记录。
        NoSuchSession = "no_such_session",
        /// 会话所在那台够不到（不在机器表 · 链路坏 · 常驻没在跑 · 过了期限）。
        Offline = "offline",
    }
}

impl Aimed for SessionRewindOptionsReq {
    fn machine(&self) -> &Machine {
        &self.machine
    }
    fn here(&self) -> Self {
        Self {
            machine: Machine::local(),
            ..self.clone()
        }
    }
}

/// `session-rewind-options`：发送那一下给哪几项（问会话所在那台）。
pub struct SessionRewindOptionsCmd;

impl Command for SessionRewindOptionsCmd {
    const NAME: &'static str = "session-rewind-options";
    type Req = SessionRewindOptionsReq;
    type Reply = RewindOptions;
    type Err = SessionRewindOptionsErr;
}

impl typed::Handle for SessionRewindOptionsCmd {
    type Needs = typed::needs::Nothing;
    async fn run(
        req: SessionRewindOptionsReq,
        cx: typed::Cx<Self::Needs>,
    ) -> Result<RewindOptions, SessionRewindOptionsErr> {
        let sid = req.sid.to_string();
        let to = req.to.clone();
        route::<Self, _>(
            &req,
            move || options_here(&sid, &to),
            SessionRewindOptionsErr::ALL,
            &cx.tz,
            cx.until,
            &REACH,
            &DialRemote,
        )
        .await
        .map_err(|f| typed::of_fail(f, SessionRewindOptionsErr::NoSuchSession))
    }
}

/// 这台 `sid` 那条会话此刻主 agent 在不在干活（只有后台在跑 · 在等人都不算：等人那一种照队的规矩等）。
pub(crate) fn busy_here(sid: &str) -> bool {
    let home = crate::history::listing::agent_home();
    crate::history::live_table::now(&home).doing_one(sid)
        == Some(Some(crate::agents::SessionActivity::Working))
}

/// 这台 `sid` 那条会话那一家的回退菜单：这台没有它的记录 ⇒ `Err`；那一家没申报 ⇒ `Ok(None)`
/// （记录认不出是哪一家 ⇒ 照缺省那一家，同 `session-permission`）。
pub(crate) fn face_here(sid: &str) -> Result<Option<RewindFace>, String> {
    Ok(face_of(&record_here(sid)?))
}

fn record_here(sid: &str) -> Result<std::path::PathBuf, String> {
    let home = crate::history::listing::agent_home();
    crate::observe::query::locate::session_record(&home, sid)
}

fn face_of(record: &Path) -> Option<RewindFace> {
    crate::agents::rewind_face(crate::agents::record_kind_of(record))
}

/// 这台那一份选项：记录在 ⇒ 那一家申报的几项 ＋ 此刻在不在干活 ＋ 被编辑那一句之后的事；记录主线上没有那一句 ⇒ 只写为什么。
pub(crate) fn options_here(sid: &str, to: &str) -> Result<RewindOptions, Said> {
    let no_record = |raw: &str| {
        crate::wire::spec::fail(
            "no_such_session",
            copy_text("beSessionCommands.session.noRecord", &[]),
        )
        .and_raw(Some(raw))
    };
    let record = record_here(sid).map_err(|raw| no_record(&raw))?;
    let face = face_of(&record);
    if face.is_none() {
        return Ok(options_of(None, false, After::default()));
    }
    // 这台的运行簿（watcher 起了才有；一次性的命令行那一形没有 ⇒ 不说子 agent）。
    let runs = crate::observe::watcher::run_book()
        .map(|b| b.runs(sid))
        .unwrap_or_default();
    let Some(after) = after_in(&record, to, &runs).map_err(|raw| no_record(&raw))? else {
        return Ok(RewindOptions {
            said: Some(copy_text("beRewind.no.noSuchLine", &[])),
            ..RewindOptions::default()
        });
    };
    Ok(options_of(face.as_ref(), busy_here(sid), after))
}

/// `record` 那份记录里、主线上 `to` 那一句之后的事：那一句起的主线记录里改没改过代码 · `runs` 里起于那一句之后、还在跑的几个。
/// 主线上没有那一句 ⇒ `None`；读不了 ⇒ `Err`（原样）。
pub(crate) fn after_in(
    record: &Path,
    to: &str,
    runs: &[cmd_wire::frame::RunInfo],
) -> Result<Option<After>, String> {
    let map = crate::observe::record_scan::global()
        .get(record)
        .map_err(|e| e.to_string())?;
    let Some(line) = map.inputs.iter().find(|r| r.uuid == to) else {
        return Ok(None);
    };
    // 读行走观测层那一份（同冷开扫描图）：从那一句起，读到扫描图扫到的那一行为止。
    let mut file = Ro::open(record).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(line.at))
        .map_err(|e| e.to_string())?;
    let mut rows = Lines::new(std::io::BufReader::new(file), line.at).upto(map.end);
    let mut records = Vec::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        records.extend(crate::observe::lines::parse(row.body()));
    }
    let kind = crate::agents::pick_kind(crate::agents::record_kind_of(record))
        .map(|(k, _)| k)
        .map_err(|e| e.to_string())?;
    let since =
        crate::common::time::parse_iso8601_ms(&line.timestamp).and_then(|t| u64::try_from(t).ok());
    Ok(Some(After {
        code: code_after(kind, &records, &map.off),
        runs: runs_after(runs, since),
    }))
}

/// 输入队投递那一下的回退（随那一句带了 [`Rewind`] 的那一条）：这台 `sid` 那条会话里替人走一趟 [`walk`]。
/// 在等人（批准 · 回答框开着）⇒ 等，同送字本体；这台没有它的记录 · 那一家没申报 ⇒ 不支持（一个键不按）。
pub(crate) fn walk_here(
    sid: &str,
    rewind: &Rewind,
    text: &str,
    client: Option<String>,
) -> Result<Walked, Said> {
    let home = crate::history::listing::agent_home();
    if crate::history::live_table::now(&home).waiting(sid) {
        return Ok(Walked::Wait);
    }
    let Ok(Some(face)) = face_here(sid) else {
        return Ok(Walked::No(Why::Unsupported));
    };
    let mut io = Live {
        sid,
        client,
        watch: None,
    };
    let t = match terminals::say_target_on(On::default(), sid, io.client.as_deref())? {
        Ok(t) => t,
        Err(why) => {
            return Ok(Walked::No(Why::Refused(terminals::input_reply(
                SentResult::Refused,
                Some(why),
                None,
            ))))
        }
    };
    io.watch = Some(crate::control::terminal_follow::Watch::open(None, &t)?);
    walk(&face, Plan { rewind, text }, &mut io)
}

/// 这台那一份 io：抓屏走 `terminal-preview`（不带色），按键 · 送字走 `session-say` 那一道（只剩 shell 的窗格不收），等重画用实时预览那个只读客户端。
struct Live<'a> {
    sid: &'a str,
    client: Option<String>,
    watch: Option<crate::control::terminal_follow::Watch>,
}

impl Live<'_> {
    fn input(&mut self, key: Option<&str>, text: Option<&str>) -> Result<Sent, Said> {
        if let Some(w) = self.watch.as_mut() {
            w.forget();
        }
        crate::control::terminals_wire::say(&TerminalInputReq {
            sid: Some(self.sid.to_string()),
            key: key.map(str::to_string),
            text: text.map(str::to_string),
            enter: text.map(|_| true),
            client: self.client.clone(),
            ..Default::default()
        })
    }
}

impl RewindIo for Live<'_> {
    fn screen(&mut self) -> Result<Vec<String>, Said> {
        let shot = terminals::preview_on(
            On::default(),
            &TerminalPreviewReq {
                sid: Some(self.sid.to_string()),
                color: Some(false),
                ..Default::default()
            },
            &crate::common::time::Tz::default(),
        )?;
        Ok(shot.lines.into_iter().map(|l| l.text).collect())
    }
    fn press(&mut self, key: &str) -> Result<Sent, Said> {
        self.input(Some(key), None)
    }
    fn redrawn(&mut self) -> bool {
        self.watch.as_mut().is_some_and(|w| w.changed())
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "调出回退菜单那两下之间隔一下（那一家申报的毫秒数：挨得太近会被并成一键）；调用方是那一趟回退，按完就返回，不是定时器"
    )]
    fn pause(&mut self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
    fn say(&mut self, text: &str) -> Result<Sent, Said> {
        self.input(None, Some(text))
    }
    fn lines(&mut self) -> Result<Vec<UserLine>, Said> {
        let no_record = |raw: &str| {
            crate::wire::spec::fail(
                "no_such_session",
                copy_text("beSessionCommands.session.noRecord", &[]),
            )
            .and_raw(Some(raw))
        };
        let home = crate::history::listing::agent_home();
        let path = crate::observe::query::locate::session_record(&home, self.sid)
            .map_err(|e| no_record(&e))?;
        let map = crate::observe::record_scan::global()
            .get(&path)
            .map_err(|e| no_record(&e.to_string()))?;
        Ok(map
            .inputs
            .iter()
            .map(|r| UserLine {
                uuid: r.uuid.clone(),
                text: r.excerpt.clone(),
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_rewind_face_tests.rs"]
mod tests;
