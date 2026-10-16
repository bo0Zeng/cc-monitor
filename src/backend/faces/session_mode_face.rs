//! 帧面 `session-permission` 的宿主壳：会话开着时切权限档。那一家只有「按一下换到下一档」的循环（适配层 `PermissionFace.switch`），
//! 所以：读此刻的档 · 不是 ⇒ 按一下 · 等它重画 · 再读，到了为止，最多按一圈。在等人不按；认不出 · 不在循环里 · 按了没变都照实说「结果未知」。

use crate::agents::ModeSwitch;
use crate::common::contract::malformed;
use crate::control::terminals::{self, no_said, CmdErr, On, Refused};
use crate::control::terminals_wire::{Sent, SentResult, TerminalInputReq, TerminalPreviewReq};
use crate::wire::typed::{self, Command};
use serde::Deserialize;

/// 按一下之后最多等几次重画（每次至多 `terminal_follow::REDRAW_WAIT`）：重画了、读到的档却还没变 ⇒ 再等一次。
const REDRAWS_PER_PRESS: usize = 4;

/// `session-permission` 的入参。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionPermissionReq {
    /// 会话 id。
    pub(crate) sid: String,
    /// 要切到的档（原值；必须是 `session-new-facts` 那一家 `permissionModes` 里的一个）。
    pub(crate) mode: String,
    /// 自报的前端（过「哪个前端的会话」那一维，同 `terminal-input`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) client: Option<String>,
}

/// `session-permission` 的回包：线上形住 `cmd-wire::session`。
pub(crate) use cmd_wire::session::ModeSwitched;

/// 切模型 · 切档的结局：线上形住 `cmd-wire::session`。
pub(crate) use cmd_wire::session::SwitchResult;

typed::codes! {
    /// `session-permission` 的码：认会话 · 列名单 · 起看屏的客户端 · 抓屏 · 送键。在等人 · 认不出 · 不归我们不是错（`result`）。
    pub(crate) enum SessionPermissionErr {
        /// 那一家会话开着时切不了档。
        Unsupported = "unsupported",
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
    /// 起看屏的那一份（只在真要按的时候起）。
    fn open(&mut self) -> Result<(), CmdErr>;
    /// 此刻的档（认不出 ⇒ `None`）。
    fn read(&mut self) -> Result<Option<&'static str>, CmdErr>;
    /// 按一下换档键。
    fn press(&mut self) -> Result<Sent, CmdErr>;
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

/// 本体：`target` 已核过是那一家给的档。
pub(crate) fn switch_with(
    target: &str,
    sw: &ModeSwitch,
    waiting: bool,
    io: &mut dyn ModeIo,
) -> Result<ModeSwitched, CmdErr> {
    if waiting {
        return Ok(reply(
            SwitchResult::Refused,
            None,
            Some(Refused::Waiting),
            Some(no_said(Refused::Waiting)),
        ));
    }
    io.open()?;
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
        let mut next = Some(m);
        for _ in 0..REDRAWS_PER_PRESS {
            if !io.redrawn() {
                break;
            }
            next = io.read()?;
            if next != Some(m) {
                break;
            }
        }
        if next == Some(m) {
            return Ok(unsure(
                Some(m),
                copy_core::copy_text("beSessionMode.unsure.noRedraw", &[]),
            ));
        }
        now = next;
    }
}

/// 生产那一份：抓屏走 `terminal-preview` 那一条（不带色），按键走 `terminal-input` 那一条（身份门同一份），等重画用实时预览那个只读客户端。
struct Live<'a> {
    sid: &'a str,
    client: Option<String>,
    sw: ModeSwitch,
    watch: Option<crate::control::terminal_follow::Watch>,
}

impl ModeIo for Live<'_> {
    fn open(&mut self) -> Result<(), CmdErr> {
        let t = terminals::follow_target_on(On::default(), None, Some(&self.sid))?;
        self.watch = Some(crate::control::terminal_follow::Watch::open(None, &t)?);
        Ok(())
    }
    fn read(&mut self) -> Result<Option<&'static str>, CmdErr> {
        let shot = terminals::preview_on(
            On::default(),
            &TerminalPreviewReq {
                sid: Some(self.sid.to_string()),
                color: Some(false),
                ..TerminalPreviewReq::default()
            },
            &crate::Tz::default(),
        )?;
        let lines: Vec<String> = shot.lines.into_iter().map(|l| l.text).collect();
        Ok((self.sw.read)(&lines))
    }
    fn press(&mut self) -> Result<Sent, CmdErr> {
        if let Some(w) = self.watch.as_mut() {
            w.forget();
        }
        crate::control::terminals_wire::input(&TerminalInputReq {
            sid: Some(self.sid.to_string()),
            key: Some(self.sw.key.to_string()),
            client: self.client.clone(),
            ..TerminalInputReq::default()
        })
    }
    fn redrawn(&mut self) -> bool {
        self.watch.as_mut().is_some_and(|w| w.changed())
    }
}

/// `session-permission`：会话开着时切权限档。阻塞档（读 pidfile · 起 tmux 几发 · 等重画）。
pub(crate) struct SessionPermission;

impl Command for SessionPermission {
    const NAME: &'static str = "session-permission";
    type Req = SessionPermissionReq;
    type Reply = ModeSwitched;
    type Err = SessionPermissionErr;
}

impl crate::wire::typed::Handle for SessionPermission {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: SessionPermissionReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ModeSwitched, SessionPermissionErr>> + Send + 'static
    {
        async move { answer(&req).map_err(|f| typed::of_fail(f, SessionPermissionErr::Unobservable)) }
    }
}

fn answer(req: &SessionPermissionReq) -> Result<ModeSwitched, CmdErr> {
    if !shell_quote_core::session_id_ok(&req.sid) {
        return Err(CmdErr::new(
            "bad_args",
            malformed("`sid` must be a session id"),
        ));
    }
    let home = crate::history::listing::agent_home();
    let kind = crate::observe::query::locate::session_record(&home, &req.sid)
        .ok()
        .and_then(|p| crate::agents::record_kind_of(&p));
    let face = crate::agents::pick_kind(kind)
        .ok()
        .and_then(|(_, f)| f.permission);
    let Some((perm, sw)) = face.and_then(|p| p.switch.map(|s| (p, s))) else {
        return Err(CmdErr::new(
            "unsupported",
            copy_core::copy_text("beSessionMode.switch.unsupported", &[]),
        ));
    };
    if !perm.modes.iter().any(|c| c.value == req.mode) {
        return Err(CmdErr::new(
            "bad_args",
            malformed("`mode` is not one this agent offers"),
        ));
    }
    let waiting =
        crate::observe::accounts_query::waiting_now(&home, &req.sid, &crate::control::asks::sids());
    let mut io = Live {
        sid: &req.sid,
        client: req.client.clone(),
        sw,
        watch: None,
    };
    let mut got = switch_with(&req.mode, &sw, waiting, &mut io)?;
    got.mode_text = got.mode.as_deref().and_then(|m| perm.text_of(m));
    Ok(got)
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_mode_face_tests.rs"]
mod tests;
