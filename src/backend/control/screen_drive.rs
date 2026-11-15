//! 读屏驱动：会话开着时替人按键的那几处（切权限档 · 换模型找确认框 · 回退）共用的一份。
//!
//! - [`Drive`]：认目标（同 `session-say` 那一道门：只剩 shell 的窗格不收）→ 起看屏的只读客户端（同实时预览那一个）·
//!   取屏走 `terminal-preview`（不带色）· 按键 / 送字前先忘掉此刻之前的变化 · 等下一次重画。
//!   托管终端：订托管口的推送（托管进程每推一帧算一次重画，不起 tmux）· 取屏走托管口 `capture`；按键送字同一道（每一下过门）。
//! - [`settle`]：屏还不是认得的样子 ⇒ 等一次重画再读，至多那几次；「认得的样子」由各个面自己判（认档 · 认框 · 认回退菜单）。
//! - 最多等几次重画只住这里：按一下之后 [`REDRAWS_PER_PRESS`]，送出一行之后 [`REDRAWS_AFTER_SEND`]。

use crate::common::said::Said;
use crate::control::terminal_follow::Watch;
use crate::control::terminals::{self, Refused};
use crate::control::terminals_wire::SentResult;
use crate::control::terminals_wire::{Sent, TerminalInputReq, TerminalPreviewReq};
use crate::host::tmux::On;

/// 按一下之后最多等几次重画（每次至多 `terminal_follow::REDRAW_WAIT`）才认定「没变」。
pub const REDRAWS_PER_PRESS: usize = 4;
/// 送出一行之后最多等几次重画（找确认框 · 从记录读回改好的那一句）。
pub const REDRAWS_AFTER_SEND: usize = 8;

/// 等屏变到 `moved` 认的样子：`now` 是此刻读到的；还不是 ⇒ `next`（等一次重画、变了就再读：`Some(读到的)`；到期没变 ⇒ `None`），
/// 至多 `redraws` 次。回最后读到的那一样（认到了 · 次数用完 · 不再重画）。
pub fn settle<S>(
    mut now: S,
    redraws: usize,
    moved: impl Fn(&S) -> bool,
    mut next: impl FnMut() -> Result<Option<S>, Said>,
) -> Result<S, Said> {
    for _ in 0..redraws {
        if moved(&now) {
            break;
        }
        match next()? {
            Some(s) => now = s,
            None => break,
        }
    }
    Ok(now)
}

/// 认框之前最多等几次重画（框多半早就画着；在重画的屏多看两眼，不多等）。
pub const REDRAWS_BEFORE_BOX: usize = 2;

/// 答终端里那个框要碰的那几样（生产：[`Drive`]；判据拿替身屏幕记下按了哪几下）。
pub trait BoxIo {
    /// 起看屏的那只只读客户端；终端不收 ⇒ 原因。
    fn open(&mut self) -> Result<Option<Refused>, Said>;
    /// 此刻那一屏（纯文本行）。
    fn screen(&mut self) -> Result<Vec<String>, Said>;
    /// 等下一次重画（到期没变 ⇒ `false`）。
    fn redrawn(&mut self) -> bool;
    /// 按一下。
    fn tap(&mut self, k: crate::agents::Keystroke) -> Result<Sent, Said>;
}

/// 答终端里那个框的结局。
#[derive(Debug)]
pub enum OnBox {
    /// 终端不收（没了 · 只剩 shell · 不归我们）· 按的那一下被拒：一个键没进终端；那一句（原因）。
    Closed(String),
    /// 屏上正显示的不是这一条的框（几次重画都认不出）：一个键没按。
    NotSeen,
    /// 认出了这一条的框，屏上却说分不出正显示的是哪一条（`doubt` 认：排着的比认得的多）：一个键没按。
    Doubt,
    /// 按了，再读两屏那个框都不在了（或已换成排着的另一条）。
    Gone,
    /// 按了（或不知道按没按进去），那个框还在 · 屏不再变判不了 · 按下之后出了错。
    Stays,
}

/// 认准才按、按完复核：起看屏 → 屏上正是这一条的框（`seen` 认，至多等 [`REDRAWS_BEFORE_BOX`] 次重画），按之前紧接着再读一屏再认一次、
/// 那一屏说分不出是哪一条（`doubt`）⇒ 不按，
/// 才按 `k` 一下 → 再读屏（至多 [`REDRAWS_PER_PRESS`] 次重画）看框还在不在，认不出了再读一屏确认（重画到一半的那一帧不算没了）。
/// 从按下那一下起出的错一律算「还在 · 判不了」（键可能已经进了终端）。
/// 余下的空档：再认那一屏与键送到之间（几十毫秒）终端里有人先答掉、换上另一条，这一下会落在那一条上；按键不带画面指纹去比
/// （框上方的转圈让指纹一直在变）。不看这条会话在不在等人：这一路恰恰在等人时才按（切档 · 换模型那道门不在这里）。
pub fn answer_box(
    io: &mut dyn BoxIo,
    seen: &dyn Fn(&[String]) -> bool,
    doubt: &dyn Fn(&[String]) -> bool,
    k: crate::agents::Keystroke,
) -> Result<OnBox, Said> {
    if let Some(why) = io.open()? {
        return Ok(OnBox::Closed(terminals::no_said(why)));
    }
    let first = io.screen()?;
    let before = settle(first, REDRAWS_BEFORE_BOX, |s| seen(s), || next_screen(io))?;
    let last = io.screen()?;
    if !seen(&before) || !seen(&last) {
        return Ok(OnBox::NotSeen);
    }
    if doubt(&last) {
        return Ok(OnBox::Doubt);
    }
    let sent = match io.tap(k) {
        Ok(s) => s,
        Err(_) => return Ok(OnBox::Stays),
    };
    match sent.result {
        SentResult::Delivered => {}
        SentResult::Refused => return Ok(OnBox::Closed(sent.said.unwrap_or(sent.note))),
        SentResult::Unsure => return Ok(OnBox::Stays),
    }
    Ok(match after_press(io, seen) {
        Ok(true) => OnBox::Gone,
        Ok(false) | Err(_) => OnBox::Stays,
    })
}

/// 按下之后：框不在了、紧接着再读一屏也不在 ⇒ 真。
fn after_press(io: &mut dyn BoxIo, seen: &dyn Fn(&[String]) -> bool) -> Result<bool, Said> {
    let now = io.screen()?;
    let after = settle(now, REDRAWS_PER_PRESS, |s| !seen(s), || next_screen(io))?;
    Ok(!seen(&after) && !seen(&io.screen()?))
}

/// 等一次重画、变了就再读（到期没变 ⇒ `None`）。
fn next_screen(io: &mut dyn BoxIo) -> Result<Option<Vec<String>>, Said> {
    if io.redrawn() {
        io.screen().map(Some)
    } else {
        Ok(None)
    }
}

/// 生产那一份：在 `sid` 那条会话的终端里认准才按（[`answer_box`] 经读屏驱动 [`Drive`]）。
pub fn press_box(
    sid: &str,
    seen: &dyn Fn(&[String]) -> bool,
    doubt: &dyn Fn(&[String]) -> bool,
    k: crate::agents::Keystroke,
) -> Result<OnBox, Said> {
    answer_box(&mut Drive::new(sid, None), seen, doubt, k)
}

impl BoxIo for Drive<'_> {
    fn open(&mut self) -> Result<Option<Refused>, Said> {
        Drive::open(self)
    }
    fn screen(&mut self) -> Result<Vec<String>, Said> {
        Drive::screen(self)
    }
    fn redrawn(&mut self) -> bool {
        Drive::redrawn(self)
    }
    fn tap(&mut self, k: crate::agents::Keystroke) -> Result<Sent, Said> {
        match k {
            crate::agents::Keystroke::Key(name) => self.press(name),
            crate::agents::Keystroke::Char(c) => self.input(None, Some(c), false),
        }
    }
}

/// 生产那一份：看一条会话所在的那个终端、往里按键送字。
pub struct Drive<'a> {
    sid: &'a str,
    client: Option<String>,
    watch: Option<Watch>,
    /// 托管那一形的托管口（取屏经它）；tmux 那一形 ⇒ `None`。
    hosted: Option<std::path::PathBuf>,
}

impl<'a> Drive<'a> {
    /// 还没起看屏的客户端（[`Drive::open`] 才起）。
    pub fn new(sid: &'a str, client: Option<String>) -> Drive<'a> {
        Drive {
            sid,
            client,
            watch: None,
            hosted: None,
        }
    }

    /// 这条会话的 sid。
    pub fn sid(&self) -> &'a str {
        self.sid
    }

    /// 起看屏的那只只读客户端；终端不收（没了 · 只剩 shell · 不归我们）⇒ 原因。
    /// 托管那一形：订托管口的推送（不起 tmux），托管口不应 ⇒ 已经没了；看屏只读、不过门（同实时预览），门在每一下按键送字。
    pub fn open(&mut self) -> Result<Option<Refused>, Said> {
        if let Some(sock) = terminals::hosted_sock_of(None, Some(self.sid))? {
            let Ok(w) = Watch::hosted(&sock) else {
                return Ok(Some(Refused::Ended));
            };
            self.watch = Some(w);
            self.hosted = Some(sock);
            return Ok(None);
        }
        let t = match terminals::say_target_on(On::default(), self.sid, self.client.as_deref())? {
            Ok(t) => t,
            Err(why) => return Ok(Some(why)),
        };
        self.watch = Some(Watch::open(None, &t)?);
        Ok(None)
    }

    /// 此刻那一屏（纯文本行）。
    pub fn screen(&self) -> Result<Vec<String>, Said> {
        let tz = crate::common::time::Tz::default();
        if let Some(sock) = &self.hosted {
            let shot = terminals::hosted_screen_at(sock, &tz)?;
            return Ok(shot.lines.into_iter().map(|l| l.text).collect());
        }
        let shot = terminals::preview_on(
            On::default(),
            &TerminalPreviewReq {
                sid: Some(self.sid.to_string()),
                color: Some(false),
                ..TerminalPreviewReq::default()
            },
            &tz,
        )?;
        Ok(shot.lines.into_iter().map(|l| l.text).collect())
    }

    /// 按一个键（`terminal-input` 键表里的名字）。
    pub fn press(&mut self, key: &str) -> Result<Sent, Said> {
        self.input(Some(key), None, false)
    }

    /// 送进一句（字面字 ＋ 回车）。
    pub fn say(&mut self, text: &str) -> Result<Sent, Said> {
        self.input(None, Some(text), true)
    }

    /// 等下一次重画（到期没变 · 没起看屏的客户端 ⇒ `false`）。
    pub fn redrawn(&mut self) -> bool {
        self.watch.as_mut().is_some_and(Watch::changed)
    }

    fn input(&mut self, key: Option<&str>, text: Option<&str>, enter: bool) -> Result<Sent, Said> {
        let req = TerminalInputReq {
            sid: Some(self.sid.to_string()),
            key: key.map(str::to_string),
            text: text.map(str::to_string),
            enter: text.map(|_| enter),
            client: self.client.clone(),
            ..TerminalInputReq::default()
        };
        self.forget_then_send(&req, |req, _| crate::control::terminals_wire::say(req, &[]))
    }

    /// 先忘掉此刻之前的变化，再送 `req`：之后等到的重画才是送那一下带来的。
    /// `send` 是送的那一下（生产 [`Drive::input`] 传 `terminals_wire::say`），拿得到看屏的那只客户端（只读；没起 ⇒ `None`）。
    fn forget_then_send<R>(
        &mut self,
        req: &TerminalInputReq,
        send: impl FnOnce(&TerminalInputReq, Option<&Watch>) -> R,
    ) -> R {
        if let Some(w) = self.watch.as_mut() {
            w.forget();
        }
        send(req, self.watch.as_ref())
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/screen_drive_tests.rs"]
mod tests;
