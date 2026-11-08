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
        self.input(Some(key), None)
    }

    /// 送进一句（字面字 ＋ 回车）。
    pub fn say(&mut self, text: &str) -> Result<Sent, Said> {
        self.input(None, Some(text))
    }

    /// 等下一次重画（到期没变 · 没起看屏的客户端 ⇒ `false`）。
    pub fn redrawn(&mut self) -> bool {
        self.watch.as_mut().is_some_and(Watch::changed)
    }

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
            ..TerminalInputReq::default()
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/screen_drive_tests.rs"]
mod tests;
