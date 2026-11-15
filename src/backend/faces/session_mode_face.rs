//! 帧面 `session-permission` 的宿主壳：切权限档。这台已经没有它的终端 ⇒ 记在这条会话上、下次恢复照它起（同 `session-model`）。开着时照那一家声明的那一形切（适配层 `PermissionFace.switch`）：
//! - 按键轮换：读此刻的档 · 不是 ⇒ 按一下 · 等它重画 · 再读，到了为止，最多按一圈。认不出 · 不在循环里 · 按了没变都照实说「结果未知」。
//! - 投一句命令：送那一家给这一档的那一句（同 `session-say` 那一道门）；送到了也读不回此刻的档 ⇒ 照实说「结果未知」。
//!
//! 在等人都不按、不送。

use super::session_model_face::ModelDrive;
use crate::agents::{ModeCycle, ModeSwitch};
use crate::common::contract::malformed;
use crate::common::said::Said;
use crate::control::screen_drive::{settle, Drive, REDRAWS_PER_PRESS};
use crate::control::terminals::{self, no_said, Refused};
use crate::control::terminals_wire::{Sent, SentResult};
use crate::wire::typed::{self, Command};

/// `session-permission` 的入参：线上形住 `cmd-wire::session`（名字与回包挂在 `cmd_wire::Call` 上）。
pub use cmd_wire::session::SessionPermissionReq;

/// `session-permission` 的回包：线上形住 `cmd-wire::session`。
pub(crate) use cmd_wire::session::ModeSwitched;

/// 切模型 · 切档的结局：线上形住 `cmd-wire::session`。
pub(crate) use cmd_wire::session::SwitchResult;

typed::codes! {
    /// `session-permission` 的码：认会话 · 列名单 · 起看屏的客户端 · 抓屏 · 送键。在等人 · 认不出 · 不归我们不是错（`result`）。
    pub enum SessionPermissionErr {
        /// 那一家会话开着时切不了档。
        Unsupported = "unsupported",
        /// 认得出这一档，却只能起会话时带参数定（开着时那一圈里没有它）。
        LaunchOnly = "launch_only",
        /// 名单里此刻没有它。
        NotKnown = "not_known",
        /// 不止一个终端挂着这个 sid。
        Ambiguous = "ambiguous",
        /// tmux 起不来。
        NoTmux = "no_tmux",
        /// tmux 不到 3.2（看不了屏有没有变）。
        TmuxTooOld = "tmux_too_old",
        /// 一个 tmux server 都没有。
        NoServer = "no_server",
        /// 那个终端在读的时候没了。
        NoSuchSession = "no_such_session",
        /// 抓屏失败。
        CaptureFailed = "capture_failed",
        /// 列名单失败。
        Unobservable = "unobservable",
        /// 那一发 `tmux` 过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

/// 切档那三样副作用（判据拿合成的屏喂）。
pub(crate) trait ModeIo {
    /// 起看屏的那一份（只在真要按的时候起）；终端不收（没了 · 只剩 shell · 不归我们）⇒ 原因。
    fn open(&mut self) -> Result<Option<Refused>, Said>;
    /// 此刻的档（认不出 ⇒ `None`）。
    fn read(&mut self) -> Result<Option<&'static str>, Said>;
    /// 按一下换档键。
    fn press(&mut self) -> Result<Sent, Said>;
    /// 等下一次重画（到期没变 ⇒ `false`）。
    fn redrawn(&mut self) -> bool;
}

fn reply(
    result: SwitchResult,
    mode: Option<&'static str>,
    why: Option<Refused>,
    said: Option<String>,
) -> ModeSwitched {
    ModeSwitched {
        result,
        mode: mode.map(str::to_string),
        mode_text: None,
        why,
        said,
    }
}

fn unsure(mode: Option<&'static str>, said: String) -> ModeSwitched {
    reply(SwitchResult::Unsure, mode, None, Some(said))
}

/// 投命令那一形的那一样副作用（判据拿合成的结局喂）。
pub(crate) trait SayIo {
    /// 往会话里送那一句（同 `session-say` 那一道门）。
    fn say(&mut self, line: &str) -> Result<Sent, Said>;
}

/// 投命令那一形的本体：`line` 是那一家给 `target` 这一档的那一句（已核过是给选的档）。在等人不送；
/// 这台已经没有它的终端 ⇒ `keep` 记下、等恢复照它起，回切到了；送到了读不回此刻的档 ⇒ 结果未知。
pub(crate) fn command_with(
    target: &str,
    line: &str,
    waiting: bool,
    io: &mut dyn SayIo,
    keep: &dyn Fn() -> Result<(), Said>,
) -> Result<ModeSwitched, Said> {
    if waiting {
        return Ok(reply(
            SwitchResult::Refused,
            None,
            Some(Refused::Waiting),
            Some(no_said(Refused::Waiting)),
        ));
    }
    let sent = io.say(line)?;
    if terminals::terminal_gone(sent.why) {
        keep()?;
        return Ok(ModeSwitched {
            mode: Some(target.to_string()),
            ..reply(SwitchResult::Done, None, None, None)
        });
    }
    Ok(match sent.result {
        SentResult::Delivered => unsure(
            None,
            copy_core::copy_text("beSessionMode.unsure.unread", &[]),
        ),
        SentResult::Unsure => reply(SwitchResult::Unsure, None, None, sent.said),
        SentResult::Refused => reply(SwitchResult::Refused, None, sent.why, sent.said),
    })
}

/// 按键轮换那一形的本体：`target` 已核过是那一家给的档。这台已经没有它的终端 ⇒ `keep` 记下、等恢复照它起，回切到了。
pub(crate) fn switch_with(
    target: &str,
    sw: &ModeCycle,
    waiting: bool,
    io: &mut dyn ModeIo,
    keep: &dyn Fn() -> Result<(), Said>,
) -> Result<ModeSwitched, Said> {
    let got = walk(target, sw, waiting, io)?;
    if !terminals::terminal_gone(got.why) {
        return Ok(got);
    }
    keep()?;
    Ok(ModeSwitched {
        mode: Some(target.to_string()),
        ..reply(SwitchResult::Done, None, None, None)
    })
}

fn walk(
    target: &str,
    sw: &ModeCycle,
    waiting: bool,
    io: &mut dyn ModeIo,
) -> Result<ModeSwitched, Said> {
    if waiting {
        return Ok(reply(
            SwitchResult::Refused,
            None,
            Some(Refused::Waiting),
            Some(no_said(Refused::Waiting)),
        ));
    }
    if let Some(why) = io.open()? {
        return Ok(reply(
            SwitchResult::Refused,
            None,
            Some(why),
            Some(no_said(why)),
        ));
    }
    let mut seen: Vec<&'static str> = Vec::new();
    let mut now = io.read()?;
    loop {
        let Some(m) = now else {
            return Ok(unsure(
                None,
                copy_core::copy_text("beSessionMode.unsure.unread", &[]),
            ));
        };
        if m == target {
            return Ok(reply(SwitchResult::Done, Some(m), None, None));
        }
        if seen.contains(&m) || seen.len() >= sw.cycle_max {
            return Ok(unsure(
                Some(m),
                copy_core::copy_text("beSessionMode.unsure.notInCycle", &[]),
            ));
        }
        seen.push(m);
        let sent = io.press()?;
        match sent.result {
            SentResult::Delivered => {}
            SentResult::Unsure => return Ok(reply(SwitchResult::Unsure, Some(m), None, sent.said)),
            SentResult::Refused => {
                return Ok(reply(SwitchResult::Refused, Some(m), sent.why, sent.said))
            }
        }
        // 重画了、读到的档却还没变 ⇒ 再等一次。
        let next = settle(
            Some(m),
            REDRAWS_PER_PRESS,
            |n| *n != Some(m),
            || io.redrawn().then(|| io.read()).transpose(),
        )?;
        if next == Some(m) {
            return Ok(unsure(
                Some(m),
                copy_core::copy_text("beSessionMode.unsure.noRedraw", &[]),
            ));
        }
        now = next;
    }
}

/// 按键轮换那一形的终端：送键 · 看屏照 [`ModelDrive`]（生产：读屏驱动 [`Drive`]，抓屏不带色 · 按键与看屏前那一道门同 `session-say`：
/// 只剩 shell 的窗格不收 · 等重画用实时预览那个只读客户端；判据拿替身记下按了什么）。
struct Live<'a> {
    drive: &'a mut dyn ModelDrive,
    sw: ModeCycle,
}

/// 投命令那一形的终端（同 [`Live`] 那一份驱动）。
struct Say<'a>(&'a mut dyn ModelDrive);

impl SayIo for Say<'_> {
    fn say(&mut self, line: &str) -> Result<Sent, Said> {
        self.0.say(line)
    }
}

impl ModeIo for Live<'_> {
    fn open(&mut self) -> Result<Option<Refused>, Said> {
        self.drive.open()
    }
    fn read(&mut self) -> Result<Option<&'static str>, Said> {
        Ok((self.sw.read)(&self.drive.screen()?))
    }
    fn press(&mut self) -> Result<Sent, Said> {
        self.drive.press(self.sw.key)
    }
    fn redrawn(&mut self) -> bool {
        self.drive.redrawn()
    }
}

/// `session-permission`：切权限档（终端没了 ⇒ 记下、恢复照它起）。阻塞档（读 pidfile · 起 tmux 几发 · 等重画）。
pub struct SessionPermission;

impl Command for SessionPermission {
    typed::by_call!(SessionPermissionReq);
    type Err = SessionPermissionErr;
}

impl crate::wire::typed::Handle for SessionPermission {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: SessionPermissionReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ModeSwitched, SessionPermissionErr>> + Send + 'static
    {
        let (_cx, own) = cx.split();
        async move {
            answer(&req, &own).map_err(|f| typed::of_fail(f, SessionPermissionErr::Unobservable))
        }
    }
}

/// 收不收这一档：那一家给选的 ⇒ 收；认得出、却不给选的（越过批准那几档：只能起会话时带参数定）⇒ `launch_only`；认不出 ⇒ `bad_args`。
pub(crate) fn offered(perm: &crate::agents::PermissionFace, mode: &str) -> Result<(), Said> {
    if perm.offered(mode).is_some() {
        return Ok(());
    }
    if perm.shown.iter().any(|c| c.value == mode) {
        return Err(crate::wire::spec::fail(
            "launch_only",
            copy_core::copy_text("beSessionMode.switch.launchOnly", &[]),
        ));
    }
    Err(crate::wire::spec::fail(
        "bad_args",
        malformed("`mode` is not one this agent offers"),
    ))
}

/// 这条会话那一家（`registry` 里）的权限档：哪一家照 [`crate::observe::accounts_query::session_kind`]（先记录、再活会话表）；
/// 认不出 · 那一家没有 ⇒ `None`（不退到默认那一家）。
pub(crate) fn permission_among(
    registry: &[crate::agents::Adapter],
    at: Option<&crate::agents::SessionAt>,
    live: Option<&str>,
) -> Option<crate::agents::PermissionFace> {
    let kind = crate::observe::accounts_query::session_kind(at, live)?;
    crate::agents::pick_kind_among(registry, Some(kind))
        .ok()
        .and_then(|(_, f)| f.permission)
}

fn answer(req: &SessionPermissionReq, own: &crate::platform::Own) -> Result<ModeSwitched, Said> {
    let home = crate::agents::local_homes();
    let at = crate::observe::query::locate::session_at(&home, &req.sid).ok();
    let live = crate::history::live_table::now(&home);
    let mut drive = Drive::new(&req.sid, req.client.clone());
    answer_among(
        crate::agents::REGISTRY,
        at.as_ref(),
        live.kind_of(&req.sid),
        live.waiting(&req.sid),
        req,
        own,
        &mut drive,
    )
}

/// 认这条会话那一家（[`permission_among`]）· 核档 · 经 `drive` 切。认不出那一家 · 那一家切不了 ⇒ 不支持，`drive` 一个键不收。
/// `at` ＝ 按 sid 找到的那一条；`live` ＝ 活会话表里写它那份 pidfile 的那一家（[`crate::history::live_table::LiveTable::kind_of`]）。
pub(crate) fn answer_among(
    registry: &[crate::agents::Adapter],
    at: Option<&crate::agents::SessionAt>,
    live: Option<&str>,
    waiting: bool,
    req: &SessionPermissionReq,
    own: &crate::platform::Own,
    drive: &mut dyn ModelDrive,
) -> Result<ModeSwitched, Said> {
    let face = permission_among(registry, at, live);
    let record = at.map(|a| a.file.clone());
    let Some((perm, sw)) = face.and_then(|p| p.switch.map(|s| (p, s))) else {
        return Err(crate::wire::spec::fail(
            "unsupported",
            copy_core::copy_text("beSessionMode.switch.unsupported", &[]),
        ));
    };
    offered(&perm, &req.mode)?;
    let keep = || match &record {
        Some(r) => crate::history::history_annotations::keep_for_resume(
            own,
            &req.sid,
            r,
            None,
            Some(&req.mode),
        ),
        None => Err(crate::wire::spec::fail(
            "no_such_session",
            copy_core::copy_text("beSessionCommands.session.noRecord", &[]),
        )),
    };
    let mut got = match sw {
        ModeSwitch::Cycle(sw) => {
            let mut io = Live { drive, sw };
            switch_with(&req.mode, &sw, waiting, &mut io, &keep)?
        }
        // 给选的档（上面 `offered` 已核过）⇒ 那一家给它的那一句；只经 `PermissionFace::switch_line` 取，哪几档有句子只在那一处判。
        ModeSwitch::Command(_) => {
            let Some(line) = perm.switch_line(&req.mode) else {
                return Err(crate::wire::spec::fail(
                    "bad_args",
                    malformed("`mode` is not one this agent offers"),
                ));
            };
            command_with(&req.mode, &line, waiting, &mut Say(drive), &keep)?
        }
    };
    got.mode_text = got.mode.as_deref().and_then(|m| perm.text_of(m));
    Ok(got)
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_mode_face_tests.rs"]
mod tests;
