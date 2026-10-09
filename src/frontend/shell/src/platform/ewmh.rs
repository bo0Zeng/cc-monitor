//! X11 上「按窗口找 / 验 / 拉前」那一族的读法（EWMH）：窗口管理器在根窗口上挂的 `_NET_CLIENT_LIST`（它管着的顶层窗口）·
//! 每个窗口的 `_NET_WM_PID`（属主进程）· `WM_TRANSIENT_FOR`（有主人的对话框 / 工具窗）· `_NET_WM_NAME` / `WM_NAME`（标题）；
//! 拉前是往根窗口发一条 `_NET_ACTIVE_WINDOW` 客户消息（来源记作「分页器」= 用户替它点的），再读回 `_NET_ACTIVE_WINDOW` 看窗口管理器答没答应。
//!
//! 只是读法：哪个窗口算我们要的、拉不动说哪句，都在 `bind.rs`。每一问现开一条到 `$DISPLAY` 的连接、问完就关（↗ 是点一下才问一次）。

#![cfg(target_os = "linux")]

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, Window,
};
use x11rb::rust_connection::RustConnection;

/// `_NET_CLIENT_LIST` 里的一个窗口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub window: u32,
    /// `_NET_WM_PID`；没挂 ⇒ 0。
    pub pid: u32,
    /// 有 `WM_TRANSIENT_FOR`（对话框 / 工具窗，不算应用窗口）。
    pub transient: bool,
    pub title: String,
}

struct Conn {
    c: RustConnection,
    root: Window,
}

fn open() -> Option<Conn> {
    let (c, screen) = x11rb::connect(None).ok()?;
    let root = c.setup().roots.get(screen)?.root;
    Some(Conn { c, root })
}

impl Conn {
    fn atom(&self, name: &str) -> Option<Atom> {
        Some(
            self.c
                .intern_atom(false, name.as_bytes())
                .ok()?
                .reply()
                .ok()?
                .atom,
        )
    }

    fn u32s(&self, win: Window, prop: Atom, ty: impl Into<Atom>) -> Vec<u32> {
        self.c
            .get_property(false, win, prop, ty.into(), 0, 4096)
            .ok()
            .and_then(|r| r.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    }

    fn text(&self, win: Window, prop: Atom, ty: Atom) -> Option<String> {
        let r = self
            .c
            .get_property(false, win, prop, ty, 0, 1024)
            .ok()?
            .reply()
            .ok()?;
        (!r.value.is_empty()).then(|| String::from_utf8_lossy(&r.value).into_owned())
    }

    fn clients(&self) -> Vec<Client> {
        let (Some(list), Some(pid), Some(name), Some(utf8)) = (
            self.atom("_NET_CLIENT_LIST"),
            self.atom("_NET_WM_PID"),
            self.atom("_NET_WM_NAME"),
            self.atom("UTF8_STRING"),
        ) else {
            return Vec::new();
        };
        self.u32s(self.root, list, AtomEnum::WINDOW)
            .into_iter()
            .map(|w| Client {
                window: w,
                pid: self
                    .u32s(w, pid, AtomEnum::CARDINAL)
                    .first()
                    .copied()
                    .unwrap_or(0),
                transient: !self
                    .u32s(w, AtomEnum::WM_TRANSIENT_FOR.into(), AtomEnum::WINDOW)
                    .is_empty(),
                title: self
                    .text(w, name, utf8)
                    .or_else(|| self.text(w, AtomEnum::WM_NAME.into(), AtomEnum::STRING.into()))
                    .unwrap_or_default(),
            })
            .collect()
    }

    fn active(&self, net_active: Atom) -> Option<u32> {
        self.u32s(self.root, net_active, AtomEnum::WINDOW)
            .first()
            .copied()
    }
}

/// 窗口管理器此刻管着的顶层窗口们（连不上 `$DISPLAY` / 窗口管理器不挂这张表 ⇒ 空）。
pub fn clients() -> Vec<Client> {
    open().map(|c| c.clients()).unwrap_or_default()
}

/// 请窗口管理器把 `window` 切到前面（最小化着的它自己会还原），最多等 `wait` 看它答没答应：`_NET_ACTIVE_WINDOW` 变成它 ⇒ `true`。
pub fn activate(window: u32, wait: std::time::Duration) -> bool {
    let Some(c) = open() else {
        return false;
    };
    let Some(net_active) = c.atom("_NET_ACTIVE_WINDOW") else {
        return false;
    };
    // data: 来源 2（分页器：替用户点的，窗口管理器不按抢焦点那一套拦）· 时刻 0（当前）· 请求方的当前活动窗口 0。
    let ev = ClientMessageEvent::new(32, window, net_active, [2u32, 0, 0, 0, 0]);
    let sent = c.c.send_event(
        false,
        c.root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        ev,
    );
    if sent.is_err() || c.c.flush().is_err() {
        return false;
    }
    let deadline = std::time::Instant::now() + wait;
    loop {
        if c.active(net_active) == Some(window) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}
