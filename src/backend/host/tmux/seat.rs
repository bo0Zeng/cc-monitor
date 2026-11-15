//! 一个进程跟它 `TMUX_PANE` 那个窗格的关系：**判法只此一处**（打标 `control::identity_tag` 与「谁在显示这条会话」
//! `observe::session_terminals` 都问这里；托管终端那一臂 —— 打标 `host::managed::tag_pid_at` 与观测面 `host::managed::terminal_ends_at`
//! 同经 `host::managed` 的 `seat_in` —— 经 [`seat_with`] 只换「首领是不是这个宿主的」那一样；读不到终端的平台（Windows · macOS）
//! 那一臂不经这里，照 ccm 起的那一跳认）。
//! 只读 `/proc`，一发 tmux 都不起。
//!
//! `TMUX_PANE` 是 tmux 起窗格时注进那个窗格根进程的，往下 fork / exec 原样继承：窗格里起的子进程、主会话 Bash 里起的
//! `claude -p`（没控制终端）都带着它。所以光看它说不出「这个窗格在显示这个进程」，还要看终端那一样事实（[`seat_of`]）。

use crate::common::tmux_utf8::TMUX_PANE_ENV;
use crate::platform::proc::{proc_env_var, tty_of, EnvRead};

/// [`pane_of`] 的三个结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneRead {
    /// 形状核过的 pane id。
    Pane(String),
    /// 环境读得到、`TMUX_PANE` 没设 ⇒ 不在 tmux 里。
    NotSet,
    /// 读不到，或者值的形状不对 ⇒ 不知道。
    Unknown,
}

/// 取这个进程环境里的 tmux 窗格（`TMUX_PANE`），核过形状（[`super::pane_id_ok`]）才回。
///
/// `proc_env_var` 在读侧就把空串压成了 `EnvRead::Unset` ⇒ `pane_id_ok("")` 在这条路上执行不到；留着它，
/// 是为了那条早退哪天换了写法时还有一道门站着（`-t ''` 不报错，静默解析成「当前 / 最近的那个会话」）。
pub fn pane_of(pid: u32) -> PaneRead {
    let raw = match proc_env_var(pid, TMUX_PANE_ENV) {
        EnvRead::Value(v) => v,
        EnvRead::Unset => return PaneRead::NotSet,
        EnvRead::Unreadable => return PaneRead::Unknown,
    };
    let pane = raw.trim().to_string();
    if super::pane_id_ok(&pane) {
        PaneRead::Pane(pane)
    } else {
        PaneRead::Unknown
    }
}

/// 一个进程跟 `TMUX_PANE` 那个窗格的关系（[`seat_of`] 判出来的）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seat {
    /// 它就是那个窗格（托管那一形：那个托管终端）前台的那一个。
    OnPane,
    /// 没有控制终端。
    Detached,
    /// 有控制终端，却在后台进程组里，或那个终端不是这个窗格（这个托管终端）的。
    Elsewhere,
    /// `/proc` 读不到。
    Unknown,
}

/// `pid` 自己环境里的窗格（[`pane_of`]）和它跟那个窗格的关系，一次交齐：窗格说得出 ⇒ [`seat_of`]`(pid, Some(窗格))`；
/// 说不出（没设 · 读不到 · 形状不对）⇒ 只判终端那一样（[`seat_of`]`(pid, None)`：`OnPane` 在这里只说「有控制终端」）。
/// 打标与「谁在显示这条会话」都经它，各自把结果说成自己的结局。
pub fn own_pane(pid: u32) -> (PaneRead, Seat) {
    match pane_of(pid) {
        PaneRead::Pane(pane) => {
            let seat = seat_of(pid, Some(&pane));
            (PaneRead::Pane(pane), seat)
        }
        other => (other, seat_of(pid, None)),
    }
}

/// `pid` 是不是 `pane` 那个窗格前台的那一个：[`seat_with`]，tmux 这一形认首领的法子是它环境里写的正是这个窗格
/// （窗格的根进程，tmux 起它时注了 `TMUX_PANE`）。`pane` 没给 ⇒ 只判有没有控制终端（不在 tmux 里那一支）。
pub fn seat_of(pid: u32, pane: Option<&str>) -> Seat {
    let Some(pane) = pane else {
        return match tty_of(pid) {
            None => Seat::Unknown,
            Some(me) if me.tty_nr == 0 => Seat::Detached,
            Some(_) => Seat::OnPane,
        };
    };
    seat_with(pid, |leader| match pane_of(leader) {
        PaneRead::Pane(p) => Some(p == pane),
        PaneRead::NotSet => Some(false),
        PaneRead::Unknown => None,
    })
}

/// `pid` 是不是某个宿主终端前台的那一个 —— **判法只此一处**（tmux 窗格 [`seat_of`] · 托管终端 `host::managed` 都经它）：
/// 有控制终端、在那个终端的前台进程组、且开那个终端的会话首领是这个宿主的。最后那一样由宿主声明：
/// `leader_is_ours(首领 pid)` 答是（`Some(true)`）· 不是（`Some(false)`）· 说不清（`None`）。
/// 不靠认命令行（`-p` · `sdk-cli`），只看 `/proc` 里终端这一样事实；这个平台读不到终端那一样（非 Linux）⇒ [`Seat::Unknown`]。
pub fn seat_with(pid: u32, leader_is_ours: impl FnOnce(u32) -> Option<bool>) -> Seat {
    let Some(me) = tty_of(pid) else {
        return Seat::Unknown;
    };
    if me.tty_nr == 0 {
        return Seat::Detached;
    }
    if me.pgrp != me.tpgid {
        return Seat::Elsewhere;
    }
    let Ok(leader) = u32::try_from(me.session) else {
        return Seat::Unknown;
    };
    match leader_is_ours(leader) {
        Some(true) => Seat::OnPane,
        Some(false) => Seat::Elsewhere,
        None => Seat::Unknown,
    }
}
