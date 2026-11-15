//! 编辑已发的一句在这台的宿主壳：帧面 `session-rewind-options`（发送那一下弹的选项）与输入队投递那一下的回退（[`walk_here`]）。
//!
//! - 选项只读，不碰会话（改动全攒在发件那一端，只有确认那一下随那句进输入队）：会话所在那台照那一家申报的「怎么退」几项交出
//!   （`control/rewind.rs::options_of`），屏上主 agent 在出字（照那一家的认屏读，[`turn_running`]）才多给「什么时候退」，读不了屏 ⇒ 只写为什么（[`options_on`]）。会话不在这台 ⇒ 经输入队那一跳交给那台
//!   （[`super::session_queue_hop::route`]），够不到 ⇒ `offline`。
//! - 回退那一串的本体住控制面（`control/rewind.rs::walk`，只吃四样 io）；这里接上这台的那几样：看屏 · 按键同 `session-permission`
//!   那一道门，读回走冷开扫描图的用户输入（观测层），会话那一家与此刻在不在等人读活会话表。控制面与观测层互不引用，接在这一层。

use super::session_queue_hop::{route, Aimed};
use crate::common::said::Said;
use crate::control::rewind::{
    code_after, options_of, runs_after, walk, After, Plan, RewindFace, RewindIo, RewindScreen,
    UserLine, Walked, Why,
};
use crate::control::screen_drive::Drive;
use crate::control::terminals;
use crate::control::terminals_wire::{Sent, SentResult};
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

/// 那一屏上主 agent 在不在出字：照那一家申报的认屏（`RewindFace.read` 认成 `Busy`），与回退那一串第 1 步同一个认法。
/// 不看活会话表：只有后台子 agent 在跑、主 agent 空着时那一态也是在跑，屏上却是空着的输入行。
pub(crate) fn turn_running(face: &RewindFace, screen: &[String]) -> bool {
    matches!((face.read)(screen), RewindScreen::Busy)
}

/// 发送那一下的选项照那一屏摆：在出字 ⇒ 多给「什么时候退」。读不了屏（`Err` ＝ 为什么）⇒ 回退那一串每按一键都要先读屏，此刻退不了：
/// 只写为什么、一项不给（不拿「不忙」顶上，不替人选时机）。
pub(crate) fn options_on(
    face: &RewindFace,
    screen: Result<Vec<String>, String>,
    after: After,
) -> RewindOptions {
    match screen {
        Ok(lines) => options_of(Some(face), turn_running(face, &lines), after),
        Err(why) => RewindOptions {
            said: Some(why),
            ..RewindOptions::default()
        },
    }
}

/// 这台 `sid` 那条会话此刻那一屏（看屏同回退那一串那只只读客户端）：终端不收（没了 · 只剩 shell · 不归我们）· 取屏出错 ⇒ 为什么。
fn screen_here(sid: &str) -> Result<Vec<String>, String> {
    let mut drive = Drive::new(sid, None);
    match drive.open() {
        Ok(None) => drive.screen().map_err(|f| f.said),
        Ok(Some(why)) => Err(Walked::No(Why::Refused(terminals::input_reply(
            SentResult::Refused,
            Some(why),
            None,
        )))
        .said()
        .unwrap_or_else(|| copy_text("beQueue.no.notRewound", &[]))),
        Err(f) => Err(f.said),
    }
}

/// 这台 `sid` 那条会话那一家（按 sid 找到它的那一家）的回退菜单：这台没有它的记录 ⇒ `Err`；那一家没申报 ⇒ `Ok(None)`。
pub(crate) fn face_here(sid: &str) -> Result<Option<RewindFace>, String> {
    Ok(face_among(crate::agents::REGISTRY, &record_here(sid)?))
}

fn record_here(sid: &str) -> Result<crate::agents::SessionAt, String> {
    let home = crate::agents::local_homes();
    crate::observe::query::locate::session_at(&home, sid)
}

/// 按 sid 找到的那一条（`at`）那一家申报的回退菜单：那一家没申报 ⇒ `None`。
pub(crate) fn face_among(
    registry: &[crate::agents::Adapter],
    at: &crate::agents::SessionAt,
) -> Option<RewindFace> {
    crate::agents::launch_face_among(registry, at.kind).and_then(|f| f.rewind)
}

/// 这台那一份选项：记录在 ⇒ 那一家申报的几项 ＋ 屏上此刻在不在出字 ＋ 被编辑那一句之后的事；记录主线上没有那一句 ⇒ 只写为什么。
pub(crate) fn options_here(sid: &str, to: &str) -> Result<RewindOptions, Said> {
    let no_record = |raw: &str| {
        crate::wire::spec::fail(
            "no_such_session",
            copy_text("beSessionCommands.session.noRecord", &[]),
        )
        .and_raw(Some(raw))
    };
    let at = record_here(sid).map_err(|raw| no_record(&raw))?;
    let Some(face) = face_among(crate::agents::REGISTRY, &at) else {
        return Ok(options_of(None, false, After::default()));
    };
    // 这台的运行簿（watcher 起了才有；一次性的命令行那一形没有 ⇒ 不说子 agent）。
    let runs = crate::observe::watcher::run_book()
        .map(|b| b.runs(sid))
        .unwrap_or_default();
    let Some(after) = after_in(&at.file, at.kind, to, &runs).map_err(|raw| no_record(&raw))? else {
        return Ok(RewindOptions {
            said: Some(copy_text("beRewind.no.noSuchLine", &[])),
            ..RewindOptions::default()
        });
    };
    Ok(options_on(&face, screen_here(sid), after))
}

/// `kind` 那一家的 `record` 那份记录里、主线上 `to` 那一句之后的事：那一句起的主线记录里改没改过代码 · `runs` 里起于那一句之后、还在跑的几个。
/// 主线上没有那一句 ⇒ `None`；读不了 ⇒ `Err`（原样）。
pub(crate) fn after_in(
    record: &Path,
    kind: &'static str,
    to: &str,
    runs: &[cmd_wire::frame::RunInfo],
) -> Result<Option<After>, String> {
    let map = crate::observe::record_scan::global()
        .get(record, crate::observe::record_scan::Family::of(kind))
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
    let home = crate::agents::local_homes();
    if crate::history::live_table::now(&home).waiting(sid) {
        return Ok(Walked::Wait);
    }
    let Ok(Some(face)) = face_here(sid) else {
        return Ok(Walked::No(Why::Unsupported));
    };
    let mut io = Live(Drive::new(sid, client));
    if let Some(why) = io.0.open()? {
        return Ok(Walked::No(Why::Refused(terminals::input_reply(
            SentResult::Refused,
            Some(why),
            None,
        ))));
    }
    walk(&face, Plan { rewind, text }, &mut io)
}

/// 这台那一份 io：读屏驱动（抓屏不带色 · 按键送字走 `session-say` 那一道：只剩 shell 的窗格不收 · 等重画用实时预览那个只读客户端），读回问冷开扫描图。
struct Live<'a>(Drive<'a>);

impl RewindIo for Live<'_> {
    fn screen(&mut self) -> Result<Vec<String>, Said> {
        self.0.screen()
    }
    fn press(&mut self, key: &str) -> Result<Sent, Said> {
        self.0.press(key)
    }
    fn redrawn(&mut self) -> bool {
        self.0.redrawn()
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "调出回退菜单那两下之间隔一下（那一家申报的毫秒数：挨得太近会被并成一键）；调用方是那一趟回退，按完就返回，不是定时器"
    )]
    fn pause(&mut self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
    fn say(&mut self, text: &str) -> Result<Sent, Said> {
        self.0.say(text)
    }
    fn lines(&mut self) -> Result<Vec<UserLine>, Said> {
        let no_record = |raw: &str| {
            crate::wire::spec::fail(
                "no_such_session",
                copy_text("beSessionCommands.session.noRecord", &[]),
            )
            .and_raw(Some(raw))
        };
        let home = crate::agents::local_homes();
        // 那一家照按 sid 找到它的那一家（扫描图照它读，不照路径另认）。
        let at = crate::observe::query::locate::session_at(&home, self.0.sid())
            .map_err(|e| no_record(&e))?;
        let map = crate::observe::record_scan::global()
            .get(&at.file, crate::observe::record_scan::Family::of(at.kind))
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
