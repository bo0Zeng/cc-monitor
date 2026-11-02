//! 后端替一个前端（桌面 · 手机）持有托管终端的输入：每个托管终端至多一条长连接，进程内一张表（托管 id → 连接 ＋ 前端）。
//!
//! 托管终端只有一个写者（A3-3-2）：持有者是一条连接。桌面 · 手机「接管输入」＝ 后端替它开一条、`take`，连接留着；
//! 送字经这一条（[`send`]）；「交还」＝ `release` ＋ `bye`、摘掉（[`release`]）。
//! 读那一半一条线，只看两样：`holder` 说不再是我（真终端接上要输入 · 别的前端接管）⇒ 摘掉；`ended` · 断开 ⇒ 摘掉。
//! 后端重启 ⇒ 表空了、那条连接随进程断开，托管进程照常把输入交给最近连上的真终端（不留陈旧的「后端持有」）。

use super::client;
use super::wire::{Ctl, Frame, WhoKind};
use cmd_wire::session::HolderWho;
use copy_core::copy_text;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

struct Held {
    conn: Arc<own_chan::BlockingStream>,
    front: String,
    /// 这一条是第几回接管（读线摘的时候核一下，别把后来那一条摘了）。
    turn: u64,
}

#[derive(Default)]
struct Table {
    held: BTreeMap<String, Held>,
    turns: u64,
}

static TABLE: Mutex<Table> = Mutex::new(Table {
    held: BTreeMap::new(),
    turns: 0,
});

fn table() -> MutexGuard<'static, Table> {
    TABLE.lock().unwrap_or_else(|p| p.into_inner())
}

/// 持有输入的是哪一端：托管口报的种类 ＋ 后端替哪个前端（`mobile` ⇒ 手机，别的 ⇒ 桌面）。
pub fn who_of(kind: WhoKind, front: Option<&str>) -> HolderWho {
    match (kind, front) {
        (WhoKind::Terminal, _) => HolderWho::TerminalWindow,
        (WhoKind::Backend, Some("mobile")) => HolderWho::Mobile,
        (WhoKind::Backend, _) => HolderWho::Desktop,
    }
}

/// 「谁持有」那一句，只这一处写（A3-3-32-5）：名单 · 实时流 · 瘦客户端底行都调它。`mine` ＝ 看的这一端就是持有者。
pub fn said_of(who: HolderWho, mine: bool) -> String {
    match (who, mine) {
        (HolderWho::None, _) => copy_text("beTerminal.holder.none", &[]),
        (_, true) => copy_text("beTerminal.holder.here", &[]),
        (HolderWho::Desktop, _) => copy_text("beTerminal.holder.desktop", &[]),
        (HolderWho::Mobile, _) => copy_text("beTerminal.holder.mobile", &[]),
        (HolderWho::TerminalWindow, _) => copy_text("beTerminal.holder.terminalWindow", &[]),
    }
}

/// 那一端的短词（瘦客户端 `前缀 D` 单子的种类那一列）：终端窗口 · 桌面 · 手机。
pub(crate) fn word_of(who: HolderWho) -> String {
    match who {
        HolderWho::TerminalWindow => copy_text("beTerminal.who.terminalWindow", &[]),
        HolderWho::Desktop => copy_text("beTerminal.who.desktop", &[]),
        HolderWho::Mobile => copy_text("beTerminal.who.mobile", &[]),
        HolderWho::None => String::new(),
    }
}

/// 此刻后端替哪个前端持有它（没有 ⇒ `None`）。
pub fn holder(id: &str) -> Option<String> {
    table().held.get(id).map(|h| h.front.clone())
}

/// 替 `front` 接管 `id` 的输入：开连接、`take`、等托管进程说「现在是你」才回。已经替它持有着 ⇒ 不再开。
pub(crate) fn take(sock: &std::path::Path, id: &str, front: &str) -> std::io::Result<()> {
    if holder(id).as_deref() == Some(front) {
        return Ok(());
    }
    // 别的前端经后端持有着：它的那一条交还，换这一条。
    release(id);
    let s = client::open(sock, Some(front))?;
    client::send(&s, &Frame::Ctl(Ctl::Take))?;
    let mut buf = Vec::new();
    loop {
        match client::next_frame(&s, &mut buf)? {
            Frame::Ctl(Ctl::Holder { holder: Some(h) }) if h.mine => break,
            Frame::Ctl(Ctl::Ended { .. }) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "hosted terminal ended",
                ))
            }
            _ => {}
        }
    }
    // 握手完了：读那一半交给一条线，不再设期限（有字节才醒）。
    s.set_read_timeout(None)?;
    let conn = Arc::new(s);
    let turn = {
        let mut t = table();
        t.turns += 1;
        let turn = t.turns;
        t.held.insert(
            id.to_string(),
            Held {
                conn: Arc::clone(&conn),
                front: front.to_string(),
                turn,
            },
        );
        turn
    };
    let id = id.to_string();
    std::thread::spawn(move || watch(&id, &conn, turn, buf));
    Ok(())
}

/// 读那一半：不再是我 · 结束了 · 断开 ⇒ 摘掉（只摘自己这一回的）。
fn watch(id: &str, conn: &own_chan::BlockingStream, turn: u64, mut buf: Vec<u8>) {
    loop {
        match client::next_frame(conn, &mut buf) {
            Ok(Frame::Ctl(Ctl::Holder { holder })) if !holder.as_ref().is_some_and(|h| h.mine) => {
                break
            }
            Ok(Frame::Ctl(Ctl::Ended { .. })) | Err(_) => break,
            Ok(_) => {}
        }
    }
    let mut t = table();
    if t.held.get(id).is_some_and(|h| h.turn == turn) {
        t.held.remove(id);
    }
}

/// 交还：`release` ＋ `bye`，摘掉（没持有 ⇒ 什么都不做）。
pub fn release(id: &str) {
    let Some(h) = table().held.remove(id) else {
        return;
    };
    if let Err(e) = client::send(&h.conn, &Frame::Ctl(Ctl::Release)) {
        tracing::debug!("托管终端 {id}：交还那一帧没写出去：{e}");
    }
    client::bye(&h.conn);
}

/// 经后端持有的那一条送字节；此刻没持有 ⇒ `Ok(false)`。
pub fn send(id: &str, bytes: &[u8]) -> std::io::Result<bool> {
    let Some(conn) = table().held.get(id).map(|h| Arc::clone(&h.conn)) else {
        return Ok(false);
    };
    client::send(&conn, &Frame::In(bytes.to_vec()))?;
    Ok(true)
}

#[cfg(test)]
#[path = "../../../../tests/backend/host/hold_tests.rs"]
mod tests;
